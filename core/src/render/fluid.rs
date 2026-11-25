//! GPU simulations behind the glass: stable fluids (velocity + dye) and, for
//! the water styles, a wave equation for ripples.

use bytemuck::{Pod, Zeroable};

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const SLOT: u64 = 256;
const MAX_PASSES: u64 = 128;
const PRESSURE_ITERATIONS: usize = 18;
const SIM_SIDE: u32 = 192;
const DYE_SIDE: u32 = 640;
const WAVE_SIDE: u32 = 360;
/// A texture the glass doesn't read: bound, but not worth memory.
const TINY: (u32, u32) = (8, 8);
/// Ripple speed in wave texels per second.
const WAVE_SPEED: f32 = 52.0;
/// Seconds of flow time per wave step. Always the same size: the solver is
/// only stable at a fixed step. With the step following the frame time,
/// jittery frames pump energy into the shortest ripples until a lattice
/// covers the water within seconds.
const WAVE_STEP: f32 = 1.0 / 240.0;
/// How fast ripples die down, per second (on their speed).
const WAVE_DAMPING: f32 = 1.6;
/// Extra damping for short ripples, in texels²/s: grid-sized ones fade in
/// about a tenth of a second, long swells hardly feel it.
const WAVE_VISCOSITY: f32 = 3.0;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default)]
struct Params {
    texel: [f32; 2],
    dt: f32,
    dissipation: f32,
    point: [f32; 2],
    radius: f32,
    aspect: f32,
    color: [f32; 4],
    curl: f32,
    _pad: [f32; 3],
}

/// What the liquid is made of; changes how it moves and fades.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Material {
    Clouds,
    Water,
    Oil,
    /// The lights outside a rainy window: slow drifting color that lingers.
    Rain,
}

impl Material {
    /// (vorticity, velocity fade per second, dye fade per second)
    fn params(self) -> (f32, f32, f32) {
        match self {
            Material::Clouds => (28.0, 0.2, 0.3),
            Material::Water => (16.0, 0.6, 0.8),
            // Viscous: swirls die out fast, the slick lingers.
            Material::Oil => (6.0, 1.4, 0.05),
            Material::Rain => (10.0, 0.5, 0.12),
        }
    }
}

/// A blob of force and color dropped into the liquid.
#[derive(Clone, Copy, Debug)]
pub struct Splat {
    /// Position in 0..1 window coordinates (y down).
    pub pos: [f32; 2],
    /// Push in simulation texels per second.
    pub force: [f32; 2],
    pub color: [f32; 3],
    /// Blob size as a fraction of the window height.
    pub radius: f32,
}

/// A drop on the water surface (negative amp pushes the surface down).
#[derive(Clone, Copy, Debug)]
pub struct Drop {
    pub pos: [f32; 2],
    pub amp: f32,
    /// Size as a fraction of the window height.
    pub radius: f32,
}

#[derive(Clone, Copy)]
enum Pipe {
    Curl,
    Vorticity,
    Divergence,
    Pressure,
    Gradient,
    Advect,
    Scale,
    Splat,
    Wave,
}

#[derive(Clone, Copy)]
enum Src {
    Vel,
    Dye,
    P,
    Div,
    Curl,
    Wave,
}

#[derive(Clone, Copy)]
enum Dst {
    Vel,
    Dye,
    P,
    Div,
    Curl,
    Wave,
}

struct Op {
    pipe: Pipe,
    a: Src,
    b: Src,
    out: Dst,
    offset: u32,
}

struct Target {
    view: wgpu::TextureView,
    /// What it holds on the GPU.
    bytes: u64,
}

impl Target {
    fn new(device: &wgpu::Device, (w, h): (u32, u32), label: &str) -> Self {
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        Self { view: tex.create_view(&wgpu::TextureViewDescriptor::default()), bytes: super::texture_bytes((w, h), FORMAT) }
    }
}

