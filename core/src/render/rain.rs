//! The rain map for the rain style: the drops on the window glass, drawn as
//! small lenses into a texture the glass shader reads. Its x is the water's
//! height (pixels / 16), y how clear the glass is where drops ran or the
//! cursor wiped, and a little round each drop (0..1), z and w where in its
//! drop each pixel sits, for seeing through it. The drops themselves move on
//! the CPU (fx/rain.rs).

use bytemuck::{Pod, Zeroable};

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// One shape in the rain map, from `head` to `tail` in window pixels: a drop
/// (a lens, drawn out into a teardrop while it runs, or a little egg-shaped
/// as it sags) or a clear streak.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug, Default, PartialEq)]
pub struct RainQuad {
    /// Head x, y and tail x, y; the same point for a round drop.
    pub ends: [f32; 4],
    /// For a drop: radius at the head and at the tail (pixels). For a streak:
    /// its half width, and the radius of the beads of water along its sides
    /// (pixels).
    /// Then the kind ([`RainQuad::DROP`], [`RainQuad::STREAK`] and the
    /// others below), and the strength: a drop's height relative to its
    /// radius, or how clear a streak is (its water fades with it).
    pub params: [f32; 4],
}

impl RainQuad {
    pub const DROP: f32 = 0.0;
    pub const STREAK: f32 = 1.0;
    /// A drop striking the glass, flattened into a splat with its rim thrown
    /// up in a crown: its radius, then where its lobes go (0..1), its
    /// height for its radius.
    pub const SPLAT: f32 = 2.0;
    /// A droplet of a strike's spray: round, as a drop.
    pub const SPRAY: f32 = 4.0;
}

pub struct RainLayer {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    uniforms: wgpu::Buffer,
    instances: wgpu::Buffer,
    capacity: usize,
    /// The map and its size, kept while the rain style is on.
    target: Option<(wgpu::TextureView, (u32, u32))>,
}

impl RainLayer {
    pub fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rain"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/rain.wgsl").into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rain"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rain"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        // Max blending: where shapes overlap, the higher water and the
        // clearer glass win.
        let max = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Max,
        };
        let attrs = wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rain"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_rain"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<RainQuad>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &attrs,
                })],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_rain"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: FORMAT,
                    blend: Some(wgpu::BlendState { color: max, alpha: max }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rain params"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let capacity = 1024;
        Self {
            pipeline,
            layout,
            uniforms,
            instances: create_instances(device, capacity),
            capacity,
            target: None,
        }
    }

    /// What its map holds on the GPU, once it's been drawn.
    pub fn gpu_bytes(&self) -> u64 {
        self.target.as_ref().map_or(0, |(_, size)| super::texture_bytes(*size, FORMAT))
    }

    /// Draws `quads` into a fresh rain map the size of the window, at
    /// `scale` pixels per point.
    pub fn draw(&mut self, gpu: (&wgpu::Device, &wgpu::Queue), encoder: &mut wgpu::CommandEncoder, size: (u32, u32), scale: f32, quads: &[RainQuad], clock: &super::timing::GpuClock) {
        let (device, queue) = gpu;
        let size = (size.0.max(1), size.1.max(1));
        if self.target.as_ref().map(|t| t.1) != Some(size) {
            let tex = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("rain map"),
                size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            self.target = Some((tex.create_view(&wgpu::TextureViewDescriptor::default()), size));
        }
        if quads.len() > self.capacity {
            self.capacity = quads.len().next_power_of_two();
            self.instances = create_instances(device, self.capacity);
        }
        queue.write_buffer(&self.uniforms, 0, bytemuck::cast_slice(&[size.0 as f32, size.1 as f32, scale, 0.0]));
        if !quads.is_empty() {
            queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(quads));
        }
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rain"),
            layout: &self.layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: self.uniforms.as_entire_binding() }],
        });
        let Some((view, _)) = &self.target else { return };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("rain"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: clock.writes(),
            occlusion_query_set: None,
            multiview_mask: None,
        });
        if !quads.is_empty() {
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.set_vertex_buffer(0, self.instances.slice(..));
            pass.draw(0..6, 0..quads.len() as u32);
        }
    }

    /// The rain map, once drawn.
    pub fn view(&self) -> Option<&wgpu::TextureView> {
        self.target.as_ref().map(|t| &t.0)
    }

    /// Frees the map while another style is on.
    pub fn release(&mut self) {
        self.target = None;
    }
}

fn create_instances(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("rain quads"),
        size: (capacity * std::mem::size_of::<RainQuad>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}
