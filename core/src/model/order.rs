//! The order a mesh's see-through triangles are drawn in. They blend over
//! what's drawn already and leave the depth as it is, so each must come
//! after everything it covers: farthest first, triangle by triangle rather
//! than part by part, so parts that reach into each other (a glass ball in
//! a glass box) blend right, and a closed see-through part draws its far
//! side before its near one.
//!
//! No order of whole triangles is right for two that cross, each half in
//! front of the other. So a mesh's see-through triangles are cut where they
//! cross as it's made ([`SeeThrough::new`]): each is cut along the plane of
//! every other it passes through, and no two of the pieces cross.
//!
//! A mesh that moves by itself is posed each frame, and where its triangles
//! cross moves with it. One with few see-through triangles (at most
//! [`EACH_FRAME`]) has them cut afresh as each frame poses them, within a
//! small budget of work ([`SeeThrough::pose`]). A bigger one, which that
//! would slow, keeps the cuts made at rest: each piece's new corners are
//! posed with the two corners they were cut between, so the pieces still
//! cover the posed triangles exactly, and they're right wherever the
//! triangles that crossed at rest move together (a glass on a turning
//! machine, a visor on a head). Where they move apart, or come to cross
//! only as they move, two can be drawn half right; so can those of a frame
//! whose cutting would pass the budget, which keeps the cuts made at rest.
//!
//! Each frame that draws the mesh sorts its see-through triangles by the
//! depth of their middles with a radix sort, whose time grows only as the
//! triangles do, and draws them in runs of one part each; the solid parts
//! are drawn whole, first, as they come.

use std::borrow::Cow;
use std::collections::HashMap;

use super::{cross, dot, normalize, sub, Mesh, Part, Pose, Vertex};

/// What cutting a mesh's see-through triangles may take at most: how many
/// triangles there are; for finding where they cross, the grid's cells,
/// the triangles' entries in them, the pairs looked at, and those whose
/// boxes meet, so are tested; and the pieces cut along a plane. Past any of
/// them, the triangles stay whole.
struct Limits {
    triangles: usize,
    cells: usize,
    entries: usize,
    pairs: usize,
    tests: usize,
    splits: usize,
}

/// As a mesh is made: finding the crossings among more would hold the
/// model up as it loads. Its cutting stops at a budget of pieces instead.
const AT_REST: Limits = Limits { triangles: 200_000, cells: 1 << 21, entries: 1 << 23, pairs: 1 << 25, tests: usize::MAX, splits: usize::MAX };
/// See-through triangles at most that a moving mesh may have for them to
/// be cut afresh each frame; one with more keeps the cuts made at rest.
pub const EACH_FRAME: usize = 2048;
/// For each frame, as timed on a Mac: a pair looked at and passed over
/// takes some 5 ns, one tested some 25, and a piece cut some 15, so a
/// frame's cutting takes a millisecond or two at the very most, and a mesh
/// at the limit still animates smoothly. A mesh's own surface fills the
/// cells it lies in, so most of these go on neighbours that touch: two
/// see-through squares of 1,800 triangles crossing look at under 2^16
/// pairs and test under 2^14, and take some 0.6 ms in all.
const PER_FRAME: Limits = Limits { triangles: EACH_FRAME, cells: 1 << 21, entries: 1 << 16, pairs: 1 << 17, tests: 1 << 15, splits: 1 << 15 };
/// Distances from a plane this small (the unit box's units) count as on it.
const EPS: f32 = 1e-5;
/// Runs of one part's triangles at most, each a draw of its own. Past it,
/// each part's triangles are drawn together, in their order, the parts by
/// where their triangles fall on average.
pub const MAX_RUNS: usize = 256;

/// A mesh's see-through triangles as they're drawn.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct SeeThrough {
    /// Each triangle's part and corners, the corners counting on past the
    /// part's own vertices into its `added` ones.
    pub triangles: Vec<(u32, [u32; 3])>,
    /// Each part's vertices that cuts made, numbered after its own.
    pub added: Vec<Vec<Vertex>>,
    /// Where each of `added` lies: the two vertices it was cut between
    /// (its part's own, or ones added before it), and how far from the
    /// first, so it can be posed with them.
    between: Vec<Vec<(u32, u32, f32)>>,
    /// The parts that were see-through when it was made, whose triangles it
    /// holds.
    covers: Vec<bool>,
    /// Its mesh moves by itself, and has few enough see-through triangles
    /// to cut them afresh as each frame poses them.
    each_frame: bool,
}

/// How a mesh is drawn, back to front.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Order {
    /// The solid parts and the cut-outs, drawn whole first, as they come.
    pub solid: Vec<usize>,
    /// Then the see-through triangles' corners, three a triangle, farthest
    /// first, numbered as their part's vertices and then its `added` ones.
    pub indices: Vec<u32>,
    /// Each run of them: its part, and how many of `indices` it takes.
    pub runs: Vec<(usize, u32)>,
}

impl SeeThrough {
    /// The see-through parts' triangles, cut where they cross; `moving` if
    /// the mesh moves by itself, so is posed each frame ([`Self::pose`]).
    pub fn new(parts: &[Part], moving: bool) -> SeeThrough {
        let own: Vec<&[Vertex]> = parts.iter().map(|p| &p.vertices[..]).collect();
        let mut st = cut(parts, &own, &AT_REST).unwrap_or_else(|covers| uncut(parts, covers));
        st.each_frame = moving && st.covers.iter().zip(parts).filter(|(c, _)| **c).map(|(_, p)| p.indices.len() / 3).sum::<usize>() <= EACH_FRAME;
        st
    }