struct Pipelines {
    curl: wgpu::RenderPipeline,
    vorticity: wgpu::RenderPipeline,
    divergence: wgpu::RenderPipeline,
    pressure: wgpu::RenderPipeline,
    gradient: wgpu::RenderPipeline,
    advect: wgpu::RenderPipeline,
    scale: wgpu::RenderPipeline,
    splat: wgpu::RenderPipeline,
    wave: wgpu::RenderPipeline,
}

pub struct Fluid {
    pipelines: Pipelines,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniforms: wgpu::Buffer,
    vel: [Target; 2],
    dye: [Target; 2],
    pressure: [Target; 2],
    wave: [Target; 2],
    divergence: Target,
    curl: Target,
    vel_i: usize,
    dye_i: usize,
    p_i: usize,
    wave_i: usize,
    pub sim_size: (u32, u32),
    pub dye_size: (u32, u32),
    pub wave_size: (u32, u32),
    /// The window's size in pixels, which the textures are sized from.
    window: (u32, u32),
    /// The glass shows the liquid (it isn't plain glass), and its ripples
    /// (it's water).
    shown: (bool, bool),
    pub splats: Vec<Splat>,
    pub drops: Vec<Drop>,
    pub material: Material,
    cleared: bool,
    /// Flow time not yet spent on whole wave steps.
    wave_clock: f32,
}

/// Spends `dt` of flow time on whole wave steps and keeps the rest in
/// `clock` for the next frame. Returns how many steps to run now.
fn wave_steps(clock: &mut f32, dt: f32) -> usize {
    *clock += dt;
    let steps = (*clock / WAVE_STEP).floor();
    *clock -= steps * WAVE_STEP;
    steps as usize
}

fn sizes(width: u32, height: u32, max_side: u32) -> (u32, u32) {
    let w = width.max(1) as f32;
    let h = height.max(1) as f32;
    let s = max_side as f32 / w.max(h);
    (((w * s).round() as u32).max(8), ((h * s).round() as u32).max(8))
}

