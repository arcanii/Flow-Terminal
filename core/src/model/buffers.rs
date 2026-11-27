//! A glTF's bytes as the loader reads them: its buffers (the binary chunk,
//! data URIs, and meshopt's fallback buffers, which hold nothing until the
//! views compressed with EXT_meshopt_compression are unpacked into them,
//! `super::meshopt`); its accessors read as floats whatever their
//! component type, as KHR_mesh_quantization lets positions, normals,
//! tangents and texture coordinates be bytes and shorts, normalized or
//! not, sparse ones too; and its images, each decoded the first time a
//! texture needs it: PNG and JPEG as the gltf crate decodes them, WebP
//! (EXT_texture_webp) with image-webp, and Basis textures in KTX2
//! (KHR_texture_basisu) transcoded to RGBA with basisu. An image Flow can't
//! decode leaves its textures out rather than the model, so a file whose
//! textures offer another source (a WebP's or a KTX2's fallback) shows the
//! one it can.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

use base64::Engine as _;
use gltf::accessor::DataType;

use super::meshopt::{self, Filter, Mode};
use super::Texture;

/// What meshopt's views unpack to, all told, at most (and Draco's meshes
/// decode to, `super::draco`): four times the
/// biggest file Flow reads (`super::MAX_BYTES`), as meshopt's own packing
/// takes some three quarters off, and far more than the most triangles a
/// model may have (`MAX_TRIANGLES`) take with all they carry.
pub const MAX_UNPACKED: usize = 256 << 20;
/// A texture's pixels at most: 8192 × 8192.
const MAX_PIXELS: u64 = 1 << 26;
/// The names meshopt's compression goes by: the first its own, the second
/// Khronos's, which may also hold the vertex codec's version 1 and the
/// color filter (both read whichever name the file uses).
const MESHOPT: [&str; 2] = ["EXT_meshopt_compression", "KHR_meshopt_compression"];

/// The file's buffers, with every view meshopt compressed unpacked into
/// its own place. `blob` is a binary glTF's chunk of bytes, which the first
/// buffer without a URI holds; a URI that names a file isn't read, as
/// models come as bytes or from one file the user allowed. Read from the
/// JSON before the gltf crate checks the document (which a Draco file's
/// accessors pass only once they're decoded, `super::draco`), so every
/// index here is checked as it's taken.
pub fn read(json: &gltf::json::Root, mut blob: Option<Vec<u8>>) -> Result<Vec<gltf::buffer::Data>, String> {
    let mut fallbacks = 0usize;
    let mut buffers = Vec::new();
    for (index, buffer) in json.buffers.iter().enumerate() {
        let extension = |name: &str| buffer.extensions.as_ref().and_then(|e| e.others.get(name));
        let fallback = MESHOPT.iter().any(|e| extension(e).and_then(|v| v.get("fallback")).and_then(|f| f.as_bool()) == Some(true));
        let length = usize::try_from(buffer.byte_length.0).map_err(|e| e.to_string())?;
        let mut data = match buffer.uri.as_deref() {
            Some(uri) if uri.starts_with("data:") => data_uri(uri)?,
            // A fallback buffer is for viewers that don't unpack the views:
            // here it's where they're unpacked to.
            _ if fallback => {
                fallbacks = fallbacks.saturating_add(length);
                if fallbacks > MAX_UNPACKED {
                    return Err(format!("its meshopt data unpack to more than {} MB", MAX_UNPACKED >> 20));
                }
                vec![0; length]
            }
            None => blob.take().ok_or_else(|| gltf::Error::MissingBlob.to_string())?,
            Some(_) => return Err(gltf::Error::ExternalReferenceInSliceImport.to_string()),
        };
        if data.len() < length {
            return Err(gltf::Error::BufferLength { buffer: index, expected: length, actual: data.len() }.to_string());
        }
        data.resize(data.len().next_multiple_of(4), 0);
        buffers.push(gltf::buffer::Data(data));
    }
    unpack(json, &mut buffers)?;
    Ok(buffers)
}

