//! Box drawing Flow draws itself, from the cell's own pixels, as it draws
//! block elements (`crate::mosaic`): every line of U+2500–U+257F (light,
//! heavy and double, and each junction between them, the dashed ones, the
//! rounded corners, the half lines and the diagonals), and the squares in
//! four sizes, black and white: ■ □, ◼ ◻, ◾ ◽ and ▪ ▫. Most of the themes'
//! fonts have none of the lines, so they came from fallback fonts of other
//! sizes, weights and middles, and a frame's sides and corners didn't meet;
//! a font that has them draws them to its own measure, which may leave a
//! gap between rows. Drawn here, each stroke is placed from the cell's own
//! edges on whole pixels, centered as every cell centers it, so a line
//! meets the next cell's at the same pixels in any font, at any size. A
//! fallback font's square could be wider than the cell, and a black one
//! drawn here beside a white one from a fallback font wouldn't match, so
//! both are drawn, the white one as the black one's outline.
//!
//! A character is a few [`Piece`]s, each a quad and how it covers it: a
//! bar, a quarter of a ring, or a straight line. Their sides that meet
//! another piece's (at the cell's edge, or where a corner's arc meets its
//! straight) are hard, and drawn on the same corners as the piece they
//! meet, so the two join with no seam even as the liquid bends them; their
//! other sides are smoothed over a pixel, which is why such a quad reaches
//! a pixel past its stroke.

use std::ops::{Deref, DerefMut};

/// How heavy one arm of a line is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Weight {
    None,
    Light,
    Heavy,
    Double,
}

/// What a character Flow draws itself is made of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// Arms from the cell's middle to its left, right, top and bottom
    /// edges, in that order.
    Arms([Weight; 4]),
    /// A line of `dashes` dashes across the cell (`across`) or down it.
    Dashed { across: bool, dashes: u8, weight: Weight },
    /// A rounded corner: a quarter arc joining the arms toward `x` (-1 the
    /// left, 1 the right) and toward `y` (-1 the top, 1 the bottom).
    Arc { x: i8, y: i8 },
    /// Lines from corner to corner: the top right to the bottom left
    /// (`rising`), the top left to the bottom right (`falling`), or both.
    Diagonal { rising: bool, falling: bool },
    /// A square in the cell's middle, `tenths` of the font's x-height on a
    /// side: filled, or (not `filled`) its outline, a light stroke thick
    /// (or a third of the side, if that's less).
    Square { tenths: u8, filled: bool },
}

/// What a character's pieces are measured by: the light stroke's
/// thickness ([`stroke`]) and the font's x-height, in a single cell's
/// pixels, and how many times that a double line draws its cells, across
/// and down (1 × 1 on a single line).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pen {
    pub stroke: f32,
    pub x_height: f32,
    pub scale: [f32; 2],
}

/// How a piece covers its quad: what the shader draws (`KIND_STROKE` in
/// `ui.wgsl`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Cover {
    /// All of it but within half a pixel of its soft edges (left, top,
    /// right, bottom), which are smoothed; a hard edge is far out, so the
    /// quad's own side is the edge.
    Bar([f32; 4]),
    /// A ring `radius` around `center`, `half` of its thickness either
    /// side, both in units `squash` pixels across and down (an ellipse on a
    /// double-width line's cells), smoothed over a pixel. The quad holds
    /// the quarter that's drawn.
    Ring { center: [f32; 2], radius: f32, half: f32, squash: [f32; 2] },
    /// A straight line through `from` and `to`, `half` its thickness
    /// either side, cut by the quad.
    Line { from: [f32; 2], to: [f32; 2], half: f32 },
}

/// One quad (x, y, w, h, in pixels) of a character and how it's covered.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Piece {
    pub quad: [f32; 4],
    pub cover: Cover,
}

/// A hard edge's place in [`Cover::Bar`]: far enough out that nothing in
/// the quad comes within a pixel of it.
const FAR: f32 = 1.0e5;

/// The light stroke's thickness for cells `cell_w` wide in a font whose
/// letters' stems are `stem` wide (`CellMetrics::stem`): as heavy as the
/// letters beside it, or an eighth of the cell where that's more, in whole
/// pixels; and no more than a quarter of the cell, so a double line's two
/// strokes keep their gap and room either side.
pub fn stroke(cell_w: f32, stem: f32) -> f32 {
    stem.max(cell_w / 8.0).round().min((cell_w / 4.0).floor()).max(1.0)
}

/// The shape `c` draws, if it's a character Flow draws itself.
pub fn of(c: char) -> Option<Shape> {
    let u = c as u32;
    let dashed = |across, dashes, weight| Some(Shape::Dashed { across, dashes, weight });
    let square = |tenths, filled| Some(Shape::Square { tenths, filled });
    let (light, heavy) = (Weight::Light, Weight::Heavy);
    match u {
        0x2504 => dashed(true, 3, light),
        0x2505 => dashed(true, 3, heavy),
        0x2506 => dashed(false, 3, light),
        0x2507 => dashed(false, 3, heavy),
        0x2508 => dashed(true, 4, light),
        0x2509 => dashed(true, 4, heavy),
        0x250A => dashed(false, 4, light),
        0x250B => dashed(false, 4, heavy),
        0x254C => dashed(true, 2, light),
        0x254D => dashed(true, 2, heavy),
        0x254E => dashed(false, 2, light),
        0x254F => dashed(false, 2, heavy),
        0x256D => Some(Shape::Arc { x: 1, y: 1 }),
        0x256E => Some(Shape::Arc { x: -1, y: 1 }),
        0x256F => Some(Shape::Arc { x: -1, y: -1 }),
        0x2570 => Some(Shape::Arc { x: 1, y: -1 }),
        0x2571 => Some(Shape::Diagonal { rising: true, falling: false }),
        0x2572 => Some(Shape::Diagonal { rising: false, falling: true }),
        0x2573 => Some(Shape::Diagonal { rising: true, falling: true }),
        0x2500..=0x257F => {
            let arms = ARMS[(u - 0x2500) as usize];
            let weight = |shift: u32| match (arms >> shift) & 0xF {
                1 => Weight::Light,
                2 => Weight::Heavy,
                3 => Weight::Double,
                _ => Weight::None,
            };
            Some(Shape::Arms([weight(12), weight(8), weight(4), weight(0)]))
        }
        // The squares, largest first: ■ □, ◼ ◻, ◾ ◽ (each two cells wide,
        // as their emoji are), ▪ ▫.
        0x25A0 => square(10, true),
        0x25A1 => square(10, false),
        0x25FC => square(8, true),
        0x25FB => square(8, false),
        0x25FE => square(7, true),
        0x25FD => square(7, false),
        0x25AA => square(6, true),
        0x25AB => square(6, false),
        _ => None,
    }
}

