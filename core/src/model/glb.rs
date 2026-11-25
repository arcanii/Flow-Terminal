//! Binary glTF: every mesh in its scene, placed by its nodes, a node's
//! instances (EXT_mesh_gpu_instancing) each a copy of its mesh, with its
//! vertex colors (`COLOR_0`) and its materials (base color, metal and
//! roughness, normal map, emission, occlusion, each a factor and maybe a
//! texture; how its alpha is taken; the extensions
//! KHR_materials_emissive_strength, _unlit, _specular, _ior, _clearcoat,
//! _sheen, _transmission and _volume; and the old
//! KHR_materials_pbrSpecularGlossiness, turned into metal and roughness),
//! its textures read through either set of texture coordinates and moved
//! by KHR_texture_transform, its lights (KHR_lights_punctual) and camera,
//! and, where it has animations, what they move (`super::anim`): its nodes,
//! skins and morph targets, and the lights, camera and instances on them.
//! A model with no animation is posed once, at rest, and keeps none of
//! that.
//!
//! Its bytes may be packed small (`super::buffers`): views compressed with
//! meshopt (EXT_meshopt_compression, KHR_meshopt_compression), meshes
//! compressed with Draco (KHR_draco_mesh_compression, `super::draco`),
//! attributes quantized to bytes and shorts (KHR_mesh_quantization, which
//! the file's nodes usually scale back, and which instances may be kept
//! in too), and WebP and Basis textures (EXT_texture_webp,
//! KHR_texture_basisu).
//!
//! A file may require the extensions it uses (`extensionsRequired`), and
//! the gltf crate refuses any it doesn't read itself, which leaves out some
//! Flow reads on its own (clearcoat, sheen, specular, volume, instancing,
//! and the packing above). So Flow checks that list itself
//! ([`REQUIRABLE`]) and lets the crate check the rest of the file: one
//! requiring what Flow doesn't do, such as anisotropy, is refused by its
//! name.

use std::sync::Arc;

use gltf::animation::util::ReadOutputs;
use gltf::animation::{Interpolation, Property};

use super::anim::{compose, facing, Channel, Clip, Interp, Mat4, Morph, Node, Path, Rest, Rig, Skin, Track};
use super::buffers::{self, floats, Images};
use super::draco;
use super::{
    cross, det, dot, normalize, smooth_normals, sub, transform, Alpha, Clearcoat, Format, Lens, Light, Material, Mesh, Part, Sheen, Specular, Tex, Texture, Transmission, UvMap,
    Vertex, IDENTITY, TEXTURES,
};

/// The extensions Flow reads, which a file may require. Meshopt's goes by
/// Khronos's name too, which adds the vertex codec's version 1 and the
/// color filter.
pub const REQUIRABLE: [&str; 18] = [
    "KHR_texture_transform",
    "KHR_draco_mesh_compression",
    "KHR_materials_pbrSpecularGlossiness",
    "KHR_materials_emissive_strength",
    "KHR_materials_unlit",
    "KHR_materials_specular",
    "KHR_materials_ior",
    "KHR_materials_clearcoat",
    "KHR_materials_sheen",
    "KHR_materials_transmission",
    "KHR_materials_volume",
    "KHR_lights_punctual",
    "KHR_mesh_quantization",
    "EXT_meshopt_compression",
    "KHR_meshopt_compression",
    "EXT_texture_webp",
    "KHR_texture_basisu",
    "EXT_mesh_gpu_instancing",
];

/// The vertices a file's instances may make, all told: as many as the
/// corners of the most triangles a model may have, each its own. Their
/// triangles count toward those (`super::MAX_TRIANGLES`), and are counted
/// before they're made, so a few bytes of instances can't ask for more
/// than a model may hold.
const MAX_INSTANCED: usize = 3 * super::MAX_TRIANGLES;

/// The file's document and buffers, as `gltf::import_slice` reads them,
/// but with its required extensions checked against what Flow reads
/// rather than what the crate does, and its buffers as Flow reads them
/// (`buffers::read`: meshopt's views unpacked; `draco::decode`: Draco's
/// meshes decoded). The crate still checks everything else (its other
/// checks don't look at that list), once what Draco decodes is in, so a
/// file is as safe to read as before. Its images are decoded as textures
/// need them ([`Images`]).
fn import(bytes: &[u8]) -> Result<(gltf::Document, Vec<gltf::buffer::Data>), String> {
    let gltf::Gltf { document, blob } = gltf::Gltf::from_slice_without_validation(bytes).map_err(|e| e.to_string())?;
    let mut json = document.into_json();
    let mut unread: Vec<&str> = json.extensions_required.iter().map(String::as_str).filter(|e| !REQUIRABLE.contains(e)).collect();
    unread.dedup();
    if let Some((last, rest)) = unread.split_last() {
        let names = if rest.is_empty() { last.to_string() } else { format!("{} and {last}", rest.join(", ")) };
        return Err(format!("it requires {names}, which Flow doesn't read"));
    }
    json.extensions_required.clear();
    // A texture whose only picture is a KTX2 or a WebP has it as its
    // source, which the crate wants every texture to have.
    for t in &mut json.textures {
        let source = |name: &str| t.extensions.as_ref().and_then(|e| e.others.get(name)?.get("source")?.as_u64()).and_then(|i| u32::try_from(i).ok());
        if let (true, Some(i)) = (t.source.value() == u32::MAX as usize, source("KHR_texture_basisu").or(source("EXT_texture_webp"))) {
            t.source = gltf::json::Index::new(i);
        }
    }
    let mut buffers = buffers::read(&json, blob)?;
    // Draco's meshes decoded into buffers of their own, which new
    // accessors read, before the crate checks the document.
    if draco::used(&json) {
        draco::decode(&mut json, &mut buffers)?;
    }
    let doc = gltf::Document::from_json(json).map_err(|e| e.to_string())?;
    Ok((doc, buffers))
}

pub(super) fn load(bytes: &[u8]) -> Result<Mesh, String> {
    let (doc, buffers) = import(bytes)?;
    let data = |b: gltf::Buffer| buffers.get(b.index()).map(|d| &d.0[..]);
    let gltf_nodes: Vec<gltf::Node> = doc.nodes().collect();
    let mut nodes: Vec<Node> = gltf_nodes
        .iter()
        .map(|n| {
            let (translation, rotation, scale) = n.transform().decomposed();
            let matrix = match n.transform() {
                gltf::scene::Transform::Matrix { matrix } => Some(matrix),
                gltf::scene::Transform::Decomposed { .. } => None,
            };
            let weights = n.weights().or_else(|| n.mesh().and_then(|m| m.weights())).map(<[f32]>::to_vec).unwrap_or_default();
            Node { parent: None, translation, rotation, scale, matrix, weights }
        })
        .collect();
    for n in &gltf_nodes {
        for c in n.children() {
            if let Some(child) = nodes.get_mut(c.index()) {
                child.parent = Some(n.index());
            }
        }
    }
    let skins = doc
        .skins()
        .map(|s| {
            let joints: Vec<usize> = s.joints().map(|j| j.index()).collect();
            let mut inverse_bind: Vec<Mat4> = s.reader(data).read_inverse_bind_matrices().map(|m| m.collect()).unwrap_or_default();
            inverse_bind.resize(joints.len(), IDENTITY);
            Skin { joints, inverse_bind }
        })
        .collect();
    // Every animation, moving or not, so a program's `clip` counts them as
    // the file does.
    let clips = doc.animations().map(|a| read_clip(&a, data)).collect();
    let mut rig = Rig::new(nodes, skins, clips);

    let roots: Vec<usize> = match doc.default_scene().or_else(|| doc.scenes().next()) {
        Some(scene) => scene.nodes().map(|n| n.index()).collect(),
        None => (0..rig.nodes.len()).filter(|&i| rig.nodes[i].parent.is_none()).collect(),
    };
    let in_scene = reachable(&gltf_nodes, &roots);
    // The file's camera (the first node of the scene that holds one) is
    // the view the model is first seen from: the scene is turned to face
    // Flow's view as it faces the camera. Where an animation moves it, the
    // view moves with it, from each pose. A perspective camera is also
    // looked through ([`Lens`]): the model is drawn as it sees it, from
    // where it stands, rather than fitted to its cells straight on.
    let mut perspective = None;
    if let Some(c) = (0..gltf_nodes.len()).find(|&i| in_scene[i] && gltf_nodes[i].camera().is_some()) {
        if (0..rig.clips.len()).any(|k| rig.moved(k)[c]) {
            rig.camera = Some(c);
        } else {
            let (world, _) = rig.pose(None, 0.0);
            rig.view = Some(facing(world[c]));
        }
        if let Some(gltf::camera::Projection::Perspective(p)) = gltf_nodes[c].camera().as_ref().map(gltf::Camera::projection) {
            perspective = Some((c, p.yfov(), p.znear()));
        }
    }
    let (world, _) = rig.pose(None, 0.0);
    // Where it stands in the view, as the scene is posed at rest: a moving
    // camera's view takes its place away, so it's always at the middle.
    let lens = perspective.map(|(c, yfov, znear)| Lens { eye: [world[c][3][0], world[c][3][1], world[c][3][2]], yfov, znear });
    let lights: Vec<Light> = (0..gltf_nodes.len()).filter(|&i| in_scene[i]).filter_map(|i| Some(read_light(&gltf_nodes[i].light()?, i, world[i]))).collect();
    let mut images = Images::new(&doc, &buffers);
    let (mut parts, mut rests) = (Vec::new(), Vec::new());
    // The triangles read so far, and the vertices instances have made.
    let (mut triangles, mut instanced) = (0usize, 0usize);
    let mut seen = vec![false; gltf_nodes.len()];
    let mut stack = roots;
    while let Some(i) = stack.pop() {
        if seen.get(i).is_none_or(|&s| s) {
            continue;
        }
        seen[i] = true;
        let node = &gltf_nodes[i];
        stack.extend(node.children().map(|c| c.index()));
        let Some(mesh) = node.mesh() else { continue };
        let skin = node.skin().map(|s| s.index());
        let places = instances(node, &doc, &buffers)?;
        for prim in mesh.primitives().filter(|p| p.mode() == gltf::mesh::Mode::Triangles) {
            let reader = prim.reader(data);
            // Positions, normals, tangents and uvs as floats, whatever the
            // file keeps them as; a quantized mesh's node scales its
            // numbers back, as it places any mesh.
            let attribute = |semantic: gltf::Semantic| prim.get(&semantic);
            let Some(positions) = attribute(gltf::Semantic::Positions).and_then(|a| floats::<3>(&a, &buffers)) else { continue };
            let count = positions.len();
            let fits = |n: usize| n == count;
            let normals: Option<Vec<[f32; 3]>> =
                attribute(gltf::Semantic::Normals).and_then(|a| floats::<3>(&a, &buffers)).filter(|n| fits(n.len())).map(|n| n.into_iter().map(unit).collect());
            let uvs: Option<Vec<[f32; 2]>> = attribute(gltf::Semantic::TexCoords(0)).and_then(|a| floats::<2>(&a, &buffers)).filter(|t| fits(t.len()));
            let uvs1: Option<Vec<[f32; 2]>> = attribute(gltf::Semantic::TexCoords(1)).and_then(|a| floats::<2>(&a, &buffers)).filter(|t| fits(t.len()));
            let mut indices: Vec<u32> = match reader.read_indices() {
                Some(i) => i.into_u32().collect(),
                None => (0..count as u32).collect(),
            };
            if indices.iter().any(|&i| i as usize >= count) {
                return Err("a triangle naming a vertex that isn't there".into());
            }
            indices.truncate(indices.len() / 3 * 3);
            if indices.is_empty() {
                continue;
            }
            // Its copies, one for each of its node's instances, counted
            // before they're made.
            let copies = places.as_ref().map_or(1, Vec::len);
            triangles = triangles.saturating_add((indices.len() / 3).saturating_mul(copies));
            if places.is_some() {
                instanced = instanced.saturating_add(count.saturating_mul(copies));
                if triangles > super::MAX_TRIANGLES {
                    return Err(format!("{triangles} triangles with its instances, more than {}", super::MAX_TRIANGLES));
                }
                if instanced > MAX_INSTANCED {
                    return Err(format!("its instances make {instanced} vertices, more than {MAX_INSTANCED}"));
                }
            }
            if copies == 0 {
                continue;
            }
            // Vertex colors (COLOR_0), RGB or RGBA (alpha 1), floats or
            // normalized bytes or shorts, linear as the material's factors
            // are, so encoded as they are.
            let colors: Option<Vec<[u8; 4]>> = attribute(gltf::Semantic::Colors(0))
                .and_then(|a| match a.dimensions() {
                    gltf::accessor::Dimensions::Vec3 => floats::<3>(&a, &buffers).map(|c| c.into_iter().map(|[r, g, b]| [r, g, b, 1.0]).collect()),
                    _ => floats::<4>(&a, &buffers),
                })
                .filter(|c| fits(c.len()))
                .map(|c| c.into_iter().map(vertex_color).collect());
            let mut material = read_material(&doc, &prim.material(), &mut images);
            let mut vertices: Vec<Vertex> = (0..count)
                .map(|k| {
                    let v = Vertex::new(positions[k], normals.as_ref().map_or([0.0; 3], |n| n[k]), uvs.as_ref().map_or([0.0; 2], |t| t[k]));
                    Vertex { uv1: uvs1.as_ref().map_or(v.uv, |t| t[k]), color: colors.as_ref().map_or(v.color, |c| c[k]), ..v }
                })
                .collect();
            if normals.is_none() {
                smooth_normals(&mut vertices, &indices, |_| true);
            }
            // A normal map needs tangents, worked out where the file gives
            // none from the uvs it reads, as its transform moves them;
            // without uvs there's nothing to map. The clearcoat's normal map
            // shares them, worked out from its own uvs where the base has
            // no normal map.
            let coat_bumps = material.clearcoat.as_ref().is_some_and(|c| c.bumps.is_some());
            let bump_map = material.maps[if material.bumps.is_some() { Tex::Normal } else { Tex::CoatNormal } as usize];
            let has_uvs = if bump_map.set == 1 { uvs1.is_some() || uvs.is_some() } else { uvs.is_some() };
            let tangents = match material.bumps.is_some() || coat_bumps {
                true if has_uvs => match attribute(gltf::Semantic::Tangents).and_then(|a| floats::<4>(&a, &buffers)).filter(|t| fits(t.len())) {
                    Some(t) => t,
                    None => {
                        let mapped: Vec<[f32; 2]> = vertices.iter().map(|v| bump_map.apply(v)).collect();
                        tangents(&vertices.iter().map(|v| v.pos).collect::<Vec<_>>(), &vertices.iter().map(|v| v.normal).collect::<Vec<_>>(), &mapped, &indices)
                    }
                },
                _ => {
                    material.bumps = None;
                    if let Some(c) = material.clearcoat.as_mut() {
                        c.bumps = None;
                    }
                    Vec::new()
                }
            };
            for (v, t) in vertices.iter_mut().zip(&tangents) {
                v.tangent = *t;
            }
            let (joints, weights) = match skin {
                Some(_) => (
                    reader.read_joints(0).map(|j| j.into_u16().collect::<Vec<_>>()).unwrap_or_default(),
                    reader.read_weights(0).map(|w| w.into_f32().collect::<Vec<_>>()).unwrap_or_default(),
                ),
                None => (Vec::new(), Vec::new()),
            };
            let skin = skin.filter(|_| fits(joints.len()) && fits(weights.len()));
            let target = |a: Option<gltf::Accessor>| a.and_then(|a| floats::<3>(&a, &buffers)).filter(|v| fits(v.len())).unwrap_or_default();
            let morphs: Vec<Morph> = prim.morph_targets().map(|m| Morph { positions: target(m.positions()), normals: target(m.normals()), tangents: target(m.tangents()) }).collect();
            // Mirrored by its node, or by an instance: the triangles turn the
            // other way round. A skinned part goes by its joints, which glTF
            // keeps unmirrored; an instance, before them, mirrors it still.
            let normals = vertices.iter().map(|v| v.normal).collect();
            let mirrored = skin.is_none() && det(world[i]) < 0.0;
            let wound = |flip: bool, base: u32| indices.as_chunks::<3>().0.iter().flat_map(move |t| if flip { [t[0], t[2], t[1]] } else { *t }).map(move |v| v + base);
            let (vertices, indices) = match &places {
                // A copy of the vertices for each instance, one after another,
                // each copy's triangles numbering its own.
                Some(places) => {
                    let mut copied = Vec::with_capacity(indices.len() * copies);
                    for (k, place) in places.iter().enumerate() {
                        copied.extend(wound(mirrored != (det(*place) < 0.0), (k * count) as u32));
                    }
                    (vertices.repeat(copies), copied)
                }
                None => (vertices, if mirrored { wound(true, 0).collect() } else { indices }),
            };
            let instances = places.clone().unwrap_or_default();
            rests.push(Rest { node: i, skin, positions, normals, tangents, joints, weights, morphs, instances });
            parts.push(Part { vertices, indices, material, node: Some(i) });
        }
    }
    // Posed at rest, in the file's units: the mesh centers and scales them.
    rig.parts = rests;
    let posed = rig.pose_parts(None, 0.0, &parts);
    for (part, vertices) in parts.iter_mut().zip(posed) {
        part.vertices = vertices;
    }
    let moving = rig.clips.iter().any(Clip::moves);
    Mesh::with_rig(parts, Format::Glb, moving.then_some(rig), lights, lens)
}

/// A vertex's color (COLOR_0, linear RGBA from 0 to 1) as a vertex holds
/// it: display values, as the material's colors are shown, in bytes as a
/// texture's texels are.
fn vertex_color(c: [f32; 4]) -> [u8; 4] {
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    [byte(c[0].max(0.0).powf(1.0 / 2.2)), byte(c[1].max(0.0).powf(1.0 / 2.2)), byte(c[2].max(0.0).powf(1.0 / 2.2)), byte(c[3])]
}

/// Where a node's instances (EXT_mesh_gpu_instancing) put copies of its
/// mesh, each in the mesh's own space, before the node places it: each
/// instance's TRANSLATION, ROTATION (a quaternion, made unit length, as a
/// quantized one is only near it) and SCALE, any of them left out (where
/// it is, unturned, its own size), read as floats however they're kept
/// (normalized bytes and shorts too, as KHR_mesh_quantization allows).
/// None for a node without them; refused where an attribute names no
/// accessor Flow can read as its kind, they don't all number alike, or
/// there are more of them than a model may have triangles (each instance
/// draws one at least), before any is read.
fn instances(node: &gltf::Node, doc: &gltf::Document, buffers: &[gltf::buffer::Data]) -> Result<Option<Vec<Mat4>>, String> {
    let Some(attributes) = node.extension_value("EXT_mesh_gpu_instancing").and_then(|e| e.get("attributes")?.as_object()) else { return Ok(None) };
    let n = node.index();
    let accessor = |name: &str, value: &gltf::json::Value| {
        let found = value.as_u64().and_then(|k| usize::try_from(k).ok()).and_then(|k| doc.accessors().nth(k));
        found.ok_or_else(|| format!("node {n}'s instances' {name} names no accessor"))
    };
    // Every attribute counts the instances, the program's own (`_ID`, say)
    // too, though only these three place them.
    let mut count = None;
    for (name, value) in attributes {
        let c = accessor(name, value)?.count();
        if count.is_some_and(|k| k != c) {
            return Err(format!("node {n}'s instances' attributes don't number alike"));
        }
        count = Some(c);
    }
    let Some(count) = count else { return Ok(None) };
    if count > super::MAX_TRIANGLES {
        return Err(format!("node {n} has {count} instances, more than {}", super::MAX_TRIANGLES));
    }
    let unread = |name: &str, kind: &str| format!("node {n}'s instances' {name} isn't a {kind} of numbers");
    let three = |name: &str| match attributes.get(name) {
        Some(v) => floats::<3>(&accessor(name, v)?, buffers).map(Some).ok_or_else(|| unread(name, "VEC3")),
        None => Ok(None),
    };
    let (translations, scales) = (three("TRANSLATION")?, three("SCALE")?);
    let rotations = match attributes.get("ROTATION") {
        Some(v) => Some(floats::<4>(&accessor("ROTATION", v)?, buffers).ok_or_else(|| unread("ROTATION", "VEC4"))?),
        None => None,
    };
    let unit = |q: [f32; 4]| {
        let l = q.iter().map(|v| v * v).sum::<f32>().sqrt();
        if l > 1e-12 && l.is_finite() {
            q.map(|v| v / l)
        } else {
            [0.0, 0.0, 0.0, 1.0]
        }
    };
    Ok(Some(
        (0..count)
            .map(|k| {
                let at = |list: &Option<Vec<[f32; 3]>>, default: [f32; 3]| list.as_ref().and_then(|l| l.get(k)).copied().unwrap_or(default);
                let turn = rotations.as_ref().and_then(|r| r.get(k)).copied().map_or([0.0, 0.0, 0.0, 1.0], unit);
                compose(at(&translations, [0.0; 3]), turn, at(&scales, [1.0; 3]))
            })
            .collect(),
    ))
}

