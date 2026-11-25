//! The drop-down hotkey: a key taken system-wide (`window.hotkey`) that
//! drops Flow's own window from the top of the screen, in front of every
//! other app's, and hides it again. Each OS crate registers it with its
//! system (`Platform::register_hotkey`) from the [`Hotkey`] read here, and
//! says where the window goes (`Platform::drop_down_rect`, the top part of
//! a screen that `window.drop_down_height` takes: [`top_part`]). The slide
//! down and back up ([`Slide`]), the wait before a window that lost the
//! keys is put away ([`Unfocused`]) and the key pressed in Settings to be
//! the hotkey ([`Hotkey::pressed`]) are worked out here too, away from the
//! window, so they're tested.

use std::time::{Duration, Instant};

use winit::keyboard::KeyCode;

/// A key, without its modifiers, which each OS crate maps to its own code
/// for it (the Mac's virtual key codes, Windows' virtual keys, X11's
/// keysyms).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// The key left of 1, which types ` on a US keyboard: the Quake
    /// console's.
    Grave,
    Space,
    /// F1 to F12.
    F(u8),
    /// A to Z, lowercase.
    Letter(char),
    /// 0 to 9.
    Digit(u8),
}

/// A key with the modifiers held for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hotkey {
    pub key: Key,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    /// ⌘ on the Mac, the Windows key on Windows, Super on Linux.
    pub cmd: bool,
}

/// The hotkeys Settings offers, as `window.hotkey` names them: none (off),
/// then ⌃`, ⌥Space, F12 and ⌃Space. Any other is pressed in Settings. Not F12
/// on Windows, which keeps it for debuggers (RegisterHotKey's docs; it
/// refuses F12 as already taken, alone and with Shift or Alt).
pub const CHOICES: &[&str] =
    if cfg!(windows) { &["", "ctrl+`", "alt+space", "ctrl+space"] } else { &["", "ctrl+`", "alt+space", "f12", "ctrl+space"] };

/// The keys a hotkey may be, by where they are on a US keyboard, as winit
/// names them: the letters, the digits and F1 to F12, which every system's
/// crate maps to its own code.
const LETTERS: [KeyCode; 26] = [
    KeyCode::KeyA, KeyCode::KeyB, KeyCode::KeyC, KeyCode::KeyD, KeyCode::KeyE, KeyCode::KeyF, KeyCode::KeyG, KeyCode::KeyH, KeyCode::KeyI,
    KeyCode::KeyJ, KeyCode::KeyK, KeyCode::KeyL, KeyCode::KeyM, KeyCode::KeyN, KeyCode::KeyO, KeyCode::KeyP, KeyCode::KeyQ, KeyCode::KeyR,
    KeyCode::KeyS, KeyCode::KeyT, KeyCode::KeyU, KeyCode::KeyV, KeyCode::KeyW, KeyCode::KeyX, KeyCode::KeyY, KeyCode::KeyZ,
];
const DIGITS: [KeyCode; 10] = [
    KeyCode::Digit0, KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4, KeyCode::Digit5, KeyCode::Digit6, KeyCode::Digit7,
    KeyCode::Digit8, KeyCode::Digit9,
];
const F_KEYS: [KeyCode; 12] = [
    KeyCode::F1, KeyCode::F2, KeyCode::F3, KeyCode::F4, KeyCode::F5, KeyCode::F6, KeyCode::F7, KeyCode::F8, KeyCode::F9, KeyCode::F10,
    KeyCode::F11, KeyCode::F12,
];

impl Key {
    /// The key at winit's `code`, where it's one a hotkey may be.
    pub fn of(code: KeyCode) -> Option<Key> {
        let at = |keys: &[KeyCode]| keys.iter().position(|&k| k == code);
        Some(match code {
            KeyCode::Backquote => Key::Grave,
            KeyCode::Space => Key::Space,
            _ => match (at(&LETTERS), at(&DIGITS), at(&F_KEYS)) {
                (Some(i), ..) => Key::Letter((b'a' + i as u8) as char),
                (_, Some(i), _) => Key::Digit(i as u8),
                (.., Some(i)) => Key::F(i as u8 + 1),
                _ => return None,
            },
        })
    }
}

