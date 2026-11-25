# Files for the assistant (after 0.1.43)

Planned on the Mac on 2026-11-16 and built there the same day, on master
while 0.1.43 is worked on, so it goes with 0.1.43 once committed.
BACKLOG's "Files dropped on the assistant" points here. Keep this design up
to date as it changes; the As built part, at the end, says what differs.

## Context

Asked for (2026-11-16): a file, such as a document or a picture, dropped
on the assistant for it to use.

Today a file dropped anywhere on the window types its path at the prompt of
the pane in front (`WindowEvent::DroppedFile` in `app.rs`), in double quotes
where it has a space, whatever the shell. The assistant reads only UTF-8
text, through `read_file`, at most 200 KB (`tools.rs`). Nothing in `ai/`
sends a picture or a document.

What was decided:
- **Kinds:** pictures, PDFs, text files, and Word and the like (docx, xlsx,
  pptx).
- **Ways in:** dropping a file on the panel, pasting a picture, the
  assistant's own `read_file` reading pictures and documents, and an Attach
  button.
- **Where a drop goes:** where it lands. On the panel it goes to the
  assistant; on the terminal its path is typed, as now.
- **Secrets:** a file that looks like a secret is added, but its chip won't
  send it until the user confirms it with a click.

## What each kind becomes

The history keeps the user's side as Claude's blocks (`ai/mod.rs`), and a
conversation keeps its provider, so each file's blocks are made for the
provider in use. Before each file there's a line of text naming it, such as
"(shot.png, 1568 × 980, made smaller from 3136 × 1960)", so the model knows
what's what and the name needs no field of its own (Claude's API refuses
unknown fields).

| Kind | Claude | OpenAI (Responses) | Gemini |
|---|---|---|---|
| Picture | `image`, base64 | `input_image`, a `data:` URL | `inlineData` |
| PDF | `document`, base64, with its `title` | `input_file` (`filename`, `file_data`) | `inlineData` |
| Text file | a `text` block of its text, inside `<file name="…">` | `input_text`, the same | `text`, the same |
| docx, xlsx, pptx | its text, which Flow takes out | `input_file`, as it is | its text, which Flow takes out |

`claude.rs` passes blocks as they are. `openai.rs`'s `input` and
`gemini.rs`'s `contents` drop any block they don't know today; they're
taught these.

