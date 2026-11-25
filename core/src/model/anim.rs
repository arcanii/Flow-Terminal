//! A glTF's own motion: its node tree, skins and morph targets, and the
//! animations that move them (`Clip`), sampled in time and applied to the
//! vertices on the CPU, each frame's written to the GPU whole. Its lights
//! and camera hang from nodes too, so a clip that moves those moves them,
//! posed with the rest, and so do the copies of a mesh its node's
//! instances make (EXT_mesh_gpu_instancing), each placed in the mesh's own
//! space before the node places it.
//!
//! On the CPU rather than in the vertex shader because Flow draws wherever
//! wgpu does, OpenGL ES and software adapters too, where a vertex shader may
//! not read a storage buffer of joints or morph targets, and a uniform
//! buffer holds only a few hundred joints; because a click's ray cast
//! (`super::pick`) needs the posed triangles anyway; and because the models
//! a terminal shows are small, so posing a few thousand vertices a frame
//! costs little beside drawing them.

use super::{cross, dot, mul, normalize, sub, transform, Light, Part, Vertex, IDENTITY};

/// Column-major, as glTF's matrices are.
pub type Mat4 = [[f32; 4]; 4];

/// How a track fills the time between its keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interp {
    /// Each key's value until the next.
    Step,
    /// In a straight line; a rotation along the shorter arc.
    Linear,
    /// Through each key with the tangents the file gives (Hermite).
    Cubic,
}

/// One animated value over time: keys at `times` (seconds, ascending), each
/// `width` numbers (for a cubic spline, three times that: the in-tangent,
/// the value and the out-tangent).
#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    times: Vec<f32>,
    values: Vec<f32>,
    pub width: usize,
    pub interp: Interp,
}

impl Track {
    /// None if the values don't fill whole keys, or the times go backwards.
    pub fn new(times: Vec<f32>, values: Vec<f32>, interp: Interp) -> Option<Track> {
        let per = if interp == Interp::Cubic { 3 } else { 1 };
        let keys = times.len();
        if keys == 0 || values.is_empty() || !values.len().is_multiple_of(keys * per) {
            return None;
        }
        if times.iter().any(|t| !t.is_finite()) || times.windows(2).any(|w| w[1] < w[0]) {
            return None;
        }
        Some(Track { width: values.len() / (keys * per), times, values, interp })
    }

    /// The last key's time.
    pub fn end(&self) -> f32 {
        self.times.last().copied().unwrap_or(0.0)
    }

    /// Key `k`'s numbers: for a cubic spline, `which` 0 is its in-tangent, 1
    /// its value and 2 its out-tangent.
    fn key(&self, k: usize, which: usize) -> &[f32] {
        let per = if self.interp == Interp::Cubic { 3 } else { 1 };
        let at = (k * per + which) * self.width;
        &self.values[at..at + self.width]
    }

    /// The value `t` seconds in, into `out` (`width` long): the first key's
    /// before it, the last's after. A rotation's quaternion stays unit length.
    pub fn sample(&self, t: f32, rotation: bool, out: &mut [f32]) {
        let value = usize::from(self.interp == Interp::Cubic);
        let next = self.times.partition_point(|&x| x <= t);
        let last = self.times.len() - 1;
        if next == 0 || next > last {
            out.copy_from_slice(self.key(if next == 0 { 0 } else { last }, value));
        } else {
            let k = next - 1;
            let dt = self.times[k + 1] - self.times[k];
            let s = if dt > 0.0 { ((t - self.times[k]) / dt).clamp(0.0, 1.0) } else { 0.0 };
            match self.interp {
                Interp::Step => out.copy_from_slice(self.key(k, 0)),
                Interp::Linear if rotation && self.width == 4 => slerp(self.key(k, 0), self.key(k + 1, 0), s, out),
                Interp::Linear => {
                    let (a, b) = (self.key(k, 0), self.key(k + 1, 0));
                    for (i, o) in out.iter_mut().enumerate() {
                        *o = a[i] + (b[i] - a[i]) * s;
                    }
                }
                Interp::Cubic => {
                    let (s2, s3) = (s * s, s * s * s);
                    let (h00, h10, h01, h11) = (2.0 * s3 - 3.0 * s2 + 1.0, s3 - 2.0 * s2 + s, -2.0 * s3 + 3.0 * s2, s3 - s2);
                    let (v0, out0, in1, v1) = (self.key(k, 1), self.key(k, 2), self.key(k + 1, 0), self.key(k + 1, 1));
                    for (i, o) in out.iter_mut().enumerate() {
                        *o = h00 * v0[i] + h10 * dt * out0[i] + h01 * v1[i] + h11 * dt * in1[i];
                    }
                }
            }
        }
        if rotation && out.len() == 4 {
            let l = out.iter().map(|v| v * v).sum::<f32>().sqrt();
            if l > 1e-12 {
                out.iter_mut().for_each(|v| *v /= l);
            }
        }
    }
}

