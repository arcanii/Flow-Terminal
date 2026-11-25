//! The terminal's scrollbar along each pane's right edge (`shell.scrollbar`):
//! where its track and thumb go for the scrollback as it's scrolled, the
//! place in the scrollback a point on it stands for, and how it shows and
//! fades. `ui::draw_terminal` draws it and `App` drags it; neither needs a
//! window for what's here, so it's tested on its own.

use std::time::{Duration, Instant};

use crate::ui::Rect;

/// What Settings offers: shown while scrolling (scrolled back, the wheel
/// turning, the pointer at the pane's edge), always where there's a
/// scrollback, or never.
pub const MODES: [&str; 3] = ["scrolling", "always", "never"];

/// How long the bar stays once there's no more reason to show it, and how
/// long it then takes to fade.
pub const HOLD: Duration = Duration::from_millis(900);
pub const FADE: Duration = Duration::from_millis(350);

/// The bar's room along the edge, in points: the thumb's width as it rests
/// and under the pointer, its gap from the edges, and the strip that takes
/// the pointer, wider than the thumb so it's easy to catch.
const THIN: f32 = 5.0;
const WIDE: f32 = 9.0;
const MARGIN: f32 = 3.0;
const ZONE: f32 = 16.0;
/// The thumb is never shorter than this, however long the scrollback.
const LEAST: f32 = 24.0;

/// A pane's scrollbar as laid out in the window, in pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bar {
    /// The track the thumb runs along, as wide as the thumb under the
    /// pointer.
    pub track: Rect,
    /// The thumb, as wide as the track; drawn thinner while it rests.
    pub thumb: Rect,
    /// What takes the pointer: the track's height, from the pane's edge.
    pub zone: Rect,
    /// The thumb's width as it rests.
    pub thin: f32,
}

impl Bar {
    /// The bar for a pane whose cells are in `term`, at the display's
    /// `scale`, with `history` lines above the screen's `screen` and the
    /// view scrolled `offset` of them up from the bottom: none without a
    /// scrollback, or where the pane is too short to hold one.
    pub fn place(term: Rect, scale: f32, history: usize, screen: usize, offset: usize) -> Option<Bar> {
        let (margin, wide) = (MARGIN * scale, WIDE * scale);
        let track = Rect::new(term.x + term.w - margin - wide, term.y + margin, wide, term.h - 2.0 * margin);
        if history == 0 || screen == 0 || track.h < LEAST * scale || term.w < ZONE * scale * 2.0 {
            return None;
        }
        let share = screen as f32 / (history + screen) as f32;
        let h = (track.h * share).max(LEAST * scale).min(track.h);
        // At the bottom (offset 0) the thumb sits at the track's foot, and
        // scrolled all the way back at its top.
        let up = offset.min(history) as f32 / history as f32;
        let y = track.y + (track.h - h) * (1.0 - up);
        let zone_w = (ZONE * scale).max(wide + margin);
        Some(Bar {
            track,
            thumb: Rect::new(track.x, y, wide, h),
            zone: Rect::new(term.x + term.w - zone_w, track.y, zone_w, track.h),
            thin: THIN * scale,
        })
    }

    /// The offset (lines scrolled up from the bottom, of `history`) that
    /// puts the thumb's top at `y` less `grab`, the point of the thumb that
    /// was taken hold of, kept within the track.
    pub fn offset_at(&self, y: f32, grab: f32, history: usize) -> usize {
        let room = self.track.h - self.thumb.h;
        if room <= 0.0 {
            return 0;
        }
        let along = ((y - grab - self.track.y) / room).clamp(0.0, 1.0);
        ((1.0 - along) * history as f32).round() as usize
    }

    /// Where on the track the mark for line `line` goes (counted from the
    /// scrollback's top, of `history` lines above the screen's `screen`): the
    /// thumb's middle as it would be with that line in the middle of the
    /// screen, so a press on the mark brings it into view.
    pub fn mark_y(&self, line: usize, history: usize, screen: usize) -> f32 {
        let room = self.track.h - self.thumb.h;
        let top = (line as f32 - screen as f32 / 2.0).clamp(0.0, history as f32);
        let along = if history == 0 { 0.0 } else { top / history as f32 };
        self.track.y + room * along + self.thumb.h / 2.0
    }

