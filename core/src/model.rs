//! 3D models in the terminal, through the Ratty graphics protocol (RGP),
//! as the Ratty terminal speaks it, so a program written for one works in
//! both: `ESC _ ratty;g;<verb>;<key=value>… ESC \`, where `s` asks what's
//! supported, `r` registers an object (`.obj` or `.glb`, sent in base64
//! chunks or named by a path), `p` places it centered on a cell over `w` ×
//! `h` cells, `u` changes how a placed object looks, and `d` deletes one or
//! all. The spec: <https://docs.rs/crate/ratty/0.4.1/source/protocols/graphics.md>.
//!
//! A placement is an image in the session's `ImageStore` whose pixels the
//! renderer draws from the mesh (`render/models.rs`), shown by placeholder
//! cells like any other image, so it scrolls, clears and goes into the
//! scrollback with the text and bends with the liquid. Where Ratty's own
//! renderer leaves things open, Flow follows what Ratty does: rows and
//! columns count from 0 on the screen, an OBJ is centered and scaled to a
//! unit box, `animate=1` spins, tumbles and bobs, `depth` gives flat artwork
//! thickness and an oblique view, `color` tints an OBJ, and `brightness`
//! scales every color. A model named by a path is read only once the user
//! says yes ([`Ask`]).
//!
//! Flow adds two keys of its own to `p` and `u`, which a terminal that
//! doesn't know them ignores, and says so in its answer to `s`: `clip=<n>`
//! plays a glTF's animation `n` (from 0, as the file lists them; the first
//! unless asked, looped; `clip=-1` holds it at rest; naming it again starts
//! it over), and `clicks=1` asks for the user's clicks on the model
//! ([`pick`]). A glTF's own materials (metal and roughness, normal maps,
//! emission, occlusion, cut-outs and see-through parts, a second set of
//! texture coordinates and moved textures, and the unlit, specular, ior,
//! clearcoat, sheen, transmission and volume extensions and the old
//! specular-glossiness material, any of which a file may require), its
//! vertex colors, its nodes' instances (GPU instancing, each a copy of the
//! node's mesh), its lights and camera (a perspective one looked through,
//! [`Lens`]), and its motion (nodes, skins, morph targets, and the lights,
//! camera and instances on them) are read by [`glb`] and played by
//! [`anim`], its bytes however they're packed (meshopt, Draco, quantized
//! attributes, WebP and KTX2 textures) by [`buffers`], [`meshopt`] and
//! [`draco`]; see-through triangles are cut
//! where they cross, as each frame poses them where the mesh moves, and
//! drawn farthest first by [`order`].

mod anim;
mod buffers;
mod draco;
mod glb;
mod meshopt;
mod order;
mod pick;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub use anim::Rig;
pub use order::{Order, SeeThrough};
pub use pick::{answer as click_answer, Button, Hit};

use base64::Engine as _;
use bytemuck::{Pod, Zeroable};
use parking_lot::Mutex;

/// The answer to `s`: everything v1 offers.
/// Its last two fields are Flow's own: `clip` and `clicks`.
pub const SUPPORT: &str = "\x1b_ratty;g;s;v=1;fmt=obj|glb;path=1;payload=1;chunk=1;anim=1;depth=1;color=1;brightness=1;transform=1;update=1;clip=1;clicks=1\x1b\\";

/// A model's file or payload, at most.
pub const MAX_BYTES: usize = 64 << 20;
/// Triangles in a model, at most.
const MAX_TRIANGLES: usize = 2_000_000;
/// Objects registered, and placed, in a tab at most.
const MAX_OBJECTS: usize = 256;
pub const MAX_SHOWN: usize = 64;
/// A placement's size in cells, at most.
pub const MAX_SPAN: u32 = 200;
/// The pixels a tab's placements cover at most, all told: on the GPU each
/// costs some 36 bytes (multisampled color and depth, and the picture).
pub const MAX_PIXELS: u64 = 8 << 20;

/// The angles of the oblique view flat artwork with `depth` gets (radians).
const OBLIQUE: (f32, f32) = (0.75, 0.35);
/// `animate=1`: radians a second about the vertical axis (the horizontal
/// turns at 0.7 of it), the bob's pace, and its height in cells.
pub const SPIN: f32 = 1.4;
pub const BOB_SPEED: f32 = 2.2;
pub const BOB_CELLS: f32 = 0.08;
/// Flow's turning logo, clicked ([`Shown::align`]): how long it takes to
/// turn back to how the logo stands, and how long it rests there after.
pub const ALIGN_TURN: Duration = Duration::from_millis(450);
pub const ALIGN_REST: Duration = Duration::from_secs(5);
/// How long frames keep coming once it has turned back, so the last one
/// drawn shows it exactly as it stands.
const ALIGN_SETTLE: Duration = Duration::from_millis(150);

static SERIAL: AtomicU64 = AtomicU64::new(1);

/// A model's file format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Obj,
    Glb,
}

impl Format {
    fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "obj" => Some(Format::Obj),
            "glb" => Some(Format::Glb),
            _ => None,
        }
    }
}

/// Where a registration's model comes from.
#[derive(Debug, Clone, PartialEq)]
pub enum Source {
    /// A file the program names; read only once the user says yes.
    Path(String),
    /// Bytes carried in the command, maybe one chunk of several.
    Payload { more: bool, data: Vec<u8> },
}

/// How a placed object looks, as `p` sets it and `u` changes it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Style {
    pub animate: bool,
    pub scale: f32,
    /// Thickness for flat artwork, which also turns to an oblique view.
    pub depth: f32,
    pub color: Option<[u8; 3]>,
    pub brightness: f32,
    /// In pixels, y up; z only moves it nearer or further.
    pub offset: [f32; 3],
    /// Degrees about x, y and z, applied in that order.
    pub rotation: [f32; 3],
    pub scale3: [f32; 3],
    /// The glTF animation it plays, from 0; below 0, none (Flow's own).
    pub clip: i32,
    /// The program hears of clicks on it (Flow's own).
    pub clicks: bool,
}

impl Default for Style {
    fn default() -> Self {
        Self { animate: false, scale: 1.0, depth: 0.0, color: None, brightness: 1.0, offset: [0.0; 3], rotation: [0.0; 3], scale3: [1.0; 3], clip: 0, clicks: false }
    }
}

/// What a `u` changes; what it leaves out stays.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Update {
    pub animate: Option<bool>,
    pub scale: Option<f32>,
    pub depth: Option<f32>,
    pub color: Option<[u8; 3]>,
    pub brightness: Option<f32>,
    pub offset: [Option<f32>; 3],
    pub rotation: [Option<f32>; 3],
    pub scale3: [Option<f32>; 3],
    pub clip: Option<i32>,
    pub clicks: Option<bool>,
}

impl Style {
    pub fn apply(&mut self, u: &Update) {
        self.animate = u.animate.unwrap_or(self.animate);
        self.clip = u.clip.unwrap_or(self.clip);
        self.clicks = u.clicks.unwrap_or(self.clicks);
        self.scale = u.scale.unwrap_or(self.scale);
        self.depth = u.depth.unwrap_or(self.depth);
        self.color = u.color.or(self.color);
        self.brightness = u.brightness.unwrap_or(self.brightness);
        for i in 0..3 {
            self.offset[i] = u.offset[i].unwrap_or(self.offset[i]);
            self.rotation[i] = u.rotation[i].unwrap_or(self.rotation[i]);
            self.scale3[i] = u.scale3[i].unwrap_or(self.scale3[i]);
        }
    }
}

/// One command, as its fields say.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Support,
    Register { id: u32, format: Format, source: Source },
    /// Centered on `row`, `col` (from 0, on the screen), over `cols` × `rows` cells.
    Place { id: u32, row: u32, col: u32, cols: u32, rows: u32, style: Style },
    Update { id: u32, update: Update },
    /// One object, or every one.
    Delete(Option<u32>),
}

/// Reads a command: `body` is what follows `ESC _ ratty;g;`. As Ratty
/// reads it, a payload's base64 is the first field without a `=` (or with a
/// key it doesn't know), and a command missing what its verb needs is
/// nothing.
pub fn parse(body: &[u8]) -> Option<Command> {
    let text = std::str::from_utf8(body).ok()?;
    let mut parts = text.split(';');
    let verb = parts.next()?;
    let mut f: HashMap<&str, &str> = HashMap::new();
    let mut payload = None;
    for part in parts.filter(|p| !p.is_empty()) {
        let payload_follows = verb == "r" && f.get("source") == Some(&"payload");
        match part.split_once('=') {
            Some((key, value)) if KEYS.contains(&key) => {
                f.insert(key, value);
            }
            // Base64 can end in `=`, so a payload may read as key=value.
            _ if payload_follows => {
                payload = Some(part);
                break;
            }
            _ => {}
        }
    }
    let num = |k: &str| f.get(k).and_then(|v| v.parse::<f32>().ok());
    let int = |k: &str| f.get(k).and_then(|v| v.parse::<u32>().ok());
    let flag = |k: &str| {
        f.get(k).and_then(|v| match *v {
            "1" | "true" => Some(true),
            "0" | "false" => Some(false),
            _ => None,
        })
    };
    let color = f.get("color").or(f.get("tint")).and_then(|v| parse_color(v));
    let clip = f.get("clip").and_then(|v| v.parse::<i32>().ok());
    let three = |keys: [&str; 3]| keys.map(num);
    Some(match verb {
        "s" => Command::Support,
        "r" => {
            let id = int("id")?;
            let format = Format::parse(f.get("fmt")?)?;
            let source = match f.get("path") {
                Some(path) => Source::Path(path.to_string()),
                None if f.get("source") == Some(&"payload") => {
                    let data = base64::engine::general_purpose::STANDARD.decode(payload.unwrap_or("")).ok()?;
                    Source::Payload { more: flag("more").unwrap_or(false), data }
                }
                None => return None,
            };
            Command::Register { id, format, source }
        }
        "p" => {
            let d = Style::default();
            let or = |v: [Option<f32>; 3], d: [f32; 3]| [0, 1, 2].map(|i| v[i].unwrap_or(d[i]));
            Command::Place {
                id: int("id")?,
                row: int("row")?,
                col: int("col")?,
                cols: int("w")?,
                rows: int("h")?,
                style: Style {
                    animate: flag("animate").unwrap_or(false),
                    scale: num("scale").unwrap_or(d.scale),
                    depth: num("depth").unwrap_or(d.depth),
                    color,
                    brightness: num("brightness").unwrap_or(d.brightness),
                    offset: or(three(["px", "py", "pz"]), d.offset),
                    rotation: or(three(["rx", "ry", "rz"]), d.rotation),
                    scale3: or(three(["sx", "sy", "sz"]), d.scale3),
                    clip: clip.unwrap_or(d.clip),
                    clicks: flag("clicks").unwrap_or(d.clicks),
                },
            }
        }
        "u" => Command::Update {
            id: int("id")?,
            update: Update {
                animate: flag("animate"),
                scale: num("scale"),
                depth: num("depth"),
                color,
                brightness: num("brightness"),
                offset: three(["px", "py", "pz"]),
                rotation: three(["rx", "ry", "rz"]),
                scale3: three(["sx", "sy", "sz"]),
                clip,
                clicks: flag("clicks"),
            },
        },
        "d" => Command::Delete(int("id")),
        _ => return None,
    })
}

/// The fields any verb takes, Flow's own last.
const KEYS: [&str; 27] = [
    "id", "fmt", "path", "source", "more", "name", "row", "col", "w", "h", "animate", "scale", "depth", "color", "tint", "brightness", "px",
    "py", "pz", "rx", "ry", "rz", "sx", "sy", "sz", "clip", "clicks",
];

fn parse_color(v: &str) -> Option<[u8; 3]> {
    let v = v.strip_prefix('#').unwrap_or(v);
    if v.len() != 6 || !v.is_ascii() {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&v[i..i + 2], 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?])
}

/// A vertex as the GPU takes it.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    /// Along the texture's u, and which side glTF's bitangent (`cross(normal,
    /// tangent) * w`) takes, for a normal map; all 0 where there's none.
    pub tangent: [f32; 4],
    /// The second set of texture coordinates (glTF's `TEXCOORD_1`), for a
    /// texture that reads it; the first set where the file has no second.
    pub uv1: [f32; 2],
    /// Its color (glTF's `COLOR_0`), which the base color is multiplied
    /// by: RGBA as a texture's texels are (display values, straight
    /// alpha), white where the file gives none, which leaves the base color
    /// as it is.
    pub color: [u8; 4],
}

impl Vertex {
    pub fn new(pos: [f32; 3], normal: [f32; 3], uv: [f32; 2]) -> Self {
        Self { pos, normal, uv, tangent: [0.0; 4], uv1: uv, color: [255; 4] }
    }
}

/// A texture's pixels, RGBA with straight alpha.
#[derive(Debug, PartialEq)]
pub struct Texture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// How a part's surface looks: a glTF's metallic-roughness material and the
/// extensions to it Flow reads, of which an OBJ, and Flow's own scenes,
/// have only a color and a gloss.
#[derive(Debug, Clone, PartialEq)]
pub struct Material {
    /// Its color as shown (RGBA, display values), before any tint.
    pub color: [f32; 4],
    pub texture: Option<Arc<Texture>>,
    /// 0 (matte) to 1: how strongly it shines with a highlight and a sheen;
    /// one less glTF's roughness.
    pub gloss: f32,
    /// 0 to 1: how much it's a metal, which takes its color into what it
    /// reflects; a rough one looks much as a matte color does.
    pub metallic: f32,
    /// glTF's metallic-roughness texture: roughness in green and metalness
    /// in blue, scaling `1 - gloss` and `metallic`.
    pub metal_rough: Option<Arc<Texture>>,
    /// A normal map (tangent space, glTF's), and how far it bends normals.
    pub bumps: Option<(Arc<Texture>, f32)>,
    /// The light it gives off (display values), times `glow` where it has one.
    pub emission: [f32; 3],
    pub glow: Option<Arc<Texture>>,
    /// glTF's occlusion map (its red: how much of the room's light reaches
    /// each point), and how strongly it darkens, 0 to 1.
    pub occlusion: Option<(Arc<Texture>, f32)>,
    /// What its alpha means.
    pub alpha: Alpha,
    /// KHR_materials_unlit: drawn in its color alone, without light, its
    /// other textures and factors left out.
    pub unlit: bool,
    /// KHR_materials_ior: its index of refraction, which sets how strongly a
    /// non-metal reflects head on; glTF's 1.5 is the glaze Flow has always
    /// drawn.
    pub ior: f32,
    pub specular: Option<Specular>,
    pub clearcoat: Option<Clearcoat>,
    pub sheen: Option<Sheen>,
    pub transmission: Option<Transmission>,
    pub anisotropy: Option<Anisotropy>,
    pub iridescence: Option<Iridescence>,
    /// The texture coordinates each texture reads, in [`Tex`]'s order (the
    /// shader's).
    pub maps: [UvMap; TEXTURES],
}