    /// Cuts a moving mesh's see-through triangles as `vertices` (each
    /// part's own, as its animation poses them) have them, adding the
    /// vertices the cuts make after each part's own. Where there are few
    /// enough triangles ([`EACH_FRAME`]) and the work stays within its
    /// budget, they're cut afresh and returned; otherwise the cuts made at
    /// rest are posed with the two vertices each lies between, and these
    /// triangles are drawn (None).
    pub fn pose(&self, parts: &[Part], vertices: &mut [Vec<Vertex>]) -> Option<SeeThrough> {
        if self.each_frame {
            let posed: Vec<&[Vertex]> = vertices.iter().map(|v| &v[..]).collect();
            if let Ok(mut st) = cut(parts, &posed, &PER_FRAME) {
                for (v, added) in vertices.iter_mut().zip(&mut st.added) {
                    v.append(added);
                }
                st.between.clear();
                return Some(st);
            }
        }
        for ((v, between), part) in vertices.iter_mut().zip(&self.between).zip(parts) {
            // Posed from other vertices than the cuts were made from: leave
            // them be rather than pose them wrong.
            if v.len() != part.vertices.len() {
                continue;
            }
            for &(a, b, t) in between {
                let (Some(a), Some(b)) = (v.get(a as usize), v.get(b as usize)) else { break };
                let at = part_way(a, b, t);
                v.push(at);
            }
        }
        None
    }
}

/// The see-through parts' triangles as they are, none cut.
fn uncut(parts: &[Part], covers: Vec<bool>) -> SeeThrough {
    let triangles = own_triangles(parts, |i| covers[i]).collect();
    SeeThrough { triangles, added: vec![Vec::new(); parts.len()], between: vec![Vec::new(); parts.len()], covers, each_frame: false }
}

/// The see-through parts' triangles over `vertices` (each part's own, as
/// they stand), each cut along the plane of every other it passes through,
/// within `limits`; past them, the parts that are see-through, whose
/// triangles would stay whole.
fn cut(parts: &[Part], vertices: &[&[Vertex]], limits: &Limits) -> Result<SeeThrough, Vec<bool>> {
    let covers: Vec<bool> = parts.iter().map(|p| p.material.see_through()).collect();
    let triangles: Vec<(u32, [u32; 3])> = own_triangles(parts, |i| covers[i]).collect();
    if triangles.len() > limits.triangles {
        return Err(covers);
    }
    let corner = |p: u32, i: u32| vertices.get(p as usize).and_then(|v| v.get(i as usize)).map_or([0.0; 3], |v| v.pos);
    let shapes: Vec<[[f32; 3]; 3]> = triangles.iter().map(|&(p, t)| t.map(|i| corner(p, i))).collect();
    let planes: Vec<Option<Plane>> = shapes.iter().map(plane).collect();
    let cuts = if triangles.len() < 2 { None } else { Some(crossings(&shapes, &planes, limits).ok_or_else(|| covers.clone())?) };
    let mut added = vec![Vec::new(); parts.len()];
    let mut between = vec![Vec::new(); parts.len()];
    let Some(cuts) = cuts.filter(|c| c.iter().any(|c| !c.is_empty())) else {
        return Ok(SeeThrough { triangles, added, between, covers, each_frame: false });
    };
    // Cut at most so many times over, which a mesh of crossings
    // everywhere might take: past it, the rest are left whole.
    let budget = triangles.len() * 2 + 4096;
    let mut out = Vec::with_capacity(triangles.len());
    let mut edges = HashMap::new();
    let mut splits = 0usize;
    for (k, &(p, t)) in triangles.iter().enumerate() {
        let mut pieces = vec![t];
        for &j in &cuts[k] {
            if out.len() + pieces.len() >= budget {
                break;
            }
            splits += pieces.len();
            if splits > limits.splits {
                return Err(covers);
            }
            let Some(by) = planes[j as usize] else { continue };
            let mut next = Vec::with_capacity(pieces.len() + 2);
            let own = vertices.get(p as usize).copied().unwrap_or(&[]);
            let mut cutter = Cutter { part: p, own, added: &mut added[p as usize], between: &mut between[p as usize], edges: &mut edges };
            for piece in pieces {
                cutter.split(piece, by, j, &mut next);
            }
            pieces = next;
        }
        out.extend(pieces.into_iter().map(|c| (p, c)));
    }
    Ok(SeeThrough { triangles: out, added, between, covers, each_frame: false })
}

/// The triangles of the parts `wanted` names, as they are.
fn own_triangles<'a>(parts: &'a [Part], wanted: impl Fn(usize) -> bool + 'a) -> impl Iterator<Item = (u32, [u32; 3])> + 'a {
    parts.iter().enumerate().filter(move |(i, _)| wanted(*i)).flat_map(|(i, p)| p.indices.as_chunks::<3>().0.iter().map(move |t| (i as u32, *t)))
}

