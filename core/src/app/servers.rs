//! The MCP servers the assistant uses ([`crate::mcp::client`]), on Flow's
//! side: `mcp-servers.json` read, each server the user switched on in
//! Settings connected on a thread of its own (signed in to where it asks,
//! [`App::sign_in_server`]), the tools and resources they turned on offered
//! to the assistant (never its helpers), each call approved as the server's
//! choice says and run as a job the tool batch waits on until the server's
//! answer comes back (or is given up, the server told), its prompts started
//! from the panel (`/<server>:<prompt>`), and what it asks the user put on
//! a card of its own ([`super::server_ask`]).

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde_json::{json, Map, Value};

use super::server_ask::{AskCard, Purpose};
use super::*;
use crate::ai::ServerTool;
use crate::history::looks_secret;
use crate::mcp::client::servers::{self as server_file, Approval, How, Server};
use crate::mcp::client::{elicit, names, oauth, Fail, Link, Listing, ServerEvent, ToolInfo};

/// How a server stands, for Settings.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Standing {
    Connecting,
    Connected { listing: Listing, signed_in: bool },
    Failed(String),
    /// It needs the user to sign in, as it said.
    SignIn(oauth::Challenge),
    /// Signing in, in the browser.
    SigningIn,
}

/// A call waiting for the user to open a link the server needs first
/// (MCP's -32042), then called again.
pub(super) struct LinkWait {
    pub server: String,
    tool: String,
    args: Value,
    /// The links' ids, which the server says are done with.
    ids: Vec<String>,
}

/// The servers, as Flow's window keeps them.
#[derive(Default)]
pub(super) struct Servers {
    /// `mcp-servers.json` as last read, and what in it didn't read.
    pub list: Vec<Server>,
    pub problems: Vec<String>,
    /// Each server connected or connecting, by name, with how it was
    /// started, so a change in the file starts it again.
    links: HashMap<String, (Link, How)>,
    pub standing: HashMap<String, Standing>,
    /// A server just added in Settings, by name, whose command or URL is
    /// still to be typed: it goes in the file once it is.
    draft: Option<String>,
    /// Remove pressed once on a server, by name, and when: a second press
    /// soon after removes it.
    remove_at: Option<(String, Instant)>,
    /// A sign-in underway, by server: its flag for giving it up.
    signing: HashMap<String, Arc<AtomicBool>>,
    /// The access token a sign-in just got, for the server's next connect.
    fresh: HashMap<String, oauth::Access>,
    /// What servers ask the user, the first on the card in front.
    pub asks: VecDeque<AskCard>,
    /// When each server last asked, for keeping one from asking too often.
    asked: Vec<(String, Instant)>,
    /// Calls waiting for the user to finish in the browser, by job.
    pub links_open: HashMap<String, LinkWait>,
    /// Prompts being got, by their request, with their server and name.
    prompting: HashMap<String, (String, String)>,
    next_prompt: u64,
    /// Settings' MCP servers, worked out when Settings opens and as they
    /// change, not each frame.
    pub view: std::rc::Rc<settings::ServersView>,
}

impl Servers {
    /// What server `name` offers, once connected.
    pub fn listing(&self, name: &str) -> Option<&Listing> {
        match self.standing.get(name) {
            Some(Standing::Connected { listing, .. }) => Some(listing),
            _ => None,
        }
    }

    /// The tools of server `name`, once connected.
    pub fn tools(&self, name: &str) -> &[ToolInfo] {
        self.listing(name).map_or(&[], |l| &l.tools)
    }
}

/// The credential store's account for a server's sign-in.
fn sign_in_account(server: &str) -> String {
    format!("mcp-oauth-{server}")
}

/// What's kept of server `server`'s sign-in.
fn kept_sign_in(server: &str) -> Option<oauth::Kept> {
    secrets::load_long(&sign_in_account(server)).and_then(|text| serde_json::from_str(&text).ok())
}

fn keep_sign_in(server: &str, kept: &oauth::Kept) {
    let text = serde_json::to_string(kept).unwrap_or_default();
    if let Err(error) = secrets::store_long(&sign_in_account(server), &text) {
        log::warn!("could not keep the sign-in to {server}: {error}");
    }
}

impl App {
    /// Reads `mcp-servers.json` again (never for snapshots, which make
    /// theirs up), then connects as the config says.
    pub(super) fn read_servers(&mut self) {
        if self.snapshot.is_some() {
            self.servers.view = settings::ServersView::made_up().into();
            return;
        }
        (self.servers.list, self.servers.problems) = server_file::load(&server_file::file());
        for problem in &self.servers.problems {
            log::warn!("mcp-servers.json: {problem}");
        }
        self.follow_servers();
    }

    /// Connects each server switched on (`ai.mcp_servers_on`) that isn't
    /// yet, starts one again whose command or URL changed, and lets go of
    /// those switched off or gone from the file. The agent hears of any
    /// change in what it's offered.
    pub(super) fn follow_servers(&mut self) {
        if self.snapshot.is_some() {
            return;
        }
        let before = self.offered_by_servers();
        let on: Vec<Server> = self.servers.list.iter().filter(|s| self.config.ai.mcp_servers_on.contains(&s.name)).cloned().collect();
        self.servers.links.retain(|name, (_, how)| on.iter().any(|s| &s.name == name && &s.how == how));
        // A server that failed, or waits for a sign-in, stays so while it's
        // on, until Connect or a sign-in tries it again.
        let waits = |standing: &Standing| matches!(standing, Standing::Failed(_) | Standing::SignIn(_) | Standing::SigningIn);
        self.servers.standing.retain(|name, standing| self.servers.links.contains_key(name) || (waits(standing) && on.iter().any(|s| &s.name == name)));
        for server in on {
            if self.servers.links.contains_key(&server.name) || self.servers.standing.get(&server.name).is_some_and(waits) {
                continue;
            }
            self.connect_server(server);
        }
        if self.offered_by_servers() != before {
            self.agent.configure(self.agent_settings());
        }
        self.see_servers();
    }

    /// What the agent is offered of the servers: their tools, and whether
    /// it may read resources.
    fn offered_by_servers(&self) -> (Vec<ServerTool>, bool) {
        (self.server_tools(), !self.resource_servers().is_empty())
    }

    /// Settings' MCP servers as they stand now: the file's servers, then
    /// one just added.
    pub(super) fn see_servers(&mut self) {
        if self.snapshot.is_some() {
            return;
        }
        let on = &self.config.ai.mcp_servers_on;
        let mut rows: Vec<settings::ServerRow> = self
            .servers
            .list
            .iter()
            .map(|s| {
                let (standing, signed_in) = match self.servers.standing.get(&s.name) {
                    Some(Standing::Connecting) => (settings::ServerStanding::Connecting, false),
                    Some(Standing::Connected { signed_in, .. }) => (settings::ServerStanding::Connected, *signed_in),
                    Some(Standing::Failed(why)) => (settings::ServerStanding::Failed(why.clone()), false),
                    Some(Standing::SignIn(_)) => (settings::ServerStanding::SignIn, false),
                    Some(Standing::SigningIn) => (settings::ServerStanding::SigningIn, false),
                    None => (settings::ServerStanding::Off, false),
                };
                let listing = self.servers.listing(&s.name);
                let tools = self.servers.tools(&s.name).iter().map(|t| settings::ServerToolRow { name: t.name.clone(), description: t.description.clone(), read_only: t.read_only }).collect();
                settings::ServerRow {
                    name: s.name.clone(),
                    line: s.how.line(),
                    remote: matches!(s.how, How::Http { .. }),
                    on: on.contains(&s.name),
                    standing,
                    tools,
                    resources: listing.map_or(0, |l| l.resources.len() + l.templates.len()),
                    prompts: listing.map_or_else(Vec::new, |l| l.prompts.iter().map(|p| p.name.clone()).collect()),
                    signed_in,
                    client_id: s.oauth.client_id.clone(),
                }
            })
            .collect();
        if let Some(name) = &self.servers.draft {
            rows.push(settings::ServerRow {
                name: name.clone(),
                line: String::new(),
                remote: false,
                on: false,
                standing: settings::ServerStanding::Off,
                tools: Vec::new(),
                resources: 0,
                prompts: Vec::new(),
                signed_in: false,
                client_id: None,
            });
        }
        let file = server_file::file();
        let file = match dirs::home_dir().and_then(|home| file.strip_prefix(&home).ok().map(|rest| std::path::Path::new("~").join(rest))) {
            Some(short) => short.display().to_string(),
            None => file.display().to_string(),
        };
        self.servers.view = settings::ServersView { servers: rows, problems: self.servers.problems.clone(), file }.into();
    }