/// A material's textures, in the order of `Material::maps`, of
/// [`Material::textures`], and of the renderer's bindings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tex {
    Base,
    MetalRough,
    Normal,
    Emissive,
    Occlusion,
    Specular,
    SpecularColor,
    Coat,
    CoatRoughness,
    CoatNormal,
    SheenColor,
    SheenRoughness,
    Transmission,
    Thickness,
    Anisotropy,
    Iridescence,
}

/// How many textures a material can have: all sixteen a shader stage may
/// sample on the GPUs Flow asks least of (OpenGL ES 3.0's).
pub const TEXTURES: usize = 16;

/// KHR_materials_specular: how strongly a non-metal reflects, and in what
/// color. A metal reflects in its own color, which this leaves alone.
#[derive(Debug, Clone, PartialEq)]
pub struct Specular {
    /// 0 to 1 (`specularFactor`), times its texture's alpha.
    pub strength: f32,
    pub strength_map: Option<Arc<Texture>>,
    /// The color it reflects head on (`specularColorFactor`, display
    /// values), times its texture.
    pub color: [f32; 3],
    pub color_map: Option<Arc<Texture>>,
}

/// KHR_materials_clearcoat: a clear glossy layer over the rest, as on a
/// car's paint, glazed as Flow glazes a glossy part, over whatever the
/// rest is (a matte color, a metal), with a normal of its own.
#[derive(Debug, Clone, PartialEq)]
pub struct Clearcoat {
    /// 0 to 1 (`clearcoatFactor`), times its texture's red.
    pub strength: f32,
    pub strength_map: Option<Arc<Texture>>,
    /// One less its roughness (`clearcoatRoughnessFactor`), which its
    /// texture's green scales.
    pub gloss: f32,
    pub roughness_map: Option<Arc<Texture>>,
    /// Its own normal map and how far it bends; without one the coat
    /// follows the surface, not the base's normal map.
    pub bumps: Option<(Arc<Texture>, f32)>,
}

/// KHR_materials_sheen: the soft light a fabric's fibres catch toward its
/// edges.
#[derive(Debug, Clone, PartialEq)]
pub struct Sheen {
    /// Its color (`sheenColorFactor`, display values), times its texture.
    pub color: [f32; 3],
    pub color_map: Option<Arc<Texture>>,
    /// 0 to 1 (`sheenRoughnessFactor`), times its texture's alpha: a rough
    /// sheen spreads in from the edges, a smooth one hugs them.
    pub roughness: f32,
    pub roughness_map: Option<Arc<Texture>>,
}

/// KHR_materials_transmission, with KHR_materials_volume: light passing
/// through the part, as through glass. A model's picture is drawn on its
/// own and laid over the terminal, so what's behind the part can't be bent
/// or tinted, only seen through it: the part is see-through, under a film
/// of its color as deep as it absorbs, with its reflections kept whole.
#[derive(Debug, Clone, PartialEq)]
pub struct Transmission {
    /// 0 to 1 (`transmissionFactor`), times its texture's red.
    pub amount: f32,
    pub map: Option<Arc<Texture>>,
    /// How thick the volume is (`thicknessFactor`, the file's units),
    /// times its texture's green; 0 for a thin wall.
    pub thickness: f32,
    pub thickness_map: Option<Arc<Texture>>,
    /// The color white light takes (linear) once it has gone `distance`
    /// through the volume (the file's units; infinite, none).
    pub attenuation: [f32; 3],
    pub distance: f32,
}

/// KHR_materials_anisotropy: a surface brushed or spun one way, whose
/// highlights stretch along it. Flow's highlights aren't GGX's, so it
/// stretches them as such a surface's look: the half vector's part along
/// the way is taken down by its strength before the highlight is worked
/// out, so it lasts that much further along.
#[derive(Debug, Clone, PartialEq)]
pub struct Anisotropy {
    /// 0 to 1 (`anisotropyStrength`), times its texture's blue.
    pub strength: f32,
    /// Its way in the surface, turned from the tangent toward the
    /// bitangent (`anisotropyRotation`, radians).
    pub rotation: f32,
    /// Its way in the texture's red and green (from -1 to 1, the tangent's
    /// and the bitangent's shares), and its strength in blue.
    pub map: Option<Arc<Texture>>,
}

/// KHR_materials_iridescence: a thin film over the surface, as on a soap
/// bubble or an oil slick, its reflections colored by the film's
/// interference, which shifts as it's seen more aslant or the film is
/// thicker.
#[derive(Debug, Clone, PartialEq)]
pub struct Iridescence {
    /// 0 to 1 (`iridescenceFactor`), times its map's red.
    pub strength: f32,
    /// The film's index of refraction (`iridescenceIor`).
    pub ior: f32,
    /// How thick the film is, in nanometres: from `thinnest` to `thickest`
    /// by its map's green, `thickest` where it has none, as glTF has it.
    pub thinnest: f32,
    pub thickest: f32,
    /// Its strength (red) and thickness (green) in one texture, as the
    /// shader reads them: glTF's two maps, made one where they're two
    /// (`buffers::Images::film`).
    pub map: Option<Arc<Texture>>,
}

impl Material {
    /// Each of its textures, in [`Tex`]'s order.
    pub fn textures(&self) -> [Option<&Arc<Texture>>; TEXTURES] {
        let (s, c, sh, t) = (self.specular.as_ref(), self.clearcoat.as_ref(), self.sheen.as_ref(), self.transmission.as_ref());
        [
            self.texture.as_ref(),
            self.metal_rough.as_ref(),
            self.bumps.as_ref().map(|(t, _)| t),
            self.glow.as_ref(),
            self.occlusion.as_ref().map(|(t, _)| t),
            s.and_then(|s| s.strength_map.as_ref()),
            s.and_then(|s| s.color_map.as_ref()),
            c.and_then(|c| c.strength_map.as_ref()),
            c.and_then(|c| c.roughness_map.as_ref()),
            c.and_then(|c| c.bumps.as_ref().map(|(t, _)| t)),
            sh.and_then(|s| s.color_map.as_ref()),
            sh.and_then(|s| s.roughness_map.as_ref()),
            t.and_then(|t| t.map.as_ref()),
            t.and_then(|t| t.thickness_map.as_ref()),
            self.anisotropy.as_ref().and_then(|a| a.map.as_ref()),
            self.iridescence.as_ref().and_then(|i| i.map.as_ref()),
        ]
    }

    /// Blended over what's behind it, so drawn after the solid parts,
    /// farthest first: its alpha says so, or light passes through it.
    pub fn see_through(&self) -> bool {
        self.alpha == Alpha::Blend || (!self.unlit && self.transmission.as_ref().is_some_and(|t| t.amount > 0.0))
    }
}

impl Default for Material {
    fn default() -> Self {
        Self {
            color: [1.0; 4],
            texture: None,
            gloss: 0.0,
            metallic: 0.0,
            metal_rough: None,
            bumps: None,
            emission: [0.0; 3],
            glow: None,
            occlusion: None,
            alpha: Alpha::Opaque,
            unlit: false,
            ior: 1.5,
            specular: None,
            clearcoat: None,
            sheen: None,
            transmission: None,
            anisotropy: None,
            iridescence: None,
            maps: [UvMap::PLAIN; TEXTURES],
        }
    }
}

/// What a part's alpha means, as glTF's `alphaMode` says.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Alpha {
    /// Nothing: the part is solid all over.
    Opaque,
    /// A cut-out: solid where its alpha reaches the cutoff, gone below it.
    Mask(f32),
    /// See-through: blended over what's behind it, so drawn after the solid
    /// parts, farthest first, triangle by triangle ([`Mesh::draw_order`]).
    Blend,
}

/// Which set of texture coordinates a texture reads, and how they're moved
/// before it does (KHR_texture_transform: scaled, turned, then offset).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UvMap {
    /// 0, or 1 for `Vertex::uv1`.
    pub set: u32,
    /// The rows of a 2 × 3 matrix: each times (u, v, 1) gives u or v.
    pub m: [[f32; 3]; 2],
}

impl UvMap {
    pub const PLAIN: UvMap = UvMap { set: 0, m: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] };

    /// KHR_texture_transform's offset, rotation (radians, turning the
    /// coordinates counter-clockwise as the texture shows them, u right and
    /// v down) and scale, composed as its spec does: offset × rotation ×
    /// scale. A set past the second reads the first, the only other a
    /// vertex has.
    pub fn new(set: u32, offset: [f32; 2], rotation: f32, scale: [f32; 2]) -> UvMap {
        let (s, c) = rotation.sin_cos();
        UvMap { set: u32::from(set == 1), m: [[c * scale[0], s * scale[1], offset[0]], [-s * scale[0], c * scale[1], offset[1]]] }
    }

    /// Where `v` samples the texture.
    pub fn apply(&self, v: &Vertex) -> [f32; 2] {
        let [u, w] = if self.set == 1 { v.uv1 } else { v.uv };
        self.m.map(|r| r[0] * u + r[1] * w + r[2])
    }
}

/// Triangles that share a material.
#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub material: Material,
    /// The glTF node it's a mesh of, which a click names.
    pub node: Option<usize>,
}

impl Part {
    /// No triangles yet, all `color` with `gloss`, see-through where its
    /// alpha is short of 1.
    pub fn plain(color: [f32; 4], gloss: f32) -> Self {
        let alpha = if color[3] < 1.0 { Alpha::Blend } else { Alpha::Opaque };
        Self { vertices: Vec::new(), indices: Vec::new(), material: Material { color, gloss, alpha, ..Material::default() }, node: None }
    }
}

/// The file's lights a model keeps at most: the strongest on its middle.
pub const MAX_LIGHTS: usize = 4;

/// A light from a glTF (KHR_lights_punctual), in place of Flow's own: where
/// the mesh's unit box puts it, as the file has it at rest, or where its
/// animation has it now ([`Pose::lights`]). It stays with the view the
/// model is first seen in (the file's camera's, or Flow's front view), so a
/// model the program turns turns under it, as under Flow's own light.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Light {
    /// Where it is; None for a directional light, which is far off.
    pub at: Option<[f32; 3]>,
    /// The way it shines: its node's -z (for a directional or spot light).
    pub aim: [f32; 3],
    /// Its color as shown (display values).
    pub color: [f32; 3],
    /// Its share of the light on the model's middle, all the file's lights
    /// together giving what Flow's own does (1); as read, the file's
    /// intensity (lux, or candela for one with a place).
    pub strength: f32,
    /// How far it reaches, or None for no end.
    pub range: Option<f32>,
    /// A spot light's cone: the cosines of its inner and outer angles.
    pub cone: Option<(f32, f32)>,
    /// The glTF node it hangs from, which an animation may move.
    pub node: Option<usize>,
}

/// A glTF's perspective camera, which its model is drawn through: where
/// its eye is in the mesh's unit box (as read, the file's units), looking
/// along -z with y up, since the model is turned to face Flow's view as it
/// faces the camera; the angle it sees from the picture's bottom to its
/// top, the picture's own shape setting how wide; and the nearest it sees
/// (the box's units, as read the file's). Its aspect ratio and farthest
/// are the file's picture's, which Flow's cells aren't.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lens {
    pub eye: [f32; 3],
    pub yfov: f32,
    pub znear: f32,
}

impl Lens {
    /// How far in front of a picture `height` pixels high the eye is, in
    /// pixels, for the camera's field of view to just span it.
    pub fn focal(&self, height: f32) -> f32 {
        0.5 * height / (0.5 * self.yfov).tan()
    }

    /// The pixels a unit of the box takes where the picture shows things
    /// at its own size, and how far in front of the eye that is (the box's
    /// units): at the model's middle, or half its `radius` in front of the
    /// eye where the middle is nearer than that or behind it.
    pub fn scale(&self, height: f32, radius: f32) -> (f32, f32) {
        let depth = self.eye[2].max(0.5 * radius).max(1e-6);
        (self.focal(height) / depth, depth)
    }
}

/// How a placement's picture is drawn: where its mesh goes in it, how
/// that's laid on the picture, and where the eye is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    /// The mesh's unit box to the picture's pixels, from its middle, y up
    /// and z toward the viewer.
    pub model: [[f32; 4]; 4],
    /// The picture's pixels to clip space.
    pub projection: [[f32; 4]; 4],
    /// How far in front of the picture's middle the eye is (pixels): the
    /// camera's, or for a model drawn straight on, where its highlights
    /// are seen from.
    pub eye: f32,
    /// Seen through a camera, the nearest it draws (pixels in front of the
    /// eye); None for a model drawn straight on.
    pub near: Option<f32>,
}

/// A model, centered on the origin in a box whose longest side is 1.
#[derive(Debug, PartialEq)]
pub struct Mesh {
    /// Unique, so the GPU's copy can be found by it.
    pub serial: u64,
    pub parts: Vec<Part>,
    pub format: Format,
    /// The box's sides.
    pub extent: [f32; 3],
    /// The farthest a vertex lies from the center.
    pub radius: f32,
    /// What a glTF's animations move, where it has any.
    pub rig: Option<Rig>,
    /// Flat artwork made thick for `depth` whose glTF moves: posed as the
    /// flat mesh, then made thick again.
    pub extrusion: Option<Extrusion>,
    /// A glTF's own lights, which take the place of Flow's; none, Flow's.
    pub lights: Vec<Light>,
    /// A glTF's perspective camera, which it's seen through; none, it's
    /// drawn straight on, fitted to its cells.
    pub lens: Option<Lens>,
    /// The see-through parts' triangles as they're drawn at rest, cut where
    /// they cross ([`order`]).
    pub see_through: SeeThrough,
}

/// A mesh that moves by itself as its animation has it at one moment.
#[derive(Debug, Clone, PartialEq)]
pub struct Pose {
    /// Each part's vertices, then those its see-through triangles' cuts
    /// made, posed too, as the triangles drawn number them.
    pub vertices: Vec<Vec<Vertex>>,
    /// The see-through triangles cut where they cross as posed now, for a
    /// mesh with few enough to cut each frame; None where the mesh's own
    /// (`Mesh::see_through`, cut at rest) are drawn, their cuts posed with
    /// the vertices they lie between.
    pub see_through: Option<SeeThrough>,
    /// The file's lights where the animation has them, if it moves any.
    pub lights: Option<Vec<Light>>,
}

/// How a thick mesh was made from flat artwork that moves by its glTF's
/// animation.
#[derive(Debug, PartialEq)]
pub struct Extrusion {
    flat: Arc<Mesh>,
    half: f32,
    /// Each part's outline: the edges only one of its triangles has.
    outlines: Vec<Vec<(u32, u32)>>,
}

impl Mesh {
    /// `parts` centered and scaled into a unit box: a model read from a
    /// file, or one Flow makes itself (`logos::boing_ball`).
    pub fn new(parts: Vec<Part>, format: Format) -> Result<Self, String> {
        Self::with_rig(parts, format, None, Vec::new(), None)
    }

