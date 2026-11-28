//! Groups of tabs (`docs/design/tab-groups.md`): a group has a name, a
//! color and its tabs, which stand together in the tabs' one order
//! (`App::tabs`, the order the title bar, the keys and the palette go
//! by), the groups in the order their tabs come. Tabs in no group (loose
//! tabs) may stand before, between or after the groups, as in a browser.
//!
//! [`TabGroups`] keeps that so however a tab moves: into a group it goes
//! after the group's tabs, out of one just after them, and a group goes
//! once no tab is in it, so there's never one with no tabs. It's apart
//! from `App`, so the rules are tested without a window: they work on any
//! list of tabs that say their group ([`Grouped`]).

/// The colors groups are marked in, handed out in turn, by the names
/// `tabs.json` keeps them by. Each reads on the title bar's dark glass and
/// apart from the others, and from the accent and the alerts' colors.
pub const COLORS: [(&str, [u8; 3]); 8] = [
    ("blue", [0x5B, 0x8D, 0xFF]),
    ("green", [0x5F, 0xD0, 0x7A]),
    ("yellow", [0xF2, 0xCC, 0x4B]),
    ("pink", [0xFF, 0x79, 0xC6]),
    ("teal", [0x2F, 0xC4, 0xB2]),
    ("orange", [0xFF, 0x9A, 0x3C]),
    ("purple", [0xB0, 0x86, 0xFF]),
    ("red", [0xF2, 0x5C, 0x5C]),
];

/// The place in [`COLORS`] of the color named `name`.
pub fn color_named(name: &str) -> Option<usize> {
    COLORS.iter().position(|(n, _)| *n == name)
}

/// A group of tabs.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub id: u64,
    pub name: String,
    /// Its place in [`COLORS`].
    pub color: usize,
    /// Folded shut in the sidebar, its tabs' rows hidden.
    pub folded: bool,
    /// The theme its tabs wear where they have none of their own (a
    /// `looks::ids` id); none, Settings' theme.
    pub theme: Option<String>,
}

impl Group {
    /// Its color, as the title bar marks its tabs.
    pub fn rgb(&self) -> [u8; 3] {
        COLORS[self.color % COLORS.len()].1
    }

    /// Its color's name, as `tabs.json` keeps it.
    pub fn color_name(&self) -> &'static str {
        COLORS[self.color % COLORS.len()].0
    }
}

/// A tab as the rules see it: which group it's in, if any, by the group's
/// id.
pub trait Grouped {
    fn group(&self) -> Option<u64>;
    fn set_group(&mut self, group: Option<u64>);
}

/// Every group, in the order their tabs come.
#[derive(Debug, Default)]
pub struct TabGroups {
    groups: Vec<Group>,
    /// The ids handed out so far.
    ids: u64,
}

impl TabGroups {
    /// Every group, in the order their tabs come.
    pub fn all(&self) -> &[Group] {
        &self.groups
    }

    pub fn get(&self, id: u64) -> Option<&Group> {
        self.groups.iter().find(|g| g.id == id)
    }

    /// The group `tab` is in.
    pub fn of(&self, tab: &impl Grouped) -> Option<&Group> {
        self.get(tab.group()?)
    }

    /// A group of no tabs yet, named `name`, in color `color` (none, the
    /// next in turn). A tab must go into it before the next [`settle`],
    /// which drops it otherwise.
    ///
    /// [`settle`]: TabGroups::settle
    pub fn add(&mut self, name: String, color: Option<usize>) -> u64 {
        self.ids += 1;
        let color = color.filter(|c| *c < COLORS.len()).unwrap_or_else(|| self.next_color());
        self.groups.push(Group { id: self.ids, name, color, folded: false, theme: None });
        self.ids
    }

    /// Group `id` renamed `name`.
    pub fn rename(&mut self, id: u64, name: String) {
        if let Some(g) = self.groups.iter_mut().find(|g| g.id == id) {
            g.name = name;
        }
    }

    /// Group `id` in color `color` (its place in [`COLORS`]).
    pub fn recolor(&mut self, id: u64, color: usize) {
        if let Some(g) = self.groups.iter_mut().find(|g| g.id == id).filter(|_| color < COLORS.len()) {
            g.color = color;
        }
    }

    /// Group `id` wearing theme `theme` for its tabs, or none again.
    pub fn set_theme(&mut self, id: u64, theme: Option<String>) {
        if let Some(g) = self.groups.iter_mut().find(|g| g.id == id) {
            g.theme = theme;
        }
    }

