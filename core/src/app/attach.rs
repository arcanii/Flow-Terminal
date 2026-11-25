//! Files given to the assistant (`docs/design/assistant-files.md`,
//! `crate::ai::files`): dropped on its panel, pasted as a picture, or
//! picked with Attach, each read on a thread of its own into a chip under
//! the panel's input, and sent with the next message. A file dropped on a
//! pane has its path typed there instead, quoted for its shell. winit gives
//! a drop no place, so Flow reads the pointer itself
//! (`Platform::pointer`), and while a file is over the window it lights
//! where it would land.

use std::path::PathBuf;

use super::*;
use crate::ai::files::{self, Attached};

/// Where a file dropped now would land.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum DropPlace {
    /// The assistant's panel, open.
    Assistant,
    /// The room the closed panel opens in, at the window's right.
    OpensAssistant,
    /// Pane `p` of the tab in front.
    Pane(usize),
    Nowhere,
}

/// Where a file dropped with the pointer `at` lands: on the panel (`panel`,
/// while it's open) the assistant, in the room it opens in (`room`, while
/// it's closed) the assistant too, on the terminal (`term`) the pane there
/// (`pane_at`). Where the pointer can't be read, the panel where it's open
/// and `has_keys`, else the pane in front (`front`).
fn drop_place(at: Option<(f32, f32)>, (panel, room, has_keys): (Option<ui::Rect>, Option<ui::Rect>, bool), term: ui::Rect, pane_at: impl Fn((f32, f32)) -> usize, front: usize) -> DropPlace {
    let Some(p) = at else {
        return if panel.is_some() && has_keys { DropPlace::Assistant } else { DropPlace::Pane(front) };
    };
    if panel.is_some_and(|r| r.contains(p)) || (panel.is_none() && room.is_some_and(|r| r.contains(p))) {
        return if panel.is_some() { DropPlace::Assistant } else { DropPlace::OpensAssistant };
    }
    if term.contains(p) {
        return DropPlace::Pane(pane_at(p));
    }
    DropPlace::Nowhere
}

impl App {
    /// Where the pointer is now, in the picture's pixels, read from the
    /// system (no `CursorMoved` comes while a file is dragged): none where
    /// it can't be read (Wayland).
    fn pointer_now(&self) -> Option<(f32, f32)> {
        let w = self.window.as_ref()?;
        let (x, y) = self.platform.pointer(w)?;
        Some(self.on_picture((x as f32, y as f32)))
    }

    /// The room the assistant's panel takes when it's open, while it's
    /// closed.
    fn panel_room(&self) -> Option<ui::Rect> {
        let border = self.config.colors.ink().border.is_some();
        let (l, scale) = (&self.layout, self.scale);
        Layout::compute(l.width, l.height, scale, (l.cell_w, l.cell_h), true, border).panel
    }

    /// Where a file dropped now would land: by the pointer, else (it can't
    /// be read) the panel where it's open and has the keys, or the pane in
    /// front.
    pub(super) fn drop_place(&self) -> DropPlace {
        if let Some(place) = self.shown_drop {
            return place;
        }
        let front = self.tabs.get(self.active).map_or(0, |t| t.focus);
        let at = self.pointer_now();
        let room = if self.layout.panel.is_none() { self.panel_room() } else { None };
        drop_place(at, (self.layout.panel, room, self.ai.focus), self.layout.term, |p| self.pane_at(p).unwrap_or(front), front)
    }

    /// What `View::drop` shows while a file is over the window.
    pub(super) fn drop_shown(&self) -> Option<(ui::Rect, String)> {
        if !self.file_over || self.covered() {
            return None;
        }
        match self.drop_place() {
            DropPlace::Assistant => Some((self.layout.panel?, t("attach.drop_assistant").to_string())),
            DropPlace::OpensAssistant => Some((self.panel_room()?, t("attach.drop_assistant").to_string())),
            DropPlace::Pane(p) => {
                let tab = self.tabs.get(self.active)?;
                let rect = Self::pane_layouts_in(tab, &self.layout, self.pane_gap(), &self.pane_looks, &self.frames(self.active)).into_iter().find(|(i, _)| *i == p).map_or(self.layout.term, |(_, l)| l.term);
                Some((rect, t("attach.drop_path").to_string()))
            }
            DropPlace::Nowhere => None,
        }
    }