- **Pictures:**
  - Read by Flow's own decoders: PNG, JPEG, GIF and WebP (the first frame of
    one that moves), and BMP and TIFF (`iterm::decode_still`). Anything else
    (HEIC, AVIF) is refused, naming its kind, as `iterm::kind_of` does.
  - One whose longer side is over 1568 pixels (Claude's own recommended
    size, past which it shrinks them anyway) is made smaller and sent as a
    PNG, or as a JPEG where the PNG would pass 3.75 MB (Claude's 5 MB, once
    it's base64). The `image` crate's `imageops::resize` and its PNG and
    JPEG encoders are in the build already.
  - A picture already small enough, and in a kind all three providers take,
    goes as it is.
- **PDFs:**
  - Sent as they are, up to 32 MB (Claude's request limit).
  - Flow doesn't count pages. Past a provider's page limit (100 or so for
    Claude), the provider's refusal is shown as any error is.
- **Text files:**
  - Anything that's UTF-8, up to read_file's 200 KB. A longer one keeps its
    start and says it was cut.
  - Something that's neither text nor a kind above is refused: "Flow can't
    send this kind of file to the assistant."
- **Word and the like:** docx, xlsx and pptx are zip files of XML.
  - Flow takes their text out itself: a docx's paragraphs; an xlsx's
    sheets, a line per row, cells parted by tabs (its shared strings read);
    a pptx's slides, each under its number.
  - It uses a small reader of a zip's central directory, with
    `miniz_oxide`'s inflate and `roxmltree`. Both are in the lock already,
    and no zip crate is.
  - The text says what it lacks: "(text taken out of report.docx: its
    pictures and layout aren't here)".
  - OpenAI takes the file itself instead.
  - The binary .doc, .xls and .ppt are refused: "save it as .docx or PDF".

At most 10 files go with a message, 24 MB of them together, which is 32 MB
once base64.

## Ways in

- **Dropped on the panel** (where it lands, below): a chip for each file
  under the input, and the input gets the keys. Several dropped at once
  come one `DroppedFile` each, and each gets a chip.
- **Pasted:** ⌘V (Ctrl+Shift+V) in the panel with a picture on the
  clipboard and no text adds a picture called "Pasted picture N". It uses
  arboard's `get_image`, already built in, encoded as PNG as `write_png`
  does. Text wins where the clipboard holds both, since Excel and Numbers
  put a picture of the cells beside their text.
- **Attach** (a button at the input's left, `Hit::Attach`): the system's
  open panel through `rfd`, which doesn't wait (`AsyncFileDialog`, as a
  recording's save does). It allows several files at once, filtered to the
  kinds above, with `hotkey.dialog` held meanwhile.
- **`read_file`:** a picture, a PDF or a docx, xlsx or pptx the assistant
  reads goes to the model as a dropped one does.
  - Its tool result says what it read ("(shot.png follows: a picture,
    1568 × 980)"). The file's blocks go after the tool results in the same
    turn: after the `tool_result` blocks for Claude, as a user message after
    the `function_call_output`s for OpenAI, and after the
    `functionResponse` parts for Gemini.
  - The same approvals as now apply: guide mode asks, and so does a
    sensitive path.
  - The function's description in `tools.rs` says it reads them.

## Where a drop goes

winit 0.30 gives a drop no place: `DroppedFile` and `HoveredFile` carry a
path alone, on every system, and no `CursorMoved` comes during a drag, so
`App::mouse` is stale then. So Flow reads the pointer itself:

- **`Platform::pointer(&Window) -> Option<(f64, f64)>`**, in the window's
  pixels:
  - the Mac's `NSEvent::mouseLocation`, from the window's frame;
  - Windows' `GetCursorPos` and `ScreenToClient`;
  - X11's `QueryPointer`;
  - Wayland: none. winit 0.30 has no file drops there at all, so on
    Wayland the Attach button and pasting are the ways in.
- **While a file is over the window** (from `HoveredFile` until
  `DroppedFile` or `HoveredFileCancelled`), Flow reads the pointer each
  frame and lights the half under it:
  - "Drop here for the assistant" over the panel;
  - "Drop here to type its path" over the pane under it.
  With the panel closed, the room it would take at the window's right
  lights as the assistant's, and a drop there opens the panel with the
  file.
- **Dropped on a pane:** its path is typed at that pane's prompt, the pane
  under the pointer and not only the one in front. It's quoted for that
  pane's shell (`shell::quote`, which the `flow` command brings), and
  several paths are parted by spaces.
- **Where the pointer can't be read:** a drop goes to the assistant where
  the panel is open and has the keys, else to the pane in front.

## The chips

- A row under the input, wrapping. Each chip has:
  - a picture's thumbnail (`Canvas::image_over`, which About and the
    gallery draw with), or a mark for its kind;
  - its name, cut with …;
  - its size;
  - a ✕ (`Hit::AttachRemove(i)`).
- **A secret's chip** shows in orange: "Looks like a secret: click to
  send it" (`Hit::AttachConfirm(i)`). Enter won't send the message while
  one is unconfirmed, and the hint says why. It counts as a secret:
  - by its path, as `tools::is_sensitive_path` judges one;
  - or, for a text file, where it holds a private key's
    `-----BEGIN … PRIVATE KEY-----`.
  Every other file the user drops, pastes or attaches goes without asking,
  since they chose it.
- **Enter sends** the words and the files together, or the files with no
  words. A message waiting while the assistant works keeps its chips.
- **In the transcript,** the user's entry shows its words and then its
  files' names, each as a chip of its own, small and without its ✕.
- **Screen readers:** each chip is a button ("Remove shot.png", "Send
  key.pem anyway"), and so is Attach (`AccessView`'s `Panel` gains them).
- **Words:** `attach.*` in every locale. The assistant's help page gets a
  part on files.

## The conversation, the journal and tokens

- **A file stays in the conversation** until its turn is summed up.
  - The summing check (`SUM_AT`, a length of the conversation's JSON)
    counts a file by what it costs, not by its base64. A picture costs
    about w × h / 750 tokens. A PDF costs about 2,000 a page, its pages
    counted by `/Type /Page` in its bytes.
  - Summed up, a file leaves a line naming it ("(a picture, shot.png, was
    here)").
- **The journal** keeps a file's name only, never what's in it, beside the
  words of the ask (`journal_ask`). The words are kept as now, and
  `looks_secret` as now.
- **Tokens** are counted from what the API reports, as now, and the cap is
  checked before sending as now. Files make a message dearer, which the
  meter shows once it's answered.

## Files

New:
- `core/src/ai/files.rs`: `Attached` (its name, kind, bytes, size, and
  whether it's a secret confirmed), reading a file into one (the decoding,
  the shrinking, the refusals), and the blocks for each provider.
- `core/src/ai/office.rs`: the zip's central directory, inflate, and the
  text of a docx, xlsx and pptx.

Changed:
- `ai/mod.rs`: `Command::Ask` carries the files, the worker puts their
  blocks in the user's turn, the summing counts them, `fold_results`.
- `ai/openai.rs`, `ai/gemini.rs`: the new blocks.
- `ai/tools.rs`: `read_file` and its description.
- `app.rs`: the drop, hover, paste, Attach and send.
- `ui.rs`: the chips, the drop's light, the transcript's chips.
- `access.rs`, `help.rs`, `platform.rs` (`pointer`) and each OS crate's
  `pointer`.
- en and ja.

## Tests

- Each kind read into an `Attached`:
  - a large picture made smaller, the PNG giving way to a JPEG past
    3.75 MB, a small one kept as it is;
  - a GIF's first frame, and HEIC refused;
  - a PDF's pages counted;
  - text cut at 200 KB;
  - a docx, an xlsx and a pptx made here in the tests and their text read
    back.
- Each provider's blocks for each kind, and `read_file`'s following its
  results, against `mock_api` as the agent's tests are.
- A secret: by path and by a key's header, and the chip holding Enter
  back.
- The summing counting a picture by its tokens, not its base64.
- Where a drop goes, as a pure function of the pointer, the panel and the
  panes.
- By hand on each system: dropping one file and several, on the panel and
  on a pane, with the panel closed; pasting a screenshot; Attach; asking
  the assistant to look at a picture on disk; with each provider.

## Not in it

- Dragging a file out of Flow, or a picture from the transcript.
- Sound and video files (Gemini alone takes them).
- Uploading through a provider's own files API, for what's past the
  inline limits.
- Showing a picture whole in the panel. The chip's thumbnail is all, and
  Flow's terminal shows pictures already (`imgcat`, kitty's).

## As built (the Mac, 2026-11-16)

Built as planned, with these differences and details:

- **Reading** runs on a `flow-attach` thread for each file, its chip
  saying "reading…" until `UserEvent::FileRead` comes back. A file that
  won't go is a toast in the user's words; for `read_file`, the same
  refusal in English goes to the model (`files::Refused`).
- **Pictures:** PNG, JPEG, BMP and TIFF are read by the `image` crate, a
  GIF's first frame by `gif`, a WebP's by `image-webp`. BMP and TIFF
  always go as a PNG (no provider takes them). The JPEG that stands in for
  a PNG past 3.75 MB starts at quality 85 and drops by 15 to no less than
  40. Each picture gets a 96-pixel thumbnail for its chip.
- **Pasting:** a picture is taken only where the clipboard has no text,
  and is named "Pasted picture N".
- **Attach** uses the system's panel that waits (`rfd::FileDialog`
  through `in_dialog`), as loading a preset does, several files at once,
  starting in the pane's folder.
- **A message of files alone** (Enter with nothing typed) asks the model
  with "(The user gave these files, with no words.)", and the panel shows
  only the files' line.
- **The transcript** shows the files sent as a line under the message, its
  names after a `+` (`Entry::Files`), not a chip each.
- **Summing up:** a file in the turns summed up goes with them, the
  summary standing for it; no line naming each is left.
- **A drop on a pane** types the path and a space after it, so several
  dropped at once are parted.
- **Tried live on the Mac** with Claude: drops from Finder on the panel
  and on a pane, the light where they'd land, a secret confirmed and
  taken off, pasting, Attach, five files sent and each read right, and
  `read_file` on a picture. OpenAI and Gemini with files are left to
  Flow's users to try (decided 2026-11-16).
- **On Linux:** `Platform::pointer`
  (QueryPointer on the X11 window) built as written, and every test
  above passes there; the snapshots draw as the Mac's. GNOME 50 has no
  Xorg session, so X11 is Flow under XWayland, dragged onto from
  Wayland's Files; winit 0.30 gives Wayland itself no file drops. Attach
  is `rfd`'s portal, GNOME's file chooser. Left to try live: drags,
  pasting, Attach, a secret's chip, sending to each provider and
  `read_file`.
- **On Windows:** `Platform::pointer` built as
  written; core's tests pass as on the Mac; the test files read through
  a path with a space, a large PNG made 1568 × 1045, both secrets found;
  Flow per-monitor DPI aware, so the pointer's pixels are the client's
  physical ones, as winit's are. Left to try live in a test copy, drags
  from Explorer among them.

Tests: `ai::files` (each kind; shrinking; the JPEG; a GIF, a BMP, HEIC
refused; a PDF's pages; text cut; a key; an Office file for each
provider; the weighing), `ai::office` (a docx, an xlsx, a pptx made in the
tests), each provider's conversion, `ai::loop_tests`' files after the ask
and after a turn's results, and `app::attach`'s `drop_place`.