    /// Group `id` folded shut, or open.
    pub fn fold(&mut self, id: u64, folded: bool) {
        if let Some(g) = self.groups.iter_mut().find(|g| g.id == id) {
            g.folded = folded;
        }
    }

    /// Ungroup: every tab of group `id` loose where it stands, and the
    /// group gone.
    pub fn ungroup<T: Grouped>(&mut self, tabs: &mut [T], id: u64) {
        for tab in tabs.iter_mut().filter(|t| t.group() == Some(id)) {
            tab.set_group(None);
        }
        self.settle(tabs);
    }

    /// Moves tab `i` one step up the order (down where `up` is false), as
    /// ⌥↑ and ⌥↓ do in the sidebar, and gives its place now. Past its
    /// neighbour where they're in the same group, or both loose; at its
    /// group's edge it leaves the group, staying where it is; loose beside
    /// a group, it joins it there, at that end. So a tab goes into and out
    /// of each group it passes, one step each, and never splits one.
    pub fn step_tab<T: Grouped>(&mut self, tabs: &mut [T], i: usize, up: bool) -> usize {
        let Some(group) = tabs.get(i).map(Grouped::group) else { return i };
        let Some(j) = (if up { i.checked_sub(1) } else { Some(i + 1).filter(|&j| j < tabs.len()) }) else {
            // At the end of the order: a group's last tab still leaves it.
            if group.is_some() {
                tabs[i].set_group(None);
                self.settle(tabs);
            }
            return i;
        };
        let beside = tabs[j].group();
        match (group, beside) {
            (g, b) if g == b => tabs.swap(i, j),
            (Some(_), _) => tabs[i].set_group(None),
            (None, b) => tabs[i].set_group(b),
        }
        self.settle(tabs);
        if group == beside { j } else { i }
    }

    /// Moves group `id`'s tabs, together, past what's before them (after,
    /// where `up` is false): a loose tab, or another group's tabs.
    pub fn step_group<T: Grouped>(&mut self, tabs: &mut [T], id: u64, up: bool) {
        let Some(start) = tabs.iter().position(|t| t.group() == Some(id)) else { return };
        let Some(end) = run_end(tabs, id) else { return };
        // What's beside it: one tab, or the whole of the group it's in.
        let (from, to) = if up {
            let Some(before) = start.checked_sub(1) else { return };
            match tabs[before].group() {
                Some(other) => (tabs.iter().position(|t| t.group() == Some(other)).unwrap_or(before), start),
                None => (before, start),
            }
        } else {
            let Some(after) = Some(end).filter(|&e| e < tabs.len()) else { return };
            match tabs[after].group() {
                Some(other) => (end, run_end(tabs, other).unwrap_or(after + 1)),
                None => (end, end + 1),
            }
        };
        // Swapping the run and what's beside it is a turn of the two.
        if up {
            tabs[from..end].rotate_left(start - from);
        } else {
            tabs[start..to].rotate_left(from - start);
        }
        self.settle(tabs);
    }

    /// The color a new group takes: the first of [`COLORS`] that the
    /// fewest groups wear, so each goes once before any goes twice, and
    /// one a group gave up comes back first.
    fn next_color(&self) -> usize {
        (0..COLORS.len()).min_by_key(|&c| self.groups.iter().filter(|g| g.color == c).count()).unwrap_or(0)
    }

    /// Puts tab `i` into a new group, named the first of `numbered`'s
    /// names (from 1: "Group 1", "Group 2") that no group has: where it
    /// is, or, in a group already, just after that group's other tabs,
    /// which stay together. The group and the tab's place now.
    pub fn new_group<T: Grouped>(&mut self, tabs: &mut Vec<T>, i: usize, numbered: impl Fn(usize) -> String) -> Option<(u64, usize)> {
        if i >= tabs.len() {
            return None;
        }
        let i = self.leave(tabs, i);
        let name = (1..).map(numbered).find(|n| self.groups.iter().all(|g| g.name != *n)).unwrap_or_default();
        let id = self.add(name, None);
        tabs[i].set_group(Some(id));
        self.settle(tabs);
        Some((id, i))
    }

