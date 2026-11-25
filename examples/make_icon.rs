//! Draws the app icon: a dark glass terminal tile with a `>_` prompt and a
//! swirling liquid in its lower half.
//!
//! cargo run --release --example make_icon
//!
//! Writes assets/icon/flow-<size>.png, flow.ico (Windows) and flow.icns (macOS).

use std::f32::consts::TAU;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

const SIZES: [u32; 11] = [16, 20, 24, 32, 40, 48, 64, 128, 256, 512, 1024];
const ICO_SIZES: [u32; 8] = [16, 20, 24, 32, 40, 48, 64, 256];

type Rgb = [f32; 3];

const CYAN: Rgb = [0.49, 0.91, 1.0];
const VIOLET: Rgb = [0.72, 0.55, 1.0];
const TEAL: Rgb = [0.25, 0.95, 0.80];
const WHITE: Rgb = [0.95, 0.98, 1.0];

fn main() -> std::io::Result<()> {
    let out = Path::new("assets/icon");
    std::fs::create_dir_all(out)?;
    let mut pngs = Vec::new();
    for size in SIZES {
        let rgba = render(size);
        let png = encode_png(size, &rgba);
        std::fs::write(out.join(format!("flow-{size}.png")), &png)?;
        pngs.push((size, rgba, png));
    }
    write_ico(&out.join("flow.ico"), &pngs)?;
    write_icns(&out.join("flow.icns"), &pngs)?;
    write_preview(Path::new("target/icon-preview.png"), &pngs)?;
    println!("icons written to {}", out.display());
    Ok(())
}

/// Small sizes blown up 4x (nearest neighbour) on light and dark strips, for review.
fn write_preview(path: &Path, images: &[(u32, Vec<u8>, Vec<u8>)]) -> std::io::Result<()> {
    let picks = [16u32, 20, 24, 32, 40, 48, 64];
    let zoom = 4;
    let gap = 16;
    let width: u32 = picks.iter().map(|s| s * zoom + gap).sum::<u32>() + gap;
    let row_h = 64 * zoom + gap * 2;
    let height = row_h * 2;
    let mut px = vec![0u8; (width * height * 4) as usize];
    for y in 0..height {
        let bg: [u8; 3] = if y < row_h { [0x20, 0x20, 0x24] } else { [0xEE, 0xEE, 0xF0] };
        for x in 0..width {
            let i = ((y * width + x) * 4) as usize;
            px[i..i + 4].copy_from_slice(&[bg[0], bg[1], bg[2], 255]);
        }
    }
    for row in 0..2 {
        let mut ox = gap;
        for &size in &picks {
            let rgba = &images.iter().find(|(s, ..)| *s == size).expect("rendered size").1;
            let oy = row * row_h + gap;
            for y in 0..size * zoom {
                for x in 0..size * zoom {
                    let si = (((y / zoom) * size + x / zoom) * 4) as usize;
                    let a = rgba[si + 3] as u32;
                    let di = (((oy + y) * width + ox + x) * 4) as usize;
                    for k in 0..3 {
                        px[di + k] = ((rgba[si + k] as u32 * a + px[di + k] as u32 * (255 - a)) / 255) as u8;
                    }
                }
            }
            ox += size * zoom + gap;
        }
    }
    std::fs::write(path, encode_png_rect(width, height, &px))
}

fn encode_png_rect(w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut bytes, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().expect("png header");
        writer.write_image_data(rgba).expect("png data");
    }
    bytes
}

// ---- drawing ---------------------------------------------------------------

/// Straight-alpha RGBA8 for one icon size.
fn render(size: u32) -> Vec<u8> {
    let ss = match size {
        0..=64 => 5,
        65..=256 => 3,
        _ => 2,
    };
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    let n = (ss * ss) as f32;
    for y in 0..size {
        for x in 0..size {
            let mut acc = [0.0f32; 4];
            for sy in 0..ss {
                for sx in 0..ss {
                    let u = (x as f32 + (sx as f32 + 0.5) / ss as f32) / size as f32;
                    let v = (y as f32 + (sy as f32 + 0.5) / ss as f32) / size as f32;
                    let c = shade(u, v, size, ss);
                    for k in 0..4 {
                        acc[k] += c[k];
                    }
                }
            }
            let a = acc[3] / n;
            let un = |c: f32| if a > 0.0 { (c / n / a).clamp(0.0, 1.0) } else { 0.0 };
            out.extend_from_slice(&[to8(un(acc[0])), to8(un(acc[1])), to8(un(acc[2])), to8(a)]);
        }
    }
    out
}

fn to8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

