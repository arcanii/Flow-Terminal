//! Mouse reports for programs that ask for them (xterm conventions): clicks
//! once a program sets `?1000h`, drags with `?1002h`, any motion with
//! `?1003h`, and the wheel with each. They're sent in SGR form (`?1006h`,
//! `CSI < b ; x ; y M`, `m` on release) when the program asked for it, else
//! in xterm's bytes (`CSI M` and three numbers offset by 32, UTF-8 encoded
//! with `?1005h`).

use alacritty_terminal::term::TermMode;
use winit::event::MouseButton;
use winit::keyboard::ModifiersState;

/// The buttons as xterm numbers them, and the wheel's notches.
pub const LEFT: u8 = 0;
pub const MIDDLE: u8 = 1;
pub const RIGHT: u8 = 2;
const WHEEL_UP: u8 = 64;
const WHEEL_DOWN: u8 = 65;
/// Added to a button for motion, and motion with no button held.
const MOTION: u8 = 32;
const NO_BUTTON: u8 = 3;

/// Whether the program has asked for any mouse reports.
pub fn wanted(mode: TermMode) -> bool {
    mode.intersects(TermMode::MOUSE_MODE)
}

/// What the program is told about the mouse: the buttons held, so their
/// drags and releases go to the program wherever the pointer goes, and the
/// cell last reported, so motion is sent only on reaching another.
#[derive(Debug, Default)]
pub struct MouseReport {
    /// Each as Flow and the program know it, the last pressed last.
    held: Vec<(MouseButton, u8)>,
    cell: Option<(usize, usize)>,
}

impl MouseReport {
    /// A button the program was told of is down.
    pub fn holding(&self) -> bool {
        !self.held.is_empty()
    }

    /// `button` went down at `cell`, which the program takes as `code`
    /// (a Control-click on macOS is a right click, so the two can differ).
    pub fn press(&mut self, mode: TermMode, button: MouseButton, code: u8, mods: ModifiersState, cell: (usize, usize)) -> Vec<u8> {
        self.held.retain(|(b, _)| *b != button);
        self.held.push((button, code));
        self.cell = Some(cell);
        encode(mode, code | bits(mods), cell, false).unwrap_or_default()
    }

    /// `button` came up: the report of its release if the program was told it
    /// went down, else `None`, and the click is Flow's.
    pub fn release(&mut self, mode: TermMode, button: MouseButton, mods: ModifiersState, cell: (usize, usize)) -> Option<Vec<u8>> {
        let at = self.held.iter().position(|(b, _)| *b == button)?;
        let (_, code) = self.held.remove(at);
        self.cell = Some(cell);
        Some(if wanted(mode) { encode(mode, code | bits(mods), cell, true).unwrap_or_default() } else { Vec::new() })
    }

    /// The pointer moved to `cell`: a report if the program asked for drags
    /// and a button is held (the last pressed, as xterm says), or for all
    /// motion, and it's a cell it wasn't told of last.
    pub fn motion(&mut self, mode: TermMode, mods: ModifiersState, cell: (usize, usize)) -> Option<Vec<u8>> {
        let button = match self.held.last() {
            Some(&(_, code)) if mode.intersects(TermMode::MOUSE_DRAG | TermMode::MOUSE_MOTION) => code,
            None if mode.contains(TermMode::MOUSE_MOTION) => NO_BUTTON,
            _ => return None,
        };
        if self.cell == Some(cell) {
            return None;
        }
        self.cell = Some(cell);
        encode(mode, (button + MOTION) | bits(mods), cell, false)
    }

    /// The wheel turned a notch, up or down, over `cell`.
    pub fn wheel(&mut self, mode: TermMode, up: bool, mods: ModifiersState, cell: (usize, usize)) -> Vec<u8> {
        self.cell = Some(cell);
        encode(mode, if up { WHEEL_UP } else { WHEEL_DOWN } | bits(mods), cell, false).unwrap_or_default()
    }

