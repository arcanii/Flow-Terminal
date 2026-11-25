//! iTerm2's inline images, `ESC ] 1337 ; File = <args> : <base64> BEL`
//! (or ST), as imgcat, chafa, timg, viu and yazi send them, or a big one
//! in parts, as iTerm2 3.5's imgcat sends it: `MultipartFile = <args>`,
//! then the base64 in any number of `FilePart = <base64>`, then `FileEnd`,
//! which the image store gathers (`ImageStore::start_file`). The scanner
//! finds them (`crate::osc`'s `Marker::ITermImage`), keeping their body from
//! the emulator; this reads the arguments, decodes the file and works out
//! the cells it takes, and the session keeps it and places it at the
//! cursor as a kitty picture is (`ImageStore::show`).
//!
//! The arguments, `key=value` parted by `;`, as iTerm2's documentation has
//! them (<https://iterm2.com/documentation-images.html>):
//! - `inline=1` shows the file; without it iTerm2 downloads it, and Flow
//!   does nothing with it;
//! - `width` and `height`: `N` cells, `Npx` pixels, `N%` of the terminal's
//!   width or height, or `auto`, the picture's own;
//! - `preserveAspectRatio=0` stretches it to fill what they give, where 1,
//!   the default, fits it in them;
//! - `name` (its file name in base64) and `size` (its bytes) say what it is;
//! - `doNotMoveCursor=1`, WezTerm's, leaves the cursor where it was, where
//!   otherwise it goes after the picture's last row, right of it.
//!
//! PNG, JPEG, BMP, TIFF (its first page), GIF and WebP are shown, a GIF or
//! a WebP that moves playing (`crate::frames`); anything else is refused,
//! with a line in the log saying what it was.

use base64::Engine;

use crate::graphics::{self, Image};

/// How wide or tall a picture is asked to be.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Extent {
    /// Its own size.
    Auto,
    Cells(u32),
    Pixels(u32),
    /// A share of the terminal's width or height.
    Percent(u32),
}

impl Extent {
    fn parse(text: &str) -> Extent {
        let number = |n: &str| n.trim().parse::<u32>().ok().filter(|&n| n > 0);
        let text = text.trim();
        if let Some(px) = text.strip_suffix("px") {
            number(px).map_or(Extent::Auto, Extent::Pixels)
        } else if let Some(share) = text.strip_suffix('%') {
            number(share).map_or(Extent::Auto, Extent::Percent)
        } else {
            number(text).map_or(Extent::Auto, Extent::Cells)
        }
    }

    /// In pixels, for cells of `cell` pixels and a terminal `cells` of them
    /// that way; none for its own size.
    fn pixels(self, cell: f64, cells: u32) -> Option<f64> {
        match self {
            Extent::Auto => None,
            Extent::Cells(n) => Some(n as f64 * cell),
            Extent::Pixels(n) => Some(n as f64),
            Extent::Percent(p) => Some(p as f64 / 100.0 * cells as f64 * cell),
        }
    }
}

/// What a `File=` asks, unknown keys left out.
#[derive(Debug, Clone, PartialEq)]
pub struct Args {
    /// Its file name, from base64.
    pub name: Option<String>,
    /// Its size in bytes, as the program says.
    pub size: Option<u64>,
    pub width: Extent,
    pub height: Extent,
    /// Fitted in its width and height, not stretched to them.
    pub keep_shape: bool,
    /// Shown, not downloaded.
    pub inline: bool,
    /// The cursor stays where it was.
    pub stay: bool,
}

impl Default for Args {
    fn default() -> Self {
        Self { name: None, size: None, width: Extent::Auto, height: Extent::Auto, keep_shape: true, inline: false, stay: false }
    }
}

