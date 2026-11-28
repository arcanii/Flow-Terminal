//! wgpu renderer: liquid simulation, glass background and 2D quads.

pub mod atlas;
pub mod capture;
mod crt;
pub mod fluid;
mod models;
pub mod palette;
pub mod rain;
mod timing;

use std::borrow::Cow;
use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use bytemuck::{Pod, Zeroable};
use winit::window::Window;

use atlas::{Atlas, CellMetrics, GlyphKey};
use crt::{CrtFrame, CrtLayer};
pub use crt::{unwarp as crt_unwarp, warp as crt_warp, CrtLook};
use fluid::Fluid;
use rain::{RainLayer, RainQuad};

use crate::box_drawing::{Cover, Piece};
use crate::graphics::Image;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug)]
pub struct Quad {
    pub rect: [f32; 4],
    pub uv: [f32; 4],
    pub color: [f32; 4],
    /// kind, corner radius (a bubble's tilt), border width / blur (a
    /// bubble's padding), ripple amount
    pub params: [f32; 4],
}

const KIND_SOLID: f32 = 0.0;
const KIND_MASK_GLYPH: f32 = 1.0;
const KIND_COLOR_GLYPH: f32 = 2.0;
const KIND_GLASS: f32 = 3.0;
const KIND_GLOW: f32 = 4.0;
const KIND_SEGMENT: f32 = 5.0;
const KIND_BUBBLE: f32 = 6.0;
const KIND_PICKER: f32 = 7.0;
/// A piece of an image a program showed, from its own texture.
const KIND_IMAGE: f32 = 8.0;
/// A piece of a line Flow draws itself (`crate::box_drawing`).
const KIND_STROKE: f32 = 9.0;

/// How long an image's texture outlives its last frame on screen.
const IMAGE_KEEP: Duration = Duration::from_secs(10);

/// Quads `range` of a frame are pieces of `image`, drawn from its texture
/// (an animation's frame from the animation's one, `Image::texture`).
pub struct ImageBatch {
    pub image: Arc<Image>,
    pub range: Range<u32>,
}

/// A picture's texture on the GPU, by `Image::texture`.
struct ImageTexture {
    view: wgpu::TextureView,
    /// When it was last drawn.
    used: Instant,
    /// The serial of the picture whose pixels it holds: for an animation,
    /// the frame last drawn, so it's written afresh only when another (or
    /// that frame edited, a picture of its own) is drawn in its place.
    holds: u64,
}

pub type Rgba = [f32; 4];

pub fn rgba(rgb: [u8; 3], a: f32) -> Rgba {
    [rgb[0] as f32 / 255.0, rgb[1] as f32 / 255.0, rgb[2] as f32 / 255.0, a]
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default)]
struct Globals {
    resolution: [f32; 2],
    time: f32,
    opacity: f32,
    corner_radius: f32,
    ripple: f32,
    focus: f32,
    activity: f32,
    tint: [f32; 4],
    accent: [f32; 4],
    accent2: [f32; 4],
    dye_texel: [f32; 2],
    dye_gain: f32,
    opaque: f32,
    caustics: f32,
    mode: f32,
    sun: f32,
    ui_scale: f32,
    flash: [f32; 4],
    wave_texel: [f32; 2],
    wet: f32,
    _pad1: f32,
    pulse: [f32; 4],
    pulse_color: [f32; 4],
    progress: [f32; 4],
    progress_color: [f32; 4],
    phosphor: [f32; 4],
}

/// Per-frame look of the window glass.
#[derive(Clone, Copy, Debug)]
pub struct FrameStyle {
    pub opacity: f32,
    pub ripple: f32,
    pub focus: f32,
    pub activity: f32,
    pub tint: Rgba,
    pub accent: Rgba,
    pub accent2: Rgba,
    pub dye_gain: f32,
    pub corner_radius: f32,
    pub caustics: f32,
    /// The liquid's own clock (advances at `flow_speed`), in seconds.
    pub flow_time: f32,
    /// Multiplier on how fast the liquid moves; 1.0 is normal.
    pub flow_speed: f32,
    /// 0 clouds, 1 water seen from above, 2 from below, 3 oil, 4 rain.
    pub mode: f32,
    /// Sunlight strength for the water styles, 0..1.
    pub sun: f32,
    /// Lightning lighting the clouds: x, y (0..1), intensity, radius.
    pub flash: [f32; 4],
    /// How wet the rain style's glass is all over, 0..1: in a downpour the
    /// water runs down it in sheets (`fx::rain::wetness`).
    pub wet: f32,
    /// DPI scale of the UI.
    pub ui_scale: f32,
    /// A ring spreading over the glass from where something happened: x, y
    /// (0..1), seconds since it began, strength (0: none).
    pub pulse: [f32; 4],
    /// The ring's color, and its width (a fraction of the window's height).
    pub pulse_color: [f32; 4],
    /// The progress tank: how full (0..1), how much it shows (0..1), 1 with a
    /// figure or 0 sloshing without one, and its clock (seconds).
    pub progress: [f32; 4],
    /// The tank's color.
    pub progress_color: [f32; 4],
    /// The one color everything is shown in, a monochrome tube's phosphor
    /// (`palette::on_phosphor`), its alpha 1; all 0 for full color.
    pub phosphor: Rgba,
    /// The CRT look; its strength 0 is off.
    pub crt: CrtLook,
}

/// Where a character drawn bigger goes ([`Canvas::glyph_scaled`]): the
/// top-left of its scaled cell, how many times its size it's drawn each way
/// (2 × 1 on a double-width line, 2 × 2 on a double-height one), and the
/// rows (top and bottom, in pixels) its line shows of it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scaled {
    pub at: (f32, f32),
    pub scale: (f32, f32),
    pub rows: (f32, f32),
}

/// Collects the quads for one frame and lays out monospace text.
pub struct Canvas<'a> {
    pub quads: Vec<Quad>,
    /// Pieces of images, drawn over the first `backdrop` of `quads` and
    /// under the rest.
    images: Vec<(Arc<Image>, Quad)>,
    backdrop: usize,
    /// Cells' backgrounds ([`Canvas::ground`]), drawn over the images and
    /// under the rest of `quads`, so what programs set off, drawn first
    /// among the rest, glows over a game's own ground, under its letters.
    grounds: Vec<Quad>,
    /// Pictures drawn among the UI, in its order (the About card's cube):
    /// the quad's index in `quads`, and the image.
    over: Vec<(usize, Arc<Image>)>,
    atlas: &'a mut Atlas,
    queue: &'a wgpu::Queue,
    pub cell: CellMetrics,
    /// How strong [`Canvas::glow`] draws, from the Glow setting (1 is normal).
    pub glow_level: f32,
    /// When what's drawn changes by itself, without a frame asked for: the
    /// soonest an animated picture shows its next frame ([`Canvas::changes_at`]).
    pub changes: Option<Instant>,
}

