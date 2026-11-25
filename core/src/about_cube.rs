//! The About card's cube, after `flow-demo cube`: a glossy small cube drops
//! onto a purple floor, bouncing, and rolls about it, hopping square to
//! square and lighting each it lands on until they spell an F. Then the eye
//! pulls back and swings round, and the floor is the top of a big cube,
//! which jumps and springs as it shows; the small one sits on its corner and
//! hops now and then, squashing as it lands.
//!
//! Each frame is a model mesh laid out in the picture's pixels
//! ([`Mesh::scene`]): a camera projects the world in perspective, with a
//! depth of one over the distance, so it stays true across a triangle, and
//! every face winds counter-clockwise, since the model shader lights a
//! clockwise one as its back. The renderer for 3D models draws it into the
//! picture the card shows over its glass.

use std::f32::consts::{FRAC_PI_2, PI};
use std::sync::Arc;

use crate::graphics::Image;
use crate::model::{Mesh, Part, Shown, Slot, Style, Vertex};

type V3 = [f32; 3];
/// A rotation, by rows.
type M3 = [[f32; 3]; 3];
type Color = [f32; 4];

const IDENTITY: M3 = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

const BODY: u32 = 0x6a58d8;
const FLOOR: u32 = 0x463aa8;
const SMALL: u32 = 0x9d8cff;
const LINES: u32 = 0x221a5a;
const LIT: u32 = 0x7ce7ff;
/// How glossy each is.
const GLOSSY: f32 = 0.55;
const GLOSSIER: f32 = 0.8;

/// The big cube's top is 3 × 3 squares; a square is (across, back to front).
type Square = (i32, i32);
/// Where the small cube drops, then where it rolls: the first six squares it
/// lands on light up (an F, its bar at the back), the rest take it to the
/// front right corner.
const START: Square = (0, 2);
const PATH: [Square; 8] = [(0, 1), (0, 0), (1, 0), (2, 0), (1, 0), (1, 1), (2, 1), (2, 2)];
const LIGHTING: usize = 6;

/// When it starts to fall, and lands.
const FALL: f32 = 0.3;
const LANDS: f32 = 0.85;
/// Its bounces after landing: when each lands again, and how high it goes.
const BOUNCES: [(f32, f32); 3] = [(1.27, 0.9), (1.53, 0.35), (1.68, 0.12)];
/// The first roll, a roll's seconds, and a roll and its rest.
const ROLLS: f32 = 1.85;
const ROLL: f32 = 0.34;
const STEP: f32 = 0.46;
/// The eye pulls back between these; the big cube jumps; all is still but
/// for the small one's hops, one every `HOP_EVERY`.
const PULL: (f32, f32) = (4.0, 6.9);
const REVEAL: f32 = 6.7;
const DONE: f32 = 7.6;
const HOP_EVERY: f32 = 3.0;

pub struct AboutCube {
    t: f32,
    /// Where the eye looks while it follows the small cube.
    follow: V3,
    /// The small cube's turn before each roll.
    turns: Vec<M3>,
    slot: Arc<Slot>,
    pub image: Arc<Image>,
    size: (u32, u32),
    settled: bool,
}

impl AboutCube {
    /// A cube for a picture `size` pixels; `settled` starts it after it has
    /// all played (for snapshots).
    pub fn new(size: (u32, u32), settled: bool) -> Self {
        let mut turns = vec![IDENTITY];
        let mut at = START;
        for &next in &PATH {
            let (axis, _) = roll_axis(at, next);
            let last = *turns.last().expect("one at least");
            turns.push(compose(rotation(axis, FRAC_PI_2), last));
            at = next;
        }
        let slot = Arc::new(Slot::default());
        let shown = Arc::new(Shown::new(slot.clone(), Style { scale: 1.0 / 0.9, ..Style::default() }, 1.0));
        let image = Arc::new(Image::model(size.0.max(1), size.1.max(1), shown));
        let t = if settled { DONE + 1.0 } else { 0.0 };
        let mut cube = Self { t, follow: [1.5, 2.55, 1.5], turns, slot, image, size, settled };
        cube.step(0.0);
        cube
    }

    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    /// Moves on to `age` seconds after the card opened (its clock, not the
    /// frames', which slow behind other windows).
    pub fn run_to(&mut self, age: f32) {
        let start = if self.settled { DONE + 1.0 } else { 0.0 };
        self.step((start + age - self.t).max(0.0));
    }

