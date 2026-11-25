---
name: sdk-programs
description: Write a program that draws with Flow: the liquid's effects and sounds (OSC 7731), pictures (kitty, sixel, iTerm2), 3D models (Ratty), Flow's settings (OSC 7732), with Flow's SDK in Rust, C or Python or with raw escapes. Use when a script, tool or game should show more than text in Flow.
---

# Programs that use what Flow draws

Flow reads escapes a program writes: effects on the liquid and sounds (OSC 7731), pictures (kitty's graphics protocol, sixel, iTerm2's inline images), 3D models (the Ratty protocol) and its own settings (OSC 7732). Other terminals ignore what they don't know, so a program that asks first and falls back degrades quietly.

## Choose a way

- **Raw escapes** for a one-off from the shell, a short script, or a language the SDK doesn't cover. `printf` is enough for an effect, a sound, or a picture from a file.
- **Flow's SDK** for a real program. It asks the terminal what it can do, falls back quietly, compresses and chunks pictures, writes their placeholder cells, builds meshes and whole 3D scenes, keeps under Flow's allowance of effects, and on Windows goes down Flow's pipe by itself. Rust: the `flow-sdk` crate. C: the `flow-sdk-c` library and its header `flow_sdk.h`. Python: the `flow_sdk` package, pure Python over the C library through ctypes (Python 3.8 or later).
- Never guess an SDK function or a protocol key. The SDK's README (https://github.com/arcanii/Flow-SDK#readme) explains each protocol as Flow speaks it. Its `examples/` (Rust), `c/examples/` and `python/examples/` are the patterns to copy. Where this page and the README differ, the README wins.

## Getting the SDK

For C and Python, in a Flow tab:

```bash
flowapp install flow-sdk
```

It prints where its library (`lib/`), C header (`include/`) and Python package (`python/`, with the library inside it) are, and puts `flow-demo` on the PATH. If it's installed already it only says so; the folders are then in `apps/flow-sdk/<version>/` beside Flow's `config.toml`, and `flowapp list` gives the version. Rust takes the crate by a tag of its repository (v0.1.3 is the newest as this is written; `git ls-remote --tags https://github.com/arcanii/Flow-SDK` lists them). The `audio` feature adds tones, tunes and songs of the program's own; `theme` writes theme packages.

```toml
[dependencies]
flow-sdk = { git = "https://github.com/arcanii/Flow-SDK", tag = "v0.1.3" }
```

## Is it Flow, and what can it show?

- Flow's tabs have `TERM_PROGRAM=flow-terminal` and `TERM_PROGRAM_VERSION`: `flow_sdk::in_flow()`, C's `flow_in_flow()`, Python's `flow.in_flow()`. Over ssh the variable doesn't arrive, but the answers below do.
- Questions: XTVERSION `CSI > 0 q` answers `ESC P >|Flow <version> ESC \`. kitty's query `ESC _Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA ESC \` answers `ESC _Gi=31;OK ESC \` (pictures). Ratty's `ESC _ratty;g;s ESC \` answers `ESC _ratty;g;s;v=1;fmt=obj|glb;… ESC \` (models). `CSI 16 t` gives a cell's pixels, `CSI 14 t` the text area's. Put DA1 (`CSI c`) last: every terminal answers it, so the wait ends at once.
- Ask with raw mode on and before anything reads keys, or the terminal echoes the answers and the key reader eats them. The SDK's `Query` (`flow_detect` in C, `flow.detect` in Python) does all of this and waits a second at most.

## Smallest programs

Python, with the SDK installed. A ripple and a chime in the middle of the screen:

```python
import flow_sdk as flow