impl<'a> Canvas<'a> {
    /// Notes that what's drawn changes by itself at `at` (an animated
    /// picture's next frame), so a frame is due then.
    pub fn changes_at(&mut self, at: Option<Instant>) {
        if let Some(at) = at {
            self.changes = Some(self.changes.map_or(at, |soonest| soonest.min(at)));
        }
    }

    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Rgba, radius: f32) {
        self.quads.push(Quad { rect: [x, y, w, h], uv: [0.0; 4], color, params: [KIND_SOLID, radius, 0.0, 0.0] });
    }

    /// A terminal cell's background over x, y, w, h. However late it's laid,
    /// it's drawn just over the images, under the effects programs ask for
    /// and everything else.
    pub fn ground(&mut self, x: f32, y: f32, w: f32, h: f32, color: Rgba) {
        self.grounds.push(Quad { rect: [x, y, w, h], uv: [0.0; 4], color, params: [KIND_SOLID, 0.0, 0.0, 0.0] });
    }

    pub fn outline(&mut self, x: f32, y: f32, w: f32, h: f32, color: Rgba, radius: f32, width: f32) {
        self.quads.push(Quad { rect: [x, y, w, h], uv: [0.0; 4], color, params: [KIND_SOLID, radius, width, 0.0] });
    }

    pub fn glass(&mut self, x: f32, y: f32, w: f32, h: f32, tint: Rgba, radius: f32) {
        self.quads.push(Quad { rect: [x, y, w, h], uv: [0.0; 4], color: tint, params: [KIND_GLASS, radius, 0.0, 0.0] });
    }

    /// A soft glow, as strong as the Glow setting makes it.
    pub fn glow(&mut self, x: f32, y: f32, w: f32, h: f32, color: Rgba, radius: f32, blur: f32) {
        let [r, g, b, a] = color;
        self.steady_glow([x, y, w, h], [r, g, b, (a * self.glow_level).min(1.0)], radius, blur);
    }

    /// A glow over `rect` (x, y, w, h) that the Glow setting leaves alone:
    /// shadows, and signals that must show however low it's set.
    pub fn steady_glow(&mut self, rect: [f32; 4], color: Rgba, radius: f32, blur: f32) {
        self.quads.push(Quad { rect, uv: [0.0; 4], color, params: [KIND_GLOW, radius, blur, 0.0] });
    }

    /// The color picker's field over `rect` (x, y, w, h): saturation across
    /// and brightness up, at `hue` (0..1).
    pub fn color_field(&mut self, rect: [f32; 4], hue: f32, radius: f32) {
        self.quads.push(Quad { rect, uv: [hue, 0.0, 0.0, 0.0], color: [1.0; 4], params: [KIND_PICKER, radius, 0.0, 0.0] });
    }

    /// The color picker's hue bar over `rect`, from red round to red.
    pub fn hue_bar(&mut self, rect: [f32; 4], radius: f32) {
        self.quads.push(Quad { rect, uv: [0.0, 1.0, 0.0, 0.0], color: [1.0; 4], params: [KIND_PICKER, radius, 0.0, 0.0] });
    }

    /// A glowing line from a to b (pixels): white-hot core, `color` glow.
    pub fn segment(&mut self, a: [f32; 2], b: [f32; 2], color: Rgba, width: f32, glow: f32) {
        self.quads.push(Quad { rect: [a[0], a[1], b[0], b[1]], uv: [0.0; 4], color, params: [KIND_SEGMENT, width, glow, 0.0] });
    }

    /// An air bubble in water around `center` (pixels): an ellipse with
    /// half-axes `half`, turned `tilt` radians clockwise, lit from `light` (a
    /// unit vector on screen). `open` runs 0..1 as it pops. `color` is the
    /// light it mirrors and its opacity.
    pub fn bubble(&mut self, center: [f32; 2], half: [f32; 2], tilt: f32, light: [f32; 2], open: f32, color: Rgba) {
        // The quad grows on every side to hold the turned ellipse and its
        // smoothed edge.
        let (a, b) = (half[0], half[1]);
        let (sin, cos) = tilt.sin_cos();
        let reach_x = (a * a * cos * cos + b * b * sin * sin).sqrt() - a;
        let reach_y = (a * a * sin * sin + b * b * cos * cos).sqrt() - b;
        let pad = reach_x.max(reach_y).max(0.0) + 1.5;
        self.quads.push(Quad {
            rect: [center[0] - a, center[1] - b, a * 2.0, b * 2.0],
            uv: [open, light[0], light[1], 0.0],
            color,
            params: [KIND_BUBBLE, tilt, pad, 0.0],
        });
    }

    /// A piece of a box-drawing character (`crate::box_drawing`), shown
    /// within `rows` (top and bottom) where a double-height line shows half
    /// of it. With `ripple` it bends with the liquid by its corners, as a
    /// picture's pieces do, so pieces that share a side stay joined.
    pub fn stroke(&mut self, piece: &Piece, rows: Option<(f32, f32)>, color: Rgba, ripple: f32) {
        let [x, y, w, h] = piece.quad;
        let (top, bottom) = rows.map_or((y, y + h), |(a, b)| (y.max(a), (y + h).min(b)));
        if bottom <= top || w <= 0.0 {
            return;
        }
        // The shader's figures are from the quad's top left.
        let (radius, half, seg) = match piece.cover {
            Cover::Bar([x0, y0, x1, y1]) => (0.0, 0.0, [x0 - x, y0 - top, x1 - x, y1 - top]),
            Cover::Ring { center, radius, half, squash } => (radius, half, [center[0] - x, center[1] - top, squash[0], squash[1]]),
            Cover::Line { from, to, half } => (-1.0, half, [from[0] - x, from[1] - top, to[0] - x, to[1] - top]),
        };
        self.quads.push(Quad { rect: [x, top, w, bottom - top], uv: seg, color, params: [KIND_STROKE, radius, half, ripple] });
    }

    /// A segment from `a` to `b` (pixels) with round ends, `half` its
    /// thickness either side, smoothed over a pixel: Flow's own small
    /// drawings, such as the tab sidebar's pin, turned any way.
    pub fn capsule(&mut self, a: [f32; 2], b: [f32; 2], half: f32, color: Rgba) {
        let pad = half + 1.0;
        let (x, y) = (a[0].min(b[0]) - pad, a[1].min(b[1]) - pad);
        let (w, h) = ((a[0] - b[0]).abs() + pad * 2.0, (a[1] - b[1]).abs() + pad * 2.0);
        self.quads.push(Quad { rect: [x, y, w, h], uv: [a[0] - x, a[1] - y, b[0] - x, b[1] - y], color, params: [KIND_STROKE, -2.0, half, 0.0] });
    }

    /// Draws one character whose cell's top-left corner is (x, y).
    pub fn glyph(&mut self, x: f32, y: f32, ch: char, color: Rgba, bold: bool, italic: bool, ripple: f32) {
        self.letter(x, y, (ch, 0), color, (bold, italic), ripple);
    }

    /// The id that draws `ch` with its combining marks (`Atlas::marked`),
    /// for [`Canvas::letter`] and [`Canvas::glyph_scaled`]; 0 draws it alone.
    pub fn marks(&mut self, ch: char, marks: &[char]) -> u32 {
        if marks.is_empty() {
            return 0;
        }
        self.atlas.marked(ch, marks)
    }

    /// One character with its marks (an id from [`Canvas::marks`]), its
    /// cell's top-left corner at (x, y).
    pub fn letter(&mut self, x: f32, y: f32, (ch, marks): (char, u32), color: Rgba, (bold, italic): (bool, bool), ripple: f32) {
        let Some(g) = self.atlas.glyph(self.queue, GlyphKey { ch, bold, italic, size: 0, marks }) else { return };
        let kind = if g.color { KIND_COLOR_GLYPH } else { KIND_MASK_GLYPH };
        self.quads.push(Quad {
            rect: [(x + g.offset[0]).round(), (y + g.offset[1]).round(), g.size[0], g.size[1]],
            uv: g.uv,
            color,
            params: [kind, 0.0, 0.0, ripple],
        });
    }

    /// One character drawn bigger, as `place` says: the half of a
    /// double-height character its line holds, say. It's drawn from the
    /// glyph at the larger scale, squashed to the smaller, which is sharper
    /// than the small one stretched.
    pub fn glyph_scaled(&mut self, (ch, marks): (char, u32), color: Rgba, (bold, italic): (bool, bool), place: Scaled, ripple: f32) {
        let Scaled { at: (x, y), scale, rows } = place;
        let px = (self.cell.font_size * scale.0.max(scale.1)).round().clamp(1.0, 400.0);
        let Some(g) = self.atlas.glyph(self.queue, GlyphKey { ch, bold, italic, size: px as u16, marks }) else { return };
        let k = px / self.cell.font_size;
        let (kx, ky) = (scale.0 / k, scale.1 / k);
        let (left, top) = ((x + g.offset[0] * kx).round(), (y + g.offset[1] * ky).round());
        let (h, [u0, v0, u1, v1]) = (g.size[1] * ky, g.uv);
        let (y0, y1) = (top.max(rows.0), (top + h).min(rows.1));
        if y1 <= y0 {
            return;
        }
        let v = |at: f32| v0 + (at - top) / h * (v1 - v0);
        let kind = if g.color { KIND_COLOR_GLYPH } else { KIND_MASK_GLYPH };
        self.quads.push(Quad { rect: [left, y0, g.size[0] * kx, y1 - y0], uv: [u0, v(y0), u1, v(y1)], color, params: [kind, 0.0, 0.0, ripple] });
    }

    /// Lays out a single line on the cell grid; returns the x after the last char.
    pub fn text(&mut self, x: f32, y: f32, s: &str, color: Rgba, bold: bool) -> f32 {
        let mut cx = x;
        for ch in s.chars() {
            let advance = self.advance(ch);
            if advance > 0.0 {
                self.glyph(cx, y, ch, color, bold, false, 0.0);
                cx += advance;
            }
        }
        cx
    }

    pub fn text_width(&self, s: &str) -> f32 {
        s.chars().map(|c| self.advance(c)).sum()
    }

    /// How far a character moves the pen in the interface's text: its cells,
    /// except Chinese, Japanese and Korean, which are 1em wide in their
    /// fonts and would look spaced out over the terminal's two cells.
    fn advance(&self, ch: char) -> f32 {
        crate::text::advance(ch, self.cell.width, self.cell.font_size)
    }

    /// Width of one cell at font size `px`.
    pub fn cell_width_at(&self, px: f32) -> f32 {
        self.cell.width * px / self.cell.font_size
    }

    /// One character at font size `px`, top-left of its cell at (x, y).
    pub fn glyph_sized(&mut self, x: f32, y: f32, ch: char, px: f32, color: Rgba, bold: bool) {
        let key = GlyphKey { ch, bold, italic: false, size: px.round().clamp(1.0, 400.0) as u16, marks: 0 };
        let Some(g) = self.atlas.glyph(self.queue, key) else { return };
        let kind = if g.color { KIND_COLOR_GLYPH } else { KIND_MASK_GLYPH };
        self.quads.push(Quad {
            rect: [(x + g.offset[0]).round(), (y + g.offset[1]).round(), g.size[0], g.size[1]],
            uv: g.uv,
            color,
            params: [kind, 0.0, 0.0, 0.0],
        });
    }

    /// A piece of an image over `rect`, `uv` (0..1) of it. Its corners ride
    /// the liquid each by the flow where it is, so the pieces of an image, and
    /// images side by side, stay joined.
    pub fn image_tile(&mut self, image: &Arc<Image>, rect: [f32; 4], uv: [f32; 4], ripple: f32) {
        let quad = Quad { rect, uv, color: [1.0; 4], params: [KIND_IMAGE, 0.0, 0.0, ripple] };
        self.images.push((image.clone(), quad));
    }

    /// `image` whole over x, y, w, h, drawn in its turn with the UI rather
    /// than under it, faded by `alpha`.
    pub fn image_over(&mut self, image: &Arc<Image>, (x, y, w, h): (f32, f32, f32, f32), alpha: f32) {
        self.over.push((self.quads.len(), image.clone()));
        self.quads.push(Quad { rect: [x, y, w, h], uv: [0.0, 0.0, 1.0, 1.0], color: [1.0, 1.0, 1.0, alpha], params: [KIND_IMAGE, 0.0, 0.0, 0.0] });
    }

    /// The quads so far are the backdrop (the liquid's bubbles and
    /// lightning): images and cells' backgrounds go over them, under what's
    /// drawn after.
    /// Draws in the first installed family of `families` at `font_px` from
    /// here on, its cell `cell`: a split tab's pane in a font of its own,
    /// and back after. The atlas keeps each face and its glyphs.
    pub fn use_font(&mut self, (families, font_px, line_height): &(Vec<String>, f32, f32)) {
        self.atlas.set_font(families, *font_px, *line_height);
        self.cell = self.atlas.metrics;
    }

    pub fn end_backdrop(&mut self) {
        self.backdrop = self.quads.len();
    }

    /// The frame's quads in the order they're drawn ([`stack`]), and which of
    /// them each image draws.
    pub fn finish(&mut self) -> (Vec<Quad>, Vec<ImageBatch>) {
        let (quads, tiles) = (std::mem::take(&mut self.quads), std::mem::take(&mut self.images));
        stack(quads, self.backdrop, tiles, std::mem::take(&mut self.grounds), std::mem::take(&mut self.over))
    }

    /// A registered picture (e.g. the app icon) stretched to w×h.
    pub fn image(&mut self, id: u32, x: f32, y: f32, w: f32, h: f32, alpha: f32) {
        let Some(g) = self.atlas.image(self.queue, id) else { return };
        self.quads.push(Quad {
            rect: [x, y, w, h],
            uv: g.uv,
            color: [1.0, 1.0, 1.0, alpha],
            params: [KIND_COLOR_GLYPH, 0.0, 0.0, 0.0],
        });
    }
}

