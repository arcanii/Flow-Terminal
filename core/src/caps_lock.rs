//! Caps Lock's notice: a pill at the foot of the terminal, glowing orange,
//! while the user types into a pane with Caps Lock on, so that a password
//! or a command isn't typed in capitals by mistake. It comes back with each
//! key typed, and goes [`SHOWN`] after the last one, or at once once Caps
//! Lock is off.
//!
//! winit doesn't say whether Caps Lock is on: its modifiers are Shift,
//! Control, Alt and Super alone. So it's the system's answer where it has
//! one (`Platform::caps_lock`: the Mac's, Windows' and X11's), and
//! otherwise (Wayland) a guess from what each key typed ([`guess`]): a
//! capital letter without Shift, or a small one with it, which Caps Lock
//! and Shift together type on Windows and Linux. A key that says nothing of it (a digit, a capital
//! typed with Shift) leaves what was known, and the Caps Lock key itself
//! turns that over where the system doesn't say.

use std::time::{Duration, Instant};

/// How long the notice stays after the last key typed with Caps Lock on.
pub const SHOWN: Duration = Duration::from_millis(2500);

/// Whether `text`, what a key typed, is typing: something to read, not a
/// control character (Enter's, Backspace's, Esc's).
pub fn types(text: &str) -> bool {
    !text.is_empty() && !text.chars().any(char::is_control)
}

/// What a key that typed `text`, with or without Shift held, says of Caps
/// Lock: on, off, or nothing, for anything but one letter that has a
/// capital and a small form of its own, and for a capital typed with
/// Shift, which the Mac types with Caps Lock on as well.
pub fn guess(text: &str, shift: bool) -> Option<bool> {
    let mut chars = text.chars();
    let (Some(c), None) = (chars.next(), chars.next()) else { return None };
    match (capital(c)?, shift) {
        (true, false) => Some(true),
        (false, false) => Some(false),
        (false, true) => Some(true),
        (true, true) => None,
    }
}

/// Whether `c` is a capital letter (true) or a small one (false), where
/// each form is one letter that turns back into the other: so not ß, whose
/// capital is two, which Caps Lock leaves as it is, nor a title-case
/// letter such as ǅ, which is neither.
fn capital(c: char) -> Option<bool> {
    let (upper, lower) = (single(c.to_uppercase())?, single(c.to_lowercase())?);
    if upper == lower {
        None
    } else if c == upper && single(lower.to_uppercase()) == Some(c) {
        Some(true)
    } else if c == lower && single(upper.to_lowercase()) == Some(c) {
        Some(false)
    } else {
        None
    }
}

/// The one letter `letters` holds, or none for none or more.
fn single(mut letters: impl Iterator<Item = char>) -> Option<char> {
    let c = letters.next()?;
    letters.next().is_none().then_some(c)
}

/// What's known of Caps Lock and whether its notice shows.
#[derive(Debug, Default)]
pub struct CapsLock {
    /// Whether it's on: none until the system or a letter typed says.
    on: Option<bool>,
    /// When the notice last came back, while it shows.
    shown_at: Option<Instant>,
}

impl CapsLock {
    /// A key typed into a pane at `now`: `system` the system's answer,
    /// `guess` what the key typed says. With Caps Lock on, the notice
    /// comes, or stays for another [`SHOWN`]; off, it goes.
    pub fn typed(&mut self, system: Option<bool>, guess: Option<bool>, now: Instant) {
        if let Some(on) = system.or(guess) {
            self.on = Some(on);
        }
        self.shown_at = (self.on == Some(true)).then_some(now);
    }

    /// The Caps Lock key itself, pressed: the system's answer, else what
    /// was known turned over. Turned off, the notice goes at once; turned
    /// on, it waits for a key typed.
    pub fn pressed(&mut self, system: Option<bool>) {
        self.on = system.or(self.on.map(|on| !on));
        if self.on != Some(true) {
            self.shown_at = None;
        }
    }

    /// Before Flow waits: the notice goes once its time is up, or once
    /// `system` (asked only while it shows) says Caps Lock is off. True
    /// when it went, for a frame without it.
    pub fn tend(&mut self, system: impl FnOnce() -> Option<bool>, now: Instant) -> bool {
        let Some(at) = self.shown_at else { return false };
        if let Some(on) = system() {
            self.on = Some(on);
        }
        if now >= at + SHOWN || self.on != Some(true) {
            self.shown_at = None;
            return true;
        }
        false
    }

