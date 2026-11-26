//! The assistant panel's input (`docs/design/assistant-input.md`): the
//! message being typed, its caret and what's selected, its undo, and the
//! messages sent before it ([`Recall`]). None of it draws, so the keys'
//! moves are tested alone. The drawing lays the text out in rows with
//! [`rows`] and keeps where they went in [`Laid`], which the mouse and
//! ↑ ↓ go by.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::ui::Rect;

/// A row of the text as it's laid out: its characters from `start` up to
/// `end`, and whether a line break ends it (where the wrap doesn't).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row {
    pub start: usize,
    pub end: usize,
    pub broken: bool,
}

/// `chars` laid out in rows `room` wide, each character as wide as `width`
/// says: a row breaks after a space, before or after a Chinese or Japanese
/// character, or, in a word longer than the row, where it runs out of
/// room. Spaces may hang past the edge, as they do in any text box, so a
/// row never starts with the space that broke it. A line break ends a row,
/// and there's always one row, empty for no text.
pub fn rows(chars: &[char], width: impl Fn(char) -> f32, room: f32) -> Vec<Row> {
    let mut rows = Vec::new();
    let (mut start, mut x, mut fit) = (0, 0.0_f32, None::<usize>);
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '\n' {
            rows.push(Row { start, end: i, broken: true });
            (start, x, fit) = (i + 1, 0.0, None);
            i += 1;
            continue;
        }
        let w = width(ch);
        if x + w > room && i > start && ch != ' ' && w > 0.0 {
            // Before this character where it may break there, else after
            // the last place the row may, else here.
            let end = if crate::text::is_cjk(ch) { i } else { fit.filter(|&f| f > start).unwrap_or(i) };
            rows.push(Row { start, end, broken: false });
            start = end;
            x = chars[start..i].iter().map(|&c| width(c)).sum();
            fit = None;
            continue;
        }
        x += w;
        if ch == ' ' || crate::text::is_cjk(ch) {
            fit = Some(i + 1);
        }
        i += 1;
    }
    rows.push(Row { start, end: chars.len(), broken: false });
    rows
}

/// The row place `at` is on: at the end of a row the wrap ended, the start
/// of the next.
pub fn row_of(rows: &[Row], at: usize) -> usize {
    rows.partition_point(|r| r.start <= at).saturating_sub(1)
}

/// The furthest the caret goes along row `i`: a row the wrap ended stops
/// before the space that ended it, since its end is the next row's start.
fn row_end(chars: &[char], rows: &[Row], i: usize) -> usize {
    let row = rows[i];
    if row.broken || i + 1 == rows.len() || row.end == row.start || chars.get(row.end - 1) != Some(&' ') {
        row.end
    } else {
        row.end - 1
    }
}

/// How far along its row place `at` is, from the row's start.
fn x_of(chars: &[char], rows: &[Row], at: usize, width: &dyn Fn(char) -> f32) -> f32 {
    let row = rows[row_of(rows, at)];
    chars[row.start..at.clamp(row.start, chars.len())].iter().map(|&c| width(c)).sum()
}

/// The place in row `i` nearest `x` along it.
fn place_in_row(chars: &[char], rows: &[Row], i: usize, x: f32, width: &dyn Fn(char) -> f32) -> usize {
    let (start, end) = (rows[i].start, row_end(chars, rows, i));
    let mut along = 0.0;
    for (k, &c) in chars.iter().enumerate().take(end).skip(start) {
        let w = width(c);
        if x < along + w / 2.0 {
            return k;
        }
        along += w;
    }
    end
}

/// Whether `ch` is part of a word, for the word moves: letters, digits and
/// `_`, in any script, as Settings' fields have it.
fn is_word(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

/// What a change to the text was, so a run of typing, or of deleting,
/// undoes as one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Change {
    Typing,
    Deleting,
    /// A paste, a cut, a message brought back: a step of its own.
    Whole,
}

/// The text, caret and selection as they were before a change.
#[derive(Debug, Clone, Default)]
struct Was {
    chars: Vec<char>,
    caret: usize,
    anchor: Option<usize>,
}