/// Reads the arguments between `File=` and the `:` before the file.
pub fn parse_args(text: &str) -> Args {
    let mut args = Args::default();
    for field in text.split(';') {
        let Some((key, value)) = field.split_once('=') else { continue };
        let value = value.trim();
        match key.trim() {
            "name" => {
                args.name = base64::engine::general_purpose::STANDARD.decode(value).ok().map(|n| String::from_utf8_lossy(&n).into_owned());
            }
            "size" => args.size = value.parse().ok(),
            "width" => args.width = Extent::parse(value),
            "height" => args.height = Extent::parse(value),
            "preserveAspectRatio" => args.keep_shape = value != "0",
            "inline" => args.inline = value == "1",
            "doNotMoveCursor" => args.stay = value == "1",
            _ => {}
        }
    }
    args
}

/// The picture in a file sent in base64: a PNG, a JPEG, a BMP, a TIFF (its
/// first page), or a GIF or a WebP still or moving, premultiplied as
/// [`Image::new`] takes it. What's wrong, for the log, otherwise.
pub fn decode(base64: &[u8]) -> Result<Image, String> {
    let mut bytes = Vec::new();
    graphics::decode_base64(base64, &mut bytes)?;
    let (width, height, mut rgba) = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        graphics::decode_png(&bytes)?
    } else if crate::frames::is_gif(&bytes) || crate::frames::is_webp(&bytes) {
        return crate::frames::decode(&bytes);
    } else if let Some((format, name)) = still_format(&bytes) {
        decode_still(&bytes, format, name)?
    } else {
        return Err(format!("not a PNG, a JPEG, a BMP, a TIFF, a GIF or a WebP but {}", kind_of(&bytes)));
    };
    graphics::premultiply(&mut rgba);
    Ok(Image::new(width, height, rgba))
}

/// The still pictures the `image` crate decodes for Flow, known by their
/// first bytes, with their names for the log: a JPEG, a BMP (Windows'
/// own, its "BM"), and a TIFF either way round (Intel's "II", Motorola's
/// "MM").
fn still_format(bytes: &[u8]) -> Option<(image::ImageFormat, &'static str)> {
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some((image::ImageFormat::Jpeg, "a JPEG"))
    } else if bytes.starts_with(b"BM") {
        Some((image::ImageFormat::Bmp, "a BMP"))
    } else if bytes.starts_with(b"II*\0") || bytes.starts_with(b"MM\0*") {
        Some((image::ImageFormat::Tiff, "a TIFF"))
    } else {
        None
    }
}

/// A still picture's pixels, as RGBA, within the sizes a kitty picture
/// keeps to: deeper ones (a 16-bit TIFF) brought down to 8 bits a channel,
/// a BMP's or a TIFF's alpha kept, and a TIFF's first page alone.
fn decode_still(bytes: &[u8], format: image::ImageFormat, name: &str) -> Result<(u32, u32, Vec<u8>), String> {
    let mut reader = image::ImageReader::with_format(std::io::Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(graphics::MAX_SIDE);
    limits.max_image_height = Some(graphics::MAX_SIDE);
    limits.max_alloc = Some(graphics::MAX_DATA as u64);
    reader.limits(limits);
    let picture = reader.decode().map_err(|e| format!("{name} that won't decode: {e}"))?.into_rgba8();
    Ok((picture.width(), picture.height(), picture.into_raw()))
}

/// What a file that isn't a picture Flow shows looks like, from its first
/// bytes, for the log.
fn kind_of(bytes: &[u8]) -> &'static str {
    let brand = bytes.get(4..12);
    if bytes.starts_with(b"%PDF") {
        "a PDF"
    } else if brand.is_some_and(|b| [&b"ftypheic"[..], b"ftypheix", b"ftypmif1"].contains(&b)) {
        "a HEIC"
    } else if brand == Some(b"ftypavif") {
        "an AVIF"
    } else {
        "something else"
    }
}