/// A frame's quads in the order they're drawn: of `quads`, the first
/// `backdrop` (the liquid's bubbles and lightning), then the images' pieces
/// `tiles` grouped by image, then the cells' `grounds`, then the rest of
/// `quads` (programs' effects first, then the UI), with the pictures `over`
/// drawn among it at the places they were laid; and which of them each image
/// draws, in order.
fn stack(mut quads: Vec<Quad>, backdrop: usize, mut tiles: Vec<(Arc<Image>, Quad)>, grounds: Vec<Quad>, over: Vec<(usize, Arc<Image>)>) -> (Vec<Quad>, Vec<ImageBatch>) {
    // By texture: an animation's pieces show the one frame due, drawn
    // together from its texture.
    tiles.sort_by_key(|(image, _)| image.texture());
    let backdrop = backdrop.min(quads.len());
    let rest = quads.split_off(backdrop);
    quads.reserve(tiles.len() + grounds.len() + rest.len());
    let mut batches: Vec<ImageBatch> = Vec::new();
    for (image, quad) in tiles {
        let n = quads.len() as u32;
        match batches.last_mut() {
            Some(b) if b.image.texture() == image.texture() => b.range.end = n + 1,
            _ => batches.push(ImageBatch { image, range: n..n + 1 }),
        }
        quads.push(quad);
    }
    quads.extend(grounds);
    // The rest moves along by the tiles and grounds put before it.
    let shift = quads.len() - backdrop;
    for (at, image) in over {
        let n = (at.max(backdrop) + shift) as u32;
        batches.push(ImageBatch { image, range: n..n + 1 });
    }
    quads.extend(rest);
    (quads, batches)
}

