//! The CRT setting: the finished frame drawn as an old tube monitor would
//! show it (shaders/crt.wgsl). While it's on, the frame is drawn into a
//! texture of its own; its bright parts are blurred at quarter size into a
//! glow; and a last pass shows it bent like a tube's glass, with scanlines
//! that swap fields each frame, the phosphor's afterglow, an aperture grille
//! and darker corners. The afterglow is what the phosphor held, kept from one
//! frame to the next in a pair of textures that take turns, which exist only
//! while Phosphor decay is on. A phosphor worn without the tube takes a
//! last pass of its own (`CrtLayer::onto_phosphor`): the frame, drawn into
//! the same texture, brought back onto the phosphor as the tube brings it.

use std::time::Instant;

use bytemuck::{Pod, Zeroable};

/// The afterglow and the glow only hold light between 0 and 1, so 8 bits a
/// channel do; the shader makes sure a faint afterglow still fades in them.
const HELD: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// How far the picture bulges at full curvature: a corner shows what lies
/// this much further out.
const BULGE: f32 = 0.15;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Params {
    resolution: [f32; 2],
    texel: [f32; 2],
    direction: [f32; 2],
    strength: f32,
    field: f32,
    line: f32,
    decay: f32,
    glow: f32,
    scanlines: f32,
    grille: f32,
    vignette: f32,
    curvature: f32,
    _pad: f32,
    phosphor: [f32; 4],
}

/// The CRT as set (fx.crt and [crt] in the config), for one frame.
#[derive(Debug, Clone, Copy, Default)]
pub struct CrtLook {
    /// How strong it all is, 0..1; 0 is off.
    pub strength: f32,
    /// How dark every other line gets, 0..1.
    pub scanlines: f32,
    /// The lit and dim lines swap each frame.
    pub interlace: bool,
    /// The light bright text spreads, 0..2.
    pub glow: f32,
    /// Seconds the phosphor takes to fade.
    pub decay: f32,
    /// The red, green and blue stripes, 0..1.
    pub grille: f32,
    /// Darker corners, 0..1.
    pub vignette: f32,
    /// The bulge of the glass, 0..1.
    pub curvature: f32,
    /// Frames are coming steadily (the window is in front), so the lines may
    /// swap and the phosphor fade over them. Otherwise the lines hold still
    /// and nothing lingers.
    pub steady: bool,
}

impl CrtLook {
    /// The bulge as drawn, for [`warp`].
    pub fn bulge(&self) -> f32 {
        self.curvature * self.strength
    }

    /// The grille as drawn over a picture in `phosphor` (alpha 1 for one,
    /// all 0 for full color): none on a monochrome tube, whose one phosphor
    /// coats the glass whole, with no stripes of red, green and blue.
    fn grille_over(&self, phosphor: [f32; 4]) -> f32 {
        if phosphor[3] > 0.5 { 0.0 } else { self.grille * self.strength }
    }
}

/// Where a point of the curved picture shows from in the flat frame, both in
/// pixels of a window `size` big, for a `bulge` from [`CrtLook::bulge`]. The
/// shader bends the picture the same way (`warp` in crt.wgsl), so a click
/// lands on what's drawn under it.
pub fn warp(p: (f32, f32), size: (f32, f32), bulge: f32) -> (f32, f32) {
    if bulge <= 0.0 {
        return p;
    }
    let (x, y) = (p.0 / size.0.max(1.0) * 2.0 - 1.0, p.1 / size.1.max(1.0) * 2.0 - 1.0);
    let k = bulge * BULGE;
    let (x, y) = (x * (1.0 + k * y * y), y * (1.0 + k * x * x));
    ((x + 1.0) * 0.5 * size.0, (y + 1.0) * 0.5 * size.1)
}

/// Where a point `p` of the flat frame shows once the curve bends it: the
/// inverse of [`warp`], for placing what the OS draws over Flow (the input
/// method's candidates) beside what Flow drew.
pub fn unwarp(p: (f32, f32), size: (f32, f32), bulge: f32) -> (f32, f32) {
    // The bend is slight, so stepping back by however far `warp` misses
    // closes in on the point within a few rounds.
    let mut s = p;
    for _ in 0..8 {
        let w = warp(s, size, bulge);
        s = (s.0 - (w.0 - p.0), s.1 - (w.1 - p.1));
    }
    s
}