/// The cells a picture `w` × `h` pixels takes as `args` ask, with cells
/// `cell` pixels and a terminal `screen` cells (columns, rows): its columns
/// and rows, 0 and 0 for its own size, and whether it's stretched to fill
/// them rather than fitted in them.
pub fn cells(args: &Args, (w, h): (u32, u32), cell: (u32, u32), screen: (u32, u32)) -> (u32, u32, bool) {
    let (cw, ch) = (cell.0.max(1) as f64, cell.1.max(1) as f64);
    let (fw, fh) = (w.max(1) as f64, h.max(1) as f64);
    let (bw, bh) = match (args.width.pixels(cw, screen.0), args.height.pixels(ch, screen.1)) {
        (None, None) => return (0, 0, false),
        (Some(bw), None) if args.keep_shape => (bw, bw * fh / fw),
        (None, Some(bh)) if args.keep_shape => (bh * fw / fh, bh),
        (bw, bh) => (bw.unwrap_or(fw), bh.unwrap_or(fh)),
    };
    let count = |px: f64, cell: f64| ((px / cell).ceil() as u32).max(1);
    (count(bw, cw), count(bh, ch), !args.keep_shape)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A PNG `w` × `h`, every pixel `rgba`.
    pub(crate) fn png(w: u32, h: u32, rgba: [u8; 4]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().unwrap();
        writer.write_image_data(&rgba.repeat((w * h) as usize)).unwrap();
        drop(writer);
        out
    }

    /// A JPEG `w` × `h`, every pixel `rgb`.
    pub(crate) fn jpeg(w: u32, h: u32, rgb: [u8; 3]) -> Vec<u8> {
        let mut out = Vec::new();
        let pixels = rgb.repeat((w * h) as usize);
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 95).encode(&pixels, w, h, image::ExtendedColorType::Rgb8).unwrap();
        out
    }

    /// A BMP `w` × `h`, every pixel `rgba`, its alpha kept (32 bits a
    /// pixel).
    pub(crate) fn bmp(w: u32, h: u32, rgba: [u8; 4]) -> Vec<u8> {
        let mut out = Vec::new();
        image::codecs::bmp::BmpEncoder::new(&mut out).encode(&rgba.repeat((w * h) as usize), w, h, image::ExtendedColorType::Rgba8).unwrap();
        out
    }

    /// A TIFF `w` × `h`, every pixel `rgb` at 16 bits a channel, written
    /// with the byte order `II` (Intel's), as most are.
    pub(crate) fn tiff16(w: u32, h: u32, rgb: [u16; 3]) -> Vec<u8> {
        let mut out = std::io::Cursor::new(Vec::new());
        let pixels: Vec<u8> = rgb.repeat((w * h) as usize).iter().flat_map(|v| v.to_ne_bytes()).collect();
        image::ImageEncoder::write_image(image::codecs::tiff::TiffEncoder::new(&mut out), &pixels, w, h, image::ExtendedColorType::Rgb16).unwrap();
        out.into_inner()
    }

    pub(crate) fn b64(bytes: &[u8]) -> Vec<u8> {
        base64::engine::general_purpose::STANDARD.encode(bytes).into_bytes()
    }

    #[test]
    fn the_arguments_are_read_and_unknown_ones_left_out() {
        let args = parse_args("name=Y2F0LnBuZw==;size=1234;width=10;height=50%;preserveAspectRatio=0;inline=1;type=image/png");
        assert_eq!(
            args,
            Args { name: Some("cat.png".into()), size: Some(1234), width: Extent::Cells(10), height: Extent::Percent(50), keep_shape: false, inline: true, stay: false }
        );
        assert_eq!(parse_args("inline=1;width=120px;height=auto;doNotMoveCursor=1"), Args { width: Extent::Pixels(120), inline: true, stay: true, ..Args::default() });
        // What can't be read is its own size; without `inline=1`, a download.
        assert_eq!(parse_args("width=x;height=-3;width"), Args::default());
        assert_eq!(parse_args("width=0px").width, Extent::Auto);
    }

    #[test]
    fn a_png_a_jpeg_a_gif_and_a_webp_decode_premultiplied_and_other_files_are_refused() {
        let image = decode(&b64(&png(3, 2, [255, 0, 0, 128]))).unwrap();
        assert_eq!((image.width, image.height, &image.pixels[..4]), (3, 2, &[128, 0, 0, 128][..]));
        let image = decode(&b64(&jpeg(16, 8, [0, 0, 255]))).unwrap();
        assert_eq!((image.width, image.height, image.pixels[3]), (16, 8, 255));
        let px = &image.pixels[..3];
        assert!(px[0] < 16 && px[1] < 16 && px[2] > 240, "blue, near enough: {px:?}");
        let gif = crate::frames::tests::gif(&[(&[1; 8], [0, 0, 4, 2], 0, gif::DisposalMethod::Keep, None)], gif::Repeat::Infinite);
        let image = decode(&b64(&gif)).unwrap();
        assert_eq!((image.width, image.height, &image.pixels[..4]), (4, 2, &[0, 255, 0, 255][..]));
        let webp = crate::frames::tests::webp(&[([255, 0, 0, 255], 50), ([0, 0, 255, 255], 50)], 0);
        assert_eq!(decode(&b64(&webp)).unwrap().frames.as_ref().map(|f| f.len()), Some(2), "moving");
        assert_eq!(decode(&b64(b"%PDF-1.7")).err().as_deref(), Some("not a PNG, a JPEG, a BMP, a TIFF, a GIF or a WebP but a PDF"));
        assert_eq!(decode(&b64(b"\0\0\0\x18ftypheic\0\0\0\0")).err().as_deref(), Some("not a PNG, a JPEG, a BMP, a TIFF, a GIF or a WebP but a HEIC"));
        assert!(decode(&b64(b"\xFF\xD8\xFFnot really")).unwrap_err().starts_with("a JPEG that won't decode"));
        assert!(decode(&b64(b"BM\x01\x00\x01\x00")).unwrap_err().starts_with("a BMP that won't decode"));
        assert!(decode(&b64(b"II*\0\x08\0\0\0")).unwrap_err().starts_with("a TIFF that won't decode"));
        assert!(decode(&b64(b"GIF89a\x01\x00\x01\x00")).unwrap_err().starts_with("a GIF that won't decode"));
        assert!(decode(b"!!!").is_err(), "not base64");
    }

    #[test]
    fn a_bmp_and_a_tiff_decode_premultiplied() {
        // A BMP keeps its alpha; a 16-bit TIFF comes down to 8 bits.
        let image = decode(&b64(&bmp(5, 3, [0, 255, 0, 128]))).unwrap();
        assert_eq!((image.width, image.height, &image.pixels[..4]), (5, 3, &[0, 128, 0, 128][..]));
        let tiff = tiff16(4, 6, [65535, 32896, 0]);
        assert!(tiff.starts_with(b"II*\0") || tiff.starts_with(b"MM\0*"), "a TIFF's own first bytes");
        let image = decode(&b64(&tiff)).unwrap();
        assert_eq!((image.width, image.height, &image.pixels[..4]), (4, 6, &[255, 128, 0, 255][..]));
        assert_eq!(image.pixels.len(), 4 * 6 * 4);
    }

    #[test]
    fn the_cells_a_picture_takes_follow_its_width_and_height() {
        let cell = (10, 20);
        let screen = (80, 24);
        let take = |args: &str| cells(&parse_args(args), (200, 100), cell, screen);
        assert_eq!(take("inline=1"), (0, 0, false), "its own size");
        // Ten cells wide, its shape kept: 100 × 50 pixels, so three rows.
        assert_eq!(take("inline=1;width=10"), (10, 3, false));
        assert_eq!(take("inline=1;height=5"), (20, 5, false));
        assert_eq!(take("inline=1;width=50px"), (5, 2, false));
        assert_eq!(take("inline=1;width=25%"), (20, 5, false));
        // Both: fitted in them, or stretched to fill them.
        assert_eq!(take("inline=1;width=4;height=4"), (4, 4, false));
        assert_eq!(take("inline=1;width=4;height=4;preserveAspectRatio=0"), (4, 4, true));
        // Stretched one way, the other its own.
        assert_eq!(take("inline=1;width=40;preserveAspectRatio=0"), (40, 5, true));
    }
}
