[← Flow's manual](../../README.md#documentation)

# Building for Flow: the SDK

Any console program runs in Flow. For one that wants what Flow adds,
Flow's SDK, [Flow-SDK](https://github.com/arcanii/Flow-SDK) (the `flow-sdk` crate, Rust, licensed
MIT or Apache-2.0), speaks it, with no part of Flow in it so it can be used
on its own:

- **What the terminal can do**, asked before the program reads its keys:
  pictures (kitty's protocol), 3D models (the Ratty protocol), a cell's size
  in pixels, its background color (and whether that's light), whether it's
  Flow; and quiet fallbacks where it can't.
- **Pictures** with kitty's protocol: sending them, placing them by
  placeholder cells, and on Windows down Flow's pipe for images.
- **3D:** models registered, placed, changed and deleted; meshes made in code
  (`mesh`) for the terminal to turn and light; and whole scenes worked out
  each frame with a camera and the vector maths for it (`scene`, `stage`),
  as the demos do.
- **A screen of cells** (`screen`) drawn whole each frame and sent as its
  difference from the last, with pictures among the letters (bare on
  Windows), on a ground of the program's own where it wants none of it left
  to the terminal's background, as both games draw theirs.
- **The liquid and sound:** OSC 7731's effects with their sounds, a queue of
  them waiting for their moment, the glass's wash, Flow's named sounds, and
  DECPS notes.
- **Audio of the program's own**, behind the `audio` feature: a small mixer,
  tones and tunes, and songs for a small band (a lute, a recorder, a drone,
  a tabor) faded one into the next, on macOS, Windows and Linux.
- **Input:** keys and the mouse as Flow reports them, through crossterm.
- **Flow's settings** seen and changed (`settings`, OSC 7732), as
  `flow-settings` does. From Flow 0.1.46 each request must carry the
  tab's own secret, the `FLOW_TAB_KEY` variable, as its `pass`
  ([why](from-the-shell.md#settings-from-the-shell)). From the SDK's
  0.1.4, `ask` (Rust's `settings::ask`, C's `flow_settings_ask`,
  Python's `settings.ask`) puts it in from the variable where a request
  has none. Without the variable, as under `sudo` or over ssh, Python's
  `list`, `get`, `set`, `reset`, `reset_terminal` and `restart` raise
  `NoKey` at once and ask nothing. Flow answers a request without the
  secret not at all, not even to refuse it, so a program whose ask comes
  back with nothing should say the secret may be missing or another
  pane's, as `flow-settings` does. An SDK before 0.1.4 sends no secret,
  so a program built on one gets no answer until it's built again.

[Its README](https://github.com/arcanii/Flow-SDK#readme) explains each protocol as Flow speaks it, with
the escapes, and its `examples/` has a picture, a scene, an effect and a tune
(`cargo run -p flow-sdk --example scene` there). The demos and both games
are built on it; a program takes it by a tag of its repository:

```toml
[dependencies]
flow-sdk = { git = "https://github.com/arcanii/Flow-SDK", tag = "v0.1.0" }
```

From other languages: its `c/` (`flow-sdk-c`) is the SDK as a C library, with
a hand-written header (`c/include/flow_sdk.h`), and its `python/` a
Python package over it (`flow_sdk`, pure Python through ctypes). Both ask
the terminal, send pictures, models, meshes, scenes on a stage each frame
(with the scene's vector maths), screens of cells, effects, sounds and the
tint, see and change Flow's settings as `flow-settings` does, play songs
of the program's own (or mix them for an output of its own), and read
keys, each with an example of a gem turning in the middle of the screen
and one of a cube turning, drawn as a scene to a dance (and C one of the
settings);
`cargo build --release -p flow-sdk-c`, in its repository, builds the library, with its audio
unless given `--no-default-features`.
[C and Python](https://github.com/arcanii/Flow-SDK#c-and-python) has the rules for who frees what
and where Python finds the library.
