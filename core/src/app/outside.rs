//! Outside apps calling Flow through `flow-mcp` ([`crate::mcp`]), on Flow's
//! side: listening while Settings' Outside apps is on, a card the first time
//! an app calls asking the user what it may do (kept in `mcp.json`), each
//! tool carried out as far as that allows, and commands run in the app's own
//! panes, never the user's, asking first where the user said to and always
//! where they're risky. All apart from the assistant's own tools and panes.

use std::path::PathBuf;
use std::sync::Arc;

use serde_json::{json, Value};

use super::*;
use crate::mcp::bridge::{Event, Handler, Listening, Request};
use crate::mcp::grants::{Grant, Grants, May};
use crate::mcp::tools::Kind;
use ui::{McpAsk, McpCard};

/// The most of a pane's text or a command's output an app is given.
const OUTSIDE_OUTPUT: usize = 64_000;

/// An app as Flow knows it: the name its MCP client gives, and the program
/// that started `flow-mcp`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AppKey {
    pub name: String,
    pub path: String,
}

impl AppKey {
    fn of(request: &Request) -> Self {
        AppKey { name: request.client.shown().to_string(), path: request.peer.host.as_ref().map(|h| h.path.display().to_string()).unwrap_or_default() }
    }
}

/// A card waiting for the user, and the calls that wait on it.
pub(super) struct Card {
    key: AppKey,
    conn: u64,
    pub ui: McpCard,
    waiting: Vec<Request>,
}

/// A command an app asked for, in one of its panes: waiting for the pane's
/// prompt, then running until it's done or its time is up.
struct Run {
    conn: u64,
    reply: crossbeam_channel::Sender<Result<String, String>>,
    session: SessionId,
    command: String,
    timeout: Duration,
    since: Instant,
    running: bool,
}

/// Outside apps, as Flow serves them.
#[derive(Default)]
pub(super) struct Outside {
    listening: Option<Box<dyn Listening>>,
    pub grants: Grants,
    /// Cards waiting for the user, the first in front.
    pub cards: Vec<Card>,
    runs: Vec<Run>,
    /// The panes each app opened or Flow opened for it, by `Pane::id`.
    own: Vec<(AppKey, u64)>,
    /// Each app's pane beside the user's, by `Pane::id`.
    side: Vec<(AppKey, u64)>,
    /// Tabs an app closed, by `Tab::id`, closed as Flow next waits.
    closing: Vec<u64>,
    /// Where the grants are kept; none for snapshots, which keep nothing.
    file: Option<PathBuf>,
    /// Settings' Outside apps, worked out when Settings opens and after a
    /// change, not each frame.
    pub view: std::rc::Rc<settings::OutsideView>,
}


/// Why a call wasn't allowed, saying where the user allows it.
fn refused(app: &str, kind: Kind) -> String {
    let what = match kind {
        Kind::Read => "read Flow's tabs",
        Kind::Tabs => "open and close tabs",
        Kind::Settings => "change Flow's settings",
        Kind::Commands => "run commands",
    };
    format!("The user hasn't allowed {app} to {what}. They can allow it in Flow's Settings → Outside apps, if they want to.")
}

fn off() -> String {
    "Flow no longer lets outside apps connect (Settings → Outside apps is off).".into()
}

impl App {
    /// Listens for outside apps while Settings' switch is on (never for
    /// snapshots, nor in Flow run as administrator), and stops when it's off,
    /// refusing what waited.
    pub(super) fn follow_outside(&mut self) {
        // A guest Flow, started for a command, leaves them to the one running.
        let on = self.config.mcp.enabled && self.snapshot.is_none() && !self.platform.elevated() && !self.guest();
        if on == self.outside.listening.is_some() {
            return;
        }
        if on {
            let waker = self.waker.clone();
            let handler: Handler = Arc::new(move |event| waker.send(UserEvent::Mcp(event)));
            match self.platform.mcp_listen(handler) {
                Ok(listening) => {
                    let file = Grants::file();
                    self.outside.grants = Grants::load(&file);
                    self.outside.file = Some(file);
                    self.outside.listening = Some(listening);
                }
                Err(error) => {
                    log::warn!("outside apps: {error}");
                    self.show_toast(tf("toast.mcp_failed", &[("error", &error)]));
                }
            }
        } else {
            self.outside.listening = None;
            for card in self.outside.cards.drain(..) {
                for request in card.waiting {
                    let _ = request.reply.send(Err(off()));
                }
            }
            // Commands under way go on in their panes; their apps hear no more.
            for run in self.outside.runs.drain(..) {
                let _ = run.reply.send(Err(off()));
            }
            self.request_redraw();
        }
    }