/// Id of the app icon picture in the atlas.
pub const ICON_IMAGE: u32 = 1;

/// The 256 px app icon, premultiplied, for the About dialog.
pub(crate) fn icon_rgba() -> Option<(Vec<u8>, u32, u32)> {
    let bytes: &[u8] = include_bytes!("../../../assets/icon/flow-256.png");
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0u8; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        return None;
    }
    buf.truncate(info.buffer_size());
    for px in buf.chunks_exact_mut(4) {
        let a = px[3] as u16;
        for c in &mut px[..3] {
            *c = (*c as u16 * a / 255) as u8;
        }
    }
    Some((buf, info.width, info.height))
}

/// What became of a frame [`Renderer::render`] was asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shown {
    /// It reached the screen.
    Yes,
    /// The window can't be seen, so there was nothing to draw on: on macOS
    /// wgpu hands out no texture while the window isn't visible, which a
    /// window just shown isn't yet. Draw it again once it can be seen.
    Hidden,
    /// The surface had to be set up again, or gave no texture in time: draw
    /// it again at the next frame's time.
    Again,
}

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pub transparent: bool,
    /// No GPU was found, so frames are drawn on the CPU (Windows' Basic
    /// Render Driver, in a VM without Direct3D 12 for one) and each costs
    /// tens of milliseconds.
    pub software: bool,
    /// The graphics API frames are drawn with ("Metal", "Direct3D 12").
    pub backend: String,
    pub fluid: Fluid,
    pub atlas: Atlas,
    /// The rain style's drops for the next frame (from the effects).
    pub rain: Vec<RainQuad>,
    rain_layer: RainLayer,
    crt: CrtLayer,
    layout: wgpu::BindGroupLayout,
    glass_pipeline: wgpu::RenderPipeline,
    quad_pipeline: wgpu::RenderPipeline,
    globals: wgpu::Buffer,
    sampler: wgpu::Sampler,
    instances: wgpu::Buffer,
    instance_capacity: usize,
    start: Instant,
    last_frame: Instant,
    /// `--record`: each frame advances exactly this many seconds, however
    /// long it takes to draw, and `stepped` adds them up in place of `start`.
    fixed_step: Option<f32>,
    stepped: f32,
    /// Textures of the images programs showed, by [`Image::texture`]: an
    /// animation's one holds whichever of its frames was last drawn.
    image_textures: HashMap<u64, ImageTexture>,
    /// The 3D models programs placed, drawn into their pictures' textures.
    models: models::Models,
    /// How long the GPU takes over frames, while what shows it shows.
    clock: timing::GpuClock,
    /// Whether frames can be copied from the surface, which a recording
    /// needs (Metal, Direct3D 12 and Vulkan can; an OpenGL surface may not).
    pub can_record: bool,
    /// While recording: the buffers its frames come back through.
    capture: Option<capture::Capture>,
}

/// What a texture `size` pixels big in `format` holds on the GPU.
fn texture_bytes((w, h): (u32, u32), format: wgpu::TextureFormat) -> u64 {
    w as u64 * h as u64 * format.block_copy_size(None).unwrap_or(4) as u64
}

impl Renderer {
    /// What the window holds on the GPU for itself, apart from programs'
    /// pictures: the glyph atlas, the liquid's textures, the CRT's and the
    /// rain's.
    pub fn own_gpu_bytes(&self) -> u64 {
        self.atlas.gpu_bytes() + self.fluid.gpu_bytes() + self.crt.gpu_bytes() + self.rain_layer.gpu_bytes() + self.capture.as_ref().map_or(0, |c| c.gpu_bytes())
    }

    /// What `images` (by `Image::texture`, with their sizes) hold on the
    /// GPU: those drawn lately enough to have a texture there, four bytes a
    /// pixel (an animation's one frame's worth), and for a 3D model's
    /// picture what its mesh and its drawing hold beside it.
    pub fn images_gpu_bytes(&self, images: &[(u64, u32, u32)]) -> u64 {
        images
            .iter()
            .filter(|(serial, _, _)| self.image_textures.contains_key(serial))
            .map(|&(serial, w, h)| w as u64 * h as u64 * 4 + self.models.gpu_bytes(serial))
            .sum()
    }

