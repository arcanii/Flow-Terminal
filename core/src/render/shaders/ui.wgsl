// Glass background and 2D quads (text, panels, glows). Output is premultiplied alpha.

struct Globals {
    resolution: vec2<f32>,
    time: f32,
    opacity: f32,
    corner_radius: f32,
    ripple: f32,
    focus: f32,
    activity: f32,
    tint: vec4<f32>,
    accent: vec4<f32>,
    accent2: vec4<f32>,
    dye_texel: vec2<f32>,
    dye_gain: f32,
    opaque: f32,
    caustics: f32,
    // 0 clouds, 1 water seen from above, 2 water seen from below, 3 oil, 4 rain
    mode: f32,
    sun: f32,
    // UI scale factor (DPI), for placing effects relative to the title bar.
    ui_scale: f32,
    // Lightning lighting the clouds: x, y (0..1), intensity, radius
    flash: vec4<f32>,
    wave_texel: vec2<f32>,
    // How wet the rain style's glass is all over, 0..1 (`sheet`).
    wet: f32,
    _pad1: f32,
    // A ring spreading from where something happened: x, y (0..1), seconds
    // since it began, strength (0: none); its color and width.
    pulse: vec4<f32>,
    pulse_color: vec4<f32>,
    // The progress tank: how full, how much it shows, 1 with a figure or 0
    // sloshing without one, its clock; its color.
    progress: vec4<f32>,
    progress_color: vec4<f32>,
    // The one color everything is shown in, a monochrome tube's phosphor,
    // its alpha 1 (`on_phosphor`); all 0 for full color.
    phosphor: vec4<f32>,
};

@group(0) @binding(0) var<uniform> G: Globals;
// Glass pass: dye. Quad pass: glyph atlas.
@group(0) @binding(1) var tex_main: texture_2d<f32>;
@group(0) @binding(2) var vel_tex: texture_2d<f32>;
@group(0) @binding(3) var samp: sampler;
// Ripple heights (x) for the water styles; the rain map for rain.
@group(0) @binding(4) var wave_tex: texture_2d<f32>;

const TAU: f32 = 6.2831853;

fn sd_round_box(p: vec2<f32>, b: vec2<f32>, r: f32) -> f32 {
    let rr = min(r, min(b.x, b.y));
    let q = abs(p) - b + vec2<f32>(rr);
    return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - rr;
}

fn lum(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.299, 0.587, 0.114));
}

// The share of its brightest part a saturated color is shown at, at the
// least, on a phosphor (palette::PHOSPHOR_FLOOR): blue text on black would
// all but vanish by luma alone.
const PHOSPHOR_FLOOR: f32 = 0.35;

// A premultiplied color as a monochrome monitor of G.phosphor shows it, the
// twin of palette::on_phosphor: a shade of the phosphor as bright as the
// color's luma, the brightness such a monitor took from the video signal
// (Rec. 601's, `lum`, as composite video and mono VGA weighed it), so black
// is black, white is the phosphor at full and never paler, and every color
// keeps its place between them. It's worked out
// per unit of what covers the pixel, so edges blend as they did; light
// added with no cover of its own (a glow) is shaded as it comes. Shades of
// one color blend into shades of it, so all Flow draws stays monochrome.
fn on_phosphor(c: vec4<f32>) -> vec4<f32> {
    if (G.phosphor.a < 0.5) {
        return c;
    }
    let k = select(1.0, c.a, c.a > 0.0001);
    let u = c.rgb / k;
    let l = max(lum(u), PHOSPHOR_FLOOR * max(u.r, max(u.g, u.b)));
    let shown = G.phosphor.rgb * min(l, 1.0);
    return vec4<f32>(shown * k, c.a);
}

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

fn dye_at(uv: vec2<f32>) -> vec3<f32> {
    return textureSampleLevel(tex_main, samp, uv, 0.0).rgb;
}

