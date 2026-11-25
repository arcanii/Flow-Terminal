//! The 3D models programs place (the Ratty graphics protocol,
//! `crate::model`), drawn into the pictures their cells show: each placement
//! has a texture the size of its picture, which the quad shader samples as
//! it does any image's. A model is drawn again only when the program
//! changes it, its mesh comes in, or, while it's animated, every frame.
//! Drawn straight on, or in perspective through a glTF's own camera, as
//! `Shown::view` lays it on the picture. Lit from above left on both sides
//! (or by a glTF's own lights), its base color times its vertices' colors,
//! glossy parts with a highlight and a sheen,
//! metal, normal maps, emission and occlusion as a glTF's materials have
//! them, and the layers its
//! extensions add (specular, clearcoat, sheen, light through it), multisampled
//! where the GPU can, and written premultiplied, as the images are.
//! See-through triangles go last, farthest first (`crate::model`'s
//! `draw_order`, written to an index buffer of the placement's own), blended
//! over what's drawn and leaving the depth as it is, so the solid parts
//! behind them show. A glTF's own animation is posed on the CPU
//! (`crate::model`), its vertices, and those its see-through triangles'
//! cuts made, written to buffers of the placement's own each frame, and its
//! lights placed where it has them.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::graphics::Image;
use crate::model::{Alpha, Light, Mesh, Shown, Texture, Vertex, MAX_LIGHTS, TEXTURES};

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// How long a placement's texture, and a mesh's buffers, outlive their last frame.
const KEEP: Duration = Duration::from_secs(10);
/// Where the light comes from: above, left and in front.
const LIGHT: [f32; 3] = [-0.45, 0.65, 0.61];

/// What one part of a model is drawn with.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Draw {
    mvp: [[f32; 4]; 4],
    model: [[f32; 4]; 4],
    color: [f32; 4],
    /// The way to Flow's own light, and how many of the file's lights
    /// (`lights`) take its place.
    light: [f32; 4],
    /// How glossy the part is (one less its roughness), how far in front of
    /// the picture the eye its highlight is seen from (pixels), how metallic
    /// it is, and how far its normal map bends.
    shine: [f32; 4],
    /// The light it gives off, and 1 where it has a normal map.
    emission: [f32; 4],
    /// How its alpha is taken (0 solid, 1 a cut-out, 2 see-through), a
    /// cut-out's cutoff, how strongly its occlusion map darkens, and 1 if
    /// it's unlit.
    alpha: [f32; 4],
    /// 1 for each layer its material has: specular (or an ior of its own),
    /// sheen, clearcoat, light through it.
    layers: [f32; 4],
    /// How strongly it reflects head on, as a color (its specular color
    /// times its strength, times how its ior's reflection compares with
    /// glTF's 1.5's), and at the edges (its strength).
    specular: [f32; 4],
    /// The clearcoat's strength, its gloss, how far its normal map bends,
    /// and 1 where it has one.
    coat: [f32; 4],
    /// The sheen's color and roughness.
    sheen: [f32; 4],
    /// How much light passes through, the volume's thickness, and one over
    /// the distance its attenuation color is reached in (0: never).
    transmit: [f32; 4],
    /// The volume's attenuation color (linear).
    attenuation: [f32; 4],
    /// 1 where it's seen through the file's perspective camera, whose eye
    /// its rim faces; 0 drawn straight on, its rim as it always was.
    view: [f32; 4],
    /// The file's lights, four rows each (`light_rows`).
    lights: [[f32; 4]; 4 * MAX_LIGHTS],
    /// Each texture's uv map, in `Material::maps`' order: two rows of a 2 ×
    /// 3 matrix, the first's last 1 where it reads the second set of uvs.
    uv: [[f32; 4]; 2 * TEXTURES],
}

/// A model's part on the GPU.
struct GpuPart {
    /// Its vertices, then those cuts made where see-through triangles cross.
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    count: u32,
    /// In `model::Tex`'s order.
    textures: [wgpu::TextureView; TEXTURES],
}

struct GpuMesh {
    parts: Vec<GpuPart>,
    used: Instant,
}

/// A placement's picture: drawn multisampled (with its depth) and resolved
/// into `view`, which its cells show.
struct Target {
    size: (u32, u32),
    msaa: Option<wgpu::TextureView>,
    depth: wgpu::TextureView,
    view: wgpu::TextureView,
    /// Each part's uniforms and bind group, made for the mesh it last drew.
    draws: Vec<(wgpu::Buffer, wgpu::BindGroup)>,
    /// Each part's vertices as its glTF's animation poses them, where it
    /// has one, and the bytes it holds: the placement's own, as two may
    /// pose one mesh differently. The vertices its see-through triangles'
    /// cuts made follow, as many as each frame makes, so it grows.
    posed: Vec<(wgpu::Buffer, u64)>,
    /// The see-through triangles' corners in the order last drawn, and the
    /// bytes it holds: the placement's own, as each sees the mesh from its
    /// own side.
    sorted: Option<(wgpu::Buffer, u64)>,
    /// What it last drew: the placement's version and the mesh.
    drawn: Option<(u64, u64)>,
    used: Instant,
    /// The placement it shows (its `Shown`'s address), so a picture made
    /// anew for it at another size replaces this one at once.
    owner: usize,
}

pub struct Models {
    pipeline: wgpu::RenderPipeline,
    /// For see-through parts: blended, and leaving the depth as it is.
    blended: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    /// For parts without a texture.
    white: wgpu::TextureView,
    samples: u32,
    meshes: HashMap<u64, GpuMesh>,
    /// By the picture's `Image::serial`.
    targets: HashMap<u64, Target>,
    /// The pictures replaced by others for the same placement, whose
    /// textures the renderer can let go.
    replaced: Vec<u64>,
}