    /// A file came over the window, or went away again.
    pub(super) fn file_hovered(&mut self, over: bool) {
        self.file_over = over;
        self.request_redraw();
    }

    /// A file dropped on the window: to the assistant where it landed on its
    /// panel (or the room it opens in), else its path typed at the prompt
    /// of the pane it landed on, quoted for that pane's shell.
    pub(super) fn file_dropped(&mut self, path: PathBuf) {
        let place = self.drop_place();
        self.file_over = false;
        if self.covered() {
            return;
        }
        match place {
            DropPlace::Assistant | DropPlace::OpensAssistant => {
                if !self.ai.open {
                    self.toggle_ai();
                }
                self.ai.focus = true;
                self.attach_paths(vec![path]);
            }
            DropPlace::Pane(p) => {
                let Some(pane) = self.tabs.get(self.active).and_then(|t| t.panes.get(p)) else { return };
                let s = &pane.session;
                // A path holding a control character isn't typed: it would
                // be keys, not letters (a Ctrl+C ending its quotes, a
                // line break running what came after), and a file's name,
                // or its folders', is whatever whoever made it chose. At
                // the prompt Flow's hook drew it goes to that shell, whose
                // hook has bash take letters past ASCII as letters. Else
                // (a program runs there, a shell started inside it or one
                // over ssh among them, or bash's hook said its prompt may
                // take them as keys) what reads it may be a bash without
                // Flow's hook, whatever the pane's shell, whose readline
                // in a C locale would take them as keys: into zsh or bash,
                // anything past ASCII goes as its bytes
                // (`shell::quote_ascii`).
                let words = [path.to_string_lossy().into_owned()];
                let quoted = match s.types_letters() {
                    true => crate::shell::quote(s.profile.kind, s.profile.flavor, &words),
                    false => crate::shell::quote_ascii(s.profile.kind, s.profile.flavor, &words),
                };
                match quoted {
                    Ok(word) => s.write(format!("{word} ").as_bytes()),
                    Err(why) => self.show_toast(tf("attach.untypable", &[("character", &why.code())])),
                }
            }
            DropPlace::Nowhere => {}
        }
        self.request_redraw();
    }