/// The message being typed: its characters, the caret (a place between
/// them, counted in characters) and the selection's other end.
#[derive(Debug, Clone, Default)]
pub struct Input {
    chars: Vec<char>,
    caret: usize,
    /// Where the selection began, the caret being its other end.
    anchor: Option<usize>,
    /// How far along its row the caret was when ↑ or ↓ began, which they
    /// keep to across shorter rows.
    goal: Option<f32>,
    undo: Vec<Was>,
    redo: Vec<Was>,
    /// The last change, while nothing has moved the caret since.
    last: Option<Change>,
    /// A number no other text has had, new with each change, so its rows
    /// are laid out again only then: an input put in another's place (a
    /// snapshot's, then the user's back) is never taken for it.
    version: u64,
}

impl Input {
    /// The most characters it takes: a pasted log, not a whole file, which
    /// goes with Attach.
    pub const MOST: usize = 100_000;
    /// The most changes undo goes back through.
    pub const UNDOS: usize = 100;

    pub fn text(&self) -> String {
        self.chars.iter().collect()
    }

    pub fn chars(&self) -> &[char] {
        &self.chars
    }

    pub fn is_empty(&self) -> bool {
        self.chars.is_empty()
    }

    pub fn caret(&self) -> usize {
        self.caret
    }