with flow.screen():                        # raw keys on the alternate screen
    terminal = flow.detect(version=True)
    size = flow.size()
    if terminal.caps.flow and size:
        at = (size.cols // 2, size.rows // 2)
        terminal.send_effects(flow.Effect.ripple(at).size(10).sound("chime").escape())
        flow.next_event(timeout=2)         # a key, or two seconds
```

```bash
PYTHONPATH="<the python folder flowapp named>" python3 ripple.py
```

`python3 -I` ignores `PYTHONPATH`; with `-I`, put that folder on `sys.path` in the script instead. A picture below the prompt: `with flow.raw_mode(): terminal = flow.detect(pictures=True)`, then, if `terminal.caps.pictures`, `flow.picture.show(terminal, flow.picture.first_id(), flow.Image.png(data), cols, rows)`. On bare cells (Windows) `show` refuses a PNG: give it `flow.Image.rgba(width, height, pixels)`. A model: `flow.model.register(1, glb, "glb")`, then `flow.model.place(1, row, col, cols, rows, flow.Look(animate=True))`, and `flow.model.delete(1)` on the way out; `python/examples/hello.py` builds one with `flow.Mesh`.

Rust, the same with a burst:

```rust
use flow_sdk::fx::{Effect, Sound};
use flow_sdk::{term, Query};

fn main() -> std::io::Result<()> {
    term::leave_on_panic();
    term::enter(false)?;                   // raw keys on the alternate screen; true adds the mouse
    let mut terminal = Query::new().version().ask();
    if terminal.caps.flow {
        let size = term::size()?;
        let middle = (size.cols / 2, size.rows / 2);
        terminal.send_effects(Effect::burst(middle).color(0x7ce7ff).size(6.0).sound(Sound::Chime).escape().as_bytes())?;
        std::thread::sleep(std::time::Duration::from_secs(2));
    }
    term::leave();
    Ok(())
}
```

Pictures in Rust: `terminal.send(picture::upload(id, &image, PLACEMENT, cols, rows).as_bytes())`, then `picture::placeholder_row` for each row; `examples/picture.rs` does it, bare cells too. Scenes each frame: `scene`, `stage` and `examples/scene.rs`.

C, linked against the installed library:

```c
#include <string.h>
#include "flow_sdk.h"

int main(void) {
    if (flow_abi_version() != FLOW_SDK_ABI) return 1;   /* header and library must match */
    flow_term_enter(false);
    FlowTerminal *terminal = flow_detect(FLOW_ASK_VERSION, 0);
    FlowCaps caps = {0};
    flow_terminal_caps(terminal, &caps);
    FlowSize size = {0};
    if (caps.flow && flow_term_size(&size) == 0) {
        FlowEffect *ripple = flow_effect_new(FLOW_FX_RIPPLE, size.cols / 2, size.rows / 2);
        flow_effect_sound(ripple, FLOW_SOUND_CHIME);
        char *escape = flow_effect_escape(ripple);
        flow_terminal_send_effects(terminal, escape, strlen(escape));
        flow_free(escape);                  /* strings go to flow_free, handles to their own _free */
        flow_effect_free(ripple);
        FlowEvent event;
        flow_next_event(2000, &event);
    }
    flow_term_leave();
    flow_terminal_free(terminal);
    return 0;
}
```

```bash
cc ripple.c -I "<include folder>" -L "<lib folder>" -lflow_sdk_c -Wl,-rpath,"<lib folder>" -o ripple
```

That's for macOS and Linux. On Windows the folder holds `flow_sdk_c.dll`, its import library `flow_sdk_c.dll.lib` and a static `flow_sdk_c.lib`; check the SDK's README before giving a Windows build line.

## The escapes

Cells count from the screen's top-left, from 0. In `printf`, ESC is `\033`, BEL `\007`, and ST is `ESC \`.

- **Effect:** `ESC ] 7731 ; fx=<bolt|burst|ripple|stir> ; at=<col>,<row> [; to=<col>,<row>] [; color=<rrggbb>] [; dir=<dx>,<dy>] [; size=<columns>] [; strength=<0..1>] [; sound=<name>|none] [; volume=<0..1>] BEL`. A bolt needs `to`, a stir `dir` (y down). `size` is 0.5 to 40 columns, `strength` 0.5 unless given, and `volume` lowers the sound alone. Flow ignores unknown keys, and the whole request where the kind is unknown or lacks what it needs.
- **Sound alone:** `ESC ] 7731 ; fx=sound ; sound=<name> [; at=<col>,<row>] [; strength=<0..1>] [; volume=<0..1>] BEL`. Names: drop, splash, thunder, whoosh, chime, low, ding, thud, click, boing, bell, patter, bubble, clack (the SDK's `Sound` has the first eleven).
- **Notes (DECPS):** `ESC [ <volume 0..7> ; <length in 32nds of a second> ; <note> [; <note> …] , ~`, notes 1 (C5) to 25 (C7), 0 a rest.
- **Tint:** `ESC ] 7731 ; fx=tint ; color=<rrggbb> ; strength=<0..1> BEL`. It holds only while the program keeps the alternate screen; `strength=0`, or leaving that screen, washes it off. Send it when it changes, not each frame.
- **kitty picture from a file**, at the cursor: `printf '\033_Ga=T,f=100,t=f,c=20,r=8,q=2;%s\033\\' "$(printf '%s' "$PWD/a.png" | base64 | tr -d '\n')"`. The payload is the path in base64, read on the computer Flow runs on. `q=2` keeps Flow from answering into the shell.
- **kitty picture sent inline**, shown by placeholder cells (the SDK's way): `ESC _G a=T,U=1,f=<24|32|100>,o=z,s=<width>,v=<height>,i=<id>,p=<placement>,c=<cols>,r=<rows>,q=2,m=<0|1> ; <base64> ESC \`. `f` is 24 for RGB, 32 for RGBA, 100 for PNG; raw pixels go zlib-compressed (`o=z`). Over 4096 base64 bytes, send chunks: the first with every key and `m=1`, the rest with `m` and `q` alone, the last `m=0`. The cells are U+10EEEE in the id's color with kitty's combining marks: let the SDK write them (`placeholder_row`). `a=d,d=I,i=<id>` forgets a picture; `a=d,d=a` takes what the screen shows.
- **iTerm2 image:** `printf '\033]1337;File=inline=1;width=20:%s\007' "$(base64 < a.png | tr -d '\n')"`. `inline=1` is required. `width` and `height` take cells, `px`, `%` or `auto`; `preserveAspectRatio=0` stretches, and `doNotMoveCursor=1` leaves the cursor. It takes PNG, JPEG, BMP, TIFF, GIF and WebP.
- **Sixel:** `ESC P <P1> ; <P2> ; <P3> q <data> ESC \`. Use a tool rather than writing one: `img2sixel a.png`, `chafa -f sixels a.png`.
- **3D model:** `ESC _ratty;g;<verb>;<key=value>… ESC \`. Register `r;id=<id>;fmt=<obj|glb>;source=payload;more=<0|1>;name=<file>;<base64>`, or by file `r;id=<id>;fmt=<obj|glb>;path=<path>`, which makes Flow ask the user first. Place `p;id=<id>;row=<row>;col=<col>;w=<cols>;h=<rows>[;<look>]`, centered on that cell; placing again moves it. Update `u;id=<id>;<look>`, delete `d[;id=<id>]`. The look: `animate=1`, `rx`, `ry`, `rz` (degrees), `scale`, `sx`, `sy`, `sz`, `px`, `py` (pixels, y up), `color` (an OBJ's), `brightness`, `depth`, `clip=<n>` (a glTF's animation) and `clicks=1`, which has Flow answer `ESC _ratty;g;click;id=<id>;button=<left|middle|right>;x=<x>;y=<y>;z=<z>[;node=<n>] ESC \`. Placements can't overlap, so a moving scene is one model registered again under the same id each frame (the SDK's `stage`).
- **Settings:** `ESC ] 7732 ; <base64 of JSON> BEL`, answered the same way. The request is `{"do": "list"|"describe"|"get"|"set"|"reset"|"reset-terminal"|"restart", "key": "fx.style", "value": …, "pass": "<FLOW_TAB_KEY>"}`; the answer is `{"ok": true, …}` or `{"ok": false, "error": …}`. `pass` is the `FLOW_TAB_KEY` environment variable, the tab's own secret: from Flow 0.1.46 a request without it, or with another pane's, gets no answer at all, not even a refusal, since anything printed in a tab (a file, a remote host) could ask otherwise and the answer would be typed into the shell's input. So with no `FLOW_TAB_KEY`, say so without asking, and where nothing answers, say the key may be another pane's (or tmux in the way), as `flow-settings` does. The answer arrives as keyboard input, so from a shell use `flow-settings` (the skill `flow-skills:flow-settings`), which sends the pass itself, and from a program the SDK's raw ask with the pass put in the request: `flow_sdk::settings::ask`, C's `flow_settings_ask`, Python's `flow_sdk.settings.ask({"do": "get", "key": "fx.style", "pass": os.environ.get("FLOW_TAB_KEY", "")})`. Python's `flow_sdk.settings.get`, `set`, `list`, `reset`, `reset_terminal` and `restart` send no pass, so Flow doesn't answer them (`NoAnswer`, after the wait) until a release of the SDK adds it: don't use them. Ask only in Flow: another terminal stays silent and the whole wait passes (about a second, on the Mac and Linux). Flow decides what a program may change, never the assistant's settings or the user's own themes, and shows a toast for each change.

Limits worth knowing: pictures up to 10,000 pixels a side and 320 MB a tab; models up to 64 MB and 2 million triangles, 256 a tab, 64 shown, each at most 200 cells and 4096 pixels a side; effects about six a second after a burst of eight.

## Windows: Flow's pipe

ConPTY, between Flow and the program, drops kitty's and Ratty's APCs, OSC 7731 and sixels, and the answers to the questions never come back. So each Flow tab names a pipe in `FLOW_IMAGE_PIPE`. Flow reads what's written there as if written to the terminal: pictures, models, effects, sounds (named and DECPS), settings, sixels and the questions. The answers come back up the pipe, DA1's last.

- The SDK does this for you. `Query::ask` opens the pipe, and `caps.bare` and `terminal.piped()` (Python's `caps.piped`) say so. `terminal.send` writes pictures and models there and waits for DA1's answer, so they're in before the cells; `send_effects` writes effects. `Query::without_pipe` (Python `detect(pipe=False)`) keeps off it.
- The cells still go through the terminal, bare: U+10EEEE in the id's color and nothing else. Flow takes a bare cell's row as the first, so each row of a taller picture is a picture of its own (`Image::strip`, `picture::bare_row`; Python's `picture.show` does both). Send raw pixels there, not a PNG.
- Send a model after the screen it sits on. A repaint (after a resize, say) can still take it away.
- iTerm2's images pass through ConPTY, so they work from any shell with no pipe.
- From a shell: in cmd, `img2sixel a.png > %FLOW_IMAGE_PIPE%`; in PowerShell, `[IO.File]::WriteAllBytes($env:FLOW_IMAGE_PIPE, [IO.File]::ReadAllBytes('a.six'))`; in Git Bash, `cmd //c 'type a.six > %FLOW_IMAGE_PIPE%'`. To write more text after a sixel, send DA1 down the pipe and wait for its answer first.
- A program opening the pipe itself retries while it's busy (Windows error 231), as the SDK does.