fn hash2(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn hash22(p: vec2<f32>) -> vec2<f32> {
    let q = vec2<f32>(dot(p, vec2<f32>(127.1, 311.7)), dot(p, vec2<f32>(269.5, 183.3)));
    return fract(sin(q) * 43758.5453);
}

fn vnoise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash2(i);
    let b = hash2(i + vec2<f32>(1.0, 0.0));
    let c = hash2(i + vec2<f32>(0.0, 1.0));
    let d = hash2(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn fbm(p: vec2<f32>) -> f32 {
    var v = 0.0;
    var amp = 0.5;
    var q = p;
    for (var i = 0; i < 4; i = i + 1) {
        v += amp * vnoise(q);
        q = q * 2.03 + vec2<f32>(1.7, 9.2);
        amp *= 0.5;
    }
    return v;
}

// Bright lines where neighbouring animated cells meet, like light focused by waves.
fn cell_edges(p: vec2<f32>, t: f32) -> f32 {
    let i = floor(p);
    let f = fract(p);
    var d1 = 8.0;
    var d2 = 8.0;
    for (var y = -1; y <= 1; y = y + 1) {
        for (var x = -1; x <= 1; x = x + 1) {
            let g = vec2<f32>(f32(x), f32(y));
            let o = hash22(i + g);
            let pt = g + vec2<f32>(0.5) + 0.42 * sin(vec2<f32>(t * 0.9, t * 0.7) + TAU * o);
            let d = length(pt - f);
            if (d < d1) {
                d2 = d1;
                d1 = d;
            } else if (d < d2) {
                d2 = d;
            }
        }
    }
    return 1.0 - smoothstep(0.0, 0.22, d2 - d1);
}

fn caustic(p: vec2<f32>, t: f32) -> f32 {
    let warp = vec2<f32>(fbm(p * 0.35 + vec2<f32>(t * 0.05, 0.0)), fbm(p * 0.35 + vec2<f32>(3.3, t * 0.04)));
    let q = p + (warp - vec2<f32>(0.5)) * 1.4;
    let a = cell_edges(q, t);
    let b = cell_edges(q * 1.7 + vec2<f32>(4.1, 1.7), t * 1.25);
    // Thin lines that brighten where two layers cross, in patches.
    let lines = pow(a, 4.0) * 0.6 + pow(b, 4.0) * 0.35 + pow(a * b, 2.0) * 0.9;
    let patches = smoothstep(0.25, 0.75, fbm(p * 0.22 + vec2<f32>(t * 0.03, 1.7)));
    return lines * (0.35 + 0.65 * patches);
}

struct Shade {
    col: vec3<f32>,
    a: f32,
};

// Ink clouds swirling in the glass, with light ribbons and lightning flashes.
// No liquid: the tinted glass alone, a touch lighter at the top like the rest.
fn shade_plain(uv: vec2<f32>) -> Shade {
    let col = G.tint.rgb + vec3<f32>(smoothstep(0.6, 0.0, uv.y) * 0.03);
    return Shade(col, G.opacity);
}

fn shade_clouds(uv: vec2<f32>, vel: vec2<f32>, res: vec2<f32>) -> Shade {
    let duv = uv + vel * G.dye_texel * 0.05;
    let dye = dye_at(duv);
    let e = G.dye_texel * 2.0;
    let hL = lum(dye_at(duv - vec2<f32>(e.x, 0.0)));
    let hR = lum(dye_at(duv + vec2<f32>(e.x, 0.0)));
    let hT = lum(dye_at(duv - vec2<f32>(0.0, e.y)));
    let hB = lum(dye_at(duv + vec2<f32>(0.0, e.y)));
    let n = normalize(vec3<f32>((hL - hR) * 5.0, (hT - hB) * 5.0, 1.0));
    let light = normalize(vec3<f32>(-0.45, -0.75, 0.55));
    let spec = pow(max(dot(reflect(-light, n), vec3<f32>(0.0, 0.0, 1.0)), 0.0), 40.0);
    let fres = 1.0 - n.z;

    var col = G.tint.rgb;
    col += vec3<f32>(smoothstep(0.6, 0.0, uv.y) * 0.03);
    // Keep the dye from washing out text: soft-clip its brightness.
    let dye_c = dye * G.dye_gain;
    col += dye_c / (1.0 + lum(dye_c) * 1.6);
    col += vec3<f32>(spec * 0.2) + G.accent.rgb * fres * 0.25;
    var a = G.opacity + clamp(lum(dye) * 0.5 + spec * 0.25, 0.0, 0.35);

    // Ribbons: slow domain-warped noise bent by the flow.
    let aspect = res.x / max(res.y, 1.0);
    let q = vec2<f32>(uv.x * aspect, uv.y) * 1.9;
    let t = G.time;
    let warp = vec2<f32>(fbm(q + vec2<f32>(0.0, t * 0.035)), fbm(q + vec2<f32>(5.2, 1.3) - vec2<f32>(t * 0.03, 0.0)));
    let w = fbm(q + warp * 2.2 - vel * G.dye_texel * 1.5);
    let ribbon = smoothstep(0.50, 0.62, w) * (1.0 - smoothstep(0.62, 0.80, w));
    let glint = pow(ribbon, 2.0);
    let ribbon_col = mix(G.accent.rgb, G.accent2.rgb, smoothstep(0.3, 0.7, warp.x));
    let speed = clamp(length(vel) * 0.003, 0.0, 1.0);
    col += ribbon_col * (ribbon * 0.07 + glint * 0.08 + speed * 0.05) * G.caustics;
    a += (ribbon * 0.05 + speed * 0.04) * G.caustics;

    // Lightning lights the clouds up from inside.
    if (G.flash.z > 0.0) {
        let dv = (uv - G.flash.xy) * vec2<f32>(aspect, 1.0);
        let glow = G.flash.z * exp(-dot(dv, dv) / max(G.flash.w * G.flash.w, 1e-4));
        col += (dye_c * 3.5 + vec3<f32>(0.09, 0.11, 0.18)) * glow;
        a += glow * 0.2;
    }
    return Shade(col, a);
}

fn swell(p: vec2<f32>, t: f32) -> f32 {
    var s = sin(dot(p, vec2<f32>(5.2, 1.7)) - t * 1.1) * 0.5;
    s += sin(dot(p, vec2<f32>(-2.6, 6.3)) - t * 1.35) * 0.35;
    s += sin(dot(p, vec2<f32>(9.1, -3.7)) - t * 1.9) * 0.18;
    s += (fbm(p * 2.4 + vec2<f32>(t * 0.12, -t * 0.08)) - 0.5) * 0.9;
    return s;
}

fn water_height(uv: vec2<f32>, aspect: f32, t: f32) -> f32 {
    let ripple = textureSampleLevel(wave_tex, samp, uv, 0.0).x;
    return ripple * 0.035 + swell(vec2<f32>(uv.x * aspect, uv.y), t) * 0.012;
}

fn water_normal(uv: vec2<f32>, aspect: f32, t: f32) -> vec3<f32> {
    let e = G.wave_texel * 1.5;
    let hl = water_height(uv - vec2<f32>(e.x, 0.0), aspect, t);
    let hr = water_height(uv + vec2<f32>(e.x, 0.0), aspect, t);
    let ht = water_height(uv - vec2<f32>(0.0, e.y), aspect, t);
    let hb = water_height(uv + vec2<f32>(0.0, e.y), aspect, t);
    let dx = (hr - hl) / (2.0 * e.x * aspect);
    let dy = (hb - ht) / (2.0 * e.y);
    return normalize(vec3<f32>(-dx, -dy, 1.0));
}

// Looking down into a pool: caustics on the floor, sky and sun on the waves.
fn shade_water_above(uv: vec2<f32>, vel: vec2<f32>, res: vec2<f32>) -> Shade {
    let aspect = res.x / max(res.y, 1.0);
    let t = G.time;
    let sun = G.sun;
    let n = water_normal(uv, aspect, t);
    // Light through the surface onto the floor, bent by the waves and currents.
    let fp = vec2<f32>(uv.x * aspect, uv.y) * 3.2 + n.xy * 0.35 + vel * G.dye_texel * 0.4;
    let caus = caustic(fp, t * 0.8);
    let deep = mix(vec3<f32>(0.03, 0.17, 0.22), vec3<f32>(0.01, 0.06, 0.10), smoothstep(0.0, 1.2, uv.y + uv.x * 0.3));
    var col = deep * (0.55 + 0.45 * sun);
    col += vec3<f32>(0.30, 0.65, 0.70) * caus * (0.10 + 0.32 * sun);
    // The sky shows on the steeper slopes.
    let fres = clamp(pow(1.0 - n.z, 2.0) * 3.0, 0.0, 1.0);
    col = mix(col, vec3<f32>(0.45, 0.62, 0.78) * (0.4 + 0.5 * sun), fres * 0.35);
    // The sun's reflection sits top right, away from where text starts; tiny
    // ripples on the surface break it into sparkles.
    let view = normalize(vec3<f32>((0.5 - uv.x) * aspect, 0.5 - uv.y, 1.1));
    let sun_dir = normalize(vec3<f32>(0.45, -0.27, 1.0));
    // The sparkle noise drifts fast, so its clock wraps: the sin hash breaks
    // down at large inputs, and a jump in sparkles once in a while is invisible.
    let ts = t % 512.0;
    let fine = vec2<f32>(uv.x * aspect, uv.y) * 160.0 + vec2<f32>(ts * 0.9, -ts * 0.7);
    let jitter = vec3<f32>(vnoise(fine) - 0.5, vnoise(fine + vec2<f32>(19.1, 7.3)) - 0.5, 0.0) * 0.07;
    let gn = normalize(n + jitter);
    let mirror = max(dot(reflect(-sun_dir, gn), view), 0.0);
    let glint = pow(mirror, 1400.0) * 1.1 + pow(max(dot(reflect(-sun_dir, n), view), 0.0), 24.0) * 0.06;
    col += vec3<f32>(1.0, 0.96, 0.86) * glint * sun;
    let a = G.opacity + 0.08 + caus * 0.08 * sun + clamp(glint, 0.0, 1.0) * 0.4 * sun;
    return Shade(col, a);
}

fn marine_snow(p: vec2<f32>, t: f32) -> f32 {
    var s = 0.0;
    for (var layer = 0; layer < 2; layer = layer + 1) {
        let fl = f32(layer);
        let q = (p + vec2<f32>(t * 0.004, t * 0.009) * (1.0 + fl)) * (22.0 + fl * 16.0);
        let i = floor(q);
        let f = fract(q);
        let h = hash22(i + vec2<f32>(fl * 17.0));
        if (h.x > 0.9) {
            let c = vec2<f32>(0.5) + 0.3 * sin(vec2<f32>(t * 0.5, t * 0.37) + h * TAU);
            let d = length(f - c);
            s += (1.0 - smoothstep(0.02, 0.09, d)) * (0.35 + 0.65 * h.y);
        }
    }
    return s;
}

// Underwater: the surface shimmering overhead, sunlight shafts, sand below.
fn shade_water_below(uv: vec2<f32>, vel: vec2<f32>, res: vec2<f32>) -> Shade {
    let aspect = res.x / max(res.y, 1.0);
    let t = G.time;
    let sun = G.sun;
    let x = uv.x * aspect;
    // The surface runs along the title bar so it never crosses the text.
    let title = 34.0 * G.ui_scale / max(res.y, 1.0);
    let sy = title + 0.006 * sin(x * 6.0 + t * 1.2) + 0.003 * sin(x * 15.0 - t * 2.0) + (fbm(vec2<f32>(x * 3.0, t * 0.3)) - 0.5) * 0.01;
    let depth = clamp((uv.y - sy) / (1.0 - sy), 0.0, 1.0);
    var col = mix(vec3<f32>(0.06, 0.30, 0.40), vec3<f32>(0.01, 0.04, 0.08), pow(depth, 0.8)) * (0.45 + 0.55 * sun);

    // Shafts of light fanning down from the sun above the surface.
    let d = vec2<f32>(x, uv.y) - vec2<f32>(aspect * 0.28, -0.55);
    let ang = atan2(d.x, d.y);
    let wob = 0.04 * sin(t * 0.7 + ang * 3.0);
    let r1 = fbm(vec2<f32>((ang + wob) * 14.0, t * 0.10));
    let r2 = fbm(vec2<f32>((ang - wob) * 31.0 + 7.0, t * 0.13 + 3.0));
    let shafts = smoothstep(0.42, 0.78, r1 * 0.65 + r2 * 0.45) * exp(-depth * 2.4);
    col += vec3<f32>(0.50, 0.82, 0.92) * shafts * 0.42 * sun;

    // Light dancing in the water, and a sandy floor catching it.
    let caus = caustic(vec2<f32>(x, uv.y) * 3.6 + vel * G.dye_texel * 0.4, t * 0.9);
    col += vec3<f32>(0.25, 0.55, 0.60) * caus * (0.06 + 0.10 * (1.0 - depth)) * sun;
    let floor_y = 0.88 + 0.025 * sin(x * 2.3 + 0.7) + 0.01 * sin(x * 7.0);
    let on_floor = smoothstep(floor_y - 0.01, floor_y + 0.03, uv.y);
    let sand = mix(vec3<f32>(0.10, 0.12, 0.11), vec3<f32>(0.20, 0.22, 0.18), fbm(vec2<f32>(x * 12.0, uv.y * 20.0)));
    col = mix(col, sand * (0.35 + 0.4 * sun) + vec3<f32>(0.4, 0.6, 0.55) * caus * 0.35 * sun, on_floor * 0.85);
    col += vec3<f32>(0.7, 0.85, 0.9) * marine_snow(vec2<f32>(x, uv.y), t) * (0.25 + 0.25 * sun);

    // The surface from below: a bright, shimmering ceiling with a lit edge.
    let above = 1.0 - smoothstep(sy - 0.004, sy + 0.004, uv.y);
    let shimmer = fbm(vec2<f32>(x * 9.0 + t * 0.6, uv.y * 30.0 - t * 0.4));
    let ceiling = vec3<f32>(0.45, 0.75, 0.85) * (0.35 + 0.65 * shimmer) * (0.35 + 0.65 * sun);
    col = mix(col, ceiling, above * 0.9);
    col += vec3<f32>(0.75, 0.95, 1.0) * exp(-pow((uv.y - sy) / 0.005, 2.0)) * (0.25 + 0.5 * sun);
    let a = G.opacity + 0.1 + above * 0.25 + shafts * 0.12 * sun + on_floor * 0.15;
    return Shade(col, a);
}

// Oil on dark water: thick slicks with a thin-film rainbow and a glossy sheen.
fn shade_oil(uv: vec2<f32>, vel: vec2<f32>, res: vec2<f32>) -> Shade {
    let duv = uv + vel * G.dye_texel * 0.03;
    let thick = lum(dye_at(duv)) * 2.2;
    let e = G.dye_texel * 2.0;
    let hL = lum(dye_at(duv - vec2<f32>(e.x, 0.0)));
    let hR = lum(dye_at(duv + vec2<f32>(e.x, 0.0)));
    let hT = lum(dye_at(duv - vec2<f32>(0.0, e.y)));
    let hB = lum(dye_at(duv + vec2<f32>(0.0, e.y)));
    let n = normalize(vec3<f32>((hL - hR) * 9.0, (hT - hB) * 9.0, 1.0));
    let slick = smoothstep(0.02, 0.10, thick);
    // Interference colors show where the film is thin; thick oil is dark and glossy.
    let thin = slick * (1.0 - smoothstep(0.22, 0.55, thick));
    let film = vec3<f32>(0.5) + 0.5 * cos(TAU * (vec3<f32>(thick * 0.9 + n.x * 0.25 - n.y * 0.2) + vec3<f32>(0.0, 0.33, 0.67)));
    let oil_body = vec3<f32>(0.035, 0.028, 0.02);
    var col = vec3<f32>(0.012, 0.014, 0.022) + vec3<f32>(0.02, 0.025, 0.04) * (1.0 - uv.y);
    col = mix(col, oil_body, slick * 0.85);
    col += film * thin * 0.32;
    let light = normalize(vec3<f32>(-0.45, -0.75, 0.55));
    let spec = pow(max(dot(reflect(-light, n), vec3<f32>(0.0, 0.0, 1.0)), 0.0), 60.0);
    col += vec3<f32>(0.9, 0.85, 0.75) * spec * 0.5 * slick;
    let a = G.opacity + slick * 0.25 + spec * 0.2 * slick;
    return Shade(col, a);
}

// The rain map (render/rain.rs): x the water's height in pixels / 16, y how
// clear the glass is where drops ran or the cursor wiped, and a little round
// each drop, z and w where in its drop a pixel sits (see `lens_in`).
fn rain_at(uv: vec2<f32>) -> vec4<f32> {
    return textureSampleLevel(wave_tex, samp, uv, 0.0);
}

// How much of the night a drop shows: this much of the window's height to
// each side of the point straight behind it.
const RAIN_FIELD: f32 = 0.35;

// The lights outside, far off: on a slowly drifting grid, partly in the
// liquid's colors (`tint`). Through clear glass (`blur` 0) each is a small,
// sharp point; fogged glass (1) spreads its light into a wide, soft disc,
// and in between it is some of each. `fine` (window heights) is the least
// size: points so small that they would flicker as specks fill that much
// instead, as dim as their light spread over it. `gain` makes the points
// brighter, up to a limit.
fn bokeh(p: vec2<f32>, t: f32, blur: f32, tint: vec3<f32>, fine: f32, gain: f32) -> vec3<f32> {
    var acc = vec3<f32>(0.0);
    for (var layer = 0; layer < 2; layer = layer + 1) {
        // The farther layer has more, smaller and dimmer lights.
        let fl = f32(layer);
        let cells = 3.5 + fl * 3.5;
        let q = p * cells + vec2<f32>(t * (0.010 + fl * 0.006), fl * 3.7);
        let i = floor(q);
        let f = fract(q);
        let least = fine * cells;
        for (var y = -1; y <= 1; y = y + 1) {
            for (var x = -1; x <= 1; x = x + 1) {
                let g = vec2<f32>(f32(x), f32(y));
                let h = hash22(i + g + vec2<f32>(fl * 31.0, 0.0));
                // Most of the night is dark.
                if (h.x > 0.35) {
                    continue;
                }
                let c = g + vec2<f32>(0.5) + (h - vec2<f32>(0.5)) * 0.7;
                let d = length(f - c);
                // Street lamps and windows, glimmering a little.
                let lamp = mix(vec3<f32>(1.0, 0.62, 0.30), vec3<f32>(0.60, 0.78, 1.0), step(0.6, fract(h.y * 7.3)));
                let glimmer = 0.85 + 0.15 * sin(t * (0.5 + h.y) + h.x * TAU);
                // As bright as `power` over the wide disc, and as much light
                // in all gathered into the point.
                let power = (0.5 + 1.4 * h.x) * (1.0 - fl * 0.35) * 0.32;
                let wide = 0.25 + 0.07 * h.y;
                let disc = (1.0 - smoothstep(wide - 0.135, wide + 0.135, d)) * power;
                let rad = max(0.004 + 0.008 * h.y, least);
                let soft = max(0.003, least * 0.5);
                let point = (1.0 - smoothstep(rad - soft, rad + soft, d)) * min(power * gain * (wide * wide) / (rad * rad), 3.0);
                acc += mix(lamp, tint, 0.5) * glimmer * mix(point, disc, blur);
            }
        }
    }
    return acc;
}

// The liquid's color, softened by `blur` (0..1) as fogged glass does.
fn dye_soft(uv: vec2<f32>, blur: f32) -> vec3<f32> {
    let c = dye_at(uv);
    if (blur < 0.01) {
        return c;
    }
    let o = G.dye_texel * 8.0 * blur;
    let around = dye_at(uv + o) + dye_at(uv - o) + dye_at(uv + vec2<f32>(o.x, -o.y)) + dye_at(uv + vec2<f32>(-o.x, o.y));
    return (c * 2.0 + around) / 6.0;
}

// The night outside the window: a dark sky, the liquid's colors as far-off
// glows, and lights (`bokeh`).
fn rain_scene(uv: vec2<f32>, aspect: f32, t: f32, blur: f32, fine: f32, gain: f32) -> vec3<f32> {
    var col = mix(vec3<f32>(0.015, 0.020, 0.040), vec3<f32>(0.040, 0.045, 0.070), smoothstep(0.0, 1.0, uv.y));
    let dye = dye_soft(uv, blur) * G.dye_gain;
    col += dye / (1.0 + lum(dye) * 1.6) * 0.5;
    let m = max(dye.r, max(dye.g, dye.b));
    let tint = mix(vec3<f32>(1.0, 0.8, 0.6), dye / max(m, 1e-3), smoothstep(0.0, 0.06, m));
    // Lightning far off (`G.flash`, the rain's storm): the whole night lit
    // for a moment, the sky most, and most toward where it struck.
    if (G.flash.z > 0.0) {
        let across = (uv.x - G.flash.x) * aspect;
        let toward = 0.45 + 0.55 * exp(-across * across * 1.5);
        let sky = 0.3 + 0.7 * (1.0 - smoothstep(0.0, 1.0, uv.y));
        col += vec3<f32>(0.55, 0.6, 0.78) * G.flash.z * toward * sky * 0.6;
    }
    // The brightest keep their color rather than washing out to white.
    let lights = bokeh(vec2<f32>(uv.x * aspect, uv.y), t, blur, tint, fine, gain);
    return col + lights / max(max(lights.r, max(lights.g, lights.b)), 1.0);
}

// The fog's grain: tiny beads of condensation on two grids (3 and 5
// points), placed anywhere in their cells. Each catches a little light on
// top and is darker along its lower edge. x: light, y: shade.
fn mist(px: vec2<f32>) -> vec2<f32> {
    var m = vec2<f32>(0.0);
    for (var layer = 0; layer < 2; layer = layer + 1) {
        let fl = f32(layer);
        let cell = (3.0 + fl * 2.0) * G.ui_scale;
        let p = px / cell + vec2<f32>(fl * 7.31, fl * 3.17);
        let i = floor(p);
        let f = fract(p);
        let h = hash22(i + vec2<f32>(fl * 41.0, 0.0));
        if (h.x > 0.6 - fl * 0.25) {
            continue;
        }
        let c = vec2<f32>(0.25) + vec2<f32>(fract(h.y * 13.7), fract(h.x * 7.9)) * 0.5;
        // Never under 0.6 of a pixel in radius, with an edge a pixel wide,
        // so that on a 1x display they aren't harsh specks.
        let r = max(0.09 + 0.12 * h.y + fl * 0.05, 0.6 / cell);
        let e = max(0.045, 0.5 / cell);
        let d = f - c;
        let bead = 1.0 - smoothstep(r - e, r + e, length(d));
        let glint = 1.0 - smoothstep(0.0, r * 0.6, length(d + vec2<f32>(r * 0.3, r * 0.4)));
        let lower = smoothstep(0.0, r, d.y);
        m += vec2<f32>(bead * (0.15 + 0.85 * glint), bead * lower);
    }
    return m;
}

// Where in its drop a texel `m` of the rain map sits: 0 in the middle of
// its head, length 1 at its rim (the map keeps it plus 2, times the
// height).
fn lens_in(m: vec4<f32>) -> vec2<f32> {
    return m.zw / max(m.x, 1e-6) - vec2<f32>(2.0);
}

// How much that changes over a pixel, from the texels before (`a`) and
// after (`b`) it: both, or at the rim whichever is still in the drop.
fn lens_step(u: vec2<f32>, a: vec4<f32>, b: vec4<f32>, alone: vec2<f32>) -> vec2<f32> {
    if (a.x > 0.0 && b.x > 0.0) {
        return (lens_in(b) - lens_in(a)) * 0.5;
    }
    if (b.x > 0.0) {
        return lens_in(b) - u;
    }
    if (a.x > 0.0) {
        return u - lens_in(a);
    }
    return alone;
}

// Where the night seen at `uv` through a drop, `u` in it, comes from, in
// window heights across and down (as `bokeh` takes it): RAIN_FIELD of it to
// each side, upside down and squeezed together toward the rim.
fn lens_view(uv: vec2<f32>, u: vec2<f32>, aspect: f32) -> vec2<f32> {
    let lu = min(length(u), 1.0);
    return vec2<f32>(uv.x * aspect, uv.y) - u * (1.0 + 0.4 * lu * lu) * RAIN_FIELD;
}

// Water running down the glass in sheets, as in a downpour (`G.wet`): a
// film flowing down in wavy streams, ripples running down across it, and
// thicker under the top edge, where it comes over in a curtain. Its
// thickness at `uv`, in pixels.
fn sheet(uv: vec2<f32>, aspect: f32, t: f32) -> f32 {
    let x = uv.x * aspect;
    let streams = 0.5 + 0.5 * sin(x * 23.0 + 2.2 * sin(x * 6.3 + uv.y * 2.1 - t * 0.5));
    let ripples = sin(uv.y * 140.0 - t * 12.0 + 1.3 * sin(x * 3.1 + t * 0.3) + 0.9 * sin(x * 7.7 - uv.y * 9.0));
    let curtain = 1.0 - smoothstep(0.0, 0.2, uv.y);
    return G.wet * (1.0 + 1.6 * streams + 2.2 * curtain) * (1.0 + 0.35 * ripples) * G.ui_scale;
}

// Rain on the window at night. The glass is fogged: a pale veil with fine
// condensation, the lights outside spread into soft discs. A drop is a small
// lens: clear, with a sharp, small, upside-down view of the night behind
// it, a glint on top, a thin dark edge and light gathered inside its lower
// rim. Drops take up the mist right round them, and where drops ran or the
// cursor wiped, the glass is clear for a while. As it pours, water runs down
// the glass in sheets (`sheet`), washing the fog off and bending the night
// behind as it flows.
fn shade_rain(uv: vec2<f32>, vel: vec2<f32>, res: vec2<f32>) -> Shade {
    let aspect = res.x / max(res.y, 1.0);
    let px = 1.0 / res;
    let here = rain_at(uv);
    // The rain map a pixel either side, and from it the water's slope
    // (height over distance, both in pixels).
    let l = rain_at(uv - vec2<f32>(px.x, 0.0));
    let r = rain_at(uv + vec2<f32>(px.x, 0.0));
    let t = rain_at(uv - vec2<f32>(0.0, px.y));
    let b = rain_at(uv + vec2<f32>(0.0, px.y));
    let slope = vec2<f32>(r.x - l.x, b.x - t.x) * 8.0;
    let water = smoothstep(0.0, 0.02, here.x);
    var fog = 1.0 - clamp(max(here.y * 0.85, water), 0.0, 1.0);
    var bend = vec2<f32>(0.0);
    if (G.wet > 0.0) {
        // Its slope (pixels over pixels) bends the night seen through it;
        // where it runs thick, the fog is washed off. It catches no light
        // of its own: the ripples' crests, lit, lined up across the window
        // as white streaks.
        let film = sheet(uv, aspect, G.time);
        let sx = sheet(uv + vec2<f32>(px.x, 0.0), aspect, G.time) - sheet(uv - vec2<f32>(px.x, 0.0), aspect, G.time);
        let sy = sheet(uv + vec2<f32>(0.0, px.y), aspect, G.time) - sheet(uv - vec2<f32>(0.0, px.y), aspect, G.time);
        bend = vec2<f32>(sx, sy) * 0.5 * 0.06;
        fog *= 1.0 - 0.4 * G.wet - 0.45 * clamp(film / (5.0 * G.ui_scale), 0.0, 1.0);
    }

    // The fogged glass: the night behind it, soft unless the glass is quite
    // clear (a little condensation already blurs it, so neither the halos
    // round drops nor a path fogging over again show it sharp), a pale veil
    // lit up a little by the lights, and the condensation's grain, which is
    // gone next to the drops. No light is under a pixel across.
    let scene = rain_scene(uv + bend, aspect, G.time, (1.0 - smoothstep(0.8, 1.0, here.y)) * (1.0 - 0.5 * G.wet), px.y, 1.0);
    var col = G.tint.rgb * 0.5 + scene;
    col += (vec3<f32>(0.050, 0.055, 0.065) + scene * 0.25) * fog;
    let grain = mist(uv * res) * smoothstep(0.5, 0.95, fog) * 0.5;
    col += (vec3<f32>(0.07, 0.075, 0.085) + scene * 0.25) * grain.x - col * grain.y * 0.12;
    var a = G.opacity + fog * 0.06 + lum(scene) * 0.3;
    if (here.x <= 0.0) {
        return Shade(col, a);
    }

    // Where in the drop this pixel sits, how much that changes over a pixel
    // across and down, and from those the drop's size and how far its rim
    // is (pixels). A lone pixel of water counts as a drop a pixel round.
    let u = lens_in(here);
    let lu = min(length(u), 1.0);
    let ux = lens_step(u, l, r, vec2<f32>(1.0, 0.0));
    let uy = lens_step(u, t, b, vec2<f32>(0.0, 1.0));
    let det = max(abs(ux.x * uy.y - uy.x * ux.y), 1e-6);
    let radius = inverseSqrt(det);
    let outward = u / max(lu, 1e-3);
    let from_rim = (1.0 - lu) / max(length(vec2<f32>(dot(outward, ux), dot(outward, uy))), 1e-3);
    // Its thin dark edge, where the light bends away, a pixel or so wide
    // and wider on big drops.
    let rim = max(1.0 - smoothstep(0.0, 0.6 * G.ui_scale + radius * 0.1, from_rim), smoothstep(1.0, 2.5, length(slope)));

    // Through it the night shows sharp, small and upside down. The lights
    // it focuses come out as bright points, at least a pixel of it across
    // (`fine`, from how far the view moves over a pixel either way). Where
    // it is drawn out more one way than the other, as up a runner's tail,
    // they are spread thinner rather than drawn into streaks, and none show
    // right at the rim.
    let v = lens_view(uv, u, aspect);
    let vx = length(lens_view(uv + vec2<f32>(px.x, 0.0), u + ux, aspect) - v);
    let vy = length(lens_view(uv + vec2<f32>(0.0, px.y), u + uy, aspect) - v);
    let fine = 0.7 * max(vx, vy);
    let focus = min(vx, vy) / max(max(vx, vy), 1e-6);
    let gain = 1.5 * focus * (1.0 - rim) * (1.0 - smoothstep(0.85, 1.0, lu));
    let seen = rain_scene(vec2<f32>(v.x / aspect, v.y), aspect, G.time, 0.0, fine, gain);
    var dc = (G.tint.rgb * 0.5 + seen) * (0.85 - 0.3 * lu * lu);
    dc *= 1.0 - rim * 0.7;
    // Light gathered on the side away from the light: a soft crescent
    // inside its lower rim, in the colors of the glow around it.
    let low = smoothstep(0.1, 0.9, u.y) * smoothstep(0.35, 0.8, lu) * (1.0 - rim);
    dc += (vec3<f32>(0.16, 0.17, 0.2) + scene * 2.0) * low;
    // And a small, sharp glint on the side toward it, up and to the left:
    // round on the screen however the drop is drawn out (`gd` is pixels
    // from it), and never under a pixel and a half across, so the smallest
    // drops are just a glint.
    let du = vec2<f32>(-0.38, -0.22) - u;
    let gd = length(vec2<f32>(uy.y * du.x - uy.x * du.y, ux.x * du.y - ux.y * du.x)) / det;
    let gs = max(max(0.75, 0.5 * G.ui_scale), radius * 0.1);
    let spec = exp(-gd * gd / (gs * gs)) * (0.5 + 0.5 * smoothstep(1.5, 4.0, radius / G.ui_scale));
    dc += vec3<f32>(0.92, 0.95, 1.0) * spec;

    let da = G.opacity + lum(seen) * 0.3 + rim * 0.25 + low * 0.06 + spec * 0.5;
    return Shade(mix(col, dc, water), mix(a, da, water));
}

// What the terminal is up to, over any style: a ring spreading from where
// a command finished (or the assistant got something done), and a tank
// that fills from the bottom while a program reports its progress.
fn signals(uv: vec2<f32>, res: vec2<f32>) -> Shade {
    var col = vec3<f32>(0.0);
    var a = 0.0;
    let aspect = res.x / max(res.y, 1.0);
    if (G.pulse.w > 0.0) {
        let age = G.pulse.z;
        let dist = length((uv - G.pulse.xy) * vec2<f32>(aspect, 1.0));
        let width = max(G.pulse_color.w, 0.002);
        // The ring runs out at half the window's height a second, fading.
        let ring = exp(-pow((dist - age * 0.5) / width, 2.0)) * exp(-age * 1.5);
        // And a brief glow where it began.
        let core = exp(-dist * dist / 0.003) * exp(-age * 5.0);
        let glow = (ring + core * 0.6) * G.pulse.w;
        col += G.pulse_color.rgb * glow * 0.55;
        a += glow * 0.2;
    }
    if (G.progress.y > 0.0) {
        let t = G.progress.w;
        // Without a figure it sloshes around the middle.
        let level = mix(0.42 + 0.22 * sin(t * 1.3), G.progress.x, G.progress.z);
        let x = uv.x * aspect;
        let surface = 1.0 - level + 0.006 * sin(x * 9.0 + t * 2.2) + 0.004 * sin(x * 23.0 - t * 3.1);
        let px = 1.0 / max(res.y, 1.0);
        let below = smoothstep(surface - px, surface + px, uv.y);
        let line = exp(-pow((uv.y - surface) / (1.4 * px * G.ui_scale), 2.0));
        let shown = G.progress.y;
        col += G.progress_color.rgb * (below * 0.07 + line * 0.6) * shown;
        a += (below * 0.05 + line * 0.3) * shown;
    }
    return Shade(col, a);
}

@fragment
fn fs_glass(in: FullOut) -> @location(0) vec4<f32> {
    let res = G.resolution;
    let px = in.uv * res;
    let d = sd_round_box(px - res * 0.5, res * 0.5, G.corner_radius);
    let mask = clamp(0.5 - d, 0.0, 1.0);
    if (mask <= 0.0) {
        return vec4<f32>(0.0);
    }
    let vel = textureSampleLevel(vel_tex, samp, in.uv, 0.0).xy;
    var s: Shade;
    if (G.mode < 0.5) {
        s = shade_clouds(in.uv, vel, res);
    } else if (G.mode < 1.5) {
        s = shade_water_above(in.uv, vel, res);
    } else if (G.mode < 2.5) {
        s = shade_water_below(in.uv, vel, res);
    } else if (G.mode < 3.5) {
        s = shade_oil(in.uv, vel, res);
    } else if (G.mode < 4.5) {
        s = shade_rain(in.uv, vel, res);
    } else {
        s = shade_plain(in.uv);
    }
    var col = s.col;
    var a = s.a;
    let o = signals(in.uv, res);
    col += o.col;
    a += o.a;

    // AI activity: a soft inner glow that hugs the frame.
    let inner = exp(d / 38.0) * G.activity;
    col += G.accent2.rgb * inner * 0.45;
    a += inner * 0.25;

    // Thin luminous edge with a slowly drifting hue.
    let rim = clamp(1.0 - abs(d + 0.75) / 0.9, 0.0, 1.0);
    let ang = atan2(px.y - res.y * 0.5, px.x - res.x * 0.5);
    let drift = 0.5 + 0.5 * sin(ang * 2.0 + G.time * 0.5);
    let rim_col = mix(G.accent.rgb, G.accent2.rgb, drift) * (0.6 + 0.4 * G.focus) + vec3<f32>(0.2);
    col = mix(col, rim_col, rim * 0.85);
    a = max(a, rim * (0.5 + 0.35 * G.focus));

    if (G.opaque > 0.5) {
        a = 1.0;
    }
    a = clamp(a, 0.0, 1.0) * mask;
    return on_phosphor(vec4<f32>(col * a, a));
}

struct QuadIn {
    @location(0) rect: vec4<f32>,
    @location(1) uv: vec4<f32>,
    @location(2) color: vec4<f32>,
    // kind, corner radius / line width / tilt, border width / blur / glow /
    // padding, ripple amount.
    @location(3) params: vec4<f32>,
};

struct QuadOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) local: vec2<f32>,
    @location(2) size: vec2<f32>,
    @location(3) color: vec4<f32>,
    @location(4) @interpolate(flat) params: vec4<f32>,
    // Segment endpoints (pixels) for kind 5; for a bubble (kind 6), its uv:
    // how far it has popped and the way to the light.
    @location(5) @interpolate(flat) seg: vec4<f32>,
};

