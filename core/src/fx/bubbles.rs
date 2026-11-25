//! Air bubbles rising through the water, in the "water seen from below"
//! style. Typing lets one or two go by the text cursor (Enter a burst and a
//! trail), the moving mouse one now and then, and the sand a few on its own
//! (scaled by `fx.ambient`). Each is one quad, which the quad shader lights
//! as a lens (`bubble_sample` in shaders/ui.wgsl).
//!
//! They move as real ones do for their size. Most are small: round, slow and
//! rising nearly straight. Bigger ones swing from side to side on the way up, and the
//! biggest flatten into wobbling ellipsoids, wider than tall and tilted across
//! their path, and rise fastest. All grow a little as the water's pressure
//! drops on the way up, then linger under the surface for a moment and pop.
//!
//! Positions are window pixels and sizes are points times the display scale.
//! Time is flow time, so the bubbles follow the flow speed setting.

use std::f32::consts::TAU;

use super::{FrameInput, Heard};
use crate::render::fluid::Splat;
use crate::render::Canvas;

/// The most bubbles in the water at once, waiting ones included.
const MAX_BUBBLES: usize = 160;
/// Radii when let go, in points.
const MIN_R: f32 = 1.3;
const MAX_R: f32 = 6.5;
/// How much sizes lean small: a radius is MIN_R + (MAX_R - MIN_R) × u^SKEW
/// for a uniform u.
const SKEW: f32 = 2.2;
/// Around this radius (points) a bubble's path starts to swing.
const SWING_R: f32 = 2.2;
/// From this radius (points) bubbles flatten, until one of MAX_R is
/// MAX_ASPECT times as wide as it is tall.
const FLAT_R: f32 = 2.8;
const MAX_ASPECT: f32 = 1.45;
/// How much a bubble grows on its way up the whole window.
const GROWTH: f32 = 0.12;
/// The surface, in points below the top of the window (`title` in
/// shade_water_below).
const SURFACE: f32 = 34.0;
/// Seconds a bubble takes to pop.
const POP: f32 = 0.14;
/// The light the bubbles mirror: the sky through the surface.
const SKY: [f32; 3] = [0.85, 0.96, 1.0];

struct Bubble {
    /// The middle of its path across (pixels); it swings around it.
    x0: f32,
    y: f32,
    /// Where it was let go (pixels), for how far it has risen.
    y0: f32,
    /// Radius when let go (pixels).
    r0: f32,
    /// Seconds since it was let go; below 0 it waits its turn in a stream.
    age: f32,
    /// Its own phase for the swing and the wobble.
    phase: f32,
    /// How wide its path swings, from 0.35 to 1 of the most for its size (a
    /// zigzag seen edge on looks narrower than a spiral), and how long its
    /// swings are, against the usual for its size.
    swing: f32,
    wave: f32,
    /// Seconds spent under the surface (below 0 while it rises), and how
    /// long it stays there before it pops.
    surfaced: f32,
    linger: f32,
    /// Its tilt and sideways speed (pixels a second) as it met the surface,
    /// which it eases out of there, and how far it bounces back down.
    tilt0: f32,
    vx0: f32,
    bounce: f32,
    pose: Pose,
}

/// How a bubble looks this frame, worked out along with its motion.
#[derive(Clone, Copy, Default)]
struct Pose {
    x: f32,
    /// Half its width and height (pixels).
    half: [f32; 2],
    /// Radians clockwise: flat bubbles turn across the way they move.
    tilt: f32,
    /// 0..1 as it pops.
    open: f32,
    /// It fades in as it comes loose.
    alpha: f32,
    /// The way to the light, the sun above the surface, on screen.
    light: [f32; 2],
}

pub struct Bubbles {
    list: Vec<Bubble>,
    rng: u32,
    /// The display scale of the last frame (pixels per point).
    s: f32,
}

impl Default for Bubbles {
    fn default() -> Self {
        Self::new()
    }
}

impl Bubbles {
    pub fn new() -> Self {
        Self { list: Vec::with_capacity(MAX_BUBBLES), rng: 0x6C8E_9CF5, s: 1.0 }
    }

    pub fn clear(&mut self) {
        self.list.clear();
    }

    /// Uniform in 0..1 (xorshift32).
    fn rand01(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        self.rng as f32 / u32::MAX as f32
    }

    /// A radius in points: `skew` above 1 leans small, below 1 big.
    fn random_r(&mut self, skew: f32) -> f32 {
        MIN_R + (MAX_R - MIN_R) * self.rand01().powf(skew)
    }