    /// The small cube's middle, its turn, and how it's squashed (up; across
    /// keeps its volume), `t` seconds in.
    fn small(&self) -> (V3, M3, f32) {
        let t = self.t;
        let rest = |s: Square| add(top_of(s), [0.0, 0.5, 0.0]);
        if t < ROLLS {
            let height = if t < LANDS {
                let k = ((t - FALL) / (LANDS - FALL)).clamp(0.0, 1.0);
                1.6 * (1.0 - k * k)
            } else {
                // Each bounce a parabola, lower than the last.
                let mut from = LANDS;
                let mut h = 0.0;
                for (until, high) in BOUNCES {
                    if t < until {
                        let k = (t - from) / (until - from);
                        h = high * 4.0 * k * (1.0 - k);
                        break;
                    }
                    from = until;
                }
                h
            };
            let impacts = [(LANDS, 0.32), (BOUNCES[0].0, 0.2), (BOUNCES[1].0, 0.12), (BOUNCES[2].0, 0.06)];
            return (add(rest(START), [0.0, height, 0.0]), IDENTITY, squash(t, &impacts));
        }
        let i = (((t - ROLLS) / STEP) as usize).min(PATH.len());
        let from = if i == 0 { START } else { PATH[i - 1] };
        if i == PATH.len() {
            // Done rolling: a hop now and then, landing with a squash.
            let landed = ROLLS + (PATH.len() - 1) as f32 * STEP + ROLL;
            let idle = (t - DONE).max(0.0);
            let since = idle % HOP_EVERY;
            let hop = if t > DONE && since < 0.5 { 0.7 * 4.0 * (since / 0.5) * (1.0 - since / 0.5) } else { 0.0 };
            let last_hop = DONE + (idle / HOP_EVERY).floor() * HOP_EVERY + 0.5;
            let impacts = [(landed, 0.14), (last_hop, if t > DONE { 0.22 } else { 0.0 })];
            return (add(rest(from), [0.0, hop, 0.0]), self.turns[i], squash(t, &impacts));
        }
        let start = ROLLS + i as f32 * STEP;
        let k = smooth(0.0, 1.0, ((t - start) / ROLL).min(1.0));
        let (axis, pivot) = roll_axis(from, PATH[i]);
        let spin = rotation(axis, FRAC_PI_2 * k);
        // It hops a little as it tips over the edge.
        let lift = 0.16 * (PI * k).sin();
        let center = add(add(pivot, apply(spin, sub(rest(from), pivot))), [0.0, lift, 0.0]);
        let landed = if i == 0 { LANDS } else { start - STEP + ROLL };
        (center, compose(spin, self.turns[i]), squash(t, &[(landed, 0.14)]))
    }

    /// When a square lit up, if it has.
    fn lit_at(&self, square: Square) -> Option<f32> {
        let mut when = (square == START).then_some(LANDS);
        for (i, &s) in PATH.iter().take(LIGHTING).enumerate() {
            if s == square && when.is_none() {
                when = Some(ROLLS + i as f32 * STEP + ROLL);
            }
        }
        when.filter(|&w| self.t >= w)
    }

    /// The big cube's jump as it shows: how high, and how squashed.
    fn big(&self) -> (f32, f32) {
        let s = self.t - REVEAL;
        let jump = if (0.0..0.42).contains(&s) { 0.35 * 4.0 * (s / 0.42) * (1.0 - s / 0.42) } else { 0.0 };
        (jump, squash(self.t, &[(REVEAL + 0.42, 0.1)]))
    }