    /// A call, or an app gone.
    pub(super) fn outside_event(&mut self, event: Event) {
        match event {
            Event::Call(request) => self.outside_call(request),
            Event::Gone(conn) => {
                let before = self.outside.cards.len();
                self.outside.cards.retain(|c| c.conn != conn);
                self.outside.runs.retain(|r| r.conn != conn);
                if self.outside.cards.len() != before {
                    self.request_redraw();
                }
            }
        }
    }

    fn outside_call(&mut self, request: Request) {
        if self.outside.listening.is_none() {
            let _ = request.reply.send(Err(off()));
            return;
        }
        let key = AppKey::of(&request);
        // While a card asks what it may do, its calls wait behind it.
        if let Some(card) = self.outside.cards.iter_mut().find(|c| c.key == key && matches!(c.ui.ask, McpAsk::Use { .. })) {
            card.waiting.push(request);
            return;
        }
        match self.outside.grants.find(&key.name, &key.path) {
            Some(i) => {
                let grant = self.outside.grants.apps[i].clone();
                self.outside_do(&grant, key, request);
            }
            None => {
                // Reading is on at first, and so is what its first call needs.
                let kind = crate::mcp::tools::find(&request.tool).map_or(Kind::Read, |t| t.kind);
                let kinds = Kind::ALL.map(|k| k == Kind::Read || k == kind);
                let conn = request.conn;
                self.outside_card(key, conn, McpAsk::Use { kinds }, vec![request]);
            }
        }
    }

    /// Puts up a card for `key`'s app, the user told where Flow is behind.
    fn outside_card(&mut self, key: AppKey, conn: u64, ask: McpAsk, waiting: Vec<Request>) {
        let title = match &ask {
            McpAsk::Use { .. } => tf("outside.use_title", &[("app", &key.name)]),
            McpAsk::Command { .. } => tf("outside.command_title", &[("app", &key.name)]),
        };
        self.outside.cards.push(Card { ui: McpCard { app: key.name.clone(), host: key.path.clone(), ask }, key, conn, waiting });
        if !self.focused {
            if let Some(window) = self.window.as_ref() {
                self.platform.notify(window, crate::APP_NAME, &title, 0);
            }
        }
        self.request_redraw();
    }

    /// Carries out `request` as `grant` allows: refused, asked about first
    /// (a command), or done.
    fn outside_do(&mut self, grant: &Grant, key: AppKey, request: Request) {
        let Some(tool) = crate::mcp::tools::find(&request.tool) else {
            let _ = request.reply.send(Err(format!("Flow has no tool {}.", request.tool)));
            return;
        };
        if !grant.may(tool.kind) {
            let _ = request.reply.send(Err(refused(&key.name, tool.kind)));
            return;
        }
        if tool.kind == Kind::Commands {
            let command = request.args["command"].as_str().unwrap_or_default().trim().to_string();
            if command.is_empty() {
                let _ = request.reply.send(Err("run_command needs a command.".into()));
                return;
            }
            // Typed, a control character would be a key, and the card
            // asking first can't show one: refused whatever the app may do.
            if let Some(c) = crate::shell::key_in(&command) {
                let _ = request.reply.send(Err(crate::ai::tools::command_holds_a_key(c)));
                return;
            }
            // Read as the shell of the pane it goes to reads it.
            let shells = self.outside_shells(&key, &request);
            match grant.command(&command, shells) {
                May::No => {
                    let _ = request.reply.send(Err(refused(&key.name, Kind::Commands)));
                }
                May::Ask => {
                    let risk = crate::ai::tools::command_risk_in(&command, shells);
                    let risk = if crate::ai::tools::always_asks(&command) { crate::ai::tools::Risk::High } else { risk };
                    let conn = request.conn;
                    self.outside_card(key, conn, McpAsk::Command { command, risk }, vec![request]);
                }
                May::Yes => self.outside_run(&key, request, command),
            }
            return;
        }
        let said = self.outside_tool(&key, tool.name, &request.args);
        let _ = request.reply.send(said);
    }