    /// Lets a bubble of radius `r` points go at (x, y), `delay` seconds from now.
    fn release(&mut self, x: f32, y: f32, r: f32, delay: f32) {
        if self.list.len() >= MAX_BUBBLES {
            return;
        }
        let phase = self.rand01() * TAU;
        let swing = 0.35 + 0.65 * self.rand01();
        let wave = 0.8 + 0.45 * self.rand01();
        // Big ones break sooner than small ones.
        let linger = (0.2 + 0.7 * self.rand01()) * (1.0 - 0.5 * smoothstep(FLAT_R, MAX_R, r));
        let pose = Pose::default();
        let r0 = r * self.s;
        self.list.push(Bubble {
            x0: x,
            y,
            y0: y,
            r0,
            age: -delay,
            phase,
            swing,
            wave,
            surfaced: -1.0,
            linger,
            tilt0: 0.0,
            vx0: 0.0,
            bounce: 0.0,
            pose,
        });
    }

    /// Up to four bubbles of mixed sizes, none over `most` points, from about
    /// (x, y) after `delay`. The biggest go first: they rise fastest, so each
    /// draws away from the next instead of running into it.
    fn stream(&mut self, x: f32, y: f32, n: usize, delay: f32, skew: f32, most: f32) {
        let mut radii = [0.0f32; 4];
        let n = n.min(radii.len());
        for r in &mut radii[..n] {
            *r = self.random_r(skew).min(most);
        }
        radii[..n].sort_by(|a, b| b.total_cmp(a));
        let mut at = delay;
        for &r in &radii[..n] {
            let dx = (self.rand01() - 0.5) * 4.0 * r * self.s;
            self.release(x + dx, y, r, at);
            at += spacing(r) * (1.0 + 0.4 * self.rand01());
        }
    }

    /// One frame: lets new bubbles go for `keys` typed, the mouse (when
    /// `at_mouse`) and the sand, moves them all, and stirs the water behind
    /// the biggest.
    pub fn frame(&mut self, f: &FrameInput, keys: &[((f32, f32), bool)], at_mouse: bool, splats: &mut Vec<Splat>, heard: &mut Vec<Heard>) {
        let s = f.scale.max(0.5);
        self.s = s;
        let (w, h) = (f.width.max(1.0), f.height.max(1.0));
        let dt = f.flow_dt.max(0.0);
        // Only water lets bubbles go, not the air above it (the tabs).
        let under = |(x, y): (f32, f32)| y > surface_y(x, h, s, f.flow_t) + 2.0 * MAX_R * s;
        // A key lets a bubble or two go by the text cursor, now and then a
        // bigger one. Enter lets a burst go: a big one, a middling one beside
        // it and a trail of small ones behind.
        for &((x, y), enter) in keys.iter().filter(|k| under(k.0)) {
            if enter {
                let (a, b) = (self.random_r(0.5), self.random_r(1.0));
                let (big, mid) = (a.max(b), a.min(b));
                self.release(x, y, big, 0.0);
                let side = if self.rand01() < 0.5 { -1.0 } else { 1.0 };
                self.release(x + side * (1.5 * (big + mid) + 2.5) * s, y, mid, 0.05);
                self.stream(x, y, 4, spacing(big), SKEW + 0.5, mid);
            } else {
                let n = 1 + (self.rand01() > 0.6) as usize;
                self.stream(x, y, n, 0.0, SKEW, MAX_R);
            }
        }
        if at_mouse && under(f.mouse) {
            let r = self.random_r(SKEW);
            self.release(f.mouse.0, f.mouse.1, r, 0.0);
        }
        // Now and then the sand lets a few go, one after another.
        if self.rand01() < f.ambient * 0.7 * dt {
            let x = self.rand01() * w;
            let y = h * (0.92 + 0.06 * self.rand01());
            let n = 1 + (self.rand01() * 3.5) as usize;
            self.stream(x, y, n, 0.0, SKEW + 0.5, MAX_R);
        }
        self.rise(f.flow_t, s, w, h, dt, heard);
        // Rising bubbles drag a little water up with them.
        let rising = self.list.iter().filter(|b| b.age >= 0.0 && b.surfaced < 0.0);
        if let Some(b) = rising.max_by(|a, b| a.r0.total_cmp(&b.r0)) {
            splats.push(Splat { pos: [b.pose.x / w, b.y / h], force: [0.0, -40.0 * 60.0 * dt], color: [0.0; 3], radius: 0.02 });
        }
    }

