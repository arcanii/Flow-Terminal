//! The tab sidebar in the window (`docs/design/tab-groups.md`'s third
//! step; `crate::tab_sidebar` holds its rows and its state, and
//! `ui::draw_sidebar` draws it). Its key (⇧⌘S, Ctrl+Shift+S), the title
//! bar's three lines, the palette and the Mac's View menu bring it out over
//! the terminal, whose size it leaves alone, and while it's out it takes
//! the keys and the pointer as the palette does. Choosing a tab brings it
//! to the front and leaves the sidebar out (decided 2026-11-28), so a tab
//! after another can be looked at; a click outside it, Esc or its key
//! again puts it away.

use super::*;
use crate::tab_sidebar::{self, Arrow, Doing, MenuItem, Row, RowId};

impl App {
    /// The tabs as the sidebar's rows see them.
    fn listed(&self) -> Vec<tab_sidebar::Listed> {
        self.tabs.iter().map(|t| tab_sidebar::Listed { id: t.id, group: t.group, panes: t.panes.len() }).collect()
    }

    /// The sidebar's rows as the tabs and groups stand.
    pub(super) fn sidebar_rows(&self) -> Vec<Row> {
        tab_sidebar::rows(&self.listed(), &self.groups, &self.sidebar.splits_open)
    }

    /// Its key, the title bar's button, the palette and the View menu: out,
    /// or in again.
    pub(super) fn toggle_sidebar(&mut self) {
        if self.sidebar.is_open() {
            self.close_sidebar();
        } else {
            self.open_sidebar();
        }
    }

    /// Out over the terminal, the keys on the tab in front's row (its
    /// group's header where that's folded), the screens that would take
    /// them first closed.
    pub(super) fn open_sidebar(&mut self) {
        self.palette = None;
        self.tab_picker = None;
        self.history_panel = None;
        self.about = None;
        self.finish_renaming(true);
        let front = self.tabs.get(self.active);
        let folded = front.and_then(|t| self.groups.of(t)).filter(|g| g.folded).map(|g| RowId::Group(g.id));
        self.sidebar.chosen = folded.or(front.map(|t| RowId::Tab(t.id)));
        self.sidebar.open = Some(Instant::now());
        self.sidebar.rows = self.sidebar_rows();
        self.request_redraw();
    }

    /// In again: its menu and what waited on a time go with it, a group's
    /// name being typed in its header is kept, and the split tabs folded
    /// open fold shut.
    pub(super) fn close_sidebar(&mut self) {
        if self.sidebar.is_open() {
            self.finish_renaming(true);
        }
        self.sidebar = tab_sidebar::Sidebar::default();
        self.request_redraw();
    }

    /// What a pane is doing, from what its shell said, for its line.
    fn doing(&self, pane: &Pane) -> Doing {
        use crate::osc::ProgressState;
        let st = pane.session.state.lock();
        let progress = st.progress.as_ref().map(|(p, _)| ((p.state != ProgressState::Busy).then_some(p.percent), p.state == ProgressState::Error));
        let running = st.command.clone().filter(|_| st.running_since.is_some() && !st.at_prompt);
        // Snapshots' shells call their own folder home.
        let home = crate::shell::plain_home().map(std::path::Path::to_path_buf).or_else(dirs::home_dir).map(|h| h.to_string_lossy().into_owned());
        let folder = st.cwd.as_deref().map(|f| tab_sidebar::folder_shown(f, home.as_deref()));
        Doing { progress, running, ended: st.ended.clone(), folder }
    }

    /// A pane's name in its row: who opened it (the assistant, an outside
    /// app), else its shell's.
    fn pane_name(&self, pane: &Pane) -> String {
        pane.by.map_or_else(|| self.profiles.get(pane.profile).map(|p| p.label.clone()).unwrap_or_default(), |by| by.label())
    }

