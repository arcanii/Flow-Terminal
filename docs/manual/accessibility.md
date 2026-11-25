[← Flow's manual](../../README.md#documentation)

# Accessibility

Screen readers read Flow: VoiceOver on the Mac, Narrator (and other UI
Automation readers) on Windows, and Orca on Linux. Flow draws everything
itself, so it tells the reader what's on screen through AccessKit: the
window, named for the tab in front; the title bar's buttons and the tabs,
the one in front selected; the terminal's rows in view as text, the cursor
as the caret, each pane of a split tab a terminal of its own; the
assistant's conversation and its input, its last reply read out once it's
done; Settings as a dialog, its sections headings that say whether they're
open, a switch a check box, a slider with its value and its range, a choice
with its words, its buttons and its text rows; the find bar, the command
palette, the command history, help, About, the API key's dialog and Flow's
questions as dialogs; and toasts, read out as they come. What has the keys
has the reader's focus, and moves as the keys move it. The reader's own
commands work as the mouse and keys do: pressing a button clicks it, a
slider steps or takes a value, and choosing an item in the palette, the
history, help's links or the new-tab picker chooses it; typing stays the
keyboard's. Flow tells a reader only once one asks, and only when what's
shown has changed, so without one it costs nothing.