impl Hotkey {
    /// `name` as `window.hotkey` holds it: modifiers and then a key, joined
    /// by `+`, in any case ("ctrl+`", "Alt+Space", "f12", "cmd+shift+t").
    /// None for "" (no hotkey), a key Flow doesn't know, or a modifier
    /// alone.
    pub fn parse(name: &str) -> Option<Hotkey> {
        let name = name.trim().to_ascii_lowercase();
        let mut parts: Vec<&str> = name.split('+').map(str::trim).collect();
        let key = key_named(parts.pop()?)?;
        let mut hotkey = Hotkey { key, ctrl: false, alt: false, shift: false, cmd: false };
        for part in parts {
            let held = match modifier_named(part)? {
                0 => &mut hotkey.ctrl,
                1 => &mut hotkey.alt,
                2 => &mut hotkey.shift,
                _ => &mut hotkey.cmd,
            };
            *held = true;
        }
        Some(hotkey)
    }

    /// The key at `code` pressed with these modifiers, where it can be a
    /// hotkey: one Flow maps (`Key::of`) held with ⌃, ⌥ or ⌘ (Ctrl, Alt or
    /// the Windows key or Super), or an F key alone. Shift alone isn't
    /// enough: the hotkey would take the capital letter from every app.
    pub fn pressed(code: KeyCode, ctrl: bool, alt: bool, shift: bool, cmd: bool) -> Option<Hotkey> {
        let key = Key::of(code)?;
        (ctrl || alt || cmd || matches!(key, Key::F(_))).then_some(Hotkey { key, ctrl, alt, shift, cmd })
    }

    /// As `window.hotkey` holds it, which `parse` reads back as this:
    /// modifiers in the order ctrl, alt, shift, cmd, then the key, all
    /// lowercase ("ctrl+`", "alt+space", "f12", "ctrl+shift+t").
    pub fn to_config(&self) -> String {
        let held = [(self.ctrl, "ctrl+"), (self.alt, "alt+"), (self.shift, "shift+"), (self.cmd, "cmd+")];
        let mut name: String = held.iter().filter(|(on, _)| *on).map(|(_, m)| *m).collect();
        match self.key {
            Key::Grave => name.push('`'),
            Key::Space => name.push_str("space"),
            Key::F(n) => name.push_str(&format!("f{n}")),
            Key::Letter(c) => name.push(c.to_ascii_lowercase()),
            Key::Digit(d) => name.push_str(&d.to_string()),
        }
        name
    }

    /// As people on this OS write it: "⌃`" and "⌥Space" on the Mac,
    /// "Ctrl+`" and "Alt+Space" elsewhere.
    pub fn label(&self) -> String {
        self.label_for(cfg!(target_os = "macos"))
    }

    /// As the Mac writes it (`mac`), or Windows and Linux.
    pub fn label_for(&self, mac: bool) -> String {
        let key = match self.key {
            Key::Grave => "`".to_string(),
            Key::Space => "Space".to_string(),
            Key::F(n) => format!("F{n}"),
            Key::Letter(c) => c.to_ascii_uppercase().to_string(),
            Key::Digit(d) => d.to_string(),
        };
        // The Mac's order, ⌃⌥⇧⌘, and Windows' Ctrl+Alt+Shift+Win.
        let held = [(self.ctrl, "⌃", "Ctrl+"), (self.alt, "⌥", "Alt+"), (self.shift, "⇧", "Shift+"), (self.cmd, "⌘", if cfg!(windows) { "Win+" } else { "Super+" })];
        let mut label: String = held.iter().filter(|(on, ..)| *on).map(|&(_, m, other)| if mac { m } else { other }).collect();
        label.push_str(&key);
        label
    }
}