## Testing in a Flow tab

- Run it in a Flow tab, which `run_command` does. Outside Flow the program should say what's missing and stop, as the SDK's examples do.
- Make a test end by itself (a key or a few seconds, as `next_event(timeout=…)` does). A program waiting for a key holds `run_command` until the user presses one.
- You can't see pictures, models or the liquid: `read_terminal` reads text only. Ask the user what they saw.
- Effects show only from the tab in front, at cells on the screen shown, and not with the liquid style None. Sounds play only with Sound on, which is off at first. Before blaming the code, read `flow-settings get` for `fx.liquid`, `fx.style`, `fx.reactions`, `fx.program_effects`, `sound.enabled`, `sound.effects` and `sound.programs`. Ask the user before turning any of them on.
- Known-good checks: `flow-demo` (alone it lists its demos) after `flowapp install flow-sdk`; `kitten icat`, `viu`, `img2sixel`, `chafa` or `imgcat` for pictures. In a checkout of the SDK, `cargo run -p flow-sdk --example effect` (or `picture`, `scene`).
- `flow_help` has pages on the model question (`model`), apps (`apps`) and Settings (`settings`).
- Be tidy: send effects only in Flow, pick picture ids unlikely to clash (`picture::Ids`, Python's `picture.first_id`), delete the pictures and models you placed, wash off a tint, and hand back the screen and raw mode on the way out, a panic's too (`term::leave_on_panic`).
