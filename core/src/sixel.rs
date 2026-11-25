//! Sixel pictures, as DEC's VT330 and VT340 drew them and xterm, mlterm and
//! foot still do: a picture in a DCS, `ESC P <P1> ; <P2> ; <P3> q <data>
//! ESC \`, painted six rows of pixels at a time. The scanner finds it
//! (`crate::osc`'s `Marker::Sixel`), this decodes it, and the session keeps
//! it and places it at the cursor as a kitty picture is
//! (`ImageStore::show`).
//!
//! The data, as DEC's VT330/VT340 Programmer Reference has it (chapter 14):
//! - a character from `?` to `~` is a sixel, six pixels one above the
//!   other: its bits once `?` is taken off, the lowest at the top, are those
//!   painted in the color chosen; the next goes to its right;
//! - `!Pn` paints the sixel after it Pn times;
//! - `$` goes back to the left edge of the band of six rows, to paint over
//!   it in another color, and `-` to the left edge of the next band down;
//! - `#Pc` chooses color register Pc, and `#Pc;Pu;Px;Py;Pz` sets it first,
//!   in HLS (Pu 1: the hue in degrees with blue at 0, red at 120 and green
//!   at 240, then lightness and saturation in percent) or RGB (Pu 2, each in
//!   percent);
//! - `"Pan;Pad;Ph;Pv` gives the raster: pixels Pan/Pad as tall as they are
//!   wide, and a picture Ph × Pv of them, which it's at least.
//!
//! Where no raster says, P1 gives the pixels' shape (2:1 unless it says
//! otherwise, as DEC's terminals had them), and P2 says what the pixels
//! nothing painted show: register 0's color, or with 1 nothing, so the
//! glass shows through. Each picture starts from the VT340's sixteen colors
//! (xterm's private color registers), so one picture's colors never change
//! another's, and a pixel keeps the color it was painted in.

use crate::graphics::{Image, MAX_SIDE};

/// The color registers a picture has, as XTSMGRAPHICS tells programs.
pub const REGISTERS: usize = 1024;

/// The most pixels a picture covers, 128 MB of them, as much as a kitty
/// picture's data may be: a few bytes of `!` repeats can ask for far more.
const MAX_PIXELS: usize = 32 << 20;

/// The VT340's sixteen colors, in percent, which each picture starts with.
const VT340: [[u8; 3]; 16] = [
    [0, 0, 0],    // black
    [20, 20, 80], // blue
    [80, 13, 13], // red
    [20, 80, 20], // green
    [80, 20, 80], // magenta
    [20, 80, 80], // cyan
    [80, 80, 20], // yellow
    [53, 53, 53], // gray 50%
    [26, 26, 26], // gray 25%
    [33, 33, 60], // pale blue
    [60, 26, 26], // pale red
    [33, 60, 33], // pale green
    [60, 33, 60], // pale magenta
    [33, 60, 60], // pale cyan
    [60, 60, 33], // pale yellow
    [80, 80, 80], // gray 75%
];

/// A part of 100 as a byte.
fn percent(p: u32) -> u8 {
    ((p.min(100) * 255 + 50) / 100) as u8
}

/// A part of 1 as a byte.
fn unit(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// DEC's HLS as RGB: `hue` in degrees with blue at 0, `light` and
/// `saturation` in percent.
fn hls(hue: u32, light: u32, saturation: u32) -> [u8; 3] {
    // The usual wheel, with red at 0, turned by 240 degrees.
    let h = ((hue % 360) as f32 + 240.0) % 360.0 / 360.0;
    let (l, s) = (light.min(100) as f32 / 100.0, saturation.min(100) as f32 / 100.0);
    if s == 0.0 {
        return [unit(l); 3];
    }
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let channel = |t: f32| {
        let t = t.rem_euclid(1.0);
        unit(if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        })
    };
    [channel(h + 1.0 / 3.0), channel(h), channel(h - 1.0 / 3.0)]
}