    /// Moves tab `i` into group `group`, after the group's tabs, and gives
    /// its place now. A tab in that group already, or a group that isn't
    /// there, leaves it where it is.
    pub fn join<T: Grouped>(&mut self, tabs: &mut Vec<T>, i: usize, group: u64) -> usize {
        if tabs.get(i).is_none_or(|t| t.group() == Some(group)) || self.get(group).is_none() {
            return i;
        }
        let Some(end) = run_end(tabs, group) else { return i };
        let mut tab = tabs.remove(i);
        tab.set_group(Some(group));
        // The group's tabs end one place sooner once a tab before them goes.
        let at = if i < end { end - 1 } else { end };
        tabs.insert(at, tab);
        self.settle(tabs);
        at
    }

    /// Takes tab `i` out of its group, putting it just after the group's
    /// other tabs, and gives its place now. A loose tab stays where it is.
    pub fn leave<T: Grouped>(&mut self, tabs: &mut Vec<T>, i: usize) -> usize {
        let Some(group) = tabs.get(i).and_then(Grouped::group) else { return i };
        let Some(end) = run_end(tabs, group) else { return i };
        let mut tab = tabs.remove(i);
        tab.set_group(None);
        // The group's other tabs now end at `end - 1`: it goes there.
        let at = end - 1;
        tabs.insert(at, tab);
        self.settle(tabs);
        at
    }

    /// Puts the tabs `picked` says into a new group, in their order,
    /// gathered after the first of them, which goes where [`new_group`]
    /// puts one: the group and its first tab's place.
    ///
    /// [`new_group`]: TabGroups::new_group
    pub fn group_picked<T: Grouped>(&mut self, tabs: &mut Vec<T>, picked: impl Fn(&T) -> bool, numbered: impl Fn(usize) -> String) -> Option<(u64, usize)> {
        let first = tabs.iter().position(&picked)?;
        let (id, _) = self.new_group(tabs, first, numbered)?;
        self.join_picked(tabs, picked, id);
        let at = tabs.iter().position(|t| t.group() == Some(id))?;
        Some((id, at))
    }

    /// Moves the tabs `picked` says into group `group`, each after its
    /// tabs, in their order. A group that isn't there takes none.
    pub fn join_picked<T: Grouped>(&mut self, tabs: &mut Vec<T>, picked: impl Fn(&T) -> bool, group: u64) {
        let outside = |tabs: &[T]| tabs.iter().filter(|t| picked(t) && t.group() != Some(group)).count();
        // From the front each time: a tab joining moves itself alone, after
        // the group, so the next one out of it is the next in order.
        while let Some(i) = tabs.iter().position(|t| picked(t) && t.group() != Some(group)) {
            let before = outside(tabs);
            self.join(tabs, i, group);
            if outside(tabs) >= before {
                break;
            }
        }
    }

    /// Takes the tabs `picked` says out of their groups, each just after
    /// the rest of its group.
    pub fn leave_picked<T: Grouped>(&mut self, tabs: &mut Vec<T>, picked: impl Fn(&T) -> bool) {
        while let Some(i) = tabs.iter().position(|t| picked(t) && t.group().is_some()) {
            self.leave(tabs, i);
        }
    }

    /// Moves tab `i` to just before tab `before` (the end where none), into
    /// group `group` there or loose, as a drag in the sidebar drops it, and
    /// gives its place now. A move that would part a group's tabs (loose,
    /// or into another group, between two of one group's tabs; into a
    /// group away from all its tabs), or into a group that isn't there,
    /// leaves it where it is.
    pub fn place<T: Grouped>(&mut self, tabs: &mut Vec<T>, i: usize, before: Option<usize>, group: Option<u64>) -> usize {
        if i >= tabs.len() || group.is_some_and(|g| self.get(g).is_none()) {
            return i;
        }
        let mut at = before.unwrap_or(tabs.len()).min(tabs.len());
        // Its place among the others once it's out of theirs.
        if at > i {
            at -= 1;
        }
        let mut tab = tabs.remove(i);
        let prev = at.checked_sub(1).and_then(|k| tabs.get(k)).and_then(Grouped::group);
        let next = tabs.get(at).and_then(Grouped::group);
        let parts = (prev.is_some() && prev == next && prev != group) || group.is_some_and(|g| prev != Some(g) && next != Some(g) && tabs.iter().any(|t| t.group() == Some(g)));
        if parts {
            tabs.insert(i, tab);
            return i;
        }
        tab.set_group(group);
        tabs.insert(at, tab);
        self.settle(tabs);
        at
    }