    /// Moves on `dt` seconds and lays out the frame.
    pub fn step(&mut self, dt: f32) {
        let first = self.t == 0.0 || dt == 0.0;
        self.t += dt;
        let (small, turn, squashed) = self.small();
        // The eye leans after it, keeping the whole floor in the picture.
        let goal = lerp([1.5, 2.55, 1.5], [small[0], 2.55, small[2]], 0.3);
        self.follow = if first { goal } else { lerp(self.follow, goal, 1.0 - (-dt * 5.0).exp()) };
        let (w, h) = (self.size.0 as f32, self.size.1 as f32);
        let k = smooth(PULL.0, PULL.1, self.t);
        // Close over it from the front right, then back and round to the
        // front left; then a slow sway.
        let sway = 0.12 * (0.4 * (self.t - DONE).max(0.0)).sin();
        let near = (self.follow, 0.3, 1.0, 7.9, 0.72);
        let far = ([1.5, 1.45, 1.5], -0.35 + sway, 0.7, 10.0, 0.62);
        let mix = |a: f32, b: f32| a + (b - a) * k;
        let cam = Camera::orbit(lerp(near.0, far.0, k), mix(near.1, far.1), mix(near.2, far.2), mix(near.3, far.3), mix(near.4, far.4), h);

        let mut b = Builder { cam: &cam, parts: Vec::new() };
        let (jump, big_squash) = self.big();
        let big_scale = [1.0 / big_squash.sqrt(), big_squash, 1.0 / big_squash.sqrt()];
        // The big cube squashes about the middle of its foot, and carries
        // the small one with it.
        let carry = move |p: V3| [1.5 + (p[0] - 1.5) * big_scale[0], p[1] * big_scale[1] + jump, 1.5 + (p[2] - 1.5) * big_scale[2]];
        let carry_n = move |n: V3| normalize([n[0] / big_scale[0], n[1] / big_scale[1], n[2] / big_scale[2]]);

        let t = self.t;
        let shine = |s: Square| {
            // A gleam sweeps across the lit squares as the cube shows.
            let at = REVEAL - 0.2 + (s.0 + s.1) as f32 * 0.12;
            smooth(at, at + 0.15, t) * (1.0 - smooth(at + 0.15, at + 0.45, t))
        };
        let lit: Vec<Option<f32>> = (0..9).map(|i| self.lit_at((i % 3, i / 3))).collect();
        let face = |n: V3, a: usize, c: usize| -> (Color, f32) {
            if n[1] < 0.5 {
                return (rgb(BODY), GLOSSY);
            }
            let s = (a as i32, c as i32);
            match lit[a + 3 * c] {
                Some(when) => (mix_color(mix_color(rgb(0xffffff), rgb(LIT), smooth(when, when + 0.4, t)), rgb(0xffffff), shine(s) * 0.7), GLOSSIER),
                None => (rgb(FLOOR), GLOSSY),
            }
        };
        b.cube(3, 1.5, &face, &|p| carry(add(p, [1.5, 1.5, 1.5])), &carry_n);

        let s = [1.0 / squashed.sqrt(), squashed, 1.0 / squashed.sqrt()];
        let foot = sub(small, [0.0, 0.5, 0.0]);
        let place = move |p: V3| {
            let q = sub(add(small, apply(turn, p)), foot);
            carry(add(foot, [q[0] * s[0], q[1] * s[1], q[2] * s[2]]))
        };
        let aim = move |n: V3| {
            let m = apply(turn, n);
            carry_n([m[0] / s[0], m[1] / s[1], m[2] / s[2]])
        };
        b.cube(1, 0.5, &|_, _, _| (rgb(SMALL), GLOSSIER), &place, &aim);
        self.slot.set(Mesh::scene(b.parts, (w, h)));
    }
}

/// How squashed something is at `t` after landing at the `impacts`' times,
/// each as hard as it says: down and springing back, dying away.
fn squash(t: f32, impacts: &[(f32, f32)]) -> f32 {
    let mut y = 1.0;
    for &(at, hard) in impacts {
        let s = t - at;
        if s >= 0.0 {
            y -= hard * (-s / 0.07).exp() * (s * 38.0).cos();
        }
    }
    y.clamp(0.5, 1.5)
}

/// The middle of a square on the big cube's top.
fn top_of((x, z): Square) -> V3 {
    [x as f32 + 0.5, 3.0, z as f32 + 0.5]
}

/// What a roll from one square to the next turns about: the axis, and a
/// point on the edge it tips over.
fn roll_axis(from: Square, to: Square) -> (V3, V3) {
    let d = [(to.0 - from.0) as f32, 0.0, (to.1 - from.1) as f32];
    (cross([0.0, 1.0, 0.0], d), add(top_of(from), mul(d, 0.5)))
}

/// A camera seeing the world (y up) in perspective, onto the picture's
/// pixels from its middle, y up, with a depth that's one over the distance.
struct Camera {
    eye: V3,
    right: V3,
    up: V3,
    ahead: V3,
    focal: f32,
    depth: f32,
}

