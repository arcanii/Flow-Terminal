//! Terminal color palette tuned for a dark translucent glass background.

use alacritty_terminal::term::color::Colors;
use alacritty_terminal::vte::ansi::{Color, NamedColor};

pub type Rgb8 = [u8; 3];

pub const FOREGROUND: Rgb8 = [0xE4, 0xE9, 0xF2];
/// Also the glass's tint.
pub const BACKGROUND: Rgb8 = [0x08, 0x0C, 0x16];
pub const CURSOR: Rgb8 = [0x8F, 0xE3, 0xFF];

/// The terminal's text, cursor and background colors, which the user can
/// pick. The background is the glass's tint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ink {
    pub text: Rgb8,
    pub cursor: Rgb8,
    pub background: Rgb8,
    /// The window's margin round the text, if it has a color of its own.
    pub border: Option<Rgb8>,
    /// Programs' sixteen colors, an old machine's; none for Flow's own.
    pub ansi: Option<[Rgb8; 16]>,
    /// The one color a monochrome tube shows everything in, as the green
    /// and amber screens' phosphor ([`on_phosphor`]); none for full color.
    pub phosphor: Option<Rgb8>,
}

impl Default for Ink {
    fn default() -> Self {
        Ink { text: FOREGROUND, cursor: CURSOR, background: BACKGROUND, border: None, ansi: None, phosphor: None }
    }
}

/// The ink in use. It lives here rather than in each session because the
/// sessions' reader threads also answer programs that ask for the colors
/// (OSC 10, 11 and 12), and every tab should answer alike.
static INK: parking_lot::RwLock<Ink> =
    parking_lot::const_rwlock(Ink { text: FOREGROUND, cursor: CURSOR, background: BACKGROUND, border: None, ansi: None, phosphor: None });

pub fn ink() -> Ink {
    *INK.read()
}

pub fn set_ink(ink: Ink) {
    *INK.write() = ink;
}

/// "#RRGGBB" (the # is optional).
pub fn parse_hex(s: &str) -> Option<Rgb8> {
    let s = s.trim();
    let digits = s.strip_prefix('#').unwrap_or(s);
    if digits.len() != 6 || !digits.is_ascii() {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&digits[i..i + 2], 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?])
}

pub fn hex(c: Rgb8) -> String {
    format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2])
}

/// The color at hue `h` (0..1), saturation `s` and brightness `v`, worked out
/// as the color picker's shader draws its field (`hsv` in ui.wgsl), so the
/// color picked is the one under the marker.
pub fn from_hsv([h, s, v]: [f32; 3]) -> Rgb8 {
    let channel = |offset: f32| {
        let k = (((h + offset).fract() * 6.0 - 3.0).abs() - 1.0).clamp(0.0, 1.0);
        ((v * (1.0 + s * (k - 1.0))).clamp(0.0, 1.0) * 255.0).round() as u8
    };
    [channel(1.0), channel(2.0 / 3.0), channel(1.0 / 3.0)]
}

/// Hue (0..1), saturation and brightness of `c`; a gray's hue is 0.
pub fn to_hsv(c: Rgb8) -> [f32; 3] {
    let [r, g, b] = c.map(|x| x as f32 / 255.0);
    let max = r.max(g).max(b);
    let d = max - r.min(g).min(b);
    let sixths = if d <= 0.0 {
        0.0
    } else if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    [sixths / 6.0, if max > 0.0 { d / max } else { 0.0 }, max]
}

const ANSI: [Rgb8; 16] = [
    [0x1E, 0x24, 0x33], // black
    [0xFF, 0x6B, 0x81], // red
    [0x7E, 0xE0, 0x9F], // green
    [0xFF, 0xD0, 0x7A], // yellow
    [0x6F, 0xA8, 0xFF], // blue
    [0xC5, 0x8B, 0xFF], // magenta
    [0x5F, 0xDD, 0xE8], // cyan
    [0xC9, 0xD1, 0xDF], // white
    [0x5A, 0x64, 0x7A], // bright black
    [0xFF, 0x8F, 0xA0], // bright red
    [0xA6, 0xF0, 0xBC], // bright green
    [0xFF, 0xE2, 0xA6], // bright yellow
    [0x9A, 0xC4, 0xFF], // bright blue
    [0xDA, 0xB2, 0xFF], // bright magenta
    [0x8E, 0xEC, 0xF4], // bright cyan
    [0xFF, 0xFF, 0xFF], // bright white
];

