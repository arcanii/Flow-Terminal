//! The tab sidebar (`docs/design/tab-groups.md`'s third step): a card down
//! the window's left side listing the tabs in their order, each group's
//! under its header (its color, its name and how many tabs it holds),
//! folded shut or open, a split tab's panes under it where it's folded
//! open, and under each tab a line saying what it's doing. It pops out
//! over the terminal with its key (⇧⌘S, Ctrl+Shift+S), the title bar's
//! button, the palette or the Mac's View menu, takes the keys as the
//! palette does, and stays out as tabs are chosen, until Esc, its key or a
//! click outside it.
//!
//! What's here is apart from the window, so it's tested without one: the
//! rows as they follow from the tabs and groups ([`rows`]), the line under a
//! tab ([`line`]), what the arrows do ([`arrow`]), the menus ([`tab_menu`],
//! [`group_menu`]) and its state ([`Sidebar`]). `App` draws it
//! (`ui::draw_sidebar`) and does what it asks (`core/src/app/sidebar.rs`).

use std::cell::Cell;
use std::time::{Duration, Instant};

use crate::i18n::{t, tf};
use crate::tab_groups::{TabGroups, COLORS};

/// How wide it is, in points, at most a third of the window.
pub const WIDTH: f32 = 260.0;

/// How long it takes to slide out (at once in efficient mode).
pub const SLIDE: Duration = Duration::from_millis(150);

/// The least it's dragged to, in points, where the window has room.
pub const LEAST_WIDTH: f32 = 160.0;

/// Its gap from the window's left edge, in points.
pub const MARGIN: f32 = 8.0;

/// `window.tab_sidebar`'s two ways: over the terminal, or beside it.
pub const POPS_OUT: &str = "pops_out";
pub const PINNED: &str = "pinned";
pub const MODES: [&str; 2] = [POPS_OUT, PINNED];

/// The fewest columns a pinned sidebar leaves the terminal: in a window
/// narrower than that, it pops out over the terminal instead.
pub const PIN_LEAST_COLS: usize = 20;

/// Whether a sidebar taking `left` pixels at the left of a window `window`
/// pixels wide leaves the terminal [`PIN_LEAST_COLS`] columns of
/// `cell_w`, besides its margins (`Layout::compute`'s, 14 points a side).
pub fn pin_fits(window: f32, left: f32, cell_w: f32, scale: f32) -> bool {
    (window - left - 28.0 * scale) / cell_w.max(1.0) >= PIN_LEAST_COLS as f32
}

/// A row, by what it's of, so the row the keys are on stays that one as
/// tabs open, close and move.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowId {
    /// A group's header, by the group's id.
    Group(u64),
    /// A tab, by its id.
    Tab(u64),
    /// A split tab's pane, by the tab's id and the pane's place in it.
    Pane(u64, usize),
}

/// A row and how far in it's set: 0 for a group's header and a loose tab,
/// 1 for a group's tab, and one more for a split tab's panes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row {
    pub id: RowId,
    pub level: usize,
}

/// A tab as the rows see it: its id, its group and how many panes it has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Listed {
    pub id: u64,
    pub group: Option<u64>,
    pub panes: usize,
}

/// The rows for `tabs`, in their order: each group's header before its
/// tabs, which are left out where it's folded, and a split tab's panes
/// after it where it's in `splits_open`.
pub fn rows(tabs: &[Listed], groups: &TabGroups, splits_open: &[u64]) -> Vec<Row> {
    let mut rows = Vec::new();
    let mut before = None;
    for tab in tabs {
        let group = tab.group.and_then(|g| groups.get(g));
        if let Some(g) = group.filter(|g| before != Some(g.id)) {
            rows.push(Row { id: RowId::Group(g.id), level: 0 });
        }
        before = group.map(|g| g.id);
        if group.is_some_and(|g| g.folded) {
            continue;
        }
        let level = usize::from(group.is_some());
        rows.push(Row { id: RowId::Tab(tab.id), level });
        if tab.panes > 1 && splits_open.contains(&tab.id) {
            rows.extend((0..tab.panes).map(|p| Row { id: RowId::Pane(tab.id, p), level: level + 1 }));
        }
    }
    rows
}

/// How far a press on a row goes, in points, before it drags the row, so a
/// click is still a click.
pub const DRAG_AFTER: f32 = 4.0;

/// Where a row dragged in the sidebar would land.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Drop {
    /// Onto group `g`'s header: a tab goes into it, after its tabs.
    Into(u64),
    /// Into the gap above the row at `at` (the rows' count: after the
    /// last): into group `group` there, where the rows either side are its,
    /// else loose.
    Gap { at: usize, group: Option<u64> },
    /// Into the title bar's gap before the tab at `at` (the tabs' count:
    /// after the last): into group `group` there, else loose ([`bar_drop`]).
    Bar { at: usize, group: Option<u64> },
}