    /// What the sidebar shows, built afresh each frame while it's out (the
    /// rows kept for what's under the pointer).
    pub(super) fn sidebar_view(&mut self) -> Option<ui::SidebarView> {
        if !self.sidebar.is_open() {
            return None;
        }
        let rows = self.sidebar_rows();
        let labels = self.tab_labels();
        let index = |id: u64| self.tabs.iter().position(|t| t.id == id);
        let shown = rows
            .iter()
            .map(|row| {
                let kind = match row.id {
                    RowId::Group(id) => {
                        let g = self.groups.get(id);
                        let count = self.tabs.iter().filter(|t| t.group == Some(id)).count();
                        ui::SidebarKind::Group { name: g.map(|g| g.name.clone()).unwrap_or_default(), color: g.map_or([0; 3], |g| g.rgb()), count, folded: g.is_some_and(|g| g.folded) }
                    }
                    RowId::Tab(id) => {
                        let i = index(id).unwrap_or(0);
                        let tab = &self.tabs[i];
                        let split = (tab.panes.len() > 1).then(|| self.sidebar.splits_open.contains(&id));
                        let line = tab_sidebar::line(&self.doing(tab.pane()));
                        let name = labels.get(i).map(|l| l.label.clone()).unwrap_or_default();
                        ui::SidebarKind::Tab { tab: i, name, line, front: i == self.active, alert: tab.alert, split, color: self.groups.of(tab).map(|g| g.rgb()) }
                    }
                    RowId::Pane(id, p) => {
                        let i = index(id).unwrap_or(0);
                        let tab = &self.tabs[i];
                        let pane = &tab.panes[p.min(tab.panes.len() - 1)];
                        ui::SidebarKind::Pane { name: self.pane_name(pane), line: tab_sidebar::line(&self.doing(pane)), front: i == self.active && p == tab.focus }
                    }
                };
                ui::SidebarRow { level: row.level, kind }
            })
            .collect();
        let chosen = self.sidebar.chosen_in(&rows);
        let menu = self.sidebar.menu.as_ref().and_then(|m| {
            let row = rows.iter().position(|r| r.id == m.row)?;
            let items = m
                .items
                .iter()
                .map(|&item| {
                    let swatch = match item {
                        MenuItem::Color(c) => crate::tab_groups::COLORS.get(c).map(|(_, rgb)| *rgb),
                        _ => None,
                    };
                    (tab_sidebar::words(item, &self.groups), swatch)
                })
                .collect();
            Some(ui::SidebarMenu { row, items, chosen: m.chosen })
        });
        let naming = self.renaming.as_ref().and_then(|r| rows.iter().position(|row| row.id == r.what.row()).map(|at| (at, r.field.clone())));
        let closing = self.sidebar.closing.filter(|(_, at)| at.elapsed() < CONFIRM_WINDOW).and_then(|(g, _)| rows.iter().position(|r| r.id == RowId::Group(g)));
        let out = self.sidebar.out(Instant::now(), self.efficient());
        self.sidebar.rows = rows;
        Some(ui::SidebarView { rows: shown, chosen, top: self.sidebar.top.get(), out, menu, naming, closing, closable: self.tabs.len() > 1 })
    }

    /// Whether it's still sliding out, which keeps frames coming.
    pub(super) fn sidebar_sliding(&self) -> bool {
        self.sidebar.sliding(Instant::now(), self.efficient())
    }

    /// The tab of row `row`, by its place in the title bar.
    fn tab_of(&self, row: RowId) -> Option<usize> {
        match row {
            RowId::Tab(id) | RowId::Pane(id, _) => self.tabs.iter().position(|t| t.id == id),
            RowId::Group(_) => None,
        }
    }

    /// The rows changed (tabs moved, closed, folded): the keys stay on
    /// their row, or go to what took its place.
    fn sidebar_changed(&mut self, was: Option<usize>) {
        let rows = self.sidebar_rows();
        let tabs: Vec<(u64, Option<u64>)> = self.tabs.iter().map(|t| (t.id, t.group)).collect();
        self.sidebar.keep_chosen(&rows, was, |row| match row {
            RowId::Tab(id) | RowId::Pane(id, _) => tabs.iter().find(|(t, _)| *t == id).and_then(|(_, g)| *g),
            RowId::Group(_) => None,
        });
        self.sidebar.rows = rows;
        self.request_redraw();
    }