/// Which modifier a word in `window.hotkey` (and `[keys]`, `keys.rs`)
/// names, lowercase already: 0 ctrl, 1 alt, 2 shift, 3 cmd.
pub(crate) fn modifier_named(word: &str) -> Option<usize> {
    Some(match word {
        "ctrl" | "control" | "⌃" => 0,
        "alt" | "option" | "opt" | "⌥" => 1,
        "shift" | "⇧" => 2,
        "cmd" | "command" | "super" | "win" | "meta" | "⌘" => 3,
        _ => return None,
    })
}

/// The key a name in `window.hotkey` names, lowercase already.
fn key_named(name: &str) -> Option<Key> {
    Some(match name {
        "`" | "grave" | "backquote" | "backtick" => Key::Grave,
        "space" => Key::Space,
        _ => {
            if let Some(n) = name.strip_prefix('f').and_then(|n| n.parse::<u8>().ok()) {
                return (1..=12).contains(&n).then_some(Key::F(n));
            }
            let mut chars = name.chars();
            match (chars.next(), chars.next()) {
                (Some(c @ 'a'..='z'), None) => Key::Letter(c),
                (Some(c @ '0'..='9'), None) => Key::Digit(c as u8 - b'0'),
                _ => return None,
            }
        }
    })
}

/// A rectangle on the screens, in pixels, its origin where winit's window
/// positions have theirs (the top left of the main screen, down and to the
/// right).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    /// Whether the point (`x`, `y`) is inside.
    fn holds(&self, x: i32, y: i32) -> bool {
        x >= self.x && y >= self.y && (x - self.x) < self.width as i32 && (y - self.y) < self.height as i32
    }

    /// Whether the two share any pixel.
    fn meets(&self, other: &Rect) -> bool {
        let (right, bottom) = (self.x + self.width as i32, self.y + self.height as i32);
        let (other_right, other_bottom) = (other.x + other.width as i32, other.y + other.height as i32);
        self.x < other_right && other.x < right && self.y < other_bottom && other.y < bottom
    }
}

/// The heights Settings offers for the window dropped down, as
/// `window.drop_down_height` names them: a third, half, two thirds or all
/// of the screen.
pub const HEIGHTS: [&str; 4] = ["third", "half", "two_thirds", "all"];

/// A share of a screen's height, `num` / `den`, kept as a fraction so a
/// third of 1080 pixels is 360, not 359.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Share {
    pub num: u32,
    pub den: u32,
}

impl Share {
    pub const HALF: Share = Share { num: 1, den: 2 };

    /// The share `window.drop_down_height` names (one of [`HEIGHTS`], in
    /// any case), half for a name Flow doesn't know.
    pub fn named(name: &str) -> Share {
        match name.trim().to_ascii_lowercase().as_str() {
            "third" => Share { num: 1, den: 3 },
            "two_thirds" => Share { num: 2, den: 3 },
            "all" => Share { num: 1, den: 1 },
            _ => Share::HALF,
        }
    }
}

/// Where the window dropped down goes on `screen`: across all of it, its
/// top `share`, never less than a pixel tall nor taller than the screen.
pub fn top_part(screen: Rect, share: Share) -> Rect {
    let height = screen.height as u64 * share.num as u64 / share.den.max(1) as u64;
    Rect { height: (height as u32).clamp(1, screen.height.max(1)), ..screen }
}

/// How long the window takes to slide down from above its screen, or back
/// up as it's put away.
pub const SLIDE: Duration = Duration::from_millis(180);

/// How often a sliding window is moved: about as often as a 120 Hz screen
/// draws.
pub const SLIDE_STEP: Duration = Duration::from_millis(8);