/// Size-dependent layout: small icons get a bigger prompt and less detail.
struct Look {
    margin: f32,
    radius: f32,
    detail: f32,
    stroke: f32,
    chevron: [(f32, f32); 3],
    underscore: [(f32, f32); 2],
    surface: f32,
    dots: bool,
}

fn look(size: u32) -> Look {
    let px = 1.0 / size as f32;
    if size < 40 {
        Look {
            margin: 0.035,
            radius: 0.21,
            detail: if size >= 24 { 0.35 } else { 0.0 },
            stroke: (0.06f32).max(0.95 * px),
            chevron: [(0.19, 0.20), (0.40, 0.355), (0.19, 0.51)],
            underscore: [(0.48, 0.51), (0.76, 0.51)],
            surface: 0.67,
            dots: false,
        }
    } else {
        Look {
            margin: 0.07,
            radius: 0.19,
            detail: if size >= 96 { 1.0 } else { 0.6 },
            stroke: (0.034f32).max(1.1 * px),
            chevron: [(0.205, 0.30), (0.335, 0.395), (0.205, 0.49)],
            underscore: [(0.395, 0.49), (0.575, 0.49)],
            surface: 0.60,
            dots: size >= 48,
        }
    }
}

/// Premultiplied RGBA at icon coordinate (u, v), y down.
fn shade(u: f32, v: f32, size: u32, ss: u32) -> [f32; 4] {
    let lk = look(size);
    let px = 1.0 / size as f32;
    let aa = px / ss as f32;
    let half = 0.5 - lk.margin;
    let d = sd_round_box(u - 0.5, v - 0.5, half, half, lk.radius);
    let cover = clamp01(0.5 - d / aa);
    if cover <= 0.0 {
        return [0.0; 4];
    }
    // Position inside the tile, 0..1 each way.
    let tu = (u - lk.margin) / (1.0 - 2.0 * lk.margin);
    let tv = (v - lk.margin) / (1.0 - 2.0 * lk.margin);

    // Glass body.
    let mut col = mix(hex(0x1C2A4E), hex(0x060A15), smooth(0.0, 1.0, tv));
    col = add(col, scale(hex(0x2A3D6E), 0.25 * clamp01(1.0 - ((tu - 0.15).powi(2) + (tv - 0.05).powi(2)).sqrt() * 1.6)));

    // The liquid, under a wavy lit surface.
    let surface = lk.surface + 0.032 * (TAU * (tu * 0.85) + 0.7).sin() + 0.012 * (TAU * tu * 2.2 + 2.1).sin();
    let flow = flow_color(tu, tv, lk.detail);
    let below = smooth(-0.5 * px, 0.5 * px, tv - surface);
    let depth = clamp01((tv - surface) / (1.0 - surface));
    let body = mix(hex(0x0E3A5C), hex(0x0B1236), depth);
    let liquid = add(body, scale(flow, 0.95 - 0.35 * depth));
    // A faint mist of the same flow above the surface.
    let mist = add(col, scale(flow, 0.08 * lk.detail));
    col = mix(mist, liquid, below);
    let line_w = (0.010f32).max(0.75 * px);
    let meniscus = (-((tv - surface) / line_w).powi(2)).exp();
    col = add(col, scale(mix(CYAN, WHITE, 0.5), 0.75 * meniscus));
    col = add(col, scale(CYAN, 0.18 * (-((tv - surface) / (line_w * 5.0)).powi(2)).exp()));

    // Bubbles drifting up through the liquid.
    if size >= 64 {
        for &(bx, by, br) in &[(0.72, 0.80, 0.022), (0.80, 0.70, 0.014), (0.64, 0.90, 0.010)] {
            let dist = ((tu - bx).powi(2) + (tv - by).powi(2)).sqrt();
            let ring = clamp01(1.0 - (dist - br).abs() / (0.006f32).max(0.8 * px));
            let fill = clamp01((br - dist) / (0.006f32).max(px));
            col = add(col, scale(WHITE, 0.55 * ring + 0.10 * fill));
        }
    }

    // Window dots.
    if lk.dots {
        for (i, c) in [hex(0xFF6B81), hex(0xFFD07A), hex(0x7EE09F)].into_iter().enumerate() {
            let cx = 0.135 + i as f32 * 0.072;
            let dist = ((tu - cx).powi(2) + (tv - 0.125).powi(2)).sqrt();
            let a = clamp01((0.024 - dist) / aa) * 0.85;
            col = mix(col, c, a);
        }
    }

    // The prompt, with a soft glow behind it.
    let [a, b, c] = lk.chevron;
    let [e, f] = lk.underscore;
    let dist = sd_segment(tu, tv, a, b)
        .min(sd_segment(tu, tv, b, c))
        .min(sd_segment(tu, tv, e, f));
    if size >= 32 {
        col = add(col, scale(CYAN, 0.30 * (-(dist.max(0.0) / 0.05).powi(2)).exp()));
    }
    let ink = clamp01(0.5 - (dist - lk.stroke) / (aa * 1.2));
    col = mix(col, WHITE, ink);

    // Glass sheen across the top.
    let sheen = clamp01(1.0 - tv / 0.42) * 0.07;
    col = add(col, [sheen, sheen, sheen * 1.1]);

    // Hairline rim: bright inside, darker outside, so the tile reads on light and dark taskbars.
    let rim_w = (0.008f32).max(1.0 * px);
    let rim = clamp01(1.0 - (d + rim_w * 0.9).abs() / rim_w);
    let angle = (v - 0.5).atan2(u - 0.5);
    let rim_col = mix(CYAN, VIOLET, 0.5 + 0.5 * (angle * 1.0 + 0.8).sin());
    col = mix(col, add(rim_col, [0.15, 0.15, 0.15]), rim * 0.85);
    let edge = clamp01(1.0 - (d + rim_w * 0.25).abs() / (rim_w * 0.6));
    col = mix(col, hex(0x04060C), edge * 0.5);

    let a = cover;
    [col[0].clamp(0.0, 1.0) * a, col[1].clamp(0.0, 1.0) * a, col[2].clamp(0.0, 1.0) * a, a]
}

