// A 3D model a program placed (render/models.rs), drawn into its picture:
// positions in pixels from the picture's center, y up, through `mvp`; lit
// from `light` on both sides, with a little rim of light at the edges, and
// a glossy part with a highlight and a sheen as an eye `shine.y` pixels in
// front would see them; the base color times its texture and the vertices'
// colors (glTF's COLOR_0, white where it has none, so alpha too), written
// premultiplied as the images are.
//
// A glTF's material on top: its roughness (one less `shine.x`, times the
// metal-roughness texture's green) sets the gloss, its metalness
// (`shine.z`, times the texture's blue) turns the color from lit to
// reflected, a normal map (`emission.w` on, `shine.w` its strength) bends
// the normal along the vertex's tangent, an occlusion map (`alpha.z` its
// strength) shades the room's light, and emission adds light of its own.
// Each texture reads the set of uvs its `uv` rows name, moved by them (a
// texture transform). Its alpha is ignored for a solid part, cuts out
// below `alpha.y` for a cut-out, and is kept for a see-through part, which
// the pipeline blends. A matte part without them is lit just as before,
// which is what programs that undo Flow's light for flat colors count on.
//
// Its extensions, each only where `layers` (or `alpha.w`) says the part
// has it, so a part without them is drawn as it always was: unlit (its
// color alone), specular and ior (how strongly and in what color a
// non-metal's glaze reflects), sheen (a fabric's soft rim), clearcoat (a
// clear glaze over the rest, with a normal of its own), light through the
// part (see-through under a film of its color, its reflections kept),
// anisotropy (highlights stretched along a way in the surface, `more.x`),
// and iridescence (a thin film coloring the glaze's reflections, `more.y`).
// A glTF's own lights (`light.w` of them, in `lights`) take the place of
// Flow's one; the room's light (the ambient, the rim, the broad light
// above, the reflections) stays.

struct Draw {
    mvp: mat4x4<f32>,
    model: mat4x4<f32>,
    color: vec4<f32>,
    light: vec4<f32>,
    shine: vec4<f32>,
    emission: vec4<f32>,
    alpha: vec4<f32>,
    layers: vec4<f32>,
    specular: vec4<f32>,
    coat: vec4<f32>,
    sheen: vec4<f32>,
    transmit: vec4<f32>,
    attenuation: vec4<f32>,
    // 1 seen through the file's perspective camera, 0 straight on.
    view: vec4<f32>,
    // Anisotropy's strength, its way's turn (cosine, sine), 1 with a map.
    anisotropy: vec4<f32>,
    // Iridescence's strength, its film's ior, its thinnest and thickest (nm).
    iridescence: vec4<f32>,
    // 1 for anisotropy, for iridescence.
    more: vec4<f32>,
    lights: array<vec4<f32>, 16>,
    uv: array<vec4<f32>, 32>,
};

@group(0) @binding(0) var<uniform> d: Draw;
@group(0) @binding(1) var tex: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;
@group(0) @binding(3) var metal_rough: texture_2d<f32>;
@group(0) @binding(4) var bumps: texture_2d<f32>;
@group(0) @binding(5) var glow: texture_2d<f32>;
@group(0) @binding(6) var occlusion: texture_2d<f32>;
@group(0) @binding(7) var specular_strength: texture_2d<f32>;
@group(0) @binding(8) var specular_color: texture_2d<f32>;
@group(0) @binding(9) var coat_strength: texture_2d<f32>;
@group(0) @binding(10) var coat_roughness: texture_2d<f32>;
@group(0) @binding(11) var coat_bumps: texture_2d<f32>;
@group(0) @binding(12) var sheen_color: texture_2d<f32>;
@group(0) @binding(13) var sheen_roughness: texture_2d<f32>;
@group(0) @binding(14) var transmission: texture_2d<f32>;
@group(0) @binding(15) var thickness: texture_2d<f32>;
@group(0) @binding(16) var anisotropy_map: texture_2d<f32>;
@group(0) @binding(17) var film_map: texture_2d<f32>;

struct Out {
    @builtin(position) pos: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) at: vec3<f32>,
    @location(3) tangent: vec4<f32>,
    @location(4) uv1: vec2<f32>,
    @location(5) color: vec4<f32>,
};

