//! Flow's keys: every shortcut in one table ([`TABLE`]), each with its
//! default on the Mac and elsewhere, as `config.toml`'s `[keys]` rebinds
//! them ([`Bindings`]). Everything that names or takes a key reads it:
//! `App::on_key`, the Mac's menus, the palette, help's pages and the words
//! that say a key. This is not the Hotkey (`hotkey.rs`), the one key taken
//! from the whole system, which goes by where a key is; these go by the
//! letter it types, so a binding follows the keyboard's layout as the menus
//! do (⌘? is ⇧⌘ß on a German keyboard).
//!
//! A key is a [`Chord`]: a letter, digit, symbol or named key with the
//! modifiers held for it, written in the config as the hotkey is
//! (`cmd+t`, `ctrl+shift+t`, `shift+f3`, `cmd+plus`) and shown as each
//! system writes it (⇧⌘T, Ctrl+Shift+T). Two entries are families whose
//! keys are fixed and whose modifiers are bound: going to tab 1 to 9 (a
//! digit) and moving between panes (an arrow).

use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};
use winit::event::KeyEvent;
use winit::keyboard::{Key as WinitKey, ModifiersState, NamedKey};
use winit::platform::modifier_supplement::KeyEventExtModifierSupplement;

use crate::event::Command;
use crate::i18n::{t, tf};

/// Whether this is the Mac, whose keys are written and held otherwise.
pub const MAC: bool = cfg!(target_os = "macos");

/// The modifiers held for a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, PartialOrd, Ord)]
pub struct Mods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    /// ⌘ on the Mac, the Windows key on Windows, Super on Linux.
    pub cmd: bool,
}

/// A key without its modifiers: what it types (a lowercase letter, a
/// digit, a symbol) or what it's named.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Key {
    Char(char),
    Named(Named),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Named {
    Tab,
    Space,
    Enter,
    Backspace,
    Delete,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,
    Left,
    Right,
    Up,
    Down,
    Escape,
    /// F1 to F24.
    F(u8),
}

/// A key with the modifiers held for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Chord {
    pub key: Key,
    pub mods: Mods,
}

/// The named keys as the config writes them, in `Named`'s order, F keys
/// apart.
const NAMES: [(Named, &str); 15] = [
    (Named::Tab, "tab"),
    (Named::Space, "space"),
    (Named::Enter, "enter"),
    (Named::Backspace, "backspace"),
    (Named::Delete, "delete"),
    (Named::Insert, "insert"),
    (Named::Home, "home"),
    (Named::End, "end"),
    (Named::PageUp, "pageup"),
    (Named::PageDown, "pagedown"),
    (Named::Left, "left"),
    (Named::Right, "right"),
    (Named::Up, "up"),
    (Named::Down, "down"),
    (Named::Escape, "escape"),
];

impl Mods {
    pub const NONE: Mods = Mods { ctrl: false, alt: false, shift: false, cmd: false };

    /// Modifiers as the config writes them, joined by `+` ("cmd+alt",
    /// "ctrl+shift"), as the hotkey reads them; "" is none. None for a
    /// word that isn't one.
    pub fn parse(text: &str) -> Option<Mods> {
        let mut mods = Mods::NONE;
        let text = text.trim();
        if text.is_empty() {
            return Some(mods);
        }
        for part in text.split('+') {
            *mods.slot(crate::hotkey::modifier_named(part.trim())?) = true;
        }
        Some(mods)
    }

    fn slot(&mut self, i: usize) -> &mut bool {
        match i {
            0 => &mut self.ctrl,
            1 => &mut self.alt,
            2 => &mut self.shift,
            _ => &mut self.cmd,
        }
    }

    /// As the config writes them: "ctrl+alt+shift+cmd" in that order, as
    /// the hotkey's `to_config` does.
    pub fn to_config(&self) -> String {
        let held = [(self.ctrl, "ctrl"), (self.alt, "alt"), (self.shift, "shift"), (self.cmd, "cmd")];
        held.iter().filter(|(on, _)| *on).map(|(_, m)| *m).collect::<Vec<_>>().join("+")
    }

    /// As the Mac writes them (⌃⌥⇧⌘, nothing between), or Windows and
    /// Linux ("Ctrl+Alt+Shift+", each followed by its `+`), as the hotkey's
    /// `label_for` does.
    pub fn label(&self, mac: bool) -> String {
        let held = [(self.ctrl, "⌃", "Ctrl+"), (self.alt, "⌥", "Alt+"), (self.shift, "⇧", "Shift+"), (self.cmd, "⌘", if cfg!(windows) { "Win+" } else { "Super+" })];
        held.iter().filter(|(on, ..)| *on).map(|&(_, m, other)| if mac { m } else { other }).collect()
    }

    /// Whether any but Shift is held: what a shortcut needs, so it doesn't
    /// take what's typed.
    pub fn any_but_shift(&self) -> bool {
        self.ctrl || self.alt || self.cmd
    }

    fn of(state: ModifiersState) -> Mods {
        Mods { ctrl: state.control_key(), alt: state.alt_key(), shift: state.shift_key(), cmd: state.super_key() }
    }
}

impl Key {
    /// A key's name in the config, lowercase already: a named key
    /// ("pageup", "f3"), "plus" for +, or the one character it types.
    fn named(name: &str) -> Option<Key> {
        if let Some(&(named, _)) = NAMES.iter().find(|(_, n)| *n == name) {
            return Some(Key::Named(named));
        }
        let alias = match name {
            "return" => Some(Named::Enter),
            "del" => Some(Named::Delete),
            "ins" => Some(Named::Insert),
            "pgup" => Some(Named::PageUp),
            "pgdn" => Some(Named::PageDown),
            "esc" => Some(Named::Escape),
            _ => None,
        };
        if let Some(named) = alias {
            return Some(Key::Named(named));
        }
        if let Some(n) = name.strip_prefix('f').and_then(|n| n.parse::<u8>().ok()) {
            return (1..=24).contains(&n).then_some(Key::Named(Named::F(n)));
        }
        match name {
            "plus" => return Some(Key::Char('+')),
            "minus" => return Some(Key::Char('-')),
            "grave" | "backquote" | "backtick" => return Some(Key::Char('`')),
            _ => {}
        }
        let mut chars = name.chars();
        match (chars.next(), chars.next()) {
            (Some(c), None) if !c.is_whitespace() && !c.is_control() => Some(Key::Char(c)),
            _ => None,
        }
    }

    fn to_config(self) -> String {
        match self {
            Key::Char('+') => "plus".into(),
            Key::Char(c) => c.to_string(),
            Key::Named(Named::F(n)) => format!("f{n}"),
            Key::Named(named) => NAMES.iter().find(|(n, _)| *n == named).map_or("", |(_, name)| name).into(),
        }
    }

    /// As people write the key itself: a letter as a capital, Space,
    /// Enter, ⌫, the arrows.
    pub fn label(self, mac: bool) -> String {
        match self {
            Key::Char(c) => c.to_uppercase().collect(),
            Key::Named(named) => match named {
                Named::Tab => "Tab".into(),
                Named::Space => "Space".into(),
                Named::Enter => "Enter".into(),
                Named::Backspace => if mac { "⌫" } else { "Backspace" }.into(),
                Named::Delete => if mac { "⌦" } else { "Delete" }.into(),
                Named::Insert => "Insert".into(),
                Named::Home => "Home".into(),
                Named::End => "End".into(),
                Named::PageUp => "Page Up".into(),
                Named::PageDown => "Page Down".into(),
                Named::Left => "←".into(),
                Named::Right => "→".into(),
                Named::Up => "↑".into(),
                Named::Down => "↓".into(),
                Named::Escape => "Esc".into(),
                Named::F(n) => format!("F{n}"),
            },
        }
    }