/// Aurora swirl; `detail` 0 gives a clean gradient for tiny sizes.
fn flow_color(u: f32, v: f32, detail: f32) -> Rgb {
    let simple = mix(mix(CYAN, VIOLET, smooth(0.1, 0.9, u)), TEAL, 0.25 * (1.0 - v));
    let simple = scale(simple, 0.55);
    if detail <= 0.0 {
        return simple;
    }
    // Twist the space around a vortex so the liquid visibly swirls.
    let (cx, cy) = (0.60, 0.84);
    let (dx, dy) = (u - cx, v - cy);
    let twist = 4.2 * (-(dx * dx + dy * dy) / 0.05).exp();
    let (s, c) = twist.sin_cos();
    let (ru, rv) = (cx + dx * c - dy * s, cy + dx * s + dy * c);
    // Stretched along x so the noise reads as currents rather than smoke.
    let (qx, qy) = (ru * 1.8 + 3.1, rv * 3.4 + 1.7);
    let wx = fbm(qx, qy);
    let wy = fbm(qx + 5.2, qy + 1.3);
    let w = fbm(qx + 2.2 * wx - 0.6, qy + 2.2 * wy + 2.4);
    let hue = mix(mix(CYAN, VIOLET, smooth(0.35, 0.7, wx)), TEAL, smooth(0.5, 0.8, wy) * 0.5);
    let density = smooth(0.25, 0.75, w);
    // Thin bright streaks that follow the flow.
    let streak = (0.5 + 0.5 * (w * 34.0).sin()).powi(6);
    let swirl = add(scale(hue, 0.22 + 0.55 * density), scale(mix(hue, WHITE, 0.5), 0.35 * streak * density));
    mix(simple, swirl, detail)
}

// ---- math ------------------------------------------------------------------

fn clamp01(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}

fn smooth(e0: f32, e1: f32, x: f32) -> f32 {
    let t = clamp01((x - e0) / (e1 - e0));
    t * t * (3.0 - 2.0 * t)
}

fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

