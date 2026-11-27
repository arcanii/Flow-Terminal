//! A tab's name typed in place, in its pill in the title bar
//! (`docs/design/tab-groups.md`'s first step): a double click on the tab
//! in front, or Rename Tab (the palette, the Mac's Shell menu, a key the
//! user gives it), puts a field there with the name in it. Every key goes
//! to the field while it's there, as Settings' text rows take them: Enter
//! keeps the name, Esc puts back the old one, and a click anywhere else
//! keeps it too. A name left empty gives the tab its shell's name again.
//! The name goes into `tabs.json` with the tab.

use super::*;

/// A tab's name being typed.
pub(super) struct Renaming {
    /// Which tab, by its id: tabs open, close and move meanwhile.
    pub(super) tab: u64,
    pub(super) field: settings::TextField,
}

impl App {
    /// Starts typing tab `i`'s name, with its name in the field (its
    /// shell's where it has none, without the theme the title bar adds)
    /// and the caret at its end. A name being typed for another tab is
    /// kept first.
    pub(super) fn rename_tab(&mut self, i: usize) {
        self.finish_renaming(true);
        let Some(tab) = self.tabs.get(i) else { return };
        let typed = tab.name.clone().unwrap_or_else(|| self.unnamed(i));
        self.renaming = Some(Renaming { tab: tab.id, field: settings::TextField::line(&typed, TAB_NAME_MOST) });
        self.request_redraw();
    }

    /// What tab `i` is called without a name of its own: its shell's,
    /// numbered among the other tabs without one.
    pub(super) fn unnamed(&self, i: usize) -> String {
        let tabs: Vec<(&str, Option<&str>)> = self.tabs.iter().enumerate().map(|(j, t)| (self.profiles[t.profile()].label.as_str(), t.name.as_deref().filter(|_| j != i))).collect();
        tab_names(&tabs).swap_remove(i)
    }

    /// The place of the tab whose name is being typed, while it's still open.
    pub(super) fn renaming_at(&self) -> Option<usize> {
        let r = self.renaming.as_ref()?;
        self.tabs.iter().position(|t| t.id == r.tab)
    }

    /// Stops typing a tab's name: keeping what's typed (`keep`), or leaving
    /// the name as it was. An empty name is none, and so is the one the tab
    /// is called without a name, so it follows its shell again, numbered as
    /// the others come and go.
    pub(super) fn finish_renaming(&mut self, keep: bool) {
        let Some(r) = self.renaming.take() else { return };
        self.request_redraw();
        let Some(i) = self.tabs.iter().position(|t| t.id == r.tab).filter(|_| keep) else { return };
        let unnamed = self.unnamed(i);
        self.tabs[i].name = tab_name(&r.field.text()).filter(|n| *n != unnamed);
    }

    /// A key while a tab's name is being typed: it takes them all. The
    /// arrows, Home and End move the caret (⌥ on the Mac and Ctrl elsewhere
    /// by words, ⌘ on the Mac to the ends), Backspace and Delete delete,
    /// ⌘V or Ctrl+Shift+V pastes, Enter keeps the name and Esc doesn't.
    pub(super) fn rename_key(&mut self, event: &KeyEvent) {
        let mods = self.modifiers;
        let mac = cfg!(target_os = "macos");
        let (ctrl, shift) = (mods.control_key(), mods.shift_key());
        let jump = if mac { mods.super_key() } else { false };
        let word = if mac { mods.alt_key() } else { ctrl };
        let command = ctrl || mods.super_key();
        let base = match event.key_without_modifiers() {
            Key::Character(c) => c.to_lowercase(),
            _ => String::new(),
        };
        let Some(f) = self.renaming.as_mut().map(|r| &mut r.field) else { return };
        match &event.logical_key {
            Key::Named(NamedKey::Escape) => self.finish_renaming(false),
            Key::Named(NamedKey::Enter | NamedKey::Tab) => self.finish_renaming(true),
            Key::Named(NamedKey::Backspace) if jump => f.backspace_line(),
            Key::Named(NamedKey::Backspace) if word => f.backspace_word(),
            Key::Named(NamedKey::Backspace) => f.backspace(),
            Key::Named(NamedKey::Delete) if word => f.delete_word(),
            Key::Named(NamedKey::Delete) => f.delete(),
            Key::Named(NamedKey::ArrowLeft | NamedKey::ArrowUp) if jump => f.home(),
            Key::Named(NamedKey::ArrowLeft) if word => f.word_left(),
            Key::Named(NamedKey::ArrowLeft) => f.left(),
            Key::Named(NamedKey::ArrowRight | NamedKey::ArrowDown) if jump => f.end(),
            Key::Named(NamedKey::ArrowRight) if word => f.word_right(),
            Key::Named(NamedKey::ArrowRight) => f.right(),
            Key::Named(NamedKey::Home | NamedKey::ArrowUp) => f.home(),
            Key::Named(NamedKey::End | NamedKey::ArrowDown) => f.end(),
            Key::Named(NamedKey::Insert) if shift => self.paste(),
            _ if command && base == "v" => self.paste(),
            // Other shortcuts type nothing, but AltGr (Ctrl+Alt on Windows)
            // types letters.
            _ if mods.super_key() || (ctrl && !mods.alt_key()) => {}
            _ => {
                if let Some(text) = &event.text {
                    f.insert(text);
                }
            }
        }
        self.request_redraw();
    }

    /// Text typed or pasted while a tab's name is being typed: into it, its
    /// first line. True where it took it.
    pub(super) fn rename_type(&mut self, text: &str) -> bool {
        let Some(r) = self.renaming.as_mut() else { return false };
        r.field.insert(text.lines().next().unwrap_or_default());
        self.request_redraw();
        true
    }
}