    /// The server Settings shows at `i`, by name.
    fn server_at(&self, i: u16) -> Option<settings::ServerRow> {
        self.servers.view.servers.get(i as usize).cloned()
    }

    /// Whether tool `k` of the server Settings shows at `i` is on.
    pub(super) fn server_tool_on(&self, i: u16, k: u16) -> bool {
        self.server_at(i).and_then(|s| s.tools.get(k as usize).map(|t| tool_key(&s.name, &t.name))).is_some_and(|key| self.config.ai.mcp_tools_on.contains(&key))
    }

    /// Whether the assistant may read the resources of the server Settings
    /// shows at `i`.
    pub(super) fn server_resources_on(&self, i: u16) -> bool {
        self.server_at(i).is_some_and(|s| self.config.ai.mcp_resources_on.contains(&s.name))
    }

    /// A change in Settings' MCP servers; true where it's done with the
    /// row (typing in it), so nothing more is made of the change.
    pub(super) fn server_setting(&mut self, id: SettingId, change: Change, on: bool, pick: usize) -> bool {
        let pressed = |n: usize| matches!(change, Change::Action(a) if a == n) || (n == 0 && change == Change::Toggle);
        let act_of = |acts: Vec<settings::ServerAct>| match change {
            Change::Action(a) => acts.get(a).copied(),
            Change::Toggle => acts.first().copied(),
            _ => None,
        };
        match id {
            SettingId::ServersFile if pressed(1) => self.open_servers_file(),
            SettingId::ServersFile if pressed(0) => {
                let taken = self.servers.view.servers.iter().map(|s| s.name.clone()).collect();
                if let Some(m) = self.menu.as_mut() {
                    m.naming = Some(Naming::server(taken));
                }
                return true;
            }
            SettingId::Server(i) | SettingId::ServerAuth(i) => {
                let Some(server) = self.server_at(i) else { return false };
                let acts = if matches!(id, SettingId::Server(_)) { settings::server_acts(&server) } else { settings::server_auth_acts(&server) };
                match act_of(acts) {
                    Some(settings::ServerAct::Connect) => self.switch_server(&server.name, true),
                    Some(settings::ServerAct::Disconnect) => self.switch_server(&server.name, false),
                    Some(settings::ServerAct::Remove) => self.remove_server(&server.name),
                    Some(settings::ServerAct::SignIn) => self.sign_in_server(&server.name),
                    Some(settings::ServerAct::CancelSignIn) => self.cancel_sign_in(&server.name),
                    Some(settings::ServerAct::SignOut) => self.sign_out_server(&server.name),
                    Some(settings::ServerAct::ClientId) => {
                        if let Some(m) = self.menu.as_mut() {
                            m.naming = Some(Naming::client_id(&server.name, server.client_id.as_deref()));
                        }
                        return true;
                    }
                    Some(settings::ServerAct::Token) => {
                        self.ask_server_token(&server.name);
                        return true;
                    }
                    None => {}
                }
            }
            SettingId::ServerHow(i) => {
                if matches!(change, Change::Toggle | Change::Action(_)) {
                    let line = self.server_at(i).map(|s| s.line).unwrap_or_default();
                    let field = TextField::long(&line);
                    if let Some(m) = self.menu.as_mut() {
                        m.editing = Some(Editing { id, field, was: settings::Was::Text(line) });
                    }
                }
                return true;
            }
            SettingId::ServerApproval(i) => {
                if let (Some(server), Some(approval)) = (self.server_at(i), settings::MCP_APPROVALS.get(pick)) {
                    match *approval {
                        "ask" => self.config.ai.mcp_approval.remove(&server.name),
                        other => self.config.ai.mcp_approval.insert(server.name, other.to_string()),
                    };
                }
            }
            SettingId::ServerResources(i) => {
                let Some(server) = self.server_at(i) else { return false };
                put_in_list(&mut self.config.ai.mcp_resources_on, server.name, on);
                self.tools_switched();
            }
            SettingId::ServerTools(i) => {
                let Some(server) = self.server_at(i) else { return false };
                let keys: Vec<String> = server.tools.iter().map(|t| tool_key(&server.name, &t.name)).collect();
                let all = keys.iter().all(|k| self.config.ai.mcp_tools_on.contains(k));
                // All on, All off; Enter turns them all on, or off where they all are.
                let turn_on = match change {
                    Change::Action(0) => true,
                    Change::Action(_) => false,
                    Change::Toggle => !all,
                    Change::Step(n) => n > 0,
                    _ => return false,
                };
                for key in keys {
                    put_in_list(&mut self.config.ai.mcp_tools_on, key, turn_on);
                }
                self.tools_switched();
            }
            SettingId::ServerTool(i, k) => {
                let Some(server) = self.server_at(i) else { return false };
                if let Some(tool) = server.tools.get(k as usize) {
                    put_in_list(&mut self.config.ai.mcp_tools_on, tool_key(&server.name, &tool.name), on);
                    self.tools_switched();
                }
            }
            _ => {}
        }
        if let Some(m) = self.menu.as_mut() {
            m.dirty = true;
        }
        self.see_servers();
        false
    }

    /// The tools or resources turned on changed: the agent is offered them
    /// from its next request.
    fn tools_switched(&mut self) {
        self.agent.configure(self.agent_settings());
    }

    /// Connect or Disconnect: server `name` switched on or off
    /// (`ai.mcp_servers_on`). Connecting one that failed tries it afresh.
    fn switch_server(&mut self, name: &str, on: bool) {
        put_in_list(&mut self.config.ai.mcp_servers_on, name.to_string(), on);
        if on {
            self.reconnect_server(name);
        } else {
            self.cancel_sign_in(name);
            self.follow_servers();
        }
    }

    /// Remove, pressed twice: server `name` out of the file and the config,
    /// its token and sign-in forgotten.
    fn remove_server(&mut self, name: &str) {
        let again = self.servers.remove_at.take().is_some_and(|(was, at)| was == name && at.elapsed() < CONFIRM_WINDOW);
        if !again {
            self.servers.remove_at = Some((name.to_string(), Instant::now()));
            self.show_toast(tf("toast.server_remove_again", &[("server", &name)]));
            return;
        }
        if self.servers.draft.as_deref() == Some(name) {
            self.servers.draft = None;
        } else {
            let list: Vec<Server> = self.servers.list.iter().filter(|s| s.name != name).cloned().collect();
            if let Err(error) = server_file::save(&server_file::file(), &list) {
                return self.show_toast(tf("toast.servers_not_saved", &[("error", &error)]));
            }
            self.servers.list = list;
            let _ = secrets::forget_server_token(name);
            let _ = secrets::forget_long(&sign_in_account(name));
        }
        self.cancel_sign_in(name);
        let ai = &mut self.config.ai;
        ai.mcp_servers_on.retain(|s| s != name);
        ai.mcp_resources_on.retain(|s| s != name);
        ai.mcp_approval.remove(name);
        let prefix = format!("{name}:");
        ai.mcp_tools_on.retain(|k| !k.starts_with(&prefix));
        self.servers.standing.remove(name);
        self.follow_servers();
        self.show_toast(tf("toast.server_removed", &[("server", &name)]));
    }

    /// Add server…'s name: refused where it can't name a server or one has
    /// it, else a new row whose command or URL is typed at once.
    pub(super) fn add_server(&mut self, name: &str) {
        if !server_file::valid_name(name) {
            return self.show_toast(t("toast.server_bad_name"));
        }
        if self.servers.view.servers.iter().any(|s| s.name == name) {
            return self.show_toast(tf("toast.server_taken", &[("server", &name)]));
        }
        self.servers.draft = Some(name.to_string());
        self.see_servers();
        // Its command is typed in its row in place: a search would hide it,
        // so it ends, and the section opens.
        if let Some(m) = self.menu.as_mut() {
            m.search = None;
            m.shown.set(None);
        }
        if !settings::Section::McpServers.open(&self.config) {
            settings::Section::McpServers.set_open(&mut self.config, true);
        }
        let at = self.servers.view.servers.len().saturating_sub(1) as u16;
        let id = SettingId::ServerHow(at);
        let rows: Vec<SettingId> = settings::rows(&self.settings_entries()).iter().map(|r| r.id).collect();
        if let (Some(row), Some(m)) = (rows.iter().position(|r| *r == id), self.menu.as_mut()) {
            m.selected = row;
            m.editing = Some(Editing { id, field: TextField::long(""), was: settings::Was::Text(String::new()) });
        }
        self.request_redraw();
    }