/// Where row `dragged` (a tab's or a group's header) would land with the
/// pointer over the row at `over` (the rows' count: past the last),
/// `part` of the way down it (0 at its top, 1 at its foot); `group_of`
/// says each tab's group, by its id. A tab lands on a header's middle
/// (into that group) or in the gap nearer the pointer, never among a split
/// tab's panes; a group only where it parts no other, the nearer end of
/// one it's over. None where it would go nowhere new.
pub fn drop_at(rows: &[Row], group_of: impl Fn(u64) -> Option<u64>, dragged: RowId, over: usize, part: f32) -> Option<Drop> {
    let group_in = |row: Option<&Row>| match row.map(|r| r.id) {
        Some(RowId::Tab(id) | RowId::Pane(id, _)) => group_of(id),
        _ => None,
    };
    if let (RowId::Tab(tab), Some(&Row { id: RowId::Group(g), .. })) = (dragged, rows.get(over)) {
        if (0.25..0.75).contains(&part) {
            return (group_of(tab) != Some(g)).then_some(Drop::Into(g));
        }
    }
    let mut at = (if part < 0.5 { over } else { over + 1 }).min(rows.len());
    while rows.get(at).is_some_and(|r| matches!(r.id, RowId::Pane(..))) {
        at += 1;
    }
    // A gap is a group's where the row below it is one of its tabs (the
    // row above being its header or another of them).
    let mut group = group_in(rows.get(at));
    // A group's rows: its header to just past its last tab's.
    let span = |id: u64| {
        let head = rows.iter().position(|r| r.id == RowId::Group(id))?;
        let last = rows.iter().rposition(|r| group_in(Some(r)) == Some(id)).unwrap_or(head);
        Some((head, last + 1))
    };
    match dragged {
        RowId::Tab(tab) => {
            let mine = rows.iter().position(|r| r.id == RowId::Tab(tab));
            let end = mine.map(|m| m + 1 + rows[m + 1..].iter().take_while(|r| matches!(r.id, RowId::Pane(t, _) if t == tab)).count());
            if (Some(at) == mine || Some(at) == end) && group == group_of(tab) {
                return None;
            }
        }
        RowId::Group(id) => {
            if let Some((head, end)) = group.filter(|&g| g != id).and_then(span) {
                at = if at - head <= end - at { head } else { end };
                group = None;
            }
            let (head, end) = span(id)?;
            if (head..=end).contains(&at) {
                return None;
            }
        }
        RowId::Pane(..) => return None,
    }
    Some(Drop::Gap { at, group })
}

/// Where tab `dragged` (by its id) would land along the title bar, `tabs`
/// being its pills in order, with the pointer over the pill at `over` (the
/// tabs' count: past the last), `part` of the way across it. It goes before
/// or after that pill by the half the pointer's in, into the pill's group;
/// but over a loose pill, past the last, or in the outer quarter of a
/// group's first or last pill, loose, so a tab can land between two groups
/// and a group's tabs are never parted. None where it would go nowhere new.
pub fn bar_drop(tabs: &[Listed], dragged: u64, over: usize, part: f32) -> Option<Drop> {
    let i = tabs.iter().position(|t| t.id == dragged)?;
    let (at, group) = match tabs.get(over) {
        None => (tabs.len(), None),
        Some(t) => {
            let at = if part < 0.5 { over } else { over + 1 };
            let group = t.group.filter(|&g| {
                let first = over == 0 || tabs[over - 1].group != Some(g);
                let last = tabs.get(over + 1).is_none_or(|n| n.group != Some(g));
                !(first && part < 0.25 || last && part > 0.75)
            });
            (at, group)
        }
    };
    if (at == i || at == i + 1) && group == tabs[i].group {
        return None;
    }
    Some(Drop::Bar { at, group })
}

/// The tab, by its place in `tabs`, that a row dropped in the gap above
/// row `at` goes before: that row's tab, or a group's first; none past
/// the last row, so at the end.
pub fn landing(rows: &[Row], tabs: &[Listed], at: usize) -> Option<usize> {
    match rows.get(at)?.id {
        RowId::Tab(id) | RowId::Pane(id, _) => tabs.iter().position(|t| t.id == id),
        RowId::Group(g) => tabs.iter().position(|t| t.group == Some(g)),
    }
}

/// What a pane is doing, as its shell said it: a program's progress (its
/// share done, none where it says only that it's busy, and whether it
/// failed), the command running, how the last one ended, and its folder.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Doing {
    pub progress: Option<(Option<u8>, bool)>,
    pub running: Option<String>,
    pub ended: Option<(String, i32)>,
    pub folder: Option<String>,
}

/// The line under a tab's (or a pane's) name.
#[derive(Debug, Clone, PartialEq)]
pub enum Line {
    /// A program's progress: its share done, and whether it failed.
    Progress { percent: Option<u8>, failed: bool },
    /// The command running.
    Running(String),
    /// How the last command ended, until the next starts.
    Ended { command: String, ok: bool },
    /// Its folder, as [`folder_shown`] writes it.
    Folder(String),
    Nothing,
}

/// The first of what `doing` has: progress, the command running, how the
/// last ended, the folder.
pub fn line(doing: &Doing) -> Line {
    if let Some((percent, failed)) = doing.progress {
        return Line::Progress { percent, failed };
    }
    if let Some(command) = doing.running.as_ref().filter(|c| !c.is_empty()) {
        return Line::Running(command.clone());
    }
    if let Some((command, code)) = doing.ended.as_ref().filter(|(c, _)| !c.is_empty()) {
        return Line::Ended { command: command.clone(), ok: *code == 0 };
    }
    match &doing.folder {
        Some(folder) => Line::Folder(folder.clone()),
        None => Line::Nothing,
    }
}