    /// The user's answer to the card in front: what the app may do (Allow,
    /// its switches as they were left; Deny, nothing), or whether its
    /// command runs.
    pub(super) fn answer_outside_card(&mut self, yes: bool) {
        if self.outside.cards.is_empty() {
            return;
        }
        let card = self.outside.cards.remove(0);
        match card.ui.ask {
            McpAsk::Use { kinds } => {
                let mut grant = Grant { name: card.key.name.clone(), path: card.key.path.clone(), ..Grant::default() };
                if yes {
                    for (kind, on) in Kind::ALL.iter().zip(kinds) {
                        grant.set(*kind, on);
                    }
                }
                self.outside.grants.put(grant.clone());
                self.save_grants();
                for request in card.waiting {
                    self.outside_do(&grant, card.key.clone(), request);
                }
            }
            McpAsk::Command { command, .. } => {
                for request in card.waiting {
                    if yes {
                        self.outside_run(&card.key, request, command.clone());
                    } else {
                        let _ = request.reply.send(Err("The user chose not to run it.".into()));
                    }
                }
            }
        }
        self.request_redraw();
    }

    /// Switches kind `i` on the card asking what an app may do.
    pub(super) fn switch_outside_kind(&mut self, i: usize) {
        if let Some(Card { ui: McpCard { ask: McpAsk::Use { kinds }, .. }, .. }) = self.outside.cards.first_mut() {
            if let Some(on) = kinds.get_mut(i) {
                *on = !*on;
                self.request_redraw();
            }
        }
    }

    /// Keeps what each app may do, as they've been answered and changed,
    /// and Settings shows it.
    pub(super) fn save_grants(&mut self) {
        if let Some(file) = &self.outside.file {
            if let Err(error) = self.outside.grants.save(file) {
                log::warn!("outside apps: {}: {error}", file.display());
            }
        }
        self.see_outside();
    }

    /// Settings' Outside apps as things stand: the apps remembered (read
    /// from `mcp.json` while Flow isn't listening), whether it listens, and
    /// `flow-mcp`'s path, where the folder of Flow's programs has it, or a
    /// build folder beside Flow does. Made up for snapshots.
    pub(super) fn see_outside(&mut self) {
        if self.snapshot.is_some() {
            self.outside.view = settings::OutsideView::made_up().into();
            return;
        }
        if self.outside.listening.is_none() {
            let file = Grants::file();
            self.outside.grants = Grants::load(&file);
            self.outside.file = Some(file);
        }
        let name = format!("flow-mcp{}", std::env::consts::EXE_SUFFIX);
        let flow_mcp = self
            .platform
            .games_dir()
            .map(|dir| dir.join(&name))
            .filter(|p| p.is_file())
            .or_else(|| std::env::current_exe().ok().map(|me| me.with_file_name(&name)).filter(|p| p.is_file()));
        self.outside.view = settings::OutsideView { apps: self.outside.grants.apps.clone(), listening: self.outside.listening.is_some(), flow_mcp }.into();
    }