    /// A server's command or URL typed (`None`: Esc). What's understood
    /// goes in the file and the server connects again with it where it's
    /// on; a server just added and left empty goes.
    pub(super) fn server_line_typed(&mut self, i: u16, typed: Option<String>) {
        let Some(row) = self.server_at(i) else { return };
        let draft = self.servers.draft.as_deref() == Some(row.name.as_str());
        let typed = typed.map(|t| t.trim().to_string()).filter(|t| !t.is_empty() && *t != row.line);
        let Some(line) = typed else {
            if draft && row.line.is_empty() {
                self.servers.draft = None;
                self.see_servers();
            }
            return;
        };
        let Some(how) = How::from_line(&line) else {
            return self.show_toast(t("toast.server_bad_line"));
        };
        let mut list = self.servers.list.clone();
        match list.iter_mut().find(|s| s.name == row.name) {
            Some(server) => server.how = how,
            None => list.push(Server { name: row.name.clone(), how, oauth: Default::default() }),
        }
        if let Err(error) = server_file::save(&server_file::file(), &list) {
            return self.show_toast(tf("toast.servers_not_saved", &[("error", &error)]));
        }
        self.servers.list = list;
        if draft {
            self.servers.draft = None;
            self.show_toast(tf("toast.server_added", &[("server", &row.name)]));
        }
        self.servers.standing.remove(&row.name);
        self.follow_servers();
    }

    /// Client ID…'s box: the client ID registered for Flow with server
    /// `name`'s sign-in, kept in its file (empty: none, so Flow registers
    /// itself).
    pub(super) fn keep_client_id(&mut self, name: &str, id: &str) {
        let mut list = self.servers.list.clone();
        let Some(server) = list.iter_mut().find(|s| s.name == name) else { return };
        server.oauth.client_id = Some(id.trim().to_string()).filter(|id| !id.is_empty());
        if let Err(error) = server_file::save(&server_file::file(), &list) {
            return self.show_toast(tf("toast.servers_not_saved", &[("error", &error)]));
        }
        self.servers.list = list;
        self.show_toast(tf(if id.trim().is_empty() { "toast.server_client_id_cleared" } else { "toast.server_client_id_kept" }, &[("server", &name)]));
        self.see_servers();
    }

    /// Starts talking to `server`: a remote one with the token the user
    /// pasted for it, else its sign-in, else nothing.
    fn connect_server(&mut self, server: Server) {
        let waker = self.waker.clone();
        let tell: Arc<dyn Fn(ServerEvent) + Send + Sync> = Arc::new(move |event| waker.send(UserEvent::McpServer(event)));
        let name = server.name.clone();
        let how = server.how.clone();
        let fresh = self.servers.fresh.remove(&name);
        let credentials: crate::mcp::client::Credentials = match &how {
            How::Http { .. } => {
                let name = name.clone();
                Box::new(move || {
                    if let Some(token) = secrets::load_server_token(&name) {
                        return oauth::Auth::Token(token);
                    }
                    match kept_sign_in(&name) {
                        Some(kept) if kept.refresh.is_some() || fresh.is_some() => {
                            let server = name.clone();
                            oauth::Auth::Session(Box::new(oauth::Session::new(kept, fresh, Box::new(move |k| keep_sign_in(&server, k)))))
                        }
                        _ => oauth::Auth::None,
                    }
                })
            }
            How::Stdio { .. } => Box::new(|| oauth::Auth::None),
        };
        let link = Link::start(server, credentials, vec![crate::apps::bin_dir()], tell);
        self.servers.links.insert(name.clone(), (link, how));
        self.servers.standing.insert(name, Standing::Connecting);
    }

    /// Connects server `name` again, as Settings' Connect does after it
    /// failed, once its token changed or it's signed in.
    pub(super) fn reconnect_server(&mut self, name: &str) {
        self.servers.links.remove(name);
        self.servers.standing.remove(name);
        self.follow_servers();
    }

    /// Settings' Sign in: server `name`'s sign-in, in the browser, on a
    /// thread of its own.
    fn sign_in_server(&mut self, name: &str) {
        let Some(server) = self.servers.list.iter().find(|s| s.name == name).cloned() else { return };
        let How::Http { url, .. } = &server.how else { return };
        let challenge = match self.servers.standing.get(name) {
            Some(Standing::SignIn(challenge)) => challenge.clone(),
            _ => oauth::Challenge::default(),
        };
        let cancel = Arc::new(AtomicBool::new(false));
        self.servers.signing.insert(name.to_string(), cancel.clone());
        self.servers.standing.insert(name.to_string(), Standing::SigningIn);
        let ask = oauth::SignIn {
            server: name.to_string(),
            url: url.clone(),
            challenge,
            client_id: server.oauth.client_id.clone(),
            port: server.oauth.port.unwrap_or(oauth::CALLBACK_PORT),
            before: None,
        };
        let waker = self.waker.clone();
        let server = name.to_string();
        let _ = std::thread::Builder::new().name("flow-mcp-sign-in".into()).spawn(move || {
            let ask = oauth::SignIn { before: kept_sign_in(&server), ..ask };
            let tell = |event| waker.send(UserEvent::McpServer(event));
            let open = |url: &str| tell(ServerEvent::OpenToSignIn { server: server.clone(), url: url.to_string() });
            match oauth::sign_in(&ask, &open, &cancel) {
                Ok((kept, access)) => {
                    keep_sign_in(&server, &kept);
                    tell(ServerEvent::SignedIn { server: server.clone(), access });
                }
                Err(why) => tell(ServerEvent::SignInFailed { server: server.clone(), why }),
            }
        });
        self.show_toast(tf("toast.server_signing_in", &[("server", &name)]));
    }

    /// Settings' Cancel: the sign-in to server `name` given up.
    fn cancel_sign_in(&mut self, name: &str) {
        if let Some(cancel) = self.servers.signing.remove(name) {
            cancel.store(true, Ordering::Relaxed);
            if matches!(self.servers.standing.get(name), Some(Standing::SigningIn)) {
                self.servers.standing.insert(name.to_string(), Standing::SignIn(oauth::Challenge::default()));
            }
        }
    }

    /// Settings' Sign out: server `name`'s tokens forgotten (its client
    /// kept, so signing in again needn't register Flow again), and the
    /// server connected again without them.
    fn sign_out_server(&mut self, name: &str) {
        if let Some(mut kept) = kept_sign_in(name) {
            kept.refresh = None;
            keep_sign_in(name, &kept);
        }
        self.show_toast(tf("toast.server_signed_out", &[("server", &name)]));
        self.reconnect_server(name);
    }

    /// What a server's thread, or a sign-in's, said.
    pub(super) fn server_event(&mut self, event: ServerEvent) {
        let before = self.offered_by_servers();
        match event {
            ServerEvent::Connected { server, listing, signed_in } => {
                if self.servers.links.contains_key(&server) {
                    log::info!("MCP server {server}: connected, {} tools, {} resources, {} prompts", listing.tools.len(), listing.resources.len() + listing.templates.len(), listing.prompts.len());
                    self.servers.standing.insert(server, Standing::Connected { listing, signed_in });
                }
            }
            ServerEvent::Failed { server, why } => {
                if self.servers.links.remove(&server).is_some() {
                    log::warn!("MCP server {server}: {why}");
                    self.servers.standing.insert(server.clone(), Standing::Failed(why));
                    self.server_gone(&server);
                }
            }
            ServerEvent::NeedsSignIn { server, challenge } => {
                if self.servers.links.remove(&server).is_some() {
                    log::info!("MCP server {server}: needs a sign-in");
                    self.servers.standing.insert(server.clone(), Standing::SignIn(challenge));
                    self.show_toast(tf("toast.server_needs_sign_in", &[("server", &server)]));
                    self.server_gone(&server);
                }
            }
            ServerEvent::Answer { job, said, server } => return self.server_answered(&server, &job, said),
            ServerEvent::Prompted { job, said, .. } => return self.prompt_got(&job, said),
            ServerEvent::Asks { server, id, ask } => return self.server_asks(server, id, ask),
            ServerEvent::AskWithdrawn { server, id } => return self.ask_withdrawn(&server, &id),
            ServerEvent::LinkDone { server, elicitation } => return self.link_done(&server, &elicitation),
            ServerEvent::OpenToSignIn { server, url } => {
                if self.servers.signing.contains_key(&server) && !self.open_url(&url) {
                    self.show_toast(t("toast.page_failed"));
                }
                return;
            }
            ServerEvent::SignedIn { server, access } => {
                if self.servers.signing.remove(&server).is_some() {
                    self.servers.fresh.insert(server.clone(), access);
                    self.show_toast(tf("toast.server_signed_in", &[("server", &server)]));
                    self.reconnect_server(&server);
                }
            }
            ServerEvent::SignInFailed { server, why } => {
                if self.servers.signing.remove(&server).is_some() {
                    log::warn!("signing in to MCP server {server}: {why}");
                    self.servers.standing.insert(server.clone(), Standing::Failed(why.clone()));
                    self.show_toast(tf("toast.server_sign_in_failed", &[("server", &server), ("why", &why)]));
                }
            }
        }
        if self.offered_by_servers() != before {
            self.agent.configure(self.agent_settings());
        }
        // Settings shows it connected, what it offers, or why it failed.
        self.see_servers();
        self.request_redraw();
    }