/// Between unit quaternions `a` and `b`, `s` of the way, along the shorter arc.
fn slerp(a: &[f32], b: &[f32], s: f32, out: &mut [f32]) {
    let mut d: f32 = (0..4).map(|i| a[i] * b[i]).sum();
    let sign = if d < 0.0 {
        d = -d;
        -1.0
    } else {
        1.0
    };
    // Nearly the same: a straight line is as good, and doesn't divide by ~0.
    let (ka, kb) = if d > 0.9995 {
        (1.0 - s, s)
    } else {
        let angle = d.min(1.0).acos();
        let sin = angle.sin();
        (((1.0 - s) * angle).sin() / sin, (s * angle).sin() / sin)
    };
    for i in 0..4 {
        out[i] = ka * a[i] + kb * sign * b[i];
    }
}

/// What an animation channel moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Path {
    Translation,
    Rotation,
    Scale,
    Weights,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Channel {
    pub node: usize,
    pub path: Path,
    pub track: Track,
}

/// One of a glTF's animations.
#[derive(Debug, Clone, PartialEq)]
pub struct Clip {
    pub channels: Vec<Channel>,
    /// Where it loops: its last key.
    pub duration: f32,
}

impl Clip {
    pub fn new(channels: Vec<Channel>) -> Clip {
        let duration = channels.iter().map(|c| c.track.end()).fold(0.0, f32::max);
        Clip { channels, duration }
    }

    /// Moves anything: one with no channels leaves the model at rest.
    pub fn moves(&self) -> bool {
        !self.channels.is_empty()
    }
}

/// A node of the glTF's tree, at rest.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub parent: Option<usize>,
    pub translation: [f32; 3],
    /// A unit quaternion: x, y, z, w.
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
    /// The file's own matrix, kept while no animation moves the node: a
    /// matrix can shear, which translation, rotation and scale can't say.
    pub matrix: Option<Mat4>,
    /// Its mesh's morph target weights at rest.
    pub weights: Vec<f32>,
}

impl Default for Node {
    fn default() -> Self {
        Node { parent: None, translation: [0.0; 3], rotation: [0.0, 0.0, 0.0, 1.0], scale: [1.0; 3], matrix: None, weights: Vec::new() }
    }
}

/// A skin: the nodes that are its joints, and for each the inverse of where
/// it was when the mesh was bound to it.
#[derive(Debug, Clone, PartialEq)]
pub struct Skin {
    pub joints: Vec<usize>,
    pub inverse_bind: Vec<Mat4>,
}

/// How a morph target moves each vertex at full weight; a list it leaves
/// out is empty.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Morph {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub tangents: Vec<[f32; 3]>,
}

/// A part's vertices as the file has them, before its node, its skin and
/// its morph targets move them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Rest {
    pub node: usize,
    pub skin: Option<usize>,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    /// Empty unless the part has a normal map.
    pub tangents: Vec<[f32; 4]>,
    /// Up to four of the skin's joints for each vertex, and how much each
    /// moves it.
    pub joints: Vec<[u16; 4]>,
    pub weights: Vec<[f32; 4]>,
    pub morphs: Vec<Morph>,
    /// Where its node's instances (EXT_mesh_gpu_instancing) put copies of
    /// it, each in the mesh's own space, before the node (or the skin's
    /// joints) places it; the part's vertices are a copy of these for
    /// each, one after another. Empty: drawn once, as its node has it.
    pub instances: Vec<Mat4>,
}

/// What moves a glTF's parts: its nodes, skins and animations, and each
/// part's rest (in the order of the mesh's parts).
#[derive(Debug, Clone, PartialEq)]
pub struct Rig {
    pub nodes: Vec<Node>,
    /// The nodes, each after its parent.
    order: Vec<usize>,
    pub skins: Vec<Skin>,
    pub clips: Vec<Clip>,
    pub parts: Vec<Rest>,
    /// Into the mesh's unit box: less the center, over the longest side.
    pub frame: ([f32; 3], f32),
    /// The turn the file's camera sees the world with, put before every
    /// node that has no parent, so the whole scene, joints and all, faces
    /// Flow's view as it faces the camera; None without one, or where an
    /// animation moves the camera (`camera`).
    pub view: Option<Mat4>,
    /// The file's camera's node, where an animation moves it: the view is
    /// then worked out afresh from each pose ([`seen_from`]), so the scene
    /// turns and slides as the camera does.
    pub camera: Option<usize>,
    /// For each clip, whether it moves any of the mesh's lights (their
    /// nodes, ones they hang from, or the camera), which are then posed
    /// with it ([`Rig::place_lights`]).
    pub moves_lights: Vec<bool>,
}