impl Fluid {
    /// What its textures hold on the GPU.
    pub fn gpu_bytes(&self) -> u64 {
        let pairs = [&self.vel, &self.dye, &self.pressure, &self.wave];
        pairs.iter().flat_map(|p| p.iter()).chain([&self.divergence, &self.curl]).map(|t| t.bytes).sum()
    }

    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("fluid"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/fluid.wgsl").into()),
        });
        let tex_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("fluid"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<Params>() as u64),
                    },
                    count: None,
                },
                tex_entry(1),
                tex_entry(2),
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("fluid"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let make = |entry: &str| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_full"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: FORMAT,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let pipelines = Pipelines {
            curl: make("fs_curl"),
            vorticity: make("fs_vorticity"),
            divergence: make("fs_divergence"),
            pressure: make("fs_pressure"),
            gradient: make("fs_gradient"),
            advect: make("fs_advect"),
            scale: make("fs_scale"),
            splat: make("fs_splat"),
            wave: make("fs_wave"),
        };
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("fluid"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("fluid params"),
            size: SLOT * MAX_PASSES,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let tiny = TINY;
        let mut fluid = Self {
            pipelines,
            layout,
            sampler,
            uniforms,
            vel: [Target::new(device, tiny, "vel"), Target::new(device, tiny, "vel")],
            dye: [Target::new(device, tiny, "dye"), Target::new(device, tiny, "dye")],
            pressure: [Target::new(device, tiny, "p"), Target::new(device, tiny, "p")],
            wave: [Target::new(device, tiny, "wave"), Target::new(device, tiny, "wave")],
            divergence: Target::new(device, tiny, "div"),
            curl: Target::new(device, tiny, "curl"),
            vel_i: 0,
            dye_i: 0,
            p_i: 0,
            wave_i: 0,
            sim_size: tiny,
            dye_size: tiny,
            wave_size: tiny,
            window: (width, height),
            shown: (true, true),
            splats: Vec::new(),
            drops: Vec::new(),
            material: Material::Clouds,
            cleared: false,
            wave_clock: 0.0,
        };
        fluid.allocate(device);
        fluid
    }

    /// The textures' sizes for the window and what the glass shows: the
    /// simulation's, the dye's and the ripples'.
    fn wanted(&self) -> [(u32, u32); 3] {
        let ((w, h), (liquid, ripples)) = (self.window, self.shown);
        let size = |on: bool, side| if on { sizes(w, h, side) } else { TINY };
        [size(liquid, SIM_SIDE), size(liquid, DYE_SIDE), size(liquid && ripples, WAVE_SIDE)]
    }

    fn allocate(&mut self, device: &wgpu::Device) {
        let [sim, dye, wave] = self.wanted();
        (self.sim_size, self.dye_size, self.wave_size) = (sim, dye, wave);
        self.vel = [Target::new(device, sim, "vel0"), Target::new(device, sim, "vel1")];
        self.pressure = [Target::new(device, sim, "p0"), Target::new(device, sim, "p1")];
        self.divergence = Target::new(device, sim, "div");
        self.curl = Target::new(device, sim, "curl");
        self.dye = [Target::new(device, dye, "dye0"), Target::new(device, dye, "dye1")];
        self.wave = [Target::new(device, wave, "wave0"), Target::new(device, wave, "wave1")];
        self.cleared = false;
    }

    /// Makes the textures again if they should be another size (the liquid
    /// restarts calm).
    fn refit(&mut self, device: &wgpu::Device) {
        if self.wanted() != [self.sim_size, self.dye_size, self.wave_size] {
            self.allocate(device);
        }
    }

    /// New render targets for a new window size.
    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.window = (width, height);
        self.refit(device);
    }

    /// Keeps full-size textures only for what the glass shows: none for plain
    /// glass, and the ripples only for water. A style that needs them again
    /// starts calm, as it does after any change of style.
    pub fn show(&mut self, device: &wgpu::Device, liquid: bool, ripples: bool) {
        self.shown = (liquid, ripples);
        self.refit(device);
    }

    /// Switches material; the old liquid is drained so styles don't mix.
    pub fn set_material(&mut self, material: Material) {
        if material != self.material {
            self.material = material;
            self.cleared = false;
            self.splats.clear();
            self.drops.clear();
        }
    }

    pub fn velocity_view(&self) -> &wgpu::TextureView {
        &self.vel[self.vel_i].view
    }

    pub fn dye_view(&self) -> &wgpu::TextureView {
        &self.dye[self.dye_i].view
    }

    /// Ripple heights (x) for the water styles.
    pub fn wave_view(&self) -> &wgpu::TextureView {
        &self.wave[self.wave_i].view
    }

    /// Advances the liquid `dt` seconds, each of its passes timed by
    /// `clock` while it times the frame.
    pub fn step(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, dt: f32, clock: &super::timing::GpuClock) {
        // No lower bound: a slowed-down flow passes small steps on purpose.
        let dt = dt.clamp(0.0, 1.0 / 30.0);
        let (curl, vel_fade, dye_fade) = self.material.params();
        let (sw, sh) = self.sim_size;
        let (dw, dh) = self.dye_size;
        let (ww, wh) = self.wave_size;
        let sim_texel = [1.0 / sw as f32, 1.0 / sh as f32];
        let dye_texel = [1.0 / dw as f32, 1.0 / dh as f32];
        let aspect = sw as f32 / sh as f32;

        let mut params: Vec<Params> = Vec::new();
        let mut push = |p: Params| -> u32 {
            params.push(p);
            ((params.len() - 1) as u64 * SLOT) as u32
        };
        let base = Params { texel: sim_texel, dt, dissipation: 1.0, aspect, curl, ..Default::default() };

        let mut ops: Vec<Op> = Vec::new();
        let off = push(base);
        ops.push(Op { pipe: Pipe::Curl, a: Src::Vel, b: Src::Vel, out: Dst::Curl, offset: off });
        ops.push(Op { pipe: Pipe::Vorticity, a: Src::Vel, b: Src::Curl, out: Dst::Vel, offset: off });

        let splats: Vec<Splat> = self.splats.drain(..).take(24).collect();
        for s in &splats {
            let r = (s.radius * s.radius).max(1e-5);
            if s.force != [0.0, 0.0] {
                let off_v = push(Params { point: s.pos, radius: r, color: [s.force[0], s.force[1], 0.0, 0.0], ..base });
                ops.push(Op { pipe: Pipe::Splat, a: Src::Vel, b: Src::Vel, out: Dst::Vel, offset: off_v });
            }
            if s.color != [0.0, 0.0, 0.0] {
                let off_d = push(Params {
                    texel: dye_texel,
                    point: s.pos,
                    radius: r,
                    color: [s.color[0], s.color[1], s.color[2], 0.0],
                    aspect: dw as f32 / dh as f32,
                    ..base
                });
                ops.push(Op { pipe: Pipe::Splat, a: Src::Dye, b: Src::Dye, out: Dst::Dye, offset: off_d });
            }
        }

        ops.push(Op { pipe: Pipe::Divergence, a: Src::Vel, b: Src::Vel, out: Dst::Div, offset: off });
        let off_relax = push(Params { dissipation: 0.8, ..base });
        ops.push(Op { pipe: Pipe::Scale, a: Src::P, b: Src::P, out: Dst::P, offset: off_relax });
        for _ in 0..PRESSURE_ITERATIONS {
            ops.push(Op { pipe: Pipe::Pressure, a: Src::P, b: Src::Div, out: Dst::P, offset: off });
        }
        ops.push(Op { pipe: Pipe::Gradient, a: Src::P, b: Src::Vel, out: Dst::Vel, offset: off });
        let off_adv_v = push(Params { dissipation: 1.0 / (1.0 + dt * vel_fade), ..base });
        ops.push(Op { pipe: Pipe::Advect, a: Src::Vel, b: Src::Vel, out: Dst::Vel, offset: off_adv_v });
        let off_adv_d = push(Params { dissipation: 1.0 / (1.0 + dt * dye_fade), ..base });
        ops.push(Op { pipe: Pipe::Advect, a: Src::Vel, b: Src::Dye, out: Dst::Dye, offset: off_adv_d });

        let drops: Vec<Drop> = self.drops.drain(..).take(16).collect();
        if self.material == Material::Water {
            let wave_base = Params { texel: [1.0 / ww as f32, 1.0 / wh as f32], aspect: ww as f32 / wh as f32, ..base };
            for d in &drops {
                let off_drop = push(Params {
                    point: d.pos,
                    radius: (d.radius * d.radius).max(1e-6),
                    color: [d.amp, 0.0, 0.0, 0.0],
                    ..wave_base
                });
                ops.push(Op { pipe: Pipe::Splat, a: Src::Wave, b: Src::Wave, out: Dst::Wave, offset: off_drop });
            }
            let steps = wave_steps(&mut self.wave_clock, dt);
            let off_wave = push(Params {
                dt: WAVE_STEP,
                curl: WAVE_SPEED * WAVE_SPEED,
                dissipation: (-WAVE_STEP * WAVE_DAMPING).exp(),
                color: [(-WAVE_STEP * 0.08f32).exp(), WAVE_VISCOSITY, 0.0, 0.0],
                ..wave_base
            });
            for _ in 0..steps {
                ops.push(Op { pipe: Pipe::Wave, a: Src::Wave, b: Src::Wave, out: Dst::Wave, offset: off_wave });
            }
        }

        let bytes: Vec<u8> = params
            .iter()
            .flat_map(|p| {
                let mut slot = bytemuck::bytes_of(p).to_vec();
                slot.resize(SLOT as usize, 0);
                slot
            })
            .collect();
        queue.write_buffer(&self.uniforms, 0, &bytes);

        if !self.cleared {
            for view in [
                &self.vel[0].view,
                &self.vel[1].view,
                &self.dye[0].view,
                &self.dye[1].view,
                &self.pressure[0].view,
                &self.pressure[1].view,
                &self.wave[0].view,
                &self.wave[1].view,
            ] {
                clear(encoder, view);
            }
            self.cleared = true;
        }

        for Op { pipe, a, b, out, offset } in ops {
            {
                let this = &*self;
                let src = |s: Src| -> &wgpu::TextureView {
                    match s {
                        Src::Vel => &this.vel[this.vel_i].view,
                        Src::Dye => &this.dye[this.dye_i].view,
                        Src::P => &this.pressure[this.p_i].view,
                        Src::Div => &this.divergence.view,
                        Src::Curl => &this.curl.view,
                        Src::Wave => &this.wave[this.wave_i].view,
                    }
                };
                let out_view: &wgpu::TextureView = match out {
                    Dst::Vel => &this.vel[1 - this.vel_i].view,
                    Dst::Dye => &this.dye[1 - this.dye_i].view,
                    Dst::P => &this.pressure[1 - this.p_i].view,
                    Dst::Div => &this.divergence.view,
                    Dst::Curl => &this.curl.view,
                    Dst::Wave => &this.wave[1 - this.wave_i].view,
                };
                let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: None,
                    layout: &this.layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                                buffer: &this.uniforms,
                                offset: 0,
                                size: wgpu::BufferSize::new(std::mem::size_of::<Params>() as u64),
                            }),
                        },
                        wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(src(a)) },
                        wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(src(b)) },
                        wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::Sampler(&this.sampler) },
                    ],
                });
                let pipeline = match pipe {
                    Pipe::Curl => &this.pipelines.curl,
                    Pipe::Vorticity => &this.pipelines.vorticity,
                    Pipe::Divergence => &this.pipelines.divergence,
                    Pipe::Pressure => &this.pipelines.pressure,
                    Pipe::Gradient => &this.pipelines.gradient,
                    Pipe::Advect => &this.pipelines.advect,
                    Pipe::Scale => &this.pipelines.scale,
                    Pipe::Splat => &this.pipelines.splat,
                    Pipe::Wave => &this.pipelines.wave,
                };
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: out_view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: clock.writes(),
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, &bind, &[offset]);
                pass.draw(0..3, 0..1);
            }
            match out {
                Dst::Vel => self.vel_i = 1 - self.vel_i,
                Dst::Dye => self.dye_i = 1 - self.dye_i,
                Dst::P => self.p_i = 1 - self.p_i,
                Dst::Wave => self.wave_i = 1 - self.wave_i,
                Dst::Div | Dst::Curl => {}
            }
        }
    }
}