    /// Centered and scaled into its unit box, which for a model that moves
    /// is the room it moves in, so it neither grows nor leaves its picture
    /// as it plays; `lights` and `lens` (in the file's units) go into the
    /// box with it.
    fn with_rig(parts: Vec<Part>, format: Format, mut rig: Option<Rig>, lights: Vec<Light>, lens: Option<Lens>) -> Result<Self, String> {
        if let Some(rig) = rig.as_mut() {
            let mut keep = parts.iter().map(|p| !p.indices.is_empty());
            rig.parts.retain(|_| keep.next().unwrap_or(false));
        }
        let mut parts: Vec<Part> = parts.into_iter().filter(|p| !p.indices.is_empty()).collect();
        let (mut lo, mut hi) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
        let mut grow = |p: [f32; 3]| {
            for i in 0..3 {
                lo[i] = lo[i].min(p[i]);
                hi[i] = hi[i].max(p[i]);
            }
        };
        parts.iter().flat_map(|p| &p.vertices).for_each(|v| grow(v.pos));
        if let Some(rig) = &rig {
            rig.sweep(&parts, &mut grow);
        }
        if parts.is_empty() || !lo[0].is_finite() {
            return Err("no triangles".into());
        }
        let triangles: usize = parts.iter().map(|p| p.indices.len() / 3).sum();
        if triangles > MAX_TRIANGLES {
            return Err(format!("{triangles} triangles, more than {MAX_TRIANGLES}"));
        }
        let center = [0, 1, 2].map(|i| (lo[i] + hi[i]) * 0.5);
        let longest = (0..3).map(|i| hi[i] - lo[i]).fold(1e-6f32, f32::max);
        let mut radius = 0f32;
        for v in parts.iter_mut().flat_map(|p| &mut p.vertices) {
            v.pos = [0, 1, 2].map(|i| (v.pos[i] - center[i]) / longest);
            radius = radius.max(length(v.pos));
        }
        if let Some(rig) = rig.as_mut() {
            rig.frame = (center, longest);
            rig.sweep(&parts, |p| radius = radius.max(length(p)));
        }
        let extent = [0, 1, 2].map(|i| (hi[i] - lo[i]) / longest);
        let radius = radius.max(1e-3);
        let mut lights = settle_lights(lights, center, longest, radius);
        // A field of view of nothing, or of all round, can't be drawn.
        let lens = lens.map(|l| Lens { eye: sub(l.eye, center).map(|v| v / longest), yfov: l.yfov.clamp(1e-3, 3.1), znear: l.znear.max(0.0) / longest });
        if let Some(rig) = rig.as_mut() {
            rig.moves_lights = (0..rig.clips.len())
                .map(|c| {
                    let moved = rig.moved(c);
                    lights.iter().any(|l| l.node.is_some_and(|n| moved.get(n).copied().unwrap_or(false)))
                })
                .collect();
            settle_over_clips(rig, &mut lights, radius);
        }
        let see_through = SeeThrough::new(&parts, rig.is_some());
        Ok(Self { serial: SERIAL.fetch_add(1, Ordering::Relaxed), parts, format, extent, radius, rig, extrusion: None, lights, lens, see_through })
    }

    /// The animation `clip` names (from 0), if the mesh has it and it moves.
    pub fn playing(&self, clip: i32) -> Option<usize> {
        if let Some(e) = &self.extrusion {
            return e.flat.playing(clip);
        }
        let i = usize::try_from(clip).ok()?;
        self.rig.as_ref()?.clips.get(i).filter(|c| c.moves()).map(|_| i)
    }

    /// Its vertices can move by themselves, so a picture of it needs room
    /// for them posed.
    pub fn moves(&self) -> bool {
        self.rig.is_some() || self.extrusion.is_some()
    }

    /// How it stands `secs` into animation `clip` (looped), if it plays
    /// one; otherwise the parts' own vertices, its see-through triangles
    /// and its lights are how it stands. Its see-through triangles are cut
    /// where they cross as posed ([`SeeThrough::pose`]).
    pub fn posed(&self, clip: i32, secs: f32) -> Option<Pose> {
        let mut pose = self.pose(clip, secs)?;
        pose.see_through = self.see_through.pose(&self.parts, &mut pose.vertices);
        Some(pose)
    }

    /// Each part's own vertices `secs` into animation `clip`, and its
    /// lights then if the animation moves any; its see-through triangles
    /// not yet cut.
    fn pose(&self, clip: i32, secs: f32) -> Option<Pose> {
        if let Some(e) = &self.extrusion {
            let flat = e.flat.pose(clip, secs)?;
            return Some(Pose { vertices: flat.vertices.iter().zip(&e.outlines).map(|(v, outline)| thicken(v, outline, e.half)).collect(), ..flat });
        }
        let i = self.playing(clip)?;
        let rig = self.rig.as_ref()?;
        let (world, weights) = rig.pose(Some(i), secs);
        let lights = rig.moves_lights.get(i).copied().unwrap_or(false).then(|| rig.place_lights(&world, &self.lights));
        Some(Pose { vertices: rig.pose_parts_at(&world, &weights, &self.parts), see_through: None, lights })
    }

    /// How to draw it once `model` has placed it (z toward the viewer): the
    /// solid parts and the cut-outs first, whole, as they come, then the
    /// see-through triangles farthest first, so each blends over all that's
    /// behind it ([`order`]). `posed` is how an animation has it, if one does.
    pub fn draw_order(&self, posed: Option<&Pose>, model: [[f32; 4]; 4]) -> Order {
        order::draw_order(self, posed, model)
    }

    /// A scene laid out already in a picture `w` × `h` pixels (from its
    /// middle, y up, z nearer), drawn as it is: its box is the picture's,
    /// so placed with a scale of 1/0.9, which undoes the margin a model is
    /// fitted with, every point lands where it was put.
    pub fn scene(parts: Vec<Part>, (w, h): (f32, f32)) -> Mesh {
        let radius = parts.iter().flat_map(|p| &p.vertices).map(|v| length(v.pos)).fold(1e-3, f32::max);
        let see_through = SeeThrough::new(&parts, false);
        Mesh { serial: SERIAL.fetch_add(1, Ordering::Relaxed), parts, format: Format::Glb, extent: [w.max(1.0), h.max(1.0), 1.0], radius, rig: None, extrusion: None, lights: Vec::new(), lens: None, see_through }
    }

    /// Lies in one plane facing the viewer, as flat artwork does.
    pub fn flat(&self) -> bool {
        self.extent[2] < 1e-4
    }

    /// Flat artwork given `depth` (Ratty's 0.03 a unit) of thickness: its
    /// face in front and behind, and sides along its open edges. A glTF's
    /// animation still plays on it: the flat mesh is posed, then made thick
    /// again ([`Mesh::posed`]).
    pub fn extruded(self: &Arc<Self>, depth: f32) -> Mesh {
        let half = depth * 0.03 * 0.5;
        let mut outlines = Vec::new();
        let parts: Vec<Part> = self
            .parts
            .iter()
            .map(|p| {
                let n = p.vertices.len() as u32;
                let mut indices = p.indices.clone();
                indices.extend(p.indices.as_chunks::<3>().0.iter().flat_map(|t| [t[2] + n, t[1] + n, t[0] + n]));
                // An edge only one triangle has is on the outline.
                let mut edges: HashMap<(u32, u32), u32> = HashMap::new();
                for t in p.indices.as_chunks::<3>().0 {
                    for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                        *edges.entry((a.min(b), a.max(b))).or_insert(0) += 1;
                    }
                }
                let mut outline: Vec<(u32, u32)> = edges.into_iter().filter(|(_, n)| *n == 1).map(|(e, _)| e).collect();
                outline.sort_unstable();
                for k in 0..outline.len() as u32 {
                    let base = 2 * n + 4 * k;
                    indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
                }
                let vertices = thicken(&p.vertices, &outline, half);
                outlines.push(outline);
                Part { vertices, indices, material: p.material.clone(), node: p.node }
            })
            .collect();
        let extent = [self.extent[0], self.extent[1], half * 2.0];
        let radius = (self.radius * self.radius + half * half).sqrt();
        let extrusion = self.rig.is_some().then(|| Extrusion { flat: self.clone(), half, outlines });
        let see_through = SeeThrough::new(&parts, extrusion.is_some());
        Mesh { serial: SERIAL.fetch_add(1, Ordering::Relaxed), parts, format: self.format, extent, radius, rig: None, extrusion, lights: self.lights.clone(), lens: self.lens, see_through }
    }
}

/// A flat part's vertices made thick, as [`Mesh::extruded`]'s indices take
/// them: its face in front at `half`, behind at `-half`, then four for the
/// side along each edge of `outline`.
fn thicken(flat: &[Vertex], outline: &[(u32, u32)], half: f32) -> Vec<Vertex> {
    // The back sees the texture mirrored, so its bitangent turns.
    let back = |t: [f32; 4]| [t[0], t[1], t[2], -t[3]];
    let mut vertices: Vec<Vertex> = flat.iter().map(|v| Vertex { pos: [v.pos[0], v.pos[1], half], normal: [0.0, 0.0, 1.0], ..*v }).collect();
    vertices.extend(flat.iter().map(|v| Vertex { pos: [v.pos[0], v.pos[1], -half], normal: [0.0, 0.0, -1.0], tangent: back(v.tangent), ..*v }));
    for &(a, b) in outline {
        let (va, vb) = (flat[a as usize], flat[b as usize]);
        let side = normalize([vb.pos[1] - va.pos[1], va.pos[0] - vb.pos[0], 0.0]);
        for (v, z) in [(va, half), (vb, half), (vb, -half), (va, -half)] {
            vertices.push(Vertex { pos: [v.pos[0], v.pos[1], z], normal: side, tangent: [0.0; 4], ..v });
        }
    }
    vertices
}

/// Reads a model.
pub fn load(format: Format, bytes: &[u8]) -> Result<Mesh, String> {
    if bytes.len() > MAX_BYTES {
        return Err(format!("{} MB, more than {} MB", bytes.len() >> 20, MAX_BYTES >> 20));
    }
    match format {
        Format::Obj => load_obj(&String::from_utf8_lossy(bytes)),
        Format::Glb => glb::load(bytes),
    }
}

/// A Wavefront OBJ: `v`, `vt`, `vn` and `f` (polygons as fans, indices
/// from 1 or from the end). Materials are left out, as Ratty does: `color`
/// tints it, white otherwise.
fn load_obj(text: &str) -> Result<Mesh, String> {
    let (mut pos, mut uvs, mut normals) = (Vec::new(), Vec::new(), Vec::new());
    let mut corners: Vec<[Option<usize>; 3]> = Vec::new();
    let nums = |rest: &str| rest.split_whitespace().filter_map(|n| n.parse::<f32>().ok()).collect::<Vec<_>>();
    for line in text.lines() {
        let line = line.trim();
        let (kind, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        match kind {
            "v" => pos.push(nums(rest).get(0..3).map(|v| [v[0], v[1], v[2]]).ok_or("a vertex without three numbers")?),
            "vt" => uvs.push(nums(rest).first().map(|u| [*u, nums(rest).get(1).copied().unwrap_or(0.0)]).unwrap_or([0.0, 0.0])),
            "vn" => normals.push(nums(rest).get(0..3).map(|v| [v[0], v[1], v[2]]).unwrap_or([0.0, 0.0, 1.0])),
            "f" => {
                let face: Vec<[Option<usize>; 3]> = rest
                    .split_whitespace()
                    .map(|c| {
                        let mut it = c.split('/');
                        let mut idx = |len: usize| {
                            let n: i64 = it.next()?.parse().ok()?;
                            let i = if n < 0 { len as i64 + n } else { n - 1 };
                            (0..len as i64).contains(&i).then_some(i as usize)
                        };
                        [idx(pos.len()), idx(uvs.len()), idx(normals.len())]
                    })
                    .collect();
                if face.len() < 3 || face.iter().any(|c| c[0].is_none()) {
                    return Err("a face naming a vertex that isn't there".into());
                }
                for i in 1..face.len() - 1 {
                    corners.extend([face[0], face[i], face[i + 1]]);
                }
            }
            _ => {}
        }
    }
    // One vertex for each corner unlike the rest; normals worked out where
    // the file gives none.
    let mut seen: HashMap<[Option<usize>; 3], u32> = HashMap::new();
    let (mut vertices, mut indices) = (Vec::new(), Vec::new());
    let mut needs_normal = Vec::new();
    for c in &corners {
        let i = *seen.entry(*c).or_insert_with(|| {
            let uv = c[1].map(|i| [uvs[i][0], 1.0 - uvs[i][1]]).unwrap_or([0.0, 0.0]);
            vertices.push(Vertex::new(pos[c[0].unwrap_or(0)], c[2].map(|i| normals[i]).unwrap_or([0.0; 3]), uv));
            needs_normal.push(c[2].is_none());
            vertices.len() as u32 - 1
        });
        indices.push(i);
    }
    smooth_normals(&mut vertices, &indices, |i| needs_normal[i]);
    Mesh::new(vec![Part { vertices, indices, ..Part::plain([1.0; 4], 0.0) }], Format::Obj)
}

/// Fills in normals (where `wanted`) as the average of the faces around
/// each vertex, weighted by their area.
fn smooth_normals(vertices: &mut [Vertex], indices: &[u32], wanted: impl Fn(usize) -> bool) {
    let mut sum = vec![[0f32; 3]; vertices.len()];
    for t in indices.as_chunks::<3>().0 {
        let [a, b, c] = t.map(|i| vertices[i as usize].pos);
        let n = cross(sub(b, a), sub(c, a));
        for &i in t {
            sum[i as usize] = add(sum[i as usize], n);
        }
    }
    for (i, v) in vertices.iter_mut().enumerate() {
        if wanted(i) {
            v.normal = normalize(sum[i]);
        }
    }
}

/// A file's lights moved into the mesh's unit box (less `center`, over
/// `longest`), the strongest few on its middle kept, and their strengths
/// made shares of 1 there. A glTF's lights are in physical units (lux, and
/// candela a square meter away), which say nothing of how bright the
/// picture should be, so together they light the model's middle as Flow's
/// own light does, each as much of that as the file gives it. A light very
/// near the middle counts as half the model's `radius` away, or it alone
/// would light it. None left with any strength: Flow's own light.
fn settle_lights(lights: Vec<Light>, center: [f32; 3], longest: f32, radius: f32) -> Vec<Light> {
    let near = 0.5 * radius;
    let mut lights: Vec<(f32, Light)> = lights
        .into_iter()
        .map(|l| {
            let at = l.at.map(|p| sub(p, center).map(|v| v / longest));
            let on_middle = match at {
                Some(p) => l.strength / (dot(p, p).max(near * near) * longest * longest),
                None => l.strength,
            };
            (on_middle, Light { at, range: l.range.map(|r| r / longest), ..l })
        })
        .filter(|(e, _)| e.is_finite() && *e > 0.0)
        .collect();
    lights.sort_by(|a, b| b.0.total_cmp(&a.0));
    lights.truncate(MAX_LIGHTS);
    let total: f32 = lights.iter().map(|(e, _)| e).sum();
    lights.into_iter().map(|(e, l)| Light { strength: e / total, ..l }).collect()
}

/// The lights' shares settled over the poses of the clips that move them,
/// as the unit box is: a light a clip carries nearer the model's middle
/// than at rest gives it more (as the file's intensities say), which could
/// light it far more than Flow's own light does and wash it out. So the
/// most light they give the middle at any moment of any such clip is what
/// Flow's own gives (1), and at rest they give as much less. Sampled some
/// 30 times a second of each clip, from 24 to 600 times, posing only the
/// nodes.
fn settle_over_clips(rig: &Rig, lights: &mut [Light], radius: f32) {
    let near = 0.5 * radius;
    let on_middle = |now: &Light, rest: &Light| match (now.at, rest.at) {
        (Some(p), Some(r)) => rest.strength * dot(r, r).max(near * near) / dot(p, p).max(near * near),
        _ => rest.strength,
    };
    let mut most = 1f32;
    for c in (0..rig.clips.len()).filter(|&c| rig.moves_lights.get(c).copied().unwrap_or(false)) {
        let duration = rig.clips[c].duration;
        let samples = ((duration * 30.0).ceil() as usize).clamp(24, 600);
        for k in 0..=samples {
            let (world, _) = rig.pose(Some(c), duration * k as f32 / samples as f32);
            let total: f32 = rig.place_lights(&world, lights).iter().zip(lights.iter()).map(|(now, rest)| on_middle(now, rest)).sum();
            if total.is_finite() {
                most = most.max(total);
            }
        }
    }
    for l in lights {
        l.strength /= most;
    }
}

const IDENTITY: [[f32; 4]; 4] = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];