fn to_clip(p: vec2<f32>) -> vec4<f32> {
    return vec4<f32>(p.x / G.resolution.x * 2.0 - 1.0, 1.0 - p.y / G.resolution.y * 2.0, 0.0, 1.0);
}

@vertex
fn vs_quad(@builtin(vertex_index) vi: u32, q: QuadIn) -> QuadOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
    );
    let c = corners[vi % 6u];
    var out: QuadOut;
    out.color = q.color;
    out.params = q.params;
    out.seg = q.rect;
    out.uv = vec2<f32>(0.0);
    out.size = vec2<f32>(0.0);
    if (q.params.x > 5.5) {
        out.seg = q.uv;
    } else if (q.params.x > 4.5) {
        // A line segment: a box around a..b, padded by width and glow.
        let a = q.rect.xy;
        let b = q.rect.zw;
        let dir = normalize(b - a + vec2<f32>(1e-4, 0.0));
        let nrm = vec2<f32>(-dir.y, dir.x);
        let r = q.params.y + q.params.z;
        let end = mix(a - dir * r, b + dir * r, c.x);
        let p = end + nrm * r * (c.y * 2.0 - 1.0);
        out.pos = to_clip(p);
        out.local = p;
        return out;
    }
    var shift = vec2<f32>(0.0);
    if (q.params.w > 0.0) {
        // Glyphs ride the liquid: shifted by the flow at their middle. An
        // image's pieces bend with it instead, each corner by the flow where
        // it is, so corners that meet move alike: the pieces of an image, and
        // images laid side by side like a game's tiles, stay joined. So do
        // the pieces of box drawing's lines (kind 9), cell to cell.
        var at = q.rect.xy + q.rect.zw * 0.5;
        if (q.params.x > 7.5) {
            at = q.rect.xy + c * q.rect.zw;
        }
        let v = textureSampleLevel(vel_tex, samp, at / G.resolution, 0.0).xy;
        shift = clamp(v * 0.01, vec2<f32>(-1.0), vec2<f32>(1.0)) * q.params.w * G.ripple;
    }
    var grow = 0.0;
    if (q.params.x > 3.5 && q.params.x < 7.5) {
        grow = q.params.z;
    }
    let size = q.rect.zw + vec2<f32>(grow * 2.0);
    let p = q.rect.xy - vec2<f32>(grow) + c * size + shift;
    out.pos = to_clip(p);
    out.uv = mix(q.uv.xy, q.uv.zw, c);
    out.local = c * size - vec2<f32>(grow);
    out.size = q.rect.zw;
    return out;
}