/// Color for an xterm palette index (0-255) or alacritty's named slots (256+),
/// with the text and cursor in `ink`'s colors.
pub fn color_for_index(index: usize, ink: Ink) -> Rgb8 {
    let ansi = ink.ansi.unwrap_or(ANSI);
    match index {
        0..=15 => ansi[index],
        16..=231 => {
            let i = index - 16;
            let level = |v: usize| if v == 0 { 0 } else { (55 + v * 40) as u8 };
            [level(i / 36), level((i / 6) % 6), level(i % 6)]
        }
        232..=255 => {
            let v = (8 + (index - 232) * 10) as u8;
            [v, v, v]
        }
        256 | 267 => ink.text,
        257 => ink.background,
        258 => ink.cursor,
        259..=266 => dim(ansi[index - 259]),
        268 => dim(ink.text),
        _ => ink.text,
    }
}

/// What a program asking for color `index` is told: the color as it's
/// shown, so on a monochrome screen its phosphor's shade.
pub fn answered(index: usize, ink: Ink) -> Rgb8 {
    let c = color_for_index(index, ink);
    ink.phosphor.map_or(c, |p| on_phosphor(c, p))
}

/// The share of its brightest part a saturated color is shown at, at the
/// least, on a monochrome screen: by luma alone pure blue is a ninth as
/// bright as white, and blue text on black would all but vanish, as it did
/// on a real monitor.
pub const PHOSPHOR_FLOOR: f32 = 0.35;

/// How bright `c` is to a monochrome monitor: the luma of its video signal,
/// as Rec. 601 weighs the display's own values, as composite video and mono
/// VGA did (`lum` in the shaders).
pub(crate) fn luma(c: [f32; 3]) -> f32 {
    0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2]
}

/// `c` as a monochrome monitor of phosphor `p` shows it, the twin of
/// `ui.wgsl`'s `on_phosphor`, which draws everything Flow shows so while one
/// is set: a shade of `p` as bright as `c`'s [`luma`] (or
/// [`PHOSPHOR_FLOOR`] of its brightest part where that's more), the
/// brightness the monitor took from the signal. So black is black, white
/// is `p` at full and never paler, as one phosphor can't be, and every
/// color keeps its place between them: `p` itself, as a color sent, shows
/// a little below full, and the green and amber screens' paler shades
/// above it.
pub fn on_phosphor(c: Rgb8, p: Rgb8) -> Rgb8 {
    let unit = |c: Rgb8| c.map(|v| v as f32 / 255.0);
    let (c, p) = (unit(c), unit(p));
    let l = luma(c).max(PHOSPHOR_FLOOR * c[0].max(c[1]).max(c[2]));
    let shown = p.map(|v| v * l.min(1.0));
    shown.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
}

fn dim(c: Rgb8) -> Rgb8 {
    [(c[0] as u16 * 2 / 3) as u8, (c[1] as u16 * 2 / 3) as u8, (c[2] as u16 * 2 / 3) as u8]
}

fn lookup(colors: &Colors, index: usize, ink: Ink) -> Rgb8 {
    match colors[index] {
        Some(rgb) => [rgb.r, rgb.g, rgb.b],
        None => color_for_index(index, ink),
    }
}

/// Resolves a cell color, honoring palette overrides set by programs (OSC 4/10/11).
pub fn resolve(color: Color, colors: &Colors, ink: Ink, bold: bool, dim_flag: bool) -> Rgb8 {
    match color {
        Color::Spec(rgb) => [rgb.r, rgb.g, rgb.b],
        Color::Indexed(i) => {
            let mut idx = i as usize;
            if bold && idx < 8 {
                idx += 8;
            }
            let c = lookup(colors, idx, ink);
            if dim_flag { dim(c) } else { c }
        }
        Color::Named(named) => {
            let mut idx = named as usize;
            if bold && idx < 8 {
                idx += 8;
            }
            if dim_flag {
                if idx < 8 {
                    return lookup(colors, NamedColor::DimBlack as usize + idx, ink);
                }
                if named == NamedColor::Foreground {
                    return lookup(colors, NamedColor::DimForeground as usize, ink);
                }
            }
            lookup(colors, idx, ink)
        }
    }
}