/// A normal read from the file, made unit length: a quantized one is only
/// near it, and the tangents worked out from it want it.
fn unit(n: [f32; 3]) -> [f32; 3] {
    if dot(n, n) > 0.0 {
        normalize(n)
    } else {
        n
    }
}

/// Which nodes the scene holds: its `roots` and all below them.
fn reachable(nodes: &[gltf::Node], roots: &[usize]) -> Vec<bool> {
    let mut seen = vec![false; nodes.len()];
    let mut stack = roots.to_vec();
    while let Some(i) = stack.pop() {
        if seen.get(i).is_none_or(|&s| s) {
            continue;
        }
        seen[i] = true;
        stack.extend(nodes[i].children().map(|c| c.index()));
    }
    seen
}

/// A light on node `node`, where its matrix `m` puts it in the world: a
/// spot or directional light shines along the node's -z (as
/// `Rig::place_lights` places it again where an animation moves it). Its
/// color is shown as the materials' are; its intensity stays the file's
/// until the mesh makes it a share of its light (`super::settle_lights`).
fn read_light(light: &gltf::khr_lights_punctual::Light, node: usize, m: Mat4) -> Light {
    use gltf::khr_lights_punctual::Kind;
    let place = [m[3][0], m[3][1], m[3][2]];
    let (at, cone) = match light.kind() {
        Kind::Directional => (None, None),
        Kind::Point => (Some(place), None),
        Kind::Spot { inner_cone_angle, outer_cone_angle } => (Some(place), Some((inner_cone_angle.cos(), outer_cone_angle.cos()))),
    };
    Light {
        at,
        aim: normalize(transform(m, [0.0, 0.0, -1.0], 0.0)),
        color: light.color().map(|c| c.max(0.0).powf(1.0 / 2.2)),
        strength: light.intensity().max(0.0),
        range: light.range().filter(|r| *r > 0.0),
        cone,
        node: Some(node),
    }
}

/// An animation's channels, each a track of keys for what it moves; one
/// that doesn't fit what it moves is left out.
fn read_clip<'a, 's>(anim: &gltf::Animation<'a>, data: impl Clone + Fn(gltf::Buffer<'a>) -> Option<&'s [u8]>) -> Clip {
    let channels = anim
        .channels()
        .filter_map(|ch| {
            let reader = ch.reader(data.clone());
            let times: Vec<f32> = reader.read_inputs()?.collect();
            let (path, values): (Path, Vec<f32>) = match reader.read_outputs()? {
                ReadOutputs::Translations(t) => (Path::Translation, t.flatten().collect()),
                ReadOutputs::Rotations(r) => (Path::Rotation, r.into_f32().flatten().collect()),
                ReadOutputs::Scales(s) => (Path::Scale, s.flatten().collect()),
                ReadOutputs::MorphTargetWeights(w) => (Path::Weights, w.into_f32().collect()),
            };
            let interp = match ch.sampler().interpolation() {
                Interpolation::Step => Interp::Step,
                Interpolation::Linear => Interp::Linear,
                Interpolation::CubicSpline => Interp::Cubic,
            };
            let track = Track::new(times, values, interp)?;
            let width = match ch.target().property() {
                Property::Translation | Property::Scale => 3,
                Property::Rotation => 4,
                Property::MorphTargetWeights => track.width,
            };
            (track.width == width).then_some(Channel { node: ch.target().node().index(), path, track })
        })
        .collect();
    Clip::new(channels)
}

/// A primitive's material: glTF's factors are linear light, and what's
/// drawn is display values, so its colors are encoded as the textures'
/// are (their product stays right, the encoding being a power).
fn read_material(doc: &gltf::Document, m: &gltf::Material, images: &mut Images) -> Material {
    let display = |c: f32| c.max(0.0).powf(1.0 / 2.2);
    let pbr = m.pbr_metallic_roughness();
    // KHR_materials_pbrSpecularGlossiness, the older way of saying how a
    // surface reflects, in place of metal and roughness: read as
    // glTF-Transform's `metalRough()` turns it into them. Its diffuse is
    // the base color, it's no metal, it reflects its specular color head on
    // (KHR_materials_specular at full strength), and its glossiness is
    // Flow's gloss; its texture's color is the specular color's map, and
    // its alpha (the glossiness) is baked into a roughness map.
    // glTF-Transform has the color reflected so by an ior of 1000; Flow's
    // glaze, at its own 1.5 (reflecting 0.04 head on), takes a specular
    // color as how much more than that it reflects, in display values as
    // the textures' are, so 0.04's is the glaze Flow always draws.
    let old = m.pbr_specular_glossiness();
    let [r, g, b, a] = old.as_ref().map_or(pbr.base_color_factor(), |o| o.diffuse_factor());
    let sg = old.as_ref().and_then(|o| o.specular_glossiness_texture());
    let base = match &old {
        Some(o) => o.diffuse_texture(),
        None => pbr.base_color_texture(),
    };
    let metal_rough = if old.is_some() { None } else { pbr.metallic_roughness_texture() };
    let baked = match (&old, &sg) {
        (Some(o), Some(t)) => images.roughness_from_gloss(&t.texture(), o.glossiness_factor()),
        _ => None,
    };
    let mut texture = |t: Option<gltf::Texture>| -> Option<Arc<Texture>> { images.texture(&t?) };
    let (normal, occlusion, emissive) = (m.normal_texture(), m.occlusion_texture(), m.emissive_texture());
    // The texture transform is typed on a plain texture's info, and left as
    // JSON on a normal or occlusion map's.
    let info = |t: &gltf::texture::Info| match t.texture_transform() {
        Some(x) => UvMap::new(x.tex_coord().unwrap_or(t.tex_coord()), x.offset(), x.rotation(), x.scale()),
        None => UvMap::new(t.tex_coord(), [0.0; 2], 0.0, [1.0; 2]),
    };
    let mut maps = [UvMap::PLAIN; TEXTURES];
    let mut map = |slot: Tex, uv: Option<UvMap>| maps[slot as usize] = uv.unwrap_or(UvMap::PLAIN);
    map(Tex::Base, base.as_ref().map(info));
    map(Tex::MetalRough, metal_rough.as_ref().or(sg.as_ref()).map(info));
    map(Tex::Normal, normal.as_ref().map(|n| moved(n.tex_coord(), n.extension_value("KHR_texture_transform"))));
    map(Tex::Emissive, emissive.as_ref().map(info));
    map(Tex::Occlusion, occlusion.as_ref().map(|o| moved(o.tex_coord(), o.extension_value("KHR_texture_transform"))));
    let strength = m.emissive_strength().unwrap_or(1.0).max(0.0);

    let specular = match &old {
        Some(o) => {
            map(Tex::SpecularColor, sg.as_ref().map(info));
            Some(Specular { strength: 1.0, strength_map: None, color: o.specular_factor().map(|c| display(c) / display(0.04)), color_map: texture(sg.as_ref().map(|t| t.texture())) })
        }
        None => m.specular().map(|s| {
            let (strength_map, color_map) = (s.specular_texture(), s.specular_color_texture());
            map(Tex::Specular, strength_map.as_ref().map(info));
            map(Tex::SpecularColor, color_map.as_ref().map(info));
            Specular {
                strength: s.specular_factor().clamp(0.0, 1.0),
                strength_map: texture(strength_map.map(|t| t.texture())),
                color: s.specular_color_factor().map(display),
                color_map: texture(color_map.map(|t| t.texture())),
            }
        }),
    };
    // Its glossiness: all in the baked map where it has a texture (the
    // factor's roughness 1), else the factor.
    let (gloss, metal_rough) = match (&old, &sg) {
        (Some(_), Some(_)) => (0.0, baked),
        (Some(o), None) => (o.glossiness_factor().clamp(0.0, 1.0), None),
        _ => ((1.0 - pbr.roughness_factor()).clamp(0.0, 1.0), texture(metal_rough.as_ref().map(|t| t.texture()))),
    };
    // Clearcoat and sheen are JSON to the gltf crate, their textures named
    // by index as a texture's info is.
    let named = |v: Option<&gltf::json::Value>| {
        let v = v?;
        let tex = doc.textures().nth(usize::try_from(v.get("index")?.as_u64()?).ok()?)?;
        let set = v.get("texCoord").and_then(|s| s.as_u64()).map_or(0, |s| s as u32);
        let scale = v.get("scale").and_then(|s| s.as_f64()).unwrap_or(1.0) as f32;
        Some((tex, moved(set, v.get("extensions").and_then(|e| e.get("KHR_texture_transform"))), scale))
    };
    let number = |v: &gltf::json::Value, key: &str, default: f32| v.get(key).and_then(|n| n.as_f64()).map_or(default, |n| n as f32);
    let clearcoat = m.extension_value("KHR_materials_clearcoat").and_then(|c| {
        let strength = number(c, "clearcoatFactor", 0.0).clamp(0.0, 1.0);
        let (strength_map, roughness_map, bumps) = (named(c.get("clearcoatTexture")), named(c.get("clearcoatRoughnessTexture")), named(c.get("clearcoatNormalTexture")));
        map(Tex::Coat, strength_map.as_ref().map(|t| t.1));
        map(Tex::CoatRoughness, roughness_map.as_ref().map(|t| t.1));
        map(Tex::CoatNormal, bumps.as_ref().map(|t| t.1));
        (strength > 0.0).then(|| Clearcoat {
            strength,
            strength_map: texture(strength_map.map(|t| t.0)),
            gloss: 1.0 - number(c, "clearcoatRoughnessFactor", 0.0).clamp(0.0, 1.0),
            roughness_map: texture(roughness_map.map(|t| t.0)),
            bumps: bumps.and_then(|(t, _, scale)| Some((texture(Some(t))?, scale))),
        })
    });
    let sheen = m.extension_value("KHR_materials_sheen").and_then(|s| {
        let color = s.get("sheenColorFactor").map_or([0.0; 3], |c| [0, 1, 2].map(|k| c.get(k).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32));
        let (color_map, roughness_map) = (named(s.get("sheenColorTexture")), named(s.get("sheenRoughnessTexture")));
        map(Tex::SheenColor, color_map.as_ref().map(|t| t.1));
        map(Tex::SheenRoughness, roughness_map.as_ref().map(|t| t.1));
        // Black, glTF's default, is no sheen at all.
        (color.iter().any(|&c| c > 0.0)).then(|| Sheen {
            color: color.map(display),
            color_map: texture(color_map.map(|t| t.0)),
            roughness: number(s, "sheenRoughnessFactor", 0.0).clamp(0.0, 1.0),
            roughness_map: texture(roughness_map.map(|t| t.0)),
        })
    });
    // A volume means something only to light passing through.
    let transmission = m.transmission().filter(|t| t.transmission_factor() > 0.0).map(|t| {
        let (amount_map, volume) = (t.transmission_texture(), m.volume());
        let thickness_map = volume.as_ref().and_then(|v| v.thickness_texture());
        map(Tex::Transmission, amount_map.as_ref().map(info));
        map(Tex::Thickness, thickness_map.as_ref().map(info));
        let distance = volume.as_ref().map_or(f32::INFINITY, |v| v.attenuation_distance());
        Transmission {
            amount: t.transmission_factor().clamp(0.0, 1.0),
            map: texture(amount_map.map(|t| t.texture())),
            thickness: volume.as_ref().map_or(0.0, |v| v.thickness_factor().max(0.0)),
            thickness_map: texture(thickness_map.map(|t| t.texture())),
            attenuation: volume.as_ref().map_or([1.0; 3], |v| v.attenuation_color().map(|c| c.clamp(0.0, 1.0))),
            distance: if distance > 0.0 { distance } else { f32::INFINITY },
        }
    });
    Material {
        color: [display(r), display(g), display(b), a],
        texture: texture(base.as_ref().map(|t| t.texture())),
        // A smooth material shines; glTF's own default is rough.
        gloss,
        // And metallic, which a rough metal hardly shows.
        metallic: if old.is_some() { 0.0 } else { pbr.metallic_factor().clamp(0.0, 1.0) },
        metal_rough,
        bumps: normal.as_ref().and_then(|n| Some((texture(Some(n.texture()))?, n.scale()))),
        emission: m.emissive_factor().map(|c| display(c * strength)),
        glow: texture(emissive.as_ref().map(|t| t.texture())),
        occlusion: occlusion.as_ref().and_then(|o| Some((texture(Some(o.texture()))?, o.strength().clamp(0.0, 1.0)))),
        alpha: match m.alpha_mode() {
            gltf::material::AlphaMode::Opaque => Alpha::Opaque,
            gltf::material::AlphaMode::Mask => Alpha::Mask(m.alpha_cutoff().unwrap_or(0.5)),
            gltf::material::AlphaMode::Blend => Alpha::Blend,
        },
        unlit: m.unlit(),
        // No real material is below 1; one that says so reads as glTF's
        // default, as a specular-glossiness one is read (above).
        ior: if old.is_some() { 1.5 } else { m.ior().filter(|&n| n >= 1.0).unwrap_or(1.5) },
        specular,
        clearcoat,
        sheen,
        transmission,
        maps,
    }
}

/// A texture's uv map from its `texCoord` and its KHR_texture_transform as
/// JSON, where it has one: each part the spec's default where it's missing.
fn moved(tex_coord: u32, transform: Option<&gltf::json::Value>) -> UvMap {
    let Some(x) = transform else { return UvMap::new(tex_coord, [0.0; 2], 0.0, [1.0; 2]) };
    let pair = |key: &str, default: [f32; 2]| {
        x.get(key).and_then(|v| Some([v.get(0)?.as_f64()? as f32, v.get(1)?.as_f64()? as f32])).unwrap_or(default)
    };
    let rotation = x.get("rotation").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
    let set = x.get("texCoord").and_then(|v| v.as_u64()).map_or(tex_coord, |n| n as u32);
    UvMap::new(set, pair("offset", [0.0; 2]), rotation, pair("scale", [1.0; 2]))
}

/// Tangents for a normal map where the file gives none: along the
/// texture's u at each vertex, summed over its triangles and squared to
/// its normal, with the side glTF's bitangent (`cross(normal, tangent) *
/// w`) takes. glTF's v runs down the texture while a normal map's green
/// points up it, so the bitangent goes against v. Not MikkTSpace, which
/// maps baked for it may follow more closely along seams.
pub fn tangents(positions: &[[f32; 3]], normals: &[[f32; 3]], uvs: &[[f32; 2]], indices: &[u32]) -> Vec<[f32; 4]> {
    let mut along_u = vec![[0f32; 3]; positions.len()];
    let mut along_v = vec![[0f32; 3]; positions.len()];
    for t in indices.as_chunks::<3>().0 {
        let [a, b, c] = t.map(|i| i as usize);
        let (e1, e2) = (sub(positions[b], positions[a]), sub(positions[c], positions[a]));
        let (du1, dv1) = (uvs[b][0] - uvs[a][0], uvs[b][1] - uvs[a][1]);
        let (du2, dv2) = (uvs[c][0] - uvs[a][0], uvs[c][1] - uvs[a][1]);
        let r = du1 * dv2 - du2 * dv1;
        if r.abs() < 1e-12 {
            continue;
        }
        let u = [0, 1, 2].map(|k| (e1[k] * dv2 - e2[k] * dv1) / r);
        let v = [0, 1, 2].map(|k| (e2[k] * du1 - e1[k] * du2) / r);
        for i in [a, b, c] {
            along_u[i] = [0, 1, 2].map(|k| along_u[i][k] + u[k]);
            along_v[i] = [0, 1, 2].map(|k| along_v[i][k] + v[k]);
        }
    }
    (0..positions.len())
        .map(|i| {
            let n = normals[i];
            let u = along_u[i];
            let mut t = sub(u, n.map(|k| k * dot(n, u)));
            if dot(t, t) < 1e-20 {
                // No uv slope here: any way across the normal will do.
                t = cross(if n[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] }, n);
            }
            let t = normalize(t);
            let w = if dot(cross(n, t), along_v[i]) > 0.0 { -1.0 } else { 1.0 };
            [t[0], t[1], t[2], w]
        })
        .collect()
}

#[cfg(test)]
pub mod tests {
    use super::super::{load, Format};
    use super::*;

    /// A binary glTF put together in code: the buffer's bytes, and the
    /// views and accessors into them.
    #[derive(Default)]
    pub struct Glb {
        bin: Vec<u8>,
        views: Vec<String>,
        accessors: Vec<String>,
        /// How long meshopt's fallback buffer is, where views are packed.
        fallback: usize,
        /// Meshopt goes by Khronos's name, KHR_meshopt_compression.
        pub khr: bool,
    }

