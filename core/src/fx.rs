//! CPU side of the liquid: what gets stirred in each frame (currents, ripple
//! drops, oil), plus bubbles (bubbles.rs) and lightning, which are drawn as
//! quads, and the drops of the rain style (rain.rs). Commands ending and the
//! assistant at work splash it ([`Reaction`]), and so do the effects programs
//! ask for ([`ProgramFx`], OSC 7731), which also glow over pictures; the
//! rings and the progress tank over any style are in signals.rs.
//!
//! Everything that should follow the flow speed is scaled by `flow_dt` (real
//! time × speed) and injected a little every frame, so a slowed-down flow
//! stays smooth instead of receiving occasional full-strength bursts.

mod bubbles;
pub mod rain;
mod signals;

pub use signals::{Ring, Signals};

use std::time::Instant;

use crate::osc::FxKind;
use crate::render::fluid::{Drop, Material, Splat};
use crate::render::rain::RainQuad;
use crate::render::{Canvas, Rgba};
use crate::ui::Rect;
use bubbles::Bubbles;
use rain::Rain;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Clouds,
    WaterAbove,
    WaterBelow,
    Oil,
    /// Rain on the window glass, with lights outside.
    Rain,
    /// No liquid: plain tinted glass, with nothing to simulate.
    None,
}

impl Style {
    pub fn from_config(style: &str, water_view: &str) -> Self {
        match style {
            "water" if water_view == "below" => Style::WaterBelow,
            "water" => Style::WaterAbove,
            "oil" => Style::Oil,
            "rain" => Style::Rain,
            "none" => Style::None,
            _ => Style::Clouds,
        }
    }

    pub fn material(self) -> Material {
        match self {
            // None never runs the simulation; any material will do.
            Style::Clouds | Style::None => Material::Clouds,
            Style::WaterAbove | Style::WaterBelow => Material::Water,
            Style::Oil => Material::Oil,
            Style::Rain => Material::Rain,
        }
    }

    /// Value of `mode` in the glass shader.
    pub fn shader_mode(self) -> f32 {
        match self {
            Style::Clouds => 0.0,
            Style::WaterAbove => 1.0,
            Style::WaterBelow => 2.0,
            Style::Oil => 3.0,
            Style::Rain => 4.0,
            Style::None => 5.0,
        }
    }
}

/// What the effects need to know about this frame.
pub struct FrameInput {
    /// Seconds of the liquid's clock that passed this frame (real dt × speed).
    pub flow_dt: f32,
    /// The liquid's clock.
    pub flow_t: f32,
    /// Real seconds since the last frame (bolts flicker in real time).
    pub dt: f32,
    pub ambient: f32,
    /// How hard it rains in the rain style, 0..1 (`fx.rain`).
    pub rain: f32,
    pub electricity: f32,
    pub width: f32,
    pub height: f32,
    pub scale: f32,
    pub focused: bool,
    pub mouse: (f32, f32),
    /// AI activity, 0..1.
    pub activity: f32,
    pub panel: Option<Rect>,
    /// Width of a simulation texel in window pixels.
    pub sim_px: f32,
}

#[derive(Default)]
pub struct FrameOutput {
    pub splats: Vec<Splat>,
    pub drops: Vec<Drop>,
    /// Lightning flash lighting the clouds: x, y (0..1), intensity, radius.
    pub flash: [f32; 4],
    /// The rain style's drops and clear paths, for the rain map.
    pub rain: Vec<RainQuad>,
    /// What the liquid did this frame that can be heard.
    pub heard: Vec<Heard>,
}

/// Something the liquid does on its own that can be heard, for the app to
/// play while the liquid's sounds are on: where across the window it was
/// (0 left to 1 right) and how big (0..1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Heard {
    /// A bolt of the clouds' own lightning, or the rain's storm's: `near`
    /// 1 close by, toward 0 far off.
    Thunder { x: f32, near: f32 },
    /// A bubble reaching the surface.
    Bubble { x: f32, size: f32 },
    /// A drop of rain landing on the glass.
    Raindrop { x: f32, size: f32 },
}

/// Something worth a splash in the liquid: a command ending, the assistant
/// at work. Positions are window pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Reaction {
    /// A command finished well; `long` when it ran a good while.
    Done { pos: (f32, f32), long: bool },
    /// A command failed.
    Failed { pos: (f32, f32) },
    /// The assistant reached for a tool.
    Tool { pos: (f32, f32) },
    /// The assistant finished a task of several steps.
    Bloom { pos: (f32, f32) },
}

/// An effect a program asked for (OSC 7731), in window pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProgramFx {
    pub kind: FxKind,
    pub pos: (f32, f32),
    /// Where a bolt ends.
    pub to: (f32, f32),
    /// How far it reaches.
    pub radius: f32,
    /// Its color where the liquid takes color; the palette's otherwise.
    pub color: Option<[f32; 3]>,
    /// Which way a stir pushes, a unit vector.
    pub dir: (f32, f32),
    /// 0..1.
    pub strength: f32,
}

/// How many effects a program may set off: a few a second, with room for a
/// short burst, so a program can't keep the liquid boiling.
#[derive(Debug, Clone, Copy)]
pub struct Allowance {
    left: f32,
    at: Option<Instant>,
}

impl Allowance {
    const PER_SECOND: f32 = 6.0;
    const BURST: f32 = 8.0;

    /// Spends one effect at `now`, if there's one to spend.
    pub fn take(&mut self, now: Instant) -> bool {
        let since = self.at.map_or(Self::BURST / Self::PER_SECOND, |at| now.saturating_duration_since(at).as_secs_f32());
        self.left = (self.left + since * Self::PER_SECOND).min(Self::BURST);
        self.at = Some(now);
        if self.left >= 1.0 {
            self.left -= 1.0;
            true
        } else {
            false
        }
    }
}

impl Default for Allowance {
    fn default() -> Self {
        Self { left: Self::BURST, at: None }
    }
}

/// The color a failure stains the clouds.
const EMBER: [f32; 3] = [1.0, 0.32, 0.12];

/// Seconds a program's burst or ripple shows over pictures and grounds.
const FLARE_LIFE: f32 = 0.6;

/// A program's burst or ripple as it shows over pictures and cells'
/// backgrounds ([`Effects::draw_over`]): a program's own pictures, or the
/// ground it lays under its letters, cover the liquid where it asked, so
/// sparks fly out, or a ring spreads, above them.
#[derive(Debug, Clone, Copy)]
struct Flare {
    kind: FxKind,
    pos: (f32, f32),
    radius: f32,
    /// The program's color, or the palette's for a burst; a ring without
    /// one takes the accent's.
    color: Option<[f32; 3]>,
    strength: f32,
    /// Where the sparks start round the circle.
    turn: f32,
    age: f32,
}

