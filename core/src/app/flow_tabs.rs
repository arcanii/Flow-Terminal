//! The tabs the `flow` command opens (`crate::flow_command`,
//! `docs/design/flow-command.md`): an `open-tab` from a pane, which Flow
//! answers once the tab is open, with a tab like the asking pane's right
//! after its own (or a pane beside it, `--split`), in front unless
//! `--behind`; the command typed once the new shell waits at its prompt,
//! as an update's is; and, with `--close`, the pane closed once that
//! command ends well. A guest Flow (`flow --window`, or `flow` with no tab
//! to ask) opens one such tab and leaves the rest to the Flow already
//! running: no saved tabs, hotkey, outside apps or updates.

use serde_json::{json, Value};

use super::*;
use crate::flow_command::{self, Asker, Guest, Opening};
use crate::session::Finished;

/// How long a shell that can't say it waits at its prompt (it sends no
/// marks) has to start before the command is typed into it.
const FLOW_TAB_WAIT: Duration = Duration::from_secs(10);

/// What Flow keeps of the `flow` command's tabs.
#[derive(Default)]
pub(super) struct FlowTabs {
    /// This Flow was started for a command, a guest beside the one running.
    guest: Option<Guest>,
    /// The panes `flow` opened, until their command is typed, and with
    /// `--close`, until it ends.
    waiting: Vec<FlowTab>,
    /// Panes whose `--close` command ended well, to close as Flow next waits.
    to_close: Vec<SessionId>,
}

/// A pane `flow` opened.
struct FlowTab {
    session: SessionId,
    /// The command, as it's typed at its shell's prompt (`shell::quote_for`),
    /// and as it's typed where the shell can't be told to take letters
    /// past ASCII as letters (`shell::quote_ascii_for`).
    typed: Option<(String, String)>,
    opened: Instant,
    sent: bool,
    /// Closed once its command ends well.
    close: bool,
}

impl FlowTabs {
    /// A command ended in pane `id`: one `flow --close` typed there closes
    /// it if it ended well (0, or cmd's, which can't say), and leaves it
    /// open to be read otherwise, Ctrl+C's 130 among them.
    pub(super) fn ended(&mut self, id: SessionId, finished: &Finished) {
        let Some(i) = self.waiting.iter().position(|w| w.session == id && w.sent && w.close) else { return };
        self.waiting.remove(i);
        if finished.exit.is_none_or(|code| code == 0) {
            self.to_close.push(id);
        }
    }
}

impl App {
    /// This Flow was started for a command (`flow_command::read_guest`).
    pub fn as_guest(&mut self, guest: Guest) {
        self.flow_tabs.guest = Some(guest);
    }

    /// Whether this Flow is a guest's, which leaves the saved tabs, the
    /// hotkey, outside apps and updates to the Flow already running.
    pub(super) fn guest(&self) -> bool {
        self.flow_tabs.guest.is_some()
    }

    /// Every theme's id and name, as `flow --theme` names them.
    fn theme_names(&self) -> Vec<(String, String)> {
        let base = self.base.as_ref().unwrap_or(&self.config);
        crate::looks::ids(base).into_iter().map(|id| (id.clone(), crate::looks::name(&id))).collect()
    }

    /// A guest's one tab: its shell and theme where it names them (a toast
    /// saying so where Flow has none such), in the folder `flow` ran in,
    /// its command typed there once its shell waits at its prompt (none
    /// holding a control character, a toast saying so). False where it
    /// couldn't open.
    pub(super) fn open_guest_tab(&mut self) -> bool {
        let Some(guest) = self.flow_tabs.guest.clone() else { return false };
        let mut profile = self.start_profile;
        if let Some(name) = &guest.shell {
            match flow_command::shell_named(&self.profiles, name) {
                Ok(p) => profile = p,
                Err(why) => self.show_toast(why),
            }
        }
        let look = match guest.theme.as_deref().map(|name| flow_command::theme_named(&self.theme_names(), name)) {
            Some(Ok(id)) => Some(id),
            Some(Err(why)) => {
                self.show_toast(why);
                None
            }
            None => None,
        };
        let Some(shell) = self.profiles.get(profile).cloned() else { return false };
        let cwd = guest.cwd.clone().filter(|d| d.is_dir() && shell.flavor != BashFlavor::Wsl);
        let before = self.tabs.len();
        self.open_tab_at(profile, look, None, cwd);
        if self.tabs.len() <= before {
            return false;
        }
        if guest.command.is_empty() {
            return true;
        }
        match crate::shell::quote_for(&shell, &guest.command).and_then(|line| Ok((line, crate::shell::quote_ascii_for(&shell, &guest.command)?))) {
            Ok(typed) => {
                let session = self.tabs[self.tabs.len() - 1].session().id;
                self.flow_tabs.waiting.push(FlowTab { session, typed: Some(typed), opened: Instant::now(), sent: false, close: guest.close });
            }
            // One holding a control character, which `flow` refuses before
            // starting a guest but Flow's own arguments may still carry, is
            // left untyped, the tab at its prompt.
            Err(why) => self.show_toast(flow_command::untypable(why)),
        }
        true
    }

