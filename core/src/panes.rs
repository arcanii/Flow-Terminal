//! A tab's panes: how its terminal is shared out among its shells. A tab
//! starts with one; ⌘D splits the pane in front in two side by side and ⇧⌘D
//! one above the other (Ctrl+Shift+D and Ctrl+Shift+Alt+D elsewhere), each
//! split halving its pane, and closing a pane gives its room back to the
//! one it was split from. The line between two sides can be dragged
//! (`Split::drag`), each keeping room for [`MIN_COLS`] × [`MIN_ROWS`], and a
//! double click puts it back in the middle. Each pane wears a theme of its
//! own (`App`'s `Tab::put_in_front`, `PaneLook`).
//!
//! A [`Split`] is the tree of them: a pane, by its place in the tab's list,
//! or two sides of a split, each a split again. It lays out the panes'
//! rectangles, finds the one beside another (⌥⌘ and the arrows), and goes
//! into `tabs.json` as it is, so a split tab opens again as it was.

use serde::{Deserialize, Serialize};

use crate::ui::Rect;

/// The fewest columns and rows a pane may have: a split that would leave
/// fewer is refused.
pub const MIN_COLS: usize = 20;
pub const MIN_ROWS: usize = 5;

/// A way to look from one pane for another.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    Left,
    Right,
    Up,
    Down,
}

/// How a tab's terminal is shared among its panes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Split {
    /// The pane at this place in the tab's list.
    Pane(usize),
    /// Two sides: `across`, side by side (a split right); else one above
    /// the other (a split down). The first has `share` of the room (half,
    /// until the line between them is dragged).
    Two {
        across: bool,
        #[serde(default = "half")]
        share: f32,
        first: Box<Split>,
        second: Box<Split>,
    },
}

/// Half: where a split's line starts, and where one from before lines
/// could be dragged stays (`tabs.json` must read whole, or every tab in it
/// would be lost).
fn half() -> f32 {
    0.5
}

/// The line between a split's two sides, as `Split::dividers` gives it.
#[derive(Debug, Clone, PartialEq)]
pub struct Divider {
    /// The way down the tree to the split: true for its second side.
    pub path: Vec<bool>,
    /// Side by side, so the line stands upright and moves across.
    pub across: bool,
    /// The gap between the two sides.
    pub line: Rect,
    /// The split's whole room, which the line moves within.
    pub area: Rect,
}

impl Default for Split {
    fn default() -> Self {
        Split::Pane(0)
    }
}

/// `r` cut in two `gap` apart, side by side (`across`) or one above the
/// other, the first taking `share` of the room, on whole pixels.
fn parts(r: Rect, across: bool, share: f32, gap: f32) -> (Rect, Rect) {
    let share = share.clamp(0.0, 1.0);
    if across {
        let w = ((r.w - gap) * share).floor().max(0.0);
        (Rect::new(r.x, r.y, w, r.h), Rect::new(r.x + w + gap, r.y, (r.w - w - gap).max(0.0), r.h))
    } else {
        let h = ((r.h - gap) * share).floor().max(0.0);
        (Rect::new(r.x, r.y, r.w, h), Rect::new(r.x, r.y + h + gap, r.w, (r.h - h - gap).max(0.0)))
    }
}

impl Split {
    /// The panes, by their places, in the order they're laid out: left to
    /// right, top to bottom.
    pub fn panes(&self) -> Vec<usize> {
        match self {
            Split::Pane(i) => vec![*i],
            Split::Two { first, second, .. } => first.panes().into_iter().chain(second.panes()).collect(),
        }
    }

    /// Each pane's rectangle in `area`, the panes `gap` apart.
    pub fn rects(&self, area: Rect, gap: f32) -> Vec<(usize, Rect)> {
        match self {
            Split::Pane(i) => vec![(*i, area)],
            Split::Two { across, share, first, second } => {
                let (a, b) = parts(area, *across, *share, gap);
                first.rects(a, gap).into_iter().chain(second.rects(b, gap)).collect()
            }
        }
    }

