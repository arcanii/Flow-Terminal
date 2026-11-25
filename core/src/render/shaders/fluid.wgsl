// Stable-fluids solver on render targets (x right, y down, velocity in texels/s).

struct Params {
    texel: vec2<f32>,
    dt: f32,
    dissipation: f32,
    point: vec2<f32>,
    radius: f32,
    aspect: f32,
    color: vec4<f32>,
    curl: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
};

@group(0) @binding(0) var<uniform> P: Params;
@group(0) @binding(1) var tex_a: texture_2d<f32>;
@group(0) @binding(2) var tex_b: texture_2d<f32>;
@group(0) @binding(3) var samp: sampler;

struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_full(@builtin(vertex_index) vi: u32) -> VOut {
    let x = f32((vi << 1u) & 2u);
    let y = f32(vi & 2u);
    var out: VOut;
    out.pos = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    out.uv = vec2<f32>(x, y);
    return out;
}

fn a_at(uv: vec2<f32>) -> vec4<f32> {
    return textureSampleLevel(tex_a, samp, uv, 0.0);
}

fn b_at(uv: vec2<f32>) -> vec4<f32> {
    return textureSampleLevel(tex_b, samp, uv, 0.0);
}

// A = velocity. Output: curl in .x
@fragment
fn fs_curl(in: VOut) -> @location(0) vec4<f32> {
    let t = P.texel;
    let L = a_at(in.uv - vec2<f32>(t.x, 0.0)).y;
    let R = a_at(in.uv + vec2<f32>(t.x, 0.0)).y;
    let T = a_at(in.uv - vec2<f32>(0.0, t.y)).x;
    let B = a_at(in.uv + vec2<f32>(0.0, t.y)).x;
    return vec4<f32>(0.5 * ((R - L) - (B - T)), 0.0, 0.0, 1.0);
}

// A = velocity, B = curl. Vorticity confinement keeps the swirls alive.
@fragment
fn fs_vorticity(in: VOut) -> @location(0) vec4<f32> {
    let t = P.texel;
    let cL = abs(b_at(in.uv - vec2<f32>(t.x, 0.0)).x);
    let cR = abs(b_at(in.uv + vec2<f32>(t.x, 0.0)).x);
    let cT = abs(b_at(in.uv - vec2<f32>(0.0, t.y)).x);
    let cB = abs(b_at(in.uv + vec2<f32>(0.0, t.y)).x);
    let c = b_at(in.uv).x;
    var n = 0.5 * vec2<f32>(cR - cL, cB - cT);
    n = n / (length(n) + 1e-5);
    let force = vec2<f32>(n.y * c, -n.x * c) * P.curl;
    var v = a_at(in.uv).xy + force * P.dt;
    v = clamp(v, vec2<f32>(-1000.0), vec2<f32>(1000.0));
    return vec4<f32>(v, 0.0, 1.0);
}

// A = velocity. Output: divergence in .x (walls reflect).
@fragment
fn fs_divergence(in: VOut) -> @location(0) vec4<f32> {
    let t = P.texel;
    let c = a_at(in.uv).xy;
    var L = a_at(in.uv - vec2<f32>(t.x, 0.0)).x;
    var R = a_at(in.uv + vec2<f32>(t.x, 0.0)).x;
    var T = a_at(in.uv - vec2<f32>(0.0, t.y)).y;
    var B = a_at(in.uv + vec2<f32>(0.0, t.y)).y;
    if (in.uv.x - t.x < 0.0) { L = -c.x; }
    if (in.uv.x + t.x > 1.0) { R = -c.x; }
    if (in.uv.y - t.y < 0.0) { T = -c.y; }
    if (in.uv.y + t.y > 1.0) { B = -c.y; }
    return vec4<f32>(0.5 * ((R - L) + (B - T)), 0.0, 0.0, 1.0);
}

// A = pressure, B = divergence. One Jacobi iteration.
@fragment
fn fs_pressure(in: VOut) -> @location(0) vec4<f32> {
    let t = P.texel;
    let L = a_at(in.uv - vec2<f32>(t.x, 0.0)).x;
    let R = a_at(in.uv + vec2<f32>(t.x, 0.0)).x;
    let T = a_at(in.uv - vec2<f32>(0.0, t.y)).x;
    let B = a_at(in.uv + vec2<f32>(0.0, t.y)).x;
    let div = b_at(in.uv).x;
    return vec4<f32>((L + R + T + B - div) * 0.25, 0.0, 0.0, 1.0);
}

// A = pressure, B = velocity. Projects velocity to be divergence free.
@fragment
fn fs_gradient(in: VOut) -> @location(0) vec4<f32> {
    let t = P.texel;
    let L = a_at(in.uv - vec2<f32>(t.x, 0.0)).x;
    let R = a_at(in.uv + vec2<f32>(t.x, 0.0)).x;
    let T = a_at(in.uv - vec2<f32>(0.0, t.y)).x;
    let B = a_at(in.uv + vec2<f32>(0.0, t.y)).x;
    let v = b_at(in.uv).xy - 0.5 * vec2<f32>(R - L, B - T);
    return vec4<f32>(v, 0.0, 1.0);
}

// A = velocity field, B = quantity to move (velocity or dye).
@fragment
fn fs_advect(in: VOut) -> @location(0) vec4<f32> {
    let coord = in.uv - P.dt * a_at(in.uv).xy * P.texel;
    let q = b_at(coord);
    return q * P.dissipation;
}

// A = field. Multiplies by `dissipation` (used to relax pressure).
@fragment
fn fs_scale(in: VOut) -> @location(0) vec4<f32> {
    return a_at(in.uv) * P.dissipation;
}

// A = wave state (x height, y vertical speed). One semi-implicit Euler step of
// the 2D wave equation: `curl` carries speed² in texels²/s², `dissipation`
// damps the speed and `color.x` slowly settles the height for this substep.
// `color.y` is a viscosity (texels²/s) on the speed, which damps a ripple by
// how short it is: the grid-sized ones fade fast, long swells barely notice.
@fragment
fn fs_wave(in: VOut) -> @location(0) vec4<f32> {
    let t = P.texel;
    let c = a_at(in.uv);
    let L = a_at(in.uv - vec2<f32>(t.x, 0.0));
    let R = a_at(in.uv + vec2<f32>(t.x, 0.0));
    let T = a_at(in.uv - vec2<f32>(0.0, t.y));
    let B = a_at(in.uv + vec2<f32>(0.0, t.y));
    let lap = L + R + T + B - 4.0 * c;
    let v = (c.y + (P.curl * lap.x + P.color.y * lap.y) * P.dt) * P.dissipation;
    let h = (c.x + v * P.dt) * P.color.x;
    return vec4<f32>(h, v, 0.0, 1.0);
}

// A = field. Adds a gaussian blob of `color` at `point`.
@fragment
fn fs_splat(in: VOut) -> @location(0) vec4<f32> {
    var p = in.uv - P.point;
    p.x = p.x * P.aspect;
    let s = exp(-dot(p, p) / P.radius);
    let base = a_at(in.uv);
    return vec4<f32>(base.xyz + P.color.xyz * s, 1.0);
}