    /// Choosing a row (Enter, a click): a tab comes to the front, a pane
    /// to the front in its tab, and a group folds open or shut; the sidebar
    /// stays out.
    fn sidebar_choose(&mut self, row: RowId) {
        self.sidebar.chosen = Some(row);
        match row {
            RowId::Group(id) => self.sidebar_fold(id),
            RowId::Tab(_) => {
                if let Some(i) = self.tab_of(row) {
                    self.select_tab(i);
                }
            }
            RowId::Pane(_, p) => {
                if let Some(i) = self.tab_of(row) {
                    self.select_tab(i);
                    self.focus_pane(p);
                }
            }
        }
        self.request_redraw();
    }

    /// Group `id` folded shut, or open again.
    fn sidebar_fold(&mut self, id: u64) {
        let was = self.sidebar.chosen_in(&self.sidebar_rows());
        let folded = self.groups.get(id).is_some_and(|g| g.folded);
        self.groups.fold(id, !folded);
        self.sidebar_changed(was);
    }

    /// A split tab's panes folded open under it, or shut.
    fn sidebar_split(&mut self, id: u64, open: bool) {
        let was = self.sidebar.chosen_in(&self.sidebar_rows());
        self.sidebar.splits_open.retain(|t| *t != id);
        if open {
            self.sidebar.splits_open.push(id);
        }
        self.sidebar_changed(was);
    }

    /// A row's fold (its ▸ or ▾, → and ←): a group's or a split tab's.
    fn sidebar_toggle_fold(&mut self, row: RowId) {
        match row {
            RowId::Group(id) => self.sidebar_fold(id),
            RowId::Tab(id) => {
                let open = self.sidebar.splits_open.contains(&id);
                self.sidebar_split(id, !open);
            }
            RowId::Pane(..) => {}
        }
    }

    /// A row's × (⌘⌫, Ctrl+Delete): its tab closes, or its group
    /// (`close_group`).
    fn sidebar_close_row(&mut self, row: RowId, el: &ActiveEventLoop) {
        let was = self.sidebar.chosen_in(&self.sidebar_rows());
        match row {
            RowId::Group(id) => self.close_group(id, el),
            _ => {
                if let Some(i) = self.tab_of(row).filter(|_| self.tabs.len() > 1) {
                    self.close_tab_asking(i, el);
                }
            }
        }
        self.sidebar_changed(was);
    }

    /// Close Group: every tab of group `id` closes, at once where each
    /// waits at its prompt. Where one doesn't, or the group holds every
    /// tab (so Flow quits), the first press says so, in its row and a
    /// toast, and a second within `CONFIRM_WINDOW` closes them.
    pub(super) fn close_group(&mut self, id: u64, el: &ActiveEventLoop) {
        let members: Vec<usize> = (0..self.tabs.len()).filter(|&i| self.tabs[i].group == Some(id)).collect();
        if members.is_empty() {
            return;
        }
        let labels = self.tab_labels();
        let busy: Vec<String> = members.iter().filter(|&&i| self.tabs[i].panes.iter().any(|p| p.session.busy_with().is_some())).map(|&i| labels[i].label.clone()).collect();
        let every = members.len() == self.tabs.len();
        let asked = self.sidebar.closing.take().is_some_and(|(g, at)| g == id && at.elapsed() < CONFIRM_WINDOW);
        log::info!("Close Group: {} tabs, {} busy, every tab {every}, asked before {asked}", members.len(), busy.len());
        if tab_sidebar::group_closes_now(busy.is_empty(), every, asked) {
            for &i in members.iter().rev() {
                self.close_tab(i, el);
            }
            return;
        }
        self.sidebar.closing = Some((id, Instant::now()));
        let words = if every { t("sidebar.every_tab").to_string() } else { tf("sidebar.busy", &[("tabs", &busy.join(", "))]) };
        self.show_toast(words);
    }

    /// A row's … (the context-menu key): its menu, or none again.
    fn sidebar_menu(&mut self, row: RowId) {
        if self.sidebar.menu.as_ref().is_some_and(|m| m.row == row) {
            self.sidebar.menu = None;
        } else {
            self.sidebar.chosen = Some(row);
            self.sidebar.menu = match row {
                RowId::Group(id) => Some(tab_sidebar::group_menu(id)),
                RowId::Tab(id) => {
                    let group = self.tabs.iter().find(|t| t.id == id).and_then(|t| t.group);
                    Some(tab_sidebar::tab_menu(id, group, &self.groups))
                }
                RowId::Pane(..) => None,
            };
        }
        self.request_redraw();
    }