    /// Moves every bubble that is out on by `dt` and works out how it looks.
    /// Those that have popped are gone.
    fn rise(&mut self, t: f32, s: f32, w: f32, h: f32, dt: f32, heard: &mut Vec<Heard>) {
        let sun = [0.28 * w, -0.55 * h];
        for b in &mut self.list {
            b.age += dt;
            if b.age < 0.0 {
                continue;
            }
            let pr = b.r0 / s;
            let risen = (b.y0 - b.y).max(0.0);
            let r = b.r0 * (1.0 + GROWTH * (risen / h).min(1.0));
            // The bigger, the flatter, and the flat ones wobble and rock.
            let flat = smoothstep(FLAT_R, MAX_R, pr);
            let mut aspect = 1.0 + (MAX_ASPECT - 1.0) * flat;
            aspect *= 1.0 + 0.12 * flat * (b.age * TAU * (4.0 - 0.3 * pr) + b.phase * 3.0).sin();
            let mut tilt = 0.12 * flat * (b.age * TAU * 1.3 + b.phase).sin();
            let x;
            let (mut vx, mut up) = (0.0, 0.0);
            if b.surfaced < 0.0 {
                // Picking up speed as it comes loose.
                up = rise_speed(pr) * s * (0.4 + 0.6 * smoothstep(0.0, 0.3, b.age));
                b.y -= up * dt;
                // Small ones rise nearly straight, drifting a little. The
                // path of bigger ones swings, more and more once they are
                // clear of where they came from, leaning at most about 30
                // degrees, and its swings drift a little in length.
                let wave = (26.0 + 7.0 * pr) * b.wave * s;
                let k = risen / wave;
                let th = k * TAU + b.phase + 0.8 * (k * 1.7 + b.phase * 3.0).sin();
                let dth = (TAU + 1.36 * (k * 1.7 + b.phase * 3.0).cos()) / wave;
                let most = 0.55 * wave / (TAU + 1.36);
                let amp = (swing(pr) * s).min(most) * b.swing * smoothstep(0.0, wave, risen);
                let drift = 1.2 * s * (b.age * 0.6 + b.phase * 2.0).sin();
                x = b.x0 + amp * th.sin() + drift;
                let slope = amp * th.cos() * dth;
                vx = slope * up;
                tilt += 0.6 * flat * slope.atan();
            } else {
                // Meeting the surface it eases out of its swing: carried on
                // sideways a little, turned back level, pressed flatter.
                b.surfaced += dt;
                let settle = smoothstep(0.0, 0.15, b.surfaced);
                let carry = b.vx0 * (-b.surfaced / 0.07).exp();
                b.x0 += (carry + b.phase.sin() * 2.5 * s) * dt;
                aspect *= 1.0 + 0.15 * settle;
                tilt += (b.tilt0 - tilt) * (1.0 - settle);
                x = b.x0;
            }
            // It comes loose small and fades in. Flattened, it keeps its volume.
            let fade = smoothstep(0.0, 0.1, b.age);
            let size = r * (0.6 + 0.4 * fade);
            let mut half = [size * aspect.cbrt(), size / (aspect * aspect).cbrt()];
            // Once it reaches the surface, it hangs from it, after bouncing
            // back down a little: more the faster it came.
            let top = surface_y(x, h, s, t);
            if b.surfaced < 0.0 && b.y - half[1] <= top {
                b.surfaced = 0.0;
                heard.push(Heard::Bubble { x: x / w.max(1.0), size: (b.r0 / (MAX_R * s)).clamp(0.0, 1.0) });
                b.x0 = x;
                b.tilt0 = tilt;
                b.vx0 = vx;
                b.bounce = 0.4 * half[1] * smoothstep(20.0, 110.0, up / s);
            }
            // Popping, it spreads out along the surface as its film opens.
            let open = ((b.surfaced - b.linger) / POP).clamp(0.0, 1.0);
            half = [half[0] * (1.0 + 0.5 * open), half[1] * (1.0 - 0.2 * open)];
            if b.surfaced >= 0.0 {
                let bounce = b.bounce * (b.surfaced * TAU / 0.16).sin().abs() * (-b.surfaced / 0.08).exp();
                b.y = top + half[1] + bounce;
            }
            let to_sun = [sun[0] - x, sun[1] - b.y];
            let len = (to_sun[0] * to_sun[0] + to_sun[1] * to_sun[1]).sqrt().max(1.0);
            b.pose = Pose { x, half, tilt, open, alpha: fade, light: [to_sun[0] / len, to_sun[1] / len] };
        }
        self.list.retain(|b| b.surfaced < b.linger + POP);
    }

