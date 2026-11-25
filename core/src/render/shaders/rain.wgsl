// The rain map (see render/rain.rs): drops drawn as lenses, and the clear
// paths behind them, max-blended so overlaps keep the larger value.
// x: height of the water in pixels / 16. y: how clear the glass is, 0..1.
// z, w: where in its drop the pixel sits (`lens_coord`), plus 2, times x,
// so that where drops overlap, the taller one's wins.

struct Params {
    resolution: vec2<f32>,
    // Pixels per point.
    scale: f32,
    _pad: f32,
};

// How clear the glass is right next to a drop, which has taken up the mist
// around it, and how far that reaches: a point and a half plus this much of
// the drop's radius.
const HALO: f32 = 0.75;
const HALO_REACH: f32 = 0.45;
// How tall the beads along a wiped path are for their radius, as drops are
// (DOME in fx/rain.rs).
const DOME: f32 = 0.55;

@group(0) @binding(0) var<uniform> P: Params;

struct In {
    // Head x, y and tail x, y in window pixels.
    @location(0) ends: vec4<f32>,
    // Drop: radius at the head and at the tail. Streak: half width, radius of
    // the beads of water along its sides. Splat: radius, where its lobes go.
    // Then kind (0 drop, 1 clear streak, 2 splat, 4 a droplet of spray,
    // drawn as a drop), strength.
    @location(1) params: vec4<f32>,
};

struct Out {
    @builtin(position) pos: vec4<f32>,
    @location(0) px: vec2<f32>,
    @location(1) @interpolate(flat) ends: vec4<f32>,
    @location(2) @interpolate(flat) params: vec4<f32>,
};

@vertex
fn vs_rain(@builtin(vertex_index) vi: u32, q: In) -> Out {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
    );
    // A box around the shape, a pixel bigger all round, and round a drop
    // its halo too (a splat's lobes reach past its radius), round a streak
    // the beads along its sides.
    let kind = q.params.z;
    var r = q.params.x + 1.0;
    if (is_drop(kind)) {
        r = max(r, q.params.y + 1.0) + halo_width(q.params.x);
    } else if (kind > 1.5 && kind < 2.5) {
        r = q.params.x * 1.25 + 1.0 + halo_width(q.params.x);
    } else {
        r += q.params.y * 1.6 + 2.1 * P.scale;
    }
    let lo = min(q.ends.xy, q.ends.zw) - vec2<f32>(r);
    let hi = max(q.ends.xy, q.ends.zw) + vec2<f32>(r);
    let p = mix(lo, hi, corners[vi % 6u]);
    var out: Out;
    out.pos = vec4<f32>(p.x / P.resolution.x * 2.0 - 1.0, 1.0 - p.y / P.resolution.y * 2.0, 0.0, 1.0);
    out.px = p;
    out.ends = q.ends;
    out.params = q.params;
    return out;
}

// A drop, or a droplet of spray drawn as one.
fn is_drop(kind: f32) -> bool {
    return kind < 0.5 || kind > 3.5;
}

fn hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

// How far out from a drop of radius `r` (pixels) the glass is clearer.
fn halo_width(r: f32) -> f32 {
    return 1.5 * P.scale + r * HALO_REACH;
}

// Where `p0` (from the middle of the head) sits in a drop of radius r1 at
// its head, swept to r2 at its tail `ba` away: 0 in the middle of the head
// and length 1 all round the rim. The front half is the head's own (as on a
// round drop); the half toward the tail is drawn out to reach its end,
// squeezed more the nearer it, so the drop curves both ways and stays a
// lens however long it is.
fn lens_coord(p0: vec2<f32>, ba: vec2<f32>, r1: f32, r2: f32) -> vec2<f32> {
    let len = length(ba);
    let s = dot(p0, ba) / max(len, 1e-3);
    if (len <= r1 - r2 + 0.01 || s <= 0.0) {
        return p0 / r1;
    }
    let d = ba / len;
    let n = vec2<f32>(-d.y, d.x);
    // Along it: v = k / (1 + (m − 1)/m · k) for k radii from the middle,
    // which starts out as on a round drop and reaches 1 at the tail's end,
    // m radii away.
    let m = (len + r2) / r1;
    let k = s / r1;
    let v = k * m / (m + (m - 1.0) * k);
    // Across it, out to its outline here: the head's circle, the tail's or
    // the straight side between them.
    let sb = (r1 - r2) / len;
    let cb = sqrt(max(1.0 - sb * sb, 0.0));
    let s1 = r1 * sb;
    let s2 = len + r2 * sb;
    var w = max(sqrt(max(r1 * r1 - s * s, 0.0)), sqrt(max(r2 * r2 - (s - len) * (s - len), 0.0)));
    if (s > s1 && s < s2) {
        w = max(w, mix(r1, r2, (s - s1) / (s2 - s1)) * cb);
    }
    let across = dot(p0, n) / max(w, 1e-3) * sqrt(max(1.0 - v * v, 0.0));
    return across * n + v * d;
}

