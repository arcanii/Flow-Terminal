//! What the terminal is up to, shown on the glass over any style (None
//! included): a ring spreading from where a command finished or the
//! assistant got something done, and a tank that fills from the bottom while
//! a program reports its progress. They run on real time, not the liquid's
//! clock, so they keep their pace whatever the flow speed, and with the
//! liquid frozen.

use crate::osc::ProgressState;
use crate::render::rgba;
use crate::ui::{ACCENT, ACCENT2};

/// Seconds a ring takes to fade away.
const RING_LIFE: f32 = 3.0;

/// Kinds of ring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ring {
    /// A command finished well.
    Done,
    /// A command that ran a good while finished well.
    Long,
    /// A command failed.
    Failed,
    /// The assistant finished a task of several steps.
    Bloom,
}

impl Ring {
    /// Color, strength and width (a fraction of the window's height).
    fn look(self) -> ([f32; 3], f32, f32) {
        match self {
            Ring::Done => (rgb(ACCENT), 0.6, 0.01),
            Ring::Long => (rgb(ACCENT2), 1.3, 0.02),
            Ring::Failed => (EMBER, 1.0, 0.014),
            Ring::Bloom => (rgb(ACCENT2), 1.5, 0.026),
        }
    }
}

const EMBER: [f32; 3] = [1.0, 0.42, 0.26];
const AMBER: [f32; 3] = [1.0, 0.78, 0.3];

fn rgb(c: [u8; 3]) -> [f32; 3] {
    let c = rgba(c, 1.0);
    [c[0], c[1], c[2]]
}

#[derive(Debug, Clone, Copy)]
struct Pulse {
    at: [f32; 2],
    ring: Ring,
    age: f32,
}

#[derive(Debug, Default)]
pub struct Signals {
    pulse: Option<Pulse>,
    /// The tank as shown: how full, how visible, 1 with a figure or 0
    /// sloshing, its color and its clock.
    level: f32,
    shown: f32,
    figure: f32,
    color: [f32; 3],
    clock: f32,
}

impl Signals {
    /// Sends a ring out from `at` (0..1 of the window), replacing any other.
    pub fn pulse(&mut self, at: [f32; 2], ring: Ring) {
        self.pulse = Some(Pulse { at, ring, age: 0.0 });
    }

    /// Moves on by `dt` real seconds. `progress` is what the running program
    /// reports: how far along (0..1) and how it's going; `None` empties the
    /// tank, which drains as it fades.
    pub fn frame(&mut self, dt: f32, progress: Option<(f32, ProgressState)>) {
        if let Some(p) = self.pulse.as_mut() {
            p.age += dt;
            if p.age > RING_LIFE {
                self.pulse = None;
            }
        }
        let k = 1.0 - (-dt * 3.0).exp();
        match progress {
            Some((level, state)) => {
                if self.shown < 0.01 {
                    // A new tank fills from empty.
                    self.level = 0.0;
                }
                self.shown += (1.0 - self.shown) * k;
                self.level += (level.clamp(0.0, 1.0) - self.level) * k;
                let figure = if state == ProgressState::Busy { 0.0 } else { 1.0 };
                self.figure += (figure - self.figure) * k;
                self.color = match state {
                    ProgressState::Normal => rgb(ACCENT),
                    ProgressState::Busy => rgb(ACCENT2),
                    ProgressState::Error => EMBER,
                    ProgressState::Paused => AMBER,
                };
            }
            None => {
                self.level -= self.level * k;
                self.shown -= self.shown * k * 0.6;
            }
        }
        if self.shown > 0.01 {
            self.clock += dt;
        }
    }

    /// Something is still moving on the glass.
    pub fn animating(&self) -> bool {
        self.pulse.is_some() || self.shown > 0.01
    }

    /// The shader's pulse, pulse color, progress and progress color.
    pub fn uniforms(&self) -> [[f32; 4]; 4] {
        let (pulse, pulse_color) = match self.pulse {
            Some(p) => {
                let (color, strength, width) = p.ring.look();
                ([p.at[0], p.at[1], p.age, strength], [color[0], color[1], color[2], width])
            }
            None => ([0.0; 4], [0.0; 4]),
        };
        let shown = if self.shown > 0.01 { self.shown } else { 0.0 };
        let c = self.color;
        [pulse, pulse_color, [self.level, shown, self.figure, self.clock], [c[0], c[1], c[2], 1.0]]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &mut Signals, secs: f32, progress: Option<(f32, ProgressState)>) {
        for _ in 0..(secs * 60.0) as usize {
            s.frame(1.0 / 60.0, progress);
        }
    }

    #[test]
    fn a_ring_spreads_then_fades_away() {
        let mut s = Signals::default();
        assert!(!s.animating() && s.uniforms()[0][3] == 0.0);
        s.pulse([0.2, 0.9], Ring::Failed);
        run(&mut s, 0.5, None);
        let [pulse, color, ..] = s.uniforms();
        assert_eq!((pulse[0], pulse[1]), (0.2, 0.9));
        assert!(pulse[2] > 0.45 && pulse[3] > 0.0);
        assert_eq!([color[0], color[1], color[2]], EMBER);
        run(&mut s, RING_LIFE, None);
        assert!(!s.animating() && s.uniforms()[0][3] == 0.0);
    }

    #[test]
    fn the_tank_follows_progress_then_drains_when_it_stops() {
        let mut s = Signals::default();
        run(&mut s, 2.0, Some((0.6, ProgressState::Normal)));
        let [_, _, tank, _] = s.uniforms();
        assert!((tank[0] - 0.6).abs() < 0.02 && tank[1] > 0.95 && tank[2] > 0.95, "{tank:?}");
        // No figure: it sloshes instead.
        run(&mut s, 2.0, Some((0.0, ProgressState::Busy)));
        assert!(s.uniforms()[2][2] < 0.05);
        run(&mut s, 4.0, None);
        assert!(!s.animating() && s.uniforms()[2][1] == 0.0);
        // The next one fills from empty, in its own color.
        s.frame(1.0 / 60.0, Some((0.9, ProgressState::Error)));
        let [_, _, tank, color] = s.uniforms();
        assert!(tank[0] < 0.1);
        assert_eq!([color[0], color[1], color[2]], EMBER);
    }
}