    /// The key winit names, where it's one a shortcut may be.
    fn of_named(key: &NamedKey) -> Option<Key> {
        const F_KEYS: [NamedKey; 24] = [
            NamedKey::F1, NamedKey::F2, NamedKey::F3, NamedKey::F4, NamedKey::F5, NamedKey::F6, NamedKey::F7, NamedKey::F8,
            NamedKey::F9, NamedKey::F10, NamedKey::F11, NamedKey::F12, NamedKey::F13, NamedKey::F14, NamedKey::F15, NamedKey::F16,
            NamedKey::F17, NamedKey::F18, NamedKey::F19, NamedKey::F20, NamedKey::F21, NamedKey::F22, NamedKey::F23, NamedKey::F24,
        ];
        Some(Key::Named(match key {
            NamedKey::Tab => Named::Tab,
            NamedKey::Space => Named::Space,
            NamedKey::Enter => Named::Enter,
            NamedKey::Backspace => Named::Backspace,
            NamedKey::Delete => Named::Delete,
            NamedKey::Insert => Named::Insert,
            NamedKey::Home => Named::Home,
            NamedKey::End => Named::End,
            NamedKey::PageUp => Named::PageUp,
            NamedKey::PageDown => Named::PageDown,
            NamedKey::ArrowLeft => Named::Left,
            NamedKey::ArrowRight => Named::Right,
            NamedKey::ArrowUp => Named::Up,
            NamedKey::ArrowDown => Named::Down,
            NamedKey::Escape => Named::Escape,
            other => Named::F(F_KEYS.iter().position(|f| f == other)? as u8 + 1),
        }))
    }

    fn of_text(text: &str) -> Option<Key> {
        let mut chars = text.chars();
        match (chars.next(), chars.next()) {
            (Some(' '), None) => Some(Key::Named(Named::Space)),
            (Some(c), None) if !c.is_control() => c.to_lowercase().next().map(Key::Char),
            _ => None,
        }
    }
}

impl Chord {
    pub const fn new(key: Key, mods: Mods) -> Chord {
        Chord { key, mods }
    }

    /// `text` as the config writes it: modifiers and then a key, joined by
    /// `+`, in any case ("cmd+t", "Ctrl+Shift+T", "shift+f3", "cmd+plus",
    /// "cmd++"). None for a key Flow doesn't know.
    pub fn parse(text: &str) -> Option<Chord> {
        let text = text.trim().to_lowercase();
        let (mods, key) = if text == "+" {
            ("", "+")
        } else if let Some(mods) = text.strip_suffix("++") {
            (mods, "+")
        } else {
            text.rsplit_once('+').unwrap_or(("", text.as_str()))
        };
        Some(Chord { key: Key::named(key.trim())?, mods: Mods::parse(mods)? })
    }

    /// As the config writes it, which `parse` reads back as this.
    pub fn to_config(&self) -> String {
        let mods = self.mods.to_config();
        let key = self.key.to_config();
        if mods.is_empty() { key } else { format!("{mods}+{key}") }
    }

    /// As people on this system write it: ⇧⌘T, ⌃Space, ⌘End on the Mac,
    /// Ctrl+Shift+T, Shift+F3, Ctrl+= elsewhere.
    pub fn label(&self, mac: bool) -> String {
        self.mods.label(mac) + &self.key.label(mac)
    }

    /// The hotkey as a chord, to keep a binding off it.
    pub fn of_hotkey(hotkey: &crate::hotkey::Hotkey) -> Chord {
        use crate::hotkey::Key as H;
        let key = match hotkey.key {
            H::Grave => Key::Char('`'),
            H::Space => Key::Named(Named::Space),
            H::F(n) => Key::Named(Named::F(n)),
            H::Letter(c) => Key::Char(c),
            H::Digit(d) => Key::Char((b'0' + d) as char),
        };
        Chord { key, mods: Mods { ctrl: hotkey.ctrl, alt: hotkey.alt, shift: hotkey.shift, cmd: hotkey.cmd } }
    }
}

/// A key as it was pressed, which a binding is matched against: the key as
/// it is without modifiers (`key_without_modifiers`, so the letter the
/// layout puts there), what it typed (Shift's symbol, `+` over `=`), and
/// the modifiers. A plain struct, so the matching is tested without winit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pressed {
    pub base: Option<Key>,
    pub typed: Option<char>,
    pub mods: Mods,
}

impl Pressed {
    pub fn of(event: &KeyEvent, state: ModifiersState) -> Pressed {
        let base = match (event.key_without_modifiers(), &event.logical_key) {
            (_, WinitKey::Named(named)) => Key::of_named(named),
            (WinitKey::Character(c), _) => Key::of_text(&c),
            (WinitKey::Named(named), _) => Key::of_named(&named),
            _ => None,
        };
        let typed = match &event.logical_key {
            WinitKey::Character(c) => {
                let mut chars = c.chars();
                chars.next().filter(|_| chars.next().is_none())
            }
            _ => None,
        };
        Pressed { base, typed, mods: Mods::of(state) }
    }

    /// The chords this press is: the key with its modifiers exactly, and
    /// where Shift made it another symbol (`+` over `=`, `?` over `/`),
    /// that symbol with and without Shift, so `cmd+plus` is ⇧⌘= on a US
    /// keyboard and ⌘+ on a German one. Not under ⌥, which types the
    /// Mac's other characters.
    fn chords(&self) -> [Option<Chord>; 3] {
        let base = self.base.map(|key| Chord { key, mods: self.mods });
        let symbol = self.symbol();
        let without = symbol.map(|c| Chord { key: Key::Char(c), mods: Mods { shift: false, ..self.mods } });
        let with = symbol.map(|c| Chord { key: Key::Char(c), mods: self.mods });
        [base, without, with]
    }

    /// The symbol Shift made of the key, where it made another one.
    fn symbol(&self) -> Option<char> {
        let typed = self.typed?.to_lowercase().next()?;
        let base = match self.base? {
            Key::Char(c) => c,
            Key::Named(_) => return None,
        };
        (self.mods.shift && !self.mods.alt && typed != base && !typed.is_alphanumeric()).then_some(typed)
    }

    /// The chord to bind this press to: the key with its modifiers, or for
    /// a symbol key Shift turned into another symbol, that symbol, as the
    /// Mac writes it without its ⇧ (⌘?, ⌘+) and elsewhere with it
    /// (Ctrl+Shift+?). A letter or a digit is itself, Shift and all
    /// (Ctrl+Shift+1, not Ctrl+Shift+!).
    pub fn recorded(&self, mac: bool) -> Option<Chord> {
        let base = self.base?;
        let symbol_key = matches!(base, Key::Char(c) if !c.is_alphanumeric());
        match self.symbol() {
            Some(c) if symbol_key => Some(Chord { key: Key::Char(c), mods: Mods { shift: !mac, ..self.mods } }),
            _ => Some(Chord { key: base, mods: self.mods }),
        }
    }

    /// No modifier held, Shift included.
    pub fn plain(&self) -> bool {
        self.mods == Mods::NONE
    }
}

/// What a binding does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Does {
    Command(Command),
    /// Going to tab 1 to 9: the digit with the modifiers bound.
    Tabs,
    /// Moving to the pane beside: an arrow with the modifiers bound.
    Panes,
}

/// What a key pressed asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Action {
    Command(Command),
    /// The tab at this place, from 0.
    Tab(usize),
    Pane(crate::panes::Dir),
}

/// Where an entry is listed, in Settings' Keys and help's page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Tabs,
    Text,
    Find,
    Assistant,
    Flow,
}

impl Group {
    pub const ALL: [Group; 5] = [Group::Tabs, Group::Text, Group::Find, Group::Assistant, Group::Flow];

    pub fn words(self) -> &'static str {
        t(match self {
            Group::Tabs => "keys.group.tabs",
            Group::Text => "keys.group.text",
            Group::Find => "keys.group.find",
            Group::Assistant => "keys.group.assistant",
            Group::Flow => "keys.group.flow",
        })
    }
}

/// One of Flow's keys: its name in `[keys]`, what it does, its words, its
/// group, and its keys by default on the Mac and elsewhere, as the config
/// writes them (a family's modifiers alone).
#[derive(Debug, Clone, Copy)]
pub struct Entry {
    pub name: &'static str,
    pub does: Does,
    pub words: &'static str,
    pub group: Group,
    pub mac: &'static [&'static str],
    pub other: &'static [&'static str],
}

impl Entry {
    /// Its words in the language in use, the assistant named by the
    /// provider in use (`provider::in_use`).
    pub fn label(&self) -> String {
        let ai = crate::ai::provider::in_use().name();
        tf(self.words, &[("app", &crate::APP_NAME as &dyn std::fmt::Display), ("ai", &ai)])
    }

