//! The makers' logos the startup logo and the old machines' themes show, in
//! a showcase of the computers of their time: Intel's and AMD's for the
//! computer's chip, set by Flow from the system's fonts, Sun Microsystems'
//! emblem on the Workstation theme's boot screen, from its artwork, SGI's
//! cube of tubes turning over the Graphics workstation's, from a model of it
//! (`assets/logos/`), and the Amiga's Boing ball spinning beside the
//! notanAmiga CLI's copyright, a ball Flow makes itself, with the hand
//! holding out its Workbench disk on the first screen before, Kickstart
//! 1.3's own picture. And beside the startup logo, the graphics API Flow
//! draws with, by its logo where it has one here ([`ApiLogo`]: Vulkan's,
//! OpenGL's, Metal's). Each is its owner's, and nothing to do with Flow
//! (`docs/trademarks.md`).

use std::sync::Arc;

use crate::graphics::Image;
use crate::model::{self, Format, Mesh, Part, Shown, Slot, Style, Vertex};

/// Who made a chip, by its name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Maker {
    Intel,
    Amd,
}

/// The chip's maker, from the name the computer gives it ("Intel(R)
/// Core(TM) i7", "AMD Ryzen 7"); none for another's (Apple's own, or Arm).
pub fn maker(chip: &str) -> Option<Maker> {
    let chip = chip.to_ascii_lowercase();
    if chip.contains("intel") {
        Some(Maker::Intel)
    } else if ["amd", "ryzen", "epyc", "athlon", "threadripper"].iter().any(|w| chip.contains(w)) {
        Some(Maker::Amd)
    } else {
        None
    }
}

/// Premultiplies `color` by alpha `a` into a pixel.
fn pixel([r, g, b]: [u8; 3], a: u8) -> [u8; 4] {
    let m = |c: u8| (c as u16 * a as u16 / 255) as u8;
    [m(r), m(g), m(b), a]
}

/// A word drawn as white coverage (from `Atlas::word`), `w` wide and `h`
/// tall.
pub struct Word {
    pub pixels: Vec<u8>,
    pub w: u32,
    pub h: u32,
}

impl Word {
    fn alpha(&self, x: i32, y: i32) -> u8 {
        if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 {
            return 0;
        }
        self.pixels[((y as u32 * self.w + x as u32) * 4 + 3) as usize]
    }
}

/// Lays `pieces` (a word and where its top left goes) into one picture
/// `w` × `h` in `color`.
fn compose(pieces: &[(&Word, i32, i32, [u8; 3])], w: u32, h: u32) -> Image {
    let mut out = vec![0u8; (w * h * 4) as usize];
    for (word, x0, y0, color) in pieces {
        for y in 0..word.h as i32 {
            for x in 0..word.w as i32 {
                let (cx, cy) = (x0 + x, y0 + y);
                let a = word.alpha(x, y);
                if a == 0 || cx < 0 || cy < 0 || cx >= w as i32 || cy >= h as i32 {
                    continue;
                }
                let at = ((cy as u32 * w + cx as u32) * 4) as usize;
                if a > out[at + 3] {
                    out[at..at + 4].copy_from_slice(&pixel(*color, a));
                }
            }
        }
    }
    Image::new(w, h, out)
}

/// Intel's wordmark as it was from 1968 to 2005, in its blue: "int", the e
/// dropped below the line, and "l". `int`, `e` and `l` are the three drawn
/// in a bold sans of the same size.
pub fn intel(int: &Word, e: &Word, l: &Word) -> Image {
    const BLUE: [u8; 3] = [0x1a, 0x8c, 0xe0];
    let gap = (int.h as f32 * 0.08).round().max(1.0) as i32;
    // The tall letters stand on the line; the e drops a third of its height
    // below it.
    let line = int.h.max(l.h) as i32;
    let drop = (e.h as f32 * 0.38).round() as i32;
    let (ex, lx) = (int.w as i32 + gap, int.w as i32 + gap + e.w as i32 + gap);
    let (w, h) = ((lx + l.w as i32) as u32, (line + drop) as u32);
    compose(&[(int, 0, line - int.h as i32, BLUE), (e, ex, line - e.h as i32 + drop, BLUE), (l, lx, line - l.h as i32, BLUE)], w, h)
}

