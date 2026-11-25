//! Combining marks: the letters a terminal puts on the letter before
//! rather than in a cell of their own, and which of them Flow draws there.
//!
//! A shell's line editor measures the line it edits with the system's
//! `wcwidth` and moves the cursor by what it measured, so the terminal has
//! to give each letter as many cells as that does: where they differ, a
//! line long enough to wrap is put back in the wrong place, and Up over a
//! long command recalls it garbled. alacritty_terminal goes by
//! unicode-width, which gives a spacing mark (General_Category Mc, such as
//! Devanagari's vowel sign ा) a cell of its own, as glibc's `wcwidth`
//! does. The Mac's gives it none, so zsh and bash there measure "नमस्ते,
//! मेरा नाम साई है।" three cells shorter than the emulator lays it out. So
//! on the Mac each of those marks goes on the letter before, as the
//! emulator puts a mark of no width ([`join`], from
//! `line_size::Margins`). Elsewhere nothing changes: glibc and
//! unicode-width agree, and ConPTY measures for itself on Windows.
//!
//! [`MAC_SPACING_MARKS`] is that set as macOS 27.2's libc has it: every
//! spacing mark its `wcwidth` gives no width where unicode-width gives
//! one. `macos/tests/widths.rs` holds it to the libc the tests run on. The
//! other letters the two measure apart (the prepended concatenation marks,
//! old Hangul's added jamo, the trigrams) are left as they are: none of
//! them goes on the letter before, and they're rare at a prompt.
//!
//! The marks a cell holds are drawn with its letter, shaped together
//! ([`drawn`], `Atlas::marked`), but for those that only choose how the
//! letter looks (variation selectors) or join and part letters (the zero
//! width joiners), which leave the letter drawn as it was.

use alacritty_terminal::grid::Grid;
use alacritty_terminal::term::cell::{Cell, Flags};