/// How `mesh` is drawn once `model` has placed it (z toward the viewer):
/// see [`Order`]. `posed` is how an animation has the mesh now, the
/// vertices its cuts made included.
pub fn draw_order(mesh: &Mesh, posed: Option<&Pose>, model: [[f32; 4]; 4]) -> Order {
    let parts = &mesh.parts;
    let see = |i: usize| parts[i].material.see_through();
    let solid: Vec<usize> = (0..parts.len()).filter(|&i| !see(i)).collect();
    // A part made see-through since the mesh was (or no longer) is drawn
    // by its own triangles as they are.
    let st = posed.and_then(|p| p.see_through.as_ref()).unwrap_or(&mesh.see_through);
    let covered = |i: usize| st.covers.get(i).copied().unwrap_or(false);
    let triangles: Cow<[(u32, [u32; 3])]> = if (0..parts.len()).all(|i| see(i) == covered(i)) {
        Cow::Borrowed(&st.triangles)
    } else {
        Cow::Owned(st.triangles.iter().filter(|(p, _)| see(*p as usize)).copied().chain(own_triangles(parts, |i| see(i) && !covered(i))).collect())
    };
    if triangles.is_empty() {
        return Order { solid, ..Order::default() };
    }
    // Only how near each point comes counts, and moving the model nearer
    // moves every point alike, so its z row alone, without the shift.
    let toward = [model[0][2], model[1][2], model[2][2]];
    let depths: Vec<Vec<f32>> = (0..parts.len())
        .map(|i| {
            if !see(i) {
                return Vec::new();
            }
            // Posed, its cuts' vertices come on after its own.
            if let Some(q) = posed.and_then(|q| q.vertices.get(i)).filter(|q| q.len() >= parts[i].vertices.len()) {
                return q.iter().map(|v| dot(toward, v.pos)).collect();
            }
            let added = st.added.get(i).filter(|_| covered(i)).map_or(&[][..], Vec::as_slice);
            parts[i].vertices.iter().chain(added).map(|v| dot(toward, v.pos)).collect()
        })
        .collect();
    let mut keyed: Vec<(u32, u32)> = triangles
        .iter()
        .enumerate()
        .map(|(k, (p, t))| {
            let d = &depths[*p as usize];
            let z: f32 = t.iter().map(|&i| d.get(i as usize).copied().unwrap_or(0.0)).sum();
            (sortable(z), k as u32)
        })
        .collect();
    radix_sort(&mut keyed);
    let mut order: Vec<usize> = keyed.iter().map(|&(_, k)| k as usize).collect();
    let changes = order.windows(2).filter(|w| triangles[w[0]].0 != triangles[w[1]].0).count();
    if changes >= MAX_RUNS {
        let mut place = vec![(0f64, 0u32); parts.len()];
        for (rank, &k) in order.iter().enumerate() {
            let at = &mut place[triangles[k].0 as usize];
            *at = (at.0 + rank as f64, at.1 + 1);
        }
        let mean = |k: usize| {
            let (sum, n) = place[triangles[k].0 as usize];
            sum / f64::from(n.max(1))
        };
        order.sort_by(|&a, &b| mean(a).total_cmp(&mean(b)).then(triangles[a].0.cmp(&triangles[b].0)));
    }
    let mut indices = Vec::with_capacity(order.len() * 3);
    let mut runs: Vec<(usize, u32)> = Vec::new();
    for k in order {
        let (p, t) = triangles[k];
        indices.extend(t);
        match runs.last_mut() {
            Some((q, n)) if *q == p as usize => *n += 3,
            _ => runs.push((p as usize, 3)),
        }
    }
    Order { solid, indices, runs }
}

/// A float's bits turned so that they sort as the float does.
fn sortable(z: f32) -> u32 {
    let b = z.to_bits();
    if b >> 31 == 1 {
        !b
    } else {
        b | 1 << 31
    }
}

/// Sorts by the first of each pair, keeping the order of equals: a byte at
/// a time from the lowest, each pass counting the bytes and laying the
/// pairs out by them, so the time grows only as the pairs do.
fn radix_sort(items: &mut Vec<(u32, u32)>) {
    let mut scratch = vec![(0, 0); items.len()];
    for shift in [0, 8, 16, 24] {
        let mut starts = [0usize; 257];
        for &(k, _) in items.iter() {
            starts[((k >> shift) & 255) as usize + 1] += 1;
        }
        // Every pair has the same byte here: it orders nothing.
        if starts.contains(&items.len()) {
            continue;
        }
        for b in 0..256 {
            starts[b + 1] += starts[b];
        }
        for &item in items.iter() {
            let b = ((item.0 >> shift) & 255) as usize;
            scratch[starts[b]] = item;
            starts[b] += 1;
        }
        std::mem::swap(items, &mut scratch);
    }
}

/// A plane: its unit normal, and how far along it from the origin it lies.
type Plane = ([f32; 3], f32);

/// The plane a triangle lies in; None if it has no area.
fn plane(t: &[[f32; 3]; 3]) -> Option<Plane> {
    let n = cross(sub(t[1], t[0]), sub(t[2], t[0]));
    let l = dot(n, n).sqrt();
    (l > 1e-12).then(|| {
        let n = n.map(|v| v / l);
        (n, dot(n, t[0]))
    })
}

/// How far each corner of `t` lies from `plane`, on its normal's side
/// above 0.
fn sides(t: &[[f32; 3]; 3], (n, d): Plane) -> [f32; 3] {
    t.map(|p| dot(n, p) - d)
}

/// Some corners lie on each side, and not just touching.
fn straddles(s: [f32; 3]) -> bool {
    s.iter().any(|&x| x > EPS) && s.iter().any(|&x| x < -EPS)
}

/// Triangles `a` and `b` (in planes `pa` and `pb`) pass through each other:
/// each has corners on both sides of the other's plane, and their
/// stretches of the line both planes hold overlap. Touching (an edge
/// shared, a corner on the other) isn't crossing.
fn crosses(a: &[[f32; 3]; 3], pa: Plane, b: &[[f32; 3]; 3], pb: Plane) -> bool {
    let (sa, sb) = (sides(a, pb), sides(b, pa));
    if !straddles(sa) || !straddles(sb) {
        return false;
    }
    let line = cross(pa.0, pb.0);
    if dot(line, line) < 1e-12 {
        return false;
    }
    let line = normalize(line);
    let (ia, ib) = (stretch(a, sa, line), stretch(b, sb, line));
    ia.1 > ib.0 + EPS && ib.1 > ia.0 + EPS
}