/// Unpacks each view meshopt compressed into its own place in its buffer,
/// every one read before any is written, as one's packed bytes may lie
/// where another's unpacked ones go.
fn unpack(json: &gltf::json::Root, buffers: &mut [gltf::buffer::Data]) -> Result<(), String> {
    let mut unpacked = Vec::new();
    let mut total = 0usize;
    for (index, view) in json.buffer_views.iter().enumerate() {
        let Some(ext) = MESHOPT.iter().find_map(|e| view.extensions.as_ref().and_then(|x| x.others.get(*e))) else { continue };
        let bad = || format!("buffer view {index}'s meshopt compression is malformed");
        let number = |key: &str| ext.get(key).and_then(|v| v.as_u64()).and_then(|v| usize::try_from(v).ok());
        let (buffer, offset, length) = (number("buffer").ok_or_else(bad)?, number("byteOffset").unwrap_or(0), number("byteLength").ok_or_else(bad)?);
        let (stride, count) = (number("byteStride").ok_or_else(bad)?, number("count").ok_or_else(bad)?);
        let mode = ext.get("mode").and_then(|m| m.as_str()).and_then(Mode::parse).ok_or_else(bad)?;
        let filter = match ext.get("filter").and_then(|f| f.as_str()) {
            None => Filter::None,
            Some(name) => Filter::parse(name).ok_or_else(|| format!("meshopt's {name} filter, which Flow doesn't read"))?,
        };
        let size = count.checked_mul(stride).ok_or_else(bad)?;
        total = total.saturating_add(size);
        if total > MAX_UNPACKED {
            return Err(format!("its meshopt data unpack to more than {} MB", MAX_UNPACKED >> 20));
        }
        let packed = buffers.get(buffer).and_then(|b| b.0.get(offset..offset.checked_add(length)?)).ok_or_else(bad)?;
        let bytes = meshopt::unpack(packed, count, stride, mode, filter).map_err(|e| format!("buffer view {index}: {e}"))?;
        let place = (view.byte_offset.map_or(0, |o| o.0), view.byte_length.0);
        unpacked.push((index, view.buffer.value(), place, bytes));
    }
    for (index, buffer, (offset, length), bytes) in unpacked {
        let reaches = || format!("buffer view {index} reaches past its buffer");
        let (offset, length) = (usize::try_from(offset).map_err(|_| reaches())?, usize::try_from(length).map_err(|_| reaches())?);
        let n = length.min(bytes.len());
        let place = buffers.get_mut(buffer).and_then(|b| b.0.get_mut(offset..offset.checked_add(n)?));
        place.ok_or_else(reaches)?.copy_from_slice(&bytes[..n]);
    }
    Ok(())
}

/// A data URI's bytes, which glTF has in base64.
fn data_uri(uri: &str) -> Result<Vec<u8>, String> {
    let (_, data) = uri.split_once(";base64,").ok_or("a data URI not in base64")?;
    base64::engine::general_purpose::STANDARD.decode(data).map_err(|e| e.to_string())
}

/// An accessor's values as floats, `N` to an element, whatever its
/// component type: a normalized integer scaled as glTF scales it (to 0..1
/// unsigned, -1..1 signed), any other as its number; then a sparse
/// accessor's values over them. None where its elements aren't `N` wide
/// or it reaches past its views.
pub fn floats<const N: usize>(accessor: &gltf::Accessor, buffers: &[gltf::buffer::Data]) -> Option<Vec<[f32; N]>> {
    if accessor.dimensions().multiplicity() != N {
        return None;
    }
    let (kind, normalized) = (accessor.data_type(), accessor.normalized());
    let size = kind.size();
    let element = |bytes: &[u8]| -> [f32; N] { std::array::from_fn(|k| number(&bytes[k * size..], kind, normalized)) };
    // Without a view, every value starts at 0: as many as could have been
    // unpacked, at most.
    let dense = match accessor.view() {
        Some(view) => Some(elements(&view, accessor.offset(), accessor.count(), N * size, buffers)?),
        None if accessor.count() > MAX_UNPACKED / (4 * N) => return None,
        None => None,
    };
    let mut out = vec![[0.0; N]; accessor.count()];
    for (o, bytes) in out.iter_mut().zip(dense.into_iter().flatten()) {
        *o = element(bytes);
    }
    if let Some(sparse) = accessor.sparse() {
        let (indices, values) = (sparse.indices(), sparse.values());
        let wide = match indices.index_type() {
            gltf::accessor::sparse::IndexType::U8 => 1,
            gltf::accessor::sparse::IndexType::U16 => 2,
            gltf::accessor::sparse::IndexType::U32 => 4,
        };
        let at = elements(&indices.view(), indices.offset(), sparse.count(), wide, buffers)?;
        let with = elements(&values.view(), values.offset(), sparse.count(), N * size, buffers)?;
        for (i, bytes) in at.zip(with) {
            let i = (0..wide).fold(0usize, |n, k| n | usize::from(i[k]) << (8 * k));
            *out.get_mut(i)? = element(bytes);
        }
    }
    Some(out)
}