/// The arms of U+2500–U+257F, a hexadecimal digit each for the left, right,
/// top and bottom: 0 none, 1 light, 2 heavy, 3 double. The dashes, arcs and
/// diagonals (0 here) are [`of`]'s own.
const ARMS: [u16; 128] = [
    // ─ ━ │ ┃, the dashes, ┌ ┍ ┎ ┏
    0x1100, 0x2200, 0x0011, 0x0022, 0, 0, 0, 0, 0, 0, 0, 0, 0x0101, 0x0201, 0x0102, 0x0202,
    // ┐ ┑ ┒ ┓ └ ┕ ┖ ┗ ┘ ┙ ┚ ┛ ├ ┝ ┞ ┟
    0x1001, 0x2001, 0x1002, 0x2002, 0x0110, 0x0210, 0x0120, 0x0220, 0x1010, 0x2010, 0x1020, 0x2020, 0x0111, 0x0211, 0x0121, 0x0112,
    // ┠ ┡ ┢ ┣ ┤ ┥ ┦ ┧ ┨ ┩ ┪ ┫ ┬ ┭ ┮ ┯
    0x0122, 0x0221, 0x0212, 0x0222, 0x1011, 0x2011, 0x1021, 0x1012, 0x1022, 0x2021, 0x2012, 0x2022, 0x1101, 0x2101, 0x1201, 0x2201,
    // ┰ ┱ ┲ ┳ ┴ ┵ ┶ ┷ ┸ ┹ ┺ ┻ ┼ ┽ ┾ ┿
    0x1102, 0x2102, 0x1202, 0x2202, 0x1110, 0x2110, 0x1210, 0x2210, 0x1120, 0x2120, 0x1220, 0x2220, 0x1111, 0x2111, 0x1211, 0x2211,
    // ╀ ╁ ╂ ╃ ╄ ╅ ╆ ╇ ╈ ╉ ╊ ╋, the dashes
    0x1121, 0x1112, 0x1122, 0x2121, 0x1221, 0x2112, 0x1212, 0x2221, 0x2212, 0x2122, 0x1222, 0x2222, 0, 0, 0, 0,
    // ═ ║ ╒ ╓ ╔ ╕ ╖ ╗ ╘ ╙ ╚ ╛ ╜ ╝ ╞ ╟
    0x3300, 0x0033, 0x0301, 0x0103, 0x0303, 0x3001, 0x1003, 0x3003, 0x0310, 0x0130, 0x0330, 0x3010, 0x1030, 0x3030, 0x0311, 0x0133,
    // ╠ ╡ ╢ ╣ ╤ ╥ ╦ ╧ ╨ ╩ ╪ ╫ ╬, the arcs
    0x0333, 0x3011, 0x1033, 0x3033, 0x3301, 0x1103, 0x3303, 0x3310, 0x1130, 0x3330, 0x3311, 0x1133, 0x3333, 0, 0, 0,
    // an arc, the diagonals, ╴ ╵ ╶ ╷ ╸ ╹ ╺ ╻ ╼ ╽ ╾ ╿
    0, 0, 0, 0, 0x1000, 0x0010, 0x0100, 0x0001, 0x2000, 0x0020, 0x0200, 0x0002, 0x1200, 0x0012, 0x2100, 0x0021,
];

/// A few of something, kept in place: a character has at most four bars
/// each way, an arm two strokes and an arc's quarter three cuts across it,
/// so laying one out each frame allocates nothing.
#[derive(Debug, Clone, Copy)]
struct Few<T, const N: usize> {
    items: [T; N],
    len: usize,
}

impl<T: Copy + Default, const N: usize> Few<T, N> {
    fn new() -> Self {
        Self { items: [T::default(); N], len: 0 }
    }

    fn push(&mut self, item: T) {
        self.items[self.len] = item;
        self.len += 1;
    }

    fn remove(&mut self, i: usize) {
        self.items.copy_within(i + 1..self.len, i);
        self.len -= 1;
    }
}

impl<T, const N: usize> Deref for Few<T, N> {
    type Target = [T];

    fn deref(&self) -> &[T] {
        &self.items[..self.len]
    }
}

impl<T, const N: usize> DerefMut for Few<T, N> {
    fn deref_mut(&mut self) -> &mut [T] {
        &mut self.items[..self.len]
    }
}

/// The point `t` of the way from `a` to `b`: `a` itself at 0 and `b` itself
/// at 1, so a cell's first and last parts end on its very edges.
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a * (1.0 - t) + b * t
}

/// Where a stroke `thick` across sits in a span `len` long from `from`:
/// in its middle, on whole pixels. Every cell of a row (or a column) finds
/// the same, so their strokes meet.
fn band(from: f32, len: f32, thick: f32) -> (f32, f32) {
    let a = (from + (len - thick) / 2.0).round();
    (a, a + thick)
}