    /// The answer to an `open-tab` (`flow_command::open_tab_asked`) from
    /// session `session` of tab `tab`: what `flow_command::tab_act`
    /// decides, opened, `{"ok": true, "pane": <id>}` once the tab or pane
    /// is there (before its command is typed), else why not. None, no
    /// answer at all, for one without the asking pane's own secret, or
    /// from a pane no longer there, whose secret can't be known.
    pub(super) fn open_tab_asked(&mut self, tab: usize, session: SessionId, asked: &Value) -> Option<Value> {
        let fail = |why: String| Some(json!({ "ok": false, "error": why }));
        self.keep_worn();
        let p = self.tabs.get(tab).and_then(|t| t.panes.iter().position(|pane| pane.session.id == session))?;
        let (asker, themes) = {
            let t = &self.tabs[tab];
            let pane = &t.panes[p];
            // Where its shell says it is, which any program can say: one on
            // another computer is never looked at (`link::on_this_computer`).
            let folder = pane.session.cwd().map(|c| tools::resolve_path(&c, None)).filter(|p| crate::link::on_this_computer(p));
            let asker = Asker { key: pane.session.key().to_string(), profile: pane.profile, theme: t.look_of(p), folder };
            (asker, self.theme_names())
        };
        let opening = match flow_command::tab_act(asked, &asker, &self.profiles, &themes)? {
            Ok(opening) => opening,
            Err(why) => return fail(why),
        };
        let opened = match opening.split {
            true => self.split_asked(tab, p, &opening),
            false => self.tab_asked(tab, &opening).ok_or_else(|| t("flow_command.not_opened").to_string()),
        };
        let id = match opened {
            Ok(id) => id,
            Err(why) => return fail(why),
        };
        if let (Some(line), Some(ascii)) = (opening.typed.clone(), opening.typed_ascii.clone()) {
            self.flow_tabs.waiting.push(FlowTab { session: id, typed: Some((line, ascii)), opened: Instant::now(), sent: false, close: opening.close });
        }
        self.request_redraw();
        let pane = self.tabs.iter().flat_map(|t| &t.panes).find(|pane| pane.session.id == id).map(|pane| pane.id);
        Some(json!({ "ok": true, "pane": pane }))
    }

    /// A tab for `opening`, right after tab `tab` in the bar and in its
    /// group, in front unless it's to open behind, where the tab in front stays and keeps
    /// the keys. Its session; none where its shell couldn't start.
    fn tab_asked(&mut self, tab: usize, opening: &Opening) -> Option<SessionId> {
        let (count, front) = (self.tabs.len(), self.active);
        // Following the asking tab's group's theme, it starts in it (its
        // start screen, its prompt) and goes on following it.
        let follows = opening.theme == (None, None);
        let (look, own) = super::look_with_group(opening.theme.0.clone(), opening.theme.1.clone(), self.tabs.get(tab).and_then(|t| self.group_theme(t)));
        self.open_tab_sized(opening.profile, look, own, opening.cwd.clone(), None);
        if self.tabs.len() <= count {
            return None;
        }
        let mut new = self.tabs.pop()?;
        if follows {
            new.look = None;
        }
        let id = new.session().id;
        let at = (tab + 1).min(self.tabs.len());
        // Beside the asking tab, it's in that tab's group too.
        new.group = self.tabs.get(tab).and_then(|t| t.group);
        self.tabs.insert(at, new);
        match opening.behind {
            true => {
                self.active = if front >= at { front + 1 } else { front };
                self.select_tab(self.active);
            }
            false => self.select_tab(at),
        }
        Some(id)
    }