    pub fn anchor(&self) -> Option<usize> {
        self.anchor
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    /// The characters selected, from the first, if any are.
    pub fn selection(&self) -> Option<Range<usize>> {
        let anchor = self.anchor?;
        let range = anchor.min(self.caret)..anchor.max(self.caret);
        (!range.is_empty()).then_some(range)
    }

    pub fn selected(&self) -> Option<String> {
        self.selection().map(|r| self.chars[r].iter().collect())
    }

    /// Puts `text` in place of all there is, the caret at its end, as one
    /// change: a message brought back, a command to edit.
    pub fn set(&mut self, text: &str) {
        if self.text() == text {
            self.end_selection();
            return;
        }
        self.keep(Change::Whole);
        self.chars = clean(text).into_iter().take(Self::MOST).collect();
        (self.caret, self.anchor) = (self.chars.len(), None);
        self.changed();
    }

    /// Types `text` at the caret, over the selection: false where it was
    /// cut at [`Input::MOST`].
    pub fn type_text(&mut self, text: &str) -> bool {
        self.put(text, Change::Typing)
    }

    /// Pastes `text` at the caret, over the selection, keeping its line
    /// breaks: false where it was cut at [`Input::MOST`].
    pub fn paste(&mut self, text: &str) -> bool {
        self.put(text, Change::Whole)
    }

    fn put(&mut self, text: &str, change: Change) -> bool {
        let text = clean(text);
        if text.is_empty() && self.selection().is_none() {
            return true;
        }
        self.keep(change);
        self.take_selection();
        let room = Self::MOST - self.chars.len();
        let n = text.len().min(room);
        self.chars.splice(self.caret..self.caret, text[..n].iter().copied());
        self.caret += n;
        self.changed();
        n == text.len()
    }

    /// Removes the selection and says what it was: Cut.
    pub fn cut(&mut self) -> Option<String> {
        let cut = self.selected()?;
        self.keep(Change::Whole);
        self.take_selection();
        self.changed();
        Some(cut)
    }

    /// Deletes the selection, or the character before the caret.
    pub fn backspace(&mut self) {
        let to = self.caret.saturating_sub(1);
        self.delete_back_to(to);
    }

    /// Deletes the selection, or the character after the caret.
    pub fn delete(&mut self) {
        let to = (self.caret + 1).min(self.chars.len());
        self.delete_on_to(to);
    }

    /// Deletes the selection, or back to the start of the word before the
    /// caret (⌥⌫, Ctrl+Backspace).
    pub fn backspace_word(&mut self) {
        let to = self.word_before();
        self.delete_back_to(to);
    }

    /// Deletes the selection, or on to the end of the word after the caret
    /// (⌥⌦, Ctrl+Delete).
    pub fn delete_word(&mut self) {
        let to = self.word_after();
        self.delete_on_to(to);
    }

    /// Deletes the selection, or back to the start of the caret's row
    /// (⌘⌫); at its start, the character before.
    pub fn backspace_row(&mut self, rows: &[Row]) {
        let start = rows[row_of(rows, self.caret)].start;
        let to = if start < self.caret { start } else { self.caret.saturating_sub(1) };
        self.delete_back_to(to);
    }

    fn delete_back_to(&mut self, to: usize) {
        if self.selection().is_none() && to == self.caret {
            return;
        }
        self.keep(Change::Deleting);
        if !self.take_selection() {
            self.chars.drain(to..self.caret);
            self.caret = to;
        }
        self.changed();
    }

    fn delete_on_to(&mut self, to: usize) {
        if self.selection().is_none() && to == self.caret {
            return;
        }
        self.keep(Change::Deleting);
        if !self.take_selection() {
            self.chars.drain(self.caret..to);
        }
        self.changed();
    }

    /// Takes the selected characters out, the caret where they were: false
    /// where none were.
    fn take_selection(&mut self) -> bool {
        let Some(range) = self.selection() else {
            self.anchor = None;
            return false;
        };
        self.caret = range.start;
        self.chars.drain(range);
        self.anchor = None;
        true
    }

    /// Notes the text as it is before a change, unless it goes on a run of
    /// the same kind; a new change can't be redone past.
    fn keep(&mut self, change: Change) {
        if change == Change::Whole || self.last != Some(change) {
            self.undo.push(Was { chars: self.chars.clone(), caret: self.caret, anchor: self.anchor });
            if self.undo.len() > Self::UNDOS {
                self.undo.remove(0);
            }
        }
        self.redo.clear();
        self.last = Some(change).filter(|&c| c != Change::Whole);
    }

    fn changed(&mut self) {
        static VERSIONS: AtomicU64 = AtomicU64::new(1);
        self.goal = None;
        self.version = VERSIONS.fetch_add(1, Ordering::Relaxed);
    }

    /// Takes the last change back: false where there's none.
    pub fn undo(&mut self) -> bool {
        let Some(was) = self.undo.pop() else { return false };
        self.redo.push(Was { chars: std::mem::take(&mut self.chars), caret: self.caret, anchor: self.anchor });
        self.put_back(was);
        true
    }

    /// Makes the last change taken back again: false where there's none.
    pub fn redo(&mut self) -> bool {
        let Some(was) = self.redo.pop() else { return false };
        self.undo.push(Was { chars: std::mem::take(&mut self.chars), caret: self.caret, anchor: self.anchor });
        self.put_back(was);
        true
    }

    fn put_back(&mut self, was: Was) {
        let n = was.chars.len();
        (self.chars, self.caret, self.anchor) = (was.chars, was.caret.min(n), was.anchor.filter(|&a| a <= n));
        self.last = None;
        self.changed();
    }

    /// Moves the caret to `to`, with `extend` taking the selection with it,
    /// else ending it.
    fn go(&mut self, to: usize, extend: bool) {
        if extend {
            self.anchor.get_or_insert(self.caret);
        } else {
            self.anchor = None;
        }
        self.caret = to.min(self.chars.len());
        if self.anchor == Some(self.caret) {
            self.anchor = None;
        }
        self.goal = None;
        self.last = None;
    }

    /// Lets go of the selection, leaving the caret: false where there was
    /// none.
    pub fn end_selection(&mut self) -> bool {
        self.anchor.take().is_some_and(|a| a != self.caret)
    }

    /// ←: a character back, or to the start of the selection.
    pub fn left(&mut self, extend: bool) {
        let to = match self.selection() {
            Some(r) if !extend => r.start,
            _ => self.caret.saturating_sub(1),
        };
        self.go(to, extend);
    }

    /// →: a character on, or to the end of the selection.
    pub fn right(&mut self, extend: bool) {
        let to = match self.selection() {
            Some(r) if !extend => r.end,
            _ => self.caret + 1,
        };
        self.go(to, extend);
    }

    fn word_before(&self) -> usize {
        let mut at = self.caret;
        while at > 0 && !is_word(self.chars[at - 1]) {
            at -= 1;
        }
        while at > 0 && is_word(self.chars[at - 1]) {
            at -= 1;
        }
        at
    }

    fn word_after(&self) -> usize {
        let mut at = self.caret;
        while at < self.chars.len() && !is_word(self.chars[at]) {
            at += 1;
        }
        while at < self.chars.len() && is_word(self.chars[at]) {
            at += 1;
        }
        at
    }

    /// To the start of the word before the caret (⌥←, Ctrl+←), past the
    /// spaces and line breaks between.
    pub fn word_left(&mut self, extend: bool) {
        let to = self.word_before();
        self.go(to, extend);
    }

    /// To the end of the word after the caret (⌥→, Ctrl+→).
    pub fn word_right(&mut self, extend: bool) {
        let to = self.word_after();
        self.go(to, extend);
    }

    /// To the start of the caret's row (Home, ⌘←).
    pub fn home(&mut self, rows: &[Row], extend: bool) {
        let to = rows[row_of(rows, self.caret)].start;
        self.go(to, extend);
    }

    /// To the end of the caret's row (End, ⌘→).
    pub fn end(&mut self, rows: &[Row], extend: bool) {
        let to = row_end(&self.chars, rows, row_of(rows, self.caret));
        self.go(to, extend);
    }

    /// To the start of the whole text (⌘↑, Ctrl+Home).
    pub fn top(&mut self, extend: bool) {
        self.go(0, extend);
    }

    /// To the end of the whole text (⌘↓, Ctrl+End).
    pub fn bottom(&mut self, extend: bool) {
        let to = self.chars.len();
        self.go(to, extend);
    }

    /// ↑: to the row above, as far along as the caret was when ↑ or ↓
    /// began; false on the first row, where it brings back a message
    /// instead (or, with `extend`, selects to the start).
    pub fn up(&mut self, rows: &[Row], width: &dyn Fn(char) -> f32, extend: bool) -> bool {
        self.vertical(rows, width, extend, false)
    }

    /// ↓: to the row below, as [`Input::up`] goes up; false on the last.
    pub fn down(&mut self, rows: &[Row], width: &dyn Fn(char) -> f32, extend: bool) -> bool {
        self.vertical(rows, width, extend, true)
    }

    fn vertical(&mut self, rows: &[Row], width: &dyn Fn(char) -> f32, extend: bool, down: bool) -> bool {
        let row = row_of(rows, self.caret);
        let Some(to) = (if down { (row + 1 < rows.len()).then_some(row + 1) } else { row.checked_sub(1) }) else {
            if extend {
                if down { self.bottom(true) } else { self.top(true) }
            }
            return false;
        };
        let goal = self.goal.unwrap_or_else(|| x_of(&self.chars, rows, self.caret, width));
        let at = place_in_row(&self.chars, rows, to, goal, width);
        self.go(at, extend);
        self.goal = Some(goal);
        true
    }

    /// Puts the caret at place `at`, as a click does; with `extend` (a
    /// drag, a ⇧-click) selecting from where the selection began, or the
    /// caret was.
    pub fn place(&mut self, at: usize, extend: bool) {
        self.go(at, extend);
    }

    /// Selects the word at place `at`, or the spaces there: a double click.
    pub fn select_word(&mut self, at: usize) {
        let at = at.min(self.chars.len());
        let kind = |c: char| if is_word(c) { 0 } else if c == '\n' { 2 } else { 1 };
        let Some(here) = self.chars.get(at).or(at.checked_sub(1).and_then(|a| self.chars.get(a))).map(|&c| kind(c)) else { return };
        let (mut from, mut to) = (at, at);
        while from > 0 && kind(self.chars[from - 1]) == here && here != 2 {
            from -= 1;
        }
        while to < self.chars.len() && kind(self.chars[to]) == here && here != 2 {
            to += 1;
        }
        self.go(from, false);
        self.go(to, true);
    }

    /// Selects the line place `at` is on, between line breaks: a triple
    /// click.
    pub fn select_line(&mut self, at: usize) {
        let at = at.min(self.chars.len());
        let from = self.chars[..at].iter().rposition(|&c| c == '\n').map_or(0, |i| i + 1);
        let to = self.chars[at..].iter().position(|&c| c == '\n').map_or(self.chars.len(), |i| at + i);
        self.go(from, false);
        self.go(to, true);
    }

    /// Selects it all (⌘A, Ctrl+A).
    pub fn select_all(&mut self) {
        self.go(0, false);
        self.bottom(true);
    }
}

/// `text` as the input takes it: line breaks as `\n`, a tab a space, and
/// no other control character, which no one types into a message.
fn clean(text: &str) -> Vec<char> {
    text.replace("\r\n", "\n")
        .chars()
        .filter_map(|c| match c {
            '\r' | '\n' => Some('\n'),
            '\t' => Some(' '),
            c if c.is_control() => None,
            c => Some(c),
        })
        .collect()
}

/// The messages sent, which ↑ and ↓ bring back into the input.
#[derive(Debug, Clone, Default)]
pub struct Recall {
    past: Vec<String>,
    /// The one shown, while going through them; `past.len()` is what was
    /// being typed.
    at: Option<usize>,
    /// What each was changed to while it was shown, and what was being
    /// typed (under `past.len()`), until the next is sent.
    edits: HashMap<usize, String>,
}

impl Recall {
    /// The most kept, oldest let go first.
    pub const MOST: usize = 200;