    /// What a menu's item does, on its row.
    fn sidebar_menu_do(&mut self, row: RowId, item: MenuItem, el: &ActiveEventLoop) {
        self.sidebar.menu = None;
        let was = self.sidebar.chosen_in(&self.sidebar_rows());
        let tab = self.tab_of(row);
        match (item, row, tab) {
            (MenuItem::Rename, RowId::Group(id), _) => self.rename_group(id),
            (MenuItem::Rename, _, Some(i)) => self.rename_tab(i),
            (MenuItem::MoveTo(group), _, Some(i)) => self.move_tab_to(i, group),
            (MenuItem::MoveToNew, _, Some(i)) => {
                if let Some(group) = self.new_group_of(i) {
                    self.rename_group(group);
                }
            }
            (MenuItem::Ungroup, _, Some(i)) => self.ungroup_tab_at(i),
            (MenuItem::RestartTab, _, Some(i)) => {
                self.select_tab(i);
                self.restart_tab_by_key();
            }
            (MenuItem::CloseTab, _, Some(_)) => self.sidebar_close_row(row, el),
            (MenuItem::Color(c), RowId::Group(id), _) => self.groups.recolor(id, c),
            (MenuItem::UngroupAll, RowId::Group(id), _) => self.regroup(|groups, tabs| groups.ungroup(tabs, id)),
            (MenuItem::CloseGroup, RowId::Group(id), _) => self.close_group(id, el),
            _ => {}
        }
        self.sidebar_changed(was);
    }

    /// ⌥↑ and ⌥↓ (Alt with them elsewhere): the chosen row's tab, or its
    /// group with all its tabs, a step up or down, into and out of the
    /// groups it passes. A tab that goes into a folded group opens it.
    fn sidebar_move(&mut self, up: bool) {
        let Some(row) = self.sidebar.chosen else { return };
        match row {
            RowId::Group(id) => self.regroup(|groups, tabs| groups.step_group(tabs, id, up)),
            RowId::Tab(id) => {
                let Some(i) = self.tab_of(row) else { return };
                self.regroup(|groups, tabs| groups.step_tab(tabs, i, up));
                let group = self.tabs.iter().find(|t| t.id == id).and_then(|t| t.group);
                if let Some(g) = group {
                    self.groups.fold(g, false);
                }
            }
            RowId::Pane(..) => return,
        }
        self.sidebar_changed(None);
    }