    pub fn command(&self) -> Option<Command> {
        match self.does {
            Does::Command(cmd) => Some(cmd),
            _ => None,
        }
    }

    fn defaults(&self, mac: bool) -> Keys {
        let written = if mac { self.mac } else { self.other };
        match self.does {
            Does::Command(_) => Keys::Chords(written.iter().filter_map(|w| Chord::parse(w)).collect()),
            Does::Tabs | Does::Panes => Keys::Held(written.first().and_then(|w| Mods::parse(w))),
        }
    }
}

const fn cmd(name: &'static str, cmd: Command, words: &'static str, group: Group, mac: &'static [&'static str], other: &'static [&'static str]) -> Entry {
    Entry { name, does: Does::Command(cmd), words, group, mac, other }
}

/// Recording's key elsewhere: Ctrl+Alt+Shift and a letter, since
/// Ctrl+Shift+R is the history's (as ⌘R is on the Mac): R on Windows, V
/// on Linux, where GNOME Shell keeps Ctrl+Alt+Shift+R for its own screen
/// recorder.
const RECORD_ELSEWHERE: &[&str] = if cfg!(windows) { &["ctrl+alt+shift+r"] } else { &["ctrl+alt+shift+v"] };

/// Every key Flow has, in the order Settings and help list them. The
/// defaults are the keys Flow had before they could be changed, which a
/// test holds them to.
pub const TABLE: [Entry; 45] = [
    cmd("new_tab", Command::NewTab, "menu.new_tab", Group::Tabs, &["cmd+t"], &["ctrl+shift+t"]),
    cmd("close_tab", Command::CloseTab, "menu.close_tab", Group::Tabs, &["cmd+w"], &["ctrl+shift+w"]),
    cmd("next_tab", Command::NextTab, "menu.next_tab", Group::Tabs, &["cmd+shift+]", "ctrl+tab"], &["ctrl+tab"]),
    cmd("previous_tab", Command::PreviousTab, "menu.previous_tab", Group::Tabs, &["cmd+shift+[", "ctrl+shift+tab"], &["ctrl+shift+tab"]),
    Entry { name: "go_to_tab", does: Does::Tabs, words: "keys.go_to_tab", group: Group::Tabs, mac: &["cmd"], other: &["ctrl+shift"] },
    cmd("restart_tab", Command::RestartTab, "menu.restart_tab", Group::Tabs, &["cmd+shift+b"], &["ctrl+shift+b"]),
    cmd("rename_tab", Command::RenameTab, "menu.rename_tab", Group::Tabs, &[], &[]),
    cmd("new_group", Command::NewGroup, "menu.new_group", Group::Tabs, &[], &[]),
    cmd("tab_sidebar", Command::TabSidebar, "menu.tab_sidebar", Group::Tabs, &["cmd+shift+s"], &["ctrl+shift+s"]),
    cmd("split_right", Command::SplitRight, "menu.split_right", Group::Tabs, &["cmd+d"], &["ctrl+shift+d"]),
    cmd("split_down", Command::SplitDown, "menu.split_down", Group::Tabs, &["cmd+shift+d"], &["ctrl+alt+shift+d"]),
    Entry { name: "move_between_panes", does: Does::Panes, words: "keys.move_between_panes", group: Group::Tabs, mac: &["cmd+alt"], other: &["ctrl+alt"] },
    cmd("copy", Command::Copy, "menu.copy", Group::Text, &["cmd+c"], &["ctrl+shift+c"]),
    cmd("paste", Command::Paste, "menu.paste", Group::Text, &["cmd+v"], &["ctrl+shift+v"]),
    cmd("select_all", Command::SelectAll, "menu.select_all", Group::Text, &["cmd+a"], &[]),
    cmd("clear_buffer", Command::ClearBuffer, "menu.clear_buffer", Group::Text, &["cmd+k"], &[]),
    cmd("scroll_to_bottom", Command::ScrollToBottom, "menu.scroll_to_bottom", Group::Text, &["cmd+end"], &["ctrl+shift+end"]),
    cmd("scroll_to_top", Command::ScrollToTop, "menu.scroll_to_top", Group::Text, &["cmd+home"], &["ctrl+shift+home"]),
    cmd("previous_command", Command::PreviousCommand, "menu.previous_command", Group::Text, &["cmd+up"], &["ctrl+shift+up"]),
    cmd("next_command", Command::NextCommand, "menu.next_command", Group::Text, &["cmd+down"], &["ctrl+shift+down"]),
    cmd("bigger_text", Command::BiggerText, "menu.bigger", Group::Text, &["cmd+plus", "cmd+="], &["ctrl+=", "ctrl+plus"]),
    cmd("smaller_text", Command::SmallerText, "menu.smaller", Group::Text, &["cmd+-"], &["ctrl+-"]),
    cmd("actual_size", Command::ActualSize, "menu.actual_size", Group::Text, &["cmd+0"], &["ctrl+0"]),
    cmd("find", Command::Find, "menu.find", Group::Find, &["cmd+f"], &["ctrl+shift+f"]),
    cmd("find_next", Command::FindNext, "menu.find_next", Group::Find, &["cmd+g"], &["f3"]),
    cmd("find_previous", Command::FindPrevious, "menu.find_previous", Group::Find, &["cmd+shift+g"], &["shift+f3"]),
    cmd("search_history", Command::SearchHistory, "menu.search_history", Group::Find, &["cmd+r"], &["ctrl+shift+r"]),
    cmd("palette", Command::Palette, "menu.palette", Group::Find, &["cmd+shift+p"], &["ctrl+shift+p"]),
    cmd("assistant", Command::ToggleAssistant, "menu.ask", Group::Assistant, &["ctrl+space"], &["ctrl+space"]),
    cmd("new_conversation", Command::NewConversation, "menu.new_conversation", Group::Assistant, &[], &[]),
    cmd("ask_why", Command::AskWhy, "menu.ask_why", Group::Assistant, &[], &[]),
    cmd("guide_mode", Command::GuideMode, "menu.guide_mode", Group::Assistant, &[], &[]),
    cmd("auto_mode", Command::AutoMode, "menu.auto_mode", Group::Assistant, &[], &[]),
    cmd("change_key", Command::ChangeKey, "menu.change_key", Group::Assistant, &["cmd+shift+k"], &["ctrl+shift+k"]),
    cmd("forget_key", Command::ForgetKey, "menu.remove_key", Group::Assistant, &[], &[]),
    cmd("settings", Command::Settings, "menu.settings", Group::Flow, &["cmd+,"], &["ctrl+,"]),
    cmd("help", Command::Help, "menu.help_item", Group::Flow, &["cmd+?"], &["ctrl+shift+?"]),
    cmd("efficient", Command::ToggleEfficient, "menu.efficient", Group::Flow, &["cmd+shift+e"], &["ctrl+shift+e"]),
    cmd("on_top", Command::ToggleOnTop, "menu.on_top", Group::Flow, &[], &[]),
    cmd("record", Command::Record, "menu.record", Group::Flow, &["cmd+shift+r"], RECORD_ELSEWHERE),
    cmd("save_theme", Command::SaveTheme, "menu.save_theme", Group::Flow, &[], &[]),
    cmd("about", Command::About, "menu.about", Group::Flow, &[], &[]),
    cmd("check_updates", Command::CheckForUpdates, "menu.check_updates", Group::Flow, &[], &[]),
    cmd("restart", Command::Restart, "menu.restart", Group::Flow, &[], &[]),
    cmd("quit", Command::Quit, "menu.quit", Group::Flow, &["cmd+q"], &[]),
];

/// Quit's key on the Mac is the system's, the same in every app: it's
/// listed, but neither rebound nor taken. Elsewhere Quit has none, and may
/// be given one.
pub fn fixed(i: usize, mac: bool) -> bool {
    mac && TABLE.get(i).is_some_and(|e| e.does == Does::Command(Command::Quit))
}

/// The keys an entry has: a command's chords (none, one or a few), or a
/// family's modifiers (none: switched off).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Keys {
    Chords(Vec<Chord>),
    Held(Option<Mods>),
}