    /// Every split's line in `area`, outermost first: where it is, the room
    /// it moves in, and the way to its split.
    pub fn dividers(&self, area: Rect, gap: f32) -> Vec<Divider> {
        let mut out = Vec::new();
        self.each_divider(area, gap, &mut Vec::new(), &mut out);
        out
    }

    fn each_divider(&self, area: Rect, gap: f32, path: &mut Vec<bool>, out: &mut Vec<Divider>) {
        let Split::Two { across, share, first, second } = self else { return };
        let (a, b) = parts(area, *across, *share, gap);
        let line = if *across { Rect::new(a.x + a.w, area.y, b.x - (a.x + a.w), area.h) } else { Rect::new(area.x, a.y + a.h, area.w, b.y - (a.y + a.h)) };
        out.push(Divider { path: path.clone(), across: *across, line, area });
        path.push(false);
        first.each_divider(a, gap, path, out);
        path.pop();
        path.push(true);
        second.each_divider(b, gap, path, out);
        path.pop();
    }

    /// The split at `path`, where there's one.
    fn at_mut(&mut self, path: &[bool]) -> Option<&mut Split> {
        match (path.split_first(), self) {
            (None, split) => Some(split),
            (Some((&second, rest)), Split::Two { first, second: other, .. }) => if second { other.at_mut(rest) } else { first.at_mut(rest) },
            (Some(_), Split::Pane(_)) => None,
        }
    }

    /// The least room it takes, in pixels: each pane [`MIN_COLS`] ×
    /// [`MIN_ROWS`] cells of `cell`, and the gaps between.
    pub fn least(&self, cell: (f32, f32), gap: f32) -> (f32, f32) {
        match self {
            Split::Pane(_) => (MIN_COLS as f32 * cell.0, MIN_ROWS as f32 * cell.1),
            Split::Two { across, first, second, .. } => {
                let (a, b) = (first.least(cell, gap), second.least(cell, gap));
                if *across {
                    (a.0 + gap + b.0, a.1.max(b.1))
                } else {
                    (a.0.max(b.0), a.1 + gap + b.1)
                }
            }
        }
    }

    /// Moves the line of the split at `path`, laid out in `area`, to the
    /// pointer at `at`, held so each side keeps its [`least`](Self::least)
    /// room; where the room holds neither, it stays. True where it moved.
    pub fn drag(&mut self, path: &[bool], at: (f32, f32), area: Rect, cell: (f32, f32), gap: f32) -> bool {
        let Some(Split::Two { across, share, first, second }) = self.at_mut(path) else { return false };
        let (along, start, room) = if *across { (at.0, area.x, area.w - gap) } else { (at.1, area.y, area.h - gap) };
        if room <= 0.0 {
            return false;
        }
        let (a, b) = (first.least(cell, gap), second.least(cell, gap));
        let (least_first, least_second) = if *across { (a.0, b.0) } else { (a.1, b.1) };
        let (lo, hi) = (least_first / room, 1.0 - least_second / room);
        if lo > hi {
            return false;
        }
        let wanted = ((along - start - gap * 0.5) / room).clamp(lo, hi);
        let moved = (wanted - *share).abs() > f32::EPSILON;
        *share = wanted;
        moved
    }

    /// Puts the line of the split at `path` back in the middle.
    pub fn even(&mut self, path: &[bool]) {
        if let Some(Split::Two { share, .. }) = self.at_mut(path) {
            *share = half();
        }
    }

    /// Splits pane `pane` in two, it keeping the first half and pane `new`
    /// taking the second. False where there's no such pane.
    pub fn split(&mut self, pane: usize, new: usize, across: bool) -> bool {
        match self {
            Split::Pane(i) if *i == pane => {
                *self = Split::Two { across, share: half(), first: Box::new(Split::Pane(pane)), second: Box::new(Split::Pane(new)) };
                true
            }
            Split::Pane(_) => false,
            Split::Two { first, second, .. } => first.split(pane, new, across) || second.split(pane, new, across),
        }
    }