/// The spacing marks (Mc) the Mac's `wcwidth` measures as no width where
/// unicode-width gives them a cell, as ranges of code points, first to last.
pub const MAC_SPACING_MARKS: &[(u32, u32)] = &[
    (0x0903, 0x0903), (0x093B, 0x093B), (0x093E, 0x0940), (0x0949, 0x094C), (0x094E, 0x094F),
    (0x0982, 0x0983), (0x09BF, 0x09C0), (0x09C7, 0x09C8), (0x09CB, 0x09CC), (0x0A03, 0x0A03),
    (0x0A3E, 0x0A40), (0x0A83, 0x0A83), (0x0ABE, 0x0AC0), (0x0AC9, 0x0AC9), (0x0ACB, 0x0ACC),
    (0x0B02, 0x0B03), (0x0B40, 0x0B40), (0x0B47, 0x0B48), (0x0B4B, 0x0B4C), (0x0BBF, 0x0BBF),
    (0x0BC1, 0x0BC2), (0x0BC6, 0x0BC8), (0x0BCA, 0x0BCC), (0x0C01, 0x0C03), (0x0C41, 0x0C44),
    (0x0C82, 0x0C83), (0x0CBE, 0x0CBE), (0x0CC1, 0x0CC1), (0x0CC3, 0x0CC4), (0x0CF3, 0x0CF3),
    (0x0D02, 0x0D03), (0x0D3F, 0x0D40), (0x0D46, 0x0D48), (0x0D4A, 0x0D4C), (0x0D82, 0x0D83),
    (0x0DD0, 0x0DD1), (0x0DD8, 0x0DDE), (0x0DF2, 0x0DF3), (0x0F3E, 0x0F3F), (0x0F7F, 0x0F7F),
    (0x102B, 0x102C), (0x1031, 0x1031), (0x1038, 0x1038), (0x103B, 0x103C), (0x1056, 0x1057),
    (0x1062, 0x1064), (0x1067, 0x106D), (0x1083, 0x1084), (0x1087, 0x108C), (0x108F, 0x108F),
    (0x109A, 0x109C), (0x17B6, 0x17B6), (0x17BE, 0x17C5), (0x17C7, 0x17C8), (0x1923, 0x1926),
    (0x1929, 0x192B), (0x1930, 0x1931), (0x1933, 0x1938), (0x1A19, 0x1A1A), (0x1A55, 0x1A55),
    (0x1A57, 0x1A57), (0x1A61, 0x1A61), (0x1A63, 0x1A64), (0x1A6D, 0x1A72), (0x1B04, 0x1B04),
    (0x1B3E, 0x1B41), (0x1B82, 0x1B82), (0x1BA1, 0x1BA1), (0x1BA6, 0x1BA7), (0x1BE7, 0x1BE7),
    (0x1BEA, 0x1BEC), (0x1BEE, 0x1BEE), (0x1C24, 0x1C2B), (0x1C34, 0x1C35), (0x1CE1, 0x1CE1),
    (0x1CF7, 0x1CF7), (0xA823, 0xA824), (0xA827, 0xA827), (0xA880, 0xA881), (0xA8B4, 0xA8C3),
    (0xA952, 0xA952), (0xA983, 0xA983), (0xA9B4, 0xA9B5), (0xA9BA, 0xA9BB), (0xA9BE, 0xA9BF),
    (0xAA2F, 0xAA30), (0xAA33, 0xAA34), (0xAA4D, 0xAA4D), (0xAA7B, 0xAA7B), (0xAA7D, 0xAA7D),
    (0xAAEB, 0xAAEB), (0xAAEE, 0xAAEF), (0xAAF5, 0xAAF5), (0xABE3, 0xABE4), (0xABE6, 0xABE7),
    (0xABE9, 0xABEA), (0xABEC, 0xABEC), (0x11000, 0x11000), (0x11002, 0x11002), (0x11082, 0x11082),
    (0x110B0, 0x110B2), (0x110B7, 0x110B8), (0x1112C, 0x1112C), (0x11145, 0x11146),
    (0x11182, 0x11182), (0x111B3, 0x111B5), (0x111BF, 0x111BF), (0x111CE, 0x111CE),
    (0x1122C, 0x1122E), (0x11232, 0x11233), (0x112E0, 0x112E2), (0x11302, 0x11303),
    (0x1133F, 0x1133F), (0x11341, 0x11344), (0x11347, 0x11348), (0x1134B, 0x1134C),
    (0x11362, 0x11363), (0x11435, 0x11437), (0x11440, 0x11441), (0x11445, 0x11445),
    (0x114B1, 0x114B2), (0x114B9, 0x114B9), (0x114BB, 0x114BC), (0x114BE, 0x114BE),
    (0x114C1, 0x114C1), (0x115B0, 0x115B1), (0x115B8, 0x115BB), (0x115BE, 0x115BE),
    (0x11630, 0x11632), (0x1163B, 0x1163C), (0x1163E, 0x1163E), (0x116AC, 0x116AC),
    (0x116AE, 0x116AF), (0x1171E, 0x1171E), (0x11720, 0x11721), (0x11726, 0x11726),
    (0x1182C, 0x1182E), (0x11838, 0x11838), (0x11931, 0x11935), (0x11937, 0x11938),
    (0x11940, 0x11940), (0x11942, 0x11942), (0x119D1, 0x119D3), (0x119DC, 0x119DF),
    (0x119E4, 0x119E4), (0x11A39, 0x11A39), (0x11A57, 0x11A58), (0x11A97, 0x11A97),
    (0x11C2F, 0x11C2F), (0x11C3E, 0x11C3E), (0x11CA9, 0x11CA9), (0x11CB1, 0x11CB1),
    (0x11CB4, 0x11CB4), (0x11D8A, 0x11D8E), (0x11D93, 0x11D94), (0x11D96, 0x11D96),
    (0x11EF5, 0x11EF6), (0x11F03, 0x11F03), (0x11F34, 0x11F35), (0x11F3E, 0x11F3F),
    (0x16F51, 0x16F87),
];

/// Whether `c` goes on the letter before, as a mark of no width, where the
/// emulator would give it a cell: on the Mac, a spacing mark its shells
/// measure as none.
pub fn joins_letter_before(c: char) -> bool {
    cfg!(target_os = "macos") && is_mac_spacing_mark(c)
}

/// Whether `c` is one of [`MAC_SPACING_MARKS`].
pub fn is_mac_spacing_mark(c: char) -> bool {
    let c = c as u32;
    // Most text is below the first, Devanagari's sign visarga.
    c >= MAC_SPACING_MARKS[0].0
        && MAC_SPACING_MARKS
            .binary_search_by(|&(first, last)| {
                if last < c {
                    std::cmp::Ordering::Less
                } else if first > c {
                    std::cmp::Ordering::Greater
                } else {
                    std::cmp::Ordering::Equal
                }
            })
            .is_ok()
}