impl Models {
    /// `samples` is 4 where the GPU multisamples these formats, else 1.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, samples: u32) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("model"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/model.wgsl").into()),
        });
        let texture_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        // The uniforms, the base color's texture and the sampler, then the
        // other textures from 3 (`texture_binding`): fourteen in all, within
        // the sixteen a shader stage may sample on any GPU wgpu runs on.
        let mut entries = vec![
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ];
        entries.extend((0..TEXTURES).map(|k| texture_entry(texture_binding(k))));
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("model"), entries: &entries });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("model"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        // As `Vertex` lays them out: its color in bytes, which the shader
        // reads as 0 to 1.
        let attrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32x4, 4 => Float32x2, 5 => Unorm8x4];
        // Solid parts write the depth and replace what's there; see-through
        // ones blend over it, premultiplied, and leave the depth for the
        // solid ones behind them.
        let make = |label, blend: Option<wgpu::BlendState>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_model"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Vertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &attrs,
                    })],
                },
                // Both sides: an open model's inside faces the viewer too.
                primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH,
                    depth_write_enabled: Some(blend.is_none()),
                    depth_compare: Some(wgpu::CompareFunction::Less),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState { count: samples, ..Default::default() },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_model"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState { format: FORMAT, blend, write_mask: wgpu::ColorWrites::ALL })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let pipeline = make("model", None);
        let blended = make("model blended", Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING));
        // glTF's textures repeat unless they say otherwise.
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("model"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let white = texture(device, queue, 1, 1, &[255; 4]);
        Self { pipeline, blended, layout, sampler, white, samples, meshes: HashMap::new(), targets: HashMap::new(), replaced: Vec::new() }
    }

    /// The pictures since replaced by others for the same placement.
    pub fn take_replaced(&mut self) -> Vec<u64> {
        std::mem::take(&mut self.replaced)
    }

    /// The texture `image`'s cells show, drawn now if anything changed.
    pub fn draw(&mut self, gpu: (&wgpu::Device, &wgpu::Queue), encoder: &mut wgpu::CommandEncoder, image: &Image, shown: &Shown, now: Instant, clock: &super::timing::GpuClock) -> wgpu::TextureView {
        let (device, queue) = gpu;
        let max = device.limits().max_texture_dimension_2d;
        let size = (image.width.clamp(1, max), image.height.clamp(1, max));
        if self.targets.get(&image.serial).is_none_or(|t| t.size != size) {
            // A picture made anew for the placement (at the cells' new size,
            // after a font change) takes the old one's place now, rather
            // than leaving it for `release`.
            let owner = shown as *const Shown as usize;
            let old: Vec<u64> = self.targets.iter().filter(|(&k, t)| k != image.serial && t.owner == owner).map(|(&k, _)| k).collect();
            for k in old {
                self.targets.remove(&k);
                self.replaced.push(k);
            }
            self.targets.insert(image.serial, new_target(device, size, self.samples, owner));
        }
        let target = self.targets.get_mut(&image.serial).expect("made above");
        // Drawn already this frame, where its picture shows in two places.
        let again = target.used == now;
        target.used = now;
        let mesh = shown.mesh();
        let key = mesh.as_ref().map(|m| (shown.version(), m.serial));
        let Some(mesh) = mesh.filter(|_| !again && (target.drawn != key || shown.animating_at(now))) else { return target.view.clone() };
        let view = target.view.clone();
        // A mesh the program has replaced lets its buffers go now, unless
        // another picture shows it: a program that sends its scene anew
        // each frame would otherwise keep ten seconds of them.
        let before = target.drawn.map(|(_, serial)| serial).filter(|&s| s != mesh.serial);
        if let Some(old) = before {
            if !self.targets.iter().any(|(&k, t)| k != image.serial && t.drawn.is_some_and(|(_, s)| s == old)) {
                self.meshes.remove(&old);
            }
        }
        // The mesh's buffers, made the first time it's drawn.
        let gpu = self.meshes.entry(mesh.serial).or_insert_with(|| upload(device, queue, &mesh, &self.white));
        gpu.used = now;
        let target = self.targets.get_mut(&image.serial).expect("made above");
        if target.drawn.map(|(_, serial)| serial) != Some(mesh.serial) || target.draws.len() != gpu.parts.len() {
            target.draws = gpu
                .parts
                .iter()
                .map(|p| {
                    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("model draw"),
                        size: std::mem::size_of::<Draw>() as u64,
                        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    });
                    let mut entries = vec![
                        wgpu::BindGroupEntry { binding: 0, resource: buffer.as_entire_binding() },
                        wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                    ];
                    entries.extend(p.textures.iter().enumerate().map(|(k, view)| wgpu::BindGroupEntry { binding: texture_binding(k), resource: wgpu::BindingResource::TextureView(view) }));
                    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("model draw"), layout: &self.layout, entries: &entries });
                    (buffer, bind)
                })
                .collect();
            // Room for the posed vertices, where the mesh moves by itself:
            // its own, and those of the cuts made at rest.
            target.posed = if mesh.moves() {
                let vertex = std::mem::size_of::<Vertex>() as u64;
                let added = |i: usize| mesh.see_through.added.get(i).map_or(0, Vec::len);
                mesh.parts.iter().enumerate().map(|(i, p)| posed_buffer(device, (p.vertices.len() + added(i)) as u64 * vertex)).collect()
            } else {
                Vec::new()
            };
        }
        target.drawn = key;
        let posed = shown.posed(&mesh, now).filter(|p| p.vertices.len() == target.posed.len());
        if let Some(posed) = &posed {
            for ((buffer, room), vertices) in target.posed.iter_mut().zip(&posed.vertices) {
                let bytes = std::mem::size_of_val(&vertices[..]) as u64;
                // Cut afresh, a frame may make more vertices than the last:
                // room for a quarter more, so it doesn't grow every frame.
                if bytes > *room {
                    (*buffer, *room) = posed_buffer(device, bytes + bytes / 4);
                }
                queue.write_buffer(buffer, 0, bytemuck::cast_slice(vertices));
            }
        }

        let (w, h) = (size.0 as f32, size.1 as f32);
        // Straight on, or through the file's camera in perspective.
        let seen = shown.view(&mesh, (w, h), shown.turn(now));
        let model = seen.model;
        let style = shown.style();
        let mvp = mul(seen.projection, model);
        let tint = match (mesh.format, style.color) {
            // Ratty tints an OBJ, which has no colors of its own.
            (crate::model::Format::Obj, Some([r, g, b])) => [r, g, b].map(|c| c as f32 / 255.0),
            _ => [1.0; 3],
        };
        let light = normalize(LIGHT);
        let lights = light_rows(&mesh, posed.as_ref().and_then(|p| p.lights.as_deref()), model);
        let light = [light[0], light[1], light[2], mesh.lights.len().min(MAX_LIGHTS) as f32];
        let k = style.brightness.max(0.0);
        let on = |b: bool| if b { 1.0 } else { 0.0 };
        // How strongly a non-metal reflects head on for its index of
        // refraction, as Fresnel has it.
        let head_on = |ior: f32| ((ior - 1.0) / (ior + 1.0)).powi(2);
        for ((buffer, _), part) in target.draws.iter().zip(&mesh.parts) {
            let m = &part.material;
            let color = [m.color[0] * tint[0] * k, m.color[1] * tint[1] * k, m.color[2] * tint[2] * k, m.color[3]];
            let bend = m.bumps.as_ref().map_or(0.0, |(_, scale)| *scale);
            let emission = [m.emission[0] * k, m.emission[1] * k, m.emission[2] * k, on(m.bumps.is_some())];
            let (mode, cutoff) = match m.alpha {
                Alpha::Opaque => (0.0, 0.0),
                Alpha::Mask(cutoff) => (1.0, cutoff),
                Alpha::Blend => (2.0, 0.0),
            };
            let alpha = [mode, cutoff, m.occlusion.as_ref().map_or(0.0, |(_, strength)| *strength), on(m.unlit)];
            let mut uv = [[0.0; 4]; 2 * TEXTURES];
            for (i, map) in m.maps.iter().enumerate() {
                let [a, b] = map.m;
                uv[2 * i] = [a[0], a[1], a[2], map.set as f32];
                uv[2 * i + 1] = [b[0], b[1], b[2], 0.0];
            }
            let shine = [m.gloss, seen.eye, m.metallic, bend];
            let reflects = m.specular.is_some() || m.ior != 1.5;
            let (strength, tone) = m.specular.as_ref().map_or((1.0, [1.0; 3]), |s| (s.strength, s.color));
            let f0 = head_on(m.ior) / head_on(1.5) * strength;
            let specular = [tone[0] * f0, tone[1] * f0, tone[2] * f0, strength];
            let coat = m.clearcoat.as_ref().map_or([0.0; 4], |c| [c.strength, c.gloss, c.bumps.as_ref().map_or(0.0, |b| b.1), on(c.bumps.is_some())]);
            let sheen = m.sheen.as_ref().map_or([0.0; 4], |s| [s.color[0], s.color[1], s.color[2], s.roughness]);
            let through = m.transmission.as_ref().filter(|t| t.amount > 0.0);
            let transmit = through.map_or([0.0; 4], |t| [t.amount, t.thickness, if t.distance.is_finite() { 1.0 / t.distance } else { 0.0 }, 0.0]);
            let attenuation = through.map_or([1.0; 4], |t| [t.attenuation[0], t.attenuation[1], t.attenuation[2], 1.0]);
            let layers = [on(reflects), on(m.sheen.is_some()), on(m.clearcoat.is_some()), on(through.is_some())];
            let view = [on(seen.near.is_some()), 0.0, 0.0, 0.0];
            let draw = Draw { mvp, model, color, light, shine, emission, alpha, layers, specular, coat, sheen, transmit, attenuation, view, lights, uv };
            queue.write_buffer(buffer, 0, bytemuck::bytes_of(&draw));
        }
        let order = mesh.draw_order(posed.as_ref(), model);
        // The see-through triangles' corners, in the order they're drawn.
        if !order.indices.is_empty() {
            let bytes = std::mem::size_of_val(&order.indices[..]) as u64;
            if target.sorted.as_ref().is_none_or(|(_, room)| *room < bytes) {
                let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("model sorted"),
                    size: bytes,
                    usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                target.sorted = Some((buffer, bytes));
            }
            if let Some((buffer, _)) = &target.sorted {
                queue.write_buffer(buffer, 0, bytemuck::cast_slice(&order.indices));
            }
        }
        let (color_view, resolve) = match &target.msaa {
            Some(msaa) => (msaa, Some(&target.view)),
            None => (&target.view, None),
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("model"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color_view,
                depth_slice: None,
                resolve_target: resolve,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &target.depth,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }),
                stencil_ops: None,
            }),
            timestamp_writes: clock.writes(),
            occlusion_query_set: None,
            multiview_mask: None,
        });
        let vertices = |i: usize| match (&posed, target.posed.get(i)) {
            (Some(_), Some((posed, _))) => posed,
            _ => &gpu.parts[i].vertices,
        };
        pass.set_pipeline(&self.pipeline);
        for i in order.solid {
            let (Some(part), Some((_, bind))) = (gpu.parts.get(i), target.draws.get(i)) else { continue };
            pass.set_bind_group(0, bind, &[]);
            pass.set_vertex_buffer(0, vertices(i).slice(..));
            pass.set_index_buffer(part.indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..part.count, 0, 0..1);
        }
        if let Some((sorted, _)) = target.sorted.as_ref().filter(|_| !order.runs.is_empty()) {
            pass.set_pipeline(&self.blended);
            pass.set_index_buffer(sorted.slice(..), wgpu::IndexFormat::Uint32);
            let mut start = 0;
            for (i, count) in order.runs {
                if let (Some(_), Some((_, bind))) = (gpu.parts.get(i), target.draws.get(i)) {
                    pass.set_bind_group(0, bind, &[]);
                    pass.set_vertex_buffer(0, vertices(i).slice(..));
                    pass.draw_indexed(start..start + count, 0, 0..1);
                }
                start += count;
            }
        }
        drop(pass);
        view
    }

    /// What placement `serial`'s picture holds on the GPU beside the
    /// texture its cells show: its multisampled picture and its depth, its
    /// parts' uniforms, its posed and sorted vertices, and the mesh it last
    /// drew, its parts' vertices, indices and textures (where two
    /// placements draw one mesh, each counts it). Nothing for a picture
    /// that isn't a model's.
    pub fn gpu_bytes(&self, serial: u64) -> u64 {
        let Some(target) = self.targets.get(&serial) else { return 0 };
        let views = target.msaa.iter().chain([&target.depth]);
        let own = views.map(|v| texture_bytes(v.texture())).sum::<u64>()
            + target.draws.iter().map(|(b, _)| b.size()).sum::<u64>()
            + target.posed.iter().map(|(b, _)| b.size()).sum::<u64>()
            + target.sorted.as_ref().map_or(0, |(b, _)| b.size());
        let mesh = target.drawn.and_then(|(_, mesh)| self.meshes.get(&mesh)).map_or(0, |mesh| {
            // A texture two parts share counts once, and the white one
            // that stands in for none isn't the model's.
            let white = self.white.texture();
            let mut textures: Vec<&wgpu::Texture> = Vec::new();
            for texture in mesh.parts.iter().flat_map(|p| p.textures.iter().map(|v| v.texture())) {
                if texture != white && !textures.contains(&texture) {
                    textures.push(texture);
                }
            }
            mesh.parts.iter().map(|p| p.vertices.size() + p.indices.size()).sum::<u64>() + textures.into_iter().map(texture_bytes).sum::<u64>()
        });
        own + mesh
    }

    /// Lets go of the textures and meshes not drawn for a while.
    pub fn release(&mut self, now: Instant) {
        self.targets.retain(|_, t| now.duration_since(t.used) < KEEP);
        self.meshes.retain(|_, m| now.duration_since(m.used) < KEEP);
    }
}

