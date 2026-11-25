//! Block graphics Flow draws itself, as rectangles from the cell's own
//! pixels, rather than taking them from the font: teletext's mosaics as
//! Unicode has them (the sextants, U+1FB00–U+1FB3B, and the separated ones
//! of the Legacy Computing Supplement, U+1CE51–U+1CE8F) and the block
//! elements (U+2580–U+259F but the shades), whose full block and halves
//! complete the sextants. Fonts that have them at all draw them to their own
//! measure, which seldom meets the next cell's, so pictures made of them
//! showed seams and steps; drawn here they join in any font, and the edges
//! they share fall on the same whole pixel.

/// What a block character fills of its cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mosaic {
    /// Of a grid `cols` across and `rows` down, the parts `mask` names (bit 0
    /// the top left one, then across and down). `separated`: each part keeps
    /// a gap on its left and below, as teletext's separated graphics do.
    Grid { cols: u8, rows: u8, mask: u8, separated: bool },
    /// A band across the cell's whole height from `from` to `to` eighths of
    /// its width (`across`), or across its whole width from `from` to `to`
    /// eighths of its height.
    Band { across: bool, from: u8, to: u8 },
}

/// The mosaic `c` draws, if it's a block character Flow draws itself.
pub fn of(c: char) -> Option<Mosaic> {
    let u = c as u32;
    let band = |across, from, to| Some(Mosaic::Band { across, from, to });
    let quadrants = |mask| Some(Mosaic::Grid { cols: 2, rows: 2, mask, separated: false });
    match u {
        // In binary, leaving out the halves and the full block, which
        // Unicode has among the block elements.
        0x1FB00..=0x1FB3B => {
            let mut mask = (u - 0x1FB00 + 1) as u8;
            if mask >= 21 {
                mask += 1;
            }
            if mask >= 42 {
                mask += 1;
            }
            Some(Mosaic::Grid { cols: 2, rows: 3, mask, separated: false })
        }
        0x1CE51..=0x1CE8F => Some(Mosaic::Grid { cols: 2, rows: 3, mask: (u - 0x1CE50) as u8, separated: true }),
        0x2580 => band(false, 0, 4),
        // Lower one to seven eighths.
        0x2581..=0x2587 => band(false, (0x2588 - u) as u8, 8),
        0x2588 => band(true, 0, 8),
        // Left seven eighths down to one.
        0x2589..=0x258F => band(true, 0, (0x2590 - u) as u8),
        0x2590 => band(true, 4, 8),
        0x2594 => band(false, 0, 1),
        0x2595 => band(true, 7, 8),
        0x2596 => quadrants(0b0100),
        0x2597 => quadrants(0b1000),
        0x2598 => quadrants(0b0001),
        0x2599 => quadrants(0b1101),
        0x259A => quadrants(0b1001),
        0x259B => quadrants(0b0111),
        0x259C => quadrants(0b1011),
        0x259D => quadrants(0b0010),
        0x259E => quadrants(0b0110),
        0x259F => quadrants(0b1110),
        _ => None,
    }
}