/// Puts `c` on the letter before the cursor, as alacritty_terminal puts a
/// character of no width: on the cursor's own cell where a wrap is due
/// (the letter just written on the last column), on a wide letter's first
/// cell, and on the first column's cell at a line's start.
pub fn join(grid: &mut Grid<Cell>, c: char) {
    let cursor = &grid.cursor;
    let (line, mut column) = (cursor.point.line, cursor.point.column);
    if !cursor.input_needs_wrap {
        column.0 = column.0.saturating_sub(1);
    }
    if grid[line][column].flags.contains(Flags::WIDE_CHAR_SPACER) {
        column.0 = column.0.saturating_sub(1);
    }
    grid[line][column].push_zerowidth(c);
}

/// The marks among `marks` that are drawn with their letter: all but those
/// that choose how it looks (variation selectors) and the invisible ones
/// that join or part letters, which drawn with it would change nothing or
/// turn a text symbol into an emoji squeezed into its cell.
pub fn drawn(marks: &[char]) -> impl Iterator<Item = char> + '_ {
    marks.iter().copied().filter(|&c| {
        !matches!(c as u32,
            0x034F | 0x115F | 0x1160 | 0x180B..=0x180F | 0x200B..=0x200F | 0x202A..=0x202E | 0x2060..=0x206F
            | 0x3164 | 0xFE00..=0xFE0F | 0xFEFF | 0xFFA0 | 0xE0000..=0xE0FFF)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::index::{Column, Line, Point};
    use unicode_width::UnicodeWidthChar;

    #[test]
    fn the_table_is_in_order_and_each_mark_in_it_has_a_cell_of_its_own_to_the_emulator() {
        for pair in MAC_SPACING_MARKS.windows(2) {
            assert!(pair[0].1 + 1 < pair[1].0, "{pair:x?}");
        }
        for &(first, last) in MAC_SPACING_MARKS {
            assert!(first <= last);
            for c in (first..=last).filter_map(char::from_u32) {
                assert_eq!(c.width(), Some(1), "U+{:04X}", c as u32);
            }
        }
    }

    #[test]
    fn devanagaris_vowel_signs_are_spacing_marks_and_its_letters_and_virama_are_not() {
        for c in ['ा', 'ि', 'ी', 'ॉ', 'ः', 'ো', 'ி', 'ಾ'] {
            assert!(is_mac_spacing_mark(c), "{c}");
        }
        for c in ['न', 'म', '्', 'े', 'ै', 'a', '中', ' ', '\u{0300}'] {
            assert!(!is_mac_spacing_mark(c), "{c}");
        }
    }

    #[test]
    fn a_mark_joins_the_letter_before_or_the_one_under_a_wrap_due() {
        let mut grid: Grid<Cell> = Grid::new(2, 4, 0);
        grid[Line(0)][Column(0)].c = 'म';
        grid.cursor.point = Point::new(Line(0), Column(1));
        join(&mut grid, 'ा');
        assert_eq!(grid[Line(0)][Column(0)].zerowidth(), Some(&['ा'][..]));
        // On the last column with a wrap due, the cursor's own cell.
        grid[Line(0)][Column(3)].c = 'र';
        grid.cursor.point = Point::new(Line(0), Column(3));
        grid.cursor.input_needs_wrap = true;
        join(&mut grid, 'ा');
        assert_eq!(grid[Line(0)][Column(3)].zerowidth(), Some(&['ा'][..]));
        // After a wide letter, its first cell.
        grid[Line(1)][Column(0)].c = '中';
        grid[Line(1)][Column(0)].flags.insert(Flags::WIDE_CHAR);
        grid[Line(1)][Column(1)].flags.insert(Flags::WIDE_CHAR_SPACER);
        grid.cursor.point = Point::new(Line(1), Column(2));
        grid.cursor.input_needs_wrap = false;
        join(&mut grid, '\u{0301}');
        assert_eq!(grid[Line(1)][Column(0)].zerowidth(), Some(&['\u{0301}'][..]));
    }

    #[test]
    fn variation_selectors_and_joiners_are_not_drawn_but_marks_are() {
        let drawn = |marks: &[char]| drawn(marks).collect::<String>();
        assert_eq!(drawn(&['\u{FE0F}']), "");
        assert_eq!(drawn(&['\u{200D}']), "");
        assert_eq!(drawn(&['\u{0947}', '\u{FE0F}', '\u{093E}']), "\u{0947}\u{093E}");
        assert_eq!(drawn(&['\u{0301}']), "\u{0301}");
    }
}