/// What `texture` holds on the GPU: each sample of each pixel of each
/// layer, its smaller mip levels a third more.
fn texture_bytes(texture: &wgpu::Texture) -> u64 {
    let size = texture.size();
    let pixels = size.width as u64 * size.height as u64 * size.depth_or_array_layers as u64 * texture.sample_count() as u64;
    let bytes = pixels * texture.format().block_copy_size(None).unwrap_or(4) as u64;
    if texture.mip_level_count() > 1 { bytes * 4 / 3 } else { bytes }
}

fn new_target(device: &wgpu::Device, (w, h): (u32, u32), samples: u32, owner: usize) -> Target {
    let make = |label, format, samples, usage| {
        device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: samples,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
            .create_view(&wgpu::TextureViewDescriptor::default())
    };
    let attach = wgpu::TextureUsages::RENDER_ATTACHMENT;
    Target {
        size: (w, h),
        msaa: (samples > 1).then(|| make("model msaa", FORMAT, samples, attach)),
        depth: make("model depth", DEPTH, samples, attach),
        // Copied from too, by the test that reads a picture back.
        view: make("model", FORMAT, 1, attach | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC),
        draws: Vec::new(),
        posed: Vec::new(),
        sorted: None,
        drawn: None,
        used: Instant::now(),
        owner,
    }
}

/// A buffer for a part's posed vertices, `bytes` long (a whole number of
/// vertices, or room for some more), and how many bytes it holds.
fn posed_buffer(device: &wgpu::Device, bytes: u64) -> (wgpu::Buffer, u64) {
    let size = bytes.next_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT).max(wgpu::COPY_BUFFER_ALIGNMENT);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor { label: Some("model posed"), size, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
    (buffer, size)
}