/// Where triangle `t`, its corners `s` from another's plane, meets that
/// plane, as a stretch along `line`.
fn stretch(t: &[[f32; 3]; 3], s: [f32; 3], line: [f32; 3]) -> (f32, f32) {
    let (mut lo, mut hi) = (f32::INFINITY, f32::NEG_INFINITY);
    let mut take = |p: [f32; 3]| {
        let x = dot(p, line);
        lo = lo.min(x);
        hi = hi.max(x);
    };
    for k in 0..3 {
        let j = (k + 1) % 3;
        if s[k].abs() <= EPS {
            take(t[k]);
        }
        if (s[k] > EPS && s[j] < -EPS) || (s[k] < -EPS && s[j] > EPS) {
            let f = s[k] / (s[k] - s[j]);
            take([0, 1, 2].map(|c| t[k][c] + (t[j][c] - t[k][c]) * f));
        }
    }
    (lo, hi)
}

/// Which others each triangle crosses, found through a grid of cells about
/// as big as a triangle, each pair whose boxes overlap tested once, in the
/// cell where the overlap begins. None where that would pass `limits` (a
/// mesh of very many big triangles over one another).
fn crossings(shapes: &[[[f32; 3]; 3]], planes: &[Option<Plane>], limits: &Limits) -> Option<Vec<Vec<u32>>> {
    let n = shapes.len();
    let boxes: Vec<([f32; 3], [f32; 3])> =
        shapes.iter().map(|t| ([0, 1, 2].map(|k| t[0][k].min(t[1][k]).min(t[2][k])), [0, 1, 2].map(|k| t[0][k].max(t[1][k]).max(t[2][k])))).collect();
    let lo = [0, 1, 2].map(|k| boxes.iter().map(|b| b.0[k]).fold(f32::INFINITY, f32::min));
    let hi = [0, 1, 2].map(|k| boxes.iter().map(|b| b.1[k]).fold(f32::NEG_INFINITY, f32::max));
    let mut sizes: Vec<f32> = boxes.iter().map(|(a, b)| (0..3).map(|k| b[k] - a[k]).fold(0.0, f32::max)).collect();
    let (_, middle, _) = sizes.select_nth_unstable_by(n / 2, f32::total_cmp);
    let mut cell = middle.max(1e-4);
    let res = loop {
        let res = [0, 1, 2].map(|k| (((hi[k] - lo[k]) / cell).ceil() as usize).clamp(1, 1024));
        if res[0] * res[1] * res[2] <= limits.cells {
            break res;
        }
        cell *= 2.0;
    };
    let index = |p: [f32; 3]| [0, 1, 2].map(|k| (((p[k] - lo[k]) / cell).max(0.0) as usize).min(res[k] - 1));
    let id = |c: [usize; 3]| ((c[0] * res[1] + c[1]) * res[2] + c[2]) as u32;
    let mut entries: Vec<(u32, u32)> = Vec::new();
    for (i, (a, b)) in boxes.iter().enumerate().filter(|(i, _)| planes[*i].is_some()) {
        let (from, to) = (index(*a), index(*b));
        for x in from[0]..=to[0] {
            for y in from[1]..=to[1] {
                for z in from[2]..=to[2] {
                    entries.push((id([x, y, z]), i as u32));
                }
            }
        }
        if entries.len() > limits.entries {
            return None;
        }
    }
    entries.sort_unstable();
    let mut cuts = vec![Vec::new(); n];
    let (mut pairs, mut tests, mut found) = (0usize, 0usize, 0usize);
    for group in entries.chunk_by(|a, b| a.0 == b.0) {
        for (k, &(cell, i)) in group.iter().enumerate() {
            for &(_, j) in &group[k + 1..] {
                pairs += 1;
                if pairs > limits.pairs {
                    return None;
                }
                let ((ai, bi), (aj, bj)) = (boxes[i as usize], boxes[j as usize]);
                let start = [0, 1, 2].map(|c| ai[c].max(aj[c]));
                if (0..3).any(|c| start[c] > bi[c].min(bj[c])) || id(index(start)) != cell {
                    continue;
                }
                let (Some(pi), Some(pj)) = (planes[i as usize], planes[j as usize]) else { continue };
                tests += 1;
                if tests > limits.tests {
                    return None;
                }
                if crosses(&shapes[i as usize], pi, &shapes[j as usize], pj) {
                    cuts[i as usize].push(j);
                    cuts[j as usize].push(i);
                    // Each crossing cuts both at least once.
                    found += 2;
                    if found > limits.splits {
                        return None;
                    }
                }
            }
        }
    }
    Some(cuts)
}

/// Cuts one part's triangles, adding the vertices the cuts make.
struct Cutter<'a> {
    part: u32,
    own: &'a [Vertex],
    added: &'a mut Vec<Vertex>,
    /// Where each of `added` lies (`SeeThrough::between`).
    between: &'a mut Vec<(u32, u32, f32)>,
    /// The vertex made where an edge (its ends, lower first) meets a
    /// triangle's plane, by part, so the pieces either side of the edge
    /// share it.
    edges: &'a mut HashMap<(u32, u32, u32, u32), u32>,
}

