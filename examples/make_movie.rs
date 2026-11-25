//! Encodes a folder of PNG frames from `flow --record <dir>` into a looping
//! animated WebP for the README (GitHub plays those inline; it won't play an
//! mp4 that lives in the repo). Frames are shrunk with an area filter first.
//!
//!   cargo run --release --example make_movie -- <frames-dir> <out.webp> [width] [quality] [fps]
//!
//! Defaults: 640 px wide, quality 70, 24 fps (the rate `--record` uses).

use anyhow::{bail, Context};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [dir, out, rest @ ..] = args.as_slice() else {
        bail!("usage: make_movie <frames-dir> <out.webp> [width] [quality] [fps]");
    };
    let width: u32 = rest.first().map(|s| s.parse()).transpose()?.unwrap_or(640);
    let quality: f32 = rest.get(1).map(|s| s.parse()).transpose()?.unwrap_or(70.0);
    let fps: f32 = rest.get(2).map(|s| s.parse()).transpose()?.unwrap_or(24.0);

    let mut paths: Vec<_> = std::fs::read_dir(dir)
        .with_context(|| format!("reading {dir}"))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "png"))
        .collect();
    paths.sort();
    if paths.is_empty() {
        bail!("no PNG frames in {dir}");
    }

    let mut frames = Vec::with_capacity(paths.len());
    let mut size = (0, 0);
    for path in &paths {
        let (w, h, rgba) = read_png(path)?;
        let (ow, oh) = (width.min(w), (h as u64 * width.min(w) as u64 / w as u64) as u32);
        frames.push(shrink(&rgba, w, h, ow, oh));
        size = (ow, oh);
    }

    let mut config = webp::WebPConfig::new().map_err(|_| anyhow::anyhow!("libwebp config"))?;
    config.lossless = 0;
    config.quality = quality;
    config.method = 6;
    let mut encoder = webp::AnimEncoder::new(size.0, size.1, &config);
    encoder.set_loop_count(0);
    for (i, frame) in frames.iter().enumerate() {
        let ms = (i as f32 * 1000.0 / fps).round() as i32;
        encoder.add_frame(webp::AnimFrame::from_rgba(frame, size.0, size.1, ms));
    }
    let data = encoder.try_encode().map_err(|e| anyhow::anyhow!("encoding failed: {e:?}"))?;
    std::fs::write(out, &*data).with_context(|| format!("writing {out}"))?;
    println!(
        "{out}: {} frames, {}x{}, {:.1} s, {:.2} MB",
        frames.len(),
        size.0,
        size.1,
        frames.len() as f32 / fps,
        data.len() as f64 / 1_048_576.0
    );
    Ok(())
}

fn read_png(path: &std::path::Path) -> anyhow::Result<(u32, u32, Vec<u8>)> {
    let file = std::io::BufReader::new(std::fs::File::open(path)?);
    let mut decoder = png::Decoder::new(file);
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info()?;
    let mut buf = vec![0; reader.output_buffer_size().context("frame too large")?];
    let info = reader.next_frame(&mut buf)?;
    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        bail!("{}: expected 8-bit RGBA, got {:?} {:?}", path.display(), info.color_type, info.bit_depth);
    }
    buf.truncate(info.buffer_size());
    Ok((info.width, info.height, buf))
}

/// Area-average resample (every source pixel counts, weighted by overlap),
/// which keeps text and fine liquid detail smooth when shrinking a lot.
fn shrink(src: &[u8], w: u32, h: u32, ow: u32, oh: u32) -> Vec<u8> {
    let (sx, sy) = (w as f32 / ow as f32, h as f32 / oh as f32);
    let mut out = vec![0u8; (ow * oh * 4) as usize];
    for oy in 0..oh {
        let (y0, y1) = (oy as f32 * sy, (oy + 1) as f32 * sy);
        for ox in 0..ow {
            let (x0, x1) = (ox as f32 * sx, (ox + 1) as f32 * sx);
            let mut acc = [0f32; 4];
            let mut total = 0.0;
            for y in y0 as u32..(y1.ceil() as u32).min(h) {
                let wy = (y1.min(y as f32 + 1.0) - y0.max(y as f32)).max(0.0);
                for x in x0 as u32..(x1.ceil() as u32).min(w) {
                    let wx = (x1.min(x as f32 + 1.0) - x0.max(x as f32)).max(0.0);
                    let wgt = wx * wy;
                    let i = ((y * w + x) * 4) as usize;
                    for c in 0..4 {
                        acc[c] += src[i + c] as f32 * wgt;
                    }
                    total += wgt;
                }
            }
            let o = ((oy * ow + ox) * 4) as usize;
            for c in 0..4 {
                out[o + c] = (acc[c] / total).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    out
}