    /// Messages sent before, oldest first: those the journal kept, from
    /// before Flow last started.
    pub fn from_past(mut past: Vec<String>) -> Self {
        if past.len() > Self::MOST {
            past.drain(..past.len() - Self::MOST);
        }
        Recall { past, ..Recall::default() }
    }

    /// `text` was sent: it's the newest, unless it's the newest already,
    /// and going through them starts again from it.
    pub fn sent(&mut self, text: &str) {
        if self.past.last().map(String::as_str) != Some(text) {
            self.past.push(text.to_string());
        }
        if self.past.len() > Self::MOST {
            self.past.drain(..self.past.len() - Self::MOST);
        }
        self.at = None;
        self.edits.clear();
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.past.len()
    }

    /// Whether one of them is shown.
    #[cfg(test)]
    fn going_through(&self) -> bool {
        self.at.is_some()
    }

    /// The message before the one shown, the input holding `now`, which is
    /// kept where it changed: none before the oldest.
    pub fn older(&mut self, now: &str) -> Option<String> {
        let here = self.at.unwrap_or(self.past.len());
        let to = here.checked_sub(1)?;
        self.leave(here, now);
        self.at = Some(to);
        Some(self.shown(to))
    }

    /// The message after the one shown, or past the newest what was being
    /// typed: none while none is shown.
    pub fn newer(&mut self, now: &str) -> Option<String> {
        let here = self.at?;
        self.leave(here, now);
        let to = here + 1;
        if to >= self.past.len() {
            self.at = None;
            return Some(self.edits.remove(&self.past.len()).unwrap_or_default());
        }
        self.at = Some(to);
        Some(self.shown(to))
    }