struct Bolt {
    main: Vec<[f32; 2]>,
    branches: Vec<Vec<[f32; 2]>>,
    age: f32,
    /// A program's color for it; the accent's otherwise.
    color: Option<[f32; 3]>,
}

const BOLT_LIFE: f32 = 0.45;

/// The seconds a storm's thunder takes to come after its flash, from the
/// nearest strike to the farthest (sound's 340 metres a second, from a
/// hundred and fifty to two thousand away, near enough).
const STORM_NEAREST: f32 = 0.4;
const STORM_FARTHEST: f32 = 5.5;

/// The storm outside the rain's window ([`Effects::storm`]).
#[derive(Default)]
struct Storm {
    /// When the next strike comes, on the liquid's clock; 0 before the
    /// first is set.
    next: f32,
    /// Strikes lighting the night: seconds since, where across (0..1), how
    /// near (0..1).
    flashes: Vec<(f32, f32, f32)>,
    /// Thunder on its way: seconds still to come, where across, how near.
    thunder: Vec<(f32, f32, f32)>,
}

pub struct Effects {
    rng: u32,
    palette: Vec<[f32; 3]>,
    mouse_force: [f32; 2],
    mouse_travel: f32,
    keys: Vec<((f32, f32), bool)>,
    bubbles: Bubbles,
    bolts: Vec<Bolt>,
    next_bolt: f32,
    rain: Rain,
    storm: Storm,
    reactions: Vec<Reaction>,
    programs: Vec<ProgramFx>,
    /// Programs' bolts, in any style, drawn over pictures and grounds.
    program_bolts: Vec<Bolt>,
    flares: Vec<Flare>,
}

pub fn palette_colors(name: &str) -> Vec<[f32; 3]> {
    match name {
        "ocean" => vec![[0.1, 0.5, 1.0], [0.0, 0.8, 0.9], [0.2, 0.3, 0.9], [0.3, 0.9, 1.0]],
        "ember" => vec![[1.0, 0.45, 0.2], [1.0, 0.2, 0.35], [1.0, 0.75, 0.3], [0.8, 0.3, 0.9]],
        "mono" => vec![[0.7, 0.75, 0.85], [0.5, 0.55, 0.7], [0.9, 0.9, 1.0]],
        _ => vec![[0.25, 0.85, 1.0], [0.6, 0.4, 1.0], [0.2, 1.0, 0.75], [0.95, 0.4, 0.9], [0.35, 0.55, 1.0]],
    }
}

impl Effects {
    pub fn new(palette: &str) -> Self {
        Self {
            rng: 0x9E37_79B9,
            palette: palette_colors(palette),
            mouse_force: [0.0; 2],
            mouse_travel: 0.0,
            keys: Vec::new(),
            bubbles: Bubbles::new(),
            bolts: Vec::new(),
            next_bolt: 2.0,
            rain: Rain::new(),
            storm: Storm::default(),
            reactions: Vec::new(),
            programs: Vec::new(),
            program_bolts: Vec::new(),
            flares: Vec::new(),
        }
    }

    pub fn set_palette(&mut self, name: &str) {
        self.palette = palette_colors(name);
    }

    /// Forget bubbles, bolts and raindrops, e.g. after switching styles.
    pub fn clear(&mut self) {
        self.bubbles.clear();
        self.bolts.clear();
        self.rain.clear();
        self.storm = Storm::default();
        self.keys.clear();
        self.reactions.clear();
        self.programs.clear();
        self.program_bolts.clear();
        self.flares.clear();
        self.mouse_force = [0.0; 2];
    }

    /// Splashes the liquid on the next frame.
    pub fn react(&mut self, reaction: Reaction) {
        self.reactions.push(reaction);
    }

    /// Sets off a program's effect on the next frame.
    pub fn request(&mut self, fx: ProgramFx) {
        self.programs.push(fx);
    }

    /// A program's effect, each style in its own way: in the clouds colored
    /// splashes, in the oil pushes of oil, and in the water and the rain the
    /// ripples, bubbles or drops that typing makes, along a bolt's path or
    /// around a burst. A stir pushes any liquid. Bolts, bursts and ripples
    /// also glow over pictures and cells' backgrounds (lightning, sparks, a
    /// ring), since where a program draws its own pictures, or lays its own
    /// ground, they cover the liquid.
    fn program(&mut self, style: Style, p: ProgramFx, f: &FrameInput, out: &mut FrameOutput, keys: &mut Vec<((f32, f32), bool)>) {
        if style == Style::None {
            return;
        }
        let (w, h) = (f.width.max(1.0), f.height.max(1.0));
        let at = [p.pos.0 / w, p.pos.1 / h];
        let radius = (p.radius / h).clamp(0.01, 0.2);
        let s = p.strength;
        let dye = |fx: &mut Self, i: f32, amount: f32| match style {
            Style::Oil => [amount * 0.4; 3],
            _ => scale3(p.color.unwrap_or_else(|| fx.palette_at(i + f.flow_t * 0.3)), amount),
        };
        match p.kind {
            FxKind::Bolt => self.spawn_bolt_between([p.pos.0, p.pos.1], [p.to.0, p.to.1], p.color),
            FxKind::Burst | FxKind::Ripple => {
                let (pick, turn) = (self.rand01() * 5.0, self.rand01());
                let color = match p.kind {
                    FxKind::Burst => Some(p.color.unwrap_or_else(|| self.palette_at(pick))),
                    _ => p.color,
                };
                self.flares.push(Flare { kind: p.kind, pos: p.pos, radius: p.radius.max(6.0 * f.scale), color, strength: s, turn, age: 0.0 });
            }
            FxKind::Stir => {}
        }
        match (p.kind, style) {
            (_, Style::None) => {}
            (FxKind::Stir, _) => {
                let color = if matches!(style, Style::Clouds | Style::Oil) { dye(self, 0.0, 0.03 * s) } else { [0.0; 3] };
                let push = 120.0 + 600.0 * s;
                out.splats.push(Splat { pos: at, force: [p.dir.0 * push, p.dir.1 * push], color, radius: radius.max(0.04) });
            }
            (FxKind::Bolt, Style::Clouds) => {
                let color = dye(self, 1.0, 0.1 + 0.2 * s);
                out.splats.push(Splat { pos: [p.to.0 / w, p.to.1 / h], force: [0.0; 2], color, radius: 0.03 });
            }
            (FxKind::Bolt, _) => {
                let (dx, dy) = (p.to.0 - p.pos.0, p.to.1 - p.pos.1);
                let len = dx.hypot(dy);
                let n = (len / p.radius.max(8.0)).clamp(2.0, 10.0) as usize;
                // Pushed along the bolt, in texels a second like a stir.
                let push = (80.0 + 320.0 * s) / len.max(1.0);
                for i in 0..=n {
                    let t = i as f32 / n as f32;
                    let pos = (p.pos.0 + dx * t, p.pos.1 + dy * t);
                    if style == Style::Oil {
                        let color = dye(self, t, 0.1 + 0.2 * s);
                        out.splats.push(Splat { pos: [pos.0 / w, pos.1 / h], force: [dx * push, dy * push], color, radius: 0.02 });
                    } else {
                        keys.push((pos, i == n));
                    }
                }
            }
            (FxKind::Burst | FxKind::Ripple, Style::Clouds | Style::Oil) => {
                let burst = p.kind == FxKind::Burst;
                let n = if burst { 8 } else { 6 };
                let force = (120.0 + 360.0 * s) * (p.radius / (40.0 * f.scale)).clamp(0.5, 2.0);
                let turn = self.rand01();
                for i in 0..n {
                    let a = (i as f32 + turn) / n as f32 * std::f32::consts::TAU;
                    let color = if burst { dye(self, i as f32 * 0.9, 0.05 + 0.2 * s) } else { [0.0; 3] };
                    out.splats.push(Splat { pos: at, force: [a.cos() * force, a.sin() * force], color, radius: radius * 0.6 });
                }
            }
            (FxKind::Burst, _) => {
                let n = 3 + (s * 6.0) as usize;
                for i in 0..n {
                    let a = i as f32 / n as f32 * std::f32::consts::TAU + self.rand01() * 0.6;
                    let d = p.radius * (0.5 + 0.5 * self.rand01());
                    keys.push(((p.pos.0 + a.cos() * d, p.pos.1 + a.sin() * d), s > 0.5));
                }
            }
            (FxKind::Ripple, _) => {
                // A drop where it was asked, and as far and hard as asked a
                // ring of lighter ones.
                keys.push((p.pos, s >= 0.4));
                let n = ((p.radius / (16.0 * f.scale)) * s).round().clamp(0.0, 8.0) as usize;
                for i in 0..n {
                    let a = i as f32 / n as f32 * std::f32::consts::TAU;
                    keys.push(((p.pos.0 + a.cos() * p.radius, p.pos.1 + a.sin() * p.radius), false));
                }
            }
        }
    }

