//! Flow's own sounds, made as they play rather than recorded: the cues for
//! its events, the sounds that go with the liquid's effects, and the notes
//! and named sounds programs ask for. Each is a [`Cue`], played as a
//! [`Voice`] that makes its samples from a few oscillators, filtered noise
//! and envelopes, panned to where it happened.
//!
//! [`Sound`] lives with the app and hands each voice to a thread of its own,
//! started the first time there's something to play. That thread opens the
//! system's output when a voice comes, and closes it again once it has been
//! quiet a few seconds, so an idle Flow keeps no audio device busy. On
//! macOS and Windows the output is cpal's, whose callback, on the system's
//! audio thread, mixes the voices ([`Mixer`]) without allocating; on Linux
//! it's ALSA's, loaded as Flow runs (the SDK's `alsa.rs`), which a thread
//! of the output's own fills the same way.

use std::f32::consts::{PI, TAU};
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};

/// What to play.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Cue {
    /// A drop into water: a ripple.
    Drop,
    /// Water splashing: a burst.
    Splash,
    /// A crack and rolling thunder: a bolt.
    Thunder,
    /// Air rushing past: a stir.
    Whoosh,
    /// Two bell notes rising: a long job done.
    Chime,
    /// Two soft notes falling: a command failed.
    Low,
    /// Two bell notes, asking: the assistant waits for an answer.
    Ding,
    /// A dull knock.
    Thud,
    /// A key's click.
    Click,
    /// A spring's boing.
    Boing,
    /// Flow's own bell, when the bell isn't the system's.
    Bell,
    /// A raindrop's tap on the glass.
    Patter,
    /// A bubble's bloop under the water.
    Bubble,
    /// A teletype's typebar striking the paper.
    Clack,
    /// A tube's mains hum, a few seconds of it, fading in and out so one
    /// laid over the last carries on.
    Hum,
    /// A teletype's margin bell, struck as the carriage nears the line's end.
    Margin,
    /// A teletype's carriage running back to the line's start, then the
    /// platen turning up a line.
    Return,
    /// A disk drive's head stepping across the disk as the machine starts,
    /// over its motor.
    Seek,
    /// A teletype's paper tape punch punching a row of holes.
    Punch,
    /// A dot-matrix printer's head buzzing across a line, then the paper
    /// fed on.
    Print,
    /// A PC's fan, a few seconds of it, laid as the hum is: its blades'
    /// tone and half its air.
    Fan,
    /// The other half of a PC's fan's air, laid on a spacing of its own
    /// ([`FAN_LAYERS`]), so where one is laid on the last never comes with
    /// `Fan`'s and the air never swells and sinks at a beat.
    FanAir,
    /// A video terminal's key click, from its own speaker: sharper than a
    /// keyboard's.
    Tick,
    /// A terminal's or a PC's speaker beeping for the bell; `size` 0 is a
    /// terminal's lower tone, 1 a PC speaker's higher one.
    Beep,
    /// A graphics workstation's chord as it starts, swelling and dying away.
    Startup,
    /// Rain too fast to hear drop by drop: its hiss on the glass, a few
    /// seconds of it laid as the hum is; `size` is how hard it rains.
    Rainfall,
    /// An empty floppy drive's head stepping out a track and back, to see
    /// whether a disk has gone in: two knocks hard on each other.
    Step,
    /// A note of `hz` held `secs` (DECPS).
    Note { hz: f32, secs: f32 },
}

/// The cues a program can ask for by name.
pub const NAMES: [(&str, Cue); 14] = [
    ("drop", Cue::Drop),
    ("splash", Cue::Splash),
    ("thunder", Cue::Thunder),
    ("whoosh", Cue::Whoosh),
    ("chime", Cue::Chime),
    ("low", Cue::Low),
    ("ding", Cue::Ding),
    ("thud", Cue::Thud),
    ("click", Cue::Click),
    ("boing", Cue::Boing),
    ("bell", Cue::Bell),
    ("patter", Cue::Patter),
    ("bubble", Cue::Bubble),
    ("clack", Cue::Clack),
];

/// How many kinds `Cue::kind` tells apart: the named ones, notes, the
/// machines' own (the hum, the margin bell, the return, the seek, the
/// punch, the printer, the fan and its other air, the tick, the beep, the
/// startup chord and the empty drive's step), and the rain's hiss.
const KINDS: usize = NAMES.len() + 14;

impl Cue {
    pub fn named(name: &str) -> Option<Self> {
        NAMES.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, c)| *c)
    }

    /// How long it sounds, in seconds.
    pub fn length(self) -> f32 {
        match self {
            Cue::Drop => 0.25,
            Cue::Splash => 0.7,
            Cue::Thunder => 2.8,
            Cue::Whoosh => 0.75,
            Cue::Chime => 2.0,
            Cue::Low => 0.6,
            Cue::Ding => 1.6,
            Cue::Thud => 0.35,
            Cue::Click => 0.06,
            Cue::Boing => 1.0,
            Cue::Bell => 0.8,
            Cue::Patter => 0.05,
            Cue::Bubble => 0.14,
            Cue::Clack => 0.09,
            Cue::Hum => HUM,
            Cue::Margin => 1.2,
            Cue::Return => RUN + 0.3,
            Cue::Seek => 1.4,
            Cue::Punch => 0.09,
            Cue::Print => PRINT + 0.12,
            Cue::Fan => HUM,
            Cue::FanAir => FAN_AIR,
            Cue::Tick => 0.03,
            Cue::Beep => 0.22,
            Cue::Startup => 3.2,
            Cue::Rainfall => HUM,
            Cue::Step => 0.09,
            Cue::Note { secs, .. } => secs + 0.02,
        }
    }

    /// Which of the kinds it is, for keeping a burst of one kind in check.
    fn kind(self) -> usize {
        match self {
            Cue::Note { .. } => NAMES.len(),
            Cue::Hum => NAMES.len() + 1,
            Cue::Margin => NAMES.len() + 2,
            Cue::Return => NAMES.len() + 3,
            Cue::Seek => NAMES.len() + 4,
            Cue::Punch => NAMES.len() + 5,
            Cue::Print => NAMES.len() + 6,
            Cue::Fan => NAMES.len() + 7,
            Cue::Tick => NAMES.len() + 8,
            Cue::Beep => NAMES.len() + 9,
            Cue::Startup => NAMES.len() + 10,
            Cue::Rainfall => NAMES.len() + 11,
            Cue::Step => NAMES.len() + 12,
            Cue::FanAir => NAMES.len() + 13,
            c => NAMES.iter().position(|(_, n)| *n == c).unwrap_or(0),
        }
    }
}

/// The VT520's note `n` for DECPS: 1 is C5, each one a semitone up to C7
/// at 25; 0 is a rest.
pub fn note_hz(n: u8) -> Option<f32> {
    (1..=25).contains(&n).then(|| 523.25 * 2f32.powf((n - 1) as f32 / 12.0))
}

/// One sound playing.
#[derive(Debug, Clone)]
pub struct Voice {
    cue: Cue,
    /// Left and right, an equal-power pan.
    sides: [f32; 2],
    gain: f32,
    /// 0..1: bigger is lower and longer where the cue says.
    size: f32,
    /// Seconds before it starts, then seconds since it started (counted
    /// in f64: a cue's thousands of steps would drift in f32).
    wait: f64,
    t: f64,
    phase: f32,
    seed: u32,
    /// Its own shade, 0..1 each, from its seed: a raindrop's ring and
    /// length, the rain's swell, so no two sound alike.
    vary: [f32; 2],
    filters: [f32; 4],
    /// Where it starts on the mixer's clock, in seconds (`Mixer::add`):
    /// a tube's hum and a PC's fan drift with that clock, not their own, so
    /// one laid on the last is in step with it.
    clock: f64,
}

impl Voice {
    /// `cue` at `pan` (-1 left to 1 right), `gain` 0..1, `size` 0..1,
    /// starting after `delay` seconds.
    pub fn new(cue: Cue, pan: f32, gain: f32, size: f32, delay: f32) -> Self {
        let angle = (pan.clamp(-1.0, 1.0) + 1.0) * PI * 0.25;
        Self {
            cue,
            sides: [angle.cos(), angle.sin()],
            gain: gain.clamp(0.0, 1.0),
            size: size.clamp(0.0, 1.0),
            wait: f64::from(delay.max(0.0)),
            t: 0.0,
            phase: 0.0,
            seed: 1,
            vary: [0.5; 2],
            filters: [0.0; 4],
            clock: 0.0,
        }
        .seeded((0x9e37_79b9 ^ pan.to_bits().rotate_left(7) ^ delay.to_bits()) | 1)
    }