/// `a` × `b`, column-major as glTF's matrices are.
fn mul(a: [[f32; 4]; 4], b: [[f32; 4]; 4]) -> [[f32; 4]; 4] {
    let mut m = [[0f32; 4]; 4];
    for (c, col) in m.iter_mut().enumerate() {
        for (r, out) in col.iter_mut().enumerate() {
            *out = (0..4).map(|k| a[k][r] * b[c][k]).sum();
        }
    }
    m
}

/// A point (`w` 1) or direction (`w` 0) through `m`.
fn transform(m: [[f32; 4]; 4], p: [f32; 3], w: f32) -> [f32; 3] {
    [0, 1, 2].map(|r| m[0][r] * p[0] + m[1][r] * p[1] + m[2][r] * p[2] + m[3][r] * w)
}

fn det(m: [[f32; 4]; 4]) -> f32 {
    let c = |i: usize| [m[i][0], m[i][1], m[i][2]];
    dot(c(0), cross(c(1), c(2)))
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn length(a: [f32; 3]) -> f32 {
    dot(a, a).sqrt()
}

fn normalize(a: [f32; 3]) -> [f32; 3] {
    let l = length(a);
    if l > 1e-12 {
        a.map(|v| v / l)
    } else {
        [0.0, 0.0, 1.0]
    }
}

/// A registered object: its mesh, once it's in (sent whole, or read from a
/// file the user allowed). Placements share it, so one placed before its
/// mesh is in shows once it is.
#[derive(Debug, Default)]
pub struct Slot {
    mesh: Mutex<Option<Arc<Mesh>>>,
}

impl Slot {
    pub fn set(&self, mesh: Mesh) {
        *self.mesh.lock() = Some(Arc::new(mesh));
    }

    pub fn get(&self) -> Option<Arc<Mesh>> {
        self.mesh.lock().clone()
    }
}

/// What a placement shows and how: its object, and the style `u` changes.
#[derive(Debug)]
pub struct Shown {
    slot: Arc<Slot>,
    style: Mutex<Style>,
    /// Counts the changes, so the renderer knows to draw it again.
    version: AtomicU64,
    /// When it was placed: `animate`'s turning starts there.
    pub since: Instant,
    /// When its glTF animation started: placed, or named again by `u`.
    clip_since: Mutex<Instant>,
    /// A cell's height in pixels (an `f32`'s bits), for how far an animated
    /// model bobs.
    cell_height: AtomicU32,
    /// The mesh made thick for `depth`, and what it was made from.
    extruded: Mutex<Option<(u64, u32, Arc<Mesh>)>>,
    /// Flow placed it itself (the startup logo), so efficient mode holds
    /// it still; a program's model moves as the program asks.
    pub flows_own: bool,
    /// Animated, it turns about its own upright alone ([`Turn::spin`]),
    /// as a ball spins on its axis, rather than tumbling.
    spins: bool,
    /// Its own clock, which stops while it's held still.
    clock: Mutex<Clock>,
    /// Clicked, it turns back to how it stands from this turn, since then
    /// ([`Shown::align`]).
    back: Mutex<Option<(Instant, Turn)>>,
}

/// A model's own clock, which stops while Flow holds it still
/// ([`Shown::hold`]) and goes on from there after.
#[derive(Debug, Default)]
struct Clock {
    /// The time it was held, all told, before `held`.
    lost: Duration,
    /// Since when it's held, while it is.
    held: Option<Instant>,
}

impl Shown {
    pub fn new(slot: Arc<Slot>, style: Style, cell_height: f32) -> Self {
        let since = Instant::now();
        Self {
            slot,
            style: Mutex::new(style),
            version: AtomicU64::new(0),
            since,
            clip_since: Mutex::new(since),
            cell_height: AtomicU32::new(cell_height.to_bits()),
            extruded: Mutex::new(None),
            flows_own: false,
            spins: false,
            clock: Mutex::new(Clock::default()),
            back: Mutex::new(None),
        }
    }

    /// Marks it as placed by Flow itself, which holds it still in
    /// efficient mode.
    pub fn flows_own(mut self) -> Self {
        self.flows_own = true;
        self
    }

    /// Has it turn about its own upright alone when animated, leaning as
    /// its style's rotation has it.
    pub fn spinning(mut self) -> Self {
        self.spins = true;
        self
    }

    /// How it has turned `secs` into its turning, as it turns.
    fn turned(&self, secs: f32) -> Turn {
        if self.spins {
            Turn::spin(secs)
        } else {
            Turn::at(secs)
        }
    }

    /// `now` by its own clock: the time it was held still taken away, and
    /// while it's held, the moment it was. Its turning and its animation
    /// go by this.
    pub fn clock(&self, now: Instant) -> Instant {
        let clock = self.clock.lock();
        let at = clock.held.map_or(now, |held| held.min(now));
        at.checked_sub(clock.lost).unwrap_or(self.since).max(self.since)
    }

    /// How far into its turning it is `now`, in seconds, by its own clock.
    pub fn secs(&self, now: Instant) -> f32 {
        self.clock(now).saturating_duration_since(self.since).as_secs_f32()
    }

    /// Holds it still where it is (`still`), or lets it move on from there,
    /// `now`. True if that changed anything: its version moves then, so
    /// it's drawn once more, where its clock stopped or goes on from.
    pub fn hold(&self, still: bool, now: Instant) -> bool {
        let mut clock = self.clock.lock();
        match (clock.held, still) {
            (None, true) => clock.held = Some(now),
            (Some(held), false) => {
                clock.lost += now.saturating_duration_since(held);
                clock.held = None;
            }
            _ => return false,
        }
        drop(clock);
        self.version.fetch_add(1, Ordering::Relaxed);
        true
    }

    /// It's held still.
    pub fn held(&self) -> bool {
        self.clock.lock().held.is_some()
    }

    /// Clicked `now` (Flow's own turning logo), it turns back the shorter
    /// way to how it stands at rest, over `ALIGN_TURN`, rests there
    /// `ALIGN_REST`, and turns on from there as from its start. Clicked
    /// while it rests, it rests the longer. Held still (efficient mode),
    /// it stands so at once, and turns on from there once let go.
    pub fn align(&self, now: Instant) {
        if !self.style().animate {
            return;
        }
        let secs = self.secs(now);
        let mut back = self.back.lock();
        if back.is_some_and(|(at, _)| now < at + ALIGN_TURN) {
            // On its way already.
            return;
        }
        let mut clock = self.clock.lock();
        // Its clock reads its start until then, and goes on from it after.
        let still_until = match clock.held {
            Some(held) => held,
            // Resting from a click already.
            None if secs == 0.0 => now + ALIGN_REST,
            None => {
                *back = Some((now, self.turned(secs)));
                now + ALIGN_TURN + ALIGN_REST
            }
        };
        clock.lost = still_until.saturating_duration_since(self.since);
        drop(clock);
        drop(back);
        // Its glTF animation, if it has one, starts again with it.
        *self.clip_since.lock() = self.since;
        self.version.fetch_add(1, Ordering::Relaxed);
    }

    /// How far it has turned `now`: by its own clock, or on its way back
    /// to how it stands, clicked.
    pub fn turn(&self, now: Instant) -> Turn {
        let back = *self.back.lock();
        match back {
            Some((at, from)) if now < at + ALIGN_TURN => {
                let k = now.saturating_duration_since(at).as_secs_f32() / ALIGN_TURN.as_secs_f32();
                // Quick at first, slowing as it comes to stand.
                from.back(1.0 - (1.0 - k).powi(3))
            }
            _ => self.turned(self.secs(now)),
        }
    }

    /// While it stands still after a click ([`Shown::align`]), when it
    /// turns on again, so a frame comes then; none while it turns or is
    /// held still.
    pub fn rests_until(&self, now: Instant) -> Option<Instant> {
        let clock = self.clock.lock();
        let until = self.since + clock.lost;
        (clock.held.is_none() && until > now).then_some(until)
    }

    pub fn style(&self) -> Style {
        *self.style.lock()
    }

    pub fn update(&self, u: &Update) {
        self.style.lock().apply(u);
        if u.clip.is_some() {
            *self.clip_since.lock() = self.clock(Instant::now());
        }
        self.version.fetch_add(1, Ordering::Relaxed);
    }

    pub fn version(&self) -> u64 {
        self.version.load(Ordering::Relaxed)
    }

    /// The cells it's shown in changed size (the font did).
    pub fn set_cell_height(&self, height: f32) {
        self.cell_height.store(height.to_bits(), Ordering::Relaxed);
    }

    fn cell_height(&self) -> f32 {
        f32::from_bits(self.cell_height.load(Ordering::Relaxed))
    }

    /// It moves by itself, turning or playing its glTF's animation, so each
    /// frame draws it anew; not while it's held still, nor while it rests
    /// after a click, once it has turned back.
    pub fn animating(&self) -> bool {
        self.animating_at(Instant::now())
    }

    /// [`Shown::animating`] `now`.
    pub fn animating_at(&self, now: Instant) -> bool {
        let style = self.style();
        let moves = style.animate || self.mesh().is_some_and(|m| m.playing(style.clip).is_some());
        let turning_back = self.back.lock().is_some_and(|(at, _)| now < at + ALIGN_TURN + ALIGN_SETTLE);
        !self.held() && moves && (turning_back || self.rests_until(now).is_none())
    }

    /// Its mesh as its animation poses it `now` (by its own clock), if one
    /// plays.
    pub fn posed(&self, mesh: &Mesh, now: Instant) -> Option<Pose> {
        let secs = self.clock(now).saturating_duration_since(*self.clip_since.lock()).as_secs_f32();
        mesh.posed(self.style().clip, secs)
    }

    /// Where on its mesh a click lands, at `at` in a picture `width` ×
    /// `height` pixels (from its top left), `now`: the nearest point of a
    /// triangle under it, or None where the picture shows none there.
    /// Drawn straight on, the ray runs along the picture's z through the
    /// point; seen through a camera, from its eye through the point, from
    /// the nearest it draws on.
    pub fn hit(&self, (width, height): (f32, f32), at: (f32, f32), now: Instant) -> Option<Hit> {
        let mesh = self.mesh()?;
        let view = self.view(&mesh, (width, height), self.turn(now));
        let inverse = invert_affine(view.model)?;
        let (x, y) = (at.0 - width * 0.5, height * 0.5 - at.1);
        let (origin, dir, from) = match view.near {
            None => (transform(inverse, [x, y, 0.0], 1.0), normalize(transform(inverse, [0.0, 0.0, -1.0], 0.0)), f32::NEG_INFINITY),
            // The eye to the point on the plane the picture shows at its
            // own size, which is that far in front of it: so the nearest
            // drawn is `near` over that of the way.
            Some(near) => (transform(inverse, [0.0, 0.0, view.eye], 1.0), transform(inverse, [x, y, -view.eye], 0.0), near / view.eye),
        };
        let posed = self.posed(&mesh, now);
        let (k, point) = match &posed {
            Some(posed) => pick::cast(posed.vertices.iter().zip(&mesh.parts).map(|(v, p)| (&v[..], &p.indices[..])), origin, dir, from),
            None => pick::cast(mesh.parts.iter().map(|p| (&p.vertices[..], &p.indices[..])), origin, dir, from),
        }?;
        Some(Hit { point, node: mesh.parts[k].node })
    }

    /// The mesh to draw, if it's in: flat artwork made thick for `depth`.
    pub fn mesh(&self) -> Option<Arc<Mesh>> {
        let base = self.slot.get()?;
        let depth = self.style().depth;
        if depth <= 0.0 || !base.flat() {
            return Some(base);
        }
        let key = (depth * 100.0).round() as u32;
        let mut cache = self.extruded.lock();
        if let Some((from, k, mesh)) = cache.as_ref() {
            if *from == base.serial && *k == key {
                return Some(mesh.clone());
            }
        }
        let mesh = Arc::new(base.extruded(depth));
        *cache = Some((base.serial, key, mesh.clone()));
        Some(mesh)
    }

    /// Where the mesh goes in a picture `width` × `height` pixels, turned
    /// as `turn` says: column-major, in pixels from the picture's
    /// center, y up. Fitted so it stays inside the picture however it
    /// turns, unless it's at rest and unturned, when its box fits instead.
    /// Seen through its camera, it's as large as the camera sees it where
    /// the picture shows things at their own size (`Lens::scale`), the
    /// camera's eye in front of the picture's middle; turned, scaled and
    /// moved there as any model is, and drawn smaller where turning it
    /// would take it out of the picture ([`kept_in_view`]).
    pub fn transform(&self, mesh: &Mesh, (width, height): (f32, f32), turn: Turn) -> [[f32; 4]; 4] {
        let s = self.style();
        let turned = s.animate || s.depth > 0.0 || s.rotation != [0.0; 3];
        let lens = mesh.lens.map(|l| (l, l.scale(height, mesh.radius)));
        let fit = match lens {
            Some((_, (pixels, _))) => pixels,
            None if turned => 0.9 * width.min(height) / (2.0 * mesh.radius),
            None => 0.9 * (width / mesh.extent[0].max(1e-3)).min(height / mesh.extent[1].max(1e-3)),
        };
        let mut scale = s.scale3.map(|k| fit * s.scale.max(1e-3) * k.max(1e-3));
        let mut r = IDENTITY;
        if s.depth > 0.0 {
            r = mul(rot_y(OBLIQUE.0), rot_x(OBLIQUE.1));
        }
        let [rx, ry, rz] = s.rotation.map(f32::to_radians);
        r = mul(r, mul(rot_x(rx), mul(rot_y(ry), rot_z(rz))));
        let mut bob = 0.0;
        if s.animate {
            r = mul(r, turn.matrix());
            bob = turn.bob * self.cell_height() * BOB_CELLS;
        }
        let mut middle = s.offset;
        if let Some((l, (pixels, depth))) = lens {
            // Moved so the camera's eye is in front of the picture's middle,
            // as far as its field of view needs to span the picture there.
            let eye = [l.eye[0], l.eye[1], l.eye[2] - depth];
            for (k, e) in eye.iter().enumerate() {
                middle[k] -= e * pixels;
            }
            if turned {
                // Turning by itself, it may turn any way: its ball.
                let k = kept_in_view(mesh, (width, height), &l, pixels, scale, middle, (!s.animate).then_some(r));
                scale = scale.map(|v| v * k);
            }
        }
        let mut m = mul(r, [[scale[0], 0.0, 0.0, 0.0], [0.0, scale[1], 0.0, 0.0], [0.0, 0.0, scale[2], 0.0], [0.0, 0.0, 0.0, 1.0]]);
        m[3] = [middle[0], middle[1] + bob, middle[2], 1.0];
        m
    }

    /// How its picture is drawn, `width` × `height` pixels, turned as
    /// `turn` says: where the mesh goes ([`Shown::transform`]), and how
    /// that's laid on the picture. Straight on, it's flattened, deep
    /// enough for anything it can be scaled to; through its camera, in
    /// perspective from the camera's eye, nearer things larger, from just
    /// in front of the model (or the camera's nearest) to just behind it.
    pub fn view(&self, mesh: &Mesh, (width, height): (f32, f32), turn: Turn) -> View {
        let model = self.transform(mesh, (width, height), turn);
        let s = self.style();
        let Some(lens) = mesh.lens else {
            let reach = width.max(height) * 2.0 * s.scale.max(1.0) * s.scale3.iter().fold(1.0f32, |a, b| a.max(*b)) + s.offset[2].abs();
            let projection = [[2.0 / width, 0.0, 0.0, 0.0], [0.0, 2.0 / height, 0.0, 0.0], [0.0, 0.0, -0.5 / reach, 0.0], [0.0, 0.0, 0.5, 1.0]];
            return View { model, projection, eye: width.max(height) * 1.4, near: None };
        };
        let eye = lens.focal(height);
        let (pixels, _) = lens.scale(height, mesh.radius);
        // How far in front of the eye the model's middle is, and how far
        // it reaches from there as placed; a pixel more either way.
        let grow = (0..3).map(|c| length([model[c][0], model[c][1], model[c][2]])).fold(0.0, f32::max);
        let (middle, reach) = (eye - model[3][2], mesh.radius * grow + 1.0);
        let near = (middle - reach).max(lens.znear * pixels).max(eye * 1e-4);
        let far = (middle + reach).max(near * 2.0);
        // Depth 0 at the nearest and 1 at the farthest, over the distance.
        let (a, b) = (far / (far - near), -far * near / (far - near));
        let projection = [[2.0 * eye / width, 0.0, 0.0, 0.0], [0.0, 2.0 * eye / height, 0.0, 0.0], [0.0, 0.0, -a, -1.0], [0.0, 0.0, a * eye + b, eye]];
        View { model, projection, eye, near: Some(near) }
    }
}

/// How much smaller than its camera has it a model turned in front of the
/// camera is drawn, so that it stays in the picture (1, where it does as
/// it is): seen from the eye `lens.focal(height)` pixels in front of the
/// picture's middle, scaled by `scale` about its middle at `middle`
/// (pixels), taken in toward that middle until it reaches no further out
/// on any side than the picture's edge, or than it reached unturned where
/// the camera has it reach further, and no nearer the eye than the
/// camera's nearest. What it reaches is the ball round it, which it never
/// leaves however it turns, or, turned by `turned` alone, its box so
/// turned where that lets it be larger (the box is square to the view, so
/// turned it may reach past all of the model). A camera among the model,
/// its box reaching behind the eye, isn't kept to anything.
fn kept_in_view(mesh: &Mesh, (width, height): (f32, f32), lens: &Lens, pixels: f32, scale: [f32; 3], middle: [f32; 3], turned: Option<[[f32; 4]; 4]>) -> f32 {
    let focal = lens.focal(height);
    // How far in front of the eye its middle is, and the nearest it's seen.
    let ahead = focal - middle[2];
    let nearest = (lens.znear * pixels).max(ahead * 0.01);
    let corners: Vec<[f32; 3]> = (0..8).map(|c| [0, 1, 2].map(|i| if c >> i & 1 == 1 { 0.5 } else { -0.5 } * mesh.extent[i] * scale[i])).collect();
    // Each side in turn (right, left, up, down): what a point's or the
    // middle's place across the picture is along it.
    let sides = |v: [f32; 3]| [v[0], -v[0], v[1], -v[1]];
    // How far out it may reach on each side, on the picture.
    let mut room = [0.5 * width, 0.5 * width, 0.5 * height, 0.5 * height];
    for q in &corners {
        let away = ahead - q[2];
        if away <= nearest {
            return 1.0;
        }
        for (r, along) in room.iter_mut().zip(sides([middle[0] + q[0], middle[1] + q[1], 0.0])) {
            *r = r.max(along * focal / away);
        }
    }
    let at = sides(middle);
    // The ball's edge on each side is seen at its middle's angle and the
    // angle its radius spans from the eye, which can be no more than the
    // room's angle less the middle's.
    let reach = mesh.radius * scale.iter().fold(0.0f32, |a, &b| a.max(b));
    let mut ball = (ahead - nearest) / reach;
    for (room, at) in room.iter().zip(at) {
        let spare = room.atan2(focal) - at.atan2(ahead);
        let most = if spare >= std::f32::consts::FRAC_PI_2 { 1.0 } else { spare.sin().max(0.0) };
        ball = ball.min(at.hypot(ahead) * most / reach);
    }
    // Each corner of the turned box, taken in by k, is on the picture at
    // (middle + k q) × focal / (ahead - k q.z) along a side, which grows
    // or shrinks with k whichever it is: so it's in its room for every k
    // up to where it meets the room's edge.
    let boxed = turned.map_or(0.0, |r| {
        let mut k = f32::INFINITY;
        for q in corners.iter().map(|&q| transform(r, q, 0.0)) {
            for ((room, at), along) in room.iter().zip(at).zip(sides(q)) {
                let grows = focal * along + room * q[2];
                if grows > 0.0 {
                    k = k.min((room * ahead - focal * at) / grows);
                }
            }
            if q[2] > 0.0 {
                k = k.min((ahead - nearest) / q[2]);
            }
        }
        k
    });
    ball.max(boxed).clamp(1e-3, 1.0)
}

/// The inverse of `m`, which moves, turns and scales but doesn't project
/// (its last row is 0 0 0 1); None if it squashes space flat.
fn invert_affine(m: [[f32; 4]; 4]) -> Option<[[f32; 4]; 4]> {
    let (a, b, c) = ([m[0][0], m[0][1], m[0][2]], [m[1][0], m[1][1], m[1][2]], [m[2][0], m[2][1], m[2][2]]);
    let rows = [cross(b, c), cross(c, a), cross(a, b)];
    let det = dot(a, rows[0]);
    if det.abs() < 1e-20 {
        return None;
    }
    let rows = rows.map(|r| r.map(|v| v / det));
    let mut inverse = IDENTITY;
    for (col, out) in inverse.iter_mut().take(3).enumerate() {
        for (row, value) in out.iter_mut().take(3).enumerate() {
            *value = rows[row][col];
        }
    }
    let t = [m[3][0], m[3][1], m[3][2]];
    inverse[3] = [-dot(rows[0], t), -dot(rows[1], t), -dot(rows[2], t), 1.0];
    Some(inverse)
}

fn rot_x(a: f32) -> [[f32; 4]; 4] {
    let (s, c) = a.sin_cos();
    [[1.0, 0.0, 0.0, 0.0], [0.0, c, s, 0.0], [0.0, -s, c, 0.0], [0.0, 0.0, 0.0, 1.0]]
}

fn rot_y(a: f32) -> [[f32; 4]; 4] {
    let (s, c) = a.sin_cos();
    [[c, 0.0, -s, 0.0], [0.0, 1.0, 0.0, 0.0], [s, 0.0, c, 0.0], [0.0, 0.0, 0.0, 1.0]]
}

fn rot_z(a: f32) -> [[f32; 4]; 4] {
    let (s, c) = a.sin_cos();
    [[c, s, 0.0, 0.0], [-s, c, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]]
}

/// How far an animated model (`animate=1`) has turned: what its turning
/// adds to its style's rotation, as a unit quaternion (w, x, y, z), and
/// where its bob is, from -1 to 1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Turn {
    pub rotation: [f32; 4],
    pub bob: f32,
}