/// Up to `N` numbers after an introducer at `data[*at..]`, `;` between them
/// (an empty one is 0), and how many there were; `at` moves past them.
fn numbers<const N: usize>(data: &[u8], at: &mut usize) -> ([u32; N], usize) {
    let mut values = [0u32; N];
    let (mut n, mut any) = (0, false);
    while let Some(&b) = data.get(*at) {
        match b {
            b'0'..=b'9' => {
                if let Some(v) = values.get_mut(n) {
                    *v = v.saturating_mul(10).saturating_add((b - b'0') as u32);
                }
            }
            b';' => n += 1,
            _ => break,
        }
        any = true;
        *at += 1;
    }
    (values, if any { (n + 1).min(N) } else { 0 })
}

/// A picture as it's painted: each band of six rows a column at a time, six
/// pixels to a column, as long as the band's been painted, so a band grows
/// by pushing alone. A pixel never painted has no alpha.
struct Painter {
    bands: Vec<Vec<[u8; 4]>>,
    band: usize,
    x: usize,
    registers: Vec<[u8; 3]>,
    color: usize,
    /// The furthest right a sixel reached, and the furthest down a pixel
    /// was painted.
    reach: (usize, usize),
    /// Pixels the bands hold.
    held: usize,
}

impl Painter {
    fn new() -> Self {
        let mut registers = vec![[0; 3]; REGISTERS];
        for (r, c) in registers.iter_mut().zip(VT340) {
            *r = c.map(|p| percent(p as u32));
        }
        Self { bands: Vec::new(), band: 0, x: 0, registers, color: 0, reach: (0, 0), held: 0 }
    }

    /// Paints sixel `bits` `count` times from where the last one left off.
    fn paint(&mut self, bits: u8, count: usize) {
        let (from, to) = (self.x, self.x.saturating_add(count).min(MAX_SIDE as usize));
        self.x = to;
        self.reach.0 = self.reach.0.max(to);
        let top = self.band * 6;
        if bits == 0 || from >= to || top >= MAX_SIDE as usize {
            return;
        }
        if self.bands.len() <= self.band {
            self.bands.resize_with(self.band + 1, Vec::new);
        }
        let band = &mut self.bands[self.band];
        if band.len() < to * 6 {
            // Past what a picture may hold, the rest of it isn't painted.
            let more = to * 6 - band.len();
            if self.held + more > MAX_PIXELS {
                return;
            }
            self.held += more;
            band.resize(to * 6, [0; 4]);
        }
        let [r, g, b] = self.registers[self.color];
        for column in band[from * 6..to * 6].as_chunks_mut::<6>().0 {
            for (row, px) in column.iter_mut().enumerate() {
                if bits >> row & 1 == 1 {
                    *px = [r, g, b, 255];
                }
            }
        }
        // The lowest pixel painted: below the highest bit set.
        let lowest = 8 - bits.leading_zeros() as usize;
        self.reach.1 = self.reach.1.max((top + lowest).min(MAX_SIDE as usize));
    }

    /// What pixel (`x`, `y`) was painted, if it was.
    fn at(&self, x: usize, y: usize) -> Option<[u8; 4]> {
        let px = *self.bands.get(y / 6)?.get(x * 6 + y % 6)?;
        (px[3] != 0).then_some(px)
    }
}