impl Keys {
    /// None, for what `does`: a command's no chords, a family off.
    pub fn none(does: Does) -> Keys {
        match does {
            Does::Command(_) => Keys::Chords(Vec::new()),
            Does::Tabs | Does::Panes => Keys::Held(None),
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            Keys::Chords(chords) => chords.is_empty(),
            Keys::Held(held) => held.is_none(),
        }
    }
}

/// An entry's keys in `[keys]`: one ("cmd+n", "none", a family's
/// modifiers "ctrl"), or a few. Anything else is kept as it was written
/// and passed over, so a config Flow can't read a key of still reads.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum KeySetting {
    One(String),
    Many(Vec<String>),
    Other(toml::Value),
}

/// Every entry's keys: the defaults, with `[keys]`' changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bindings {
    keys: Vec<Keys>,
    mac: bool,
}

/// What a family's keys are, with their modifiers.
fn expand(does: Does, mods: Mods) -> Vec<Chord> {
    match does {
        Does::Tabs => ('1'..='9').map(|c| Chord { key: Key::Char(c), mods }).collect(),
        Does::Panes => [Named::Left, Named::Right, Named::Up, Named::Down].into_iter().map(|n| Chord { key: Key::Named(n), mods }).collect(),
        Does::Command(_) => Vec::new(),
    }
}

/// Why a key can't be bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Why {
    /// No ⌃, ⌥ or ⌘ (bar an F key): it's typed.
    Typing,
    /// F1 alone: help for the screen in front.
    Help,
    /// The system's (⌘Q, ⌘Tab, Alt+F4, the Windows key's).
    System,
    /// Settings' Hotkey, which the system takes first.
    Hotkey,
    /// A family takes its modifiers from a digit or an arrow.
    Family,
}

/// What a key pressed for an entry would do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Offer {
    Refused(Why),
    /// Another entry has it: swap them (two commands only), or take it.
    Taken { other: usize, swap: bool },
    /// It can be bound; `shells` where the shell and its programs would
    /// otherwise get it.
    Fine { shells: bool },
}

/// Whether Settings waits for a key to be an entry's in place of what it
/// has (Change), or one more beside them (Add Key, a command's alone).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Change,
    Add,
}

/// Settings waiting for a key for entry `entry` (its Keys section): for
/// the key, after one it refused, or asking what to do about one that's
/// another's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Wait {
    Waiting(usize, Mode),
    Refused(usize, Mode, Why),
    /// `keys`, pressed for `entry`, are `other`'s: swap them (`swap`, two
    /// commands), take them, or keep what `entry` has.
    Taken { entry: usize, keys: Keys, other: usize, swap: bool },
}

impl Wait {
    pub fn entry(&self) -> usize {
        match self {
            Wait::Waiting(i, _) | Wait::Refused(i, ..) => *i,
            Wait::Taken { entry, .. } => *entry,
        }
    }

    /// Whether it waits for a key to add (Add Key), not one to change to.
    pub fn mode(&self) -> Mode {
        match self {
            Wait::Waiting(_, mode) | Wait::Refused(_, mode, _) => *mode,
            Wait::Taken { .. } => Mode::Change,
        }
    }
}

impl Bindings {
    pub fn defaults(mac: bool) -> Bindings {
        Bindings { keys: TABLE.iter().map(|e| e.defaults(mac)).collect(), mac }
    }

    /// The defaults with `[keys]`' changes; a name or a key Flow can't read
    /// is logged and its default kept.
    pub fn with(changes: &BTreeMap<String, KeySetting>, mac: bool) -> Bindings {
        let mut bindings = Bindings::defaults(mac);
        for (name, setting) in changes {
            let Some(i) = TABLE.iter().position(|e| e.name == name) else {
                log::warn!("[keys] in the config names {name}, which isn't one of Flow's keys");
                continue;
            };
            if fixed(i, mac) {
                continue;
            }
            match read(TABLE[i].does, setting) {
                Some(keys) => bindings.keys[i] = keys,
                None => log::warn!("[keys] {name} = {setting:?} isn't a key Flow can read; it keeps its default"),
            }
        }
        bindings
    }

    pub fn mac(&self) -> bool {
        self.mac
    }

    pub fn get(&self, i: usize) -> &Keys {
        &self.keys[i]
    }

    pub fn index_of(cmd: Command) -> Option<usize> {
        TABLE.iter().position(|e| e.does == Does::Command(cmd))
    }

    /// A command's chords, the first the one shown; none where it has none.
    pub fn chords(&self, cmd: Command) -> &[Chord] {
        match Self::index_of(cmd).map(|i| &self.keys[i]) {
            Some(Keys::Chords(chords)) => chords,
            _ => &[],
        }
    }

    /// Every chord an entry takes: a command's, a family's nine or four.
    pub fn all_of(&self, i: usize) -> Vec<Chord> {
        match &self.keys[i] {
            Keys::Chords(chords) => chords.clone(),
            Keys::Held(Some(mods)) => expand(TABLE[i].does, *mods),
            Keys::Held(None) => Vec::new(),
        }
    }

    /// A command's key as it's said in words: its first, or "no key".
    pub fn label(&self, cmd: Command) -> String {
        self.chords(cmd).first().map_or_else(|| t("keys.none").to_string(), |c| c.label(self.mac))
    }

    /// An entry's keys as Settings and help list them: every chord, or a
    /// family's (⌘1…⌘9, ⌥⌘←→↑↓), or "no key".
    pub fn label_at(&self, i: usize) -> String {
        label_of(TABLE[i].does, &self.keys[i], self.mac)
    }

    /// A family's modifiers alone, as words put them before "and an arrow"
    /// (⌥⌘, Ctrl+Alt).
    pub fn held_label(&self, does: Does) -> String {
        match TABLE.iter().position(|e| e.does == does).map(|i| &self.keys[i]) {
            Some(Keys::Held(Some(mods))) => mods.label(self.mac).trim_end_matches('+').to_string(),
            _ => t("keys.none").to_string(),
        }
    }

    /// What `pressed` asks for, where it's one of the bound keys.
    pub(crate) fn action_for(&self, pressed: &Pressed) -> Option<Action> {
        let chords: Vec<Chord> = pressed.chords().into_iter().flatten().collect();
        for (i, keys) in self.keys.iter().enumerate() {
            let does = TABLE[i].does;
            match keys {
                Keys::Chords(bound) => {
                    if let Does::Command(cmd) = does {
                        if bound.iter().any(|b| chords.contains(b)) {
                            return Some(Action::Command(cmd));
                        }
                    }
                }
                Keys::Held(Some(mods)) => {
                    let Some(chord) = chords.first().filter(|c| c.mods == *mods) else { continue };
                    match (does, chord.key) {
                        (Does::Tabs, Key::Char(c @ '1'..='9')) => return Some(Action::Tab(c as usize - '1' as usize)),
                        (Does::Panes, Key::Named(n)) => {
                            use crate::panes::Dir;
                            let dir = match n {
                                Named::Left => Dir::Left,
                                Named::Right => Dir::Right,
                                Named::Up => Dir::Up,
                                Named::Down => Dir::Down,
                                _ => continue,
                            };
                            return Some(Action::Pane(dir));
                        }
                        _ => {}
                    }
                }
                Keys::Held(None) => {}
            }
        }
        None
    }

    /// The entry other than `except` that takes `chord`.
    pub fn holder(&self, chord: &Chord, except: usize) -> Option<usize> {
        (0..TABLE.len()).filter(|&i| i != except).find(|&i| self.all_of(i).contains(chord))
    }

    /// What `chord`, pressed for entry `i`, would bind it to, and whether it
    /// can: refused, another's, or fine. `hotkey` is Settings' Hotkey.
    pub fn offer(&self, i: usize, chord: Chord, hotkey: Option<Chord>) -> (Keys, Offer) {
        self.offer_as(i, chord, hotkey, Mode::Change)
    }