impl Turn {
    /// Unturned: the model as its style stands it.
    pub const REST: Turn = Turn { rotation: [1.0, 0.0, 0.0, 0.0], bob: 0.0 };

    /// `secs` into its turning: about the upright at `SPIN`, then across
    /// at 0.7 of it.
    pub fn at(secs: f32) -> Self {
        let (sy, cy) = (secs * SPIN * 0.5).sin_cos();
        let (sx, cx) = (secs * SPIN * 0.7 * 0.5).sin_cos();
        // The turn about y, then the one about x.
        Turn { rotation: [cy * cx, cy * sx, sy * cx, -sy * sx], bob: (secs * BOB_SPEED).sin() }
    }

    /// `secs` into spinning about the upright alone, at `SPIN`, bobbing
    /// as [`Turn::at`] does.
    pub fn spin(secs: f32) -> Self {
        let (s, c) = (secs * SPIN * 0.5).sin_cos();
        Turn { rotation: [c, 0.0, s, 0.0], bob: (secs * BOB_SPEED).sin() }
    }

    /// `k` of the way from this back to [`Turn::REST`] (0 none of it, 1
    /// all), the shorter way: about one axis, as far as it takes.
    fn back(self, k: f32) -> Self {
        let mut q = self.rotation;
        if q[0] < 0.0 {
            q = q.map(|v| -v);
        }
        let sin = (q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
        let left = sin.atan2(q[0]) * (1.0 - k);
        let rotation = match sin {
            s if s < 1e-6 => Turn::REST.rotation,
            s => [left.cos(), q[1] / s * left.sin(), q[2] / s * left.sin(), q[3] / s * left.sin()],
        };
        Turn { rotation, bob: self.bob * (1.0 - k) }
    }

    /// Its turn as a matrix, column-major.
    fn matrix(&self) -> [[f32; 4]; 4] {
        let [w, x, y, z] = self.rotation;
        [
            [1.0 - 2.0 * (y * y + z * z), 2.0 * (x * y + w * z), 2.0 * (x * z - w * y), 0.0],
            [2.0 * (x * y - w * z), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z + w * x), 0.0],
            [2.0 * (x * z + w * y), 2.0 * (y * z - w * x), 1.0 - 2.0 * (x * x + y * y), 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ]
    }
}

/// A model in a file a program named, waiting for the user's yes.
#[derive(Debug, Clone)]
pub struct Ask {
    pub id: u32,
    pub format: Format,
    pub path: PathBuf,
    pub slot: Arc<Slot>,
}

impl Ask {
    /// Reads and loads the file, into the object it's for. Only a regular
    /// file on this computer is read, as pictures' files are
    /// (`crate::graphics`), and no more of it than a model may be, whatever
    /// its size says: the user said yes to a file, and a device
    /// (`/dev/zero`) would be read until Flow ran out of memory, a pipe
    /// would wait for ever, and on Windows a share on another computer
    /// would have Windows sign in to it as the user.
    pub fn load(&self) -> Result<(), String> {
        use std::io::Read;
        let not_a_file = || crate::i18n::t("model_ask.not_a_file").to_string();
        if !crate::graphics::is_local_path(&self.path) {
            return Err(not_a_file());
        }
        let real = self.path.canonicalize().map_err(|e| e.to_string())?;
        let meta = std::fs::metadata(&real).map_err(|e| e.to_string())?;
        if crate::graphics::is_sensitive(&real) || !meta.is_file() {
            return Err(not_a_file());
        }
        if meta.len() > MAX_BYTES as u64 {
            return Err(format!("{} MB, more than {} MB", meta.len() >> 20, MAX_BYTES >> 20));
        }
        let file = std::fs::File::open(&real).map_err(|e| e.to_string())?;
        // The one opened, should the path have changed since.
        if !file.metadata().is_ok_and(|m| m.is_file()) {
            return Err(not_a_file());
        }
        // One byte past the most, which `load` refuses.
        let mut bytes = Vec::new();
        file.take(MAX_BYTES as u64 + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        self.slot.set(load(self.format, &bytes)?);
        Ok(())
    }
}

/// Where an object is shown: its image in the session's `ImageStore`, and
/// the cells it was written to (the top one's line counted from the top of
/// the scrollback).
#[derive(Debug, Clone)]
pub struct Showing {
    pub image: u32,
    pub top: i64,
    pub left: usize,
    pub cols: u32,
    pub rows: u32,
    /// Its picture's size, width times height.
    pub pixels: u64,
    pub shown: Arc<Shown>,
}

/// A tab's objects: registered, coming in chunks, shown, and named by files
/// that wait for the user.
#[derive(Debug, Default)]
pub struct ModelStore {
    slots: HashMap<u32, Arc<Slot>>,
    receiving: HashMap<u32, (Format, Vec<u8>)>,
    pub shown: HashMap<u32, Showing>,
    asks: Vec<Ask>,
}

impl ModelStore {
    /// The object `id`, made empty if it's new.
    pub fn slot(&mut self, id: u32) -> Option<Arc<Slot>> {
        if !self.slots.contains_key(&id) && self.slots.len() >= MAX_OBJECTS {
            return None;
        }
        Some(self.slots.entry(id).or_default().clone())
    }

    /// A chunk of `id`'s payload; the last one loads the model.
    pub fn receive(&mut self, id: u32, format: Format, more: bool, data: Vec<u8>) -> Result<(), String> {
        let (_, buf) = self.receiving.entry(id).or_insert((format, Vec::new()));
        if buf.len() + data.len() > MAX_BYTES {
            self.receiving.remove(&id);
            return Err(format!("object {id} is more than {} MB", MAX_BYTES >> 20));
        }
        buf.extend_from_slice(&data);
        if more {
            return Ok(());
        }
        let (format, bytes) = self.receiving.remove(&id).unwrap_or((format, Vec::new()));
        let mesh = load(format, &bytes).map_err(|e| format!("object {id}: {e}"))?;
        self.slot(id).ok_or("too many objects")?.set(mesh);
        Ok(())
    }