    /// A change to one of Settings' Outside apps rows: `on`, a switch's new
    /// state; `pick`, a choice's.
    pub(super) fn outside_setting(&mut self, id: SettingId, change: Change, on: bool, pick: usize) {
        use crate::mcp::grants::Commands;
        match id {
            SettingId::McpEnabled => {
                self.config.mcp.enabled = on;
                self.follow_outside();
                self.see_outside();
            }
            SettingId::McpConnect => {
                let Some(path) = self.outside.view.flow_mcp.clone() else { return };
                let path = path.display().to_string();
                let text = match change {
                    Change::Action(1) => serde_json::to_string_pretty(&json!({ "mcpServers": { "flow": { "command": path } } })).unwrap_or_default(),
                    Change::Action(_) | Change::Toggle => {
                        // Windows' path (C:\Program Files\…) in double quotes,
                        // which PowerShell, cmd and Git Bash all read, and
                        // which no path there holds.
                        let quoted = if !path.contains([' ', '\'', '"']) {
                            path
                        } else if cfg!(windows) {
                            format!("\"{path}\"")
                        } else {
                            format!("'{}'", path.replace('\'', r"'\''"))
                        };
                        format!("claude mcp add flow -- {quoted}")
                    }
                    _ => return,
                };
                let copied = self.clipboard_set(text);
                self.show_toast(t(if copied { "toast.copied" } else { "toast.copy_failed" }));
            }
            SettingId::McpApp(i) => {
                if matches!(change, Change::Action(_) | Change::Toggle) && (i as usize) < self.outside.grants.apps.len() {
                    let gone = self.outside.grants.apps.remove(i as usize);
                    self.show_toast(tf("toast.mcp_forgot", &[("app", &gone.name)]));
                    self.save_grants();
                }
            }
            SettingId::McpKind(i, k) => {
                if let (Some(grant), Some(kind)) = (self.outside.grants.apps.get_mut(i as usize), Kind::ALL.get(k as usize)) {
                    grant.set(*kind, on);
                    self.save_grants();
                }
            }
            SettingId::McpCommands(i) => {
                if let (Some(grant), Some(commands)) = (self.outside.grants.apps.get_mut(i as usize), Commands::ALL.get(pick)) {
                    grant.commands = *commands;
                    self.save_grants();
                }
            }
            _ => {}
        }
    }

    /// The shells `request`'s command is read as (`tools::Shells::of`), by
    /// the pane `outside_run` sends it to: the one it names, else `key`'s
    /// app's own beside the user's, else the tab in front's shell, which
    /// that pane opens with.
    fn outside_shells(&self, key: &AppKey, request: &Request) -> crate::ai::tools::Shells {
        let pane = request.args["pane"].as_u64().or_else(|| self.outside.side.iter().find(|(k, _)| k == key).map(|(_, id)| *id));
        let kind = match pane.and_then(|id| self.pane_place(id)) {
            Some((t, p)) => Some(self.tabs[t].panes[p].session.profile.kind),
            None => self.tabs.get(self.active).and_then(|t| self.profiles.get(t.profile())).map(|p| p.kind),
        };
        kind.map_or(crate::ai::tools::Shells::Posix, crate::ai::tools::Shells::of)
    }

    /// Where pane `id` is: its tab and its place there.
    fn pane_place(&self, id: u64) -> Option<(usize, usize)> {
        self.tabs.iter().enumerate().find_map(|(t, tab)| tab.panes.iter().position(|p| p.id == id).map(|p| (t, p)))
    }

    fn owns(&self, key: &AppKey, pane: u64) -> bool {
        self.outside.own.iter().any(|(k, id)| k == key && *id == pane)
    }