    /// A reaction as the keystrokes it lands like, for the styles that answer
    /// typing with ripples, bubbles or raindrops.
    fn reaction_keys(&mut self, reaction: Reaction, scale: f32, keys: &mut Vec<((f32, f32), bool)>) {
        let (pos, n, spread, heavy) = match reaction {
            Reaction::Done { pos, long: false } => (pos, 1, 0.0, true),
            Reaction::Done { pos, long: true } => (pos, 6, 70.0, true),
            // Close together, so they run into one another: in the rain, one
            // fat drop that slides away.
            Reaction::Failed { pos } => (pos, 3, 6.0, true),
            Reaction::Tool { pos } => (pos, 2, 30.0, false),
            Reaction::Bloom { pos } => (pos, 10, 130.0, true),
        };
        for i in 0..n {
            let a = i as f32 / n as f32 * std::f32::consts::TAU + self.rand01() * 0.6;
            let d = spread * scale * (0.6 + 0.4 * self.rand01());
            keys.push(((pos.0 + a.cos() * d, pos.1 + a.sin() * d), heavy));
        }
    }

    /// A reaction in the clouds or the oil: a ring of splats pushing out from
    /// where it happened, in the palette's colors (or ember for a failure),
    /// and lightning when a long command is done.
    fn burst(&mut self, style: Style, reaction: Reaction, f: &FrameInput, out: &mut FrameOutput) {
        let (pos, n, force, radius, amount) = match reaction {
            Reaction::Done { pos, long: false } => (pos, 6, 140.0, 0.03, 0.07),
            Reaction::Done { pos, long: true } => (pos, 12, 320.0, 0.04, 0.16),
            Reaction::Failed { pos } => (pos, 8, 220.0, 0.035, 0.2),
            Reaction::Tool { pos } => (pos, 4, 100.0, 0.025, 0.05),
            Reaction::Bloom { pos } => (pos, 16, 380.0, 0.045, 0.18),
        };
        let at = [pos.0 / f.width.max(1.0), pos.1 / f.height.max(1.0)];
        let turn = self.rand01();
        for i in 0..n {
            let a = (i as f32 + turn) / n as f32 * std::f32::consts::TAU;
            let color = match (style, reaction) {
                (Style::Oil, Reaction::Failed { .. }) => [0.05; 3],
                (Style::Oil, _) => [amount * 0.25; 3],
                (_, Reaction::Failed { .. }) => scale3(EMBER, amount),
                _ => scale3(self.palette_at(i as f32 * 0.9 + f.flow_t * 0.3), amount),
            };
            out.splats.push(Splat { pos: at, force: [a.cos() * force, a.sin() * force], color, radius });
        }
        if style == Style::Clouds && matches!(reaction, Reaction::Done { long: true, .. }) {
            self.spawn_bolt(f);
        }
    }