// What an air bubble's surface mirrors along `refl`, in the bubble's frame
// with `up` straight up, as light it adds (rgb) and how much of the water
// behind the bubble still shows (a). The quad can't read what is behind it,
// so the water is told relative to that: lighter water higher up (`above`,
// added), the same to the sides, darker water below and, at a grazing angle
// near the edge, a dimmer slant through the water behind. That way the
// bubble takes on the water it is in, light shafts and depth and all. Only
// the window of sky overhead (Snell's window, within 49 degrees of straight
// up) is a color of its own.
fn bubble_env(refl: vec3<f32>, up: vec2<f32>, sky: vec3<f32>, above: vec3<f32>) -> vec4<f32> {
    let u = dot(refl.xy, up);
    let below = 1.0 - smoothstep(-0.8, 0.0, u);
    let behind = 1.0 - smoothstep(-0.95, -0.55, refl.z);
    let keep = (1.0 - 0.45 * below) * (1.0 - 0.35 * behind);
    let lit = above * smoothstep(-0.3, 0.6, u) * (1.0 - behind);
    // Kept short of opaque so text over a bubble stays readable.
    let snell = smoothstep(0.5, 0.75, u) * (1.0 - behind);
    return mix(vec4<f32>(lit, keep), vec4<f32>(sky * 0.6, 0.4), snell);
}