    /// When the notice goes, while it shows.
    pub fn due(&self) -> Option<Instant> {
        self.shown_at.map(|at| at + SHOWN)
    }

    /// Whether the notice shows at `now`.
    pub fn shown(&self, now: Instant) -> bool {
        self.shown_at.is_some_and(|at| now < at + SHOWN)
    }

    /// The window has lost the keys: the notice goes, and what was known
    /// is forgotten, since Caps Lock may change in another program, where
    /// Flow sees no key. The system, where it says, is asked again at the
    /// next key.
    pub fn left(&mut self) {
        *self = CapsLock::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_capital_without_shift_or_a_small_letter_with_it_says_caps_lock_is_on() {
        assert_eq!(guess("A", false), Some(true));
        assert_eq!(guess("a", true), Some(true), "Windows' and Linux's Caps Lock and Shift");
        assert_eq!(guess("a", false), Some(false));
        // The Mac types a capital with Shift whether Caps Lock is on or not.
        assert_eq!(guess("A", true), None);
        // Other alphabets too.
        assert_eq!((guess("Ж", false), guess("é", false), guess("Ω", false)), (Some(true), Some(false), Some(true)));
    }

    #[test]
    fn what_isnt_one_letter_of_two_forms_says_nothing() {
        for text in ["1", " ", "?", "ab", "", "ß", "ǅ", "あ", "中", "\r"] {
            assert_eq!(guess(text, false), None, "{text:?}");
            assert_eq!(guess(text, true), None, "{text:?}");
        }
    }

    #[test]
    fn typing_is_text_without_control_characters() {
        assert!(types("a") && types(" ") && types("é"));
        assert!(!types("") && !types("\r") && !types("\u{8}") && !types("\u{1b}"));
    }

    #[test]
    fn the_notice_comes_with_each_key_typed_while_caps_lock_is_on_and_goes_after() {
        let t0 = Instant::now();
        let mut caps = CapsLock::default();
        // Nothing known, a digit says nothing: no notice.
        caps.typed(None, guess("1", false), t0);
        assert!(!caps.shown(t0));
        // A capital without Shift: it comes.
        caps.typed(None, guess("A", false), t0);
        assert!(caps.shown(t0));
        assert_eq!(caps.due(), Some(t0 + SHOWN));
        // A digit now, Caps Lock known to be on: it stays longer.
        let t1 = t0 + Duration::from_secs(2);
        caps.typed(None, guess("1", false), t1);
        assert_eq!(caps.due(), Some(t1 + SHOWN));
        // Its time up, it goes, once.
        let late = t1 + SHOWN;
        assert!(!caps.shown(late));
        assert!(caps.tend(|| None, late));
        assert!(!caps.tend(|| None, late));
        assert_eq!(caps.due(), None);
        // A small letter: it's off, and stays gone.
        caps.typed(None, guess("a", false), late);
        assert!(!caps.shown(late));
    }

    #[test]
    fn the_systems_answer_comes_before_the_guess() {
        let now = Instant::now();
        let mut caps = CapsLock::default();
        // The Mac, Caps Lock on, a capital typed with Shift: the guess
        // can't tell, the system can.
        caps.typed(Some(true), guess("A", true), now);
        assert!(caps.shown(now));
        caps.typed(Some(false), guess("A", false), now);
        assert!(!caps.shown(now));
    }

    #[test]
    fn caps_lock_turned_off_takes_the_notice_away_at_once() {
        let now = Instant::now();
        let mut caps = CapsLock::default();
        caps.typed(None, Some(true), now);
        // Where the system doesn't say, the key turns over what was known.
        caps.pressed(None);
        assert!(!caps.shown(now));
        caps.typed(None, guess("1", false), now);
        assert!(!caps.shown(now), "known to be off now");
        caps.pressed(None);
        assert!(!caps.shown(now), "turned on, it waits for a key");
        caps.typed(None, guess("1", false), now);
        assert!(caps.shown(now));
        // Where the system says, before Flow waits.
        assert!(!caps.tend(|| Some(true), now));
        assert!(caps.tend(|| Some(false), now));
        assert!(!caps.shown(now));
    }

    #[test]
    fn leaving_the_window_forgets_what_was_known() {
        let now = Instant::now();
        let mut caps = CapsLock::default();
        caps.typed(None, Some(true), now);
        caps.left();
        assert!(!caps.shown(now));
        // Caps Lock may have gone off elsewhere: a digit says nothing now.
        caps.typed(None, guess("1", false), now);
        assert!(!caps.shown(now));
    }
}