/// The strokes of an arm of `weight` across a span (`from`, `len`) whose
/// light stroke is `light` thick: one, or a double's two, a light stroke's
/// width apart. None draws as light, for an arm's end where nothing
/// crosses it.
fn strokes(from: f32, len: f32, light: f32, weight: Weight) -> Few<(f32, f32), 2> {
    let mut out = Few::new();
    match weight {
        Weight::None | Weight::Light => out.push(band(from, len, light)),
        Weight::Heavy => out.push(band(from, len, light * 2.0)),
        Weight::Double => {
            let (a, _) = band(from, len, light * 3.0);
            out.push((a, a + light));
            out.push((a + light * 2.0, a + light * 3.0));
        }
    }
    out
}

/// A bar over x0..x1, y0..y1, smoothed on each side but those `hard` names
/// (left, top, right, bottom), where its quad stops at the bar.
fn bar([x0, y0, x1, y1]: [f32; 4], hard: [bool; 4]) -> Piece {
    let out = |hard: bool| if hard { 0.0 } else { 1.0 };
    let (qx0, qy0, qx1, qy1) = (x0 - out(hard[0]), y0 - out(hard[1]), x1 + out(hard[2]), y1 + out(hard[3]));
    let edge = |hard: bool, at: f32, far: f32| if hard { far } else { at };
    Piece {
        quad: [qx0, qy0, qx1 - qx0, qy1 - qy0],
        cover: Cover::Bar([edge(hard[0], x0, -FAR), edge(hard[1], y0, -FAR), edge(hard[2], x1, FAR), edge(hard[3], y1, FAR)]),
    }
}

/// Joins bars that run the same way on the same rows (or columns) and
/// meet or overlap, so a line through the cell is one bar, with no
/// smoothed ends in its middle.
fn join(bars: &mut Few<[f32; 4], 4>, across: bool) {
    let (along, side) = if across { (0, 1) } else { (1, 0) };
    let mut i = 0;
    while i < bars.len() {
        let mut j = i + 1;
        while j < bars.len() {
            let (a, b) = (bars[i], bars[j]);
            let same = a[side] == b[side] && a[side + 2] == b[side + 2];
            if same && a[along] <= b[along + 2] && b[along] <= a[along + 2] {
                bars[i][along] = a[along].min(b[along]);
                bars[i][along + 2] = a[along + 2].max(b[along + 2]);
                bars.remove(j);
                j = i + 1;
            } else {
                j += 1;
            }
        }
        i += 1;
    }
}