impl Cutter<'_> {
    fn vertex(&self, i: u32) -> Vertex {
        let i = i as usize;
        match self.own.get(i) {
            Some(v) => *v,
            None => self.added[i - self.own.len()],
        }
    }

    /// Triangle `t` cut by plane `by` (triangle `j`'s) into `out`: as it is
    /// if it doesn't pass through, else the pieces on each side, wound as
    /// it is.
    fn split(&mut self, t: [u32; 3], by: Plane, j: u32, out: &mut Vec<[u32; 3]>) {
        let s = sides(&t.map(|i| self.vertex(i).pos), by);
        if !straddles(s) {
            out.push(t);
            return;
        }
        let (mut front, mut back) = (Vec::with_capacity(4), Vec::with_capacity(4));
        for k in 0..3 {
            let (a, b, sa, sb) = (t[k], t[(k + 1) % 3], s[k], s[(k + 1) % 3]);
            if sa >= -EPS {
                front.push(a);
            }
            if sa <= EPS {
                back.push(a);
            }
            if (sa > EPS && sb < -EPS) || (sa < -EPS && sb > EPS) {
                let (lo, hi, slo, shi) = if a < b { (a, b, sa, sb) } else { (b, a, sb, sa) };
                let x = match self.edges.get(&(self.part, lo, hi, j)) {
                    Some(&x) => x,
                    None => {
                        let f = slo / (slo - shi);
                        self.added.push(part_way(&self.vertex(lo), &self.vertex(hi), f));
                        self.between.push((lo, hi, f));
                        let x = (self.own.len() + self.added.len() - 1) as u32;
                        self.edges.insert((self.part, lo, hi, j), x);
                        x
                    }
                };
                front.push(x);
                back.push(x);
            }
        }
        for side in [front, back] {
            for k in 1..side.len().saturating_sub(1) {
                out.push([side[0], side[k], side[k + 1]]);
            }
        }
    }
}

/// The vertex `t` of the way from `a` to `b`.
fn part_way(a: &Vertex, b: &Vertex, t: f32) -> Vertex {
    let mix = |x: f32, y: f32| x + (y - x) * t;
    let three = |x: [f32; 3], y: [f32; 3]| [0, 1, 2].map(|k| mix(x[k], y[k]));
    let two = |x: [f32; 2], y: [f32; 2]| [mix(x[0], y[0]), mix(x[1], y[1])];
    let tangent = if a.tangent[3] == 0.0 {
        [0.0; 4]
    } else {
        let t = normalize(three([a.tangent[0], a.tangent[1], a.tangent[2]], [b.tangent[0], b.tangent[1], b.tangent[2]]));
        [t[0], t[1], t[2], a.tangent[3]]
    };
    let color = std::array::from_fn(|k| mix(f32::from(a.color[k]), f32::from(b.color[k])).round() as u8);
    Vertex { pos: three(a.pos, b.pos), normal: normalize(three(a.normal, b.normal)), uv: two(a.uv, b.uv), tangent, uv1: two(a.uv1, b.uv1), color }
}

#[cfg(test)]
mod tests {
    use super::super::anim::{Channel, Clip, Interp, Node, Path, Rest, Rig, Track};
    use super::super::{Alpha, Format, Material, IDENTITY};
    use super::*;

    /// A see-through square in the plane its `corners` span, of one part.
    fn pane(corners: [[f32; 3]; 4], color: [f32; 4]) -> Part {
        let vertices = corners.iter().map(|&p| Vertex::new(p, [0.0, 0.0, 1.0], [0.0, 0.0])).collect();
        Part { vertices, indices: vec![0, 1, 2, 0, 2, 3], material: Material { color, alpha: Alpha::Blend, ..Material::default() }, node: None, instances: Vec::new() }
    }

    /// Two see-through squares crossing in an X seen from above: one
    /// leaning back to the right, the other back to the left, crossing
    /// along the y axis.
    fn crossed() -> Vec<Part> {
        let lean = |k: f32| [[-0.5, -0.5, -0.5 * k], [0.5, -0.5, 0.5 * k], [0.5, 0.5, 0.5 * k], [-0.5, 0.5, -0.5 * k]];
        vec![pane(lean(1.0), [1.0, 0.0, 0.0, 0.5]), pane(lean(-1.0), [0.0, 0.0, 1.0, 0.5])]
    }

    fn mesh(parts: Vec<Part>) -> Mesh {
        Mesh::with_rig(parts, super::super::Format::Glb, None, Vec::new(), None).unwrap()
    }

    #[test]
    fn triangles_that_cross_are_cut_where_they_meet_and_no_two_pieces_cross() {
        let m = mesh(crossed());
        let st = &m.see_through;
        assert!(st.triangles.len() > 4, "cut: {} pieces", st.triangles.len());
        assert!(!st.added[0].is_empty() && !st.added[1].is_empty(), "each square gets vertices along the cut");
        // Every new vertex lies on the line where they cross (x = 0, z = 0).
        for v in st.added.iter().flatten() {
            assert!(v.pos[0].abs() < 1e-5 && v.pos[2].abs() < 1e-5, "{:?}", v.pos);
        }
        let corner = |p: u32, i: u32| {
            let part = &m.parts[p as usize];
            part.vertices.get(i as usize).copied().unwrap_or_else(|| st.added[p as usize][i as usize - part.vertices.len()]).pos
        };
        let shapes: Vec<[[f32; 3]; 3]> = st.triangles.iter().map(|&(p, t)| t.map(|i| corner(p, i))).collect();
        for (a, sa) in shapes.iter().enumerate() {
            for sb in &shapes[a + 1..] {
                let (Some(pa), Some(pb)) = (plane(sa), plane(sb)) else { continue };
                assert!(!crosses(sa, pa, sb, pb), "{sa:?} still crosses {sb:?}");
            }
        }
        // The pieces cover the squares: each square's area is kept.
        let area = |t: &[[f32; 3]; 3]| 0.5 * dot(cross(sub(t[1], t[0]), sub(t[2], t[0])), cross(sub(t[1], t[0]), sub(t[2], t[0]))).sqrt();
        let total: f32 = shapes.iter().map(area).sum();
        assert!((total - 2.0 * 2f32.sqrt()).abs() < 1e-4, "{total}");
    }

