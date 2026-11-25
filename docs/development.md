[← Flow's documentation](../README.md#documentation)

# Development

```bash
cargo run --release
```

```bash
cargo test --workspace
```

This runs the core's tests and each OS crate's; the games' are in their own
repository (Flow-Games). On Windows, those include
`windows/tests/shells.rs`, which drives cmd, PowerShell, Git Bash and the
bundled BusyBox in a real ConPTY, checks that each finds the games' folder on
its PATH once, resizes one to check that every line shows once and stays in
the scrollback, checks that the startup logo outlasts a full-screen
program and resizes but not `clear`, and sends a picture down a tab's pipe
for images and checks that bare placeholder cells come through ConPTY one to
a column with their color. On macOS, `macos/tests/shells.rs`
drives zsh and bash in a real PTY. On Linux, `linux/tests/shells.rs` does the
same, and `linux/tests/theme.rs` checks that prompt themes which set the
prompt before each one keep Flow's marks. The SDK's tests, its C library's
and Python package's among them, are in its own repository now.

On Linux, building needs only Rust and a C compiler (for SQLite and `ring`).
winit, wgpu and the clipboard load the system's Wayland, X11, xkbcommon,
Vulkan and OpenGL libraries when Flow starts, and the sound output loads
ALSA's at the first sound, so no development packages are needed.

```bash
cargo test --workspace -- --ignored
```

The ignored tests send each provider's real API (Claude, GPT and Gemini) a fake
key, to check that it is turned down and reported as a key problem. They also
open the sound output at no volume and draw 3D models on the GPU, and on
Linux they install the release the live feed offers into a scratch folder,
as an update would.

```bash
cargo run --release -- --snapshot docs/screenshots
```

This renders a scripted session offscreen, the same one used for the README's
pictures. It writes PNGs for the main window, the key dialog, settings, each
liquid style and About, so the look can be checked without a visible window.
Snapshots run on Flow's default config and plain shells of their own
(PowerShell on Windows, else bash, started without your startup files in a
folder they make), so their prompts name neither the user nor the computer,
and the sample conversation uses that shell's commands.

```bash
cargo run --release -- --record frames
```

This renders each liquid style but None, without the CRT, for 4 seconds at 24 fps and saves every frame
as `frames/<style>/NNNN.png`. Time advances exactly one frame per frame,
however long a capture takes, so the motion is smooth.

```bash
cargo run --release --example make_movie -- frames/oil oil.webp
```

This turns one style's frames into a looping animated WebP, 640 px wide by
default, like the animations in Flow-releases' README. GitHub plays animated
images inline but not videos stored in a repo.

The display name lives in one constant, `APP_NAME` in `core/src/lib.rs`.

Every release script (`release.sh` and `app-release.sh` on the Mac,
`package.sh` and `app-release.sh` on Linux, `release.ps1` and `app-release.ps1` on Windows) builds with
`--remap-path-prefix`, the home folder as `~` and the repository as `flow`,
so the programs' panic messages name no build folder, and with it no one's
account; the scripts of the repositories whose apps flowapp installs do
the same. Flow builds without reaching any of them; their scripts run
Flow's release tools from a checkout of it.

## Languages

Each language is a JSON file in `core/locales`: nested objects whose leaves
are the words, looked up by their dotted path (`settings.title`), with `{name}`
where a value is filled in. `en.json` is the reference; a key another file
lacks falls back to it. To add a language:

1. Copy `en.json` to `<code>.json` (a two-letter code such as `fr`) and
   translate the values, keeping every `{name}`. `language.name` is the
   language's name for itself, shown in Settings, and `language.model` its
   English name, which the assistant is told.
2. Add the file to `FILES` in `core/src/i18n.rs`.
3. For macOS, add the code to `CFBundleLocalizations` in
   `macos/packaging/Info.plist`, and translate
   `macos/packaging/ja.lproj/InfoPlist.strings` into a `<code>.lproj` beside it.

`cargo test -p flow-core i18n` checks that every language has every key with
the same `{name}`s, and that every key the code names is in `en.json`. Settings
shows two lines of each hint, so keep hints short. What goes to the
assistant's model (tool results, the system prompt) stays in English.

## macOS app bundle

`cargo run` starts a bare binary: it works, but without Flow.app's menu bar
name, Dock behavior or updater. To try the real app:

```bash
cargo build --release && macos/scripts/bundle.sh target/release/flow target/release
```

```bash
open target/release/Flow.app
```

`macos/scripts/release.sh --adhoc --allow-dirty` builds the whole release
(Sparkle, notices, signing, the DMG) without a certificate, for this Mac only.

## Linux release

```bash
linux/scripts/package.sh
```

This builds `flow`, both games and `flow-demo` in release mode against
glibc 2.31, with zig and cargo-zigbuild (see
[linux/RELEASING.md](../linux/RELEASING.md)), so they run on Ubuntu 20.04,
Debian 11 and every Linux since, and writes
`target/linux/Flow-<version>-linux-<arch>.tar.gz` with its `.sha256`: Flow,
the games and demos, the menu entry (`linux/packaging/com.arcanii.Flow.desktop`), the
icons, `THIRD-PARTY-NOTICES.txt` and `install.sh`, laid out as they install.
Where `dpkg-deb` is, it writes the same files under `/usr` as
`Flow-<version>-linux-amd64.deb` (package `flow-terminal`, version
`<version>.<build>`) with its `.sha256` too.
It refuses a tree with changes, whose build number would name the wrong
commit, unless given `--allow-dirty`; `--native` builds with plain cargo
instead, for this computer's Linux and newer only; `--no-build` packs what
was built last and `--out <dir>` writes elsewhere. It builds for the
computer it runs on, so make the Arm64 tarball on an Arm64 one. Nothing is
published.

## App icon

```bash
cargo run --release --example make_icon
```

This redraws the icon from code into `assets/icon/`:

- PNGs from 16 to 1024 px
- `flow.ico` for Windows and `flow.icns` for macOS
- a zoomed review sheet at `target/icon-preview.png`

Sizes under 40 px use a simpler layout with a bigger prompt so they stay
legible. `build.rs` embeds `flow.ico` and the version info into `flow.exe`. That
needs `rc.exe` from the Windows SDK and is skipped without it.
`macos/scripts/bundle.sh` copies `flow.icns` into Flow.app.
