# The assistant's input

Asked for on 2026-11-26: the assistant panel's input needed a better
history. You couldn't select in it, cut or paste a part of it, or click
where you wanted to edit, and a long message ran off the left of its one
line, so you moved back along it to see the start.

## What it was

- One line, scrolled sideways to keep the caret in view (`ui::input_line`).
  A paste had its line breaks made spaces.
- A caret alone. ←, →, Home, End and the Mac's ⌘←, ⌘→ and ⌘⌫ moved or
  deleted; nothing selected, and no key cut.
- A click on it only gave the panel the keys; the caret stayed where it was.
- ↑ and ↓ brought back the messages sent since Flow started. ↑ threw
  away what was being typed, and ↓ past the newest emptied the input.

## What it is

The input is its own type, `panel_input::Input` (`core/src/panel_input.rs`):
the text as characters, the caret, the selection's other end (`anchor`),
and its undo. It's tested without drawing, as Settings' `TextField` is.

### A box of lines

- The text wraps onto as many rows as it needs, up to a third of the
  panel's height, and scrolls up and down past that to keep the caret's
  row in view. So the whole of a message shows while it's short enough,
  and the conversation above gives up the room.
- Rows break after a space, between Chinese and Japanese characters, or,
  in a word longer than the row, where it runs out of room
  (`panel_input::rows`). A line break ends a row.
- Widths are worked out as the interface's text is drawn (`text::advance`,
  which `Canvas` goes by too), so the keys and the mouse know the rows
  between frames.
- **⇧Enter** starts a new line; Enter sends, as before. A paste keeps its
  line breaks, and the message goes as written.
- Attach stays at the input's right end, on its last row.

### Selecting, cutting and pasting

- **Keys.** ⇧ with any move selects: ⇧← ⇧→ a letter, ⇧↑ ⇧↓ a row, with
  ⌥ on the Mac or Ctrl elsewhere a word, Home and End (⌘← ⌘→ on the Mac)
  to the row's ends, and ⌘↑ ⌘↓ on the Mac or Ctrl+Home and Ctrl+End
  elsewhere to the text's. A move without ⇧ ends the selection at the
  side it moves to.
- **The mouse.** A click puts the caret there, ⇧-click selects to there,
  a double click takes a word and a triple click its line, and a drag
  selects, the rows scrolling past the box's top or foot.
- **Select All, Copy, Cut, Paste.** While the panel has the keys:
  - the Mac's ⌘A (Edit → Select All) selects the input, ⌘C (Edit → Copy)
    copies its selection, or else the conversation's, ⌘X cuts it and ⌘V
    pastes over it.
  - elsewhere Ctrl+A, Ctrl+C, Ctrl+X and Ctrl+V do, since the panel isn't
    a shell; Ctrl+Shift+C and Ctrl+Shift+V, Ctrl+Insert, Shift+Insert and
    Shift+Delete still work.
  - typing replaces the selection, and ⌫ or ⌦ delete it.
- **Undo.** ⌘Z and ⇧⌘Z on the Mac, Ctrl+Z and Ctrl+Y (or Ctrl+Shift+Z)
  elsewhere. A run of typing, or of deleting, comes back as one step;
  a paste, a cut and a message brought back are steps of their own.
- **Deleting.** ⌥⌫ (Ctrl+Backspace) a word back, ⌥⌦ (Ctrl+Delete) a word
  on, ⌘⌫ to the row's start. The Mac's Ctrl+A and Ctrl+E go to the row's
  ends, as its other text fields do.
- **Esc** first lets go of a selection; then it does what it did.
- **Linux.** A selection made in the input becomes the primary selection,
  and a middle click pastes it where it's clicked, as in the terminal.

None of these are new commands in `keys::TABLE`: they're the input's
own, as ←, → and ⌘⌫ were. ⌘X and ⌘Z reach it because no menu item takes
them. Flow's keys still come first, so one bound to ⌘X in `[keys]` wins.

### The history

The messages sent are a `panel_input::Recall`:
- **↑ on the first row** brings back the message before, and **↓ on the
  last row** the one after. Elsewhere they move between rows.
- **What was being typed is kept:** ↓ past the newest gives it back.
- **An edit is kept** on the message it was made to, while you move
  through the others, until you send (readline's way). What's sent goes
  in as the newest; the one it came from stays as it was.
- The last `Recall::MOST` (200), none twice in a row. Slash commands
  aren't kept, as before.
- **Kept across restarts** (the user's word, 2026-11-26: "Keep across
  restarts") while Settings' **Memory** is on, which already keeps what
  you asked: each message as sent goes into the assistant's journal
  (`assistant.sqlite3`), in a table of its own, `sent` (`Journal::keep_sent`,
  the newest `SENT_KEPT`), and Flow starts with them (`Journal::sent`,
  `Recall::from_past`). Not the journal's asks, which carry the names of
  files attached after them, and Ask why's and Carry on's words. One that
  `history::looks_secret` flags is kept only until Flow quits. Remembered's
  Clear forgets them, and the ones of this run. Snapshots and recordings
  keep an in-memory journal, so they read and keep none. They never go to
  the model.

### Readers

The input's field carries its selection to screen readers
(`access::Field::anchor`, AccessKit's `TextSelection`).

## Limits

- The input takes at most `Input::MOST` (100,000) characters; a paste
  past that is cut, and a toast says so. Files go with Attach.
- The wheel over the input scrolls the conversation, as before; the
  input's rows follow the caret.