    #[test]
    fn crossing_see_through_parts_are_drawn_far_halves_first() {
        let m = mesh(crossed());
        let order = m.draw_order(None, IDENTITY);
        assert!(order.solid.is_empty());
        assert_eq!(order.indices.len(), m.see_through.triangles.len() * 3, "every piece drawn once");
        let z = |p: usize, i: u32| {
            let part = &m.parts[p];
            part.vertices.get(i as usize).copied().unwrap_or_else(|| m.see_through.added[p][i as usize - part.vertices.len()]).pos[2]
        };
        // Each piece, in the order drawn, and which half it's in: behind
        // where the squares cross, or in front.
        let mut drawn = Vec::new();
        let mut at = 0usize;
        for &(p, n) in &order.runs {
            for t in order.indices[at..at + n as usize].chunks(3) {
                let zs = [z(p, t[0]), z(p, t[1]), z(p, t[2])];
                let behind = zs.iter().all(|&z| z <= 1e-5);
                assert!(behind || zs.iter().all(|&z| z >= -1e-5), "a piece on both sides: {zs:?}");
                drawn.push((p, behind));
            }
            at += n as usize;
        }
        // Seen from the front: the far halves of both first, then the near.
        let first_near = drawn.iter().position(|d| !d.1).expect("some in front");
        assert!(drawn[first_near..].iter().all(|d| !d.1), "{drawn:?}");
        for half in [&drawn[..first_near], &drawn[first_near..]] {
            assert!(half.iter().any(|d| d.0 == 0) && half.iter().any(|d| d.0 == 1), "{drawn:?}");
        }
    }

    #[test]
    fn a_closed_see_through_part_draws_its_far_side_first() {
        // A box's faces as they come in the file: front, then back.
        let front = [[-0.5, -0.5, 0.5], [0.5, -0.5, 0.5], [0.5, 0.5, 0.5], [-0.5, 0.5, 0.5]];
        let back = [[-0.5, -0.5, -0.5], [-0.5, 0.5, -0.5], [0.5, 0.5, -0.5], [0.5, -0.5, -0.5]];
        let mut part = pane(front, [1.0, 1.0, 1.0, 0.3]);
        part.vertices.extend(back.iter().map(|&p| Vertex::new(p, [0.0, 0.0, -1.0], [0.0, 0.0])));
        part.indices.extend([4, 5, 6, 4, 6, 7]);
        let m = mesh(vec![part]);
        let order = m.draw_order(None, IDENTITY);
        assert_eq!(order.runs, [(0, 12)]);
        assert_eq!(&order.indices[..6], &[4, 5, 6, 4, 6, 7], "the back first");
        // Turned half round, the front is the far side.
        let behind = [[-1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, -1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];
        assert_eq!(&m.draw_order(None, behind).indices[..6], &[0, 1, 2, 0, 2, 3]);
    }

    /// `parts` as a mesh that moves by itself: each on a node of its own,
    /// the last sliding by `slide` over a second.
    fn moving(parts: Vec<Part>, slide: [f32; 3]) -> Mesh {
        let last = parts.len() - 1;
        let track = Track::new(vec![0.0, 1.0], vec![0.0, 0.0, 0.0, slide[0], slide[1], slide[2]], Interp::Linear).unwrap();
        let mut rig = Rig::new(vec![Node::default(); parts.len()], Vec::new(), vec![Clip::new(vec![Channel { node: last, path: Path::Translation, track }])]);
        rig.parts = parts
            .iter()
            .enumerate()
            .map(|(i, p)| Rest { node: i, positions: p.vertices.iter().map(|v| v.pos).collect(), normals: p.vertices.iter().map(|v| v.normal).collect(), ..Rest::default() })
            .collect();
        Mesh::with_rig(parts, Format::Glb, Some(rig), Vec::new(), None).unwrap()
    }

    /// The corners of each of `triangles`, numbered into `vertices`.
    fn shapes(vertices: &[Vec<Vertex>], triangles: &[(u32, [u32; 3])]) -> Vec<[[f32; 3]; 3]> {
        triangles.iter().map(|&(p, t)| t.map(|i| vertices[p as usize][i as usize].pos)).collect()
    }

    fn area(t: &[[f32; 3]; 3]) -> f32 {
        let n = cross(sub(t[1], t[0]), sub(t[2], t[0]));
        0.5 * dot(n, n).sqrt()
    }

    #[test]
    fn a_moving_meshs_crossing_triangles_are_cut_where_they_cross_as_each_frame_poses_them() {
        // The blue square slides half a unit right over a second, so where
        // the two cross moves right with it, a quarter unit in the file
        // (in the mesh's box, 1.5 units long, from a sixth left of its
        // middle to the middle).
        let m = moving(crossed(), [0.5, 0.0, 0.0]);
        assert!(m.see_through.added.iter().all(|a| a.iter().all(|v| (v.pos[0] + 1.0 / 6.0).abs() < 1e-5)), "cut at rest where they cross then");
        let pose = m.posed(0, 1.0).expect("it plays");
        let st = pose.see_through.as_ref().expect("cut afresh, as it has few see-through triangles");
        let own = |p: usize| m.parts[p].vertices.len();
        let plane_of = |p: usize| plane(&[0, 1, 2].map(|i| pose.vertices[p][i].pos)).unwrap();
        for p in 0..2 {
            let added = &pose.vertices[p][own(p)..];
            assert!(!added.is_empty(), "the cuts' vertices come after square {p}'s own");
            // Each lies where the squares cross now: in both their planes.
            for v in added {
                assert!(v.pos[0].abs() < 1e-5, "{:?}", v.pos);
                for q in 0..2 {
                    let (n, d) = plane_of(q);
                    assert!((dot(n, v.pos) - d).abs() < 1e-5, "{:?} off square {q}", v.pos);
                }
            }
        }
        let pieces = shapes(&pose.vertices, &st.triangles);
        for (a, sa) in pieces.iter().enumerate() {
            for sb in &pieces[a + 1..] {
                let (Some(pa), Some(pb)) = (plane(sa), plane(sb)) else { continue };
                assert!(!crosses(sa, pa, sb, pb), "{sa:?} still crosses {sb:?}");
            }
        }
        let whole: f32 = shapes(&pose.vertices, &own_triangles(&m.parts, |_| true).collect::<Vec<_>>()).iter().map(area).sum();
        assert!((pieces.iter().map(area).sum::<f32>() - whole).abs() < 1e-5, "the pieces cover the squares");
        // Seen from the front, the halves behind where they cross now are
        // drawn first, then those in front.
        let order = m.draw_order(Some(&pose), IDENTITY);
        assert_eq!(order.indices.len(), st.triangles.len() * 3, "every piece drawn once");
        let line = pose.vertices[0][own(0)].pos[2];
        let mut drawn = Vec::new();
        let mut at = 0usize;
        for &(p, n) in &order.runs {
            for t in order.indices[at..at + n as usize].chunks(3) {
                let zs = t.iter().map(|&i| pose.vertices[p][i as usize].pos[2] - line).collect::<Vec<_>>();
                let behind = zs.iter().all(|&z| z <= 1e-5);
                assert!(behind || zs.iter().all(|&z| z >= -1e-5), "a piece on both sides: {zs:?}");
                drawn.push((p, behind));
            }
            at += n as usize;
        }
        let first_near = drawn.iter().position(|d| !d.1).expect("some in front");
        assert!(drawn[first_near..].iter().all(|d| !d.1) && drawn[..first_near].iter().any(|d| d.0 == 0) && drawn[..first_near].iter().any(|d| d.0 == 1), "{drawn:?}");
    }

    /// A see-through square as `crossed` leans it (`k` 1 or -1), in `n` × `n`
    /// squares of two triangles each.
    fn fine_pane(k: f32, n: u32, color: [f32; 4]) -> Part {
        let mut part = pane([[0.0; 3]; 4], color);
        part.vertices = (0..=n)
            .flat_map(|j| (0..=n).map(move |i| (i as f32 / n as f32 - 0.5, j as f32 / n as f32 - 0.5)))
            .map(|(x, y)| Vertex::new([x, y, k * x], [0.0, 0.0, 1.0], [0.0; 2]))
            .collect();
        let at = |i: u32, j: u32| j * (n + 1) + i;
        part.indices = (0..n).flat_map(|j| (0..n).flat_map(move |i| [at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j), at(i + 1, j + 1), at(i, j + 1)])).collect();
        part
    }