    /// Tool `name` with `args`, for `key`'s app, allowed already.
    fn outside_tool(&mut self, key: &AppKey, name: &str, args: &Value) -> Result<String, String> {
        match name {
            "list_tabs" => Ok(self.outside_tabs(key)),
            "read_pane" => {
                let lines = args["lines"].as_u64().unwrap_or(200).clamp(1, 2000) as usize;
                let session = match args["pane"].as_u64() {
                    Some(id) => self.pane_place(id).map(|(t, p)| &self.tabs[t].panes[p].session).ok_or_else(|| no_pane(id))?,
                    None => self.session().ok_or("Flow has no tab open.")?,
                };
                let text = session.emu.lock().recent_text(lines);
                Ok(truncate_tail(&text, OUTSIDE_OUTPUT))
            }
            "open_tab" => {
                let profile = match args["shell"].as_str() {
                    Some(shell) => self.profiles.iter().position(|p| p.label.eq_ignore_ascii_case(shell) || p.id.eq_ignore_ascii_case(shell)).ok_or_else(|| {
                        format!("Flow has no shell {shell}. Its shells: {}.", self.profiles.iter().map(|p| p.label.as_str()).collect::<Vec<_>>().join(", "))
                    })?,
                    None => self.default_profile(),
                };
                let themes = crate::looks::ids(self.base.as_ref().unwrap_or(&self.config));
                let look = match args["theme"].as_str() {
                    Some(theme) if themes.iter().any(|id| id == theme) => Some(theme.to_string()),
                    Some(theme) => return Err(format!("Flow has no theme {theme}. Its themes: {}.", themes.join(", "))),
                    None => None,
                };
                let folder = match args["folder"].as_str() {
                    Some(folder) if std::path::Path::new(folder).is_dir() => Some(PathBuf::from(folder)),
                    Some(folder) => return Err(format!("There's no folder {folder}.")),
                    None => None,
                };
                let before = self.tabs.len();
                self.open_tab_at(profile, look, None, folder);
                if self.tabs.len() == before {
                    return Err("The tab didn't open.".into());
                }
                let slot = ui::outside_slot(&key.name);
                let tab = self.tabs.last_mut().ok_or("The tab didn't open.")?;
                let pane = &mut tab.panes[0];
                pane.by = Some(ui::PaneFor::Outside(slot));
                let id = pane.id;
                self.outside.own.push((key.clone(), id));
                Ok(json!({ "pane": id, "tab": self.tabs.len() }).to_string())
            }
            "split_pane" => {
                let id = args["pane"].as_u64().ok_or("split_pane needs a pane.")?;
                if !self.owns(key, id) {
                    return Err(format!("Pane {id} isn't yours: you may split only panes you opened, or yours beside the user's."));
                }
                let (t, p) = self.pane_place(id).ok_or_else(|| no_pane(id))?;
                self.select_tab(t);
                self.focus_pane(p);
                let before = self.tabs[t].panes.len();
                self.split_pane(args["across"].as_bool().unwrap_or(false));
                if self.tabs[t].panes.len() == before {
                    return Err("There's no room to split that pane.".into());
                }
                let slot = ui::outside_slot(&key.name);
                let new = self.tabs[t].panes.len() - 1;
                let pane = &mut self.tabs[t].panes[new];
                pane.by = Some(ui::PaneFor::Outside(slot));
                let id = pane.id;
                self.outside.own.push((key.clone(), id));
                Ok(json!({ "pane": id }).to_string())
            }
            "close_pane" => {
                let id = args["pane"].as_u64().ok_or("close_pane needs a pane.")?;
                if !self.owns(key, id) {
                    return Err(format!("Pane {id} isn't yours: the user's own panes and tabs can't be closed."));
                }
                let (t, p) = self.pane_place(id).ok_or_else(|| no_pane(id))?;
                if self.tabs[t].panes.len() > 1 {
                    let session = self.tabs[t].panes[p].session.id;
                    self.close_pane(t, session);
                } else if self.tabs.len() == 1 {
                    return Err("That's Flow's last tab, which stays open.".into());
                } else {
                    self.outside.closing.push(self.tabs[t].id);
                }
                self.outside.own.retain(|(_, pane)| *pane != id);
                self.outside.side.retain(|(_, pane)| *pane != id);
                Ok(format!("Closed pane {id}."))
            }
            "focus_pane" => {
                let id = args["pane"].as_u64().ok_or("focus_pane needs a pane.")?;
                let (t, p) = self.pane_place(id).ok_or_else(|| no_pane(id))?;
                self.select_tab(t);
                self.focus_pane(p);
                Ok(format!("Pane {id} is in front."))
            }
            "flow_settings" => {
                use base64::Engine as _;
                let asked = json!({ "do": args["do"], "key": args["key"], "value": args["value"] });
                let request = base64::engine::general_purpose::STANDARD.encode(asked.to_string());
                let session = self.session().map(|s| s.id).ok_or("Flow has no tab open.")?;
                // Flow's own tool always gets an answer.
                let answer = self.settings_answer(self.active, session, &request, SettingsAsker::Tool).unwrap_or_default();
                let text = serde_json::to_string_pretty(&answer).unwrap_or_default();
                if answer["ok"].as_bool().unwrap_or(false) {
                    Ok(text)
                } else {
                    Err(text)
                }
            }
            "flow_help" => flow_help(args["page"].as_str()),
            other => Err(format!("Flow has no tool {other}.")),
        }
    }