    /// Reads `paths` for the assistant, each on a thread of its own, its
    /// chip showing it's being read until it comes back
    /// (`UserEvent::FileRead`); past [`files::MOST_FILES`] a toast says so.
    pub(super) fn attach_paths(&mut self, paths: Vec<PathBuf>) {
        for path in paths {
            if self.ai.files.len() + self.ai.reading.len() >= files::MOST_FILES {
                self.show_toast(tf("attach.too_many", &[("most", &files::MOST_FILES)]));
                break;
            }
            let name = path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned());
            self.ai.reading.push(name.clone());
            let waker = self.waker.clone();
            let read = move || {
                let read = files::read(&path).map(Box::new).map_err(|why| why.words());
                waker.send(UserEvent::FileRead { name, read });
            };
            if let Err(e) = std::thread::Builder::new().name("flow-attach".into()).spawn(read) {
                log::warn!("couldn't read a file for the assistant: {e}");
                self.ai.reading.pop();
            }
        }
        self.request_redraw();
    }

    /// A file read for the assistant: its chip, unless the files together
    /// would pass [`files::MOST_TOGETHER`]; or why it couldn't be, in a
    /// toast.
    pub(super) fn file_read(&mut self, name: String, read: Result<Box<Attached>, String>) {
        if let Some(i) = self.ai.reading.iter().position(|n| *n == name) {
            self.ai.reading.remove(i);
        }
        match read {
            Ok(file) => {
                let together: u64 = self.ai.files.iter().map(|f| f.size).sum::<u64>() + file.size;
                if together > files::MOST_TOGETHER {
                    self.show_toast(tf("attach.too_much", &[("size", &files::size_words(together)), ("most", &files::size_words(files::MOST_TOGETHER))]));
                } else {
                    self.ai.files.push(*file);
                }
            }
            Err(words) => self.show_toast(words),
        }
        self.request_redraw();
    }

    /// The panel's Attach: the system's panel for picking files, several at
    /// once, in the shell's folder where that's one on this computer (any
    /// program can say where the shell is: `link::on_this_computer`).
    pub(super) fn attach_dialog(&mut self) {
        let mut dialog = rfd::FileDialog::new().set_title(t("attach.dialog_title"));
        if let Some(dir) = self.session().and_then(|s| s.cwd()).map(PathBuf::from).filter(|d| crate::link::on_this_computer(d) && d.is_dir()).or_else(dirs::home_dir) {
            dialog = dialog.set_directory(dir);
        }
        if let Some(window) = self.window.as_deref() {
            dialog = dialog.set_parent(window);
        }
        let Some(paths) = self.in_dialog(|| dialog.pick_files()) else { return };
        if !self.ai.open {
            self.toggle_ai();
        }
        self.ai.focus = true;
        self.attach_paths(paths);
    }

    /// A paste into the panel with no text on the clipboard: a picture
    /// there, if there's one, made a chip ("Pasted picture 1"). False
    /// where there's none.
    pub(super) fn paste_picture(&mut self) -> bool {
        if self.clipboard.is_none() {
            self.clipboard = arboard::Clipboard::new().map_err(|e| log::warn!("no clipboard: {e}")).ok();
        }
        let Some(image) = self.clipboard.as_mut().and_then(|cb| cb.get_image().ok()) else { return false };
        if self.ai.files.len() + self.ai.reading.len() >= files::MOST_FILES {
            self.show_toast(tf("attach.too_many", &[("most", &files::MOST_FILES)]));
            return true;
        }
        self.pasted_pictures += 1;
        let name = tf("attach.pasted", &[("n", &self.pasted_pictures)]);
        let (w, h, rgba) = (image.width as u32, image.height as u32, image.bytes.into_owned());
        self.ai.reading.push(name.clone());
        let waker = self.waker.clone();
        let read = move || {
            let read = files::pasted(&name, w, h, rgba).map(Box::new).map_err(|why| why.words());
            waker.send(UserEvent::FileRead { name, read });
        };
        if std::thread::Builder::new().name("flow-attach".into()).spawn(read).is_err() {
            self.ai.reading.pop();
        }
        self.request_redraw();
        true
    }

    /// A chip's ✕: the file no longer goes.
    pub(super) fn remove_attached(&mut self, i: usize) {
        if i < self.ai.files.len() {
            self.ai.files.remove(i);
        }
        self.request_redraw();
    }

    /// A click on a secret's chip: it may go.
    pub(super) fn confirm_attached(&mut self, i: usize) {
        if let Some(f) = self.ai.files.get_mut(i) {
            f.confirmed = true;
        }
        self.request_redraw();
    }

    /// For snapshots: the panel with files waiting on their chips (a
    /// picture, a PDF, a key that waits to be confirmed, one being read)
    /// under a made-up conversation that sent two before
    /// (`assistant-files.png`), then a picture dragged over it
    /// (`assistant-drop.png`), all put back after.
    pub(super) fn show_files(&mut self, files_png: &std::path::Path, drop_png: &std::path::Path) {
        // A made-up screenshot: a window's frame over a wash.
        let (w, h) = (1200u32, 760u32);
        let shot = image::RgbaImage::from_fn(w, h, |x, y| {
            let frame = (120..1080).contains(&x) && (90..680).contains(&y);
            let bar = frame && y < 130;
            match (bar, frame) {
                (true, _) => image::Rgba([0x2b, 0x31, 0x3f, 255]),
                (_, true) => image::Rgba([0xf4, 0xf1, 0xea, 255]),
                _ => image::Rgba([(40 + x * 60 / w) as u8, (70 + y * 90 / h) as u8, 160, 255]),
            }
        });
        let mut png = Vec::new();
        let _ = image::ImageEncoder::write_image(image::codecs::png::PngEncoder::new(&mut png), shot.as_raw(), w, h, image::ExtendedColorType::Rgba8);
        let made: Vec<Attached> = [
            files::from_bytes("checkout-page.png", png),
            files::from_bytes("design-spec.pdf", b"%PDF-1.7 /Type /Page /Type /Page /Type /Page".to_vec()),
            files::from_bytes("deploy_key", b"-----BEGIN OPENSSH PRIVATE KEY-----\nmade up\n-----END OPENSSH PRIVATE KEY-----\n".to_vec()),
        ]
        .into_iter()
        .flatten()
        .collect();
        let entries = std::mem::replace(
            &mut self.ai.entries,
            vec![
                Entry::User("why does the total wrap onto two lines here?".into()),
                Entry::Files(vec!["cart.png".into()]),
                Entry::Assistant("The total sits in a `flex` row with the coupon field, which takes **60%** of it, so at this width the price has 96 px and wraps. Give the field `flex: 1` and the total `white-space: nowrap`.".into()),
            ],
        );
        let (files, reading) = (std::mem::replace(&mut self.ai.files, made), std::mem::replace(&mut self.ai.reading, vec!["notes.md".into()]));
        let input = std::mem::take(&mut self.ai.input);
        self.ai.set_input("does the page match the spec now?".into());
        let (open, focus) = (self.ai.open, self.ai.focus);
        if !self.ai.open {
            self.toggle_ai();
        }
        self.ai.focus = true;
        self.capture(files_png, true);
        // A picture dragged over the panel.
        self.file_over = true;
        self.shown_drop = Some(DropPlace::Assistant);
        self.capture(drop_png, true);
        self.file_over = false;
        self.shown_drop = None;
        self.ai.entries = entries;
        self.ai.files = files;
        self.ai.reading = reading;
        self.ai.set_input(input);
        if self.ai.open != open {
            self.toggle_ai();
        }
        self.ai.focus = focus;
    }

    /// The files to go with a message now: all of them where every one may
    /// go, else none, a toast saying a secret waits on its chip.
    pub(super) fn files_to_send(&mut self) -> Result<Vec<Attached>, ()> {
        if self.ai.files.iter().any(|f| !f.ready()) {
            self.show_toast(t("attach.secret_waits").to_string());
            return Err(());
        }
        Ok(std::mem::take(&mut self.ai.files))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drop_lands_where_the_pointer_is_else_where_the_keys_are() {
        let term = ui::Rect::new(0.0, 40.0, 600.0, 560.0);
        let panel = ui::Rect::new(600.0, 40.0, 400.0, 560.0);
        // Two panes side by side, the line between them at 300.
        let pane_at = |(x, _): (f32, f32)| usize::from(x >= 300.0);
        let open = (Some(panel), None, false);
        assert_eq!(drop_place(Some((800.0, 300.0)), open, term, pane_at, 0), DropPlace::Assistant);
        assert_eq!(drop_place(Some((100.0, 300.0)), open, term, pane_at, 1), DropPlace::Pane(0), "the pane under it, not the one in front");
        assert_eq!(drop_place(Some((400.0, 300.0)), open, term, pane_at, 0), DropPlace::Pane(1));
        assert_eq!(drop_place(Some((300.0, 10.0)), open, term, pane_at, 0), DropPlace::Nowhere, "the title bar");
        // Closed, the room it would open in is the assistant's.
        let wide = ui::Rect::new(0.0, 40.0, 1000.0, 560.0);
        let closed = (None, Some(panel), false);
        assert_eq!(drop_place(Some((800.0, 300.0)), closed, wide, pane_at, 0), DropPlace::OpensAssistant);
        assert_eq!(drop_place(Some((100.0, 300.0)), closed, wide, pane_at, 0), DropPlace::Pane(0));
        // No pointer to read (Wayland): the panel where it has the keys.
        assert_eq!(drop_place(None, (Some(panel), None, true), term, pane_at, 1), DropPlace::Assistant);
        assert_eq!(drop_place(None, (Some(panel), None, false), term, pane_at, 1), DropPlace::Pane(1));
        assert_eq!(drop_place(None, closed, wide, pane_at, 0), DropPlace::Pane(0));
    }
}