    /// Server `server` went away: what it asked the user is withdrawn.
    fn server_gone(&mut self, server: &str) {
        self.servers.asks.retain(|card| card.server != server || matches!(card.purpose, Purpose::Link { .. }));
    }

    /// The tools the assistant is offered: each one turned on
    /// (`ai.mcp_tools_on`) of each server connected, in the file's order.
    pub(super) fn server_tools(&self) -> Vec<ServerTool> {
        let on = &self.config.ai.mcp_tools_on;
        let mut out = Vec::new();
        for server in &self.servers.list {
            for tool in self.servers.tools(&server.name) {
                if on.contains(&tool_key(&server.name, &tool.name)) {
                    out.push(ServerTool {
                        name: names::for_model(&server.name, &tool.name),
                        server: server.name.clone(),
                        tool: tool.name.clone(),
                        description: tool.description.clone(),
                        schema: tool.schema.clone(),
                        read_only: tool.read_only,
                    });
                }
            }
        }
        out
    }

    /// The servers whose resources the assistant may read: connected, with
    /// some, and turned on (`ai.mcp_resources_on`).
    pub(super) fn resource_servers(&self) -> Vec<String> {
        self.servers
            .list
            .iter()
            .filter(|s| self.config.ai.mcp_resources_on.contains(&s.name))
            .filter(|s| self.servers.listing(&s.name).is_some_and(|l| !l.resources.is_empty() || !l.templates.is_empty()))
            .map(|s| s.name.clone())
            .collect()
    }

    /// The tool the model calls `name`, where it's still offered.
    fn server_tool(&self, name: &str) -> Option<ServerTool> {
        self.server_tools().into_iter().find(|t| t.name == name)
    }

    /// How server `name`'s calls are approved, as the user chose.
    pub(super) fn server_approval(&self, name: &str) -> Approval {
        self.config.ai.mcp_approval.get(name).map(|key| Approval::of(key)).unwrap_or_default()
    }

    /// Whether a call of the tool the model calls `name` waits for the
    /// user's yes outside guide mode, as its server's choice says. One no
    /// longer offered doesn't: it fails as it starts.
    pub(super) fn server_call_asks(&self, name: &str) -> bool {
        self.server_tool(name).is_some_and(|t| self.server_approval(&t.server).asks(t.read_only))
    }

    /// Whether reading one of server `server`'s resources waits for the
    /// user's yes outside guide mode: as a read-only tool of its would.
    pub(super) fn resource_read_asks(&self, server: &str) -> bool {
        self.server_approval(server).asks(true)
    }

    /// What a call's card says: the server and tool, and its arguments.
    pub(super) fn server_card(&self, name: &str, args: &Value) -> (String, String) {
        let title = match self.server_tool(name) {
            Some(t) => tf("card.server_call", &[("server", &t.server), ("tool", &t.tool)]),
            None => name.to_string(),
        };
        (title, card_args(args))
    }

    /// Sends the job in hand's call to its server; the batch waits for the
    /// answer ([`App::server_answered`]), or past the command timeout.
    pub(super) fn start_server_call(&mut self, name: &str, args: Value) {
        let Some(tool) = self.server_tool(name) else {
            return self.finish_job(Err(format!(
                "The tool {name} is no longer offered: its server was disconnected, or the user turned it off in Flow's Settings."
            )));
        };
        let Some((link, _)) = self.servers.links.get(&tool.server) else {
            return self.finish_job(Err(format!("The MCP server {} isn't connected.", tool.server)));
        };
        let Some(batch) = self.tools.as_ref() else { return };
        let job = batch.jobs[batch.current].id.clone();
        link.call(&job, &tool.tool, args, SERVING_LONGEST);
        self.serving(&tool.server);
    }

    /// How long a server is given to answer, as the user's command timeout
    /// says: the window's clock for a call or a read, which gives it up
    /// (`advance_tools`), and the link's own for a prompt.
    pub(super) fn server_wait(&self) -> Duration {
        Duration::from_secs(self.config.ai.command_timeout_secs.max(10))
    }

    /// The job in hand waits for server `server`'s answer, up to the
    /// command timeout, by the window's clock alone: the link waits far
    /// longer (`SERVING_LONGEST`), so a call out of time is given up here,
    /// journaled so and its server told, never failed there first. A call
    /// made again after its links keeps the time it was first made.
    fn serving(&mut self, server: &str) {
        let until = Instant::now() + self.server_wait();
        let Some(batch) = self.tools.as_mut() else { return };
        let idx = batch.current;
        let since = match batch.jobs[idx].state {
            JobState::Serving { since, .. } => since,
            _ => Instant::now(),
        };
        batch.jobs[idx].state = JobState::Serving { until, server: server.to_string(), since };
        let entry = batch.jobs[idx].entry;
        if let Some(card) = self.ai.card_mut(entry) {
            card.status = CardStatus::Running;
            card.started = Some(Instant::now());
            card.result.clear();
        }
        self.request_redraw();
    }

    /// `mcp_resources`: what the servers whose resources are on offer to
    /// be read, from what they listed (one server's alone where it's named).
    pub(super) fn list_resources(&self, only: Option<&str>) -> Result<String, String> {
        let servers = self.resource_servers();
        if servers.is_empty() {
            return Err("No MCP server's resources are turned on in Flow's Settings.".into());
        }
        if let Some(name) = only.filter(|n| !servers.iter().any(|s| s == n)) {
            return Err(format!("The MCP server {name} isn't connected, or its resources aren't turned on. Those whose are: {}.", servers.join(", ")));
        }
        let mut out = String::new();
        for name in servers.iter().filter(|s| only.is_none_or(|o| o == *s)) {
            let Some(listing) = self.servers.listing(name) else { continue };
            out.push_str(&format!("Server {name}:\n"));
            for r in &listing.resources {
                let mime = r.mime.as_deref().map(|m| format!(" ({m})")).unwrap_or_default();
                let about = if r.description.is_empty() { String::new() } else { format!(": {}", r.description) };
                out.push_str(&format!("- {} {}{mime}{about}\n", r.uri, r.name));
            }
            for t in &listing.templates {
                let about = if t.description.is_empty() { String::new() } else { format!(": {}", t.description) };
                out.push_str(&format!("- template {} {}{about}\n", t.uri_template, t.name));
            }
        }
        Ok(truncate_head(out.trim_end(), MAX_TOOL_OUTPUT))
    }

    /// `mcp_read_resource`: resource `uri` of server `server` read, the
    /// batch waiting for it as for a call.
    pub(super) fn start_read_resource(&mut self, server: &str, uri: &str) {
        if !self.resource_servers().iter().any(|s| s == server) {
            return self.finish_job(Err(format!("The MCP server {server} isn't connected, or its resources aren't turned on in Flow's Settings.")));
        }
        let Some((link, _)) = self.servers.links.get(server) else {
            return self.finish_job(Err(format!("The MCP server {server} isn't connected.")));
        };
        let Some(batch) = self.tools.as_ref() else { return };
        let job = batch.jobs[batch.current].id.clone();
        link.read(&job, uri, SERVING_LONGEST);
        self.serving(server);
    }

