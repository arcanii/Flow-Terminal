//! Glyph rasterization (cosmic-text + swash) into a single RGBA atlas
//! texture, which starts small and doubles as it fills.

use std::collections::HashMap;

use unicode_width::UnicodeWidthChar;

use cosmic_text::skrifa::{self, prelude::*};
use cosmic_text::{fontdb, Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Style, SwashCache, SwashContent, SwashImage, Weight};

/// The atlas starts this many pixels square, which holds the glyphs of a
/// session in one font at one size, and doubles once it's three quarters
/// full, up to MAX_SIZE. Full at that, it starts over.
const START_SIZE: u32 = 1024;
const MAX_SIZE: u32 = 2048;

/// Fonts the atlas keeps besides the one in use, with their cells and the
/// places of their glyphs, so tabs in themes of other fonts (or a size
/// stepped back to) draw at once when they come to the front: the most
/// recently used, past which the oldest goes.
const KEPT_FACES: usize = 8;

/// Fonts Flow builds in for its old machines' themes (`assets/fonts`, with
/// their licenses), so a theme looks as it should without installing
/// anything: a video terminal's (VT323), a teletext character generator's
/// (Bedstead), which also draws teletext's mosaics, a workstation's console
/// (Gallant), a graphics workstation's (SGI's Screen, its bitmaps made
/// outlines by `assets/fonts/bitmap2ttf.py`), the teletype's (Flow Teletype,
/// Flow's own, drawn by `assets/fonts/teletype.py` after the Model 33's
/// printouts), a typewriter's (Special Elite, which the teletype's look
/// keeps after its own), the TV home computer's dots (Flow Instruments,
/// Flow's own too, drawn by `assets/fonts/instruments.py`), the 16-bit home
/// computer's tall dots (Flow notaTopaz, Flow's own, drawn by
/// `assets/fonts/notatopaz.py`), and an 8-bit home computer's and a PC's
/// letters (unscii's two sizes). Each goes with the family it's found by where that
/// isn't the one it names: both of unscii's call themselves "unscii", so
/// each goes by its full name.
const BUILT_IN: [(&[u8], Option<&str>); 10] = [
    (include_bytes!("../../../assets/fonts/VT323-Regular.ttf"), None),
    (include_bytes!("../../../assets/fonts/bedstead.otf"), None),
    (include_bytes!("../../../assets/fonts/gallant.ttf"), None),
    (include_bytes!("../../../assets/fonts/sgi-screen.ttf"), None),
    (include_bytes!("../../../assets/fonts/flow-teletype.ttf"), None),
    (include_bytes!("../../../assets/fonts/SpecialElite-Regular.ttf"), None),
    (include_bytes!("../../../assets/fonts/flow-instruments.ttf"), None),
    (include_bytes!("../../../assets/fonts/flow-notatopaz.ttf"), None),
    (include_bytes!("../../../assets/fonts/unscii-8.ttf"), Some("unscii-8")),
    (include_bytes!("../../../assets/fonts/unscii-16.ttf"), Some("unscii-16")),
];

/// The installed fonts, then Flow's own, then those of the themes
/// installed as packages (`looks::packaged`).
fn font_system() -> FontSystem {
    let mut font_system = FontSystem::new();
    load_built_in(font_system.db_mut());
    for file in crate::looks::packaged::font_files() {
        if let Err(err) = font_system.db_mut().load_font_file(&file) {
            log::warn!("a theme's font {}: {err}", file.display());
        }
    }
    font_system
}

fn load_built_in(db: &mut fontdb::Database) {
    for (font, family) in BUILT_IN {
        let ids = db.load_font_source(fontdb::Source::Binary(std::sync::Arc::new(as_regular(font))));
        let Some(family) = family else { continue };
        for id in ids {
            if let Some(mut face) = db.face(id).cloned() {
                db.remove_face(id);
                face.families = vec![(family.to_string(), fontdb::Language::English_UnitedStates)];
                db.push_face_info(face);
            }
        }
    }
}