    /// `list_tabs`: each tab and its panes, as JSON.
    fn outside_tabs(&self, key: &AppKey) -> String {
        let labels = self.tab_labels();
        let tabs: Vec<Value> = self
            .tabs
            .iter()
            .enumerate()
            .map(|(t, tab)| {
                let panes: Vec<Value> = tab
                    .panes
                    .iter()
                    .enumerate()
                    .map(|(p, pane)| {
                        json!({
                            "pane": pane.id,
                            "shell": self.profiles.get(pane.profile).map(|s| s.label.clone()),
                            "folder": pane.session.cwd(),
                            "at_prompt": pane.session.at_prompt(),
                            "front": t == self.active && p == tab.focus,
                            "yours": self.owns(key, pane.id),
                            "working_for": pane.by.map(|b| b.label()),
                        })
                    })
                    .collect();
                json!({ "tab": t + 1, "title": labels.get(t).map(|l| l.label.clone()), "front": t == self.active, "panes": panes })
            })
            .collect();
        serde_json::to_string_pretty(&json!({ "tabs": tabs })).unwrap_or_default()
    }

    /// The session of `key`'s pane beside the user's, opened where it
    /// hasn't one (or it was closed).
    fn outside_side(&mut self, key: &AppKey) -> Option<SessionId> {
        if let Some((_, id)) = self.outside.side.iter().find(|(k, _)| k == key) {
            if let Some((t, p)) = self.pane_place(*id) {
                return Some(self.tabs[t].panes[p].session.id);
            }
        }
        self.outside.side.retain(|(k, _)| k != key);
        let session = self.open_side_pane(ui::PaneFor::Outside(ui::outside_slot(&key.name)))?;
        let id = self.tabs.get(self.active)?.panes.iter().find(|p| p.session.id == session)?.id;
        self.outside.side.push((key.clone(), id));
        self.outside.own.push((key.clone(), id));
        Some(session)
    }

    /// Runs `command` for `key`'s app: in the pane it named, one of its own,
    /// or its pane beside the user's.
    fn outside_run(&mut self, key: &AppKey, request: Request, command: String) {
        let session = match request.args["pane"].as_u64() {
            Some(id) if !self.owns(key, id) => Err(format!("Pane {id} isn't yours: commands run only in panes you opened, or yours beside the user's.")),
            Some(id) => self.pane_place(id).map(|(t, p)| self.tabs[t].panes[p].session.id).ok_or_else(|| no_pane(id)),
            None => self.outside_side(key).ok_or_else(|| "There's no room for a pane of yours beside the user's: open a tab of your own with open_tab.".to_string()),
        };
        let session = match session {
            Ok(session) if self.outside.runs.iter().any(|r| r.session == session) => Err("That pane is still running a command.".to_string()),
            other => other,
        };
        let session = match session {
            Ok(session) => session,
            Err(error) => {
                let _ = request.reply.send(Err(error));
                return;
            }
        };
        let timeout = Duration::from_secs(request.args["timeout_secs"].as_u64().unwrap_or(self.config.ai.command_timeout_secs).clamp(1, 600));
        self.outside.runs.push(Run { conn: request.conn, reply: request.reply, session, command, timeout, since: Instant::now(), running: false });
        self.advance_outside(None);
    }