/// AMD's wordmark in `ink` and its arrow beside it: a square whose
/// top and right edges make the arrow's head, its point to the upper
/// right, and a smaller square below left of it with the corner toward the
/// point cut away.
pub fn amd(word: &Word, ink: [u8; 3]) -> Image {
    let side = word.h as i32;
    let gap = (side as f32 * 0.35).round() as i32;
    let (w, h) = ((word.w as i32 + gap + side) as u32, word.h);
    let mut image = compose(&[(word, 0, 0, ink)], w, h);
    let x0 = word.w as i32 + gap;
    let band = 0.27;
    let mut set = |x: i32, y: i32, a: u8| {
        let at = ((y as u32 * w + x as u32) * 4) as usize;
        image.pixels[at..at + 4].copy_from_slice(&pixel(ink, a));
    };
    for y in 0..side {
        for x in 0..side {
            // Four samples a pixel, in the square's own units, y up.
            let mut hits = 0;
            for (sx, sy) in [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)] {
                let (u, v) = ((x as f32 + sx) / side as f32, 1.0 - (y as f32 + sy) / side as f32);
                let head = u >= 1.0 - band || v >= 1.0 - band;
                let tail = u < 1.0 - band - 0.12 && v < 1.0 - band - 0.12 && u + v < 1.0 - band;
                hits += u8::from(head || tail);
            }
            if hits > 0 {
                set(x0 + x, y, (hits as u16 * 255 / 4) as u8);
            }
        }
    }
    image
}

/// Sun Microsystems' emblem as it was from 1983 to 1996, in its purple:
/// a square on its corner, woven of the letters u and n that read "sun"
/// four ways. The picture is the emblem itself, made from the artwork
/// (`assets/logos/README.md`).
const SUN: &[u8] = include_bytes!("../../assets/logos/sun.png");

/// The graphics APIs' logos, each a trademark of its owner: Vulkan's and
/// OpenGL's as Wikimedia Commons has them, Metal's Apple's newer icon
/// (`assets/logos/README.md`).
const VULKAN: &[u8] = include_bytes!("../../assets/logos/vulkan.png");
const OPENGL: &[u8] = include_bytes!("../../assets/logos/opengl.png");
const METAL: &[u8] = include_bytes!("../../assets/logos/metal.png");
/// Microsoft's DirectX wordmark, the word alone (without "Microsoft®"
/// above it or the ® after it), black as Commons draws it.
const DIRECTX: &[u8] = include_bytes!("../../assets/logos/directx.png");

/// A graphics API's logo, which the banner beside the startup logo shows
/// in place of the API's name (`App::banner`): Vulkan's and OpenGL's
/// wordmarks, Metal's icon alone, as the user would have it, and DirectX's
/// wordmark for Direct3D 12, in the text's ink.
pub struct ApiLogo {
    /// The API's name, written where there's no room for the logo.
    pub name: String,
    width: u32,
    height: u32,
    /// Its pixels, premultiplied.
    pixels: Vec<u8>,
    /// Drawn in the text's ink (`in_ink`), its shape alone: DirectX's
    /// wordmark, black, which wouldn't show on Flow's dark glass, as X11's
    /// X in About.
    one_color: bool,
}

/// The logo of the API `backend` names, as the renderer says it ("Metal",
/// "Vulkan", "OpenGL", "Direct3D 12").
pub fn api(backend: &str) -> Option<ApiLogo> {
    let (png, one_color) = match backend {
        "Vulkan" => (VULKAN, false),
        "OpenGL" => (OPENGL, false),
        "Metal" => (METAL, false),
        "Direct3D 12" => (DIRECTX, true),
        _ => return None,
    };
    let (width, height, mut pixels) = crate::graphics::decode_png(png).ok()?;
    crate::graphics::premultiply(&mut pixels);
    Some(ApiLogo { name: backend.to_string(), width, height, pixels, one_color })
}