    /// Uniform in -1..1 (xorshift32).
    fn rand(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng as f32 / u32::MAX as f32) * 2.0 - 1.0
    }

    fn rand01(&mut self) -> f32 {
        (self.rand() + 1.0) * 0.5
    }

    /// A smooth walk around the palette: x = 0, 1, 2 ... land on its colors.
    fn palette_at(&self, x: f32) -> [f32; 3] {
        let n = self.palette.len() as f32;
        let x = x.rem_euclid(n);
        let i = x.floor() as usize;
        let (a, b) = (self.palette[i % self.palette.len()], self.palette[(i + 1) % self.palette.len()]);
        let t = x.fract();
        [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
    }

    /// Mouse moved by (dx, dy) pixels over `dt` seconds.
    pub fn mouse_moved(&mut self, dx: f32, dy: f32, dt: f32) {
        let dt = dt.max(1.0 / 240.0);
        self.mouse_force[0] += dx / dt * 0.02;
        self.mouse_force[1] += dy / dt * 0.02;
        self.mouse_travel += (dx * dx + dy * dy).sqrt();
    }

    /// A key was typed with the cursor at `pos` (pixels).
    pub fn keystroke(&mut self, pos: (f32, f32), enter: bool) {
        self.keys.push((pos, enter));
    }

    /// Ambient emitter `k`'s position (0..1) at flow time `t`.
    fn emitter_pos(k: usize, t: f32) -> [f32; 2] {
        let ph = t * (0.06 + 0.025 * k as f32) + k as f32 * 2.1;
        [0.5 + 0.47 * (ph * 1.3).sin(), 0.5 + 0.44 * (ph * 0.9 + 1.0).cos()]
    }

    fn emitter_dir(k: usize, t: f32) -> [f32; 2] {
        let ph = t * (0.06 + 0.025 * k as f32) + k as f32 * 2.1;
        [(ph * 1.7).cos(), (ph * 1.1).sin()]
    }

    pub fn frame(&mut self, style: Style, f: &FrameInput) -> FrameOutput {
        let mut out = FrameOutput::default();
        let (w, h) = (f.width.max(1.0), f.height.max(1.0));
        let mouse = [f.mouse.0 / w, f.mouse.1 / h];
        let force = std::mem::take(&mut self.mouse_force);
        let travel = std::mem::take(&mut self.mouse_travel);
        let mut keys = std::mem::take(&mut self.keys);
        let moving = f.focused && (force[0].abs() + force[1].abs()) > 0.5;
        // Mouse push in simulation texels per second.
        let push = [
            (force[0] * f.sim_px * 12.0).clamp(-900.0, 900.0),
            (force[1] * f.sim_px * 12.0).clamp(-900.0, 900.0),
        ];
        let amb = f.ambient;
        // Per-second rates, matched to the look the bursty version had at 1×.
        let rate = amb * (0.25 + 0.2 * amb) * 60.0 * f.flow_dt;

        // Commands ending and the assistant at work splash the liquid: the
        // clouds and the oil with bursts, the rest the way they take typing.
        for reaction in std::mem::take(&mut self.reactions) {
            match style {
                Style::Clouds | Style::Oil => self.burst(style, reaction, f, &mut out),
                Style::None => {}
                _ => self.reaction_keys(reaction, f.scale, &mut keys),
            }
        }
        // Programs' bolts and flares fade on real time, like lightning.
        for bolt in &mut self.program_bolts {
            bolt.age += f.dt;
        }
        self.program_bolts.retain(|b| b.age < BOLT_LIFE);
        for flare in &mut self.flares {
            flare.age += f.dt;
        }
        self.flares.retain(|fl| fl.age < FLARE_LIFE);
        for program in std::mem::take(&mut self.programs) {
            self.program(style, program, f, &mut out, &mut keys);
        }
        // While the assistant thinks, now and then a drop, bubble or ripple
        // turns up along the top of its panel.
        if f.activity > 0.05 && matches!(style, Style::WaterAbove | Style::WaterBelow | Style::Rain) {
            if let Some(p) = f.panel {
                if self.rand01() < 2.0 * f.activity * f.flow_dt {
                    let x = p.x + p.w * (0.08 + 0.84 * self.rand01());
                    keys.push(((x, p.y - 6.0 * f.scale), false));
                }
            }
        }

        match style {
            Style::Clouds => {
                if moving {
                    let strength = ((force[0].abs() + force[1].abs()) * 0.001).clamp(0.03, 0.22);
                    let c = self.palette_at(f.flow_t * 0.4);
                    out.splats.push(Splat { pos: mouse, force: push, color: scale3(c, strength), radius: 0.045 });
                }
                for k in 0..3 {
                    let dir = Self::emitter_dir(k, f.flow_t);
                    let c = self.palette_at(k as f32 * 1.7 + f.flow_t * 0.07);
                    out.splats.push(Splat {
                        pos: Self::emitter_pos(k, f.flow_t),
                        force: [dir[0] * 140.0 * amb * rate, dir[1] * 140.0 * amb * rate],
                        color: scale3(c, 0.06 * amb * rate),
                        radius: 0.11,
                    });
                }
                for (pos, enter) in keys {
                    let (fx, fy) = if enter { (0.0, 320.0) } else { (self.rand() * 160.0, self.rand() * 160.0) };
                    let pick = self.rand01() * 5.0;
                    let c = self.palette_at(pick);
                    out.splats.push(Splat {
                        pos: [pos.0 / w, pos.1 / h],
                        force: [fx, fy],
                        color: scale3(c, if enter { 0.28 } else { 0.12 }),
                        radius: if enter { 0.05 } else { 0.025 },
                    });
                }
                self.lightning(f, &mut out);
            }
            Style::WaterAbove | Style::WaterBelow => {
                // Gentle currents bend the light; water carries no dye.
                for k in 0..3 {
                    let dir = Self::emitter_dir(k, f.flow_t);
                    out.splats.push(Splat {
                        pos: Self::emitter_pos(k, f.flow_t),
                        force: [dir[0] * 90.0 * amb * rate, dir[1] * 90.0 * amb * rate],
                        color: [0.0; 3],
                        radius: 0.12,
                    });
                }
                if moving {
                    out.splats.push(Splat { pos: mouse, force: [push[0] * 0.5, push[1] * 0.5], color: [0.0; 3], radius: 0.05 });
                }
                if style == Style::WaterAbove {
                    // A drop per few pixels of travel rather than per frame, so a
                    // 144 Hz display doesn't stir the water harder than 60 Hz.
                    if moving && travel > 3.0 * f.scale {
                        let amp = (travel / (14.0 * f.scale)).min(0.9);
                        out.drops.push(Drop { pos: mouse, amp: -amp * 0.5, radius: 0.012 });
                    } else if moving {
                        self.mouse_travel = travel;
                    }
                    for (pos, enter) in keys {
                        out.drops.push(Drop {
                            pos: [pos.0 / w, pos.1 / h],
                            amp: if enter { -1.6 } else { -0.7 },
                            radius: if enter { 0.022 } else { 0.010 },
                        });
                    }
                    // Light rain, a few drops a second.
                    if self.rand01() < amb * 5.0 * f.flow_dt {
                        let pos = [self.rand01(), 0.08 + self.rand01() * 0.9];
                        out.drops.push(Drop { pos, amp: -0.45 - 0.35 * self.rand01(), radius: 0.008 });
                    }
                } else {
                    // A bubble every so often along the mouse's way.
                    let at_mouse = moving && travel > 30.0 * f.scale;
                    if moving && !at_mouse {
                        self.mouse_travel = travel;
                    }
                    self.bubbles.frame(f, &keys, at_mouse, &mut out.splats, &mut out.heard);
                }
            }
            Style::Oil => {
                // Thick and slow: little new oil, pushed around by the mouse.
                for k in 0..3 {
                    let dir = Self::emitter_dir(k, f.flow_t);
                    out.splats.push(Splat {
                        pos: Self::emitter_pos(k, f.flow_t),
                        force: [dir[0] * 110.0 * amb * rate, dir[1] * 110.0 * amb * rate],
                        color: scale3([1.0, 1.0, 1.0], 0.012 * amb * rate),
                        radius: 0.10,
                    });
                }
                if moving {
                    let strength = ((force[0].abs() + force[1].abs()) * 0.0004).clamp(0.01, 0.06);
                    out.splats.push(Splat { pos: mouse, force: push, color: [strength; 3], radius: 0.05 });
                }
                for (pos, enter) in keys {
                    out.splats.push(Splat {
                        pos: [pos.0 / w, pos.1 / h],
                        force: [0.0, if enter { 120.0 } else { 0.0 }],
                        color: [if enter { 0.5 } else { 0.18 }; 3],
                        radius: if enter { 0.04 } else { 0.022 },
                    });
                }
            }
            Style::Rain => {
                // Lights outside drift by slowly in the palette's colors; the
                // glass in front holds the rain, which the cursor wipes.
                for k in 0..3 {
                    let dir = Self::emitter_dir(k, f.flow_t);
                    let c = self.palette_at(k as f32 * 1.7 + f.flow_t * 0.05);
                    out.splats.push(Splat {
                        pos: Self::emitter_pos(k, f.flow_t),
                        force: [dir[0] * 50.0 * amb * rate, dir[1] * 50.0 * amb * rate],
                        color: scale3(c, 0.03 * amb * rate),
                        radius: 0.14,
                    });
                }
                self.rain.frame(f, &keys, travel, &mut out.rain, &mut out.heard);
                self.storm(f, &mut out);
            }
            // Plain glass: nothing to stir (the app doesn't run frames for it).
            Style::None => {}
        }

        // While the assistant works, a current streams along the panel's top
        // edge, and the clouds, oil and the lights outside the rain lean in.
        if f.activity > 0.05 {
            if let Some(p) = f.panel {
                if matches!(style, Style::Clouds | Style::Oil | Style::Rain) && self.rand01() < 8.0 * f.activity * f.flow_dt {
                    let x = p.x + p.w * self.rand01();
                    let color = match style {
                        Style::Oil => [0.008; 3],
                        _ => scale3(self.palette_at(x * 0.01 + f.flow_t * 0.2), 0.03),
                    };
                    out.splats.push(Splat {
                        pos: [x / w, (p.y - 50.0 * f.scale) / h],
                        force: [(p.x + p.w * 0.5 - x) * 0.3, 160.0],
                        color,
                        radius: 0.03,
                    });
                }
                let x = 0.5 + 0.45 * (f.flow_t * 0.8).sin();
                let k = f.activity * 60.0 * f.flow_dt;
                let color = match style {
                    Style::Clouds => [0.0225 * k, 0.015 * k, 0.054 * k],
                    _ => [0.0; 3],
                };
                out.splats.push(Splat {
                    pos: [(p.x + p.w * x) / w, p.y / h],
                    force: [(f.flow_t * 0.8).cos() * 260.0 * k, -60.0 * k],
                    color,
                    radius: 0.05,
                });
            }
        }
        if style != Style::WaterBelow {
            self.bubbles.clear();
        }
        if style != Style::Clouds {
            self.bolts.clear();
        }
        if style != Style::Rain {
            self.rain.clear();
            self.storm = Storm::default();
        }
        out
    }

    /// Stirs a slow whirlpool around `center` (0..1), behind the About dialog.
    pub fn whirl(&mut self, style: Style, f: &FrameInput, center: [f32; 2], out: &mut FrameOutput) {
        let k = 60.0 * f.flow_dt;
        let aspect = f.width / f.height.max(1.0);
        for i in 0..3 {
            let a = f.flow_t * 1.3 + i as f32 * std::f32::consts::TAU / 3.0;
            let pos = [center[0] + a.cos() * 0.27 / aspect, center[1] + a.sin() * 0.27];
            let force = [-a.sin() * 260.0 * k, a.cos() * 260.0 * k];
            let color = match style {
                Style::Clouds => scale3(self.palette_at(i as f32 * 1.7 + f.flow_t * 0.2), 0.05 * k),
                Style::Oil => [0.004 * k; 3],
                Style::Rain => scale3(self.palette_at(i as f32 * 1.7 + f.flow_t * 0.2), 0.02 * k),
                Style::WaterAbove | Style::WaterBelow | Style::None => [0.0; 3],
            };
            out.splats.push(Splat { pos, force, color, radius: 0.06 });
        }
        if style == Style::WaterAbove && self.rand01() < 3.0 * f.flow_dt {
            let a = self.rand01() * std::f32::consts::TAU;
            let pos = [center[0] + a.cos() * 0.3 / aspect, center[1] + a.sin() * 0.3];
            out.drops.push(Drop { pos, amp: -0.8, radius: 0.012 });
        }
    }

    fn lightning(&mut self, f: &FrameInput, out: &mut FrameOutput) {
        for bolt in &mut self.bolts {
            bolt.age += f.dt;
        }
        self.bolts.retain(|b| b.age < BOLT_LIFE);
        let e = f.electricity;
        // With electricity off no bolts come on their own, but one already
        // striking (a long command done) plays out and lights the clouds.
        if e > 0.0 && f.flow_t >= self.next_bolt {
            self.spawn_bolt(f);
            if let Some(b) = self.bolts.last() {
                out.heard.push(Heard::Thunder { x: b.main[0][0] / f.width.max(1.0), near: 1.0 });
            }
            let mut interval = 1.2 + (1.0 - e) * 6.0;
            if f.activity > 0.5 {
                interval *= 0.5;
            }
            self.next_bolt = f.flow_t + interval * (0.5 + self.rand01());
        }
        let all = self.bolts.iter().chain(&self.program_bolts);
        if let Some(b) = all.max_by(|a, b| bolt_intensity(a.age).total_cmp(&bolt_intensity(b.age))) {
            let mid = b.main[b.main.len() / 2];
            out.flash = [mid[0] / f.width.max(1.0), mid[1] / f.height.max(1.0), bolt_intensity(b.age) * e.max(0.6), 0.26];
        }
    }

    /// The rain's storm: with Electricity up and the rain heavy enough
    /// (`rain::storminess`), lightning strikes far off now and then,
    /// lighting the night through the glass (`FrameOutput::flash`, which
    /// the glass shader lights its night with, and so every drop showing
    /// it), and its thunder comes after: sooner and louder the nearer.
    fn storm(&mut self, f: &FrameInput, out: &mut FrameOutput) {
        for flash in &mut self.storm.flashes {
            flash.0 += f.dt;
        }
        self.storm.flashes.retain(|flash| flash.0 < BOLT_LIFE * 1.5);
        // Sound comes in real time, whatever the liquid's speed.
        let mut due = Vec::new();
        self.storm.thunder.retain_mut(|thunder| {
            thunder.0 -= f.dt;
            if thunder.0 > 0.0 {
                return true;
            }
            due.push(*thunder);
            false
        });
        out.heard.extend(due.into_iter().map(|(_, x, near)| Heard::Thunder { x, near }));
        let (e, storm) = (f.electricity, rain::storminess(f.rain));
        if e > 0.0 && storm > 0.0 {
            // The first a little after the rain sets in.
            if self.storm.next <= 0.0 {
                self.storm.next = f.flow_t + 2.0;
            }
            if f.flow_t >= self.storm.next {
                let (x, near) = (0.1 + 0.8 * self.rand01(), self.rand01());
                self.storm.flashes.push((0.0, x, near));
                self.storm.thunder.push((STORM_NEAREST + (STORM_FARTHEST - STORM_NEAREST) * (1.0 - near), x, near));
                let interval = (3.0 + 15.0 * (1.0 - e)) / (0.4 + 0.6 * storm);
                self.storm.next = f.flow_t + interval * (0.5 + self.rand01());
            }
        }
        let brightest = self.storm.flashes.iter().map(|&(age, x, near)| (bolt_intensity(age) * (0.35 + 0.65 * near), x)).max_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((intensity, x)) = brightest {
            out.flash = [x, 0.0, intensity, 0.0];
        }
    }

    /// Starts a bolt next to a cloud (an emitter) right away (snapshots use this).
    pub fn spawn_bolt(&mut self, f: &FrameInput) {
        let (w, h) = (f.width, f.height);
        let k = ((self.rand01() * 3.0) as usize).min(2);
        let p = Self::emitter_pos(k, f.flow_t);
        let start = [(p[0] * w).clamp(0.08 * w, 0.92 * w), (p[1] * h).clamp(0.12 * h, 0.9 * h)];
        let ang = self.rand() * std::f32::consts::PI;
        let len = (0.14 + 0.2 * self.rand01()) * w.min(h);
        let end = [
            (start[0] + ang.cos() * len).clamp(0.04 * w, 0.96 * w),
            (start[1] + ang.sin() * len).clamp(0.08 * h, 0.96 * h),
        ];
        let main = self.jagged(start, end, 6, len * 0.16);
        let mut branches = Vec::new();
        for _ in 0..(1 + (self.rand01() * 2.5) as usize) {
            let i = 2 + (self.rand01() * (main.len() as f32 - 4.0)) as usize;
            let from = main[i.min(main.len() - 1)];
            let side = if self.rand() > 0.0 { 0.7 } else { -0.7 };
            let a = ang + side + self.rand() * 0.35;
            let bl = len * (0.25 + 0.25 * self.rand01());
            let to = [from[0] + a.cos() * bl, from[1] + a.sin() * bl];
            branches.push(self.jagged(from, to, 4, bl * 0.18));
        }
        self.bolts.push(Bolt { main, branches, age: 0.0, color: None });
    }

    /// Starts a program's bolt from `a` to `b` (pixels), with a branch, in
    /// `color` or the accent's.
    fn spawn_bolt_between(&mut self, a: [f32; 2], b: [f32; 2], color: Option<[f32; 3]>) {
        let len = (b[0] - a[0]).hypot(b[1] - a[1]).max(1.0);
        let main = self.jagged(a, b, 5, len * 0.16);
        let from = main[main.len() / 3];
        let ang = (b[1] - a[1]).atan2(b[0] - a[0]) + if self.rand() > 0.0 { 0.7 } else { -0.7 };
        let to = [from[0] + ang.cos() * len * 0.3, from[1] + ang.sin() * len * 0.3];
        let branches = vec![self.jagged(from, to, 3, len * 0.05)];
        self.program_bolts.push(Bolt { main, branches, age: 0.0, color });
    }

    /// Midpoint displacement: a crooked path from a to b.
    fn jagged(&mut self, a: [f32; 2], b: [f32; 2], levels: u32, spread: f32) -> Vec<[f32; 2]> {
        let mut pts = vec![a, b];
        let mut amount = spread;
        for _ in 0..levels {
            let mut next = Vec::with_capacity(pts.len() * 2);
            for pair in pts.windows(2) {
                let (p, q) = (pair[0], pair[1]);
                let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
                let len = (dx * dx + dy * dy).sqrt().max(1e-3);
                let off = self.rand() * amount;
                next.push(p);
                next.push([(p[0] + q[0]) * 0.5 - dy / len * off, (p[1] + q[1]) * 0.5 + dx / len * off]);
            }
            next.push(*pts.last().unwrap());
            pts = next;
            amount *= 0.55;
        }
        pts
    }

    /// Bubbles and bolts, drawn behind the terminal's pictures, grounds and
    /// text.
    pub fn draw(&self, c: &mut Canvas<'_>, scale: f32, accent: Rgba) {
        self.bubbles.draw(c);
        for bolt in &self.bolts {
            draw_bolt(c, bolt, scale, accent);
        }
    }

    /// What programs set off, over pictures and cells' backgrounds
    /// (`Canvas::ground`) but under text and panels: their bolts, the sparks
    /// of their bursts and the rings of their ripples.
    pub fn draw_over(&self, c: &mut Canvas<'_>, scale: f32, accent: Rgba) {
        for bolt in &self.program_bolts {
            draw_bolt(c, bolt, scale, accent);
        }
        let tau = std::f32::consts::TAU;
        for f in &self.flares {
            let t = (f.age / FLARE_LIFE).clamp(0.0, 1.0);
            let fade = (1.0 - t) * (1.0 - t);
            let spread = 1.0 - (1.0 - t).powi(3);
            let [r, g, b] = f.color.unwrap_or([accent[0] * 0.6 + 0.3, accent[1] * 0.6 + 0.35, 1.0]);
            let around = |a: f32, d: f32| [f.pos.0 + a.cos() * d, f.pos.1 + a.sin() * d];
            match f.kind {
                FxKind::Ripple => {
                    let d = f.radius * (0.2 + 0.8 * spread);
                    let color = [r, g, b, fade * (0.35 + 0.65 * f.strength)];
                    let n = 32;
                    for i in 0..n {
                        let (a0, a1) = (i as f32 / n as f32 * tau, (i + 1) as f32 / n as f32 * tau);
                        c.segment(around(a0, d), around(a1, d), color, 0.9 * scale, (3.0 + 5.0 * f.strength) * scale);
                    }
                }
                _ => {
                    let glow = f.radius * 0.6 * (1.0 - t);
                    if glow > 1.0 {
                        let color = [r, g, b, fade * 0.5 * f.strength.max(0.3)];
                        c.glow(f.pos.0 - glow, f.pos.1 - glow, 2.0 * glow, 2.0 * glow, color, glow, glow);
                    }
                    let n = 6 + (f.strength * 8.0) as usize;
                    let color = [r, g, b, fade * (0.5 + 0.5 * f.strength)];
                    let far = f.radius * (0.25 + 0.75 * spread);
                    let near = far - f.radius * 0.3 * (1.0 - t);
                    for i in 0..n {
                        let a = (i as f32 + f.turn) / n as f32 * tau;
                        c.segment(around(a, near), around(a, far), color, scale, 4.0 * scale);
                    }
                }
            }
        }
    }
}