/// `count` elements of `size` bytes from `view`, from `offset` in it,
/// each its stride apart (or packed, where it gives none); None where they
/// reach past it.
fn elements<'a>(view: &gltf::buffer::View, offset: usize, count: usize, size: usize, buffers: &'a [gltf::buffer::Data]) -> Option<impl Iterator<Item = &'a [u8]> + 'a> {
    let data = buffers.get(view.buffer().index())?.0.get(view.offset()..view.offset().checked_add(view.length())?)?;
    let stride = view.stride().unwrap_or(size);
    if stride < size {
        return None;
    }
    if count > 0 && offset.checked_add(stride.checked_mul(count - 1)?)?.checked_add(size)? > data.len() {
        return None;
    }
    Some((0..count).map(move |i| &data[offset + i * stride..offset + i * stride + size]))
}

/// One component, little-endian.
fn number(b: &[u8], kind: DataType, normalized: bool) -> f32 {
    match (kind, normalized) {
        (DataType::I8, false) => f32::from(b[0] as i8),
        (DataType::I8, true) => (f32::from(b[0] as i8) / 127.0).max(-1.0),
        (DataType::U8, false) => f32::from(b[0]),
        (DataType::U8, true) => f32::from(b[0]) / 255.0,
        (DataType::I16, false) => f32::from(i16::from_le_bytes([b[0], b[1]])),
        (DataType::I16, true) => (f32::from(i16::from_le_bytes([b[0], b[1]])) / 32767.0).max(-1.0),
        (DataType::U16, false) => f32::from(u16::from_le_bytes([b[0], b[1]])),
        (DataType::U16, true) => f32::from(u16::from_le_bytes([b[0], b[1]])) / 65535.0,
        (DataType::U32, false) => u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32,
        (DataType::U32, true) => u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32 / u32::MAX as f32,
        (DataType::F32, _) => f32::from_le_bytes([b[0], b[1], b[2], b[3]]),
    }
}

/// Iridescence's strength and thickness maps, by their textures' indices.
type FilmMaps = (Option<usize>, Option<usize>);

/// A file's images, each decoded the first time a texture needs it.
pub struct Images<'a> {
    doc: &'a gltf::Document,
    buffers: &'a [gltf::buffer::Data],
    decoded: HashMap<usize, Option<Arc<Texture>>>,
    /// Roughness maps baked from a specular-glossiness texture's alpha, by
    /// the texture and the glossiness factor's bits.
    baked: HashMap<(usize, u32), Option<Arc<Texture>>>,
    /// Iridescence's two maps made one, by their textures.
    films: HashMap<FilmMaps, Option<Arc<Texture>>>,
}

impl<'a> Images<'a> {
    pub fn new(doc: &'a gltf::Document, buffers: &'a [gltf::buffer::Data]) -> Self {
        Images { doc, buffers, decoded: HashMap::new(), baked: HashMap::new(), films: HashMap::new() }
    }

    /// KHR_materials_iridescence's strength map (its red) and thickness map
    /// (its green) as one texture, as the shader reads them, since a
    /// material's sixteen textures leave it one: either texture itself
    /// where they're the same, else made from them, the strength's red and
    /// the thickness's green, white where it has none (all its strength,
    /// its thickest film), the size of the strength's where it has one,
    /// the thickness's read at the same place in its own. Read through the
    /// strength map's uvs where they differ (`glb::read_material`). Made
    /// once for each pair, so the parts that share a material share it.
    pub fn film(&mut self, strength: Option<&gltf::Texture>, thickness: Option<&gltf::Texture>) -> Option<Arc<Texture>> {
        let key = (strength.map(|t| t.index()), thickness.map(|t| t.index()));
        if let Some(made) = self.films.get(&key) {
            return made.clone();
        }
        let made = match (strength, thickness) {
            (Some(s), Some(t)) if s.index() == t.index() => self.texture(s),
            _ => {
                let (s, t) = (strength.and_then(|s| self.texture(s)), thickness.and_then(|t| self.texture(t)));
                (s.is_some() || t.is_some()).then(|| Arc::new(film_of(s.as_deref(), t.as_deref())))
            }
        };
        self.films.insert(key, made.clone());
        made
    }