impl Shape {
    /// Hands `f` each piece of the shape in the cell whose edges are
    /// `cell` (left, top, right, bottom, in pixels: on a double line, the
    /// whole doubled cell), measured by `pen`. The edges are the grid's
    /// own, the very numbers the next cell's begin with, so the two round
    /// to the same pixel. Every edge of a piece falls on a whole pixel,
    /// rounded from the cell's own position as its neighbours round
    /// theirs, and no quad reaches past the cell's whole pixels.
    pub fn each_piece(self, cell: [f32; 4], pen: Pen, mut f: impl FnMut(Piece)) {
        let [left, top, right, bottom] = cell;
        let (x, y, w, h) = (left, top, right - left, bottom - top);
        let (x0, x1, y0, y1) = (left.round(), right.round(), top.round(), bottom.round());
        let [sx, sy] = pen.scale;
        // The light stroke down (a vertical line's width) and across.
        let (lx, ly) = (pen.stroke * sx, pen.stroke * sy);
        // A side on the cell's edge meets the next cell's piece there.
        let on_edge = |[a, b, c, d]: [f32; 4]| [a <= x0, b <= y0, c >= x1, d >= y1];
        match self {
            Shape::Arms(arms) => {
                let (mut across, mut down) = (Few::new(), Few::new());
                arms_bars(arms, [x0, y0, x1, y1], (x, w, lx), (y, h, ly), &mut across, &mut down);
                join(&mut across, true);
                join(&mut down, false);
                for &b in across.iter().chain(down.iter()) {
                    f(bar(b, on_edge(b)));
                }
            }
            Shape::Dashed { across, dashes, weight } => {
                let n = f32::from(dashes);
                let (from, len, light) = if across { (y, h, ly) } else { (x, w, lx) };
                let (s0, s1) = strokes(from, len, light, weight)[0];
                let (start, end) = if across { (left, right) } else { (top, bottom) };
                let at = |k: f32| lerp(start, end, k / n).round();
                // A third of each dash's share of the cell is gap, split
                // between its two ends, so the gaps are even cell to cell.
                let gap = ((end - start) / n / 3.0).round().max(1.0);
                let (before, after) = ((gap / 2.0).floor(), (gap / 2.0).ceil());
                for k in 0..dashes {
                    let (a, b) = (at(f32::from(k)) + before, at(f32::from(k) + 1.0) - after);
                    if b > a {
                        let r = if across { [a, s0, b, s1] } else { [s0, a, s1, b] };
                        f(bar(r, on_edge(r)));
                    }
                }
            }
            Shape::Arc { x: dx, y: dy } => {
                let (dx, dy) = (f32::from(dx), f32::from(dy));
                let (vx0, vx1) = band(x, w, lx);
                let (hy0, hy1) = band(y, h, ly);
                let (vm, hm) = ((vx0 + vx1) / 2.0, (hy0 + hy1) / 2.0);
                // The edges its arms run to.
                let (ex, ey) = (if dx > 0.0 { x1 } else { x0 }, if dy > 0.0 { y1 } else { y0 });
                // As round as the cell allows (half its width, as a rule),
                // its ends where the arms' strokes would cross, on whole
                // pixels (which a double-width line's ellipse may need
                // pulling in a little for).
                let most = ((ex - vm).abs() / sx).min((ey - hm).abs() / sy);
                let cy = if dy > 0.0 { (hm + most * sy).floor() } else { (hm - most * sy).ceil() };
                let radius = (cy - hm).abs() / sy;
                if radius < 1.0 {
                    // Too small a cell to bend in: a plain corner.
                    let light = Weight::Light;
                    let arms = [dx < 0.0, dx > 0.0, dy < 0.0, dy > 0.0].map(|on| if on { light } else { Weight::None });
                    return Shape::Arms(arms).each_piece(cell, pen, f);
                }
                let cx = vm + dx * radius * sx;
                let span = |a: f32, b: f32| (a.min(b), a.max(b));
                // What's left of each arm past the arc, hard at both ends:
                // the cell's edge and the arc's.
                if cx != ex {
                    let (a, b) = span(cx, ex);
                    f(bar([a, hy0, b, hy1], [true, false, true, false]));
                }
                if cy != ey {
                    let (a, b) = span(cy, ey);
                    f(bar([vx0, a, vx1, b], [false, true, false, true]));
                }
                // The arc's quarter of the cell, from a pixel out past its
                // outer side (to smooth it) to its center's lines, in up
                // to four quads cut where the straights' quads end, so each
                // side it shares with one is the same side.
                let (outer_x, cut_x) = if dx > 0.0 { (vx0 - 1.0, vx1 + 1.0) } else { (vx1 + 1.0, vx0 - 1.0) };
                let (outer_y, cut_y) = if dy > 0.0 { (hy0 - 1.0, hy1 + 1.0) } else { (hy1 + 1.0, hy0 - 1.0) };
                let cuts = |outer: f32, cut: f32, center: f32| {
                    let (lo, hi) = span(outer, center);
                    let mut at = Few::<f32, 3>::new();
                    for v in [lo, cut.clamp(lo, hi), hi] {
                        if at.last() != Some(&v) {
                            at.push(v);
                        }
                    }
                    at
                };
                let (xs, ys) = (cuts(outer_x, cut_x, cx), cuts(outer_y, cut_y, cy));
                let ring = Cover::Ring { center: [cx, cy], radius, half: pen.stroke / 2.0, squash: pen.scale };
                for across in xs.windows(2) {
                    for down in ys.windows(2) {
                        f(Piece { quad: [across[0], down[0], across[1] - across[0], down[1] - down[0]], cover: ring });
                    }
                }
            }
            Shape::Diagonal { rising, falling } => {
                let quad = [x0, y0, x1 - x0, y1 - y0];
                let half = pen.stroke * sx.min(sy) / 2.0;
                if rising {
                    f(Piece { quad, cover: Cover::Line { from: [x1, y0], to: [x0, y1], half } });
                }
                if falling {
                    f(Piece { quad, cover: Cover::Line { from: [x0, y0], to: [x1, y1], half } });
                }
            }
            Shape::Square { tenths, filled } => {
                // Square, and never touching the next cell's. The room is
                // the cell's own size, not its whole pixels, which take
                // turns a pixel apart where cells are a fraction of a
                // pixel wide, so every square in a row is the same size.
                let room = (w / sx).min(h / sy).floor() - 2.0;
                // The smaller squares are shares of the black one as drawn,
                // so where the cell cuts it short they shrink with it.
                let side = (pen.x_height.min(room) * f32::from(tenths) / 10.0).round();
                if side < 1.0 {
                    return;
                }
                let (across, down) = (side * sx, side * sy);
                let (l, t) = ((x + (w - across) / 2.0).round(), (y + (h - down) / 2.0).round());
                let (r, b) = (l + across, t + down);
                // An outline a light stroke thick, but no more than a third
                // of the side, so a small one's hole is never narrower than
                // its stroke and it doesn't read as black.
                let thick = pen.stroke.min((side / 3.0).floor()).max(1.0);
                let (tx, ty) = (thick * sx, thick * sy);
                if filled || across <= 2.0 * tx || down <= 2.0 * ty {
                    f(bar([l, t, r, b], [false; 4]));
                } else {
                    // The black square's outline: its sides whole, and its
                    // top and bottom between them, meeting them hard.
                    f(bar([l, t, l + tx, b], [false; 4]));
                    f(bar([r - tx, t, r, b], [false; 4]));
                    f(bar([l + tx, t, r - tx, t + ty], [true, false, true, false]));
                    f(bar([l + tx, b - ty, r - tx, b], [true, false, true, false]));
                }
            }
        }
    }
}