/// `folder` as the line shows it: `~` for the home folder `home`, and what's
/// under it from `~/`.
pub fn folder_shown(folder: &str, home: Option<&str>) -> String {
    let Some(home) = home.map(|h| h.trim_end_matches(['/', '\\'])).filter(|h| !h.is_empty()) else { return folder.to_string() };
    match folder.strip_prefix(home) {
        Some("") => "~".to_string(),
        Some(rest) if rest.starts_with(['/', '\\']) => format!("~{rest}"),
        _ => folder.to_string(),
    }
}

/// What → or ← does on a row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arrow {
    /// Folds group `id` shut (true) or open.
    Fold(u64, bool),
    /// Folds split tab `id`'s panes open (true) or shut.
    Split(u64, bool),
    /// The keys go to this row.
    Go(RowId),
    Nothing,
}

/// What → (`right`) or ← does on `row`: → folds a group or a split tab
/// open; ← folds an open one shut, and from a group's tab goes to its
/// header, from a pane to its tab.
pub fn arrow(row: Row, rows: &[Row], groups: &TabGroups, splits_open: &[u64], panes: usize, right: bool) -> Arrow {
    match (row.id, right) {
        (RowId::Group(id), right) => match groups.get(id) {
            Some(g) if g.folded == right => Arrow::Fold(id, !right),
            _ => Arrow::Nothing,
        },
        (RowId::Tab(id), true) if panes > 1 && !splits_open.contains(&id) => Arrow::Split(id, true),
        (RowId::Tab(id), false) if splits_open.contains(&id) => Arrow::Split(id, false),
        (RowId::Tab(_), false) if row.level > 0 => {
            // Its group's header: the nearest group row above it.
            let at = rows.iter().position(|r| *r == row).unwrap_or(0);
            rows[..at].iter().rev().find(|r| matches!(r.id, RowId::Group(_))).map_or(Arrow::Nothing, |r| Arrow::Go(r.id))
        }
        (RowId::Pane(tab, _), false) => Arrow::Go(RowId::Tab(tab)),
        _ => Arrow::Nothing,
    }
}

/// The tabs whose rows are from `from` to `to`, either way round, in the
/// rows' order; group headers and panes passed over.
pub fn between(rows: &[Row], from: RowId, to: RowId) -> Vec<u64> {
    let (Some(a), Some(b)) = (rows.iter().position(|r| r.id == from), rows.iter().position(|r| r.id == to)) else {
        return match to {
            RowId::Tab(id) => vec![id],
            _ => Vec::new(),
        };
    };
    rows[a.min(b)..=a.max(b)]
        .iter()
        .filter_map(|r| match r.id {
            RowId::Tab(id) => Some(id),
            _ => None,
        })
        .collect()
}

/// The first row after `from` (from the top where none), going round,
/// whose name starts with what `letter` typed, in any case: typing a
/// letter goes to it, as in a list.
pub fn starting_with(names: &[String], from: Option<usize>, letter: &str) -> Option<usize> {
    let letter = letter.to_lowercase();
    if letter.is_empty() || names.is_empty() {
        return None;
    }
    let start = from.map_or(0, |f| f + 1);
    (0..names.len()).map(|k| (start + k) % names.len()).find(|&i| names[i].to_lowercase().starts_with(&letter))
}

/// Whether Close Group closes a group's tabs at once: where each waits at
/// its prompt (`idle`) and they aren't every tab (`every`, so Flow would
/// quit), or it was pressed once already (`asked`, within
/// `CONFIRM_WINDOW`).
pub fn group_closes_now(idle: bool, every: bool, asked: bool) -> bool {
    (idle && !every) || asked
}

/// What a row's … menu can do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuItem {
    /// Name the tab or the group, in its row.
    Rename,
    /// The tab into this group, by its id.
    MoveTo(u64),
    /// The tab into a group of its own.
    MoveToNew,
    /// The tab out of its group.
    Ungroup,
    RestartTab,
    CloseTab,
    /// The group in this color, by its place in `tab_groups::COLORS`.
    Color(usize),
    /// Every tab of the group out of it.
    UngroupAll,
    /// Every tab of the group closed.
    CloseGroup,
}

/// A row's menu while it's open, and the item the keys are on: for its
/// row alone, or for the `many` tabs picked with it, and opened from the
/// row's … in the sidebar or from the tab's pill in the title bar (`pill`).
#[derive(Debug, Clone, PartialEq)]
pub struct Menu {
    pub row: RowId,
    pub items: Vec<MenuItem>,
    pub chosen: usize,
    pub many: usize,
    pub pill: bool,
}

