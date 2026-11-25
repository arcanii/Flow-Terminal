//! Draco's compressed meshes (KHR_draco_mesh_compression), decoded with
//! draco-core, Google's codec in Rust. A compressed primitive names the
//! view its Draco bytes are in and, for each attribute it compresses, the
//! Draco attribute that holds it; its accessors say what the decoded values
//! are (their component type and how many to an element) and have no view
//! of their own, or one for viewers that don't read Draco. Each such
//! primitive is decoded into a buffer of its own: each attribute's values
//! in its accessor's component type, then the triangles' corners, each
//! behind a view, and new accessors over them take the primitive's old
//! ones' places, so the rest of the loader reads it as it would any
//! primitive once the document is made again from the JSON.

use draco_core::{DataType, DecodeLimits, DecoderBuffer, FaceIndex, MeshDecoder};
use gltf::json::accessor::{ComponentType, GenericComponentType};
use gltf::json::validation::{Checked, USize64};

const NAME: &str = "KHR_draco_mesh_compression";

/// Whether any of the file's primitives is compressed with Draco.
pub fn used(json: &gltf::json::Root) -> bool {
    json.meshes.iter().flat_map(|m| &m.primitives).any(|p| p.extensions.as_ref().is_some_and(|e| e.others.contains_key(NAME)))
}

/// Decodes every primitive compressed with Draco into a buffer of its own
/// at the end of `buffers` (and of `json`'s), pointing its accessors at
/// what was decoded: each in place (they have no view, which the gltf
/// crate's checks don't allow), or a copy of it where another primitive
/// took it already. All a file's Draco meshes decode to at most what
/// meshopt's views may unpack to, and each to no more points and triangles
/// than a model may have.
pub fn decode(json: &mut gltf::json::Root, buffers: &mut Vec<gltf::buffer::Data>) -> Result<(), String> {
    let mut total = 0usize;
    let mut taken = std::collections::HashSet::new();
    for m in 0..json.meshes.len() {
        for k in 0..json.meshes[m].primitives.len() {
            let Some(ext) = json.meshes[m].primitives[k].extensions.as_ref().and_then(|e| e.others.get(NAME)).cloned() else { continue };
            let bad = || format!("mesh {m}'s Draco compression is malformed");
            let view = ext.get("bufferView").and_then(|v| v.as_u64()).and_then(|v| usize::try_from(v).ok()).ok_or_else(bad)?;
            let ids = ext.get("attributes").and_then(|a| a.as_object()).ok_or_else(bad)?;
            let v = json.buffer_views.get(view).ok_or_else(bad)?;
            let (offset, length) = (v.byte_offset.map_or(0, |o| o.0 as usize), v.byte_length.0 as usize);
            let bytes = buffers.get(v.buffer.value()).and_then(|b| b.0.get(offset..offset.checked_add(length)?)).ok_or_else(bad)?;
            let limits = DecodeLimits::default()
                .with_max_points(super::MAX_TRIANGLES as u64 * 3)
                .with_max_faces(super::MAX_TRIANGLES as u64)
                .with_max_decoded_bytes((super::buffers::MAX_UNPACKED - total) as u64);
            let mut mesh = draco_core::Mesh::new();
            MeshDecoder::new().decode(&mut DecoderBuffer::new(bytes).with_limits(limits), &mut mesh).map_err(|e| format!("mesh {m}'s Draco data: {e}"))?;
            let points = mesh.num_points();
            let buffer = json.buffers.len();
            let mut out = Vec::new();
            // Each attribute the primitive compresses, in its accessor's
            // component type, behind a view and a new accessor.
            let named: Vec<(String, usize)> = ids.iter().filter_map(|(name, id)| Some((name.clone(), usize::try_from(id.as_u64()?).ok()?))).collect();
            for (name, id) in named {
                let Some(slot) = json.meshes[m].primitives[k].attributes.iter().find(|(s, _)| s.to_string() == name).map(|(s, a)| (s.clone(), a.value())) else { continue };
                let attribute = (0..mesh.num_attributes()).filter_map(|i| mesh.try_attribute(i).ok()).find(|a| a.unique_id() as usize == id);
                let attribute = attribute.ok_or_else(|| format!("mesh {m}'s Draco data has no attribute {id} for its {name}"))?;
                let mut accessor = json.accessors.get(slot.1).ok_or_else(bad)?.clone();
                let (Checked::Valid(GenericComponentType(kind)), Checked::Valid(shape)) = (accessor.component_type, accessor.type_) else { return Err(bad()) };
                let values = attribute.read_f32s(points, shape.multiplicity());
                let from = normal_of(attribute.data_type(), attribute.normalized());
                let start = out.len();
                for &value in &values {
                    put(&mut out, value * from, kind, accessor.normalized);
                }
                total += out.len() - start;
                let view = add_view(json, buffer, start, out.len() - start)?;
                (accessor.buffer_view, accessor.byte_offset, accessor.count, accessor.sparse) = (Some(gltf::json::Index::new(view)), None, USize64(points as u64), None);
                let index = place(json, &mut taken, slot.1, accessor);
                json.meshes[m].primitives[k].attributes.insert(slot.0, index);
                align(&mut out);
            }
            // The triangles' corners, as 32-bit indices: Draco may split a
            // point where an attribute's seam runs through it, so there may
            // be more points than the file's indices could name.
            let start = out.len();
            for f in 0..mesh.num_faces() {
                for p in mesh.face(FaceIndex(f as u32)) {
                    out.extend(p.0.to_le_bytes());
                }
            }
            total += out.len() - start;
            if total > super::buffers::MAX_UNPACKED {
                return Err(format!("its Draco meshes decode to more than {} MB", super::buffers::MAX_UNPACKED >> 20));
            }
            let view = add_view(json, buffer, start, out.len() - start)?;
            let indices = serde_json::json!({"bufferView": view, "componentType": 5125, "count": mesh.num_faces() * 3, "type": "SCALAR"});
            let indices = serde_json::from_value(indices).map_err(|e| e.to_string())?;
            let index = match json.meshes[m].primitives[k].indices {
                Some(old) if old.value() < json.accessors.len() => place(json, &mut taken, old.value(), indices),
                _ => {
                    json.accessors.push(indices);
                    gltf::json::Index::new(json.accessors.len() as u32 - 1)
                }
            };
            json.meshes[m].primitives[k].indices = Some(index);
            align(&mut out);
            json.buffers.push(serde_json::from_value(serde_json::json!({"byteLength": out.len()})).map_err(|e| e.to_string())?);
            buffers.push(gltf::buffer::Data(out));
        }
    }
    Ok(())
}

