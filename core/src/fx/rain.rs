//! Rain on the window glass, the "rain" liquid style. Drops bead on the
//! glass, slowly dry up, and merge when they touch. Once heavy enough they
//! run down, catching now and then and going on at a new slant. On the way
//! they take in every drop they run into and the film of water in the fog
//! they clear, so they grow and speed up as they go, and leave a thin clear
//! trail behind, beaded with tiny drops. Small drops are round; bigger ones
//! sag, fuller at the bottom.
//! The cursor wipes a clear path, pushing drops aside, typing lands new ones
//! by the text cursor, and the rain keeps coming, as hard as `fx.rain`
//! says: from a drizzle through rain to a downpour ([`rate`]). Each new drop
//! strikes: flattened into a splat that throws a ring of spray, it gathers
//! into its bead, or, big enough, bursts and runs at once. Water follows
//! water: a runner near an earlier one's wet trail is drawn onto it and
//! runs down it without catching or leaving beads, each keeping the track
//! open and wetter, so a track many run down becomes a stream, the drops on
//! it catching up and joining. As it pours the
//! glass gets wet all over ([`wetness`]): runners hold their course in
//! rivulets, the fog's film of water thickens, and a clear path closes over
//! again quickly; the glass shader runs the water down it in sheets.
//!
//! Positions are window pixels and sizes are points times the display scale.
//! Time is flow time, so the rain follows the flow speed setting.

use super::{FrameInput, Heard};
use crate::render::rain::RainQuad;

/// Drops smaller than this (points) have dried up.
const MIN_R: f32 = 0.8;
/// From this radius (points) a drop is too heavy to cling, and runs.
const RUN_R: f32 = 5.5;
/// Drops don't grow past this (points); the rest runs off.
const MAX_R: f32 = 13.0;
/// The water in the fog, as a film this many points thick. Runners sweep it
/// up across their clear trail, so they grow on the way down until the beads
/// they leave behind take as much as they gather.
const MIST: f32 = 0.2;
/// How fast clinging drops dry up, in points per second.
const DRY: f32 = 0.012;
/// The most drops kept on the glass; past it, the smallest go.
const MAX_DROPS: usize = 700;
/// Half the width of the path the cursor wipes, in points.
const WIPE_R: f32 = 15.0;
/// Radius of the beads of water the cursor pushes up along the sides of its
/// path, in points.
const RIDGE: f32 = 1.0;
/// Seconds until a clear path has fogged over again; it starts to after a
/// quarter of that.
const STREAK_LIFE: f32 = 12.0;
const MAX_STREAKS: usize = 600;
/// New drops per second for a 1100 × 700 point window where Rain's amount
/// reaches [`DRIZZLE_AT`], as the drizzle always fell at its fullest.
const DRIZZLE: f32 = 3.0;
const DRIZZLE_AT: f32 = 0.2;
/// How many times the drizzle's drops a downpour brings, at Rain's top.
const DOWNPOUR: f32 = 50.0;
/// Seconds a new drop takes to strike: a splat throwing its spray, then
/// gathered into its bead.
const IMPACT: f32 = 0.16;
/// From this much of [`RUN_R`] a drop striking the glass bursts and runs at
/// once: over the 0.85 a runner clings again below, so it runs on rather
/// than stopping a frame later and splatting where it stopped.
const BURST: f32 = 0.9;
/// The most drops a second heard as taps; past it the rain's hiss, laid by
/// the app, carries the rest.
const TAPS: f32 = 8.0;
/// Drops already on the glass when the rain starts, per 100 × 100 points.
const START_DENSITY: f32 = 3.5;
/// How tall a drop is for its radius: a flattened dome.
const DOME: f32 = 0.55;
/// Half the width of the clear trail behind a runner, for its radius.
const TRAIL: f32 = 0.42;
/// How much wider a stream's clear channel is than a runner's trail, at
/// its wettest (`Streak::flow`, or the glass all wet).
const STREAM: f32 = 0.8;
/// From this radius (points) clinging drops start to sag.
const SAG_R: f32 = 2.5;

/// New drops a second for a 1100 × 700 point window at Rain's `amount`
/// (0..1): none to the drizzle's [`DRIZZLE`] at [`DRIZZLE_AT`], then
/// climbing evenly in ratio to [`DOWNPOUR`] times that at the top.
pub fn rate(amount: f32) -> f32 {
    let a = amount.clamp(0.0, 1.0);
    if a <= DRIZZLE_AT {
        DRIZZLE * a / DRIZZLE_AT
    } else {
        DRIZZLE * DOWNPOUR.powf((a - DRIZZLE_AT) / (1.0 - DRIZZLE_AT))
    }
}

/// How wet the glass is all over at Rain's `amount`, 0..1: not at all until
/// heavy rain, all of it in a downpour, where the water runs in sheets.
pub fn wetness(amount: f32) -> f32 {
    smoothstep(0.55, 1.0, amount)
}

/// How loud the rain's hiss on the glass is at Rain's `amount`, 0..1: from
/// rain on, as the taps come too fast to tell apart.
pub fn hiss(amount: f32) -> f32 {
    smoothstep(0.3, 1.0, amount)
}