    #[test]
    fn a_big_moving_meshs_cuts_made_at_rest_move_with_the_vertices_they_lie_between() {
        // Too many see-through triangles to cut each frame. The blue square
        // slides along where they cross, which so stays where it was cut.
        let parts = vec![fine_pane(1.0, 25, [1.0, 0.0, 0.0, 0.5]), fine_pane(-1.0, 25, [0.0, 0.0, 1.0, 0.5])];
        assert!(parts.iter().map(|p| p.indices.len() / 3).sum::<usize>() > EACH_FRAME);
        let m = moving(parts, [0.0, 0.5, 0.0]);
        let rest = &m.see_through;
        assert!(!rest.each_frame && rest.added.iter().all(|a| !a.is_empty()), "cut once, at rest");
        let pose = m.posed(0, 1.0).expect("it plays");
        assert!(pose.see_through.is_none(), "the cuts made at rest are drawn");
        // The red square stays; the blue one and the vertices cut on it
        // slide by half a unit in the file, a third in the mesh's box.
        for (p, shift) in [(0, 0.0), (1, 0.5 / 1.5)] {
            let (own, posed) = (m.parts[p].vertices.len(), &pose.vertices[p]);
            assert_eq!(posed.len(), own + rest.added[p].len());
            for (was, now) in rest.added[p].iter().zip(&posed[own..]) {
                assert!((now.pos[1] - was.pos[1] - shift).abs() < 1e-5 && (now.pos[0] - was.pos[0]).abs() < 1e-5 && (now.pos[2] - was.pos[2]).abs() < 1e-5, "{:?} then {:?}", was.pos, now.pos);
            }
        }
        // So, posed, the pieces still don't cross.
        let pieces = shapes(&pose.vertices, &rest.triangles);
        let planes: Vec<Option<Plane>> = pieces.iter().map(plane).collect();
        assert!(crossings(&pieces, &planes, &AT_REST).unwrap().iter().all(Vec::is_empty));
        // Nor does it cut afresh at rest, held there.
        assert_eq!(m.draw_order(None, IDENTITY).indices.len(), rest.triangles.len() * 3);
    }

    /// The color `by_height` gives a corner at height `y`: red at the
    /// bottom (-0.5), blue and fainter at the top (0.5).
    fn height_color(y: f32) -> [u8; 4] {
        let t = y + 0.5;
        [255.0 * (1.0 - t), 0.0, 255.0 * t, 255.0 - 204.0 * t].map(|c| c.round() as u8)
    }

    /// `part` with each corner colored by its height.
    fn by_height(mut part: Part) -> Part {
        for v in &mut part.vertices {
            v.color = height_color(v.pos[1]);
        }
        part
    }