    /// Takes pane `pane` away: the other side of its split takes the
    /// split's room, and the panes after it in the tab's list move up one,
    /// as the list does when it's taken out. False for the only pane, or
    /// one there isn't.
    pub fn remove(&mut self, pane: usize) -> bool {
        if !self.take(pane) {
            return false;
        }
        self.renumber(pane);
        true
    }

    fn take(&mut self, pane: usize) -> bool {
        let Split::Two { first, second, .. } = self else { return false };
        if **first == Split::Pane(pane) {
            *self = std::mem::take(second.as_mut());
            true
        } else if **second == Split::Pane(pane) {
            *self = std::mem::take(first.as_mut());
            true
        } else {
            first.take(pane) || second.take(pane)
        }
    }

    fn renumber(&mut self, gone: usize) {
        match self {
            Split::Pane(i) if *i > gone => *i -= 1,
            Split::Pane(_) => {}
            Split::Two { first, second, .. } => {
                first.renumber(gone);
                second.renumber(gone);
            }
        }
    }

    /// The pane beside pane `from` that way, laid out in `area`: the
    /// nearest whose side faces it and that shares some of its height (or
    /// width), the one nearest its middle where several do.
    pub fn beside(&self, from: usize, dir: Dir, area: Rect, gap: f32) -> Option<usize> {
        let rects = self.rects(area, gap);
        let (_, at) = *rects.iter().find(|(i, _)| *i == from)?;
        let middle = (at.x + at.w * 0.5, at.y + at.h * 0.5);
        let overlaps = |a: f32, aw: f32, b: f32, bw: f32| a < b + bw && b < a + aw;
        rects
            .iter()
            .filter(|(i, _)| *i != from)
            .filter_map(|&(i, r)| {
                let (gap_to, beside) = match dir {
                    Dir::Left => (at.x - (r.x + r.w), overlaps(at.y, at.h, r.y, r.h)),
                    Dir::Right => (r.x - (at.x + at.w), overlaps(at.y, at.h, r.y, r.h)),
                    Dir::Up => (at.y - (r.y + r.h), overlaps(at.x, at.w, r.x, r.w)),
                    Dir::Down => (r.y - (at.y + at.h), overlaps(at.x, at.w, r.x, r.w)),
                };
                let off = match dir {
                    Dir::Left | Dir::Right => (r.y + r.h * 0.5 - middle.1).abs(),
                    Dir::Up | Dir::Down => (r.x + r.w * 0.5 - middle.0).abs(),
                };
                (beside && gap_to >= -0.5).then_some((gap_to, off, i))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)))
            .map(|(_, _, i)| i)
    }

    /// Whether it lists each of `count` panes once, as a tab's must: one
    /// read from `tabs.json` that doesn't is left for a single pane.
    pub fn fits(&self, count: usize) -> bool {
        let mut panes = self.panes();
        panes.sort_unstable();
        panes == (0..count).collect::<Vec<_>>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: Rect = Rect { x: 0.0, y: 0.0, w: 1000.0, h: 600.0 };

    #[test]
    fn a_split_halves_its_pane_and_a_second_splits_a_half() {
        let mut s = Split::default();
        assert_eq!(s.rects(AREA, 6.0), [(0, AREA)]);
        assert!(s.split(0, 1, true));
        assert_eq!(s.rects(AREA, 6.0), [(0, Rect::new(0.0, 0.0, 497.0, 600.0)), (1, Rect::new(503.0, 0.0, 497.0, 600.0))]);
        assert!(s.split(1, 2, false), "the right one, one above the other");
        let rects = s.rects(AREA, 6.0);
        assert_eq!(rects[1], (1, Rect::new(503.0, 0.0, 497.0, 297.0)));
        assert_eq!(rects[2], (2, Rect::new(503.0, 303.0, 497.0, 297.0)));
        assert_eq!(s.panes(), [0, 1, 2]);
        assert!(!s.split(7, 3, true), "no such pane");
    }

    #[test]
    fn closing_a_pane_gives_its_room_back_and_the_panes_after_move_up() {
        let mut s = Split::default();
        s.split(0, 1, true);
        s.split(1, 2, false);
        assert!(s.remove(1));
        assert_eq!(s, Split::Two { across: true, share: 0.5, first: Box::new(Split::Pane(0)), second: Box::new(Split::Pane(1)) }, "2 is 1 now, in 1's place");
        assert!(s.remove(0));
        assert_eq!(s, Split::Pane(0));
        assert!(!s.remove(0), "the only pane stays");
        assert!(s.fits(1) && !s.fits(2));
    }

    #[test]
    fn the_pane_beside_is_the_nearest_that_way() {
        let mut s = Split::default();
        s.split(0, 1, true);
        s.split(1, 2, false);
        assert_eq!(s.beside(0, Dir::Right, AREA, 6.0), Some(1), "of two as near, the first");
        assert_eq!(s.beside(2, Dir::Left, AREA, 6.0), Some(0));
        assert_eq!(s.beside(1, Dir::Down, AREA, 6.0), Some(2));
        assert_eq!(s.beside(2, Dir::Up, AREA, 6.0), Some(1));
        assert_eq!(s.beside(0, Dir::Left, AREA, 6.0), None, "nothing past the edge");
        assert_eq!(s.beside(1, Dir::Right, AREA, 6.0), None);
    }

    #[test]
    fn a_split_goes_into_tabs_json_and_back() {
        let mut s = Split::default();
        s.split(0, 1, false);
        s.drag(&[], (0.0, 400.0), AREA, (10.0, 20.0), 6.0);
        let text = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<Split>(&text).unwrap(), s);
        // One written before lines could be dragged reads as halves.
        let old = r#"{"Two":{"across":true,"first":{"Pane":0},"second":{"Pane":1}}}"#;
        assert_eq!(serde_json::from_str::<Split>(old).unwrap().rects(AREA, 6.0)[0].1.w, 497.0);
    }

    #[test]
    fn the_line_between_two_sides_moves_as_far_as_each_keeps_its_least() {
        let mut s = Split::default();
        s.split(0, 1, true);
        s.split(1, 2, false);
        let dividers = s.dividers(AREA, 6.0);
        assert_eq!(dividers.len(), 2);
        assert_eq!((dividers[0].path.clone(), dividers[0].across, dividers[0].line), (vec![], true, Rect::new(497.0, 0.0, 6.0, 600.0)));
        assert_eq!((dividers[1].path.clone(), dividers[1].across), (vec![true], false));
        assert_eq!(dividers[1].area, Rect::new(503.0, 0.0, 497.0, 600.0), "the right half, which the lower line moves in");
        // Dragged to x 700, the left side is 700 wide; far left, it keeps
        // its 20 columns of 10 pixels.
        let cell = (10.0, 20.0);
        assert!(s.drag(&[], (703.0, 300.0), AREA, cell, 6.0));
        assert_eq!(s.rects(AREA, 6.0)[0].1.w, 700.0);
        s.drag(&[], (5.0, 300.0), AREA, cell, 6.0);
        assert_eq!(s.rects(AREA, 6.0)[0].1.w, 200.0);
        s.drag(&[], (5000.0, 300.0), AREA, cell, 6.0);
        let right = s.rects(AREA, 6.0)[1].1.w;
        assert!((200.0..=201.0).contains(&right), "the right keeps its 20 too: {right}");
        // The lower line moves within the right side alone.
        let right = s.dividers(AREA, 6.0)[1].area;
        assert!(s.drag(&[true], (0.0, 103.0), right, cell, 6.0));
        assert_eq!(s.rects(AREA, 6.0)[1].1.h, 100.0, "five rows of 20");
        s.even(&[]);
        assert_eq!(s.rects(AREA, 6.0)[0].1.w, 497.0, "back to halves");
        assert!(!s.drag(&[false, true], (0.0, 0.0), AREA, cell, 6.0), "no split there");
        // Where the room can't hold both sides' least, it stays.
        assert!(!s.drag(&[], (300.0, 0.0), Rect::new(0.0, 0.0, 300.0, 600.0), cell, 6.0));
    }
}