/// How stormy it is at Rain's `amount`, 0..1: no storm in a drizzle, a
/// thunderstorm from heavy rain on (with Electricity up: `Effects`' storm).
pub fn storminess(amount: f32) -> f32 {
    smoothstep(0.3, 0.8, amount)
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[derive(Clone, Debug)]
struct Drop {
    pos: [f32; 2],
    r: f32,
    /// Seconds since it struck the glass; [`IMPACT`] or more once it has
    /// gathered into its bead.
    landed: f32,
    /// Where its spray goes, 0..1.
    seed: f32,
    /// It struck heavy enough to burst, throwing its spray farther.
    burst: bool,
    /// How wet the track it runs down is (a streak's `flow`), 0 off one.
    flow: f32,
    /// The wiped path that last held it back (its start), which it has
    /// broken through.
    dammed: Option<[f32; 2]>,
    /// Downward speed in pixels per second; 0 while it clings.
    speed: f32,
    /// Seconds a caught runner waits before it moves on.
    pause: f32,
    /// Sideways pixels per pixel down while it runs.
    drift: f32,
    /// Pixels run since it last left a drop behind.
    run: f32,
    /// Where the clear trail behind it currently starts.
    trail: [f32; 2],
}

impl Drop {
    fn new(pos: [f32; 2], r: f32, drift: f32) -> Self {
        Self { pos, r, landed: IMPACT, seed: 0.0, burst: false, flow: 0.0, dammed: None, speed: 0.0, pause: 0.0, drift, run: 0.0, trail: pos }
    }

    /// Whether it is still striking the glass.
    fn striking(&self) -> bool {
        self.landed < IMPACT
    }

    fn volume(&self) -> f32 {
        self.r.powi(3)
    }
}

/// A clear path through the fogged glass, fading back to fog.
#[derive(Clone, Debug)]
struct Streak {
    a: [f32; 2],
    b: [f32; 2],
    r: f32,
    /// Radius of the beads of water along its sides (pixels): wiped paths
    /// have some.
    ridge: f32,
    /// A runner's trail or a wiped path, which runners are drawn to and
    /// run down where it slants down the glass.
    track: bool,
    /// How much water has run down it since, 0..1: a well-run track is a
    /// stream, with water in its middle as on wet glass (a rivulet).
    flow: f32,
    age: f32,
}

pub struct Rain {
    drops: Vec<Drop>,
    streaks: Vec<Streak>,
    rng: u32,
    /// The cursor on the last frame, and where the current wiped path starts.
    mouse: Option<[f32; 2]>,
    wipe_from: Option<[f32; 2]>,
    /// The glass has been wetted for this rain (it starts already wet).
    started: bool,
    /// The display scale of the last frame (pixels per point).
    s: f32,
    /// How thick the fog's film of water is (points); [`MIST`] but for tests.
    mist: f32,
    /// How wet the glass is all over, from how hard it rains ([`wetness`]).
    wet: f32,
}

impl Default for Rain {
    fn default() -> Self {
        Self::new()
    }
}

impl Rain {
    pub fn new() -> Self {
        Self {
            drops: Vec::new(),
            streaks: Vec::new(),
            rng: 0x2545_F491,
            mouse: None,
            wipe_from: None,
            started: false,
            s: 1.0,
            mist: MIST,
            wet: 0.0,
        }
    }

    /// Dries the glass, e.g. after switching styles. The next frame starts a
    /// fresh, already wet window.
    pub fn clear(&mut self) {
        self.drops.clear();
        self.streaks.clear();
        self.mouse = None;
        self.wipe_from = None;
        self.started = false;
    }

    /// Uniform in 0..1 (xorshift32).
    fn rand01(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        self.rng as f32 / u32::MAX as f32
    }

    /// One frame of rain: new drops, the cursor's wipe (`moved` is how far it
    /// really moved), runners, drying. Appends the shapes to draw to `out`.
    pub fn frame(&mut self, f: &FrameInput, keys: &[((f32, f32), bool)], moved: f32, out: &mut Vec<RainQuad>, heard: &mut Vec<Heard>) {
        let s = f.scale.max(0.5);
        self.s = s;
        let (w, h) = (f.width.max(1.0), f.height.max(1.0));
        let dt = f.flow_dt.max(0.0);
        let amount = f.rain;
        self.wet = wetness(amount);
        if !self.started {
            self.started = true;
            self.scatter(w, h, s, amount);
        }
        // Drops left outside after the window shrank are gone.
        self.drops.retain(|d| d.pos[0] > -d.r && d.pos[0] < w + d.r && d.pos[1] < h + d.r);

        // The rain, as many drops a second for a bigger window. Each is
        // heard as a tap until they come too fast to tell apart.
        let area = (w / s) * (h / s) / (1100.0 * 700.0);
        let per_second = rate(amount) * area;
        let tapped = (TAPS / per_second.max(1e-6)).min(1.0);
        let mut due = per_second * dt;
        while due > 0.0 {
            if self.rand01() < due {
                let d = self.random_drop(w, h, s);
                let d = self.strike(d, s);
                if tapped >= 1.0 || self.rand01() < tapped {
                    heard.push(Heard::Raindrop { x: d.pos[0] / w, size: (d.r / (7.5 * s)).clamp(0.0, 1.0) });
                }
                self.add(d);
            }
            due -= 1.0;
        }
        // Typing lands a drop next to the text cursor; Enter a heavy one.
        for &((x, y), enter) in keys {
            let (r, spread) = if enter { (6.5, 3.0) } else { (2.2 + 1.2 * self.rand01(), 6.0) };
            let pos = [x + (self.rand01() - 0.5) * 2.0 * spread * s, y + (self.rand01() - 0.5) * 1.4 * spread * s];
            let drift = self.random_drift(0.3);
            let d = self.strike(Drop::new(pos, r * s, drift), s);
            self.add(d);
        }

        // The cursor wipes. `moved` filters out jumps, such as the cursor
        // coming back into the window somewhere else.
        let mouse = f.focused.then_some([f.mouse.0, f.mouse.1]);
        match (self.mouse, mouse) {
            (Some(a), Some(b)) if moved > 0.0 && dist(a, b) <= moved * 1.5 + 2.0 => self.wipe(a, b, WIPE_R * s),
            _ => self.wipe_from = None,
        }
        self.mouse = mouse;

        self.run_all(dt, s, h);

        // Drops striking the glass gather into their beads.
        self.settle(dt, s, per_second);
        // Clinging drops dry up; clear paths fog over.
        let min_r = MIN_R * s;
        for d in &mut self.drops {
            if d.speed == 0.0 {
                d.r -= DRY * s * dt;
            }
        }
        self.drops.retain(|d| d.r >= min_r);
        // More of them while it pours.
        let most = MAX_DROPS + (MAX_DROPS as f32 * 1.2 * self.wet) as usize;
        if self.drops.len() > most {
            self.drops.sort_unstable_by(|a, b| b.r.total_cmp(&a.r));
            self.drops.truncate(most);
        }
        // Wet glass fogs over again sooner.
        for st in &mut self.streaks {
            st.age += dt * (1.0 + 3.0 * self.wet);
        }
        self.streaks.retain(|st| st.age < STREAK_LIFE);
        // Past the most, the oldest go: by age, as tracks still being run
        // down stay young.
        if self.streaks.len() > MAX_STREAKS {
            self.streaks.sort_by(|a, b| a.age.total_cmp(&b.age));
            self.streaks.truncate(MAX_STREAKS);
        }

        self.draw(out);
    }

    /// The glass when the rain starts: already wet, with drops of all sizes
    /// and a runner or two, more the harder it rains.
    fn scatter(&mut self, w: f32, h: f32, s: f32, amount: f32) {
        let n = ((w / s) * (h / s) / 10_000.0 * START_DENSITY * (0.35 + (amount / DRIZZLE_AT).min(1.65))) as usize;
        for _ in 0..n.min(MAX_DROPS / 2) {
            let d = self.random_drop(w, h, s);
            self.add(d);
        }
    }

    /// A drop of rain landing anywhere: mostly fine ones, a third of them
    /// bigger, now and then a heavy one.
    fn random_drop(&mut self, w: f32, h: f32, s: f32) -> Drop {
        let u = self.rand01();
        let r = match self.rand01() {
            k if k < 0.06 => 4.5 + 3.0 * u,
            k if k < 0.4 => 2.2 + 2.3 * u,
            _ => 0.9 + 1.3 * u,
        };
        let pos = [self.rand01() * w, self.rand01() * h];
        let drift = self.random_drift(0.3);
        Drop::new(pos, r * s, drift)
    }

    /// A runner's way down: mostly straight, slanting a little either way.
    fn random_drift(&mut self, most: f32) -> f32 {
        (self.rand01() - 0.5) * 2.0 * most
    }

    /// `d` striking the glass rather than lying on it: its splat and spray
    /// to come, and, heavy enough, bursting into a run at once.
    fn strike(&mut self, mut d: Drop, s: f32) -> Drop {
        d.landed = 0.0;
        d.seed = self.rand01();
        if d.r >= RUN_R * BURST * s {
            d.burst = true;
            d.speed = d.speed.max(40.0 * s);
        }
        d
    }

    /// Drops striking the glass go on gathering into their beads; as one's
    /// spray settles, a droplet or two of it may stay, fewer the harder it
    /// rains so they don't crowd out the rest.
    fn settle(&mut self, dt: f32, s: f32, per_second: f32) {
        let mut settled = Vec::new();
        for d in &mut self.drops {
            if d.striking() {
                d.landed += dt;
                if !d.striking() {
                    settled.push(d.clone());
                }
            }
        }
        let keep = 0.35 / (1.0 + per_second / 20.0);
        for d in settled {
            for (pos, r) in spray(&d, 1.0, s).collect::<Vec<_>>() {
                if self.rand01() < keep {
                    self.add(Drop::new(pos, r.max(MIN_R * s * 1.2), 0.0));
                }
            }
        }
    }

    /// Puts a drop on the glass, merging it with any it touches.
    fn add(&mut self, mut d: Drop) {
        // On wet glass they run together sooner.
        let reach = 0.8 + 0.2 * self.wet;
        let mut i = 0;
        while i < self.drops.len() {
            if touching(&self.drops[i], &d, reach) {
                let other = self.drops.swap_remove(i);
                d = merge(d, other, MAX_R * self.s);
                // It grew, so it may reach drops already checked.
                i = 0;
            } else {
                i += 1;
            }
        }
        self.drops.push(d);
    }

    /// The cursor moved from `a` to `b`: drops on its path are pushed to the
    /// sides, where they may run together, and the glass behind is clear.
    fn wipe(&mut self, a: [f32; 2], b: [f32; 2], wr: f32) {
        let side = normalize(perp(sub(b, a)));
        let mut pushed = Vec::new();
        let mut i = 0;
        while i < self.drops.len() {
            let d = &self.drops[i];
            if dist(d.pos, closest(a, b, d.pos)) < wr + d.r * 0.5 {
                // Straight out to the side it is on, not along the path, so
                // no water piles up ahead of the cursor.
                let lat = dot(sub(d.pos, a), side);
                let dir = if lat < 0.0 { scale(side, -1.0) } else { side };
                let mut d = self.drops.swap_remove(i);
                d.pos = add(d.pos, scale(dir, wr + d.r * 0.8 - lat.abs()));
                d.trail = d.pos;
                pushed.push(d);
            } else {
                i += 1;
            }
        }
        for d in pushed {
            self.add(d);
        }
        let from = *self.wipe_from.get_or_insert(a);
        if dist(from, b) > wr * 0.6 {
            // An earlier wipe's beads of water that this one crosses are
            // wiped away too (but not those of the piece it carries on from).
            for st in &mut self.streaks {
                let mid = scale(add(st.a, st.b), 0.5);
                if st.ridge > 0.0 && st.b != from && dist(mid, closest(from, b, mid)) < wr * 0.85 {
                    st.ridge = 0.0;
                }
            }
            self.streaks.push(Streak { a: from, b, r: wr * 0.85, ridge: RIDGE * self.s, track: true, flow: 0.0, age: 0.0 });
            self.wipe_from = Some(b);
        }
    }

    /// Moves every drop heavy enough to run, and lets runners pick up what
    /// they touch. Runners leaving the bottom of the window are gone.
    fn run_all(&mut self, dt: f32, s: f32, h: f32) {
        let run_r = RUN_R * s;
        let mut runners = Vec::new();
        let mut i = 0;
        while i < self.drops.len() {
            if self.drops[i].r >= run_r || self.drops[i].speed > 0.0 {
                runners.push(self.drops.swap_remove(i));
            } else {
                i += 1;
            }
        }
        for mut d in runners {
            let from = d.pos;
            let bead = self.run(&mut d, dt, s);
            // Every drop it ran into on the way, not only those it touches
            // where it stopped, so a fast one doesn't skip over any.
            let mut j = 0;
            while j < self.drops.len() {
                let other = &self.drops[j];
                if dist(other.pos, closest(from, d.pos, other.pos)) < (other.r + d.r) * 0.85 {
                    let other = self.drops.swap_remove(j);
                    d = merge(d, other, MAX_R * s);
                } else {
                    j += 1;
                }
            }
            // The bead it just left goes down after that, or it would take
            // it straight back.
            self.drops.extend(bead);
            if d.pos[1] - d.r < h {
                self.drops.push(d);
            } else {
                self.streak(d.trail, d.pos, d.r * TRAIL, d.flow);
            }
        }
    }

    /// Where a runner going from `from` to `to` meets a path wiped across
    /// the glass (more across it than down), and that path's start; none
    /// where it meets none, or only `passed`, which it broke through.
    fn dam_across(&self, from: [f32; 2], to: [f32; 2], passed: Option<[f32; 2]>) -> Option<([f32; 2], [f32; 2])> {
        self.streaks
            .iter()
            .filter(|st| st.ridge > 0.0 && (st.b[0] - st.a[0]).abs() > (st.b[1] - st.a[1]).abs() && passed != Some(st.a))
            .find_map(|st| crossing(from, to, st.a, st.b).map(|at| (at, st.a)))
    }

    /// The wet track a runner at `pos` of radius `r` is drawn to: the
    /// nearest piece of an earlier runner's trail running on down below it,
    /// within reach to either side; and where on it to make for, a little
    /// way down, and how far to the side it is.
    fn track_below(&self, pos: [f32; 2], r: f32, s: f32) -> Option<(usize, [f32; 2], f32)> {
        let look = (r * 3.0).max(12.0 * s);
        let reach = r * 2.0 + 6.0 * s;
        let y = pos[1] + look;
        let mut best: Option<(usize, [f32; 2], f32)> = None;
        for (i, st) in self.streaks.iter().enumerate() {
            if !st.track {
                continue;
            }
            let (top, bottom) = if st.a[1] <= st.b[1] { (st.a, st.b) } else { (st.b, st.a) };
            // Running on below the drop (not its own trail, behind it), and
            // more down the glass than across it.
            if bottom[1] <= pos[1] + r * 0.5 || top[1] > y || bottom[1] - top[1] < (bottom[0] - top[0]).abs() {
                continue;
            }
            let t = ((y.min(bottom[1]) - top[1]) / (bottom[1] - top[1]).max(1e-3)).clamp(0.0, 1.0);
            let at = [top[0] + (bottom[0] - top[0]) * t, top[1] + (bottom[1] - top[1]) * t];
            let side = (at[0] - pos[0]).abs();
            if side < reach && best.is_none_or(|(_, _, nearest)| side < nearest) {
                best = Some((i, at, side));
            }
        }
        best
    }

    /// One step of a runner: faster the heavier it is, slanting a little,
    /// catching on the glass now and then, sweeping up the fog's water as it
    /// goes, and leaving tiny beads of water behind along its trail. Returns
    /// the bead it leaves this step, if any.
    fn run(&mut self, d: &mut Drop, dt: f32, s: f32) -> Option<Drop> {
        if d.pause > 0.0 {
            // Caught: it slows to a stop, drawn in behind it as it does,
            // rather than snapping short.
            d.pause -= dt;
            d.speed *= (-dt * 10.0).exp();
            return None;
        }
        let run_r = RUN_R * s;
        // Water follows water: near a wet track below, it makes for it, and
        // on it runs down it, keeping it open and wetter.
        let track = self.track_below(d.pos, d.r, s);
        let on_track = track.is_some_and(|(_, _, side)| side < d.r * 0.6);
        d.flow = 0.0;
        if let Some((i, at, _)) = track {
            // Water runs down: drawn aside, it slants at most so far.
            let want = ((at[0] - d.pos[0]) / (at[1] - d.pos[1]).max(1.0)).clamp(-0.6, 0.6);
            d.drift += (want - d.drift) * (1.0 - (-dt * 8.0).exp());
            if on_track {
                let st = &mut self.streaks[i];
                st.flow = (st.flow + dt * 0.4).min(1.0);
                st.age = st.age.min(STREAK_LIFE * 0.15);
                d.flow = st.flow;
            }
        }
        // Down a wet track it runs freely.
        let free = if on_track { 1.4 } else { 1.0 };
        let target = 90.0 * s * free * ((d.r / run_r - 0.9) * 1.6).clamp(0.12, 3.0);
        d.speed += (target - d.speed) * (1.0 - (-dt * 5.0).exp());
        let dy = d.speed * dt;
        // On wet glass, or a wet track, it wavers less, holding its course.
        let steady = if on_track { 0.0 } else { 1.0 - 0.6 * self.wet };
        let from = d.pos;
        d.pos[0] += (d.drift + (self.rand01() - 0.5) * 0.25 * steady) * dy;
        d.pos[1] += dy;
        // A path wiped across its way holds it back a moment, by the water
        // pushed up along its edge, before it breaks through.
        if let Some((at, start)) = self.dam_across(from, d.pos, d.dammed) {
            d.pos = [at[0], at[1] - d.r * 0.5];
            d.pause = 0.2 + 0.3 * self.rand01();
            d.dammed = Some(start);
            return None;
        }
        d.run += dy;
        // The fog's film of water thickens as it pours.
        let film = self.mist * (1.0 + 3.0 * self.wet) * s * 2.0 * d.r * TRAIL * dy;
        d.r = (d.volume() + film).cbrt().min(MAX_R * s);
        let mut bead = None;
        // Small runners catch far more often than heavy ones, and go on from
        // the snag in a new direction.
        // On wet glass it seldom catches, and goes on much as it was.
        // Drawn to a track, it makes for it without catching on the way.
        if track.is_none() && self.rand01() < 0.9 * (run_r * 1.25 / d.r).powi(3) * dt * (1.0 - 0.85 * self.wet) {
            d.pause = 0.12 + 0.4 * self.rand01();
            d.drift = self.random_drift(0.35 * (1.0 - 0.7 * self.wet));
        }
        if on_track {
            // The water stays in the track: no beads.
            d.run = d.run.min(0.0);
        } else if d.run > (d.r * 1.1).max(4.0 * s) {
            // The next one comes soon or, now and then, much later, so they
            // don't line up like beads on a string. Most are tiny, a few
            // bigger.
            d.run = -d.r * 2.5 * self.rand01().powi(3);
            let k = if self.rand01() < 0.1 { 0.3 + 0.1 * self.rand01() } else { 0.12 + 0.14 * self.rand01() };
            let tr = (d.r * k).max(MIN_R * s * 1.2);
            // Left just out of reach, so the runner doesn't take it back, and
            // a little to one side of the trail's middle.
            let side = (self.rand01() - 0.5) * d.r * 0.7;
            let at = [d.pos[0] + side, d.pos[1] - (d.r + tr) * 1.15];
            d.r = (d.volume() - tr.powi(3)).max(0.0).cbrt();
            bead = Some(Drop::new(at, tr, 0.0));
        }
        let end = [d.pos[0], d.pos[1] - d.r * 0.5];
        if dist(d.trail, end) > (d.r * 3.0).max(10.0 * s) {
            self.streak(d.trail, end, d.r * TRAIL, d.flow);
            d.trail = end;
        }
        if d.r < run_r * 0.85 {
            // Too light now: it clings again, its trail ending behind it.
            self.streak(d.trail, end, d.r * TRAIL, d.flow);
            d.speed = 0.0;
            d.trail = d.pos;
        }
        bead
    }

    /// The clear trail behind a runner, a track for others, as wet as the
    /// one it ran down (`flow`).
    fn streak(&mut self, a: [f32; 2], b: [f32; 2], r: f32, flow: f32) {
        self.streaks.push(Streak { a, b, r, ridge: 0.0, track: true, flow, age: 0.0 });
    }

    /// The shapes for the rain map: clear paths, then the drops.
    fn draw(&self, out: &mut Vec<RainQuad>) {
        for st in &self.streaks {
            let k = (st.age / STREAK_LIFE - 0.25) / 0.75;
            let fog = k.clamp(0.0, 1.0);
            let clear = 1.0 - fog * fog * (3.0 - 2.0 * fog);
            // A stream (many have run down it) or a rivulet (on wet glass)
            // is a wider clear channel. Not lens water down its middle: where
            // its pieces overlapped, their lenses mixed into ones no drop
            // has, which focused the lights behind into white specks. A
            // wiped path is as wide as it was wiped.
            let water = if st.track && st.ridge == 0.0 { self.wet.max(st.flow) } else { 0.0 };
            out.push(RainQuad { ends: [st.a[0], st.a[1], st.b[0], st.b[1]], params: [st.r * (1.0 + STREAM * water), st.ridge, RainQuad::STREAK, clear] });
        }
        for d in &self.drops {
            if d.speed > 0.0 {
                // The path it is clearing right now, as wide as the stream
                // or rivulet it runs down.
                let water = self.wet.max(d.flow);
                out.push(RainQuad {
                    ends: [d.trail[0], d.trail[1], d.pos[0], d.pos[1]],
                    params: [d.r * TRAIL * (1.0 + STREAM * water), 0.0, RainQuad::STREAK, 1.0],
                });
            }
            if d.striking() {
                // Striking: its spray flying out, and, lying where it
                // struck, a splat, flattened with its rim thrown up, that
                // gathers into its bead.
                let k = d.landed / IMPACT;
                for (pos, r) in spray(d, k, self.s) {
                    out.push(RainQuad { ends: [pos[0], pos[1], pos[0], pos[1]], params: [r, r, RainQuad::SPRAY, DOME * (1.0 - 0.4 * k)] });
                }
                if d.speed == 0.0 {
                    let e = ease(k);
                    out.push(RainQuad {
                        ends: [d.pos[0], d.pos[1], d.pos[0], d.pos[1]],
                        params: [d.r * (1.0 + 0.8 * (1.0 - e)), d.seed, RainQuad::SPLAT, DOME * (0.25 + 0.75 * e)],
                    });
                    continue;
                }
            }
            // Small drops are round. Bigger ones sag: fuller below their
            // middle and drawn out above it, back from the way they will run,
            // so no two look quite alike. Running draws them out further
            // behind their round head into a thinner tail, the longer the
            // faster they go.
            let sag = ((d.r / self.s - SAG_R) / (RUN_R - SAG_R)).clamp(0.0, 1.0);
            let run = (d.speed * 0.06).min(d.r * 1.8);
            let head = [d.pos[0], d.pos[1] + d.r * 0.1 * sag];
            let tail = sub(head, scale(normalize([d.drift, 1.0]), d.r * 0.45 * sag + run));
            out.push(RainQuad {
                ends: [head[0], head[1], tail[0], tail[1]],
                params: [d.r, (d.r * (1.0 - 0.2 * sag) - run * 0.45).max(d.r * 0.3), RainQuad::DROP, DOME],
            });
        }
    }
}

/// Whether two drops overlap by enough to run together.
fn touching(a: &Drop, b: &Drop, reach: f32) -> bool {
    dist(a.pos, b.pos) < (a.r + b.r) * reach
}

/// One drop from two: their water adds up (radius as the cube root of the
/// volume), it sits nearer the bigger one, and keeps the faster one's run.
fn merge(a: Drop, b: Drop, max_r: f32) -> Drop {
    let (va, vb) = (a.volume(), b.volume());
    let (big, small) = if va >= vb { (a, b) } else { (b, a) };
    let k = small.volume() / (va + vb).max(1e-9);
    let mut d = big;
    d.pos = add(d.pos, scale(sub(small.pos, d.pos), k));
    d.r = (va + vb).cbrt().min(max_r);
    if small.speed > d.speed {
        d.speed = small.speed;
        d.trail = small.trail;
    }
    // A smaller drop striking a bead runs into it: the bead doesn't splat
    // again (struck often in a downpour, beads would bounce), and a bigger
    // one striking keeps its own splash, being `big`.
    d.pause = 0.0;
    d
}

/// Where the segments `a`..`b` and `c`..`d` cross, if they do.
fn crossing(a: [f32; 2], b: [f32; 2], c: [f32; 2], d: [f32; 2]) -> Option<[f32; 2]> {
    let (r, s) = (sub(b, a), sub(d, c));
    let denom = r[0] * s[1] - r[1] * s[0];
    if denom.abs() < 1e-6 {
        return None;
    }
    let ca = sub(c, a);
    let t = (ca[0] * s[1] - ca[1] * s[0]) / denom;
    let u = (ca[0] * r[1] - ca[1] * r[0]) / denom;
    ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)).then(|| add(a, scale(r, t)))
}