    /// The same with its noise and its shade from `seed`: [`Sound`] gives
    /// each voice it plays a seed of its own, so the rain's hiss laid on
    /// again, or a drop tapping where the last did, isn't the same sound.
    pub fn seeded(mut self, seed: u32) -> Self {
        self.seed = seed | 1;
        let a = self.noise() * 0.5 + 0.5;
        let b = self.noise() * 0.5 + 0.5;
        self.vary = [a, b];
        self
    }

    /// The same starting at `start` on the mixer's clock, as `Mixer::add`
    /// has it.
    pub fn at(mut self, start: f64) -> Self {
        self.clock = start;
        self
    }

    /// Its next sample (left and right), `dt` seconds on; none once it's done.
    pub fn next(&mut self, dt: f32) -> Option<[f32; 2]> {
        if self.wait > 0.0 {
            self.wait -= f64::from(dt);
            return Some([0.0; 2]);
        }
        if self.t >= f64::from(self.cue.length()) {
            return None;
        }
        let s = self.sample(dt) * self.gain;
        self.t += f64::from(dt);
        Some([s * self.sides[0], s * self.sides[1]])
    }

    fn sample(&mut self, dt: f32) -> f32 {
        let (t, size) = (self.t as f32, self.size);
        match self.cue {
            Cue::Drop => {
                // The pitch springs up as the drop goes in.
                let hz = 560.0 / (1.0 + size) * (1.0 + 1.5 * (1.0 - (-t / 0.018).exp()));
                let tick = self.noise() * env(t, 0.0003, 0.002) * 0.3;
                self.sine(hz, dt) * env(t, 0.002, 0.06) * 0.8 + tick
            }
            Cue::Splash => {
                let n = self.noise();
                let bright = self.low_pass(0, n, 500.0 + 5000.0 * (-t / 0.1).exp(), dt);
                let deep = self.low_pass(1, bright, 180.0, dt);
                (bright - deep) * (env(t, 0.004, 0.13) + 0.3 * env(t, 0.03, 0.35)) * 1.4
            }
            Cue::Thunder => {
                let n = self.noise();
                let crack = (n - self.low_pass(0, n, 1500.0, dt)) * env(t, 0.001, 0.04) * 0.7;
                let rumble = self.low_pass(1, n, 110.0 + 60.0 * (1.0 - size), dt);
                let rumble = self.low_pass(2, rumble, 160.0, dt);
                let flutter = self.low_pass(3, n, 4.0, dt);
                let swell = (0.6 + 40.0 * flutter).clamp(0.1, 1.5);
                crack + rumble * swell * env(t, 0.12, 0.7 + 0.5 * size) * 6.0
            }
            Cue::Whoosh => {
                let n = self.noise();
                let swell = (PI * t / Cue::Whoosh.length()).sin().powi(2);
                let band = self.low_pass(0, n, 300.0 + 2500.0 * swell, dt) - self.low_pass(1, n, 200.0, dt);
                band * swell * 1.6
            }
            Cue::Chime => (bell(t, 880.0, 1.0) + bell(t - 0.13, 1318.5, 1.0)) * 0.3,
            Cue::Low => {
                let tone = |t: f32, hz: f32| if t < 0.0 { 0.0 } else { (TAU * hz * t).sin() + 0.2 * (3.0 * TAU * hz * t).sin() };
                let second = t - 0.15;
                (tone(t, 220.0) * env(t, 0.008, 0.09) + tone(second, 165.0) * env(second.max(0.0), 0.008, 0.16)) * 0.45
            }
            Cue::Ding => (bell(t, 784.0, 0.8) + bell(t - 0.16, 1046.5, 0.9)) * 0.3,
            Cue::Thud => {
                let hz = (55.0 + 75.0 * (-t / 0.025).exp()) / (1.0 + 0.5 * size);
                let knock = self.noise();
                let knock = self.low_pass(0, knock, 1800.0, dt) * env(t, 0.0005, 0.008) * 2.0;
                (self.sine(hz, dt) * env(t, 0.002, 0.08) + knock) * 0.85
            }
            Cue::Click => {
                let n = self.noise();
                let hiss = n - self.low_pass(0, n, 2500.0, dt);
                hiss * env(t, 0.0002, 0.003) * 0.7 + (TAU * 1900.0 * t).sin() * env(t, 0.0005, 0.01) * 0.25
            }
            Cue::Boing => {
                // A spring: its pitch drops away and wobbles as it settles.
                let wobble = 1.0 + 0.08 * (TAU * 7.0 * t).sin() * (-t / 0.35).exp();
                let hz = (90.0 + 300.0 * (-t / 0.04).exp()) * wobble;
                self.phase = (self.phase + hz * dt).fract();
                let x = TAU * self.phase;
                (x.sin() + 0.3 * (2.0 * x).sin()) * env(t, 0.003, 0.28) * 0.7
            }
            Cue::Bell => bell(t, 1568.0, 0.4) * 0.3,
            Cue::Patter => {
                // A tick high up, and a glassy ring just after: higher for
                // a small drop, and each drop's a little its own, its ring
                // higher or lower, its tick shorter or longer, softer or
                // louder.
                let [a, b] = self.vary;
                let n = self.noise();
                let tick = n - self.low_pass(0, n, 2200.0 + 1800.0 * b, dt);
                let ring = self.sine((2600.0 + 1400.0 * (1.0 - size)) * (0.8 + 0.4 * a), dt) * env(t, 0.0005, 0.004 + 0.004 * b) * 0.25;
                (tick * env(t, 0.0002, 0.003 + 0.002 * a) * 0.6 + ring) * (0.75 + 0.5 * b)
            }
            Cue::Bubble => {
                // The pitch rises as the bubble shrinks to the surface.
                let hz = (380.0 + 900.0 * (t / 0.05).min(1.0)) / (1.0 + size);
                self.sine(hz, dt) * env(t, 0.002, 0.03) * 0.6
            }
            Cue::Clack => {
                // The typebar's knock, its metal's ring, and the platen's thump.
                let n = self.noise();
                let knock = self.low_pass(0, n, 2200.0, dt) * env(t, 0.0003, 0.006) * 1.6;
                let ring = ((TAU * 2400.0 * t).sin() * 0.25 + (TAU * 3700.0 * t).sin() * 0.15) * env(t, 0.0005, 0.02);
                knock + ring + (TAU * 120.0 * t).sin() * env(t, 0.001, 0.02) * 0.3
            }
            Cue::Hum => {
                // The mains' 60 Hz and its harmonics, never quite the same:
                // its level, the balance of its harmonics and, a little, its
                // pitch drift with the mixer's clock, slowly enough not to
                // be heard as a wobble, and alike in every hum laid on.
                let fade = (t / 0.5).min(1.0) * ((HUM - t) / 0.5).clamp(0.0, 1.0);
                let at = self.clock + self.t;
                let turns = hum_turns(at);
                let harmonic = |k: f64| (TAU * (k * turns).fract() as f32).sin();
                let [level, second, third] = hum_drift(at);
                (harmonic(1.0) * 0.6 + harmonic(2.0) * second + harmonic(3.0) * third) * fade * 0.25 * level
            }
            // A small bell, high and bright, struck once.
            Cue::Margin => bell(t, 1975.5, 0.9) * 0.28,
            Cue::Return => {
                // The carriage rattling back, its knock on the left stop,
                // and the platen's ratchet as it turns up a line.
                let n = self.noise();
                let along = (t / RUN).min(1.0);
                let band = self.low_pass(0, n, 2400.0 - 1300.0 * along, dt) - self.low_pass(1, n, 380.0, dt);
                let rattle = 0.55 + 0.45 * (TAU * (90.0 - 40.0 * along) * t).sin();
                let running = if t < RUN { (t / 0.02).min(1.0) * (0.6 + 0.4 * along) } else { (-(t - RUN) / 0.008).exp() };
                let at = t - RUN;
                let knock = if at < 0.0 { 0.0 } else { self.low_pass(2, n, 1100.0, dt) * env(at, 0.0004, 0.012) * 2.4 + (TAU * 95.0 * at).sin() * env(at, 0.001, 0.05) * 0.5 };
                let feed = at - 0.08;
                let ratchet = if feed < 0.0 { 0.0 } else { (n - self.low_pass(3, n, 2500.0, dt)) * env(feed, 0.0003, 0.006) * 0.7 };
                (band * rattle * running * 1.3 + knock + ratchet) * 0.8
            }
            Cue::Seek => {
                // The motor's whirr, the head stepping across the tracks in
                // a buzz with a thunk where it stops, and twice more as it
                // finds its place.
                let n = self.noise();
                let motor = self.low_pass(0, n, 260.0, dt) * 2.5 * (t / 0.12).min(1.0) * ((Cue::Seek.length() - t) / 0.25).clamp(0.0, 1.0);
                let since = seek_step(t);
                let tick = self.low_pass(1, n, 3000.0, dt) - self.low_pass(2, n, 700.0, dt);
                let step = since.map_or(0.0, |at| tick * env(at, 0.0002, 0.003) * 1.6 + (TAU * 820.0 * at).sin() * env(at, 0.0004, 0.006) * 0.3);
                let stop = t - SEEK_BUZZ.0 - SEEK_BUZZ.2 as f32 * SEEK_BUZZ.1;
                let thunk = if stop < 0.0 { 0.0 } else { (TAU * 70.0 * stop).sin() * env(stop, 0.001, 0.04) * 0.6 };
                (motor * 0.3 + step + thunk) * 0.7
            }
            Cue::Punch => {
                // The die's pins through the tape: a hard knock, the pins'
                // rattle, and the tape stepped on.
                let n = self.noise();
                let knock = self.low_pass(0, n, 1600.0, dt) * env(t, 0.0004, 0.01) * 2.2;
                let rattle = (n - self.low_pass(1, n, 3500.0, dt)) * env(t, 0.001, 0.02) * 0.5;
                let step = t - 0.05;
                let feed = if step < 0.0 { 0.0 } else { (TAU * 140.0 * step).sin() * env(step, 0.001, 0.012) * 0.4 };
                (knock + rattle + (TAU * 210.0 * t).sin() * env(t, 0.001, 0.03) * 0.35 + feed) * 0.8
            }
            Cue::Print => {
                // The head's pins firing as it crosses the line, a buzz that
                // comes and goes with the letters, then the platen's step.
                let n = self.noise();
                let along = t / PRINT;
                let printing = if t < PRINT { (t / 0.01).min(1.0) * ((PRINT - t) / 0.02).clamp(0.0, 1.0) } else { 0.0 };
                let pins = (TAU * 190.0 * t).sin().max(0.0).powi(6);
                let letters = 0.55 + 0.45 * (TAU * 9.0 * t + 2.0 * (TAU * 1.3 * t).sin()).sin();
                let buzz = (self.low_pass(0, n, 2600.0, dt) - self.low_pass(1, n, 500.0, dt)) * pins * letters * printing * (1.0 - 0.3 * along) * 3.0;
                let at = t - PRINT;
                let feed = if at < 0.0 { 0.0 } else { self.low_pass(2, n, 900.0, dt) * env(at, 0.0005, 0.015) * 1.6 + (TAU * 120.0 * at).sin() * env(at, 0.001, 0.03) * 0.4 };
                (buzz + feed) * 0.7
            }
            Cue::Rainfall => {
                // A hiss of drops striking too fast to tell apart, brighter
                // and with more of a rush beneath it the harder it rains,
                // and a drop standing out of it now and then. Each is noise
                // of its own (`seeded`) that swells and ebbs at its own
                // pace, and fades in and out at equal power, so one laid on
                // the last carries on without a dip or a seam.
                let [a, b] = self.vary;
                let quarter = std::f32::consts::FRAC_PI_2;
                let fade = ((t / 0.5).min(1.0) * quarter).sin() * (((HUM - t) / 0.5).clamp(0.0, 1.0) * quarter).sin();
                let swell = 1.0 + 0.22 * (TAU * (0.2 + 0.35 * a) * t + TAU * b).sin() + 0.12 * (TAU * (0.7 + 0.6 * b) * t + TAU * a).sin();
                let n = self.noise();
                let high = n - self.low_pass(1, n, 1300.0 + 400.0 * a, dt);
                let hiss = self.low_pass(0, high, (4500.0 + 3500.0 * size) * (0.85 + 0.3 * b), dt);
                let rush = self.low_pass(2, n, 700.0, dt) - self.low_pass(3, n, 150.0, dt);
                let tick = if self.noise() > 0.996 - 0.003 * size { self.noise() } else { 0.0 };
                (hiss * (1.4 + 0.8 * size) * swell + rush * 1.2 * size * (2.0 - swell) + tick * 0.5) * fade * 0.3
            }
            Cue::Fan => {
                // The blades' tone at the motor's speed, which wanders a
                // little as a fan's does, by the mixer's clock: a fan laid
                // on the last is in step with it, so the two tones' straight
                // fades add to its level. And half the air through them.
                let line = (t / FAN_FADE).min(1.0) * ((HUM - t) / FAN_FADE).clamp(0.0, 1.0);
                let at = self.clock + self.t;
                let turns = fan_turns(at);
                let blades = (TAU * turns.fract() as f32).sin() * 0.25 + (TAU * (2.0 * turns).fract() as f32).sin() * 0.08;
                (self.fans_air(at, dt) * equal_power(t, HUM, FAN_FADE) + blades * line) * 0.2
            }
            Cue::FanAir => {
                let at = self.clock + self.t;
                self.fans_air(at, dt) * equal_power(t, FAN_AIR, FAN_FADE) * 0.2
            }
            Cue::Tick => {
                // A pulse through a small speaker: a sharp edge that rings
                // briefly at the cone's own pitch.
                let edge = if t < 0.0006 { 1.0 - t / 0.0006 } else { 0.0 };
                (edge * 0.8 + (TAU * 3100.0 * t).sin() * env(t, 0.0002, 0.003) * 0.6 + (TAU * 900.0 * t).sin() * env(t, 0.0005, 0.006) * 0.3) * 0.8
            }
            Cue::Beep => {
                // A square wave through a small speaker, which takes off its
                // lowest and its sharpest.
                let hz = 760.0 + 140.0 * size;
                self.phase = (self.phase + hz * dt).fract();
                let square = if self.phase < 0.5 { 1.0 } else { -1.0 };
                let band = self.low_pass(0, square, 3500.0, dt) - self.low_pass(1, square, 300.0, dt);
                band * (t / 0.004).min(1.0) * ((Cue::Beep.length() - t) / 0.05).clamp(0.0, 1.0) * 0.35
            }
            Cue::Startup => {
                // A chord of Flow's own, each note a little detuned against
                // itself, swelling in and dying away, with a shimmer above.
                const NOTES: [f32; 5] = [130.81, 196.0, 261.63, 329.63, 392.0];
                let swell = (t / 0.35).min(1.0).powi(2) * (-(t - 0.35).max(0.0) / 1.1).exp();
                let chord: f32 = NOTES.iter().map(|&hz| (TAU * hz * t).sin() + 0.6 * (TAU * hz * 1.003 * t).sin() + 0.2 * (TAU * hz * 2.0 * t).sin()).sum();
                let shimmer = (TAU * 1046.5 * t).sin() * (t / 0.6).min(1.0) * (-t / 0.8).exp() * 0.4;
                (chord * 0.07 + shimmer * 0.3) * swell
            }
            Cue::Step => {
                // The head's motor knocking it out a track against its
                // stop and back again, the second softer, each with the
                // drive's metal ringing and a thump through its case.
                let n = self.noise();
                let hard = self.low_pass(0, n, 2600.0, dt) - self.low_pass(1, n, 400.0, dt);
                let back = t - STEP_BACK;
                let (out, home) = (env(t, 0.0002, 0.004), if back < 0.0 { 0.0 } else { env(back, 0.0002, 0.004) * 0.7 });
                let ring = (TAU * 1250.0 * t).sin() * env(t, 0.0003, 0.008) * 0.3 + if back < 0.0 { 0.0 } else { (TAU * 1250.0 * back).sin() * env(back, 0.0003, 0.008) * 0.2 };
                let thump = (TAU * 95.0 * t).sin() * env(t, 0.001, 0.015) * 0.4;
                (hard * (out + home) * 2.0 + ring + thump) * 0.7
            }
            Cue::Note { hz, secs } => {
                self.phase = (self.phase + hz * dt).fract();
                let x = TAU * self.phase;
                // A soft square, as a terminal's speaker had.
                let tone = x.sin() + (3.0 * x).sin() / 3.0 + (5.0 * x).sin() / 5.0;
                tone * (t / 0.005).min(1.0) * ((secs - t) / 0.015).clamp(0.0, 1.0) * 0.4
            }
        }
    }