impl Rig {
    pub fn new(nodes: Vec<Node>, skins: Vec<Skin>, clips: Vec<Clip>) -> Rig {
        let order = parents_first(&nodes);
        Rig { nodes, order, skins, clips, parts: Vec::new(), frame: ([0.0; 3], 1.0), view: None, camera: None, moves_lights: Vec::new() }
    }

    /// Which nodes clip `clip` moves: those its channels move, turn or
    /// scale, and all below them; every one, where it moves the camera the
    /// scene is seen from.
    pub fn moved(&self, clip: usize) -> Vec<bool> {
        let mut moved = vec![false; self.nodes.len()];
        for ch in self.clips.get(clip).into_iter().flat_map(|c| &c.channels).filter(|ch| ch.path != Path::Weights) {
            if let Some(m) = moved.get_mut(ch.node) {
                *m = true;
            }
        }
        for &i in &self.order {
            if let Some(p) = self.nodes[i].parent.filter(|&p| p < moved.len() && p != i) {
                moved[i] |= moved[p];
            }
        }
        if self.camera.is_some_and(|c| moved.get(c).copied().unwrap_or(false)) {
            moved.fill(true);
        }
        moved
    }

    /// Each node's matrix in the world, and its morph weights, `t` seconds
    /// into clip `clip` (looped), or at rest.
    pub fn pose(&self, clip: Option<usize>, t: f32) -> (Vec<Mat4>, Vec<Vec<f32>>) {
        let mut trs: Vec<([f32; 3], [f32; 4], [f32; 3])> = self.nodes.iter().map(|n| (n.translation, n.rotation, n.scale)).collect();
        let mut moved = vec![false; self.nodes.len()];
        let mut weights: Vec<Vec<f32>> = self.nodes.iter().map(|n| n.weights.clone()).collect();
        if let Some(clip) = clip.and_then(|i| self.clips.get(i)) {
            // Looped, though its very end shows as the end, not the start.
            let t = if clip.duration > 0.0 && t > clip.duration { t.rem_euclid(clip.duration) } else { t.clamp(0.0, clip.duration.max(0.0)) };
            for ch in &clip.channels {
                let Some(node) = trs.get_mut(ch.node) else { continue };
                moved[ch.node] = true;
                match ch.path {
                    Path::Translation => ch.track.sample(t, false, &mut node.0),
                    Path::Rotation => ch.track.sample(t, true, &mut node.1),
                    Path::Scale => ch.track.sample(t, false, &mut node.2),
                    Path::Weights => {
                        let w = &mut weights[ch.node];
                        w.resize(w.len().max(ch.track.width), 0.0);
                        ch.track.sample(t, false, &mut w[..ch.track.width]);
                    }
                }
            }
        }
        let mut world = vec![IDENTITY; self.nodes.len()];
        for &i in &self.order {
            let node = &self.nodes[i];
            let local = match node.matrix {
                Some(m) if !moved[i] => m,
                _ => compose(trs[i].0, trs[i].1, trs[i].2),
            };
            world[i] = match (node.parent, self.view) {
                (Some(p), _) if p < world.len() && p != i => mul(world[p], local),
                (_, Some(view)) => mul(view, local),
                (_, None) => local,
            };
        }
        // A moving camera's view, from where it is now, before everything.
        if let Some(c) = self.camera.filter(|&c| c < world.len()) {
            let view = seen_from(world[c]);
            world.iter_mut().for_each(|m| *m = mul(view, *m));
        }
        (world, weights)
    }

    /// Every part's vertices `t` seconds into clip `clip` (or at rest), in
    /// the mesh's box, with the uvs of `parts`.
    pub fn pose_parts(&self, clip: Option<usize>, t: f32, parts: &[Part]) -> Vec<Vec<Vertex>> {
        let (world, weights) = self.pose(clip, t);
        self.pose_parts_at(&world, &weights, parts)
    }

    /// Every part's vertices with the nodes where `world` has them and the
    /// morph weights `weights` (as [`Rig::pose`] gives them).
    pub fn pose_parts_at(&self, world: &[Mat4], weights: &[Vec<f32>], parts: &[Part]) -> Vec<Vec<Vertex>> {
        let joints: Vec<Vec<Mat4>> = self
            .skins
            .iter()
            .map(|s| s.joints.iter().zip(&s.inverse_bind).map(|(&j, &inverse)| mul(world.get(j).copied().unwrap_or(IDENTITY), inverse)).collect())
            .collect();
        self.parts.iter().zip(parts).map(|(rest, part)| self.pose_part(rest, &part.vertices, world, &joints, weights)).collect()
    }