    /// A key while the sidebar has them (`tab_sidebar`'s keys, help's Tab
    /// sidebar page): the rows' own, a menu's while one is open, and every
    /// other shortcut of Flow's (`action`) as it's bound. Nothing reaches
    /// the terminal meanwhile.
    pub(super) fn sidebar_key(&mut self, event: &KeyEvent, action: Option<crate::keys::Action>, el: &ActiveEventLoop) {
        let mods = self.modifiers;
        let mac = cfg!(target_os = "macos");
        let rows = self.sidebar_rows();
        self.sidebar.rows = rows.clone();
        let chosen = self.sidebar.chosen_in(&rows);
        let row = chosen.map(|at| rows[at]);
        // A menu takes ↑ ↓, Enter and Esc.
        if let Some(menu) = self.sidebar.menu.as_mut() {
            let n = menu.items.len().max(1);
            match &event.logical_key {
                Key::Named(NamedKey::ArrowDown) => menu.chosen = (menu.chosen + 1) % n,
                Key::Named(NamedKey::ArrowUp) => menu.chosen = (menu.chosen + n - 1) % n,
                Key::Named(NamedKey::Enter | NamedKey::Space) => {
                    let (row, item) = (menu.row, menu.items.get(menu.chosen).copied());
                    if let Some(item) = item {
                        self.sidebar_menu_do(row, item, el);
                    }
                }
                Key::Named(NamedKey::Escape) => self.sidebar.menu = None,
                _ => {}
            }
            self.request_redraw();
            return;
        }
        let close = if mac { mods.super_key() && matches!(event.logical_key, Key::Named(NamedKey::Backspace)) } else { mods.control_key() && matches!(event.logical_key, Key::Named(NamedKey::Delete)) };
        let alt = mods.alt_key();
        match &event.logical_key {
            _ if close => {
                if let Some(row) = row {
                    self.sidebar_close_row(row.id, el);
                }
            }
            Key::Named(NamedKey::ArrowUp) if alt => self.sidebar_move(true),
            Key::Named(NamedKey::ArrowDown) if alt => self.sidebar_move(false),
            Key::Named(NamedKey::ArrowUp) => self.sidebar.step(&rows, -1),
            Key::Named(NamedKey::ArrowDown) => self.sidebar.step(&rows, 1),
            Key::Named(NamedKey::PageUp) => self.sidebar.step(&rows, -8),
            Key::Named(NamedKey::PageDown) => self.sidebar.step(&rows, 8),
            Key::Named(NamedKey::Home) => self.sidebar.step(&rows, -(rows.len() as isize)),
            Key::Named(NamedKey::End) => self.sidebar.step(&rows, rows.len() as isize),
            Key::Named(NamedKey::Enter) => {
                if let Some(row) = row {
                    self.sidebar_choose(row.id);
                }
            }
            Key::Named(arrow @ (NamedKey::ArrowRight | NamedKey::ArrowLeft)) => {
                if let Some(row) = row {
                    let panes = self.tab_of(row.id).map_or(1, |i| self.tabs[i].panes.len());
                    match tab_sidebar::arrow(row, &rows, &self.groups, &self.sidebar.splits_open, panes, *arrow == NamedKey::ArrowRight) {
                        Arrow::Fold(..) => self.sidebar_toggle_fold(row.id),
                        Arrow::Split(id, open) => self.sidebar_split(id, open),
                        Arrow::Go(to) => self.sidebar.chosen = Some(to),
                        Arrow::Nothing => {}
                    }
                }
            }
            Key::Named(NamedKey::F2) => match row.map(|r| r.id) {
                Some(RowId::Group(id)) => self.rename_group(id),
                Some(other) => {
                    if let Some(i) = self.tab_of(other) {
                        self.rename_tab(i);
                    }
                }
                None => {}
            },
            Key::Named(NamedKey::ContextMenu) => {
                if let Some(row) = row {
                    self.sidebar_menu(row.id);
                }
            }
            Key::Named(NamedKey::Escape) => self.close_sidebar(),
            // Another of Flow's keys, as it's bound.
            _ if action.is_some() => {
                if let Some(action) = action {
                    self.do_bound(action, el);
                }
            }
            // A letter goes to the next row whose name starts with it.
            _ if !mods.control_key() && !mods.super_key() => {
                if let Some(text) = event.text.as_deref().filter(|t| t.chars().all(|c| !c.is_control())) {
                    let names = self.sidebar_names(&rows);
                    if let Some(at) = tab_sidebar::starting_with(&names, chosen, text) {
                        self.sidebar.chosen = Some(rows[at].id);
                    }
                }
            }
            _ => {}
        }
        self.request_redraw();
    }

    /// Each row's name, as its row shows it.
    fn sidebar_names(&self, rows: &[Row]) -> Vec<String> {
        let labels = self.tab_labels();
        rows.iter()
            .map(|row| match row.id {
                RowId::Group(id) => self.groups.get(id).map(|g| g.name.clone()).unwrap_or_default(),
                RowId::Tab(_) => self.tab_of(row.id).and_then(|i| labels.get(i)).map(|l| l.label.clone()).unwrap_or_default(),
                RowId::Pane(_, p) => self.tab_of(row.id).and_then(|i| self.tabs[i].panes.get(p)).map(|pane| self.pane_name(pane)).unwrap_or_default(),
            })
            .collect()
    }