    /// A server answered job `job`: its result goes to the model, cut to
    /// what a tool result may hold, from its start (a file read, a tool's
    /// answer, read from the top). A call that needs the user to open
    /// links first puts them on cards. One that no longer waits (stopped,
    /// or timed out) is let go.
    fn server_answered(&mut self, server: &str, job: &str, said: Result<String, Fail>) {
        let waiting = self.tools.as_ref().and_then(|b| b.jobs.get(b.current)).and_then(|j| match (&j.state, &j.call) {
            (JobState::Serving { since, .. }, Ok(call)) if j.id == job => Some((*since, call.clone())),
            _ => None,
        });
        let Some((since, call)) = waiting else { return };
        match said {
            Ok(text) => {
                self.servers.links_open.remove(job);
                self.journal_call(&call, since, CallEnd::Answered(&text));
                let summary = answer_summary(&text);
                self.complete_current(truncate_head(&text, MAX_TOOL_OUTPUT), false, CardStatus::Done { ok: true }, summary);
            }
            Err(Fail::Links(links)) => return self.links_needed(server, job, links),
            Err(fail) => {
                self.servers.links_open.remove(job);
                let why = fail.text();
                self.journal_call(&call, since, CallEnd::Failed(&why));
                self.finish_job(Err(why));
            }
        }
        self.advance_tools();
        self.request_redraw();
    }

    /// A call of `job` needs the user to open `links` first: each on a card,
    /// the call waiting meanwhile for the server to say they're done with,
    /// then made again.
    fn links_needed(&mut self, server: &str, job: &str, links: Vec<elicit::LinkAsk>) {
        let call = self.tools.as_ref().and_then(|b| b.jobs.get(b.current)).and_then(|j| match &j.call {
            Ok(ToolCall::Server { name, args }) => Some((name.clone(), args.clone())),
            _ => None,
        });
        let Some((name, args)) = call.and_then(|(name, args)| self.server_tool(&name).map(|t| (t.tool, args))) else {
            self.finish_job(Err(Fail::Links(links).text()));
            return self.advance_tools();
        };
        let ids = links.iter().map(|l| l.id.clone()).collect();
        self.servers.links_open.insert(job.to_string(), LinkWait { server: server.to_string(), tool: name, args, ids });
        for link in links {
            self.servers.asks.push_back(AskCard::link(server, Purpose::Link { job: job.to_string() }, link));
        }
        if let Some(batch) = self.tools.as_mut() {
            let idx = batch.current;
            // The call goes on, so it keeps the time it was first made.
            let since = match batch.jobs[idx].state {
                JobState::Serving { since, .. } => since,
                _ => Instant::now(),
            };
            batch.jobs[idx].state = JobState::Serving { until: Instant::now() + LINK_WAIT, server: server.to_string(), since };
            let entry = batch.jobs[idx].entry;
            if let Some(card) = self.ai.card_mut(entry) {
                card.status = CardStatus::InBrowser;
                card.result = tf("card.link_waits", &[("server", &server)]);
            }
        }
        self.tell_of_ask(server);
        self.request_redraw();
    }

    /// The server says link `elicitation` is done with: a call waiting for
    /// all its links is made again.
    fn link_done(&mut self, server: &str, elicitation: &str) {
        let done: Option<String> = self.servers.links_open.iter_mut().find(|(_, w)| w.server == server && w.ids.iter().any(|id| id == elicitation)).map(|(job, w)| {
            w.ids.retain(|id| id != elicitation);
            job.clone()
        });
        self.servers.asks.retain(|card| !matches!(&card.ui.body, ui::AskBody::Link(l) if l.id == elicitation));
        if let Some(job) = done.filter(|job| self.servers.links_open.get(job).is_some_and(|w| w.ids.is_empty())) {
            self.call_again(&job);
        }
        self.request_redraw();
    }

    /// The card's Try again, or every link done with: the call waiting for
    /// the browser made again.
    pub(super) fn call_again(&mut self, job: &str) {
        let waits = self.tools.as_ref().is_some_and(|b| b.jobs.get(b.current).is_some_and(|j| j.id == job && matches!(j.state, JobState::Serving { .. })));
        let Some(wait) = self.servers.links_open.remove(job).filter(|_| waits) else { return };
        self.servers.asks.retain(|card| !matches!(&card.purpose, Purpose::Link { job: j } if j == job));
        let Some((link, _)) = self.servers.links.get(&wait.server) else {
            self.finish_job(Err(format!("The MCP server {} isn't connected.", wait.server)));
            return self.advance_tools();
        };
        link.call(job, &wait.tool, wait.args, SERVING_LONGEST);
        self.serving(&wait.server);
    }

    /// The card's Give up, or a link declined: the call waiting for the
    /// browser fails, the model told why.
    pub(super) fn give_up_link(&mut self, job: &str, why: &str) {
        let waits = self.tools.as_ref().is_some_and(|b| b.jobs.get(b.current).is_some_and(|j| j.id == job && matches!(j.state, JobState::Serving { .. })));
        self.servers.links_open.remove(job);
        self.servers.asks.retain(|card| !matches!(&card.purpose, Purpose::Link { job: j } if j == job));
        if waits {
            self.finish_job(Err(why.to_string()));
            self.advance_tools();
        }
        self.request_redraw();
    }

    /// Flow gives up job `job` (stopped, or out of time): its server is
    /// told, and a link it waited on put away.
    pub(super) fn give_up_serving(&mut self, server: &str, job: &str, reason: &str) {
        if let Some((link, _)) = self.servers.links.get(server) {
            link.cancel(job, reason);
        }
        self.servers.links_open.remove(job);
        self.servers.asks.retain(|card| !matches!(&card.purpose, Purpose::Link { job: j } if j == job));
    }

    /// A call to an MCP server ended: a step of the task's journal, as a
    /// command is, and a line in the log, so what the assistant did
    /// through a server can be found afterwards (by the user, and by
    /// `search_journal`). The log names the server and the tool alone:
    /// arguments and answers can hold what isn't the log's to keep.
    pub(super) fn journal_call(&self, call: &ToolCall, since: Instant, end: CallEnd<'_>) {
        let (server, what, args) = match call {
            ToolCall::Server { name, args } => match self.server_tool(name) {
                Some(t) => (t.server, t.tool, args.clone()),
                None => (String::new(), name.clone(), args.clone()),
            },
            ToolCall::ReadResource { server, uri } => (server.clone(), "read".to_string(), Value::String(uri.clone())),
            _ => return,
        };
        let took = since.elapsed();
        log::info!("{}", call_log(&server, &what, took.as_millis(), end));
        let (text, exit, output) = call_step(&server, &what, &args, end);
        self.journal_step(&Step { kind: Some(StepKind::Called), text: &text, cwd: None, exit, secs: Some(took.as_secs_f64()), output: &output });
    }

    /// A server asks the user something: on a card, unless it asks too
    /// often (more than `ASKS_WAITING` waiting, or `ASKS_A_MINUTE` in a
    /// minute), then refused.
    fn server_asks(&mut self, server: String, id: Value, ask: elicit::Ask) {
        let Some((link, _)) = self.servers.links.get(&server) else { return };
        let now = Instant::now();
        self.servers.asked.retain(|(_, at)| now.duration_since(*at) < Duration::from_secs(60));
        let waiting = self.servers.asks.iter().filter(|c| c.server == server).count();
        let lately = self.servers.asked.iter().filter(|(s, _)| *s == server).count();
        if waiting >= ASKS_WAITING || lately >= ASKS_A_MINUTE {
            log::warn!("MCP server {server} asks too often; refused");
            return link.refuse(&id, -32603, "The user's client refused: too many questions at once.");
        }
        self.servers.asked.push((server.clone(), now));
        let card = match ask {
            elicit::Ask::Form(form) => AskCard::form(&server, Purpose::Server { id }, form, None),
            elicit::Ask::Link(link) => AskCard::link(&server, Purpose::Server { id }, link),
        };
        self.servers.asks.push_back(card);
        // A call it asks during waits as long as the user takes.
        self.hold_serving(&server);
        self.tell_of_ask(&server);
        self.request_redraw();
    }

    /// The server gave up asking `id`: its card goes.
    fn ask_withdrawn(&mut self, server: &str, id: &Value) {
        self.servers.asks.retain(|card| !(card.server == server && matches!(&card.purpose, Purpose::Server { id: i } if i == id)));
        self.request_redraw();
    }