impl Camera {
    /// Looking at `target` from `distance` away, `yaw` radians round from
    /// in front (+z) and `pitch` above, seeing `fov` radians up and down in
    /// a picture `height` pixels tall.
    fn orbit(target: V3, yaw: f32, pitch: f32, distance: f32, fov: f32, height: f32) -> Self {
        let eye = add(target, mul([yaw.sin() * pitch.cos(), pitch.sin(), yaw.cos() * pitch.cos()], distance));
        let ahead = normalize(sub(target, eye));
        let right = normalize(cross(ahead, [0.0, 1.0, 0.0]));
        let up = cross(right, ahead);
        Self { eye, right, up, ahead, focal: height * 0.5 / (fov * 0.5).tan(), depth: height * 2.0 }
    }

    fn project(&self, p: V3) -> Option<V3> {
        let d = sub(p, self.eye);
        let z = dot(d, self.ahead);
        (z > 0.05).then(|| [self.focal * dot(d, self.right) / z, self.focal * dot(d, self.up) / z, self.depth / z])
    }

    /// A direction in the world as the picture has it: x right, y up, z
    /// toward the eye.
    fn turn(&self, n: V3) -> V3 {
        [dot(n, self.right), dot(n, self.up), -dot(n, self.ahead)]
    }
}

/// The frame's parts, one a color and gloss.
struct Builder<'a> {
    cam: &'a Camera,
    parts: Vec<Part>,
}