    /// Where on the thumb a press at `y` takes hold of it: where it is, on
    /// the thumb; off it, on the track, its middle, which then jumps there.
    pub fn grab_at(&self, y: f32) -> (f32, bool) {
        let on = y >= self.thumb.y && y < self.thumb.y + self.thumb.h;
        if on { (y - self.thumb.y, true) } else { (self.thumb.h * 0.5, false) }
    }
}

/// How a pane's scrollbar shows while it's `scrolling`: lit while it has a
/// reason to (`see`'s `live`), and for `HOLD` after it last had one or the
/// view last moved, then fading over `FADE`.
#[derive(Debug, Clone, Copy, Default)]
pub struct Light {
    /// When it last had a reason to show, or the view last moved.
    since: Option<Instant>,
    live: bool,
    offset: usize,
}

impl Light {
    /// A frame's look at the pane: whether it has a reason to show now
    /// (scrolled back, the pointer at its edge, its thumb held), and how
    /// far it's scrolled back. Losing its reason, or moving, starts the
    /// hold afresh, so a bar that was lit for a while still fades slowly.
    pub fn see(&mut self, live: bool, offset: usize, now: Instant) {
        if live || self.live || offset != self.offset {
            self.since = Some(now);
        }
        (self.live, self.offset) = (live, offset);
    }

    /// The wheel turned over the pane, whether or not the view moved (at
    /// the bottom it can't), which shows the bar a moment.
    pub fn nudge(&mut self, now: Instant) {
        self.since = Some(now);
    }

    /// How much it shows at `now`, from 0 (gone) to 1.
    pub fn shown(&self, now: Instant) -> f32 {
        if self.live {
            return 1.0;
        }
        let Some(since) = self.since else { return 0.0 };
        let past = now.saturating_duration_since(since);
        if past <= HOLD {
            1.0
        } else {
            (1.0 - (past - HOLD).as_secs_f32() / FADE.as_secs_f32()).max(0.0)
        }
    }