    /// The bubbles that are out, drawn behind everything else.
    pub fn draw(&self, c: &mut Canvas<'_>) {
        for b in self.list.iter().filter(|b| b.age >= 0.0) {
            let p = &b.pose;
            c.bubble([p.x, b.y], p.half, p.tilt, p.light, p.open, [SKY[0], SKY[1], SKY[2], p.alpha]);
        }
    }
}

/// How fast a bubble of radius `r` points rises, in points per second: the
/// small ones slowly, the big ones several times faster.
fn rise_speed(r: f32) -> f32 {
    26.0 + 2.0 * r * r
}

/// Seconds to wait after letting a bubble of radius `r` points go before the
/// next from the same spot, so that by then it is a couple of diameters on.
fn spacing(r: f32) -> f32 {
    4.0 * r / (0.7 * rise_speed(r))
}

/// How far to either side the path of a bubble of radius `r` points swings
/// at most, in points: not at all for small ones.
fn swing(r: f32) -> f32 {
    smoothstep(SWING_R - 0.6, SWING_R + 1.0, r) * (1.0 + 0.9 * r)
}

/// Where shade_water_below draws the surface at x (pixels), leaving out its
/// noise.
fn surface_y(x: f32, h: f32, s: f32, t: f32) -> f32 {
    let k = x / h;
    SURFACE * s + h * (0.006 * (k * 6.0 + t * 1.2).sin() + 0.003 * (k * 15.0 - t * 2.0).sin())
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(flow_dt: f32) -> FrameInput {
        FrameInput {
            flow_dt,
            flow_t: 10.0,
            dt: flow_dt,
            ambient: 0.0,
            rain: 0.0,
            electricity: 0.0,
            width: 1100.0,
            height: 700.0,
            scale: 1.0,
            focused: false,
            mouse: (0.0, 0.0),
            activity: 0.0,
            panel: None,
            sim_px: 0.2,
        }
    }

    fn step(bubbles: &mut Bubbles, f: &FrameInput) {
        bubbles.frame(f, &[], false, &mut Vec::new(), &mut Vec::new());
    }

    #[test]
    fn sizes_lean_small_with_a_few_big_ones() {
        let mut bubbles = Bubbles::new();
        let mut radii: Vec<f32> = (0..4000).map(|_| bubbles.random_r(SKEW)).collect();
        radii.sort_by(f32::total_cmp);
        let median = radii[radii.len() / 2];
        assert!(median < 2.6, "median radius {median}");
        let big = radii.iter().filter(|&&r| r > 4.5).count();
        assert!((200..1000).contains(&big), "{big} big ones in 4000");
    }

    #[test]
    fn small_bubbles_rise_slowly_and_straight_big_ones_fast_and_swinging() {
        let mut bubbles = Bubbles::new();
        bubbles.release(300.0, 650.0, 1.4, 0.0);
        bubbles.release(700.0, 650.0, 5.5, 0.0);
        bubbles.list[1].swing = 1.0;
        let f = input(1.0 / 60.0);
        // The steepest the path leans, sideways over up.
        let mut slant = [0.0f32; 2];
        step(&mut bubbles, &f);
        let mut last = [0, 1].map(|i| [bubbles.list[i].pose.x, bubbles.list[i].y]);
        for _ in 0..120 {
            step(&mut bubbles, &f);
            for (i, b) in bubbles.list.iter().enumerate() {
                let up = last[i][1] - b.y;
                if up > 0.0 {
                    slant[i] = slant[i].max((b.pose.x - last[i][0]).abs() / up);
                }
                last[i] = [b.pose.x, b.y];
            }
        }
        let (small, big) = (650.0 - bubbles.list[0].y, 650.0 - bubbles.list[1].y);
        assert!(big > small * 2.5, "the big one rose {big}, the small one {small}");
        assert!(slant[0] < 0.05, "the small one leans {}", slant[0]);
        assert!(slant[1] > 0.4, "the big one leans only {}", slant[1]);
    }

    #[test]
    fn big_bubbles_flatten_and_wobble_small_ones_stay_round() {
        let mut bubbles = Bubbles::new();
        bubbles.release(300.0, 650.0, 1.4, 0.0);
        bubbles.release(700.0, 650.0, 6.2, 0.0);
        let f = input(1.0 / 60.0);
        let (mut least, mut most) = (f32::MAX, 0.0f32);
        for i in 0..60 {
            step(&mut bubbles, &f);
            let small = bubbles.list[0].pose;
            assert!((small.half[0] / small.half[1] - 1.0).abs() < 1e-4);
            let big = bubbles.list[1].pose;
            if i > 10 {
                least = least.min(big.half[0] / big.half[1]);
                most = most.max(big.half[0] / big.half[1]);
                // Flattened, not shrunk: the same volume as a sphere.
                let r = (big.half[0] * big.half[0] * big.half[1]).cbrt();
                assert!(r > 6.2 && r < 6.2 * (1.0 + GROWTH), "radius {r}");
            }
        }
        assert!(least > 1.2, "only {least} times as wide as tall");
        assert!(most - least > 0.15, "wobbles from {least} to {most}");
    }

    #[test]
    fn bubbles_grow_a_little_as_they_rise() {
        let mut bubbles = Bubbles::new();
        bubbles.release(300.0, 690.0, 1.5, 0.0);
        let f = input(1.0 / 60.0);
        for _ in 0..10 {
            step(&mut bubbles, &f);
        }
        let low = bubbles.list[0].pose.half[0];
        while bubbles.list[0].y > 100.0 {
            step(&mut bubbles, &f);
        }
        let ratio = bubbles.list[0].pose.half[0] / low;
        assert!(ratio > 1.06 && ratio < 1.0 + GROWTH, "grew {ratio} times");
    }

    #[test]
    fn a_key_lets_a_short_stream_go_and_enter_a_varied_burst() {
        let mut bubbles = Bubbles::new();
        let f = input(1.0 / 60.0);
        bubbles.frame(&f, &[((400.0, 300.0), false)], false, &mut Vec::new(), &mut Vec::new());
        let n = bubbles.list.len();
        assert!((1..=2).contains(&n), "{n} bubbles for a key");
        assert!(bubbles.list.iter().all(|b| (b.x0 - 400.0).abs() < 2.0 * MAX_R && b.y0 == 300.0));

        bubbles.clear();
        bubbles.frame(&f, &[((400.0, 300.0), true)], false, &mut Vec::new(), &mut Vec::new());
        assert_eq!(bubbles.list.len(), 6);
        let radii: Vec<f32> = bubbles.list.iter().map(|b| b.r0).collect();
        let (least, most) = (radii.iter().copied().fold(f32::MAX, f32::min), radii.iter().copied().fold(0.0, f32::max));
        assert!(most > least * 1.8, "radii {radii:?}");
        // The big one goes first, and the trail waits its turn.
        assert_eq!(radii[0], most, "radii {radii:?}");
        let waiting = bubbles.list.iter().filter(|b| b.age < 0.0).count();
        assert!(waiting >= 4, "{waiting} waiting");
    }

    #[test]
    fn a_bubble_reaching_the_surface_is_heard_there_once() {
        let mut bubbles = Bubbles::new();
        let f = input(1.0 / 60.0);
        bubbles.frame(&f, &[((400.0, 300.0), false)], false, &mut Vec::new(), &mut Vec::new());
        let released = bubbles.list.len();
        let mut heard = Vec::new();
        for _ in 0..600 {
            bubbles.frame(&f, &[], false, &mut Vec::new(), &mut heard);
        }
        assert_eq!(heard.len(), released, "{heard:?}");
        assert!(heard.iter().all(|h| matches!(h, Heard::Bubble { x, size } if (*x - 400.0 / f.width).abs() < 0.05 && (0.0..=1.0).contains(size))), "{heard:?}");
    }

    #[test]
    fn bubbles_of_a_burst_or_stream_never_run_into_each_other() {
        for seed in [1u32, 7, 99, 1234, 0xBEEF] {
            let mut bubbles = Bubbles::new();
            bubbles.rng = seed;
            let f = input(1.0 / 60.0);
            bubbles.frame(&f, &[((400.0, 600.0), true), ((700.0, 600.0), false)], false, &mut Vec::new(), &mut Vec::new());
            for _ in 0..240 {
                step(&mut bubbles, &f);
                let out: Vec<&Bubble> = bubbles.list.iter().filter(|b| b.age > 0.0 && b.surfaced < 0.0).collect();
                for (i, a) in out.iter().enumerate() {
                    for b in &out[i + 1..] {
                        let d = ((a.pose.x - b.pose.x).powi(2) + (a.y - b.y).powi(2)).sqrt();
                        let reach = a.pose.half[0].max(a.pose.half[1]) + b.pose.half[0].max(b.pose.half[1]);
                                assert!(d > reach, "seed {seed}: bubbles {d} apart overlap");
                    }
                }
            }
        }
    }

    #[test]
    fn bubbles_come_from_the_water_not_the_air_above_it() {
        let mut bubbles = Bubbles::new();
        let mut f = input(1.0 / 60.0);
        // The tabs, over the surface.
        f.mouse = (500.0, 20.0);
        bubbles.frame(&f, &[], true, &mut Vec::new(), &mut Vec::new());
        assert!(bubbles.list.is_empty());
        f.mouse = (500.0, 400.0);
        bubbles.frame(&f, &[], true, &mut Vec::new(), &mut Vec::new());
        assert_eq!(bubbles.list.len(), 1);
    }

    #[test]
    fn the_count_stays_bounded() {
        let mut bubbles = Bubbles::new();
        let mut f = input(1.0 / 60.0);
        f.ambient = 1.0;
        f.mouse = (500.0, 400.0);
        for i in 0..1200 {
            let keys = [((300.0 + (i % 50) as f32 * 8.0, 500.0), i % 7 == 0)];
            bubbles.frame(&f, &keys, i % 3 == 0, &mut Vec::new(), &mut Vec::new());
            assert!(bubbles.list.len() <= MAX_BUBBLES);
        }
        assert!(bubbles.list.len() > MAX_BUBBLES / 2);
    }

    #[test]
    fn bubbles_linger_under_the_surface_then_pop() {
        let mut bubbles = Bubbles::new();
        bubbles.release(500.0, 200.0, 3.0, 0.0);
        let f = input(1.0 / 60.0);
        let (mut at_surface, mut opened) = (0, 0.0f32);
        for _ in 0..600 {
            step(&mut bubbles, &f);
            let Some(b) = bubbles.list.first() else { break };
            // Never through the surface.
            let below = b.y - b.pose.half[1] - surface_y(b.pose.x, 700.0, 1.0, f.flow_t);
            assert!(below > -0.01, "{below} through the surface");
            if b.surfaced >= 0.0 {
                at_surface += 1;
                // Hanging from it by the time it pops.
                if b.pose.open > 0.0 {
                    assert!(below < 0.05 * b.pose.half[1], "{below} under the surface");
                }
                assert!(b.pose.open >= opened);
                opened = b.pose.open;
            }
        }
        assert!(bubbles.list.is_empty(), "never popped");
        assert!(at_surface >= 15, "only {at_surface} frames at the surface");
        assert!(opened > 0.8);
    }

    #[test]
    fn a_swinging_bubble_eases_into_the_surface_without_a_jolt() {
        let mut bubbles = Bubbles::new();
        bubbles.release(500.0, 300.0, 6.0, 0.0);
        bubbles.list[0].swing = 1.0;
        let f = input(1.0 / 60.0);
        step(&mut bubbles, &f);
        let mut last: Option<Pose> = None;
        let (mut turn, mut squash, mut slide, mut hit) = (0.0f32, 0.0f32, 0.0f32, false);
        while let Some(b) = bubbles.list.first() {
            if b.pose.open > 0.0 {
                break;
            }
            if let Some(l) = last {
                turn = turn.max((b.pose.tilt - l.tilt).abs());
                squash = squash.max((b.pose.half[0] / b.pose.half[1] - l.half[0] / l.half[1]).abs());
                slide = slide.max((b.pose.x - l.x).abs());
            }
            hit |= b.surfaced >= 0.0;
            last = Some(b.pose);
            step(&mut bubbles, &f);
        }
        assert!(hit, "never reached the surface");
        assert!(turn < 0.05, "turned {turn} radians in a frame");
        assert!(squash < 0.06, "squashed {squash} in a frame");
        assert!(slide < 2.0, "slid {slide} pixels in a frame");
    }

    #[test]
    fn slower_flow_slower_bubbles() {
        let risen = |flow_dt: f32| {
            let mut bubbles = Bubbles::new();
            bubbles.release(500.0, 600.0, 3.0, 0.0);
            for _ in 0..30 {
                step(&mut bubbles, &input(flow_dt));
            }
            600.0 - bubbles.list[0].y
        };
        let (normal, slow) = (risen(1.0 / 60.0), risen(0.2 / 60.0));
        assert!(slow > 0.0 && slow < normal * 0.3, "rose {slow} slowed against {normal}");
    }
}