impl ApiLogo {
    /// The logo in `ink`, the color of the line it ends, where it's of one
    /// color (its alpha the shape); a logo of its own colors as it is.
    pub fn in_ink(mut self, [r, g, b]: [u8; 3]) -> Self {
        if self.one_color {
            for p in self.pixels.as_chunks_mut::<4>().0 {
                let a = p[3] as u16;
                *p = [(r as u16 * a / 255) as u8, (g as u16 * a / 255) as u8, (b as u16 * a / 255) as u8, p[3]];
            }
        }
        self
    }

    /// The logo as one row of cells `cell` (width, height, in pixels) shows
    /// it: as tall as the row less a little, averaged down from the artwork,
    /// at the left of as many whole cells as it takes, so it's never
    /// stretched; and how many.
    pub fn in_row(&self, (cw, ch): (f32, f32)) -> (Image, u32) {
        let tall = ((ch * 0.86).round() as u32).clamp(1, self.height);
        let wide = ((self.width as f32 * tall as f32 / self.height as f32).round() as u32).max(1);
        let logo = crate::record::shrink(&self.pixels, self.width, self.height, wide, tall);
        let cols = (wide as f32 / cw.max(1.0)).ceil().max(1.0) as u32;
        let (w, h) = (((cols as f32 * cw).round() as u32).max(wide), (ch.round() as u32).max(tall));
        let mut pixels = vec![0u8; (w * h * 4) as usize];
        let top = (h - tall) / 2;
        for y in 0..tall {
            let from = (y * wide * 4) as usize;
            let to = (((top + y) * w) * 4) as usize;
            pixels[to..to + (wide * 4) as usize].copy_from_slice(&logo[from..from + (wide * 4) as usize]);
        }
        (Image::new(w, h, pixels), cols)
    }
}

/// SGI's cube of tubes, a model of the logo made from an STL by
/// `assets/logos/stl2glb.py`, in its blue.
const SGI_CUBE: &[u8] = include_bytes!("../../assets/logos/sgi-cube.glb");

/// Sun's emblem `size` pixels square, shrunk from the artwork by averaging
/// what each pixel covers.
pub fn sun(size: u32) -> Image {
    shrunk(SUN, size, None)
}

/// The X Window System's X, black, as the Commons' `X11.svg` draws it
/// (`assets/logos/README.md`).
const X11: &[u8] = include_bytes!("../../assets/logos/x11.png");

/// Wayland's logo, its white W on a yellow disc, as the Commons'
/// `Wayland_Logo.svg` draws it (`assets/logos/README.md`).
const WAYLAND: &[u8] = include_bytes!("../../assets/logos/wayland.png");

/// Wayland's yellow, its disc's, for the glow About gives its logo.
pub const WAYLAND_YELLOW: [u8; 3] = [0xFF, 0xBC, 0x00];

/// The color of the glow About gives the window system `name`'s logo, so
/// it shows on the glass: Wayland's in its yellow, X11's black X, drawn in
/// `ink`, in that.
pub fn windowing_glow(name: &str, ink: [u8; 3]) -> [u8; 3] {
    if name == "Wayland" { WAYLAND_YELLOW } else { ink }
}

/// The logo of the window system `name`, as `Platform::windowing` names it
/// (Wayland or X11), `size` pixels square for About's version line:
/// Wayland's in its own colors, X11's black X in `ink`, so it shows on
/// the glass. None for a name without one.
pub fn windowing(name: &str, size: u32, ink: [u8; 3]) -> Option<Image> {
    match name {
        "Wayland" => Some(shrunk(WAYLAND, size, None)),
        "X11" => Some(shrunk(X11, size, Some(ink))),
        _ => None,
    }
}