    /// A pane for `opening` beside pane `p` of tab `tab`, as ⌘D splits one,
    /// even where another tab is in front: brought to the front with the
    /// keys, unless it's to open behind, where the pane and tab in front
    /// stay. Its session, or why not, where a half would be too small.
    fn split_asked(&mut self, tab: usize, p: usize, opening: &Opening) -> Result<SessionId, String> {
        let (front, focus) = (self.active, self.tabs[tab].focus);
        // Split as ⌘D splits the pane in front of the tab in front.
        self.active = tab;
        self.tabs[tab].put_in_front(p);
        let shell = (opening.profile, opening.cwd.clone(), opening.theme.clone());
        let split = self.split_front_pane(true, Some(shell), !opening.behind);
        if opening.behind {
            if let Some(t) = self.tabs.get_mut(tab) {
                t.put_in_front(focus);
            }
            self.active = front;
        }
        if !opening.behind {
            self.select_tab(tab);
        }
        self.pane_looks.clear();
        self.panes_changed();
        match split {
            Ok(Some(id)) => Ok(id),
            Ok(None) => Err(t("flow_command.not_opened").to_string()),
            Err(()) => Err(t("flow_command.no_room").to_string()),
        }
    }

    /// Types each `flow` command into its pane once its shell waits at its
    /// prompt (where it can't say, once it's had `FLOW_TAB_WAIT`), a start
    /// screen's `{wait}` going on by itself first, since the command stands
    /// in for the key; and forgets those whose pane has closed. After
    /// `watch_commands`, so a command done before this one isn't taken
    /// for it. Where the shell hasn't shown the prompt Flow's hook draws
    /// (its startup files `exec`ed another, the hook never reached), or
    /// bash's hook said it may take letters past ASCII as keys there, the
    /// letters past ASCII go into zsh and bash as their bytes
    /// (`Session::types_letters`).
    pub(super) fn tend_flow_tabs(&mut self) {
        if self.flow_tabs.waiting.is_empty() {
            return;
        }
        let mut waiting = std::mem::take(&mut self.flow_tabs.waiting);
        waiting.retain_mut(|w| {
            let Some(session) = self.session_by_id(w.session) else { return false };
            if w.sent {
                return w.close;
            }
            if session.state.lock().awaiting_key {
                session.go_on_from_start();
            }
            let ready = match session.at_prompt() {
                Some(at) => at,
                None => w.opened.elapsed() >= FLOW_TAB_WAIT,
            };
            if !ready {
                return true;
            }
            if let Some((line, ascii)) = &w.typed {
                session.run_typed(if session.types_letters() { line } else { ascii });
            }
            w.sent = true;
            w.close
        });
        // A pane opened meanwhile (none can be, but keep any).
        waiting.append(&mut self.flow_tabs.waiting);
        self.flow_tabs.waiting = waiting;
    }

    /// When the next `flow` command is due to be typed into a shell that
    /// can't say it's at its prompt, for Flow to wake then.
    pub(super) fn flow_tab_due(&self) -> Option<Instant> {
        self.flow_tabs.waiting.iter().filter(|w| !w.sent).map(|w| w.opened + FLOW_TAB_WAIT).min()
    }

    /// Closes the panes whose `--close` command ended well, or their tab
    /// where it's the only one, as ⌘W does, since the shell waits at its
    /// prompt again.
    pub(super) fn close_flow_tabs(&mut self, el: &ActiveEventLoop) {
        if self.flow_tabs.to_close.is_empty() {
            return;
        }
        for id in std::mem::take(&mut self.flow_tabs.to_close) {
            let Some(i) = self.tabs.iter().position(|t| t.holds(id)) else { continue };
            match self.tabs[i].panes.len() > 1 {
                true => self.close_pane(i, id),
                false => self.close_tab(i, el),
            }
        }
        self.request_redraw();
    }
}