/// A bolt of lightning as it strobes, in its color or the accent's.
fn draw_bolt(c: &mut Canvas<'_>, bolt: &Bolt, scale: f32, accent: Rgba) {
    let i = bolt_intensity(bolt.age);
    if i < 0.02 {
        return;
    }
    let color = match bolt.color {
        Some(c) => [c[0], c[1], c[2], i],
        None => [accent[0] * 0.6 + 0.3, accent[1] * 0.6 + 0.35, 1.0, i],
    };
    for pair in bolt.main.windows(2) {
        c.segment(pair[0], pair[1], color, 1.3 * scale, 10.0 * scale);
    }
    for branch in &bolt.branches {
        let dim = [color[0], color[1], color[2], i * 0.7];
        for pair in branch.windows(2) {
            c.segment(pair[0], pair[1], dim, 0.8 * scale, 6.0 * scale);
        }
    }
}

/// A lightning strobe: flash, dip, second flash, then a quick fade.
fn bolt_intensity(age: f32) -> f32 {
    match age {
        a if a < 0.05 => 1.0,
        a if a < 0.09 => 0.25,
        a if a < 0.14 => 0.9,
        a => 0.9 * (-(a - 0.14) * 12.0).exp(),
    }
}

fn scale3(c: [f32; 3], s: f32) -> [f32; 3] {
    [c[0] * s, c[1] * s, c[2] * s]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(flow_dt: f32) -> FrameInput {
        FrameInput {
            flow_dt,
            flow_t: 10.0,
            dt: 1.0 / 60.0,
            ambient: 0.6,
            rain: 0.0,
            electricity: 0.0,
            width: 1000.0,
            height: 600.0,
            scale: 1.0,
            focused: true,
            mouse: (0.0, 0.0),
            activity: 0.0,
            panel: None,
            sim_px: 0.2,
        }
    }

    #[test]
    fn a_storm_in_heavy_rain_lights_the_night_and_its_thunder_comes_after() {
        // A minute of rain at `amount` with Electricity at `e`: the frames
        // lit, and each flash's start and its thunder's, with how near.
        let storm = |amount: f32, e: f32| {
            let mut fx = Effects::new("aurora");
            let (mut lit, mut flashes, mut thunder) = (0, Vec::new(), Vec::new());
            let mut was = 0.0;
            for i in 0..3600 {
                let f = FrameInput { flow_t: i as f32 / 60.0, rain: amount, electricity: e, ..input(1.0 / 60.0) };
                let mut out = FrameOutput::default();
                fx.storm(&f, &mut out);
                if out.flash[2] > 0.0 {
                    lit += 1;
                    if was == 0.0 {
                        flashes.push(i);
                    }
                }
                was = out.flash[2];
                thunder.extend(out.heard.iter().filter_map(|h| match *h {
                    Heard::Thunder { near, .. } => Some((i, near)),
                    _ => None,
                }));
            }
            (lit, flashes, thunder)
        };
        // None in a drizzle, nor with Electricity off.
        assert_eq!(storm(0.15, 1.0).0, 0);
        assert_eq!(storm(1.0, 0.0).0, 0);
        // A downpour with Electricity up: several strikes, each lit briefly,
        // each thunder after its flash, later the farther off.
        let (lit, flashes, thunder) = storm(1.0, 0.8);
        assert!((4..=20).contains(&flashes.len()), "{} strikes in a minute", flashes.len());
        assert!(lit < flashes.len() * 60, "each lit for under a second: {lit} frames");
        assert!(!thunder.is_empty() && thunder.len() <= flashes.len());
        for (at, near) in &thunder {
            assert!(flashes.iter().any(|&f| f <= *at), "a flash before its thunder");
            assert!((0.0..=1.0).contains(near));
        }
        let first_wait = thunder[0].0 - flashes[0];
        let expected = (STORM_NEAREST + (STORM_FARTHEST - STORM_NEAREST) * (1.0 - thunder[0].1)) * 60.0;
        assert!((first_wait as f32 - expected).abs() <= 2.0, "{first_wait} frames after, {expected} expected");
    }

    #[test]
    fn commands_and_the_assistant_splash_each_style_its_own_way() {
        let f = input(1.0 / 60.0);
        let pos = (300.0, 200.0);
        let at = |s: &Splat| (s.pos[0] - 0.3).abs() < 1e-4 && (s.pos[1] - 200.0 / 600.0).abs() < 1e-4;

        // The clouds: a ring of splats from the prompt, ember for a failure.
        let mut fx = Effects::new("aurora");
        fx.react(Reaction::Failed { pos });
        let out = fx.frame(Style::Clouds, &f);
        let ring: Vec<&Splat> = out.splats.iter().filter(|s| at(s)).collect();
        assert_eq!(ring.len(), 8);
        assert!(ring.iter().all(|s| s.color[0] > s.color[2] * 3.0), "not ember: {:?}", ring[0].color);
        // A long command's success brings lightning along.
        let mut fx = Effects::new("aurora");
        fx.react(Reaction::Done { pos, long: true });
        fx.frame(Style::Clouds, &f);
        assert_eq!(fx.bolts.len(), 1);

        // Water seen from above: a ring of ripples.
        let mut fx = Effects::new("aurora");
        fx.react(Reaction::Bloom { pos });
        assert!(fx.frame(Style::WaterAbove, &f).drops.len() >= 10);

        // Rain: a failure's drops land close enough to run together.
        let mut fx = Effects::new("aurora");
        fx.react(Reaction::Failed { pos });
        let out = fx.frame(Style::Rain, &f);
        let heavy = out.rain.iter().any(|q| {
            q.params[2] == RainQuad::DROP && (q.ends[0] - pos.0).hypot(q.ends[1] - pos.1) < 25.0 && q.params[0] > 7.0
        });
        assert!(heavy, "no heavy drop by the prompt");

        // None has nothing to splash, and switching the liquid off drops
        // what was waiting.
        let mut fx = Effects::new("aurora");
        fx.react(Reaction::Tool { pos });
        let out = fx.frame(Style::None, &f);
        assert!(out.splats.is_empty() && out.drops.is_empty() && out.rain.is_empty());
        fx.react(Reaction::Tool { pos });
        fx.clear();
        assert!(fx.reactions.is_empty());
    }

    #[test]
    fn programs_effects_land_where_they_asked_in_each_style() {
        let f = input(1.0 / 60.0);
        let effect = |kind, strength| ProgramFx { kind, pos: (300.0, 200.0), to: (600.0, 300.0), radius: 30.0, color: Some([1.0, 0.0, 0.0]), dir: (1.0, 0.0), strength };
        let near = |s: &Splat, x: f32, y: f32| (s.pos[0] - x / 1000.0).abs() < 1e-4 && (s.pos[1] - y / 600.0).abs() < 1e-4;

        // The clouds: a bolt between the two cells in the program's color, a
        // burst of red splats, a ripple that pushes without dye.
        let mut fx = Effects::new("aurora");
        fx.request(effect(FxKind::Bolt, 0.5));
        fx.frame(Style::Clouds, &f);
        let bolt = &fx.program_bolts[0];
        assert_eq!((bolt.main[0], *bolt.main.last().unwrap(), bolt.color), ([300.0, 200.0], [600.0, 300.0], Some([1.0, 0.0, 0.0])));
        assert!(fx.bolts.is_empty(), "not one of the clouds' own");
        fx.request(effect(FxKind::Burst, 1.0));
        let out = fx.frame(Style::Clouds, &f);
        let burst: Vec<&Splat> = out.splats.iter().filter(|s| near(s, 300.0, 200.0)).collect();
        assert_eq!(burst.len(), 8);
        assert!(burst.iter().all(|s| s.color[0] > 0.0 && s.color[1] == 0.0 && s.color[2] == 0.0));
        fx.request(effect(FxKind::Ripple, 1.0));
        let out = fx.frame(Style::Clouds, &f);
        let ripple: Vec<&Splat> = out.splats.iter().filter(|s| near(s, 300.0, 200.0)).collect();
        assert_eq!(ripple.len(), 6);
        assert!(ripple.iter().all(|s| s.color == [0.0; 3] && s.force[0].hypot(s.force[1]) > 100.0));
        // A stir pushes the way it was told.
        fx.request(effect(FxKind::Stir, 1.0));
        let out = fx.frame(Style::Clouds, &f);
        let stir = out.splats.iter().find(|s| near(s, 300.0, 200.0)).unwrap();
        assert!(stir.force[0] > 500.0 && stir.force[1] == 0.0);

        // Water from above: ripples along a bolt's path, from one end to the
        // other, and the bolt itself to show over pictures.
        let mut fx = Effects::new("aurora");
        fx.request(effect(FxKind::Bolt, 0.5));
        let drops = fx.frame(Style::WaterAbove, &f).drops;
        assert!(drops.len() >= 3);
        assert!(drops.iter().any(|d| (d.pos[0] - 0.3).abs() < 1e-4) && drops.iter().any(|d| (d.pos[0] - 0.6).abs() < 1e-4));
        assert_eq!(fx.program_bolts.len(), 1);
        // A ripple reaches as far, and as hard, as asked.
        let ripple_drops = |strength| {
            let mut fx = Effects::new("aurora");
            fx.request(ProgramFx { radius: 64.0, ..effect(FxKind::Ripple, strength) });
            fx.frame(Style::WaterAbove, &f).drops.iter().filter(|d| (d.pos[0] * 1000.0 - 300.0).hypot(d.pos[1] * 600.0 - 200.0) < 70.0).count()
        };
        assert!(ripple_drops(1.0) >= 4 && ripple_drops(0.1) == 1, "{} {}", ripple_drops(1.0), ripple_drops(0.1));

        // The oil: a bolt pushes oil along its way, as hard whatever its length.
        let mut fx = Effects::new("aurora");
        fx.request(effect(FxKind::Bolt, 1.0));
        let out = fx.frame(Style::Oil, &f);
        let along: Vec<&Splat> = out.splats.iter().filter(|s| s.radius == 0.02).collect();
        assert!(along.len() >= 3);
        for s in &along {
            let push = s.force[0].hypot(s.force[1]);
            assert!((push - 400.0).abs() < 1.0 && s.color[0] == s.color[1], "{push} {:?}", s.color);
        }

        // Bursts and ripples also glow over pictures, and fade.
        let mut fx = Effects::new("aurora");
        fx.request(effect(FxKind::Burst, 1.0));
        fx.request(ProgramFx { color: None, ..effect(FxKind::Ripple, 1.0) });
        fx.frame(Style::Rain, &f);
        assert_eq!(fx.flares.len(), 2);
        assert_eq!((fx.flares[0].color, fx.flares[1].color), (Some([1.0, 0.0, 0.0]), None));
        let later = FrameInput { dt: FLARE_LIFE, ..input(1.0 / 60.0) };
        fx.frame(Style::Rain, &later);
        assert!(fx.flares.is_empty());

        // Plain glass: nothing at all.
        let mut fx = Effects::new("aurora");
        for kind in [FxKind::Bolt, FxKind::Burst, FxKind::Ripple, FxKind::Stir] {
            fx.request(effect(kind, 1.0));
        }
        let out = fx.frame(Style::None, &f);
        assert!(out.splats.is_empty() && out.drops.is_empty() && out.rain.is_empty());
        assert!(fx.flares.is_empty() && fx.program_bolts.is_empty());
    }

    #[test]
    fn a_program_gets_a_few_effects_a_second_after_a_short_burst() {
        let start = Instant::now();
        let mut allowance = Allowance::default();
        let granted = (0..20).filter(|_| allowance.take(start)).count();
        assert_eq!(granted, 8);
        // Half a second later, three more.
        let later = start + std::time::Duration::from_millis(500);
        assert_eq!((0..20).filter(|_| allowance.take(later)).count(), 3);
    }

    /// Slowing the flow must slow the dye going in, every frame, not in bursts.
    #[test]
    fn clouds_inject_continuously_and_scale_with_speed() {
        let mut fx = Effects::new("aurora");
        let normal = fx.frame(Style::Clouds, &input(1.0 / 60.0));
        let slow = fx.frame(Style::Clouds, &input(0.1 / 60.0));
        assert_eq!(normal.splats.len(), 3);
        assert_eq!(slow.splats.len(), 3);
        let dye = |o: &FrameOutput| o.splats.iter().map(|s| s.color.iter().sum::<f32>()).sum::<f32>();
        let ratio = dye(&slow) / dye(&normal);
        assert!((ratio - 0.1).abs() < 1e-3, "ratio {ratio}");
    }

    #[test]
    fn styles_parse() {
        assert_eq!(Style::from_config("water", "below"), Style::WaterBelow);
        assert_eq!(Style::from_config("water", "above"), Style::WaterAbove);
        assert_eq!(Style::from_config("oil", ""), Style::Oil);
        assert_eq!(Style::from_config("rain", "above"), Style::Rain);
        assert_eq!(Style::from_config("none", ""), Style::None);
        assert_eq!(Style::from_config("whatever", ""), Style::Clouds);
    }

    #[test]
    fn bolts_are_connected_paths() {
        let mut fx = Effects::new("aurora");
        let f = input(1.0 / 60.0);
        fx.spawn_bolt(&f);
        let bolt = &fx.bolts[0];
        assert_eq!(bolt.main.len(), 65);
        assert!(bolt_intensity(0.0) > 0.9 && bolt_intensity(BOLT_LIFE) < 0.05);
    }
}