// One sample of an air bubble in water, a sphere squashed into the ellipse
// with half-axes `half` (pixels). `q` is the point in the bubble's own frame
// (y down), `up` and `light` are straight up and the way to the light in that
// frame, `above` the light of the water higher up, and `open` runs 0..1 as it
// pops. Returns premultiplied color to lay over the water.
//
// The bubble is a diverging lens. Toward its edge, light in the water meets
// the air past the critical angle (sin = 1 / 1.33) and is all reflected, so
// the outer quarter of the radius mirrors the water around it and, at the top,
// the sky. The middle is clear, with a sharp glint toward the light and a
// faint glow low on the far side, where light came through the bubble.
fn bubble_sample(q: vec2<f32>, half: vec2<f32>, up: vec2<f32>, light: vec2<f32>, sky: vec3<f32>, above: vec3<f32>, open: f32) -> vec4<f32> {
    let e0 = q / half;
    let e = e0 / max(length(e0), 1.0);
    let r = length(e);
    let n = vec3<f32>(e, sqrt(max(1.0 - r * r, 0.0)));
    let rad = min(half.x, half.y);
    // Reflectance: 2% head on, rising steeply to total reflection. As the
    // bubble pops, its film opens from the middle out.
    let crit = mix(0.75, 0.97, open);
    let f = 0.02 + 0.98 * pow(min(r / crit, 1.0), 12.0);
    let refl = vec3<f32>(2.0 * n.z * n.xy, 2.0 * n.z * n.z - 1.0);
    let env = bubble_env(refl, up, sky, above);
    // Through the middle, a small upright view of the water around: lighter
    // at the top, where it shows the water above, darker at the bottom.
    let v = dot(e, up);
    let through = vec4<f32>(above * 0.3 * max(v, 0.0), 0.97 - 0.08 * max(-v, 0.0));
    let seen = mix(through, env, f);
    // The glint is sharp, but never under a pixel across.
    let hv = normalize(normalize(vec3<f32>(light, 0.5)) + vec3<f32>(0.0, 0.0, 1.0));
    let glint = pow(max(dot(n, hv), 0.0), clamp(2.5 * rad * rad, 5.0, 70.0)) * 1.5;
    let z = (r - 0.66) / 0.18;
    let far = smoothstep(0.7, 0.95, dot(e, -light) / max(r, 1e-4)) * exp(-z * z);
    let keep = 1.0 - open;
    let col = seen.rgb + (vec3<f32>(1.0, 0.98, 0.94) * glint + sky * far * 0.25) * keep;
    let a = 1.0 - seen.a + (glint * 0.6 + far * 0.12) * keep;
    return vec4<f32>(col, clamp(a, 0.0, 1.0));
}