    #[test]
    fn a_cut_takes_the_color_between_the_corners_it_lies_between() {
        // Colored by height, which runs straight along every edge, so a
        // cut's color is the color at its own height, within rounding.
        let near = |a: [u8; 4], b: [u8; 4]| a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= 1);
        let m = mesh(crossed().into_iter().map(by_height).collect());
        assert!(m.see_through.added.iter().all(|a| !a.is_empty()));
        for v in m.see_through.added.iter().flatten() {
            assert!(near(v.color, height_color(v.pos[1])), "{:?} at {:?}", v.color, v.pos);
        }
        // Cut afresh as a moving mesh is posed: the blue square slides
        // across, so the mesh's box is 1.5 units long, and its heights a
        // third smaller in it.
        let m = moving(crossed().into_iter().map(by_height).collect(), [0.5, 0.0, 0.0]);
        let pose = m.posed(0, 1.0).expect("it plays");
        assert!(pose.see_through.is_some(), "cut afresh");
        for p in 0..2 {
            let added = &pose.vertices[p][m.parts[p].vertices.len()..];
            assert!(!added.is_empty());
            for v in added {
                assert!(near(v.color, height_color(v.pos[1] / m.extent[1])), "{:?} at {:?}", v.color, v.pos);
            }
        }
        // The cuts made at rest, posed with the vertices they lie between,
        // keep the colors they were cut with.
        let parts = vec![by_height(fine_pane(1.0, 25, [1.0, 1.0, 1.0, 0.5])), by_height(fine_pane(-1.0, 25, [1.0, 1.0, 1.0, 0.5]))];
        let m = moving(parts, [0.0, 0.5, 0.0]);
        let pose = m.posed(0, 1.0).expect("it plays");
        assert!(pose.see_through.is_none(), "the cuts made at rest");
        let colors = |v: &[Vertex]| v.iter().map(|v| v.color).collect::<Vec<_>>();
        for p in 0..2 {
            let own = m.parts[p].vertices.len();
            assert!(!m.see_through.added[p].is_empty());
            assert_eq!(colors(&pose.vertices[p][own..]), colors(&m.see_through.added[p]));
        }
    }

    #[test]
    fn a_frame_whose_cutting_would_take_too_long_keeps_the_cuts_made_at_rest() {
        // Six hundred triangles turned about the y axis like the pages of
        // an open book, each crossing every other along it: cut once at
        // rest, but too many pairs to look at each frame.
        let mut part = pane([[0.0; 3]; 4], [1.0, 1.0, 1.0, 0.3]);
        part.vertices.clear();
        part.indices.clear();
        for k in 0..600u32 {
            let (s, c) = (k as f32 * std::f32::consts::PI / 600.0).sin_cos();
            part.vertices.extend([[-c, -1.0, -s], [c, -1.0, s], [0.0, 1.0, 0.0]].map(|p| Vertex::new(p, [0.0, 0.0, 1.0], [0.0; 2])));
            part.indices.extend([3 * k, 3 * k + 1, 3 * k + 2]);
        }
        let m = moving(vec![part], [0.0, 0.1, 0.0]);
        assert!(m.see_through.each_frame && m.see_through.triangles.len() > 600, "few enough to try each frame, cut at rest");
        let pose = m.posed(0, 0.5).expect("it plays");
        assert!(pose.see_through.is_none(), "kept the cuts made at rest");
        assert_eq!(pose.vertices[0].len(), m.parts[0].vertices.len() + m.see_through.added[0].len());
    }

    #[test]
    fn a_part_made_see_through_after_the_mesh_is_still_drawn() {
        let mut parts = crossed();
        parts[1].material.alpha = Alpha::Opaque;
        let mut m = mesh(parts);
        m.parts[1].material.alpha = Alpha::Blend;
        m.parts[0].material.alpha = Alpha::Opaque;
        let order = m.draw_order(None, IDENTITY);
        assert_eq!(order.solid, [0]);
        assert_eq!(order.runs, [(1, 6)]);
    }

    #[test]
    fn too_many_runs_draw_each_part_whole_in_its_own_order() {
        // Two parts' triangles laid alternately from far to near.
        let strip = |first: f32| {
            let mut part = pane([[0.0; 3]; 4], [1.0, 1.0, 1.0, 0.5]);
            part.vertices.clear();
            part.indices.clear();
            for k in 0..MAX_RUNS as u32 {
                let z = first + 2.0 * k as f32;
                part.vertices.extend([[0.0, 0.0, z], [1.0, 0.0, z], [0.0, 1.0, z]].map(|p| Vertex::new(p, [0.0, 0.0, 1.0], [0.0; 2])));
                part.indices.extend([3 * k, 3 * k + 1, 3 * k + 2]);
            }
            part
        };
        let m = mesh(vec![strip(0.0), strip(1.0)]);
        let order = m.draw_order(None, IDENTITY);
        assert_eq!(order.runs, [(0, 3 * MAX_RUNS as u32), (1, 3 * MAX_RUNS as u32)]);
        assert!(order.indices[..6] == [0, 1, 2, 3, 4, 5], "each part's own farthest first");
    }

    #[test]
    fn the_radix_sort_orders_floats_as_they_compare_and_keeps_equals_in_order() {
        let zs = [3.5f32, -1.0, 0.0, -0.0, 1e-30, -7.25, 3.5, 1e9, -1e9];
        let mut items: Vec<(u32, u32)> = zs.iter().enumerate().map(|(i, &z)| (sortable(z), i as u32)).collect();
        radix_sort(&mut items);
        let got: Vec<u32> = items.iter().map(|&(_, i)| i).collect();
        let mut want: Vec<u32> = (0..zs.len() as u32).collect();
        want.sort_by(|&a, &b| zs[a as usize].total_cmp(&zs[b as usize]));
        assert_eq!(got, want);
        let mut many: Vec<(u32, u32)> = (0..5000u32).map(|i| (sortable(((i * 7919) % 5000) as f32 * 0.37 - 900.0), i)).collect();
        radix_sort(&mut many);
        assert!(many.windows(2).all(|w| w[0].0 <= w[1].0));
    }
}