    /// A press while the sidebar is out, on `hit`: its rows, folds, ×s,
    /// …s, menu and foot; anywhere outside it puts it away.
    pub(super) fn sidebar_click(&mut self, hit: Option<Hit>, el: &ActiveEventLoop) {
        let row = |i: usize, s: &Self| s.sidebar.rows.get(i).map(|r| r.id);
        match hit {
            Some(Hit::SidebarRow(i)) => {
                let Some(id) = row(i, self) else { return };
                let double = self.clicks.1 == 2 && self.sidebar.chosen == Some(id);
                self.sidebar.menu = None;
                match (id, double) {
                    // A double click names it: a tab's row, or a group's
                    // header, its fold put back as it was.
                    (RowId::Tab(_), true) => {
                        if let Some(i) = self.tab_of(id) {
                            self.rename_tab(i);
                        }
                    }
                    (RowId::Group(g), true) => {
                        self.sidebar_fold(g);
                        self.rename_group(g);
                    }
                    _ => self.sidebar_choose(id),
                }
            }
            Some(Hit::SidebarFold(i)) => {
                if let Some(id) = row(i, self) {
                    self.sidebar.chosen = Some(id);
                    self.sidebar_toggle_fold(id);
                }
            }
            Some(Hit::SidebarClose(i)) => {
                if let Some(id) = row(i, self) {
                    self.sidebar.chosen = Some(id);
                    self.sidebar_close_row(id, el);
                }
            }
            Some(Hit::SidebarMore(i)) => {
                if let Some(id) = row(i, self) {
                    self.sidebar_menu(id);
                }
            }
            Some(Hit::SidebarMenuItem(k)) => {
                if let Some((r, item)) = self.sidebar.menu.as_ref().and_then(|m| Some((m.row, *m.items.get(k)?))) {
                    self.sidebar_menu_do(r, item, el);
                }
            }
            Some(Hit::SidebarNewTab) => {
                self.sidebar.menu = None;
                self.new_tab();
                self.sidebar.chosen = self.tabs.get(self.active).map(|t| RowId::Tab(t.id));
            }
            Some(Hit::SidebarNewGroup) => {
                self.sidebar.menu = None;
                self.new_group();
            }
            // On the card, or its menu, where nothing else is: a menu shut.
            Some(Hit::SidebarCard) => self.sidebar.menu = None,
            _ => self.close_sidebar(),
        }
        self.request_redraw();
    }

    /// A right click while the sidebar is out: on a row, its menu.
    pub(super) fn sidebar_context(&mut self, hit: Option<Hit>) {
        if let Some(Hit::SidebarRow(i) | Hit::SidebarMore(i)) = hit {
            if let Some(id) = self.sidebar.rows.get(i).map(|r| r.id) {
                self.sidebar_menu(id);
            }
        }
    }

    /// The wheel over the sidebar scrolls its rows.
    pub(super) fn sidebar_wheel(&mut self, lines: isize) {
        let n = self.sidebar.rows.len();
        let top = (self.sidebar.top.get() as isize + lines).clamp(0, n.saturating_sub(1) as isize) as usize;
        self.sidebar.top.set(top);
        // The chosen row stays, unless it's scrolled out of view above.
        if self.sidebar.chosen_in(&self.sidebar.rows).is_some_and(|at| at < top) {
            self.sidebar.chosen = self.sidebar.rows.get(top).map(|r| r.id);
        }
        self.request_redraw();
    }

    /// Its rows' line as a reader hears it.
    fn line_words(line: &tab_sidebar::Line) -> String {
        use tab_sidebar::Line;
        match line {
            Line::Progress { percent: Some(p), .. } => tf("sidebar.progress", &[("percent", p)]),
            Line::Progress { percent: None, .. } => String::new(),
            Line::Running(command) => tf("sidebar.running", &[("command", command)]),
            Line::Ended { command, ok: true } => tf("sidebar.ended_ok", &[("command", command)]),
            Line::Ended { command, ok: false } => tf("sidebar.ended_failed", &[("command", command)]),
            Line::Folder(folder) => folder.clone(),
            Line::Nothing => String::new(),
        }
    }

