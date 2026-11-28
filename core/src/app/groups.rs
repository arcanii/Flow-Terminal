//! The tabs' groups in the window (`docs/design/tab-groups.md`'s second
//! step; `crate::tab_groups` holds the rules that keep each group's tabs
//! together). New Group puts the tab in front into a group of its own,
//! Move Tab to Group moves it into another, after that group's tabs, and
//! Remove Tab from Group takes it out, all from the command palette (New
//! Group from the Mac's Shell menu too, and from a key the user gives it).
//! A tab opened from the tab in front joins its group, and the groups go
//! into `tabs.json` with their tabs. Each tab in a group is marked in its
//! group's color under its pill in the title bar.

use super::*;

impl App {
    /// A group's name where it's given none: "Group 1", "Group 2".
    pub(super) fn group_numbered(n: usize) -> String {
        tf("tab_group.name", &[("n", &n)])
    }

    /// New Group: the tab in front goes into a group of its own, where it
    /// is (or, in a group already, just after the rest of it).
    pub(super) fn new_group(&mut self) {
        let Some((group, at)) = self.groups.new_group(&mut self.tabs, self.active, Self::group_numbered) else { return };
        self.active = at;
        self.say_grouped(Some(group));
    }

    /// Move Tab to Group: the tab in front goes into group `group`, after
    /// its tabs.
    pub(super) fn move_to_group(&mut self, group: u64) {
        self.active = self.groups.join(&mut self.tabs, self.active, group);
        self.say_grouped(Some(group));
    }

    /// Remove Tab from Group: the tab in front leaves its group, just
    /// after the rest of it.
    pub(super) fn ungroup_tab(&mut self) {
        if self.tabs.get(self.active).is_none_or(|t| t.group.is_none()) {
            return;
        }
        self.active = self.groups.leave(&mut self.tabs, self.active);
        self.say_grouped(None);
    }

    /// A toast saying which group the tab in front is in now, or none.
    fn say_grouped(&mut self, group: Option<u64>) {
        let tab = self.tab_labels().get(self.active).map(|l| l.label.clone()).unwrap_or_default();
        match group.and_then(|id| self.groups.get(id)) {
            Some(g) => {
                let words = tf("toast.grouped", &[("tab", &tab), ("group", &g.name)]);
                self.show_toast(words);
            }
            None => self.show_toast(tf("toast.ungrouped", &[("tab", &tab)])),
        }
    }

    /// Go to Tab: the tab `id` comes to the front, wherever it is now.
    pub(super) fn go_to_tab(&mut self, id: u64) {
        if let Some(i) = self.tabs.iter().position(|t| t.id == id) {
            self.select_tab(i);
        }
    }

    /// A tab just opened at the end of the bar, from the tab in front, goes
    /// into that tab's group (`group`), after its tabs, and stays in front.
    /// Loose there, it stays at the end.
    pub(super) fn join_front_group(&mut self, group: Option<u64>) {
        let (Some(group), Some(last)) = (group, self.tabs.len().checked_sub(1)) else { return };
        let at = self.groups.join(&mut self.tabs, last, group);
        self.select_tab(at);
    }

    /// A group kept in `tabs.json`, the `k`th, made again: its name held
    /// as a tab's is (a file written by hand may hold anything), else
    /// numbered by its place, and its color by its name, else the next in
    /// turn.
    pub(super) fn kept_group(&mut self, kept: &crate::config::OpenGroup, k: usize) -> u64 {
        let name = tab_name(&kept.name).unwrap_or_else(|| Self::group_numbered(k + 1));
        self.groups.add(name, crate::tab_groups::color_named(&kept.color))
    }

    /// What the palette offers of the tabs and groups: each tab to go to,
    /// with its group's name, and each group the tab in front may move to.
    pub(super) fn palette_tabs(&self) -> crate::command_palette::Tabs {
        let labels = self.tab_labels();
        let front = self.tabs.get(self.active).and_then(|t| t.group);
        let tabs = self.tabs.iter().zip(labels).map(|(t, l)| (t.id, l.label, self.groups.of(t).map(|g| g.name.clone()).unwrap_or_default())).collect();
        let groups = self.groups.all().iter().filter(|g| Some(g.id) != front).map(|g| (g.id, g.name.clone())).collect();
        crate::command_palette::Tabs { tabs, groups, grouped: front.is_some() }
    }
}