    /// `instance` comes from the platform, which knows what gives its
    /// windows a transparent surface.
    pub fn new(
        window: Arc<Window>,
        mut instance: wgpu::InstanceDescriptor,
        families: &[String],
        font_px: f32,
        line_height: f32,
    ) -> Result<Self> {
        let size = window.inner_size();
        // OpenGL (Linux without Vulkan) can only draw to windows of the
        // display it's given; Vulkan, Metal and DX12 don't use it.
        if instance.display.is_none() {
            instance.display = Some(Box::new(window.clone()));
        }
        let instance = wgpu::Instance::new(instance);
        let surface = instance.create_surface(window.clone()).context("creating GPU surface")?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
            apply_limit_buckets: false,
        }))
        .map_err(|e| anyhow!("no suitable GPU adapter: {e}"))?;
        let info = adapter.get_info();
        let software = info.device_type == wgpu::DeviceType::Cpu;
        log::info!("GPU: {} ({:?}){}", info.name, info.backend, if software { ", drawing on the CPU" } else { "" });
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("flow"),
            required_limits: limits(&adapter),
            // Timestamps where the adapter has them, for the frame's time
            // on the GPU (About's Resources).
            required_features: adapter.features() & wgpu::Features::TIMESTAMP_QUERY,
            memory_hints: wgpu::MemoryHints::MemoryUsage,
            ..Default::default()
        }))
        .context("creating GPU device")?;

        let caps = surface.get_capabilities(&adapter);
        let can_record = caps.usages.contains(wgpu::TextureUsages::COPY_SRC);
        // Eight bits a channel, not sRGB. Wayland drivers can list half
        // floats and 10-bit formats first, whose alpha is wider than needed
        // or only 2 bits (four steps of glass), and snapshots read back 4
        // bytes a pixel.
        let format = [wgpu::TextureFormat::Bgra8Unorm, wgpu::TextureFormat::Rgba8Unorm]
            .into_iter()
            .find(|f| caps.formats.contains(f))
            .or_else(|| caps.formats.iter().copied().find(|f| !f.is_srgb()))
            .unwrap_or(caps.formats[0]);
        let alpha_mode = [wgpu::CompositeAlphaMode::PreMultiplied, wgpu::CompositeAlphaMode::PostMultiplied]
            .into_iter()
            .find(|m| caps.alpha_modes.contains(m));
        let transparent = alpha_mode.is_some();
        log::debug!("surface formats {:?}, alpha {:?}", caps.formats, caps.alpha_modes);
        if !transparent {
            log::warn!("surface has no alpha support ({:?}); drawing opaque glass", caps.alpha_modes);
        }
        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .ok_or_else(|| anyhow!("surface not supported by adapter"))?;
        config.format = format;
        config.alpha_mode = alpha_mode.unwrap_or(caps.alpha_modes[0]);
        config.present_mode = wgpu::PresentMode::AutoVsync;
        // How many finished frames may wait for the screen. While frames run
        // continuously (a liquid style, the CRT), each one waiting puts what
        // was just typed a frame later, so Windows keeps one. On the Mac one
        // means two Metal drawables instead of three, which can drop frames
        // at 120 Hz under the heavier effects, so it keeps two there.
        config.desired_maximum_frame_latency = if cfg!(target_os = "macos") { 2 } else { 1 };
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ui"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/ui.wgsl").into()),
        });
        let tex_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                tex_entry(1),
                tex_entry(2),
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                tex_entry(4),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ui"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let blend = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
        };
        let target = [Some(wgpu::ColorTargetState { format, blend: Some(blend), write_mask: wgpu::ColorWrites::ALL })];
        let glass_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("glass"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_full"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_glass"),
                compilation_options: Default::default(),
                targets: &target,
            }),
            multiview_mask: None,
            cache: None,
        });
        let quad_attrs = wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32x4];
        let quad_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("quads"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_quad"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Quad>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &quad_attrs,
                })],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_quad"),
                compilation_options: Default::default(),
                targets: &target,
            }),
            multiview_mask: None,
            cache: None,
        });
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ui"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let instance_capacity = 4096;
        let instances = create_instance_buffer(&device, instance_capacity);
        let fluid = Fluid::new(&device, config.width, config.height);
        let rain_layer = RainLayer::new(&device);
        // Models are multisampled where the formats are sure to allow it.
        let msaa = [wgpu::TextureFormat::Rgba8Unorm, wgpu::TextureFormat::Depth32Float]
            .iter()
            .all(|f| f.guaranteed_format_features(device.features()).flags.sample_count_supported(4));
        let models = models::Models::new(&device, &queue, if msaa { 4 } else { 1 });
        let crt = CrtLayer::new(&device, format);
        let clock = timing::GpuClock::new(&device, &queue);
        let mut atlas = Atlas::new(&device, families, font_px, line_height);
        match icon_rgba() {
            Some((rgba, w, h)) => atlas.register_image(ICON_IMAGE, rgba, w, h),
            None => log::warn!("could not decode the app icon"),
        }
        let now = Instant::now();
        Ok(Self {
            surface,
            device,
            queue,
            config,
            transparent,
            software,
            backend: match info.backend {
                wgpu::Backend::Dx12 => "Direct3D 12".to_string(),
                wgpu::Backend::Gl => "OpenGL".to_string(),
                other => format!("{other:?}"),
            },
            fluid,
            atlas,
            rain: Vec::new(),
            rain_layer,
            crt,
            layout,
            glass_pipeline,
            quad_pipeline,
            globals,
            sampler,
            instances,
            instance_capacity,
            start: now,
            last_frame: now,
            fixed_step: None,
            stepped: 0.0,
            image_textures: HashMap::new(),
            models,
            clock,
            can_record,
            capture: None,
        })
    }

    /// Times frames on the GPU from now on, where it can say, or stops.
    pub fn time_frames(&mut self, on: bool) {
        self.clock.set_on(on);
    }

    /// The last second's frames' mean time on the GPU, in milliseconds,
    /// while frames are timed and once one is read.
    pub fn gpu_frame_ms(&self) -> Option<f32> {
        self.clock.frame_ms()
    }

    /// Whether this GPU can say how long it takes over a frame.
    pub fn gpu_timed(&self) -> bool {
        self.clock.measures()
    }

    pub fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    /// Whether frames wait for the display's refresh (Settings → Screen
    /// refresh). Without, each is shown as soon as it's drawn: a little
    /// sooner, but as often as the GPU manages.
    /// Starts or stops taking frames for a recording: the surface can be
    /// copied from only while one runs, since that can cost a driver its
    /// fastest way of showing frames. False where it can't be copied at all.
    pub fn set_recording(&mut self, on: bool) -> bool {
        if on && !self.can_record {
            return false;
        }
        let usage = if on { wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC } else { wgpu::TextureUsages::RENDER_ATTACHMENT };
        if self.config.usage != usage {
            self.config.usage = usage;
            self.surface.configure(&self.device, &self.config);
        }
        self.capture = on.then(capture::Capture::new);
        true
    }

    /// Snapshots' recording, which draws offscreen as they do: buffers for
    /// its frames, the surface left as it is.
    pub fn record_offscreen_start(&mut self) {
        self.capture = Some(capture::Capture::new());
    }

    /// A frame drawn offscreen for snapshots' recording, the liquid stepped
    /// as for one shown, and with `take` its crop copied out as a shown
    /// frame's is; hands back the frames read back so far, waiting for this
    /// one.
    pub fn record_offscreen(&mut self, quads: &[Quad], images: &[ImageBatch], style: &FrameStyle, take: Option<(capture::Crop, std::time::Duration)>) -> Vec<crate::record::Frame> {
        let (w, h) = (self.config.width, self.config.height);
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("recorded offscreen"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self.encode_frame(&view, quads, images, style, true);
        let bgra = matches!(self.config.format, wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb);
        if let (Some(capture), Some((crop, at))) = (self.capture.as_mut(), take) {
            if let Some(crop) = crop.within(w, h) {
                capture.copy(&self.device, &mut encoder, &texture, crop, bgra, at);
            }
        }
        self.queue.submit(Some(encoder.finish()));
        match self.capture.as_mut() {
            Some(capture) => {
                capture.map_copied();
                capture.take_last(&self.device)
            }
            None => Vec::new(),
        }
    }

    /// Whether a recording's next frame has a buffer free to go into.
    pub fn recording_room(&self) -> bool {
        self.capture.as_ref().is_some_and(|c| c.has_room())
    }

    /// The recording's frames still on their way back, waited for, as it
    /// ends.
    pub fn recorded_last(&mut self) -> Vec<crate::record::Frame> {
        match self.capture.as_mut() {
            Some(c) => c.take_last(&self.device),
            None => Vec::new(),
        }
    }

    /// Whether a recording's frame is still on its way back from the GPU.
    pub fn recording_waiting(&self) -> bool {
        self.capture.as_ref().is_some_and(|c| c.waiting())
    }

    /// The frames of a recording the GPU has handed back since last asked,
    /// oldest first, without waiting for any.
    pub fn recorded(&mut self) -> Vec<crate::record::Frame> {
        match self.capture.as_mut() {
            Some(c) => c.take(&self.device),
            None => Vec::new(),
        }
    }

    pub fn set_vsync(&mut self, on: bool) {
        let mode = if on { wgpu::PresentMode::AutoVsync } else { wgpu::PresentMode::AutoNoVsync };
        if self.config.present_mode != mode {
            self.config.present_mode = mode;
            self.surface.configure(&self.device, &self.config);
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        self.fluid.resize(&self.device, width, height);
    }

    pub fn canvas(&mut self) -> Canvas<'_> {
        self.atlas.grow_if_crowded(&self.device, &self.queue);
        let cell = self.atlas.metrics;
        Canvas { quads: Vec::with_capacity(8192), images: Vec::new(), backdrop: 0, grounds: Vec::new(), over: Vec::new(), atlas: &mut self.atlas, queue: &self.queue, cell, glow_level: 1.0, changes: None }
    }

    pub fn time(&self) -> f32 {
        match self.fixed_step {
            Some(_) => self.stepped,
            None => self.start.elapsed().as_secs_f32(),
        }
    }

    /// From now on every frame advances `step` seconds (for recordings).
    pub fn set_fixed_step(&mut self, step: f32) {
        self.stepped = self.time();
        self.fixed_step = Some(step);
    }

    /// Seconds the liquid should advance this frame.
    fn frame_dt(&mut self) -> f32 {
        let now = Instant::now();
        let dt = (now - self.last_frame).as_secs_f32();
        self.last_frame = now;
        match self.fixed_step {
            Some(step) => {
                self.stepped += step;
                step
            }
            None => dt,
        }
    }

    /// Draws and presents a frame, and says whether it reached the screen.
    /// Draws a frame and shows it; while recording, with `take` (the
    /// terminal area, and the time since the recording began) its pixels
    /// there are copied out too, if a buffer is free for them.
    pub fn render(&mut self, quads: &[Quad], images: &[ImageBatch], style: &FrameStyle, simulate: bool, take: Option<(capture::Crop, std::time::Duration)>) -> Shown {
        // A suboptimal frame is still shown. The surface is reconfigured only
        // after presenting it: configuring while a frame is held fails, and
        // wgpu treats that as fatal.
        let (frame, suboptimal) = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) => (t, false),
            wgpu::CurrentSurfaceTexture::Suboptimal(t) => (t, true),
            wgpu::CurrentSurfaceTexture::Occluded => return Shown::Hidden,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                return Shown::Again;
            }
            _ => return Shown::Again,
        };
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        // A frame timed earlier read back, if the GPU is done with it, and
        // this one timed too while that's asked for.
        self.clock.poll(&self.device);
        self.clock.begin_frame();
        let mut encoder = self.encode_frame(&view, quads, images, style, simulate);
        self.clock.end_frame(&mut encoder);
        drop(view);
        let bgra = matches!(self.config.format, wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb);
        if let (Some(capture), Some((crop, at))) = (self.capture.as_mut(), take) {
            if let Some(crop) = crop.within(self.config.width, self.config.height) {
                capture.copy(&self.device, &mut encoder, &frame.texture, crop, bgra, at);
            }
        }
        self.queue.submit(Some(encoder.finish()));
        self.clock.submitted();
        if let Some(capture) = self.capture.as_mut() {
            capture.map_copied();
        }
        self.queue.present(frame);
        if suboptimal {
            self.surface.configure(&self.device, &self.config);
        }
        Shown::Yes
    }

    /// Steps the liquid without drawing (used while the window is hidden),
    /// for a frame of shader `mode` (`Style::shader_mode`): its textures
    /// made for that style first, as a frame drawn makes them, or the next
    /// drawn makes them again, empty, and the liquid stirred meanwhile is
    /// lost (the oil's snapshot came out black).
    pub fn simulate_only(&mut self, flow_speed: f32, mode: f32) {
        let dt = self.frame_dt();
        self.show_liquid(mode);
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("sim") });
        self.fluid.step(&self.device, &self.queue, &mut encoder, dt * flow_speed, &self.clock);
        self.queue.submit(Some(encoder.finish()));
    }

    /// Renders one frame offscreen and returns straight-alpha RGBA8 pixels
    /// (premultiplied output composited over `backdrop`, if given).
    pub fn snapshot(
        &mut self,
        quads: &[Quad],
        images: &[ImageBatch],
        style: &FrameStyle,
        backdrop: Option<&dyn Fn(u32, u32) -> [u8; 3]>,
    ) -> (u32, u32, Vec<u8>) {
        let (w, h) = (self.config.width, self.config.height);
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("snapshot"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        // Timed on the GPU while frames are, and read back once it's done,
        // as it's waited for.
        self.clock.begin_frame();
        let mut encoder = self.encode_frame(&view, quads, images, style, true);
        self.clock.end_frame(&mut encoder);
        let row = (w * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("snapshot readback"),
            size: (row * h) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: &texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) },
            },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        self.queue.submit(Some(encoder.finish()));
        self.clock.submitted();
        buffer.map_async(wgpu::MapMode::Read, .., |_| {});
        let _ = self.device.poll(wgpu::PollType::Wait { submission_index: None, timeout: None });
        self.clock.poll(&self.device);
        let bgra = matches!(self.config.format, wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb);
        let mut out = Vec::with_capacity((w * h * 4) as usize);
        {
            let data = buffer.get_mapped_range(..).expect("mapped snapshot buffer");
            for y in 0..h {
                for x in 0..w {
                    let i = (y * row + x * 4) as usize;
                    let (r, g, b, a) = if bgra {
                        (data[i + 2], data[i + 1], data[i], data[i + 3])
                    } else {
                        (data[i], data[i + 1], data[i + 2], data[i + 3])
                    };
                    match backdrop {
                        Some(bg) => {
                            // Premultiplied over an opaque backdrop.
                            let under = bg(x, y);
                            let inv = 255 - a as u16;
                            let mix = |c: u8, u: u8| (c as u16 + (u as u16 * inv) / 255).min(255) as u8;
                            out.extend_from_slice(&[mix(r, under[0]), mix(g, under[1]), mix(b, under[2]), 255]);
                        }
                        None => {
                            let un = |c: u8| if a == 0 { 0 } else { ((c as u16 * 255) / a as u16).min(255) as u8 };
                            out.extend_from_slice(&[un(r), un(g), un(b), a]);
                        }
                    }
                }
            }
        }
        buffer.unmap();
        (w, h, out)
    }

    /// The liquid's textures for shader `mode`: plain glass (mode 5) reads
    /// none of the liquid, and only the water styles (modes 1 and 2) read
    /// its ripples.
    fn show_liquid(&mut self, mode: f32) {
        self.fluid.show(&self.device, mode < 4.5, (0.5..2.5).contains(&mode));
    }

    fn encode_frame(
        &mut self,
        view: &wgpu::TextureView,
        quads: &[Quad],
        images: &[ImageBatch],
        style: &FrameStyle,
        simulate: bool,
    ) -> wgpu::CommandEncoder {
        let dt = self.frame_dt();

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("frame") });
        self.show_liquid(style.mode);
        if simulate {
            self.fluid.step(&self.device, &self.queue, &mut encoder, dt * style.flow_speed, &self.clock);
        }

        let globals = Globals {
            resolution: [self.config.width as f32, self.config.height as f32],
            time: style.flow_time,
            opacity: style.opacity,
            corner_radius: style.corner_radius,
            ripple: style.ripple,
            focus: style.focus,
            activity: style.activity,
            tint: style.tint,
            accent: style.accent,
            accent2: style.accent2,
            dye_texel: [1.0 / self.fluid.dye_size.0 as f32, 1.0 / self.fluid.dye_size.1 as f32],
            dye_gain: style.dye_gain,
            opaque: if self.transparent { 0.0 } else { 1.0 },
            caustics: style.caustics,
            mode: style.mode,
            sun: style.sun,
            ui_scale: style.ui_scale,
            flash: style.flash,
            wave_texel: [1.0 / self.fluid.wave_size.0 as f32, 1.0 / self.fluid.wave_size.1 as f32],
            wet: style.wet,
            _pad1: 0.0,
            pulse: style.pulse,
            pulse_color: style.pulse_color,
            progress: style.progress,
            progress_color: style.progress_color,
            phosphor: style.phosphor,
        };
        self.queue.write_buffer(&self.globals, 0, bytemuck::bytes_of(&globals));

        if quads.len() > self.instance_capacity {
            self.instance_capacity = quads.len().next_power_of_two();
            self.instances = create_instance_buffer(&self.device, self.instance_capacity);
        }
        if !quads.is_empty() {
            self.queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(quads));
        }

        // The rain style draws its drops into the rain map first; the glass
        // reads it where the water styles read their ripples.
        let raining = style.mode > 3.5 && style.mode < 4.5;
        if raining {
            let size = (self.config.width, self.config.height);
            self.rain_layer.draw((&self.device, &self.queue), &mut encoder, size, style.ui_scale, &self.rain, &self.clock);
        } else {
            self.rain_layer.release();
        }
        // With the CRT on, the frame is drawn for it to show; with a phosphor
        // and no tube, for a last pass that brings it back onto the phosphor
        // (`CrtLayer::onto_phosphor`).
        let tube = style.crt.strength > 0.0;
        let onto_phosphor = !tube && style.phosphor[3] > 0.5;
        if tube || onto_phosphor {
            // The afterglow's pair of textures only while it lingers: with the
            // window in front, and frames coming steadily.
            let afterglow = tube && style.crt.steady && style.crt.decay > 0.0;
            self.crt.prepare(&self.device, (self.config.width, self.config.height), afterglow, tube);
        } else {
            self.crt.release();
        }
        // Each image's texture, made the first time it shows (an animation's
        // written again as its frame changes), and each model's, drawn when
        // it changed; those not drawn for a while go.
        let now = Instant::now();
        for b in images {
            match &b.image.model {
                Some(shown) => {
                    let view = self.models.draw((&self.device, &self.queue), &mut encoder, &b.image, shown, now, &self.clock);
                    self.image_textures.insert(b.image.texture(), ImageTexture { view, used: now, holds: b.image.serial });
                }
                None => {
                    upload_image((&self.device, &self.queue), &mut self.image_textures, &b.image, now);
                }
            }
        }
        // A model's picture drawn anew at another size frees the old one now,
        // and so does a picture that's gone (sent again, or deleted), an
        // animation once the last of its frames is.
        for serial in self.models.take_replaced().into_iter().chain(crate::graphics::take_gone()) {
            if !images.iter().any(|b| b.image.texture() == serial) {
                self.image_textures.remove(&serial);
            }
        }
        self.image_textures.retain(|_, t| now.duration_since(t.used) < IMAGE_KEEP);
        self.models.release(now);
        let surface = match self.rain_layer.view() {
            Some(rain) if raining => rain,
            _ => self.fluid.wave_view(),
        };
        let bind = |tex: &wgpu::TextureView| {
            self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: self.globals.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(tex) },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(self.fluid.velocity_view()),
                    },
                    wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                    wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(surface) },
                ],
            })
        };
        let glass_bind = bind(self.fluid.dye_view());
        let quad_bind = bind(&self.atlas.view);
        let image_binds: Vec<wgpu::BindGroup> = images.iter().map(|b| bind(&self.image_textures[&b.image.texture()].view)).collect();
        {
            let main = match self.crt.frame() {
                Some(frame) if tube || onto_phosphor => frame,
                _ => view,
            };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: main,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: self.clock.writes(),
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.glass_pipeline);
            pass.set_bind_group(0, &glass_bind, &[]);
            pass.draw(0..3, 0..1);
            if !quads.is_empty() {
                pass.set_pipeline(&self.quad_pipeline);
                pass.set_vertex_buffer(0, self.instances.slice(..));
                // The backdrop, the images over it, then everything else,
                // with any picture drawn among it in its turn.
                let mut at = 0;
                for (b, bind) in images.iter().zip(&image_binds) {
                    if at < b.range.start {
                        pass.set_bind_group(0, &quad_bind, &[]);
                        pass.draw(0..6, at..b.range.start);
                    }
                    pass.set_bind_group(0, bind, &[]);
                    pass.draw(0..6, b.range.clone());
                    at = b.range.end;
                }
                pass.set_bind_group(0, &quad_bind, &[]);
                pass.draw(0..6, at..quads.len() as u32);
            }
        }
        if tube {
            let frame = CrtFrame { look: style.crt, fixed_dt: self.fixed_step, scale: style.ui_scale, phosphor: style.phosphor };
            self.crt.draw((&self.device, &self.queue), &mut encoder, &self.sampler, view, frame, &self.clock);
        } else if onto_phosphor {
            self.crt.onto_phosphor((&self.device, &self.queue), &mut encoder, &self.sampler, view, style.phosphor, &self.clock);
        }
        encoder
    }
}