fn clear(encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView) {
    let _ = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("clear"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
}

#[cfg(test)]
mod tests {
    use super::{wave_steps, WAVE_DAMPING, WAVE_SPEED, WAVE_STEP, WAVE_VISCOSITY};

    /// `fs_wave`'s Laplacian at (x, y), with edges clamped like the sampler.
    fn lap(f: &[f32], w: usize, ht: usize, x: isize, y: isize) -> f32 {
        let at = |x: isize, y: isize| f[y.clamp(0, ht as isize - 1) as usize * w + x.clamp(0, w as isize - 1) as usize];
        at(x - 1, y) + at(x + 1, y) + at(x, y - 1) + at(x, y + 1) - 4.0 * at(x, y)
    }

    /// One `fs_wave` step on the CPU: height and speed per texel.
    fn wave_step(h: &mut Vec<f32>, v: &mut Vec<f32>, w: usize, ht: usize, dt: f32) {
        let (damp, settle) = ((-dt * WAVE_DAMPING).exp(), (-dt * 0.08f32).exp());
        let mut nh = vec![0.0; h.len()];
        let mut nv = vec![0.0; v.len()];
        for y in 0..ht as isize {
            for x in 0..w as isize {
                let i = y as usize * w + x as usize;
                let pull = WAVE_SPEED * WAVE_SPEED * lap(h, w, ht, x, y) + WAVE_VISCOSITY * lap(v, w, ht, x, y);
                nv[i] = (v[i] + pull * dt) * damp;
                nh[i] = (h[i] + nv[i] * dt) * settle;
            }
        }
        *h = nh;
        *v = nv;
    }

    /// Wave energy in height units: speed² / c² plus the stretch, -h·∇²h.
    fn energy(h: &[f32], v: &[f32], w: usize, ht: usize) -> f32 {
        (0..h.len())
            .map(|i| (v[i] / WAVE_SPEED).powi(2) - h[i] * lap(h, w, ht, (i % w) as isize, (i / w) as isize))
            .sum()
    }

    /// Runs `seconds` of frames with lengths from `frame_dt`, stepping the
    /// waves the way `Fluid::step` does.
    fn run(h: &mut Vec<f32>, v: &mut Vec<f32>, w: usize, ht: usize, seconds: f32, mut frame_dt: impl FnMut() -> f32) {
        let (mut clock, mut t) = (0.0, 0.0);
        while t < seconds {
            let dt = frame_dt().min(1.0 / 30.0);
            t += dt;
            for _ in 0..wave_steps(&mut clock, dt) {
                wave_step(h, v, w, ht, WAVE_STEP);
            }
        }
    }

    fn noise(seed: &mut u32) -> f32 {
        *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (*seed >> 8) as f32 / (1 << 24) as f32 - 0.5
    }

    #[test]
    fn jittery_frames_cant_blow_up_the_ripples() {
        // 144 Hz at flow speed 0.7 with a quarter of the frames running long:
        // with steps that followed the frame time, this exploded in seconds.
        let (w, ht) = (64, 48);
        let mut seed = 1;
        let mut h: Vec<f32> = (0..w * ht).map(|_| noise(&mut seed)).collect();
        let mut v: Vec<f32> = (0..w * ht).map(|_| noise(&mut seed) * WAVE_SPEED).collect();
        let start = energy(&h, &v, w, ht);
        let base = 0.7 / 144.0;
        run(&mut h, &mut v, w, ht, 20.0, || if noise(&mut seed) < -0.25 { 2.0 * base } else { base });
        let end = energy(&h, &v, w, ht);
        assert!(end.is_finite() && end < start * 0.01, "energy went from {start} to {end}");
    }

    #[test]
    fn ripples_decay_short_ones_first() {
        let (w, ht) = (64, 64);
        let frame = || 1.0 / 60.0;
        // Grid-sized ripples are gone within half a second.
        let mut h: Vec<f32> = (0..w * ht).map(|i| if (i % w + i / w) % 2 == 0 { 1.0 } else { -1.0 }).collect();
        let mut v = vec![0.0; w * ht];
        let start = energy(&h, &v, w, ht);
        run(&mut h, &mut v, w, ht, 0.5, frame);
        assert!(energy(&h, &v, w, ht) < start * 0.01, "grid ripples kept {}", energy(&h, &v, w, ht) / start);

        // A ripple 32 texels long is still there after a second, and has
        // died down after three.
        let wave = |i: usize| ((i % w) as f32 * std::f32::consts::TAU / 32.0).sin();
        let mut h: Vec<f32> = (0..w * ht).map(wave).collect();
        let mut v = vec![0.0; w * ht];
        let start = energy(&h, &v, w, ht);
        run(&mut h, &mut v, w, ht, 1.0, frame);
        let after_1s = energy(&h, &v, w, ht) / start;
        run(&mut h, &mut v, w, ht, 2.0, frame);
        let after_3s = energy(&h, &v, w, ht) / start;
        assert!(after_1s > 0.1, "a long ripple kept only {after_1s} after 1 s");
        assert!(after_3s < 0.02, "a long ripple still had {after_3s} after 3 s");
    }

    #[test]
    fn wave_steps_carry_the_remainder() {
        let mut clock = 0.0;
        let total: usize = (0..144).map(|_| wave_steps(&mut clock, 0.7 / 144.0)).sum();
        // 0.7 s of flow time is 168 steps of 1/240 s, give or take rounding.
        assert!((167..=168).contains(&total), "{total} steps");
        assert!((0.0..WAVE_STEP).contains(&clock));
    }
}