    /// Forgets a held button without telling the program, as when its tab
    /// closes.
    pub fn forget(&mut self) {
        *self = Self::default();
    }
}

/// The modifiers' bits: Shift 4, Alt (Meta) 8, Control 16.
fn bits(mods: ModifiersState) -> u8 {
    4 * mods.shift_key() as u8 + 8 * mods.alt_key() as u8 + 16 * mods.control_key() as u8
}

/// A report of `code` at `cell` (counted from 0), or `None` where xterm's
/// bytes can't hold the cell: past column or row 222, or 2014 with UTF-8.
/// Only SGR says which button came up; xterm's bytes say 3 for any.
fn encode(mode: TermMode, code: u8, (col, row): (usize, usize), release: bool) -> Option<Vec<u8>> {
    if mode.contains(TermMode::SGR_MOUSE) {
        return Some(format!("\x1b[<{code};{};{}{}", col + 1, row + 1, if release { 'm' } else { 'M' }).into_bytes());
    }
    let code = if release { (code & !0b11) | NO_BUTTON } else { code };
    let mut out = b"\x1b[M".to_vec();
    for n in [u32::from(code), col as u32 + 1, row as u32 + 1].map(|n| n + 32) {
        if mode.contains(TermMode::UTF8_MOUSE) {
            let c = char::from_u32(n).filter(|_| n < 2048)?;
            out.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes());
        } else {
            out.push(u8::try_from(n).ok()?);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLICKS: TermMode = TermMode::MOUSE_REPORT_CLICK;
    const SGR: TermMode = TermMode::MOUSE_REPORT_CLICK.union(TermMode::SGR_MOUSE);
    const NONE: ModifiersState = ModifiersState::empty();

    #[test]
    fn a_click_is_reported_in_sgr_with_the_button_that_came_up() {
        let mut m = MouseReport::default();
        assert_eq!(m.press(SGR, MouseButton::Left, LEFT, NONE, (4, 9)), b"\x1b[<0;5;10M");
        assert!(m.holding());
        assert_eq!(m.release(SGR, MouseButton::Left, NONE, (4, 9)).unwrap(), b"\x1b[<0;5;10m");
        assert!(!m.holding());
        m.press(SGR, MouseButton::Right, RIGHT, NONE, (0, 0));
        assert_eq!(m.release(SGR, MouseButton::Right, NONE, (0, 0)).unwrap(), b"\x1b[<2;1;1m");
    }

    #[test]
    fn xterm_bytes_offset_each_number_by_32_and_release_as_button_3() {
        let mut m = MouseReport::default();
        assert_eq!(m.press(CLICKS, MouseButton::Middle, MIDDLE, NONE, (0, 1)), b"\x1b[M\x21\x21\x22");
        assert_eq!(m.release(CLICKS, MouseButton::Middle, NONE, (0, 1)).unwrap(), b"\x1b[M\x23\x21\x22");
    }

    #[test]
    fn modifiers_add_their_bits() {
        let mut m = MouseReport::default();
        let mods = ModifiersState::CONTROL | ModifiersState::ALT;
        assert_eq!(m.press(SGR, MouseButton::Left, LEFT, mods, (1, 1)), b"\x1b[<24;2;2M");
    }

    #[test]
    fn a_release_not_reported_down_is_flows() {
        let mut m = MouseReport::default();
        assert_eq!(m.release(SGR, MouseButton::Left, NONE, (0, 0)), None);
        m.press(SGR, MouseButton::Left, LEFT, NONE, (0, 0));
        assert_eq!(m.release(SGR, MouseButton::Right, NONE, (0, 0)), None, "another button");
        // The program stopped asking while the button was down: the release is
        // still the program's, but nothing is sent.
        assert_eq!(m.release(TermMode::empty(), MouseButton::Left, NONE, (0, 0)), Some(Vec::new()));
    }

    #[test]
    fn two_buttons_held_each_come_up() {
        let mut m = MouseReport::default();
        let drags = SGR | TermMode::MOUSE_DRAG;
        m.press(drags, MouseButton::Left, LEFT, NONE, (0, 0));
        m.press(drags, MouseButton::Right, RIGHT, NONE, (0, 0));
        assert_eq!(m.motion(drags, NONE, (1, 0)).unwrap(), b"\x1b[<34;2;1M", "the last pressed drags");
        assert_eq!(m.release(drags, MouseButton::Left, NONE, (1, 0)).unwrap(), b"\x1b[<0;2;1m");
        assert!(m.holding());
        assert_eq!(m.release(drags, MouseButton::Right, NONE, (1, 0)).unwrap(), b"\x1b[<2;2;1m");
        assert!(!m.holding());
    }

    #[test]
    fn a_control_click_taken_as_right_comes_up_as_right() {
        let mut m = MouseReport::default();
        m.press(SGR, MouseButton::Left, RIGHT, NONE, (2, 3));
        assert_eq!(m.release(SGR, MouseButton::Left, NONE, (2, 3)).unwrap(), b"\x1b[<2;3;4m");
    }

    #[test]
    fn drags_are_reported_once_a_cell_only_when_asked_for() {
        let mut m = MouseReport::default();
        m.press(SGR, MouseButton::Left, LEFT, NONE, (0, 0));
        assert_eq!(m.motion(SGR, NONE, (1, 0)), None, "clicks only");
        let drags = SGR | TermMode::MOUSE_DRAG;
        assert_eq!(m.motion(drags, NONE, (2, 0)).unwrap(), b"\x1b[<32;3;1M");
        assert_eq!(m.motion(drags, NONE, (2, 0)), None, "the same cell");
        m.release(drags, MouseButton::Left, NONE, (2, 0));
        assert_eq!(m.motion(drags, NONE, (3, 0)), None, "no button held");
    }

    #[test]
    fn all_motion_reports_no_button_as_3() {
        let mut m = MouseReport::default();
        let any = TermMode::MOUSE_MOTION | TermMode::SGR_MOUSE;
        assert_eq!(m.motion(any, NONE, (5, 5)).unwrap(), b"\x1b[<35;6;6M");
        m.press(any, MouseButton::Right, RIGHT, NONE, (5, 5));
        assert_eq!(m.motion(any, NONE, (6, 5)).unwrap(), b"\x1b[<34;7;6M");
    }

    #[test]
    fn the_wheel_is_buttons_64_and_65() {
        let mut m = MouseReport::default();
        assert_eq!(m.wheel(SGR, true, NONE, (0, 0)), b"\x1b[<64;1;1M");
        assert_eq!(m.wheel(CLICKS, false, NONE, (0, 0)), b"\x1b[M\x61\x21\x21");
    }

    #[test]
    fn far_cells_need_sgr_or_utf8() {
        let mut m = MouseReport::default();
        assert_eq!(m.press(CLICKS, MouseButton::Left, LEFT, NONE, (222, 0)), b"\x1b[M\x20\xff\x21");
        assert_eq!(m.press(CLICKS, MouseButton::Left, LEFT, NONE, (223, 0)), b"", "past 255");
        let utf8 = CLICKS | TermMode::UTF8_MOUSE;
        assert_eq!(m.press(utf8, MouseButton::Left, LEFT, NONE, (300, 0)), "\x1b[M\u{20}\u{14d}\u{21}".as_bytes());
        assert_eq!(m.press(SGR, MouseButton::Left, LEFT, NONE, (300, 0)), b"\x1b[<0;301;1M");
    }

    #[test]
    fn any_of_the_three_modes_wants_reports() {
        assert!(!wanted(TermMode::SGR_MOUSE), "the form alone asks for nothing");
        for mode in [TermMode::MOUSE_REPORT_CLICK, TermMode::MOUSE_DRAG, TermMode::MOUSE_MOTION] {
            assert!(wanted(mode));
        }
    }
}