    /// Whether it's still to fade, so frames should keep coming: not while
    /// it's lit (it holds still then) or once it's gone.
    pub fn fading(&self, now: Instant) -> bool {
        !self.live && self.since.is_some_and(|since| now.saturating_duration_since(since) < HOLD + FADE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TERM: Rect = Rect { x: 10.0, y: 40.0, w: 800.0, h: 606.0 };

    #[test]
    fn the_thumb_sits_at_the_foot_at_the_bottom_and_at_the_top_scrolled_all_the_way_back() {
        let bottom = Bar::place(TERM, 1.0, 300, 30, 0).unwrap();
        assert_eq!(bottom.thumb.y + bottom.thumb.h, bottom.track.y + bottom.track.h);
        let top = Bar::place(TERM, 1.0, 300, 30, 300).unwrap();
        assert_eq!(top.thumb.y, top.track.y);
        // Half way back, half way up.
        let half = Bar::place(TERM, 1.0, 300, 30, 150).unwrap();
        let room = half.track.h - half.thumb.h;
        assert!((half.thumb.y - (half.track.y + room * 0.5)).abs() < 0.01);
    }

    #[test]
    fn the_thumb_is_the_screens_share_of_the_lines_but_never_too_short_to_take_hold_of() {
        let bar = Bar::place(TERM, 1.0, 90, 30, 0).unwrap();
        assert!((bar.thumb.h - bar.track.h * 0.25).abs() < 0.01);
        let long = Bar::place(TERM, 2.0, 100_000, 30, 0).unwrap();
        assert_eq!(long.thumb.h, LEAST * 2.0);
    }

    #[test]
    fn there_is_no_bar_without_a_scrollback_or_room_for_one() {
        assert!(Bar::place(TERM, 1.0, 0, 30, 0).is_none());
        assert!(Bar::place(Rect::new(0.0, 0.0, 800.0, 20.0), 1.0, 100, 1, 0).is_none());
        assert!(Bar::place(Rect::new(0.0, 0.0, 20.0, 600.0), 1.0, 100, 30, 0).is_none());
    }

    #[test]
    fn the_bar_keeps_to_the_panes_right_edge_and_takes_the_pointer_wider_than_it_is() {
        let bar = Bar::place(TERM, 2.0, 100, 30, 0).unwrap();
        let right = TERM.x + TERM.w;
        assert_eq!(bar.track.x + bar.track.w, right - MARGIN * 2.0);
        assert_eq!(bar.zone.x + bar.zone.w, right);
        assert!(bar.zone.w > bar.track.w && bar.zone.x < bar.track.x);
        assert!(bar.thin < bar.track.w);
    }

    #[test]
    fn dragging_the_thumb_gives_back_the_offset_it_was_drawn_at() {
        for offset in [0, 1, 37, 150, 299, 300] {
            let bar = Bar::place(TERM, 1.0, 300, 30, offset).unwrap();
            let grab = bar.thumb.h * 0.3;
            assert_eq!(bar.offset_at(bar.thumb.y + grab, grab, 300), offset);
        }
        // Past either end of the track, the ends.
        let bar = Bar::place(TERM, 1.0, 300, 30, 10).unwrap();
        assert_eq!(bar.offset_at(-500.0, 0.0, 300), 300);
        assert_eq!(bar.offset_at(5000.0, 0.0, 300), 0);
    }

    #[test]
    fn a_press_on_a_lines_mark_brings_that_line_into_view() {
        let (history, screen) = (300, 30);
        let bar = Bar::place(TERM, 1.0, history, screen, 0).unwrap();
        for line in [0, 14, 15, 100, 200, 314, 329] {
            let y = bar.mark_y(line, history, screen);
            assert!(y >= bar.track.y && y <= bar.track.y + bar.track.h, "{line}: on the track");
            let (grab, _) = bar.grab_at(y);
            let offset = bar.offset_at(y, grab, history);
            // The screen's top line from the scrollback's top, and so the
            // lines in view.
            let top = history - offset;
            assert!((top..top + screen).contains(&line), "{line} is in view at offset {offset}");
        }
        // Marks keep their order down the track.
        assert!(bar.mark_y(10, history, screen) <= bar.mark_y(200, history, screen));
        assert!(bar.mark_y(200, history, screen) < bar.mark_y(329, history, screen));
    }

    #[test]
    fn a_press_on_the_track_jumps_the_thumbs_middle_there() {
        let bar = Bar::place(TERM, 1.0, 300, 30, 0).unwrap();
        let (grab, on) = bar.grab_at(bar.thumb.y + 4.0);
        assert!(on);
        assert_eq!(grab, 4.0);
        // The middle of the track, above the thumb: the thumb's middle goes
        // there, half way back.
        let middle = bar.track.y + bar.track.h * 0.5;
        let (grab, on) = bar.grab_at(middle);
        assert!(!on);
        assert_eq!(bar.offset_at(middle, grab, 300), 150);
    }

    #[test]
    fn the_bar_holds_after_its_reason_goes_then_fades() {
        let t0 = Instant::now();
        let mut light = Light::default();
        light.see(false, 0, t0);
        assert_eq!(light.shown(t0), 0.0);
        assert!(!light.fading(t0));
        // Scrolled back: lit, and nothing to fade, so no frames needed.
        light.see(true, 12, t0);
        assert_eq!(light.shown(t0 + Duration::from_secs(60)), 1.0);
        assert!(!light.fading(t0 + Duration::from_secs(60)));
        // Back at the bottom a minute later: the hold starts then.
        let t1 = t0 + Duration::from_secs(60);
        light.see(false, 0, t1);
        assert_eq!(light.shown(t1 + HOLD), 1.0);
        assert!(light.fading(t1 + HOLD));
        let half = light.shown(t1 + HOLD + FADE / 2);
        assert!(half > 0.4 && half < 0.6, "{half}");
        assert_eq!(light.shown(t1 + HOLD + FADE), 0.0);
        assert!(!light.fading(t1 + HOLD + FADE));
        // A later frame with nothing new keeps it gone.
        light.see(false, 0, t1 + HOLD + FADE * 2);
        assert_eq!(light.shown(t1 + HOLD + FADE * 2), 0.0);
    }

    #[test]
    fn the_wheel_at_the_bottom_shows_the_bar_a_moment() {
        let t0 = Instant::now();
        let mut light = Light::default();
        light.see(false, 0, t0);
        light.nudge(t0);
        assert_eq!(light.shown(t0), 1.0);
        light.see(false, 0, t0 + HOLD);
        assert!(light.shown(t0 + HOLD + FADE) == 0.0);
    }
}