/// How the CRT shows one frame.
#[derive(Debug, Clone, Copy)]
pub struct CrtFrame {
    pub look: CrtLook,
    /// `--record`'s fixed step between frames, in seconds. Otherwise the
    /// afterglow fades by the time since the CRT last drew, which the
    /// liquid's clock doesn't tell: it also steps while nothing is drawn.
    pub fixed_dt: Option<f32>,
    /// The window's pixels per point.
    pub scale: f32,
    /// The one color a monochrome tube shows (`FrameStyle::phosphor`), its
    /// alpha 1, which the glow mustn't make paler and which has no grille;
    /// all 0 for full color.
    pub phosphor: [f32; 4],
}

/// The textures the CRT draws with, all for one window size.
struct Targets {
    size: (u32, u32),
    frame: wgpu::TextureView,
    /// What the phosphor holds: read from one, written to the other. None
    /// while nothing lingers (Phosphor decay at 0).
    held: Option<[wgpu::TextureView; 2]>,
    /// The glow, at quarter size, and the texture its blur passes through.
    /// None for a phosphor without the tube, which has no glow of its own.
    glow: Option<[wgpu::TextureView; 2]>,
}

/// The glow's size for a window `size` pixels big.
fn glow_size(size: (u32, u32)) -> (u32, u32) {
    (size.0.div_ceil(4), size.1.div_ceil(4))
}

pub struct CrtLayer {
    format: wgpu::TextureFormat,
    layout: wgpu::BindGroupLayout,
    bright: wgpu::RenderPipeline,
    blur: wgpu::RenderPipeline,
    crt: wgpu::RenderPipeline,
    /// The last pass without an afterglow to keep.
    flat: wgpu::RenderPipeline,
    /// The last pass of a phosphor worn without the tube.
    onto: wgpu::RenderPipeline,
    /// The passes' parameters: the bright pass and the CRT pass, then each
    /// direction of the blur.
    params: [wgpu::Buffer; 3],
    targets: Option<Targets>,
    /// Which of `held` the last frame wrote.
    current: usize,
    /// Which lines are lit this frame.
    field: bool,
    /// When the CRT last drew.
    last: Option<Instant>,
}