/// A mesh's buffers and textures, a texture shared by parts made once. A
/// part without one of its material's textures gets white, which leaves
/// its factor as it is (a normal map's is switched off instead).
fn upload(device: &wgpu::Device, queue: &wgpu::Queue, mesh: &Mesh, white: &wgpu::TextureView) -> GpuMesh {
    let mut textures: HashMap<*const Texture, wgpu::TextureView> = HashMap::new();
    let parts = mesh
        .parts
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let textures = p.material.textures().map(|t| match t {
                Some(t) => textures.entry(std::sync::Arc::as_ptr(t)).or_insert_with(|| texture(device, queue, t.width, t.height, &t.rgba)).clone(),
                None => white.clone(),
            });
            let added = mesh.see_through.added.get(i).map_or(&[][..], Vec::as_slice);
            let vertices: std::borrow::Cow<[Vertex]> = if added.is_empty() { (&p.vertices[..]).into() } else { [&p.vertices[..], added].concat().into() };
            GpuPart {
                vertices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("model vertices"),
                    contents: bytemuck::cast_slice(&vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                }),
                indices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("model indices"),
                    contents: bytemuck::cast_slice(&p.indices),
                    usage: wgpu::BufferUsages::INDEX,
                }),
                count: p.indices.len() as u32,
                textures,
            }
        })
        .collect();
    GpuMesh { parts, used: Instant::now() }
}

/// Where texture `k` (in `model::Tex`'s order) is bound: the base color's
/// at 1, beside the uniforms, the rest from 3, after the sampler.
fn texture_binding(k: usize) -> u32 {
    if k == 0 {
        1
    } else {
        k as u32 + 2
    }
}

/// The file's lights as the shader takes them, four rows each, in the
/// picture's pixels (y up, z toward the viewer): placed and aimed as
/// `posed` has them where an animation moves them, else as at rest. They
/// keep to the view the model is first seen in: a light with a place is
/// where the model's middle is now plus its place in the unit box at the
/// model's size, and a directional one keeps its way, however the program
/// turns the model. Rows: where it is (w 1), or the way to it (w 0); its
/// color times its share, and how far it reaches (0, no end); the way it
/// shines, and its cone's scale; its cone's offset (a point light's, and a
/// directional one's, leave it whole), its distance from the middle
/// squared at rest, where it gives its share, and the nearest it counts as
/// being, squared (`model::settle_lights`). So a light an animation brings
/// nearer than at rest brightens, as the file's intensities say it would.
fn light_rows(mesh: &Mesh, posed: Option<&[Light]>, model: [[f32; 4]; 4]) -> [[f32; 4]; 4 * MAX_LIGHTS] {
    let mut rows = [[0.0; 4]; 4 * MAX_LIGHTS];
    let middle = [model[3][0], model[3][1], model[3][2]];
    let scale = (0..3).map(|c| (model[c][0].powi(2) + model[c][1].powi(2) + model[c][2].powi(2)).sqrt()).sum::<f32>() / 3.0;
    let near = 0.5 * mesh.radius * scale;
    for (k, rest) in mesh.lights.iter().take(MAX_LIGHTS).enumerate() {
        let l = posed.and_then(|p| p.get(k)).unwrap_or(rest);
        let aim = normalize(l.aim);
        let (place, away) = match (l.at, rest.at) {
            (Some(p), Some(r)) => {
                let at = [0, 1, 2].map(|i| middle[i] + p[i] * scale);
                ([at[0], at[1], at[2], 1.0], (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]) * scale * scale)
            }
            _ => ([-aim[0], -aim[1], -aim[2], 0.0], 0.0),
        };
        let (cone, offset) = match l.cone {
            Some((inner, outer)) => {
                let s = 1.0 / (inner - outer).max(0.001);
                (s, -outer * s)
            }
            None => (0.0, 1.0),
        };
        rows[4 * k] = place;
        rows[4 * k + 1] = [l.color[0] * l.strength, l.color[1] * l.strength, l.color[2] * l.strength, l.range.map_or(0.0, |r| r * scale)];
        rows[4 * k + 2] = [aim[0], aim[1], aim[2], cone];
        rows[4 * k + 3] = [offset, away.max(near * near), near * near, 0.0];
    }
    rows
}