/// Quick at first and gentle at the end, 0..1.
fn ease(k: f32) -> f32 {
    1.0 - (1.0 - k.clamp(0.0, 1.0)).powi(3)
}

/// The spray drop `d` throws as it strikes, `k` (0..1) of the way through:
/// where each droplet is and its radius (pixels). A ring of them round the
/// splat, unevenly spaced and flung, farther from a drop that burst.
fn spray(d: &Drop, k: f32, s: f32) -> impl Iterator<Item = ([f32; 2], f32)> + '_ {
    let n = 5 + (d.r / s) as usize * if d.burst { 2 } else { 1 };
    let flung = ease(k) * if d.burst { 1.8 } else { 1.0 };
    (0..n).map(move |i| {
        let fi = i as f32;
        let h1 = (d.seed * 97.31 + fi * 0.618).fract();
        let h2 = (d.seed * 53.17 + fi * 0.377 + h1 * 0.5).fract();
        let angle = std::f32::consts::TAU * (fi + 0.6 * h1) / n as f32;
        let far = d.r * (1.15 + (0.8 + 1.4 * h2) * flung);
        let r = (d.r * (0.12 + 0.16 * h2) * (1.0 - 0.35 * k)).max(0.5 * s);
        ([d.pos[0] + angle.cos() * far, d.pos[1] + angle.sin() * far], r)
    })
}