    /// A metallic-roughness texture made from a specular-glossiness one, as
    /// glTF-Transform's `metalRough()` makes it: roughness in green, one
    /// less its alpha (the glossiness) times `factor`, and no metal. Made
    /// once for each texture and factor, so the parts that share a
    /// material share it.
    pub fn roughness_from_gloss(&mut self, t: &gltf::Texture, factor: f32) -> Option<Arc<Texture>> {
        let key = (t.index(), factor.to_bits());
        if let Some(baked) = self.baked.get(&key) {
            return baked.clone();
        }
        let baked = self.texture(t).map(|sg| {
            let rgba = sg.rgba.as_chunks::<4>().0.iter().flat_map(|p| [0, 255 - (f32::from(p[3]) * factor.clamp(0.0, 1.0)).round() as u8, 0, 255]).collect();
            Arc::new(Texture { width: sg.width, height: sg.height, rgba })
        });
        self.baked.insert(key, baked.clone());
        baked
    }

    /// A texture's picture: its KTX2 (KHR_texture_basisu) or its WebP
    /// (EXT_texture_webp) where it has one that decodes, else its own
    /// source, as a file offering more than one expects of a viewer that
    /// reads them.
    pub fn texture(&mut self, t: &gltf::Texture) -> Option<Arc<Texture>> {
        let source = |name: &str| t.extension_value(name).and_then(|v| v.get("source")?.as_u64()).and_then(|i| usize::try_from(i).ok());
        let (basis, webp) = (source("KHR_texture_basisu"), source("EXT_texture_webp"));
        basis.into_iter().chain(webp).chain([t.source().index()]).find_map(|i| self.image(i))
    }

    fn image(&mut self, i: usize) -> Option<Arc<Texture>> {
        if let Some(t) = self.decoded.get(&i) {
            return t.clone();
        }
        let t = self.decode(i).map(Arc::new);
        self.decoded.insert(i, t.clone());
        t
    }

    fn decode(&self, i: usize) -> Option<Texture> {
        let image = self.doc.images().nth(i)?;
        let source = image.source();
        let bytes: Cow<[u8]> = match source {
            gltf::image::Source::View { ref view, .. } => {
                Cow::Borrowed(self.buffers.get(view.buffer().index())?.0.get(view.offset()..view.offset().checked_add(view.length())?)?)
            }
            gltf::image::Source::Uri { uri, .. } if uri.starts_with("data:") => Cow::Owned(data_uri(uri).ok()?),
            gltf::image::Source::Uri { .. } => return None,
        };
        // Known by its bytes, as a file may call it anything.
        if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
            return webp(&bytes);
        }
        if bytes.starts_with(&KTX2) {
            return basis(&bytes);
        }
        // PNG and JPEG, as the gltf crate decodes them. It reads a data
        // URI only when given a folder, which a data URI never reads from.
        let folder = matches!(source, gltf::image::Source::Uri { .. }).then(|| std::path::Path::new(""));
        to_rgba(&gltf::image::Data::from_source(source, folder, self.buffers).ok()?)
    }
}

/// Iridescence's strength (`s`'s red) and thickness (`t`'s green) in one
/// texture, white where either is missing, at `s`'s size where it has one;
/// `t` is read at the same place in its own (nearest), so the two needn't
/// be the same size.
fn film_of(s: Option<&Texture>, t: Option<&Texture>) -> Texture {
    let (width, height) = s.or(t).map_or((1, 1), |p| (p.width, p.height));
    let at = |p: &Texture, x: u32, y: u32, k: usize| {
        let (px, py) = ((x as u64 * p.width as u64 / width as u64) as u32, (y as u64 * p.height as u64 / height as u64) as u32);
        p.rgba[(py.min(p.height - 1) as usize * p.width as usize + px.min(p.width - 1) as usize) * 4 + k]
    };
    let rgba = (0..height).flat_map(|y| (0..width).map(move |x| (x, y))).flat_map(|(x, y)| [s.map_or(255, |s| at(s, x, y, 0)), t.map_or(255, |t| at(t, x, y, 1)), 255, 255]).collect();
    Texture { width, height, rgba }
}

/// The 12 bytes a KTX2 file starts with.
const KTX2: [u8; 12] = [0xab, b'K', b'T', b'X', b' ', b'2', b'0', 0xbb, b'\r', b'\n', 0x1a, b'\n'];