/// The window dropping down (from above its screen to where it drops) or
/// going away (back up, to be put away there). A move of the window's
/// origin alone, so its size and the surface drawn into stay as they are.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Slide {
    from: (i32, i32),
    to: (i32, i32),
    began: Instant,
    takes: Duration,
    /// When the window was last moved.
    stepped: Option<Instant>,
    /// Up and away: the window is put away once it's there.
    pub away: bool,
}

impl Slide {
    /// A slide from `from` to `to` begun at `now`, taking [`SLIDE`] for a
    /// window's height (`height` pixels) and less for less, as a slide
    /// turned back halfway has.
    pub fn new(from: (i32, i32), to: (i32, i32), height: u32, away: bool, now: Instant) -> Slide {
        let far = (to.0 - from.0).unsigned_abs().max((to.1 - from.1).unsigned_abs());
        let share = (far as f32 / height.max(1) as f32).min(1.0);
        // In whole nanoseconds, so a whole slide takes SLIDE exactly with
        // every Rust: an older one's `mul_f32` (1.92's) lands a few past.
        let takes = Duration::from_nanos((SLIDE.as_nanos() as f64 * share as f64).round() as u64);
        Slide { from, to, began: now, takes, stepped: None, away }
    }

    /// Where it ends.
    pub fn to(&self) -> (i32, i32) {
        self.to
    }

    /// Where the window is at `now`, and whether it has got there. Coming
    /// down it slows as it lands; going away it speeds up as it goes.
    pub fn at(&self, now: Instant) -> ((i32, i32), bool) {
        let t = match self.takes.is_zero() {
            true => 1.0,
            false => now.saturating_duration_since(self.began).as_secs_f32() / self.takes.as_secs_f32(),
        };
        if t >= 1.0 {
            return (self.to, true);
        }
        let eased = if self.away { t * t * t } else { 1.0 - (1.0 - t).powi(3) };
        let along = |a: i32, b: i32| a + ((b - a) as f32 * eased).round() as i32;
        ((along(self.from.0, self.to.0), along(self.from.1, self.to.1)), false)
    }

    /// Where to move the window at `now`, and whether it has got there.
    /// None while it last moved less than [`SLIDE_STEP`] ago and isn't
    /// there yet: each move comes back to Flow as an event, which would
    /// otherwise move it again at once.
    pub fn step(&mut self, now: Instant) -> Option<((i32, i32), bool)> {
        let (at, done) = self.at(now);
        if !done && self.stepped.is_some_and(|last| now < last + SLIDE_STEP) {
            return None;
        }
        self.stepped = Some(now);
        Some((at, done))
    }

    /// When the window is next to move: a step after it last did, or as
    /// the slide ends, if that's sooner.
    pub fn next_step(&self, now: Instant) -> Instant {
        let end = self.began + self.takes;
        self.stepped.map_or(now, |last| last + SLIDE_STEP).min(end)
    }
}

/// Where the window dropping down to `rect` starts: right above it, out of
/// sight past its screen's top. None where that's on another screen (one
/// above this), which the window would be seen crossing: there it shows
/// at once. `screens` are every screen's rectangle, in `rect`'s pixels.
pub fn slide_from(rect: Rect, screens: &[Rect]) -> Option<(i32, i32)> {
    let above = Rect { y: rect.y - rect.height as i32, ..rect };
    let home = screens.iter().position(|s| s.holds(rect.x, rect.y));
    let crossed = screens.iter().enumerate().any(|(i, s)| Some(i) != home && s.meets(&above));
    (!crossed).then_some((above.x, above.y))
}

/// How long Flow waits, once the window dropped down has lost the keys,
/// before putting it away (`window.drop_down_hides`): X11 takes them for a
/// moment as the hotkey's grab begins, and gives them back.
pub const HIDE_AFTER: Duration = Duration::from_millis(150);