@vertex
fn vs_model(@location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) uv: vec2<f32>, @location(3) tangent: vec4<f32>, @location(4) uv1: vec2<f32>, @location(5) color: vec4<f32>) -> Out {
    var o: Out;
    o.pos = d.mvp * vec4<f32>(pos, 1.0);
    o.normal = (d.model * vec4<f32>(normal, 0.0)).xyz;
    o.uv = uv;
    o.at = (d.model * vec4<f32>(pos, 1.0)).xyz;
    o.tangent = vec4<f32>((d.model * vec4<f32>(tangent.xyz, 0.0)).xyz, tangent.w);
    o.uv1 = uv1;
    o.color = color;
    return o;
}

// Where texture `k` (in `Material::maps`' order) samples.
fn mapped(k: u32, uv0: vec2<f32>, uv1: vec2<f32>) -> vec2<f32> {
    let a = d.uv[k * 2u];
    let b = d.uv[k * 2u + 1u];
    let p = vec3<f32>(select(uv0, uv1, a.w > 0.5), 1.0);
    return vec2<f32>(dot(a.xyz, p), dot(b.xyz, p));
}

// Normal `n` bent by a normal map's texel `bump`, `scale` its strength, in
// the frame of the tangent `t` (squared to `n` already), the bitangent
// (`cross(n, t) * w`, glTF's) and the normal.
fn bent(n: vec3<f32>, t: vec3<f32>, w: f32, bump: vec3<f32>, scale: f32) -> vec3<f32> {
    let m = vec3<f32>((bump.xy * 2.0 - 1.0) * scale, bump.z * 2.0 - 1.0);
    let tt = normalize(t);
    let b = cross(n, tt) * select(1.0, -1.0, w < 0.0);
    return normalize(tt * m.x + b * m.y + n * max(m.z, 0.0));
}

// A half vector `h` with its part `along` an anisotropic surface's way
// taken down (`along` the way, as long as the square root of the share
// taken), so a highlight reaching that way lasts further along it: none
// taken where `along` is nothing.
fn stretched(h: vec3<f32>, along: vec3<f32>) -> vec3<f32> {
    return normalize(h - along * dot(h, along));
}

// The color a thin film of `ior`, `thickness` nanometres thick, reflects
// seen at `cos_v` (the cosine of the angle from its normal): each of red,
// green and blue (650, 532 and 450 nm) as bright as the light reflected
// from its two sides comes back in step, 0 to 1. A film of no thickness
// reflects white, so one thinning out leaves the surface as it was.
fn thin_film(cos_v: f32, ior: f32, thickness: f32) -> vec3<f32> {
    let sin2 = (1.0 - cos_v * cos_v) / (ior * ior);
    let cos_t = sqrt(max(1.0 - sin2, 0.0));
    let path = 2.0 * ior * thickness * cos_t;
    return 0.5 + 0.5 * cos(6.2831853 * path / vec3<f32>(650.0, 532.0, 450.0));
}

// What the lights give a point at `at` facing `n`, seen from `v`: their
// light on it, their glint toward the eye (stretched `along` an
// anisotropic surface's way), and their light wrapped a way round it (for
// a fabric's sheen), each in the lights' colors.
struct Lit {
    diffuse: vec3<f32>,
    glint: vec3<f32>,
    wrap: vec3<f32>,
};

fn lighting(n: vec3<f32>, v: vec3<f32>, at: vec3<f32>, along: vec3<f32>) -> Lit {
    var o: Lit;
    let count = u32(d.light.w + 0.5);
    if (count == 0u) {
        // Flow's own: one white light from above left, in front.
        let facing = dot(n, d.light.xyz);
        o.diffuse = vec3<f32>(max(facing, 0.0));
        o.glint = vec3<f32>(pow(max(dot(n, stretched(normalize(d.light.xyz + v), along)), 0.0), 60.0));
        o.wrap = vec3<f32>(clamp(facing * 0.5 + 0.5, 0.0, 1.0));
        return o;
    }
    o.diffuse = vec3<f32>(0.0);
    o.glint = vec3<f32>(0.0);
    o.wrap = vec3<f32>(0.0);
    for (var k = 0u; k < count; k = k + 1u) {
        let place = d.lights[k * 4u];
        let tone = d.lights[k * 4u + 1u];
        let aim = d.lights[k * 4u + 2u];
        let more = d.lights[k * 4u + 3u];
        var l = place.xyz;
        var strength = tone.rgb;
        if (place.w > 0.5) {
            // A light with a place: fainter with the square of the
            // distance (no nearer than `more.z` counts), its share given at
            // the model's middle (`more.y` away), and to nothing at its
            // range as glTF fades it; a spot's cone on top.
            let to = place.xyz - at;
            let far = dot(to, to);
            l = to * inverseSqrt(max(far, 1e-8));
            var falloff = more.y / max(far, more.z);
            if (tone.w > 0.0) {
                let r = far / (tone.w * tone.w);
                let fade = clamp(1.0 - r * r, 0.0, 1.0);
                falloff = falloff * fade * fade;
            }
            let cone = clamp(dot(aim.xyz, -l) * aim.w + more.x, 0.0, 1.0);
            strength = strength * falloff * cone * cone;
        }
        let facing = dot(n, l);
        o.diffuse = o.diffuse + max(facing, 0.0) * strength;
        o.glint = o.glint + pow(max(dot(n, stretched(normalize(l + v), along)), 0.0), 60.0) * strength;
        o.wrap = o.wrap + clamp(facing * 0.5 + 0.5, 0.0, 1.0) * strength;
    }
    return o;
}

