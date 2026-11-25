[← Flow's documentation](../README.md#documentation)

# How it is built

The repo is a Cargo workspace with one shared core and one crate per OS:

| Folder | What's in it |
| --- | --- |
| `core/` | `flow-core`: everything shared (the terminal, the assistant, the liquid, the UI) and the `Platform` interface. It never calls an OS API. |
| `windows/` | `flow-windows`: DWM glass, WinSparkle updates, the display language, the computer for the startup logo, and the Windows shells. Also its packaging (`packaging/`), release scripts (`scripts/`), vendored binaries (`third_party/`) and `RELEASING.md`. |
| `macos/` | `flow-macos`: the title bar and traffic lights, blur, the menu bar and context menu, Sparkle updates, and the Unix shells with a UTF-8 locale. Also its packaging (`packaging/`), release scripts (`scripts/`) and `RELEASING.md`. |
| `linux/` | `flow-linux`: the window's name and icon for the desktop, startup notification, blur where the compositor offers it, the window manager's menu, the primary selection, the computer and the distribution's logo for the startup logo, the user's languages, Flow's own updater (and the command that installs a .deb's update), and the Unix shells. Also its menu entry (`packaging/`) and the scripts that make and install a release (`scripts/`); see its README. |
| `src/main.rs` | The `flow` binary: runs the core on the OS crate it was built for. |
| `demos/` | The `flow-demo` package, which Flow carried until 0.1.33 and `flow-sdk`'s download for flowapp holds now: one call into the demos' program from [Flow-SDK](https://github.com/arcanii/Flow-SDK), which holds the demos and `flow-sdk`, the SDK they and the games are built on (see [Demos](manual/demos.md) and [Building for Flow](manual/sdk.md)). Flow takes both by a tag, in the root `Cargo.toml`. |

Each OS crate compiles to nothing on other systems, so `cargo build --workspace`
works everywhere. Adding an OS is a new crate that implements
`flow_core::Platform`, plus a line in `src/main.rs` and in the root `Cargo.toml`.

Inside the core:
- `session.rs`: the PTY (`portable-pty`, ConPTY on Windows) plus the terminal emulator
  (`alacritty_terminal`); tracks commands run for Claude using the prompt markers.
  On Windows it resizes the screen the way ConPTY does, which repaints it
  after every resize, and keeps the startup logo, which ConPTY doesn't know
  is there, through ConPTY's repaints; and serves the tab's pipe for images
- `pipe.rs`: the pipe for images on Windows, where ConPTY drops them (the
  named pipe itself is `windows/src/pipe.rs`)
- `shell.rs`: shell types, the integration hooks and the Unix shell discovery
  (the Windows discovery is in `windows/src/shells.rs`)
- `history.rs`: the command history, in SQLite (`rusqlite`, with SQLite built
  in) and its trigram full-text index; `mouse.rs`: the mouse reports for
  programs that ask for them
- `model.rs`: the Ratty graphics protocol, OBJ and glTF models (`gltf`, with
  meshopt's codecs written in `model/meshopt.rs`, Draco's meshes through
  `draco-core`, WebP textures through `image-webp` and Basis textures through
  `basisu`) and where each is shown; `render/models.rs`
  (`shaders/model.wgsl`) draws them
- `looks.rs`: the themes, their start screens (with teletext's colors and
  double lines) and their prompts, written for each shell
- `line_size.rs`: DEC's double-width and double-height lines, kept as a mark
  in each line's last cell; `mosaic.rs`: the block graphics Flow draws
  itself, teletext's mosaics among them; `box_drawing.rs`: the lines of box
  drawing and the squares (■ □ and their smaller sizes), which it draws
  itself too
- `render/`: wgpu renderer, a GPU stable-fluids simulation and a wave
  simulation for water (`fluid.rs`, `shaders/fluid.wgsl`), the map of the drops
  on the glass for rain (`rain.rs`, `shaders/rain.wgsl`), the glass shader, and a
  glyph atlas (`cosmic-text`), with the themes' fonts built in from
  `assets/fonts/` (their licenses beside them, and in each release's notices;
  the teletype's, Flow's own, drawn by `assets/fonts/teletype.py`)
- `fx.rs`: the effects layered on the liquid (bubbles, drops, lightning, and
  the splashes when a command ends, the assistant works or a program asks with
  OSC 7731); `fx/signals.rs`
  keeps the rings and the progress tank;
  `fx/bubbles.rs` moves the bubbles of the underwater style, `fx/rain.rs` runs
  the rain on the glass
- `ai/`: streaming clients over raw HTTP (Rust has no official SDKs) for
  Claude's Messages API (`claude.rs`), OpenAI's Responses API (`openai.rs`) and
  Gemini's generateContent (`gemini.rs`), what they share (`provider.rs`), the
  tool-use loop, the tools (`run_command`, `read_terminal`, `read_file`,
  `write_file`, plus server-side web search) and the token meter
- `ui.rs`, `app.rs`: layout, drawing, input, approvals, settings, About
- `access.rs`: what screen readers are told, a tree of what's on screen
  made for AccessKit (`accesskit`, handed to each system's reader by
  `accesskit_winit`)
- `i18n.rs` and `locales/`: the interface's words, one JSON file per language
  (`en.json`, `ja.json`), built into the binary
- `platform.rs`: the `Platform` interface the OS crates implement

On Windows the window is transparent through a DirectComposition swapchain
(DX12) with DWM acrylic behind it (`windows/src/lib.rs`, `windows/src/window.rs`).
On macOS it is a transparent Metal layer over the window server's background
blur, with the traffic lights centered in Flow's own title bar
(`macos/src/window.rs`). On Linux it is a transparent Vulkan surface (an
opaque OpenGL one where there's no Vulkan) on Wayland or X11, without the window manager's
decorations: Flow draws its own title bar and buttons and asks the window
manager to move and resize the window (`linux/src/window.rs`).