    /// Moves group `id`'s tabs, together, to just before tab `before` (the
    /// end where none), as a drag of its header in the sidebar drops it.
    /// Into another group's tabs, or within its own, it stays where it is.
    pub fn place_group<T: Grouped>(&mut self, tabs: &mut Vec<T>, id: u64, before: Option<usize>) {
        let Some(start) = tabs.iter().position(|t| t.group() == Some(id)) else { return };
        let Some(end) = run_end(tabs, id) else { return };
        let before = before.unwrap_or(tabs.len()).min(tabs.len());
        if (start..=end).contains(&before) {
            return;
        }
        let run: Vec<T> = tabs.drain(start..end).collect();
        let at = if before > end { before - (end - start) } else { before };
        let prev = at.checked_sub(1).and_then(|k| tabs.get(k)).and_then(Grouped::group);
        let next = tabs.get(at).and_then(Grouped::group);
        let at = if prev.is_some() && prev == next { start } else { at };
        tabs.splice(at..at, run);
        self.settle(tabs);
    }

    /// After tabs closed, opened or moved: a group no tab is in goes, a tab
    /// naming a group that isn't there is loose, and the groups go in the
    /// order their tabs come.
    pub fn settle<T: Grouped>(&mut self, tabs: &mut [T]) {
        for tab in tabs.iter_mut() {
            if tab.group().is_some_and(|g| self.get(g).is_none()) {
                tab.set_group(None);
            }
        }
        let first = |id: u64| tabs.iter().position(|t| t.group() == Some(id));
        self.groups.retain(|g| first(g.id).is_some());
        self.groups.sort_by_key(|g| first(g.id));
    }

    /// Every group gone, as all the tabs close.
    pub fn clear(&mut self) {
        self.groups.clear();
    }
}

/// The place after group `group`'s last tab, where it has any.
fn run_end<T: Grouped>(tabs: &[T], group: u64) -> Option<usize> {
    tabs.iter().rposition(|t| t.group() == Some(group)).map(|i| i + 1)
}