fn sub(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn add(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] + b[0], a[1] + b[1]]
}

fn scale(a: [f32; 2], k: f32) -> [f32; 2] {
    [a[0] * k, a[1] * k]
}

fn len(a: [f32; 2]) -> f32 {
    (a[0] * a[0] + a[1] * a[1]).sqrt()
}

fn dist(a: [f32; 2], b: [f32; 2]) -> f32 {
    len(sub(a, b))
}

fn dot(a: [f32; 2], b: [f32; 2]) -> f32 {
    a[0] * b[0] + a[1] * b[1]
}

fn perp(a: [f32; 2]) -> [f32; 2] {
    [-a[1], a[0]]
}

fn normalize(a: [f32; 2]) -> [f32; 2] {
    let l = len(a);
    if l > 1e-6 {
        scale(a, 1.0 / l)
    } else {
        [1.0, 0.0]
    }
}

/// The point on the segment a..b closest to `p`.
fn closest(a: [f32; 2], b: [f32; 2], p: [f32; 2]) -> [f32; 2] {
    let ab = sub(b, a);
    let t = dot(sub(p, a), ab) / dot(ab, ab).max(1e-6);
    add(a, scale(ab, t.clamp(0.0, 1.0)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A frame of `drizzle` (0..1 of the drizzle at its fullest).
    fn input(drizzle: f32, flow_dt: f32) -> FrameInput {
        FrameInput {
            flow_dt,
            flow_t: 10.0,
            dt: flow_dt,
            ambient: 0.6,
            rain: drizzle * DRIZZLE_AT,
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

    /// A frame of Rain at `amount` (0..1, a downpour at the top).
    fn raining(amount: f32, flow_dt: f32) -> FrameInput {
        FrameInput { rain: amount, ..input(0.0, flow_dt) }
    }

    #[test]
    fn the_rain_goes_from_a_drizzle_to_a_downpour_fifty_times_as_hard() {
        assert_eq!((rate(0.0), rate(DRIZZLE_AT)), (0.0, DRIZZLE));
        assert!((rate(1.0) - DRIZZLE * DOWNPOUR).abs() < 1e-3);
        let rates: Vec<f32> = (0..=20).map(|i| rate(i as f32 / 20.0)).collect();
        assert!(rates.windows(2).all(|w| w[1] > w[0]), "{rates:?}");
        // The glass gets wet all over, and the rain hisses, only as it pours.
        assert_eq!((wetness(0.5), wetness(1.0), hiss(0.2)), (0.0, 1.0, 0.0));
        assert!(hiss(0.5) > 0.0 && hiss(0.5) < hiss(0.9));
    }

    #[test]
    fn a_drop_strikes_in_a_splat_throwing_spray_then_gathers_into_its_bead() {
        let mut rain = Rain::new();
        rain.started = true;
        let d = rain.strike(Drop::new([400.0, 300.0], 3.0, 0.0), 1.0);
        assert!(!d.burst && d.speed == 0.0);
        rain.add(d);
        let kinds = |rain: &Rain| {
            let mut out = Vec::new();
            rain.draw(&mut out);
            out
        };
        let out = kinds(&rain);
        let splat = out.iter().find(|q| q.params[2] == RainQuad::SPLAT).expect("a splat");
        assert!(splat.params[0] > 3.0 * 1.5 && splat.params[3] < DOME * 0.5, "wider and flatter than its bead: {:?}", splat.params);
        let spray: Vec<_> = out.iter().filter(|q| q.params[2] == RainQuad::SPRAY).collect();
        assert!(spray.len() >= 6 && spray.iter().all(|q| dist([q.ends[0], q.ends[1]], [400.0, 300.0]) > 3.0), "a ring of spray round it");
        // Halfway, the spray is flung farther and the splat drawn in.
        for _ in 0..5 {
            rain.frame(&input(0.0, IMPACT / 10.0), &[], 0.0, &mut Vec::new(), &mut Vec::new());
        }
        let half = kinds(&rain);
        let far = |out: &[RainQuad]| out.iter().filter(|q| q.params[2] == RainQuad::SPRAY).map(|q| dist([q.ends[0], q.ends[1]], [400.0, 300.0])).fold(0.0, f32::max);
        assert!(far(&half) > far(&out));
        assert!(half.iter().find(|q| q.params[2] == RainQuad::SPLAT).unwrap().params[0] < splat.params[0]);
        // Then it's its bead, as a drop that never struck.
        for _ in 0..10 {
            rain.frame(&input(0.0, IMPACT / 10.0), &[], 0.0, &mut Vec::new(), &mut Vec::new());
        }
        let after = kinds(&rain);
        assert!(after.iter().all(|q| q.params[2] == RainQuad::DROP), "{after:?}");
        assert!(after.iter().any(|q| (q.ends[0] - 400.0).abs() < 1.0 && (q.params[0] - 3.0).abs() < 0.1));
    }

    #[test]
    fn a_bead_struck_by_a_smaller_drop_takes_it_in_without_splatting_again() {
        let mut rain = Rain::new();
        rain.started = true;
        rain.add(Drop::new([300.0, 300.0], 4.0, 0.0));
        let small = rain.strike(Drop::new([302.0, 300.0], 1.5, 0.0), 1.0);
        rain.add(small);
        assert_eq!(rain.drops.len(), 1);
        assert!(!rain.drops[0].striking(), "the bead splatted again");
        // A drop bigger than the bead it lands on splashes as it would.
        let big = rain.strike(Drop::new([300.0, 300.0], 4.4, 0.0), 1.0);
        rain.add(big);
        assert!(rain.drops[0].striking());
    }

    #[test]
    fn a_caught_runner_slows_to_a_stop_rather_than_snapping_short() {
        let mut rain = Rain::new();
        rain.started = true;
        rain.mist = 0.0;
        let mut d = Drop::new([300.0, 100.0], 7.0, 0.0);
        d.speed = 100.0;
        d.pause = 0.3;
        let mut speeds = Vec::new();
        for _ in 0..6 {
            rain.run(&mut d, 1.0 / 60.0, 1.0);
            speeds.push(d.speed);
        }
        assert!(speeds[0] > 80.0 && speeds.windows(2).all(|w| w[1] < w[0]) && speeds[5] > 30.0, "{speeds:?}");
    }

    #[test]
    fn water_follows_water_down_a_wet_track_that_becomes_a_stream() {
        let mut rain = Rain::new();
        rain.started = true;
        rain.mist = 0.0;
        // An earlier runner's trail straight down at x = 300.
        for k in 0..12 {
            let y = 80.0 + k as f32 * 40.0;
            rain.streak([300.0, y], [300.0, y + 40.0], 3.0, 0.0);
        }
        // A runner beside it, slanting away, is drawn onto it and follows it.
        let mut d = Drop::new([312.0, 60.0], 7.0, 0.3);
        d.speed = 60.0;
        rain.add(d);
        let mut out = Vec::new();
        for _ in 0..180 {
            out.clear();
            rain.frame(&input(0.0, 1.0 / 60.0), &[], 0.0, &mut out, &mut Vec::new());
        }
        let runner = rain.drops.iter().max_by(|a, b| a.r.total_cmp(&b.r)).unwrap();
        assert!((runner.pos[0] - 300.0).abs() < 3.0 && runner.pos[1] > 220.0, "{:?}", runner.pos);
        // On it, it left no beads, and the track it ran down is wetter: a
        // stream, wider and still clear.
        assert!(rain.drops.iter().filter(|d| d.pos[1] > 120.0 && d.pos[1] < runner.pos[1] - 20.0).count() == 0, "beads left on the track");
        let wet: Vec<_> = rain.streaks.iter().filter(|st| st.track && st.flow > 0.0).collect();
        assert!(!wet.is_empty() && wet.iter().all(|st| st.age < STREAK_LIFE * 0.25));
        let widest = out.iter().filter(|q| q.params[2] == RainQuad::STREAK).map(|q| q.params[0]).fold(0.0, f32::max);
        assert!(widest > 3.0 * 1.05, "no wider stream drawn: {widest}");
        // Only drops and clear channels go in the rain map.
        assert!(out.iter().all(|q| [RainQuad::DROP, RainQuad::STREAK, RainQuad::SPLAT, RainQuad::SPRAY].contains(&q.params[2])));
        // A runner far from any track runs as it always did.
        let mut rain = Rain::new();
        rain.started = true;
        rain.streak([300.0, 80.0], [300.0, 500.0], 3.0, 0.0);
        let mut d = Drop::new([600.0, 60.0], 7.0, 0.0);
        d.speed = 60.0;
        rain.add(d);
        for _ in 0..60 {
            rain.frame(&input(0.0, 1.0 / 60.0), &[], 0.0, &mut Vec::new(), &mut Vec::new());
        }
        assert!(rain.drops.iter().all(|d| (d.pos[0] - 300.0).abs() > 200.0));
    }

    #[test]
    fn the_mouses_paths_steer_runners_down_them_and_hold_them_back_across_them() {
        // A wipe across the glass, as the mouse makes one.
        let wipe = |rain: &mut Rain, from: [f32; 2], step: [f32; 2]| {
            let mut f = input(0.0, 1.0 / 60.0);
            f.focused = true;
            for k in 0..=30 {
                f.mouse = (from[0] + step[0] * k as f32, from[1] + step[1] * k as f32);
                rain.frame(&f, &[], if k == 0 { 0.0 } else { 10.0 }, &mut Vec::new(), &mut Vec::new());
            }
        };
        let idle = input(0.0, 1.0 / 60.0);
        // Across the glass at y = 300: a runner above it is held at it a
        // moment, then breaks through and runs on.
        let mut rain = Rain::new();
        rain.started = true;
        rain.mist = 0.0;
        wipe(&mut rain, [200.0, 300.0], [10.0, 0.0]);
        let mut d = Drop::new([350.0, 250.0], 8.0, 0.0);
        d.speed = 120.0;
        rain.add(d);
        let mut held = 0;
        let mut lowest = 0.0f32;
        for _ in 0..180 {
            rain.frame(&idle, &[], 0.0, &mut Vec::new(), &mut Vec::new());
            let runner = rain.drops.iter().max_by(|a, b| a.r.total_cmp(&b.r)).unwrap();
            if runner.pause > 0.0 && (runner.pos[1] - 296.0).abs() < 6.0 {
                held += 1;
            }
            lowest = lowest.max(runner.pos[1]);
        }
        assert!(held >= 10, "held at the wiped path for {held} frames");
        assert!(lowest > 340.0, "it broke through: lowest at {lowest}");
        // Down the glass, slanting: a runner beside it is drawn into it.
        let mut rain = Rain::new();
        rain.started = true;
        rain.mist = 0.0;
        wipe(&mut rain, [400.0, 100.0], [3.0, 12.0]);
        let mut d = Drop::new([415.0, 90.0], 7.0, 0.0);
        d.speed = 60.0;
        rain.add(d);
        for _ in 0..120 {
            rain.frame(&idle, &[], 0.0, &mut Vec::new(), &mut Vec::new());
        }
        let runner = rain.drops.iter().max_by(|a, b| a.r.total_cmp(&b.r)).unwrap();
        // The path at the runner's height is at x = 400 + (y - 100) / 4.
        let path_x = 400.0 + (runner.pos[1] - 100.0) / 4.0;
        assert!((runner.pos[0] - path_x).abs() < 6.0 && runner.pos[1] > 150.0, "{:?} against {path_x}", runner.pos);
    }

    #[test]
    fn runners_on_one_track_catch_up_and_join() {
        let mut rain = Rain::new();
        rain.started = true;
        rain.mist = 0.0;
        for k in 0..12 {
            let y = 80.0 + k as f32 * 40.0;
            rain.streak([300.0, y], [300.0, y + 40.0], 3.0, 0.0);
        }
        // A small runner ahead, a heavier one behind, both on the track.
        let mut ahead = Drop::new([300.0, 150.0], 5.0, 0.0);
        ahead.speed = 20.0;
        let mut behind = Drop::new([300.0, 90.0], 9.0, 0.0);
        behind.speed = 120.0;
        let water = ahead.volume() + behind.volume();
        rain.add(ahead);
        rain.add(behind);
        for _ in 0..90 {
            rain.frame(&input(0.0, 1.0 / 60.0), &[], 0.0, &mut Vec::new(), &mut Vec::new());
        }
        let biggest = rain.drops.iter().map(|d| d.volume()).fold(0.0, f32::max);
        assert!(biggest > water * 0.9, "they joined: {biggest} of {water}");
    }

    #[test]
    fn a_heavy_drop_bursts_and_runs_at_once() {
        let mut rain = Rain::new();
        rain.started = true;
        let small = rain.strike(Drop::new([200.0, 100.0], 3.0, 0.0), 1.0);
        let heavy = rain.strike(Drop::new([600.0, 100.0], 5.0, 0.0), 1.0);
        assert!(heavy.burst && heavy.speed > 0.0, "it runs straight away");
        let reach = |d: &Drop| spray(d, 1.0, 1.0).map(|(p, _)| dist(p, d.pos) / d.r).fold(0.0, f32::max);
        assert!(reach(&heavy) > reach(&small) * 1.3, "its spray flung farther");
        rain.add(heavy);
        rain.frame(&input(0.0, 1.0 / 60.0), &[], 0.0, &mut Vec::new(), &mut Vec::new());
        assert!(rain.drops.iter().any(|d| d.pos[0] > 590.0 && d.pos[1] > 100.0));
    }

    #[test]
    fn taps_thin_out_as_the_rain_comes_too_fast_to_hear() {
        let heard_in = |amount: f32| {
            let mut rain = Rain::new();
            let mut heard = Vec::new();
            for _ in 0..180 {
                rain.frame(&raining(amount, 1.0 / 60.0), &[], 0.0, &mut Vec::new(), &mut heard);
            }
            heard.len()
        };
        // A downpour brings some 450 drops in three seconds; at most about
        // TAPS a second of them are heard, the hiss carrying the rest.
        let (drizzle, downpour) = (heard_in(0.15), heard_in(1.0));
        assert!(drizzle > 2, "{drizzle} taps in a drizzle");
        assert!((15..=40).contains(&downpour), "{downpour} taps in a downpour");
    }

    #[test]
    fn a_downpour_runs_in_rivulets_fogs_paths_over_sooner_and_stays_in_bounds() {
        let mut rain = Rain::new();
        let mut out = Vec::new();
        for _ in 0..600 {
            out.clear();
            rain.frame(&raining(1.0, 1.0 / 60.0), &[], 0.0, &mut out, &mut Vec::new());
            assert!(rain.drops.len() <= MAX_DROPS * 2 + MAX_DROPS / 5);
        }
        // Runners' trails on wet glass are wider channels.
        let trails: Vec<f32> = rain.streaks.iter().filter(|st| st.track).map(|st| st.r).collect();
        let drawn = out.iter().filter(|q| q.params[2] == RainQuad::STREAK).map(|q| q.params[0]).fold(0.0, f32::max);
        assert!(!trails.is_empty() && drawn > trails.iter().fold(0.0, |a: f32, &b| a.max(b)), "{drawn}");
        assert!(rain.drops.len() > MAX_DROPS, "{} drops on the glass", rain.drops.len());
        // A clear path closes over again sooner on wet glass.
        let fogged_after = |amount: f32| {
            let mut rain = Rain::new();
            rain.started = true;
            rain.mist = 0.0;
            rain.streak([100.0, 100.0], [100.0, 200.0], 3.0, 0.0);
            // Its own path, not those the new rain runs.
            let ours = |rain: &Rain| rain.streaks.iter().any(|st| st.a == [100.0, 100.0] && st.b == [100.0, 200.0]);
            let mut secs = 0.0;
            while ours(&rain) && secs < 60.0 {
                rain.frame(&raining(amount, 0.1), &[], 0.0, &mut Vec::new(), &mut Vec::new());
                secs += 0.1;
            }
            secs
        };
        assert!(fogged_after(1.0) < fogged_after(0.2) * 0.4);
    }

    #[test]
    fn rain_landing_on_the_glass_is_heard_where_it_lands_but_typing_is_not() {
        let mut rain = Rain::new();
        let mut heard = Vec::new();
        for _ in 0..180 {
            rain.frame(&input(1.0, 1.0 / 60.0), &[], 0.0, &mut Vec::new(), &mut heard);
        }
        assert!(heard.len() > 3, "{} drops heard in three seconds", heard.len());
        assert!(heard.iter().all(|h| matches!(h, Heard::Raindrop { x, size } if (0.0..=1.0).contains(x) && (0.0..=1.0).contains(size))));
        heard.clear();
        rain.frame(&input(0.0, 1.0 / 60.0), &[((300.0, 200.0), true)], 0.0, &mut Vec::new(), &mut heard);
        assert!(heard.is_empty(), "keys click on their own");
    }

    fn total_volume(rain: &Rain) -> f32 {
        rain.drops.iter().map(Drop::volume).sum()
    }

    #[test]
    fn the_glass_starts_wet_and_drizzle_follows_ambient() {
        let mut rain = Rain::new();
        let mut out = Vec::new();
        rain.frame(&input(0.6, 1.0 / 60.0), &[], 0.0, &mut out, &mut Vec::new());
        let start = rain.drops.len();
        assert!((150..=350).contains(&start), "{start} drops at the start");
        assert_eq!(out.iter().filter(|q| q.params[2] == RainQuad::DROP).count(), start);

        // No drizzle and no water in the fog: water only goes (drying,
        // running off), it never comes.
        let mut dry = Rain::new();
        dry.mist = 0.0;
        dry.frame(&input(0.0, 1.0 / 60.0), &[], 0.0, &mut Vec::new(), &mut Vec::new());
        let mut water = total_volume(&dry);
        for _ in 0..600 {
            dry.frame(&input(0.0, 1.0 / 60.0), &[], 0.0, &mut Vec::new(), &mut Vec::new());
            let now = total_volume(&dry);
            assert!(now <= water * 1.0001, "water went from {water} to {now}");
            water = now;
        }

        // Full drizzle for a minute: plenty of new drops, but never too many.
        for _ in 0..3600 {
            rain.frame(&input(1.0, 1.0 / 60.0), &[], 0.0, &mut Vec::new(), &mut Vec::new());
            assert!(rain.drops.len() <= MAX_DROPS);
        }
        assert!(rain.drops.len() > start, "{} drops after a minute of rain", rain.drops.len());
    }

    #[test]
    fn touching_drops_merge_and_keep_their_water() {
        let mut rain = Rain::new();
        rain.started = true;
        rain.add(Drop::new([100.0, 100.0], 2.0, 0.0));
        rain.add(Drop::new([102.0, 100.0], 2.0, 0.0));
        assert_eq!(rain.drops.len(), 1);
        let d = &rain.drops[0];
        assert!((d.volume() - 16.0).abs() < 1e-3, "volume {}", d.volume());
        assert!((d.pos[0] - 101.0).abs() < 1e-3);
        rain.add(Drop::new([300.0, 100.0], 2.0, 0.0));
        assert_eq!(rain.drops.len(), 2);
    }

    #[test]
    fn heavy_drops_run_down_leaving_drops_and_a_clear_path() {
        let mut rain = Rain::new();
        rain.started = true;
        rain.add(Drop::new([550.0, 50.0], 8.0, 0.0));
        let f = input(0.0, 1.0 / 60.0);
        let mut seen_streak = false;
        // Two seconds: longer than any time it catches on the glass.
        for _ in 0..120 {
            let mut out = Vec::new();
            rain.frame(&f, &[], 0.0, &mut out, &mut Vec::new());
            let trails: Vec<_> = out.iter().filter(|q| q.params[2] == RainQuad::STREAK).collect();
            seen_streak |= !trails.is_empty();
            // Only the cursor pushes water up along a path's sides.
            assert!(trails.iter().all(|q| q.params[1] == 0.0));
        }
        let runner = rain.drops.iter().max_by(|a, b| a.r.total_cmp(&b.r)).unwrap();
        assert!(runner.pos[1] > 70.0, "the runner is still at {:?}", runner.pos);
        assert!(rain.drops.len() > 1, "no drops left behind");
        assert!(rain.drops.iter().filter(|d| d.r < runner.r).all(|d| d.pos[1] < runner.pos[1]));
        assert!(seen_streak, "no clear path behind the runner");
        // It keeps going down the glass (or off it), slowing as it gives its
        // water away.
        let at_2s = runner.pos[1];
        for _ in 0..1200 {
            rain.frame(&f, &[], 0.0, &mut Vec::new(), &mut Vec::new());
        }
        let lowest = rain.drops.iter().map(|d| d.pos[1]).fold(0.0, f32::max);
        assert!(lowest > at_2s + 200.0, "got from {at_2s} to only {lowest} in 20 s");
    }

    #[test]
    fn runners_grow_and_speed_up_as_they_sweep_up_the_water_on_their_way() {
        // On fogged glass alone it gathers more than its beads take.
        let mut rain = Rain::new();
        rain.started = true;
        rain.add(Drop::new([550.0, 50.0], 6.5, 0.0));
        let f = input(0.0, 1.0 / 60.0);
        let mut track = Vec::new();
        for _ in 0..8 {
            for _ in 0..60 {
                rain.frame(&f, &[], 0.0, &mut Vec::new(), &mut Vec::new());
            }
            let runner = rain.drops.iter().max_by(|a, b| a.r.total_cmp(&b.r)).unwrap();
            track.push((runner.r, runner.pos[1]));
        }
        let (r, y) = track[7];
        assert!(r > 6.5 * 1.15 && y > 300.0, "after 8 s: radius {r} at {y}");
        assert!(track.windows(2).all(|w| w[1].0 >= w[0].0 * 0.98), "it shrank: {track:?}");
        // The last second covered more ground than the first.
        assert!(track[7].1 - track[6].1 > track[0].1 - 50.0, "no faster: {track:?}");

        // A fast one takes in a drop it passes between two frames, not only
        // what it touches where it stops.
        let mut rain = Rain::new();
        rain.started = true;
        rain.mist = 0.0;
        let mut d = Drop::new([300.0, 100.0], 9.0, 0.0);
        d.speed = 600.0;
        rain.add(d);
        rain.add(Drop::new([300.0, 112.0], 1.0, 0.0));
        rain.frame(&input(0.0, 0.05), &[], 0.0, &mut Vec::new(), &mut Vec::new());
        let runner = rain.drops.iter().max_by(|a, b| a.r.total_cmp(&b.r)).unwrap();
        assert!(runner.pos[1] > 120.0, "the runner only got to {:?}", runner.pos);
        assert!(!rain.drops.iter().any(|d| d.r <= 1.0), "it ran past the small drop");

        // Two runners that meet run on as one, with both their water.
        let mut rain = Rain::new();
        rain.started = true;
        rain.mist = 0.0;
        let mut a = Drop::new([300.0, 100.0], 7.0, 0.0);
        a.speed = 300.0;
        let b = Drop::new([300.0, 130.0], 6.0, 0.0);
        let water = a.volume() + b.volume();
        rain.add(a);
        rain.add(b);
        for _ in 0..60 {
            rain.frame(&input(0.0, 1.0 / 60.0), &[], 0.0, &mut Vec::new(), &mut Vec::new());
        }
        let biggest = rain.drops.iter().map(|d| d.volume()).fold(0.0, f32::max);
        assert!(biggest > water * 0.9, "biggest drop {biggest} of {water}");
    }

    #[test]
    fn runners_leave_a_thin_trail_beaded_with_tiny_drops() {
        let mut rain = Rain::new();
        rain.started = true;
        rain.add(Drop::new([550.0, 50.0], 8.0, 0.0));
        let f = input(0.0, 1.0 / 60.0);
        let mut out = Vec::new();
        for _ in 0..90 {
            out.clear();
            rain.frame(&f, &[], 0.0, &mut out, &mut Vec::new());
        }
        let runner = rain.drops.iter().max_by(|a, b| a.r.total_cmp(&b.r)).unwrap();
        let beads: Vec<_> = rain.drops.iter().filter(|d| d.r < runner.r).collect();
        assert!(beads.len() >= 4, "only {} beads behind it", beads.len());
        assert!(beads.iter().all(|d| d.r <= runner.r * 0.45), "beads too big for a runner of {}", runner.r);
        // Its trail is much narrower than the drop itself.
        let trails: Vec<_> = out.iter().filter(|q| q.params[2] == RainQuad::STREAK).collect();
        assert!(!trails.is_empty() && trails.iter().all(|q| q.params[0] < runner.r * 0.5));
    }

    #[test]
    fn small_drops_are_round_and_bigger_ones_sag() {
        let mut rain = Rain::new();
        rain.started = true;
        rain.add(Drop::new([100.0, 100.0], 1.5, 0.2));
        rain.add(Drop::new([300.0, 100.0], 5.0, 0.2));
        let mut out = Vec::new();
        rain.draw(&mut out);
        let shape = |x: f32| out.iter().find(|q| (q.ends[0] - x).abs() < 1.0).unwrap();
        let small = shape(100.0);
        assert_eq!(small.ends, [100.0, 100.0, 100.0, 100.0]);
        assert_eq!(small.params[0], small.params[1]);
        // Fuller below its middle, narrower on top, and leaning back from the
        // way it will run.
        let big = shape(300.0);
        assert!(big.ends[1] > 100.0 && big.ends[3] < 100.0, "{:?}", big.ends);
        assert!(big.params[1] < big.params[0] && big.ends[2] < big.ends[0]);
    }

    #[test]
    fn runners_draw_out_behind_them_along_their_path() {
        let mut rain = Rain::new();
        rain.started = true;
        let mut d = Drop::new([300.0, 100.0], 7.0, 0.3);
        d.speed = 120.0;
        rain.add(d);
        let mut out = Vec::new();
        rain.draw(&mut out);
        let drop = out.iter().find(|q| q.params[2] == RainQuad::DROP).unwrap();
        // Up and back from the way it runs (down and to the right), and
        // thinner there than at its head.
        let back = [drop.ends[2] - drop.ends[0], drop.ends[3] - drop.ends[1]];
        assert!(back[1] < -7.0 && (back[0] / back[1] - 0.3).abs() < 1e-3, "{:?}", drop.ends);
        assert!(drop.params[1] < drop.params[0] * 0.8);
    }

    #[test]
    fn wiping_across_an_earlier_wipe_wipes_away_its_beads() {
        fn stroke(rain: &mut Rain, from: [f32; 2], step: [f32; 2]) {
            let mut f = input(0.0, 1.0 / 60.0);
            f.focused = true;
            for k in 0..=30 {
                f.mouse = (from[0] + step[0] * k as f32, from[1] + step[1] * k as f32);
                rain.frame(&f, &[], if k == 0 { 0.0 } else { 10.0 }, &mut Vec::new(), &mut Vec::new());
            }
        }
        let mut rain = Rain::new();
        rain.started = true;
        stroke(&mut rain, [100.0, 300.0], [10.0, 0.0]);
        stroke(&mut rain, [250.0, 150.0], [0.0, 10.0]);
        let across: Vec<_> = rain.streaks.iter().filter(|st| st.a[1] == 300.0 && st.b[1] == 300.0).collect();
        let down: Vec<_> = rain.streaks.iter().filter(|st| st.a[0] == 250.0 && st.b[0] == 250.0).collect();
        assert!(across.len() > 20 && down.len() > 20);
        // Where the second crossed the first, the first's beads are gone;
        // elsewhere both keep theirs.
        assert!(across.iter().filter(|st| ((st.a[0] + st.b[0]) * 0.5 - 250.0).abs() < 10.0).all(|st| st.ridge == 0.0));
        assert!(across.iter().filter(|st| (st.a[0] - 250.0).abs() > 30.0).all(|st| st.ridge > 0.0));
        assert!(down.iter().all(|st| st.ridge > 0.0));
    }

    #[test]
    fn the_cursor_wipes_a_path_clear() {
        let mut rain = Rain::new();
        rain.started = true;
        for i in 0..40 {
            rain.add(Drop::new([100.0 + i as f32 * 10.0, 300.0], 1.5, 0.0));
        }
        let mut f = input(0.0, 1.0 / 60.0);
        f.focused = true;
        f.mouse = (80.0, 300.0);
        rain.frame(&f, &[], 0.0, &mut Vec::new(), &mut Vec::new());
        let volume = total_volume(&rain);
        let mut out = Vec::new();
        for k in 1..=44 {
            f.mouse = (80.0 + k as f32 * 10.0, 300.0);
            out.clear();
            rain.frame(&f, &[], 10.0, &mut out, &mut Vec::new());
        }
        assert!(rain.drops.iter().all(|d| (d.pos[1] - 300.0).abs() > WIPE_R * 0.9), "drops left on the path");
        // Pushed aside and run together, not lost (only a little dried up).
        assert!(total_volume(&rain) > volume * 0.95);
        // The path is clear, with the water pushed up along its sides.
        let path: Vec<_> = out.iter().filter(|q| q.params[2] == RainQuad::STREAK).collect();
        assert!(!path.is_empty() && path.iter().all(|q| q.params[1] > 0.0 && q.params[3] > 0.9));
    }

    #[test]
    fn a_jump_back_into_the_window_does_not_wipe() {
        let mut rain = Rain::new();
        rain.started = true;
        rain.add(Drop::new([500.0, 300.0], 1.5, 0.0));
        let mut f = input(0.0, 1.0 / 60.0);
        f.focused = true;
        f.mouse = (100.0, 300.0);
        rain.frame(&f, &[], 0.0, &mut Vec::new(), &mut Vec::new());
        f.mouse = (900.0, 300.0);
        rain.frame(&f, &[], 3.0, &mut Vec::new(), &mut Vec::new());
        assert_eq!(rain.drops[0].pos, [500.0, 300.0]);
    }

    #[test]
    fn typing_lands_drops_by_the_cursor() {
        let mut rain = Rain::new();
        rain.started = true;
        let f = input(0.0, 1.0 / 60.0);
        rain.frame(&f, &[((400.0, 200.0), false)], 0.0, &mut Vec::new(), &mut Vec::new());
        assert_eq!(rain.drops.len(), 1);
        let d = &rain.drops[0];
        assert!(dist(d.pos, [400.0, 200.0]) < 10.0 && d.r < RUN_R);
        // Enter lands one heavy enough to run.
        rain.frame(&f, &[((700.0, 200.0), true)], 0.0, &mut Vec::new(), &mut Vec::new());
        assert!(rain.drops.iter().any(|d| d.r >= RUN_R * 0.85 && d.pos[0] > 690.0));
    }

    #[test]
    fn slower_flow_slower_rain() {
        let run_for = |flow_dt: f32| {
            let mut rain = Rain::new();
            rain.started = true;
            rain.add(Drop::new([550.0, 50.0], 9.0, 0.0));
            for _ in 0..20 {
                rain.frame(&input(0.0, flow_dt), &[], 0.0, &mut Vec::new(), &mut Vec::new());
            }
            rain.drops.iter().map(|d| d.pos[1]).fold(0.0, f32::max) - 50.0
        };
        let normal = run_for(1.0 / 60.0);
        let slow = run_for(0.2 / 60.0);
        assert!(normal > 0.0 && slow < normal * 0.5, "moved {slow} slowed against {normal}");
    }
}

