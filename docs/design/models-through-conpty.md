# 3D models through ConPTY (0.1.48, Windows)

Planned on 2026-11-27 for Windows' 0.1.48 (BACKLOG's 0.1.48, "3D models:
more of glTF, and Windows' checks"). A program places a model with the
Ratty protocol; on the Mac and Linux it all goes through the terminal, but
on Windows ConPTY sits between Flow and the program, and three things don't
work yet:

- a model's cells are Flow's own, so ConPTY's repaints go over them;
- `show_model`, the example that places a model by hand, needs a Unix tty;
- whether a click's answer reaches the program through ConPTY was untried.

## What's there already

- ConPTY drops the protocol's APCs, so on Windows a program sends them down
  the tab's pipe for images (`FLOW_IMAGE_PIPE`, `session::serve_pipe`),
  which answers `s` there and places models through `handle_model` as the
  terminal does. A placement down the pipe waits for ConPTY to paint what
  the program wrote before it (`wait_for_paint`).
- `place_model` writes a model's cells into the grid itself (placeholder
  cells, as pictures are), and records the placement (`model::Showing`);
  placing it again, or deleting it, clears its cells (`clear_model`) and
  forgets its picture (`ImageStore::forget_own`).
- The startup logo is such cells too, and on Windows is kept through
  ConPTY's repaints (`Banner`): ConPTY sends each repaint as a frame
  between `?25l` and `?25h`, and `advance` watches for one being due
  (leaving the alternate screen, `?1049l`, or a resize, `repaint_due`),
  notes the logo's rows then, and puts back any of them the frame left
  blank (`Emulator::frame`).
- A click on a model that asked for them (`clicks=1`) is answered by
  writing `ESC _ ratty;g;click;… ESC \` to the pane's input
  (`Session::click_model`, `report`).

## 1. A model kept through ConPTY's repaints

**What it does.** The `Emulator` keeps the image ids of the models placed
in it (`place_model` adds one, `clear_model` takes it away), and on
Windows (`Emulator::conpty`) `advance` follows ConPTY's frames while there
are any, as it does for the logo. When a repaint is due, `repaint_due`
notes, besides the logo's rows, every cell on the screen in use that
shows one of those models (its line, column and cell), and which screen
it is. As the repaint's frame ends, `frame` puts back each noted cell
that the frame left blank, on the same screen, and only for a model still
placed: one placed again or deleted meanwhile has a new image or none, so
no old copy comes back.

**Why this way.** It's the logo's way, cell by cell rather than row by
row: a model sits among a program's text (a game's board, a prompt's
line), so a row is never the model's alone, and only the cells the
repaint blanked are the model's to put back. Noting
them when the repaint is due, after Flow's own resize has rewrapped the
screen from the top as ConPTY does (`resize_from_top`), finds them where
ConPTY will paint. Both screens count: a game on the alternate screen that
doesn't place its models again after a resize keeps them, and one that
does clears the old ones as it places the new.

**What it leaves.**
- A model below the cursor, in rows ConPTY holds blank, isn't kept:
  ConPTY paints them blank again from the cursor's line down whenever a
  line there wraps or the screen scrolls, with no repaint due (found
  through a real ConPTY on 2026-11-27, a long command typed at the prompt
  above such a model). So a program makes its rows first and places the
  model in them, as `show_model` does (2); a game on the alternate screen
  places its models over the board it has drawn.
- A clear (`cls`, `Clear-Host`) has no repaint frame and takes the model
  away, as it does the logo and as it should.
- A repaint that moves lines (a narrowing resize rewrapping a line above
  the model differently from Flow's own rewrap) puts the model back where
  Flow's rewrap left it, which may be a row off from ConPTY's text.
- A frame that writes text where the model was isn't blank there, so the
  text wins, as it should.

## 2. `show_model` down the pipe

**What it does.** On Windows, `examples/show_model.rs` goes through
Flow-SDK's `Pipe` (`flow_sdk::pipe`, which the root package already takes)
instead of `/dev/tty` and `stty`:
- it asks `s` down the pipe (`Pipe::ask`, with DA1 after it) and stops
  where Flow doesn't answer it;
- it makes room as before, by printing new lines through ConPTY, and finds
  the cursor's row from the console (crossterm's `cursor::position`, the
  root package's already), which ConPTY keeps as Flow's screen shows it;
- it sends the file's chunks and the placement down the pipe, the
  placement last (`Pipe::send_and_wait`), so Flow places it once ConPTY
  has painted the new lines;
- with `--clicks`, it puts the console in VT-input mode
  (`ENABLE_VIRTUAL_TERMINAL_INPUT`, line input, echo and processed input
  off) while it reads the answers, and puts the mode back after: three of
  kernel32's calls (`GetStdHandle`, `GetConsoleMode`, `SetConsoleMode`)
  declared in the example itself, as `macos/src/hotkey.rs` declares
  Carbon's, since the workspace's `windows-sys` doesn't carry the console's
  feature and an example shouldn't add one for every crate.

Elsewhere it's as it was. Its doc comment and the manual's images page say
it runs on Windows too.

**Why this way.** It's how Flow-SDK's programs (the games, the demos)
already send models on Windows, so the example tries the same path they
take.

## 3. Clicks answered through ConPTY

**Found on 2026-11-27** by a probe through a real ConPTY (PowerShell
running a program that reads its console): Flow's answer, written to the
pane's input as it is now, reaches a program whose console is in VT-input
mode whole (`ESC _ ratty;g;click;id=5;button=left;… ESC \`); in plain raw
mode ConPTY swallows the APC and only a stray `\` from its end gets
through.

**What it does.** Nothing changes in Flow: the answer goes as it does. A
program on Windows that asks for clicks reads them in VT-input mode, which
`show_model` does (2), and the manual's images page says so. Flow-SDK
reads no clicks, so it needs no new tag.

**What it leaves.** A program in plain raw mode on Windows (one reading
key events, as crossterm's do) never sees a click, and gets a `\`.

## How it's tested

- **Core, on every system:** a model placed in a ConPTY emulator comes back
  through a repaint's frame after `?1049l` and after a resize, cell by cell
  beside text a frame writes; a model placed again or deleted before the
  frame ends doesn't come back; the logo's own test stands.
- **Windows, through a real ConPTY** (`windows/tests/models.rs`, BusyBox
  tabs): a model placed down the pipe in rows made above the prompt
  outlasts a full-screen program and two resizes, and `clear` takes it
  away, as `the_startup_logo_outlasts_full_screen_programs_and_resizes`
  holds for the logo; and Flow's answer to a click, written to the tab's
  input as `Session::click_model` writes it (core's tests hold what it
  says), reaches a program reading its console in VT-input mode whole,
  the test binary run in the tab as that program.
- **By hand, in a test copy:** `show_model` with the forest, the fox and
  the anisotropy spheres from `core/src/model/testdata`, `--clicks`, a
  resize and a full-screen program over them.
- `cargo clippy --workspace --tests` adds nothing. The core change builds on
  the Mac and Linux but does nothing there, since only ConPTY repaints.