/// What Flow asks of the GPU. It draws without compute shaders or storage
/// buffers, so it asks no more than OpenGL ES 3.0 promises: an OpenGL
/// without compute (VirtualBox's, or Mesa's below 4.3) offers none, and
/// wgpu refuses a device asked for more than its adapter has. Textures may
/// be as big as the adapter allows, for windows wider than 2048 pixels.
pub(crate) fn limits(adapter: &wgpu::Adapter) -> wgpu::Limits {
    wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits())
}

/// Gives `image` a texture in `textures`, made the first time it's drawn
/// (scaled down if the GPU can't hold it whole), and notes that it's in
/// use. The frames of an animation share one (`Image::texture`), which is
/// written with a frame's pixels only when that isn't the frame it holds:
/// so a moving picture holds one frame's worth on the GPU, and costs a
/// write only as its frame changes. Says whether it wrote.
fn upload_image((device, queue): (&wgpu::Device, &wgpu::Queue), textures: &mut HashMap<u64, ImageTexture>, image: &Image, now: Instant) -> bool {
    let key = image.texture();
    let (w, h, pixels) = match textures.get_mut(&key) {
        Some(held) if held.holds == image.serial => {
            held.used = now;
            return false;
        }
        Some(held) => {
            held.used = now;
            let (w, h, pixels) = fit_texture(image, device.limits().max_texture_dimension_2d);
            let texture = held.view.texture();
            // Another frame of the same size, written over the last.
            if (texture.width(), texture.height()) == (w, h) {
                write_pixels(queue, texture, (w, h), &pixels);
                held.holds = image.serial;
                return true;
            }
            (w, h, pixels)
        }
        None => fit_texture(image, device.limits().max_texture_dimension_2d),
    };
    // Read back by the tests, which check what it holds.
    let read_back = if cfg!(test) { wgpu::TextureUsages::COPY_SRC } else { wgpu::TextureUsages::empty() };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("image"),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST | read_back,
        view_formats: &[],
    });
    write_pixels(queue, &texture, (w, h), &pixels);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    textures.insert(key, ImageTexture { view, used: now, holds: image.serial });
    true
}