    /// As [`Bindings::offer`], with `Mode::Add` adding `chord` to the keys
    /// a command has (a family has one set of modifiers, and changes): one
    /// it has already is fine as it is, and one another has is taken, not
    /// swapped, as nothing of this one's is given up.
    pub fn offer_as(&self, i: usize, chord: Chord, hotkey: Option<Chord>, mode: Mode) -> (Keys, Offer) {
        let does = TABLE[i].does;
        let adding = mode == Mode::Add && matches!(does, Does::Command(_));
        let keys = match does {
            Does::Command(_) if adding => {
                let mut chords = self.all_of(i);
                if !chords.contains(&chord) {
                    chords.push(chord);
                }
                Keys::Chords(chords)
            }
            Does::Command(_) => Keys::Chords(vec![chord]),
            Does::Tabs | Does::Panes => Keys::Held(Some(chord.mods)),
        };
        let family_key = match does {
            Does::Tabs => matches!(chord.key, Key::Char('1'..='9')),
            Does::Panes => matches!(chord.key, Key::Named(Named::Left | Named::Right | Named::Up | Named::Down)),
            Does::Command(_) => true,
        };
        if !family_key {
            return (keys, Offer::Refused(Why::Family));
        }
        if let Some(why) = refused(&chord, self.mac, hotkey.as_ref()) {
            return (keys, Offer::Refused(why));
        }
        let chords = match &keys {
            Keys::Held(Some(mods)) => expand(does, *mods),
            _ => vec![chord],
        };
        for c in &chords {
            if let Some(other) = self.holder(c, i) {
                if fixed(other, self.mac) {
                    return (keys, Offer::Refused(Why::System));
                }
                let swap = !adding && matches!(does, Does::Command(_)) && matches!(TABLE[other].does, Does::Command(_));
                return (keys, Offer::Taken { other, swap });
            }
        }
        (keys, Offer::Fine { shells: shells(&chord, self.mac) })
    }

    /// Entry `i` takes `keys`.
    pub fn put(&mut self, i: usize, keys: Keys) {
        if !fixed(i, self.mac) {
            self.keys[i] = keys;
        }
    }

    /// Entry `other` gives up what `keys`, for entry `i`, takes of it: the
    /// chords, or a family switched off.
    pub fn take_from(&mut self, other: usize, i: usize, keys: &Keys) {
        let taken = match keys {
            Keys::Held(Some(mods)) => expand(TABLE[i].does, *mods),
            Keys::Chords(chords) => chords.clone(),
            Keys::Held(None) => Vec::new(),
        };
        match &mut self.keys[other] {
            Keys::Chords(chords) => chords.retain(|c| !taken.contains(c)),
            held @ Keys::Held(_) => *held = Keys::Held(None),
        }
    }

    /// Entry `i` takes `keys` and `other` takes what `i` had: two
    /// commands trade.
    pub fn swap(&mut self, i: usize, other: usize, keys: Keys) {
        let had = std::mem::replace(&mut self.keys[i], keys);
        self.keys[other] = had;
    }

    /// Whether entry `i` is as Flow has it.
    pub fn is_default(&self, i: usize) -> bool {
        self.keys[i] == TABLE[i].defaults(self.mac)
    }

    /// Whether every entry is.
    pub fn all_default(&self) -> bool {
        (0..TABLE.len()).all(|i| self.is_default(i))
    }

    /// Entry `i` back to its default, taking its keys from any other
    /// that has them since.
    pub fn reset(&mut self, i: usize) {
        let keys = TABLE[i].defaults(self.mac);
        let chords = match &keys {
            Keys::Held(Some(mods)) => expand(TABLE[i].does, *mods),
            Keys::Chords(chords) => chords.clone(),
            Keys::Held(None) => Vec::new(),
        };
        for c in &chords {
            if let Some(other) = self.holder(c, i) {
                self.take_from(other, i, &keys);
            }
        }
        self.keys[i] = keys;
    }

    /// The entries that differ from `other`'s, to write.
    pub fn changed_from(&self, other: &Bindings) -> Vec<usize> {
        (0..TABLE.len()).filter(|&i| self.keys[i] != other.keys[i]).collect()
    }

    /// Entry `i`'s keys by default.
    pub fn default_label(&self, i: usize) -> String {
        label_of(TABLE[i].does, &TABLE[i].defaults(self.mac), self.mac)
    }

    /// Writes entry `i` into `[keys]` as it now is: taken away where it's
    /// as Flow has it, so only what's changed is kept.
    pub fn write(&self, i: usize, changes: &mut BTreeMap<String, KeySetting>) {
        let name = TABLE[i].name.to_string();
        if self.is_default(i) {
            changes.remove(&name);
            return;
        }
        let setting = match &self.keys[i] {
            Keys::Chords(chords) if chords.is_empty() => KeySetting::One("none".into()),
            Keys::Chords(chords) if chords.len() == 1 => KeySetting::One(chords[0].to_config()),
            Keys::Chords(chords) => KeySetting::Many(chords.iter().map(Chord::to_config).collect()),
            Keys::Held(Some(mods)) => KeySetting::One(mods.to_config()),
            Keys::Held(None) => KeySetting::One("none".into()),
        };
        changes.insert(name, setting);
    }

    /// `[keys]` as config.toml holds it, read alone (`flow-help`, which
    /// reads the config without being Flow): the defaults where there's
    /// none or it doesn't read.
    pub fn from_config_text(text: &str, mac: bool) -> Bindings {
        #[derive(Deserialize, Default)]
        struct OnlyKeys {
            #[serde(default)]
            keys: BTreeMap<String, KeySetting>,
        }
        let only: OnlyKeys = toml::from_str(text).unwrap_or_default();
        Bindings::with(&only.keys, mac)
    }
}

fn label_of(does: Does, keys: &Keys, mac: bool) -> String {
    match (does, keys) {
        (_, Keys::Chords(chords)) if !chords.is_empty() => chords.iter().map(|c| c.label(mac)).collect::<Vec<_>>().join(", "),
        (Does::Tabs, Keys::Held(Some(mods))) => {
            let held = mods.label(mac);
            if mac { format!("{held}1…{held}9") } else { format!("{held}1…9") }
        }
        (Does::Panes, Keys::Held(Some(mods))) => format!("{}←→↑↓", mods.label(mac)),
        _ => t("keys.none").to_string(),
    }
}

/// An entry's keys as `[keys]` writes them.
fn read(does: Does, setting: &KeySetting) -> Option<Keys> {
    let written: Vec<&str> = match setting {
        KeySetting::One(one) => vec![one.as_str()],
        KeySetting::Many(many) => many.iter().map(String::as_str).collect(),
        KeySetting::Other(_) => return None,
    };
    let none = |w: &str| matches!(w.trim().to_lowercase().as_str(), "" | "none");
    match does {
        Does::Command(_) => {
            let chords: Option<Vec<Chord>> = written.iter().filter(|w| !none(w)).map(|w| Chord::parse(w)).collect();
            Some(Keys::Chords(chords?))
        }
        Does::Tabs | Does::Panes => match written.as_slice() {
            [w] if none(w) => Some(Keys::Held(None)),
            [w] => Mods::parse(w).filter(Mods::any_but_shift).map(|m| Keys::Held(Some(m))),
            _ => None,
        },
    }
}

/// Why `chord` can't be a shortcut, if it can't: typed (no ⌃, ⌥ or ⌘,
/// bar an F key from F2), F1 alone (help's), the system's, or the Hotkey.
pub fn refused(chord: &Chord, mac: bool, hotkey: Option<&Chord>) -> Option<Why> {
    if chord.key == Key::Named(Named::F(1)) && !chord.mods.any_but_shift() {
        return Some(Why::Help);
    }
    let f_key = matches!(chord.key, Key::Named(Named::F(_)));
    if !chord.mods.any_but_shift() && !f_key {
        return Some(Why::Typing);
    }
    if hotkey == Some(chord) {
        return Some(Why::Hotkey);
    }
    let system: &[&str] = if mac {
        &[
            "cmd+q", "cmd+h", "alt+cmd+h", "cmd+m", "cmd+tab", "shift+cmd+tab", "cmd+`", "shift+cmd+`", "cmd+space", "ctrl+cmd+space",
            "alt+cmd+space", "shift+cmd+3", "shift+cmd+4", "shift+cmd+5", "ctrl+shift+cmd+3", "ctrl+shift+cmd+4", "ctrl+cmd+q",
            "alt+cmd+escape", "ctrl+cmd+f", "shift+cmd+w",
        ]
    } else {
        &["alt+f4", "alt+tab", "alt+shift+tab", "ctrl+alt+delete", "ctrl+escape", "ctrl+shift+escape", "alt+space"]
    };
    // Off the Mac the Windows key and Super are the system's, whatever
    // they're held with.
    if (!mac && chord.mods.cmd) || system.iter().filter_map(|s| Chord::parse(s)).any(|s| s == *chord) {
        return Some(Why::System);
    }
    None
}