/// A tab's menu: Rename, Move to each other group and to a new one,
/// Ungroup where it's in one, Restart Tab and Close.
pub fn tab_menu(tab: u64, group: Option<u64>, groups: &TabGroups) -> Menu {
    let mut items = vec![MenuItem::Rename];
    items.extend(groups.all().iter().filter(|g| Some(g.id) != group).map(|g| MenuItem::MoveTo(g.id)));
    items.push(MenuItem::MoveToNew);
    if group.is_some() {
        items.push(MenuItem::Ungroup);
    }
    items.extend([MenuItem::RestartTab, MenuItem::CloseTab]);
    Menu { row: RowId::Tab(tab), items, chosen: 0, many: 1, pill: false }
}

/// The menu of `many` tabs picked together, opened from tab `tab`'s row:
/// a new group of them, each group to move them to, Remove from Group
/// where one is in a group (`grouped`), and Close; Rename and Restart Tab
/// are for one tab.
pub fn picked_menu(tab: u64, many: usize, grouped: bool, groups: &TabGroups) -> Menu {
    let mut items = vec![MenuItem::MoveToNew];
    items.extend(groups.all().iter().map(|g| MenuItem::MoveTo(g.id)));
    if grouped {
        items.push(MenuItem::Ungroup);
    }
    items.push(MenuItem::CloseTab);
    Menu { row: RowId::Tab(tab), items, chosen: 0, many, pill: false }
}

/// A group's menu: Rename, each color, Ungroup and Close Group.
pub fn group_menu(group: u64) -> Menu {
    let mut items = vec![MenuItem::Rename];
    items.extend((0..COLORS.len()).map(MenuItem::Color));
    items.extend([MenuItem::UngroupAll, MenuItem::CloseGroup]);
    Menu { row: RowId::Group(group), items, chosen: 0, many: 1, pill: false }
}

/// An item's words, in the language in use, for `many` tabs where it acts
/// on more than one.
pub fn words(item: MenuItem, groups: &TabGroups, many: usize) -> String {
    let group = |id: u64| groups.get(id).map(|g| g.name.clone()).unwrap_or_default();
    match item {
        MenuItem::MoveTo(id) if many > 1 => tf("sidebar.move_many_to", &[("n", &many), ("group", &group(id))]),
        MenuItem::MoveToNew if many > 1 => tf("sidebar.group_many", &[("n", &many)]),
        MenuItem::Ungroup if many > 1 => tf("sidebar.ungroup_many", &[("n", &many)]),
        MenuItem::CloseTab if many > 1 => tf("sidebar.close_many", &[("n", &many)]),
        MenuItem::Rename => t("sidebar.rename").to_string(),
        MenuItem::MoveTo(id) => tf("sidebar.move_to", &[("group", &group(id))]),
        MenuItem::MoveToNew => t("sidebar.move_to_new").to_string(),
        MenuItem::Ungroup => t("sidebar.ungroup_tab").to_string(),
        MenuItem::RestartTab => t("menu.restart_tab").to_string(),
        MenuItem::CloseTab => t("menu.close_tab").to_string(),
        MenuItem::Color(c) => t(&format!("sidebar.color.{}", COLORS[c % COLORS.len()].0)).to_string(),
        MenuItem::UngroupAll => t("sidebar.ungroup").to_string(),
        MenuItem::CloseGroup => t("sidebar.close_group").to_string(),
    }
}

/// The sidebar's state: whether it's out, the row the keys are on, how far
/// its list is scrolled, the split tabs folded open (which isn't kept), a
/// row's menu, and what waits on a time.
#[derive(Debug, Default)]
pub struct Sidebar {
    /// Out over the terminal, since when (for its slide).
    pub open: Option<Instant>,
    pub chosen: Option<RowId>,
    /// The first row shown, which the drawing keeps the chosen one in view by.
    pub top: Cell<usize>,
    pub splits_open: Vec<u64>,
    pub menu: Option<Menu>,
    /// Close Group pressed once on group `id`, where its tabs aren't all
    /// waiting at their prompts or are every tab: pressed again by then,
    /// within `CONFIRM_WINDOW`, it closes them.
    pub closing: Option<(u64, Instant)>,
    /// The rows as they were last laid out, for what's under the pointer.
    pub rows: Vec<Row>,
    /// The tabs picked together (⌘-click, Shift-click, Shift and the
    /// arrows), by their ids, and the row the pick began at.
    pub picked: Vec<u64>,
    pub anchor: Option<RowId>,
    /// Pinned, whether it has the keys (popped out, it always has); its
    /// key moves them in and out.
    pub keys: bool,
    /// It was opened pinned, so Settings or a program taking the pin away
    /// puts it away (its own pin leaves it out, popped).
    pub pinned: bool,
    /// Its edge is being dragged.
    pub dragging: bool,
    /// A press on a tab's row or a group's header, and where it was (in
    /// pixels): moved [`DRAG_AFTER`] from there, it drags the row.
    pub pressed: Option<(RowId, (f32, f32))>,
    /// The row being dragged, and where it would land as the pointer
    /// stands ([`drop_at`]).
    pub drag: Option<(RowId, Option<Drop>)>,
}