/// Each tab's group as it's read back (from `tabs.json`, which may be
/// written by hand), by the group's place among those kept: a tab whose
/// group's tabs ended before it is loose, so each group's tabs stand
/// together as the rules keep them.
pub fn together(groups: &[Option<usize>]) -> Vec<Option<usize>> {
    let mut ended = Vec::new();
    let mut before = None;
    groups
        .iter()
        .map(|&g| {
            let g = g.filter(|k| !ended.contains(k));
            if before != g {
                ended.extend(before);
            }
            before = g;
            g
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tab, by a letter, and its group.
    #[derive(Debug, Clone, PartialEq)]
    struct T(char, Option<u64>);

    impl Grouped for T {
        fn group(&self) -> Option<u64> {
            self.1
        }
        fn set_group(&mut self, group: Option<u64>) {
            self.1 = group;
        }
    }

    fn tabs(letters: &str) -> Vec<T> {
        letters.chars().map(|c| T(c, None)).collect()
    }

    fn letters(tabs: &[T]) -> String {
        tabs.iter().map(|t| t.0).collect()
    }

    fn numbered(n: usize) -> String {
        format!("Group {n}")
    }

    /// Each group's tabs stand together, the groups in the order their
    /// tabs come, and every group has a tab.
    fn holds(groups: &TabGroups, tabs: &[T]) {
        let mut runs: Vec<u64> = Vec::new();
        for t in tabs {
            if let Some(g) = t.1 {
                if runs.last() != Some(&g) {
                    assert!(!runs.contains(&g), "group {g}'s tabs are apart: {tabs:?}");
                    runs.push(g);
                }
            }
        }
        assert_eq!(groups.all().iter().map(|g| g.id).collect::<Vec<_>>(), runs, "{tabs:?}");
    }

    #[test]
    fn a_new_group_takes_its_tab_where_it_is_with_the_first_free_name() {
        let mut g = TabGroups::default();
        let mut t = tabs("abcd");
        let (one, at) = g.new_group(&mut t, 1, numbered).unwrap();
        assert_eq!((at, letters(&t)), (1, "abcd".into()));
        assert_eq!(t[1].1, Some(one));
        let (two, at) = g.new_group(&mut t, 3, numbered).unwrap();
        assert_eq!(at, 3);
        assert_eq!(g.get(one).unwrap().name, "Group 1");
        assert_eq!(g.get(two).unwrap().name, "Group 2");
        holds(&g, &t);
        // Group 1 gone with its tab, its name is free again.
        t.remove(1);
        g.settle(&mut t);
        let (three, _) = g.new_group(&mut t, 0, numbered).unwrap();
        assert_eq!(g.get(three).unwrap().name, "Group 1");
        holds(&g, &t);
        assert_eq!(g.new_group(&mut t, 9, numbered), None);
    }

    #[test]
    fn a_tab_joining_a_group_goes_after_its_tabs_from_either_side() {
        let mut g = TabGroups::default();
        let mut t = tabs("abcdef");
        let (id, _) = g.new_group(&mut t, 2, numbered).unwrap();
        // From before it: c's group is then b's place on.
        assert_eq!(g.join(&mut t, 0, id), 2);
        assert_eq!(letters(&t), "bcadef");
        // From after it.
        assert_eq!(g.join(&mut t, 4, id), 3);
        assert_eq!(letters(&t), "bcaedf");
        assert_eq!(t.iter().map(|t| t.1.is_some()).collect::<Vec<_>>(), [false, true, true, true, false, false]);
        holds(&g, &t);
        // Already in it, or a group that isn't there: nothing moves.
        assert_eq!(g.join(&mut t, 2, id), 2);
        assert_eq!(g.join(&mut t, 0, 99), 0);
        assert_eq!(letters(&t), "bcaedf");
    }

    #[test]
    fn a_tab_leaving_its_group_goes_just_after_the_rest_of_it() {
        let mut g = TabGroups::default();
        let mut t = tabs("abcde");
        let (id, _) = g.new_group(&mut t, 1, numbered).unwrap();
        g.join(&mut t, 2, id);
        g.join(&mut t, 3, id);
        assert_eq!(letters(&t), "abcde");
        // b, first of b c d, leaves after d.
        assert_eq!(g.leave(&mut t, 1), 3);
        assert_eq!(letters(&t), "acdbe");
        assert_eq!(t[3].1, None);
        holds(&g, &t);
        // A loose tab stays.
        assert_eq!(g.leave(&mut t, 4), 4);
        assert_eq!(letters(&t), "acdbe");
    }

    #[test]
    fn a_group_goes_with_its_last_tab_however_it_leaves() {
        let mut g = TabGroups::default();
        let mut t = tabs("abc");
        // Closed.
        let (one, _) = g.new_group(&mut t, 0, numbered).unwrap();
        t.remove(0);
        g.settle(&mut t);
        assert!(g.get(one).is_none());
        // Taken out.
        let (two, _) = g.new_group(&mut t, 0, numbered).unwrap();
        g.leave(&mut t, 0);
        assert!(g.get(two).is_none());
        // Moved into another group.
        let (three, _) = g.new_group(&mut t, 0, numbered).unwrap();
        let (four, _) = g.new_group(&mut t, 1, numbered).unwrap();
        g.join(&mut t, 0, four);
        assert!(g.get(three).is_none());
        assert_eq!(letters(&t), "cb");
        assert_eq!(t.iter().map(|t| t.1).collect::<Vec<_>>(), [Some(four), Some(four)]);
        holds(&g, &t);
        // Its tab put into a new group of its own, after the rest of four.
        let (five, at) = g.new_group(&mut t, 0, numbered).unwrap();
        assert_eq!((at, letters(&t)), (1, "bc".into()));
        assert_eq!(t[1].1, Some(five));
        holds(&g, &t);
        g.new_group(&mut t, 0, numbered);
        assert!(g.get(four).is_none());
        holds(&g, &t);
    }

    #[test]
    fn colors_go_in_turn_and_one_given_up_comes_back_first() {
        let mut g = TabGroups::default();
        let mut t = tabs("abcdefghij");
        for i in 0..10 {
            g.new_group(&mut t, i, numbered);
        }
        let colors: Vec<usize> = g.all().iter().map(|g| g.color).collect();
        assert_eq!(colors, [0, 1, 2, 3, 4, 5, 6, 7, 0, 1]);
        // Green's group (the second) goes: its color is next.
        t.remove(1);
        g.settle(&mut t);
        let id = g.add("Kept".into(), None);
        assert_eq!(g.get(id).unwrap().color, 1);
        // Asked for by name, as tabs.json keeps it.
        let id = g.add("Kept".into(), color_named("purple"));
        assert_eq!(g.get(id).unwrap().color_name(), "purple");
        assert_eq!(g.get(id).unwrap().rgb(), COLORS[6].1);
        assert_eq!(color_named("mauve"), None);
    }

    #[test]
    fn the_groups_go_in_the_order_their_tabs_come() {
        let mut g = TabGroups::default();
        let mut t = tabs("abcd");
        let (late, _) = g.new_group(&mut t, 3, numbered).unwrap();
        let (early, _) = g.new_group(&mut t, 0, numbered).unwrap();
        assert_eq!(g.all().iter().map(|g| g.id).collect::<Vec<_>>(), [early, late]);
        // A tab naming a group that isn't there is loose.
        t[1].1 = Some(42);
        g.settle(&mut t);
        assert_eq!(t[1].1, None);
        holds(&g, &t);
    }

    #[test]
    fn any_moves_keep_every_groups_tabs_together() {
        // A fixed run of moves, picked by a small generator so it's the
        // same every time.
        let mut seed = 0x2545_F491_u64;
        let mut next = |n: usize| {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 33) as usize % n.max(1)
        };
        let mut g = TabGroups::default();
        let mut t = tabs("abcdefgh");
        let mut letter = b'i';
        for _ in 0..3000 {
            let i = next(t.len());
            match next(9) {
                0 => {
                    g.new_group(&mut t, i, numbered);
                }
                1 => {
                    if let Some(group) = g.all().get(next(g.all().len().max(1))).map(|g| g.id) {
                        g.join(&mut t, i, group);
                    }
                }
                2 => {
                    g.leave(&mut t, i);
                }
                3 if t.len() > 1 => {
                    t.remove(i);
                    g.settle(&mut t);
                }
                4 => {
                    let up = next(2) == 0;
                    g.step_tab(&mut t, i, up);
                }
                5 => {
                    let up = next(2) == 0;
                    if let Some(group) = g.all().get(next(g.all().len().max(1))).map(|g| g.id) {
                        g.step_group(&mut t, group, up);
                    }
                }
                // Dragged anywhere, into any group or none, the refused
                // moves among them.
                6 => {
                    let before = Some(next(t.len() + 1)).filter(|&b| b < t.len());
                    let group = g.all().get(next(g.all().len() + 1)).map(|g| g.id);
                    g.place(&mut t, i, before, group);
                }
                7 => {
                    let before = Some(next(t.len() + 1)).filter(|&b| b < t.len());
                    if let Some(group) = g.all().get(next(g.all().len().max(1))).map(|g| g.id) {
                        g.place_group(&mut t, group, before);
                    }
                }
                _ => {
                    // A new tab at the end, joining the group of the one in
                    // front as ⌘T's does.
                    t.push(T(letter as char, None));
                    letter = if letter == b'z' { b'a' } else { letter + 1 };
                    if let Some(group) = t[i].1 {
                        let last = t.len() - 1;
                        g.join(&mut t, last, group);
                    }
                }
            }
            holds(&g, &t);
        }
    }

    /// Each tab's letter and whether it's in a group, as `aB` writes them:
    /// a capital is in group `id`'s run, by the order of the groups.
    fn shown(groups: &TabGroups, tabs: &[T]) -> String {
        tabs.iter()
            .map(|t| match t.1.and_then(|g| groups.all().iter().position(|x| x.id == g)) {
                Some(k) => format!("{}{}", t.0.to_ascii_uppercase(), k + 1),
                None => t.0.to_string(),
            })
            .collect()
    }

    #[test]
    fn a_tab_stepped_along_goes_into_and_out_of_each_group_it_passes() {
        let mut g = TabGroups::default();
        let mut t = tabs("abcd");
        let (id, _) = g.new_group(&mut t, 1, numbered).unwrap();
        g.join(&mut t, 2, id);
        assert_eq!(shown(&g, &t), "aB1C1d");
        // d, loose, steps up into the group at its end, then along it, then
        // out of it at its start, then past a.
        assert_eq!(g.step_tab(&mut t, 3, true), 3);
        assert_eq!(shown(&g, &t), "aB1C1D1");
        assert_eq!(g.step_tab(&mut t, 3, true), 2);
        assert_eq!(shown(&g, &t), "aB1D1C1");
        assert_eq!(g.step_tab(&mut t, 2, true), 1);
        assert_eq!(shown(&g, &t), "aD1B1C1");
        assert_eq!(g.step_tab(&mut t, 1, true), 1);
        assert_eq!(shown(&g, &t), "adB1C1");
        assert_eq!(g.step_tab(&mut t, 1, true), 0);
        assert_eq!(shown(&g, &t), "daB1C1");
        // At the top it stays; a group's last tab at the end steps out.
        assert_eq!(g.step_tab(&mut t, 0, true), 0);
        assert_eq!(g.step_tab(&mut t, 3, false), 3);
        assert_eq!(shown(&g, &t), "daB1c");
        holds(&g, &t);
        // A group's only tab stepping out takes the group with it.
        g.step_tab(&mut t, 2, true);
        assert_eq!((shown(&g, &t), g.all().len()), ("dabc".into(), 0));
    }

    #[test]
    fn a_group_steps_past_a_loose_tab_or_a_whole_group() {
        let mut g = TabGroups::default();
        let mut t = tabs("abcdef");
        let (one, _) = g.new_group(&mut t, 1, numbered).unwrap();
        g.join(&mut t, 2, one);
        let (two, _) = g.new_group(&mut t, 3, numbered).unwrap();
        g.join(&mut t, 4, two);
        assert_eq!(shown(&g, &t), "aB1C1D2E2f");
        g.step_group(&mut t, two, true);
        assert_eq!(shown(&g, &t), "aD1E1B2C2f", "past the whole of group 1, which is now second");
        g.step_group(&mut t, two, true);
        assert_eq!(shown(&g, &t), "D1E1aB2C2f");
        g.step_group(&mut t, two, true);
        assert_eq!(shown(&g, &t), "D1E1aB2C2f", "at the top it stays");
        g.step_group(&mut t, one, false);
        assert_eq!(shown(&g, &t), "D1E1afB2C2");
        g.step_group(&mut t, one, false);
        assert_eq!(shown(&g, &t), "D1E1afB2C2", "at the end it stays");
        holds(&g, &t);
    }

    #[test]
    fn a_tab_dropped_goes_where_it_lands_but_never_parts_a_group() {
        let mut g = TabGroups::default();
        let mut t = tabs("abcdef");
        let (one, _) = g.new_group(&mut t, 1, numbered).unwrap();
        g.join(&mut t, 2, one);
        g.join(&mut t, 3, one);
        assert_eq!(shown(&g, &t), "aB1C1D1ef");
        // a dropped between c and d, into the group there.
        assert_eq!(g.place(&mut t, 0, Some(3), Some(one)), 2);
        assert_eq!(shown(&g, &t), "B1C1A1D1ef");
        // f before b, the group's first: into it, first.
        assert_eq!(g.place(&mut t, 5, Some(0), Some(one)), 0);
        assert_eq!(shown(&g, &t), "F1B1C1A1D1e");
        // c out, loose after the group, before e; then loose at the end.
        assert_eq!(g.place(&mut t, 2, Some(5), None), 4);
        assert_eq!(shown(&g, &t), "F1B1A1D1ce");
        assert_eq!(g.place(&mut t, 4, None, None), 5);
        assert_eq!(shown(&g, &t), "F1B1A1D1ec");
        // Before itself: where it is, its group as said.
        assert_eq!(g.place(&mut t, 4, Some(4), None), 4);
        assert_eq!(shown(&g, &t), "F1B1A1D1ec");
        holds(&g, &t);
        // Refused: loose between b and a, into the group away from its
        // tabs, into a group that isn't there.
        assert_eq!(g.place(&mut t, 4, Some(2), None), 4);
        assert_eq!(g.place(&mut t, 4, None, Some(one)), 4);
        assert_eq!(g.place(&mut t, 4, Some(0), Some(99)), 4);
        assert_eq!(shown(&g, &t), "F1B1A1D1ec");
        // Into another group between two of the first's: refused too.
        let (two, _) = g.new_group(&mut t, 5, numbered).unwrap();
        assert_eq!(g.place(&mut t, 4, Some(2), Some(two)), 4);
        // A group's last tab dropped into another: the group goes.
        let (three, _) = g.new_group(&mut t, 4, numbered).unwrap();
        g.place(&mut t, 4, Some(5), Some(two));
        assert!(g.get(three).is_none());
        assert_eq!(shown(&g, &t), "F1B1A1D1E2C2");
        holds(&g, &t);
    }

    #[test]
    fn a_group_dropped_goes_whole_but_never_into_another() {
        let mut g = TabGroups::default();
        let mut t = tabs("abcdefg");
        let (one, _) = g.new_group(&mut t, 1, numbered).unwrap();
        g.join(&mut t, 2, one);
        let (two, _) = g.new_group(&mut t, 4, numbered).unwrap();
        g.join(&mut t, 5, two);
        assert_eq!(shown(&g, &t), "aB1C1dE2F2g");
        // Group one to the end, then before a.
        g.place_group(&mut t, one, None);
        assert_eq!(shown(&g, &t), "adE1F1gB2C2");
        g.place_group(&mut t, one, Some(0));
        assert_eq!(shown(&g, &t), "B1C1adE2F2g");
        // Group two between a and d.
        g.place_group(&mut t, two, Some(3));
        assert_eq!(shown(&g, &t), "B1C1aE2F2dg");
        holds(&g, &t);
        // Refused: into group one's tabs, within or beside its own.
        g.place_group(&mut t, two, Some(1));
        g.place_group(&mut t, two, Some(4));
        g.place_group(&mut t, two, Some(5));
        g.place_group(&mut t, two, Some(3));
        assert_eq!(shown(&g, &t), "B1C1aE2F2dg");
        g.place_group(&mut t, 99, Some(0));
        assert_eq!(shown(&g, &t), "B1C1aE2F2dg");
    }

    #[test]
    fn tabs_picked_together_gather_into_a_group_in_their_order() {
        let mut g = TabGroups::default();
        let mut t = tabs("abcdef");
        // b and c in a group already; a, c and e picked.
        let (old, _) = g.new_group(&mut t, 1, numbered).unwrap();
        g.join(&mut t, 2, old);
        let picked = |t: &T| matches!(t.0, 'a' | 'c' | 'e');
        let (id, at) = g.group_picked(&mut t, picked, numbered).unwrap();
        assert_eq!((at, shown(&g, &t)), (0, "A1C1E1B2df".into()));
        assert_eq!(t.iter().filter(|t| t.1 == Some(id)).map(|t| t.0).collect::<String>(), "ace");
        assert_eq!(t[3].1, Some(old), "b stays in its own");
        holds(&g, &t);
        // Moved, and out again.
        let (two, _) = g.new_group(&mut t, 5, numbered).unwrap();
        g.join_picked(&mut t, |t| matches!(t.0, 'a' | 'b'), two);
        assert_eq!(shown(&g, &t), "C1E1dF2A2B2", "a and b after f, in their order, b's own group gone");
        g.leave_picked(&mut t, |t| matches!(t.0, 'f' | 'b'));
        assert_eq!(shown(&g, &t), "C1E1dA2bf");
        holds(&g, &t);
        // Nothing picked, or a group that isn't there: nothing moves.
        assert_eq!(g.group_picked(&mut t, |_| false, numbered), None);
        g.join_picked(&mut t, |t| t.0 == 'f', 99);
        assert_eq!(shown(&g, &t), "C1E1dA2bf");
    }

    #[test]
    fn a_group_folds_renames_recolors_and_ungroups() {
        let mut g = TabGroups::default();
        let mut t = tabs("abc");
        let (id, _) = g.new_group(&mut t, 0, numbered).unwrap();
        g.join(&mut t, 2, id);
        g.fold(id, true);
        g.rename(id, "Web".into());
        g.recolor(id, 5);
        g.recolor(id, 99);
        assert_eq!(g.get(id).map(|g| (g.folded, g.name.as_str(), g.color)), Some((true, "Web", 5)));
        g.set_theme(id, Some("green".into()));
        assert_eq!(g.get(id).and_then(|g| g.theme.as_deref()), Some("green"));
        g.set_theme(id, None);
        assert_eq!(g.get(id).and_then(|g| g.theme.as_deref()), None);
        // Ungrouped, its tabs stay where they are.
        g.ungroup(&mut t, id);
        assert_eq!((letters(&t), t.iter().all(|t| t.1.is_none()), g.all().len()), ("acb".into(), true, 0));
    }

    #[test]
    fn tabs_read_back_keep_each_groups_tabs_together() {
        assert_eq!(together(&[Some(0), Some(0), None, Some(1), Some(1)]), [Some(0), Some(0), None, Some(1), Some(1)]);
        // A group's tabs apart: those after the first run are loose.
        assert_eq!(together(&[Some(0), None, Some(0)]), [Some(0), None, None]);
        assert_eq!(together(&[Some(0), Some(1), Some(0), Some(1)]), [Some(0), Some(1), None, None]);
        assert_eq!(together(&[]), Vec::<Option<usize>>::new());
    }
}