/// Whether the shell and its programs would get `chord` if Flow didn't
/// take it: Ctrl and a letter (^C, ^L), or ⌥ (Alt) and one, which
/// readline and the Mac's keyboard read. Flow's own keys elsewhere hold
/// Ctrl+Shift to leave these alone.
pub fn shells(chord: &Chord, mac: bool) -> bool {
    let Key::Char(_) = chord.key else { return false };
    let Mods { ctrl, alt, shift, cmd } = chord.mods;
    !cmd && if mac { ctrl || alt } else { (ctrl && !shift && !alt) || (alt && !ctrl) }
}

// What every frame and every thread reads: set by `App` from the config,
// per thread in tests so they don't change each other's.
static BOUND: RwLock<Option<Arc<Bindings>>> = RwLock::new(None);

#[cfg(test)]
thread_local! {
    static TEST_BOUND: std::cell::RefCell<Option<Arc<Bindings>>> = const { std::cell::RefCell::new(None) };
}

/// Makes `bindings` the keys in use.
pub fn set(bindings: Bindings) {
    #[cfg(test)]
    {
        TEST_BOUND.with(|b| *b.borrow_mut() = Some(Arc::new(bindings)));
    }
    #[cfg(not(test))]
    {
        *BOUND.write().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(bindings));
    }
}

/// The keys in use: as `set` last made them, else Flow's own.
pub fn bound() -> Arc<Bindings> {
    #[cfg(test)]
    if let Some(b) = TEST_BOUND.with(|b| b.borrow().clone()) {
        return b;
    }
    BOUND.read().unwrap_or_else(|e| e.into_inner()).clone().unwrap_or_else(|| Arc::new(Bindings::defaults(MAC)))
}

/// The keys as config.toml binds them, read alone and never written: for
/// `flow-help`, which isn't Flow but prints its keys as they're bound.
pub fn from_config_file() -> Bindings {
    let text = std::fs::read_to_string(crate::config::config_path()).unwrap_or_default();
    Bindings::from_config_text(&text, MAC)
}