    /// The mesh's `lights` (settled in its box at rest) where the nodes
    /// they hang from are in `world`: placed and aimed as the file's own
    /// lights are read (`glb::read_light`), in the mesh's box. What they
    /// give is left as settled at rest, so a light that comes nearer
    /// brightens as the file's intensities would.
    pub fn place_lights(&self, world: &[Mat4], lights: &[Light]) -> Vec<Light> {
        let (center, longest) = self.frame;
        lights
            .iter()
            .map(|l| match l.node.and_then(|n| world.get(n)) {
                Some(&m) => Light {
                    at: l.at.map(|_| sub([m[3][0], m[3][1], m[3][2]], center).map(|v| v / longest)),
                    aim: normalize(transform(m, [0.0, 0.0, -1.0], 0.0)),
                    ..*l
                },
                None => *l,
            })
            .collect()
    }

    /// A part's vertices posed, each copy its node's instances make placed
    /// by its own instance, and all a vertex carries besides (its uvs, its
    /// color) kept from `from`.
    fn pose_part(&self, rest: &Rest, from: &[Vertex], world: &[Mat4], joints: &[Vec<Mat4>], weights: &[Vec<f32>]) -> Vec<Vertex> {
        let node = world.get(rest.node).copied().unwrap_or(IDENTITY);
        let skin = rest.skin.and_then(|s| joints.get(s));
        let morphs: Vec<(&Morph, f32)> =
            weights.get(rest.node).map(|w| w.iter().zip(&rest.morphs).filter(|(k, _)| **k != 0.0).map(|(k, m)| (m, *k)).collect()).unwrap_or_default();
        let (center, longest) = self.frame;
        let shrink = 1.0 / longest.max(1e-12);
        let count = rest.positions.len();
        let copies = rest.instances.len().max(1);
        let mut out = Vec::with_capacity(from.len().min(count * copies));
        for copy in 0..copies {
            // An instance is placed in the mesh's own space, so the node (or
            // the skin) places it after; a part without any, as it is.
            let instance = rest.instances.get(copy).copied();
            let placed = instance.map_or(node, |at| mul(node, at));
            let placed_normals = normal_matrix(&placed);
            for i in 0..count {
                let Some(was) = from.get(copy * count + i) else { return out };
                let (mut p, mut n) = (rest.positions[i], rest.normals.get(i).copied().unwrap_or([0.0, 0.0, 1.0]));
                let mut tangent = rest.tangents.get(i).copied().unwrap_or([0.0; 4]);
                for &(m, k) in &morphs {
                    p = add_scaled(p, m.positions.get(i), k);
                    n = add_scaled(n, m.normals.get(i), k);
                    let t = add_scaled([tangent[0], tangent[1], tangent[2]], m.tangents.get(i), k);
                    tangent = [t[0], t[1], t[2], tangent[3]];
                }
                let skinned = skin.zip(rest.joints.get(i)).zip(rest.weights.get(i)).and_then(|((js, &j), &w)| skin_matrix(js, j, w));
                let (m, turn) = match skinned {
                    Some(m) => {
                        let m = instance.map_or(m, |at| mul(m, at));
                        (m, normal_matrix(&m))
                    }
                    None => (placed, placed_normals),
                };
                let pos = sub(transform(m, p, 1.0), center).map(|v| v * shrink);
                let t = normalize(transform(m, [tangent[0], tangent[1], tangent[2]], 0.0));
                let tangent = if tangent[3] == 0.0 { [0.0; 4] } else { [t[0], t[1], t[2], tangent[3]] };
                out.push(Vertex { pos, normal: normalize(apply3(turn, n)), tangent, ..*was });
            }
        }
        out
    }

    /// Calls `f` with every vertex's place over each moving clip, sampled
    /// often enough to find the room it moves in, not so often that a big
    /// model takes long to load.
    pub fn sweep(&self, parts: &[Part], mut f: impl FnMut([f32; 3])) {
        const BUDGET: usize = 2_000_000;
        let vertices: usize = parts.iter().map(|p| p.vertices.len()).sum();
        let moving: Vec<usize> = (0..self.clips.len()).filter(|&i| self.clips[i].moves()).collect();
        let samples = (BUDGET / (vertices * moving.len()).max(1)).clamp(2, 24);
        for &c in &moving {
            for s in 0..samples {
                let t = self.clips[c].duration * s as f32 / (samples - 1) as f32;
                for part in self.pose_parts(Some(c), t, parts) {
                    part.iter().for_each(|v| f(v.pos));
                }
            }
        }
    }
}