    /// While server `server`'s question waits, the call it may be working
    /// on waits too, however long (`advance_tools` keeps it a whole
    /// command timeout ahead while the card shows).
    fn hold_serving(&mut self, server: &str) {
        let wait = self.server_wait();
        if let Some(batch) = self.tools.as_mut() {
            if let Some(JobState::Serving { until, server: s, .. }) = batch.jobs.get_mut(batch.current).map(|j| &mut j.state) {
                if s == server {
                    *until = (*until).max(Instant::now() + wait);
                }
            }
        }
    }

    /// The user is told a server asks something, by the system where Flow
    /// is behind.
    fn tell_of_ask(&self, server: &str) {
        if !self.focused {
            self.notify_assistant(&tf("notify.server_asks", &[("server", &server)]));
        }
    }

    /// The card in front answered: `answer` goes where it's for.
    pub(super) fn answer_server_ask(&mut self, answer: super::server_ask::Answer) {
        use super::server_ask::Answer;
        let Some(card) = self.servers.asks.pop_front() else { return };
        // A server's question waiting keeps its call waiting meanwhile.
        self.hold_serving(&card.server);
        let open = |app: &mut App| {
            if let ui::AskBody::Link(link) = &card.ui.body {
                if !app.open_url(&link.url) {
                    app.show_toast(t("toast.page_failed"));
                }
            }
        };
        match (&card.purpose, answer) {
            (Purpose::Server { id }, answer) => {
                let result = match answer {
                    Answer::Accept(content) => {
                        open(self);
                        match content {
                            Some(content) => json!({ "action": "accept", "content": content }),
                            None => json!({ "action": "accept" }),
                        }
                    }
                    Answer::Decline => json!({ "action": "decline" }),
                    Answer::Cancel => json!({ "action": "cancel" }),
                };
                if let Some((link, _)) = self.servers.links.get(&card.server) {
                    link.answer(id, result);
                }
            }
            (Purpose::Link { .. }, Answer::Accept(_)) => open(self),
            (Purpose::Link { job }, Answer::Decline | Answer::Cancel) => {
                let job = job.clone();
                self.give_up_link(&job, "The user declined to open the link the MCP server needed opened first.");
            }
            (Purpose::Prompt { prompt, given }, Answer::Accept(Some(Value::Object(typed)))) => {
                let mut args = given.clone();
                args.extend(typed);
                let (server, prompt) = (card.server.clone(), prompt.clone());
                self.get_prompt(&server, &prompt, args);
            }
            (Purpose::Prompt { .. }, _) => {}
        }
        self.request_redraw();
    }

    /// The panel's `/<server>:<prompt> [words]`: true where `text` named a
    /// connected server's prompt, which is then got (its arguments asked for
    /// first where words don't give them all).
    pub(super) fn prompt_command(&mut self, text: &str) -> bool {
        let Some(rest) = text.strip_prefix('/') else { return false };
        let (command, words) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
        let Some((server, name)) = command.split_once(':') else { return false };
        let Some(prompt) = self.servers.listing(server).and_then(|l| l.prompts.iter().find(|p| p.name == name)).cloned() else {
            if self.servers.list.iter().any(|s| s.name == server) {
                self.ai.entries.push(Entry::Notice(tf("notice.prompt_unknown", &[("server", &server), ("prompt", &name)])));
            }
            return self.servers.list.iter().any(|s| s.name == server);
        };
        self.ai.set_input(String::new());
        // Its text goes to the assistant, which needs its key first.
        if !self.has_key {
            self.ai.entries.push(Entry::Notice(self.key_help()));
            return true;
        }
        let given = prompt_args(&prompt, words);
        let missing = prompt.arguments.iter().any(|a| a.required && !given.contains_key(&a.name));
        if missing {
            let fields = prompt
                .arguments
                .iter()
                .map(|a| elicit::Field {
                    key: a.name.clone(),
                    title: a.name.clone(),
                    description: a.description.clone(),
                    required: a.required,
                    kind: elicit::Kind::Text { min: None, max: None, pattern: None, format: None },
                })
                .collect();
            let form = elicit::Form::plain(prompt.description.clone(), fields);
            let card = AskCard::form(server, Purpose::Prompt { prompt: prompt.name.clone(), given: Map::new() }, form, Some((&prompt.name, &given)));
            self.servers.asks.push_back(card);
        } else {
            self.get_prompt(server, &prompt.name, given);
        }
        self.request_redraw();
        true
    }

    /// Asks server `server` for prompt `name` filled in with `args`.
    fn get_prompt(&mut self, server: &str, name: &str, args: Map<String, Value>) {
        let Some((link, _)) = self.servers.links.get(server) else { return };
        self.servers.next_prompt += 1;
        let job = format!("prompt-{}", self.servers.next_prompt);
        link.prompt(&job, name, args, self.server_wait());
        self.servers.prompting.insert(job, (server.to_string(), name.to_string()));
        self.ai.entries.push(Entry::Notice(tf("notice.prompt_getting", &[("server", &server), ("prompt", &name)])));
    }

    /// A prompt's text came: sent as the user's message.
    fn prompt_got(&mut self, job: &str, said: Result<String, String>) {
        let Some((server, prompt)) = self.servers.prompting.remove(job) else { return };
        match said {
            Ok(text) if !text.trim().is_empty() => {
                self.send_to_assistant(text.clone(), text);
            }
            Ok(_) => self.ai.entries.push(Entry::Notice(tf("notice.prompt_empty", &[("server", &server), ("prompt", &prompt)]))),
            Err(why) => self.ai.entries.push(Entry::Error(tf("notice.prompt_failed", &[("server", &server), ("prompt", &prompt), ("why", &why)]))),
        }
        self.request_redraw();
    }

    /// `/mcp`: each server, how it stands, what it offers and its prompts.
    pub(super) fn servers_notice(&self) -> String {
        if self.servers.list.is_empty() {
            return t("notice.mcp_none").to_string();
        }
        let mut out = t("notice.mcp_title").to_string();
        for server in &self.servers.list {
            let standing = match self.servers.standing.get(&server.name) {
                None => t("settings.mcp_server.off").to_string(),
                Some(Standing::Connecting) => t("settings.mcp_server.connecting").to_string(),
                Some(Standing::Failed(why)) => format!("{}: {why}", t("settings.mcp_server.failed")),
                Some(Standing::SignIn(_)) => t("settings.mcp_server.needs_sign_in").to_string(),
                Some(Standing::SigningIn) => t("settings.mcp_server.signing_in").to_string(),
                Some(Standing::Connected { listing, .. }) => {
                    let on = listing.tools.iter().filter(|t| self.config.ai.mcp_tools_on.contains(&tool_key(&server.name, &t.name))).count();
                    tf(
                        "notice.mcp_connected",
                        &[("on", &on), ("tools", &listing.tools.len()), ("resources", &(listing.resources.len() + listing.templates.len())), ("prompts", &listing.prompts.len())],
                    )
                }
            };
            out.push_str(&format!("\n• {}: {standing}", server.name));
            for prompt in self.servers.listing(&server.name).map_or(&[][..], |l| &l.prompts) {
                let args: Vec<String> = prompt.arguments.iter().map(|a| if a.required { format!("<{}>", a.name) } else { format!("[{}]", a.name) }).collect();
                let about = if prompt.description.is_empty() { String::new() } else { format!(" — {}", prompt.description) };
                out.push_str(&format!("\n    /{}:{} {}{about}", server.name, prompt.name, args.join(" ")));
            }
        }
        out
    }

    /// Settings' Token…: the box for server `name`'s token.
    pub(super) fn ask_server_token(&mut self, name: &str) {
        self.key_dialog = Some(KeyDialog::server_token(name));
        self.request_redraw();
    }

    /// The token box's Enter: `token` kept for server `name` (an empty one
    /// forgets what was kept), and the server connected again with it.
    pub(super) fn keep_server_token(&mut self, name: &str, token: &str) {
        let kept = if token.is_empty() { secrets::forget_server_token(name) } else { secrets::store_server_token(name, token) };
        match kept {
            Ok(()) => {
                self.key_dialog = None;
                self.show_toast(tf(if token.is_empty() { "toast.server_token_forgot" } else { "toast.server_token_kept" }, &[("server", &name)]));
                if self.config.ai.mcp_servers_on.iter().any(|s| s == name) {
                    self.reconnect_server(name);
                }
            }
            Err(error) => {
                log::warn!("could not keep the token for {name}: {error}");
                if let Some(d) = self.key_dialog.as_mut() {
                    d.error = Some(tf("key_dialog.server_not_kept", &[("error", &error)]));
                }
            }
        }
    }