    fn sine(&mut self, hz: f32, dt: f32) -> f32 {
        self.phase = (self.phase + hz * dt).fract();
        (TAU * self.phase).sin()
    }

    /// White noise, -1..1.
    fn noise(&mut self) -> f32 {
        let mut x = self.seed;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.seed = x;
        x as f32 / u32::MAX as f32 * 2.0 - 1.0
    }

    /// Half a PC's fan's air at `at` on the mixer's clock: noise of this
    /// voice's own, below the blades' tone and above the motor's rumble,
    /// swelling as the motor speeds up. `Fan` and `FanAir` each carry
    /// half, at a share of the power that makes the two the whole.
    fn fans_air(&mut self, at: f64, dt: f32) -> f32 {
        let n = self.noise();
        let air = self.low_pass(0, n, 700.0, dt) - self.low_pass(1, n, 120.0, dt);
        air * 1.8 * fan_air(at) * std::f32::consts::FRAC_1_SQRT_2
    }

    /// One pole's low-pass at `hz`, its state in filter `i`.
    fn low_pass(&mut self, i: usize, x: f32, hz: f32, dt: f32) -> f32 {
        let k = 1.0 - (-TAU * hz * dt).exp();
        self.filters[i] += k * (x - self.filters[i]);
        self.filters[i]
    }
}