/// `pixels` (RGBA, `w` × `h`) into `texture`, whole.
fn write_pixels(queue: &wgpu::Queue, texture: &wgpu::Texture, (w, h): (u32, u32), pixels: &[u8]) {
    queue.write_texture(
        texture.as_image_copy(),
        pixels,
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 4), rows_per_image: Some(h) },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
}

/// An image's pixels as a texture can hold them: whole, or averaged down by
/// a whole factor until no side is longer than `max`.
fn fit_texture(image: &Image, max: u32) -> (u32, u32, Cow<'_, [u8]>) {
    let (w, h) = (image.width, image.height);
    let k = w.max(h).div_ceil(max.max(1));
    if k <= 1 {
        return (w, h, Cow::Borrowed(&image.pixels));
    }
    let (nw, nh) = (w / k, h / k);
    let mut out = vec![0u8; (nw * nh * 4) as usize];
    for y in 0..nh {
        for x in 0..nw {
            let mut sum = [0u32; 4];
            for dy in 0..k {
                for dx in 0..k {
                    let i = (((y * k + dy) * w + x * k + dx) * 4) as usize;
                    for (s, p) in sum.iter_mut().zip(&image.pixels[i..i + 4]) {
                        *s += *p as u32;
                    }
                }
            }
            let o = ((y * nw + x) * 4) as usize;
            for (c, s) in out[o..o + 4].iter_mut().zip(sum) {
                *c = (s / (k * k)) as u8;
            }
        }
    }
    (nw, nh, Cow::Owned(out))
}