/// Since when Flow has been without the keys, and whether the window
/// dropped down is to be put away once they've stayed away [`HIDE_AFTER`]
/// (`watched`). A loss that recent is still Flow in front for the hotkey,
/// since X11 takes the keys for its grab as the hotkey is pressed.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Unfocused {
    since: Option<Instant>,
    watched: bool,
}

impl Unfocused {
    /// Flow lost the keys at `now` (not `focused`), the window to be put
    /// away for that where `watch`, or has them back. A loss already noted
    /// keeps its time.
    pub fn focus(&mut self, focused: bool, watch: bool, now: Instant) {
        *self = match focused {
            true => Unfocused::default(),
            false => Unfocused { since: self.since.or(Some(now)), watched: watch },
        };
    }

    /// Stops waiting to put the window away: it came forward, or went.
    pub fn clear(&mut self) {
        self.watched = false;
    }

    /// Starts the wait again from `now`, where there is one: the keys were
    /// Flow's own file panel's, just closed, and come back next.
    pub fn wait_again(&mut self, now: Instant) {
        if self.watched {
            self.since = Some(now);
        }
    }

    /// When the wait is over, while there is one.
    pub fn due(&self) -> Option<Instant> {
        self.since.filter(|_| self.watched).map(|at| at + HIDE_AFTER)
    }

    /// Whether the wait is over at `now`: true once.
    pub fn take_due(&mut self, now: Instant) -> bool {
        let due = self.due().is_some_and(|due| now >= due);
        if due {
            self.watched = false;
        }
        due
    }