/// The square picture `png` `size` pixels square, shrunk by averaging what
/// each pixel covers, premultiplied; in `ink` alone where given, its
/// shape kept.
fn shrunk(png: &[u8], size: u32, ink: Option<[u8; 3]>) -> Image {
    let size = size.max(1);
    let Ok((w, h, rgba)) = crate::graphics::decode_png(png) else { return Image::new(size, size, vec![0; (size * size * 4) as usize]) };
    // Each of the picture's pixels spans `step` of the artwork's, partly
    // at its edges: `spans` gives the artwork's pixels and how much of each.
    let step = w.max(h) as f32 / size as f32;
    let spans = |i: u32| -> Vec<(u32, f32)> {
        let (a, b) = (i as f32 * step, (i + 1) as f32 * step);
        (a as u32..(b.ceil() as u32).min(w.max(h))).map(|k| (k, b.min(k as f32 + 1.0) - a.max(k as f32))).filter(|(_, part)| *part > 0.0).collect()
    };
    let spans: Vec<_> = (0..size).map(spans).collect();
    let mut pixels = Vec::with_capacity((size * size * 4) as usize);
    for ys in &spans {
        for xs in &spans {
            // Premultiplied as they're added, so the edges keep the color.
            let mut sum = [0f32; 4];
            for &(y, wy) in ys.iter().filter(|(y, _)| *y < h) {
                for &(x, wx) in xs.iter().filter(|(x, _)| *x < w) {
                    let p = &rgba[((y * w + x) * 4) as usize..][..4];
                    let a = p[3] as f32 / 255.0 * wx * wy;
                    for c in 0..3 {
                        sum[c] += p[c] as f32 * a;
                    }
                    sum[3] += a;
                }
            }
            let area = step * step;
            if let Some(ink) = ink {
                sum = [ink[0] as f32 * sum[3], ink[1] as f32 * sum[3], ink[2] as f32 * sum[3], sum[3]];
            }
            pixels.extend([sum[0] / area, sum[1] / area, sum[2] / area, sum[3] / area * 255.0].map(|v| v.round().clamp(0.0, 255.0) as u8));
        }
    }
    Image::new(size, size, pixels)
}

/// SGI's cube of tubes as a mesh.
pub fn sgi_cube() -> Option<Mesh> {
    model::load(Format::Glb, SGI_CUBE).ok()
}

/// How SGI's cube stands at rest, as the logo is drawn: looking down the
/// long diagonal the tubes turn about in threefold symmetry, so its
/// outline is a hexagon with a point at the top and the bottom and the
/// tubes' bends turn about its middle. Of the cube's four long diagonals
/// it's the one from corner (1, -1, 1) of the model to (-1, 1, -1); down
/// any other, the tubes cross every way. Turned about it, it stands as the
/// logo does the right way up: (-35.264, -45, 0), which looks down the same
/// diagonal, has it upside down, the same turned half round in the
/// picture (Flow 0.1.31 stood it so).
pub const SGI_REST: [f32; 3] = [35.264, 45.0, 180.0];

/// SGI's cube turning, as a picture `px` pixels square for the startup
/// logo, for cells `cell_height` pixels tall. It's Flow's own, so
/// efficient mode holds it still, and a click brings it back to stand as
/// the logo does ([`SGI_REST`]).
pub fn sgi(px: u32, cell_height: f32) -> Option<Image> {
    let slot = Arc::new(Slot::default());
    slot.set(sgi_cube()?);
    let style = Style { animate: true, rotation: SGI_REST, ..Style::default() };
    let shown = Arc::new(Shown::new(slot, style, cell_height).flows_own());
    Some(Image::model(px.max(1), px.max(1), shown))
}

/// The Boing ball's checks: 16 round its axis and 8 from pole to pole, in
/// its red and its white.
const BOING_AROUND: usize = 16;
const BOING_DOWN: usize = 8;
const BOING_RED: [f32; 4] = [0xE8 as f32 / 255.0, 0x23 as f32 / 255.0, 0x1C as f32 / 255.0, 1.0];
const BOING_WHITE: [f32; 4] = [0xF4 as f32 / 255.0, 0xF4 as f32 / 255.0, 0xF4 as f32 / 255.0, 1.0];
/// Each check's sides cut in this many, so the ball is round.
const BOING_FINE: usize = 4;
/// How the ball stands: leaning right of upright, as it did in the demo
/// of 1984, about which it spins.
pub const BOING_LEAN: [f32; 3] = [0.0, 0.0, -17.0];