/// A sound's level `t` seconds into its `length`, rising and falling over
/// `fade` seconds at equal power: where one fades out as the next of its
/// noise fades in, the two's power adds to the whole, with no dip.
fn equal_power(t: f32, length: f32, fade: f32) -> f32 {
    let quarter = std::f32::consts::FRAC_PI_2;
    ((t / fade).min(1.0) * quarter).sin() * (((length - t) / fade).clamp(0.0, 1.0) * quarter).sin()
}

/// A slow sine at `hz` with its own `offset`, at `at` seconds.
fn slow(at: f64, hz: f64, offset: f64) -> f64 {
    (std::f64::consts::TAU * hz * at + offset).sin()
}

/// How far round a tube's hum is at `at` seconds on the mixer's clock: 60
/// Hz, wandering by a fifth of a percent over twenty seconds or so. It's
/// the closed form of that pitch's sum, so every hum agrees on it, and in
/// f64, as an hour's turns would lose their fraction in f32.
pub fn hum_turns(at: f64) -> f64 {
    const HZ: f64 = 60.0;
    const WANDER: f64 = 0.002;
    const RATE: f64 = 0.047;
    HZ * at - HZ * WANDER * (std::f64::consts::TAU * RATE * at).cos() / (std::f64::consts::TAU * RATE)
}

/// A tube's hum's level (about 0.85 to 1.15) and its second and third
/// harmonics' shares at `at` seconds, each drifting at a pace of its own.
pub fn hum_drift(at: f64) -> [f32; 3] {
    let level = 1.0 + 0.15 * (0.6 * slow(at, 0.031, 0.0) + 0.4 * slow(at, 0.083, 1.3));
    let second = 0.3 * (1.0 + 0.3 * slow(at, 0.043, 0.7));
    let third = 0.15 * (1.0 + 0.4 * slow(at, 0.027, 2.1));
    [level as f32, second as f32, third as f32]
}

/// How far round a PC's fan's blades are at `at` seconds: 96 a second,
/// the motor speeding up and slowing by up to three percent, in closed
/// form as the hum's is.
pub fn fan_turns(at: f64) -> f64 {
    const HZ: f64 = 96.0;
    let tau = std::f64::consts::TAU;
    let part = |share: f64, rate: f64, offset: f64| -HZ * 0.03 * share * (tau * rate * at + offset).cos() / (tau * rate);
    HZ * at + part(0.7, 0.07, 0.0) + part(0.3, 0.19, 1.1)
}

/// How hard a PC's fan's air rushes at `at` seconds, about 0.85 to 1.15.
fn fan_air(at: f64) -> f32 {
    (1.0 + 0.15 * slow(at, 0.05, 0.4)) as f32
}

/// How long a teletype's carriage takes to run back, in seconds.
const RUN: f32 = 0.2;

/// How long a printer's head takes to cross a line, in seconds.
const PRINT: f32 = 0.32;

/// A disk drive's seek: its head steps from `.0` seconds, every `.1`, `.2`
/// times in a buzz, then once at each of `SEEK_SETTLE`.
const SEEK_BUZZ: (f32, f32, usize) = (0.1, 1.0 / 150.0, 50);
const SEEK_SETTLE: [f32; 2] = [0.78, 0.9];

/// How long after an empty drive's head steps out it steps back.
const STEP_BACK: f32 = 0.012;

/// Seconds since the seek's last step at `t`, or None before the first.
fn seek_step(t: f32) -> Option<f32> {
    let (start, every, steps) = SEEK_BUZZ;
    let end = start + steps as f32 * every;
    if t < start {
        None
    } else if t < end {
        Some((t - start) % every)
    } else {
        Some(t - SEEK_SETTLE.iter().rev().copied().find(|&at| t >= at).unwrap_or(end - every))
    }
}

/// Rising over `attack` seconds, then dying away by e every `decay`.
fn env(t: f32, attack: f32, decay: f32) -> f32 {
    if t < attack {
        t / attack
    } else {
        (-(t - attack) / decay).exp()
    }
}

/// A struck bell of `hz`, `t` seconds after it was struck; `ring` stretches
/// how long it rings.
fn bell(t: f32, hz: f32, ring: f32) -> f32 {
    if t < 0.0 {
        return 0.0;
    }
    const PARTIALS: [(f32, f32, f32); 4] = [(1.0, 1.0, 1.0), (2.0, 0.45, 0.6), (3.01, 0.22, 0.35), (4.17, 0.1, 0.2)];
    let attack = (t / 0.004).min(1.0);
    attack * PARTIALS.iter().map(|&(r, a, d)| a * (TAU * hz * r * t).sin() * (-t / (d * ring)).exp()).sum::<f32>()
}

/// Voices playing at once, at most; a new one past this ends the oldest.
const MAX_VOICES: usize = 24;
/// Frames mixed at a time.
const BLOCK: usize = 1024;

/// The voices playing, mixed into the output.
pub struct Mixer {
    voices: Vec<Voice>,
    mix: Vec<[f32; 2]>,
    pub volume: f32,
    /// Seconds of sound it has made: the clock voices that drift go by.
    time: f64,
}

impl Mixer {
    pub fn new(volume: f32) -> Self {
        Self { voices: Vec::with_capacity(MAX_VOICES), mix: vec![[0.0; 2]; BLOCK], volume, time: 0.0 }
    }

    /// Takes `voice`, starting where its wait ends on the mixer's clock,
    /// however late it came.
    pub fn add(&mut self, voice: Voice) {
        if self.voices.len() >= MAX_VOICES {
            self.voices.remove(0);
        }
        let start = self.time + voice.wait;
        self.voices.push(voice.at(start));
    }

    pub fn busy(&self) -> bool {
        !self.voices.is_empty()
    }

    /// Fills `out`, `channels` samples a frame, at `rate` frames a second:
    /// left and right in the first two, their mean in a single one.
    pub fn fill(&mut self, out: &mut [f32], channels: usize, rate: f32) {
        let channels = channels.max(1);
        let dt = 1.0 / rate.max(1.0);
        for frames in out.chunks_mut(BLOCK * channels) {
            let n = frames.len() / channels;
            self.time += n as f64 * f64::from(dt);
            let mix = &mut self.mix[..n];
            mix.fill([0.0; 2]);
            self.voices.retain_mut(|v| {
                for m in mix.iter_mut() {
                    let Some([l, r]) = v.next(dt) else { return false };
                    m[0] += l;
                    m[1] += r;
                }
                true
            });
            for (frame, [l, r]) in frames.chunks_mut(channels).zip(mix.iter()) {
                // A soft limit, so voices piling up never clip.
                let (l, r) = ((l * self.volume).tanh(), (r * self.volume).tanh());
                match frame {
                    [only] => *only = (l + r) * 0.5,
                    [a, b, rest @ ..] => {
                        (*a, *b) = (l, r);
                        rest.fill(0.0);
                    }
                    [] => {}
                }
            }
        }
    }
}