impl Sidebar {
    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    /// How far out it is, 0 to 1, eased; all the way at once in efficient
    /// mode (`at_once`).
    pub fn out(&self, now: Instant, at_once: bool) -> f32 {
        match self.open {
            Some(_) if at_once => 1.0,
            Some(since) => {
                let t = (now.saturating_duration_since(since).as_secs_f32() / SLIDE.as_secs_f32()).clamp(0.0, 1.0);
                1.0 - (1.0 - t) * (1.0 - t) * (1.0 - t)
            }
            None => 0.0,
        }
    }

    /// Whether it's still sliding out, which keeps frames coming.
    pub fn sliding(&self, now: Instant, at_once: bool) -> bool {
        self.open.is_some() && self.out(now, at_once) < 1.0
    }

    /// ⌘-click (Ctrl+click elsewhere) on tab `id`'s row: it's picked, or
    /// not again, the pick beginning there. The row the keys were on comes
    /// into the pick first, so two tabs are picked by a click on the other.
    pub fn pick(&mut self, id: u64) {
        if self.picked.is_empty() {
            if let Some(RowId::Tab(was)) = self.chosen.filter(|c| *c != RowId::Tab(id)) {
                self.picked.push(was);
            }
        }
        match self.picked.iter().position(|t| *t == id) {
            Some(at) => {
                self.picked.remove(at);
            }
            None => self.picked.push(id),
        }
        self.anchor = Some(RowId::Tab(id));
        self.chosen = Some(RowId::Tab(id));
    }

    /// Shift-click, or Shift and an arrow: every tab from where the pick
    /// began (the chosen row, where none had) to `to`, which takes the keys.
    pub fn pick_to(&mut self, rows: &[Row], to: RowId) {
        let anchor = *self.anchor.get_or_insert(self.chosen.unwrap_or(to));
        self.picked = between(rows, anchor, to);
        self.chosen = Some(to);
    }

    /// A plain click or arrow: nothing picked.
    pub fn unpick(&mut self) {
        self.picked.clear();
        self.anchor = None;
    }

    /// The tabs picked, where more than one is; none for one alone, which a
    /// row's own menu and New Group act on as they always have.
    pub fn several(&self) -> Option<&[u64]> {
        (self.picked.len() > 1).then_some(&self.picked[..])
    }

    /// The chosen row's place in `rows`.
    pub fn chosen_in(&self, rows: &[Row]) -> Option<usize> {
        let chosen = self.chosen?;
        rows.iter().position(|r| r.id == chosen)
    }

    /// The keys go `step` rows on from the chosen one (back where it's
    /// below 0), held to the list; to the first row where none is chosen.
    pub fn step(&mut self, rows: &[Row], step: isize) {
        let Some(last) = rows.len().checked_sub(1) else { return };
        let at = match self.chosen_in(rows) {
            Some(at) => (at as isize + step).clamp(0, last as isize) as usize,
            None => 0,
        };
        self.chosen = Some(rows[at].id);
    }