/// Kickstart 1.3's picture asking for the Workbench disk, pixel for pixel
/// in its four colors, as its 320 × 200 screen showed it: a hand holding
/// out the disk, its label upside down, and its version beside
/// (`assets/logos/README.md`).
pub(crate) const KICKSTART: &[u8] = include_bytes!("../../assets/logos/kickstart.png");

/// The Kickstart's picture at least `tall` pixels tall, each of its pixels
/// made a square of a whole number of them, so it stays as sharp as it was
/// however large it's shown.
pub fn kickstart(tall: f32) -> Option<Image> {
    let (w, h, rgba) = crate::graphics::decode_png(KICKSTART).ok()?;
    let dot = (tall / h as f32).ceil().clamp(1.0, 16.0) as u32;
    let (width, height) = (w * dot, h * dot);
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        let row = &rgba[((y / dot) * w * 4) as usize..][..(w * 4) as usize];
        for x in 0..width {
            pixels.extend_from_slice(&row[((x / dot) * 4) as usize..][..4]);
        }
    }
    Some(Image::new(width, height, pixels))
}

/// The Boing ball, checkered red and white, as a mesh: a part of each
/// color, every check a few rows of quads on the sphere, lit by normals
/// straight out from its middle.
pub fn boing_ball() -> Option<Mesh> {
    let (around, down) = (BOING_AROUND * BOING_FINE, BOING_DOWN * BOING_FINE);
    let point = |i: usize, j: usize| {
        let lon = std::f32::consts::TAU * i as f32 / around as f32;
        let lat = std::f32::consts::FRAC_PI_2 - std::f32::consts::PI * j as f32 / down as f32;
        let p = [lat.cos() * lon.cos(), lat.sin(), -lat.cos() * lon.sin()];
        Vertex::new(p, p, [i as f32 / around as f32, j as f32 / down as f32])
    };
    let mut parts = [Part::plain(BOING_RED, 0.35), Part::plain(BOING_WHITE, 0.35)];
    for j in 0..down {
        for i in 0..around {
            let part = &mut parts[(i / BOING_FINE + j / BOING_FINE) % 2];
            let at = part.vertices.len() as u32;
            part.vertices.extend([point(i, j), point(i + 1, j), point(i + 1, j + 1), point(i, j + 1)]);
            part.indices.extend([at, at + 3, at + 2, at, at + 2, at + 1]);
        }
    }
    Mesh::new(parts.into(), Format::Glb).ok()
}