    /// The sidebar for screen readers: its rows as a tree (each group
    /// folded open or shut, each tab selected while it's in front, each
    /// row's line its description), a row's menu as a menu, and its foot's
    /// buttons.
    pub(super) fn sidebar_dialog(&self) -> Option<access::Dialog> {
        use access::{Button, Chooser, Dialog, DialogKind, Item, List};
        if !self.sidebar.is_open() {
            return None;
        }
        let rows = &self.sidebar.rows;
        let labels = self.tab_labels();
        let names = self.sidebar_names(rows);
        let items = rows
            .iter()
            .zip(names)
            .enumerate()
            .map(|(k, (row, name))| {
                let rect = self.rect_of(Hit::SidebarRow(k));
                match row.id {
                    RowId::Group(id) => {
                        let count = self.tabs.iter().filter(|t| t.group == Some(id)).count();
                        let label = if count == 1 { tf("sidebar.group_one", &[("group", &name)]) } else { tf("sidebar.group", &[("group", &name), ("n", &count)]) };
                        let open = self.groups.get(id).is_some_and(|g| !g.folded);
                        Item { label, detail: String::new(), rect, tree: Some((row.level, Some(open))) }
                    }
                    RowId::Tab(id) => {
                        let i = self.tab_of(row.id).unwrap_or(0);
                        let tab = &self.tabs[i];
                        let label = labels.get(i).map(|l| l.label.clone()).unwrap_or(name);
                        let open = (tab.panes.len() > 1).then(|| self.sidebar.splits_open.contains(&id));
                        Item { label, detail: Self::line_words(&tab_sidebar::line(&self.doing(tab.pane()))), rect, tree: Some((row.level, open)) }
                    }
                    RowId::Pane(_, p) => {
                        let detail = self.tab_of(row.id).and_then(|i| self.tabs[i].panes.get(p)).map(|pane| Self::line_words(&tab_sidebar::line(&self.doing(pane)))).unwrap_or_default();
                        Item { label: name, detail, rect, tree: Some((row.level, None)) }
                    }
                }
            })
            .collect();
        // The row in front among them: the one the keys are on.
        let chosen = self.sidebar.chosen_in(rows);
        let mut lists = vec![List { label: t("sidebar.title").to_string(), chooser: Chooser::Sidebar, items, chosen }];
        if let Some(m) = &self.sidebar.menu {
            let items = m.items.iter().enumerate().map(|(k, &item)| Item { label: tab_sidebar::words(item, &self.groups), rect: self.rect_of(Hit::SidebarMenuItem(k)), ..Item::default() }).collect();
            lists.push(List { label: t("sidebar.more").to_string(), chooser: Chooser::SidebarMenu, items, chosen: Some(m.chosen) });
        }
        let buttons = vec![Button::new(t("menu.new_tab"), self.rect_of(Hit::SidebarNewTab)), Button::new(t("menu.new_group"), self.rect_of(Hit::SidebarNewGroup))];
        Some(Dialog { kind: DialogKind::TabSidebar, title: t("sidebar.title").to_string(), rect: self.rect_of(Hit::SidebarCard), lists, buttons, ..Dialog::default() })
    }

    /// What has the keys while the sidebar is out: a menu's item, else the
    /// row they're on.
    pub(super) fn sidebar_focus(&self) -> access::Focus {
        use access::{DialogKind, Focus};
        match (&self.sidebar.menu, self.sidebar.chosen_in(&self.sidebar.rows)) {
            (Some(m), _) => Focus::Item(DialogKind::TabSidebar, 1, m.chosen),
            (None, Some(at)) => Focus::Item(DialogKind::TabSidebar, 0, at),
            (None, None) => Focus::Dialog(DialogKind::TabSidebar),
        }
    }

    /// A reader's Expand or Collapse on a row: its group or split tab
    /// folded open or shut.
    pub(super) fn fold_for_reader(&mut self, chooser: access::Chooser, i: usize, open: bool) {
        let Some(row) = self.sidebar.rows.get(i).copied().filter(|_| chooser == access::Chooser::Sidebar) else { return };
        self.sidebar.chosen = Some(row.id);
        match row.id {
            RowId::Group(id) if self.groups.get(id).is_some_and(|g| g.folded == open) => self.sidebar_fold(id),
            RowId::Tab(id) => self.sidebar_split(id, open),
            _ => {}
        }
    }