    /// Settings' Servers file: `mcp-servers.json` in the user's text
    /// editor, made where there's none yet.
    pub(super) fn open_servers_file(&mut self) {
        let path = server_file::file();
        let made = path.is_file() || server_file::save(&path, &[]).is_ok();
        if !made || !self.open_url(&memory::file_url(&path)) {
            self.show_toast(t("toast.page_failed"));
        }
    }
}

impl App {
    /// For snapshots: Settings' MCP servers with two servers made up, one
    /// connected with a tool turned on and its resources, the other waiting
    /// for a sign-in, its Sign-in row chosen (`settings-mcp-servers.png`); then the panel with one
    /// of its calls done and another waiting for the user's yes
    /// (`mcp-call.png`); then a server's question as a form
    /// (`mcp-ask-form.png`) and as a link (`mcp-ask-link.png`), all made up
    /// and put back after.
    pub(super) fn show_servers(&mut self, dir: &std::path::Path) {
        let was_open = settings::Section::McpServers.open(&self.config);
        settings::Section::McpServers.set_open(&mut self.config, true);
        self.servers.view = settings::ServersView::made_up().into();
        let ai = &mut self.config.ai;
        let on = std::mem::replace(&mut ai.mcp_tools_on, vec!["notes:find_notes".into(), "notes:read_note".into()]);
        let approval = std::mem::replace(&mut ai.mcp_approval, [("notes".to_string(), "read_only".to_string())].into());
        let resources = std::mem::replace(&mut ai.mcp_resources_on, vec!["notes".into()]);
        let at = settings::rows(&self.settings_entries()).iter().position(|r| r.id == SettingId::ServerAuth(1)).unwrap_or(0);
        self.menu = Some(SettingsMenu { selected: at, ..SettingsMenu::default() });
        self.capture(&dir.join("settings-mcp-servers.png"), true);
        self.menu = None;
        settings::Section::McpServers.set_open(&mut self.config, was_open);
        let card = |tool: &str, detail: &str, status: CardStatus, result: &str| {
            Entry::Tool(ToolCard {
                title: tf("card.server_call", &[("server", &"notes"), ("tool", &tool)]),
                command: None,
                detail: detail.into(),
                risk: None,
                status,
                result: result.into(),
                started: Some(Instant::now() - Duration::from_secs(2)),
                plan: false,
                undo: None,
            })
        };
        let entries = std::mem::replace(
            &mut self.ai.entries,
            vec![
                Entry::User("find my notes on the trip to Kyoto and add the train times to them".into()),
                card("find_notes", "{\"words\":\"Kyoto trip\"}", CardStatus::Done { ok: true }, "3 notes: Kyoto trip, Kyoto hotels, Rail passes"),
                Entry::Assistant("Your trip's note is **Kyoto trip**. I'll add the train to it:".into()),
                card("add_note", "{\"note\":\"Kyoto trip\",\"text\":\"Nozomi 7, Tokyo 07:00 → Kyoto 09:15\"}", CardStatus::Waiting, ""),
            ],
        );
        let awaiting = self.ai.awaiting.replace(3);
        let input = std::mem::take(&mut self.ai.input);
        self.ai.set_input(String::new());
        self.capture(&dir.join("mcp-call.png"), true);
        self.ai.awaiting = awaiting;
        // A server's questions, over the panel.
        let params = json!({ "message": "Which notebook should the train times go in?", "requestedSchema": { "type": "object", "properties": {
            "notebook": { "type": "string", "title": "Notebook", "oneOf": [{ "const": "trips", "title": "Trips" }, { "const": "japan", "title": "Japan 2026" }, { "const": "inbox", "title": "Inbox" }], "default": "japan" },
            "heading": { "type": "string", "title": "Heading", "description": "Put above the times", "maxLength": 40, "default": "Getting there" },
            "pin": { "type": "boolean", "title": "Pin the note", "default": true },
            "remind": { "type": "array", "title": "Remind me", "items": { "type": "string", "enum": ["the day before", "an hour before"] } }
        }, "required": ["notebook"] } });
        let snap = |app: &mut App, card: AskCard, name: &str| {
            app.servers.asks.push_back(card);
            app.capture(&dir.join(name), true);
            app.servers.asks.clear();
        };
        if let Ok(elicit::Ask::Form(form)) = elicit::Ask::read(&params) {
            let mut card = AskCard::form("notes", Purpose::Server { id: json!(1) }, form, None);
            card.ui.focus = 1;
            snap(self, card, "mcp-ask-form.png");
        }
        let link = elicit::LinkAsk { message: "Connect your rail account so the notes can show your seat bookings.".into(), url: "https://notes.example.com/connect/rail?session=8f2c41".into(), id: "e1".into() };
        snap(self, AskCard::link("notes", Purpose::Server { id: json!(2) }, link), "mcp-ask-link.png");
        self.ai.entries = entries;
        self.ai.input = input;
        let ai = &mut self.config.ai;
        (ai.mcp_tools_on, ai.mcp_approval, ai.mcp_resources_on) = (on, approval, resources);
    }
}

/// Words typed after a prompt's command, as its arguments in order: the
/// last takes the rest.
fn prompt_args(prompt: &crate::mcp::client::PromptInfo, words: &str) -> Map<String, Value> {
    let mut args = Map::new();
    let mut rest = words.trim();
    for (i, arg) in prompt.arguments.iter().enumerate() {
        if rest.is_empty() {
            break;
        }
        let last = i + 1 == prompt.arguments.len();
        let (word, after) = if last { (rest, "") } else { rest.split_once(char::is_whitespace).unwrap_or((rest, "")) };
        args.insert(arg.name.clone(), json!(word));
        rest = after.trim_start();
    }
    args
}

/// What a call's card shows of its arguments: on one line where they fit,
/// else laid out.
fn card_args(args: &Value) -> String {
    let line = serde_json::to_string(args).unwrap_or_default();
    let shown = match args.as_object() {
        Some(o) if o.is_empty() => String::new(),
        _ if line.chars().count() <= CARD_ARG_LINE => line,
        _ => serde_json::to_string_pretty(args).unwrap_or_default(),
    };
    let mut detail: String = shown.lines().take(CARD_ARG_LINES).collect::<Vec<_>>().join("\n");
    if detail.chars().count() > CARD_ARG_CHARS {
        detail = detail.chars().take(CARD_ARG_CHARS).collect::<String>() + "…";
    } else if shown.lines().count() > CARD_ARG_LINES {
        detail.push_str("\n…");
    }
    detail
}

/// The line a call's card shows for what the tool said: JSON on one line
/// (its first line alone would be a lone brace), else its first line with
/// words, cut to what a card's line holds.
fn answer_summary(text: &str) -> String {
    let line = match serde_json::from_str::<Value>(text.trim()) {
        Ok(json) if json.is_object() || json.is_array() => serde_json::to_string(&json).unwrap_or_default(),
        _ => text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or_default().to_string(),
    };
    if line.is_empty() {
        return t("card.done").to_string();
    }
    let mut cut: String = line.chars().take(SUMMARY_CHARS).collect();
    if line.chars().count() > SUMMARY_CHARS {
        cut.push('…');
    }
    cut
}

/// How much of what a tool said its card's line shows.
const SUMMARY_CHARS: usize = 100;