    /// The chosen row is still there after a change; where it's gone (its
    /// tab closed, its group folded), the keys go to what took its place:
    /// its group's header, else the row it would have been at, held to the
    /// list.
    pub fn keep_chosen(&mut self, rows: &[Row], was: Option<usize>, group_of: impl Fn(RowId) -> Option<u64>) {
        let Some(chosen) = self.chosen else { return };
        if rows.iter().any(|r| r.id == chosen) {
            return;
        }
        let header = group_of(chosen).map(RowId::Group).filter(|h| rows.iter().any(|r| r.id == *h));
        self.chosen = header.or_else(|| rows.get(was.unwrap_or(0).min(rows.len().saturating_sub(1))).map(|r| r.id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tab_groups::Grouped;

    #[derive(Debug, Clone)]
    struct T(u64, Option<u64>);

    impl Grouped for T {
        fn group(&self) -> Option<u64> {
            self.1
        }
        fn set_group(&mut self, group: Option<u64>) {
            self.1 = group;
        }
    }

    fn listed(tabs: &[T], panes: &[usize]) -> Vec<Listed> {
        tabs.iter().zip(panes).map(|(t, &panes)| Listed { id: t.0, group: t.1, panes }).collect()
    }

    /// Tabs 1 to 5, 2 and 3 in a group, 4 in another.
    fn grouped() -> (TabGroups, Vec<T>, u64, u64) {
        let mut groups = TabGroups::default();
        let mut tabs: Vec<T> = (1..=5).map(|i| T(i, None)).collect();
        let (one, _) = groups.new_group(&mut tabs, 1, |n| format!("Group {n}")).unwrap();
        groups.join(&mut tabs, 2, one);
        let (two, _) = groups.new_group(&mut tabs, 3, |n| format!("Group {n}")).unwrap();
        (groups, tabs, one, two)
    }

    #[test]
    fn the_rows_are_each_groups_header_then_its_tabs_unless_folded_and_a_split_tabs_panes() {
        let (mut groups, tabs, one, two) = grouped();
        let panes = [1, 2, 1, 1, 3];
        let ids = |rows: &[Row]| rows.iter().map(|r| (r.id, r.level)).collect::<Vec<_>>();
        let all = rows(&listed(&tabs, &panes), &groups, &[]);
        use RowId::*;
        assert_eq!(ids(&all), [(Tab(1), 0), (Group(one), 0), (Tab(2), 1), (Tab(3), 1), (Group(two), 0), (Tab(4), 1), (Tab(5), 0)]);
        // Group 1 folded, and tabs 2 and 5 folded open to their panes.
        groups.fold(one, true);
        let some = rows(&listed(&tabs, &panes), &groups, &[2, 5]);
        assert_eq!(ids(&some), [(Tab(1), 0), (Group(one), 0), (Group(two), 0), (Tab(4), 1), (Tab(5), 0), (Pane(5, 0), 1), (Pane(5, 1), 1), (Pane(5, 2), 1)]);
        assert!(rows(&[], &groups, &[]).is_empty());
    }

    #[test]
    fn a_tab_dragged_lands_on_a_header_or_in_the_gap_nearer_the_pointer() {
        let (groups, tabs, one, _) = grouped();
        let all = rows(&listed(&tabs, &[1; 5]), &groups, &[]);
        let group_of = |id: u64| tabs.iter().find(|t| t.0 == id).and_then(|t| t.1);
        let drop = |rows: &[Row], tab: u64, over: usize, part: f32| drop_at(rows, group_of, RowId::Tab(tab), over, part);
        // Rows: tab 1, group one, 2, 3, group two, 4, 5.
        assert_eq!(drop(&all, 5, 1, 0.5), Some(Drop::Into(one)), "a header's middle");
        assert_eq!(drop(&all, 2, 1, 0.5), None, "its own group's");
        assert_eq!(drop(&all, 5, 1, 0.1), Some(Drop::Gap { at: 1, group: None }), "a header's top: before the group");
        assert_eq!(drop(&all, 5, 2, 0.2), Some(Drop::Gap { at: 2, group: Some(one) }), "between a header and its first tab");
        assert_eq!(drop(&all, 5, 3, 0.8), Some(Drop::Gap { at: 4, group: None }), "after a group's last tab: loose");
        assert_eq!(drop(&all, 3, 3, 0.8), Some(Drop::Gap { at: 4, group: None }), "its own row's foot, out of its group");
        assert_eq!(drop(&all, 3, 3, 0.2), None, "where it is");
        assert_eq!(drop(&all, 1, 0, 0.8), None, "just after itself, loose as it is");
        assert_eq!(drop(&all, 2, all.len(), 0.0), Some(Drop::Gap { at: all.len(), group: None }), "past the last row");
        // Never among a split tab's panes: after them.
        let split = rows(&listed(&tabs, &[1, 2, 1, 1, 1]), &groups, &[2]);
        assert_eq!(split[3].id, RowId::Pane(2, 0));
        assert_eq!(drop(&split, 5, 3, 0.2), Some(Drop::Gap { at: 5, group: Some(one) }));
        assert_eq!(drop_at(&split, group_of, RowId::Pane(2, 1), 0, 0.2), None, "a pane isn't dragged");
        // Where each gap lands among the tabs.
        let tabs_listed = listed(&tabs, &[1; 5]);
        assert_eq!([1, 2, 4, 5, all.len()].map(|at| landing(&all, &tabs_listed, at)), [Some(1), Some(1), Some(3), Some(3), None]);
    }

    #[test]
    fn a_tab_dragged_along_the_title_bar_joins_the_group_it_lands_in_or_none() {
        let (_, tabs, one, two) = grouped();
        let bar = listed(&tabs, &[1; 5]);
        let drop = |tab: u64, over: usize, part: f32| bar_drop(&bar, tab, over, part);
        let gap = |at: usize, group: Option<u64>| Some(Drop::Bar { at, group });
        // Pills: 1, [2 3] in one, [4] in two, 5.
        assert_eq!(drop(5, 1, 0.4), gap(1, Some(one)), "a group's first pill, inside its outer quarter: first in it");
        assert_eq!(drop(5, 1, 0.1), gap(1, None), "its outer quarter: before the group");
        assert_eq!(drop(5, 2, 0.4), gap(2, Some(one)), "between two of its tabs");
        assert_eq!(drop(5, 2, 0.9), gap(3, None), "a group's last pill's outer quarter: after the group");
        assert_eq!(drop(5, 3, 0.5), gap(4, Some(two)), "a group of one tab, its middle");
        assert_eq!(drop(2, 4, 0.2), gap(4, None), "a loose pill: loose beside it");
        assert_eq!(drop(2, bar.len(), 0.0), gap(5, None), "past the last");
        assert_eq!(drop(1, 0, 0.8), None, "beside itself, loose as it is");
        assert_eq!(drop(3, 2, 0.4), None, "where it is in its group");
        assert_eq!(drop(3, 2, 0.9), gap(3, None), "its own pill's outer quarter: out of its group, where it is");
        assert_eq!(drop(9, 0, 0.5), None, "a tab that isn't there");
    }

    #[test]
    fn a_group_dragged_lands_only_between_groups_and_loose_tabs() {
        let (groups, tabs, one, two) = grouped();
        let all = rows(&listed(&tabs, &[1; 5]), &groups, &[]);
        let group_of = |id: u64| tabs.iter().find(|t| t.0 == id).and_then(|t| t.1);
        let drop = |group: u64, over: usize, part: f32| drop_at(&all, group_of, RowId::Group(group), over, part);
        // Rows: tab 1, group one, 2, 3, group two, 4, 5.
        assert_eq!(drop(one, 0, 0.2), Some(Drop::Gap { at: 0, group: None }));
        assert_eq!(drop(one, 6, 0.8), Some(Drop::Gap { at: 7, group: None }));
        assert_eq!(drop(one, 1, 0.5), None, "onto its own header");
        assert_eq!(drop(one, 3, 0.2), None, "within its own tabs");
        assert_eq!(drop(one, 4, 0.2), None, "just after itself");
        // Over another's tabs: that group's nearer end.
        assert_eq!(drop(two, 2, 0.2), Some(Drop::Gap { at: 1, group: None }), "nearer its header");
        assert_eq!(drop(two, 2, 0.8), None, "nearer its end, which is just before two");
        assert_eq!(drop(two, 0, 0.8), Some(Drop::Gap { at: 1, group: None }), "a tab's foot");
    }

    #[test]
    fn the_line_says_progress_then_the_command_running_then_the_last_end_then_the_folder() {
        let all = Doing { progress: Some((Some(40), false)), running: Some("cargo build".into()), ended: Some(("make".into(), 2)), folder: Some("~/src".into()) };
        assert_eq!(line(&all), Line::Progress { percent: Some(40), failed: false });
        let running = Doing { progress: None, ..all.clone() };
        assert_eq!(line(&running), Line::Running("cargo build".into()));
        let ended = Doing { running: None, ..running.clone() };
        assert_eq!(line(&ended), Line::Ended { command: "make".into(), ok: false });
        assert_eq!(line(&Doing { ended: Some(("ls".into(), 0)), ..ended.clone() }), Line::Ended { command: "ls".into(), ok: true });
        let idle = Doing { ended: None, ..ended };
        assert_eq!(line(&idle), Line::Folder("~/src".into()));
        assert_eq!(line(&Doing::default()), Line::Nothing);
    }

    #[test]
    fn a_folder_under_home_starts_with_a_tilde() {
        assert_eq!(folder_shown("/Users/kim", Some("/Users/kim")), "~");
        assert_eq!(folder_shown("/Users/kim/src/flow", Some("/Users/kim/")), "~/src/flow");
        assert_eq!(folder_shown("/Users/kimberly", Some("/Users/kim")), "/Users/kimberly", "not a folder under it");
        assert_eq!(folder_shown("C:\\Users\\kim\\src", Some("C:\\Users\\kim")), "~\\src");
        assert_eq!(folder_shown("/tmp", None), "/tmp");
    }

    #[test]
    fn the_arrows_fold_groups_and_split_tabs_and_go_up_to_a_header() {
        let (mut groups, tabs, one, _) = grouped();
        let panes = [1, 2, 1, 1, 1];
        let all = rows(&listed(&tabs, &panes), &groups, &[]);
        let row = |id: RowId| *all.iter().find(|r| r.id == id).unwrap();
        use RowId::*;
        assert_eq!(arrow(row(Group(one)), &all, &groups, &[], 1, false), Arrow::Fold(one, true));
        assert_eq!(arrow(row(Group(one)), &all, &groups, &[], 1, true), Arrow::Nothing, "open already");
        assert_eq!(arrow(row(Tab(2)), &all, &groups, &[], 2, true), Arrow::Split(2, true));
        assert_eq!(arrow(row(Tab(2)), &all, &groups, &[2], 2, false), Arrow::Split(2, false));
        assert_eq!(arrow(row(Tab(3)), &all, &groups, &[], 1, false), Arrow::Go(Group(one)));
        assert_eq!(arrow(row(Tab(1)), &all, &groups, &[], 1, false), Arrow::Nothing, "loose");
        assert_eq!(arrow(Row { id: Pane(2, 1), level: 2 }, &all, &groups, &[2], 2, false), Arrow::Go(Tab(2)));
        groups.fold(one, true);
        assert_eq!(arrow(row(Group(one)), &all, &groups, &[], 1, true), Arrow::Fold(one, false));
    }

    #[test]
    fn a_letter_goes_to_the_next_row_whose_name_starts_with_it() {
        let names: Vec<String> = ["zsh 1", "Web", "zsh 2", "bash"].iter().map(|s| s.to_string()).collect();
        assert_eq!(starting_with(&names, None, "z"), Some(0));
        assert_eq!(starting_with(&names, Some(0), "Z"), Some(2));
        assert_eq!(starting_with(&names, Some(2), "z"), Some(0), "round again");
        assert_eq!(starting_with(&names, Some(0), "w"), Some(1));
        assert_eq!(starting_with(&names, None, "q"), None);
    }

    #[test]
    fn the_menus_hold_what_a_tab_or_a_group_can_do() {
        let (groups, _, one, two) = grouped();
        use MenuItem::*;
        assert_eq!(tab_menu(2, Some(one), &groups).items, [Rename, MoveTo(two), MoveToNew, Ungroup, RestartTab, CloseTab]);
        assert_eq!(tab_menu(1, None, &groups).items, [Rename, MoveTo(one), MoveTo(two), MoveToNew, RestartTab, CloseTab]);
        let menu = group_menu(one);
        assert_eq!(menu.items.len(), 3 + COLORS.len());
        assert_eq!((menu.items[1], *menu.items.last().unwrap()), (Color(0), CloseGroup));
        crate::i18n::set_language("en", &[]);
        assert_eq!(words(MoveTo(two), &groups, 1), "Move to Group 2");
        assert_eq!(words(Color(4), &groups, 1), "Teal");
        // Several picked: no Rename or Restart Tab, and their counts said.
        let many = picked_menu(1, 3, true, &groups);
        assert_eq!(many.items, [MoveToNew, MoveTo(one), MoveTo(two), Ungroup, CloseTab]);
        assert_eq!(words(MoveTo(two), &groups, 3), "Move 3 Tabs to Group 2");
        assert_eq!(words(MoveToNew, &groups, 3), "New Group of 3 Tabs");
        assert_eq!(words(CloseTab, &groups, 3), "Close 3 Tabs");
        assert!(!picked_menu(1, 2, false, &groups).items.contains(&Ungroup), "none in a group");
    }

    #[test]
    fn tabs_are_picked_by_a_click_each_or_a_run_from_where_the_pick_began() {
        let (groups, tabs, one, _) = grouped();
        let all = rows(&listed(&tabs, &[1; 5]), &groups, &[]);
        let mut s = Sidebar { chosen: Some(RowId::Tab(1)), ..Sidebar::default() };
        // ⌘-click on tab 3 picks it with the row the keys were on.
        s.pick(3);
        assert_eq!((s.picked.clone(), s.chosen), (vec![1, 3], Some(RowId::Tab(3))));
        s.pick(3);
        assert_eq!(s.picked, [1], "again, not");
        assert_eq!(s.several(), None, "one alone is the row's own");
        // Shift-click from tab 2 to tab 5: the header between passed over.
        s.unpick();
        s.chosen = Some(RowId::Tab(2));
        s.pick_to(&all, RowId::Tab(5));
        assert_eq!(s.several(), Some(&[2, 3, 4, 5][..]));
        // Back up, from the same place.
        s.pick_to(&all, RowId::Tab(1));
        assert_eq!(s.picked, [1, 2]);
        assert_eq!(between(&all, RowId::Group(one), RowId::Tab(3)), [2, 3]);
        s.unpick();
        assert!(s.picked.is_empty() && s.anchor.is_none());
    }

    #[test]
    fn the_keys_step_through_the_rows_and_find_their_way_back_when_one_goes() {
        let (mut groups, tabs, one, _) = grouped();
        let panes = [1; 5];
        let all = rows(&listed(&tabs, &panes), &groups, &[]);
        let mut s = Sidebar::default();
        s.step(&all, 1);
        assert_eq!(s.chosen, Some(RowId::Tab(1)), "the first where none is chosen");
        s.step(&all, 3);
        assert_eq!(s.chosen, Some(RowId::Tab(3)));
        s.step(&all, 99);
        assert_eq!(s.chosen, Some(RowId::Tab(5)), "held to the list");
        // Its group folded, tab 3's row goes: its header takes the keys.
        s.chosen = Some(RowId::Tab(3));
        let was = s.chosen_in(&all);
        groups.fold(one, true);
        let folded = rows(&listed(&tabs, &panes), &groups, &[]);
        s.keep_chosen(&folded, was, |id| tabs.iter().find(|t| RowId::Tab(t.0) == id).and_then(|t| t.1));
        assert_eq!(s.chosen, Some(RowId::Group(one)));
        // A tab closed: the row where it was.
        s.chosen = Some(RowId::Tab(99));
        s.keep_chosen(&folded, Some(2), |_| None);
        assert_eq!(s.chosen, Some(folded[2].id));
    }

    #[test]
    fn close_group_asks_first_where_a_tab_is_busy_or_its_every_tab() {
        assert!(group_closes_now(true, false, false));
        assert!(!group_closes_now(false, false, false), "a tab busy");
        assert!(!group_closes_now(true, true, false), "Flow would quit");
        assert!(group_closes_now(false, true, true), "asked once already");
    }

    #[test]
    fn a_pinned_sidebar_leaves_the_terminal_twenty_columns_or_pops_out() {
        // 260 points and its margin at a scale of 1, cells 10 wide.
        assert!(pin_fits(1100.0, 268.0, 10.0, 1.0));
        assert!(pin_fits(496.0, 268.0, 10.0, 1.0), "just 20");
        assert!(!pin_fits(495.0, 268.0, 10.0, 1.0));
        assert!(!pin_fits(980.0, 536.0, 20.0, 2.0), "at twice the scale, twice the room");
    }

    #[test]
    fn it_slides_out_and_comes_at_once_in_efficient_mode() {
        let now = Instant::now();
        let s = Sidebar { open: Some(now), ..Sidebar::default() };
        assert_eq!(s.out(now, false), 0.0);
        assert!(s.sliding(now, false));
        assert_eq!(s.out(now + SLIDE, false), 1.0);
        assert!(!s.sliding(now + SLIDE, false));
        assert_eq!(s.out(now, true), 1.0);
        assert_eq!(Sidebar::default().out(now, false), 0.0);
    }
}