/// The Boing ball spinning, as a picture `px` pixels square for the startup
/// logo, for cells `cell_height` pixels tall: Flow's own, so efficient mode
/// holds it still, and a click brings it back to stand as it starts.
pub fn boing(px: u32, cell_height: f32) -> Option<Image> {
    let slot = Arc::new(Slot::default());
    slot.set(boing_ball()?);
    let style = Style { animate: true, rotation: BOING_LEAN, ..Style::default() };
    let shown = Arc::new(Shown::new(slot, style, cell_height).flows_own().spinning());
    Some(Image::model(px.max(1), px.max(1), shown))
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_graphics_apis_logos_fit_one_row_in_whole_cells_unstretched() {
        assert!(super::api("Software").is_none(), "none for another");
        for name in ["Vulkan", "OpenGL", "Metal", "Direct3D 12"] {
            let logo = super::api(name).expect(name);
            assert_eq!(logo.name, name);
            for cell in [(8.0, 18.0), (16.4, 36.0), (7.3, 15.0)] {
                let (image, cols) = logo.in_row(cell);
                // Its cells' pixels, within one of their edge.
                assert_eq!(image.height, (cell.1 as f32).round() as u32);
                assert!((image.width as f32 - cols as f32 * cell.0).abs() <= 1.0, "{name} {cell:?}");
                // As wide as its artwork is for its height: never stretched.
                let tall = (cell.1 * 0.86).round();
                let wide = (logo.width as f32 * tall / logo.height as f32).round();
                assert!(wide <= image.width as f32 && wide > (cols as f32 - 1.0) * cell.0, "{name} {cell:?}");
                // Something drawn, and clear at the far right and the corners.
                assert!(image.pixels.as_chunks::<4>().0.iter().any(|p| p[3] > 200));
                assert_eq!(image.pixels[3], 0);
            }
        }
    }

    #[test]
    fn directx_takes_the_texts_ink_and_the_others_keep_their_own_colors() {
        let ink = [0x80, 0xC0, 0xFF];
        // DirectX's black wordmark: each pixel in the text's ink,
        // premultiplied by its alpha, shaped as before.
        let black = super::api("Direct3D 12").unwrap();
        assert!(black.pixels.as_chunks::<4>().0.iter().all(|p| p[..3] == [0, 0, 0]), "black as Commons draws it");
        let inked = super::api("Direct3D 12").unwrap().in_ink(ink);
        assert_eq!(black.pixels.len(), inked.pixels.len());
        let solid = inked.pixels.as_chunks::<4>().0.iter().find(|p| p[3] == 255).copied().expect("a solid pixel");
        assert_eq!(solid, [0x80, 0xC0, 0xFF, 255]);
        for (b, i) in black.pixels.as_chunks::<4>().0.iter().zip(inked.pixels.as_chunks::<4>().0) {
            assert_eq!((b[3], i[0] as u16), (i[3], ink[0] as u16 * i[3] as u16 / 255));
        }
        // Vulkan's red stays red.
        let vulkan = super::api("Vulkan").unwrap();
        assert_eq!(vulkan.pixels.clone(), super::api("Vulkan").unwrap().in_ink(ink).pixels);
    }

    use super::*;

    #[test]
    fn the_kickstarts_picture_is_its_own_four_colors_each_pixel_kept_square() {
        let small = kickstart(1.0).unwrap();
        assert_eq!((small.width, small.height), (130, 131));
        let colors: std::collections::BTreeSet<[u8; 4]> = small.pixels.chunks(4).map(|p| [p[0], p[1], p[2], p[3]]).collect();
        let four = [[0, 0, 0, 255], [0x77, 0x77, 0xCC, 255], [0xBB, 0xBB, 0xBB, 255], [255, 255, 255, 255]];
        assert!(colors.iter().eq(four.iter()), "{colors:?}");
        let at = |image: &Image, x: u32, y: u32| image.pixels[((y * image.width + x) * 4) as usize..][..3].to_vec();
        // The disk's violet at its top left, its shutter's grey, and the
        // white page round it; each a block of 7 × 7 at seven times.
        let large = kickstart(900.0).unwrap();
        assert_eq!((large.width, large.height), (910, 917));
        for (x, y, rgb) in [(30, 10, [0x77, 0x77, 0xCC]), (60, 10, [0xBB, 0xBB, 0xBB]), (2, 2, [255, 255, 255])] {
            assert_eq!(at(&small, x, y), rgb, "at {x}, {y}");
            assert!((0..7).all(|dy| (0..7).all(|dx| at(&large, x * 7 + dx, y * 7 + dy) == rgb)));
        }
    }

    #[test]
    fn a_chip_names_its_maker() {
        assert_eq!(maker("Intel(R) Core(TM) i7-8700K CPU @ 3.70GHz"), Some(Maker::Intel));
        assert_eq!(maker("AMD Ryzen 9 7950X 16-Core Processor"), Some(Maker::Amd));
        assert_eq!(maker("Apple M3 Pro"), None);
    }

    #[test]
    fn suns_emblem_stands_on_its_corner_in_its_purple() {
        let image = sun(128);
        let px = |x: u32, y: u32| &image.pixels[((y * 128 + x) * 4) as usize..][..4];
        // The corners of the picture lie outside the diamond.
        assert!([(0, 0), (127, 0), (0, 127), (127, 127)].iter().all(|(x, y)| px(*x, *y)[3] == 0));
        let drawn = image.pixels.chunks(4).filter(|p| p[3] > 128).count();
        let diamond = 128 * 128 / 2;
        assert!(drawn > diamond / 2 && drawn < diamond * 9 / 10, "{drawn} of {diamond}: woven, with gaps");
        // A quarter turn shows the same.
        let turned = |x: u32, y: u32| px(127 - y, x)[3];
        let same = (0..128).flat_map(|y| (0..128).map(move |x| (x, y))).filter(|(x, y)| (px(*x, *y)[3] > 128) == (turned(*x, *y) > 128)).count();
        assert!(same > 128 * 128 * 95 / 100, "{same}");
        // Wholly covered pixels are the purple itself.
        let solid = image.pixels.chunks(4).find(|p| p[3] == 255).expect("a solid pixel");
        assert_eq!(solid, [0x5d, 0x4a, 0x9f, 255]);
    }

    #[test]
    fn the_window_systems_logos_keep_their_colors_but_x11s_takes_the_ink() {
        let ink = [0xE4, 0xE9, 0xF2];
        let wayland = windowing("Wayland", 32, ink).expect("Wayland's");
        let x11 = windowing("X11", 32, ink).expect("X11's");
        assert!(windowing("Quartz", 32, ink).is_none());
        // Wayland's disc is yellow where its white W leaves it.
        let count = |test: fn(&[u8]) -> bool| wayland.pixels.chunks(4).filter(|p| test(p)).count();
        let yellow = count(|p| p[3] == 255 && p[0] > 240 && p[1] > 170 && p[1] < 205 && p[2] < 20);
        let white = count(|p| p[3] == 255 && p.iter().all(|&c| c > 240));
        assert!(yellow > 32 * 32 / 4 && white > 32 * 32 / 10, "{yellow} yellow, {white} white");
        // X11's X is the ink wherever it's drawn, premultiplied, its arms
        // crossing in the middle and clear between them.
        let at = |x: u32, y: u32| x11.pixels[((y * 32 + x) * 4 + 3) as usize];
        assert!(at(16, 16) > 128 && at(16, 1) == 0 && at(16, 30) == 0, "{} {} {}", at(16, 16), at(16, 1), at(16, 30));
        assert!(x11.pixels.chunks(4).filter(|p| p[3] > 0).all(|p| (0..3).all(|c| (p[c] as i32 - ink[c] as i32 * p[3] as i32 / 255).abs() <= 1)));
        assert!(x11.pixels.chunks(4).filter(|p| p[3] == 255).count() > 32 * 32 / 5);
    }

    #[test]
    fn sgis_cube_reads_as_one_mesh_as_wide_as_it_is_tall() {
        let mesh = sgi_cube().expect("the cube's mesh");
        let triangles: usize = mesh.parts.iter().map(|p| p.indices.len() / 3).sum();
        assert_eq!(triangles, 16466);
        assert!(mesh.extent.iter().all(|e| (*e - mesh.extent[0]).abs() < 0.05), "a cube: {:?}", mesh.extent);
        assert!(sgi(64, 20.0).is_some_and(|i| i.model.is_some()));
    }

    /// Which of a `GRID` square's cells the cube's vertices fall in, placed
    /// as `sgi` stands it at rest and turned a further `degrees` about the
    /// middle of the picture.
    fn sgi_cells(rotation: [f32; 3], degrees: f32) -> Vec<bool> {
        const GRID: usize = 40;
        let mesh = sgi_cube().unwrap();
        let shown = Shown::new(Arc::new(Slot::default()), Style { rotation, ..Style::default() }, 20.0);
        let m = shown.transform(&mesh, (1.0, 1.0), crate::model::Turn::REST);
        let (s, c) = degrees.to_radians().sin_cos();
        let mut cells = vec![false; GRID * GRID];
        for v in mesh.parts.iter().flat_map(|p| &p.vertices) {
            let [x, y, z] = v.pos;
            let (px, py) = (m[0][0] * x + m[1][0] * y + m[2][0] * z, m[0][1] * x + m[1][1] * y + m[2][1] * z);
            let (px, py) = (c * px - s * py, s * px + c * py);
            let cell = |t: f32| ((t + 0.5) * GRID as f32).floor().clamp(0.0, GRID as f32 - 1.0) as usize;
            cells[cell(py) * GRID + cell(px)] = true;
        }
        cells
    }

    #[test]
    fn sgis_cube_stands_at_rest_as_its_logo_down_the_diagonal_its_tubes_turn_about() {
        // Turned a third of the way round, the cube at rest covers the same
        // cells, as the logo's threefold tubes do; the pose it stood in
        // before, down another diagonal, doesn't.
        let alike = |rotation: [f32; 3]| {
            let (rest, turned) = (sgi_cells(rotation, 0.0), sgi_cells(rotation, 120.0));
            let both = rest.iter().zip(&turned).filter(|(a, b)| **a && **b).count();
            let either = rest.iter().zip(&turned).filter(|(a, b)| **a || **b).count();
            both as f32 / either as f32
        };
        assert!(alike(SGI_REST) > 0.9, "{}", alike(SGI_REST));
        assert!(alike([35.26, 45.0, 0.0]) < alike(SGI_REST) - 0.1, "{} {}", alike([35.26, 45.0, 0.0]), alike(SGI_REST));
        // A point at the top and the bottom, so its sides stand upright: the
        // tube down its leftmost edge runs half its height, where a
        // hexagon lying on a side would meet it in a corner.
        let cells = sgi_cells(SGI_REST, 0.0);
        let filled: Vec<(usize, usize)> = (0..40 * 40).filter(|&i| cells[i]).map(|i| (i % 40, i / 40)).collect();
        let left = filled.iter().map(|c| c.0).min().unwrap();
        let rows = |near: &dyn Fn(usize) -> bool| -> Vec<usize> { filled.iter().filter(|c| near(c.0)).map(|c| c.1).collect() };
        let span = |r: Vec<usize>| (r.iter().max().unwrap() - r.iter().min().unwrap() + 1) as f32;
        let (side, tall) = (span(rows(&|x| x <= left + 1)), span(rows(&|_| true)));
        assert!(side / tall > 0.4, "{side} of {tall}");
        // The right way up: the stance it had, upside down, turned half
        // round in the picture.
        assert_eq!(cells, sgi_cells([-35.264, -45.0, 0.0], 180.0));
    }

    #[test]
    fn sgis_cube_turns_as_flows_own_which_efficient_mode_holds_still() {
        let logo = sgi(64, 20.0).expect("the logo");
        let shown = logo.model.as_ref().expect("a model");
        assert!(shown.flows_own && shown.animating());
        shown.hold(true, std::time::Instant::now());
        assert!(!shown.animating(), "no frames kept coming while it's held");
    }

    #[test]
    fn the_boing_ball_is_round_and_checkered_red_and_white_half_and_half() {
        let ball = boing_ball().expect("the ball");
        assert_eq!(ball.parts.len(), 2);
        assert_eq!((ball.parts[0].material.color, ball.parts[1].material.color), (BOING_RED, BOING_WHITE));
        // As many checks of each, every vertex on the sphere.
        let quads = |p: &Part| p.indices.len() / 6;
        assert_eq!((quads(&ball.parts[0]), quads(&ball.parts[1])), (BOING_AROUND * BOING_DOWN * BOING_FINE * BOING_FINE / 2, BOING_AROUND * BOING_DOWN * BOING_FINE * BOING_FINE / 2));
        for v in ball.parts.iter().flat_map(|p| &p.vertices) {
            let r = v.pos.iter().map(|c| c * c).sum::<f32>().sqrt();
            assert!((r - 0.5).abs() < 1e-4, "{r}");
        }
        assert!(ball.extent.iter().all(|e| (e - 1.0).abs() < 1e-3), "{:?}", ball.extent);
        // Next to each other just above its middle, the checks take turns.
        let red_at = |lon: f32| {
            let near = |p: &Part| p.vertices.iter().any(|v| (0.03..0.07).contains(&v.pos[1]) && (v.pos[2].atan2(v.pos[0]) + lon).abs() < 0.05);
            near(&ball.parts[0]) && !near(&ball.parts[1])
        };
        let check = std::f32::consts::TAU / BOING_AROUND as f32;
        assert_ne!(red_at(check * 0.5), red_at(check * 1.5));
        // It spins on its own, as Flow's own.
        let logo = boing(64, 20.0).expect("the logo");
        let shown = logo.model.as_ref().expect("a model");
        assert!(shown.flows_own && shown.animating());
    }
}