// Hue (0..1), saturation and brightness to a color; palette::from_hsv
// works it out the same way.
fn hsv(c: vec3<f32>) -> vec3<f32> {
    let k = clamp(abs(fract(vec3<f32>(c.x) + vec3<f32>(1.0, 2.0 / 3.0, 1.0 / 3.0)) * 6.0 - 3.0) - 1.0, vec3<f32>(0.0), vec3<f32>(1.0));
    return c.z * mix(vec3<f32>(1.0), k, c.y);
}

// Every quad Flow draws, from the cells' letters and backgrounds and the
// pictures and models programs show to Flow's own title bar and panels,
// through the phosphor where there is one.
@fragment
fn fs_quad(in: QuadOut) -> @location(0) vec4<f32> {
    let c = quad_color(in);
    // The color picker shows the colors it offers as they are.
    if (in.params.x > 6.5 && in.params.x < 7.5) {
        return c;
    }
    return on_phosphor(c);
}

fn quad_color(in: QuadOut) -> vec4<f32> {
    let kind = in.params.x;
    let half_size = in.size * 0.5;
    if (kind < 0.5) {
        // Solid rounded rect; params.z > 0 makes it an outline of that width.
        let d = sd_round_box(in.local - half_size, half_size, in.params.y);
        var a = clamp(0.5 - d, 0.0, 1.0);
        if (in.params.z > 0.0) {
            a = a * clamp(0.5 + d + in.params.z, 0.0, 1.0);
        }
        let alpha = a * in.color.a;
        return vec4<f32>(in.color.rgb * alpha, alpha);
    } else if (kind < 1.5) {
        let t = textureSample(tex_main, samp, in.uv);
        let alpha = t.a * in.color.a;
        return vec4<f32>(in.color.rgb * alpha, alpha);
    } else if (kind < 2.5) {
        let t = textureSample(tex_main, samp, in.uv);
        return t * in.color.a;
    } else if (kind < 3.5) {
        // Frosted glass card with a bright hairline edge.
        let d = sd_round_box(in.local - half_size, half_size, in.params.y);
        let a = clamp(0.5 - d, 0.0, 1.0);
        let tn = clamp(in.local.y / max(in.size.y, 1.0), 0.0, 1.0);
        var col = in.color.rgb * (1.08 - tn * 0.16);
        let edge = clamp(1.0 - abs(d + 0.75) / 0.9, 0.0, 1.0);
        col = mix(col, vec3<f32>(1.0), edge * (0.14 + 0.28 * (1.0 - tn)));
        let alpha = a * max(in.color.a, edge * 0.55);
        return vec4<f32>(col * alpha, alpha);
    } else if (kind > 8.5) {
        // A piece of a line Flow draws itself (box_drawing.rs), smoothed
        // over a pixel. params.y above 0 is a ring's radius around seg.xy,
        // in units seg.zw pixels across and down, params.z its half
        // thickness; below 0 a line through seg.xy and seg.zw, params.z
        // thick either side (below -1.5 the segment between them, its ends
        // round: Canvas::capsule); 0 a bar whose soft edges are seg (left,
        // top, right, bottom), a hard one far out, so the quad's side is its
        // edge.
        var a = 0.0;
        if (in.params.y > 0.0) {
            // The distance to the ring in units, made pixels the way it points.
            let q = (in.local - in.seg.xy) / in.seg.zw;
            let len = max(length(q), 1e-4);
            let k = length(q / len * in.seg.zw);
            a = clamp((in.params.z - abs(len - in.params.y)) * k + 0.5, 0.0, 1.0);
        } else if (in.params.y < 0.0) {
            let pa = in.local - in.seg.xy;
            let ba = in.seg.zw - in.seg.xy;
            var dist = abs(pa.x * ba.y - pa.y * ba.x) / max(length(ba), 1e-4);
            if (in.params.y < -1.5) {
                let along = clamp(dot(pa, ba) / max(dot(ba, ba), 1e-4), 0.0, 1.0);
                dist = length(pa - ba * along);
            }
            a = clamp(in.params.z + 0.5 - dist, 0.0, 1.0);
        } else {
            let e = min(in.local - in.seg.xy, in.seg.zw - in.local);
            a = clamp(0.5 + e.x, 0.0, 1.0) * clamp(0.5 + e.y, 0.0, 1.0);
        }
        let alpha = a * in.color.a;
        return vec4<f32>(in.color.rgb * alpha, alpha);
    } else if (kind > 7.5) {
        // A piece of an image a program showed: premultiplied, like color glyphs.
        return textureSample(tex_main, samp, in.uv) * in.color.a;
    } else if (kind > 6.5) {
        // The color picker: its field (seg.y 0), saturation across and
        // brightness up at hue seg.x, or its hue bar (seg.y 1).
        let t = clamp(in.local / max(in.size, vec2<f32>(1.0)), vec2<f32>(0.0), vec2<f32>(1.0));
        var col = hsv(vec3<f32>(t.x, 1.0, 1.0));
        if (in.seg.y < 0.5) {
            col = hsv(vec3<f32>(in.seg.x, t.x, 1.0 - t.y));
        }
        let d = sd_round_box(in.local - half_size, half_size, in.params.y);
        let a = clamp(0.5 - d, 0.0, 1.0) * in.color.a;
        return vec4<f32>(col * a, a);
    } else if (kind > 5.5) {
        // An air bubble (fx/bubbles.rs) tilted by params.y; seg holds how far
        // it has popped and the way to the light on screen.
        let cs = cos(in.params.y);
        let sn = sin(in.params.y);
        let p = in.local - half_size;
        let q = vec2<f32>(cs * p.x + sn * p.y, cs * p.y - sn * p.x);
        let up = vec2<f32>(-sn, -cs);
        let light = vec2<f32>(cs * in.seg.y + sn * in.seg.z, cs * in.seg.z - sn * in.seg.y);
        // Coverage from the distance to the edge in pixels, smooth at any size.
        let e = q / half_size;
        let le = max(length(e), 1e-4);
        let d = (le - 1.0) * le / max(length(e / half_size), 1e-4);
        let cover = clamp(0.5 - d, 0.0, 1.0);
        // The water higher up, which it mirrors: shade_water_below's color
        // at a shallower depth than here.
        let res = max(G.resolution, vec2<f32>(1.0));
        let sy = 34.0 * G.ui_scale / res.y;
        let depth = clamp((in.pos.y / res.y - sy) / (1.0 - sy), 0.0, 1.0);
        let above = mix(vec3<f32>(0.06, 0.30, 0.40), vec3<f32>(0.01, 0.04, 0.08), pow(depth * 0.6, 0.8)) * 0.5;
        // Four samples a pixel keep the rim and glints of small bubbles smooth.
        let sky = in.color.rgb;
        let open = in.seg.x;
        var acc = bubble_sample(q + vec2<f32>(-0.125, -0.375), half_size, up, light, sky, above, open);
        acc += bubble_sample(q + vec2<f32>(0.375, -0.125), half_size, up, light, sky, above, open);
        acc += bubble_sample(q + vec2<f32>(0.125, 0.375), half_size, up, light, sky, above, open);
        acc += bubble_sample(q + vec2<f32>(-0.375, 0.125), half_size, up, light, sky, above, open);
        // Lit as the water around it is, by the sun setting.
        let lit = 0.45 + 0.55 * G.sun;
        return vec4<f32>(acc.rgb * lit, acc.a) * (0.25 * cover * in.color.a * (1.0 - open));
    } else if (kind > 4.5) {
        // Glowing segment: white-hot core inside a colored glow.
        let pa = in.local - in.seg.xy;
        let ba = in.seg.zw - in.seg.xy;
        let h = clamp(dot(pa, ba) / max(dot(ba, ba), 1e-4), 0.0, 1.0);
        let dist = length(pa - ba * h);
        let core = 1.0 - smoothstep(in.params.y * 0.5, in.params.y * 0.5 + 1.0, dist);
        let glow = exp(-pow(dist / max(in.params.z * 0.5, 0.5), 2.0));
        let alpha = clamp(core + glow * 0.45, 0.0, 1.0) * in.color.a;
        let col = mix(in.color.rgb, vec3<f32>(1.0), core);
        return vec4<f32>(col * alpha, alpha);
    }
    // Soft glow / shadow.
    let blur = max(in.params.z, 1.0);
    let d = sd_round_box(in.local - half_size, half_size, in.params.y);
    let a = (1.0 - smoothstep(-blur * 0.25, blur, d)) * in.color.a;
    return vec4<f32>(in.color.rgb * a, a);
}