/// `font` with its weight class set to regular (400): each built-in font is
/// a family of one face, which Flow picks as the family's regular, and
/// Bedstead's says it's medium (500), unscii's 0.
fn as_regular(font: &[u8]) -> Vec<u8> {
    let mut font = font.to_vec();
    let tables = font.get(4..6).map_or(0, |n| u16::from_be_bytes([n[0], n[1]]) as usize);
    let os2 = (0..tables).map(|i| 12 + 16 * i).find(|&at| font.get(at..at + 4) == Some(b"OS/2"));
    if let Some(at) = os2 {
        let offset = u32::from_be_bytes(font[at + 8..at + 12].try_into().expect("four bytes")) as usize;
        if let Some(weight) = font.get_mut(offset + 4..offset + 6) {
            weight.copy_from_slice(&400u16.to_be_bytes());
        }
    }
    font
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GlyphKey {
    pub ch: char,
    pub bold: bool,
    pub italic: bool,
    /// Font size in pixels; 0 is the terminal's size.
    pub size: u16,
    /// The letter's combining marks, drawn with it: an id from
    /// [`Atlas::marked`], or 0 for none.
    pub marks: u32,
}

/// The most letters with marks the atlas keeps ids for. A program writing
/// a stream of new ones gets its letters drawn without their marks past it,
/// rather than the list growing for good.
const MOST_MARKED: usize = 4096;

#[derive(Debug, Clone, Copy)]
pub struct GlyphInfo {
    /// u0, v0, u1, v1
    pub uv: [f32; 4],
    /// Offset of the bitmap's top-left from the cell's top-left, in pixels.
    pub offset: [f32; 2],
    pub size: [f32; 2],
    pub color: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct CellMetrics {
    pub width: f32,
    pub height: f32,
    pub baseline: f32,
    pub font_size: f32,
    /// How tall the font's x is, in pixels, which the squares Flow draws
    /// itself are sized by (`crate::box_drawing`).
    pub x_height: f32,
    /// How wide the stem of the font's l is, in pixels (0 without one),
    /// which box drawing's light stroke is at least as heavy as
    /// (`box_drawing::stroke`), so its lines sit with the letters.
    pub stem: f32,
}

pub struct Atlas {
    texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    /// Pixels on a side.
    size: u32,
    font_system: FontSystem,
    /// The font files of themes installed as packages it has loaded.
    theme_fonts: Vec<std::path::PathBuf>,
    /// Fonts loaded from memory (a package's, for the theme gallery's
    /// preview), by a hash of their bytes, so each is loaded once.
    font_data: Vec<u64>,
    swash: SwashCache,
    /// The configured family that was found, or None for cosmic-text's fallback.
    family: Option<String>,
    /// The family that actually draws the text, for Settings to show.
    in_use: Option<String>,
    /// Whether bold text finds a bold face in that family. Without one,
    /// cosmic-text would draw it in some other font.
    own_bold: bool,
    /// Whether the family's letters differ in width (a typewriter's, such as
    /// Special Elite): its cells are as wide as its M, and
    /// each letter is set in the middle of its cell, as a typewriter sets
    /// it, rather than at the cell's left.
    centered: bool,
    /// Installed monospaced families that draw their own letters, found once
    /// when the atlas is made.
    monospace: Vec<String>,
    map: HashMap<GlyphKey, Option<GlyphInfo>>,
    /// Each letter with its marks that a key names ([`GlyphKey::marks`],
    /// one more than its place here), and its id by its text.
    marked_text: Vec<String>,
    marked_ids: HashMap<String, u32>,
    /// The other fonts it keeps (`KEPT_FACES`), the most recently used last.
    kept: Vec<Face>,
    /// Pictures (premultiplied RGBA), uploaded on first use and again after a reset.
    image_sources: HashMap<u32, (Vec<u8>, u32, u32)>,
    images: HashMap<u32, Option<GlyphInfo>>,
    cursor_x: u32,
    cursor_y: u32,
    shelf_h: u32,
    pub metrics: CellMetrics,
    line_height_factor: f32,
}

/// A font at a size and line height as the atlas keeps it while another is
/// in use: what the atlas's own fields of the same names hold for the one
/// in use. Its glyphs stay where they are in the texture.
struct Face {
    family: Option<String>,
    in_use: Option<String>,
    own_bold: bool,
    centered: bool,
    metrics: CellMetrics,
    line_height_factor: f32,
    map: HashMap<GlyphKey, Option<GlyphInfo>>,
}

impl Face {
    fn is(&self, family: &Option<String>, font_px: f32, line_height: f32) -> bool {
        self.family == *family && self.metrics.font_size == font_px && self.line_height_factor == line_height
    }
}

impl Atlas {
    /// What it holds on the GPU: one texture of four bytes a pixel.
    pub fn gpu_bytes(&self) -> u64 {
        self.size as u64 * self.size as u64 * 4
    }

    pub fn new(device: &wgpu::Device, families: &[String], font_px: f32, line_height: f32) -> Self {
        let texture = atlas_texture(device, START_SIZE);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut font_system = font_system();
        let family = pick_family(&font_system, families);
        let monospace = monospace_families(&font_system);
        let metrics = measure(&mut font_system, family.as_deref(), font_px, line_height);
        let (in_use, own_bold) = resolve(&mut font_system, family.as_deref());
        let centered = family.as_deref().is_some_and(|f| regular_face(&font_system, f) == Some(false));
        log::info!(
            "terminal font: {} ({} monospaced families installed)",
            in_use.as_deref().unwrap_or("none found"),
            monospace.len()
        );
        Self {
            texture,
            view,
            size: START_SIZE,
            font_system,
            theme_fonts: crate::looks::packaged::font_files(),
            font_data: Vec::new(),
            swash: SwashCache::new(),
            family,
            in_use,
            own_bold,
            centered,
            monospace,
            map: HashMap::new(),
            marked_text: Vec::new(),
            marked_ids: HashMap::new(),
            kept: Vec::new(),
            image_sources: HashMap::new(),
            images: HashMap::new(),
            cursor_x: 1,
            cursor_y: 1,
            shelf_h: 0,
            metrics,
            line_height_factor: line_height,
        }
    }

    /// Loads those of `files` it hasn't yet, the fonts of a theme package
    /// installed while Flow runs, so a theme can name their families.
    pub fn load_fonts(&mut self, files: &[std::path::PathBuf]) {
        let mut added = false;
        for file in files {
            if self.theme_fonts.contains(file) {
                continue;
            }
            match self.font_system.db_mut().load_font_file(file) {
                Ok(()) => added = true,
                Err(err) => log::warn!("a theme's font {}: {err}", file.display()),
            }
            self.theme_fonts.push(file.clone());
        }
        if added {
            self.monospace = monospace_families(&self.font_system);
        }
    }

    /// Loads those of `fonts` (font files' bytes) it hasn't yet: a package's
    /// that the theme gallery shows before it's installed, so a preview can
    /// be drawn in them.
    pub fn load_font_data(&mut self, fonts: &[Vec<u8>]) {
        use std::hash::{Hash, Hasher};
        let mut added = false;
        for data in fonts {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            data.hash(&mut hasher);
            let key = hasher.finish();
            if self.font_data.contains(&key) {
                continue;
            }
            self.font_system.db_mut().load_font_data(data.clone());
            self.font_data.push(key);
            added = true;
        }
        if added {
            self.monospace = monospace_families(&self.font_system);
        }
    }

    /// The same family at `font_px`.
    pub fn set_font_size(&mut self, font_px: f32) {
        self.use_face(self.family.clone(), font_px, self.line_height_factor);
    }

    /// Switches to the first installed family of `families` at the current
    /// size; glyphs missing from it still come from fallback fonts.
    pub fn set_families(&mut self, families: &[String]) {
        let family = pick_family(&self.font_system, families);
        self.use_face(family, self.metrics.font_size, self.line_height_factor);
    }

    /// Switches to the first installed family of `families` at `font_px`,
    /// its lines `line_height` times that apart: at once where it's one the
    /// atlas keeps, its glyphs where they were.
    pub fn set_font(&mut self, families: &[String], font_px: f32, line_height: f32) {
        let family = pick_family(&self.font_system, families);
        self.use_face(family, font_px, line_height);
    }

    /// The cell the first installed family of `families` has at `font_px`
    /// and `line_height`, without drawing in it: a tab's own theme's, for
    /// its grid while another is in front. That font is kept, for when it
    /// comes to the front.
    pub fn metrics_for(&mut self, families: &[String], font_px: f32, line_height: f32) -> CellMetrics {
        let family = pick_family(&self.font_system, families);
        if self.family == family && self.metrics.font_size == font_px && self.line_height_factor == line_height {
            return self.metrics;
        }
        if let Some(face) = self.kept.iter().find(|f| f.is(&family, font_px, line_height)) {
            return face.metrics;
        }
        let face = self.make_face(family, font_px, line_height);
        let metrics = face.metrics;
        self.keep(face);
        metrics
    }

    /// Puts `family` at `font_px` and `line_height` in use, keeping the one
    /// that was: one the atlas keeps comes back as it was, another is
    /// measured afresh, its glyphs drawn as they're needed.
    fn use_face(&mut self, family: Option<String>, font_px: f32, line_height: f32) {
        if self.family == family && self.metrics.font_size == font_px && self.line_height_factor == line_height {
            return;
        }
        // Said once, as a face is made: a frame drawing a split tab's panes
        // in fonts of their own switches between kept faces each time.
        let face = match self.kept.iter().position(|f| f.is(&family, font_px, line_height)) {
            Some(i) => self.kept.remove(i),
            None => {
                let face = self.make_face(family, font_px, line_height);
                log::info!("terminal font: {} at {font_px} px", face.in_use.as_deref().unwrap_or("none found"));
                face
            }
        };
        let was = Face {
            family: std::mem::replace(&mut self.family, face.family),
            in_use: std::mem::replace(&mut self.in_use, face.in_use),
            own_bold: std::mem::replace(&mut self.own_bold, face.own_bold),
            centered: std::mem::replace(&mut self.centered, face.centered),
            metrics: std::mem::replace(&mut self.metrics, face.metrics),
            line_height_factor: std::mem::replace(&mut self.line_height_factor, face.line_height_factor),
            map: std::mem::replace(&mut self.map, face.map),
        };
        self.keep(was);
    }

    /// `family` at `font_px` and `line_height`, measured, with no glyphs yet.
    fn make_face(&mut self, family: Option<String>, font_px: f32, line_height: f32) -> Face {
        let metrics = measure(&mut self.font_system, family.as_deref(), font_px, line_height);
        let (in_use, own_bold) = resolve(&mut self.font_system, family.as_deref());
        let centered = family.as_deref().is_some_and(|f| regular_face(&self.font_system, f) == Some(false));
        Face { family, in_use, own_bold, centered, metrics, line_height_factor: line_height, map: HashMap::new() }
    }

    /// Keeps `face` as the most recently used, letting the oldest go past
    /// `KEPT_FACES`.
    fn keep(&mut self, face: Face) {
        self.kept.push(face);
        if self.kept.len() > KEPT_FACES {
            self.kept.remove(0);
        }
    }

    /// `ch` drawn `px` tall from the first of `families` that has it, as
    /// premultiplied white pixels (for a symbol such as a logo).
    pub fn symbol(&mut self, ch: char, px: f32, families: &[&str]) -> Option<(Vec<u8>, u32, u32)> {
        for want in families {
            let Some(name) = installed_family(&self.font_system, want) else { continue };
            let mut buffer = Buffer::new(&mut self.font_system, Metrics::new(px, px * 1.2));
            let mut text = [0u8; 4];
            buffer.set_text(ch.encode_utf8(&mut text), &Attrs::new().family(Family::Name(&name)), Shaping::Advanced, None);
            buffer.shape_until_scroll(&mut self.font_system, false);
            let Some(glyph) = buffer.layout_runs().next().and_then(|run| run.glyphs.first().cloned()) else { continue };
            // A fallback font drew it, or nothing did: try the next family.
            if glyph.glyph_id == 0 || family_of(&self.font_system, glyph.font_id).as_deref() != Some(name.as_str()) {
                continue;
            }
            let image = self.swash.get_image_uncached(&mut self.font_system, glyph.physical((0.0, 0.0), 1.0).cache_key)?;
            let (w, h) = (image.placement.width, image.placement.height);
            let alpha: Vec<u8> = match image.content {
                SwashContent::Mask => image.data,
                _ => image.data.as_chunks::<4>().0.iter().map(|p| p[3]).collect(),
            };
            if w == 0 || h == 0 || alpha.len() < (w * h) as usize {
                return None;
            }
            return Some((alpha.iter().flat_map(|&a| [a, a, a, a]).collect(), w, h));
        }
        None
    }

    /// `text` drawn `px` tall in the first of `families` installed, bold if
    /// asked, as premultiplied white pixels cropped to what's drawn (for a
    /// wordmark such as a chip maker's logo).
    pub fn word(&mut self, text: &str, px: f32, families: &[&str], bold: bool) -> Option<(Vec<u8>, u32, u32)> {
        let name = families.iter().find_map(|want| installed_family(&self.font_system, want))?;
        let mut buffer = Buffer::new(&mut self.font_system, Metrics::new(px, px * 1.3));
        let attrs = Attrs::new().family(Family::Name(&name)).weight(if bold { Weight::BOLD } else { Weight::NORMAL });
        buffer.set_text(text, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut self.font_system, false);
        let (glyphs, line_y, line_w) = {
            let run = buffer.layout_runs().next()?;
            (run.glyphs.iter().map(|g| g.physical((0.0, 0.0), 1.0)).collect::<Vec<_>>(), run.line_y, run.line_w)
        };
        let (w, h) = ((line_w + px).ceil() as i32, (px * 1.6).ceil() as i32);
        let mut alpha = vec![0u8; (w * h) as usize];
        for glyph in glyphs {
            let Some(image) = self.swash.get_image_uncached(&mut self.font_system, glyph.cache_key) else { continue };
            let (gw, gh) = (image.placement.width as i32, image.placement.height as i32);
            let data: Vec<u8> = match image.content {
                SwashContent::Mask => image.data,
                _ => image.data.as_chunks::<4>().0.iter().map(|p| p[3]).collect(),
            };
            let (x0, y0) = (glyph.x + image.placement.left + (px / 2.0) as i32, line_y.round() as i32 + glyph.y - image.placement.top + (px * 0.15) as i32);
            for y in 0..gh {
                for x in 0..gw {
                    let (cx, cy) = (x0 + x, y0 + y);
                    if (0..w).contains(&cx) && (0..h).contains(&cy) {
                        let a = data.get((y * gw + x) as usize).copied().unwrap_or(0);
                        let at = (cy * w + cx) as usize;
                        alpha[at] = alpha[at].max(a);
                    }
                }
            }
        }
        // Cropped to what's drawn.
        let drawn = |x: i32, y: i32| alpha[(y * w + x) as usize] > 0;
        let cols: Vec<i32> = (0..w).filter(|x| (0..h).any(|y| drawn(*x, y))).collect();
        let rows: Vec<i32> = (0..h).filter(|y| (0..w).any(|x| drawn(x, *y))).collect();
        let (&left, &right, &top, &bottom) = (cols.first()?, cols.last()?, rows.first()?, rows.last()?);
        let (cw, ch) = (right - left + 1, bottom - top + 1);
        let pixels = (top..=bottom).flat_map(|y| (left..=right).map(move |x| (x, y))).map(|(x, y)| alpha[(y * w + x) as usize]).flat_map(|a| [a, a, a, a]).collect();
        Some((pixels, cw as u32, ch as u32))
    }

    /// The family the terminal is set to: the configured one that was found,
    /// or what cosmic-text falls back to when none was. Settings shows it, so
    /// a pick always shows as itself and stepping goes on from there.
    pub fn family_in_use(&self) -> Option<&str> {
        self.family.as_deref().or(self.in_use.as_deref())
    }

    /// Families to offer in Settings: the installed monospaced ones, plus
    /// those of `configured` that are installed and monospaced but not
    /// flagged so (Monaco isn't). Unsorted and may repeat.
    pub fn family_choices(&self, configured: &[String]) -> Vec<String> {
        let fs = &self.font_system;
        let mut names = self.monospace.clone();
        names.extend(
            configured.iter().filter_map(|want| installed_family(fs, want)).filter(|n| regular_face(fs, n) == Some(true)),
        );
        names
    }

    /// Doubles the atlas once three quarters of it are used, keeping the
    /// glyphs in it, so it rarely has to start over. Called before a frame
    /// is drawn, while no quad holds a place in it.
    pub fn grow_if_crowded(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        if self.size >= MAX_SIZE || self.cursor_y + self.shelf_h < self.size * 3 / 4 {
            return;
        }
        let size = self.size * 2;
        let texture = atlas_texture(device, size);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("grow atlas") });
        let old = wgpu::Extent3d { width: self.size, height: self.size, depth_or_array_layers: 1 };
        encoder.copy_texture_to_texture(self.texture.as_image_copy(), texture.as_image_copy(), old);
        queue.submit(Some(encoder.finish()));
        // Each glyph keeps its pixels, now a smaller part of the whole.
        let k = self.size as f32 / size as f32;
        let kept = self.kept.iter_mut().flat_map(|f| f.map.values_mut());
        for info in self.map.values_mut().chain(kept).chain(self.images.values_mut()).flatten() {
            info.uv = info.uv.map(|v| v * k);
        }
        log::info!("glyph atlas grown to {size} px");
        self.view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.texture = texture;
        self.size = size;
    }

    /// Starts the texture over: every font's glyphs and every picture are
    /// drawn again as they're needed.
    fn clear(&mut self) {
        self.map.clear();
        for face in &mut self.kept {
            face.map.clear();
        }
        self.images.clear();
        self.cursor_x = 1;
        self.cursor_y = 1;
        self.shelf_h = 0;
    }

    /// Keeps a premultiplied RGBA picture to draw with [`Atlas::image`].
    pub fn register_image(&mut self, id: u32, rgba: Vec<u8>, w: u32, h: u32) {
        self.image_sources.insert(id, (rgba, w, h));
        self.images.remove(&id);
    }

    pub fn image(&mut self, queue: &wgpu::Queue, id: u32) -> Option<GlyphInfo> {
        if let Some(info) = self.images.get(&id) {
            return *info;
        }
        let (rgba, w, h) = self.image_sources.get(&id)?.clone();
        let info = self.allocate(w, h).map(|(x, y)| {
            self.upload(queue, x, y, w, h, &rgba);
            let s = self.size as f32;
            GlyphInfo {
                uv: [x as f32 / s, y as f32 / s, (x + w) as f32 / s, (y + h) as f32 / s],
                offset: [0.0, 0.0],
                size: [w as f32, h as f32],
                color: true,
            }
        });
        self.images.insert(id, info);
        info
    }

    /// Writes the bitmap with a clear 1px frame over the gap around it. Text
    /// rides the liquid at fractions of a pixel, so filtering reads that gap,
    /// which after a reset may still hold a glyph of the previous font or size.
    fn upload(&self, queue: &wgpu::Queue, x: u32, y: u32, w: u32, h: u32, rgba: &[u8]) {
        let (fw, fh) = (w + 2, h + 2);
        let row = (w * 4) as usize;
        let mut framed = vec![0u8; (fw * fh * 4) as usize];
        for (r, src) in rgba.chunks(row).take(h as usize).enumerate() {
            let at = ((r + 1) * fw as usize + 1) * 4;
            framed[at..at + src.len()].copy_from_slice(src);
        }
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x: x - 1, y: y - 1, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            &framed,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(fw * 4), rows_per_image: Some(fh) },
            wgpu::Extent3d { width: fw, height: fh, depth_or_array_layers: 1 },
        );
    }

    /// The id that draws `ch` with those of `marks` that are drawn
    /// (`combining::drawn`) in a [`GlyphKey`], or 0 where none are (or the
    /// atlas keeps `MOST_MARKED` already).
    pub fn marked(&mut self, ch: char, marks: &[char]) -> u32 {
        let mut text = String::from(ch);
        text.extend(crate::combining::drawn(marks));
        if text.chars().count() == 1 {
            return 0;
        }
        if let Some(&id) = self.marked_ids.get(&text) {
            return id;
        }
        if self.marked_text.len() >= MOST_MARKED {
            return 0;
        }
        self.marked_text.push(text.clone());
        let id = self.marked_text.len() as u32;
        self.marked_ids.insert(text, id);
        id
    }

    pub fn glyph(&mut self, queue: &wgpu::Queue, key: GlyphKey) -> Option<GlyphInfo> {
        if let Some(info) = self.map.get(&key) {
            return *info;
        }
        let info = self.rasterize(queue, key);
        self.map.insert(key, info);
        info
    }

    fn rasterize(&mut self, queue: &wgpu::Queue, key: GlyphKey) -> Option<GlyphInfo> {
        if key.ch == ' ' || key.ch == '\0' || key.ch.is_control() {
            return None;
        }
        let m = self.metrics;
        let (font_size, line_h) = if key.size > 0 {
            (key.size as f32, (key.size as f32 * self.line_height_factor).round())
        } else {
            (m.font_size, m.height)
        };
        let mut buffer = Buffer::new(&mut self.font_system, Metrics::new(font_size, line_h));
        buffer.set_size(Some(font_size * 8.0), Some(line_h * 2.0));
        let named = self.family.clone();
        let family = match &named {
            Some(name) => Family::Name(name),
            None => Family::Monospace,
        };
        // Bold in a family without a bold face is its regular face, thickened below.
        let fake_bold = key.bold && !self.own_bold;
        let attrs = Attrs::new()
            .family(family)
            .weight(if key.bold && !fake_bold { Weight::BOLD } else { Weight::NORMAL })
            .style(if key.italic { Style::Italic } else { Style::Normal });
        if key.marks > 0 {
            return self.rasterize_marked(queue, key, &attrs, (font_size, line_h), fake_bold);
        }
        let mut text = [0u8; 4];
        buffer.set_text(key.ch.encode_utf8(&mut text), &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut self.font_system, false);

        let (glyph, line_y, advance) = {
            let run = buffer.layout_runs().next()?;
            let g = run.glyphs.first()?;
            (g.physical((0.0, 0.0), 1.0), run.line_y, g.w)
        };
        let image = self.swash.get_image_uncached(&mut self.font_system, glyph.cache_key)?;
        let w = image.placement.width;
        let h = image.placement.height;
        if w == 0 || h == 0 {
            return None;
        }
        let (rgba, color) = match image.content {
            SwashContent::Mask => {
                let mut out = Vec::with_capacity((w * h * 4) as usize);
                for &a in &image.data {
                    out.extend_from_slice(&[255, 255, 255, a]);
                }
                (out, false)
            }
            SwashContent::SubpixelMask => {
                let mut out = Vec::with_capacity((w * h * 4) as usize);
                for px in image.data.chunks_exact(4) {
                    let a = ((px[0] as u16 + px[1] as u16 + px[2] as u16) / 3) as u8;
                    out.extend_from_slice(&[255, 255, 255, a]);
                }
                (out, false)
            }
            SwashContent::Color => {
                let mut out = Vec::with_capacity((w * h * 4) as usize);
                for px in image.data.chunks_exact(4) {
                    let a = px[3] as u16;
                    out.extend_from_slice(&[
                        (px[0] as u16 * a / 255) as u8,
                        (px[1] as u16 * a / 255) as u8,
                        (px[2] as u16 * a / 255) as u8,
                        px[3],
                    ]);
                }
                (out, true)
            }
        };
        let (rgba, w) = if fake_bold && !color {
            embolden(&rgba, w, h, (font_size / 24.0).round().max(1.0) as u32)
        } else {
            (rgba, w)
        };

        // In a family of letters of differing widths, a one-cell letter in
        // the middle of its cell.
        let mut left = glyph.x as f32 + image.placement.left as f32;
        if self.centered && key.ch.width() == Some(1) && !crate::text::is_cjk(key.ch) {
            left += ((m.width * font_size / m.font_size - advance) * 0.5).round();
        }
        let (x, y) = self.allocate(w, h)?;
        self.upload(queue, x, y, w, h, &rgba);
        let s = self.size as f32;
        Some(GlyphInfo {
            uv: [x as f32 / s, y as f32 / s, (x + w) as f32 / s, (y + h) as f32 / s],
            offset: [left, line_y.round() + glyph.y as f32 - image.placement.top as f32],
            size: [w as f32, h as f32],
            color,
        })
    }

    /// A letter drawn with its marks, shaped together so each mark sits on
    /// it as its script has it, in its own cells: squeezed into them where
    /// the marks make it wider, as a spacing mark does on the Mac, which
    /// gives it no cell of its own (`crate::combining`).
    fn rasterize_marked(&mut self, queue: &wgpu::Queue, key: GlyphKey, attrs: &Attrs, (font_size, line_h): (f32, f32), fake_bold: bool) -> Option<GlyphInfo> {
        let text = self.marked_text.get(key.marks as usize - 1)?.clone();
        let drawn = draw_shaped(&mut self.font_system, &mut self.swash, &text, attrs, (font_size, line_h))?;
        let (rgba, w) = if fake_bold && !drawn.color {
            embolden(&drawn.rgba, drawn.w, drawn.h, (font_size / 24.0).round().max(1.0) as u32)
        } else {
            (drawn.rgba, drawn.w)
        };
        let m = self.metrics;
        let room = key.ch.width().unwrap_or(1).max(1) as f32 * m.width * font_size / m.font_size;
        let squeeze = if drawn.advance > room { room / drawn.advance } else { 1.0 };
        let mut left = drawn.left * squeeze;
        if self.centered && drawn.advance < room {
            left += ((room - drawn.advance) * 0.5).round();
        }
        let (x, y) = self.allocate(w, drawn.h)?;
        self.upload(queue, x, y, w, drawn.h, &rgba);
        let s = self.size as f32;
        Some(GlyphInfo {
            uv: [x as f32 / s, y as f32 / s, (x + w) as f32 / s, (y + drawn.h) as f32 / s],
            offset: [left, drawn.top],
            size: [w as f32 * squeeze, drawn.h as f32],
            color: drawn.color,
        })
    }

    /// Shelf packing; starts over when the atlas fills up before it could
    /// grow (see [`Atlas::grow_if_crowded`]).
    fn allocate(&mut self, w: u32, h: u32) -> Option<(u32, u32)> {
        if w + 2 > self.size || h + 2 > self.size {
            return None;
        }
        if self.cursor_x + w + 1 > self.size {
            self.cursor_x = 1;
            self.cursor_y += self.shelf_h + 1;
            self.shelf_h = 0;
        }
        if self.cursor_y + h + 1 > self.size {
            log::info!("glyph atlas full, starting over");
            self.clear();
        }
        let pos = (self.cursor_x, self.cursor_y);
        self.cursor_x += w + 1;
        self.shelf_h = self.shelf_h.max(h);
        Some(pos)
    }
}