    /// A file `id` names, to read once the user says yes.
    pub fn ask(&mut self, id: u32, format: Format, path: PathBuf) {
        if let Some(slot) = self.slot(id) {
            self.asks.retain(|a| a.id != id);
            self.asks.push(Ask { id, format, path, slot });
        }
    }

    /// The files waiting for the user, oldest first.
    pub fn take_asks(&mut self) -> Vec<Ask> {
        std::mem::take(&mut self.asks)
    }

    /// Whether `ask`'s object is still the one the program named the file
    /// for: deleted, or the tab gone, there's nothing left to ask about.
    pub fn still_asks(&self, ask: &Ask) -> bool {
        self.slots.get(&ask.id).is_some_and(|slot| Arc::ptr_eq(slot, &ask.slot))
    }

    pub fn update(&mut self, id: u32, u: &Update) {
        if let Some(s) = self.shown.get(&id) {
            s.shown.update(u);
        }
    }

    /// Forgets `id` (registration and placement), or every object; returns
    /// where they were shown, to clear.
    pub fn delete(&mut self, id: Option<u32>) -> Vec<Showing> {
        match id {
            Some(id) => {
                self.slots.remove(&id);
                self.receiving.remove(&id);
                self.asks.retain(|a| a.id != id);
                self.shown.remove(&id).into_iter().collect()
            }
            None => {
                self.slots.clear();
                self.receiving.clear();
                self.asks.clear();
                self.shown.drain().map(|(_, s)| s).collect()
            }
        }
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    pub use glb::tests::{
        ball_glb, ball_v1_glb, basis_ball_glb, colored_triangle, crossed_glb, draco_ball, draco_glb, instanced_glb, layers_glb, material_glb, moving_lights_glb, orbiting_camera_glb, perspective_cube_glb, perspective_glb, plank_glb,
        sliding_crossed_glb, spec_gloss_glb, BASIS_CHECKERS, CORNER_COLORS, DRACO_IDS, GOOGLES_DRACO, GOOGLES_IDS,
    };

    fn b64(bytes: &[u8]) -> String {
        base64::engine::general_purpose::STANDARD.encode(bytes)
    }

    /// A square of two triangles in the z = 0 plane, as OBJ.
    pub const SQUARE: &str = "v 0 0 0\nv 2 0 0\nv 2 2 0\nv 0 2 0\nf 1 2 3 4\n";

    /// A one-triangle binary glTF, its node moving it by (10, 0, 0) and
    /// its material red.
    pub fn triangle_glb() -> Vec<u8> {
        let mut bin: Vec<u8> = [[0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]].iter().flatten().flat_map(|f| f.to_le_bytes()).collect();
        bin.extend([0u16, 1, 2].iter().flat_map(|i| i.to_le_bytes()));
        bin.resize(bin.len().div_ceil(4) * 4, 0);
        let json = format!(
            r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0,"translation":[10,0,0]}}],
            "meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}},"indices":1,"material":0}}]}}],
            "materials":[{{"pbrMetallicRoughness":{{"baseColorFactor":[1,0,0,1]}}}}],
            "buffers":[{{"byteLength":{}}}],
            "bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},{{"buffer":0,"byteOffset":36,"byteLength":6}}],
            "accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]}},
                         {{"bufferView":1,"componentType":5123,"count":3,"type":"SCALAR"}}]}}"#,
            bin.len()
        );
        let mut json = json.into_bytes();
        json.resize(json.len().div_ceil(4) * 4, b' ');
        let mut out = b"glTF".to_vec();
        out.extend(2u32.to_le_bytes());
        out.extend(((12 + 8 + json.len() + 8 + bin.len()) as u32).to_le_bytes());
        out.extend((json.len() as u32).to_le_bytes());
        out.extend(b"JSON");
        out.extend(&json);
        out.extend((bin.len() as u32).to_le_bytes());
        out.extend(b"BIN\0");
        out.extend(&bin);
        out
    }

    #[test]
    fn commands_read_as_ratty_reads_them() {
        assert_eq!(parse(b"s"), Some(Command::Support));
        assert_eq!(parse(b"r;id=42;fmt=obj;path=CairoSpinyMouse.obj"), Some(Command::Register { id: 42, format: Format::Obj, source: Source::Path("CairoSpinyMouse.obj".into()) }));
        let place = parse(b"p;id=42;row=12;col=8;w=4;h=2;animate=1;scale=1.0;depth=2.5;color=ff8844;brightness=1.5;px=1;py=2;pz=3;rx=0;ry=45;rz=0;sx=1;sy=2;sz=1");
        let style = Style { animate: true, scale: 1.0, depth: 2.5, color: Some([0xff, 0x88, 0x44]), brightness: 1.5, offset: [1.0, 2.0, 3.0], rotation: [0.0, 45.0, 0.0], scale3: [1.0, 2.0, 1.0], ..Style::default() };
        assert_eq!(place, Some(Command::Place { id: 42, row: 12, col: 8, cols: 4, rows: 2, style }));
        let bare = parse(b"p;id=7;row=5;col=10;w=3;h=2");
        assert_eq!(bare, Some(Command::Place { id: 7, row: 5, col: 10, cols: 3, rows: 2, style: Style::default() }), "the original v1 fields alone");
        assert_eq!(parse(b"p;id=7;row=5;col=10;w=3"), None, "no height");
        let Some(Command::Update { id: 7, update }) = parse(b"u;id=7;ry=180;px=0.25;animate=0;tint=#00ff00") else { panic!() };
        assert_eq!((update.rotation, update.offset[0], update.animate, update.color), ([None, Some(180.0), None], Some(0.25), Some(false), Some([0, 255, 0])));
        assert_eq!(parse(b"d;id=7"), Some(Command::Delete(Some(7))));
        assert_eq!(parse(b"d"), Some(Command::Delete(None)));
        assert_eq!(parse(b"x;id=1"), None);
        assert_eq!(parse(b"r;id=1;fmt=stl;path=a.stl"), None, "a format v1 doesn't have");
    }

    #[test]
    fn flows_own_keys_pick_an_animation_and_ask_for_clicks() {
        let Some(Command::Place { style, .. }) = parse(b"p;id=1;row=0;col=0;w=2;h=2;clip=2;clicks=1") else { panic!() };
        assert_eq!((style.clip, style.clicks), (2, true));
        let Some(Command::Place { style, .. }) = parse(b"p;id=1;row=0;col=0;w=2;h=2") else { panic!() };
        assert_eq!((style.clip, style.clicks), (0, false), "the first animation plays, and clicks go on as before");
        let Some(Command::Update { update, .. }) = parse(b"u;id=1;clip=-1;clicks=0") else { panic!() };
        assert_eq!((update.clip, update.clicks), (Some(-1), Some(false)));
        let Some(Command::Update { update, .. }) = parse(b"u;id=1;ry=5") else { panic!() };
        assert_eq!((update.clip, update.clicks), (None, None));
        assert!(SUPPORT.contains(";clip=1;clicks=1\x1b"), "and says so");
    }

    #[test]
    fn a_payload_is_the_first_field_that_isnt_a_key_even_ending_in_equals() {
        let data = b"v 0 0 0\n";
        let body = format!("r;id=3;fmt=obj;source=payload;more=1;name=rat.obj;{}", b64(data));
        assert!(b64(data).ends_with('='));
        assert_eq!(parse(body.as_bytes()), Some(Command::Register { id: 3, format: Format::Obj, source: Source::Payload { more: true, data: data.to_vec() } }));
        assert_eq!(parse(b"r;id=3;fmt=obj;source=payload;more=0;!!!"), None, "not base64");
    }

    #[test]
    fn an_update_changes_only_what_it_names() {
        let mut s = Style { color: Some([1, 2, 3]), rotation: [10.0, 20.0, 30.0], ..Style::default() };
        s.apply(&Update { rotation: [None, Some(90.0), None], scale: Some(2.0), ..Update::default() });
        assert_eq!((s.rotation, s.scale, s.color), ([10.0, 90.0, 30.0], 2.0, Some([1, 2, 3])));
    }

    #[test]
    fn an_obj_is_centered_in_a_unit_box_with_normals_worked_out() {
        let mesh = load(Format::Obj, SQUARE.as_bytes()).unwrap();
        let p = &mesh.parts[0];
        assert_eq!((p.vertices.len(), p.indices.len()), (4, 6), "a quad as a fan of two triangles");
        assert_eq!(p.vertices[0].pos, [-0.5, -0.5, 0.0]);
        assert_eq!(p.vertices[2].pos, [0.5, 0.5, 0.0]);
        assert_eq!(mesh.extent, [1.0, 1.0, 0.0]);
        assert!(mesh.flat());
        assert!(p.vertices.iter().all(|v| v.normal == [0.0, 0.0, 1.0]), "facing the viewer");
        assert!((mesh.radius - 0.5f32.hypot(0.5)).abs() < 1e-5);
        let negative = load(Format::Obj, b"v 0 0 0\nv 1 0 0\nv 0 1 0\nvn 0 0 1\nf -3//1 -2//1 -1//1\n").unwrap();
        assert_eq!(negative.parts[0].indices, [0, 1, 2], "indices counted from the end");
        assert!(load(Format::Obj, b"v 0 0 0\nf 1 2 3\n").is_err(), "a face naming a missing vertex");
        assert!(load(Format::Obj, b"# nothing\n").is_err());
    }

    #[test]
    fn a_glb_is_placed_by_its_node_and_colored_by_its_material() {
        let mesh = load(Format::Glb, &triangle_glb()).unwrap();
        assert_eq!(mesh.parts.len(), 1);
        let p = &mesh.parts[0];
        // Moved by the node, then centered: the same triangle.
        assert_eq!(p.vertices.iter().map(|v| v.pos).collect::<Vec<_>>(), [[-0.5, -0.5, 0.0], [0.5, -0.5, 0.0], [-0.5, 0.5, 0.0]]);
        assert_eq!(p.material.color, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(p.vertices[0].normal, [0.0, 0.0, 1.0], "worked out, as the file has none");
        assert!(load(Format::Glb, b"glTF but not really").is_err());
    }

    #[test]
    fn flat_artwork_given_depth_gets_a_back_and_sides() {
        let square = Arc::new(load(Format::Obj, SQUARE.as_bytes()).unwrap());
        let thick = square.extruded(10.0);
        let p = &thick.parts[0];
        // Front and back faces of 4 vertices and 2 triangles each, and a
        // side of 4 vertices and 2 triangles for each of the 4 outline edges.
        assert_eq!((p.vertices.len(), p.indices.len() / 3), (4 + 4 + 16, 2 + 2 + 8));
        assert!((thick.extent[2] - 0.3).abs() < 1e-6, "0.03 a unit of depth");
        assert!(!thick.flat());
        let shown = Shown::new(Arc::new(Slot::default()), Style { depth: 10.0, ..Style::default() }, 20.0);
        assert!(shown.mesh().is_none(), "nothing to draw before the mesh is in");
        shown.slot.set(load(Format::Obj, SQUARE.as_bytes()).unwrap());
        let first = shown.mesh().unwrap();
        assert!(!first.flat());
        assert!(Arc::ptr_eq(&first, &shown.mesh().unwrap()), "made once");
    }

    /// A flat triangle whose node slides a unit along x over a second.
    fn sliding_flat_glb() -> Vec<u8> {
        let mut g = glb::tests::Glb::default();
        let pos = g.floats(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0], 3);
        let idx = g.shorts(&[0, 1, 2], 1);
        let times = g.floats(&[0.0, 1.0], 1);
        let slide = g.floats(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0], 3);
        g.finish(&format!(
            r#""scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0}}],
            "meshes":[{{"primitives":[{{"attributes":{{"POSITION":{pos}}},"indices":{idx}}}]}}],
            "animations":[{{"samplers":[{{"input":{times},"output":{slide},"interpolation":"LINEAR"}}],"channels":[{{"sampler":0,"target":{{"node":0,"path":"translation"}}}}]}}]"#
        ))
    }

    #[test]
    fn flat_artwork_given_depth_turns_tumbles_and_bobs_when_animated() {
        let slot = Arc::new(Slot::default());
        slot.set(load(Format::Obj, SQUARE.as_bytes()).unwrap());
        let shown = Shown::new(slot, Style { depth: 4.0, animate: true, ..Style::default() }, 20.0);
        assert!(shown.animating(), "drawn again each frame");
        let mesh = shown.mesh().unwrap();
        assert!(!mesh.flat(), "made thick");
        let (at, later) = (shown.transform(&mesh, (100.0, 50.0), Turn::REST), shown.transform(&mesh, (100.0, 50.0), Turn::at(0.6)));
        assert!((0..3).any(|c| (0..3).any(|r| (at[c][r] - later[c][r]).abs() > 1e-3)), "turned and tumbled: {at:?} then {later:?}");
        assert_ne!(at[3][1], later[3][1], "and bobbed");
        for secs in [0.0, 0.6, 1.3, 2.9] {
            let m = shown.transform(&mesh, (100.0, 50.0), Turn::at(secs));
            for v in &mesh.parts[0].vertices {
                let p = transform(m, v.pos, 1.0);
                assert!(p[0].abs() <= 50.0 && p[1].abs() <= 25.0, "{secs}s: {p:?}");
            }
        }
    }

    #[test]
    fn a_turn_is_the_turning_about_the_upright_then_across() {
        for secs in [0.0, 0.3, 1.7, 4.0, 11.5] {
            let (got, want) = (Turn::at(secs).matrix(), mul(rot_y(secs * SPIN), rot_x(secs * SPIN * 0.7)));
            for (c, r) in (0..4).flat_map(|c| (0..4).map(move |r| (c, r))) {
                assert!((got[c][r] - want[c][r]).abs() < 1e-5, "{secs}s: {got:?} for {want:?}");
            }
        }
        assert_eq!(Turn::at(0.0).matrix(), IDENTITY);
    }

    #[test]
    fn a_spinning_model_turns_about_its_upright_alone_and_leans_as_its_style_says() {
        for secs in [0.0, 0.3, 1.7, 4.0, 11.5] {
            let (got, want) = (Turn::spin(secs).matrix(), rot_y(secs * SPIN));
            for (c, r) in (0..4).flat_map(|c| (0..4).map(move |r| (c, r))) {
                assert!((got[c][r] - want[c][r]).abs() < 1e-5, "{secs}s: {got:?} for {want:?}");
            }
            assert_eq!(Turn::spin(secs).bob, Turn::at(secs).bob);
        }
        // Leaning, its axis stays where the lean puts it as it spins.
        let style = Style { animate: true, rotation: [0.0, 0.0, -20.0], ..Style::default() };
        let mesh = load(Format::Obj, SQUARE.as_bytes()).unwrap();
        let shown = Shown::new(Arc::new(Slot::default()), style, 20.0).spinning();
        let axis = |secs: f32| {
            let m = shown.transform(&mesh, (100.0, 100.0), shown.turned(secs));
            let up = [m[1][0], m[1][1], m[1][2]];
            let len = length(up);
            up.map(|v| v / len)
        };
        for secs in [0.5, 2.0, 7.25] {
            let (a, b) = (axis(0.0), axis(secs));
            assert!((0..3).all(|i| (a[i] - b[i]).abs() < 1e-4), "{a:?} then {b:?}");
        }
        assert!(axis(0.0)[0] > 0.3, "leaning right: {:?}", axis(0.0));
    }

    #[test]
    fn a_turn_goes_back_to_rest_the_shorter_way_about_one_axis() {
        let angle = |t: Turn| 2.0 * t.rotation[1..].iter().map(|v| v * v).sum::<f32>().sqrt().atan2(t.rotation[0].abs());
        for secs in [0.4, 2.0, 3.1, 7.9] {
            let from = Turn::at(secs);
            assert!(angle(from) <= std::f32::consts::PI + 1e-5, "never more than half a turn back");
            assert!(from.back(0.0).rotation.iter().zip(from.rotation).all(|(a, b)| (a.abs() - b.abs()).abs() < 1e-5), "starts where it was");
            assert!((angle(from.back(0.5)) - angle(from) * 0.5).abs() < 1e-4, "half way, half the turn");
            assert!((from.back(0.5).bob - from.bob * 0.5).abs() < 1e-6);
            let rest = from.back(1.0);
            assert!((rest.rotation[0] - 1.0).abs() < 1e-6 && rest.bob == 0.0, "{rest:?}");
        }
    }

    #[test]
    fn flows_logo_clicked_turns_back_rests_and_turns_on_from_how_it_stands() {
        let slot = Arc::new(Slot::default());
        slot.set(load(Format::Obj, SQUARE.as_bytes()).unwrap());
        let shown = Shown::new(slot, Style { animate: true, rotation: [35.26, 45.0, 0.0], ..Style::default() }, 20.0).flows_own();
        let mesh = shown.mesh().unwrap();
        let at = |secs: f32| shown.since + Duration::from_secs_f32(secs);
        let size = (100.0, 100.0);
        let standing = shown.transform(&mesh, size, Turn::REST);
        let clicked = at(2.0);
        let version = shown.version();
        shown.align(clicked);
        assert!(shown.version() > version);
        let near = |a: Turn, b: Turn| (a.bob - b.bob).abs() < 1e-5 && a.rotation.iter().zip(b.rotation).all(|(x, y)| (x - y).abs() < 1e-5);
        assert!(near(shown.turn(clicked), Turn::at(2.0)), "it starts back from where it was");
        let partway = shown.turn(clicked + ALIGN_TURN / 2);
        assert!(!near(partway, Turn::at(2.0)) && !near(partway, Turn::REST) && shown.animating_at(clicked + ALIGN_TURN / 2));
        // Turned back, it stands as the logo does, and frames come a
        // moment longer so the last shows it so.
        let back = clicked + ALIGN_TURN;
        assert_eq!(shown.transform(&mesh, size, shown.turn(back)), standing);
        assert!(shown.animating_at(back) && !shown.animating_at(back + ALIGN_SETTLE));
        // It rests there, waking Flow when it's to turn on again.
        let until = back + ALIGN_REST;
        assert_eq!((shown.rests_until(back + ALIGN_SETTLE), shown.turn(until - Duration::from_millis(1))), (Some(until), Turn::REST));
        // A click on it now changes nothing on its way, and once it rests,
        // rests the longer.
        shown.align(clicked + ALIGN_TURN / 2);
        assert_eq!(shown.rests_until(back), Some(until), "on its way, a click does nothing");
        shown.align(back + Duration::from_secs(1));
        let until = back + Duration::from_secs(1) + ALIGN_REST;
        assert_eq!((shown.rests_until(back + Duration::from_secs(2)), shown.turn(until - Duration::from_millis(1))), (Some(until), Turn::REST));
        // Then it turns on as from its start.
        let later = until + Duration::from_millis(700);
        assert!(shown.rests_until(until).is_none() && shown.animating_at(until));
        assert!((shown.secs(later) - 0.7).abs() < 1e-3, "{}", shown.secs(later));
        assert_eq!(shown.turn(later), Turn::at(shown.secs(later)));
        // A model that doesn't turn has nothing to turn back from.
        let still = Shown::new(Arc::new(Slot::default()), Style::default(), 20.0).flows_own();
        still.align(at(1.0));
        assert!(still.rests_until(at(1.0)).is_none());
    }

    #[test]
    fn flows_logo_held_still_and_clicked_stands_so_at_once() {
        let slot = Arc::new(Slot::default());
        slot.set(load(Format::Obj, SQUARE.as_bytes()).unwrap());
        let shown = Shown::new(slot, Style { animate: true, ..Style::default() }, 20.0).flows_own();
        let at = |secs: f32| shown.since + Duration::from_secs_f32(secs);
        shown.hold(true, at(1.0));
        let version = shown.version();
        shown.align(at(3.0));
        assert_eq!((shown.turn(at(3.0)), shown.version() > version), (Turn::REST, true), "drawn once more, standing");
        assert!(!shown.animating_at(at(3.0)) && shown.rests_until(at(3.0)).is_none(), "no frames while held");
        // Let go, it turns on from there.
        shown.hold(false, at(10.0));
        assert!((shown.secs(at(11.0)) - 1.0).abs() < 1e-3 && shown.animating_at(at(11.0)));
    }

    #[test]
    fn a_model_held_still_stops_where_it_is_and_turns_on_from_there() {
        let slot = Arc::new(Slot::default());
        slot.set(load(Format::Obj, SQUARE.as_bytes()).unwrap());
        let shown = Shown::new(slot, Style { animate: true, ..Style::default() }, 20.0);
        assert!(!shown.flows_own, "a program's, unless Flow says it's its own");
        let mesh = shown.mesh().unwrap();
        let at = |secs: f32| shown.since + Duration::from_secs_f32(secs);
        let version = shown.version();
        assert!(shown.hold(true, at(1.0)) && !shown.hold(true, at(2.0)), "held once");
        assert!(shown.held() && !shown.animating(), "no frames kept coming");
        assert!(shown.version() > version, "but drawn once more, where it stopped");
        assert_eq!(shown.secs(at(9.0)), shown.secs(at(1.0)));
        let size = (100.0, 50.0);
        assert_eq!(shown.transform(&mesh, size, shown.turn(at(9.0))), shown.transform(&mesh, size, shown.turn(at(1.0))));
        // Let go, it turns on from where it stopped rather than jumping.
        assert!(shown.hold(false, at(9.0)) && shown.animating_at(at(9.0)));
        assert!((shown.secs(at(9.5)) - 1.5).abs() < 1e-3, "{}", shown.secs(at(9.5)));
        // Held and let go again, all the time it was held is taken away.
        shown.hold(true, at(10.0));
        assert!((shown.secs(at(20.0)) - 2.0).abs() < 1e-3);
        shown.hold(false, at(20.0));
        assert!((shown.secs(at(21.0)) - 3.0).abs() < 1e-3);
    }

    #[test]
    fn flat_artwork_given_depth_still_plays_its_gltfs_animation() {
        let slot = Arc::new(Slot::default());
        slot.set(load(Format::Glb, &sliding_flat_glb()).unwrap());
        assert!(slot.get().unwrap().flat(), "it slides in its own plane");
        let shown = Shown::new(slot, Style { depth: 4.0, ..Style::default() }, 20.0);
        let mesh = shown.mesh().unwrap();
        assert!(!mesh.flat() && mesh.moves());
        assert!(shown.animating(), "its animation plays, made thick or not");
        let half = 4.0 * 0.03 * 0.5;
        let start = shown.posed(&mesh, shown.since).expect("posed as it plays");
        let later = shown.posed(&mesh, shown.since + std::time::Duration::from_millis(500)).unwrap();
        assert_eq!(start.vertices[0].len(), mesh.parts[0].vertices.len(), "every vertex of the thick mesh");
        // The front's first corner has slid along x, and stays in front.
        assert!(later.vertices[0][0].pos[0] - start.vertices[0][0].pos[0] > 0.1, "{:?} then {:?}", start.vertices[0][0].pos, later.vertices[0][0].pos);
        assert!((later.vertices[0][0].pos[2] - half).abs() < 1e-6 && (later.vertices[0][3].pos[2] + half).abs() < 1e-6, "front and back");
        // At rest it's the thick mesh as made.
        shown.update(&Update { clip: Some(-1), ..Update::default() });
        assert!(!shown.animating() && shown.posed(&mesh, shown.since).is_none());
    }

    #[test]
    fn a_texture_transform_turns_counter_clockwise_as_the_texture_shows_it() {
        let v = Vertex { uv1: [0.0, 1.0], ..Vertex::new([0.0; 3], [0.0, 0.0, 1.0], [1.0, 0.0]) };
        let quarter = UvMap::new(0, [0.0; 2], std::f32::consts::FRAC_PI_2, [1.0; 2]);
        // u's way (right) turns to up the texture, which is v less.
        let [u, w] = quarter.apply(&v);
        assert!(u.abs() < 1e-6 && (w + 1.0).abs() < 1e-6, "{u} {w}");
        // Scaled before it's turned, and offset after; the second set read.
        let moved = UvMap::new(1, [0.25, 0.5], 0.0, [2.0, 3.0]);
        assert_eq!(moved.apply(&v), [0.25, 3.5]);
        assert_eq!(UvMap::PLAIN.apply(&v), [1.0, 0.0]);
        assert_eq!(UvMap::new(2, [0.0; 2], 0.0, [1.0; 2]).set, 0, "a third set isn't kept, so the first is read");
    }

    #[test]
    fn a_plain_part_short_of_full_alpha_is_see_through() {
        assert_eq!(Part::plain([1.0, 0.0, 0.0, 0.35], 0.0).material.alpha, Alpha::Blend);
        assert_eq!(Part::plain([1.0, 0.0, 0.0, 1.0], 0.0).material.alpha, Alpha::Opaque);
    }

    #[test]
    fn a_model_at_rest_fills_its_cells_and_a_turning_one_stays_inside() {
        let mesh = load(Format::Obj, SQUARE.as_bytes()).unwrap();
        let still = Shown::new(Arc::new(Slot::default()), Style::default(), 20.0);
        let m = still.transform(&mesh, (100.0, 50.0), Turn::REST);
        assert_eq!((m[0][0], m[1][1]), (45.0, 45.0), "the unit square fits the shorter side, less a margin");
        let turning = Shown::new(Arc::new(Slot::default()), Style { animate: true, ..Style::default() }, 20.0);
        for secs in [0.0, 0.7, 1.9, 4.2] {
            let m = turning.transform(&mesh, (100.0, 50.0), Turn::at(secs));
            for v in &mesh.parts[0].vertices {
                let p = transform(m, v.pos, 1.0);
                assert!(p[0].abs() <= 50.0 && p[1].abs() <= 25.0, "{secs}s: {p:?}");
            }
        }
    }

    #[test]
    fn a_glbs_animation_plays_until_asked_to_rest_and_starts_over_when_named() {
        let slot = Arc::new(Slot::default());
        slot.set(load(Format::Glb, &glb::tests::animated_glb()).unwrap());
        let shown = Shown::new(slot.clone(), Style::default(), 20.0);
        assert!(shown.animating(), "its first animation plays");
        let mesh = shown.mesh().unwrap();
        let later = shown.since + std::time::Duration::from_millis(750);
        let posed = shown.posed(&mesh, later).expect("posed as it plays");
        assert_ne!(posed.vertices[0][0].pos, mesh.parts[0].vertices[0].pos, "the morph target has lifted it");
        shown.update(&Update { clip: Some(-1), ..Update::default() });
        assert!(!shown.animating() && shown.posed(&mesh, later).is_none(), "at rest");
        shown.update(&Update { clip: Some(0), ..Update::default() });
        let now = Instant::now();
        assert_eq!(shown.posed(&mesh, now).unwrap().vertices[0][0].pos, mesh.posed(0, 0.0).unwrap().vertices[0][0].pos, "named again, from its start");
        // A still model, turning or not, plays nothing.
        let still = Shown::new(Arc::new(Slot::default()), Style::default(), 20.0);
        still.slot.set(load(Format::Obj, SQUARE.as_bytes()).unwrap());
        assert!(!still.animating());
    }

    #[test]
    fn a_click_finds_the_point_of_the_model_under_it_however_it_is_placed() {
        // The unit square, in a picture 100 × 50: fitted to 45 pixels a
        // side, centered at (50, 25) from the top left.
        let slot = Arc::new(Slot::default());
        slot.set(load(Format::Obj, SQUARE.as_bytes()).unwrap());
        let shown = Shown::new(slot, Style::default(), 20.0);
        let now = shown.since;
        let hit = shown.hit((100.0, 50.0), (50.0 + 9.0, 25.0 - 4.5), now).expect("on the square");
        assert!((hit.point[0] - 0.2).abs() < 1e-5 && (hit.point[1] - 0.1).abs() < 1e-5 && hit.point[2].abs() < 1e-5, "{hit:?}");
        assert_eq!(hit.node, None, "an OBJ has no nodes");
        assert_eq!(shown.hit((100.0, 50.0), (5.0, 25.0), now), None, "beside it");
        // Turned half round about y and moved 10 pixels right: fitted by its
        // radius now, and seen from behind, so x runs the other way.
        shown.update(&Update { rotation: [None, Some(180.0), None], offset: [Some(10.0), None, None], ..Update::default() });
        let fit = 0.9 * 50.0 / (2.0 * 0.5f32.hypot(0.5));
        let hit = shown.hit((100.0, 50.0), (60.0 + 0.2 * fit, 25.0 - 0.1 * fit), now).expect("on the square");
        assert!((hit.point[0] + 0.2).abs() < 1e-4 && (hit.point[1] - 0.1).abs() < 1e-4, "{hit:?}");
        // A glTF's node is named.
        let slot = Arc::new(Slot::default());
        slot.set(load(Format::Glb, &triangle_glb()).unwrap());
        let gltf = Shown::new(slot, Style::default(), 20.0);
        let hit = gltf.hit((100.0, 100.0), (40.0, 60.0), now).expect("on the triangle");
        assert_eq!(hit.node, Some(0));
    }

    #[test]
    fn a_click_finds_each_of_a_nodes_instances_and_names_the_node() {
        let slot = Arc::new(Slot::default());
        slot.set(load(Format::Glb, &glb::tests::instanced_glb(false)).unwrap());
        let shown = Shown::new(slot, Style::default(), 20.0);
        let (mesh, now) = (shown.mesh().unwrap(), shown.since);
        let view = shown.view(&mesh, (100.0, 100.0), Turn::REST);
        // The middle of each instance's triangle, as drawn: clicked there,
        // it's met, the node named.
        let p = &mesh.parts[0];
        for copy in 0..3 {
            let corners = [0, 1, 2].map(|k| p.vertices[p.indices[3 * copy + k] as usize].pos);
            let middle = [0, 1, 2].map(|c| corners.iter().map(|v| v[c]).sum::<f32>() / 3.0);
            let at = transform(view.model, middle, 1.0);
            let hit = shown.hit((100.0, 100.0), (50.0 + at[0], 50.0 - at[1]), now).unwrap_or_else(|| panic!("instance {copy}"));
            assert!((0..3).all(|c| (hit.point[c] - middle[c]).abs() < 1e-4) && hit.node == Some(0), "instance {copy}: {hit:?}");
        }
        // Between them, nothing.
        let gap = transform(view.model, [(9.5 - 9.0) / 6.0, (4.0 - 3.0) / 6.0, 0.0], 1.0);
        assert_eq!(shown.hit((100.0, 100.0), (50.0 + gap[0], 50.0 - gap[1]), now), None);
    }

    /// Where `view` puts `p` (in the mesh's box) on a picture `w` × `h`:
    /// pixels from its top left, and its depth, 0 nearest to 1 farthest.
    fn on_picture(view: &View, (w, h): (f32, f32), p: [f32; 3]) -> (f32, f32, f32) {
        let m = mul(view.projection, view.model);
        let c: [f32; 4] = std::array::from_fn(|r| m[0][r] * p[0] + m[1][r] * p[1] + m[2][r] * p[2] + m[3][r]);
        ((c[0] / c[3] + 1.0) * 0.5 * w, (1.0 - c[1] / c[3]) * 0.5 * h, c[2] / c[3])
    }

    #[test]
    fn a_perspective_camera_draws_the_model_as_it_sees_it() {
        let slot = Arc::new(Slot::default());
        slot.set(load(Format::Glb, &glb::tests::perspective_glb(false)).unwrap());
        let shown = Shown::new(slot, Style::default(), 20.0);
        let mesh = shown.mesh().unwrap();
        // A picture twice as wide as high: the camera's field of view spans
        // its height, and its own aspect ratio (1.5) isn't used.
        let size = (120.0, 60.0);
        let view = shown.view(&mesh, size, Turn::REST);
        let focal = 0.5 * size.1 / 0.4f32.tan();
        assert_eq!(view.eye, focal);
        // Each square's corners where a pinhole camera at (0, 0, 3) looking
        // down -z puts them, nearer ones larger, and none cut off.
        let middles = [[-0.55f32, -0.45, 0.0], [0.5, 0.5, -3.0], [3.0, 3.0, -9.0]];
        for part in &mesh.parts {
            let middle = middles[part.node.unwrap()];
            for (v, corner) in part.vertices.iter().zip([[-0.5, -0.5], [0.5, -0.5], [0.5, 0.5], [-0.5, 0.5]]) {
                let file = [middle[0] + corner[0], middle[1] + corner[1], middle[2]];
                let away = 3.0 - file[2];
                let (x, y, depth) = on_picture(&view, size, v.pos);
                let (px, py) = (0.5 * size.0 + focal * file[0] / away, 0.5 * size.1 - focal * file[1] / away);
                assert!((x - px).abs() < 1e-2 && (y - py).abs() < 1e-2, "{file:?}: ({x}, {y}), not ({px}, {py})");
                assert!(depth > 0.0 && depth < 1.0, "{file:?}: depth {depth}");
            }
        }
        // Its orthographic twin is drawn straight on, as every model was.
        let slot = Arc::new(Slot::default());
        slot.set(load(Format::Glb, &glb::tests::perspective_glb(true)).unwrap());
        let flat = Shown::new(slot, Style::default(), 20.0);
        let mesh = flat.mesh().unwrap();
        let view = flat.view(&mesh, size, Turn::REST);
        assert_eq!((view.near, view.eye, view.model), (None, 120.0 * 1.4, flat.transform(&mesh, size, Turn::REST)));
        assert_eq!(view.projection[3][3], 1.0, "no perspective");
    }

    #[test]
    fn a_turned_camera_puts_what_it_looks_at_in_the_middle_of_the_picture() {
        // `lights_glb`'s camera stands five units out along +x, looking
        // back at the triangle's corner at the origin: the corner is in the
        // picture's middle, and the others a unit right and up of it, five
        // units off.
        let slot = Arc::new(Slot::default());
        slot.set(load(Format::Glb, &glb::tests::lights_glb(true)).unwrap());
        let shown = Shown::new(slot, Style::default(), 20.0);
        let mesh = shown.mesh().unwrap();
        let size = (80.0, 80.0);
        let view = shown.view(&mesh, size, Turn::REST);
        let focal = 40.0 / 0.4f32.tan();
        let seen = [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0)];
        for (v, (right, up)) in mesh.parts[0].vertices.iter().zip(seen) {
            let (x, y, _) = on_picture(&view, size, v.pos);
            assert!((x - (40.0 + focal * right / 5.0)).abs() < 1e-2 && (y - (40.0 - focal * up / 5.0)).abs() < 1e-2, "({x}, {y})");
        }
    }

    #[test]
    fn a_click_through_a_perspective_camera_finds_what_it_shows_there() {
        let slot = Arc::new(Slot::default());
        slot.set(load(Format::Glb, &glb::tests::perspective_glb(false)).unwrap());
        let shown = Shown::new(slot, Style::default(), 20.0);
        let (size, now) = ((64.0, 64.0), shown.since);
        let focal = 32.0 / 0.4f32.tan();
        let seen = |p: [f32; 3]| (32.0 + focal * p[0] / (3.0 - p[2]), 32.0 - focal * p[1] / (3.0 - p[2]));
        // In the box: less (1.225, 1.275, -4.5), over 9.
        let boxed = |p: [f32; 3]| [(p[0] - 1.225) / 9.0, (p[1] - 1.275) / 9.0, (p[2] + 4.5) / 9.0];
        for (node, middle) in [[-0.55f32, -0.45, 0.0], [0.5, 0.5, -3.0], [3.0, 3.0, -9.0]].into_iter().enumerate() {
            let hit = shown.hit(size, seen(middle), now).unwrap_or_else(|| panic!("square {node}"));
            assert_eq!(hit.node, Some(node));
            assert!(hit.point.iter().zip(boxed(middle)).all(|(a, b)| (a - b).abs() < 1e-4), "{:?} not {:?}", hit.point, boxed(middle));
        }
        // Just off the far square, where a straight-on picture would have
        // had it: nothing.
        assert_eq!(shown.hit(size, (seen([3.0, 3.0, -9.0]).0 + 5.0, 32.0), now), None);
        // Brought nearer than the eye, the red square isn't drawn, nor met:
        // what's in front of the camera is.
        shown.update(&Update { offset: [None, None, Some(focal * 0.5)], ..Update::default() });
        let red = seen([-0.55, -0.45, 0.0]);
        assert_ne!(shown.hit(size, red, now).and_then(|h| h.node), Some(0));
    }

    #[test]
    fn a_model_turned_in_front_of_its_camera_is_drawn_smaller_only_as_far_as_it_must_to_stay_in_the_picture() {
        let size = (120.0, 60.0);
        let shown = |style: Style, file: &[u8]| {
            let slot = Arc::new(Slot::default());
            slot.set(load(Format::Glb, file).unwrap());
            Shown::new(slot, style, 20.0)
        };
        // Where each corner of the model's box lands on the picture, and
        // whether all are on it.
        let corners = |shown: &Shown, turn: Turn| -> Vec<(f32, f32)> {
            let mesh = shown.mesh().unwrap();
            let view = shown.view(&mesh, size, turn);
            (0..8).map(|c| on_picture(&view, size, [0, 1, 2].map(|i| if c >> i & 1 == 1 { 0.5 } else { -0.5 } * mesh.extent[i]))).map(|(x, y, _)| (x, y)).collect()
        };
        let inside = |points: &[(f32, f32)]| points.iter().all(|&(x, y)| (-0.01..=size.0 + 0.01).contains(&x) && (-0.01..=size.1 + 0.01).contains(&y));
        let edge = |points: &[(f32, f32)]| points.iter().map(|&(x, y)| x.min(y).min(size.0 - x).min(size.1 - y)).fold(f32::MAX, f32::min);
        let grown = |shown: &Shown, turn: Turn| {
            let m = shown.transform(&shown.mesh().unwrap(), size, turn);
            length([m[0][0], m[0][1], m[0][2]])
        };
        // A plank the camera frames across the picture's width.
        let plank = glb::tests::plank_glb();
        let rest = shown(Style::default(), &plank);
        let pixels = grown(&rest, Turn::REST);
        assert!(inside(&corners(&rest, Turn::REST)) && edge(&corners(&rest, Turn::REST)) < 8.0, "{:?}", corners(&rest, Turn::REST));
        // Turned a little about the view, it still fits: drawn as large.
        let nudged = shown(Style { rotation: [0.0, 0.0, 4.0], ..Style::default() }, &plank);
        let same = |a: f32, b: f32| (a - b).abs() < 1e-4 * b;
        assert!(same(grown(&nudged, Turn::REST), pixels));
        // Stood up, or turned far, its ends would leave the picture: it's
        // drawn smaller, and they stay on it, one touching its edge.
        for rotation in [[0.0, 0.0, 90.0], [30.0, 60.0, 0.0], [-30.0, 120.0, 50.0]] {
            let turned = shown(Style { rotation, ..Style::default() }, &plank);
            let points = corners(&turned, Turn::REST);
            assert!(grown(&turned, Turn::REST) < pixels && inside(&points), "{rotation:?}: {points:?}");
            assert!(edge(&points).abs() < 0.05, "{rotation:?}: {} pixels in from the edge", edge(&points));
        }
        // Turning by itself, it's kept as small as its ball needs, the
        // same size however it has turned.
        let animated = shown(Style { animate: true, ..Style::default() }, &plank);
        let spun = grown(&animated, Turn::REST);
        assert!(spun < pixels);
        for secs in [0.0, 0.7, 1.9, 3.3, 5.1] {
            let turn = Turn::at(secs);
            assert!(same(grown(&animated, turn), spun), "{secs}s");
            let points = corners(&animated, Turn { bob: 0.0, ..turn });
            assert!(inside(&points), "{secs}s: {points:?}");
        }
        // A cube its camera sees with room round it fits however it turns.
        let cube = glb::tests::perspective_cube_glb(false);
        let still = grown(&shown(Style::default(), &cube), Turn::REST);
        for style in [Style { rotation: [0.0, 45.0, 0.0], ..Style::default() }, Style { rotation: [60.0, 45.0, 0.0], ..Style::default() }, Style { animate: true, ..Style::default() }] {
            assert!(same(grown(&shown(style, &cube), Turn::at(1.3)), still), "{style:?}");
        }
        // A camera framing what reaches past the picture already (the far
        // squares off its top right): turned, it reaches no further than it
        // did, and stays on the picture where it was on it.
        let squares = glb::tests::perspective_glb(false);
        let reach = |points: &[(f32, f32)]| points.iter().fold((f32::MAX, f32::MIN), |(lo, hi), &(x, y)| (lo.min(x.min(size.1 - y)), hi.max(x.max(size.1 - y))));
        let (_, far) = reach(&corners(&shown(Style::default(), &squares), Turn::REST));
        assert!(far > size.0, "{far}: the camera has it past the picture");
        let (near, far_turned) = reach(&corners(&shown(Style { rotation: [0.0, 0.0, 30.0], ..Style::default() }, &squares), Turn::REST));
        assert!(far_turned <= far + 0.05 && near >= -0.05, "{near}, {far_turned} of {far}");
    }

    #[test]
    fn a_camera_coming_nearer_draws_the_model_larger() {
        // `dolly_glb`'s camera goes from eight units off the square two
        // units wide to four: through a field of view of 0.6, a picture 64
        // high shows it 2 × focal / 8 wide, then twice that.
        let slot = Arc::new(Slot::default());
        slot.set(load(Format::Glb, &glb::tests::dolly_glb()).unwrap());
        let shown = Shown::new(slot, Style::default(), 20.0);
        let mesh = shown.mesh().unwrap();
        let focal = 32.0 / 0.3f32.tan();
        for (secs, away) in [(0.0, 8.0), (0.5, 6.0), (1.0, 4.0)] {
            let posed = shown.posed(&mesh, shown.since + std::time::Duration::from_secs_f32(secs)).unwrap();
            let view = shown.view(&mesh, (64.0, 64.0), Turn::at(secs));
            let xs: Vec<f32> = posed.vertices[0].iter().map(|v| on_picture(&view, (64.0, 64.0), v.pos).0).collect();
            let wide = xs.iter().copied().fold(f32::MIN, f32::max) - xs.iter().copied().fold(f32::MAX, f32::min);
            assert!((wide - 2.0 * focal / away).abs() < 1e-2, "{secs}s: {wide} pixels, not {}", 2.0 * focal / away);
            // And always in the middle, as the camera looks at it.
            assert!((xs.iter().sum::<f32>() / 4.0 - 32.0).abs() < 1e-3);
        }
    }


    #[test]
    fn chunks_come_together_and_the_last_loads_the_model() {
        let mut store = ModelStore::default();
        let bytes = triangle_glb();
        let (a, b) = bytes.split_at(bytes.len() / 2);
        store.receive(9, Format::Glb, true, a.to_vec()).unwrap();
        assert!(store.slot(9).unwrap().get().is_none());
        store.receive(9, Format::Glb, false, b.to_vec()).unwrap();
        assert_eq!(store.slot(9).unwrap().get().unwrap().parts.len(), 1);
        assert!(store.receive(10, Format::Obj, false, b"nonsense".to_vec()).is_err());
        store.ask(11, Format::Obj, PathBuf::from("/nowhere/x.obj"));
        let asks = store.take_asks();
        assert_eq!((asks.len(), asks[0].id), (1, 11));
        assert!(asks[0].load().is_err(), "a file that isn't there");
        assert!(store.take_asks().is_empty());
        assert!(store.still_asks(&asks[0]), "taken, it's asked until the object goes");
        store.delete(Some(11));
        assert!(!store.still_asks(&asks[0]), "deleted, there's nothing left to ask about");
        store.delete(None);
        assert!(store.slot(9).unwrap().get().is_none(), "deleted: registered anew, empty");
    }

    #[test]
    fn a_model_is_read_only_from_a_regular_file_on_this_computer() {
        let dir = std::env::temp_dir().join(format!("flow-model-file-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("square.obj");
        std::fs::write(&file, SQUARE).unwrap();
        let slot = Arc::new(Slot::default());
        let ask = |path: PathBuf| Ask { id: 1, format: Format::Obj, path, slot: slot.clone() }.load();
        let refused = Err(crate::i18n::t("model_ask.not_a_file").to_string());
        // A folder; a path from wherever Flow was started; on Windows a
        // share on another computer; and a device whose size, 0, says
        // nothing of how much it would give.
        let mut not_files = vec![dir.clone(), PathBuf::from("square.obj"), PathBuf::from(r"\\server\share\square.obj")];
        if cfg!(unix) {
            not_files.push(PathBuf::from("/dev/zero"));
        }
        for path in not_files {
            assert_eq!(ask(path.clone()), refused, "{}", path.display());
        }
        assert!(slot.get().is_none());
        assert_eq!(ask(file), Ok(()));
        assert_eq!(slot.get().unwrap().parts.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
