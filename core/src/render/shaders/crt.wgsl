// The CRT look, over the finished frame (render/crt.rs): the bright parts
// blurred into a glow at quarter size, then the frame shown as a tube would,
// bent like its glass, with scanlines that swap fields each frame
// (interlaced), the phosphor's afterglow, an aperture grille and darker
// corners. The amounts arrive already scaled by the CRT's strength.
// Colors are premultiplied, as everywhere in Flow.

struct Params {
    // Window size in pixels, and one texel of the glow textures.
    resolution: vec2<f32>,
    texel: vec2<f32>,
    // The blur's step, in glow texels.
    direction: vec2<f32>,
    // 0..1, the CRT setting.
    strength: f32,
    // Which lines are lit this frame: 0 or 1.
    field: f32,
    // Scanline height in pixels.
    line: f32,
    // How much of the last frame's light the phosphor still holds (0..1).
    decay: f32,
    // How strong the glow is.
    glow: f32,
    // How dark the dim lines get (0..1).
    scanlines: f32,
    // How strong the grille is (0..1).
    grille: f32,
    // How dark the corners get (0..1).
    vignette: f32,
    // How far a corner shows from beyond the frame (0..0.15).
    curvature: f32,
    _pad: f32,
    // The one color a monochrome tube shows (ui.wgsl's G.phosphor), its
    // alpha 1; all 0 for full color.
    phosphor: vec4<f32>,
};

@group(0) @binding(0) var<uniform> P: Params;
@group(0) @binding(1) var tex_a: texture_2d<f32>;
@group(0) @binding(2) var tex_b: texture_2d<f32>;
@group(0) @binding(3) var tex_c: texture_2d<f32>;
@group(0) @binding(4) var samp: sampler;

struct FullOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_full(@builtin(vertex_index) vi: u32) -> FullOut {
    let x = f32((vi << 1u) & 2u);
    let y = f32(vi & 2u);
    var out: FullOut;
    out.pos = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    out.uv = vec2<f32>(x, y);
    return out;
}

fn lum(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.299, 0.587, 0.114));
}

// What of `c` is bright enough to glow: text and the brightest of the liquid.
fn bright(c: vec3<f32>) -> vec3<f32> {
    return c * smoothstep(0.2, 0.75, lum(c));
}

// The frame (tex_a) at quarter size, keeping only what glows. The four
// samples each blend 2x2 pixels, so together they cover the 4x4 a glow
// texel stands for. Each is judged on its own before they're averaged:
// over all 16 pixels, a thin stroke of text would fall short of glowing.
@fragment
fn fs_bright(in: FullOut) -> @location(0) vec4<f32> {
    let h = P.texel * 0.25;
    var c = bright(textureSample(tex_a, samp, in.uv + vec2<f32>(-h.x, -h.y)).rgb);
    c += bright(textureSample(tex_a, samp, in.uv + vec2<f32>(h.x, -h.y)).rgb);
    c += bright(textureSample(tex_a, samp, in.uv + vec2<f32>(-h.x, h.y)).rgb);
    c += bright(textureSample(tex_a, samp, in.uv + vec2<f32>(h.x, h.y)).rgb);
    c *= 0.25;
    return vec4<f32>(c, lum(c));
}

// One direction of a Gaussian blur of tex_a.
@fragment
fn fs_blur(in: FullOut) -> @location(0) vec4<f32> {
    var weights = array<f32, 5>(0.2270270270, 0.1945945946, 0.1216216216, 0.0540540541, 0.0162162162);
    let step = P.direction * P.texel;
    var acc = textureSample(tex_a, samp, in.uv) * weights[0];
    for (var i = 1; i < 5; i++) {
        let o = step * f32(i);
        acc += (textureSample(tex_a, samp, in.uv + o) + textureSample(tex_a, samp, in.uv - o)) * weights[i];
    }
    return acc;
}

struct CrtOut {
    // What the window shows.
    @location(0) color: vec4<f32>,
    // What the phosphor holds, for the next frame's afterglow.
    @location(1) held: vec4<f32>,
};

// The picture bulges like a tube's glass: each point shows what lies a
// little further from the middle, the more so toward the corners. warp() in
// crt.rs bends the mouse the same way.
fn warp(uv: vec2<f32>) -> vec2<f32> {
    let c = uv * 2.0 - 1.0;
    let w = c * (1.0 + P.curvature * c.yx * c.yx);
    return w * 0.5 + 0.5;
}