impl Builder<'_> {
    /// A flat face of the world, if it faces the eye.
    fn face(&mut self, (color, gloss): (Color, f32), points: [V3; 4], normal: V3) {
        let center = mul(points.iter().fold([0.0; 3], |a, &p| add(a, p)), 0.25);
        if dot(sub(self.cam.eye, center), normal) <= 0.0 {
            return;
        }
        let Some(mut at) = points.iter().map(|&p| self.cam.project(p)).collect::<Option<Vec<V3>>>() else { return };
        // Counter-clockwise on the picture, or the shader lights its back.
        let twice_area: f32 = (0..4).map(|i| at[i][0] * at[(i + 1) % 4][1] - at[(i + 1) % 4][0] * at[i][1]).sum();
        if twice_area < 0.0 {
            at.reverse();
        }
        let normal = self.cam.turn(normal);
        let part = match self.parts.iter().position(|p| p.material.color == color && p.material.gloss == gloss) {
            Some(i) => &mut self.parts[i],
            None => {
                self.parts.push(Part::plain(color, gloss));
                self.parts.last_mut().expect("just pushed")
            }
        };
        let base = part.vertices.len() as u32;
        part.vertices.extend(at.iter().map(|&pos| Vertex::new(pos, normal, [0.0, 0.0])));
        part.indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    /// A cube `half` either side of its middle, each face parted into
    /// `cells` × `cells` squares colored by `color` (from the face's way out
    /// and the square), with dark lines between them and round its edges;
    /// `place` and `aim` take its points and ways out into the world.
    fn cube(&mut self, cells: usize, half: f32, color: &dyn Fn(V3, usize, usize) -> (Color, f32), place: &dyn Fn(V3) -> V3, aim: &dyn Fn(V3) -> V3) {
        let side = half * 2.0;
        let step = side / cells as f32;
        let width = 0.035 * side.min(1.0);
        let ways: [(V3, V3, V3); 6] = [
            ([1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
            ([-1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
            ([0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
            ([0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
            ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
            ([0.0, 0.0, -1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        ];
        for (n, u, v) in ways {
            let normal = aim(n);
            let at = |a: f32, b: f32, lift: f32| place(add(mul(n, half + lift), add(mul(u, a - half), mul(v, b - half))));
            let square = |a0: f32, b0: f32, a1: f32, b1: f32, lift: f32| [at(a0, b0, lift), at(a1, b0, lift), at(a1, b1, lift), at(a0, b1, lift)];
            for a in 0..cells {
                for c in 0..cells {
                    let (a0, b0) = (a as f32 * step, c as f32 * step);
                    self.face(color(n, a, c), square(a0, b0, a0 + step, b0 + step, 0.0), normal);
                }
            }
            // The lines lie just off the face, those at its edges on it only.
            for k in 0..=cells {
                let p = k as f32 * step;
                let (lo, hi) = ((p - width * 0.5).max(0.0), (p + width * 0.5).min(side));
                self.face((rgb(LINES), 0.0), square(lo, 0.0, hi, side, 0.006), normal);
                self.face((rgb(LINES), 0.0), square(0.0, lo, side, hi, 0.006), normal);
            }
        }
    }
}

fn rgb(hex: u32) -> Color {
    let c = |shift: u32| ((hex >> shift) & 0xff) as f32 / 255.0;
    [c(16), c(8), c(0), 1.0]
}

fn mix_color(a: Color, b: Color, t: f32) -> Color {
    [0, 1, 2, 3].map(|i| a[i] + (b[i] - a[i]) * t)
}

fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn mul(a: V3, k: f32) -> V3 {
    [a[0] * k, a[1] * k, a[2] * k]
}

fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn normalize(a: V3) -> V3 {
    let len = dot(a, a).sqrt();
    if len < 1e-9 {
        [0.0, 0.0, 1.0]
    } else {
        mul(a, 1.0 / len)
    }
}

fn lerp(a: V3, b: V3, t: f32) -> V3 {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

/// Eases from 0 to 1 as `t` goes from `a` to `b`.
fn smooth(a: f32, b: f32, t: f32) -> f32 {
    let x = ((t - a) / (b - a)).clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// Turning by `angle` radians about `axis`, the right-hand way.
fn rotation(axis: V3, angle: f32) -> M3 {
    let [x, y, z] = normalize(axis);
    let (s, c) = angle.sin_cos();
    let k = 1.0 - c;
    [
        [c + x * x * k, x * y * k - z * s, x * z * k + y * s],
        [y * x * k + z * s, c + y * y * k, y * z * k - x * s],
        [z * x * k - y * s, z * y * k + x * s, c + z * z * k],
    ]
}

fn apply(m: M3, v: V3) -> V3 {
    [dot(m[0], v), dot(m[1], v), dot(m[2], v)]
}

/// `a` after `b`.
fn compose(a: M3, b: M3) -> M3 {
    let col = |j: usize| [b[0][j], b[1][j], b[2][j]];
    let row = |i: usize| [dot(a[i], col(0)), dot(a[i], col(1)), dot(a[i], col(2))];
    [row(0), row(1), row(2)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lit_squares_spell_an_f_and_the_small_cube_ends_on_the_corner() {
        let mut cube = AboutCube::new((300, 190), true);
        let lit: Vec<Square> = (0..3).flat_map(|z| (0..3).map(move |x| (x, z))).filter(|&s| cube.lit_at(s).is_some()).collect();
        assert_eq!(lit, vec![(0, 0), (1, 0), (2, 0), (0, 1), (1, 1), (0, 2)]);
        // Between hops it sits on the front right corner, square and whole.
        cube.t = DONE + HOP_EVERY - 0.2;
        let (center, turn, squashed) = cube.small();
        assert!(sub(center, [2.5, 3.5, 2.5]).iter().all(|d| d.abs() < 1e-3), "{center:?}");
        assert!(turn.iter().flatten().all(|v| v.abs() < 1e-4 || (v.abs() - 1.0).abs() < 1e-4));
        assert!((squashed - 1.0).abs() < 0.01);
    }

    #[test]
    fn it_bounces_lower_each_time_and_squashes_as_it_lands() {
        let cube = |t: f32| AboutCube { t, ..AboutCube::new((300, 190), false) };
        let high = |from: f32, until: f32| cube((from + until) * 0.5).small().0[1] - 3.5;
        let (one, two, three) = (high(LANDS, BOUNCES[0].0), high(BOUNCES[0].0, BOUNCES[1].0), high(BOUNCES[1].0, BOUNCES[2].0));
        assert!(one > two && two > three && three > 0.0, "{one} {two} {three}");
        assert!(cube(LANDS + 0.005).small().2 < 0.8, "squashed as it lands");
        assert!((cube(LANDS + 0.4).small().2 - 1.0).abs() < 0.05, "and springing back");
    }

    #[test]
    fn every_frame_is_glossy_faces_that_wind_counter_clockwise_inside_the_picture() {
        let mut cube = AboutCube::new((300, 190), false);
        for _ in 0..24 {
            cube.step(0.5);
            let mesh = cube.slot.get().expect("a frame");
            assert!(mesh.parts.iter().any(|p| p.material.gloss > 0.5));
            for p in &mesh.parts {
                for t in p.indices.chunks(3) {
                    let [a, b, c] = [t[0], t[1], t[2]].map(|i| p.vertices[i as usize].pos);
                    let area = (b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1]);
                    assert!(area >= -1e-3, "a clockwise triangle");
                }
            }
            let inside = mesh.parts.iter().flat_map(|p| &p.vertices).all(|v| v.pos[0].abs() <= 150.0 && v.pos[1].abs() <= 95.0);
            assert!(inside, "{} seconds in, the cube fits its picture", cube.t);
        }
    }
}