/// A texture of straight-alpha RGBA, as glTF's are (the shader premultiplies),
/// shrunk by whole factors to what the GPU holds.
fn texture(device: &wgpu::Device, queue: &wgpu::Queue, w: u32, h: u32, rgba: &[u8]) -> wgpu::TextureView {
    let max = device.limits().max_texture_dimension_2d;
    let k = w.max(h).div_ceil(max.max(1)).max(1);
    let (tw, th) = ((w / k).max(1), (h / k).max(1));
    let pixels: std::borrow::Cow<[u8]> = if k == 1 {
        rgba.into()
    } else {
        (0..th).flat_map(|y| (0..tw).flat_map(move |x| (((y * k) * w + x * k) * 4) as usize..(((y * k) * w + x * k) * 4 + 4) as usize)).map(|i| rgba[i]).collect::<Vec<u8>>().into()
    };
    let size = wgpu::Extent3d { width: tw, height: th, depth_or_array_layers: 1 };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("model texture"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(texture.as_image_copy(), &pixels, wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(tw * 4), rows_per_image: Some(th) }, size);
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

/// `a` × `b`, column-major.
fn mul(a: [[f32; 4]; 4], b: [[f32; 4]; 4]) -> [[f32; 4]; 4] {
    let mut m = [[0f32; 4]; 4];
    for (c, col) in m.iter_mut().enumerate() {
        for (r, out) in col.iter_mut().enumerate() {
            *out = (0..4).map(|k| a[k][r] * b[c][k]).sum();
        }
    }
    m
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6);
    v.map(|c| c / l)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::model::{self, Clearcoat, Format, Sheen, Slot, Specular, Style, Transmission, UvMap};

    /// The machine's GPU, if wgpu finds one, with the limits the app asks for.
    fn gpu() -> Option<(wgpu::Device, wgpu::Queue)> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).ok()?;
        let required_limits = crate::render::limits(&adapter);
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor { required_limits, ..Default::default() })).ok()
    }

    /// `mesh` drawn with `style` into a picture 64 pixels square, read
    /// back: premultiplied RGBA, by rows. With `FLOW_MODEL_PICTURES` set
    /// to a folder, it's also saved there as `<name>.png`, to look at.
    fn picture(gpu: &(wgpu::Device, wgpu::Queue), name: &str, mesh: model::Mesh, style: Style) -> Vec<u8> {
        picture_at(gpu, name, mesh, style, None)
    }

    /// As [`picture`], `secs` into its animation where given.
    fn picture_at(gpu: &(wgpu::Device, wgpu::Queue), name: &str, mesh: model::Mesh, style: Style, secs: Option<f32>) -> Vec<u8> {
        const SIDE: u32 = 64;
        let (device, queue) = gpu;
        let mut models = Models::new(device, queue, 4);
        let slot = Arc::new(Slot::default());
        slot.set(mesh);
        let shown = Arc::new(Shown::new(slot, style, 20.0));
        let image = Image::model(SIDE, SIDE, shown.clone());
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        let now = secs.map_or_else(Instant::now, |s| shown.since + Duration::from_secs_f32(s));
        let view = models.draw((device, queue), &mut encoder, &image, &shown, now, &crate::render::timing::GpuClock::off());
        let row = (SIDE * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: u64::from(row * SIDE),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            view.texture().as_image_copy(),
            wgpu::TexelCopyBufferInfo { buffer: &buffer, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(SIDE) } },
            wgpu::Extent3d { width: SIDE, height: SIDE, depth_or_array_layers: 1 },
        );
        queue.submit(Some(encoder.finish()));
        buffer.map_async(wgpu::MapMode::Read, .., |_| {});
        let _ = device.poll(wgpu::PollType::Wait { submission_index: None, timeout: None });
        let data = buffer.get_mapped_range(..).expect("mapped");
        let pixels: Vec<u8> = (0..SIDE).flat_map(|y| data[(y * row) as usize..(y * row + SIDE * 4) as usize].to_vec()).collect();
        if let Some(dir) = std::env::var_os("FLOW_MODEL_PICTURES") {
            let file = std::fs::File::create(std::path::Path::new(&dir).join(format!("{name}.png"))).expect("a file for the picture");
            let mut png = png::Encoder::new(file, SIDE, SIDE);
            png.set_color(png::ColorType::Rgba);
            png.write_header().and_then(|mut w| w.write_image_data(&pixels)).expect("written");
        }
        pixels
    }

    fn at(pixels: &[u8], (x, y): (usize, usize)) -> [u8; 4] {
        let i = (y * 64 + x) * 4;
        [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
    }

    /// A white square facing the viewer, its texture `texture` (one row of
    /// colors) read through `maps[0]` and its vertices' uvs set by `uvs`.
    fn square(texture: Option<&[u8]>, map: UvMap, uvs: impl Fn(&mut Vertex)) -> model::Mesh {
        let mut mesh = model::load(Format::Obj, model::tests::SQUARE.as_bytes()).unwrap();
        let part = &mut mesh.parts[0];
        part.material.texture = texture.map(|rgba| Arc::new(Texture { width: rgba.len() as u32 / 4, height: 1, rgba: rgba.to_vec() }));
        part.material.maps[0] = map;
        part.vertices.iter_mut().for_each(uvs);
        mesh
    }

    #[test]
    #[ignore = "draws on the GPU"]
    fn sgis_cube_at_rest_is_drawn_as_its_logo_on_the_gpu() {
        let Some(gpu) = gpu() else { return };
        // As `logos::sgi` stands it, at its start: what a click brings back.
        let style = Style { animate: true, rotation: crate::logos::SGI_REST, ..Style::default() };
        let pixels = picture_at(&gpu, "sgi-rest", crate::logos::sgi_cube().unwrap(), style, Some(0.0));
        // Drawn, and as the logo is, the same turned a third of the way
        // round the picture's middle.
        let covered = |x: f32, y: f32| {
            let (x, y) = (x.round() as i32, y.round() as i32);
            (0..64).contains(&x) && (0..64).contains(&y) && at(&pixels, (x as usize, y as usize))[3] > 128
        };
        let (s, c) = 120f32.to_radians().sin_cos();
        let mut alike = (0, 0);
        for y in 0..64 {
            for x in 0..64 {
                let (dx, dy) = (x as f32 - 31.5, y as f32 - 31.5);
                let here = covered(x as f32, y as f32);
                let there = covered(31.5 + c * dx - s * dy, 31.5 + s * dx + c * dy);
                alike.0 += usize::from(here && there);
                alike.1 += usize::from(here || there);
            }
        }
        assert!(alike.1 > 600 && alike.0 * 100 / alike.1 > 75, "{alike:?}");
    }

    #[test]
    #[ignore = "draws on the GPU"]
    fn models_are_drawn_with_their_materials_on_the_gpu() {
        let Some(gpu) = gpu() else { return };
        // Seen straight on, the three triangles of `layers_glb` cover the
        // same pixels: the see-through one in front blends over the solid
        // one, and the one behind that is hidden.
        let layers = picture(&gpu, "layers", model::load(Format::Glb, &model::tests::layers_glb()).unwrap(), Style::default());
        let [r, _, b, a] = at(&layers, (18, 46));
        assert_eq!(a, 255, "over a solid part");
        assert!(r > 80 && b > 40, "the solid orange under the blue: {:?}", at(&layers, (18, 46)));
        // Turned to its side a little, the solid part shows on its own
        // where the see-through one no longer covers it.
        let turned = picture(&gpu, "layers-turned", model::load(Format::Glb, &model::tests::layers_glb()).unwrap(), Style { rotation: [0.0, 40.0, 0.0], ..Style::default() });
        assert!(turned.chunks(4).any(|p| p[3] == 255) && turned.chunks(4).any(|p| p[3] > 0 && p[3] < 255), "solid and see-through both");

        // An occlusion map shades the room's light.
        let lit = picture(&gpu, "unshaded", square(None, UvMap::PLAIN, |_| {}), Style::default());
        let mut dark = square(None, UvMap::PLAIN, |_| {});
        dark.parts[0].material.occlusion = Some((Arc::new(Texture { width: 1, height: 1, rgba: vec![0, 0, 0, 255] }), 1.0));
        let shaded = picture(&gpu, "occluded", dark, Style::default());
        let (open, closed) = (at(&lit, (32, 32)), at(&shaded, (32, 32)));
        assert!(i32::from(closed[0]) + 40 < i32::from(open[0]) && closed[3] == 255, "{open:?} then {closed:?}");

        // Red then blue: a texture read through moved uvs, or the second set.
        let texture: &[u8] = &[255, 0, 0, 255, 0, 0, 255, 255];
        let red = picture(&gpu, "first-set", square(Some(texture), UvMap::PLAIN, |v| v.uv = [0.25, 0.5]), Style::default());
        let moved = picture(&gpu, "moved", square(Some(texture), UvMap::new(0, [0.5, 0.0], 0.0, [1.0; 2]), |v| v.uv = [0.25, 0.5]), Style::default());
        let second = picture(&gpu, "second-set", square(Some(texture), UvMap::new(1, [0.0; 2], 0.0, [1.0; 2]), |v| (v.uv, v.uv1) = ([0.25, 0.5], [0.75, 0.5])), Style::default());
        let blue = |p: [u8; 4]| p[2] > 100 && p[0] < 20;
        assert!(at(&red, (32, 32))[0] > 100 && at(&red, (32, 32))[2] < 20, "{:?}", at(&red, (32, 32)));
        assert!(blue(at(&moved, (32, 32))) && blue(at(&second, (32, 32))), "{:?} {:?}", at(&moved, (32, 32)), at(&second, (32, 32)));

        // Its glTF material as before, turned to show its sheen.
        picture(&gpu, "material", model::load(Format::Glb, &model::tests::material_glb()).unwrap(), Style { rotation: [20.0, 30.0, 0.0], ..Style::default() });
        picture(&gpu, "tinted", model::load(Format::Obj, model::tests::SQUARE.as_bytes()).unwrap(), Style { color: Some([0x7c, 0xe7, 0xff]), rotation: [18.0, 24.0, 0.0], ..Style::default() });
        let glazed = picture(&gpu, "ball-glazed", ball(|m| (m.color, m.gloss) = ([0.2, 0.45, 0.9, 1.0], 0.8)), Style::default());
        picture(&gpu, "ball-gold", ball(|m| (m.color, m.gloss, m.metallic) = ([1.0, 0.78, 0.34, 1.0], 0.7, 1.0)), Style::default());
        let brightest = |p: &[u8]| p.chunks(4).map(|c| u32::from(c[0]) + u32::from(c[1]) + u32::from(c[2])).max().unwrap_or(0);

        // Unlit: its color alone, the same all over.
        let flat = picture(&gpu, "unlit", ball(|m| (m.color, m.unlit) = ([1.0, 0.5, 0.0, 1.0], true)), Style::default());
        let middle = at(&flat, (32, 32));
        assert!(middle[0] == 255 && middle[2] == 0 && middle == at(&flat, (14, 46)) && middle == at(&flat, (50, 20)), "{middle:?} {:?}", at(&flat, (14, 46)));

        // A specular strength of 0: the glaze reflects nothing.
        let none = Specular { strength: 0.0, strength_map: None, color: [1.0; 3], color_map: None };
        let dull = picture(&gpu, "specular-none", ball(|m| (m.color, m.gloss, m.specular) = ([0.2, 0.45, 0.9, 1.0], 0.8, Some(none.clone()))), Style::default());
        assert!(brightest(&dull) + 150 < brightest(&glazed), "{} then {}", brightest(&glazed), brightest(&dull));
        // A red one, at a diamond's ior: its glints redder and stronger.
        let red = Specular { strength: 1.0, color: [1.0, 0.2, 0.2], ..none };
        picture(&gpu, "specular-red", ball(|m| (m.color, m.gloss, m.ior, m.specular) = ([0.2, 0.45, 0.9, 1.0], 0.8, 2.4, Some(red.clone()))), Style::default());

        // A clearcoat over matte red: glints where the matte one has none.
        let matte = picture(&gpu, "matte-red", ball(|m| m.color = [0.8, 0.1, 0.1, 1.0]), Style::default());
        let coat = Clearcoat { strength: 1.0, strength_map: None, gloss: 1.0, roughness_map: None, bumps: None };
        let coated = picture(&gpu, "clearcoat", ball(|m| (m.color, m.clearcoat) = ([0.8, 0.1, 0.1, 1.0], Some(coat.clone()))), Style::default());
        assert!(brightest(&coated) > brightest(&matte) + 200, "{} then {}", brightest(&matte), brightest(&coated));

        // A sheen: a dark ball's edges catch its color.
        let rim_red = |p: &[u8]| {
            let ring: Vec<u32> = (0..64 * 64).filter(|i| (24.0..27.0).contains(&((i % 64) as f32 - 31.5).hypot((i / 64) as f32 - 31.5))).map(|i| u32::from(p[i * 4])).collect();
            ring.iter().sum::<u32>() / ring.len() as u32
        };
        let dark = picture(&gpu, "dark", ball(|m| m.color = [0.1, 0.1, 0.3, 1.0]), Style::default());
        let sheen = Sheen { color: [1.0, 0.8, 0.6], color_map: None, roughness: 0.3, roughness_map: None };
        let velvet = picture(&gpu, "sheen", ball(|m| (m.color, m.sheen) = ([0.1, 0.1, 0.3, 1.0], Some(sheen.clone()))), Style::default());
        assert!(rim_red(&velvet) > rim_red(&dark) + 20, "{} then {}", rim_red(&dark), rim_red(&velvet));

        // Light through it: a clear glass ball's middle shows what's behind
        // it, its glints kept; red glass lays a red film over it.
        let clear = Transmission { amount: 1.0, map: None, thickness: 0.0, thickness_map: None, attenuation: [1.0; 3], distance: f32::INFINITY };
        let glass = picture(&gpu, "glass", ball(|m| (m.color, m.gloss, m.transmission) = ([1.0; 4], 0.9, Some(clear.clone()))), Style::default());
        let red_glass = picture(&gpu, "glass-red", ball(|m| (m.color, m.gloss, m.transmission) = ([1.0, 0.1, 0.1, 1.0], 0.9, Some(clear.clone()))), Style::default());
        let (see, tinted) = (at(&glass, (32, 32)), at(&red_glass, (32, 32)));
        assert!(see[3] < 100 && brightest(&glass) > 450, "{see:?}, brightest {}", brightest(&glass));
        assert!(tinted[3] > see[3] + 60 && tinted[0] > tinted[1] + 60, "{tinted:?}");
        // A volume: thick glass that turns light green deepens the film.
        let deep = Transmission { thickness: 1.0, attenuation: [0.2, 0.9, 0.3], distance: 0.25, ..clear };
        let bottle = picture(&gpu, "glass-volume", ball(|m| (m.color, m.gloss, m.transmission) = ([1.0; 4], 0.9, Some(deep.clone()))), Style::default());
        let green = at(&bottle, (32, 32));
        assert!(green[3] > see[3] + 60 && green[1] > green[0] + 40, "{green:?}");

        // Two see-through squares crossing in an X: each half nearer the
        // viewer shows over the other's, the red on the right, the blue on
        // the left (drawn by their middles, the one drawn last covered both).
        let crossed = picture(&gpu, "crossed", model::load(Format::Glb, &model::tests::crossed_glb()).unwrap(), Style::default());
        let (right, left) = (at(&crossed, (48, 32)), at(&crossed, (16, 32)));
        assert!(right[0] > right[2] + 60 && left[2] > left[0], "{right:?} {left:?}");

        // A file's light in place of Flow's: a red one off to the right.
        let mut lamp = ball(|m| m.color = [1.0; 4]);
        lamp.lights = vec![Light { at: Some([2.0, 0.0, 0.5]), aim: [-1.0, 0.0, 0.0], color: [1.0, 0.0, 0.0], strength: 1.0, range: None, cone: None, node: None }];
        let lit = picture(&gpu, "red-light", lamp, Style::default());
        let (right, left) = (at(&lit, (56, 32)), at(&lit, (8, 32)));
        assert!(right[0] > right[1] + 80 && left[0].abs_diff(left[1]) < 12, "{right:?} {left:?}");

        // A red light an animation carries across in front of a white
        // square: the side it's nearer is the redder, left then right.
        // Seen through the file's camera, five units off, the square spans
        // the middle half of the picture.
        let red = |p: [u8; 4]| i32::from(p[0]) - i32::from(p[1]);
        for (secs, name) in [(0.0, "light-moving-start"), (1.0, "light-moving-end"), (0.5, "light-moving-middle")] {
            if secs == 0.5 {
                // Halfway, right in front of it: lit no more than Flow's
                // own light would, its shares settled over the clip.
                let swept = picture_at(&gpu, name, model::load(Format::Glb, &model::tests::moving_lights_glb()).unwrap(), Style::default(), Some(secs));
                let (left, right) = (red(at(&swept, (20, 32))), red(at(&swept, (44, 32))));
                assert!(left.abs_diff(right) < 20 && left > 30, "{left} {right}");
                continue;
            }
            let swept = picture_at(&gpu, name, model::load(Format::Glb, &model::tests::moving_lights_glb()).unwrap(), Style::default(), Some(secs));
            let (left, right) = (red(at(&swept, (20, 32))), red(at(&swept, (44, 32))));
            let (near, far) = if secs == 0.0 { (left, right) } else { (right, left) };
            assert!(near > far + 30, "{secs}s: {left} on the left, {right} on the right");
        }

        // The crossed squares, one sliding: cut where they cross now (the
        // picture's middle, the two overlapping from 22 to 42 pixels across),
        // each half nearer the viewer still shows over the other's, both
        // sides; sorted whole, as they were, one side was wrong.
        let slid = picture_at(&gpu, "crossed-moving", model::load(Format::Glb, &model::tests::sliding_crossed_glb()).unwrap(), Style::default(), Some(1.0));
        let (right, left) = (at(&slid, (38, 32)), at(&slid, (26, 32)));
        assert!(right[0] > right[2] + 60 && left[2] > left[0], "{right:?} {left:?}");

        // Three equal squares, each further off: through the file's
        // perspective camera the green one is half as wide as the red and
        // the blue a quarter (25, 12.6 and 6.3 pixels); through its
        // orthographic twin, drawn straight on as before, all alike.
        let wide = |p: &[u8], channel: usize| {
            let xs: Vec<usize> = (0..64 * 64).filter(|&i| p[i * 4 + channel] > 200 && (0..3).all(|c| c == channel || p[i * 4 + c] < 60)).map(|i| i % 64).collect();
            xs.iter().max().map_or(0, |m| m + 1 - xs.iter().min().unwrap_or(m))
        };
        let seen = picture(&gpu, "perspective", model::load(Format::Glb, &model::tests::perspective_glb(false)).unwrap(), Style::default());
        let (r, g, b) = (wide(&seen, 0), wide(&seen, 1), wide(&seen, 2));
        assert!((24..=27).contains(&r) && (12..=14).contains(&g) && (6..=7).contains(&b), "{r}, {g} and {b} pixels wide");
        let flat = picture(&gpu, "perspective-orthographic", model::load(Format::Glb, &model::tests::perspective_glb(true)).unwrap(), Style::default());
        let (r, g, b) = (wide(&flat, 0), wide(&flat, 1), wide(&flat, 2));
        assert!(r.abs_diff(g) <= 1 && g.abs_diff(b) <= 1 && r > 5, "{r}, {g} and {b} pixels wide");
        // A cube close up, its near corner's edges longer than its far
        // ones; straight on, they're alike. To look at.
        picture(&gpu, "perspective-cube", model::load(Format::Glb, &model::tests::perspective_cube_glb(false)).unwrap(), Style::default());
        picture(&gpu, "perspective-cube-orthographic", model::load(Format::Glb, &model::tests::perspective_cube_glb(true)).unwrap(), Style::default());
        picture(&gpu, "perspective-cube-turned", model::load(Format::Glb, &model::tests::perspective_cube_glb(false)).unwrap(), Style { rotation: [0.0, 30.0, 0.0], ..Style::default() });
        // A camera going round a triangle, through its eye.
        for (secs, name) in [(0.0, "camera-orbit-start"), (0.5, "camera-orbit-middle")] {
            picture_at(&gpu, name, model::load(Format::Glb, &model::tests::orbiting_camera_glb()).unwrap(), Style::default(), Some(secs));
        }

        // A ball packed as gltfpack packs one (meshopt, quantized, a WebP
        // texture) draws as its plain twin does, near enough: its normals
        // in bytes shade a little differently.
        let plain = picture(&gpu, "ball-plain", model::load(Format::Glb, &model::tests::ball_glb(false)).unwrap(), Style { rotation: [20.0, 0.0, 0.0], ..Style::default() });
        let packed = picture(&gpu, "ball-packed", model::load(Format::Glb, &model::tests::ball_glb(true)).unwrap(), Style { rotation: [20.0, 0.0, 0.0], ..Style::default() });
        let most = plain.iter().zip(&packed).map(|(a, b)| a.abs_diff(*b)).max().unwrap_or(0);
        let off = plain.iter().zip(&packed).filter(|(a, b)| a.abs_diff(**b) > 8).count();
        assert!(off < 40, "{off} values more than 8 apart, at most {most}");
        assert!(packed.chunks(4).any(|p| p[0] > 120 && p[1] < 50) && packed.chunks(4).any(|p| p[1] > 120 && p[2] > 120), "its texture's red and white");
        // Packed with meshopt's vertex codec version 1, or as a Draco mesh
        // (by either of Draco's ways), it draws as the plain one does too,
        // but for the colors their corners carry (COLOR_0: warmer toward
        // the top), which are left white for the likeness. With them, the
        // top is redder than the bottom.
        let tilted = Style { rotation: [20.0, 0.0, 0.0], ..Style::default() };
        let white = |mut mesh: model::Mesh| {
            mesh.parts.iter_mut().flat_map(|p| &mut p.vertices).for_each(|v| v.color = [255; 4]);
            mesh
        };
        let warmer = |p: &[u8]| {
            let red = |rows: std::ops::Range<usize>| rows.flat_map(|y| (0..64).map(move |x| (x, y))).map(|xy| at(p, xy)).filter(|c| c[3] > 200).map(|c| i32::from(c[0]) - i32::from(c[2])).sum::<i32>();
            red(8..20) > red(44..56) + 2000
        };
        let colored = picture(&gpu, "ball-meshopt-v1-colors", model::load(Format::Glb, &model::tests::ball_v1_glb()).unwrap(), tilted);
        assert!(warmer(&colored), "meshopt v1: warmer at the top");
        let v1 = picture(&gpu, "ball-meshopt-v1", white(model::load(Format::Glb, &model::tests::ball_v1_glb()).unwrap()), tilted);
        for method in [0, 1] {
            let draco = || model::load(Format::Glb, &model::tests::draco_glb(&model::tests::draco_ball(method), model::tests::DRACO_IDS)).unwrap();
            let colored = picture(&gpu, &format!("ball-draco-{method}-colors"), draco(), tilted);
            assert!(warmer(&colored), "Draco {method}: warmer at the top");
            let drawn = picture(&gpu, &format!("ball-draco-{method}"), white(draco()), tilted);
            for (name, other) in [("meshopt v1", &v1), ("Draco", &drawn)] {
                let off = plain.iter().zip(other).filter(|(a, b)| a.abs_diff(**b) > 8).count();
                assert!(off < 40, "{name}: {off} values more than 8 apart");
            }
        }

        // Google's own encoder's Draco ball draws as the plain one does,
        // and the ball textured with Binomial's own KTX2 files (ETC1S,
        // UASTC) shows their red and white.
        for (level, file) in [0, 7, 10].into_iter().zip(model::tests::GOOGLES_DRACO) {
            let drawn = picture(&gpu, &format!("ball-draco-google-{level}"), model::load(Format::Glb, &model::tests::draco_glb(file, model::tests::GOOGLES_IDS)).unwrap(), tilted);
            let off = plain.iter().zip(&drawn).filter(|(a, b)| a.abs_diff(**b) > 8).count();
            assert!(off < 40, "level {level}: {off} values more than 8 apart");
        }
        for (kind, file) in ["etc1s", "uastc"].into_iter().zip(model::tests::BASIS_CHECKERS) {
            let drawn = picture(&gpu, &format!("ball-ktx2-{kind}"), model::load(Format::Glb, &model::tests::basis_ball_glb(file, false)).unwrap(), tilted);
            assert!(drawn.chunks(4).any(|p| p[0] > 120 && p[1] < 50) && drawn.chunks(4).any(|p| p[1] > 120 && p[2] > 120), "{kind}: its texture's red and white");
        }

        // The old specular-glossiness material: a dark red under a gold
        // glaze, glossy where its texture's alpha is (round one side of
        // the ball), rough round the other; or glossy all over by its
        // factor.
        let gold = |p: &[u8]| p[0] > 140 && p[1] > 80 && p[2] < p[1];
        for (textured, name) in [(true, "spec-gloss"), (false, "spec-gloss-plain")] {
            let drawn = picture(&gpu, name, model::load(Format::Glb, &model::tests::spec_gloss_glb(textured)).unwrap(), Style::default());
            assert!(drawn.chunks(4).any(gold), "{name}: gold glints");
            assert!(drawn.chunks(4).any(|p| p[3] > 200 && p[0] > p[1].saturating_add(40) && p[1] < 60), "{name}: its dark red");
        }

        // A plank its camera frames past the picture's sides, stood up:
        // drawn smaller, so its ends just reach the top and bottom, where
        // they'd have gone past them; as wide as it is, it'd be some 14
        // pixels across.
        let stood = picture(&gpu, "plank-stood", model::load(Format::Glb, &model::tests::plank_glb()).unwrap(), Style { rotation: [0.0, 0.0, 90.0], ..Style::default() });
        let row = |y: usize| (0..64).filter(|&x| at(&stood, (x, y))[3] > 128).count();
        assert!(row(1) > 0 && row(62) > 0 && (5..=10).contains(&row(32)), "{} {} {}", row(1), row(32), row(62));

        // Vertex colors (COLOR_0) times the base color: a triangle red at
        // its bottom left, green at its bottom right, blue (with a fifth of
        // red and green) at its top left, blended between them.
        let corners: Vec<u8> = model::tests::CORNER_COLORS.iter().flatten().flat_map(|f| f.to_le_bytes()).collect();
        let matte = |extra: &str| format!(r#""pbrMetallicRoughness":{{"baseColorFactor":[1,1,1,1],"metallicFactor":0}}{extra}"#);
        let colored = picture(&gpu, "vertex-colors", model::load(Format::Glb, &model::tests::colored_triangle(&corners, 5126, false, "VEC4", 16, &matte(""))).unwrap(), Style::default());
        let (red, green, blue) = (at(&colored, (8, 56)), at(&colored, (52, 58)), at(&colored, (5, 9)));
        assert!(red[0] > red[1] + 80 && red[0] > red[2] + 80 && red[3] == 255, "{red:?}");
        assert!(green[1] > green[0] + 60 && green[1] > green[2] + 60 && green[3] == 255, "solid, its colors' alpha ignored: {green:?}");
        assert!(blue[2] > blue[0] + 50 && blue[2] > blue[1] + 50, "{blue:?}");
        // See-through, their alpha too: the green corner's a quarter.
        let faint = picture(&gpu, "vertex-colors-blended", model::load(Format::Glb, &model::tests::colored_triangle(&corners, 5126, false, "VEC4", 16, &matte(r#","alphaMode":"BLEND""#))).unwrap(), Style::default());
        let (solid, clear) = (at(&faint, (8, 56)), at(&faint, (52, 58)));
        assert!(solid[3] > 200 && clear[3] < 130 && clear[1] > clear[0], "{solid:?} {clear:?}");
        // White vertex colors leave a model as it was drawn without any.
        let white = model::load(Format::Glb, &model::tests::colored_triangle(&[255; 12], 5121, true, "VEC4", 4, r#""pbrMetallicRoughness":{"baseColorFactor":[1,0,0,1]}"#)).unwrap();
        assert!(white.parts[0].vertices.iter().all(|v| v.color == [255; 4]));
        let without = picture(&gpu, "vertex-colors-none", model::load(Format::Glb, &model::tests::triangle_glb()).unwrap(), Style::default());
        assert_eq!(picture(&gpu, "vertex-colors-white", white, Style::default()), without);

        // A node's three instances (EXT_mesh_gpu_instancing): one above
        // another, turned, doubled and mirrored as each says, the box
        // holding all three, from (8, 0) to (10, 6), so a unit 9.6 pixels:
        // the first in rows 51 to 60, the second 32 to 51, the third 3 to
        // 12, and nothing between the second and the third.
        let instanced = picture(&gpu, "instanced", model::load(Format::Glb, &model::tests::instanced_glb(false)).unwrap(), Style::default());
        let row = |y: usize| (0..64).filter(|&x| at(&instanced, (x, y))[3] > 128).count();
        assert!([5, 10, 36, 48, 54, 58].iter().all(|&y| row(y) > 0), "{:?}", (0..64).map(row).collect::<Vec<_>>());
        assert!((15..30).all(|y| row(y) == 0), "{:?}", (0..64).map(row).collect::<Vec<_>>());
        // As the node slides up, they go with it.
        let moved = picture_at(&gpu, "instanced-moving", model::load(Format::Glb, &model::tests::instanced_glb(true)).unwrap(), Style::default(), Some(1.0));
        let top = |p: &[u8]| (0..64).find(|&y| (0..64).any(|x| at(p, (x, y))[3] > 128));
        let start = picture_at(&gpu, "instanced-moving-start", model::load(Format::Glb, &model::tests::instanced_glb(true)).unwrap(), Style::default(), Some(0.0));
        assert!(top(&moved).zip(top(&start)).is_some_and(|(a, b)| a + 10 < b), "{:?} then {:?}", top(&start), top(&moved));
    }

    /// A ball of `material`, smooth-shaded.
    fn ball(material: impl Fn(&mut model::Material)) -> model::Mesh {
        let (rings, segments) = (16, 32);
        let mut obj = String::new();
        for r in 0..=rings {
            let (y, across) = ((r as f32 / rings as f32) * std::f32::consts::PI).sin_cos();
            for s in 0..segments {
                let (z, x) = ((s as f32 / segments as f32) * std::f32::consts::TAU).sin_cos();
                obj += &format!("v {} {} {}\n", x * y, across, z * y);
            }
        }
        for r in 0..rings {
            for s in 0..segments {
                let at = |r: usize, s: usize| r * segments + s % segments + 1;
                obj += &format!("f {} {} {} {}\n", at(r, s), at(r, s + 1), at(r + 1, s + 1), at(r + 1, s));
            }
        }
        let mut mesh = model::load(Format::Obj, obj.as_bytes()).unwrap();
        material(&mut mesh.parts[0].material);
        mesh
    }
}