/// `accessor` in accessor `old`'s place, or, where another primitive's
/// decoded values took that place already, after the rest; its index.
fn place(json: &mut gltf::json::Root, taken: &mut std::collections::HashSet<usize>, old: usize, accessor: gltf::json::Accessor) -> gltf::json::Index<gltf::json::Accessor> {
    if taken.insert(old) {
        json.accessors[old] = accessor;
        return gltf::json::Index::new(old as u32);
    }
    json.accessors.push(accessor);
    gltf::json::Index::new(json.accessors.len() as u32 - 1)
}

/// A view of `length` bytes from `offset` in buffer `buffer`; its index.
fn add_view(json: &mut gltf::json::Root, buffer: usize, offset: usize, length: usize) -> Result<u32, String> {
    let view = serde_json::json!({"buffer": buffer, "byteOffset": offset, "byteLength": length});
    json.buffer_views.push(serde_json::from_value(view).map_err(|e| e.to_string())?);
    Ok(json.buffer_views.len() as u32 - 1)
}

/// Pads `out` to a whole number of words, where the next view may start.
fn align(out: &mut Vec<u8>) {
    out.resize(out.len().next_multiple_of(4), 0);
}

/// What a Draco attribute's values are scaled by to be 0 to 1 (or -1 to
/// 1), where it keeps them as normalized integers: 1 otherwise.
fn normal_of(kind: DataType, normalized: bool) -> f32 {
    let most = match kind {
        DataType::Int8 => 127.0,
        DataType::Uint8 => 255.0,
        DataType::Int16 => 32767.0,
        DataType::Uint16 => 65535.0,
        DataType::Int32 => i32::MAX as f32,
        DataType::Uint32 => u32::MAX as f32,
        _ => return 1.0,
    };
    if normalized {
        1.0 / most
    } else {
        1.0
    }
}

/// One value written as `kind`, little-endian: a normalized integer from
/// what it stands for (0 to 1, or -1 to 1), any other rounded.
fn put(out: &mut Vec<u8>, value: f32, kind: ComponentType, normalized: bool) {
    let int = |most: f32, least: f32| (if normalized { value * most } else { value }).round().clamp(least, most);
    match kind {
        ComponentType::I8 => out.push(int(127.0, -128.0) as i8 as u8),
        ComponentType::U8 => out.push(int(255.0, 0.0) as u8),
        ComponentType::I16 => out.extend((int(32767.0, -32768.0) as i16).to_le_bytes()),
        ComponentType::U16 => out.extend((int(65535.0, 0.0) as u16).to_le_bytes()),
        ComponentType::U32 => out.extend((int(u32::MAX as f32, 0.0) as u32).to_le_bytes()),
        ComponentType::F32 => out.extend(value.to_le_bytes()),
    }
}