/// The nodes in an order where each comes after its parent; one in a loop
/// (which a valid file hasn't) never comes, and stays where it is.
fn parents_first(nodes: &[Node]) -> Vec<usize> {
    let n = nodes.len();
    let root = |i: usize| nodes[i].parent.is_none_or(|p| p >= n || p == i);
    let mut children = vec![Vec::new(); n];
    for i in (0..n).filter(|&i| !root(i)) {
        children[nodes[i].parent.unwrap_or(0)].push(i);
    }
    let mut order = Vec::with_capacity(n);
    let mut stack: Vec<usize> = (0..n).filter(|&i| root(i)).rev().collect();
    while let Some(i) = stack.pop() {
        order.push(i);
        stack.extend(children[i].iter().rev());
    }
    order
}

/// The turn that undoes a camera's (`camera` its node's matrix in the
/// world), so what it looks at faces Flow's view: its -z away from the
/// viewer, its y up. Squared up first, as a node's scale or skew would
/// otherwise squash the scene.
pub fn facing(camera: Mat4) -> Mat4 {
    let x = normalize([camera[0][0], camera[0][1], camera[0][2]]);
    let z = normalize([camera[2][0], camera[2][1], camera[2][2]]);
    let y = normalize(cross(z, x));
    let x = cross(y, z);
    // Its axes as the rows, column by column.
    [[x[0], y[0], z[0], 0.0], [x[1], y[1], z[1], 0.0], [x[2], y[2], z[2], 0.0], [0.0, 0.0, 0.0, 1.0]]
}

/// The view from a camera that moves: [`facing`]'s turn, and the camera's
/// place taken away, so what it looks at stays where it sees it as it
/// goes round (and the scene slides as it slides). A still camera's place
/// would only move the whole scene, which the mesh's box takes away, so it
/// keeps `facing` alone.
pub fn seen_from(camera: Mat4) -> Mat4 {
    let mut view = facing(camera);
    let t = transform(view, [camera[3][0], camera[3][1], camera[3][2]], 0.0);
    view[3] = [-t[0], -t[1], -t[2], 1.0];
    view
}

/// `p` moved by `delta` (if the target has one) at weight `k`.
fn add_scaled(p: [f32; 3], delta: Option<&[f32; 3]>, k: f32) -> [f32; 3] {
    match delta {
        Some(d) => [p[0] + d[0] * k, p[1] + d[1] * k, p[2] + d[2] * k],
        None => p,
    }
}

/// A translation, a rotation (unit quaternion) and a scale as one matrix,
/// applied scale first.
pub fn compose(t: [f32; 3], q: [f32; 4], s: [f32; 3]) -> Mat4 {
    let [x, y, z, w] = q;
    [
        [(1.0 - 2.0 * (y * y + z * z)) * s[0], 2.0 * (x * y + z * w) * s[0], 2.0 * (x * z - y * w) * s[0], 0.0],
        [2.0 * (x * y - z * w) * s[1], (1.0 - 2.0 * (x * x + z * z)) * s[1], 2.0 * (y * z + x * w) * s[1], 0.0],
        [2.0 * (x * z + y * w) * s[2], 2.0 * (y * z - x * w) * s[2], (1.0 - 2.0 * (x * x + y * y)) * s[2], 0.0],
        [t[0], t[1], t[2], 1.0],
    ]
}

/// The matrix a skinned vertex moves by: its joints' matrices (each the
/// joint's place in the world times its inverse bind matrix) blended by
/// their weights, made to add up to one as some exporters don't. None
/// where it names no joint with any weight.
pub fn skin_matrix(joints: &[Mat4], j: [u16; 4], w: [f32; 4]) -> Option<Mat4> {
    let used = |k: usize| (j[k] as usize) < joints.len() && w[k] > 0.0;
    let total: f32 = (0..4).filter(|&k| used(k)).map(|k| w[k]).sum();
    if total <= 1e-6 {
        return None;
    }
    let mut m = [[0f32; 4]; 4];
    for k in (0..4).filter(|&k| used(k)) {
        let (jm, wk) = (&joints[j[k] as usize], w[k] / total);
        for (col, jcol) in m.iter_mut().zip(jm) {
            for (out, v) in col.iter_mut().zip(jcol) {
                *out += wk * v;
            }
        }
    }
    Some(m)
}

/// What turns normals as `m` turns points: its 3 × 3's inverse transpose,
/// up to a positive scale (columns).
pub fn normal_matrix(m: &Mat4) -> [[f32; 3]; 3] {
    let (a, b, c) = ([m[0][0], m[0][1], m[0][2]], [m[1][0], m[1][1], m[1][2]], [m[2][0], m[2][1], m[2][2]]);
    let cof = [cross(b, c), cross(c, a), cross(a, b)];
    // The cofactors carry the determinant's sign, which would turn a
    // mirrored part's normals inside out.
    if dot(a, cof[0]) < 0.0 {
        cof.map(|col| col.map(|v| -v))
    } else {
        cof
    }
}