/// How long the output stays open with nothing playing.
const IDLE: Duration = Duration::from_secs(4);
/// How long after the output wouldn't open it's tried again, rather than at
/// every sound.
const RETRY: Duration = Duration::from_secs(10);
/// The same kind of sound again sooner than this is let go.
const AGAIN: Duration = Duration::from_millis(40);
/// How long a hum lasts; one is laid on every [`HUM_EVERY`], overlapping the
/// last's fade.
pub const HUM: f32 = 3.0;
pub const HUM_EVERY: Duration = Duration::from_millis(2500);
/// How long a PC's fan's other air lasts ([`Cue::FanAir`]), laid on every
/// [`FAN_AIR_EVERY`]: as much longer than its spacing as the fan's own,
/// so each fades in as the last fades out, but at a spacing the fan's
/// doesn't divide, so their crossings come together only every 92.5 s.
pub const FAN_AIR: f32 = 4.2;
pub const FAN_AIR_EVERY: Duration = Duration::from_millis(3700);
/// How long a PC's fan's layers take to fade in and out: the overlap
/// where one is laid on the last.
const FAN_FADE: f32 = 0.5;
/// A PC's fan's layers, each laid on its own spacing.
pub const FAN_LAYERS: [(Cue, Duration); 2] = [(Cue::Fan, HUM_EVERY), (Cue::FanAir, FAN_AIR_EVERY)];
/// How far ahead notes may be queued.
const NOTES_AHEAD: Duration = Duration::from_secs(10);

enum Msg {
    Play(Voice),
    Volume(f32),
}

/// Flow's sounds: what it plays, and the thread that plays them.
pub struct Sound {
    /// Off for snapshots and recordings, which make none.
    quiet: bool,
    tx: Option<Sender<Msg>>,
    volume: f32,
    /// When each kind last played.
    last: [Option<Instant>; KINDS],
    /// When each kind's run of [`Sound::play_run`] ends.
    runs: [Option<Instant>; KINDS],
    /// When the notes queued so far end.
    notes_until: Option<Instant>,
    seed: u32,
}

impl Sound {
    pub fn new(quiet: bool, volume: f32) -> Self {
        Self { quiet, tx: None, volume, last: [None; KINDS], runs: [None; KINDS], notes_until: None, seed: 0x2545_f491 }
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
        if let Some(tx) = &self.tx {
            let _ = tx.send(Msg::Volume(self.volume));
        }
    }

    /// Plays `cue` at `pan`, unless the same kind just played. A splash
    /// brings a few drops with it.
    pub fn play(&mut self, cue: Cue, pan: f32, gain: f32, size: f32) {
        let now = Instant::now();
        let kind = cue.kind();
        if self.last[kind].is_some_and(|at| now.duration_since(at) < AGAIN) {
            return;
        }
        self.last[kind] = Some(now);
        let seed = self.random().to_bits();
        self.send(Voice::new(cue, pan, gain, size, 0.0).seeded(seed));
        if cue == Cue::Splash {
            for i in 0..3 {
                let (a, b) = (self.random(), self.random());
                self.send(Voice::new(Cue::Drop, pan + (a - 0.5) * 0.4, gain * 0.35, 0.2 + 0.5 * b, 0.04 + 0.08 * i as f32 + 0.05 * a));
            }
        }
    }

    /// `count` of `cue`, `every` seconds apart, after any run of it still
    /// going: a machine at work, such as a punch while lines come. Runs
    /// queue at most a second ahead; what would go further is let go.
    pub fn play_run(&mut self, cue: Cue, gain: f32, count: usize, every: f32) {
        let now = Instant::now();
        let kind = cue.kind();
        let start = self.runs[kind].filter(|&end| end > now).unwrap_or(now);
        let mut delay = start.duration_since(now).as_secs_f32();
        for _ in 0..count {
            if delay > 1.0 {
                break;
            }
            self.send(Voice::new(cue, 0.0, gain, 0.0, delay));
            delay += every;
        }
        self.runs[kind] = Some(now + Duration::from_secs_f32(delay));
    }

    /// DECPS's notes, each `secs` long at `gain`, after any still queued.
    pub fn play_notes(&mut self, notes: &[u8], secs: f32, gain: f32) {
        let now = Instant::now();
        let start = self.notes_until.filter(|&end| end > now).unwrap_or(now);
        if start.duration_since(now) > NOTES_AHEAD {
            return;
        }
        let mut delay = start.duration_since(now).as_secs_f32();
        for &n in notes {
            if let (Some(hz), true) = (note_hz(n), gain > 0.0) {
                self.send(Voice::new(Cue::Note { hz, secs }, 0.0, gain, 0.0, delay));
            }
            delay += secs;
        }
        self.notes_until = Some(now + Duration::from_secs_f32(delay));
    }

    fn send(&mut self, voice: Voice) {
        if self.quiet {
            return;
        }
        let volume = self.volume;
        let tx = self.tx.get_or_insert_with(|| {
            let (tx, rx) = crossbeam_channel::unbounded();
            let spawned = std::thread::Builder::new().name("flow-sound".into()).spawn(move || run(rx, volume));
            if let Err(e) = spawned {
                log::warn!("no sound: {e}");
            }
            tx
        });
        let _ = tx.send(Msg::Play(voice));
    }

    fn random(&mut self) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        self.seed as f32 / u32::MAX as f32
    }
}

/// The sound thread: opens the output for the first voice, passes voices
/// and the volume to it, and closes it once it has been quiet a while.
fn run(rx: Receiver<Msg>, mut volume: f32) {
    let mut output: Option<output::Output> = None;
    let mut last_voice = Instant::now();
    let mut failed_at: Option<Instant> = None;
    loop {
        match rx.recv_timeout(Duration::from_millis(500)) {
            Ok(Msg::Play(voice)) => {
                last_voice = Instant::now();
                // Opened for the first voice, and again where its device
                // went away (the sound server restarted, say).
                if output.as_ref().is_none_or(|o| o.stopped()) && failed_at.is_none_or(|at| at.elapsed() >= RETRY) {
                    output = output::Output::open(volume);
                    failed_at = output.is_none().then(Instant::now);
                }
                if let Some(out) = &output {
                    out.play(voice);
                }
            }
            Ok(Msg::Volume(v)) => {
                volume = v;
                if let Some(out) = &output {
                    out.set_volume(v);
                }
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                if output.as_ref().is_some_and(|o| !o.busy()) && last_voice.elapsed() > IDLE {
                    output = None;
                }
            }
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => return,
        }
    }
}

#[cfg(any(target_os = "macos", windows))]
mod output {
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::sync::Arc;

    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use crossbeam_channel::Sender;

    use super::{Mixer, Voice};

    /// The system's output, open, and what its callback reads.
    pub struct Output {
        _stream: cpal::Stream,
        voices: Sender<Voice>,
        volume: Arc<AtomicU32>,
        busy: Arc<AtomicBool>,
    }

    impl Output {
        pub fn open(volume: f32) -> Option<Self> {
            let device = cpal::default_host().default_output_device()?;
            let supported = device.default_output_config().map_err(|e| log::warn!("no sound output: {e}")).ok()?;
            let (tx, rx) = crossbeam_channel::bounded::<Voice>(64);
            let level = Arc::new(AtomicU32::new(volume.to_bits()));
            let busy = Arc::new(AtomicBool::new(false));
            let (channels, rate) = (supported.channels() as usize, supported.sample_rate() as f32);
            let stream = match supported.sample_format() {
                cpal::SampleFormat::F32 => build::<f32>(&device, supported.config(), (channels, rate), rx, level.clone(), busy.clone()),
                cpal::SampleFormat::I16 => build::<i16>(&device, supported.config(), (channels, rate), rx, level.clone(), busy.clone()),
                cpal::SampleFormat::U16 => build::<u16>(&device, supported.config(), (channels, rate), rx, level.clone(), busy.clone()),
                cpal::SampleFormat::I32 => build::<i32>(&device, supported.config(), (channels, rate), rx, level.clone(), busy.clone()),
                other => {
                    log::warn!("no sound output: samples as {other:?}");
                    return None;
                }
            }?;
            stream.play().map_err(|e| log::warn!("no sound output: {e}")).ok()?;
            Some(Self { _stream: stream, voices: tx, volume: level, busy })
        }