fn create_instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("quads"),
        size: (capacity * std::mem::size_of::<Quad>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A quad known by `n`.
    fn quad(n: f32) -> Quad {
        Quad { rect: [n, 0.0, 1.0, 1.0], uv: [0.0; 4], color: [1.0; 4], params: [KIND_SOLID, 0.0, 0.0, 0.0] }
    }

    #[test]
    fn cells_grounds_go_over_pictures_and_under_what_programs_set_off() {
        let (a, b) = (Arc::new(Image::new(1, 1, vec![255; 4])), Arc::new(Image::new(1, 1, vec![255; 4])));
        // The liquid's bolt (1), then a program's burst (2), the title bar
        // (3), a cell's letter (4) and the About card's cube (5); the
        // pictures' pieces (10 to 12) and the cells' grounds (20, 21) laid
        // as the terminal is drawn, after the burst.
        let quads = vec![quad(1.0), quad(2.0), quad(3.0), quad(4.0), quad(5.0)];
        let tiles = vec![(b.clone(), quad(12.0)), (a.clone(), quad(10.0)), (a.clone(), quad(11.0))];
        let (quads, batches) = stack(quads, 1, tiles, vec![quad(20.0), quad(21.0)], vec![(4, b.clone())]);
        let order: Vec<f32> = quads.iter().map(|q| q.rect[0]).collect();
        assert_eq!(order, [1.0, 10.0, 11.0, 12.0, 20.0, 21.0, 2.0, 3.0, 4.0, 5.0]);
        // Each picture's pieces together, and the cube in its place.
        let drawn: Vec<(u64, Range<u32>)> = batches.iter().map(|batch| (batch.image.serial, batch.range.clone())).collect();
        assert_eq!(drawn, [(a.serial, 1..3), (b.serial, 3..4), (b.serial, 9..10)]);
    }

    /// An animation 2 × 1 pixels, its frames each all of one of `colors`
    /// for 50 ms.
    fn animation(colors: &[[u8; 4]]) -> Image {
        let frames = colors.iter().map(|c| (Image::new(2, 1, c.repeat(2)), Duration::from_millis(50))).collect();
        Image::animated(2, 1, crate::frames::Frames::new(frames, None))
    }

    #[test]
    fn an_animations_pieces_are_drawn_together_from_its_one_texture() {
        let moving = animation(&[[255, 0, 0, 255], [0, 0, 255, 255]]);
        let frames = moving.frames.as_ref().unwrap();
        let (shown, still) = (frames.frame(1).unwrap().clone(), Arc::new(Image::new(1, 1, vec![255; 4])));
        let tiles = vec![(shown.clone(), quad(10.0)), (still.clone(), quad(12.0)), (shown.clone(), quad(11.0))];
        let (_, batches) = stack(vec![quad(1.0)], 1, tiles, Vec::new(), Vec::new());
        let drawn: Vec<(u64, usize)> = batches.iter().map(|batch| (batch.image.texture(), batch.range.len())).collect();
        assert_eq!(drawn.len(), 2);
        assert!(drawn.contains(&(frames.reel(), 2)) && drawn.contains(&(still.serial, 1)), "{drawn:?}");
    }

    /// The machine's GPU, if wgpu finds one, with the limits the app asks for.
    fn gpu() -> Option<(wgpu::Device, wgpu::Queue)> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).ok()?;
        let required_limits = limits(&adapter);
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor { required_limits, ..Default::default() })).ok()
    }

    /// What `texture` holds, read back: RGBA by rows.
    fn read_back((device, queue): (&wgpu::Device, &wgpu::Queue), texture: &wgpu::Texture) -> Vec<u8> {
        let (w, h) = (texture.width(), texture.height());
        let row = (w * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: u64::from(row * h),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo { buffer: &buffer, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) } },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        queue.submit(Some(encoder.finish()));
        buffer.map_async(wgpu::MapMode::Read, .., |_| {});
        let _ = device.poll(wgpu::PollType::Wait { submission_index: None, timeout: None });
        let data = buffer.get_mapped_range(..).expect("mapped");
        (0..h).flat_map(|y| data[(y * row) as usize..(y * row + w * 4) as usize].to_vec()).collect()
    }

    #[test]
    #[ignore = "draws on the GPU"]
    fn an_animation_holds_one_texture_on_the_gpu_written_only_as_its_frame_changes() {
        let Some((device, queue)) = gpu() else {
            eprintln!("no GPU found: not tried");
            return;
        };
        let gpu = (&device, &queue);
        let (red, blue, green) = ([255, 0, 0, 255], [0, 0, 255, 255], [0, 255, 0, 255]);
        let moving = animation(&[red, blue]);
        let frames = moving.frames.as_ref().unwrap();
        let (first, second) = (frames.frame(0).unwrap().clone(), frames.frame(1).unwrap().clone());
        let mut textures = HashMap::new();
        let now = Instant::now();
        let held = |textures: &HashMap<u64, ImageTexture>| read_back(gpu, textures[&frames.reel()].view.texture());
        assert!(upload_image(gpu, &mut textures, &first, now), "made and written");
        assert_eq!(held(&textures), red.repeat(2));
        assert!(!upload_image(gpu, &mut textures, &first, now), "the frame it holds: nothing to write");
        assert!(upload_image(gpu, &mut textures, &second, now));
        assert_eq!((held(&textures), textures.len()), (blue.repeat(2), 1), "one texture, written over");
        // A frame edited is a picture of its own in the frame's place, so
        // it's written over what was there although it's the same frame.
        let mut edited = frames.clone();
        edited.put(1, Image::new(2, 1, green.repeat(2)));
        assert!(upload_image(gpu, &mut textures, edited.frame(1).unwrap(), now));
        assert_eq!((held(&textures), textures.len()), (green.repeat(2), 1));
        // A still picture has a texture of its own.
        let still = Image::new(1, 1, vec![1, 2, 3, 255]);
        assert!(upload_image(gpu, &mut textures, &still, now));
        assert_eq!((textures.len(), read_back(gpu, textures[&still.serial].view.texture())), (2, vec![1, 2, 3, 255]));
    }
}