    fn leave(&mut self, here: usize, now: &str) {
        if self.past.get(here).map(String::as_str) == Some(now) {
            self.edits.remove(&here);
        } else {
            self.edits.insert(here, now.to_string());
        }
    }

    fn shown(&self, i: usize) -> String {
        self.edits.get(&i).cloned().unwrap_or_else(|| self.past[i].clone())
    }
}

/// The input as the last frame laid it out and drew it, for the keys and
/// the mouse, which can't measure text themselves.
#[derive(Debug, Clone, Default)]
pub struct Laid {
    /// The input's version it was laid out at, and the room a row has.
    pub version: u64,
    pub room: f32,
    /// A cell's width and the font's size, which every character's width
    /// is worked out from (`text::advance`).
    pub cell: f32,
    pub em: f32,
    pub rows: Vec<Row>,
    /// The first row in view, and how many are.
    pub first: usize,
    pub shown: usize,
    /// Where each row's text begins, the top of the first in view, and a
    /// row's height.
    pub left: f32,
    pub top: f32,
    pub row_h: f32,
    /// The box the text is in, which takes the mouse.
    pub area: Rect,
}

impl Laid {
    /// How wide `ch` is drawn; before the first frame, a cell.
    pub fn width(&self, ch: char) -> f32 {
        if self.cell > 0.0 { crate::text::advance(ch, self.cell, self.em) } else { 1.0 }
    }