/// Text shaped and drawn as one picture: each glyph where the shaping put
/// it, so marks sit on their letter.
struct Shaped {
    /// RGBA: white with the coverage as alpha, or premultiplied color where
    /// any glyph is in color.
    rgba: Vec<u8>,
    w: u32,
    h: u32,
    /// Its top left, from the pen's start at the line's top, in pixels.
    left: f32,
    top: f32,
    /// How far the text moves the pen.
    advance: f32,
    color: bool,
}

/// `text` shaped in `attrs` at `font_size` on lines `line_h` tall, its
/// glyphs drawn into one picture, as `Atlas::rasterize` draws one.
fn draw_shaped(font_system: &mut FontSystem, swash: &mut SwashCache, text: &str, attrs: &Attrs, (font_size, line_h): (f32, f32)) -> Option<Shaped> {
    let mut buffer = Buffer::new(font_system, Metrics::new(font_size, line_h));
    buffer.set_size(Some(font_size * 16.0), Some(line_h * 2.0));
    buffer.set_text(text, attrs, Shaping::Advanced, None);
    buffer.shape_until_scroll(font_system, false);
    let (glyphs, line_y, advance) = {
        let run = buffer.layout_runs().next()?;
        (run.glyphs.iter().map(|g| g.physical((0.0, 0.0), 1.0)).collect::<Vec<_>>(), run.line_y.round() as i32, run.line_w)
    };
    let pieces: Vec<(i32, i32, SwashImage)> = glyphs
        .iter()
        .filter_map(|g| {
            let image = swash.get_image_uncached(font_system, g.cache_key)?;
            let p = image.placement;
            (p.width > 0 && p.height > 0).then(|| (g.x + p.left, line_y + g.y - p.top, image))
        })
        .collect();
    let left = pieces.iter().map(|p| p.0).min()?;
    let top = pieces.iter().map(|p| p.1).min()?;
    let right = pieces.iter().map(|p| p.0 + p.2.placement.width as i32).max()?;
    let bottom = pieces.iter().map(|p| p.1 + p.2.placement.height as i32).max()?;
    let (w, h) = ((right - left) as u32, (bottom - top) as u32);
    let color = pieces.iter().any(|p| matches!(p.2.content, SwashContent::Color));
    let mut rgba: Vec<u8> = if color { vec![0; (w * h * 4) as usize] } else { [255, 255, 255, 0].repeat((w * h) as usize) };
    for (x0, y0, image) in &pieces {
        let (pw, ph) = (image.placement.width as usize, image.placement.height as usize);
        for y in 0..ph {
            for x in 0..pw {
                let i = y * pw + x;
                // Premultiplied, a mask's coverage as white.
                let [r, g, b, a] = match image.content {
                    SwashContent::Mask => [image.data[i]; 4],
                    SwashContent::SubpixelMask => {
                        let px = &image.data[i * 4..i * 4 + 3];
                        [((px[0] as u16 + px[1] as u16 + px[2] as u16) / 3) as u8; 4]
                    }
                    SwashContent::Color => {
                        let px = &image.data[i * 4..i * 4 + 4];
                        let a = px[3] as u16;
                        [(px[0] as u16 * a / 255) as u8, (px[1] as u16 * a / 255) as u8, (px[2] as u16 * a / 255) as u8, px[3]]
                    }
                };
                let at = (((y0 - top) as usize + y) * w as usize + (x0 - left) as usize + x) * 4;
                let out = &mut rgba[at..at + 4];
                if color {
                    // Over what's drawn already.
                    let keep = 255 - a as u16;
                    for (o, v) in out.iter_mut().zip([r, g, b, a]) {
                        *o = (v as u16 + *o as u16 * keep / 255).min(255) as u8;
                    }
                } else {
                    out[3] = out[3].max(a);
                }
            }
        }
    }
    Some(Shaped { rgba, w, h, left: left as f32, top: top as f32, advance, color })
}