    /// A name being typed, for readers: a field of its own, where it's
    /// typed (the tab's pill, or its row in the sidebar).
    pub(super) fn naming_dialog(&self) -> Option<access::Dialog> {
        use access::{Dialog, DialogKind, Field};
        if !self.naming() {
            return None;
        }
        let r = self.renaming.as_ref()?;
        let row = self.sidebar.rows.iter().position(|row| row.id == r.what.row()).map(|k| self.rect_of(Hit::SidebarRow(k))).filter(|_| self.sidebar.is_open()).flatten();
        let (title, hint, placeholder, rect) = match r.what {
            tab_names::Named::Tab(_) => {
                let i = self.renaming_at()?;
                (t("tab_name.title"), t("tab_name.hint"), self.unnamed(i), row.or(self.rect_of(Hit::Tab(i))))
            }
            tab_names::Named::Group(id) => (t("tab_name.group_title"), t("tab_name.group_hint"), self.groups.get(id).map(|g| g.name.clone()).unwrap_or_default(), row),
        };
        let (line, col) = r.field.caret();
        Some(Dialog {
            kind: DialogKind::TabName,
            title: title.to_string(),
            rect,
            text: vec![hint.to_string()],
            field: Some(Field { caret: (line, col), ..Field::line(title, placeholder, &r.field.text(), rect) }),
            ..Dialog::default()
        })
    }

    /// For snapshots: four more tabs in the snapshot's shell, for the
    /// sidebar's picture; where the first of them is.
    pub(super) fn show_sidebar_open(&mut self) -> usize {
        let (first, front) = (self.tabs.len(), self.active);
        for _ in 0..4 {
            self.open_tab_at(self.start_profile, None, None, None);
        }
        self.select_tab(front);
        first
    }

    /// For snapshots: the sidebar out over the split tab, folded open to its
    /// panes, and the tabs `show_sidebar_open` opened, made up as a group
    /// of a server running and its tests passed, and a folded group of two;
    /// written as `tab-sidebar.png`, then everything as it was.
    pub(super) fn show_sidebar(&mut self, path: &std::path::Path, first: usize, el: &ActiveEventLoop) {
        let front = self.active;
        if self.tabs.len() < first + 4 {
            log::warn!("the sidebar's snapshot has no tabs to show");
            return;
        }
        let home = crate::shell::plain_home().map(|h| h.to_string_lossy().into_owned()).unwrap_or_default();
        let sep = std::path::MAIN_SEPARATOR;
        let web = self.groups.add("Web".into(), crate::tab_groups::color_named("teal"));
        let docs = self.groups.add("Docs".into(), crate::tab_groups::color_named("pink"));
        self.groups.fold(docs, true);
        let made = [("server", web, Some("npm run dev"), None), ("tests", web, None, Some(("cargo test", 0))), ("notes", docs, None, None), ("drafts", docs, None, None)];
        for (k, (name, group, running, ended)) in made.into_iter().enumerate() {
            let tab = &mut self.tabs[first + k];
            tab.name = Some(name.to_string());
            tab.group = Some(group);
            let mut st = tab.panes[0].session.state.lock();
            st.cwd = Some(format!("{home}{sep}src{sep}{name}"));
            st.command = running.map(str::to_string);
            st.running_since = running.map(|_| Instant::now());
            st.at_prompt = running.is_none();
            st.ended = ended.map(|(command, code)| (command.to_string(), code));
        }
        self.groups.settle(&mut self.tabs);
        // The split tab's first pane in front, in Flow's own look.
        let focus = self.tabs[front].focus;
        self.focus_pane(0);
        let id = self.tabs[front].id;
        self.open_sidebar();
        self.sidebar.open = Instant::now().checked_sub(tab_sidebar::SLIDE);
        self.sidebar.splits_open = vec![id];
        self.sidebar.chosen = Some(RowId::Tab(self.tabs[first + 1].id));
        self.capture(path, true);
        self.close_sidebar();
        for i in (first..first + 4).rev() {
            self.close_tab(i, el);
        }
        self.select_tab(front.min(self.tabs.len().saturating_sub(1)));
        self.focus_pane(focus);
    }

}