    /// The rows of `input` as it is now: those drawn, or, where it changed
    /// since, laid out again the same way; before the first frame, a row a
    /// line.
    pub fn rows_of(&self, input: &Input) -> Vec<Row> {
        if self.version == input.version() && !self.rows.is_empty() {
            return self.rows.clone();
        }
        let room = if self.room > 0.0 { self.room } else { f32::INFINITY };
        rows(input.chars(), |c| self.width(c), room)
    }

    /// The place under point `p`, or the nearest: above the rows in view,
    /// the row above them, and below, the row below, so a drag past the
    /// box's edge scrolls it.
    pub fn place_at(&self, input: &Input, p: (f32, f32)) -> usize {
        let rows = self.rows_of(input);
        let row_h = self.row_h.max(1.0);
        let i = if p.1 < self.top {
            self.first.saturating_sub(1)
        } else {
            (self.first + ((p.1 - self.top) / row_h) as usize).min(self.first + self.shown.max(1)).min(rows.len() - 1)
        };
        place_in_row(input.chars(), &rows, i, p.0 - self.left, &|c| self.width(c))
    }

    /// The first row to show for the caret's row `caret` of `total`, `room`
    /// rows showing: it moves only as far as keeps the caret in view.
    pub fn follow(first: usize, caret: usize, total: usize, room: usize) -> usize {
        let room = room.max(1);
        let first = if caret < first { caret } else if caret >= first + room { caret + 1 - room } else { first };
        first.min(total.saturating_sub(room))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(_: char) -> f32 {
        1.0
    }

    fn typed(text: &str) -> Input {
        let mut input = Input::default();
        input.type_text(text);
        input
    }

    fn shown(chars: &str, rows: &[Row]) -> Vec<String> {
        let chars: Vec<char> = chars.chars().collect();
        rows.iter().map(|r| chars[r.start..r.end].iter().collect()).collect()
    }

    #[test]
    fn rows_break_after_spaces_and_at_line_breaks() {
        let text = "the quick brown fox\njumps";
        let laid = rows(&text.chars().collect::<Vec<_>>(), one, 10.0);
        assert_eq!(shown(text, &laid), ["the quick ", "brown fox", "jumps"]);
        assert_eq!(laid.iter().map(|r| r.broken).collect::<Vec<_>>(), [false, true, false]);
    }

    #[test]
    fn a_word_longer_than_a_row_is_cut_where_it_runs_out_of_room() {
        let text = "ab abcdefghij";
        assert_eq!(shown(text, &rows(&text.chars().collect::<Vec<_>>(), one, 4.0)), ["ab ", "abcd", "efgh", "ij"]);
    }

    #[test]
    fn rows_break_between_japanese_characters() {
        let text = "日本語の文です";
        let wide = |c: char| if crate::text::is_cjk(c) { 2.0 } else { 1.0 };
        assert_eq!(shown(text, &rows(&text.chars().collect::<Vec<_>>(), wide, 6.0)), ["日本語", "の文で", "す"]);
    }

    #[test]
    fn no_text_and_a_trailing_line_break_each_have_a_row() {
        assert_eq!(rows(&[], one, 10.0), [Row { start: 0, end: 0, broken: false }]);
        let laid = rows(&['a', '\n'], one, 10.0);
        assert_eq!(laid, [Row { start: 0, end: 1, broken: true }, Row { start: 2, end: 2, broken: false }]);
        assert_eq!(row_of(&laid, 1), 0);
        assert_eq!(row_of(&laid, 2), 1);
    }

    #[test]
    fn the_caret_at_a_wraps_end_is_on_the_next_row() {
        let text: Vec<char> = "aaaa bbbb".chars().collect();
        let laid = rows(&text, one, 5.0);
        assert_eq!(row_of(&laid, 5), 1);
        assert_eq!(row_of(&laid, 4), 0);
    }

    #[test]
    fn typing_replaces_the_selection_and_backspace_deletes_it() {
        let mut input = typed("hello world");
        input.word_left(true);
        assert_eq!(input.selected().as_deref(), Some("world"));
        input.type_text("there");
        assert_eq!(input.text(), "hello there");
        input.word_left(true);
        input.word_left(true);
        input.backspace();
        assert_eq!(input.text(), "");
    }

    #[test]
    fn a_move_without_shift_ends_the_selection_at_the_side_it_moves_to() {
        let mut input = typed("abcdef");
        input.place(2, false);
        input.place(4, true);
        input.left(false);
        assert_eq!((input.caret(), input.selection()), (2, None));
        input.place(4, true);
        input.right(false);
        assert_eq!((input.caret(), input.selection()), (4, None));
    }

    #[test]
    fn words_are_selected_and_deleted_as_settings_fields_take_them() {
        let mut input = typed("cargo test --workspace");
        input.backspace_word();
        assert_eq!(input.text(), "cargo test --");
        input.top(false);
        input.delete_word();
        assert_eq!(input.text(), " test --");
        input.select_word(3);
        assert_eq!(input.selected().as_deref(), Some("test"));
        input.select_word(0);
        assert_eq!(input.selected().as_deref(), Some(" "));
    }

    #[test]
    fn a_triple_click_takes_its_line_and_select_all_everything() {
        let mut input = typed("first\nsecond line\nthird");
        input.select_line(8);
        assert_eq!(input.selected().as_deref(), Some("second line"));
        input.select_all();
        assert_eq!(input.selected().as_deref(), Some("first\nsecond line\nthird"));
    }

    #[test]
    fn cut_takes_the_selection_out() {
        let mut input = typed("keep cut");
        input.word_left(true);
        assert_eq!(input.cut().as_deref(), Some("cut"));
        assert_eq!(input.text(), "keep ");
        assert_eq!(input.cut(), None);
    }

    #[test]
    fn a_paste_keeps_its_line_breaks_and_stops_at_the_most() {
        let mut input = Input::default();
        assert!(input.paste("one\r\ntwo\tthree\x07"));
        assert_eq!(input.text(), "one\ntwo three");
        let big = "x".repeat(Input::MOST);
        assert!(!input.paste(&big));
        assert_eq!(input.chars().len(), Input::MOST);
    }

    #[test]
    fn undo_takes_back_a_run_of_typing_at_once_and_redo_makes_it_again() {
        let mut input = typed("hello");
        input.type_text(" there");
        input.left(false);
        input.type_text("!");
        assert_eq!(input.text(), "hello ther!e");
        assert!(input.undo());
        assert_eq!(input.text(), "hello there");
        assert!(input.undo());
        assert_eq!(input.text(), "");
        assert!(!input.undo());
        assert!(input.redo());
        assert_eq!(input.text(), "hello there");
        assert!(input.redo());
        assert_eq!(input.text(), "hello ther!e");
        assert!(!input.redo());
    }

    #[test]
    fn a_paste_or_a_cut_is_a_step_of_its_own() {
        let mut input = typed("ab");
        input.paste("cd");
        input.paste("ef");
        input.select_all();
        input.cut();
        input.undo();
        assert_eq!(input.text(), "abcdef");
        input.undo();
        assert_eq!(input.text(), "abcd");
        input.undo();
        assert_eq!(input.text(), "ab");
    }

    #[test]
    fn a_new_change_ends_what_redo_could_bring_back() {
        let mut input = typed("one");
        input.undo();
        input.type_text("two");
        assert!(!input.redo());
        assert_eq!(input.text(), "two");
    }

    #[test]
    fn up_and_down_move_between_rows_keeping_how_far_along_they_were() {
        let mut input = typed("abcdefgh ab abcdefgh");
        let laid = rows(input.chars(), one, 9.0);
        assert_eq!(shown(&input.text(), &laid), ["abcdefgh ", "ab ", "abcdefgh"]);
        input.place(7, false);
        assert!(input.down(&laid, &one, false));
        // The short row's end is before the space that ended it.
        assert_eq!(input.caret(), 11);
        assert!(input.down(&laid, &one, false));
        assert_eq!(input.caret(), 19);
        assert!(!input.down(&laid, &one, false));
        assert!(input.up(&laid, &one, false));
        assert!(input.up(&laid, &one, false));
        assert_eq!(input.caret(), 7);
        assert!(!input.up(&laid, &one, false));
        assert!(!input.up(&laid, &one, true));
        assert_eq!(input.selected().as_deref(), Some("abcdefg"));
    }

    #[test]
    fn home_and_end_go_to_the_rows_ends_and_command_backspace_deletes_to_its_start() {
        let mut input = typed("first row second");
        let laid = rows(input.chars(), one, 10.0);
        input.place(13, false);
        input.home(&laid, false);
        assert_eq!(input.caret(), 10);
        input.end(&laid, true);
        assert_eq!(input.selected().as_deref(), Some("second"));
        input.place(13, false);
        input.backspace_row(&laid);
        assert_eq!(input.text(), "first row ond");
    }

    #[test]
    fn going_back_through_messages_keeps_what_was_typed_and_each_edit() {
        let mut recall = Recall::default();
        recall.sent("first");
        recall.sent("second");
        recall.sent("second");
        assert_eq!(recall.len(), 2);
        assert_eq!(recall.newer("draft").as_deref(), None);
        assert_eq!(recall.older("draft").as_deref(), Some("second"));
        assert_eq!(recall.older("second, changed").as_deref(), Some("first"));
        assert_eq!(recall.older("first").as_deref(), None);
        assert_eq!(recall.newer("first").as_deref(), Some("second, changed"));
        assert_eq!(recall.newer("second, changed").as_deref(), Some("draft"));
        assert!(!recall.going_through());
        // What's sent is the newest; the one it came from stays as it was.
        recall.older("");
        recall.sent("second, changed");
        assert_eq!(recall.older("").as_deref(), Some("second, changed"));
        assert_eq!(recall.older("").as_deref(), Some("second"));
    }

    #[test]
    fn messages_from_before_a_restart_come_back_first_newest() {
        let mut recall = Recall::from_past(vec!["older".into(), "newest".into()]);
        assert_eq!(recall.older("").as_deref(), Some("newest"));
        assert_eq!(recall.older("newest").as_deref(), Some("older"));
        recall.sent("today's");
        assert_eq!(recall.older("").as_deref(), Some("today's"));
    }

    #[test]
    fn recall_keeps_the_newest_it_can() {
        let mut recall = Recall::default();
        for i in 0..Recall::MOST + 5 {
            recall.sent(&i.to_string());
        }
        assert_eq!(recall.len(), Recall::MOST);
        assert_eq!(recall.older("").as_deref(), Some((Recall::MOST + 4).to_string().as_str()));
    }

    #[test]
    fn laid_finds_the_place_under_the_pointer_and_past_the_box() {
        let mut input = typed("abcdefgh ab abcdefgh");
        let mut laid = Laid { room: 9.0, cell: 10.0, em: 10.0, left: 100.0, top: 50.0, row_h: 20.0, shown: 2, ..Laid::default() };
        // A cell's widths: each letter 10 wide, so room for 9 is 90.
        laid.room = 90.0;
        laid.rows = rows(input.chars(), |c| laid.width(c), laid.room);
        laid.version = input.version();
        assert_eq!(laid.place_at(&input, (100.0 + 34.0, 55.0)), 3);
        assert_eq!(laid.place_at(&input, (100.0 + 36.0, 55.0)), 4);
        assert_eq!(laid.place_at(&input, (500.0, 75.0)), 11);
        // Below the rows in view, the row under them.
        assert_eq!(laid.place_at(&input, (100.0, 500.0)), 12);
        input.type_text("x");
        // Changed since the frame: laid out again.
        assert_eq!(laid.rows_of(&input).last().map(|r| r.end), Some(21));
    }

    #[test]
    fn rows_laid_out_for_one_input_are_never_taken_for_anothers() {
        let first = typed("does the page match the spec now?");
        let laid = Laid { version: first.version(), rows: rows(first.chars(), one, f32::INFINITY), ..Laid::default() };
        let second = typed("push both, then tag site as v2.1 and write a short note");
        assert_ne!(first.version(), second.version());
        assert_eq!(laid.rows_of(&second).last().map(|r| r.end), Some(second.chars().len()));
    }

    #[test]
    fn the_rows_in_view_follow_the_caret() {
        assert_eq!(Laid::follow(0, 0, 10, 3), 0);
        assert_eq!(Laid::follow(0, 5, 10, 3), 3);
        assert_eq!(Laid::follow(3, 2, 10, 3), 2);
        assert_eq!(Laid::follow(8, 9, 4, 3), 1);
    }
}