/// A Basis Universal texture in KTX2 (ETC1S or UASTC, its levels
/// supercompressed or not), its largest level transcoded to RGBA bytes,
/// as they were before it was encoded (straight alpha, sRGB where it's a
/// color). Its first layer or face, of an array or a cube; none for an HDR
/// one, which has no RGBA bytes, or a video.
fn basis(bytes: &[u8]) -> Option<Texture> {
    let texture = basisu::Transcoder::new(bytes).ok()?;
    let (width, height) = texture.base_dimensions();
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_PIXELS {
        return None;
    }
    let rgba = texture.transcode_image(0, 0, 0, basisu::TargetFormat::Rgba32, basisu::DecodeFlags::NONE).ok()?;
    (rgba.len() as u64 == u64::from(width) * u64::from(height) * 4).then_some(Texture { width, height, rgba })
}

/// A WebP image (lossy or lossless, the first frame of an animated one) as
/// RGBA bytes.
fn webp(bytes: &[u8]) -> Option<Texture> {
    let mut decoder = image_webp::WebPDecoder::new(std::io::Cursor::new(bytes)).ok()?;
    let (width, height) = decoder.dimensions();
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_PIXELS {
        return None;
    }
    let mut pixels = vec![0; decoder.output_buffer_size()?];
    decoder.read_image(&mut pixels).ok()?;
    let rgba = if decoder.has_alpha() { pixels } else { pixels.as_chunks::<3>().0.iter().flat_map(|p| [p[0], p[1], p[2], 255]).collect() };
    Some(Texture { width, height, rgba })
}

/// A glTF image as RGBA bytes, from 8 or 16 bits a channel (16-bit PNGs are
/// common for normal maps).
fn to_rgba(img: &gltf::image::Data) -> Option<Texture> {
    use gltf::image::Format as F;
    let (channels, wide) = match img.format {
        F::R8 => (1, false),
        F::R8G8 => (2, false),
        F::R8G8B8 => (3, false),
        F::R8G8B8A8 => (4, false),
        F::R16 => (1, true),
        F::R16G16 => (2, true),
        F::R16G16B16 => (3, true),
        F::R16G16B16A16 => (4, true),
        _ => return None,
    };
    // 16-bit channels come in the machine's byte order: their high byte.
    let bytes: Vec<u8> = if wide { img.pixels.as_chunks::<2>().0.iter().map(|b| (u16::from_ne_bytes(*b) >> 8) as u8).collect() } else { img.pixels.clone() };
    let rgba = bytes
        .chunks_exact(channels)
        .flat_map(|p| match channels {
            1 => [p[0], p[0], p[0], 255],
            2 => [p[0], p[0], p[0], p[1]],
            3 => [p[0], p[1], p[2], 255],
            _ => [p[0], p[1], p[2], p[3]],
        })
        .collect();
    Some(Texture { width: img.width, height: img.height, rgba })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iridescences_two_maps_are_made_one_at_the_strengths_size() {
        let strength = Texture { width: 2, height: 2, rgba: vec![10, 0, 0, 255, 20, 0, 0, 255, 30, 0, 0, 255, 40, 0, 0, 255] };
        let thickness = Texture { width: 1, height: 1, rgba: vec![0, 99, 0, 255] };
        let both = film_of(Some(&strength), Some(&thickness));
        assert_eq!((both.width, both.height), (2, 2));
        assert_eq!(both.rgba.chunks(4).map(|p| (p[0], p[1])).collect::<Vec<_>>(), [(10, 99), (20, 99), (30, 99), (40, 99)]);
        // Missing, all its strength, or its thickest film.
        assert_eq!(film_of(None, Some(&thickness)).rgba, [255, 99, 255, 255]);
        assert_eq!(film_of(Some(&strength), None).rgba.chunks(4).map(|p| p[1]).collect::<Vec<_>>(), [255; 4]);
        // A thickness map wider than the strength's is read at its place.
        let wide = Texture { width: 4, height: 1, rgba: [1, 2, 3, 4].iter().flat_map(|&g| [0, g, 0, 255]).collect() };
        let small = Texture { width: 2, height: 1, rgba: vec![0, 0, 0, 255, 0, 0, 0, 255] };
        assert_eq!(film_of(Some(&small), Some(&wide)).rgba.chunks(4).map(|p| p[1]).collect::<Vec<_>>(), [1, 3]);
    }
}