/// How a call to a server ended, for the journal and the log.
#[derive(Debug, Clone, Copy)]
pub(super) enum CallEnd<'a> {
    /// The server answered: what it said.
    Answered(&'a str),
    /// It failed: why, as the model is told.
    Failed(&'a str),
    /// Flow gave it up, out of time or stopped by the user: why.
    GivenUp(&'a str),
}

/// The most of a call's arguments its journal step keeps, in characters.
const ARGS_KEPT: usize = 300;
/// The most of why a call failed its log line keeps, in characters.
const FAILURE_LOGGED: usize = 200;

/// A call's journal step: its text (the server, the tool and its
/// arguments, cut short, or a word that they were left out where they
/// look like they hold a secret), its exit (0 answered, 1 failed, none
/// given up), and its output (the start of the answer, or why it failed
/// or was given up).
fn call_step(server: &str, what: &str, args: &Value, end: CallEnd<'_>) -> (String, Option<i32>, String) {
    let written = match args {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    let mut words = String::new();
    args_as_words(args, &mut words);
    let args = if looks_secret(&written) || looks_secret(&words) {
        "(arguments left out)".to_string()
    } else {
        let mut cut: String = written.chars().take(ARGS_KEPT).collect();
        if written.chars().count() > ARGS_KEPT {
            cut.push('…');
        }
        cut
    };
    let text = [server, what, args.as_str()].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" ");
    match end {
        CallEnd::Answered(answer) => (text, Some(0), crate::ai::memory::kept_answer(answer)),
        CallEnd::Failed(why) => (text, Some(1), why.trim().to_string()),
        CallEnd::GivenUp(why) => (text, None, why.trim().to_string()),
    }
}

/// A call's arguments as `name=value` words, so `looks_secret` sees a
/// secret-sounding name given a value, as it would on a command line.
fn args_as_words(args: &Value, out: &mut String) {
    match args {
        Value::Object(map) => {
            for (name, value) in map {
                match value {
                    Value::Object(_) | Value::Array(_) => args_as_words(value, out),
                    Value::String(s) => out.push_str(&format!(" {name}={s}")),
                    other => out.push_str(&format!(" {name}={other}")),
                }
            }
        }
        Value::Array(items) => items.iter().for_each(|item| args_as_words(item, out)),
        Value::String(s) => {
            out.push(' ');
            out.push_str(s);
        }
        _ => {}
    }
}

/// A call's line in the log: the server, the tool, how it ended and how
/// long it took. Never its arguments or its answer; why it failed only
/// where that doesn't look like it holds a secret.
fn call_log(server: &str, what: &str, ms: u128, end: CallEnd<'_>) -> String {
    let how = match end {
        CallEnd::Answered(answer) => format!("answered, {} characters", answer.chars().count()),
        CallEnd::Failed(why) => {
            let why = why.split_whitespace().collect::<Vec<_>>().join(" ");
            let mut shown: String = if looks_secret(&why) { "(why left out)".into() } else { why.chars().take(FAILURE_LOGGED).collect() };
            if why.chars().count() > FAILURE_LOGGED && !looks_secret(&why) {
                shown.push('…');
            }
            format!("failed: {shown}")
        }
        CallEnd::GivenUp(why) => format!("given up: {}", why.trim()),
    };
    // A reason's own full stop would come before the comma.
    let how = how.strip_suffix('.').unwrap_or(&how);
    format!("MCP server {server} {what}: {how}, in {ms} ms")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::client::{PromptArg, PromptInfo};

    #[test]
    fn a_servers_call_is_journaled_with_its_arguments_and_logged_without_them() {
        let (text, exit, output) = call_step("filmcraft", "command_run", &json!({ "id": "captions.list" }), CallEnd::Answered("  {\"tracks\":[]}\n"));
        assert_eq!((text.as_str(), exit, output.as_str()), (r#"filmcraft command_run {"id":"captions.list"}"#, Some(0), r#"{"tracks":[]}"#));
        let (text, exit, _) = call_step("filmcraft", "command_run", &json!({ "text": "x".repeat(1000) }), CallEnd::Failed("bad"));
        assert_eq!((text.chars().count(), text.ends_with('…'), exit), ("filmcraft command_run ".chars().count() + ARGS_KEPT + 1, true, Some(1)));
        let (text, exit, output) = call_step("docs", "read", &json!("file:///notes/plan.md"), CallEnd::GivenUp("The user stopped it."));
        assert_eq!((text.as_str(), exit, output.as_str()), ("docs read file:///notes/plan.md", None, "The user stopped it."));
        // A secret-sounding name given a value, or a key in its service's form.
        for args in [json!({ "token": "abc123" }), json!({ "headers": { "auth": "x" } }), json!({ "q": "sk-abcdefghijklmnopqrstuv" })] {
            assert_eq!(call_step("api", "fetch", &args, CallEnd::Answered("ok")).0, "api fetch (arguments left out)", "{args}");
        }
        assert_eq!(
            call_log("filmcraft", "render_frame", 1520, CallEnd::Failed("cannot connect:\n  actively refused")),
            "MCP server filmcraft render_frame: failed: cannot connect: actively refused, in 1520 ms"
        );
        assert_eq!(call_log("filmcraft", "command_run", 3, CallEnd::Answered("the answer")), "MCP server filmcraft command_run: answered, 10 characters, in 3 ms");
        assert!(call_log("api", "fetch", 3, CallEnd::Failed("401 for token=abc123")).ends_with("failed: (why left out), in 3 ms"));
        assert!(call_log("x", "y", 9, CallEnd::GivenUp("Flow gave up waiting for the answer.")).contains("given up: Flow gave up"));
    }

    #[test]
    fn a_calls_log_line_leaves_its_reasons_full_stop_out_before_the_time() {
        assert_eq!(call_log("everything", "slow", 2790, CallEnd::GivenUp("The user stopped it.")), "MCP server everything slow: given up: The user stopped it, in 2790 ms");
        assert_eq!(
            call_log("everything", "slow", 20001, CallEnd::GivenUp("Flow gave up waiting for the answer.")),
            "MCP server everything slow: given up: Flow gave up waiting for the answer, in 20001 ms"
        );
        assert_eq!(call_log("everything", "slow", 9, CallEnd::Failed("It answered 500.")), "MCP server everything slow: failed: It answered 500, in 9 ms");
        assert_eq!(call_log("everything", "echo", 7, CallEnd::Answered("Echo: hi.")), "MCP server everything echo: answered, 9 characters, in 7 ms");
    }

    #[test]
    fn a_calls_card_sums_up_json_on_one_line_and_text_by_its_first_words() {
        crate::i18n::set_language("en", &[]);
        assert_eq!(answer_summary("{\n  \"tabs\": [\n    {\"id\": 1}\n  ]\n}"), r#"{"tabs":[{"id":1}]}"#);
        assert_eq!(answer_summary("\n        Flow 0.1.42.DEV (build 640)\n  macOS"), "Flow 0.1.42.DEV (build 640)");
        assert_eq!(answer_summary("  \n "), t("card.done"));
        let long = "x".repeat(150);
        assert_eq!(answer_summary(&long).chars().count(), SUMMARY_CHARS + 1);
    }

    #[test]
    fn a_servers_long_answer_keeps_its_start() {
        let text = format!("# Features\n{}", "x".repeat(MAX_TOOL_OUTPUT));
        let cut = truncate_head(&text, MAX_TOOL_OUTPUT);
        assert!(cut.starts_with("# Features\n"), "the start, where a file's heading is");
        assert!(cut.ends_with("[… 11 more characters cut …]"), "{}", &cut[cut.len() - 40..]);
        assert_eq!(truncate_head("short", MAX_TOOL_OUTPUT), "short");
    }

    #[test]
    fn words_after_a_prompts_command_fill_its_arguments_in_order() {
        let arg = |name: &str, required| PromptArg { name: name.into(), description: String::new(), required };
        let prompt = PromptInfo { name: "review".into(), description: String::new(), arguments: vec![arg("pr", true), arg("focus", false)] };
        let args = prompt_args(&prompt, "  12   the tests and the docs ");
        assert_eq!(Value::Object(args), json!({ "pr": "12", "focus": "the tests and the docs" }));
        assert!(prompt_args(&prompt, "").is_empty());
        assert_eq!(Value::Object(prompt_args(&prompt, "12")), json!({ "pr": "12" }));
    }
}

/// A tool's place in `ai.mcp_tools_on`.
pub(super) fn tool_key(server: &str, tool: &str) -> String {
    format!("{server}:{tool}")
}

/// How much of a call's arguments its card shows: on one line up to
/// `CARD_ARG_LINE` characters, else laid out on at most `CARD_ARG_LINES`.
const CARD_ARG_LINE: usize = 72;
const CARD_ARG_LINES: usize = 8;
const CARD_ARG_CHARS: usize = 600;
/// How long a server's thread waits for a call's or a read's answer by
/// itself. The window gives it up long before, as the command timeout says
/// (held while the server asks the user something); this only keeps the
/// thread from waiting for good on one the window let go of untold.
const SERVING_LONGEST: Duration = Duration::from_secs(60 * 60);
/// How long a call waits for the user to finish in the browser.
const LINK_WAIT: Duration = Duration::from_secs(10 * 60);
/// At most so many of a server's questions wait at once, and come in a
/// minute.
const ASKS_WAITING: usize = 3;
const ASKS_A_MINUTE: usize = 10;