/// A command's key in use as words put it.
pub fn label(cmd: Command) -> String {
    bound().label(cmd)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_assistants_words_name_the_provider_in_use() {
        use crate::ai::provider::{set_in_use, Provider};
        crate::i18n::set_language("en", &[]);
        let ask = TABLE.iter().find(|e| e.command() == Some(Command::ToggleAssistant)).unwrap();
        for (provider, words) in [(Provider::OpenAi, "Ask GPT"), (Provider::Gemini, "Ask Gemini"), (Provider::Anthropic, "Ask Claude")] {
            set_in_use(provider);
            assert_eq!(ask.label(), words);
        }
        crate::i18n::set_language("ja", &[]);
        set_in_use(Provider::OpenAi);
        assert_eq!(ask.label(), "GPTに質問");
        crate::i18n::set_language("en", &[]);
        set_in_use(Provider::Anthropic);
    }

    fn press(base: Key, typed: Option<char>, mods: &str) -> Pressed {
        Pressed { base: Some(base), typed, mods: Mods::parse(mods).unwrap() }
    }

    fn chord(text: &str) -> Chord {
        Chord::parse(text).unwrap_or_else(|| panic!("{text} doesn't parse"))
    }

    fn at(name: &str) -> usize {
        TABLE.iter().position(|e| e.name == name).unwrap()
    }

    #[test]
    fn keys_are_read_and_written_as_the_config_writes_them() {
        for text in ["cmd+t", "ctrl+shift+t", "shift+f3", "cmd+end", "cmd+plus", "ctrl+=", "cmd+?", "alt+cmd+left", "f24", "ctrl+alt+shift+cmd+pagedown", "ctrl+space", "cmd+,", "cmd+-"] {
            let c = chord(text);
            assert_eq!(Chord::parse(&c.to_config()), Some(c), "{text}");
        }
        assert_eq!(chord("Cmd++"), chord("cmd+plus"));
        assert_eq!(chord("Ctrl+Shift+T"), chord("ctrl+shift+t"));
        assert_eq!(chord("⌘+⇧+t"), chord("cmd+shift+t"));
        assert_eq!(chord("cmd+plus").to_config(), "cmd+plus");
        assert_eq!(chord("shift+ctrl+t").to_config(), "ctrl+shift+t");
        assert_eq!(chord("esc"), chord("escape"));
        for bad in ["", "cmd+", "hyper+t", "cmd+f25", "cmd+tt", "ctrl+shift"] {
            assert_eq!(Chord::parse(bad), None, "{bad}");
        }
        assert_eq!(Mods::parse("cmd+alt"), Some(Mods { alt: true, cmd: true, ..Mods::NONE }));
        assert_eq!(Mods::parse("ctrl+shift").unwrap().to_config(), "ctrl+shift");
    }

    #[test]
    fn keys_are_shown_as_each_system_writes_them() {
        assert_eq!(chord("cmd+shift+t").label(true), "⇧⌘T");
        assert_eq!(chord("ctrl+space").label(true), "⌃Space");
        assert_eq!(chord("cmd+end").label(true), "⌘End");
        assert_eq!(chord("cmd+backspace").label(true), "⌘⌫");
        assert_eq!(chord("ctrl+shift+t").label(false), "Ctrl+Shift+T");
        assert_eq!(chord("shift+f3").label(false), "Shift+F3");
        assert_eq!(chord("ctrl+=").label(false), "Ctrl+=");
        assert_eq!(chord("ctrl+alt+shift+d").label(false), "Ctrl+Alt+Shift+D");
    }

    /// The keys Flow had before they could be changed, as the palette and
    /// help wrote them, the Mac's then everywhere else's.
    #[test]
    fn the_defaults_are_the_keys_flow_had_on_each_system() {
        let record = if cfg!(windows) { "Ctrl+Alt+Shift+R" } else { "Ctrl+Alt+Shift+V" };
        let had: [(&str, &str, &str); 45] = [
            ("new_tab", "⌘T", "Ctrl+Shift+T"),
            ("close_tab", "⌘W", "Ctrl+Shift+W"),
            ("next_tab", "⇧⌘], ⌃Tab", "Ctrl+Tab"),
            ("previous_tab", "⇧⌘[, ⌃⇧Tab", "Ctrl+Shift+Tab"),
            ("go_to_tab", "⌘1…⌘9", "Ctrl+Shift+1…9"),
            ("restart_tab", "⇧⌘B", "Ctrl+Shift+B"),
            ("rename_tab", "", ""),
            ("new_group", "", ""),
            ("tab_sidebar", "⇧⌘S", "Ctrl+Shift+S"),
            ("split_right", "⌘D", "Ctrl+Shift+D"),
            ("split_down", "⇧⌘D", "Ctrl+Alt+Shift+D"),
            ("move_between_panes", "⌥⌘←→↑↓", "Ctrl+Alt+←→↑↓"),
            ("copy", "⌘C", "Ctrl+Shift+C"),
            ("paste", "⌘V", "Ctrl+Shift+V"),
            ("select_all", "⌘A", ""),
            ("clear_buffer", "⌘K", ""),
            ("scroll_to_bottom", "⌘End", "Ctrl+Shift+End"),
            ("scroll_to_top", "⌘Home", "Ctrl+Shift+Home"),
            ("previous_command", "⌘↑", "Ctrl+Shift+↑"),
            ("next_command", "⌘↓", "Ctrl+Shift+↓"),
            ("bigger_text", "⌘+, ⌘=", "Ctrl+=, Ctrl++"),
            ("smaller_text", "⌘-", "Ctrl+-"),
            ("actual_size", "⌘0", "Ctrl+0"),
            ("find", "⌘F", "Ctrl+Shift+F"),
            ("find_next", "⌘G", "F3"),
            ("find_previous", "⇧⌘G", "Shift+F3"),
            ("search_history", "⌘R", "Ctrl+Shift+R"),
            ("palette", "⇧⌘P", "Ctrl+Shift+P"),
            ("assistant", "⌃Space", "Ctrl+Space"),
            ("new_conversation", "", ""),
            ("ask_why", "", ""),
            ("guide_mode", "", ""),
            ("auto_mode", "", ""),
            ("change_key", "⇧⌘K", "Ctrl+Shift+K"),
            ("forget_key", "", ""),
            ("settings", "⌘,", "Ctrl+,"),
            ("help", "⌘?", "Ctrl+Shift+?"),
            ("efficient", "⇧⌘E", "Ctrl+Shift+E"),
            ("on_top", "", ""),
            ("record", "⇧⌘R", record),
            ("save_theme", "", ""),
            ("about", "", ""),
            ("check_updates", "", ""),
            ("restart", "", ""),
            ("quit", "⌘Q", ""),
        ];
        crate::i18n::set_language("en", &[]);
        let none = t("keys.none");
        for (mac, column) in [(true, 1), (false, 2)] {
            let bindings = Bindings::defaults(mac);
            for (i, row) in had.iter().enumerate() {
                assert_eq!(TABLE[i].name, row.0);
                let want = [row.1, row.2][column - 1];
                let want = if want.is_empty() { none } else { want };
                assert_eq!(bindings.label_at(i), want, "{} on the {}", row.0, if mac { "Mac" } else { "PC" });
            }
        }
    }

    #[test]
    fn no_two_keys_share_a_chord_on_either_system() {
        for mac in [true, false] {
            let bindings = Bindings::defaults(mac);
            let mut seen: BTreeMap<Chord, &str> = BTreeMap::new();
            for (i, entry) in TABLE.iter().enumerate() {
                for c in bindings.all_of(i) {
                    if let Some(other) = seen.insert(c, entry.name) {
                        panic!("{} is both {other}'s and {}'s on the {}", c.label(mac), entry.name, if mac { "Mac" } else { "PC" });
                    }
                    if !fixed(i, mac) {
                        assert_eq!(refused(&c, mac, None), None, "{}'s default {} is refused", entry.name, c.label(mac));
                    }
                }
            }
        }
    }

    #[test]
    fn a_key_matches_by_the_layout_as_the_menus_do() {
        let mac = Bindings::defaults(true);
        let pc = Bindings::defaults(false);
        let cmd = |c| Some(Action::Command(c));
        // Bigger text: ⇧⌘= on a US keyboard, ⌘+ on a German one.
        assert_eq!(mac.action_for(&press(Key::Char('='), Some('+'), "cmd+shift")), cmd(Command::BiggerText));
        assert_eq!(mac.action_for(&press(Key::Char('+'), Some('+'), "cmd")), cmd(Command::BiggerText));
        assert_eq!(mac.action_for(&press(Key::Char('='), Some('='), "cmd")), cmd(Command::BiggerText));
        assert_eq!(pc.action_for(&press(Key::Char('='), Some('+'), "ctrl+shift")), cmd(Command::BiggerText));
        // Help: ⇧⌘/ in the US, ⇧⌘ß in Germany, Ctrl+Shift+? elsewhere.
        assert_eq!(mac.action_for(&press(Key::Char('/'), Some('?'), "cmd+shift")), cmd(Command::Help));
        assert_eq!(mac.action_for(&press(Key::Char('ß'), Some('?'), "cmd+shift")), cmd(Command::Help));
        assert_eq!(pc.action_for(&press(Key::Char('/'), Some('?'), "ctrl+shift")), cmd(Command::Help));
        // Exact: ⇧⌘T isn't New Tab, nor ⌥⌘T.
        assert_eq!(mac.action_for(&press(Key::Char('t'), Some('T'), "cmd")), cmd(Command::NewTab));
        assert_eq!(mac.action_for(&press(Key::Char('t'), Some('T'), "cmd+shift")), None);
        assert_eq!(mac.action_for(&press(Key::Char('t'), Some('†'), "cmd+alt")), None);
        // The families.
        assert_eq!(pc.action_for(&press(Key::Char('1'), Some('!'), "ctrl+shift")), Some(Action::Tab(0)));
        assert_eq!(mac.action_for(&press(Key::Char('3'), Some('3'), "cmd")), Some(Action::Tab(2)));
        assert_eq!(mac.action_for(&press(Key::Char('0'), Some('0'), "cmd")), cmd(Command::ActualSize));
        assert_eq!(mac.action_for(&press(Key::Named(Named::Left), None, "cmd+alt")), Some(Action::Pane(crate::panes::Dir::Left)));
        assert_eq!(pc.action_for(&press(Key::Named(Named::Down), None, "ctrl+alt")), Some(Action::Pane(crate::panes::Dir::Down)));
        // Tabs on both, F3 elsewhere, the Mac's ⇧⌘] typing }.
        assert_eq!(mac.action_for(&press(Key::Named(Named::Tab), None, "ctrl")), cmd(Command::NextTab));
        assert_eq!(pc.action_for(&press(Key::Named(Named::Tab), None, "ctrl+shift")), cmd(Command::PreviousTab));
        assert_eq!(pc.action_for(&press(Key::Named(Named::F(3)), None, "")), cmd(Command::FindNext));
        assert_eq!(mac.action_for(&press(Key::Char(']'), Some('}'), "cmd+shift")), cmd(Command::NextTab));
        assert_eq!(mac.action_for(&press(Key::Named(Named::Space), Some(' '), "ctrl")), cmd(Command::ToggleAssistant));
        // A letter typed alone is nobody's.
        assert_eq!(pc.action_for(&press(Key::Char('t'), Some('t'), "")), None);
    }

    #[test]
    fn a_key_pressed_is_bound_as_each_system_writes_it() {
        assert_eq!(press(Key::Char('/'), Some('?'), "cmd+shift").recorded(true), Some(chord("cmd+?")));
        assert_eq!(press(Key::Char('/'), Some('?'), "ctrl+shift").recorded(false), Some(chord("ctrl+shift+?")));
        assert_eq!(press(Key::Char('1'), Some('!'), "ctrl+shift").recorded(false), Some(chord("ctrl+shift+1")));
        assert_eq!(press(Key::Char('t'), Some('T'), "cmd+shift").recorded(true), Some(chord("cmd+shift+t")));
        assert_eq!(press(Key::Named(Named::F(5)), None, "").recorded(false), Some(chord("f5")));
    }

    #[test]
    fn keys_that_are_typed_the_systems_or_the_hotkey_are_refused() {
        let mac = Bindings::defaults(true);
        let pc = Bindings::defaults(false);
        let new_tab = at("new_tab");
        let refused_on = |b: &Bindings, text: &str| match b.offer(new_tab, chord(text), None).1 {
            Offer::Refused(why) => Some(why),
            _ => None,
        };
        assert_eq!(refused_on(&mac, "t"), Some(Why::Typing));
        assert_eq!(refused_on(&mac, "shift+t"), Some(Why::Typing));
        assert_eq!(refused_on(&mac, "f1"), Some(Why::Help));
        assert_eq!(refused_on(&mac, "cmd+q"), Some(Why::System));
        assert_eq!(refused_on(&mac, "cmd+tab"), Some(Why::System));
        assert_eq!(refused_on(&pc, "alt+f4"), Some(Why::System));
        assert_eq!(refused_on(&pc, "cmd+e"), Some(Why::System));
        assert_eq!(refused_on(&mac, "f5"), None);
        assert_eq!(refused_on(&mac, "cmd+n"), None);
        let hotkey = chord("ctrl+`");
        assert_eq!(mac.offer(new_tab, hotkey, Some(hotkey)).1, Offer::Refused(Why::Hotkey));
        // A family takes a digit (or an arrow) with its modifiers.
        assert_eq!(mac.offer(at("go_to_tab"), chord("ctrl+x"), None).1, Offer::Refused(Why::Family));
        let (keys, offer) = mac.offer(at("go_to_tab"), chord("ctrl+4"), None);
        assert_eq!(keys, Keys::Held(Some(Mods { ctrl: true, ..Mods::NONE })));
        assert_eq!(offer, Offer::Fine { shells: true });
    }

    #[test]
    fn a_key_another_has_is_swapped_or_taken() {
        let mut b = Bindings::defaults(true);
        let (new_tab, find) = (at("new_tab"), at("find"));
        let (keys, offer) = b.offer(find, chord("cmd+t"), None);
        assert_eq!(offer, Offer::Taken { other: new_tab, swap: true });
        b.swap(find, new_tab, keys);
        assert_eq!(b.label_at(find), "⌘T");
        assert_eq!(b.label_at(new_tab), "⌘F");
        assert!(!b.is_default(find) && !b.is_default(new_tab));

        // Taking one of a command's two leaves it the other.
        let mut b = Bindings::defaults(true);
        let bigger = at("bigger_text");
        let (keys, offer) = b.offer(find, chord("cmd+="), None);
        assert_eq!(offer, Offer::Taken { other: bigger, swap: true });
        b.take_from(bigger, find, &keys);
        b.put(find, keys);
        assert_eq!(b.label_at(bigger), "⌘+");
        assert_eq!(b.label_at(find), "⌘=");

        // A family taken from is switched off; it can't be swapped.
        let mut b = Bindings::defaults(true);
        let (keys, offer) = b.offer(find, chord("cmd+5"), None);
        assert_eq!(offer, Offer::Taken { other: at("go_to_tab"), swap: false });
        b.take_from(at("go_to_tab"), find, &keys);
        assert_eq!(b.get(at("go_to_tab")), &Keys::Held(None));
        // ⌘Q is the system's even as a binding's.
        assert_eq!(b.offer(find, chord("cmd+q"), None).1, Offer::Refused(Why::System));
    }

    #[test]
    fn a_command_given_another_key_keeps_those_it_had() {
        let mut b = Bindings::defaults(true);
        let (new_tab, find) = (at("new_tab"), at("find"));
        // ⌘N beside ⌘T: both open a tab, the first the one the menus show.
        let (keys, offer) = b.offer_as(new_tab, chord("cmd+n"), None, Mode::Add);
        assert_eq!(offer, Offer::Fine { shells: false });
        b.put(new_tab, keys);
        assert_eq!(b.label_at(new_tab), "⌘T, ⌘N");
        assert_eq!(b.chords(Command::NewTab), [chord("cmd+t"), chord("cmd+n")]);
        // One it has already changes nothing.
        let (keys, offer) = b.offer_as(new_tab, chord("cmd+n"), None, Mode::Add);
        assert_eq!((keys, offer), (Keys::Chords(vec![chord("cmd+t"), chord("cmd+n")]), Offer::Fine { shells: false }));
        // Another's is taken, never swapped: nothing of New Tab's is given up.
        let (keys, offer) = b.offer_as(new_tab, chord("cmd+f"), None, Mode::Add);
        assert_eq!(offer, Offer::Taken { other: find, swap: false });
        b.take_from(find, new_tab, &keys);
        b.put(new_tab, keys);
        assert_eq!((b.label_at(new_tab).as_str(), b.chords(Command::Find)), ("⌘T, ⌘N, ⌘F", &[][..]));
        // Refused as a change is; a family changes its modifiers whatever
        // the mode.
        assert_eq!(b.offer_as(new_tab, chord("cmd+q"), None, Mode::Add).1, Offer::Refused(Why::System));
        let (keys, _) = b.offer_as(at("go_to_tab"), chord("ctrl+4"), None, Mode::Add);
        assert_eq!(keys, Keys::Held(Some(Mods { ctrl: true, ..Mods::NONE })));
        // Written as the list the config takes.
        let mut changes = BTreeMap::new();
        b.write(new_tab, &mut changes);
        assert_eq!(changes.get("new_tab"), Some(&KeySetting::Many(vec!["cmd+t".into(), "cmd+n".into(), "cmd+f".into()])));
    }

    #[test]
    fn the_hotkey_takes_a_commands_key_from_it_but_never_the_systems() {
        // As App::press_hotkey does: the system takes the hotkey before
        // Flow, so the command gives it up.
        let mut b = Bindings::defaults(true);
        let of = |text: &str| Chord::of_hotkey(&crate::hotkey::Hotkey::parse(text).unwrap());
        let t = of("cmd+t");
        let other = b.holder(&t, usize::MAX);
        assert_eq!(other, Some(at("new_tab")));
        b.take_from(at("new_tab"), at("new_tab"), &Keys::Chords(vec![t]));
        assert!(b.chords(Command::NewTab).is_empty());
        // A family's digit switches the family off, as a binding taking it does.
        let mut b = Bindings::defaults(true);
        let one = of("cmd+1");
        let tabs = b.holder(&one, usize::MAX).unwrap();
        b.take_from(tabs, tabs, &Keys::Chords(vec![one]));
        assert_eq!(b.get(at("go_to_tab")), &Keys::Held(None));
        // ⌘Q and the like would be taken from every app.
        assert_eq!(refused(&of("cmd+q"), true, None), Some(Why::System));
        assert_eq!(refused(&of("ctrl+`"), true, None), None);
    }

    #[test]
    fn a_key_put_back_takes_its_default_from_whoever_has_it_since() {
        let mut b = Bindings::defaults(true);
        let (new_tab, find) = (at("new_tab"), at("find"));
        b.put(new_tab, Keys::Chords(vec![chord("cmd+n")]));
        b.put(find, Keys::Chords(vec![chord("cmd+t")]));
        b.reset(new_tab);
        assert_eq!(b.label_at(new_tab), "⌘T");
        assert!(b.get(find).is_empty(), "Find gave ⌘T back");
        let was = Bindings::defaults(true);
        assert_eq!(b.changed_from(&was), [find]);
    }

    #[test]
    fn only_whats_changed_is_kept_in_the_config() {
        let mut changes = BTreeMap::new();
        let mut b = Bindings::defaults(true);
        let new_tab = at("new_tab");
        b.put(new_tab, Keys::Chords(vec![chord("cmd+n")]));
        b.write(new_tab, &mut changes);
        b.put(at("record"), Keys::Chords(Vec::new()));
        b.write(at("record"), &mut changes);
        b.put(at("go_to_tab"), Keys::Held(Some(Mods::parse("ctrl").unwrap())));
        b.write(at("go_to_tab"), &mut changes);
        assert_eq!(changes.get("new_tab"), Some(&KeySetting::One("cmd+n".into())));
        assert_eq!(changes.get("record"), Some(&KeySetting::One("none".into())));
        assert_eq!(changes.get("go_to_tab"), Some(&KeySetting::One("ctrl".into())));
        assert_eq!(Bindings::with(&changes, true), b);
        // Put back as it was, it's gone from the config.
        b.put(new_tab, Keys::Chords(vec![chord("cmd+t")]));
        b.write(new_tab, &mut changes);
        assert!(!changes.contains_key("new_tab"));
    }

    #[test]
    fn a_config_flow_cant_read_a_key_of_keeps_the_defaults_there() {
        let text = "[keys]\nnew_tab = \"cmd+n\"\nfind = \"hyper+f\"\nnot_a_key = \"cmd+x\"\ncopy = 5\nbigger_text = [\"cmd+plus\", \"cmd+]\"]\nquit = \"cmd+w\"\n";
        let b = Bindings::from_config_text(text, true);
        assert_eq!(b.label_at(at("new_tab")), "⌘N");
        assert_eq!(b.label_at(at("find")), "⌘F");
        assert_eq!(b.label_at(at("copy")), "⌘C");
        assert_eq!(b.label_at(at("bigger_text")), "⌘+, ⌘]");
        assert_eq!(b.label_at(at("quit")), "⌘Q");
        // The whole config still reads with a key that isn't one.
        let cfg: crate::config::Config = toml::from_str(text).unwrap();
        assert_eq!(cfg.keys.get("copy"), Some(&KeySetting::Other(toml::Value::Integer(5))));
    }

    #[test]
    fn keys_the_shell_would_get_are_said_so() {
        assert!(shells(&chord("ctrl+l"), false));
        assert!(!shells(&chord("ctrl+shift+l"), false));
        assert!(shells(&chord("ctrl+shift+l"), true));
        assert!(shells(&chord("alt+b"), false));
        assert!(!shells(&chord("cmd+l"), true));
        assert!(!shells(&chord("ctrl+f5"), false));
    }
}