    /// Moves outside apps' commands on: one waiting for its pane's prompt
    /// starts there, one running answers once it's done or its time is up.
    /// With the event loop, the tabs apps closed close too.
    pub(super) fn advance_outside(&mut self, el: Option<&ActiveEventLoop>) {
        if let Some(el) = el {
            for id in std::mem::take(&mut self.outside.closing) {
                if self.tabs.len() > 1 {
                    if let Some(i) = self.tabs.iter().position(|t| t.id == id) {
                        self.close_tab(i, el);
                    }
                }
            }
        }
        let mut i = 0;
        while i < self.outside.runs.len() {
            let run = &self.outside.runs[i];
            let Some(s) = self.session_by_id(run.session) else {
                let run = self.outside.runs.remove(i);
                let _ = run.reply.send(Err("Its pane was closed.".into()));
                continue;
            };
            if !run.running {
                if s.at_prompt().unwrap_or(run.since.elapsed() >= FRESH_PANE) {
                    // Letters past ASCII that bash at this prompt may take
                    // as keys (`Session::letter_as_keys`) aren't typed.
                    if s.letter_as_keys(&run.command).is_some() {
                        let run = self.outside.runs.remove(i);
                        let _ = run.reply.send(Err(i18n::english("card.letters_as_keys").into()));
                        continue;
                    }
                    s.run_command(&run.command);
                    let run = &mut self.outside.runs[i];
                    (run.running, run.since) = (true, Instant::now());
                } else if run.since.elapsed() > OPEN_WAIT {
                    let run = self.outside.runs.remove(i);
                    let _ = run.reply.send(Err("Its pane's shell didn't come to its prompt.".into()));
                    continue;
                }
                i += 1;
                continue;
            }
            match s.poll_command(run.timeout) {
                Some(result) => {
                    let folder = s.cwd().unwrap_or_default();
                    let run = self.outside.runs.remove(i);
                    let mut said = match result.exit_code {
                        Some(code) => format!("exit code: {code}\n"),
                        None if result.still_running => String::new(),
                        None => "exit code: unknown\n".into(),
                    };
                    if !folder.is_empty() {
                        said.push_str(&format!("folder: {folder}\n"));
                    }
                    if result.still_running {
                        said.push_str(&format!("still running after {} s; its output so far:\n", run.timeout.as_secs()));
                    } else {
                        said.push_str("output:\n");
                    }
                    said.push_str(&truncate_tail(&result.output, OUTSIDE_OUTPUT));
                    let _ = run.reply.send(Ok(said));
                }
                None => i += 1,
            }
        }
    }

    /// When Flow must look at outside apps' commands again, though nothing
    /// else wakes it: once a second while any wait or run.
    pub(super) fn outside_due(&self, now: Instant) -> Option<Instant> {
        (!self.outside.runs.is_empty()).then(|| now + Duration::from_secs(1))
    }
}

impl App {
    /// For snapshots: Settings' Outside apps with two apps made up
    /// (`settings-mcp.png`), then the card asking what an app may do
    /// (`mcp-ask.png`) and one asking to run a risky command
    /// (`mcp-command.png`), all made up and put back after.
    pub(super) fn show_outside(&mut self, dir: &std::path::Path) {
        let was_open = settings::Section::OutsideApps.open(&self.config);
        settings::Section::OutsideApps.set_open(&mut self.config, true);
        let enabled = std::mem::replace(&mut self.config.mcp.enabled, true);
        self.see_outside();
        let at = settings::rows(&self.settings_entries()).iter().position(|r| r.id == SettingId::McpCommands(0)).unwrap_or(0);
        self.menu = Some(SettingsMenu { selected: at, ..SettingsMenu::default() });
        self.capture(&dir.join("settings-mcp.png"), true);
        self.menu = None;
        self.config.mcp.enabled = enabled;
        settings::Section::OutsideApps.set_open(&mut self.config, was_open);
        let key = AppKey { name: "claude-code".into(), path: "/opt/homebrew/bin/claude".into() };
        let card = |ask| Card { key: key.clone(), conn: 0, ui: McpCard { app: key.name.clone(), host: key.path.clone(), ask }, waiting: Vec::new() };
        self.outside.cards = vec![card(McpAsk::Use { kinds: [true, true, false, false] })];
        self.capture(&dir.join("mcp-ask.png"), true);
        let command = "rm -rf ~/Library/Caches/old-builds".to_string();
        self.outside.cards = vec![card(McpAsk::Command { risk: crate::ai::tools::command_risk(&command), command })];
        self.capture(&dir.join("mcp-command.png"), true);
        self.outside.cards.clear();
    }
}

fn no_pane(id: u64) -> String {
    format!("There's no pane {id}: list_tabs says which there are.")
}