    impl Glb {
        pub fn view(&mut self, bytes: &[u8]) -> usize {
            self.bin.resize(self.bin.len().div_ceil(4) * 4, 0);
            self.views.push(format!(r#"{{"buffer":0,"byteOffset":{},"byteLength":{}}}"#, self.bin.len(), bytes.len()));
            self.bin.extend(bytes);
            self.views.len() - 1
        }

        /// Floats `n` a value (1, 2, 3, 4, or 16 for a matrix), with the
        /// bounds glTF wants for positions; returns the accessor.
        pub fn floats(&mut self, values: &[f32], n: usize) -> usize {
            let view = self.view(&values.iter().flat_map(|f| f.to_le_bytes()).collect::<Vec<_>>());
            let count = values.len() / n;
            if n == 16 {
                self.accessors.push(format!(r#"{{"bufferView":{view},"componentType":5126,"count":{count},"type":"MAT4"}}"#));
                return self.accessors.len() - 1;
            }
            let kind = ["SCALAR", "VEC2", "VEC3", "VEC4"][n - 1];
            let bound = |pick: fn(f32, f32) -> f32| (0..n).map(|k| values.iter().skip(k).step_by(n).copied().reduce(pick).unwrap_or(0.0).to_string()).collect::<Vec<_>>().join(",");
            self.accessors.push(format!(
                r#"{{"bufferView":{view},"componentType":5126,"count":{count},"type":"{kind}","min":[{}],"max":[{}]}}"#,
                bound(f32::min),
                bound(f32::max)
            ));
            self.accessors.len() - 1
        }

        /// Unsigned shorts `n` a value: indices (1) or joints (4).
        pub fn shorts(&mut self, values: &[u16], n: usize) -> usize {
            let view = self.view(&values.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<_>>());
            let kind = ["SCALAR", "VEC2", "VEC3", "VEC4"][n - 1];
            self.accessors.push(format!(r#"{{"bufferView":{view},"componentType":5123,"count":{},"type":"{kind}"}}"#, values.len() / n));
            self.accessors.len() - 1
        }

        /// An accessor of `count` elements of `kind` (SCALAR, VEC2…) in
        /// `view`, its components glTF's `component` type (5120 a byte,
        /// 5121 unsigned, 5122 a short, 5123 unsigned, 5126 a float),
        /// `rest` its other members (each with a comma before it).
        pub fn accessor(&mut self, view: usize, component: u32, normalized: bool, count: usize, kind: &str, rest: &str) -> usize {
            self.accessors.push(format!(r#"{{"bufferView":{view},"componentType":{component},"normalized":{normalized},"count":{count},"type":"{kind}"{rest}}}"#));
            self.accessors.len() - 1
        }

        /// An accessor of `count` elements of `kind` with no view, as a
        /// Draco-compressed primitive's are; returns it.
        pub fn accessor_without_view(&mut self, component: u32, normalized: bool, count: usize, kind: &str, rest: &str) -> usize {
            self.accessors.push(format!(r#"{{"componentType":{component},"normalized":{normalized},"count":{count},"type":"{kind}"{rest}}}"#));
            self.accessors.len() - 1
        }

        /// `raw` (elements `stride` bytes long) packed with meshopt as
        /// `mode`, through `filter` (already applied), its view's place in
        /// the fallback buffer and its packed bytes in the first; returns
        /// the view. Vertex data's view has its stride.
        pub fn packed(&mut self, raw: &[u8], stride: usize, mode: &str, filter: &str) -> usize {
            use super::super::meshopt::tests::{pack_sequence, pack_triangles, pack_vertices};
            let indices = || -> Vec<u32> { raw.chunks_exact(stride).map(|b| if stride == 2 { u32::from(u16::from_le_bytes([b[0], b[1]])) } else { u32::from_le_bytes([b[0], b[1], b[2], b[3]]) }).collect() };
            let bytes = match mode {
                "ATTRIBUTES" => pack_vertices(raw, stride),
                "TRIANGLES" => pack_triangles(&indices(), 1),
                _ => pack_sequence(&indices()),
            };
            self.packed_as(raw, stride, mode, filter, bytes)
        }

        /// Vertex data packed as `packed` packs it, with the vertex
        /// codec's version 1, `channels` taking each four bytes' deltas.
        pub fn packed_v1(&mut self, raw: &[u8], stride: usize, filter: &str, channels: &[u8]) -> usize {
            let bytes = super::super::meshopt::tests::pack_vertices_v1(raw, stride, channels);
            self.packed_as(raw, stride, "ATTRIBUTES", filter, bytes)
        }

        fn packed_as(&mut self, raw: &[u8], stride: usize, mode: &str, filter: &str, bytes: Vec<u8>) -> usize {
            let count = raw.len() / stride;
            let name = self.meshopt();
            self.bin.resize(self.bin.len().div_ceil(4) * 4, 0);
            let (at, place) = (self.bin.len(), self.fallback);
            self.bin.extend(&bytes);
            self.fallback = (place + raw.len()).div_ceil(4) * 4;
            let byte_stride = if mode == "ATTRIBUTES" { format!(r#","byteStride":{stride}"#) } else { String::new() };
            self.views.push(format!(
                r#"{{"buffer":1,"byteOffset":{place},"byteLength":{}{byte_stride},"extensions":{{"{name}":{{"buffer":0,"byteOffset":{at},"byteLength":{},"byteStride":{stride},"count":{count},"mode":"{mode}","filter":"{filter}"}}}}}}"#,
                raw.len(),
                bytes.len()
            ));
            self.views.len() - 1
        }

        /// The name meshopt's compression goes by in this file.
        fn meshopt(&self) -> &'static str {
            if self.khr {
                "KHR_meshopt_compression"
            } else {
                "EXT_meshopt_compression"
            }
        }

        /// A lossless WebP of RGBA pixels, for "images"; returns its view.
        pub fn webp(&mut self, w: u32, h: u32, rgba: &[u8]) -> usize {
            let mut out = Vec::new();
            image_webp::WebPEncoder::new(&mut out).encode(rgba, w, h, image_webp::ColorType::Rgba8).unwrap();
            self.view(&out)
        }

        /// A PNG of RGBA pixels, for "images"; returns its view.
        pub fn png(&mut self, w: u32, h: u32, rgba: &[u8]) -> usize {
            let mut out = Vec::new();
            let mut enc = png::Encoder::new(&mut out, w, h);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            enc.write_header().unwrap().write_image_data(rgba).unwrap();
            self.view(&out)
        }

        /// The file, `rest` being the JSON's other members.
        pub fn finish(mut self, rest: &str) -> Vec<u8> {
            self.bin.resize(self.bin.len().div_ceil(4) * 4, 0);
            // Meshopt's fallback buffer has no bytes in the file.
            let fallback = if self.fallback > 0 { format!(r#",{{"byteLength":{},"extensions":{{"{}":{{"fallback":true}}}}}}"#, self.fallback, self.meshopt()) } else { String::new() };
            let json = format!(
                r#"{{"asset":{{"version":"2.0"}},"buffers":[{{"byteLength":{}}}{fallback}],"bufferViews":[{}],"accessors":[{}],{rest}}}"#,
                self.bin.len(),
                self.views.join(","),
                self.accessors.join(",")
            );
            let mut json = json.into_bytes();
            json.resize(json.len().div_ceil(4) * 4, b' ');
            let mut out = b"glTF".to_vec();
            out.extend(2u32.to_le_bytes());
            out.extend(((12 + 8 + json.len() + 8 + self.bin.len()) as u32).to_le_bytes());
            out.extend((json.len() as u32).to_le_bytes());
            out.extend(b"JSON");
            out.extend(&json);
            out.extend((self.bin.len() as u32).to_le_bytes());
            out.extend(b"BIN\0");
            out.extend(&self.bin);
            out
        }
    }

    /// A square with uvs facing the viewer, its material metal and
    /// roughness, a normal map and emission, each with a texture.
    pub fn material_glb() -> Vec<u8> {
        let mut g = Glb::default();
        let pos = g.floats(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 0.0], 3);
        let uv = g.floats(&[0.0, 1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0], 2);
        let idx = g.shorts(&[0, 1, 2, 0, 2, 3], 1);
        let metal = g.png(1, 1, &[0, 128, 255, 255]);
        let bumps = g.png(1, 1, &[128, 200, 230, 255]);
        let glow = g.png(1, 1, &[255, 0, 0, 255]);
        g.finish(&format!(
            r#""scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0}}],
            "meshes":[{{"primitives":[{{"attributes":{{"POSITION":{pos},"TEXCOORD_0":{uv}}},"indices":{idx},"material":0}}]}}],
            "materials":[{{"pbrMetallicRoughness":{{"baseColorFactor":[1,1,1,1],"metallicFactor":0.25,"roughnessFactor":0.4,"metallicRoughnessTexture":{{"index":0}}}},
                "normalTexture":{{"index":1,"scale":0.5}},"emissiveFactor":[1,0.5,0],"emissiveTexture":{{"index":2}}}}],
            "textures":[{{"source":0}},{{"source":1}},{{"source":2}}],
            "images":[{{"bufferView":{metal},"mimeType":"image/png"}},{{"bufferView":{bumps},"mimeType":"image/png"}},{{"bufferView":{glow},"mimeType":"image/png"}}]"#
        ))
    }

    /// A triangle skinned to two joints, the top corner on the second,
    /// which slides a unit along x over a second (linear); its morph target
    /// lifts every corner a unit in z, its weight stepping from 0 to 1 at
    /// half a second; and the first joint swells to twice its size by a
    /// cubic spline, level at both ends.
    pub fn animated_glb() -> Vec<u8> {
        let mut g = Glb::default();
        let pos = g.floats(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 2.0, 0.0], 3);
        let lift = g.floats(&[0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0], 3);
        let joints = g.shorts(&[0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0], 4);
        let weights = g.floats(&[1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0], 4);
        let idx = g.shorts(&[0, 1, 2], 1);
        let inverse = g.floats(&[1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, -1.0, 0.0, 1.0], 16);
        let times = g.floats(&[0.0, 1.0], 1);
        let slide = g.floats(&[0.0, 1.0, 0.0, 1.0, 1.0, 0.0], 3);
        let step_times = g.floats(&[0.0, 0.5], 1);
        let steps = g.floats(&[0.0, 1.0], 1);
        let swell = g.floats(&[0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 2.0, 2.0, 2.0, 0.0, 0.0, 0.0], 3);
        g.finish(&format!(
            r#""scene":0,"scenes":[{{"nodes":[0,1]}}],
            "nodes":[{{"mesh":0,"skin":0}},{{"children":[2]}},{{"translation":[0,1,0]}}],
            "skins":[{{"joints":[1,2],"inverseBindMatrices":{inverse}}}],
            "meshes":[{{"primitives":[{{"attributes":{{"POSITION":{pos},"JOINTS_0":{joints},"WEIGHTS_0":{weights}}},"indices":{idx},"targets":[{{"POSITION":{lift}}}]}}],"weights":[0]}}],
            "animations":[{{"samplers":[{{"input":{times},"output":{slide},"interpolation":"LINEAR"}},{{"input":{step_times},"output":{steps},"interpolation":"STEP"}},{{"input":{times},"output":{swell},"interpolation":"CUBICSPLINE"}}],
                "channels":[{{"sampler":0,"target":{{"node":2,"path":"translation"}}}},{{"sampler":1,"target":{{"node":0,"path":"weights"}}}},{{"sampler":2,"target":{{"node":1,"path":"scale"}}}}]}}]"#
        ))
    }

    #[test]
    fn a_glb_material_has_its_metal_roughness_normal_map_and_emission() {
        let mesh = load(Format::Glb, &material_glb()).unwrap();
        assert!(mesh.rig.is_none(), "nothing moves it");
        let m = &mesh.parts[0].material;
        assert!((m.gloss - 0.6).abs() < 1e-6 && (m.metallic - 0.25).abs() < 1e-6);
        assert_eq!(m.metal_rough.as_ref().map(|t| t.rgba.clone()), Some(vec![0, 128, 255, 255]));
        let (bumps, scale) = m.bumps.clone().expect("a normal map");
        assert_eq!((bumps.rgba[1], scale), (200, 0.5));
        assert_eq!(m.emission, [1.0, 0.5f32.powf(1.0 / 2.2), 0.0]);
        assert!(m.glow.is_some());
        // Tangents worked out from the uvs: u runs along x, and v down the
        // square, so the bitangent (up the texture) is +y, w = +1.
        for v in &mesh.parts[0].vertices {
            assert!(v.tangent.iter().zip([1.0, 0.0, 0.0, 1.0]).all(|(a, b)| (a - b).abs() < 1e-5), "{:?}", v.tangent);
            let b = cross(v.normal, [v.tangent[0], v.tangent[1], v.tangent[2]]).map(|k| k * v.tangent[3]);
            assert!((b[1] - 1.0).abs() < 1e-5);
        }
    }

    /// A square facing the viewer with two sets of uvs, its material reading
    /// the second for its color (moved by a texture transform) and its
    /// occlusion, a normal map moved by a transform kept as JSON, emission
    /// made stronger, and a cut-out's alpha.
    pub fn features_glb() -> Vec<u8> {
        let mut g = Glb::default();
        let pos = g.floats(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 0.0], 3);
        let uv = g.floats(&[0.0, 1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0], 2);
        let uv1 = g.floats(&[0.5, 0.5, 0.75, 0.5, 0.75, 0.25, 0.5, 0.25], 2);
        let idx = g.shorts(&[0, 1, 2, 0, 2, 3], 1);
        let color = g.png(2, 1, &[255, 0, 0, 255, 0, 0, 255, 255]);
        let occlusion = g.png(1, 1, &[64, 64, 64, 255]);
        let bumps = g.png(1, 1, &[128, 128, 255, 255]);
        g.finish(&format!(
            r#""extensionsUsed":["KHR_texture_transform","KHR_materials_emissive_strength"],"extensionsRequired":["KHR_texture_transform"],
            "scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0}}],
            "meshes":[{{"primitives":[{{"attributes":{{"POSITION":{pos},"TEXCOORD_0":{uv},"TEXCOORD_1":{uv1}}},"indices":{idx},"material":0}}]}}],
            "materials":[{{"pbrMetallicRoughness":{{"baseColorTexture":{{"index":0,"extensions":{{"KHR_texture_transform":{{"offset":[0.5,0],"rotation":1.5707964,"scale":[2,1],"texCoord":1}}}}}}}},
                "occlusionTexture":{{"index":1,"strength":0.7,"texCoord":1}},
                "normalTexture":{{"index":2,"extensions":{{"KHR_texture_transform":{{"scale":[3,3]}}}}}},
                "emissiveFactor":[0.25,0.5,0],"extensions":{{"KHR_materials_emissive_strength":{{"emissiveStrength":4}}}},
                "alphaMode":"MASK","alphaCutoff":0.3}}],
            "textures":[{{"source":0}},{{"source":1}},{{"source":2}}],
            "images":[{{"bufferView":{color},"mimeType":"image/png"}},{{"bufferView":{occlusion},"mimeType":"image/png"}},{{"bufferView":{bumps},"mimeType":"image/png"}}]"#
        ))
    }

    /// Three triangles of one mesh, each a material of its own: see-through
    /// in front (z 1), solid in the middle, and see-through behind (z -1).
    pub fn layers_glb() -> Vec<u8> {
        let mut g = Glb::default();
        let triangle = |z: f32| [0.0, 0.0, z, 1.0, 0.0, z, 0.0, 1.0, z];
        let (near, middle, far) = (g.floats(&triangle(1.0), 3), g.floats(&triangle(0.0), 3), g.floats(&triangle(-1.0), 3));
        let idx = g.shorts(&[0, 1, 2], 1);
        g.finish(&format!(
            r#""scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0}}],
            "meshes":[{{"primitives":[{{"attributes":{{"POSITION":{near}}},"indices":{idx},"material":0}},
                {{"attributes":{{"POSITION":{middle}}},"indices":{idx},"material":1}},
                {{"attributes":{{"POSITION":{far}}},"indices":{idx},"material":0}}]}}],
            "materials":[{{"pbrMetallicRoughness":{{"baseColorFactor":[0,0.5,1,0.4]}},"alphaMode":"BLEND"}},{{"pbrMetallicRoughness":{{"baseColorFactor":[1,0.5,0,1]}}}}]"#
        ))
    }

    /// Two squares facing the viewer with two sets of uvs: the first's
    /// material has every extension Flow reads beyond emissive strength
    /// (specular and ior, clearcoat with its own normal map, sheen,
    /// transmission and volume), the second's is unlit. The file requires
    /// every one of them, as one may.
    pub fn extensions_glb() -> Vec<u8> {
        let mut g = Glb::default();
        let pos = g.floats(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 0.0], 3);
        let uv = g.floats(&[0.0, 1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0], 2);
        let idx = g.shorts(&[0, 1, 2, 0, 2, 3], 1);
        let gray = g.png(1, 1, &[128, 128, 128, 200]);
        let green = g.png(1, 1, &[0, 255, 0, 255]);
        let bumps = g.png(1, 1, &[128, 128, 255, 255]);
        g.finish(&format!(
            r#""extensionsUsed":["KHR_materials_specular","KHR_materials_ior","KHR_materials_clearcoat","KHR_materials_sheen","KHR_materials_transmission","KHR_materials_volume","KHR_materials_unlit","KHR_texture_transform"],
            "extensionsRequired":["KHR_materials_specular","KHR_materials_ior","KHR_materials_clearcoat","KHR_materials_sheen","KHR_materials_transmission","KHR_materials_volume","KHR_materials_unlit","KHR_texture_transform"],
            "scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0}}],
            "meshes":[{{"primitives":[{{"attributes":{{"POSITION":{pos},"TEXCOORD_0":{uv},"TEXCOORD_1":{uv}}},"indices":{idx},"material":0}},
                {{"attributes":{{"POSITION":{pos},"TEXCOORD_0":{uv}}},"indices":{idx},"material":1}}]}}],
            "materials":[{{"pbrMetallicRoughness":{{"baseColorFactor":[1,1,1,1],"roughnessFactor":0.2,"metallicFactor":0}},"extensions":{{
                    "KHR_materials_specular":{{"specularFactor":0.5,"specularColorFactor":[1,0.5,0.25],"specularTexture":{{"index":0}},"specularColorTexture":{{"index":1,"texCoord":1}}}},
                    "KHR_materials_ior":{{"ior":2.0}},
                    "KHR_materials_clearcoat":{{"clearcoatFactor":0.8,"clearcoatRoughnessFactor":0.25,"clearcoatTexture":{{"index":0}},
                        "clearcoatRoughnessTexture":{{"index":1,"texCoord":1,"extensions":{{"KHR_texture_transform":{{"scale":[2,2]}}}}}},"clearcoatNormalTexture":{{"index":2,"scale":0.5}}}},
                    "KHR_materials_sheen":{{"sheenColorFactor":[1,0.25,0],"sheenRoughnessFactor":0.6,"sheenColorTexture":{{"index":1}}}},
                    "KHR_materials_transmission":{{"transmissionFactor":0.9,"transmissionTexture":{{"index":0}}}},
                    "KHR_materials_volume":{{"thicknessFactor":0.2,"thicknessTexture":{{"index":1}},"attenuationDistance":0.5,"attenuationColor":[1,0.5,0.25]}}}}}},
                {{"pbrMetallicRoughness":{{"baseColorFactor":[1,0.5,0,1]}},"extensions":{{"KHR_materials_unlit":{{}},
                    "KHR_materials_clearcoat":{{"clearcoatFactor":0}},"KHR_materials_sheen":{{"sheenRoughnessFactor":0.5}}}}}}],
            "textures":[{{"source":0}},{{"source":1}},{{"source":2}}],
            "images":[{{"bufferView":{gray},"mimeType":"image/png"}},{{"bufferView":{green},"mimeType":"image/png"}},{{"bufferView":{bumps},"mimeType":"image/png"}}]"#
        ))
    }

    #[test]
    fn a_glbs_material_extensions_are_read_with_their_textures() {
        let mesh = load(Format::Glb, &extensions_glb()).unwrap();
        let (layered, unlit) = (&mesh.parts[0].material, &mesh.parts[1].material);
        let display = |c: f32| c.powf(1.0 / 2.2);
        let s = layered.specular.as_ref().expect("specular");
        assert_eq!((s.strength, s.color), (0.5, [1.0, display(0.5), display(0.25)]));
        assert_eq!((s.strength_map.as_ref().map(|t| t.rgba[3]), s.color_map.as_ref().map(|t| t.rgba[1])), (Some(200), Some(255)));
        assert_eq!((layered.maps[Tex::Specular as usize].set, layered.maps[Tex::SpecularColor as usize].set), (0, 1));
        assert_eq!(layered.ior, 2.0);
        let c = layered.clearcoat.as_ref().expect("a clearcoat");
        assert_eq!((c.strength, c.gloss, c.bumps.as_ref().map(|b| b.1)), (0.8, 0.75, Some(0.5)));
        assert!(c.strength_map.is_some() && c.roughness_map.is_some());
        assert_eq!(layered.maps[Tex::CoatRoughness as usize], UvMap::new(1, [0.0; 2], 0.0, [2.0; 2]), "its JSON texture's set and transform");
        let sh = layered.sheen.as_ref().expect("a sheen");
        assert_eq!((sh.color, sh.roughness, sh.color_map.is_some(), sh.roughness_map.is_some()), ([1.0, display(0.25), 0.0], 0.6, true, false));
        let t = layered.transmission.as_ref().expect("transmission");
        assert_eq!((t.amount, t.thickness, t.distance, t.attenuation), (0.9, 0.2, 0.5, [1.0, 0.5, 0.25]));
        assert!(t.map.is_some() && t.thickness_map.is_some());
        assert!(layered.see_through() && !layered.unlit, "light passes through it, so it's blended");
        // The clearcoat's normal map needs tangents, though the base has none.
        assert!(mesh.parts[0].vertices.iter().all(|v| v.tangent[3] != 0.0), "{:?}", mesh.parts[0].vertices[0].tangent);
        // Unlit, and a clearcoat of no strength or a black sheen is none.
        assert!(unlit.unlit && !unlit.see_through());
        assert!(unlit.clearcoat.is_none() && unlit.sheen.is_none() && unlit.specular.is_none() && unlit.transmission.is_none());
        assert_eq!(unlit.ior, 1.5);
    }

    /// A ball in the old specular-glossiness material, required: a dark
    /// red diffuse, a gold glaze, its glossiness from its texture's alpha
    /// (`textured`: glossy round one side, rough round the other, read
    /// through the second set of uvs, moved), else its factor alone. A
    /// second ball shares the material, and the file's
    /// pbrMetallicRoughness (a white metal, a fallback) is passed over.
    pub fn spec_gloss_glb(textured: bool) -> Vec<u8> {
        let (pos, normals, uvs, indices) = ball(12, 24);
        let mut g = Glb::default();
        let (p, n, t, i) = (g.floats(&pos.concat(), 3), g.floats(&normals.concat(), 3), g.floats(&uvs.concat(), 2), g.shorts(&indices, 1));
        let diffuse = g.png(1, 1, &[255, 255, 255, 255]);
        let sg = g.png(2, 1, &[255, 255, 255, 255, 255, 255, 255, 51]);
        let texture = if textured { r#","specularGlossinessTexture":{"index":1,"texCoord":1,"extensions":{"KHR_texture_transform":{"offset":[0.25,0]}}}"# } else { "" };
        g.finish(&format!(
            r#""extensionsUsed":["KHR_materials_pbrSpecularGlossiness","KHR_texture_transform"],"extensionsRequired":["KHR_materials_pbrSpecularGlossiness"],
            "scene":0,"scenes":[{{"nodes":[0,1]}}],"nodes":[{{"mesh":0}},{{"mesh":0,"translation":[3,0,0]}}],
            "meshes":[{{"primitives":[{{"attributes":{{"POSITION":{p},"NORMAL":{n},"TEXCOORD_0":{t},"TEXCOORD_1":{t}}},"indices":{i},"material":0}}]}}],
            "materials":[{{"pbrMetallicRoughness":{{"baseColorFactor":[1,1,1,1],"metallicFactor":1,"roughnessFactor":0}},"extensions":{{"KHR_materials_pbrSpecularGlossiness":{{
                "diffuseFactor":[0.5,0.02,0.02,1],"diffuseTexture":{{"index":0}},"specularFactor":[1,0.6,0.2],"glossinessFactor":0.9{texture}}}}}}}],
            "textures":[{{"source":0}},{{"source":1}}],
            "images":[{{"bufferView":{diffuse},"mimeType":"image/png"}},{{"bufferView":{sg},"mimeType":"image/png"}}]"#
        ))
    }

    #[test]
    fn a_specular_glossiness_material_is_read_as_metal_and_roughness_as_gltf_transform_turns_it() {
        let display = |c: f32| c.powf(1.0 / 2.2);
        for textured in [true, false] {
            let mesh = load(Format::Glb, &spec_gloss_glb(textured)).expect("Flow reads the old material, required");
            let m = &mesh.parts[0].material;
            // Its diffuse is the base color, over the metallic-roughness.
            assert_eq!(m.color, [display(0.5), display(0.02), display(0.02), 1.0]);
            assert_eq!(m.texture.as_ref().map(|t| t.rgba.clone()), Some(vec![255; 4]));
            assert_eq!(m.metallic, 0.0, "no metal");
            // Its specular color is KHR_materials_specular's at full
            // strength, as how much more than Flow's own glaze (0.04 head
            // on, at its ior of 1.5) it reflects, in display values.
            let s = m.specular.as_ref().expect("a specular layer");
            let glaze = display(0.04);
            assert_eq!((s.strength, s.color, s.strength_map.is_none()), (1.0, [1.0 / glaze, display(0.6) / glaze, display(0.2) / glaze], true));
            assert_eq!(m.ior, 1.5);
            if textured {
                // The texture's color is the specular color's map, and its
                // alpha times the factor the roughness map's green, read
                // as the file reads the texture; the factor's roughness 1.
                assert_eq!(s.color_map.as_ref().map(|t| t.rgba.clone()), Some(vec![255, 255, 255, 255, 255, 255, 255, 51]));
                assert_eq!(m.gloss, 0.0);
                assert_eq!(m.metal_rough.as_ref().map(|t| t.rgba.clone()), Some(vec![0, 255 - 230, 0, 255, 0, 255 - 46, 0, 255]));
                let moved = UvMap::new(1, [0.25, 0.0], 0.0, [1.0; 2]);
                assert_eq!((m.maps[Tex::SpecularColor as usize], m.maps[Tex::MetalRough as usize]), (moved, moved));
                // The second ball shares what was baked.
                let shared = |k: usize| mesh.parts[k].material.metal_rough.clone().unwrap();
                assert!(Arc::ptr_eq(&shared(0), &shared(1)));
            } else {
                assert!((m.gloss - 0.9).abs() < 1e-6 && m.metal_rough.is_none() && s.color_map.is_none());
            }
        }
    }

    /// A triangle facing +x, a perspective camera five units out along +x
    /// looking back at it, and three lights: a directional one shining
    /// down, a red point light two units out along +x, and a spot above.
    pub fn lights_glb(camera: bool) -> Vec<u8> {
        let mut g = Glb::default();
        let pos = g.floats(&[0.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 1.0, 0.0], 3);
        let idx = g.shorts(&[0, 1, 2], 1);
        let half = std::f32::consts::FRAC_1_SQRT_2;
        let roots = if camera { "[0,1,2,3,4]" } else { "[0,2,3,4]" };
        g.finish(&format!(
            r#""extensionsUsed":["KHR_lights_punctual"],
            "extensions":{{"KHR_lights_punctual":{{"lights":[{{"type":"directional","intensity":2}},{{"type":"point","color":[1,0,0],"intensity":9,"range":10}},
                {{"type":"spot","intensity":1,"spot":{{"innerConeAngle":0.2,"outerConeAngle":0.4}}}}]}}}},
            "cameras":[{{"type":"perspective","perspective":{{"yfov":0.8,"znear":0.1}}}}],
            "scene":0,"scenes":[{{"nodes":{roots}}}],
            "nodes":[{{"mesh":0}},{{"camera":0,"translation":[5,0,0],"rotation":[0,{half},0,{half}]}},
                {{"rotation":[-{half},0,0,{half}],"extensions":{{"KHR_lights_punctual":{{"light":0}}}}}},
                {{"translation":[2,0,0],"extensions":{{"KHR_lights_punctual":{{"light":1}}}}}},
                {{"translation":[0,3,0],"rotation":[-{half},0,0,{half}],"extensions":{{"KHR_lights_punctual":{{"light":2}}}}}}],
            "meshes":[{{"primitives":[{{"attributes":{{"POSITION":{pos}}},"indices":{idx}}}]}}]"#
        ))
    }

    #[test]
    fn a_glbs_camera_turns_the_model_to_face_the_view_as_it_faces_the_camera() {
        // Without the camera: the triangle stands edge on, in the yz plane.
        let plain = load(Format::Glb, &lights_glb(false)).unwrap();
        assert_eq!(plain.extent, [0.0, 1.0, 1.0]);
        // Seen as the camera sees it: face on, the file's -z to the right.
        let mesh = load(Format::Glb, &lights_glb(true)).unwrap();
        let close = |a: [f32; 3], b: [f32; 3]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-5);
        assert!(close(mesh.extent, [1.0, 1.0, 0.0]) && mesh.flat(), "{:?}", mesh.extent);
        let p = &mesh.parts[0];
        assert!(close(p.vertices[1].pos, [0.5, -0.5, 0.0]), "{:?}", p.vertices[1].pos);
        assert!(p.vertices.iter().all(|v| close(v.normal, [0.0, 0.0, 1.0])), "facing the viewer: {:?}", p.vertices[0].normal);
    }

    #[test]
    fn a_glbs_lights_are_placed_in_its_box_and_share_the_light_on_its_middle() {
        let mesh = load(Format::Glb, &lights_glb(true)).unwrap();
        let close = |a: [f32; 3], b: [f32; 3]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-5);
        assert_eq!(mesh.lights.len(), 3);
        let total: f32 = mesh.lights.iter().map(|l| l.strength).sum();
        assert!((total - 1.0).abs() < 1e-5, "{total}");
        // Strongest first: the sun (2 lux) and the point light (9 candela
        // 4.5 units² from the middle: 2 lux) alike, then the spot.
        let (sun, point, spot) = (mesh.lights[0], mesh.lights[1], mesh.lights[2]);
        assert!(sun.at.is_none() && close(sun.aim, [0.0, -1.0, 0.0]), "{sun:?}");
        assert!((sun.strength - point.strength).abs() < 1e-5 && spot.strength < point.strength * 0.1, "{:?}", mesh.lights);
        // The point light, toward the camera, sits in front of the model in
        // its box (less the middle, over the longest side, which is 1).
        assert!(close(point.at.unwrap(), [-0.5, -0.5, 2.0]) && point.color == [1.0, 0.0, 0.0] && point.range == Some(10.0), "{point:?}");
        let (inner, outer) = spot.cone.expect("a cone");
        assert!((inner - 0.2f32.cos()).abs() < 1e-6 && (outer - 0.4f32.cos()).abs() < 1e-6 && close(spot.aim, [0.0, -1.0, 0.0]));
        // A file without lights keeps Flow's own.
        assert!(load(Format::Glb, &material_glb()).unwrap().lights.is_empty());
    }

    /// Two see-through squares, red and blue, crossing in an X seen from
    /// above: the red one leaning back to the left, the blue to the right.
    pub fn crossed_glb() -> Vec<u8> {
        let mut g = Glb::default();
        let lean = |k: f32| [-0.5, -0.5, -0.5 * k, 0.5, -0.5, 0.5 * k, 0.5, 0.5, 0.5 * k, -0.5, 0.5, -0.5 * k];
        let (red, blue) = (g.floats(&lean(1.0), 3), g.floats(&lean(-1.0), 3));
        let idx = g.shorts(&[0, 1, 2, 0, 2, 3], 1);
        g.finish(&format!(
            r#""scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0}}],
            "meshes":[{{"primitives":[{{"attributes":{{"POSITION":{red}}},"indices":{idx},"material":0}},{{"attributes":{{"POSITION":{blue}}},"indices":{idx},"material":1}}]}}],
            "materials":[{{"pbrMetallicRoughness":{{"baseColorFactor":[1,0,0,0.6],"metallicFactor":0}},"alphaMode":"BLEND"}},
                {{"pbrMetallicRoughness":{{"baseColorFactor":[0,0,1,0.6],"metallicFactor":0}},"alphaMode":"BLEND"}}]"#
        ))
    }

    /// `crossed_glb`'s squares on nodes of their own, the blue one sliding
    /// half a unit right over a second, so where they cross slides a
    /// quarter of a unit right.
    pub fn sliding_crossed_glb() -> Vec<u8> {
        let mut g = Glb::default();
        let lean = |k: f32| [-0.5, -0.5, -0.5 * k, 0.5, -0.5, 0.5 * k, 0.5, 0.5, 0.5 * k, -0.5, 0.5, -0.5 * k];
        let (red, blue) = (g.floats(&lean(1.0), 3), g.floats(&lean(-1.0), 3));
        let idx = g.shorts(&[0, 1, 2, 0, 2, 3], 1);
        let times = g.floats(&[0.0, 1.0], 1);
        let slide = g.floats(&[0.0, 0.0, 0.0, 0.5, 0.0, 0.0], 3);
        g.finish(&format!(
            r#""scene":0,"scenes":[{{"nodes":[0,1]}}],"nodes":[{{"mesh":0}},{{"mesh":1}}],
            "meshes":[{{"primitives":[{{"attributes":{{"POSITION":{red}}},"indices":{idx},"material":0}}]}},{{"primitives":[{{"attributes":{{"POSITION":{blue}}},"indices":{idx},"material":1}}]}}],
            "materials":[{{"pbrMetallicRoughness":{{"baseColorFactor":[1,0,0,0.6],"metallicFactor":0}},"alphaMode":"BLEND"}},
                {{"pbrMetallicRoughness":{{"baseColorFactor":[0,0,1,0.6],"metallicFactor":0}},"alphaMode":"BLEND"}}],
            "animations":[{{"samplers":[{{"input":{times},"output":{slide},"interpolation":"LINEAR"}}],"channels":[{{"sampler":0,"target":{{"node":1,"path":"translation"}}}}]}}]"#
        ))
    }

    /// A white square two units wide facing the viewer, lit by a red point
    /// light in front of it that the first animation carries from its left
    /// to its right over a second, a directional light shining at it that
    /// the same animation turns a quarter about y, and a spot light above
    /// that stays; the second animation lifts the square and moves no light.
    /// Its camera, five units in front looking at it as Flow does, stays.
    pub fn moving_lights_glb() -> Vec<u8> {
        let mut g = Glb::default();
        let pos = g.floats(&[-1.0, -1.0, 0.0, 1.0, -1.0, 0.0, 1.0, 1.0, 0.0, -1.0, 1.0, 0.0], 3);
        let idx = g.shorts(&[0, 1, 2, 0, 2, 3], 1);
        let times = g.floats(&[0.0, 1.0], 1);
        let carry = g.floats(&[-1.5, 0.0, 1.0, 1.5, 0.0, 1.0], 3);
        let half = std::f32::consts::FRAC_1_SQRT_2;
        let turn = g.floats(&[0.0, 0.0, 0.0, 1.0, 0.0, half, 0.0, half], 4);
        let lift = g.floats(&[0.0, 0.0, 0.0, 0.0, 0.0, 0.5], 3);
        g.finish(&format!(
            r#""extensionsUsed":["KHR_lights_punctual"],"extensionsRequired":["KHR_lights_punctual"],
            "extensions":{{"KHR_lights_punctual":{{"lights":[{{"type":"point","color":[1,0,0],"intensity":5}},{{"type":"directional","intensity":1}},
                {{"type":"spot","intensity":1,"spot":{{"outerConeAngle":0.5}}}}]}}}},
            "cameras":[{{"type":"perspective","perspective":{{"yfov":0.8,"znear":0.1}}}}],
            "scene":0,"scenes":[{{"nodes":[0,1,2,3,4]}}],
            "nodes":[{{"mesh":0}},{{"translation":[-1.5,0,1],"extensions":{{"KHR_lights_punctual":{{"light":0}}}}}},{{"extensions":{{"KHR_lights_punctual":{{"light":1}}}}}},
                {{"translation":[0,2,1],"extensions":{{"KHR_lights_punctual":{{"light":2}}}}}},{{"camera":0,"translation":[0,0,5]}}],
            "meshes":[{{"primitives":[{{"attributes":{{"POSITION":{pos}}},"indices":{idx},"material":0}}]}}],
            "materials":[{{"pbrMetallicRoughness":{{"baseColorFactor":[1,1,1,1],"metallicFactor":0}}}}],
            "animations":[{{"samplers":[{{"input":{times},"output":{carry},"interpolation":"LINEAR"}},{{"input":{times},"output":{turn},"interpolation":"LINEAR"}}],
                    "channels":[{{"sampler":0,"target":{{"node":1,"path":"translation"}}}},{{"sampler":1,"target":{{"node":2,"path":"rotation"}}}}]}},
                {{"samplers":[{{"input":{times},"output":{lift},"interpolation":"LINEAR"}}],"channels":[{{"sampler":0,"target":{{"node":0,"path":"translation"}}}}]}}]"#
        ))
    }

    /// A triangle with a corner at (10, 0, 0), facing +z, and a camera five
    /// units in front of that corner looking back at it, on a pivot there
    /// that an animation turns a quarter about y over a second, so the
    /// camera goes round to the triangle's side.
    pub fn orbiting_camera_glb() -> Vec<u8> {
        let mut g = Glb::default();
        let pos = g.floats(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0], 3);
        let idx = g.shorts(&[0, 1, 2], 1);
        let times = g.floats(&[0.0, 1.0], 1);
        let half = std::f32::consts::FRAC_1_SQRT_2;
        let turn = g.floats(&[0.0, 0.0, 0.0, 1.0, 0.0, half, 0.0, half], 4);
        g.finish(&format!(
            r#""cameras":[{{"type":"perspective","perspective":{{"yfov":0.8,"znear":0.1}}}}],
            "scene":0,"scenes":[{{"nodes":[0,1]}}],
            "nodes":[{{"mesh":0,"translation":[10,0,0]}},{{"translation":[10,0,0],"children":[2]}},{{"camera":0,"translation":[0,0,5]}}],
            "meshes":[{{"primitives":[{{"attributes":{{"POSITION":{pos}}},"indices":{idx}}}]}}],
            "animations":[{{"samplers":[{{"input":{times},"output":{turn},"interpolation":"LINEAR"}}],"channels":[{{"sampler":0,"target":{{"node":1,"path":"rotation"}}}}]}}]"#
        ))
    }

    /// A triangle whose file requires `required` (the JSON list's insides).
    fn requiring(required: &str) -> Vec<u8> {
        let mut g = Glb::default();
        let pos = g.floats(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0], 3);
        g.finish(&format!(
            r#""extensionsUsed":[{required}],"extensionsRequired":[{required}],"scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0}}],
            "meshes":[{{"primitives":[{{"attributes":{{"POSITION":{pos}}}}}]}}]"#
        ))
    }

    #[test]
    fn a_file_may_require_the_extensions_flow_reads_and_one_requiring_others_is_refused_by_name() {
        // Clearcoat, sheen, specular and volume are Flow's own reading,
        // which the gltf crate alone won't take as required.
        let all = extensions_glb();
        assert!(gltf::import_slice(&all).is_err(), "the gltf crate alone refuses it");
        let mesh = load(Format::Glb, &all).expect("Flow reads all it requires");
        let m = &mesh.parts[0].material;
        assert!(m.clearcoat.is_some() && m.sheen.is_some() && m.specular.is_some() && m.transmission.as_ref().is_some_and(|t| t.thickness > 0.0));
        for name in REQUIRABLE {
            assert!(load(Format::Glb, &requiring(&format!("\"{name}\""))).is_ok(), "{name}");
        }
        // What Flow doesn't do, by name.
        assert_eq!(load(Format::Glb, &requiring(r#""KHR_materials_anisotropy""#)).unwrap_err(), "it requires KHR_materials_anisotropy, which Flow doesn't read");
        let several = requiring(r#""KHR_draco_mesh_compression","KHR_materials_anisotropy","EXT_meshopt_compression","EXT_mesh_gpu_instancing","KHR_materials_iridescence","KHR_mesh_quantization","EXT_texture_webp""#);
        assert_eq!(load(Format::Glb, &several).unwrap_err(), "it requires KHR_materials_anisotropy and KHR_materials_iridescence, which Flow doesn't read");
        // The crate still checks the rest of the file.
        let mut broken = Glb::default();
        let pos = broken.floats(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0], 3);
        let broken = broken.finish(&format!(
            r#""extensionsRequired":["KHR_materials_sheen"],"scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0}}],
            "meshes":[{{"primitives":[{{"attributes":{{"POSITION":{pos}}},"indices":7}}]}}]"#
        ));
        assert!(load(Format::Glb, &broken).is_err(), "indices naming an accessor that isn't there");
    }

    /// A triangle facing the viewer whose corners' colors (COLOR_0) are
    /// `bytes`: `kind` (VEC3 or VEC4) of glTF's `component` type,
    /// normalized or not, each `stride` bytes apart; its material
    /// `material` (the JSON object's insides).
    pub fn colored_triangle(bytes: &[u8], component: u32, normalized: bool, kind: &str, stride: usize, material: &str) -> Vec<u8> {
        let mut g = Glb::default();
        let pos = g.floats(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0], 3);
        let view = g.view(bytes);
        let end = g.views[view].len() - 1;
        g.views[view].insert_str(end, &format!(r#","byteStride":{stride}"#));
        let color = g.accessor(view, component, normalized, 3, kind, "");
        g.finish(&format!(
            r#""scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0}}],
            "meshes":[{{"primitives":[{{"attributes":{{"POSITION":{pos},"COLOR_0":{color}}},"material":0}}]}}],
            "materials":[{{{material}}}]"#
        ))
    }

    /// Three linear colors, RGBA from 0 to 1, one for each of
    /// `colored_triangle`'s corners.
    pub const CORNER_COLORS: [[f32; 4]; 3] = [[1.0, 0.0, 0.0, 1.0], [0.0, 0.5, 0.0, 0.25], [0.2, 0.2, 1.0, 0.75]];

    #[test]
    fn vertex_colors_reach_the_vertices_as_shown_however_theyre_kept() {
        let white = r#""pbrMetallicRoughness":{"baseColorFactor":[1,1,1,1]}"#;
        // `CORNER_COLORS` as floats, or as normalized bytes or shorts (`max`
        // their largest, `wide` their bytes), RGB or RGBA, each padded to
        // `stride`; and what glTF reads them as.
        let kept = |max: f32, wide: usize, n: usize, stride: usize| -> (Vec<u8>, Vec<[f32; 4]>) {
            let (mut bytes, mut held) = (Vec::new(), Vec::new());
            for c in CORNER_COLORS {
                let q = c.map(|v| (v * max).round());
                for v in &q[..n] {
                    bytes.extend(if wide == 1 { vec![*v as u8] } else { (*v as u16).to_le_bytes().to_vec() });
                }
                bytes.resize(bytes.len().next_multiple_of(stride), 0);
                held.push([q[0] / max, q[1] / max, q[2] / max, if n == 4 { q[3] / max } else { 1.0 }]);
            }
            (bytes, held)
        };
        let floats = |n: usize| -> (Vec<u8>, Vec<[f32; 4]>) {
            let bytes = CORNER_COLORS.iter().flat_map(|c| c[..n].iter().flat_map(|v| v.to_le_bytes())).collect();
            (bytes, CORNER_COLORS.iter().map(|c| if n == 4 { *c } else { [c[0], c[1], c[2], 1.0] }).collect())
        };
        let cases = [
            ("floats, RGBA", floats(4), 5126, false, "VEC4", 16),
            ("floats, RGB", floats(3), 5126, false, "VEC3", 12),
            ("bytes, RGBA", kept(255.0, 1, 4, 4), 5121, true, "VEC4", 4),
            ("bytes, RGB", kept(255.0, 1, 3, 4), 5121, true, "VEC3", 4),
            ("shorts, RGBA", kept(65535.0, 2, 4, 8), 5123, true, "VEC4", 8),
            ("shorts, RGB", kept(65535.0, 2, 3, 8), 5123, true, "VEC3", 8),
        ];
        for (name, (bytes, held), component, normalized, kind, stride) in cases {
            let mesh = load(Format::Glb, &colored_triangle(&bytes, component, normalized, kind, stride, white)).unwrap_or_else(|e| panic!("{name}: {e}"));
            let got: Vec<[u8; 4]> = mesh.parts[0].vertices.iter().map(|v| v.color).collect();
            assert_eq!(got, held.into_iter().map(vertex_color).collect::<Vec<_>>(), "{name}");
            // Display values, as the material's colors are shown; RGB is
            // opaque. The material's own color is left as it is.
            if name == "floats, RGBA" {
                assert_eq!(got, [[255, 0, 0, 255], [0, 186, 0, 64], [123, 123, 255, 191]]);
            }
            if kind == "VEC3" {
                assert!(got.iter().all(|c| c[3] == 255), "{name}");
            }
            assert_eq!(mesh.parts[0].material.color, [1.0; 4]);
        }
        // Their alpha goes with the material's, as its alpha mode takes it.
        let (bytes, _) = floats(4);
        let blended = load(Format::Glb, &colored_triangle(&bytes, 5126, false, "VEC4", 16, r#""alphaMode":"BLEND""#)).unwrap();
        assert!(blended.parts[0].material.see_through() && blended.parts[0].vertices[1].color[3] == 64);
        // A model without them is white at every corner, as before.
        for mesh in [load(Format::Glb, &material_glb()).unwrap(), load(Format::Glb, &ball_glb(false)).unwrap(), load(Format::Obj, b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap()] {
            assert!(mesh.parts.iter().flat_map(|p| &p.vertices).all(|v| v.color == [255; 4]));
        }
    }

    /// A triangle (its corners at the origin, a unit along x and a unit
    /// along y) on a node that turns it a quarter about z and moves it 10
    /// along x, drawn for each of the node's three instances, which the
    /// file requires (EXT_mesh_gpu_instancing): as it is; moved 3 along x
    /// in the node's own space, turned a quarter about z and twice the
    /// size; and moved 6, mirrored in x. Its turns are normalized shorts
    /// and its sizes bytes, as KHR_mesh_quantization allows. `animated`
    /// adds an animation sliding the node 4 up over a second.
    pub fn instanced_glb(animated: bool) -> Vec<u8> {
        let mut g = Glb::default();
        let pos = g.floats(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0], 3);
        let idx = g.shorts(&[0, 1, 2], 1);
        let moves = g.floats(&[0.0, 0.0, 0.0, 3.0, 0.0, 0.0, 6.0, 0.0, 0.0], 3);
        let turns = g.view(&[0i16, 0, 0, 32767, 0, 0, 23170, 23170, 0, 0, 0, 32767].iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<_>>());
        let turns = g.accessor(turns, 5122, true, 3, "VEC4", "");
        let sizes = g.view(&[1i8, 1, 1, 0, 2, 2, 2, 0, -1, 1, 1, 0].map(|v| v as u8));
        let end = g.views[sizes].len() - 1;
        g.views[sizes].insert_str(end, r#","byteStride":4"#);
        let sizes = g.accessor(sizes, 5120, false, 3, "VEC3", "");
        let animation = if animated {
            let (times, slide) = (g.floats(&[0.0, 1.0], 1), g.floats(&[10.0, 0.0, 0.0, 10.0, 4.0, 0.0], 3));
            format!(r#","animations":[{{"samplers":[{{"input":{times},"output":{slide},"interpolation":"LINEAR"}}],"channels":[{{"sampler":0,"target":{{"node":0,"path":"translation"}}}}]}}]"#)
        } else {
            String::new()
        };
        let h = std::f32::consts::FRAC_1_SQRT_2;
        g.finish(&format!(
            r#""extensionsUsed":["EXT_mesh_gpu_instancing","KHR_mesh_quantization"],"extensionsRequired":["EXT_mesh_gpu_instancing","KHR_mesh_quantization"],
            "scene":0,"scenes":[{{"nodes":[0]}}],
            "nodes":[{{"mesh":0,"translation":[10,0,0],"rotation":[0,0,{h},{h}],"extensions":{{"EXT_mesh_gpu_instancing":{{"attributes":{{"TRANSLATION":{moves},"ROTATION":{turns},"SCALE":{sizes}}}}}}}}}],
            "meshes":[{{"primitives":[{{"attributes":{{"POSITION":{pos}}},"indices":{idx}}}]}}]{animation}"#
        ))
    }

    /// Where `instanced_glb`'s node puts each instance's corners, in the
    /// file's units (x and y; z is 0): each instance placed in the node's
    /// space, then the node turning and moving it.
    pub const INSTANCED_CORNERS: [[f32; 2]; 9] = [[10.0, 0.0], [10.0, 1.0], [9.0, 0.0], [10.0, 3.0], [8.0, 3.0], [10.0, 1.0], [10.0, 6.0], [10.0, 5.0], [9.0, 6.0]];

    #[test]
    fn a_nodes_instances_each_draw_its_mesh_placed_in_its_own_space_before_the_node_places_it() {
        let mesh = load(Format::Glb, &instanced_glb(false)).expect("Flow reads the instancing it requires");
        let close = |a: [f32; 3], b: [f32; 3]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-5);
        assert_eq!(mesh.parts.len(), 1, "one part, a copy for each instance");
        let p = &mesh.parts[0];
        assert_eq!((p.vertices.len(), p.node), (9, Some(0)));
        // In the box that holds them all, from (8, 0) to (10, 6): less its
        // middle, over its longest side.
        for (v, [x, y]) in p.vertices.iter().zip(INSTANCED_CORNERS) {
            assert!(close(v.pos, [(x - 9.0) / 6.0, (y - 3.0) / 6.0, 0.0]), "{:?}, not ({x}, {y})", v.pos);
        }
        assert!(close(mesh.extent, [1.0 / 3.0, 1.0, 0.0]), "{:?}", mesh.extent);
        // The mirrored one's triangle turned round, so each faces the way
        // its normals do, toward the viewer.
        assert_eq!(p.indices, [0, 1, 2, 3, 4, 5, 6, 8, 7]);
        for t in p.indices.as_chunks::<3>().0 {
            let [a, b, c] = t.map(|i| p.vertices[i as usize]);
            assert!(dot(cross(sub(b.pos, a.pos), sub(c.pos, a.pos)), a.normal) > 0.0, "{t:?}");
        }
        assert!(p.vertices.iter().all(|v| close(v.normal, [0.0, 0.0, 1.0])));
        assert!(mesh.rig.is_none(), "nothing moves it");
    }

    #[test]
    fn instances_follow_their_node_as_it_moves_and_the_box_holds_them_all_the_way() {
        let mesh = load(Format::Glb, &instanced_glb(true)).unwrap();
        let close = |a: [f32; 3], b: [f32; 3]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-5);
        // The node slides 4 up, so the box runs from (8, 0) to (10, 10):
        // its middle (9, 5), its longest side 10.
        assert!(close(mesh.extent, [0.2, 1.0, 0.0]), "{:?}", mesh.extent);
        for (secs, up) in [(0.0, 0.0), (0.5, 2.0), (1.0, 4.0)] {
            let posed = mesh.posed(0, secs).expect("it plays");
            assert_eq!(posed.vertices[0].len(), 9);
            for (v, [x, y]) in posed.vertices[0].iter().zip(INSTANCED_CORNERS) {
                assert!(close(v.pos, [(x - 9.0) / 10.0, (y + up - 5.0) / 10.0, 0.0]), "{secs}s: {:?}, not ({x}, {})", v.pos, y + up);
            }
        }
    }

    /// A mesh of `corners` corners (the first three a triangle) and
    /// `triangles` triangles (all that one), on a node whose instances'
    /// `attributes` (the JSON object's insides) `make` writes.
    fn instanced(corners: usize, triangles: usize, make: impl FnOnce(&mut Glb) -> String) -> Vec<u8> {
        let mut g = Glb::default();
        let mut pos = vec![0.0f32; corners * 3];
        (pos[3], pos[7]) = (1.0, 1.0);
        let pos = g.floats(&pos, 3);
        let idx = g.shorts(&[0, 1, 2].repeat(triangles), 1);
        let attributes = make(&mut g);
        g.finish(&format!(
            r#""scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0,"extensions":{{"EXT_mesh_gpu_instancing":{{"attributes":{{{attributes}}}}}}}}}],
            "meshes":[{{"primitives":[{{"attributes":{{"POSITION":{pos}}},"indices":{idx}}}]}}]"#
        ))
    }

    #[test]
    fn instances_past_what_a_model_holds_or_that_cant_be_read_are_refused_saying_why() {
        let moved = |n: usize| move |g: &mut Glb| format!(r#""TRANSLATION":{}"#, g.floats(&vec![0.0; 3 * n], 3));
        // Their triangles count toward the model's 2 million, before
        // they're made; and their vertices are at most 6 million.
        assert!(load(Format::Glb, &instanced(3, 500, moved(4000))).is_ok(), "2 million triangles, as many as a model may have");
        assert_eq!(load(Format::Glb, &instanced(3, 1000, moved(2001))).unwrap_err(), "2001000 triangles with its instances, more than 2000000");
        assert_eq!(load(Format::Glb, &instanced(3000, 1, moved(2001))).unwrap_err(), "its instances make 6003000 vertices, more than 6000000");
        // More instances than a model's triangles, refused before they're
        // read: a sparse accessor of zeros, two million and one long.
        let many = instanced(3, 1, |g| {
            let (at, value) = (g.view(&0u32.to_le_bytes()), g.view(&[0u8; 12]));
            g.accessors.push(format!(r#"{{"componentType":5126,"count":2000001,"type":"VEC3","sparse":{{"count":1,"indices":{{"bufferView":{at},"componentType":5125}},"values":{{"bufferView":{value}}}}}}}"#));
            format!(r#""TRANSLATION":{}"#, g.accessors.len() - 1)
        });
        assert_eq!(load(Format::Glb, &many).unwrap_err(), "node 0 has 2000001 instances, more than 2000000");
        // Attributes that don't number alike, aren't what they say, or
        // name no accessor.
        let unlike = instanced(3, 1, |g| format!(r#""TRANSLATION":{},"SCALE":{}"#, g.floats(&[0.0; 9], 3), g.floats(&[1.0; 6], 3)));
        assert_eq!(load(Format::Glb, &unlike).unwrap_err(), "node 0's instances' attributes don't number alike");
        let flat = instanced(3, 1, |g| format!(r#""TRANSLATION":{}"#, g.floats(&[0.0; 6], 2)));
        assert_eq!(load(Format::Glb, &flat).unwrap_err(), "node 0's instances' TRANSLATION isn't a VEC3 of numbers");
        let nowhere = instanced(3, 1, |_| r#""ROTATION":99"#.to_string());
        assert_eq!(load(Format::Glb, &nowhere).unwrap_err(), "node 0's instances' ROTATION names no accessor");
        // A program's own attribute counts them too, each where the node
        // puts its mesh.
        let ids = load(Format::Glb, &instanced(3, 1, |g| format!(r#""_ID":{}"#, g.floats(&[0.0, 1.0], 1)))).unwrap();
        assert_eq!((ids.parts[0].vertices.len(), &ids.parts[0].indices[..]), (6, &[0, 1, 2, 3, 4, 5][..]));
        assert_eq!(ids.parts[0].vertices[0].pos, ids.parts[0].vertices[3].pos);
    }


    /// A ball of `rings` × `segments` faces, a unit in radius around (10,
    /// -3, 2), with normals and uvs, and a texture checked red and white.
    /// Plain, its numbers are floats and its texture a PNG. Packed as gltfpack
    /// packs a model: positions quantized to 14 bits (the node's scale and
    /// translation turning them back), normals to bytes through meshopt's
    /// octahedral filter, uvs to normalized shorts, every view meshopt
    /// compressed (the triangles too), the texture a WebP alone, and the
    /// file requiring all of it.
    /// A ball's corners, normals, uvs and triangles.
    pub type Ball = (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<[f32; 2]>, Vec<u16>);

    /// A ball's corners (around (10, -3, 2)), normals, uvs (u round it, v
    /// down from its top) and triangles: `rings` × `segments` faces, a unit
    /// in radius.
    pub fn ball(rings: u32, segments: u32) -> Ball {
        let (mut pos, mut normals, mut uvs) = (Vec::new(), Vec::new(), Vec::new());
        for r in 0..=rings {
            let (across, up) = (r as f32 / rings as f32 * std::f32::consts::PI).sin_cos();
            for k in 0..=segments {
                let (z, x) = (k as f32 / segments as f32 * std::f32::consts::TAU).sin_cos();
                let n = [x * across, up, z * across];
                normals.push(n);
                pos.push([10.0 + n[0], -3.0 + n[1], 2.0 + n[2]]);
                uvs.push([k as f32 / segments as f32, r as f32 / rings as f32]);
            }
        }
        let mut indices: Vec<u16> = Vec::new();
        for r in 0..rings {
            for k in 0..segments {
                let at = |r: u32, k: u32| (r * (segments + 1) + k) as u16;
                // Counter-clockwise seen from outside, as its normals face.
                indices.extend([at(r, k), at(r + 1, k + 1), at(r + 1, k), at(r, k), at(r, k + 1), at(r + 1, k + 1)]);
            }
        }
        (pos, normals, uvs, indices)
    }

    pub fn ball_glb(packed: bool) -> Vec<u8> {
        let (pos, normals, uvs, indices) = ball(12, 24);
        let checker: Vec<u8> = (0..16).flat_map(|i| if (i % 4 + i / 4) % 2 == 0 { [220, 30, 30, 255] } else { [240, 240, 240, 255] }).collect();
        let mut g = Glb::default();
        if !packed {
            let p = g.floats(&pos.concat(), 3);
            let n = g.floats(&normals.concat(), 3);
            let t = g.floats(&uvs.concat(), 2);
            let i = g.shorts(&indices, 1);
            let image = g.png(4, 4, &checker);
            return g.finish(&format!(
                r#""scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0}}],
                "meshes":[{{"primitives":[{{"attributes":{{"POSITION":{p},"NORMAL":{n},"TEXCOORD_0":{t}}},"indices":{i},"material":0}}]}}],
                "materials":[{{"pbrMetallicRoughness":{{"baseColorTexture":{{"index":0}},"metallicFactor":0}}}}],
                "textures":[{{"source":0}}],"images":[{{"bufferView":{image},"mimeType":"image/png"}}]"#
            ));
        }
        // 14 bits over the ball's box, the same scale every way, so the
        // node turns normals back as it turns the file's.
        let (lo, step) = ([9.0f32, -4.0, 1.0], 2.0 / 16383.0);
        let q: Vec<u8> = pos.iter().flat_map(|p| [0, 1, 2].map(|k| ((p[k] - lo[k]) / step).round() as u16).into_iter().chain([0]).flat_map(u16::to_le_bytes)).collect();
        let oct: Vec<u8> = normals.iter().flat_map(|&n| super::super::meshopt::tests::octahedral_normal(n, 8, 0).map(|c| c as i8 as u8)).collect();
        let t: Vec<u8> = uvs.iter().flat_map(|u| u.map(|v| (v * 65535.0).round() as u16).into_iter().flat_map(u16::to_le_bytes)).collect();
        let i: Vec<u8> = indices.iter().flat_map(|v| v.to_le_bytes()).collect();
        let (pv, nv, tv, iv) = (g.packed(&q, 8, "ATTRIBUTES", "NONE"), g.packed(&oct, 4, "ATTRIBUTES", "OCTAHEDRAL"), g.packed(&t, 4, "ATTRIBUTES", "NONE"), g.packed(&i, 2, "TRIANGLES", "NONE"));
        let count = pos.len();
        let p = g.accessor(pv, 5123, false, count, "VEC3", r#","min":[0,0,0],"max":[16383,16383,16383]"#);
        let n = g.accessor(nv, 5120, true, count, "VEC3", "");
        let t = g.accessor(tv, 5123, true, count, "VEC2", "");
        let i = g.accessor(iv, 5123, false, indices.len(), "SCALAR", "");
        let image = g.webp(4, 4, &checker);
        let used = r#""KHR_mesh_quantization","EXT_meshopt_compression","EXT_texture_webp""#;
        g.finish(&format!(
            r#""extensionsUsed":[{used}],"extensionsRequired":[{used}],"scene":0,"scenes":[{{"nodes":[0]}}],
            "nodes":[{{"mesh":0,"translation":[{},{},{}],"scale":[{step},{step},{step}]}}],
            "meshes":[{{"primitives":[{{"attributes":{{"POSITION":{p},"NORMAL":{n},"TEXCOORD_0":{t}}},"indices":{i},"material":0}}]}}],
            "materials":[{{"pbrMetallicRoughness":{{"baseColorTexture":{{"index":0}},"metallicFactor":0}}}}],
            "textures":[{{"extensions":{{"EXT_texture_webp":{{"source":0}}}}}}],"images":[{{"bufferView":{image},"mimeType":"image/webp"}}]"#,
            lo[0], lo[1], lo[2]
        ))
    }

    /// `ball_glb`'s ball packed as KHR_meshopt_compression may pack it,
    /// which the file requires: as `ball_glb(true)` packs it, but every
    /// attribute through the vertex codec's version 1, its positions'
    /// shorts as 16-bit numbers and its uvs' as 32-bit ones xored, and a
    /// color for each corner (warmer toward its top, more opaque toward its
    /// right) in 8 bits through the color filter, kept in 7.
    pub fn ball_v1_glb() -> Vec<u8> {
        let (pos, normals, uvs, indices) = ball(12, 24);
        let checker: Vec<u8> = (0..16).flat_map(|i| if (i % 4 + i / 4) % 2 == 0 { [220, 30, 30, 255] } else { [240, 240, 240, 255] }).collect();
        let mut g = Glb { khr: true, ..Glb::default() };
        let (lo, step) = ([9.0f32, -4.0, 1.0], 2.0 / 16383.0);
        let q: Vec<u8> = pos.iter().flat_map(|p| [0, 1, 2].map(|k| ((p[k] - lo[k]) / step).round() as u16).into_iter().chain([0]).flat_map(u16::to_le_bytes)).collect();
        let oct: Vec<u8> = normals.iter().flat_map(|&n| super::super::meshopt::tests::octahedral_normal(n, 8, 0).map(|c| c as i8 as u8)).collect();
        let t: Vec<u8> = uvs.iter().flat_map(|u| u.map(|v| (v * 65535.0).round() as u16).into_iter().flat_map(u16::to_le_bytes)).collect();
        let i: Vec<u8> = indices.iter().flat_map(|v| v.to_le_bytes()).collect();
        let colors: Vec<u8> = ball_colors(&pos).iter().flat_map(|c| color_filtered(*c, 7)).collect();
        let (pv, nv, tv, cv) = (g.packed_v1(&q, 8, "NONE", &[1, 1]), g.packed_v1(&oct, 4, "OCTAHEDRAL", &[0]), g.packed_v1(&t, 4, "NONE", &[0x22]), g.packed_v1(&colors, 4, "COLOR", &[0]));
        let iv = g.packed(&i, 2, "TRIANGLES", "NONE");
        let count = pos.len();
        let p = g.accessor(pv, 5123, false, count, "VEC3", r#","min":[0,0,0],"max":[16383,16383,16383]"#);
        let n = g.accessor(nv, 5120, true, count, "VEC3", "");
        let t = g.accessor(tv, 5123, true, count, "VEC2", "");
        let c = g.accessor(cv, 5121, true, count, "VEC4", "");
        let i = g.accessor(iv, 5123, false, indices.len(), "SCALAR", "");
        let image = g.webp(4, 4, &checker);
        let used = r#""KHR_mesh_quantization","KHR_meshopt_compression","EXT_texture_webp""#;
        g.finish(&format!(
            r#""extensionsUsed":[{used}],"extensionsRequired":[{used}],"scene":0,"scenes":[{{"nodes":[0]}}],
            "nodes":[{{"mesh":0,"translation":[{},{},{}],"scale":[{step},{step},{step}]}}],
            "meshes":[{{"primitives":[{{"attributes":{{"POSITION":{p},"NORMAL":{n},"TEXCOORD_0":{t},"COLOR_0":{c}}},"indices":{i},"material":0}}]}}],
            "materials":[{{"pbrMetallicRoughness":{{"baseColorTexture":{{"index":0}},"metallicFactor":0}}}}],
            "textures":[{{"extensions":{{"EXT_texture_webp":{{"source":0}}}}}}],"images":[{{"bufferView":{image},"mimeType":"image/webp"}}]"#,
            lo[0], lo[1], lo[2]
        ))
    }

    /// A color for each of the ball's corners, RGBA from 0 to 1: warmer
    /// toward its top, more opaque toward its right.
    fn ball_colors(pos: &[[f32; 3]]) -> Vec<[f32; 4]> {
        pos.iter().map(|p| [0.5 + 0.5 * (p[1] + 3.0), 0.4, 0.5 - 0.5 * (p[1] + 3.0), 0.5 + 0.5 * (p[0] - 10.0)]).collect()
    }

    /// An RGBA color through meshoptimizer's color filter in 8 bits, as
    /// its `meshopt_encodeFilterColor` keeps it in `bits`.
    fn color_filtered(c: [f32; 4], bits: u32) -> [u8; 4] {
        let q = |v: f32| (v.clamp(0.0, 1.0) * ((1 << bits) - 1) as f32 + 0.5) as i32;
        let (r, g, b) = (q(c[0]), q(c[1]), q(c[2]));
        let co = (r - b) / 2;
        let cg = (g - (b + co)) / 2;
        let y = b + co + cg;
        [y as u8, co as u8, cg as u8, ((q(c[3]) >> 1) | 1 << (bits - 1)) as u8]
    }

    #[test]
    fn a_ball_packed_with_meshopts_version_1_reads_as_the_plain_one_does_and_its_colors_through_their_filter() {
        let (plain, packed) = (load(Format::Glb, &ball_glb(false)).unwrap(), load(Format::Glb, &ball_v1_glb()).expect("Flow reads what it requires"));
        let (a, b) = (&plain.parts[0], &packed.parts[0]);
        assert!(same_triangles(&a.indices, &b.indices));
        let close = |x: &[f32], y: &[f32], by: f32| x.iter().zip(y).all(|(p, q)| (p - q).abs() < by);
        for (v, w) in a.vertices.iter().zip(&b.vertices) {
            assert!(close(&v.pos, &w.pos, 1e-4) && close(&v.normal, &w.normal, 0.02) && close(&v.uv, &w.uv, 1e-4), "{v:?} {w:?}");
        }
        // The colors come back as they were kept, within their seven bits,
        // and each vertex holds its own as shown; the plain ball has none.
        let (doc, buffers) = import(&ball_v1_glb()).unwrap();
        let accessor = doc.meshes().next().unwrap().primitives().next().unwrap().get(&gltf::Semantic::Colors(0)).unwrap();
        let colors = floats::<4>(&accessor, &buffers).unwrap();
        let (pos, ..) = ball(12, 24);
        for ((got, want), v) in colors.iter().zip(ball_colors(&pos)).zip(&b.vertices) {
            assert!(close(got, &want, 2.0 / 127.0), "{got:?} {want:?}");
            assert_eq!(v.color, vertex_color(*got));
        }
        assert!(a.vertices.iter().all(|v| v.color == [255; 4]));
    }

    /// The ball's corners, normals and uvs, and a color for each corner,
    /// as a Draco mesh: positions kept in 14 bits, normals in 10, uvs in
    /// 12, colors as bytes, the triangles by Draco's `method` (0 one by
    /// one, 1 edgebreaker), with draco-core's encoder. Its attributes' ids,
    /// as the glTF names them: 0 to 3.
    pub fn draco_ball(method: i32) -> Vec<u8> {
        use draco_core::{DataType, EncoderBuffer, EncoderOptions, GeometryAttributeType, MeshEncoder, PointAttribute, PointIndex};
        let (pos, normals, uvs, indices) = ball(12, 24);
        let colors: Vec<[u8; 4]> = ball_colors(&pos).iter().map(|c| c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)).collect();
        let mut mesh = draco_core::Mesh::new();
        let mut attribute = |kind: GeometryAttributeType, components: u8, data: DataType, normalized: bool, bytes: Vec<u8>| {
            let mut a = PointAttribute::new();
            a.init(kind, components, data, normalized, pos.len());
            a.buffer_mut().write(0, &bytes);
            mesh.add_attribute(a);
        };
        attribute(GeometryAttributeType::Position, 3, DataType::Float32, false, pos.concat().iter().flat_map(|v| v.to_le_bytes()).collect());
        attribute(GeometryAttributeType::Normal, 3, DataType::Float32, false, normals.concat().iter().flat_map(|v| v.to_le_bytes()).collect());
        attribute(GeometryAttributeType::TexCoord, 2, DataType::Float32, false, uvs.concat().iter().flat_map(|v| v.to_le_bytes()).collect());
        attribute(GeometryAttributeType::Color, 4, DataType::Uint8, true, colors.concat());
        for t in indices.as_chunks::<3>().0 {
            mesh.add_face(t.map(|i| PointIndex(u32::from(i))));
        }
        let mut options = EncoderOptions::new();
        options.set_global_int("encoding_method", method);
        for (id, bits) in [(0, 14), (1, 10), (2, 12)] {
            options.set_attribute_int(id, "quantization_bits", bits);
        }
        let mut encoder = MeshEncoder::new();
        encoder.set_mesh(mesh);
        let mut out = EncoderBuffer::new();
        encoder.encode(&options, &mut out).expect("encoded");
        out.data().to_vec()
    }

    /// A glTF of `draco` (a Draco mesh of `ball`'s, as `draco_ball` makes
    /// one), its accessors saying what it decodes to and the file
    /// requiring KHR_draco_mesh_compression; `ids` the attributes' Draco ids.
    pub fn draco_glb(draco: &[u8], ids: &str) -> Vec<u8> {
        let (pos, ..) = ball(12, 24);
        let (count, corners) = (pos.len(), 12 * 24 * 6);
        let mut g = Glb::default();
        let view = g.view(draco);
        let p = g.accessor_without_view(5126, false, count, "VEC3", r#","min":[9,-4,1],"max":[11,-2,3]"#);
        let n = g.accessor_without_view(5126, false, count, "VEC3", "");
        let t = g.accessor_without_view(5126, false, count, "VEC2", "");
        // Colors only where the Draco mesh has them: an accessor without
        // a view that nothing decodes into isn't a glTF's.
        let c = ids.contains("COLOR_0").then(|| g.accessor_without_view(5121, true, count, "VEC4", ""));
        let colors = c.map_or(String::new(), |c| format!(r#","COLOR_0":{c}"#));
        let i = g.accessor_without_view(5123, false, corners, "SCALAR", "");
        let checker: Vec<u8> = (0..16).flat_map(|i| if (i % 4 + i / 4) % 2 == 0 { [220, 30, 30, 255] } else { [240, 240, 240, 255] }).collect();
        let image = g.png(4, 4, &checker);
        g.finish(&format!(
            r#""extensionsUsed":["KHR_draco_mesh_compression"],"extensionsRequired":["KHR_draco_mesh_compression"],"scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0}}],
            "meshes":[{{"primitives":[{{"attributes":{{"POSITION":{p},"NORMAL":{n},"TEXCOORD_0":{t}{colors}}},"indices":{i},"material":0,
                "extensions":{{"KHR_draco_mesh_compression":{{"bufferView":{view},"attributes":{{{ids}}}}}}}}}]}}],
            "materials":[{{"pbrMetallicRoughness":{{"baseColorTexture":{{"index":0}},"metallicFactor":0}}}}],
            "textures":[{{"source":0}}],"images":[{{"bufferView":{image},"mimeType":"image/png"}}]"#
        ))
    }

    /// The Draco ids `draco_ball`'s attributes have.
    pub const DRACO_IDS: &str = r#""POSITION":0,"NORMAL":1,"TEXCOORD_0":2,"COLOR_0":3"#;

    /// Whether `mesh` is `ball`'s ball as drawn: each corner on the ball
    /// (half a unit from the middle of its box), its normal straight out,
    /// and its uvs where the ball's are for that normal (u round it, v
    /// down from the top), each within `by`; and its triangles all there,
    /// those of any size at least (an edgebreaker leaves out the poles'
    /// triangles of none, and quantized, two of them may come out slivers).
    fn is_the_ball(mesh: &super::super::Mesh, by: f32) -> bool {
        let p = &mesh.parts[0];
        let (tau, pi) = (std::f32::consts::TAU, std::f32::consts::PI);
        let sized = p.indices.as_chunks::<3>().0.iter().filter(|t| {
            let [a, b, c] = t.map(|i| p.vertices[i as usize].pos);
            super::super::length(cross(sub(b, a), sub(c, a))) > 1e-6
        });
        (12 * 24 * 2 - 2 * 24..=12 * 24 * 2).contains(&sized.count())
            && p.indices.len() <= 12 * 24 * 6
            && p.vertices.iter().all(|v| {
                let n = v.pos.map(|c| c / 0.5);
                let seen = [(v.uv[0] * tau).cos() * (v.uv[1] * pi).sin(), (v.uv[1] * pi).cos(), (v.uv[0] * tau).sin() * (v.uv[1] * pi).sin()];
                (0..3).all(|k| (n[k] - v.normal[k]).abs() < by && (n[k] - seen[k]).abs() < by)
            })
    }

    #[test]
    fn a_draco_mesh_reads_as_its_ball_by_either_of_dracos_ways_with_its_colors() {
        for method in [0, 1] {
            let file = draco_glb(&draco_ball(method), DRACO_IDS);
            let mesh = load(Format::Glb, &file).unwrap_or_else(|e| panic!("method {method}: {e}"));
            assert!(is_the_ball(&mesh, 0.01), "method {method}");
            assert!(mesh.parts[0].material.texture.is_some());
            // The colors as bytes again, each vertex holding its own as shown.
            let (doc, buffers) = import(&file).unwrap();
            let primitive = doc.meshes().next().unwrap().primitives().next().unwrap();
            let colors = floats::<4>(&primitive.get(&gltf::Semantic::Colors(0)).unwrap(), &buffers).unwrap();
            let corners = floats::<3>(&primitive.get(&gltf::Semantic::Positions).unwrap(), &buffers).unwrap();
            for ((c, p), v) in colors.iter().zip(&corners).zip(&mesh.parts[0].vertices) {
                let want = ball_colors(&[*p])[0].map(|v| (v.clamp(0.0, 1.0) * 255.0).round() / 255.0);
                assert!(c.iter().zip(want).all(|(a, b)| (a - b).abs() < 1.5 / 255.0), "{c:?} at {p:?}, not {want:?}");
                assert_eq!(v.color, vertex_color(*c), "at {p:?}");
            }
        }
        // An id the Draco mesh hasn't, or bytes that aren't Draco's.
        let missing = draco_glb(&draco_ball(1), r#""POSITION":0,"NORMAL":7"#);
        assert_eq!(load(Format::Glb, &missing).unwrap_err(), "mesh 0's Draco data has no attribute 7 for its NORMAL");
        let mut cut = draco_ball(1);
        cut.truncate(cut.len() / 2);
        assert!(load(Format::Glb, &draco_glb(&cut, DRACO_IDS)).unwrap_err().starts_with("mesh 0's Draco data: "));
        assert!(load(Format::Glb, &draco_glb(b"not Draco at all", DRACO_IDS)).unwrap_err().starts_with("mesh 0's Draco data: "));
    }

    /// Google's own encoder's Draco files of the ball (`testdata`), at
    /// compression levels 0, 7 and 10.
    pub const GOOGLES_DRACO: [&[u8]; 3] = [include_bytes!("testdata/ball-cl0.drc"), include_bytes!("testdata/ball-cl7.drc"), include_bytes!("testdata/ball-cl10.drc")];
    /// Their attributes' Draco ids.
    pub const GOOGLES_IDS: &str = r#""POSITION":0,"TEXCOORD_0":1,"NORMAL":2"#;

    #[test]
    fn draco_meshes_googles_encoder_made_read_as_the_ball_they_were() {
        for (level, file) in [0, 7, 10].into_iter().zip(GOOGLES_DRACO) {
            let mesh = load(Format::Glb, &draco_glb(file, GOOGLES_IDS)).unwrap_or_else(|e| panic!("level {level}: {e}"));
            assert!(is_the_ball(&mesh, 0.01), "level {level}");
        }
    }

    /// Binomial's own encoder's KTX2 files of a 16 × 16 texture checked red
    /// and white in squares of 4 (`testdata`): ETC1S (BasisLZ), and UASTC
    /// (Zstandard, mipmapped).
    pub const BASIS_CHECKERS: [&[u8]; 2] = [include_bytes!("testdata/checker-etc1s.ktx2"), include_bytes!("testdata/checker-uastc.ktx2")];

    /// `ball_glb`'s ball in plain floats, its texture the KTX2 `ktx2`
    /// alone (KHR_texture_basisu, required) or with a blue PNG to fall
    /// back on.
    pub fn basis_ball_glb(ktx2: &[u8], fallback: bool) -> Vec<u8> {
        let (pos, normals, uvs, indices) = ball(12, 24);
        let mut g = Glb::default();
        let (p, n, t, i) = (g.floats(&pos.concat(), 3), g.floats(&normals.concat(), 3), g.floats(&uvs.concat(), 2), g.shorts(&indices, 1));
        let (picture, blue) = (g.view(ktx2), g.png(1, 1, &[0, 0, 255, 255]));
        let (used, source) = if fallback { (r#""extensionsUsed":["KHR_texture_basisu"]"#, r#""source":1,"#) } else { (r#""extensionsUsed":["KHR_texture_basisu"],"extensionsRequired":["KHR_texture_basisu"]"#, "") };
        g.finish(&format!(
            r#"{used},"scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0}}],
            "meshes":[{{"primitives":[{{"attributes":{{"POSITION":{p},"NORMAL":{n},"TEXCOORD_0":{t}}},"indices":{i},"material":0}}]}}],
            "materials":[{{"pbrMetallicRoughness":{{"baseColorTexture":{{"index":0}},"metallicFactor":0}}}}],
            "textures":[{{{source}"extensions":{{"KHR_texture_basisu":{{"source":0}}}}}}],
            "images":[{{"bufferView":{picture},"mimeType":"image/ktx2"}},{{"bufferView":{blue},"mimeType":"image/png"}}]"#
        ))
    }

    #[test]
    fn basis_textures_binomials_encoder_made_are_read_before_their_fallback() {
        let red = [220, 30, 30, 255];
        let white = [240, 240, 240, 255];
        for (kind, file) in ["ETC1S", "UASTC"].into_iter().zip(BASIS_CHECKERS) {
            for fallback in [false, true] {
                let mesh = load(Format::Glb, &basis_ball_glb(file, fallback)).unwrap_or_else(|e| panic!("{kind}: {e}"));
                let t = mesh.parts[0].material.texture.clone().unwrap_or_else(|| panic!("{kind}: a texture"));
                // Its largest level, near enough the checker it was made of.
                assert_eq!((t.width, t.height), (16, 16), "{kind}");
                for (k, pixel) in t.rgba.as_chunks::<4>().0.iter().enumerate() {
                    let (x, y) = (k % 16, k / 16);
                    let want = if (x / 4 + y / 4) % 2 == 0 { red } else { white };
                    assert!(pixel.iter().zip(want).all(|(a, b)| a.abs_diff(b) <= 12), "{kind}: ({x}, {y}) is {pixel:?}, not {want:?}");
                }
            }
        }
        // A KTX2 that isn't Basis's (here cut short) shows the PNG beside it.
        let cut = &BASIS_CHECKERS[0][..200];
        let mesh = load(Format::Glb, &basis_ball_glb(cut, true)).unwrap();
        assert_eq!(mesh.parts[0].material.texture.as_ref().map(|t| t.rgba.clone()), Some(vec![0, 0, 255, 255]));
    }

    /// The same triangles, whichever corner each starts at.
    fn same_triangles(a: &[u32], b: &[u32]) -> bool {
        a.len() == b.len() && a.as_chunks::<3>().0.iter().zip(b.as_chunks::<3>().0).all(|(x, y)| [*y, [y[1], y[2], y[0]], [y[2], y[0], y[1]]].contains(x))
    }

    #[test]
    fn a_packed_ball_reads_as_the_plain_one_does() {
        let (plain, packed) = (load(Format::Glb, &ball_glb(false)).unwrap(), load(Format::Glb, &ball_glb(true)).unwrap());
        let (a, b) = (&plain.parts[0], &packed.parts[0]);
        assert_eq!(a.vertices.len(), b.vertices.len());
        assert!(same_triangles(&a.indices, &b.indices), "the triangles, as the index codec turns them");
        let close = |x: &[f32], y: &[f32], by: f32| x.iter().zip(y).all(|(p, q)| (p - q).abs() < by);
        for (v, w) in a.vertices.iter().zip(&b.vertices) {
            // Placed in the same box by the node that undoes the quantizing.
            assert!(close(&v.pos, &w.pos, 1e-4), "{:?} {:?}", v.pos, w.pos);
            assert!(close(&v.normal, &w.normal, 0.02), "{:?} {:?}", v.normal, w.normal);
            assert!(close(&v.uv, &w.uv, 1e-4), "{:?} {:?}", v.uv, w.uv);
            assert!((super::super::length(w.normal) - 1.0).abs() < 1e-5);
        }
        assert!(close(&plain.extent, &packed.extent, 1e-4) && (plain.radius - packed.radius).abs() < 1e-4);
        let texture = |m: &super::super::Mesh| m.parts[0].material.texture.as_ref().map(|t| (t.width, t.height, t.rgba.clone()));
        assert!(texture(&plain).is_some());
        assert_eq!(texture(&packed), texture(&plain), "the WebP's pixels, the PNG's");
    }

    /// A triangle with a normal map, a second set of uvs and a morph
    /// target that an animation steps up at half a second: as floats, or
    /// quantized as KHR_mesh_quantization allows: its corners unsigned
    /// shorts that its node scales to a unit, its normals normalized bytes,
    /// its uvs normalized unsigned bytes, its tangents normalized shorts,
    /// its second uvs and its morph target plain shorts, each view with a
    /// stride that keeps them to whole words.
    fn quantized_triangle(quantized: bool) -> Vec<u8> {
        let mut g = Glb::default();
        let (p, n, t, tan, t1, up, scale) = if quantized {
            let mut strided = |values: &[i32], size: usize, width: usize, stride: usize| -> usize {
                let bytes: Vec<u8> = values
                    .chunks(width)
                    .flat_map(|e| {
                        let mut out: Vec<u8> = e.iter().flat_map(|&c| if size == 1 { vec![c as u8] } else { (c as u16).to_le_bytes().to_vec() }).collect();
                        out.resize(stride, 0);
                        out
                    })
                    .collect();
                let view = g.view(&bytes);
                let end = g.views[view].len() - 1;
                g.views[view].insert_str(end, &format!(r#","byteStride":{stride}"#));
                view
            };
            let (pos, normals, uvs) = (strided(&[0, 0, 0, 1000, 0, 0, 0, 1000, 0], 2, 3, 8), strided(&[0, 0, 127, 0, 0, 127, 0, 0, 127], 1, 3, 4), strided(&[0, 255, 255, 255, 0, 0], 1, 2, 4));
            let tangents = strided(&[32767, 0, 0, 32767, 32767, 0, 0, -32767, 32767, 0, 0, 32767], 2, 4, 8);
            let (uvs1, lift) = (strided(&[-2, 0, 4, 8, 0, 6], 2, 2, 4), strided(&[0, 0, 500, 0, 0, 500, 0, 0, 500], 2, 3, 8));
            (
                g.accessor(pos, 5123, false, 3, "VEC3", r#","min":[0,0,0],"max":[1000,1000,0]"#),
                g.accessor(normals, 5120, true, 3, "VEC3", ""),
                g.accessor(uvs, 5121, true, 3, "VEC2", ""),
                g.accessor(tangents, 5122, true, 3, "VEC4", ""),
                g.accessor(uvs1, 5122, false, 3, "VEC2", ""),
                g.accessor(lift, 5122, false, 3, "VEC3", ""),
                0.001,
            )
        } else {
            (
                g.floats(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0], 3),
                g.floats(&[0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0], 3),
                g.floats(&[0.0, 1.0, 1.0, 1.0, 0.0, 0.0], 2),
                g.floats(&[1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, -1.0, 1.0, 0.0, 0.0, 1.0], 4),
                g.floats(&[-2.0, 0.0, 4.0, 8.0, 0.0, 6.0], 2),
                g.floats(&[0.0, 0.0, 0.5, 0.0, 0.0, 0.5, 0.0, 0.0, 0.5], 3),
                1.0,
            )
        };
        let i = g.shorts(&[0, 1, 2], 1);
        let (times, steps) = (g.floats(&[0.0, 0.5], 1), g.floats(&[0.0, 1.0], 1));
        let bumps = g.png(1, 1, &[128, 128, 255, 255]);
        g.finish(&format!(
            r#""extensionsUsed":["KHR_mesh_quantization"],"extensionsRequired":["KHR_mesh_quantization"],"scene":0,"scenes":[{{"nodes":[0]}}],
            "nodes":[{{"mesh":0,"scale":[{scale},{scale},{scale}]}}],
            "meshes":[{{"primitives":[{{"attributes":{{"POSITION":{p},"NORMAL":{n},"TEXCOORD_0":{t},"TEXCOORD_1":{t1},"TANGENT":{tan}}},"indices":{i},"material":0,"targets":[{{"POSITION":{up}}}]}}],"weights":[0]}}],
            "materials":[{{"normalTexture":{{"index":0}}}}],"textures":[{{"source":0}}],"images":[{{"bufferView":{bumps},"mimeType":"image/png"}}],
            "animations":[{{"samplers":[{{"input":{times},"output":{steps},"interpolation":"STEP"}}],"channels":[{{"sampler":0,"target":{{"node":0,"path":"weights"}}}}]}}]"#
        ))
    }

    #[test]
    fn a_quantized_mesh_reads_as_its_floats() {
        let (floats, quantized) = (load(Format::Glb, &quantized_triangle(false)).unwrap(), load(Format::Glb, &quantized_triangle(true)).unwrap());
        let close = |a: &[f32], b: &[f32]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-4);
        let same = |a: &[Vertex], b: &[Vertex]| {
            a.len() == b.len() && a.iter().zip(b).all(|(v, w)| close(&v.pos, &w.pos) && close(&v.normal, &w.normal) && close(&v.uv, &w.uv) && close(&v.uv1, &w.uv1) && close(&v.tangent, &w.tangent))
        };
        assert!(same(&floats.parts[0].vertices, &quantized.parts[0].vertices), "{:?}\n{:?}", floats.parts[0].vertices, quantized.parts[0].vertices);
        assert_eq!(quantized.parts[0].vertices[1].uv1, [4.0, 8.0], "plain shorts as their numbers");
        assert_eq!(quantized.parts[0].vertices[1].tangent[3], -1.0);
        // The morph target, in shorts, lifts it as far as the floats do.
        let (a, b) = (floats.posed(0, 0.5).unwrap(), quantized.posed(0, 0.5).unwrap());
        assert!(same(&a.vertices[0], &b.vertices[0]), "{:?}\n{:?}", a.vertices[0], b.vertices[0]);
        assert!(a.vertices[0][0].pos[2] > floats.parts[0].vertices[0].pos[2] + 0.4, "lifted");
    }

    #[test]
    fn meshopt_compressed_animations_unpack_through_their_filters() {
        // A triangle a node turns a quarter about z and slides along x over
        // a second: its keys' times and slide packed through the
        // exponential filter, its turn as normalized shorts through the
        // quaternion filter, as gltfpack packs them.
        let animated = |packed: bool| {
            let mut g = Glb::default();
            let pos = g.floats(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0], 3);
            let idx = g.shorts(&[0, 1, 2], 1);
            let h = std::f32::consts::FRAC_1_SQRT_2;
            let (times, slide, turn) = if packed {
                // A mantissa in the low 24 bits, the exponent in the high 8.
                let exp = |m: i32, e: i32| ((e << 24) | (m & 0xff_ffff)) as u32;
                let words = |w: &[u32]| -> Vec<u8> { w.iter().flat_map(|v| v.to_le_bytes()).collect() };
                let t = g.packed(&words(&[exp(0, 0), exp(1, 0)]), 4, "ATTRIBUTES", "EXPONENTIAL");
                let s = g.packed(&words(&[exp(0, 0), exp(0, 0), exp(0, 0), exp(1, 1), exp(0, 0), exp(0, 0)]), 12, "ATTRIBUTES", "EXPONENTIAL");
                // The identity, w left out; a quarter about z, z left out,
                // then w, x and y in turn.
                let q: Vec<u8> = [0i16, 0, 0, 32767, 32767, 0, 0, 32764 | 2].iter().flat_map(|v| v.to_le_bytes()).collect();
                let r = g.packed(&q, 8, "ATTRIBUTES", "QUATERNION");
                (g.accessor(t, 5126, false, 2, "SCALAR", r#","min":[0],"max":[1]"#), g.accessor(s, 5126, false, 2, "VEC3", ""), g.accessor(r, 5122, true, 2, "VEC4", ""))
            } else {
                (g.floats(&[0.0, 1.0], 1), g.floats(&[0.0, 0.0, 0.0, 2.0, 0.0, 0.0], 3), g.floats(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, h, h], 4))
            };
            g.finish(&format!(
                r#""extensionsUsed":["EXT_meshopt_compression"],"scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0}}],
                "meshes":[{{"primitives":[{{"attributes":{{"POSITION":{pos}}},"indices":{idx}}}]}}],
                "animations":[{{"samplers":[{{"input":{times},"output":{slide},"interpolation":"LINEAR"}},{{"input":{times},"output":{turn},"interpolation":"LINEAR"}}],
                    "channels":[{{"sampler":0,"target":{{"node":0,"path":"translation"}}}},{{"sampler":1,"target":{{"node":0,"path":"rotation"}}}}]}}]"#
            ))
        };
        let (plain, packed) = (load(Format::Glb, &animated(false)).unwrap(), load(Format::Glb, &animated(true)).unwrap());
        assert_eq!(packed.rig.as_ref().unwrap().clips[0].duration, 1.0);
        for t in [0.0, 0.3, 0.5, 1.0] {
            let (a, b) = (plain.posed(0, t).unwrap(), packed.posed(0, t).unwrap());
            for (v, w) in a.vertices[0].iter().zip(&b.vertices[0]) {
                assert!(v.pos.iter().zip(w.pos).all(|(x, y)| (x - y).abs() < 1e-4), "{t}s: {:?} {:?}", v.pos, w.pos);
            }
        }
    }

    /// Three triangles, their textures a WebP (`webp` writes it) with a
    /// red PNG to fall back on, the WebP alone, and the PNG alone.
    fn webp_glb(webp: impl Fn(&mut Glb) -> usize) -> Vec<u8> {
        let mut g = Glb::default();
        let pos = g.floats(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0], 3);
        let uv = g.floats(&[0.0, 0.0, 1.0, 0.0, 0.0, 1.0], 2);
        let (picture, red) = (webp(&mut g), g.png(1, 1, &[255, 0, 0, 255]));
        let primitive = |k: usize| format!(r#"{{"attributes":{{"POSITION":{pos},"TEXCOORD_0":{uv}}},"material":{k}}}"#);
        let material = |k: usize| format!(r#"{{"pbrMetallicRoughness":{{"baseColorTexture":{{"index":{k}}}}}}}"#);
        g.finish(&format!(
            r#""extensionsUsed":["EXT_texture_webp"],"scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0}}],
            "meshes":[{{"primitives":[{},{},{}]}}],"materials":[{},{},{}],
            "textures":[{{"source":1,"extensions":{{"EXT_texture_webp":{{"source":0}}}}}},{{"extensions":{{"EXT_texture_webp":{{"source":0}}}}}},{{"source":1}}],
            "images":[{{"bufferView":{picture},"mimeType":"image/webp"}},{{"bufferView":{red},"mimeType":"image/png"}}]"#,
            primitive(0),
            primitive(1),
            primitive(2),
            material(0),
            material(1),
            material(2)
        ))
    }

    #[test]
    fn a_webp_texture_is_read_before_its_fallback_and_its_fallback_where_it_wont_decode() {
        let color = |m: &super::super::Mesh, k: usize| m.parts[k].material.texture.as_ref().map(|t| [t.rgba[0], t.rgba[1], t.rgba[2], t.rgba[3]]);
        let colors = |m: &super::super::Mesh| [color(m, 0), color(m, 1), color(m, 2)];
        let good = load(Format::Glb, &webp_glb(|g| g.webp(1, 1, &[0, 255, 0, 128]))).unwrap();
        assert_eq!(colors(&good), [Some([0, 255, 0, 128]), Some([0, 255, 0, 128]), Some([255, 0, 0, 255])], "the WebP first, alone, and a plain PNG");
        // A WebP that won't decode: the PNG where there is one, else no
        // texture, and the model all the same.
        let broken = load(Format::Glb, &webp_glb(|g| g.view(b"RIFF\x14\0\0\0WEBPVP8 \x08\0\0\0nonsense"))).unwrap();
        assert_eq!(colors(&broken), [Some([255, 0, 0, 255]), None, Some([255, 0, 0, 255])]);
        // A KTX2 picture (KHR_texture_basisu, which Flow doesn't read) with
        // a PNG to fall back on shows the PNG, where the file doesn't
        // require the KTX2.
        let mut g = Glb::default();
        let pos = g.floats(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0], 3);
        let (ktx, png) = (g.view(b"\xabKTX 20\xbb\r\n\x1a\nnot really"), g.png(1, 1, &[0, 0, 255, 255]));
        let basis = g.finish(&format!(
            r#""extensionsUsed":["KHR_texture_basisu"],"scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0}}],
            "meshes":[{{"primitives":[{{"attributes":{{"POSITION":{pos}}},"material":0}}]}}],
            "materials":[{{"pbrMetallicRoughness":{{"baseColorTexture":{{"index":0}}}}}}],
            "textures":[{{"source":1,"extensions":{{"KHR_texture_basisu":{{"source":0}}}}}}],
            "images":[{{"bufferView":{ktx},"mimeType":"image/ktx2"}},{{"bufferView":{png},"mimeType":"image/png"}}]"#
        ));
        assert_eq!(color(&load(Format::Glb, &basis).unwrap(), 0), Some([0, 0, 255, 255]));
    }

    #[test]
    fn a_file_with_meshopt_data_flow_cant_unpack_is_refused_saying_why() {
        let mut g = Glb::default();
        let raw: Vec<u8> = [0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0].iter().flat_map(|f| f.to_le_bytes()).collect();
        let view = g.packed(&raw, 12, "ATTRIBUTES", "NONE");
        g.accessor(view, 5126, false, 3, "VEC3", r#","min":[0,0,0],"max":[1,1,0]"#);
        let rest = r#""scene":0,"scenes":[{"nodes":[0]}],"nodes":[{"mesh":0}],"meshes":[{"primitives":[{"attributes":{"POSITION":0}}]}]"#;
        let copy = |g: &Glb| Glb { bin: g.bin.clone(), views: g.views.clone(), accessors: g.accessors.clone(), fallback: g.fallback, khr: g.khr };
        // Its packed bytes' header (the first) of a vertex codec newer
        // than version 1.
        let mut newer = copy(&g);
        newer.bin[0] = 0xa2;
        assert_eq!(load(Format::Glb, &newer.finish(rest)).unwrap_err(), "buffer view 0: meshopt's vertex codec version 2, which Flow doesn't read");
        // A filter of no name meshopt has, or one on elements it can't be.
        let mut unknown = copy(&g);
        unknown.views[0] = unknown.views[0].replace(r#""NONE""#, r#""SPHERICAL""#);
        assert_eq!(load(Format::Glb, &unknown.finish(rest)).unwrap_err(), "meshopt's SPHERICAL filter, which Flow doesn't read");
        let mut colors = copy(&g);
        colors.views[0] = colors.views[0].replace(r#""NONE""#, r#""COLOR""#);
        assert_eq!(load(Format::Glb, &colors.finish(rest)).unwrap_err(), "buffer view 0: meshopt's Color filter on elements 12 bytes long");
        let mut triangles = copy(&g);
        triangles.views[0] = triangles.views[0].replace(r#""mode":"ATTRIBUTES""#, r#""mode":"TRIANGLES""#);
        assert_eq!(load(Format::Glb, &triangles.finish(rest)).unwrap_err(), "buffer view 0: meshopt triangles of 3 indices 12 bytes each");
        assert!(load(Format::Glb, &g.finish(rest)).is_ok());
    }


    /// Three squares a unit wide facing +z, red, green and blue (nodes 0,
    /// 1 and 2), each further off than the last (z 0, -3 and -9) and
    /// further up and right, and a camera (node 3) three units in front of
    /// the first's plane, looking down -z: through it the green one is half
    /// as wide as the red, and the blue a quarter. `orthographic`, its
    /// camera says nothing of how near things are.
    pub fn perspective_glb(orthographic: bool) -> Vec<u8> {
        let mut g = Glb::default();
        let pos = g.floats(&[-0.5, -0.5, 0.0, 0.5, -0.5, 0.0, 0.5, 0.5, 0.0, -0.5, 0.5, 0.0], 3);
        let idx = g.shorts(&[0, 1, 2, 0, 2, 3], 1);
        let camera = if orthographic {
            r#"{"type":"orthographic","orthographic":{"xmag":2,"ymag":2,"znear":0.1,"zfar":50}}"#
        } else {
            r#"{"type":"perspective","perspective":{"yfov":0.8,"znear":0.1,"aspectRatio":1.5}}"#
        };
        g.finish(&format!(
            r#""cameras":[{camera}],"scene":0,"scenes":[{{"nodes":[0,1,2,3]}}],
            "nodes":[{{"mesh":0,"translation":[-0.55,-0.45,0]}},{{"mesh":1,"translation":[0.5,0.5,-3]}},{{"mesh":2,"translation":[3,3,-9]}},{{"camera":0,"translation":[0,0,3]}}],
            "meshes":[{{"primitives":[{{"attributes":{{"POSITION":{pos}}},"indices":{idx},"material":0}}]}},{{"primitives":[{{"attributes":{{"POSITION":{pos}}},"indices":{idx},"material":1}}]}},
                {{"primitives":[{{"attributes":{{"POSITION":{pos}}},"indices":{idx},"material":2}}]}}],
            "materials":[{{"pbrMetallicRoughness":{{"baseColorFactor":[1,0,0,1],"metallicFactor":0}},"extensions":{{"KHR_materials_unlit":{{}}}}}},
                {{"pbrMetallicRoughness":{{"baseColorFactor":[0,1,0,1],"metallicFactor":0}},"extensions":{{"KHR_materials_unlit":{{}}}}}},
                {{"pbrMetallicRoughness":{{"baseColorFactor":[0,0,1,1],"metallicFactor":0}},"extensions":{{"KHR_materials_unlit":{{}}}}}}]"#
        ))
    }

    /// A unit cube, each face its own color, and a camera close in front
    /// of one corner, looking at its middle from above right (its node a
    /// matrix): through it, the near corner's edges are longer than the far
    /// ones.
    pub fn perspective_cube_glb(orthographic: bool) -> Vec<u8> {
        let mut g = Glb::default();
        let (mut pos, mut normals, mut idx) = (Vec::new(), Vec::new(), Vec::new());
        for axis in 0..3 {
            for side in [-1.0f32, 1.0] {
                let n: [f32; 3] = std::array::from_fn(|k| if k == axis { side } else { 0.0 });
                let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
                let base = (pos.len() / 3) as u16;
                for (a, b) in [(-0.5, -0.5), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)] {
                    let mut p = [0.0f32; 3];
                    p[axis] = 0.5 * side;
                    p[u] = a;
                    p[v] = b;
                    pos.extend(p);
                    normals.extend(n);
                }
                idx.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
            }
        }
        let (p, n, i) = (g.floats(&pos, 3), g.floats(&normals, 3), g.shorts(&idx, 1));
        // Looking from `eye` at the middle, y up: the camera's -z toward it.
        let eye = [1.3f32, 1.1, 1.7];
        let f = normalize(sub([0.0; 3], eye));
        let z = f.map(|v| -v);
        let x = normalize(cross([0.0, 1.0, 0.0], z));
        let y = cross(z, x);
        let matrix = [x[0], x[1], x[2], 0.0, y[0], y[1], y[2], 0.0, z[0], z[1], z[2], 0.0, eye[0], eye[1], eye[2], 1.0].map(|v| v.to_string()).join(",");
        let camera = if orthographic {
            r#"{"type":"orthographic","orthographic":{"xmag":1,"ymag":1,"znear":0.1,"zfar":50}}"#
        } else {
            r#"{"type":"perspective","perspective":{"yfov":1.0,"znear":0.05}}"#
        };
        g.finish(&format!(
            r#""cameras":[{camera}],"scene":0,"scenes":[{{"nodes":[0,1]}}],"nodes":[{{"mesh":0}},{{"camera":0,"matrix":[{matrix}]}}],
            "meshes":[{{"primitives":[{{"attributes":{{"POSITION":{p},"NORMAL":{n}}},"indices":{i},"material":0}}]}}],
            "materials":[{{"pbrMetallicRoughness":{{"baseColorFactor":[0.9,0.6,0.25,1],"metallicFactor":0,"roughnessFactor":0.4}}}}]"#
        ))
    }

    /// A plank four units long along x and half a unit thick, and a
    /// camera five units in front of its middle looking at it, whose field
    /// of view (0.46, from the picture's bottom to its top) frames its
    /// length across most of a picture twice as wide as high.
    pub fn plank_glb() -> Vec<u8> {
        let mut g = Glb::default();
        let (mut pos, mut idx) = (Vec::new(), Vec::new());
        for axis in 0..3 {
            for side in [-1.0f32, 1.0] {
                let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
                let base = (pos.len() / 3) as u16;
                for (a, b) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                    let mut p = [0.0f32; 3];
                    (p[axis], p[u], p[v]) = (side, a, b);
                    pos.extend([p[0] * 2.0, p[1] * 0.25, p[2] * 0.25]);
                }
                idx.extend(if side > 0.0 { [base, base + 1, base + 2, base, base + 2, base + 3] } else { [base, base + 2, base + 1, base, base + 3, base + 2] });
            }
        }
        let (p, i) = (g.floats(&pos, 3), g.shorts(&idx, 1));
        g.finish(&format!(
            r#""cameras":[{{"type":"perspective","perspective":{{"yfov":0.46,"znear":0.1}}}}],"scene":0,"scenes":[{{"nodes":[0,1]}}],
            "nodes":[{{"mesh":0}},{{"camera":0,"translation":[0,0,5]}}],
            "meshes":[{{"primitives":[{{"attributes":{{"POSITION":{p}}},"indices":{i},"material":0}}]}}],
            "materials":[{{"pbrMetallicRoughness":{{"baseColorFactor":[0.3,0.7,0.4,1],"metallicFactor":0,"roughnessFactor":0.5}}}}]"#
        ))
    }

    /// A square two units wide facing +z, and a camera eight units in front
    /// that an animation carries to four units in front over a second.
    pub fn dolly_glb() -> Vec<u8> {
        let mut g = Glb::default();
        let pos = g.floats(&[-1.0, -1.0, 0.0, 1.0, -1.0, 0.0, 1.0, 1.0, 0.0, -1.0, 1.0, 0.0], 3);
        let idx = g.shorts(&[0, 1, 2, 0, 2, 3], 1);
        let times = g.floats(&[0.0, 1.0], 1);
        let carry = g.floats(&[0.0, 0.0, 8.0, 0.0, 0.0, 4.0], 3);
        g.finish(&format!(
            r#""cameras":[{{"type":"perspective","perspective":{{"yfov":0.6,"znear":0.1}}}}],"scene":0,"scenes":[{{"nodes":[0,1]}}],
            "nodes":[{{"mesh":0}},{{"camera":0,"translation":[0,0,8]}}],
            "meshes":[{{"primitives":[{{"attributes":{{"POSITION":{pos}}},"indices":{idx}}}]}}],
            "animations":[{{"samplers":[{{"input":{times},"output":{carry},"interpolation":"LINEAR"}}],"channels":[{{"sampler":0,"target":{{"node":1,"path":"translation"}}}}]}}]"#
        ))
    }

    #[test]
    fn a_perspective_camera_is_kept_where_it_stands_in_the_box_and_an_orthographic_one_isnt() {
        let mesh = load(Format::Glb, &perspective_glb(false)).unwrap();
        let lens = mesh.lens.expect("seen through its camera");
        // The box: x from -1.05 to 3.5, y from -0.95 to 3.5, z from -9 to
        // 0; so its middle (1.225, 1.275, -4.5), its longest side 9.
        let close = |a: [f32; 3], b: [f32; 3]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-5);
        assert!(close(lens.eye, [-1.225 / 9.0, -1.275 / 9.0, 7.5 / 9.0]), "{lens:?}");
        assert_eq!(lens.yfov, 0.8);
        assert!((lens.znear - 0.1 / 9.0).abs() < 1e-7);
        assert!(load(Format::Glb, &perspective_glb(true)).unwrap().lens.is_none(), "drawn straight on, as before");
        // A moving camera's eye is where its view puts it: the middle.
        let dolly = load(Format::Glb, &dolly_glb()).unwrap();
        let rig = dolly.rig.as_ref().expect("it moves");
        assert!(rig.camera.is_some());
        let (center, longest) = rig.frame;
        assert!(close(dolly.lens.unwrap().eye, center.map(|c| -c / longest)), "{:?} {:?}", dolly.lens, rig.frame);
    }

    #[test]
    fn a_sparse_accessor_puts_its_values_over_its_view_or_over_zeros() {
        // The corners as unsigned shorts, the second moved from (10, 0, 0)
        // to (20, 0, 0) by a sparse value; and a morph target, at full
        // weight, with no view of its own, lifting only the third by 5.
        let mut g = Glb::default();
        let shorts = |v: &[u16]| -> Vec<u8> { v.iter().flat_map(|c| c.to_le_bytes()).collect() };
        let base = g.view(&shorts(&[0, 0, 0, 0, 10, 0, 0, 0, 0, 10, 0, 0]));
        let end = g.views[base].len() - 1;
        g.views[base].insert_str(end, r#","byteStride":8"#);
        let (at, moved) = (g.view(&[1]), g.view(&shorts(&[20, 0, 0])));
        let (at_lift, lift) = (g.view(&shorts(&[2])), g.view(&shorts(&[0, 0, 5])));
        let pos = g.accessor(base, 5123, false, 3, "VEC3", &format!(r#","min":[0,0,0],"max":[20,10,0],"sparse":{{"count":1,"indices":{{"bufferView":{at},"componentType":5121}},"values":{{"bufferView":{moved}}}}}"#));
        g.accessors.push(format!(r#"{{"componentType":5122,"count":3,"type":"VEC3","sparse":{{"count":1,"indices":{{"bufferView":{at_lift},"componentType":5123}},"values":{{"bufferView":{lift}}}}}}}"#));
        let up = g.accessors.len() - 1;
        let glb = g.finish(&format!(
            r#""extensionsUsed":["KHR_mesh_quantization"],"scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0}}],
            "meshes":[{{"primitives":[{{"attributes":{{"POSITION":{pos}}},"targets":[{{"POSITION":{up}}}]}}],"weights":[1]}}]"#
        ));
        let mesh = load(Format::Glb, &glb).unwrap();
        // In the box from (0, 0, 0) to (20, 10, 5): less its middle, over 20.
        let corners: Vec<[f32; 3]> = mesh.parts[0].vertices.iter().map(|v| v.pos).collect();
        assert_eq!(corners, [[-0.5, -0.25, -0.125], [0.5, -0.25, -0.125], [-0.5, 0.25, 0.125]]);
    }


    #[test]
    fn a_buffer_or_an_image_in_a_data_uri_is_read_and_one_in_a_file_isnt() {
        use base64::Engine as _;
        let b64 = |b: &[u8]| base64::engine::general_purpose::STANDARD.encode(b);
        let floats: Vec<u8> = [0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0].iter().flat_map(|f| f.to_le_bytes()).collect();
        let mut webp = Vec::new();
        image_webp::WebPEncoder::new(&mut webp).encode(&[0, 0, 255, 255], 1, 1, image_webp::ColorType::Rgba8).unwrap();
        let file = |buffer: &str, image: &str| {
            let json = format!(
                r#"{{"asset":{{"version":"2.0"}},"buffers":[{{"byteLength":36,"uri":"{buffer}"}}],"bufferViews":[{{"buffer":0,"byteLength":36}}],
                "accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]}}],
                "scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0}}],"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}},"material":0}}]}}],
                "materials":[{{"pbrMetallicRoughness":{{"baseColorTexture":{{"index":0}}}}}}],"textures":[{{"source":0}}],"images":[{{"uri":"{image}"}}]}}"#
            );
            let mut json = json.into_bytes();
            json.resize(json.len().div_ceil(4) * 4, b' ');
            let mut out = b"glTF".to_vec();
            out.extend(2u32.to_le_bytes());
            out.extend(((12 + 8 + json.len()) as u32).to_le_bytes());
            out.extend((json.len() as u32).to_le_bytes());
            out.extend(b"JSON");
            out.extend(&json);
            out
        };
        let inline = file(&format!("data:application/octet-stream;base64,{}", b64(&floats)), &format!("data:image/webp;base64,{}", b64(&webp)));
        let mesh = load(Format::Glb, &inline).unwrap();
        assert_eq!(mesh.parts[0].material.texture.as_ref().map(|t| t.rgba.clone()), Some(vec![0, 0, 255, 255]));
        // An image in a file is left out; a buffer in one leaves nothing to draw.
        let near = load(Format::Glb, &file(&format!("data:application/octet-stream;base64,{}", b64(&floats)), "texture.png")).unwrap();
        assert!(near.parts[0].material.texture.is_none());
        assert!(load(Format::Glb, &file("mesh.bin", "texture.png")).is_err());
    }


    #[test]
    fn a_glbs_lights_move_as_its_animation_moves_their_nodes() {
        let mesh = load(Format::Glb, &moving_lights_glb()).unwrap();
        let rig = mesh.rig.as_ref().expect("it moves");
        assert!(rig.view.is_some() && rig.camera.is_none(), "its camera stays, so the view is taken once");
        assert_eq!(rig.moves_lights, [true, false]);
        let on = |n: usize| mesh.lights.iter().position(|l| l.node == Some(n)).unwrap();
        let (point, sun, spot) = (on(1), on(2), on(3));
        let close = |a: [f32; 3], b: [f32; 3]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-5);
        // The box: the square, two units wide, lifted half a unit by the
        // second animation; so less (0, 0, 0.25), over 2.
        assert!(close(mesh.lights[point].at.unwrap(), [-0.75, 0.0, 0.375]), "{:?}", mesh.lights[point]);
        let start = mesh.posed(0, 0.0).unwrap().lights.expect("the first animation moves lights");
        assert_eq!(start, mesh.lights, "where they are at rest");
        let end = mesh.posed(0, 1.0).unwrap().lights.unwrap();
        assert!(close(end[point].at.unwrap(), [0.75, 0.0, 0.375]), "carried right: {:?}", end[point]);
        assert_eq!((end[point].strength, end[point].color), (mesh.lights[point].strength, [1.0, 0.0, 0.0]), "giving what it gave at rest");
        assert!(close(end[sun].aim, [-1.0, 0.0, 0.0]) && close(mesh.lights[sun].aim, [0.0, 0.0, -1.0]), "turned a quarter: {:?}", end[sun]);
        assert_eq!(end[spot], mesh.lights[spot], "the spot stays as it was");
        assert!(mesh.posed(1, 0.5).unwrap().lights.is_none(), "the second moves none");
        // Held at rest, nothing is posed: the lights as at rest.
        assert!(mesh.posed(-1, 0.5).is_none());
    }

    #[test]
    fn a_light_a_clip_brings_nearer_is_settled_so_its_brightest_lights_the_model_as_flows_own_light_does() {
        let mesh = load(Format::Glb, &moving_lights_glb()).unwrap();
        // What the lights give the middle, as the shader has them: each
        // its share where it was at rest, more as it comes nearer.
        let near = 0.5 * mesh.radius;
        let given = |now: &[Light]| -> f32 {
            now.iter()
                .zip(&mesh.lights)
                .map(|(l, rest)| match (l.at, rest.at) {
                    (Some(p), Some(r)) => rest.strength * dot(r, r).max(near * near) / dot(p, p).max(near * near),
                    _ => rest.strength,
                })
                .sum()
        };
        let over: Vec<f32> = (0..=200).map(|k| given(&mesh.posed(0, k as f32 / 200.0).unwrap().lights.unwrap())).collect();
        let most = over.iter().copied().fold(0.0, f32::max);
        assert!((most - 1.0).abs() < 1e-3, "at its brightest {most}");
        // Brightest halfway, the red light passing in front of the middle;
        // at rest, with it off to the left, the model is lit the less.
        assert!((over[100] - most).abs() < 1e-3, "{}", over[100]);
        let at_rest: f32 = mesh.lights.iter().map(|l| l.strength).sum();
        assert!((at_rest - over[0]).abs() < 1e-5 && at_rest < 0.5, "{at_rest}");
        // A file whose lights stay shares out what it gives at rest.
        let still: f32 = load(Format::Glb, &lights_glb(true)).unwrap().lights.iter().map(|l| l.strength).sum();
        assert!((still - 1.0).abs() < 1e-5);
    }

    #[test]
    fn a_glbs_camera_moving_with_its_animation_turns_the_view_and_keeps_what_it_looks_at_in_place() {
        let mesh = load(Format::Glb, &orbiting_camera_glb()).unwrap();
        let rig = mesh.rig.as_ref().expect("it moves");
        assert_eq!((rig.camera, rig.view), (Some(2), None), "the view is worked out from each pose");
        let close = |a: [f32; 3], b: [f32; 3]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-4);
        let at = |t: f32| mesh.posed(0, t).unwrap().vertices[0].clone();
        let (start, quarter, end) = (at(0.0), at(0.5), at(1.0));
        // At first face on, as the camera at rest sees it.
        assert!(start.iter().zip(&mesh.parts[0].vertices).all(|(a, b)| close(a.pos, b.pos)), "{start:?}");
        assert!(start.iter().all(|v| close(v.normal, [0.0, 0.0, 1.0]) && (v.pos[2] - start[0].pos[2]).abs() < 1e-5), "{start:?}");
        // The corner it looks at stays where it sees it, all the way round.
        assert!(close(quarter[0].pos, start[0].pos) && close(end[0].pos, start[0].pos), "{:?} {:?} {:?}", start[0].pos, quarter[0].pos, end[0].pos);
        // Gone round to its side: seen edge on, the far corner come nearer.
        assert!(end.iter().all(|v| (v.pos[0] - end[0].pos[0]).abs() < 1e-4 && close(v.normal, [-1.0, 0.0, 0.0])), "{end:?}");
        assert!(end[1].pos[2] > end[0].pos[2] + 0.9, "{end:?}");
    }

    #[test]
    fn an_occlusion_map_is_read_with_its_strength_and_its_set_of_uvs() {
        let mesh = load(Format::Glb, &features_glb()).unwrap();
        let m = &mesh.parts[0].material;
        let (map, strength) = m.occlusion.clone().expect("an occlusion map");
        assert_eq!((map.rgba[0], strength), (64, 0.7));
        assert_eq!(m.maps[4], UvMap { set: 1, ..UvMap::PLAIN }, "read from TEXCOORD_1, unmoved");
        // A material without one is lit as before.
        assert!(load(Format::Glb, &material_glb()).unwrap().parts[0].material.occlusion.is_none());
    }

    #[test]
    fn a_texture_reads_the_second_set_of_uvs_where_it_asks_for_it() {
        let mesh = load(Format::Glb, &features_glb()).unwrap();
        let p = &mesh.parts[0];
        let uv1: Vec<[f32; 2]> = p.vertices.iter().map(|v| v.uv1).collect();
        assert_eq!(uv1, [[0.5, 0.5], [0.75, 0.5], [0.75, 0.25], [0.5, 0.25]]);
        assert_eq!(p.vertices[1].uv, [1.0, 1.0], "and the first set as it was");
        assert_eq!(p.material.maps[0].set, 1, "the transform's own texCoord");
        assert_eq!(p.material.maps[2].set, 0);
        // A file with one set reads it for both.
        let one = load(Format::Glb, &material_glb()).unwrap();
        assert!(one.parts[0].vertices.iter().all(|v| v.uv1 == v.uv));
    }

    #[test]
    fn a_texture_transform_scales_turns_and_offsets_the_uvs() {
        let mesh = load(Format::Glb, &features_glb()).unwrap();
        let p = &mesh.parts[0];
        // (0.75, 0.5) scaled to (1.5, 0.5), turned a quarter counter-clockwise
        // as the texture shows it (u right, v down) to (0.5, -1.5), then
        // moved half a texture right.
        let at = p.material.maps[0].apply(&p.vertices[1]);
        assert!((at[0] - 1.0).abs() < 1e-5 && (at[1] + 1.5).abs() < 1e-5, "{at:?}");
        // The normal map's, kept as JSON by the gltf crate, is read too.
        let bumps = p.material.maps[2];
        assert_eq!(bumps.apply(&p.vertices[1]), [3.0, 3.0]);
        // Its tangents follow the moved uvs: u still runs along x.
        assert!(p.vertices.iter().all(|v| (v.tangent[0] - 1.0).abs() < 1e-5), "{:?}", p.vertices[0].tangent);
    }

    #[test]
    fn emissive_strength_scales_the_emission_before_it_is_shown() {
        let mesh = load(Format::Glb, &features_glb()).unwrap();
        let e = mesh.parts[0].material.emission;
        // [0.25, 0.5, 0] four times over is [1, 2, 0] in linear light.
        assert!((e[0] - 1.0).abs() < 1e-6 && (e[1] - 2f32.powf(1.0 / 2.2)).abs() < 1e-6 && e[2] == 0.0, "{e:?}");
        let plain = load(Format::Glb, &material_glb()).unwrap();
        assert_eq!(plain.parts[0].material.emission, [1.0, 0.5f32.powf(1.0 / 2.2), 0.0], "a factor alone, as before");
    }

    #[test]
    fn alpha_modes_are_read_and_see_through_parts_are_drawn_last_farthest_first() {
        let cut = load(Format::Glb, &features_glb()).unwrap();
        assert_eq!(cut.parts[0].material.alpha, Alpha::Mask(0.3));
        let mesh = load(Format::Glb, &layers_glb()).unwrap();
        let modes: Vec<Alpha> = mesh.parts.iter().map(|p| p.material.alpha).collect();
        assert_eq!(modes, [Alpha::Blend, Alpha::Opaque, Alpha::Blend]);
        // Seen straight on (z toward the viewer): the solid one, then the
        // far one, then the near one over both.
        let order = mesh.draw_order(None, IDENTITY);
        assert_eq!((order.solid, order.runs), (vec![1], vec![(2, 3), (0, 3)]));
        // Turned half round, the far one comes nearest.
        let behind = [[-1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, -1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];
        assert_eq!(mesh.draw_order(None, behind).runs, [(0, 3), (2, 3)]);
    }

    #[test]
    fn a_glb_material_naming_only_a_color_is_rough_metal_as_gltf_says() {
        let mesh = load(Format::Glb, &super::super::tests::triangle_glb()).unwrap();
        let m = &mesh.parts[0].material;
        assert_eq!((m.gloss, m.metallic, m.emission), (0.0, 1.0, [0.0; 3]));
        assert!(m.texture.is_none() && m.metal_rough.is_none() && m.bumps.is_none() && m.glow.is_none());
    }

    #[test]
    fn tangents_follow_the_texture_and_turn_with_a_mirrored_uv() {
        let positions = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let normals = [[0.0, 0.0, 1.0]; 3];
        // The texture laid on as glTF does (v down): tangent +x, w 1.
        let t = tangents(&positions, &normals, &[[0.0, 1.0], [1.0, 1.0], [0.0, 0.0]], &[0, 1, 2]);
        assert!(t.iter().all(|t| *t == [1.0, 0.0, 0.0, 1.0]), "{t:?}");
        // Flipped upside down (v up the triangle): the bitangent goes the other way.
        let t = tangents(&positions, &normals, &[[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]], &[0, 1, 2]);
        assert!(t.iter().all(|t| *t == [1.0, 0.0, 0.0, -1.0]), "{t:?}");
        // Turned a quarter in the texture: u runs up y.
        let t = tangents(&positions, &normals, &[[0.0, 0.0], [0.0, 1.0], [1.0, 0.0]], &[0, 1, 2]);
        assert!((t[0][1] - 1.0).abs() < 1e-6, "{t:?}");
        // No uvs to go by: still across the normal, and unit length.
        let t = tangents(&positions, &normals, &[[0.0, 0.0]; 3], &[0, 1, 2]);
        assert!(t.iter().all(|t| t[2].abs() < 1e-6 && (t[0] * t[0] + t[1] * t[1] - 1.0).abs() < 1e-5));
    }

    #[test]
    fn a_glbs_animation_skin_and_morph_target_are_read_and_played() {
        let mesh = load(Format::Glb, &animated_glb()).unwrap();
        let rig = mesh.rig.as_ref().expect("it moves");
        assert_eq!(rig.clips.len(), 1);
        let kinds: Vec<(Path, Interp)> = rig.clips[0].channels.iter().map(|c| (c.path, c.track.interp)).collect();
        assert_eq!(kinds, [(Path::Translation, Interp::Linear), (Path::Weights, Interp::Step), (Path::Scale, Interp::Cubic)]);
        assert_eq!(rig.clips[0].duration, 1.0);
        assert_eq!((mesh.playing(0), mesh.playing(1), mesh.playing(-1)), (Some(0), None, None));
        // In the file's units, where the mesh's box puts them.
        let (center, longest) = rig.frame;
        let file = |p: [f32; 3]| [0, 1, 2].map(|k| p[k] * longest + center[k]);
        let at = |t: f32| mesh.posed(0, t).unwrap().vertices[0].iter().map(|v| file(v.pos)).collect::<Vec<_>>();
        let close = |a: [f32; 3], b: [f32; 3]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-4);
        // At the start: as bound. The top corner rides the second joint.
        let start = at(0.0);
        assert!(close(start[0], [0.0, 0.0, 0.0]) && close(start[2], [0.0, 2.0, 0.0]), "{start:?}");
        // At 0.25 s: the corner a quarter of the way along; the morph's
        // weight still 0; the first joint swollen (3s² − 2s³ of 1 more),
        // carrying everything on it, the second joint too.
        let swell = 1.0 + 0.15625;
        let quarter = at(0.25);
        assert!(close(quarter[1], [swell, 0.0, 0.0]), "{quarter:?}");
        assert!(close(quarter[2], [0.25 * swell, 2.0 * swell, 0.0]), "{quarter:?}");
        // At 0.75 s: the morph target's weight has stepped to 1.
        let late = at(0.75);
        let swell = 1.0 + 0.84375;
        assert!(close(late[0], [0.0, 0.0, swell]), "{late:?}");
        // However it moves, it stays in its unit box.
        for t in [0.0, 0.1, 0.3, 0.5, 0.6, 0.9, 1.0] {
            for v in &mesh.posed(0, t).unwrap().vertices[0] {
                assert!(v.pos.iter().all(|c| c.abs() <= 0.5 + 1e-4), "{t}s: {:?}", v.pos);
                assert!(super::super::length(v.pos) <= mesh.radius + 1e-4, "{t}s: {:?} beyond {}", v.pos, mesh.radius);
            }
        }
    }
}
