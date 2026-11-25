//! Keyboard and paste encoding for the PTY (xterm conventions).

use alacritty_terminal::term::TermMode;
use winit::event::KeyEvent;
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::platform::modifier_supplement::KeyEventExtModifierSupplement;

/// Bytes to send for a key press. `option_as_meta` only matters on macOS,
/// where Option types characters (ƒ, ©) unless it is set to act as Meta.
pub fn encode_key(event: &KeyEvent, mods: ModifiersState, mode: TermMode, option_as_meta: bool) -> Option<Vec<u8>> {
    let ctrl = mods.control_key();
    let alt = mods.alt_key();
    let shift = mods.shift_key();
    let meta = alt && (!cfg!(target_os = "macos") || option_as_meta);
    let m = 1 + shift as u8 + 2 * alt as u8 + 4 * ctrl as u8;
    let app_cursor = mode.contains(TermMode::APP_CURSOR);

    let csi = |fin: char| -> Vec<u8> {
        if m > 1 {
            format!("\x1b[1;{m}{fin}").into_bytes()
        } else if app_cursor {
            format!("\x1bO{fin}").into_bytes()
        } else {
            format!("\x1b[{fin}").into_bytes()
        }
    };
    let tilde = |n: u8| -> Vec<u8> {
        if m > 1 {
            format!("\x1b[{n};{m}~").into_bytes()
        } else {
            format!("\x1b[{n}~").into_bytes()
        }
    };
    let ss3 = |fin: char| -> Vec<u8> {
        if m > 1 {
            format!("\x1b[1;{m}{fin}").into_bytes()
        } else {
            format!("\x1bO{fin}").into_bytes()
        }
    };

    if cfg!(target_os = "macos") {
        let named = match &event.logical_key {
            Key::Named(n) => Some(*n),
            _ => None,
        };
        if mods.super_key() {
            // ⌘← ⌘→ jump to the start and end of the line and ⌘⌫ deletes to
            // its start, as in iTerm2. Other ⌘ keys are menu shortcuts.
            return match named {
                Some(NamedKey::ArrowLeft) => Some(b"\x01".to_vec()),
                Some(NamedKey::ArrowRight) => Some(b"\x05".to_vec()),
                Some(NamedKey::Backspace) => Some(b"\x15".to_vec()),
                _ => None,
            };
        }
        // ⌥← ⌥→ move by word and ⌥⌦ deletes one, as in Terminal.
        if alt && !ctrl && !shift {
            match named {
                Some(NamedKey::ArrowLeft) => return Some(b"\x1bb".to_vec()),
                Some(NamedKey::ArrowRight) => return Some(b"\x1bf".to_vec()),
                Some(NamedKey::Delete) => return Some(b"\x1bd".to_vec()),
                _ => {}
            }
        }
    }

    if let Key::Named(named) = &event.logical_key {
        let bytes = match named {
            NamedKey::Enter => {
                if meta {
                    b"\x1b\r".to_vec()
                } else {
                    b"\r".to_vec()
                }
            }
            NamedKey::Backspace => {
                if ctrl {
                    b"\x08".to_vec()
                } else if alt {
                    b"\x1b\x7f".to_vec()
                } else {
                    b"\x7f".to_vec()
                }
            }
            NamedKey::Tab => {
                if shift {
                    b"\x1b[Z".to_vec()
                } else {
                    b"\t".to_vec()
                }
            }
            NamedKey::Escape => b"\x1b".to_vec(),
            NamedKey::Space => {
                if ctrl {
                    vec![0]
                } else if meta {
                    b"\x1b ".to_vec()
                } else {
                    b" ".to_vec()
                }
            }
            NamedKey::ArrowUp => csi('A'),
            NamedKey::ArrowDown => csi('B'),
            NamedKey::ArrowRight => csi('C'),
            NamedKey::ArrowLeft => csi('D'),
            NamedKey::Home => csi('H'),
            NamedKey::End => csi('F'),
            NamedKey::PageUp => tilde(5),
            NamedKey::PageDown => tilde(6),
            NamedKey::Insert => tilde(2),
            NamedKey::Delete => tilde(3),
            NamedKey::F1 => ss3('P'),
            NamedKey::F2 => ss3('Q'),
            NamedKey::F3 => ss3('R'),
            NamedKey::F4 => ss3('S'),
            NamedKey::F5 => tilde(15),
            NamedKey::F6 => tilde(17),
            NamedKey::F7 => tilde(18),
            NamedKey::F8 => tilde(19),
            NamedKey::F9 => tilde(20),
            NamedKey::F10 => tilde(21),
            NamedKey::F11 => tilde(23),
            NamedKey::F12 => tilde(24),
            _ => return None,
        };
        return Some(bytes);
    }

    let text = event.text_with_all_modifiers().map(|t| t.to_string());
    // AltGr arrives as Ctrl+Alt on Windows; trust the layout's text then.
    if ctrl && alt {
        if let Some(t) = &text {
            if t.chars().all(|c| !c.is_control()) && !t.is_empty() {
                return Some(t.as_bytes().to_vec());
            }
        }
    }
    if ctrl {
        if let Key::Character(base) = event.key_without_modifiers() {
            if let Some(code) = base.chars().next().and_then(ctrl_code) {
                let mut out = Vec::new();
                if alt {
                    out.push(0x1b);
                }
                out.push(code);
                return Some(out);
            }
        }
    }
    let text = text?;
    if text.is_empty() {
        return None;
    }
    if meta {
        let base = match event.key_without_modifiers() {
            Key::Character(c) => c.to_string(),
            _ => text,
        };
        let mut out = vec![0x1b];
        out.extend_from_slice(base.as_bytes());
        return Some(out);
    }
    Some(text.into_bytes())
}

fn ctrl_code(c: char) -> Option<u8> {
    Some(match c.to_ascii_lowercase() {
        c @ 'a'..='z' => c as u8 - b'a' + 1,
        '@' | '2' | ' ' => 0,
        '[' | '3' => 0x1b,
        '\\' | '4' => 0x1c,
        ']' | '5' => 0x1d,
        '^' | '6' => 0x1e,
        '_' | '-' | '7' | '/' => 0x1f,
        '8' | '?' => 0x7f,
        _ => return None,
    })
}

/// Paste text the way shells expect it (CR line endings, bracketed if asked for).
pub fn paste_bytes(text: &str, bracketed: bool) -> Vec<u8> {
    let normalized = text.replace("\r\n", "\r").replace('\n', "\r");
    if bracketed {
        let clean: String = normalized.chars().filter(|&c| c != '\x1b').collect();
        let mut out = b"\x1b[200~".to_vec();
        out.extend_from_slice(clean.as_bytes());
        out.extend_from_slice(b"\x1b[201~");
        out
    } else {
        normalized.into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ctrl_codes() {
        assert_eq!(ctrl_code('c'), Some(3));
        assert_eq!(ctrl_code('C'), Some(3));
        assert_eq!(ctrl_code('['), Some(0x1b));
        assert_eq!(ctrl_code('1'), None);
    }

    #[test]
    fn paste_wraps_and_normalizes() {
        assert_eq!(paste_bytes("a\nb", false), b"a\rb");
        assert_eq!(paste_bytes("x\x1b", true), b"\x1b[200~x\x1b[201~");
    }
}