@fragment
fn fs_rain(in: Out) -> @location(0) vec4<f32> {
    let a = in.ends.xy;
    let ba = in.ends.zw - a;
    let p0 = in.px - a;
    let kind = in.params.z;
    if (kind > 1.5 && kind < 2.5) {
        // A drop striking: flattened into a splat, its outline in lobes
        // and its rim thrown up in a crown, lower in the middle.
        let r0 = in.params.x;
        let ang = atan2(p0.y, p0.x);
        let seed = in.params.y * 6.2831853;
        let rt = r0 * (1.0 + 0.12 * sin(ang * 7.0 + seed) + 0.06 * sin(ang * 13.0 + seed * 2.7));
        let d = length(p0);
        let depth = rt - d;
        let crown = 0.35 + 0.65 * smoothstep(0.3, 0.85, d / rt);
        let h = sqrt(max(rt * rt - d * d, 0.0)) * crown * in.params.w / 16.0 * clamp(depth + 0.5, 0.0, 1.0);
        let halo = HALO * (1.0 - smoothstep(0.0, halo_width(r0), -depth));
        return vec4<f32>(h, halo, (p0 / rt + 2.0) * h);
    }
    if (is_drop(kind)) {
        // A drop: half a sphere of radius r1 at the head, swept to one of
        // r2 at the tail while it runs (or sags): round, a teardrop, an egg.
        // Its height is that of the sphere along the way that stands
        // tallest here, the t where R(t)² − |p − c(t)|² is largest.
        let r1 = in.params.x;
        let dr = in.params.y - r1;
        let l2 = dot(ba, ba);
        var t = 0.0;
        if (l2 > dr * dr + 1e-4) {
            t = clamp((r1 * dr + dot(p0, ba)) / (l2 - dr * dr), 0.0, 1.0);
        }
        let rt = r1 + dr * t;
        let off = p0 - ba * t;
        let depth = rt - length(off);
        // Its outermost half pixel fades out, so outlines aren't jagged.
        let h = sqrt(max(rt * rt - dot(off, off), 0.0)) * in.params.w / 16.0 * clamp(depth + 0.5, 0.0, 1.0);
        let halo = HALO * (1.0 - smoothstep(0.0, halo_width(r1), -depth));
        return vec4<f32>(h, halo, (lens_coord(p0, ba, r1, in.params.y) + 2.0) * h);
    }
    // A clear streak: clear across most of its width, with a soft edge.
    let len = max(length(ba), 1e-3);
    let t = clamp(dot(p0, ba) / (len * len), 0.0, 1.0);
    let r = max(in.params.x, 0.01);
    let clear = (1.0 - smoothstep(r * 0.6, r, length(p0 - ba * t))) * in.params.w;
    // Maybe the water pushed aside, in two uneven rows of small beads along
    // each side (the outer one sparser and finer) that dry up as it fogs
    // over: in some cells of a row, a bead of 0.4 to 1.6 times `ridge` in
    // radius, as tall for it as a drop. Only beads along this piece of the
    // path count, so none come twice where the next piece carries on.
    let ridge = in.params.y * in.params.w;
    let dir = ba / len;
    let across = vec2<f32>(dir.y, -dir.x);
    var water = 0.0;
    var at_bead = vec2<f32>(0.0);
    if (ridge > 0.01) {
        let q = vec2<f32>(dot(p0, dir), dot(p0, across));
        let side = select(-1.0, 1.0, q.y > 0.0);
        let spacing = 2.4 * P.scale;
        let seed = fract(in.ends.xy * 0.0137) * 97.0;
        for (var row = 0; row < 2; row = row + 1) {
            let fr = f32(row);
            let cell = floor(q.x / spacing - fr * 0.5);
            let some = 0.6 - fr * 0.15;
            for (var k = -1; k <= 1; k = k + 1) {
                let c = cell + f32(k);
                let h = vec2<f32>(hash(vec2<f32>(c, side + fr * 3.0) + seed), hash(vec2<f32>(side - fr * 5.0, c) + seed.yx));
                let lat = r * 0.9 + fr * spacing * 0.6 + (h.x / some - 0.5) * spacing * 0.5;
                let at = vec2<f32>((c + fr * 0.5 + 0.15 + 0.7 * h.y) * spacing, side * lat);
                if (h.x > some || at.x < 0.0 || at.x >= len) {
                    continue;
                }
                let rb = ridge * (0.4 + 1.2 * fract(h.y * 7.3)) * (1.0 - 0.3 * fr);
                let d = q - at;
                let hb = sqrt(max(rb * rb - dot(d, d), 0.0)) * DOME * clamp(rb - length(d) + 0.5, 0.0, 1.0);
                if (hb > water) {
                    water = hb;
                    at_bead = d / rb;
                }
            }
        }
    }
    let h = water / 16.0;
    let u = at_bead.x * dir + at_bead.y * across;
    return vec4<f32>(h, clear, (u + 2.0) * h);
}