fn add(a: Rgb, b: Rgb) -> Rgb {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scale(a: Rgb, s: f32) -> Rgb {
    [a[0] * s, a[1] * s, a[2] * s]
}

fn hex(c: u32) -> Rgb {
    [((c >> 16) & 255) as f32 / 255.0, ((c >> 8) & 255) as f32 / 255.0, (c & 255) as f32 / 255.0]
}

fn sd_round_box(px: f32, py: f32, bx: f32, by: f32, r: f32) -> f32 {
    let qx = px.abs() - bx + r;
    let qy = py.abs() - by + r;
    (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - r
}

fn sd_segment(px: f32, py: f32, a: (f32, f32), b: (f32, f32)) -> f32 {
    let (pax, pay) = (px - a.0, py - a.1);
    let (bax, bay) = (b.0 - a.0, b.1 - a.1);
    let h = clamp01((pax * bax + pay * bay) / (bax * bax + bay * bay));
    ((pax - bax * h).powi(2) + (pay - bay * h).powi(2)).sqrt()
}

fn hash(ix: i32, iy: i32) -> f32 {
    let mut h = (ix as u32).wrapping_mul(0x8da6_b343) ^ (iy as u32).wrapping_mul(0xd816_3841);
    h ^= h >> 13;
    h = h.wrapping_mul(0x85eb_ca6b);
    h ^= h >> 16;
    h as f32 / u32::MAX as f32
}

fn vnoise(x: f32, y: f32) -> f32 {
    let (ix, iy) = (x.floor(), y.floor());
    let (fx, fy) = (x - ix, y - iy);
    let (ux, uy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let (ix, iy) = (ix as i32, iy as i32);
    let a = hash(ix, iy);
    let b = hash(ix + 1, iy);
    let c = hash(ix, iy + 1);
    let d = hash(ix + 1, iy + 1);
    a + (b - a) * ux + (c - a) * uy + (a - b - c + d) * ux * uy
}

fn fbm(x: f32, y: f32) -> f32 {
    let (mut x, mut y) = (x, y);
    let mut v = 0.0;
    let mut amp = 0.5;
    for _ in 0..5 {
        v += amp * vnoise(x, y);
        // Rotate each octave so the grid doesn't show.
        let (nx, ny) = (x * 1.6 + y * 1.2, -x * 1.2 + y * 1.6);
        x = nx + 1.7;
        y = ny + 9.2;
        amp *= 0.5;
    }
    v
}

// ---- file formats ----------------------------------------------------------

fn encode_png(size: u32, rgba: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut bytes, size, size);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().expect("png header");
        w.write_image_data(rgba).expect("png data");
    }
    bytes
}

/// ICO with classic 32-bit DIB entries for small sizes and PNG for 256.
fn write_ico(path: &Path, images: &[(u32, Vec<u8>, Vec<u8>)]) -> std::io::Result<()> {
    let mut blobs: Vec<(u32, Vec<u8>)> = Vec::new();
    for &size in &ICO_SIZES {
        let (_, rgba, png) = images.iter().find(|(s, ..)| *s == size).expect("rendered size");
        blobs.push((size, if size >= 256 { png.clone() } else { dib(size, rgba) }));
    }
    let mut out = Vec::new();
    out.extend_from_slice(&[0, 0, 1, 0]);
    out.extend_from_slice(&(blobs.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * blobs.len() as u32;
    for (size, data) in &blobs {
        let dim = if *size >= 256 { 0 } else { *size as u8 };
        out.extend_from_slice(&[dim, dim, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += data.len() as u32;
    }
    for (_, data) in &blobs {
        out.extend_from_slice(data);
    }
    std::fs::write(path, out)
}

fn dib(size: u32, rgba: &[u8]) -> Vec<u8> {
    let mask_row = size.div_ceil(32) * 4;
    let mut out = Vec::new();
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(size as i32).to_le_bytes());
    out.extend_from_slice(&((size * 2) as i32).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(size * size * 4 + mask_row * size).to_le_bytes());
    out.extend_from_slice(&[0u8; 16]);
    for y in (0..size).rev() {
        for x in 0..size {
            let i = ((y * size + x) * 4) as usize;
            out.extend_from_slice(&[rgba[i + 2], rgba[i + 1], rgba[i], rgba[i + 3]]);
        }
    }
    for y in (0..size).rev() {
        let mut row = vec![0u8; mask_row as usize];
        for x in 0..size {
            if rgba[((y * size + x) * 4 + 3) as usize] == 0 {
                row[(x / 8) as usize] |= 0x80 >> (x % 8);
            }
        }
        out.extend_from_slice(&row);
    }
    out
}

/// ICNS with PNG payloads (macOS 10.7+).
fn write_icns(path: &Path, images: &[(u32, Vec<u8>, Vec<u8>)]) -> std::io::Result<()> {
    let png = |size: u32| &images.iter().find(|(s, ..)| *s == size).expect("rendered size").2;
    let entries: [(&[u8; 4], u32); 11] = [
        (b"icp4", 16),
        (b"icp5", 32),
        (b"icp6", 64),
        (b"ic07", 128),
        (b"ic08", 256),
        (b"ic09", 512),
        (b"ic10", 1024),
        (b"ic11", 32),
        (b"ic12", 64),
        (b"ic13", 256),
        (b"ic14", 512),
    ];
    let mut body = Vec::new();
    for (kind, size) in entries {
        let data = png(size);
        body.extend_from_slice(kind);
        body.extend_from_slice(&(data.len() as u32 + 8).to_be_bytes());
        body.extend_from_slice(data);
    }
    let mut file = BufWriter::new(File::create(path)?);
    file.write_all(b"icns")?;
    file.write_all(&(body.len() as u32 + 8).to_be_bytes())?;
    file.write_all(&body)?;
    file.flush()
}