impl CrtLayer {
    /// `format` is the window's.
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("crt"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/crt.wgsl").into()),
        });
        let texture = |binding| wgpu::BindGroupLayoutEntry {
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
            label: Some("crt"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                texture(1),
                texture(2),
                texture(3),
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("crt"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = |label, entry, targets: &[Option<wgpu::ColorTargetState>]| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
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
                    targets,
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let target = |format| Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL });
        let bright = pipeline("crt bright", "fs_bright", &[target(HELD)]);
        let blur = pipeline("crt blur", "fs_blur", &[target(HELD)]);
        let crt = pipeline("crt", "fs_crt", &[target(format), target(HELD)]);
        let flat = pipeline("crt flat", "fs_crt_flat", &[target(format)]);
        let onto = pipeline("onto phosphor", "fs_onto_phosphor", &[target(format)]);
        let params = std::array::from_fn(|_| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("crt params"),
                size: std::mem::size_of::<Params>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        });
        Self { format, layout, bright, blur, crt, flat, onto, params, targets: None, current: 0, field: false, last: None }
    }

    /// Makes the textures for a window `size` pixels big, if they aren't,
    /// with the pair for the afterglow only when there is one, and the
    /// glow's only for the `tube` (a phosphor without it draws the frame
    /// alone).
    pub fn prepare(&mut self, device: &wgpu::Device, size: (u32, u32), afterglow: bool, tube: bool) {
        let size = (size.0.max(1), size.1.max(1));
        let make = |label, size: (u32, u32), format| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .create_view(&wgpu::TextureViewDescriptor::default())
        };
        let held = || [make("crt held", size, HELD), make("crt held", size, HELD)];
        let glow = || [make("crt glow", glow_size(size), HELD), make("crt glow", glow_size(size), HELD)];
        match &mut self.targets {
            Some(t) if t.size == size => {
                if t.held.is_some() != afterglow {
                    t.held = afterglow.then(held);
                }
                if t.glow.is_some() != tube {
                    t.glow = tube.then(glow);
                }
            }
            _ => self.targets = Some(Targets { size, frame: make("crt frame", size, self.format), held: afterglow.then(held), glow: tube.then(glow) }),
        }
    }

    /// What its textures hold on the GPU, while it's on.
    pub fn gpu_bytes(&self) -> u64 {
        let Some(t) = &self.targets else { return 0 };
        let held = if t.held.is_some() { 2 } else { 0 };
        let glow = if t.glow.is_some() { 2 } else { 0 };
        super::texture_bytes(t.size, self.format) + held * super::texture_bytes(t.size, HELD) + glow * super::texture_bytes(glow_size(t.size), HELD)
    }

    /// The texture to draw the frame into, once prepared.
    pub fn frame(&self) -> Option<&wgpu::TextureView> {
        self.targets.as_ref().map(|t| &t.frame)
    }

    /// Shows the frame drawn into [`CrtLayer::frame`] on `target`.
    pub fn draw(
        &mut self,
        gpu: (&wgpu::Device, &wgpu::Queue),
        encoder: &mut wgpu::CommandEncoder,
        sampler: &wgpu::Sampler,
        target: &wgpu::TextureView,
        frame: CrtFrame,
        clock: &super::timing::GpuClock,
    ) {
        let (device, queue) = gpu;
        let (look, k) = (frame.look, frame.look.strength);
        if look.steady && look.interlace {
            self.field = !self.field;
        }
        let dt = frame.fixed_dt.unwrap_or_else(|| self.last.map_or(1.0, |t| t.elapsed().as_secs_f32()));
        self.last = Some(Instant::now());
        let Some(t) = &self.targets else { return };
        let Some([glow, spread]) = &t.glow else { return };
        let glow_px = glow_size(t.size);
        let fade = look.decay * k;
        let decay = if look.steady && fade > 0.0 { (-dt / fade).exp() } else { 0.0 };
        let params = Params {
            resolution: [t.size.0 as f32, t.size.1 as f32],
            texel: [1.0 / glow_px.0 as f32, 1.0 / glow_px.1 as f32],
            direction: [0.0; 2],
            strength: k,
            field: if self.field { 1.0 } else { 0.0 },
            line: frame.scale.round().max(1.0),
            decay,
            glow: look.glow * k,
            scanlines: look.scanlines * k,
            grille: look.grille_over(frame.phosphor),
            vignette: look.vignette * k,
            curvature: look.bulge() * BULGE,
            _pad: 0.0,
            phosphor: frame.phosphor,
        };
        queue.write_buffer(&self.params[0], 0, bytemuck::bytes_of(&params));
        // Steps of three quarters of a glow texel: 3 px of the window, as
        // wide a glow as it always had.
        queue.write_buffer(&self.params[1], 0, bytemuck::bytes_of(&Params { direction: [0.75, 0.0], ..params }));
        queue.write_buffer(&self.params[2], 0, bytemuck::bytes_of(&Params { direction: [0.0, 0.75], ..params }));

        let bind = |params: &wgpu::Buffer, a: &wgpu::TextureView, b: &wgpu::TextureView, c: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("crt"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: params.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(a) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(b) },
                    wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(c) },
                    wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::Sampler(sampler) },
                ],
            })
        };
        let pass = |encoder: &mut wgpu::CommandEncoder,
                    label,
                    pipeline: &wgpu::RenderPipeline,
                    bind: &wgpu::BindGroup,
                    targets: &[Option<wgpu::RenderPassColorAttachment>]| {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(label),
                color_attachments: targets,
                depth_stencil_attachment: None,
                timestamp_writes: clock.writes(),
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, bind, &[]);
            pass.draw(0..3, 0..1);
        };
        let out = |view| {
            Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
            })
        };
        pass(encoder, "crt bright", &self.bright, &bind(&self.params[0], &t.frame, &t.frame, &t.frame), &[out(glow)]);
        pass(encoder, "crt blur", &self.blur, &bind(&self.params[1], glow, glow, glow), &[out(spread)]);
        pass(encoder, "crt blur", &self.blur, &bind(&self.params[2], spread, spread, spread), &[out(glow)]);
        match &t.held {
            Some(held) => {
                let (before, after) = (&held[self.current], &held[1 - self.current]);
                pass(encoder, "crt", &self.crt, &bind(&self.params[0], &t.frame, before, glow), &[out(target), out(after)]);
                self.current = 1 - self.current;
            }
            // Nothing is read from the afterglow's binding here.
            None => pass(encoder, "crt", &self.flat, &bind(&self.params[0], &t.frame, &t.frame, glow), &[out(target)]),
        }
    }

    /// Shows the frame drawn into [`CrtLayer::frame`] on `target` as a
    /// monochrome screen of `phosphor` without the tube: as it is, but each
    /// pixel brought back onto the phosphor (`fs_onto_phosphor`), so a glow
    /// that clipped over bright cells is the phosphor at full, not paler.
    pub fn onto_phosphor(
        &mut self,
        gpu: (&wgpu::Device, &wgpu::Queue),
        encoder: &mut wgpu::CommandEncoder,
        sampler: &wgpu::Sampler,
        target: &wgpu::TextureView,
        phosphor: [f32; 4],
        clock: &super::timing::GpuClock,
    ) {
        let (device, queue) = gpu;
        let Some(t) = &self.targets else { return };
        let params = Params { resolution: [t.size.0 as f32, t.size.1 as f32], phosphor, ..Params::zeroed() };
        queue.write_buffer(&self.params[0], 0, bytemuck::bytes_of(&params));
        let frame = wgpu::BindingResource::TextureView(&t.frame);
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("onto phosphor"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.params[0].as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: frame.clone() },
                wgpu::BindGroupEntry { binding: 2, resource: frame.clone() },
                wgpu::BindGroupEntry { binding: 3, resource: frame },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::Sampler(sampler) },
            ],
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("onto phosphor"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: clock.writes(),
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.onto);
        pass.set_bind_group(0, &bind, &[]);
        pass.draw(0..3, 0..1);
    }

    /// Frees the textures while the CRT is off.
    pub fn release(&mut self) {
        self.targets = None;
        self.last = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_monochrome_tube_has_no_grille() {
        let look = CrtLook { strength: 0.8, grille: 0.4, ..CrtLook::default() };
        assert!((look.grille_over([0.0; 4]) - 0.32).abs() < 1e-6);
        assert_eq!(look.grille_over([0.25, 1.0, 0.43, 1.0]), 0.0);
    }

    #[test]
    fn the_curve_leaves_the_middle_and_bends_the_edges_outward() {
        let size = (1000.0, 600.0);
        // Flat, nothing moves.
        assert_eq!(warp((123.0, 456.0), size, 0.0), (123.0, 456.0));
        // The middle stays put, and so do the middles of the edges' far axis.
        let (x, y) = warp((500.0, 300.0), size, 1.0);
        assert!((x - 500.0).abs() < 1e-3 && (y - 300.0).abs() < 1e-3);
        // Near a corner, the pointer shows what lies further out.
        let (x, y) = warp((950.0, 570.0), size, 1.0);
        assert!(x > 950.0 && y > 570.0, "{x} {y}");
        // The same distance on the other side, mirrored.
        let (x2, y2) = warp((50.0, 30.0), size, 1.0);
        assert!(((1000.0 - x) - x2).abs() < 1e-3 && ((600.0 - y) - y2).abs() < 1e-3);
    }

    #[test]
    fn unwarping_finds_where_the_curve_shows_a_point() {
        let size = (1000.0, 600.0);
        for p in [(500.0, 300.0), (950.0, 570.0), (20.0, 15.0), (700.0, 40.0)] {
            let s = unwarp(p, size, 1.0);
            let back = warp(s, size, 1.0);
            assert!((back.0 - p.0).abs() < 0.1 && (back.1 - p.1).abs() < 0.1, "{p:?} {s:?} {back:?}");
        }
        assert_eq!(unwarp((123.0, 456.0), size, 0.0), (123.0, 456.0));
    }
}