/// An empty atlas `size` pixels square. It can be copied from, to grow.
fn atlas_texture(device: &wgpu::Device, size: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("glyph atlas"),
        size: wgpu::Extent3d { width: size, height: size, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

/// The first of `wanted` that is installed and draws its own letters. One
/// that can't (symbols or bitmaps only) would leave every letter to a
/// proportional fallback, so the next one is tried instead.
fn pick_family(fs: &FontSystem, wanted: &[String]) -> Option<String> {
    for want in wanted {
        let want_l = want.to_lowercase();
        let found = fs.db().faces().find_map(|face| face.families.iter().find(|(n, _)| n.to_lowercase() == want_l));
        if let Some((name, _)) = found {
            if regular_face(fs, name).is_some() {
                return Some(name.clone());
            }
        }
    }
    None
}

/// The family's own name, as the first face that has it spells it.
fn installed_family(fs: &FontSystem, want: &str) -> Option<String> {
    let want = want.to_lowercase();
    let face = fs.db().faces().find(|face| face.families.iter().any(|(n, _)| n.to_lowercase() == want))?;
    face.families.first().map(|(n, _)| n.clone())
}

/// Families with a monospaced face whose regular text is monospaced too,
/// once each. Names starting with a dot are macOS's private system fonts,
/// which apps aren't meant to pick.
fn monospace_families(fs: &FontSystem) -> Vec<String> {
    let mut names: Vec<String> = fs
        .db()
        .faces()
        .filter(|face| face.monospaced && !face.post_script_name.contains("Emoji"))
        .filter_map(|face| face.families.first().map(|(n, _)| n.clone()))
        .filter(|n| !n.starts_with('.'))
        .collect();
    names.sort();
    names.dedup();
    names.retain(|n| regular_face(fs, n) == Some(true));
    names
}

/// Whether the face cosmic-text draws `family`'s regular text with has
/// letters of one width, or None when it has no letters (a symbols or
/// bitmap-only font) or no regular weight, which leaves the text to some
/// fallback. The `monospaced` flag can't tell: it is per face, and Osaka's
/// flagged face isn't the one its name picks.
fn regular_face(fs: &FontSystem, family: &str) -> Option<bool> {
    let db = fs.db();
    let query = fontdb::Query { families: &[Family::Name(family)], weight: Weight::NORMAL, ..Default::default() };
    let id = db.query(&query)?;
    let regular = db.face(id)?.weight == Weight::NORMAL;
    db.with_face_data(id, |data, index| {
        let font = skrifa::FontRef::from_index(data, index).ok()?;
        // Like cosmic-text: another weight only counts if it is variable and reaches 400.
        let wght = font.axes().get_by_tag(Tag::new(b"wght"));
        if !regular && !wght.is_some_and(|a| a.min_value() <= 400.0 && a.max_value() >= 400.0) {
            return None;
        }
        let (charmap, metrics) = (font.charmap(), font.glyph_metrics(Size::unscaled(), LocationRef::default()));
        let advance = |ch| metrics.advance_width(charmap.map(ch)?);
        Some(advance('i')? == advance('M')?)
    })?
}

/// The family that draws an `M` (the picked one, or what cosmic-text falls
/// back to when none is installed), and whether bold text stays in it.
fn resolve(fs: &mut FontSystem, family: Option<&str>) -> (Option<String>, bool) {
    let regular = shaped_face(fs, family, Weight::NORMAL).and_then(|id| family_of(fs, id));
    let bold = shaped_face(fs, family, Weight::BOLD).and_then(|id| family_of(fs, id));
    let own_bold = bold.is_none() || bold == regular;
    (regular, own_bold)
}

fn shaped_face(fs: &mut FontSystem, family: Option<&str>, weight: Weight) -> Option<fontdb::ID> {
    let mut buffer = Buffer::new(fs, Metrics::new(16.0, 20.0));
    let fam = match family {
        Some(name) => Family::Name(name),
        None => Family::Monospace,
    };
    buffer.set_text("M", &Attrs::new().family(fam).weight(weight), Shaping::Advanced, None);
    buffer.shape_until_scroll(fs, false);
    let id = buffer.layout_runs().next()?.glyphs.first()?.font_id;
    Some(id)
}

fn family_of(fs: &FontSystem, id: fontdb::ID) -> Option<String> {
    fs.db().face(id)?.families.first().map(|(n, _)| n.clone())
}

/// Thickens a mask glyph (white RGBA) by smearing it `n` pixels to the
/// right, the way terminals fake bold for fonts that have none.
fn embolden(rgba: &[u8], w: u32, h: u32, n: u32) -> (Vec<u8>, u32) {
    if w == 0 {
        return (rgba.to_vec(), w);
    }
    let wide = w + n;
    let mut out = vec![255u8; (wide * h * 4) as usize];
    for y in 0..h {
        for x in 0..wide {
            let from = x.saturating_sub(n);
            let to = x.min(w - 1);
            let a = (from..=to).map(|sx| rgba[((y * w + sx) * 4 + 3) as usize]).max().unwrap_or(0);
            out[((y * wide + x) * 4 + 3) as usize] = a;
        }
    }
    (out, wide)
}

fn measure(fs: &mut FontSystem, family: Option<&str>, font_px: f32, line_height: f32) -> CellMetrics {
    let height = (font_px * line_height).round().max(1.0);
    let mut buffer = Buffer::new(fs, Metrics::new(font_px, height));
    buffer.set_size(Some(font_px * 40.0), Some(height * 2.0));
    let fam = match family {
        Some(name) => Family::Name(name),
        None => Family::Monospace,
    };
    let attrs = Attrs::new().family(fam);
    buffer.set_text("MMMMMMMMMM", &attrs, Shaping::Advanced, None);
    buffer.shape_until_scroll(fs, false);
    let (width, baseline) = buffer
        .layout_runs()
        .next()
        .map(|run| (run.line_w / 10.0, run.line_y))
        .unwrap_or((font_px * 0.6, font_px));
    // The x's ink from the baseline up, and the l's stem, where the font
    // has them.
    let x_height = ink(fs, &mut buffer, &attrs, "x")
        .map(|image| image.placement.top as f32)
        .filter(|&top| top > 0.0 && top < font_px)
        .unwrap_or(font_px * 0.5);
    let stem = ink(fs, &mut buffer, &attrs, "l").map(|image| stem_width(&image)).unwrap_or(0.0);
    CellMetrics { width: width.max(1.0), height, baseline: baseline.round(), font_size: font_px, x_height, stem }
}

/// `text`'s first glyph drawn alone at the buffer's size, where a font has
/// it.
fn ink(fs: &mut FontSystem, buffer: &mut Buffer, attrs: &Attrs, text: &str) -> Option<SwashImage> {
    buffer.set_text(text, attrs, Shaping::Advanced, None);
    buffer.shape_until_scroll(fs, false);
    let glyph = buffer.layout_runs().next()?.glyphs.first().filter(|g| g.glyph_id != 0)?.physical((0.0, 0.0), 1.0);
    SwashCache::new().get_image_uncached(fs, glyph.cache_key)
}

/// How wide a stem's ink is, in pixels, across the middle third of the
/// glyph's rows on average (an l's, below its top's serif and above its
/// foot), its smoothed edges counted by how much they cover.
fn stem_width(image: &SwashImage) -> f32 {
    let (w, h) = (image.placement.width as usize, image.placement.height as usize);
    // Each pixel's bytes, and which of them says how much it covers (a
    // subpixel mask's green, its middle third).
    let (step, at) = match image.content {
        SwashContent::Mask => (1, 0),
        SwashContent::SubpixelMask => (4, 1),
        SwashContent::Color => (4, 3),
    };
    let rows = h / 3..(2 * h).div_ceil(3);
    if w == 0 || rows.is_empty() || image.data.len() < w * h * step {
        return 0.0;
    }
    let count = rows.len() as f32;
    let ink: u32 = rows.flat_map(|y| (0..w).map(move |x| (y * w + x) * step + at)).map(|i| u32::from(image.data[i])).sum();
    ink as f32 / 255.0 / count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "draws on the GPU"]
    fn a_font_switched_back_to_draws_with_its_glyphs_where_they_were() {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let Ok(adapter) = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())) else { return };
        let required_limits = crate::render::limits(&adapter);
        let Ok((device, queue)) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor { required_limits, ..Default::default() })) else { return };
        let key = GlyphKey { ch: 'A', bold: false, italic: false, size: 0, marks: 0 };
        let (vt, bedstead) = (["VT323".to_string()], ["Bedstead".to_string()]);
        let mut atlas = Atlas::new(&device, &vt, 32.0, 1.2);
        let first = atlas.glyph(&queue, key).unwrap();
        let cell = atlas.metrics;
        // Another font's cell, asked for without drawing in it, leaves the
        // one in use as it is.
        let other = atlas.metrics_for(&bedstead, 32.0, 1.2);
        assert_eq!((atlas.family_in_use(), atlas.metrics.width), (Some("VT323"), cell.width));
        assert_ne!(other.width, cell.width);
        // Switched to, it draws its own A elsewhere in the texture, in the
        // cell it was measured with.
        atlas.set_font(&bedstead, 32.0, 1.2);
        assert_eq!((atlas.family_in_use(), atlas.metrics.width), (Some("Bedstead"), other.width));
        let theirs = atlas.glyph(&queue, key).unwrap();
        assert_ne!(theirs.uv, first.uv);
        // Back, the first A is where it was, and so is its cell; so is
        // Bedstead's after a size and back.
        atlas.set_font(&vt, 32.0, 1.2);
        assert_eq!((atlas.metrics.width, atlas.glyph(&queue, key).unwrap().uv), (cell.width, first.uv));
        atlas.set_font(&bedstead, 32.0, 1.2);
        atlas.set_font_size(40.0);
        assert_ne!(atlas.metrics.width, other.width);
        atlas.set_font_size(32.0);
        assert_eq!(atlas.glyph(&queue, key).unwrap().uv, theirs.uv);
        // Grown, every font's glyphs move with the texture.
        atlas.cursor_y = atlas.size;
        atlas.grow_if_crowded(&device, &queue);
        atlas.set_font(&vt, 32.0, 1.2);
        assert_eq!(atlas.glyph(&queue, key).unwrap().uv, first.uv.map(|v| v / 2.0));
    }

    #[test]
    fn a_letter_is_drawn_with_its_marks_shaped_on_it() {
        let (mut fonts, mut swash) = (font_system(), SwashCache::new());
        let attrs = Attrs::new().family(Family::Monospace);
        let mut draw = |text: &str| draw_shaped(&mut fonts, &mut swash, text, &attrs, (32.0, 40.0)).unwrap();
        let (e, accented) = (draw("e"), draw("e\u{0301}"));
        // The accent over the e: drawn higher, the pen moved no further.
        assert!(accented.top < e.top, "{} {}", accented.top, e.top);
        assert!((accented.advance - e.advance).abs() < 0.5, "{} {}", accented.advance, e.advance);
        assert!(!accented.color && accented.rgba.chunks(4).all(|p| p[..3] == [255, 255, 255]));
    }

    /// Devanagari as Flow draws it on the Mac, each letter with its marks in
    /// its own cell. With `FLOW_GLYPH_PICTURES` set to a folder, the line is
    /// saved there as `devanagari.png`, to look at.
    #[test]
    #[cfg(target_os = "macos")]
    fn devanagaris_signs_are_drawn_on_their_letters_each_in_its_cell() {
        let (mut fonts, mut swash) = (font_system(), SwashCache::new());
        let attrs = Attrs::new().family(Family::Monospace);
        let m = measure(&mut fonts, None, 32.0, 1.25);
        // रा is wider than र: the sign is a spacing one, which the Mac's
        // widths put in र's cell.
        let ra = draw_shaped(&mut fonts, &mut swash, "\u{0930}", &attrs, (32.0, m.height)).unwrap();
        let raa = draw_shaped(&mut fonts, &mut swash, "\u{0930}\u{093E}", &attrs, (32.0, m.height)).unwrap();
        assert!(raa.advance > ra.advance + 4.0, "{} {}", raa.advance, ra.advance);
        let line = "नमस्ते, मेरा नाम साई है। e\u{0301}";
        let mut cells: Vec<String> = Vec::new();
        for c in line.chars() {
            match cells.last_mut() {
                Some(cell) if c.width() == Some(0) || crate::combining::is_mac_spacing_mark(c) => cell.push(c),
                _ => cells.push(c.to_string()),
            }
        }
        let (cw, ch) = (m.width.round() as usize, m.height.round() as usize);
        let (w, h) = (cw * cells.len(), ch);
        let mut picture = vec![255u8; w * h];
        for (i, cell) in cells.iter().enumerate() {
            let Some(drawn) = draw_shaped(&mut fonts, &mut swash, cell, &attrs, (32.0, m.height)) else { continue };
            let squeeze = if drawn.advance > m.width { m.width / drawn.advance } else { 1.0 };
            let (dw, dh) = ((drawn.w as f32 * squeeze).round() as usize, drawn.h as usize);
            for y in 0..dh {
                for x in 0..dw {
                    let from = ((x as f32 / squeeze) as usize).min(drawn.w as usize - 1);
                    let a = drawn.rgba[(y * drawn.w as usize + from) * 4 + 3];
                    let (px, py) = ((i * cw) as isize + (drawn.left * squeeze) as isize + x as isize, drawn.top as isize + y as isize);
                    if (0..w as isize).contains(&px) && (0..h as isize).contains(&py) {
                        let at = py as usize * w + px as usize;
                        picture[at] = picture[at].min(255 - a);
                    }
                }
            }
        }
        assert!(picture.iter().any(|&p| p < 128));
        if let Some(dir) = std::env::var_os("FLOW_GLYPH_PICTURES") {
            let path = std::path::Path::new(&dir).join("devanagari.png");
            image::GrayImage::from_raw(w as u32, h as u32, picture).unwrap().save(&path).unwrap();
        }
    }

    fn alphas(rgba: &[u8]) -> Vec<u8> {
        rgba.iter().skip(3).step_by(4).copied().collect()
    }

    #[test]
    fn a_fonts_stems_are_measured_from_its_l_and_its_x_height_from_its_x() {
        let mut db = fontdb::Database::new();
        load_built_in(&mut db);
        let mut fs = FontSystem::new_with_locale_and_db("en-US".into(), db);
        // VT323 at 22 points on a Retina screen: stems nearly four pixels
        // wide in cells under eighteen, so box drawing's lines are four
        // pixels too, not an eighth of the cell.
        let vt = measure(&mut fs, Some("VT323"), 44.0, 1.25);
        assert!((3.5..4.0).contains(&vt.stem), "{vt:?}");
        assert_eq!(crate::box_drawing::stroke(vt.width, vt.stem), 4.0);
        // Gallant's are two of its pixels, which are two of the screen's.
        let gallant = measure(&mut fs, Some("Gallant Raster Term"), 44.0, 1.25);
        assert!((gallant.stem - 4.0).abs() < 0.1, "{gallant:?}");
        assert_eq!((vt.x_height, gallant.x_height), (18.0, 20.0));
    }

    #[test]
    fn the_built_in_fonts_are_found_by_the_names_the_themes_give() {
        let mut db = fontdb::Database::new();
        load_built_in(&mut db);
        let fs = FontSystem::new_with_locale_and_db("en-US".into(), db);
        assert_eq!(pick_family(&fs, &["Not A Font".into(), "vt323".into()]).as_deref(), Some("VT323"));
        assert_eq!(pick_family(&fs, &["Bedstead".into()]).as_deref(), Some("Bedstead"));
        assert_eq!(pick_family(&fs, &["Gallant Raster Term".into()]).as_deref(), Some("Gallant Raster Term"));
        assert_eq!(pick_family(&fs, &["unscii".into(), "unscii-8".into()]).as_deref(), Some("unscii-8"), "neither size goes by the name both give");
        assert_eq!(pick_family(&fs, &["unscii-16".into()]).as_deref(), Some("unscii-16"));
        assert_eq!(pick_family(&fs, &["Screen".into(), "SGI Screen".into()]).as_deref(), Some("SGI Screen"));
        assert_eq!(pick_family(&fs, &["Flow Teletype".into(), "Special Elite".into()]).as_deref(), Some("Flow Teletype"));
        assert_eq!(regular_face(&fs, "Flow Teletype"), Some(true), "the teletype's own letters, one width, set at the left of their cells");
        assert_eq!(pick_family(&fs, &["Special Elite".into()]).as_deref(), Some("Special Elite"));
        assert_eq!(regular_face(&fs, "Special Elite"), Some(false), "a typewriter's letters, of differing widths, set in the middle of their cells");
        assert_eq!(pick_family(&fs, &["Flow Instruments".into(), "unscii-8".into()]).as_deref(), Some("Flow Instruments"));
        assert_eq!(regular_face(&fs, "Flow Instruments"), Some(true));
        assert_eq!(pick_family(&fs, &["Flow notaTopaz".into(), "unscii-16".into()]).as_deref(), Some("Flow notaTopaz"));
        assert_eq!(regular_face(&fs, "Flow notaTopaz"), Some(true));
        assert_eq!(monospace_families(&fs), ["Bedstead", "Flow Instruments", "Flow Teletype", "Flow notaTopaz", "Gallant Raster Term", "SGI Screen", "VT323", "unscii-16", "unscii-8"]);
    }

    #[test]
    fn the_sixteen_bit_home_computers_font_draws_ascii_in_cells_of_tall_dots() {
        let font = skrifa::FontRef::new(include_bytes!("../../../assets/fonts/flow-notatopaz.ttf")).expect("a font");
        let (charmap, metrics) = (font.charmap(), font.glyph_metrics(Size::unscaled(), LocationRef::default()));
        let missing: String = (' '..='~').chain(['©']).filter(|&c| charmap.map(c).is_none()).collect();
        assert_eq!(missing, "", "drawn by fallback fonts in other letters");
        let widths: std::collections::BTreeSet<_> = charmap.mappings().filter_map(|(_, id)| metrics.advance_width(id)).map(|w| w as u32).collect();
        let upem = font.metrics(Size::unscaled(), LocationRef::default()).units_per_em as f32;
        // Eight dots across half an em, eight down the em: each dot twice
        // as tall as it's wide, as the machine's screen had them.
        assert_eq!(widths.into_iter().collect::<Vec<_>>(), [upem as u32 / 2], "every glyph a cell half as wide as it is tall");
        let (dot_w, dot_h) = (upem / 16.0, upem / 8.0);
        // Its capitals seven rows tall on the line, their uprights two dots
        // wide; tails go into the eighth row, below the line.
        let a = metrics.bounds(charmap.map('A').expect("an A")).expect("its box");
        assert_eq!((a.y_min, a.y_max, a.x_max - a.x_min), (0.0, dot_h * 7.0, dot_w * 7.0));
        let l = metrics.bounds(charmap.map('|').expect("a bar")).expect("its box");
        assert_eq!(l.x_max - l.x_min, dot_w * 2.0);
        let g = metrics.bounds(charmap.map('g').expect("a g")).expect("its box");
        assert_eq!((g.y_min, g.y_max), (-dot_h, dot_h * 5.0));
        // Its lines' metrics are the cell's, the OS/2 table whole.
        let line = font.metrics(Size::unscaled(), LocationRef::default());
        assert_eq!((line.ascent, line.descent, line.leading), (dot_h * 7.0, -dot_h, 0.0));
        assert_eq!((line.x_height, line.cap_height), (Some(dot_h * 5.0), Some(dot_h * 7.0)));
    }

    #[test]
    fn the_home_computers_font_draws_ascii_in_square_cells_of_eight_dots() {
        let font = skrifa::FontRef::new(include_bytes!("../../../assets/fonts/flow-instruments.ttf")).expect("a font");
        let (charmap, metrics) = (font.charmap(), font.glyph_metrics(Size::unscaled(), LocationRef::default()));
        let missing: String = (' '..='~').chain(['©']).filter(|&c| charmap.map(c).is_none()).collect();
        assert_eq!(missing, "", "drawn by fallback fonts in other letters");
        let widths: std::collections::BTreeSet<_> = charmap.mappings().filter_map(|(_, id)| metrics.advance_width(id)).map(|w| w as u32).collect();
        let upem = font.metrics(Size::unscaled(), LocationRef::default()).units_per_em as u32;
        assert_eq!(widths.into_iter().collect::<Vec<_>>(), [upem], "every glyph a cell as wide as it is tall");
        // Its capitals seven dots of eight tall, standing on the line.
        let a = metrics.bounds(charmap.map('A').expect("an A")).expect("its box");
        assert_eq!((a.y_min, a.y_max), (0.0, upem as f32 * 7.0 / 8.0));
    }

    #[test]
    fn the_teletypes_font_draws_ascii_latin_1_and_its_arrows_all_one_width() {
        let font = skrifa::FontRef::new(include_bytes!("../../../assets/fonts/flow-teletype.ttf")).expect("a font");
        let (charmap, metrics) = (font.charmap(), font.glyph_metrics(Size::unscaled(), LocationRef::default()));
        let wanted = (' '..='~').chain('\u{A0}'..='\u{FF}').chain(['←', '↑', '→', '↓']);
        let missing: String = wanted.filter(|&c| charmap.map(c).is_none()).collect();
        assert_eq!(missing, "", "drawn by fallback fonts in other letters");
        let widths: std::collections::BTreeSet<_> = charmap.mappings().filter_map(|(_, id)| metrics.advance_width(id)).map(|w| w as u32).collect();
        assert_eq!(widths.into_iter().collect::<Vec<_>>(), [600], "every glyph in a cell of the same width");
    }

    #[test]
    fn fake_bold_widens_each_stroke_to_the_right() {
        // Two rows of three pixels: a thin stroke in the middle, then a dot on the left.
        let mask: Vec<u8> = [0u8, 200, 0, 90, 0, 0].iter().flat_map(|&a| [255, 255, 255, a]).collect();
        let (out, w) = embolden(&mask, 3, 2, 1);
        assert_eq!(w, 4);
        assert_eq!(alphas(&out), [0, 200, 200, 0, 90, 90, 0, 0]);
        assert!(out.iter().enumerate().all(|(i, &v)| i % 4 == 3 || v == 255));
    }

    #[test]
    fn fake_bold_keeps_the_strongest_coverage_where_strokes_meet() {
        let mask: Vec<u8> = [40u8, 255, 10].iter().flat_map(|&a| [255, 255, 255, a]).collect();
        let (out, w) = embolden(&mask, 3, 1, 2);
        assert_eq!(w, 5);
        assert_eq!(alphas(&out), [40, 255, 255, 255, 10]);
    }
}