        pub fn play(&self, voice: Voice) {
            self.busy.store(true, Ordering::Relaxed);
            let _ = self.voices.try_send(voice);
        }

        pub fn set_volume(&self, volume: f32) {
            self.volume.store(volume.to_bits(), Ordering::Relaxed);
        }

        pub fn busy(&self) -> bool {
            self.busy.load(Ordering::Relaxed)
        }

        /// cpal's stream goes on as long as it's kept.
        pub fn stopped(&self) -> bool {
            false
        }
    }

    fn build<T: cpal::SizedSample + cpal::FromSample<f32>>(
        device: &cpal::Device,
        config: cpal::StreamConfig,
        (channels, rate): (usize, f32),
        voices: crossbeam_channel::Receiver<Voice>,
        volume: Arc<AtomicU32>,
        busy: Arc<AtomicBool>,
    ) -> Option<cpal::Stream> {
        let mut mixer = Mixer::new(f32::from_bits(volume.load(Ordering::Relaxed)));
        let mut floats: Vec<f32> = Vec::new();
        let stream = device.build_output_stream(
            config,
            move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                for v in voices.try_iter() {
                    mixer.add(v);
                }
                mixer.volume = f32::from_bits(volume.load(Ordering::Relaxed));
                // Grown once, to the size the system asks for.
                floats.resize(data.len(), 0.0);
                mixer.fill(&mut floats, channels, rate);
                for (d, s) in data.iter_mut().zip(&floats) {
                    *d = T::from_sample(*s);
                }
                busy.store(mixer.busy(), Ordering::Relaxed);
            },
            |e| log::warn!("sound output: {e}"),
            None,
        );
        stream.map_err(|e| log::warn!("no sound output: {e}")).ok()
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod alsa;

/// ALSA's default device (Linux), which carries the sound on to PipeWire or
/// PulseAudio where they run: a thread of the output's own mixes a block at
/// a time and writes it, waiting while the device's buffer is full, as
/// cpal's callback is called for more elsewhere.
#[cfg(all(unix, not(target_os = "macos")))]
mod output {
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::sync::Arc;
    use std::thread::JoinHandle;

    use crossbeam_channel::Sender;

    use super::alsa::Pcm;
    use super::{Mixer, Voice};

    /// What's asked of the device, which converts to its own as it must.
    const RATE: u32 = 48_000;
    const CHANNELS: usize = 2;
    /// Frames mixed at a time: 10 ms.
    const BLOCK: usize = RATE as usize / 100;

    /// The device, open, and what its thread reads.
    pub struct Output {
        voices: Sender<Voice>,
        volume: Arc<AtomicU32>,
        busy: Arc<AtomicBool>,
        open: Arc<AtomicBool>,
        thread: Option<JoinHandle<()>>,
    }

    impl Output {
        pub fn open(volume: f32) -> Option<Self> {
            let mut pcm = Pcm::open(CHANNELS, RATE).map_err(|e| log::warn!("no sound output: {e}")).ok()?;
            let (tx, rx) = crossbeam_channel::bounded::<Voice>(64);
            let level = Arc::new(AtomicU32::new(volume.to_bits()));
            let busy = Arc::new(AtomicBool::new(false));
            let open = Arc::new(AtomicBool::new(true));
            let (l, b, o) = (level.clone(), busy.clone(), open.clone());
            let thread = std::thread::Builder::new()
                .name("flow-sound-out".into())
                .spawn(move || {
                    let mut mixer = Mixer::new(f32::from_bits(l.load(Ordering::Relaxed)));
                    let mut floats = vec![0.0; BLOCK * CHANNELS];
                    while o.load(Ordering::Relaxed) {
                        for v in rx.try_iter() {
                            mixer.add(v);
                        }
                        mixer.volume = f32::from_bits(l.load(Ordering::Relaxed));
                        mixer.fill(&mut floats, CHANNELS, RATE as f32);
                        if !pcm.write(&floats) {
                            log::warn!("sound output stopped");
                            break;
                        }
                        b.store(mixer.busy(), Ordering::Relaxed);
                    }
                    b.store(false, Ordering::Relaxed);
                })
                .map_err(|e| log::warn!("no sound output: {e}"))
                .ok()?;
            Some(Self { voices: tx, volume: level, busy, open, thread: Some(thread) })
        }

        pub fn play(&self, voice: Voice) {
            self.busy.store(true, Ordering::Relaxed);
            let _ = self.voices.try_send(voice);
        }

        pub fn set_volume(&self, volume: f32) {
            self.volume.store(volume.to_bits(), Ordering::Relaxed);
        }

        pub fn busy(&self) -> bool {
            self.busy.load(Ordering::Relaxed)
        }

        /// Its thread ended, the device having gone away.
        pub fn stopped(&self) -> bool {
            self.thread.as_ref().is_none_or(|t| t.is_finished())
        }
    }

    impl Drop for Output {
        /// Its thread finishes the block it's writing and closes the device.
        fn drop(&mut self) {
            self.open.store(false, Ordering::Relaxed);
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }
}

#[cfg(test)]
mod output_tests {
    use super::*;

    /// Needs an output device, so it's left out of the usual run.
    #[test]
    #[ignore]
    fn the_systems_output_opens_and_plays_a_voice_through_at_no_volume() {
        let out = output::Output::open(0.0).expect("this machine's default output opens");
        out.play(Voice::new(Cue::Click, 0.0, 1.0, 0.0, 0.0));
        assert!(out.busy());
        std::thread::sleep(Duration::from_millis(400));
        assert!(!out.busy(), "the output's callback played the click to its end");
    }
}

/// Where there's no output (no OS Flow is built for), nothing plays.
#[cfg(not(any(target_os = "macos", windows, unix)))]
mod output {
    use super::Voice;

    pub struct Output;

    impl Output {
        pub fn open(_volume: f32) -> Option<Self> {
            None
        }

        pub fn play(&self, _voice: Voice) {}

        pub fn set_volume(&self, _volume: f32) {}

        pub fn busy(&self) -> bool {
            false
        }

