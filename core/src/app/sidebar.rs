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
        // Pinned, it stays out, and its key moves the keys into it and out.
        if self.sidebar_pinned() {
            self.sidebar.keys = !self.sidebar.keys;
            self.sidebar.menu = None;
            if self.sidebar.keys && self.sidebar.chosen_in(&self.sidebar_rows()).is_none() {
                self.sidebar.chosen = self.tabs.get(self.active).map(|t| RowId::Tab(t.id));
            }
            self.request_redraw();
        } else if self.sidebar.is_open() {
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
        self.sidebar.keys = true;
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
                let picked = matches!(row.id, RowId::Tab(id) if self.sidebar.picked.contains(&id)) && self.sidebar.picked.len() > 1;
                ui::SidebarRow { level: row.level, kind, picked }
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
                    (tab_sidebar::words(item, &self.groups, m.many), swatch)
                })
                .collect();
            Some(ui::SidebarMenu { row, items, chosen: m.chosen })
        });
        let naming = self.renaming.as_ref().and_then(|r| rows.iter().position(|row| row.id == r.what.row()).map(|at| (at, r.field.clone())));
        let closing = self.sidebar.closing.filter(|(_, at)| at.elapsed() < CONFIRM_WINDOW).and_then(|(g, _)| rows.iter().position(|r| r.id == RowId::Group(g)));
        // A row or a pill dragged: where it would land in the sidebar, in
        // the color of a group it would join (its name beside the pointer,
        // and where it would land in the title bar, are `tab_drag_view`'s).
        let drag = self.sidebar.drag.map(|(dragged, to)| {
            let (onto, gap) = match to {
                Some(tab_sidebar::Drop::Into(g)) => (rows.iter().position(|r| r.id == RowId::Group(g)), None),
                Some(tab_sidebar::Drop::Gap { at, group }) => (None, Some((at, group.and_then(|g| self.groups.get(g)).map(|g| g.rgb())))),
                _ => (None, None),
            };
            ui::SidebarDrag { row: rows.iter().position(|r| r.id == dragged), onto, gap }
        });
        let out = self.sidebar.out(Instant::now(), self.efficient());
        self.sidebar.rows = rows;
        let (pinned, keys) = (self.sidebar_pinned(), self.sidebar_has_keys());
        Some(ui::SidebarView { rows: shown, chosen, top: self.sidebar.top.get(), out: if pinned { 1.0 } else { out }, menu, naming, closing, picked: self.sidebar.picked.len(), width: self.sidebar_width(), pinned, keys, drag })
    }

    /// Whether Settings has it pinned (`window.tab_sidebar`).
    pub(super) fn sidebar_pin_set(&self) -> bool {
        self.config.window.tab_sidebar == tab_sidebar::PINNED
    }

    /// How wide it's drawn, in pixels: `window.tab_sidebar_width` points,
    /// at most a third of the window.
    pub(super) fn sidebar_width(&self) -> f32 {
        let window = self.renderer.as_ref().map_or(self.layout.width, |r| r.size().0 as f32);
        let least = (tab_sidebar::LEAST_WIDTH * self.scale).min(window / 3.0);
        (self.config.window.tab_sidebar_width * self.scale).min(window / 3.0).max(least)
    }

    /// Whether it's out and pinned beside the terminal: Settings has it so,
    /// and the window leaves the terminal `PIN_LEAST_COLS` columns beside
    /// it; in a narrower one it pops out over the terminal instead, until
    /// the window is wider again.
    pub(super) fn sidebar_pinned(&self) -> bool {
        if !self.sidebar_pin_set() || !self.sidebar.is_open() {
            return false;
        }
        self.sidebar_fits()
    }

    /// Whether the window has room for it pinned (`tab_sidebar::pin_fits`).
    fn sidebar_fits(&self) -> bool {
        let window = self.renderer.as_ref().map_or(self.layout.width, |r| r.size().0 as f32);
        let left = tab_sidebar::MARGIN * self.scale + self.sidebar_width();
        tab_sidebar::pin_fits(window, left, self.cell().0, self.scale)
    }

    /// Whether it has the keys: popped out, while it's out; pinned, while
    /// its key, a click on it or a menu has given them to it.
    pub(super) fn sidebar_has_keys(&self) -> bool {
        self.sidebar.is_open() && (!self.sidebar_pinned() || self.sidebar.keys || self.sidebar.menu.is_some())
    }

    /// Follows `window.tab_sidebar`: pinned, it comes out (the keys staying
    /// with the pane) where the window has room, and pinned no more, it
    /// goes; the terminal's room follows either way. At the start, as
    /// Settings or a program change it, and as the window's size changes.
    pub(super) fn follow_sidebar_pin(&mut self) {
        let pin = self.sidebar_pin_set();
        if pin && !self.sidebar.is_open() && self.sidebar_fits() {
            self.open_sidebar();
            self.sidebar.keys = false;
            self.sidebar.open = Instant::now().checked_sub(tab_sidebar::SLIDE);
        } else if !pin && self.sidebar.pinned {
            self.close_sidebar();
        }
        self.sidebar.pinned = pin && self.sidebar.is_open();
    }

    /// The sidebar's pin, at its foot: pinned, or popped out over the
    /// terminal, where it stays out with the keys until Esc or a click
    /// outside it puts it away (the user's word, 2026-11-28: it went at
    /// once). Saved, as Settings saves a change.
    pub(super) fn toggle_sidebar_pin(&mut self) {
        let pinned = self.sidebar_pin_set();
        self.config.window.tab_sidebar = (if pinned { tab_sidebar::POPS_OUT } else { tab_sidebar::PINNED }).to_string();
        // Unpinned by its own pin, it isn't put away as Settings' change is.
        self.sidebar.pinned = false;
        self.follow_sidebar_pin();
        // Out still, it has the keys, which a click on its pin gave it.
        self.sidebar.keys = true;
        self.relayout();
        self.save_config();
        self.request_redraw();
    }

    /// Something else takes the keys (the palette, Settings, the history,
    /// About): popped out, the sidebar goes; pinned, it stays, and gives up
    /// the keys and its menu.
    pub(super) fn sidebar_aside(&mut self) {
        if self.sidebar_pinned() {
            self.sidebar.keys = false;
            self.sidebar.menu = None;
            self.finish_renaming(true);
            self.request_redraw();
        } else {
            self.close_sidebar();
        }
    }

    /// The room it takes at the window's left while it's pinned, in pixels
    /// (its margin and its width); none otherwise.
    pub(super) fn sidebar_room(&self) -> f32 {
        if self.sidebar_pinned() {
            tab_sidebar::MARGIN * self.scale + self.sidebar_width()
        } else {
            0.0
        }
    }

    /// What the terminal shares the window with: the panel where it's open,
    /// a border where the look has one, and the sidebar where it's pinned.
    pub(super) fn around(&self, border: bool) -> ui::Around {
        ui::Around { panel: self.ai.open, border, left: self.sidebar_room() }
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
                // Pinned, the tab chosen has the keys.
                self.sidebar.keys = false;
            }
            RowId::Pane(_, p) => {
                if let Some(i) = self.tab_of(row) {
                    self.select_tab(i);
                    self.focus_pane(p);
                }
                self.sidebar.keys = false;
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
            // Picked with others: they all close.
            RowId::Tab(id) if self.sidebar.several().is_some_and(|p| p.contains(&id)) => {
                let places = self.picked_places();
                if self.close_tabs_asking(&places, el) {
                    self.sidebar.unpick();
                }
            }
            // The last tab closes Flow, so it asks first, as Close Group
            // does for every tab; the others as any tab does.
            _ => match self.tab_of(row) {
                Some(i) if self.tabs.len() == 1 => {
                    self.close_tabs_asking(&[i], el);
                }
                Some(i) => self.close_tab_asking(i, el),
                None => {}
            },
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
                // Picked with others, the menu acts on them all.
                RowId::Tab(id) if self.sidebar.several().is_some_and(|p| p.contains(&id)) => {
                    let picked = self.sidebar.picked.clone();
                    let grouped = self.tabs.iter().any(|t| picked.contains(&t.id) && t.group.is_some());
                    Some(tab_sidebar::picked_menu(id, picked.len(), grouped, &self.groups))
                }
                RowId::Tab(id) => {
                    let group = self.tabs.iter().find(|t| t.id == id).and_then(|t| t.group);
                    Some(tab_sidebar::tab_menu(id, group, &self.groups))
                }
                RowId::Pane(..) => None,
            };
        }
        self.request_redraw();
    }

    /// A right click on tab `i`'s pill in the title bar: its menu, as its
    /// row's in the sidebar, under the pill.
    pub(super) fn pill_menu(&mut self, i: usize) {
        let Some(tab) = self.tabs.get(i) else { return };
        self.tab_picker = None;
        self.sidebar.menu = Some(tab_sidebar::Menu { pill: true, ..tab_sidebar::tab_menu(tab.id, tab.group, &self.groups) });
        self.request_redraw();
    }

    /// A pill's menu as it's drawn: its tab's place and its items.
    pub(super) fn pill_menu_view(&self) -> Option<(usize, ui::SidebarMenu)> {
        let m = self.sidebar.menu.as_ref().filter(|_| self.pill_menu_open())?;
        let i = self.tab_of(m.row)?;
        let items = m.items.iter().map(|&item| (tab_sidebar::words(item, &self.groups, m.many), None)).collect();
        Some((i, ui::SidebarMenu { row: 0, items, chosen: m.chosen }))
    }

    /// A tab's menu opened from its pill, while the sidebar is in and its
    /// tab is still open.
    pub(super) fn pill_menu_open(&self) -> bool {
        !self.sidebar.is_open() && self.sidebar.menu.as_ref().is_some_and(|m| m.pill && self.tab_of(m.row).is_some())
    }

    /// A press while a pill's menu is open: an item does what it says, and
    /// anywhere else shuts the menu.
    pub(super) fn pill_menu_click(&mut self, hit: Option<Hit>, el: &ActiveEventLoop) {
        match hit {
            Some(Hit::SidebarMenuItem(k)) => {
                if let Some(m) = self.sidebar.menu.as_ref() {
                    if let Some(item) = m.items.get(k).copied() {
                        let (row, many) = (m.row, m.many);
                        self.sidebar_menu_do(row, item, many, el);
                    }
                }
            }
            Some(Hit::SidebarCard) => {}
            _ => self.sidebar.menu = None,
        }
        self.request_redraw();
    }

    /// The tabs picked together, as their places now, in order.
    fn picked_places(&self) -> Vec<usize> {
        (0..self.tabs.len()).filter(|&i| self.sidebar.picked.contains(&self.tabs[i].id)).collect()
    }

    /// New Group of the tabs picked together, gathered at the first's
    /// place, its name ready to be typed; nothing picked any longer.
    pub(super) fn group_picked(&mut self) {
        let picked = std::mem::take(&mut self.sidebar.picked);
        self.sidebar.anchor = None;
        let made = self.regroup(|groups, tabs| groups.group_picked(tabs, |t| picked.contains(&t.id), Self::group_numbered));
        if let Some((group, _)) = made {
            self.rename_group(group);
        }
    }

    /// What a menu's item does, on its row, or on the `many` tabs picked
    /// with it.
    fn sidebar_menu_do(&mut self, row: RowId, item: MenuItem, many: usize, el: &ActiveEventLoop) {
        self.sidebar.menu = None;
        let was = self.sidebar.chosen_in(&self.sidebar_rows());
        if many > 1 {
            let picked = std::mem::take(&mut self.sidebar.picked);
            self.sidebar.anchor = None;
            let is_picked = |t: &Tab| picked.contains(&t.id);
            match item {
                MenuItem::MoveToNew => {
                    self.sidebar.picked = picked;
                    self.group_picked();
                }
                MenuItem::MoveTo(group) => self.regroup(|groups, tabs| groups.join_picked(tabs, is_picked, group)),
                MenuItem::Ungroup => self.regroup(|groups, tabs| groups.leave_picked(tabs, is_picked)),
                MenuItem::CloseTab => {
                    self.sidebar.picked = picked;
                    let places = self.picked_places();
                    // Asked first, they stay picked for the same again.
                    if self.close_tabs_asking(&places, el) {
                        self.sidebar.unpick();
                    }
                }
                _ => {}
            }
            self.sidebar_changed(was);
            return;
        }
        let tab = self.tab_of(row);
        match (item, row, tab) {
            (MenuItem::Rename, RowId::Group(id), _) => self.rename_group(id),
            (MenuItem::Rename, _, Some(i)) => self.rename_tab(i),
            (MenuItem::MoveTo(group), _, Some(i)) => self.move_tab_to(i, group),
            // Named in its header in the sidebar; from a pill, where no
            // header shows, a toast says where the tab went.
            (MenuItem::MoveToNew, _, Some(i)) => {
                let tab = self.tabs[i].id;
                if let Some(group) = self.new_group_of(i) {
                    if self.sidebar.is_open() {
                        self.rename_group(group);
                    } else {
                        self.say_tab_grouped(tab, group);
                    }
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
        if self.sidebar.menu.is_some() {
            return self.menu_key(event, el);
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
            // Shift with them picks every tab from where the pick began.
            Key::Named(arrow @ (NamedKey::ArrowUp | NamedKey::ArrowDown)) if mods.shift_key() => {
                if self.sidebar.anchor.is_none() {
                    self.sidebar.anchor = self.sidebar.chosen;
                }
                self.sidebar.step(&rows, if *arrow == NamedKey::ArrowUp { -1 } else { 1 });
                if let Some(to) = self.sidebar.chosen {
                    self.sidebar.pick_to(&rows, to);
                }
            }
            Key::Named(NamedKey::ArrowUp) => {
                self.sidebar.unpick();
                self.sidebar.step(&rows, -1);
            }
            Key::Named(NamedKey::ArrowDown) => {
                self.sidebar.unpick();
                self.sidebar.step(&rows, 1);
            }
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
            // Esc lets go of what's picked first.
            Key::Named(NamedKey::Escape) if !self.sidebar.picked.is_empty() => self.sidebar.unpick(),
            // Pinned, it stays, the keys going back to the pane.
            Key::Named(NamedKey::Escape) if self.sidebar_pinned() => self.sidebar.keys = false,
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

    /// A key while a row's or a pill's menu is open: ↑ and ↓ choose, Enter
    /// or Space does what's chosen, Esc shuts it; nothing else.
    pub(super) fn menu_key(&mut self, event: &KeyEvent, el: &ActiveEventLoop) {
        let Some(menu) = self.sidebar.menu.as_mut() else { return };
        let n = menu.items.len().max(1);
        match &event.logical_key {
            Key::Named(NamedKey::ArrowDown) => menu.chosen = (menu.chosen + 1) % n,
            Key::Named(NamedKey::ArrowUp) => menu.chosen = (menu.chosen + n - 1) % n,
            Key::Named(NamedKey::Enter | NamedKey::Space) => {
                let (row, item, many) = (menu.row, menu.items.get(menu.chosen).copied(), menu.many);
                if let Some(item) = item {
                    self.sidebar_menu_do(row, item, many, el);
                }
            }
            Key::Named(NamedKey::Escape) => self.sidebar.menu = None,
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
                // ⌘-click (Ctrl+click elsewhere) picks a tab or lets it go,
                // Shift-click picks a run; a plain click lets them all go.
                let toggle = if cfg!(target_os = "macos") { self.modifiers.super_key() } else { self.modifiers.control_key() };
                match id {
                    RowId::Tab(tab) if toggle => {
                        self.sidebar.pick(tab);
                        self.request_redraw();
                        return;
                    }
                    RowId::Tab(_) if self.modifiers.shift_key() => {
                        let rows = self.sidebar.rows.clone();
                        self.sidebar.pick_to(&rows, id);
                        self.request_redraw();
                        return;
                    }
                    _ => self.sidebar.unpick(),
                }
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
                    // A group's header folds as the press is let go, unless
                    // it's dragged first.
                    (RowId::Group(_), false) => {
                        self.sidebar.chosen = Some(id);
                        self.sidebar.pressed = Some((id, self.mouse));
                    }
                    // A tab comes forward at once, and may be dragged on.
                    (RowId::Tab(_), false) => {
                        self.sidebar_choose(id);
                        self.sidebar.pressed = Some((id, self.mouse));
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
                if let Some((r, item, many)) = self.sidebar.menu.as_ref().and_then(|m| Some((m.row, *m.items.get(k)?, m.many))) {
                    self.sidebar_menu_do(r, item, many, el);
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
            Some(Hit::SidebarPin) => {
                self.sidebar.menu = None;
                self.toggle_sidebar_pin();
                return;
            }
            Some(Hit::SidebarEdge) => {
                self.sidebar.menu = None;
                self.sidebar.dragging = true;
            }
            // On the card, or its menu, where nothing else is: a menu shut.
            Some(Hit::SidebarCard) => self.sidebar.menu = None,
            // Pinned, a press elsewhere shuts its menu and gives the keys
            // back; popped out, it puts the sidebar away.
            _ if self.sidebar_pinned() => {
                self.sidebar.menu = None;
                self.sidebar.keys = false;
                self.request_redraw();
                return;
            }
            _ => self.close_sidebar(),
        }
        // A press on it, but on a tab or a pane to choose, gives it the keys.
        if self.sidebar_pinned() && !matches!(hit, Some(Hit::SidebarRow(_))) {
            self.sidebar.keys = true;
        }
        self.request_redraw();
    }

    /// Its edge dragged to window x `x` (pixels): its width follows, in
    /// points, within its least and a third of the window. Pinned, the tab
    /// in front's shells follow it at once and the rest as it's let go.
    pub(super) fn drag_sidebar_edge(&mut self, x: f32) {
        let window = self.renderer.as_ref().map_or(self.layout.width, |r| r.size().0 as f32);
        let points = (x - tab_sidebar::MARGIN * self.scale) / self.scale;
        let most = window / 3.0 / self.scale;
        let width = points.clamp(tab_sidebar::LEAST_WIDTH.min(most), most).round();
        if width == self.config.window.tab_sidebar_width {
            return;
        }
        self.config.window.tab_sidebar_width = width;
        if self.sidebar_pinned() {
            if let Some(r) = &self.renderer {
                let (w, h) = r.size();
                let cell = (r.atlas.metrics.width, r.atlas.metrics.height);
                self.layout = Layout::compute(w as f32, h as f32, self.scale, cell, self.around(palette::ink().border.is_some()));
            }
            self.relayout_front();
        }
        self.request_redraw();
    }

    /// Its edge let go: every tab's shells sized to the room it leaves, and
    /// its width saved, as Settings saves a change.
    pub(super) fn sidebar_edge_let_go(&mut self) {
        if self.sidebar_pinned() {
            self.relayout();
        }
        self.save_config();
        self.request_redraw();
    }

    /// The pointer moved to `p` with a tab's pill, a tab's row or a group's
    /// header pressed: once it's gone `DRAG_AFTER` from the press, it's
    /// dragged, and where it would land follows the pointer, in the title
    /// bar or the sidebar, whichever it's over.
    pub(super) fn drag_sidebar_row(&mut self, p: (f32, f32)) {
        if self.sidebar.drag.is_none() {
            let Some((row, from)) = self.sidebar.pressed else { return };
            if (p.0 - from.0).hypot(p.1 - from.1) < tab_sidebar::DRAG_AFTER * self.scale {
                return;
            }
            self.sidebar.menu = None;
            self.sidebar.drag = Some((row, None));
        }
        let to = self.drop_under(p);
        if let Some(drag) = self.sidebar.drag.as_mut() {
            drag.1 = to;
        }
        // Its name follows the pointer.
        self.request_redraw();
    }

    /// Where the tab or group dragged would land with the pointer at `p`:
    /// a tab along the title bar while the pointer's in it (or a little
    /// under it), by its pills as the frame drew them; else in the sidebar
    /// while it's out and the pointer's over its card, by its rows as the
    /// frame drew them, over a row, above the first shown or below the
    /// last (past every row where that's the last); nowhere otherwise.
    fn drop_under(&self, p: (f32, f32)) -> Option<tab_sidebar::Drop> {
        let (dragged, _) = self.sidebar.drag?;
        if let RowId::Tab(id) = dragged {
            if p.1 <= self.layout.title.h + 10.0 * self.scale {
                let (over, part) = self.pill_under(p.0)?;
                return tab_sidebar::bar_drop(&self.listed(), id, over, part);
            }
        }
        let card = self.rect_of(Hit::SidebarCard).filter(|_| self.sidebar.is_open())?;
        if p.0 < card.x || p.0 > card.x + card.w {
            return None;
        }
        let rows = &self.sidebar.rows;
        let drawn: Vec<(usize, Rect)> = (0..rows.len()).filter_map(|i| Some((i, self.rect_of(Hit::SidebarRow(i))?))).collect();
        let first = drawn.first()?.0;
        let (over, part) = match drawn.iter().rev().find(|(_, r)| r.y <= p.1) {
            None => (first, 0.0),
            // Each row's slot runs to the next one's top.
            Some(&(i, r)) => {
                let slot = drawn.iter().find(|(k, _)| *k > i).map_or(r.h + 2.0 * self.scale, |(_, next)| next.y - r.y);
                match (p.1 - r.y) / slot.max(1.0) {
                    part if part <= 1.0 => (i, part),
                    _ if i + 1 == rows.len() => (rows.len(), 0.0),
                    _ => (i, 1.0),
                }
            }
        };
        let group_of = |id: u64| self.tabs.iter().find(|t| t.id == id).and_then(|t| t.group);
        tab_sidebar::drop_at(rows, group_of, dragged, over, part)
    }

    /// The title bar's pill under window x `x` (pixels), and how far across
    /// it, as the frame last drew them: the gap halfway to the next counts
    /// as the pill's edge, left of the first is its start, and right of the
    /// last past every pill.
    fn pill_under(&self, x: f32) -> Option<(usize, f32)> {
        let pills = self.pills.as_slice();
        if pills.len() != self.tabs.len() {
            return None;
        }
        let first = pills.first()?;
        if x < first.x {
            return Some((0, 0.0));
        }
        let half_gap = 3.0 * self.scale;
        let i = pills.iter().rposition(|r| r.x - half_gap <= x)?;
        let r = pills[i];
        if x > r.x + r.w + half_gap && i + 1 == pills.len() {
            return Some((pills.len(), 0.0));
        }
        Some((i, ((x - r.x) / r.w.max(1.0)).clamp(0.0, 1.0)))
    }

    /// A tab or a group being dragged, as the title bar and the pointer
    /// show it: the tab's pill dimmed, where in the bar it would land, and
    /// its name, or the group's, beside the pointer.
    pub(super) fn tab_drag_view(&self) -> Option<ui::TabDrag> {
        let (dragged, to) = self.sidebar.drag?;
        let tab = match dragged {
            RowId::Tab(_) => self.tab_of(dragged),
            _ => None,
        };
        let name = match dragged {
            RowId::Group(g) => self.groups.get(g).map(|g| g.name.clone()).unwrap_or_default(),
            _ => tab.and_then(|i| self.tab_labels().get(i).map(|l| l.label.clone())).unwrap_or_default(),
        };
        let bar = match to {
            Some(tab_sidebar::Drop::Bar { at, group }) => Some((at, group.and_then(|g| self.groups.get(g)).map(|g| g.rgb()))),
            _ => None,
        };
        Some(ui::TabDrag { tab, bar, name })
    }

    /// The pointer let go with a pill or a row pressed: what's dragged
    /// lands where it showed it would (nowhere shown, it stays); a group's
    /// header pressed and let go without a drag folds, as its click does.
    pub(super) fn sidebar_row_let_go(&mut self) {
        let pressed = self.sidebar.pressed.take();
        match self.sidebar.drag.take() {
            Some((row, Some(to))) => self.drop_sidebar_row(row, to),
            Some(_) => {}
            None => {
                if let Some((row @ RowId::Group(_), _)) = pressed {
                    self.sidebar_choose(row);
                }
            }
        }
        self.request_redraw();
    }

    /// Row `row` dropped at `to`: a tab into a group, or among the tabs
    /// (by the sidebar's rows or along the title bar) into a group or
    /// loose; a group with all its tabs between the others. A group a tab
    /// goes into opens, so it's seen there.
    fn drop_sidebar_row(&mut self, row: RowId, to: tab_sidebar::Drop) {
        use tab_sidebar::Drop;
        let rows = self.sidebar.rows.clone();
        let was = self.sidebar.chosen_in(&rows);
        let before = |s: &Self, at: usize| tab_sidebar::landing(&rows, &s.listed(), at);
        match (row, to) {
            (RowId::Tab(_), Drop::Into(g)) => {
                if let Some(i) = self.tab_of(row) {
                    self.move_tab_to(i, g);
                    self.groups.fold(g, false);
                }
            }
            (RowId::Tab(_), Drop::Gap { at, group }) => {
                if let Some(i) = self.tab_of(row) {
                    let b = before(self, at);
                    self.regroup(|groups, tabs| groups.place(tabs, i, b, group));
                }
            }
            (RowId::Tab(_), Drop::Bar { at, group }) => {
                if let Some(i) = self.tab_of(row) {
                    let b = (at < self.tabs.len()).then_some(at);
                    self.regroup(|groups, tabs| groups.place(tabs, i, b, group));
                    if let Some(g) = group {
                        self.groups.fold(g, false);
                    }
                }
            }
            (RowId::Group(id), Drop::Gap { at, .. }) => {
                let b = before(self, at);
                self.regroup(|groups, tabs| groups.place_group(tabs, id, b));
            }
            _ => return,
        }
        self.sidebar.chosen = Some(row);
        self.sidebar_changed(was);
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
        // A tab's menu from its pill: a menu alone, the keys in it.
        if let Some(m) = self.sidebar.menu.as_ref().filter(|_| self.pill_menu_open()) {
            let items = m.items.iter().enumerate().map(|(k, &item)| Item { label: tab_sidebar::words(item, &self.groups, m.many), rect: self.rect_of(Hit::SidebarMenuItem(k)), ..Item::default() }).collect();
            let title = self.tab_of(m.row).and_then(|i| self.tab_labels().get(i).map(|l| l.label.clone())).unwrap_or_default();
            return Some(Dialog { kind: DialogKind::TabSidebar, title, lists: vec![List { label: t("sidebar.tab_menu").to_string(), chooser: Chooser::SidebarMenu, items, chosen: Some(m.chosen) }], ..Dialog::default() });
        }
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
                        Item { label, detail: String::new(), rect, tree: Some((row.level, Some(open), false)) }
                    }
                    RowId::Tab(id) => {
                        let i = self.tab_of(row.id).unwrap_or(0);
                        let tab = &self.tabs[i];
                        let label = labels.get(i).map(|l| l.label.clone()).unwrap_or(name);
                        let open = (tab.panes.len() > 1).then(|| self.sidebar.splits_open.contains(&id));
                        let picked = self.sidebar.several().is_some_and(|p| p.contains(&id));
                        Item { label, detail: Self::line_words(&tab_sidebar::line(&self.doing(tab.pane()))), rect, tree: Some((row.level, open, picked)) }
                    }
                    RowId::Pane(_, p) => {
                        let detail = self.tab_of(row.id).and_then(|i| self.tabs[i].panes.get(p)).map(|pane| Self::line_words(&tab_sidebar::line(&self.doing(pane)))).unwrap_or_default();
                        Item { label: name, detail, rect, tree: Some((row.level, None, false)) }
                    }
                }
            })
            .collect();
        // The row in front among them: the one the keys are on.
        let chosen = self.sidebar.chosen_in(rows);
        let mut lists = vec![List { label: t("sidebar.title").to_string(), chooser: Chooser::Sidebar, items, chosen }];
        if let Some(m) = &self.sidebar.menu {
            let items = m.items.iter().enumerate().map(|(k, &item)| Item { label: tab_sidebar::words(item, &self.groups, m.many), rect: self.rect_of(Hit::SidebarMenuItem(k)), ..Item::default() }).collect();
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
            (Some(m), _) if m.pill => Focus::Item(DialogKind::TabSidebar, 0, m.chosen),
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
    /// written as `tab-sidebar.png`, then pinned beside the split tab as
    /// `tab-sidebar-pinned.png`, then everything as it was.
    pub(super) fn show_sidebar(&mut self, (path, pinned): (&std::path::Path, &std::path::Path), first: usize, el: &ActiveEventLoop) {
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
        // Pinned, the terminal narrowed for it and the keys with the pane.
        self.config.window.tab_sidebar = tab_sidebar::PINNED.into();
        self.follow_sidebar_pin();
        self.sidebar.keys = false;
        self.relayout();
        self.capture(pinned, true);
        self.config.window.tab_sidebar = tab_sidebar::POPS_OUT.into();
        self.follow_sidebar_pin();
        self.relayout();
        self.close_sidebar();
        for i in (first..first + 4).rev() {
            self.close_tab(i, el);
        }
        self.select_tab(front.min(self.tabs.len().saturating_sub(1)));
        self.focus_pane(focus);
    }

}