    /// Flow lost the keys less than [`HIDE_AFTER`] before `now`.
    pub fn just_lost(&self, now: Instant) -> bool {
        self.since.is_some_and(|at| now < at + HIDE_AFTER)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(key: Key, ctrl: bool, alt: bool) -> Hotkey {
        Hotkey { key, ctrl, alt, shift: false, cmd: false }
    }

    #[test]
    fn the_hotkeys_settings_offers_are_read() {
        let read: Vec<Option<Hotkey>> = CHOICES.iter().map(|c| Hotkey::parse(c)).collect();
        let mut offered = vec![None, Some(key(Key::Grave, true, false)), Some(key(Key::Space, false, true))];
        // Windows keeps F12 for debuggers.
        if !cfg!(windows) {
            offered.push(Some(key(Key::F(12), false, false)));
        }
        offered.push(Some(key(Key::Space, true, false)));
        assert_eq!(read, offered);
    }

    #[test]
    fn hand_written_hotkeys_are_read_in_any_case_and_spacing() {
        assert_eq!(Hotkey::parse(" Ctrl + Grave "), Some(key(Key::Grave, true, false)));
        assert_eq!(Hotkey::parse("Option+Space"), Some(key(Key::Space, false, true)));
        assert_eq!(
            Hotkey::parse("cmd+shift+T"),
            Some(Hotkey { key: Key::Letter('t'), ctrl: false, alt: false, shift: true, cmd: true })
        );
        assert_eq!(Hotkey::parse("ctrl+alt+1"), Some(key(Key::Digit(1), true, true)));
        assert_eq!(Hotkey::parse("F1"), Some(key(Key::F(1), false, false)));
    }

    #[test]
    fn names_flow_doesnt_know_are_no_hotkey() {
        for name in ["ctrl", "ctrl+", "f13", "f0", "hyper+space", "ctrl+enter", "ctrl+ab", "+"] {
            assert_eq!(Hotkey::parse(name), None, "{name}");
        }
    }

    #[test]
    fn hotkeys_are_written_as_each_system_writes_them() {
        let names = ["ctrl+`", "alt+space", "f12", "ctrl+space"];
        let labels = |mac| names.iter().map(|n| Hotkey::parse(n).unwrap().label_for(mac)).collect::<Vec<_>>();
        assert_eq!(labels(true), ["⌃`", "⌥Space", "F12", "⌃Space"]);
        assert_eq!(labels(false), ["Ctrl+`", "Alt+Space", "F12", "Ctrl+Space"]);
        let all = Hotkey::parse("cmd+shift+alt+ctrl+k").unwrap();
        assert_eq!(all.label_for(true), "⌃⌥⇧⌘K");
        assert!(all.label_for(false).starts_with("Ctrl+Alt+Shift+"));
    }

    #[test]
    fn the_window_drops_down_over_the_top_part_of_its_screen_that_its_height_says() {
        let screen = Rect { x: 0, y: 0, width: 2560, height: 1440 };
        let heights: Vec<u32> = HEIGHTS.iter().map(|h| top_part(screen, Share::named(h)).height).collect();
        assert_eq!(heights, [480, 720, 960, 1440]);
        assert_eq!(top_part(screen, Share::HALF), Rect { x: 0, y: 0, width: 2560, height: 720 });
        // A screen left of and above the main one keeps its origin, and an
        // odd height rounds down: a third of 1080 is 360, two of 1081 720.
        let beside = Rect { x: -1920, y: -200, width: 1920, height: 1081 };
        assert_eq!(top_part(beside, Share::HALF), Rect { x: -1920, y: -200, width: 1920, height: 540 });
        assert_eq!(top_part(Rect { height: 1080, ..beside }, Share::named("third")).height, 360);
        assert_eq!(top_part(beside, Share::named("Two_Thirds")).height, 720);
        assert_eq!(top_part(Rect { x: 5, y: 7, width: 10, height: 1 }, Share::named("third")).height, 1);
        // A name Flow doesn't know is half, and no share is taller than all.
        assert_eq!(Share::named("most"), Share::HALF);
        assert_eq!(top_part(screen, Share { num: 3, den: 2 }).height, 1440);
    }

    #[test]
    fn a_hotkey_written_for_the_config_reads_back_as_itself() {
        // Settings' own are written as they're listed, so a key pressed
        // that's one of them shows as that choice.
        for name in &CHOICES[1..] {
            assert_eq!(Hotkey::parse(name).unwrap().to_config(), *name);
        }
        let mut keys: Vec<Key> = vec![Key::Grave, Key::Space];
        keys.extend((1..=12).map(Key::F));
        keys.extend((0..=9).map(Key::Digit));
        keys.extend(('a'..='z').map(Key::Letter));
        for key in keys {
            for held in 0..16u8 {
                let hotkey = Hotkey { key, ctrl: held & 1 != 0, alt: held & 2 != 0, shift: held & 4 != 0, cmd: held & 8 != 0 };
                assert_eq!(Hotkey::parse(&hotkey.to_config()), Some(hotkey), "{}", hotkey.to_config());
            }
        }
        let all = Hotkey { key: Key::Letter('t'), ctrl: true, alt: true, shift: true, cmd: true };
        assert_eq!(all.to_config(), "ctrl+alt+shift+cmd+t");
    }

    #[test]
    fn winit_s_keys_are_the_hotkey_s_where_every_system_maps_them() {
        assert_eq!(Key::of(KeyCode::Backquote), Some(Key::Grave));
        assert_eq!(Key::of(KeyCode::Space), Some(Key::Space));
        assert_eq!((Key::of(KeyCode::KeyA), Key::of(KeyCode::KeyZ)), (Some(Key::Letter('a')), Some(Key::Letter('z'))));
        assert_eq!((Key::of(KeyCode::Digit0), Key::of(KeyCode::Digit9)), (Some(Key::Digit(0)), Some(Key::Digit(9))));
        assert_eq!((Key::of(KeyCode::F1), Key::of(KeyCode::F12)), (Some(Key::F(1)), Some(Key::F(12))));
        // Every letter, digit and F key to its own.
        let mapped: Vec<Key> = LETTERS.iter().chain(&DIGITS).chain(&F_KEYS).filter_map(|&c| Key::of(c)).collect();
        let mut names: Vec<String> = mapped.iter().map(|&key| Hotkey { key, ctrl: true, alt: false, shift: false, cmd: false }.to_config()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), 48);
        for code in [KeyCode::Enter, KeyCode::Escape, KeyCode::F13, KeyCode::ArrowUp, KeyCode::Numpad1, KeyCode::Minus, KeyCode::ShiftLeft] {
            assert_eq!(Key::of(code), None, "{code:?}");
        }
    }