@fragment
fn fs_model(i: Out, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    // Every texture first: sampling must not wait on a branch. (The
    // extensions' textures below are sampled at their one level, which
    // needs nothing from the pixels around, so they can wait on whether
    // the part has them.) The base color is its factor, its texture and
    // the vertices' colors, its alpha too, as glTF has it.
    let base = d.color * i.color * textureSample(tex, samp, mapped(0u, i.uv, i.uv1));
    let mr = textureSample(metal_rough, samp, mapped(1u, i.uv, i.uv1));
    let bump = textureSample(bumps, samp, mapped(2u, i.uv, i.uv1)).xyz;
    let lit = textureSample(glow, samp, mapped(3u, i.uv, i.uv1)).rgb;
    let shade = textureSample(occlusion, samp, mapped(4u, i.uv, i.uv1)).r;
    var a = 1.0;
    if (d.alpha.x > 1.5) {
        // See-through: nothing to blend where it's clear.
        a = base.a;
        if (a < 0.002) {
            discard;
        }
    } else if (d.alpha.x > 0.5) {
        // A cut-out's clear parts leave what's behind them.
        if (base.a < d.alpha.y) {
            discard;
        }
    }
    if (d.alpha.w > 0.5) {
        // Unlit (KHR_materials_unlit): its color, nothing else.
        return vec4<f32>(min(base.rgb, vec3<f32>(1.0)) * a, a);
    }
    var n = normalize(i.normal);
    // The surface's own normal, before the normal map: the clearcoat's.
    let surface = n;
    // The normal map, in the frame of the tangent, the bitangent
    // (`cross(n, t) * w`, glTF's) and the normal.
    let t = i.tangent.xyz - n * dot(n, i.tangent.xyz);
    if (d.emission.w > 0.5 && dot(t, t) > 1e-12) {
        n = bent(n, t, i.tangent.w, bump, d.shine.w);
    }
    // The back of a face turned to the viewer is lit as its front.
    if (!front) {
        n = -n;
    }
    let v = normalize(vec3<f32>(0.0, 0.0, d.shine.y) - i.at);
    let above = normalize(vec3<f32>(-0.3, 0.9, 0.3));
    // KHR_materials_anisotropy: its way in the surface, the tangent turned
    // toward the bitangent by its rotation (and by its map's red and green,
    // its strength by its blue), along which its highlights stretch.
    var along = vec3<f32>(0.0);
    if (d.more.x > 0.5 && dot(t, t) > 1e-12) {
        var way = vec2<f32>(1.0, 0.0);
        var s = d.anisotropy.x;
        if (d.anisotropy.w > 0.5) {
            let a = textureSampleLevel(anisotropy_map, samp, mapped(14u, i.uv, i.uv1), 0.0);
            way = a.rg * 2.0 - 1.0;
            s = s * a.b;
        }
        way = vec2<f32>(way.x * d.anisotropy.y - way.y * d.anisotropy.z, way.x * d.anisotropy.z + way.y * d.anisotropy.y);
        let ta = normalize(t - n * dot(n, t));
        let ba = cross(n, ta) * select(1.0, -1.0, i.tangent.w < 0.0);
        let w = ta * way.x + ba * way.y;
        if (dot(w, w) > 1e-8) {
            along = normalize(w) * sqrt(0.9 * clamp(s, 0.0, 1.0));
        }
    }
    // KHR_materials_iridescence: a thin film over the glaze, coloring what
    // it reflects (by twice the film's color, white on the whole), as much
    // as its strength says.
    var film = vec3<f32>(1.0);
    if (d.more.y > 0.5) {
        let f = textureSampleLevel(film_map, samp, mapped(15u, i.uv, i.uv1), 0.0);
        let k = clamp(d.iridescence.x * f.r, 0.0, 1.0);
        let thick = mix(d.iridescence.z, d.iridescence.w, f.g);
        film = mix(vec3<f32>(1.0), 2.0 * thin_film(max(dot(n, v), 0.0), d.iridescence.y, thick), k);
    }
    let light = lighting(n, v, i.at, along);
    // How much of the room's light reaches here: all of it without an
    // occlusion map.
    let ao = 1.0 + d.alpha.z * (shade - 1.0);
    // A little light round its edges: where it turns away from the eye,
    // which straight on is where it turns side on to the picture (and is
    // kept so, as programs that undo the light for flat colors count on).
    let facing = select(abs(n.z), abs(dot(n, v)), d.view.x > 0.5);
    let rim = pow(1.0 - facing, 3.0) * 0.25 * ao;
    let matte = 0.35 * ao + 0.7 * light.diffuse + rim;
    var rgb = base.rgb * matte;
    let gloss = 1.0 - (1.0 - d.shine.x) * mr.g;
    let metal = clamp(d.shine.z * mr.b, 0.0, 1.0);
    // The light a non-metal's glaze reflects, for a part light passes
    // through, which keeps it.
    var reflected = vec3<f32>(0.0);
    if (gloss > 0.0) {
        // A glaze: a broad light high above glancing off the tops toward the
        // eye, the main light's small glint, and the room reflected all
        // over (a bright ceiling, a dim floor), more of it toward the edges.
        let spot = pow(max(dot(n, stretched(normalize(above + v), along)), 0.0), 14.0);
        let glint = light.glint;
        let up = reflect(-v, n).y;
        let room = mix(0.05, 0.24, smoothstep(-0.8, 0.8, up)) * ao;
        let edge = pow(1.0 - max(dot(n, v), 0.0), 3.0);
        var glazed: vec3<f32>;
        if (d.layers.x > 0.5) {
            // KHR_materials_specular and _ior: head on it reflects as its
            // color and strength say (the strength's texture in alpha, the
            // color's in rgb), at the edges as its strength alone, so a
            // glaze of no strength reflects nothing and the color under it
            // isn't dimmed for it.
            let s = textureSampleLevel(specular_strength, samp, mapped(5u, i.uv, i.uv1), 0.0).a;
            let c = textureSampleLevel(specular_color, samp, mapped(6u, i.uv, i.uv1), 0.0).rgb;
            let head_on = min(d.specular.rgb * c * s, vec3<f32>(25.0));
            let edges = d.specular.w * s;
            reflected = gloss * ((vec3<f32>(spot * 0.35) + glint) * head_on + vec3<f32>(room) * (0.5 * head_on + edge * edges)) * film;
            glazed = rgb * (1.0 - 0.1 * gloss * edges) + reflected;
        } else {
            reflected = gloss * (vec3<f32>(spot * 0.35) + glint * 1.0 + vec3<f32>(room * (0.5 + edge))) * film;
            glazed = rgb * (1.0 - 0.1 * gloss) + reflected;
        }
        // A metal has no light of its own color under the glaze: it shows
        // the room in its color, sharper the smoother it is (blurred to
        // nothing, as lit as a matte color), its highlights in its color
        // too, whitening only at the very edges.
        let mirror = mix(0.12, 1.1, smoothstep(-0.7, 0.7, up)) * ao + rim;
        let shiny = (base.rgb * (mix(matte, vec3<f32>(mirror), gloss) + gloss * (vec3<f32>(spot * 0.5) + glint * 1.5)) + gloss * edge * 0.25 * (1.0 - base.rgb)) * film;
        rgb = mix(glazed, shiny, metal);
    }
    if (d.layers.y > 0.5) {
        // KHR_materials_sheen: a fabric's fibres catch the light toward its
        // edges, where it grazes them: a rim of the sheen's color, hugging
        // the edges when smooth and spreading in when rough, lit by the
        // room and by the lights wrapped round, and the color under it
        // dimmed by as much.
        let sc = d.sheen.rgb * textureSampleLevel(sheen_color, samp, mapped(10u, i.uv, i.uv1), 0.0).rgb;
        let sr = clamp(d.sheen.w * textureSampleLevel(sheen_roughness, samp, mapped(11u, i.uv, i.uv1), 0.0).a, 0.0, 1.0);
        let grazing = pow(1.0 - max(dot(n, v), 0.0), mix(5.0, 1.5, sr));
        rgb = rgb * (1.0 - max(sc.r, max(sc.g, sc.b)) * grazing * 0.5) + sc * grazing * (0.4 * ao + 0.6 * light.wrap);
    }
    if (d.layers.z > 0.5) {
        // KHR_materials_clearcoat: a clear glaze over all of it, glossy as
        // its roughness says (its texture's green), following the surface
        // or its own normal map, reflecting as Flow's own glaze does (more
        // toward the edges) and dimming what's under it as that does. Not
        // by Fresnel's full share at the edges: the room it reflects there
        // is dim, so the edges would go dark.
        let cc = clamp(d.coat.x * textureSampleLevel(coat_strength, samp, mapped(7u, i.uv, i.uv1), 0.0).r, 0.0, 1.0);
        let cg = clamp(1.0 - (1.0 - d.coat.y) * textureSampleLevel(coat_roughness, samp, mapped(8u, i.uv, i.uv1), 0.0).g, 0.0, 1.0);
        var cn = surface;
        if (d.coat.w > 0.5 && dot(t, t) > 1e-12) {
            cn = bent(surface, t, i.tangent.w, textureSampleLevel(coat_bumps, samp, mapped(9u, i.uv, i.uv1), 0.0).xyz, d.coat.z);
        }
        if (!front) {
            cn = -cn;
        }
        let coat_light = lighting(cn, v, i.at, vec3<f32>(0.0));
        let spot = pow(max(dot(cn, normalize(above + v)), 0.0), 14.0);
        let room = mix(0.05, 0.24, smoothstep(-0.8, 0.8, reflect(-v, cn).y)) * ao;
        let edge = pow(1.0 - max(dot(cn, v), 0.0), 3.0);
        let glaze = cc * cg * (vec3<f32>(spot * 0.35) + coat_light.glint + vec3<f32>(room * (0.5 + edge)));
        let under = 1.0 - 0.1 * cc * cg;
        rgb = rgb * under + glaze;
        reflected = reflected * under + glaze;
    }
    if (d.layers.w > 0.5) {
        // KHR_materials_transmission (and _volume): light passes through.
        // The picture is drawn on its own and laid over the terminal, so
        // what's behind can't be bent or tinted, only seen: as much of it
        // as passes is drawn as clear, under a film of the color the light
        // takes through it (its base color, deepened through a volume's
        // thickness by the attenuation color) as deep as that absorbs, a
        // rough part frosted, and the reflections kept whole. A metal
        // passes nothing.
        let amount = textureSampleLevel(transmission, samp, mapped(12u, i.uv, i.uv1), 0.0).r;
        let thick = textureSampleLevel(thickness, samp, mapped(13u, i.uv, i.uv1), 0.0).g;
        let through = clamp(d.transmit.x * amount, 0.0, 1.0) * (1.0 - metal);
        var tint = base.rgb;
        if (d.transmit.z > 0.0) {
            tint = tint * pow(max(d.attenuation.rgb, vec3<f32>(1e-4)), vec3<f32>(d.transmit.y * thick * d.transmit.z / 2.2));
        }
        var film = clamp((1.0 - (tint.r + tint.g + tint.b) / 3.0) * 0.8 + (1.0 - gloss) * 0.4, 0.0, 1.0);
        // A volume's far side: its near side's film took the whole thickness.
        if (!front && d.transmit.y > 0.0) {
            film = 0.0;
        }
        let shown = min((1.0 - through) * rgb + through * (tint * film + reflected) + d.emission.rgb * lit, vec3<f32>(1.0)) * a;
        let cover = a * ((1.0 - through) + through * film);
        // Reflections over the clear part cover what's behind as much as
        // they're bright, so the color stays premultiplied.
        return vec4<f32>(shown, max(cover, max(shown.r, max(shown.g, shown.b))));
    }
    rgb = min(rgb + d.emission.rgb * lit, vec3<f32>(1.0));
    return vec4<f32>(rgb * a, a);
}