/// The bars of a character of `arms` (left, right, up, down) in the cell
/// whose whole-pixel edges are `edges` (x0, y0, x1, y1), whose columns
/// start at `x` and are `w` wide with light strokes `lx` thick down (`cols`)
/// and whose rows are the same with `ly` across (`rows`): those of the arms
/// to its left and right into `across`, those up and down into `down`.
///
/// Each arm runs from its edge into the middle. A light or heavy one goes
/// across the strokes it meets, so the junction is filled; where a double
/// line goes straight through and nothing carries on beyond it (╟), it
/// stops at the nearer of its two strokes. A double arm is two strokes, each
/// stopping at the nearer of the strokes it meets if the arm on its side
/// meets them too (an inner corner, as in ╔ or ╬), and going across them
/// all if not (an outer corner, or a double line passing a T).
fn arms_bars(arms: [Weight; 4], [x0, y0, x1, y1]: [f32; 4], cols: (f32, f32, f32), rows: (f32, f32, f32), across: &mut Few<[f32; 4], 4>, down: &mut Few<[f32; 4], 4>) {
    let [l, r, u, d] = arms;
    let none = Weight::None;
    // An axis's strokes at the middle: a double's two if either arm is
    // double (Unicode never mixes double with single on one axis), else
    // the heavier arm's, which holds a lighter one's.
    let middle = |a: Weight, b: Weight, (from, len, light): (f32, f32, f32)| {
        let weight = if a == Weight::Double || b == Weight::Double { Weight::Double } else { a.max(b) };
        (weight == Weight::Double, strokes(from, len, light, weight))
    };
    let (v_double, v) = middle(u, d, cols);
    let (h_double, hz) = middle(l, r, rows);
    let all = |s: &[(f32, f32)]| (s[0].0, s[s.len() - 1].1);
    // Whether an arm's `i`th stroke stops at the near stroke it meets, from
    // the arms it meets (`toward`: for a double's strokes, the one on each
    // one's side), the arm opposite it, and whether those it meets are double.
    let short = |weight: Weight, i: usize, toward: [Weight; 2], opposite: Weight, meets_double: bool| {
        if weight == Weight::Double {
            toward[i] != none
        } else {
            meets_double && toward[0] != none && toward[1] != none && opposite == none
        }
    };
    for (left, weight, opposite) in [(true, l, r), (false, r, l)] {
        if weight == none {
            continue;
        }
        let near = if left { v[0] } else { v[v.len() - 1] };
        for (i, &(s0, s1)) in strokes(rows.0, rows.1, rows.2, weight).iter().enumerate() {
            let (a, b) = if short(weight, i, [u, d], opposite, v_double) { near } else { all(&v[..]) };
            across.push(if left { [x0, s0, b, s1] } else { [a, s0, x1, s1] });
        }
    }
    for (up, weight, opposite) in [(true, u, d), (false, d, u)] {
        if weight == none {
            continue;
        }
        let near = if up { hz[0] } else { hz[hz.len() - 1] };
        for (i, &(s0, s1)) in strokes(cols.0, cols.1, cols.2, weight).iter().enumerate() {
            let (a, b) = if short(weight, i, [l, r], opposite, h_double) { near } else { all(&hz[..]) };
            down.push(if up { [s0, y0, s1, b] } else { [s0, a, s1, y1] });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PEN: Pen = Pen { stroke: 1.0, x_height: 5.0, scale: [1.0, 1.0] };

    fn pieces(c: char, cell: [f32; 4], pen: Pen) -> Vec<Piece> {
        let mut out = Vec::new();
        of(c).unwrap_or_else(|| panic!("{c} isn't drawn")).each_piece(cell, pen, |p| out.push(p));
        out
    }

    /// How much of the pixel whose middle is (px, py) the pieces cover, as
    /// the shader works it out (`KIND_STROKE` in `ui.wgsl`), laid one over
    /// another.
    fn coverage(pieces: &[Piece], px: f32, py: f32) -> f32 {
        let mut alpha = 0.0;
        for p in pieces {
            let [qx, qy, qw, qh] = p.quad;
            if px < qx || px >= qx + qw || py < qy || py >= qy + qh {
                continue;
            }
            let a = match p.cover {
                Cover::Bar([a, b, c, d]) => {
                    let (ex, ey) = ((px - a).min(c - px), (py - b).min(d - py));
                    (0.5 + ex).clamp(0.0, 1.0) * (0.5 + ey).clamp(0.0, 1.0)
                }
                Cover::Ring { center, radius, half, squash } => {
                    let (dx, dy) = ((px - center[0]) / squash[0], (py - center[1]) / squash[1]);
                    let len = dx.hypot(dy).max(1e-4);
                    let k = (dx / len * squash[0]).hypot(dy / len * squash[1]);
                    ((half - (len - radius).abs()) * k + 0.5).clamp(0.0, 1.0)
                }
                Cover::Line { from, to, half } => {
                    let (bx, by) = (to[0] - from[0], to[1] - from[1]);
                    let dist = ((px - from[0]) * by - (py - from[1]) * bx).abs() / bx.hypot(by);
                    (half + 0.5 - dist).clamp(0.0, 1.0)
                }
            };
            alpha = a + alpha * (1.0 - a);
        }
        alpha
    }

    /// The characters drawn in a row of cells 9 by 17 at whole pixels, one
    /// letter a pixel: `#` covered, `+` partly, `.` not.
    fn picture(chars: &str) -> Vec<String> {
        let all: Vec<Piece> = chars.chars().enumerate().flat_map(|(i, c)| pieces(c, [i as f32 * 9.0, 0.0, (i + 1) as f32 * 9.0, 17.0], PEN)).collect();
        (0..17)
            .map(|py| {
                (0..chars.chars().count() * 9)
                    .map(|px| match coverage(&all, px as f32 + 0.5, py as f32 + 0.5) {
                        a if a > 0.9 => '#',
                        a if a > 0.1 => '+',
                        _ => '.',
                    })
                    .collect()
            })
            .collect()
    }

    /// The edges of the cell at `col` and `row` of a grid of cells `w` by
    /// `h` from (3.3, 41.7), worked out from the column and row as Flow's
    /// grid works them out.
    fn grid_cell(w: f32, h: f32, col: f32, row: f32) -> [f32; 4] {
        let (x, y) = (|col: f32| 3.3 + col * w, |row: f32| 41.7 + row * h);
        [x(col), y(row), x(col + 1.0), y(row + 1.0)]
    }

    const SQUARES: [(char, char); 4] = [('■', '□'), ('◼', '◻'), ('◾', '◽'), ('▪', '▫')];

    fn all_chars() -> impl Iterator<Item = char> {
        (0x2500..=0x257F).map(|u| char::from_u32(u).unwrap()).chain(SQUARES.iter().flat_map(|&(black, white)| [black, white]))
    }

    #[test]
    fn every_light_and_heavy_set_of_arms_has_its_character_once_and_double_lines_too() {
        let arms = |c| match of(c) {
            Some(Shape::Arms(a)) => Some(a),
            _ => None,
        };
        let (mut plain, mut double) = (Vec::new(), Vec::new());
        for c in (0x2500..=0x257F).map(|u| char::from_u32(u).unwrap()) {
            if let Some(a) = arms(c) {
                if a.contains(&Weight::Double) { double.push(a) } else { plain.push(a) }
            }
        }
        // Each of the 80 ways of four arms none, light or heavy, but none at all.
        let weights = [Weight::None, Weight::Light, Weight::Heavy];
        let mut every = Vec::new();
        for l in weights {
            for r in weights {
                for u in weights {
                    for d in weights {
                        every.push([l, r, u, d]);
                    }
                }
            }
        }
        every.remove(0);
        plain.sort();
        every.sort();
        assert_eq!(plain, every);
        // Doubles: two arms or more, each axis single or double alike.
        assert_eq!(double.len(), 29);
        for a in &double {
            assert!(a.iter().filter(|&&w| w != Weight::None).count() >= 2, "{a:?}");
            for pair in [[a[0], a[1]], [a[2], a[3]]] {
                assert!(pair[0] == Weight::None || pair[1] == Weight::None || pair[0] == pair[1], "{a:?}");
                assert!(!pair.contains(&Weight::Heavy), "{a:?}");
            }
        }
        double.sort();
        double.dedup();
        assert_eq!(double.len(), 29);
        assert_eq!((of('A'), of('█'), of('▬'), of('·')), (None, None, None, None));
    }

    #[test]
    fn the_lines_and_their_junctions_draw_as_they_should() {
        // ╔'s strokes meet in an outer and an inner corner, and ╤ meets ═
        // on either side. ╤'s single line stops at the near stroke it meets,
        // ╓'s goes on to the far one, and ╟'s stops at the near one, since
        // nothing goes on beyond it. ╭ curves down from the line ─ would
        // draw to the one │ would, ╼ is light then heavy, and ┄'s dashes
        // leave even gaps, cell to cell.
        assert_eq!(
            picture("╔╤╓╟╭╼┄"),
            [
            "..............................#.#..............................",
            "..............................#.#..............................",
            "..............................#.#..............................",
            "..............................#.#..............................",
            "..............................#.#..............................",
            "..............................#.#..............................",
            "..............................#.#..............................",
            "...###############............#.#..............................",
            "...#.................######...#.####......++############.##.##.",
            "...#.#############...#.#......#.#........+++.....#####.........",
            "...#.#.......#.......#.#......#.#.......++.....................",
            "...#.#.......#.......#.#......#.#.......++.....................",
            "...#.#.......#.......#.#......#.#.......#......................",
            "...#.#.......#.......#.#......#.#.......#......................",
            "...#.#.......#.......#.#......#.#.......#......................",
            "...#.#.......#.......#.#......#.#.......#......................",
            "...#.#.......#.......#.#......#.#.......#......................",
            ]
        );
    }

    #[test]
    fn strokes_meet_the_next_cells_at_the_same_pixels() {
        // Cells at a fraction of a pixel, 9.6 by 19.4, two pixels a stroke.
        let (w, h) = (9.6_f32, 19.4_f32);
        let pen = Pen { stroke: 2.0, ..PEN };
        let cell = |col: f32, row: f32| grid_cell(w, h, col, row);
        let (x1, y1) = (cell(0.0, 0.0)[2].round(), cell(0.0, 0.0)[3].round());
        let arm = |c: char, i: usize| match of(c) {
            Some(Shape::Arms(a)) => a[i],
            Some(Shape::Arc { x, y }) if [x < 0, x > 0, y < 0, y > 0][i] => Weight::Light,
            _ => Weight::None,
        };
        // The sides on the edge at `at` (a column's, or a row's if not
        // `across`) of the quads that cover something beside it, at `next`.
        let sides = |all: &[Piece], across: bool, at: f32, next: f32| {
            let mut out: Vec<(f32, f32)> = all
                .iter()
                .filter_map(|p| {
                    let [qx, qy, qw, qh] = p.quad;
                    let (from, to, along, len) = if across { (qx, qx + qw, qy, qh) } else { (qy, qy + qh, qx, qw) };
                    let shows = (0..len as i32).any(|k| {
                        let k = along + k as f32 + 0.5;
                        let (px, py) = if across { (next, k) } else { (k, next) };
                        coverage(&[*p], px, py) > 0.0
                    });
                    ((from == at || to == at) && shows).then_some((along, along + len))
                })
                .collect();
            out.sort_by(|a, b| a.partial_cmp(b).unwrap());
            out
        };
        let mut pairs = 0;
        for a in all_chars() {
            for b in all_chars() {
                // Side by side, a's right arm meeting b's left one: the last
                // column of a's cell and the first of b's are covered alike,
                // by quads with the same sides.
                if arm(a, 1) != Weight::None && arm(a, 1) == arm(b, 0) {
                    let (left, right) = (pieces(a, cell(0.0, 0.0), pen), pieces(b, cell(1.0, 0.0), pen));
                    for py in 30..80 {
                        let py = py as f32 + 0.5;
                        let (here, there) = (coverage(&left, x1 - 0.5, py), coverage(&right, x1 + 0.5, py));
                        assert!((here - there).abs() < 0.05, "{a}{b} at y {py}: {here} {there}");
                    }
                    let (here, there) = (sides(&left, true, x1, x1 - 0.5), sides(&right, true, x1, x1 + 0.5));
                    assert!(!here.is_empty() && here == there, "{a}{b}: {here:?} {there:?}");
                    pairs += 1;
                }
                // One over the other, a's down arm meeting b's up one.
                if arm(a, 3) != Weight::None && arm(a, 3) == arm(b, 2) {
                    let (above, below) = (pieces(a, cell(0.0, 0.0), pen), pieces(b, cell(0.0, 1.0), pen));
                    for px in 0..20 {
                        let px = px as f32 + 0.5;
                        let (here, there) = (coverage(&above, px, y1 - 0.5), coverage(&below, px, y1 + 0.5));
                        assert!((here - there).abs() < 0.05, "{a} over {b} at x {px}: {here} {there}");
                    }
                    let (here, there) = (sides(&above, false, y1, y1 - 0.5), sides(&below, false, y1, y1 + 0.5));
                    assert!(!here.is_empty() && here == there, "{a} over {b}: {here:?} {there:?}");
                    pairs += 1;
                }
            }
        }
        assert!(pairs > 4000, "{pairs}");
    }

    #[test]
    fn an_arc_meets_its_straights_and_the_next_cells() {
        let (w, h) = (9.6_f32, 19.4_f32);
        for (stroke, scale) in [(1.0, [1.0, 1.0]), (2.0, [1.0, 1.0]), (2.0, [2.0, 1.0]), (1.0, [2.0, 2.0])] {
            let pen = Pen { stroke, scale, ..PEN };
            let (w, h) = (w * scale[0], h * scale[1]);
            let cell = |col: f32, row: f32| grid_cell(w, h, col, row);
            // ╭─ over │: the arc's ends cover just what the straights do.
            let (corner, line, side) = (pieces('╭', cell(0.0, 0.0), pen), pieces('─', cell(1.0, 0.0), pen), pieces('│', cell(0.0, 1.0), pen));
            let (x1, y1) = (cell(0.0, 0.0)[2].round(), cell(0.0, 0.0)[3].round());
            for py in 30..120 {
                let py = py as f32 + 0.5;
                let (inside, outside) = (coverage(&corner, x1 - 0.5, py), coverage(&line, x1 + 0.5, py));
                assert!((inside - outside).abs() < 0.05, "{stroke} {scale:?} at y {py}: {inside} {outside}");
            }
            for px in 0..60 {
                let px = px as f32 + 0.5;
                let (inside, outside) = (coverage(&corner, px, y1 - 0.5), coverage(&side, px, y1 + 0.5));
                assert!((inside - outside).abs() < 0.05, "{stroke} {scale:?} at x {px}: {inside} {outside}");
            }
            // And nothing breaks along the arc: every pixel of the stroke
            // between its ends is covered somewhere across it.
            let thick = stroke * scale[0].min(scale[1]);
            let (vx, hy) = (band(3.3, w, stroke * scale[0]), band(41.7, h, stroke * scale[1]));
            for py in (hy.0 as i32)..(y1 as i32) {
                let most = (vx.0 as i32..x1 as i32).map(|px| coverage(&corner, px as f32 + 0.5, py as f32 + 0.5)).fold(0.0, f32::max);
                assert!(most > 0.6, "{stroke} {scale:?} row {py}: {most} (thick {thick})");
            }
        }
    }

    #[test]
    fn nothing_spills_past_the_cell_and_every_edge_is_a_whole_pixel() {
        for (stroke, scale) in [(1.0, [1.0, 1.0]), (2.0, [1.0, 1.0]), (2.0, [2.0, 1.0]), (2.0, [2.0, 2.0])] {
            for (w, h) in [(7.0_f32, 15.0_f32), (9.6, 19.4), (15.0, 37.0), (17.8, 37.0)] {
                let (w, h) = (w * scale[0], h * scale[1]);
                let at = grid_cell(w, h, 5.0, 2.0);
                let [x0, y0, x1, y1] = at.map(f32::round);
                let pen = Pen { stroke, x_height: (w / scale[0] * 0.55).round(), scale };
                for c in all_chars() {
                    let all = pieces(c, at, pen);
                    assert!(!all.is_empty(), "{c}");
                    for p in all {
                        let [qx, qy, qw, qh] = p.quad;
                        assert!(qx >= x0 && qy >= y0 && qx + qw <= x1 && qy + qh <= y1, "{c} at {w}x{h}: {p:?} outside {x0} {y0} {x1} {y1}");
                        assert!(qw > 0.0 && qh > 0.0, "{c}: {p:?}");
                        let whole = |v: &f32| v.fract() == 0.0;
                        assert!(p.quad.iter().all(whole), "{c}: {p:?}");
                        if let Cover::Bar(e) = p.cover {
                            assert!(e.iter().all(whole), "{c}: {p:?}");
                        }
                        if let Cover::Line { .. } = p.cover {
                            assert_eq!(p.quad, [x0, y0, x1 - x0, y1 - y0]);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn heavy_strokes_hold_the_light_ones_and_doubles_keep_a_light_gap() {
        for (from, len) in [(0.0_f32, 17.0_f32), (41.7, 19.4), (3.3, 9.6)] {
            for light in [1.0, 2.0, 3.0] {
                let l = strokes(from, len, light, Weight::Light)[0];
                let h = strokes(from, len, light, Weight::Heavy)[0];
                assert!(h.0 <= l.0 && h.1 >= l.1, "{l:?} {h:?}");
                assert_eq!(h.1 - h.0, 2.0 * light);
                let d = strokes(from, len, light, Weight::Double);
                assert_eq!((d[0].1 - d[0].0, d[1].0 - d[0].1, d[1].1 - d[1].0), (light, light, light));
                // In the middle, within a pixel.
                let middle = from + len / 2.0;
                assert!(((d[0].0 + d[1].1) / 2.0 - middle).abs() <= 0.5 && ((l.0 + l.1) / 2.0 - middle).abs() <= 0.5);
            }
        }
    }

    #[test]
    fn the_light_stroke_is_as_heavy_as_the_letters_within_a_quarter_of_the_cell() {
        // An eighth of the cell where the font's stems are thinner.
        assert_eq!((stroke(9.0, 0.0), stroke(18.0, 1.4), stroke(26.5, 2.6)), (1.0, 2.0, 3.0));
        // A stem as thick as the letters' where that's more: VT323 at 22
        // points on a Retina screen, with stems nearly four pixels wide.
        assert_eq!((stroke(17.67, 3.6), stroke(24.0, 4.0)), (4.0, 4.0));
        // But never more than a quarter of the cell, nor less than a pixel.
        assert_eq!((stroke(17.67, 7.0), stroke(6.0, 2.0), stroke(3.0, 0.0)), (4.0, 1.0, 1.0));
    }

    /// The box the pieces cover (left, top, right, bottom), at whole pixels.
    fn ink_box(all: &[Piece]) -> [f32; 4] {
        let edges = |p: &Piece| match p.cover {
            Cover::Bar(e) => e.map(|v| v.clamp(-1.0e4, 1.0e4)),
            _ => panic!("{p:?}"),
        };
        let quads = |p: &Piece| [p.quad[0], p.quad[1], p.quad[0] + p.quad[2], p.quad[1] + p.quad[3]];
        // A hard edge is the quad's side.
        let sides = |p: &Piece| {
            let (e, q) = (edges(p), quads(p));
            [e[0].max(q[0]), e[1].max(q[1]), e[2].min(q[2]), e[3].min(q[3])]
        };
        all.iter().map(sides).fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |a, b| [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])])
    }

    #[test]
    fn the_squares_sit_in_the_middle_of_the_cell_as_big_as_the_x_height() {
        let pen = Pen { stroke: 2.0, x_height: 11.0, scale: [1.0, 1.0] };
        assert_eq!(ink_box(&pieces('■', [10.0, 20.0, 25.0, 57.0], pen)), [12.0, 33.0, 23.0, 44.0]);
        let small = ink_box(&pieces('▪', [10.0, 20.0, 25.0, 57.0], pen));
        assert_eq!((small[2] - small[0], small[3] - small[1]), (7.0, 7.0));
        // Never as wide as the cell, and still square.
        assert_eq!(ink_box(&pieces('■', [0.0, 0.0, 8.0, 16.0], Pen { stroke: 1.0, x_height: 9.0, scale: [1.0, 1.0] })), [1.0, 5.0, 7.0, 11.0]);
        // Twice as wide on a double-width line.
        let wide = ink_box(&pieces('■', [0.0, 0.0, 30.0, 37.0], Pen { scale: [2.0, 1.0], ..pen }));
        assert_eq!((wide[2] - wide[0], wide[3] - wide[1]), (22.0, 11.0));
        // Smaller from ■ to ▪, each size's white square as big as its black one.
        let sides: Vec<f32> = SQUARES
            .iter()
            .map(|&(black, white)| {
                let (b, w) = (ink_box(&pieces(black, [0.0, 0.0, 18.0, 37.0], Pen { x_height: 16.0, ..pen })), ink_box(&pieces(white, [0.0, 0.0, 18.0, 37.0], Pen { x_height: 16.0, ..pen })));
                assert_eq!(b, w, "{black}{white}");
                assert_eq!(b[2] - b[0], b[3] - b[1], "{black}");
                b[2] - b[0]
            })
            .collect();
        assert_eq!(sides, [16.0, 13.0, 11.0, 10.0]);
    }

    #[test]
    fn every_square_in_a_row_of_cells_a_fraction_of_a_pixel_wide_is_the_same_size() {
        // Cells 17.67 pixels wide, so their whole pixels take turns at 17
        // and 18, with an x-height of 16 that only the wider ones have
        // room for.
        let pen = Pen { stroke: 2.0, x_height: 16.0, scale: [1.0, 1.0] };
        for c in ['■', '□', '◼', '▪'] {
            let boxes: Vec<[f32; 4]> = (0..12).map(|col| ink_box(&pieces(c, grid_cell(17.67, 37.0, col as f32, 1.0), pen))).collect();
            let sides: Vec<(f32, f32)> = boxes.iter().map(|b| (b[2] - b[0], b[3] - b[1])).collect();
            assert!(sides.iter().all(|&(w, h)| w == h && (w, h) == sides[0]), "{c}: {sides:?}");
            // And as far from each cell's left edge, give or take the pixel
            // the cells' own edges round by.
            for (col, b) in boxes.iter().enumerate() {
                let left = grid_cell(17.67, 37.0, col as f32, 1.0)[0];
                assert!((b[0] - left - (17.67 - sides[0].0) / 2.0).abs() <= 0.5, "{c} in column {col}: {b:?}");
            }
        }
    }

    #[test]
    fn a_white_square_is_its_black_ones_outline_a_light_stroke_thick() {
        let pen = Pen { stroke: 2.0, x_height: 16.0, scale: [1.0, 1.0] };
        let cell = grid_cell(17.67, 37.0, 3.0, 1.0);
        for (black, white) in SQUARES {
            let (b, w) = (pieces(black, cell, pen), pieces(white, cell, pen));
            let [l, t, r, bottom] = ink_box(&b);
            for py in (t as i32 - 2)..(bottom as i32 + 2) {
                for px in (l as i32 - 2)..(r as i32 + 2) {
                    let (fx, fy) = (px as f32 + 0.5, py as f32 + 0.5);
                    let inside = fx > l && fx < r && fy > t && fy < bottom;
                    let hole = fx > l + 2.0 && fx < r - 2.0 && fy > t + 2.0 && fy < bottom - 2.0;
                    assert_eq!(coverage(&b, fx, fy), if inside { 1.0 } else { 0.0 }, "{black} at {px} {py}");
                    assert_eq!(coverage(&w, fx, fy), if inside && !hole { 1.0 } else { 0.0 }, "{white} at {px} {py}");
                }
            }
        }
        // A small one under a heavy stroke (VT323's ▫ at 15 points on a
        // Retina screen, its stroke 3, in a cell that cuts ■ to 10 pixels
        // and so ▫ to 6) keeps a hole as wide as its outline, which thins
        // to a third of it.
        let small = pieces('▫', grid_cell(12.0, 38.0, 3.0, 1.0), Pen { stroke: 3.0, x_height: 12.0, scale: [1.0, 1.0] });
        let [l, t, r, b] = ink_box(&small);
        assert_eq!(r - l, 6.0);
        let across: String = (l as i32..r as i32).map(|px| if coverage(&small, px as f32 + 0.5, (t + b) / 2.0) > 0.5 { '#' } else { '.' }).collect();
        assert_eq!(across, "##..##");
    }
}