    #[test]
    fn a_key_pressed_is_the_hotkey_with_a_modifier_or_alone_as_an_f_key() {
        let ctrl_grave = Hotkey::pressed(KeyCode::Backquote, true, false, false, false);
        assert_eq!(ctrl_grave.map(|h| h.to_config()).as_deref(), Some("ctrl+`"));
        assert_eq!(Hotkey::pressed(KeyCode::KeyT, false, false, true, true).map(|h| h.to_config()).as_deref(), Some("shift+cmd+t"));
        assert_eq!(Hotkey::pressed(KeyCode::F5, false, false, false, false).map(|h| h.to_config()).as_deref(), Some("f5"));
        assert_eq!(Hotkey::pressed(KeyCode::Space, false, true, false, false).map(|h| h.to_config()).as_deref(), Some("alt+space"));
        // A letter alone, or with Shift alone, would take typing from
        // every app; a key Flow can't map is never one.
        assert_eq!(Hotkey::pressed(KeyCode::KeyA, false, false, false, false), None);
        assert_eq!(Hotkey::pressed(KeyCode::KeyA, false, false, true, false), None);
        assert_eq!(Hotkey::pressed(KeyCode::Space, false, false, false, false), None);
        assert_eq!(Hotkey::pressed(KeyCode::Enter, true, false, false, false), None);
    }

    #[test]
    fn the_window_slides_down_slowing_as_it_lands_and_ends_where_it_drops() {
        let now = Instant::now();
        let slide = Slide::new((0, -700), (0, 0), 700, false, now);
        let at = |ms: u64| slide.at(now + Duration::from_millis(ms));
        assert_eq!(at(0), ((0, -700), false));
        // Most of the way in the first half of the time, then easing in.
        let ((_, half), _) = at(90);
        assert!((-100..-50).contains(&half), "{half}");
        let mut last = -701;
        for ms in (0..180).step_by(5) {
            let ((x, y), done) = at(ms);
            assert!(x == 0 && y >= last && !done, "{ms} ms: {y}");
            last = y;
        }
        assert_eq!(at(180), ((0, 0), true));
        assert_eq!(at(400), ((0, 0), true));
        assert_eq!(slide.to(), (0, 0));
    }

    #[test]
    fn the_window_moves_a_step_at_a_time_and_last_where_it_lands() {
        let now = Instant::now();
        let ms = |n: u64| now + Duration::from_millis(n);
        let mut slide = Slide::new((0, -700), (0, 0), 700, false, now);
        assert_eq!(slide.next_step(now), now);
        assert!(slide.step(ms(1)).is_some_and(|(_, done)| !done));
        // The move's own event, a moment after: not again so soon.
        assert_eq!(slide.step(ms(2)), None);
        assert_eq!(slide.next_step(ms(2)), ms(9));
        assert!(slide.step(ms(9)).is_some());
        // Near the end it wakes as the slide ends, and lands however soon.
        assert!(slide.step(ms(176)).is_some());
        assert_eq!(slide.next_step(ms(177)), ms(180));
        assert_eq!(slide.step(ms(180)), Some(((0, 0), true)));
    }