// The frame (tex_a) shown as a tube would: bent like its glass, with what
// the phosphor held from the last frame (`before`), the glow (tex_c),
// scanlines, the grille, the corners, and a black bezel beyond the picture.
fn tube(screen: vec2<f32>, before: vec4<f32>) -> CrtOut {
    let uv = warp(screen);
    let frame = textureSample(tex_a, samp, uv);
    let glow = textureSample(tex_c, samp, uv).rgb * P.glow;
    // The window's own shape, with its rounded corners, for the bezel.
    let shape = smoothstep(0.0, 0.1, textureSample(tex_a, samp, screen).a);

    // Scanlines, following the glass: every other line darker. Interlaced,
    // the lit and the dim lines swap each frame. A dim line darkens what
    // shows through the glass too: black laid over it, in premultiplied terms.
    let px = uv * P.resolution;
    let dim = abs(floor(px.y / P.line) + P.field) % 2.0;
    let shade = P.scanlines * dim;
    let beam = vec4<f32>(frame.rgb * (1.0 - shade), frame.a * (1.0 - shade) + shade);
    // The phosphor keeps glowing a moment after the beam has passed: a line
    // dimmed this frame still holds some of the last, which steadies the
    // interlacing, and what moves or changes leaves a fading trace. Held in
    // 8 bits, a faint trace would round back to itself and stay for good, so
    // it also loses a step each frame.
    let held = max(beam, before * P.decay - vec4<f32>(1.0 / 255.0));
    var rgb = held.rgb;
    var a = held.a;

    // Aperture grille: thin columns of red, green and blue phosphor. The lit
    // channel is picked with select, not written as grille[column]: DX12's
    // shader compiler (FXC) can't assign to a vector at a run-time index.
    let column = u32(max(floor(px.x / P.line), 0.0)) % 3u;
    let lit = vec3<u32>(0u, 1u, 2u) == vec3<u32>(column);
    let grille = select(vec3<f32>(1.0 - 0.2 * P.grille), vec3<f32>(1.0 + 0.2 * P.grille), lit);
    rgb *= grille;

    // The glow spreads over the lines, as light in the glass does.
    rgb += glow;

    // A tube with one phosphor is brightest at that color, never paler: the
    // glow added over bright text would make it so, so what it makes is
    // brought back onto the phosphor, as bright as it came out and no
    // brighter than the phosphor at full. Its own shades (all the frame
    // holds, ui.wgsl's on_phosphor) come back as they were. Such a tube has
    // no grille (render/crt.rs hands it none).
    if (P.phosphor.a > 0.5) {
        rgb = P.phosphor.rgb * min(lum(rgb) / max(lum(P.phosphor.rgb), 0.001), 1.0);
    }

    // The tube's edge: darker toward the corners.
    let d = uv - 0.5;
    let r2 = dot(d, d);
    let corner = clamp(4.0 * P.vignette * r2 * r2, 0.0, 1.0);
    rgb *= 1.0 - corner;
    a = a * (1.0 - corner) + corner;

    // Beyond the bent picture, the tube's black bezel, its corners rounded
    // the more it bulges. Flat, the picture fills the window as before.
    let radius = P.curvature * 0.4 * min(P.resolution.x, P.resolution.y);
    let q = abs(d) * P.resolution - (0.5 * P.resolution - radius);
    let outside = length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - radius;
    let tube = select(1.0, 1.0 - smoothstep(-1.0, 1.0, outside), P.curvature > 0.0);

    let picture = vec4<f32>(rgb, clamp(max(a, max(rgb.r, max(rgb.g, rgb.b))), 0.0, 1.0));
    var out: CrtOut;
    out.color = mix(vec4<f32>(0.0, 0.0, 0.0, shape), picture, tube);
    out.held = held;
    return out;
}

// The tube with the afterglow from the last frame (tex_b), keeping this
// frame's for the next.
@fragment
fn fs_crt(in: FullOut) -> CrtOut {
    return tube(in.uv, textureSample(tex_b, samp, in.uv));
}

// The tube when nothing lingers (Phosphor decay at 0): no afterglow to read
// or keep.
@fragment
fn fs_crt_flat(in: FullOut) -> @location(0) vec4<f32> {
    return tube(in.uv, vec4<f32>(0.0)).color;
}

// A phosphor worn without the tube: the frame (tex_a) as it is, but each
// pixel brought back onto the phosphor as the tube brings it, its twin
// palette::onto_phosphor. A glow added over bright cells clips channel by
// channel in the 8-bit frame, the phosphor's strongest channel first, and
// comes out paler than the phosphor; here it's the phosphor at full. Every
// shade ui.wgsl's on_phosphor drew comes back as it was.
@fragment
fn fs_onto_phosphor(in: FullOut) -> @location(0) vec4<f32> {
    let c = textureSample(tex_a, samp, in.uv);
    let rgb = P.phosphor.rgb * min(lum(c.rgb) / max(lum(P.phosphor.rgb), 0.001), 1.0);
    return vec4<f32>(rgb, clamp(max(c.a, max(rgb.r, max(rgb.g, rgb.b))), 0.0, 1.0));
}