pub fn is_default_bg(color: Color) -> bool {
    matches!(color, Color::Named(NamedColor::Background))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_colors_read_with_or_without_the_hash_and_write_back() {
        assert_eq!(parse_hex("#8FE3FF"), Some(CURSOR));
        assert_eq!(parse_hex(" e4e9f2 "), Some(FOREGROUND));
        assert_eq!(parse_hex("#FFF"), None);
        assert_eq!(parse_hex("#GG0000"), None);
        assert_eq!(parse_hex("#ÿÿÿ"), None);
        assert_eq!(hex([0xE4, 0x09, 0xF2]), "#E409F2");
    }

    #[test]
    fn colors_survive_a_trip_through_hue_saturation_and_brightness() {
        assert_eq!(from_hsv([0.0, 1.0, 1.0]), [255, 0, 0]);
        assert_eq!(from_hsv([1.0 / 3.0, 1.0, 1.0]), [0, 255, 0]);
        assert_eq!(from_hsv([0.5, 0.0, 0.5]), [128, 128, 128]);
        for c in [FOREGROUND, BACKGROUND, CURSOR, [0, 0, 0], [255, 255, 255], [0xC5, 0x8B, 0xFF], [0x12, 0x34, 0x56], [200, 10, 90]] {
            let back = from_hsv(to_hsv(c));
            assert!(c.iter().zip(back).all(|(a, b)| a.abs_diff(b) <= 1), "{c:?} came back as {back:?}");
        }
    }

    #[test]
    fn plain_text_takes_the_ink_but_program_colors_stay() {
        let ink = Ink { text: [1, 2, 3], cursor: [4, 5, 6], background: [7, 8, 9], border: None, ansi: None, phosphor: None };
        let colors = Colors::default();
        assert_eq!(resolve(Color::Named(NamedColor::Foreground), &colors, ink, false, false), [1, 2, 3]);
        assert_eq!(resolve(Color::Named(NamedColor::Foreground), &colors, ink, false, true), dim([1, 2, 3]));
        assert_eq!(resolve(Color::Named(NamedColor::Red), &colors, ink, false, false), ANSI[1]);
        assert_eq!(color_for_index(NamedColor::Cursor as usize, ink), [4, 5, 6]);
        assert_eq!(resolve(Color::Named(NamedColor::Background), &colors, ink, false, false), [7, 8, 9]);
    }

    const GREEN: Rgb8 = [0x41, 0xFF, 0x6E];

    #[test]
    fn a_phosphor_shows_black_as_black_and_white_as_itself_at_full() {
        for p in crate::settings::PHOSPHORS.map(|(_, p)| p) {
            assert_eq!(on_phosphor([0, 0, 0], p), [0, 0, 0]);
            assert_eq!(on_phosphor([255, 255, 255], p), p);
        }
    }

    #[test]
    fn each_color_shows_as_bright_as_its_signal_between_black_and_the_phosphor() {
        // A grey's luma is its own level, so it shows at that share of the
        // phosphor; the phosphor's own color sent is a little below full.
        for g in [0x20u8, 0x40, 0x80, 0xC0, 0xF0] {
            let shown = on_phosphor([g, g, g], GREEN);
            assert!((shown[1] as i32 - g as i32).abs() <= 1, "grey {g:#x} showed as {shown:?}");
        }
        let text = on_phosphor(GREEN, GREEN);
        let share = luma(GREEN.map(|v| v as f32 / 255.0));
        assert!((text[1] as f32 - 255.0 * share).abs() <= 1.0, "the phosphor's own color showed as {text:?}");
        assert!(text[1] < 255);
    }

    #[test]
    fn every_color_is_a_shade_of_the_phosphor_and_none_is_paler() {
        let pale = [[0xFF, 0xFF, 0xFF], [0xD0, 0xFF, 0xDC], [0xB4, 0xFF, 0xFF], [0xFF, 0xE6, 0xA8], [0xC0, 0xC0, 0xC0]];
        for p in crate::settings::PHOSPHORS.map(|(_, p)| p) {
            let brightest = (0..3).max_by_key(|&k| p[k]).unwrap();
            for c in [[0x80, 0x10, 0x10], [0x00, 0x00, 0xFF], [0x30, 0x30, 0x30], [0x12, 0x34, 0x56], [0x00, 0x40, 0x00]].into_iter().chain(pale) {
                let shown = on_phosphor(c, p);
                // Each part the same share of the phosphor's, within
                // rounding, and none past it.
                let share = shown[brightest] as f32 / p[brightest] as f32;
                for k in 0..3 {
                    assert!((shown[k] as f32 - p[k] as f32 * share).abs() <= 1.0, "{c:?} showed as {shown:?} on {p:?}");
                    assert!(shown[k] <= p[k], "{c:?} showed paler than {p:?}, as {shown:?}");
                }
            }
        }
        // The green and amber screens' palest shades show brighter than
        // their text, the phosphor's own color, and short of white.
        for (palest, p) in [([0xD0, 0xFF, 0xDC], GREEN), ([0xFF, 0xE6, 0xA8], [0xFF, 0xB0, 0x00])] {
            let (shown, text, white) = (on_phosphor(palest, p), on_phosphor(p, p), on_phosphor([255; 3], p));
            assert!(text[1] < shown[1] && shown[1] < white[1], "{text:?} < {shown:?} < {white:?}");
        }
    }

    #[test]
    fn blue_on_black_stays_readable_on_a_phosphor() {
        let blue = on_phosphor([0, 0, 0xFF], GREEN);
        assert!(blue[1] as f32 >= 0.35 * 255.0 * 0.95, "pure blue showed as {blue:?}");
        // Brighter colors stay brighter.
        assert!(on_phosphor([0xFF, 0xFF, 0], GREEN)[1] > on_phosphor([0xFF, 0, 0], GREEN)[1]);
    }

    /// A pixel of the finished 8-bit frame brought back onto `p`, as the
    /// tube's last pass does and, without the tube, `crt.wgsl`'s
    /// `fs_onto_phosphor`: `p` as bright as the pixel's luma is of `p`'s,
    /// and no brighter than `p` at full.
    fn onto_phosphor(c: Rgb8, p: Rgb8) -> Rgb8 {
        let unit = |c: Rgb8| c.map(|v| v as f32 / 255.0);
        let (c, p) = (unit(c), unit(p));
        let k = (luma(c) / luma(p).max(0.001)).min(1.0);
        p.map(|v| (v * k * 255.0).round() as u8)
    }

    #[test]
    fn a_glow_clipped_over_bright_text_goes_back_onto_the_phosphor_and_shades_stay() {
        for p in crate::settings::PHOSPHORS.map(|(_, p)| p) {
            // Every shade the frame was drawn in comes back as it was.
            for c in [[0x10, 0x10, 0x10], [0x80, 0x80, 0x80], p, [0x00, 0x00, 0xFF], [0xD0, 0xFF, 0xDC], [0xFF; 3]] {
                let shade = on_phosphor(c, p);
                let back = onto_phosphor(shade, p);
                assert!((0..3).all(|k| shade[k].abs_diff(back[k]) <= 1), "{shade:?} came back as {back:?} on {p:?}");
            }
            // The text, the phosphor sent, with a glow of it added over it:
            // the 8-bit frame clips the strongest channel first, and the
            // pixel drifts toward white, paler than the phosphor can be.
            let text = on_phosphor(p, p);
            let glow = on_phosphor(p, p).map(|v| v / 2);
            let clipped: Rgb8 = std::array::from_fn(|k| text[k].saturating_add(glow[k]));
            let paler = (0..3).any(|k| clipped[k] > p[k]);
            assert!(paler, "the glow over {text:?} on {p:?} stayed within the phosphor, as {clipped:?}");
            // Brought back, it's the phosphor at full, never paler.
            let back = onto_phosphor(clipped, p);
            assert!((0..3).all(|k| back[k].abs_diff(p[k]) <= 1), "{clipped:?} came back as {back:?}, not {p:?}");
        }
    }

    #[test]
    fn programs_asking_for_a_color_are_told_its_phosphor_shade() {
        let ink = Ink { phosphor: Some(GREEN), ..Ink::default() };
        assert_eq!(answered(1, ink), on_phosphor(ANSI[1], GREEN));
        assert_eq!(answered(1, Ink::default()), ANSI[1]);
    }
}