    #[test]
    fn the_window_goes_away_speeding_up_and_a_short_slide_is_quicker() {
        let now = Instant::now();
        let away = Slide::new((100, 66), (100, -1018), 1084, true, now);
        assert!(away.away);
        let ((_, early), _) = away.at(now + Duration::from_millis(45));
        assert!(early > 0, "it starts slowly: {early}");
        assert_eq!(away.at(now + SLIDE), ((100, -1018), true));
        // Turned back halfway, it takes half the time.
        let back = Slide::new((0, -350), (0, 0), 700, false, now);
        assert_eq!(back.at(now + Duration::from_millis(90)), ((0, 0), true));
        assert!(!back.at(now + Duration::from_millis(80)).1);
        // Nowhere to go is there at once.
        assert_eq!(Slide::new((3, 4), (3, 4), 700, false, now).at(now), ((3, 4), true));
    }

    #[test]
    fn the_slide_starts_above_the_screen_unless_another_screen_is_there() {
        let main = Rect { x: 0, y: 0, width: 2560, height: 1440 };
        let rect = top_part(Rect { y: 50, height: 1390, ..main }, Share::HALF);
        assert_eq!(slide_from(rect, &[main]), Some((0, 50 - 695)));
        // Screens beside it, even a taller one, aren't crossed.
        let right = Rect { x: 2560, y: -300, width: 1920, height: 1080 };
        let left = Rect { x: -1920, y: 0, width: 1920, height: 1080 };
        assert_eq!(slide_from(rect, &[main, right, left]), Some((0, -645)));
        // One above it would be.
        let above = Rect { x: 400, y: -1080, width: 1920, height: 1080 };
        assert_eq!(slide_from(rect, &[main, above]), None);
        // Dropped on the screen above, it slides from above that one.
        let up_there = top_part(above, Share::named("third"));
        assert_eq!(slide_from(up_there, &[main, above]), Some((400, -1440)));
    }

    #[test]
    fn the_window_is_put_away_only_once_the_keys_have_stayed_away() {
        let now = Instant::now();
        let ms = |n: u64| now + Duration::from_millis(n);
        let mut gone = Unfocused::default();
        assert_eq!(gone.due(), None);
        // X11's pair as the hotkey's grab begins: lost and back at once.
        gone.focus(false, true, now);
        gone.focus(true, false, ms(20));
        assert!(!gone.take_due(ms(500)));
        // Lost for good: not before the wait, then once.
        gone.focus(false, true, ms(600));
        gone.focus(false, true, ms(650));
        assert_eq!(gone.due(), Some(ms(750)));
        assert!(!gone.take_due(ms(740)));
        assert!(gone.take_due(ms(750)));
        assert!(!gone.take_due(ms(900)));
        // A loss not to be watched (Put away when unfocused isn't on) has
        // nothing to wait for.
        gone.focus(true, false, ms(950));
        gone.focus(false, false, ms(1000));
        assert_eq!(gone.due(), None);
        // Flow's own file panel had the keys: the wait starts again as it closes.
        gone.focus(true, false, ms(1001));
        gone.focus(false, true, ms(1002));
        gone.wait_again(ms(1400));
        assert!(!gone.take_due(ms(1500)));
        assert!(gone.take_due(ms(1550)));
        // With no wait there's none to start again.
        gone.wait_again(ms(1600));
        assert_eq!(gone.due(), None);
        // Brought forward or put away, it waits no more.
        gone.focus(true, false, ms(1650));
        gone.focus(false, true, ms(1700));
        gone.clear();
        assert_eq!(gone.due(), None);
    }

    #[test]
    fn keys_lost_a_moment_ago_still_leave_flow_in_front_for_the_hotkey() {
        let now = Instant::now();
        let ms = |n: u64| now + Duration::from_millis(n);
        let mut gone = Unfocused::default();
        assert!(!gone.just_lost(now));
        // The hotkey's grab takes the keys as it's pressed, whether or not
        // the window is to be put away for it.
        gone.focus(false, false, now);
        assert!(gone.just_lost(ms(100)) && !gone.just_lost(ms(150)));
        gone.focus(true, false, ms(200));
        assert!(!gone.just_lost(ms(201)));
    }
}