impl Mosaic {
    /// Hands `f` each rectangle (x, y, w, h) it fills of the cell whose
    /// edges are `cell` (left, top, right, bottom, in pixels: the grid's
    /// own, the very numbers the next cell's begin with). Every edge is
    /// rounded to a whole pixel the same way, from the cell's own position,
    /// its first and last parts' from the cell's very edges, so an edge two
    /// blocks share, in one cell or side by side, is one pixel line, with
    /// no seam or overlap where they meet.
    pub fn each_rect(self, cell: [f32; 4], mut f: impl FnMut([f32; 4])) {
        let [left, top, right, bottom] = cell;
        let h = bottom - top;
        // The way from one edge to the other, the edges themselves at its
        // ends.
        let lerp = |a: f32, b: f32, t: f32| a * (1.0 - t) + b * t;
        let at_x = |n: u32, of: u32| lerp(left, right, n as f32 / of as f32).round();
        let at_y = |n: u32, of: u32| lerp(top, bottom, n as f32 / of as f32).round();
        match self {
            Mosaic::Grid { cols, rows, mask, separated } => {
                let (cols, rows) = (u32::from(cols), u32::from(rows));
                // Teletext's separated graphics: a third of a part's width
                // off its left, and a tenth of the cell's height off its foot.
                let gap_y = if separated { (h / 10.0).round().max(1.0) } else { 0.0 };
                for row in 0..rows {
                    for col in 0..cols {
                        if mask & (1 << (row * cols + col)) == 0 {
                            continue;
                        }
                        let (x0, x1, y0, y1) = (at_x(col, cols), at_x(col + 1, cols), at_y(row, rows), at_y(row + 1, rows));
                        let gap_x = if separated { ((x1 - x0) / 3.0).round().max(1.0) } else { 0.0 };
                        let (x0, y1) = (x0 + gap_x, y1 - gap_y);
                        if x1 > x0 && y1 > y0 {
                            f([x0, y0, x1 - x0, y1 - y0]);
                        }
                    }
                }
            }
            Mosaic::Band { across, from, to } => {
                let (from, to) = (u32::from(from), u32::from(to));
                let (x0, x1, y0, y1) = if across { (at_x(from, 8), at_x(to, 8), at_y(0, 1), at_y(1, 1)) } else { (at_x(0, 1), at_x(1, 1), at_y(from, 8), at_y(to, 8)) };
                if x1 > x0 && y1 > y0 {
                    f([x0, y0, x1 - x0, y1 - y0]);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Which of a cell's six sextants `m` covers, as a mask, read at their
    /// middles in a cell 12 by 18.
    fn sextants_covered(m: Mosaic) -> u8 {
        let mut rects = Vec::new();
        m.each_rect([0.0, 0.0, 12.0, 18.0], |r| rects.push(r));
        let mut mask = 0;
        for k in 0..6 {
            let (px, py) = (3.0 + 6.0 * (k % 2) as f32, 3.0 + 6.0 * (k / 2) as f32);
            if rects.iter().any(|r| px >= r[0] && px < r[0] + r[2] && py >= r[1] && py < r[1] + r[3]) {
                mask |= 1 << k;
            }
        }
        mask
    }

    #[test]
    fn the_sextants_count_in_binary_and_the_halves_and_full_block_complete_them() {
        let grid = |mask| Some(Mosaic::Grid { cols: 2, rows: 3, mask, separated: false });
        assert_eq!(of('\u{1FB00}'), grid(1), "sextant 1, the top left");
        assert_eq!(of('\u{1FB02}'), grid(0b11), "sextants 1 and 2");
        assert_eq!(of('\u{1FB13}'), grid(20), "sextants 3 and 5");
        assert_eq!(of('\u{1FB14}'), grid(22), "past the left half: 2, 3 and 5");
        assert_eq!(of('\u{1FB28}'), grid(43), "past the right half too");
        assert_eq!(of('\u{1FB3B}'), grid(62), "2 to 6");
        // Every one of the 63 shapes, once: 60 sextants, ▌, ▐ and █.
        let mut seen: Vec<u8> = (0x1FB00..=0x1FB3B).chain([0x258C, 0x2590, 0x2588]).map(|u| sextants_covered(of(char::from_u32(u).unwrap()).unwrap())).collect();
        seen.sort();
        assert_eq!(seen, (1..=63).collect::<Vec<u8>>());
        // The separated ones have all 63.
        let separated: Vec<u8> = (0x1CE51..=0x1CE8F).map(|u| sextants_covered(of(char::from_u32(u).unwrap()).unwrap())).collect();
        assert_eq!(separated, (1..=63).collect::<Vec<u8>>());
        assert_eq!((of('A'), of('─'), of('░'), of(' ')), (None, None, None, None));
    }

    #[test]
    fn the_block_elements_fill_their_eighths_halves_and_quadrants() {
        let rects = |c| {
            let mut out = Vec::new();
            of(c).unwrap().each_rect([0.0, 0.0, 8.0, 16.0], |r| out.push(r));
            out
        };
        assert_eq!(rects('▀'), [[0.0, 0.0, 8.0, 8.0]]);
        assert_eq!(rects('▁'), [[0.0, 14.0, 8.0, 2.0]]);
        assert_eq!(rects('▇'), [[0.0, 2.0, 8.0, 14.0]]);
        assert_eq!(rects('▉'), [[0.0, 0.0, 7.0, 16.0]]);
        assert_eq!(rects('▏'), [[0.0, 0.0, 1.0, 16.0]]);
        assert_eq!(rects('▕'), [[7.0, 0.0, 1.0, 16.0]]);
        assert_eq!(rects('▔'), [[0.0, 0.0, 8.0, 2.0]]);
        assert_eq!(rects('▚'), [[0.0, 0.0, 4.0, 8.0], [4.0, 8.0, 4.0, 8.0]]);
        assert_eq!(rects('▟').len(), 3);
    }

    #[test]
    fn blocks_side_by_side_meet_on_one_whole_pixel() {
        // A cell 9.6 pixels wide and 19.4 tall, at a fraction of a pixel,
        // its edges from its column and row as the grid has them.
        let (w, h) = (9.6, 19.4);
        let (x, y) = (|col: f32| 3.3 + col * w, |row: f32| 41.7 + row * h);
        let cell = |col: f32, row: f32| [x(col), y(row), x(col + 1.0), y(row + 1.0)];
        let all = |c: char, at| {
            let mut out = Vec::new();
            of(c).unwrap().each_rect(at, |r| out.push(r));
            out
        };
        for c in ['█', '\u{1FB3B}', '▌', '\u{1FB0F}'] {
            for r in all(c, cell(5.0, 2.0)) {
                assert!(r.iter().all(|v| v.fract() == 0.0), "{c}: {r:?}");
            }
        }
        let (left, right) = (all('█', cell(5.0, 2.0))[0], all('█', cell(6.0, 2.0))[0]);
        assert_eq!(left[0] + left[2], right[0]);
        let below = all('█', cell(5.0, 3.0))[0];
        assert_eq!(left[1] + left[3], below[1]);
        // A half block's edge is the sextants' middle.
        let (half, sextant_one) = (all('▌', cell(5.0, 2.0))[0], all('\u{1FB00}', cell(5.0, 2.0))[0]);
        assert_eq!(half[0] + half[2], sextant_one[0] + sextant_one[2]);
    }

    #[test]
    fn separated_mosaics_keep_a_gap_left_of_and_below_each_part() {
        let mut parts = Vec::new();
        of('\u{1CE8F}').unwrap().each_rect([0.0, 0.0, 12.0, 30.0], |r| parts.push(r));
        assert_eq!(parts, [[2.0, 0.0, 4.0, 7.0], [8.0, 0.0, 4.0, 7.0], [2.0, 10.0, 4.0, 7.0], [8.0, 10.0, 4.0, 7.0], [2.0, 20.0, 4.0, 7.0], [8.0, 20.0, 4.0, 7.0]]);
    }
}