/// The picture in a sixel DCS: its parameters before the `q` (P1 the
/// pixels' shape, P2 what's unpainted, P3 the grid, which is passed over)
/// and its data after it. Premultiplied RGBA, as [`Image::new`] takes; none
/// if nothing in it has a size.
pub fn decode(params: &[u16], data: &[u8]) -> Option<Image> {
    let param = |i: usize| params.get(i).copied().unwrap_or(0);
    // DEC's shapes for P1: 2:1 unless 2 (5:1), 3 or 4 (3:1), or 7 to 9 (1:1).
    let (mut tall, mut wide) = match param(0) {
        2 => (5, 1),
        3 | 4 => (3, 1),
        7..=9 => (1, 1),
        _ => (2, 1),
    };
    let clear = param(1) == 1;
    let mut raster = (0, 0);
    let mut p = Painter::new();
    let mut at = 0;
    while let Some(&b) = data.get(at) {
        at += 1;
        match b {
            b'?'..=b'~' => p.paint(b - b'?', 1),
            b'!' => {
                let ([count], _) = numbers::<1>(data, &mut at);
                if let Some(&c @ b'?'..=b'~') = data.get(at) {
                    at += 1;
                    p.paint(c - b'?', count.max(1) as usize);
                }
            }
            b'$' => p.x = 0,
            b'-' => {
                p.x = 0;
                p.band += 1;
            }
            b'#' => {
                let ([index, space, x, y, z], given) = numbers::<5>(data, &mut at);
                if given == 0 {
                    continue;
                }
                p.color = index as usize % REGISTERS;
                if given == 5 {
                    let color = match space {
                        1 => hls(x, y, z),
                        2 => [percent(x), percent(y), percent(z)],
                        _ => continue,
                    };
                    p.registers[p.color] = color;
                }
            }
            b'"' => {
                let ([pan, pad, ph, pv], given) = numbers::<4>(data, &mut at);
                if given >= 2 {
                    (tall, wide) = (pan.max(1), pad.max(1));
                }
                if given == 4 {
                    raster = (ph.min(MAX_SIDE) as usize, pv.min(MAX_SIDE) as usize);
                }
            }
            // Line breaks a program sends to keep its lines short, and
            // anything else, are passed over.
            _ => {}
        }
    }
    let width = p.reach.0.max(raster.0);
    let height = p.reach.1.max(raster.1);
    if width == 0 || height == 0 {
        return None;
    }
    // Its pixels' shape: each row drawn as many times as they're taller than
    // wide, within what a picture may hold.
    let most = (MAX_SIDE as usize).min(MAX_PIXELS / width);
    let shown = ((height as u64 * tall as u64 + wide as u64 / 2) / wide as u64).clamp(1, most as u64) as usize;
    let background = if clear {
        [0; 4]
    } else {
        let [r, g, b] = p.registers[0];
        [r, g, b, 255]
    };
    let mut pixels = Vec::with_capacity(width * shown * 4);
    for row in 0..shown {
        let y = (row as u64 * wide as u64 / tall as u64).min(height as u64 - 1) as usize;
        for x in 0..width {
            pixels.extend_from_slice(&p.at(x, y).unwrap_or(background));
        }
    }
    Some(Image::new(width as u32, shown as u32, pixels))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sixel's pixels, decoded with `params`.
    fn picture(params: &[u16], data: &str) -> Image {
        decode(params, data.as_bytes()).expect("a picture")
    }

    fn pixel(image: &Image, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * image.width + x) * 4) as usize;
        image.pixels[i..i + 4].try_into().unwrap()
    }

    const RED: [u8; 4] = [255, 0, 0, 255];
    const BLUE: [u8; 4] = [0, 0, 255, 255];

    #[test]
    fn a_sixel_two_pixels_wide_paints_each_column_in_its_own_color() {
        let image = picture(&[0, 0, 0], "\"1;1;2;6#0;2;100;0;0#1;2;0;0;100#0~#1~");
        assert_eq!((image.width, image.height), (2, 6));
        for y in 0..6 {
            assert_eq!((pixel(&image, 0, y), pixel(&image, 1, y)), (RED, BLUE), "row {y}");
        }
    }

    #[test]
    fn repeats_carriage_returns_and_new_bands_paint_where_they_say() {
        // Ten red, ten blue painted over the band's other ten, then a second
        // band whose top pixel alone is green, twenty wide.
        let image = picture(&[], "\"1;1;20;12#1;2;100;0;0!10~!10?$#2;2;0;0;100!10?!10~-#3;2;0;100;0!20@");
        assert_eq!((image.width, image.height), (20, 12));
        assert_eq!((pixel(&image, 0, 0), pixel(&image, 9, 5), pixel(&image, 10, 0), pixel(&image, 19, 5)), (RED, RED, BLUE, BLUE));
        assert_eq!(pixel(&image, 5, 6), [0, 255, 0, 255]);
        // Below it, painted nothing: register 0, the VT340's black.
        assert_eq!(pixel(&image, 5, 7), [0, 0, 0, 255]);
        // A repeat of 0 paints once, as one of 1 does.
        assert_eq!(picture(&[], "\"1;1;1;1!0~").width, 1);
    }

    #[test]
    fn colors_set_in_hls_turn_decs_wheel_with_blue_at_its_top() {
        let image = picture(&[], "\"1;1;5;6#1;1;120;50;100~#2;1;240;50;100~#3;1;0;50;100~#4;1;0;100;0~#5;1;0;50;0~");
        let shown: Vec<[u8; 4]> = (0..5).map(|x| pixel(&image, x, 0)).collect();
        assert_eq!(shown, [RED, [0, 255, 0, 255], BLUE, [255; 4], [128, 128, 128, 255]]);
        // Without colors of its own, the VT340's: register 2 is its red.
        assert_eq!(pixel(&picture(&[], "\"1;1;1;6#2~"), 0, 0), [204, 33, 33, 255]);
        // A register past the last wraps round.
        assert_eq!(pixel(&picture(&[], "\"1;1;1;6#1026~"), 0, 0), [204, 33, 33, 255]);
    }

    #[test]
    fn with_p2_of_1_what_isnt_painted_shows_the_glass() {
        // Only each sixel's top pixel, and a gap between them.
        let clear = picture(&[0, 1], "\"1;1;3;6#1;2;100;0;0@?@");
        assert_eq!((pixel(&clear, 0, 0), pixel(&clear, 1, 0), pixel(&clear, 0, 1)), (RED, [0; 4], [0; 4]));
        let filled = picture(&[0, 0], "\"1;1;3;6#0;2;0;0;100#1;2;100;0;0@?@");
        assert_eq!((pixel(&filled, 0, 0), pixel(&filled, 1, 0), pixel(&filled, 0, 1)), (RED, BLUE, BLUE), "register 0's color");
    }

    #[test]
    fn its_size_is_the_rasters_or_as_far_as_its_pixels_reach() {
        // No raster: as wide as its sixels go, as tall as its lowest pixel,
        // which DEC's 2:1 pixels make twice as tall.
        let image = picture(&[], "#1~~?-@");
        assert_eq!((image.width, image.height), (3, 14));
        assert_eq!((pixel(&image, 0, 11), pixel(&image, 0, 12), pixel(&image, 0, 13)), ([51, 51, 204, 255], [51, 51, 204, 255], [51, 51, 204, 255]));
        assert_eq!(picture(&[9], "#1~~?-@").height, 7, "P1 of 9: square pixels");
        assert_eq!(picture(&[2], "~").height, 30, "P1 of 2: five times as tall");
        // A raster's shape wins over P1's, and its size is the least.
        let image = picture(&[], "\"2;1;4;3#1~");
        assert_eq!((image.width, image.height), (4, 12));
        assert_eq!(pixel(&image, 3, 0), [0, 0, 0, 255], "past what was painted, the background");
        // Nothing with a size is no picture.
        assert!(decode(&[], b"#1;2;100;0;0").is_none());
        assert!(decode(&[], b"").is_none());
    }

    #[test]
    fn a_picture_is_kept_within_the_largest_side() {
        let image = picture(&[0, 0, 0], "\"1;1;1;1!40000~");
        assert_eq!((image.width, image.height), (MAX_SIDE, 6));
        assert_eq!(picture(&[], "\"1;1;30000;6~").width, MAX_SIDE, "a raster too");
        // Bands below the largest side aren't painted.
        let deep = format!("\"1;1;1;1~{}~", "-".repeat(2000));
        assert_eq!(picture(&[], &deep).height, 6);
    }
}