/// `v` through a 3 × 3 matrix of columns.
pub fn apply3(m: [[f32; 3]; 3], v: [f32; 3]) -> [f32; 3] {
    [0, 1, 2].map(|r| m[0][r] * v[0] + m[1][r] * v[1] + m[2][r] * v[2])
}

#[cfg(test)]
mod tests {
    use super::super::{length, Material};
    use super::*;

    fn close(a: &[f32], b: &[f32]) -> bool {
        a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-4)
    }

    fn at(track: &Track, t: f32, rotation: bool) -> Vec<f32> {
        let mut out = vec![0.0; track.width];
        track.sample(t, rotation, &mut out);
        out
    }

    #[test]
    fn a_step_track_holds_each_key_until_the_next() {
        let track = Track::new(vec![0.0, 1.0, 2.0], vec![0.0, 10.0, 20.0], Interp::Step).unwrap();
        assert_eq!(track.width, 1);
        for (t, v) in [(-1.0, 0.0), (0.0, 0.0), (0.99, 0.0), (1.0, 10.0), (1.5, 10.0), (2.0, 20.0), (9.0, 20.0)] {
            assert_eq!(at(&track, t, false), [v], "{t}s");
        }
    }

    #[test]
    fn a_linear_track_goes_straight_and_a_rotation_goes_the_short_way_round() {
        let track = Track::new(vec![1.0, 3.0], vec![0.0, 2.0, 4.0, 4.0, 6.0, 8.0], Interp::Linear).unwrap();
        assert_eq!(track.width, 3);
        assert!(close(&at(&track, 2.0, false), &[2.0, 4.0, 6.0]));
        assert!(close(&at(&track, 0.0, false), &[0.0, 2.0, 4.0]), "before the first key, its value");
        // A quarter turn about z, halfway: an eighth of a turn.
        let h = std::f32::consts::FRAC_1_SQRT_2;
        let turn = Track::new(vec![0.0, 1.0], vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, h, h], Interp::Linear).unwrap();
        let (s, c) = (std::f32::consts::FRAC_PI_8.sin(), std::f32::consts::FRAC_PI_8.cos());
        assert!(close(&at(&turn, 0.5, true), &[0.0, 0.0, s, c]));
        // The same end as its negative (the same rotation): still the short way.
        let flipped = Track::new(vec![0.0, 1.0], vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, -h, -h], Interp::Linear).unwrap();
        assert!(close(&at(&flipped, 0.5, true), &[0.0, 0.0, s, c]));
    }

    #[test]
    fn a_cubic_spline_passes_through_its_keys_along_their_tangents() {
        // Keys at 0 and 2 seconds, values 0 and 1, slopes 0 at both ends:
        // smoothstep, scaled to two seconds.
        let track = Track::new(vec![0.0, 2.0], vec![9.0, 0.0, 0.0, 0.0, 1.0, 9.0], Interp::Cubic).unwrap();
        assert_eq!(track.width, 1);
        assert!(close(&at(&track, 0.0, false), &[0.0]));
        assert!(close(&at(&track, 2.0, false), &[1.0]));
        assert!(close(&at(&track, 1.0, false), &[0.5]));
        assert!(close(&at(&track, 0.5, false), &[0.15625]), "3s² − 2s³ at a quarter");
        // A slope of 1 a second at both ends of a line from 0 to 2 over 2 s:
        // the line itself, as Hermite reproduces straight lines.
        let line = Track::new(vec![0.0, 2.0], vec![1.0, 0.0, 1.0, 1.0, 2.0, 1.0], Interp::Cubic).unwrap();
        for t in [0.3, 0.8, 1.7] {
            assert!(close(&at(&line, t, false), &[t]), "{t}s");
        }
        assert!(Track::new(vec![0.0, 1.0], vec![1.0, 2.0, 3.0, 4.0], Interp::Cubic).is_none(), "not whole keys");
        assert!(Track::new(vec![1.0, 0.0], vec![1.0, 2.0], Interp::Linear).is_none(), "time running backwards");
    }

    #[test]
    fn translation_rotation_and_scale_compose_scale_first() {
        let h = std::f32::consts::FRAC_1_SQRT_2;
        // Scaled by 2, turned a quarter about z, moved by (10, 0, 0).
        let m = compose([10.0, 0.0, 0.0], [0.0, 0.0, h, h], [2.0, 2.0, 2.0]);
        assert!(close(&transform(m, [1.0, 0.0, 0.0], 1.0), &[10.0, 2.0, 0.0]));
        assert!(close(&transform(m, [0.0, 1.0, 0.0], 1.0), &[8.0, 0.0, 0.0]));
    }

    #[test]
    fn a_skinned_vertex_moves_by_its_joints_blended_by_weight() {
        let still = IDENTITY;
        let up = compose([0.0, 4.0, 0.0], [0.0, 0.0, 0.0, 1.0], [1.0; 3]);
        // Weights that don't add up to one are made to.
        let m = skin_matrix(&[still, up], [0, 1, 0, 0], [1.0, 3.0, 0.0, 0.0]).unwrap();
        assert!(close(&transform(m, [1.0, 1.0, 1.0], 1.0), &[1.0, 4.0, 1.0]));
        assert!(skin_matrix(&[still], [5, 6, 0, 0], [1.0, 1.0, 0.0, 0.0]).is_none(), "naming joints that aren't there");
        // Mirrored, normals still point out.
        let mirror = [[-1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];
        assert!(close(&apply3(normal_matrix(&mirror), [0.0, 0.0, 1.0]), &[0.0, 0.0, 1.0]));
        assert!(close(&apply3(normal_matrix(&mirror), [1.0, 0.0, 0.0]), &[-1.0, 0.0, 0.0]));
        // Squashed in y, a slope's normal stands up more, not less.
        let squash = [[1.0, 0.0, 0.0, 0.0], [0.0, 0.5, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];
        let n = normalize(apply3(normal_matrix(&squash), normalize([1.0, 1.0, 0.0])));
        assert!(n[1] > n[0], "{n:?}");
    }

    /// A part of one vertex at (1, 0, 0) on node 1, below node 0.
    fn one_vertex_rig(skin: bool) -> (Rig, Vec<Part>) {
        let nodes = vec![
            Node { translation: [0.0, 0.0, 5.0], ..Node::default() },
            Node { parent: Some(0), weights: vec![0.0, 0.5], ..Node::default() },
            Node { translation: [0.0, 3.0, 0.0], ..Node::default() },
        ];
        let skins = vec![Skin { joints: vec![2], inverse_bind: vec![IDENTITY] }];
        let h = std::f32::consts::FRAC_1_SQRT_2;
        // Over a second node 1 turns a quarter about z, and its morph
        // targets' weights go from (0, 0.5) to (1, 0).
        let clip = Clip::new(vec![
            Channel { node: 1, path: Path::Rotation, track: Track::new(vec![0.0, 1.0], vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, h, h], Interp::Linear).unwrap() },
            Channel { node: 1, path: Path::Weights, track: Track::new(vec![0.0, 1.0], vec![0.0, 0.5, 1.0, 0.0], Interp::Linear).unwrap() },
        ]);
        let mut rig = Rig::new(nodes, skins, vec![clip]);
        rig.parts.push(Rest {
            node: 1,
            skin: skin.then_some(0),
            positions: vec![[1.0, 0.0, 0.0]],
            normals: vec![[0.0, 0.0, 1.0]],
            joints: vec![[0; 4]],
            weights: vec![[1.0, 0.0, 0.0, 0.0]],
            morphs: vec![
                Morph { positions: vec![[0.0, 0.0, 2.0]], ..Morph::default() },
                Morph { positions: vec![[1.0, 0.0, 0.0]], normals: vec![[1.0, 0.0, 0.0]], ..Morph::default() },
            ],
            ..Rest::default()
        });
        let part = Part { vertices: vec![Vertex::new([1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.25, 0.75])], indices: vec![0, 0, 0], material: Material::default(), node: Some(1) };
        (rig, vec![part])
    }

    #[test]
    fn morph_targets_blend_by_their_weights_and_nodes_carry_their_children() {
        let (rig, parts) = one_vertex_rig(false);
        // At rest: weights (0, 0.5) move it half of (1, 0, 0) along x, and
        // its parent lifts it 5 in z.
        let v = rig.pose_parts(None, 0.0, &parts)[0][0];
        assert!(close(&v.pos, &[1.5, 0.0, 5.0]), "{:?}", v.pos);
        assert!(close(&v.normal, &normalize([0.5, 0.0, 1.0])));
        assert_eq!(v.uv, [0.25, 0.75], "the uv is kept");
        // A second in, it's turned a quarter, with weights (1, 0).
        let v = rig.pose_parts(Some(0), 1.0, &parts)[0][0];
        assert!(close(&v.pos, &[0.0, 1.0, 7.0]), "{:?}", v.pos);
        // Halfway: weights (0.5, 0.25), turned an eighth.
        let v = rig.pose_parts(Some(0), 0.5, &parts)[0][0];
        let r = 1.25 * std::f32::consts::FRAC_1_SQRT_2;
        assert!(close(&v.pos, &[r, r, 6.0]), "{:?}", v.pos);
        // Looped: two and a half seconds in is half a second in.
        assert!(close(&rig.pose_parts(Some(0), 2.5, &parts)[0][0].pos, &v.pos));
    }

    #[test]
    fn a_skinned_part_follows_its_joints_not_its_node() {
        let (mut rig, parts) = one_vertex_rig(true);
        // Joint 2 is 3 up; the part's own node (turning, under node 0) is
        // left out, as glTF says. A second in, the morph weights are (1, 0).
        let v = rig.pose_parts(Some(0), 1.0, &parts)[0][0];
        assert!(close(&v.pos, &[1.0, 3.0, 2.0]), "{:?}", v.pos);
        // In the mesh's box: less its center, over its longest side.
        rig.frame = ([1.0, 1.0, 0.0], 2.0);
        let v = rig.pose_parts(Some(0), 1.0, &parts)[0][0];
        assert!(close(&v.pos, &[0.0, 1.0, 1.0]));
        assert!((length(v.normal) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn instances_are_placed_after_the_morph_targets_and_before_the_node_or_the_skin() {
        for skin in [false, true] {
            let (mut rig, mut parts) = one_vertex_rig(skin);
            // As it is, and twice the size moved 1 along z; each copy with
            // its own uv and color, which posing keeps.
            rig.parts[0].instances = vec![IDENTITY, compose([0.0, 0.0, 1.0], [0.0, 0.0, 0.0, 1.0], [2.0; 3])];
            let first = parts[0].vertices[0];
            parts[0].vertices.push(Vertex { uv: [0.5, 0.5], color: [255, 0, 0, 128], ..first });
            let (rest, later) = (rig.pose_parts(None, 0.0, &parts), rig.pose_parts(Some(0), 1.0, &parts));
            assert_eq!((rest[0].len(), later[0][1].uv, later[0][1].color), (2, [0.5, 0.5], [255, 0, 0, 128]));
            if skin {
                // A second in, morphed to (1, 0, 2), doubled and moved to
                // (2, 0, 5), then 3 up with its joint.
                assert!(close(&later[0][0].pos, &[1.0, 3.0, 2.0]) && close(&later[0][1].pos, &[2.0, 3.0, 5.0]), "{:?}", later[0]);
            } else {
                // At rest, morphed to (1.5, 0, 0), doubled and moved to (3,
                // 0, 1), then 5 along z with its node's parent.
                assert!(close(&rest[0][0].pos, &[1.5, 0.0, 5.0]) && close(&rest[0][1].pos, &[3.0, 0.0, 6.0]), "{:?}", rest[0]);
                assert!(close(&rest[0][1].normal, &rest[0][0].normal), "a scale the same every way leaves the normals");
            }
        }
    }

    #[test]
    fn a_clip_moves_the_nodes_it_turns_and_all_below_them_and_everything_if_it_moves_the_camera() {
        let (mut rig, _) = one_vertex_rig(false);
        // Node 1 is turned (its weights don't move it); node 2 is on its own.
        assert_eq!(rig.moved(0), [false, true, false]);
        rig.nodes[2].parent = Some(1);
        rig.order = parents_first(&rig.nodes);
        assert_eq!(rig.moved(0), [false, true, true], "carried by its parent");
        rig.nodes[2].parent = None;
        rig.order = parents_first(&rig.nodes);
        rig.camera = Some(1);
        assert_eq!(rig.moved(0), [true; 3], "seen from a camera it moves, the whole scene moves");
        assert_eq!(rig.moved(5), [false; 3], "a clip it doesn't have moves nothing");
    }

    #[test]
    fn a_moving_cameras_view_takes_its_place_away_as_well_as_its_turn() {
        let h = std::f32::consts::FRAC_1_SQRT_2;
        // Five units out along x, looking back along -x.
        let camera = compose([5.0, 0.0, 0.0], [0.0, h, 0.0, h], [1.0; 3]);
        let view = seen_from(camera);
        assert!(close(&transform(view, [0.0, 0.0, 0.0], 1.0), &[0.0, 0.0, -5.0]), "what it looks at, ahead of it");
        assert!(close(&transform(view, [0.0, 0.0, 1.0], 1.0), &[-1.0, 0.0, -5.0]), "+z to its left");
        assert!(close(&transform(facing(camera), [5.0, 0.0, 0.0], 1.0), &[0.0, 0.0, 5.0]), "a still camera's view turns alone");
    }

    #[test]
    fn a_node_in_a_loop_stays_put_rather_than_hang() {
        let nodes = vec![Node { parent: Some(1), ..Node::default() }, Node { parent: Some(0), ..Node::default() }, Node::default()];
        assert_eq!(parents_first(&nodes), [2]);
        let rig = Rig::new(nodes, Vec::new(), Vec::new());
        assert_eq!(rig.pose(None, 0.0).0.len(), 3);
    }
}