        pub fn stopped(&self) -> bool {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 48_000.0;

    /// A cue played alone, left and right.
    fn render(cue: Cue, pan: f32) -> Vec<[f32; 2]> {
        let mut v = Voice::new(cue, pan, 1.0, 0.5, 0.0);
        std::iter::from_fn(|| v.next(1.0 / RATE)).collect()
    }

    fn peak(samples: &[[f32; 2]]) -> f32 {
        samples.iter().flatten().fold(0.0f32, |a, s| a.max(s.abs()))
    }

    #[test]
    fn every_cue_is_heard_without_clipping_and_ends_quiet() {
        let mut cues: Vec<Cue> = NAMES.iter().map(|(_, c)| *c).collect();
        cues.extend([Cue::Margin, Cue::Return, Cue::Seek, Cue::Punch, Cue::Print, Cue::Tick, Cue::Beep, Cue::Startup, Cue::Step, Cue::Note { hz: 880.0, secs: 0.25 }]);
        let mut wrong = Vec::new();
        for cue in cues {
            let s = render(cue, 0.0);
            let (p, tail) = (peak(&s), peak(&s[s.len() - (0.01 * RATE) as usize..]));
            if s.len().abs_diff((cue.length() * RATE) as usize) > 2 {
                wrong.push(format!("{cue:?} lasts {} samples", s.len()));
            }
            if !s.iter().flatten().all(|x| x.is_finite()) || !(0.2..=1.0).contains(&p) {
                wrong.push(format!("{cue:?} peaks at {p}"));
            }
            if tail >= p * 0.25 {
                wrong.push(format!("{cue:?} doesn't die away: its last 10 ms at {tail} of {p}"));
            }
        }
        assert!(wrong.is_empty(), "{wrong:#?}");
    }

    /// A minute of a PC's fan, each of its layers laid on its own spacing
    /// as `App::hear_machines` lays them, each voice with noise of its own
    /// as `Sound::play` gives it, mixed: the left channel.
    fn a_fans_minute(layers: &[(Cue, Duration)]) -> Vec<f32> {
        let mut mixer = Mixer::new(1.0);
        for (layer, &(cue, every)) in layers.iter().enumerate() {
            let every = every.as_secs_f32();
            for i in 0..=(60.0 / every) as u32 {
                mixer.add(Voice::new(cue, 0.0, 0.5, 0.0, i as f32 * every).seeded((layer as u32 * 1000 + i).wrapping_mul(2_654_435_761)));
            }
        }
        let mut out = vec![0.0; (60.0 * RATE) as usize * 2];
        mixer.fill(&mut out, 2, RATE);
        out.chunks(2).map(|frame| frame[0]).collect()
    }

    /// How much quieter `s` is where a layer laid every `every` crosses
    /// from one voice to the next than around it, in decibels, from 5 s to
    /// 55 s in: its power over the middle 0.3 s of each crossing (the half
    /// second after each is laid on, as the last fades out) against its
    /// power over 0.4 s either side of that, half a second off, which
    /// shares the fan's slow swell. Over some twenty crossings the noise's
    /// own ups and downs even out to a tenth of a decibel or so.
    fn dip(s: &[f32], every: Duration) -> f64 {
        let every = every.as_secs_f64();
        let power = |a: f64, b: f64| s[(a * f64::from(RATE)) as usize..(b * f64::from(RATE)) as usize].iter().map(|x| f64::from(x * x)).sum::<f64>();
        let (mut at, mut crossing, mut around) = ((5.0 / every).ceil() * every, 0.0, 0.0);
        while at + 1.0 <= 55.0 {
            crossing += power(at + 0.1, at + 0.4) / 0.3;
            around += (power(at - 0.5, at - 0.1) + power(at + 0.6, at + 1.0)) / 0.8;
            at += every;
        }
        10.0 * (around / crossing).log10()
    }

    #[test]
    fn a_pcs_fan_holds_its_level_with_no_beat_where_one_is_laid_on_the_last() {
        let s = a_fans_minute(&FAN_LAYERS);
        for (_, every) in FAN_LAYERS {
            // The fan's straight fades dipped 1.2 dB or so here, half its
            // air faded so 0.3; at equal power, a tenth or less.
            let dip = dip(&s, every);
            assert!(dip.abs() < 0.25, "a beat every {every:?}: {dip:.2} dB quieter where one voice crosses to the next");
        }
    }

    #[test]
    fn a_tubes_hum_fades_in_and_out_and_one_laid_on_the_last_holds_steady() {
        let s = render(Cue::Hum, 0.0);
        let level = |s: &[[f32; 2]], from: f32| peak(&s[(from * RATE) as usize..((from + 0.1) * RATE) as usize]);
        let (start, middle, end) = (level(&s, 0.0), level(&s, 1.5), level(&s, HUM - 0.1));
        assert!((0.05..0.3).contains(&middle), "quiet, but there: {middle}");
        assert!(start < middle * 0.4 && end < middle * 0.4, "it fades in and out: {start} {middle} {end}");
        // Laid every HUM_EVERY, each fades in as the last fades out, in
        // step (both go by the mixer's clock), so the hum goes on.
        let every = HUM_EVERY.as_secs_f32();
        let mut a = Voice::new(Cue::Hum, 0.0, 1.0, 0.0, 0.0);
        let mut b = Voice::new(Cue::Hum, 0.0, 1.0, 0.0, every).at(f64::from(every));
        let both: Vec<[f32; 2]> = (0..((every + HUM) * RATE) as usize)
            .map(|_| {
                let (x, y) = (a.next(1.0 / RATE).unwrap_or([0.0; 2]), b.next(1.0 / RATE).unwrap_or([0.0; 2]));
                [x[0] + y[0], x[1] + y[1]]
            })
            .collect();
        for at in [every - 0.2, every + 0.1, every + 0.25, every + 0.45, every + 0.8] {
            let l = level(&both, at);
            assert!((l - middle).abs() < middle * 0.15, "steady across the overlap at {at}s: {l} against {middle}");
        }
    }

    #[test]
    fn a_hum_drifts_so_no_two_stretches_of_it_sound_the_same() {
        let hum_at = |start: f64| {
            let mut v = Voice::new(Cue::Hum, 0.0, 1.0, 0.0, 0.0).at(start);
            std::iter::from_fn(|| v.next(1.0 / RATE)).map(|[l, _]| l).collect::<Vec<f32>>()
        };
        let (now, later) = (hum_at(0.0), hum_at(10.0));
        let differ = now.iter().zip(&later).map(|(a, b)| (a - b).abs()).fold(0.0, f32::max);
        assert!(differ > 0.01, "ten seconds on, a hum isn't the one before: {differ}");
        assert_eq!(now, hum_at(0.0), "the same moment, the same hum");
        // Over an hour its level and harmonics keep within their bounds,
        // and its pitch within a fifth of a percent of the mains'.
        for i in 0..3600 {
            let at = f64::from(i);
            let [level, second, third] = hum_drift(at);
            assert!((0.84..=1.16).contains(&level) && (0.2..=0.4).contains(&second) && (0.08..=0.22).contains(&third), "at {at}s: {level} {second} {third}");
            let hz = (hum_turns(at + 0.001) - hum_turns(at)) / 0.001;
            assert!((hz - 60.0).abs() <= 60.0 * 0.0021, "at {at}s the hum is {hz} Hz");
        }
    }

    #[test]
    fn a_fans_blades_speed_up_and_slow_near_96_turns_a_second() {
        let speeds: Vec<f64> = (0..3600).map(|i| f64::from(i) * 0.5).map(|at| (fan_turns(at + 0.001) - fan_turns(at)) / 0.001).collect();
        assert!(speeds.iter().all(|hz| (hz - 96.0).abs() <= 96.0 * 0.031), "within three percent");
        let (low, high) = speeds.iter().fold((f64::MAX, f64::MIN), |(lo, hi), &hz| (lo.min(hz), hi.max(hz)));
        assert!(high - low > 96.0 * 0.02, "and wandering: {low}..{high}");
        assert!((0..3600).map(|i| fan_air(f64::from(i))).all(|a| (0.84..=1.16).contains(&a)));
    }

    #[test]
    fn a_voice_starts_on_the_mixers_clock_however_late_it_comes() {
        let mut mixer = Mixer::new(1.0);
        let mut out = vec![0.0; 4800 * 2];
        mixer.fill(&mut out, 2, RATE);
        mixer.add(Voice::new(Cue::Hum, 0.0, 1.0, 0.0, 0.25));
        let start = mixer.voices[0].clock;
        assert!((start - (4800.0 / f64::from(RATE) + 0.25)).abs() < 1e-6, "where its wait ends: {start}");
    }

    #[test]
    fn rain_too_fast_to_tell_apart_hisses_louder_the_harder_it_rains() {
        let rms = |size: f32, from: f32| {
            let mut v = Voice::new(Cue::Rainfall, 0.0, 1.0, size, 0.0);
            let s: Vec<[f32; 2]> = std::iter::from_fn(|| v.next(1.0 / RATE)).collect();
            assert!(s.iter().flatten().all(|x| x.is_finite() && x.abs() <= 1.0));
            let part = &s[(from * RATE) as usize..((from + 0.2) * RATE) as usize];
            (part.iter().map(|[l, _]| l * l).sum::<f32>() / part.len() as f32).sqrt()
        };
        let (rain, downpour) = (rms(0.2, 1.5), rms(1.0, 1.5));
        assert!((0.01..0.25).contains(&rain) && downpour > rain * 1.3, "rain {rain}, a downpour {downpour}");
        assert!(rms(1.0, 0.0) < downpour * 0.5 && rms(1.0, HUM - 0.2) < downpour * 0.5, "it fades in and out");
    }

    #[test]
    fn no_two_seeds_of_the_rain_sound_the_same() {
        let render_seeded = |cue: Cue, seed: u32| {
            let mut v = Voice::new(cue, 0.0, 1.0, 0.6, 0.0).seeded(seed);
            std::iter::from_fn(|| v.next(1.0 / RATE)).collect::<Vec<[f32; 2]>>()
        };
        for cue in [Cue::Rainfall, Cue::Patter] {
            let (a, b) = (render_seeded(cue, 7), render_seeded(cue, 8));
            assert_ne!(a, b, "{cue:?}");
            assert_eq!(a, render_seeded(cue, 7), "{cue:?}: the same seed, the same sound");
            assert!(a.iter().chain(&b).flatten().all(|x| x.is_finite() && x.abs() <= 1.0));
        }
        // Laid every HUM_EVERY, one hiss on the last holds its level
        // through the overlap, at equal power: no dip where they cross.
        let every = HUM_EVERY.as_secs_f32();
        let mut first = Voice::new(Cue::Rainfall, 0.0, 1.0, 0.6, 0.0).seeded(11);
        let mut second = Voice::new(Cue::Rainfall, 0.0, 1.0, 0.6, every).seeded(12);
        let both: Vec<f32> = (0..((every + HUM) * RATE) as usize)
            .map(|_| first.next(1.0 / RATE).unwrap_or([0.0; 2])[0] + second.next(1.0 / RATE).unwrap_or([0.0; 2])[0])
            .collect();
        let rms = |from: f32| {
            let part = &both[(from * RATE) as usize..((from + 0.25) * RATE) as usize];
            (part.iter().map(|x| x * x).sum::<f32>() / part.len() as f32).sqrt()
        };
        let (before, across) = (rms(every - 0.6), rms(every + 0.125));
        assert!(across > before * 0.6, "a dip where they cross: {across} against {before}");
    }

    #[test]
    fn a_pcs_fan_whirs_quietly_and_fades_in_and_out_as_the_hum_does() {
        let s = render(Cue::Fan, 0.0);
        let rms = |from: f32| {
            let part = &s[(from * RATE) as usize..((from + 0.2) * RATE) as usize];
            (part.iter().map(|[l, _]| l * l).sum::<f32>() / part.len() as f32).sqrt()
        };
        let (start, middle, end) = (rms(0.0), rms(1.5), rms(HUM - 0.2));
        assert!((0.01..0.15).contains(&middle), "there, under everything else: {middle}");
        assert!(start < middle * 0.6 && end < middle * 0.6, "it fades in and out: {start} {middle} {end}");
    }

    #[test]
    fn a_run_of_a_cue_queues_after_the_last_and_no_more_than_a_second_ahead() {
        let mut sound = Sound::new(true, 0.5);
        sound.play_run(Cue::Punch, 1.0, 4, 0.1);
        let first = sound.runs[Cue::Punch.kind()].expect("a run").duration_since(Instant::now()).as_secs_f32();
        assert!((0.35..0.41).contains(&first), "four, a tenth apart: {first}");
        sound.play_run(Cue::Punch, 1.0, 40, 0.1);
        let both = sound.runs[Cue::Punch.kind()].unwrap().duration_since(Instant::now()).as_secs_f32();
        assert!((1.0..1.15).contains(&both), "the second after the first, up to a second ahead: {both}");
    }

    #[test]
    fn a_disk_drive_buzzes_across_its_tracks_then_steps_twice_more() {
        let (start, every, steps) = SEEK_BUZZ;
        assert_eq!(seek_step(start * 0.5), None, "the motor first");
        assert!(seek_step(start + every * 2.5).is_some_and(|at| (at - every * 0.5).abs() < 1e-4));
        let end = start + steps as f32 * every;
        assert!(seek_step(end + 0.05).is_some_and(|at| at > 0.05), "then a pause");
        assert!(seek_step(SEEK_SETTLE[1] + 0.01).is_some_and(|at| (at - 0.01).abs() < 1e-4));
        let s = render(Cue::Seek, 0.0);
        let level = |from: f32, to: f32| peak(&s[(from * RATE) as usize..(to * RATE) as usize]);
        let (buzz, pause, settle) = (level(start, end - every), level(end + 0.15, SEEK_SETTLE[0] - 0.01), level(SEEK_SETTLE[0], SEEK_SETTLE[0] + 0.01));
        assert!(buzz > pause * 2.0 && settle > pause * 2.0, "steps over the motor's whirr: {buzz} {pause} {settle}");
    }

    #[test]
    fn an_empty_drive_steps_out_and_back_in_two_knocks() {
        let s = render(Cue::Step, 0.0);
        let level = |from: f32, to: f32| peak(&s[(from * RATE) as usize..(to * RATE) as usize]);
        let (out, between, home, after) = (level(0.0, 0.004), level(0.009, 0.0115), level(STEP_BACK, STEP_BACK + 0.004), level(0.06, 0.09));
        assert!(out > between * 1.5 && home > between, "two knocks: {out} {between} {home}");
        assert!(after < out * 0.2, "then quiet: {after}");
    }

    #[test]
    fn a_teletypes_carriage_knocks_when_it_has_run_back() {
        let s = render(Cue::Return, 0.0);
        let level = |from: f32, to: f32| peak(&s[(from * RATE) as usize..(to * RATE) as usize]);
        let (running, knock, after) = (level(0.05, RUN - 0.01), level(RUN, RUN + 0.02), level(RUN + 0.2, RUN + 0.3));
        assert!(knock > running && running > after * 3.0, "{running} {knock} {after}");
    }

    #[test]
    fn thunder_rolls_on_after_its_crack() {
        let s = render(Cue::Thunder, 0.0);
        let rms = |from: f32, to: f32| {
            let part = &s[(from * RATE) as usize..(to * RATE) as usize];
            (part.iter().map(|[l, _]| l * l).sum::<f32>() / part.len() as f32).sqrt()
        };
        assert!(rms(0.3, 1.2) > 0.05, "a rumble, not just a crack: {}", rms(0.3, 1.2));
        assert!(rms(2.4, 2.8) < rms(0.3, 1.2) * 0.3, "that dies away");
    }

    #[test]
    fn a_sound_on_the_left_is_louder_on_the_left() {
        let s = render(Cue::Drop, -0.8);
        let (l, r) = s.iter().fold((0.0, 0.0), |(l, r), [a, b]| (l + a * a, r + b * b));
        assert!(l > r * 4.0);
        let c = render(Cue::Drop, 0.0);
        let (l, r) = c.iter().fold((0.0, 0.0), |(l, r), [a, b]| (l + a * a, r + b * b));
        assert!((l - r).abs() < l * 1e-4, "centered, it's even");
    }

    #[test]
    fn the_mixer_fills_each_channel_and_lets_finished_voices_go() {
        let mut mixer = Mixer::new(1.0);
        mixer.add(Voice::new(Cue::Click, -1.0, 1.0, 0.0, 0.0));
        mixer.add(Voice::new(Cue::Click, 1.0, 1.0, 0.0, 0.01));
        let mut out = vec![0.0f32; 4800 * 2];
        mixer.fill(&mut out, 2, RATE);
        assert!(out.iter().step_by(2).any(|s| s.abs() > 0.05), "the left one is heard on the left");
        assert!(out.iter().all(|s| s.abs() <= 1.0));
        assert!(!mixer.busy(), "both clicks are over within the 100 ms");
        let mut mono = vec![0.0f32; 480];
        mixer.add(Voice::new(Cue::Click, 0.0, 1.0, 0.0, 0.0));
        mixer.fill(&mut mono, 1, RATE);
        assert!(mono.iter().any(|s| s.abs() > 0.05));
        for _ in 0..MAX_VOICES + 5 {
            mixer.add(Voice::new(Cue::Chime, 0.0, 1.0, 0.0, 0.0));
        }
        assert_eq!(mixer.voices.len(), MAX_VOICES);
    }

    #[test]
    fn decps_notes_run_from_c5_to_c7_and_names_find_their_cues() {
        assert_eq!(note_hz(0), None);
        assert!((note_hz(1).unwrap() - 523.25).abs() < 0.01);
        assert!((note_hz(13).unwrap() - 1046.5).abs() < 0.01);
        assert!((note_hz(25).unwrap() - 2093.0).abs() < 0.01);
        assert_eq!(note_hz(26), None);
        assert_eq!(Cue::named("Boing"), Some(Cue::Boing));
        assert_eq!(Cue::named("kazoo"), None);
    }

    #[test]
    fn quiet_sound_starts_no_thread_and_the_same_kind_waits_its_turn() {
        let mut sound = Sound::new(true, 0.5);
        sound.play(Cue::Drop, 0.0, 1.0, 0.0);
        assert!(sound.tx.is_none(), "snapshots make no sound");
        let first = sound.last[Cue::Drop.kind()];
        sound.play(Cue::Drop, 0.0, 1.0, 0.0);
        assert_eq!(sound.last[Cue::Drop.kind()], first, "a second drop at once is let go");
        sound.play_notes(&[1, 0, 5], 0.25, 0.5);
        let end = sound.notes_until.expect("queued");
        assert!(end.duration_since(Instant::now()) > Duration::from_millis(700));
    }
}
