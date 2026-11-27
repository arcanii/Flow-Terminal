# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

Flow is a terminal emulator in Rust (winit + wgpu) with a liquid-glass look and
Claude built in. Its user manual is `docs/manual/`, a page a topic (install,
keys, the terminal, settings, themes, liquid styles, sound, shells, config
keys, updates, the assistant, outside apps, images, flowapp, the SDK, games,
demos), and `docs/` also describes the build (`how-it-is-built.md`,
`development.md`), what has been tested on each OS (`status.md`) and whose
logos Flow shows (`trademarks.md`). Flow's own name, icon and startup logo
are arcanii's marks, outside the MIT License (`docs/brand/LICENSE-brand.txt`,
decided 2026-11-26, after the ArtCraft repositories' brand license): a fork
changes them unless it's pristine, and CONTRIBUTING's Forks says where
each is, so a change that adds a place the name, icon or identity lives
adds it there too. README.md is short: the icon and
shields, then "Why Flow", the pitch (written 2026-11-23: Omarchy's way
of working on the computer you have, AI running your PC, old machines, an
exciting terminal, the everyday things on every OS), whose claims link to
the manual's sections that hold them, a short tour, install, getting
started, and the index of the docs (its Documentation section, which each
page's first line links back to). CONTRIBUTING.md tells contributors how
to build and test and the house style below, with `.github/`'s issue and
pull request templates, and CODE_OF_CONDUCT.md how they treat each other
(reports through the Security tab's private report, titled "Conduct:";
no CLA or DCO, decided 2026-11-27). When a change adds or alters a setting, shortcut,
config key, style or platform behavior, update the matching page in
`docs/manual/` too (a new page goes in the README's Documentation), and
Why Flow where it's worth selling or a claim there no longer holds.
BACKLOG.md lists what's planned for Flow; the games keep their own, in
their repository (arcanii/Flow-Games).
A feature's design, the plan agreed before it's built and kept up to
date as it changes, goes in `docs/design/<feature>.md` (a rule since
2026-11-16), not only in a session's plan file, which the next plan
overwrites.

## Commands

```bash
cargo run --release                     # a bare binary; Flow.app is below
cargo test --workspace                  # core's tests and this OS crate's, with a real-PTY shell test
cargo test -p flow-core fx::rain        # one module
cargo test -p flow-core the_cursor_wipes_a_path_clear   # one test
cargo test -p flow-macos --test shells  # zsh and bash in a real PTY (flow-windows on Windows, flow-linux on Linux)
cargo test --workspace -- --ignored     # sends each provider's real API a fake key; with FLOW_MCP_EVERYTHING naming npm's @modelcontextprotocol/server-everything's dist/index.js, runs Flow's MCP client against it; opens the sound output, silently; draws 3D models on the GPU; times failed reads of a picture's file; fetches a release's file through GitHub's API; on Linux, installs the published release in a scratch folder
cargo clippy --workspace
cargo run --bin flow-help -- settings   # Flow's help pages in the terminal (flow-help in a Flow tab; none for all, --list)
cargo run --bin flow-settings -- --help # Flow's settings from the shell (only in a Flow tab, which answers it)
cargo run --bin flowapp -- list           # Flow's apps, as Homebrew keeps formulae (FLOWAPP_CATALOG for a catalog of your own)
cargo run --bin flow-mcp -- --help        # Flow as an MCP server for outside apps, which an app's MCP config runs (FLOW_MCP_SOCKET for another socket)
cargo run --bin flow-command -- --help    # the flow command (in a Flow tab: a tab for a command in the running Flow), `flow` in a release
cargo test --test mcp                     # flow-mcp run for real against a stand-in Flow on a scratch socket (Unix)
cargo test --test flowapp                 # flowapp run for real in a scratch home, its catalog a file (Unix)
(cd ../Flow-Games && cargo run --release -p flow-rogue)   # a game, from a checkout of arcanii/Flow-Games (private) beside this one (flow-mines, flow-invaders; run it inside Flow)
cargo run --release -p flow-demo -- boing    # a demo, from Flow-SDK's tag (boing, cube, plasma, teletext, vector, sounds, tint, model; none lists them)
```

- Don't run `cargo fmt`. There is no rustfmt config and the code isn't
  rustfmt-formatted (lines often pass 100 columns), so it would rewrite the
  whole repo. Match the surrounding style by hand.
- `cargo clippy --workspace` reports 9 warnings in older code (too many
  arguments, `chunks_exact`, …). New code should add none.
- Each OS crate is `#![cfg(...)]`-gated and compiles to nothing elsewhere, so a
  build on one OS never checks another OS's crate. The root `build.rs` embeds
  the Windows icon and version info only when `rc.exe` (Windows SDK) is there.
- Debug builds log to stderr (`RUST_LOG` works). Release builds log to
  `flow.log` next to `config.toml` instead (a guest Flow, started for the
  `flow` command, or a snapshot or recording, to `flow-window.log`).
- On Linux `flow` is also the `flow` command, so a bare `cargo run
  --release` typed in a Flow tab opens a tab in that Flow rather than a
  new window: run it as `env -u FLOW_TAB_KEY cargo run --release` there
  (or from another terminal).
- Environment: `FLOW_SHELL` picks the first tab (`cmd`, `powershell`, `bash`,
  `busybox`, `zsh`); `FLOW_SKIP_PROFILE=1` keeps the bash and BusyBox hooks from
  sourcing the user's profile; `FLOW_UPDATER=1` starts the updater in a debug
  build, or a release build made without `FLOW_UPDATES=1` (see Versions and
  releases); `FLOW_UPDATE_FEED=<url>` points Linux's updater at another appcast
  (what it offers must still be signed with Flow's key); `ANTHROPIC_API_KEY`, `OPENAI_API_KEY` and `GEMINI_API_KEY` override the
  stored keys, and `ANTHROPIC_BASE_URL`, `OPENAI_BASE_URL` and
  `GEMINI_BASE_URL` the endpoints. On Windows `APPDATA` names the folder
  Flow's own folder goes in (`config::config_dir`; `dirs::config_dir`
  alone reads the user's known folder and ignores it), so a test copy
  started with a scratch `APPDATA` and its own `FLOW_MCP_SOCKET` keeps
  its config, tabs, history and apps there, as a scratch `HOME` does on
  the Mac. The credential store is still the user's.
- On macOS, `cargo build --release -p flow-terminal && macos/scripts/bundle.sh target/release/flow target/release --game target/release/flow-help --game target/release/flow-settings --game target/release/flowapp --game target/release/flow-mcp --game target/release/flow-command`
  makes `target/release/Flow.app` with the menu bar name and Dock behavior
  that the bare binary lacks (no updater without `--sparkle`), and
  `flow-help`, `flow-settings`, `flowapp`, `flow-mcp` and the `flow`
  command (`flow-command`, which `bundle.sh` names `flow`) on its tabs'
  PATH, as a release has them. From 0.1.34 Flow carries no game or demo (`flowapp`
  installs them); a test build may still take them, a game built in
  Flow-Games and flow-demo with `-p flow-demo`, each given as a `--game`.
- On Linux, `linux/scripts/package.sh --allow-dirty` builds Flow and its
  programs against glibc 2.31 with cargo-zigbuild (`--native` builds
  with cargo for this machine's glibc, which only it and newer Linuxes have)
  and writes `target/linux/Flow-<ver>-linux-<arch>.tar.gz` (and, where
  dpkg-deb is, the same under `/usr` as `Flow-<ver>-linux-amd64.deb`),
  laid out as its `install.sh` installs it (`bin/flow`, `lib/flow-terminal/games/`, the
  .desktop file and icons under `share/`). `install.sh --prefix <dir>` into a
  scratch folder tries it without touching `~/.local`. Building needs only
  Rust and a C compiler: winit, wgpu, arboard and the sound output
  (ALSA's) load the system's libraries at run time.

## Checking the look

Rendering has no unit tests but an ignored one that draws 3D models, and
the WGSL shaders are compiled when the app starts (`include_str!`), so
`cargo build` doesn't catch shader errors. After
touching rendering, run the app or a snapshot and look at the result.

```bash
cargo run --release -- --snapshot <dir>  # main window, key dialog, settings (settings-keys.png too), style-<name>.png per style and the CRT, About, help.png, keys.png, split.png, scrolled.png, and recording.webp last
cargo run --release -- --record <dir>    # each style but None (no CRT) for 4 s at 24 fps: <dir>/<style>/NNNN.png
cargo run --release --example make_movie -- <dir>/oil oil.webp
```

- Both play a scripted session in a hidden window: the stage machine in
  `app.rs` (`snapshot_tick`, `record_tick`, `SHOWCASE`, `stir_showcase`).
  Captures are composited over a synthetic wallpaper (`main-alpha.png` is the
  one without). `--record` advances time exactly one frame per frame, however
  long a capture takes.
- `docs/screenshots/` holds the README's and the manual's pictures, made with
  `--snapshot docs/screenshots`. No user or host name may appear in them,
  and a snapshot (or recording) runs on made-up inputs alone, whoever makes
  it:
  - Flow's default config (`starting_config`), which it never reads or
    writes (`save_config` writes nothing), plus what its stages set;
  - a made-up token meter (`Meter::made_up`, 4.2k, no cap);
  - no `updates.json`, tabs, history, memory, keys, packages, apps, skills
    or MCP servers of the user's.

  Its shell is PowerShell on Windows, else bash, whatever FLOW_SHELL says
  (`snapshot_shell`), started plain (`shell::make_shells_plain`):
  `FLOW_SKIP_PROFILE=1`, PowerShell's `-NoProfile`, cmd's `/D`, nothing of
  flowapp's, and a home folder of its own. That folder is made with six
  made-up files at fixed times on the local clock, so PowerShell's
  listing doesn't show the zone (`make_snapshot_home`, `SNAPSHOT_FILES`,
  `snapshot_file_time`), and every tab starts in it: `C:\flow-snapshot` on Windows, so
  PowerShell's prompt names nobody, else Flow's cache, which bash calls
  `~`. So bash's prompt shows only its version (the bash hook sets it,
  since Debian's bash reads `/etc/bash.bashrc`, whose prompt names both,
  before the hook), and the demo command runs `uname -srm` for the same
  reason.
  The current pictures were made on Windows, except `style-rain.png` (a
  downpour, `SHOWCASE_RAIN`),
  `style-water-below.png`, `style-none.png`, `style-crt.png`, `key.png`, `new-tab.png`, `tab-card.png`, `resources.png`, `settings.png`,
  `settings-font.png`, `settings-apps.png`, `settings-skills.png`, `theme-gallery.png`, `find.png`, `palette.png`, `split.png`, `history.png`, `help.png`, `model.png`, `theme-home.webp`, `theme-green.png`, `theme-instruments.png`, `theme-amiga.png`, `settings-mcp.png`, `mcp-ask.png`, `settings-mcp-servers.png`, `mcp-call.png`, `mcp-ask-form.png`, `mcp-ask-link.png`, `assistant-files.png`, `assistant-drop.png`, `assistant-input.png` and `assistant-models.png` (macOS, scaled to 1650 × 1050
  from a 2× screen, but `new-tab.png`, `tab-card.png` and `resources.png`,
  kept at 2200 × 1400). `theme-home.webp` is the
  snapshot's `theme-home.png` through `cwebp -q 90 -m 6`, since its tube's
  grille makes a PNG of 3 MB.
  A snapshot's history keeps only made-up commands (`App::demo_history`), in
  memory, since real folders could name the user. The Flow notanAmiga
  CLI's prompt shows the folder, so snapshots open its tab in their own
  home folder, which the Unix shells write as `~`, and Windows' as
  `C:\flow-snapshot` (`App::start_theme`).
- Snapshot and record runs never read the credential store, and their hidden
  window counts as focused. A normal run reads the stored key; on macOS each
  new build may then ask for Keychain access, which `ANTHROPIC_API_KEY` avoids.

## Architecture

**Workspace.** `core/` (`flow-core`) holds everything shared and never calls an
OS API. `windows/`, `macos/` and `linux/` each implement `flow_core::Platform`
(`core/src/platform.rs`: window attributes, backdrop, wgpu instance, shells,
the system's languages, menus, context menu, updater, error dialog, the
games' folder, the pipe for images, the computer and the system's logo for
the startup logo, the primary selection, the window manager's menu, the
memory banks and buses for the Workstation's PROM (`Platform::hardware`), a web
page opened, as About's Release notes does: `Platform::open_url` with
`version::release_page`, and the room the screen leaves for windows and
keeping the window in it, for Settings' Fit window to:
`Platform::screen_room`, `keep_on_screen`, the Mac's and Windows' (its
screen's work area, `GetMonitorInfoW`'s `rcWork`), Linux's a taskbar's
guess, `TASKBAR`; a long
command done told of, `Platform::notify`; and the drop-down hotkey,
`register_hotkey`, `drop_down_rect`, `screens`, `set_dropped_down`,
`bring_forward` and `put_away`: see Notifications and the hotkey). `src/main.rs` is the `flow` binary
and picks the OS crate with `cfg` (`src/bin/flow-help.rs`, beside it, prints
help in a tab: see Help). Flow's SDK (`flow-sdk`, with a C library and a Python package over it) and
the demos built on it live in their own public repository,
arcanii/Flow-SDK, whose CLAUDE.md has their notes. Flow takes both by a
tag (`flow-sdk` and `flow-demos` in the root Cargo.toml's
`[workspace.dependencies]`): `flow-settings` asks Flow through
`flow_sdk::settings`, the games are built on it, core's and Windows'
tests read what Flow answers with it, and `demos/` here is only the
`flow-demo` package that flow-sdk's download for `flowapp` holds (Flow's
releases carried it until 0.1.33), one call into
`flow_demos::app::main`. Core keeps a copy of the SDK's ALSA output for
its own sounds (`core/src/sound/alsa.rs`). To change the SDK with Flow,
point those two dependencies at a checkout by `path` while working, then
tag it there and point them at the tag. The games live in a private
repository of their own, arcanii/Flow-Games, which builds them and makes
their downloads; this workspace takes nothing from it (see Games). Platform wording and shortcuts inside core
use `cfg!(target_os = "macos")`, `platform::keys(mac, other)`, or
`platform::per_os(mac, windows, linux)` where Linux differs too.

**Threads and events.** All UI state lives on the winit event-loop thread in
`App` (`app.rs`, `ApplicationHandler<UserEvent>`). Other threads reach it only
through `session::Waker`, which sends `event::UserEvent`s through the loop
proxy.
- `Session` (`session.rs`): a PTY (portable-pty, ConPTY on Windows), a reader
  thread feeding an `alacritty_terminal` emulator behind a mutex, a thread
  that waits for the child, and one that writes to the shell (`ToShell`):
  keys, pastes, mouse reports and the answers to programs' questions all
  go to its queue and return at once, so a program that stops reading
  (its pty's input full) holds up that thread alone, never the window or
  the reader. The queue drops mouse motion past `MOTION_ROOM` and
  anything past `ROOM`, and closing the tab drops what waits. They send
  `Wake` and `ChildExited`. Each terminal
  tab (`Tab` in `app.rs`: its theme, a glow flag, and its panes) holds one
  or more, each a `Pane` (a shell profile, its session, its effects'
  allowance and its history run); `App::tabs` is the tab bar and `active`
  the tab in front, and `App::session()` is its pane in front's
  (`Tab::focus`), which the keys, the assistant and the find bar go to.
  A split tab's panes share its terminal as `panes::Split` lays them out
  (a tree of two sides each, the first taking its `share`, `rects`,
  `beside`, `remove`; `dividers` gives each split's line, which
  `ui::draw_dividers` draws as `Hit::Divider`, `App::dragging_divider`
  drags through `Split::drag`, held to each side's `least`, and a double
  click `even`s, `relayout_front` sizing the tab in front's shells alone); `App::pane_layouts_in`
  gives each a `Layout::within` its rectangle, `relayout` sizes every
  pane's pty (the tabs behind by their own theme's layout), `term_layout`
  is the pane in front's (what the mouse's cell math and the cursor go
  by), and `ui::draw_terminal` draws each `PaneView`, its cursor gliding
  only in the pane in front, `draw_dividers` between. `split_pane` (⌘D,
  ⇧⌘D) opens a shell as a tab would (`open_pane`, taking it out of the
  tab bar) and splits the pane in front; `focus_beside` (⌥⌘ and the
  arrows), a click (`pane_at`, before the press does anything else),
  `close_pane` (⌘W first, `close_pane_or_tab`, a ✕ at the top right of
  the pane under the pointer, `Hit::PaneClose`, `ui::pane_close_rect`,
  left of its scrollbar, the pill moving left of it, on panes without a
  theme's frame, whose close box is `Hit::FrameClose`, both through
  `close_pane_at`, readers' `Do::ClosePane`, or a shell exiting) and
  `panes_changed` keep them. Each pane wears a theme of its own: the
  tab's `look` and `own` are its pane in front's, and each `Pane` keeps
  its own while another is in front (`Tab::look_of`); `Tab::put_in_front`
  swaps them as the keys move, so `worn_for` is a tab's and its pane's ids
  (`Pane::id`, which Restart Tab's new shell doesn't keep), and
  `keep_worn` puts what Settings changed back on the pane it was worn for,
  in front or behind by then (`pane_config`). The panes behind draw in
  their own theme from `App::pane_looks` (`PaneLook`: colors, font and its
  cell, worked out by `see_pane_looks` when a theme or the scale changes):
  `pane_layouts_in` gives each its own cell (`Layout::within_cell`),
  `PaneView` carries its `ink` and `font`, `ui::draw` switches the atlas's
  face for it (`Canvas::use_font`, the atlas keeping every face's glyphs)
  and back, and a background other than the window's is laid under it in
  the backdrop; the liquid, glass, tube and phosphor stay the window's.
  `give_inks` answers each pane's programs from its own. Settings on a
  split tab says **this pane** (`Tally::on_pane`). A new pane starts in
  the theme and at the grid of the half it takes (`open_tab_sized`).
  `tabs.json` keeps a split tab's other panes (`OpenTab::more`,
  `OpenPane`, each with its `PaneTheme`, none from an older Flow, whose
  panes wear their tab's), its `split` and `focus`. The wheel scrolls the
  pane under the pointer, and links and the I-beam work over any pane
  (`App::pane_under`). Snapshots end on a split tab, `split.png`
  (`SPLIT_STAGE`), its new pane in the green screen's theme and its line
  moved (`wear_pane_theme`), then scrolled back, `scrolled.png`
  (`SCROLLED_STAGE`).
  `App::restart` (Settings' Restart, About's Restart Flow, and through
  `App::restart_by_key` the Mac's Flow → Restart Flow, which asks first
  while a tab isn't at its prompt, `Session::at_prompt`) closes them all
  without quitting, reads config.toml again
  (`Config::reread`, `App::follow_config`) and opens each tab again
  (`App::open_tabs`, `reopen`: its shell profile by id and label,
  `reopened_shell`, its theme and its folder, through `open_tab_at`),
  greeting as every tab does (the startup logo, a theme's start screen;
  snapshots show no logo). Quitting keeps them in `tabs.json` beside
  config.toml (`config::OpenTabs`, written in `exiting`, removed once the
  last tab has closed, never by snapshots), and a start opens them again
  (`App::open_first_tabs`) where `shell.reopen_tabs` says, unless
  FLOW_SHELL picks the shell. Once an update has put a new Flow in place of the running one
  (`Platform::replaced`: on Linux, `/proc/self/exe` marked deleted, after
  install.sh or apt), it starts that one (`App::hand_over`,
  `Platform::relaunch`, after saving the tabs marked `restart`, which the
  new one reopens whatever the switch says) and quits
  at the next `about_to_wait` (`handed_over`) instead. ⇧⌘B or
  Ctrl+Shift+B (the Mac's Shell → Restart Tab, `Command::RestartTab`)
  restarts only the tab in front (`App::restart_tab_by_key`,
  `restart_tab`): a new shell of its profile opens in its theme and folder,
  greeting as every tab does, takes the old one's place in the bar, and
  the old session ends; it asks first while that tab isn't at its prompt.
  The
  assistant's running command remembers its session by id, not by tab
  position, since tabs open and close under it. ConPTY repaints its whole
  screen after a resize and keeps its top row, while alacritty's resize keeps
  the bottom row, which doubled lines and lost scrollback. So on Windows
  `Session::resize` goes through `resize_from_top` (`Emulator::resize_for`),
  which rewraps the screen from the top and the scrollback on its own. It
  keeps nothing below the cursor, which ConPTY paints again, so a test of
  `Emulator::resize` that counts on alacritty's resize is
  `#[cfg(not(windows))]`, and one calling `resize_as_conpty` (the tests'
  name for its resize once ConPTY has its size) itself checks Windows'. ConPTY resized before it has
  heard where its cursor is (which it asks first, and Flow answers once it
  reads the shell's output, after a start screen is typed) paints down a
  screen as tall as it last saw and scrolls what Flow drew off the top; a
  window being shown is sized to the screen and back. So the pty's size
  waits for the reader's first output (`Pty`, in `session.rs`), and until
  then ConPTY repaints nothing, so the emulator's resize keeps the rows
  below the cursor too (`resize_from_top`'s `keep_below`): a start screen
  typed in meanwhile, such as the Flow notanAmiga's white first page,
  stays whole, and the rows a resize leaves are blank, not in the colors
  the start screen last wrote in.
- Shell integration: `shell.rs` injects a hook into each shell that emits OSC
  133 prompt marks and OSC 633 `Cwd`. `osc.rs` finds them in the byte stream
  (and OSC 9;4 progress), and `Session::run_command` / `poll_command` use them
  to tell when a command finished, its exit code and the folder. cmd can't
  report exit codes. The hooks bash, BusyBox and zsh read are files in
  `shell-integration/` beside config.toml, each named after a hash of its
  text (`hook_path`: `flow-<12 hex>.bash`, `.ash`, and a folder
  `zsh-<12 hex>/` of zsh's four), written only where missing or different,
  whole then renamed into place (`put_hook`), so two Flows of different
  versions at once, or another checkout's tests, never overwrite the hooks
  the other's shells read; the old `flow.bash` and `zsh/` are left for
  Flows before them. macOS's `/etc/zshrc` keeps zsh's history in
  `ZDOTDIR`, so `ZSH_RC` points it back at `zsh/.zsh_history`, where it
  always was. zsh and bash put the prompt's marks (and a look's
  prompt) on `PS1` before each prompt, after the user's own hooks (last in
  bash 5.1's array `PROMPT_COMMAND`, which Ubuntu sets up, while Flow's first
  command stays in front of the user's first, where bash-preexec looks; zsh's
  hook moves itself last again when a theme hooks in later), since prompt
  themes set `PS1` afresh each time (`linux/tests/theme.rs`). The hooks'
  variables read as `${VAR:-}` after the user's files, which may set nounset. bash's hook turns on
  `checkhash` before them, so a program gone since bash ran it (an app
  `flowapp remove` took away) is looked for again along the PATH, as zsh
  and ash look themselves (`linux/tests/shells.rs`). Before each prompt
  bash's `__flow_prompt` has readline take every byte as a letter
  (`builtin bind 'set convert-meta off' 'set input-meta on' 'set
  output-meta on'`, quiet with no line editing, never failing): in a C or POSIX locale, or none, or
  with convert-meta in the user's `.inputrc`, readline took each byte
  past ASCII as Meta and a key, so a letter in what Flow types (the
  assistant's command, a reply's block put at the prompt, the flow
  command's words) was keys, macOS's bash 3.2 taking Ē's second byte for
  revert-line (`macos/tests/shells.rs`'s C-locale test, with an
  `INPUTRC` of convert-meta on). readline 8.1 and later (bash 5.1 on)
  look at the locale again as each line is read and, where it changed
  since the line before, put that locale's 8-bit settings back over the
  bind for that line. bash's own copy of readline reads bash's variables,
  exported or not, at once; one of its own (Homebrew's, a dylib, and
  some Linux distributions') reads the C library's environment through
  `getenv`, which bash brings up to date only as it runs a program or a
  command substitution, so a change typed at one prompt reached readline
  a prompt later, once PS0's `$(__flow_line)` had run, or a line that set
  a C locale for a program and took it back left it there. So from 5.1
  `__flow_prompt` runs `: "$(:)"` before each prompt, before the bind,
  which starts readline the first time (about half a millisecond on the
  Mac), and where the locale (`__flow_locale`: each of `LC_ALL`,
  `LC_CTYPE` and `LANG` with its `@a` attributes, so an `export` counts)
  changed since the last prompt (or since the rcfile began, the hook's
  own first call passing over it, `__flow_starting`) and the one readline
  goes by doesn't name UTF-8 (`LC_ALL`, else `LC_CTYPE`, else `LANG`, read
  over every one set, as bash's own readline does, and over those
  exported, as `getenv` does; none set counts too), it prints OSC
  `633;P;FlowMetaKeys=1` (`Marker::MetaKeys`, no nonce, since it only has
  Flow type less), `ShellState::meta_keys` until the next "done".
  `ShellState::letters_may_be_keys` says where Flow can't know readline
  takes letters: never integrated, so marked, or, for bash and zsh
  (`Session::weighs_letters`), no "done" with the nonce yet (`guarded`,
  which each hook says before every prompt, the first too; a read-only
  `PROMPT_COMMAND` runs no hook before its prompts, and a zsh whose
  startup files `exec`ed bash runs none at all, whatever prompt a program
  forges). `Session::types_letters` (at the hook's prompt, none of those)
  and `Session::letter_as_keys` (a bash or zsh pane where they hold) say
  what Flow may type there: `App::run_in`, the
  helpers' and outside apps' commands fail with `card.letters_as_keys`, a
  reply's Run and the history's put toast `toast.letters_as_keys`, and
  the drop and the flow command take `shell::quote_ascii`
  (`readline_puts_a_c_locales_settings_back_only_at_the_prompt_bashs_hook_says_may_take_letters_as_keys`
  reads `bind -v` at each prompt, holding that readline puts them back
  only where the hook said, and types the raw bytes where it did:
  Homebrew's readline 8.3 took Ē's as letters there; Linux's 5.1 and 5.2
  weren't tried). `ShellState` also times every command for the glass:
  zsh (preexec) and bash mark the start (133;C: bash's PS0, `__flow_line`,
  from 4.4, and 3.2's DEBUG trap or bash-preexec, `__flow_old_line`, the
  trap passing over an empty line's PROMPT_COMMAND and the hook's own last
  line), the other shells count from Enter at a prompt
  with something typed (`ShellState::sent`), and the "done" mark leaves a
  `Finished` that `App::watch_commands` takes each frame. `at_prompt` is
  true from a prompt drawn (133;B) until Enter or 133;C, and, a session
  starting at its prompt, false from the shell's first mark until its first
  prompt (`handle_marker`: bash says 133;D and its folder a moment before
  it). 133;C is what turns it off for a line typed ahead while another ran,
  which starts once that one's prompt is drawn; PowerShell, cmd and BusyBox
  send none, so there it stays on (`windows/tests/shells.rs`'s Git Bash
  test). Anything printed in a tab can send the marks, so zsh's, bash's and
  PowerShell's hooks send the pane's nonce (see Command history) after each
  133;D (`133;D;<code>;<nonce>`), and zsh's preexec and bash's PS0 after
  133;C too (bash 3.2's trap, which isn't run for a line that starts with a
  subshell, doesn't); `osc.rs` hands it on (`Marker::CommandStart`,
  `CommandDone`), and `ShellState::forged` has `handle_marker` pass over,
  once a 133;D has come with it (`guarded`), a 133;D without it and a
  prompt (133;A, 133;B) drawn while a command runs (`started`, from a
  133;C, where one with the nonce has come, `starts_said`; else from Enter
  at the prompt, `running_since`), since Enter would read what the program
  drew after it as a command and Flow would take the program for the shell.
  A pane whose hook never sends one (cmd, ash, a bash whose PROMPT_COMMAND
  is read-only) takes every mark as before. The hooks escape every control
  character in the line as `\xNN`, as VS Code's do (a raw CAN or SUB ends
  an OSC early and shows the rest, nonce and all), and turn `set -x` off
  while they say it. Windows shell
  discovery is in `windows/src/shells.rs`, Unix discovery in `shell.rs`.
- Assistant (`ai/`): `Agent` runs a worker thread that takes commands over a
  crossbeam channel and runs the tool loop with the provider in
  `ai.provider`, sending back `UserEvent::Ai(AiEvent)`. Each provider has a
  client speaking its wire format itself (ureq and SSE; there are no Rust
  SDKs): Claude's Messages API (`ai/claude.rs`), OpenAI's Responses API
  (`ai/openai.rs`) and Gemini's streamGenerateContent (`ai/gemini.rs`).
  `ai/provider.rs` holds what they share: the `Provider` table (names, models,
  keys, endpoints; `/model` opens `model_list` as a list over the panel's
  input, `AiPanel::model_pick` the row chosen, `ui::draw_model_pick`
  leaving the conversation a row, `App::model_pick_key` its keys (↑↓,
  Home, End, Enter, a number, Esc; any other closes it and goes on),
  `Hit::ModelPick` its clicks, readers' `DialogKind::AiModels` and
  `Chooser::Models`; `/model` with words puts in use what `find_model`
  names, `App::model_command`, `use_model`, through `set_provider` for
  another provider's, saved at once, a notice in the conversation saying
  which model answers; snapshots show `assistant-models.png`; below
  Flow's own, the provider in use's own list, `ai/models.rs`,
  `docs/design/live-models.md`: `models::fetch` on a `flow-models`
  thread, `App::ask_for_models` as the list opens, with `Agent::key` and
  the endpoint in use, at most every `KEPT_FOR` (`AGAIN_AFTER` after a
  failure), never for snapshots, back as `UserEvent::ModelsListed` into
  `AiPanel::live` in memory (`models_listed`, the row chosen staying on
  its model, `App::pending_model_words` looked for again), `read_page`
  per provider, `chat_model` for OpenAI's, `snapshot_of` and `extras`
  leaving out Flow's own and dated copies, `provider::Model` and
  `model_list(in_use, live)`), errors, retries, SSE reading, and `Reply`, which hands the
  loop its tool calls and why it stopped. The history keeps the user's side as
  Claude's blocks and each reply in its provider's own form (`openai_item`
  items with encrypted reasoning, `gemini_part` parts with thought
  signatures), so switching provider starts a new conversation. Client tool
  calls (`run_command`, `read_terminal`, `read_file`, `write_file`) arrive as
  `AiEvent::ToolCalls`, read by `tools::parse_call`, which refuses a command
  holding a control character but its line breaks (`shell::key_in`, the
  model told why by `tools::command_holds_a_key`), since typed it would be
  a key that no card can show. `App::begin_tools` decides which need approval: guide
  mode asks for everything; auto mode asks for high-risk commands (the higher
  of the model's rating and `ai/tools.rs` `command_risk`), sensitive paths
  (`is_sensitive_file`: `is_sensitive_path` of the path as given and as it
  really is, `real_path` following each link one part at a time, a
  dangling one's target too) and overwrites (a link there counting as
  one). The risk rules are patterns, not a sandbox, and the manual says
  so: `command_risk`, `needs_admin`, `restarts` and `always_asks` read the
  line as `tools::Line` does, in views each pattern is looked for in
  (`Line::any`: the line as typed, `raw` and `spaced`, and a `View` for
  each of a word's two readings, `posix`, its quotes, `$'…'`'s escapes and
  escaping backslashes gone, and `windows`, its quotes, carets and
  PowerShell's backticks gone and backslashes kept, each `plain`, `named`
  and `names`). `pieces` splits it as a shell would, quotes keeping words
  whole, a backslash-newline dropped and a redirection a word of its own
  (`2>&1`, `&>log`), which `take` passes over with its target
  (`redirection`); a word may be a program at a command's start and after
  a `WRAPPERS` word or `find -exec`, by `program_name` of the first of its
  `braces` (`shell_variable`'s `$SHELL` a shell), and a quoted one is read
  as a line again (`sh -c "…"`, what `trap`, `alias` and PROMPT_COMMAND
  keep). Every word, wherever it is, is cut into `tokens` and their
  `names` (`Line::note`, `note_reading`, which reads a reading still
  holding quotes or escapes again, three levels deep, so `trap 'r\m …'` is
  `rm`), so an unlisted wrapper still shows what it names (`names_any`),
  and `is_root` reads the tokens for a root, home spellings, `~bob`, `/*`
  and `..` worked out. Apart from those views, **one rule** says what's
  hidden (decided 2026-11-24, after listing the shell's ways
  of putting a program together kept missing some and rating ordinary
  commands high): a command is rated by the programs Flow can read for
  certain, and one whose program it can't read asks; where the reader
  meets a shape it doesn't read in full, the line asks rather than the
  shape being modelled. `Reader` reads the line for it (`Line::read_in`,
  after `take`, with the `Shells` the line is read as: `Shells::of` the
  pane's kind, PowerShell's alone `Any`, which `Place::shells`
  (`App::shells_for`), a helper's pane, `outside_shells` and
  `run_from_reply` hand `own::decide`, `helper_refuses`, `Grant::command`
  and `command_risk_in`; `Shells::here` where no pane is known, `Any` on
  Windows; `hides_program_in`). It's bounded: a line past `MOST_LINE` (64
  KiB) isn't read, only hidden; `Reader::program` reads programs one in
  another's words (`read_program`) at most `MOST_PROGRAMS` deep, groups
  open at most `MOST_NESTED` (`Reader::open`), `tokens` stops looking for
  arithmetic past that many `((` that open none, and `Line::take`, the
  `(` handling and `case`'s keywords (`leading`) look through nothing
  twice, so `env`×thousands or `(`×thousands reads in milliseconds on a
  megabyte's stack. `tokens` cuts it as
  `pieces` does but keeps a command's output, `${…}`, `$[…]`, a backtick's
  command and `<(…)` in their word, takes a here-document's lines with
  the word that begins it (`Lexer`, `region_end` on a stack,
  `here_document`), arithmetic at a command's start one `TokKind::Arith`
  between its parentheses (`arithmetic_end`, `BEGINS`, so its `<<` begins
  no here-document; `((a) && (b))` two subshells as bash reads it), and,
  with `comments`, a `#` that begins a word a comment to the line's end.
  `Line::read_in` reads the line twice where it has one
  (`Reader::saw_comment`, `without_comments` for `take` and `Known`): as bash, whose
  comment's quote opens nothing, and as zsh without INTERACTIVE_COMMENTS,
  a word; a quoted command a program runs (`Ctx::depth`) takes comments
  as comments. `Reader::line` follows separators, subshells (`Open`, which
  give back their folders), `{ … }`, `while`/`if`/`for`/`case`
  (`LineState`, a case's patterns, after `then` and the like too; zsh's
  `case w { … }` hidden), zsh's `for x (…) cmd`, `foreach … end`,
  `repeat N cmd` and a command after `[[ … ]]` or `(( … ))`, a function's
  `()`, an array's and PowerShell's grouping (`GROUPED`, `foreach_list`),
  and PowerShell's `&` where a statement begins inside a command (after
  `{`, `=`, `try`, `return`, `exit` or `throw`, an `&` word `program`
  reads as `Given::called`);
  each simple command's words go to `simple` (what `commands_in` finds in
  them read first, as lines of their own, `apart`, its `Inside`;
  redirections into `Redirs`, a descriptor's name before one, `{fd}>`, and
  `>|` among them: `past_descriptor`, `names_descriptor`), `command` (the
  shell's keywords, `for_header`, `loop_over`,
  `assignment`) and `program`, which finds the program word past the
  wrappers whose words Flow knows (`WRAPS`, `Wraps`, `parse_options`: an
  option it doesn't know hides the program, a `%` in one is cmd's;
  `RUNS_UNKNOWN`'s every word must be written out, `unknown_wrapper`, and
  `SUBCOMMAND_RUNS`' after their command: `uv run`, `uv tool`, `pipx run`,
  `bundle exec`; `RUNS_BY_OPTION`'s options whose value runs,
  `option_values_written`: `tar --to-command`, `man -P`, `open -a`) and
  holds it to `literal` (no variable, command's output, glob, brace
  expansion, zsh's `=cmd`, `~user`, `~+`, cmd's `%x%`; quotes and escapes
  spelling letters read as them). Its only exceptions are `exempt`'s:
  `$EDITOR`, `$VISUAL` and `$PAGER` whole, and where the line may be
  PowerShell's (`Shells::Any`, or in what only PowerShell runs,
  `Ctx::powershell_only`: its script blocks, `Reader::blocks`, a word's
  parentheses, `pwsh -Command`) a statement that begins with a variable
  (`ps_value`, `ps_operator`, `-c` and `-i` alone not among its
  operators: `$files | Measure-Object`, `$x = Get-Date`, whose right side
  runs); both only where its name is nowhere else on the line where a word
  could set it (`Known`: `writes` of each name by `writes_of` in either
  reading of any word; `names`, `names_in`'s every word-shaped place, by
  its case, `declare -n r=x` and `${x:=…}` among them; `computes`, `${!r}`
  and zsh's `${(P)r}`; `names_computed`, a `SETS_NAMES` command with a
  variable anywhere, `builtin read "$n"` too), and not a shell-set name
  (`ZSH_OWN`, `$_` but in PowerShell's blocks), not piped into, not
  wrapped (`Given::wrapped`), only at the line's own depth. What runs
  from a string or the input is hidden too: `eval`, `iex`, `xargs`,
  `parallel`; `source` of what isn't written out, and a shell's script
  that isn't (`script`); `find`'s and `fd`'s `-exec` of `{}` in any
  reading (`{''}`, `\{\}`) or of what isn't written out (`Ctx::finding`,
  `{}` in a shell's or an interpreter's code); a shell's or an
  interpreter's code that isn't written out (`shell`, `posix_shell`,
  `fish`, `powershell`, `powershell_command`, cmd's `/c`, `interpreter`,
  `code`, `applescript_shell`), the code it is read as a line by
  `nested`; one reading its input (`reads_input`: no script, `-s`,
  `STDIN_PATHS`, `sudo -s`, `su`, `at`, `ssh host` with no command; a
  here-document's or a literal here-string's text read as its commands)
  that's fed what Flow doesn't read, its own (`Given::fed`, an expanding
  here-document holding `$` or a backtick among it) or its group's
  (`Open::fed`, `feed`, `LineState::closed` for `(sh) < x`), decided in
  `finish`; an alias, a function (`f()`, `function`), `hash -p` or
  `enable -f`; a variable the shell finds programs by set
  (`finds_programs`: `IFS`, `PATH`, `LD_PRELOAD`, `DYLD_*`, `BASH_ENV`,
  `ENV`, `ZDOTDIR`, the tables of what a name runs, bash's `BASH_CMDS`
  and `BASH_ALIASES`, zsh's `commands`, `functions`, `aliases`, `dis_*`;
  `sets_names`, printf's and print's `-v` alone); a name `sets_names`
  can't read for certain (a nameref, `declare -n`; a name word,
  `name_words`, not written out or holding a command's output, an index's
  arithmetic kept, `name_word`, `name_index`, `test`'s and `[[`'s `-v`
  too, `tested_names`; `declare`'s and its kin's before their `=`,
  `name_written`; a non-literal option; `mapfile -C`, `compgen -C`), a
  command line, prompt or interpreter's options set from input; `HOME`
  and the like (`points_at_settings`) set with a shell, git or ssh
  started (`sets_home`, `starts_shell`), `INTERPRETER_OPTIONS` that
  aren't written out; `RUN_STRINGS` (`EDITOR`, `GIT_SSH_COMMAND`,
  `PROMPT_COMMAND`, zsh's `NULLCMD`, …, `runs_string` with git's
  `GIT_CONFIG_VALUE_*`) and `git -c`'s commands (`git_setting`) read as
  lines, `PROMPTS` not written out or with a command's output; a value
  read again as code (`Inside::unread`: bash's `${x@P}`, zsh's `${(e)x}`;
  and arithmetic, `Reader::arithmetic` from `((…))`, `$((…))`, `$[…]`,
  `let`, `declare -i`, an index or an offset, `[[ … -eq … ]]`, on a
  command's output, `runs_output`, or on a variable the line set to what
  Flow can't read, `unread_vars` by `assignment` with `without_arithmetic`
  and `holds_code`, `sets_names`, `loop_over`, `names_read`; or on the
  positional words, `reads_positional`); ssh's remote words and
  `-o ProxyCommand` read as lines (`ssh`, `SSH`); PowerShell's
  `&` of what isn't written out (`Given::called`), `Start-Process`
  (`start_process`), `[Diagnostics.Process]::Start` (`ps_strings`),
  `Invoke-CimMethod`'s and `wmic`'s `Win32_Process` `Create`,
  `Invoke-Item`, `-EncodedCommand`, `[scriptblock]::Create`, `pwsh -File`
  of what isn't written out, `Set-Item function:`; and a file the line
  wrote run (`Cmd::runs_file` against `Cmd::writes`, a redirection's or
  `written_by`'s: `tee`, `sponge`, `cp`, `dd of=`, `curl -o` and `-O` in
  a cluster, compared by `path_keys` and `same_file` from the folders
  each runs in, `worked_out`).
  Each program it finds is a `Cmd` with its names,
  words, what its redirections write and the folders it may run in
  (`Folder`: the pane's, one written out, `Elsewhere` for a variable the
  line doesn't set, `Unread` for the pane's last or what the line made;
  `cd` and `cd_to`, `CdTo`, `join_folder` working out dots, `MOST_FOLDERS`;
  a `cd` that may fail keeps the folder before it past `&&`,
  `Reader::before`; in a pipeline, the background or a loop it only adds
  one, in a subshell or a child's quoted command it stays, one a trap
  runs, `Ctx::deferred`, adds to every later command's, `later`; `pushd`,
  `popd`, `env -C`, `sudo -D`, zsh's AUTO_CD of `..`; zsh's `cd old new`,
  the stack's `cd +1` and a `CDPATH` the line sets lead to `Unread`,
  `UNREAD_FOLDER`; past `MOST_CDS` every folder is). `always_asks_in_folder`
  (from `own::decide`, given the folder of the pane the command goes to,
  `Place::folder_for`, and `helper_refuses`, the helper's pane's) judges
  each delete by its own paths (`DELETE_PROGRAMS` as the program,
  `delete_paths`, `find`'s with `-delete` or `-exec rm`, `git clean`'s;
  `deletes_root_from`, `delete_asks`): written out, as `reaches_root` reads
  it from each folder (a glob that may match a root, `only_wild`);
  `$PWD`, `~+` and `$OLDPWD` as the folder (`read_path`, `PathRead`,
  `Start`), all of a folder Flow can't name asking (`whole_or_climbs`); a
  variable, a command's output or a glob that may make a root asking,
  one naming an entry after it not (`names_entry`, but this home's name
  or a `SYSTEM` folder's, a glob that may match one, `glob_matches`, any
  part under a users' folder, or a `..` anywhere); each of a path's brace
  expansions judged (`path_asks`), and its Windows reading too, a
  backslash a folder's and a space after one parting words
  (`windows_words`, so `"$d\"` is `$d`); a loop's variable over words
  written out read as each (`Reader::ranges`); PowerShell's delete piped
  what to delete, by what the command before it lists (`Given::from`,
  `Reader::pipe_from`: `Get-ChildItem`'s folder's entries, read as
  PowerShell reads them, `.\` the folder, `Get-Item`'s own), else
  `FED_PATHS`, which asks. The older rule that any delete with a root word
  anywhere asks stays beside it.
  They rate high what `always_asks` (credential tools' sign-ins, the
  firewall turned off, Defender's exclusions, accounts and groups changed,
  a program's words in any order: `opens_firewall`, `changes_accounts`,
  `writes_accounts`, `account_file`, `copies_windows_secrets`, too),
  `needs_admin` (any `su`) and `restarts` say, a program Flow can't read
  (`hides_program`, the rule's, with `encoded_command`; a shell or an
  interpreter given its program from a pipe, a file or `<(…)`,
  `runs_from_elsewhere`, `shell_reads_its_input`, `feed`; the history's
  `fc`, `r`, `!!` and `^x^y`, `expands_history`; a control character,
  `typed_as_key`, which `parse_call` and an outside app's `run_command`
  refuse, `untypable`), output piped or fed into a shell or `ssh`, a
  download into an interpreter (`reads_its_program`) or made runnable,
  curl, wget or PowerShell sending data, a command's output in what
  reaches another machine (`Simple::substituted`, `REACHES`), a raw
  connection (`Line::sends`, `uploads`, `CONNECTS`, `/dev/tcp/`, `openssl
  s_client` given something to send, not `/dev/null` or a bare `echo`,
  `feeds_nothing`), mail sent (`MAILS`, `sends_mail`), a gist made, and a
  word naming a sensitive path or a variable named for a secret
  (`names_secret`, from where a `cd` on the line went too,
  `Reader::cd_paths`, `secret_word`: `SURELY_SECRET` always, `/etc/shadow`
  and NSS's `key4.db` among them, the browsers' stores by their whole
  names, `browser_store`, `STORE_NAMES`, `BROWSER_STORES` and Firefox's in
  a profile's folder, `MAYBE_SECRET` in a file's name only outside
  `SOURCE` and words with spaces). The tests hold the rule to the shapes
  found to put a program together, each for `rm`, `reboot` and
  `diskutil`, in either `Shells` (`tools::PUT_TOGETHER`, `put_together`;
  `PUT_TOGETHER_POSIX` in any pane but PowerShell's), and ordinary
  commands to the rating the rules before it gave them (`tools::ORDINARY`,
  PowerShell's values in `ORDINARY_POWERSHELL` in a PowerShell pane,
  asking in any other: `$files | Measure-Object` runs `$files` there). The
  rule is by pattern: it asks wherever it can't tell what runs, so in
  auto mode a command built from variables asks even where it's
  harmless, and a program named in a variable an earlier command set is
  read only where the line says so.
  Commands run in the
  visible shell, and `ToolOutcome`s go back with `Agent::tool_results`.
  Text Flow didn't write goes to the model inside its own blocks
  (`<terminal_context>`, `<memory>`, `<file>`) and fences, so
  `ai/untrusted.rs` writes their tags found inside it with `&lt;`
  (`defang`, closing tags in any case, opening ones as Flow writes them,
  `_` as `-`, a space or nothing, read through `seen`, which folds
  fullwidth forms and drops zero-width characters; `one_line` for a
  folder's or a name's; `fence` for Ask why's and the carry-on message's
  blocks), and `SYSTEM_PROMPT` says all but the user's own words is data
  and Flow's blocks are only in the user's messages. Tool results stay as
  they are, in each provider's own result blocks (but `fold_results`'
  text). Agent tests run against
  `mock_api` in `ai/mod.rs`, a local server with canned SSE, for every
  provider; the ignored tests send each real API a bad key. The panel's
  replies are laid out by `text::layout_markdown`, which also finds their
  fenced blocks (`text::Block`), and their lines that are nothing but one
  span of code (`Block::inline`, a list item's after its bullet). A fenced
  block gets a row of Copy and, for a shell's (`Block::is_shell`), Run under
  it (`Hit::CodeCopy`, `Hit::CodeRun`); a span gets a small Copy at the end
  of its own line (`Row::copy`, `ui::small_button`), with no guessing
  whether it's a command, so no Run: such a line wraps short by the room the
  button takes (`layout_markdown`'s `room`), and the last line of a reply
  still coming in (not `whole`: the last entry while `AiPanel::busy`) is
  one only once a newline ends it, so neither the button nor the wrap comes
  and goes with the deltas. Copy reply goes on the reply's first row in
  view without a Copy of its own (`reply_copy_row`), or none. A command's
  card (`ToolCard::command`) has the same Copy on its line once it ran, and
  beside Run, Edit and Skip while it waits (`Hit::CardCopy`); the system
  prompt tells the model to put a command for the user in a fenced block of
  its own. `App::run_from_reply` runs a block's commands through
  `Session::run_typed` (one line as typed, several as a bracketed paste,
  noted for the history as the user's) where the shell's marks say it's at
  its prompt (`Session::at_prompt`), `command_risk` isn't high and the lines
  can go as written (`brackets_pastes`), else puts at the prompt (joined by
  `one_line` where the shell takes no bracketed paste); `put_at_prompt`
  writes a paste as typing whose line breaks aren't Enter, leaving out any
  other control character but a tab in a bracketed paste
  (`shell::without_keys`); a block holding one is neither run nor put
  there, in any shell (`key_in`, `toast.block_holds_a_key`), nor one with
  letters past ASCII that bash there may take as keys
  (`Session::letter_as_keys`, `toast.letters_as_keys`). A shell
  session's block runs its prompted lines only (`Block::commands`). A reply
  under the pointer shows Copy reply (`Hit::ReplyCopy`), and moving over
  the panel draws a frame when the hit or row under the pointer changes.
  The drawing keeps what it laid out in `AiPanel::laid` (`PanelLaid`: each
  row's text and the rows in view's character edges), so a drag selects
  (`AiPanel::selection`, places of row and character, `App::panel_selecting`)
  and Copy takes it before the terminal's while the panel has the keys,
  a row the wrap carried down joining the one above as written
  (`text::Wrapped`, kept per row in `PanelLaid::wraps`, so a long command
  pastes as one); a copy that fails toasts so (`App::copy_out`). Any new
  layout, Esc, Run and Select All clear it (`App::relayout`).
- The assistant's memory (`ai/memory.rs`, `ai.memory`, on at first): a
  journal in `assistant.sqlite3` beside config.toml (`Journal`: `tasks`, one
  a conversation, and `steps`, trigram-indexed as the history is; and
  `sent`, the panel's messages as typed, for its input's ↑), which
  `App` writes by itself, `journal_ask` at each message of the user's
  (`send_to_assistant`, the first beginning `App::task`) and `journal_step`
  as each tool's outcome comes (a command in `finish_command`, files read
  and written, searches, and an MCP server's call or resource read as a
  `Kind::Called` step, its exit 0 answered, 1 failed, none given up:
  `App::journal_call` from `server_answered`, `advance_tools`' timeout and
  `cancel_tools`, through `servers::call_step`, its arguments left out
  where `looks_secret` sees one in them or their `name=value` words,
  `memory::kept_answer` the start of the answer, and `call_log`'s line in
  the log, server, tool, end and time alone, the call's start kept on
  `JobState::Serving`'s `since`); facts a line each in `assistant-memory.txt`
  (`memory::remember`, `forget`, `read_facts`); and four tools offered only
  with it on (`tools::functions(memory)`, `AiSettings::memory`, which adds
  `MEMORY_PROMPT` to the system prompt): `remember`, `forget`,
  `save_progress` (a task's summary, next step and `Status`) and
  `search_journal`. Nothing `history::looks_secret` flags is kept
  (`kept_output` puts such output lines out). A new conversation's first
  message carries a `<memory>` block (`memory::context_block`, through
  `App::fresh_conversation`); `/clear` and a new provider end the task
  (`end_task`, `Left` unless `Done`). Progress saved as `Restart` has
  `Platform::start_once(true)` start Flow once at the next login (the
  Mac's LaunchAgent, `memory::launch_agent`; Windows' `RunOnce` value,
  `windows/src/start_once.rs`; Linux's autostart entry,
  `memory::autostart_entry`), which every start takes away again; at
  start `App::offer_carry_on` puts the task `Journal::offer`s (left for a
  restart, or working within a week) at the panel's foot as an
  `Entry::Resume`, opening the panel for a restart's: Carry on
  (`Hit::ResumeOn`, Enter in an empty input, `App::carry_on`, which sends
  `memory::carry_on_message` and journals into the same task) or Not now
  (`Hit::ResumeLater`, `leave_for_now`). Settings' **Memory** and
  **Remembered** (`SettingId::Memory`, `MemoryKept`: Open,
  `open_memory_file` through `memory::file_url`, and Clear, pressed twice),
  and `/memory` (`App::memory_notice`). Snapshots keep it in memory
  (`Memory::in_memory`) and show `resume.png` and `settings-memory.png`.
- Working on its own (`ai/own.rs`, `ai.own`, `ai.own_minutes`,
  `ai.own_tokens`): the `work_on_its_own` tool (offered with `ai.own` on,
  `tools::functions(memory, own)`, `OWN_PROMPT`) always waits for the user
  as a card (`ToolCard::plan`: Let it work, `Hit::Approve`; Ask each time,
  `Hit::Skip`; `App::plan_detail`). Let it work starts a `Grant`
  (`App::start_own`: its minutes `own::minutes`, its tokens counted from
  the meter, `ROUNDS_ON_ITS_OWN` a turn through `App::agent_settings`),
  under which `needs_approval` asks only where `own::decide` says
  (`Why`: rights not given, `tools::needs_admin`, `tools::restarts`; what
  `tools::always_asks`; a program `tools::hides_program`; a sensitive path,
  or a command that `tools::names_secret`; another plan), the card saying
  why and the system notifying with Flow behind (`notify_assistant`). It
  ends (`end_own`, `own::End`) when saved `done`, out of time or tokens
  (`check_own`, before each tool), stopped (Esc, `stop_ai`; the mode pill
  or Tab, `toggle_mode`; Settings' switch), or cleared, and the next tool
  result tells the model (`own_ended`, `End::told`). The panel's header
  and the title bar's pill say it (`View::own_left`). A command waiting at
  a password's prompt (`Session::password_prompt`,
  `session::is_password_prompt`) waits up to `PASSWORD_WAIT` and asks
  once (`ask_for_password`). With admin on Windows, Flow opens again as
  administrator (`Platform::relaunch_elevated`, `windows/src/elevate.rs`,
  `ShellExecuteExW`'s `runas`) with `--carry-on <task>` (`CARRY_ON`,
  `App::carry_on_at_start`, which `offer_carry_on` offers at once), the
  grant kept with the task (`Journal::save_own`, `tasks.own`, `own::Kept`)
  and taken up by Carry on there; an elevated Flow (`Platform::elevated`)
  shows an Administrator pill and saves no `tabs.json`. Snapshots show
  `own-plan.png`, `own-working.png` and `settings-own.png`.
- The assistant's own pane, questions, waiting, undo, Flow itself, the
  summing up and helpers: a command goes to the pane in front while it's
  free (`Session::taken`), else to a pane of its own (`open_side_pane`,
  `Pane::by`, a `ui::PaneFor` drawn as a label; `assistant_pane`,
  `ai.pane`), a fresh one's command waiting for its prompt
  (`JobState::Opening`, `ready_for_command`, `command_ready`; under a
  grant until its shell has said its folder too, past `OPEN_WAIT`
  `card.busy`). Which is settled as the call is judged, before anything
  opens (`command_pane`, through `App::place_for`'s `Place`: the pane in
  front's folder for files, and the command's `CommandPane`, the
  assistant's `Own`, the user's `In` or a pane to open `Beside` the
  user's, with that pane's folder, or for one to open the folder its
  shell will start in, `beside_start`, `shell::start_folder`, each read
  once), kept on
  `JobState::Awaiting` while a card waits and handed to `start_job`,
  whose `work_session` sends it there (`card.pane_taken` where the user
  has taken theirs since; its own pane closed meanwhile, it's judged
  again, `card.pane_closed`) and opens a pane in that folder
  (`open_side_pane_from`). A call isn't judged while a pane it goes by
  draws the prompt after a command that just ended (`prompt_coming_for`,
  `waits_for_prompt`; `Session::drawing_prompt`, from 133;D to 133;B,
  `ShellState::prompt_due`, at most `PROMPT_WAIT`), when it counts as
  taken and its folder may be unsaid, so a batch's next command stays in
  the pane the last ran in. Under a grant `run_in` types it only while
  the pane's shell is still in the folder judged (`same_folder`), else
  it's judged again where it is (`judge_again`, `card.folder_moved`;
  twice, `card.folder_unsteady`).
  A command at a question (`session::question`, after `QUESTION_STILL`)
  waits up to `QUESTION_WAIT`, its tab brought forward, the user told
  (`ask_user_to_answer`) and the model too. `wait` is
  `JobState::Waiting`, which draws no frames (`ToolBatch::waiting`, a 1 s
  wake in `about_to_wait`). `ai/undo.rs` keeps a file's first state per
  task before `write_file` (`Undo::keep`, `undo_key`), put back by a card's
  Undo (`Hit::CardUndo`), `/undo` or `undo_changes`. `flow_help` and
  `flow_settings` (through `settings_answer`). The worker sums up older
  turns past `AiSettings::sum_at` (`Worker::sum_up`, `sum_cut`,
  `fold_results`). Helpers: `ask_helper` starts an `Agent::helper` (a
  `Role::Helper`, `HELPER_PROMPT`, `Toolset::helper`, its events
  `UserEvent::Helper`) in a pane of its own (`App::start_helper`, `Helper`,
  `JobState::Helping`, the batch's results waiting for every report and
  sent in the calls' order), its calls run by `advance_helper` within
  `own::helper_refuses`, its report its result (`helper_done`); they end
  with the turn (`end_helpers`). Snapshots show `helpers.png` (a stage of
  its own, `HELPERS_STAGE`) and `undo.png`.
- The panel's input (`panel_input.rs`, `docs/design/assistant-input.md`):
  `AiPanel::input` is a `panel_input::Input`, the text as characters, the
  caret and the selection's other end (`anchor`), with its undo (`keep`,
  a run of typing or of deleting one step, a paste, a cut or `set` one of
  its own, `UNDOS`) and at most `Input::MOST` characters
  (`toast.input_full`); `AiPanel::history` a `Recall`, the messages sent
  (`sent`, `MOST`), which ↑ on the first row and ↓ on the last go through
  (`older`, `newer`), keeping what was being typed and each edit made to
  one (`edits`) until the next is sent, `/` commands among them
  (`App::remember_sent`). While `ai.memory` is on, each
  sent goes into the journal's table of its own, `sent`
  (`Journal::keep_sent`, the newest `SENT_KEPT`, none that `looks_secret`),
  which `App::new` begins it from (`Journal::sent`, `Recall::from_past`),
  so ↑ brings them back after a restart; Remembered's Clear forgets them. None of it draws, so it's tested
  alone. `ui::draw_input` lays the text out in rows (`panel_input::rows`:
  after a space, around Chinese and Japanese characters, or where a long
  word runs out of room; a line break ends one), each character as wide
  as `text::advance` says, which `Canvas` draws by too, up to a third of
  the panel's height, the rows in view following the caret
  (`Laid::follow`), what's selected under the text and what the input
  method composes spliced in at the caret; it keeps them in
  `AiPanel::input_laid` (`Laid`, by the input's `version`, a number no
  other input has had, so a snapshot's put in its place is never taken
  for it), which the keys (`rows_of`, laid out again where the text
  changed since) and the mouse (`place_at`) go by. `App::ai_key` hands
  the input's own keys to it, none of them in `keys::TABLE`: ⇧ with any
  move selects, ⌥ (Ctrl) by word, ⌘← ⌘→ or Home and End the row's ends,
  ⌘↑ ⌘↓ or Ctrl+Home and Ctrl+End the text's, ⇧Enter a new line, ⌘X and
  ⌘Z (no menu item takes them) or elsewhere Ctrl+A, C, X, Z and Y, the
  panel not being a shell (`cut_input`, `type_into_input`). Select All,
  Copy and Paste go to the input while the panel has the keys
  (`select_all`, `copy_selection`, its selection before the
  conversation's, `paste_text`, keeping line breaks). A click in the input
  puts the caret there, ⇧ selecting, two a word and three its line, and a
  drag selects (`App::input_selecting`); a selection made there takes the
  conversation's place, becomes Linux's primary selection, and a middle
  click pastes where it's clicked. Readers get the selection
  (`access::Field::anchor`, `selected_from`). Snapshots show
  `assistant-input.png` (`App::show_input`).
- Files for the assistant (`ai/files.rs`, `ai/office.rs`, `app/attach.rs`;
  `docs/design/assistant-files.md`): a file dropped on the panel
  (`App::file_dropped`, where `attach::drop_place` says from the pointer
  `Platform::pointer` reads, winit giving a drop no place: the Mac's
  `NSEvent::mouseLocation`, Windows' `GetCursorPos`, X11's `QueryPointer`;
  none on Wayland, which has no drops in winit), picked with Attach
  (`Hit::Attach`, `attach_dialog`) or a picture pasted with no text on the
  clipboard (`paste_picture`, arboard's `get_image`) is read on a
  `flow-attach` thread (`files::read`, `pasted`) into an `Attached`
  (`Kind`: a picture as it is where it's small enough, else made smaller to
  `LONG_SIDE` as a PNG or a JPEG past `MOST_PICTURE`, its thumbnail
  premultiplied; a PDF and its pages, `pages_of`; text cut at
  `MOST_TEXT`; an Office file and the text `office::text` takes out of its
  zip, the central directory read by hand, `miniz_oxide`'s inflate and
  `roxmltree`; refusals a `Refused`, in the user's words or English),
  coming back as `UserEvent::FileRead` to a chip under the panel's input
  (`AiPanel::files`, `reading`, `ui::draw_chips`, `Hit::AttachRemove`,
  and a secret, `is_sensitive_file` or a private key's header, waiting for
  `Hit::AttachConfirm`). `send_to_assistant` takes them all
  (`files_to_send`, none while a secret waits) into `Agent::ask_with`,
  `Entry::Files` naming them in the transcript and the journal; the
  worker puts each one's blocks (`Attached::blocks`, Claude's `image`,
  `document` and `text`, an Office file a `document` for OpenAI alone)
  after the words, `openai.rs` and `gemini.rs` turning them into
  `input_image`, `input_file` and `inlineData`. `read_file` reads them
  too (`tools::read_any`, `files::is_attachable`), its `ToolOutcome::files`
  going after the turn's results. The summing up weighs a file by its
  tokens, not its base64 (`files::weigh`). While `HoveredFile` holds,
  `App::drop_shown` lights where it would land (`ui::draw_drop`); a drop
  on a pane types its path there, `shell::quote`d where
  `Session::types_letters` (at the prompt its hook drew), else (a
  program runs there, a shell started inside it or over ssh among them,
  or bash's hook said that prompt may take them as keys)
  `shell::quote_ascii`d, letters past ASCII into zsh or bash as their
  bytes, whatever the pane's shell, since what reads them may be a bash
  without Flow's hook (`a_file_dropped_while_a_bash_runs_inside_the_pane_runs_nothing`;
  one holding a control character untyped, `attach.untypable`'s
  toast). Snapshots show
  `assistant-files.png` and `assistant-drop.png` (`App::show_files`).
- The user's instructions and skills (`ai/skills.rs`): the instructions
  file (`INSTRUCTIONS`, `read_instructions`) and the skills' folder (`DIR`:
  `<name>.md` or `<name>/SKILL.md`, front matter read by `front_matter`,
  `skills::list`) are read before each message
  (`App::refresh_assistant_files`, the agent configured again only on a
  change) into `AiSettings::instructions` and `skills`, which
  `skills::prompt_part` adds to the assistant's and its helpers' system
  prompts; `read_skill` (offered with skills there, `Toolset::skills`) and
  `save_skill` (the assistant's alone, `skills::save`). Settings' Your
  instructions and Skills open them (`open_instructions`, `open_skills`);
  `/skills` lists them. Snapshots show `settings-skills.png`. Packages
  `flowapp` installs may bring skills (`apps::skill_dirs`: each installed
  package's version folder's `skills/`, an app's beside its programs, or
  a package of nothing but skills, `skills::FILE` naming it, kind
  `apps::SKILLS`, the catalog's `skills` map), each named
  `<package>:<name>` and read only once turned on (`ai.skills_on`, which
  programs can't change). Settings' Skills section (`Section::Skills`,
  after the Assistant's, `skills_rows`) lists them all: Your skills
  (`SettingId::Skills`), a switch for each of the user's own
  (`SettingId::Skill`, on until switched off, `ai.skills_off`), then each
  package's row (`SettingId::SkillPackage`: how many are on, All on and
  All off) and a switch for each of its skills (`SettingId::PackageSkill`,
  off until turned on), by their places in `settings::SkillsView`, which
  `App::see_apps` reads into `skills_view` (`put_in_list`).
  `App::skills_now` hands every installed package's folder, `skills_off`
  and `skills_on` to `skills::gather`, the user's own first, which `read`
  finds as listed. Snapshots show the section made up
  (`SkillsView::made_up`, `App::show_skills`). Settings' footer shows the
  chosen row's whole hint, up to `ui::HINT_LINES`, the list giving up the
  room. An install that
  brings skills says where they're turned on (`toast.skills_off`,
  flowapp's `say_skills`, reading config.toml through
  `skills::turned_on`).
- A task's steps: `set_steps` (`tools::StepStatus`, `steps_from`,
  `steps_to_json`) sets `AiPanel::steps`, drawn under the panel's header
  (`ui::steps_window`, `STEPS_SHOWN`), its job making no card (an entry
  past the end, which `card_mut` passes over); `start_own` seeds it from a
  plan, `TurnDone` clears it once all are done, the journal keeps it
  (`Journal::save_steps`, `tasks.steps`) for the resume card and Carry on,
  and readers hear it in the panel's label (`ui::steps_spoken`).
- Ask why: at Enter, `Session::write` notes the command and the line its
  output starts on (`ShellState::command`, `output_from`; 633;E, with the
  pane's nonce alone, and 133;C correct them), and at 133;D a command of
  the user's that failed (not 130, not the assistant's tracked one) is
  kept with the end of its output
  (`ShellState::failed`, a `Failure`, `tail_of`), until the next starts.
  `Finished` carries the command too. While the shell is at its prompt,
  `App::failure_shown` offers it, `ui::draw_ask_why` draws the pill
  (`Hit::AskWhy`, `Command::AskWhy`), and `App::ask_why` opens the panel and
  sends it through `send_to_assistant` (the tail of `submit_ai`), shown in
  the user's language, asked in English.
- `secrets.rs` stores one API key per provider (its env var first, then
  Credential Manager or Keychain through `keyring`). Flow never opens the key
  dialog on its own, not even at first start: without a key, the panel and
  the title bar's "Add API key" point to Settings, where the provider is
  picked and its key added. `ai/usage.rs` keeps the running token total on
  disk, and the agent stops before a request once it reaches `ai.token_cap`.

**A frame** (`App::redraw`):
1. `Effects::frame` (`fx.rs`) is the CPU side of the liquid for the current
   `Style`: splats and drops that stir the fluid, lightning, the underwater
   style's bubbles (`fx/bubbles.rs`, drawn as their own quad kind), the rain
   style's drops (`fx/rain.rs`: as many a second as `fx.rain` says through
   `rain::rate`, a drizzle to a downpour of fifty times it, each striking as
   a `RainQuad::SPLAT` with `SPRAY` round it for `IMPACT`, a heavy one
   bursting into a run, a smaller one hitting a bead running into it
   without the bead splatting again; runners are drawn onto the wet
   tracks earlier ones and the mouse left (`Rain::track_below`, a `Streak`
   with `track`), run down them without catching or leaving beads, and
   keep them young and wetter (`Streak::flow`), so a well-run track is a
   stream, a wider clear channel (`STREAM`; lens water down its middle
   mixed its pieces' lenses into white specks); a path wiped across the
   glass holds a runner a moment before it breaks through
   (`Rain::dam_across`, `Drop::dammed`); `rain::wetness` makes runners hold
   their course, widens their trails, paths fog over sooner and the glass
   shader's `sheet` run
   water down it, bending the night, through `FrameStyle::wet`; past
   `TAPS` a second the taps give way to `Cue::Rainfall`, laid by
   `App::hear_rainfall` as the hum is, each voice its own noise and shade
   (`Voice::seeded`, from `Sound`'s own random numbers); and with
   Electricity up and the rain heavy (`rain::storminess`), `Effects::storm`
   strikes far off: `FrameOutput::flash` lights the night in `rain_scene`,
   and `Heard::Thunder` comes `STORM_NEAREST` to `STORM_FARTHEST` seconds
   after, its `near` setting how loud), and the splashes queued with
   `Effects::react` when a command ends or the assistant works. Programs ask
   for effects at a cell with OSC 7731 (`osc.rs` `parse_fx`, any program, not
   just the shell's hooks): `ShellState::effects` keeps them (`AskedFx`) with
   the line the screen's top row was and which screen it was, and
   `App::watch_effects` places them for the tab in front within its
   `Allowance` and hands them to `Effects::request`, with `fx.reactions`,
   `fx.program_effects` and the liquid on. A program's own pictures cover the
   liquid, and so do the backgrounds it gives its cells, so its bolts, bursts
   and ripples also glow over both, under the letters (`Effects::draw_over`,
   drawn right after `end_backdrop`, with the cells' backgrounds put in under
   it: `Canvas::ground`); the liquid itself stays under the cells. Over every
   style, `fx/signals.rs` keeps the rings and the progress tank, which the
   glass shader draws (`signals` in `ui.wgsl`) from the `pulse` and
   `progress` uniforms. They run on real time, unlike the liquid.
2. `ui::draw` (`ui.rs`) lays out the title bar, terminal grid, assistant panel
   and dialogs as instanced quads. `Canvas::finish` stacks a frame's
   quads (`render::stack`): the backdrop (the liquid's bubbles and
   lightning), the images' pieces, the cells' backgrounds (`Canvas::ground`,
   which `ui::fill_cell` lays as the grid is drawn), then the rest
   as drawn, programs' effects first, then the UI with the cells'
   letters. Block graphics and box drawing are drawn from each cell's own
   pixels rather than the font (see Double lines and mosaics). Glyphs
   come from a cosmic-text atlas (`render/atlas.rs`),
   1024 px square at first and doubled to 2048 between frames once
   it's three quarters full (`Atlas::grow_if_crowded`), keeping besides
   the font in use up to `KEPT_FACES` others (`atlas::Face`: a family at
   a size and line height, its cell and its glyphs' places, swapped in by
   `Atlas::set_font`, `use_face`; `metrics_for` measures one without
   drawing in it), so tabs in other themes' fonts switch at once, over the
   installed fonts and the ten built in for the themes (`BUILT_IN`, from
   `assets/fonts/`, whose `NOTICES.txt` each OS's release script adds to
   THIRD-PARTY-NOTICES.txt; unscii's two sizes both call themselves
   "unscii", so `load_built_in` renames each to its full name, and SGI's
   Screen was bitmaps only, made outlines by `assets/fonts/bitmap2ttf.py`).
   The teletype's, Flow Teletype, is Flow's own: `assets/fonts/teletype.py`
   draws each glyph as strokes along center lines traced by a round pen
   (grown and shrunk back by `INK`, which fills inside corners as the
   Model 33's ink did), traces the outline by marching squares and writes
   the TrueType file with plain Python, its lowercase small capitals, and
   Latin-1 drawn so accented text stays in it; change the script and run it
   again rather than the .ttf, which it makes the same each time. The
   Flow Instruments Home Computer's, Flow Instruments, is Flow's own too:
   `assets/fonts/instruments.py` holds each glyph's 8 × 8 dots as text and
   writes them as squares of the outline, the same each time as well; a
   config from before it wore unscii-8 and takes it as it's read
   (`looks::fill_in_fonts`, `FONTS_BEFORE`). The Flow notanAmiga CLI's,
   Flow notaTopaz, is Flow's own as well, drawn in the manner of the
   Amiga's Topaz, which isn't free to ship: `assets/fonts/notatopaz.py`
   holds each glyph's 8 × 8 dots and writes each as a rectangle half as
   wide as it's tall, as the machine's screen had them, so its uprights
   are two dots wide and its tails take the cell's last row; unscii-16,
   whose cells are the same shape, comes after it. A family
   whose letters differ in width (`Atlas::centered`, such as Special Elite,
   which the teletype's look keeps after its own) has cells as wide as its
   M, and each one-cell letter is set in the middle of its cell;
   `text.rs` wraps the panel's text and its markdown.
   Uncolored text, the cursor and the glass's tint (`FrameStyle::tint`) take
   `palette::ink()`, which `App` sets from `colors` in the config; it's
   process-wide because the sessions' reader threads also answer programs'
   color queries with it. `Canvas::glow` is
   scaled by the Glow setting, `steady_glow` isn't (shadows, tab alerts).
   The cursor glides toward its cell, so `App::animating` keeps frames coming
   until it's there. With Cursor glide off (`fx.cursor_glide`), or without a
   GPU, a step of a few cells along its row (typing, Backspace) it takes at
   once (`ease_cursor`), so it never trails what was just typed. Its shape and
   blinking are the emulator's default style (`CursorConfig::style`,
   `Session::set_cursor_style`), which programs can change; a blinking cursor
   gets a frame only when it lights up or goes dark (`blink_phase`, in
   `about_to_wait`), not frames all the time.
3. `Renderer::render` (`render/mod.rs`) steps the GPU fluid (`render/fluid.rs`,
   `shaders/fluid.wgsl`: stable fluids for velocity and dye, plus a wave
   equation for the water styles; `Fluid::show` keeps its textures tiny for
   plain glass and the ripples' for water only, through
   `Renderer::show_liquid`, which a snapshot's steps between its pictures
   take too, `simulate_only`, or a style's picture came just after its
   textures were made again, empty, and the oil's was black), draws the rain map when raining
   (`render/rain.rs`, `shaders/rain.wgsl`), then in one pass draws the
   full-screen glass (`fs_glass` in `shaders/ui.wgsl`) and every quad
   (`fs_quad`). With `fx.crt` on, that pass draws into a texture instead, and
   `render/crt.rs` (`shaders/crt.wgsl`) shows it as a tube would: a glow
   blurred at quarter size, then the picture bent by its curvature, with
   interlaced scanlines and the phosphor's afterglow, kept in a pair of
   textures that take turns, made only while Phosphor decay is on (else
   `fs_crt_flat` draws without them). They and the glow are 8-bit, so the
   afterglow also loses a step each frame, or a faint trace would never
   fade. The afterglow fades by the CRT's own clock, not
   the liquid's, which steps without drawing. `App::animating` keeps frames
   coming for interlacing and decay while the window is focused. Once the
   afterglow has faded since the picture last changed (`changed_at`, which
   `request_redraw` sets and `next_frame` doesn't) and nothing else moves,
   `App::crt_idle` lets them come at most every `CRT_IDLE` (30 a second),
   unless the lines interlace. The afterglow's pair of textures exists only
   while it's drawn (`CrtLook::steady`), so not behind other windows. Its parts
   are `[crt]` in the config (`fx.crt` is the strength), with presets built
   in (`settings::builtin_crt_presets`) and saved by the user
   (`crt.presets`). The curve bends the pointer too: `App::on_picture` runs
   it through `crt::warp`, the shader's `warp` in Rust, so hits and
   selections land on what's drawn; resizing and the context menu use the
   real position (`mouse_raw`).

- `fs_glass` branches on `G.mode`, which is `Style::shader_mode()`: 0 clouds,
  1 water above, 2 water below, 3 oil, 4 rain, 5 none (plain glass; the app
  runs no simulation or effects for it, see `App::liquid_on`). Binding 4 is
  the wave texture, or the rain map when raining.
- Efficient mode (`ui.efficient`, `App::efficient`, the title bar's
  `Hit::ToggleEfficient`, whose pill alone reads 🌱e-mode, `title.efficient`,
  everything else keeping Efficient; `Command::ToggleEfficient`) lies over the
  settings without changing them: `App::style` gives `Style::None`,
  `crt_look` no strength, the rings don't pulse, the machines are quiet
  (`machines_heard`), effects' sounds go with the effects, the cursor
  skips `ease_cursor`, and a start screen is typed at baud 0, one already
  typing told to hurry (`Session::hurry`, `ShellState::hurry`). Flow's own
  turning logo (a `Shown` made `flows_own`, as `logos::sgi`'s is) is held
  still by `prepare_frame` (`Shown::hold`: its own clock stops, it's drawn
  once more, `animating` is false, and let go it turns on from there).
  A left click on it (`App::align_logo`, `Session::align_logo`) has
  `Shown::align` turn it back the shorter way to how it stands
  (`Shown::turn` gives a `Turn`, a quaternion and the bob, eased from
  where it was over `ALIGN_TURN`) and stop its clock at its start for
  `ALIGN_REST`; no frames come while it rests, and `App::models_rest`
  (`Shown::rests_until`) wakes Flow when it's to turn on.
  `ui.efficient_on_battery` turns it on while the computer saves power
  (`App::efficient_for_power`, never touching `ui.efficient`):
  `Platform::power` (`power.rs`'s `Power`: the Mac's IOKit power sources
  and Low Power Mode, Windows' `GetSystemPowerStatus`, Linux's
  `/sys/class/power_supply` read by `power::linux`, so it's tested
  anywhere) is read before the first tab opens (so its start screen is
  typed and heard as efficient mode has it), then on a `flow-power` thread
  on focus and at most every `POWER_EVERY` as `about_to_wait` runs, coming
  back as `UserEvent::Power`; `power::Follow` keeps it, and the user's turning
  efficient mode off while it's on for the power, which holds until the
  computer stops saving power (plugged in, Low Power Mode or energy saver
  off) or Flow quits, since it isn't saved. The title bar's pill says so
  (`View::efficient_for`).
  Snapshots and recordings turn it off and never read the power, and wear
  their themes whole whatever Theme details say. `cursor.speed` scales
  `ease_cursor`'s rate.
- Pacing: in front, each frame that reaches the screen asks for the next
  (`App::redraw`) and vsync paces them; `about_to_wait` is the fallback, and
  behind other windows it draws at most 20 a second. Screen refresh
  (`window.refresh_rate`, `window.vsync`) can cap them instead
  (`App::frame_interval`), and then output that comes too soon after a frame
  waits for the next (`output_waiting`); without vsync the surface presents
  with `AutoNoVsync` (`Renderer::set_vsync`). At most one finished frame waits
  for the screen on Windows, two on the Mac (`desired_maximum_frame_latency`).
  Without a GPU (`Renderer::software`, a CPU adapter such as Windows' Basic
  Render Driver in Parallels) frames cost tens of milliseconds, and a key
  would queue behind them. So there the liquid and the focused panel's glow
  stop keeping frames coming for `TYPING_HOLD` after each key
  (`App::typing_held`), the CRT never does (`CrtLook::steady`), and a key
  sent to the shell isn't drawn at once but with its echo, or after
  `ECHO_WAIT` (`key_frame_due`, handled in `about_to_wait`).
  `Renderer::render` says what became of a frame (`Shown`). On macOS wgpu
  hands out no texture while the window isn't visible (`Shown::Hidden`),
  which a window just shown isn't yet: the first frame, the focus's and
  often the shell's prompt come before it's on screen. With nothing moving
  (Efficient mode) nothing would ask again, so `App::seen` (`Seen`) draws
  it once `WindowEvent::Occluded(false)` comes, without asking meanwhile,
  and draws a frame lost to the surface being set up again
  (`Shown::Again`) at the next frame's time.
- The surface format is chosen 8-bit and non-sRGB, so shaders write display
  values (Wayland drivers list half floats and 10-bit formats, whose alpha
  has 2 bits, ahead of it). The
  window is transparent and output is premultiplied alpha, over DWM acrylic on
  Windows or the window server's blur on macOS.

**Images.** Programs send them with the kitty graphics protocol (APC `_G`), which
`osc.rs` finds in the byte stream like its other markers (with XTVERSION and
`CSI 16 t`, which alacritty_terminal doesn't answer). On the reader thread,
`graphics::ImageStore` (one per `Session`) decodes and keeps them, answers the
program, and has `session::place_image` write each one into the grid as
Unicode placeholder cells (U+10EEEE; image id in the foreground color,
placement id in the underline color, row and column in kitty's combining
marks), the same cells programs write for virtual placements (`U=1`). So
images scroll, clear and go into scrollback as text does. Text rewraps too
when the grid narrows (alacritty_terminal reflows the main screen), which
would split a picture's rows, so `Session::resize` has
`graphics::crop_pictures` blank the main screen's picture cells past the new
edge first, and `graphics::regrow_pictures` fill a picture cut at the old
edge out again from its placement as it widens (into blank cells only, as
far as the placement goes). While a program holds the alternate screen,
both reach the main one through `swap_alt` and back (`on_main_screen`,
which puts the alternate grid back as it was). The same wraps
`resize_as_conpty` on Windows; ConPTY's own repaint after a resize is
another matter. `ui::draw_terminal`
turns placeholder cells into pieces of the image (`graphics::tile`), which the
renderer draws from each image's own texture (`ImageBatch`, kept by
`Image::texture`, made on first use by `render::upload_image` and dropped
`IMAGE_KEEP` after the last, or at the next frame once the image is gone
from everywhere, sent again or deleted: `Image`'s `Drop` notes it for
`graphics::take_gone` (`note_gone`), so a picture sent anew each frame
keeps no seconds of frames on the GPU) over the liquid's bubbles and lightning and under
everything else (`Canvas::end_backdrop`), cells' backgrounds (`Canvas::ground`)
and programs' effects (`Effects::draw_over`) included. Images bend with
the liquid rather than slide: each corner of a piece moves by the flow where it
is (kind 8 in `ui.wgsl`), so pieces that meet stay joined, within an image and
between images laid side by side, as box drawing's strokes do (kind 9). An image within a pixel of its cells fills
them (`graphics::tile`), since cells are a fraction of a pixel wide. A delete
of every image (`d=a`, `d=A`) takes only what the screen in use shows, as
kitty's spec says: the store can't see the grid, so it hands that back
(`Response::clear_screen`) and the session reads the screen's placeholder
cells (`shown_images`) for `ImageStore::delete_shown`. A game leaving the
alternate screen this way keeps the pictures in the main screen and the
scrollback. The startup logo is such an image that Flow places itself
(`App::banner`, `Session::show_banner`; which logo, `App::startup_logo`:
Flow's icon, the system's, the chip maker's from `logos.rs`, Intel's or
AMD's wordmark set from the system's fonts by `Atlas::word`, or an old
machine's maker's with its theme, Sun's emblem from its artwork or SGI's
cube of tubes from a model of it turning, both in `assets/logos/` (whose
README says how they were made), or the Amiga's Boing ball, a mesh Flow
makes (`logos::boing_ball`), spinning on its leaning axis alone
(`Shown::spinning`, `Turn::spin`); the makers' logos are their owners',
`docs/trademarks.md`; its last line's graphics API by its logo where Flow
has one, `logos::api`'s Vulkan, OpenGL, Metal and Direct3D 12's (DirectX's
wordmark, black, drawn in the text's ink by `ApiLogo::in_ink` as X11's X
is), one row tall in whole cells, `ApiLogo::in_row`, put by
`session::put_api_logo`, Metal's icon alone as Vulkan's and OpenGL's
wordmarks are, the name where the line hasn't room; Windows' repaints put
it back with the banner's rows, `windows/tests/shells.rs`),
between `Session::launch` and
`Session::start`, so the shell's first output comes after it; on Windows,
ConPTY starts by asking where the cursor is. ConPTY doesn't know the logo is
there: once a program has drawn over its rows on the alternate screen, its
repaints (on leaving that screen, and after each resize) blank them from the
top. So on Windows `session::advance` watches for those (`?1049l`, or
`Session::resize`), notes the logo's rows, and puts back those the repaint's
frame (`?25l` … `?25h`) left blank (`Banner`); a clear has neither and still
takes the logo away. A model's cells are Flow's own too, so they're kept
the same way, cell by cell since a model sits among a program's text
(`Emulator::models`, which `place_model` and `clear_model` keep, and
`Repaint::cells`, noted on either screen and put back on that one, only
for a model still placed; `docs/design/models-through-conpty.md`). Below
the cursor, where ConPTY's screen is blank, it paints over them again
whenever a line there wraps, with no repaint due, so a program makes its
rows first (`windows/tests/models.rs`).

Sixel pictures (a DCS, `ESC P … q … ESC \`, `Marker::Sixel`, found by the
scanner's `Dcs` states up to `MAX_DCS`; a DCS with anything but digits and
`;` before its `q` is the emulator's) and iTerm2's inline images (OSC 1337
`File=` with `inline=1`, `Marker::ITermImage`, up to `MAX_ITERM`) come the
same way. `sixel.rs` decodes a sixel from DEC's spec: the raster (`"`) and
its pixels' shape, else P1's, applied by repeating rows; 1024 color
registers (`REGISTERS`) begun from the VT340's sixteen, set in RGB or HLS
(DEC's hue, blue at 0); `!` repeats, `$` and `-`; P2's 1 leaving what isn't
painted clear, else register 0's color; each band of six rows kept a column
at a time, so it grows by pushing, within `MAX_SIDE` a side and
`MAX_PIXELS`. `iterm.rs` reads `File=`'s arguments (`parse_args`), decodes
its PNG (`graphics::decode_png`), JPEG, BMP or TIFF (the `image` crate,
its png, jpeg, bmp and tiff features alone, `iterm::decode_still`, a
TIFF's first page, deeper ones brought to 8 bits), GIF or WebP
(`frames.rs`), refusing anything else with a line in the log naming it
(`kind_of`: a PDF, a HEIC, an AVIF), and works out its cells (`iterm::cells`: cells, `px`, `%` or `auto`, fitted or
stretched). `ImageStore::show` keeps either under an id of Flow's own and
places it through `place`, as kitty's `a=T` is, shrunk to the room right of
the cursor (`session::at_cursor`), and `place_image` writes its cells and
leaves the cursor as each terminal does (`Then`): kitty's and iTerm2's after
the last row, right of it (`Beside`), a sixel's at the start of the line
below (`Below`, xterm's scrolling mode) or with sixelScrollsRight beside it
too, `C=1` and `doNotMoveCursor=1` where it was (`Stay`); a sixel in DECSDM's
display mode goes at the screen's top left (`Top`), as wide as the screen
and cut at its foot, written into its rows as `place_logo` writes a logo,
nothing moved or scrolled. Through ConPTY (`Emulator::conpty`, a Windows
session's), which never learns a picture's room and keeps its cursor on the
row the picture began on, a `Beside` one on the main screen rises from the
cursor's row instead (`rise_into_place`: the lines above scrolled up by all
but one of its rows), so the next line ConPTY draws comes below it. The body of every OSC 1337 is kept from the emulator
(`Piece::Drop`, the scanner's `dropping`), since vte keeps an OSC whole and
alacritty spells out every byte of one it doesn't know for its log; its
`ESC ] 1337 ;` and its end still go, so the emulator reads an empty one. A
sixel's own bytes go on, which vte's DCS hooks only log. DA1 says sixel:
`EventProxy` puts Flow's answer (`session::DA1`, `?62;4;22c`) in place of
alacritty's `?6c`, and the pipe gives the same; XTSMGRAPHICS
(`Marker::GraphicsQuery`, `graphics_answer`) gets the 1024 registers, and
the text area's whole pixels, at most `MAX_SIDE`, as the largest sixel shown
whole. Down the pipe on Windows, where ConPTY drops a sixel's DCS whole, a
sixel goes where ConPTY's cursor is once its output has stopped
(`wait_for_paint`, `show_piped_sixel`): ConPTY never learns it's there, so
nothing from the cursor's line down may move, and it goes in lines of its
own above that line (`Then::Above`, `rise_above`: the screen scrolled up by
its rows, the lines from there down put back), or above the prompt's first
line where the shell is at its prompt (the cursor on `input_start`'s line,
since in PowerShell, cmd and BusyBox, which don't say when a command
starts, a line typed ahead has `at_prompt` true all through it);
mid-line or with sixelScrollsRight it rises from the cursor
(`rise_into_place`), in DECSDM at the top left, and on the alternate screen
at the cursor, which stays. The pipe keeps DECSDM and sixelScrollsRight and
answers DECRQM for them; iTerm2's pictures, which ConPTY passes on, it
passes over. `macos/tests/pictures.rs` prints one of
each from zsh and bash in a real PTY, then a PNG in parts and an animated GIF.

DECSDM (80) and xterm's sixelScrollsRight (8452), which alacritty_terminal
doesn't keep, are the scanner's `Marker::Modes` (`CSI ? Pm h` and `l`; RIS,
`ESC c`, is `Modes::RESET`, found after any ESC by `OscScanner::escape`),
kept on the `Emulator` (`sixel_display`, `sixel_right`). Flow answers DECRQM
for either (`Marker::ModeQuery`), `EventProxy` keeping back alacritty's own
answer, which says it doesn't know them (`UNKNOWN_MODES`). iTerm2's file in
parts (`MultipartFile=`, `FilePart=`, `FileEnd`: `Marker::ITermFileStart`,
`ITermFilePart`, `ITermFileEnd`) is gathered as base64 on the `ImageStore`
(`start_file`, `file_part`, `end_file`, at most `MAX_DATA` of it), decoded
once at its end and shown as a `File=` is (`session::show_iterm`). A picture
wrapped for tmux (`DCS tmux;`, its ESCs doubled) needs nothing of its own:
the first ESC of a pair ends the DCS the scanner passes over, and the second
begins the picture's escape (osc.rs's
`pictures_wrapped_for_tmux_are_found_as_they_would_be_unwrapped`).

A GIF or a WebP that moves (`frames.rs`: the `gif` crate, its frames laid
over each other by their disposal methods, cleared to transparent as
browsers clear them; `image-webp`, which lays a WebP's itself) is one
`Image` holding its `Frames`, each frame an `Image` of its own with its own
serial, the one holding them with no `pixels`. They share one texture:
`Frames` takes a serial of its own for it (`Reel`, `Frames::reel`, held in
an `Arc` its copies share, whose `Drop` tells `take_gone` once the last
goes), each frame carries it (`Image::reel`, so `Image::texture` is the
reel's; neither a frame nor the picture holding them notes its own serial
gone), and `upload_image` writes a frame's pixels over the texture with
`queue.write_texture` only when the frame drawn isn't the one it holds
(`ImageTexture::holds`, a frame's serial: a frame edited is a new `Image`
in its place, so it's written too); `stack` batches pieces by texture.
`Image::bytes` counts every frame for the quota, `ImageStore::held` lists
the reel once for the GPU's count (one frame's worth, in
`Renderer::images_gpu_bytes`, About's Resources and the tab card), and
`MAX_FRAMES` and `MAX_DATA` bound one (the frames read play). Kitty's
`f=100` takes them too (`graphics::decode`). `Frames::at` gives the frame
shown at a time (played as many times as its loop count says, a GIF's
plus one; a frame without its own time `UNSET`'s 100 ms) and when it gives
way, in closed form (`Frames::position`, from `ends`, each frame's end in
a play, halved over), from where it was last changed (`from`, `since`,
`played`); `ui::draw_terminal` draws that frame in each cell and hands the
time to `Canvas::changes_at`, which `App::prepare_frame` keeps as
`pictures_due`, and `about_to_wait` draws a frame then, as for
`models_rest`: an animation costs a frame for each of its own and none
between. The ignored `render::tests::an_animation_holds_one_texture_on_the_gpu_written_only_as_its_frame_changes`
reads the texture back (it has `COPY_SRC` under `cfg(test)` alone).

Kitty's own animation commands are `graphics/animation.rs` (`a=f`
`ImageStore::frame`, `a=a` `play`, `a=c` `compose`, read from `Command`'s
keys as its doc says: `z` the gap, `big_x` and `big_y` its `X` and `Y`), on
the same `Frames`, played as kitty plays them (`frames::State`: `Stopped`,
where `Frames::kitty` begins from the picture's own pixels, the root
frame, with no gap until given one; `Loading`, held at the last frame
until `push` brings another, which shows at once; `Running`). A frame of
zero gap (`z` below 0, or the root's) is passed over; `settle` notes where
it is before each change, so play goes on from the frame it's at.
`ImageStore::animate` makes each change on a clone of the frames (cheap:
`Arc`s) and puts a new `Image::animated` in the old one's place under its
id, its placements and its place in `order` kept, the store's `bytes`
following, so a frame being drawn stays as it was and a command that
fails changes nothing; a still picture becomes one frame first. `a=f`
answers `r=<frame>` as kitty does, `a=a` only errors, and frames past
`MAX_FRAMES` or `MAX_DATA` are `ENOSPC`; chunks (`m=1`) gather as a
transmission's do. Pixels stay premultiplied, so `lay` blends with plain
"over". Kitty's `d=f` deletes a frame (`ImageStore::delete_frame`,
`Frames::remove`), as kitty's own code does, its spec saying no more: `r`
counted from 1, none the first, past the last the last; the frames after
it move down and the frame shown stays where it's still there; a
picture's last frame stays, and `d=F` deletes it whole then. Flow's
frames are whole, so kitty's keeping of frames that depend on the one
deleted has nothing to do.

Pictures `ImageStore::show` placed (sixels, iTerm2's), which nothing names
again, are noted with the screen they went on (`graphics::Screen`,
`shown_on`) and let go once none of their cells show
(`ImageStore::let_go_unseen`, given the ids the session reads from the
grid's placeholder cells, `session::let_go_unseen`): on the alternate screen
after each new one (`show_picture`), at `CSI 2 J` (`Marker::Erased`) and as
the screen is left (`Modes::alternate`); on the main one, its screen and
scrollback read, at `CSI 3 J` (`clear`), at a full reset, and after a new
one once they hold half the quota and a sixteenth of it more than at the
last sweep (`sweep_due`, `swept_at`). Kitty's pictures, which a program may
place again, Flow's own (`show_own`) and models stay for the quota's
`make_room`.

A kitty command naming a file (`t=f`, `t=t`) goes through
`graphics::FileFailures::load` and `load_file`, so that a program, which
may be on another computer, learns nothing of this one's files from the
answers but whether a path holds a picture. A file that isn't a temporary
one as kitty's spec has it (`t=f`, or `t=t` elsewhere) is a picture alone
(`only_a_picture`: `f=100`, no `O` or compression, `S` not heeded;
`read_picture` reads it whole once its first bytes, `PICTURE_HEAD`, are a
GIF's, a WebP's or a PNG's whose header is within `MAX_SIDE`). Raw pixels,
compression and a part (`read_part`) come only from a temporary file
(`t=t` and `is_temp`: in a temporary folder, with `TEMP_MARK` in the path
as sent and in its real one), deleted once opened.
`/proc`, `/sys` and `/dev` but `/dev/shm` (`is_sensitive`), a path that
isn't whole and Windows' shares (`is_local_path`) are never touched. Every
failure, decoding's too, answers `FILE_ERROR`, why going to the log at
debug with the program's path escaped. Some failures take longer to find
than others, which a program asking by the thousand could time, so each
ends `FILE_FAILURE_TIME` after it began (`wait_out`, awake the whole time,
watching the clock: a sleep ends when the system's timers say, which on
the Mac is later the longer it's asked for, and milliseconds late for a
thread run at the utility class, so what was found showed; the ignored
`a_failure_takes_the_same_time_whatever_it_found` times it), and past
`FILE_FAILURES` in `FILE_FAILURE_WINDOW` every file the tab names is
refused unread until the window is over (a picture read doesn't count),
the count kept through `flow-settings reset terminal` (`ImageStore::reset`,
from `Session::reset_terminal`, which `macos/tests/pictures.rs` and
`linux/tests/pictures.rs` reset in a real PTY). What can still show, a few
times a window, is a lookup slower than the 2 ms, a long file that begins
as a picture but won't decode (read whole first, so the time says how
long it is), or a reader the system holds back meanwhile.

ConPTY drops APCs, so on Windows each tab also has a named pipe for images
(`pipe.rs`, `Platform::open_pipe`, `windows/src/pipe.rs`), opened in
`Session::launch` before the shell starts, which gets its name in
`FLOW_IMAGE_PIPE`. Pipes' names can be listed by anyone, so each instance
is the user's alone (`pipe::UserOnly`: owned by their SID, a DACL of it
and nothing else, where the default let everyone read, and labelled high,
`S:(ML;;NW;;;HI)`, in an elevated Flow, `user_only`), its first claiming
the name (`FILE_FLAG_FIRST_PIPE_INSTANCE`), none let go before the next is
made (`pipe::serve_each`, which both pipes serve by, so the name is never
free while Flow serves it, even after a connect's `ERROR_NO_DATA`), and
each later one checked to be of a pipe the user owns (`UserOnly::owns`,
`pipe::owned_by`), or the pipe serves no more and its name goes free once
its connections end.
`session::serve_pipe` reads what a program writes there
with an `OscScanner` of its own: graphics commands go to the tab's
`ImageStore` (placed nowhere: the pipe has no cursor), sixels to the screen
where ConPTY's cursor is (see Images), OSC 7731 effects to
`ShellState::effects` as from the terminal (`queue_effect`), Ratty's
commands to the tab's `ModelStore`, and XTVERSION,
`CSI 16 t`, `CSI 14 t` (from the cell's fractional size, `Session::cell`),
the colors OSC 4, 10, 11 and 12 ask for (`Marker::ColorQuery`,
`color_answers`: from `palette::ink`, written as `EventProxy` writes
the emulator's answers in the terminal and ended as the question was) and DA1,
whose answer comes last, are answered down the pipe; a test hands the SDK's
own questions to `serve_pipe` and reads the answers with its `detect::parse`
(core's one use of `flow-sdk`, a dev-dependency). The placeholder
cells still go through ConPTY, whose console gives each combining mark a
column of its own and reads `58;2;r;g;b` as a reset and bold, so programs write
them bare: U+10EEEE in the image's color and nothing else. `ui::draw_terminal`
then takes a cell's column from the one to its left and its row as 0, and
`ImageStore::lookup` with placement 0 takes the image's first (placements are
a `BTreeMap` by image, so that's one range lookup). A model's cells, though,
are Flow's own, which ConPTY doesn't know of, and it paints what a program
wrote a moment after writing it: so a placement from the pipe waits for
ConPTY's output to stop (`wait_for_paint`), and Flow Rogue sends its models
after the screen they're over. A click's answer to a model that asked for
them goes through ConPTY's input as everywhere, which passes an APC on only
to a program whose console is in VT-input mode (`ENABLE_VIRTUAL_TERMINAL_INPUT`;
in plain raw mode it's dropped), as `show_model` reads it there.

**Double lines and mosaics.** DEC's line sizes (`ESC # 3` and `ESC # 4`
the halves of a double-height line, `ESC # 6` double width, `ESC # 5`
single) are `Marker::LineSize` in `osc.rs`. alacritty_terminal keeps no line
attributes, so `line_size.rs` marks the line itself, as images live in
placeholder cells: a magic underline color on its last cell (`of_row` looks
there alone). Nothing draws an underline color on a blank cell, and the
emulator counts such a cell as blank, so the line's text and length are
untouched. The mark scrolls
and goes into the scrollback with the row; a reset row (a clear, ED from the
line's start) is single. A single line made double loses what's past its
half (`line_size::make`, from `set_cursor_line_size`), as on DEC's
terminals, so made single again it shows blanks there (xterm only hides
them). alacritty_terminal has every line's right margin at
the screen's edge, so once a program has made a line double
(`Emulator::sized`), `Emulator::run` hands the emulator its output through
`line_size::Margins`, a `Handler` that passes every call on but puts a
double line's margin at its half, as xterm and DEC's terminals do: text
wraps there, marking the row wrapped (`WRAPLINE`) on its last cell beside
the mark, where the emulator marks a single line's wrap (a wide letter that
doesn't fit leaves `LEADING_WIDE_CHAR_SPACER` padding, as at a single line's
end), or, with autowrap off, writes over its last column; the
cursor goes no further (`clamp_cursor` after each move, a line moved under
it, and a resize); ICH, DCH, ECH and insert mode work within the half (DCH,
and an ECH or EL that reaches the half, end its wrap, as on a single line);
and EL, and ED from past the line's start, put back the mark they cleared, as
DEC keeps a line's size through them. So the mark never leaves the last
cell. What reads the grid's text reads a double line only to its half, and
one wrapped there runs on into the next line's text: `Emulator::text_between`,
`input_from` (the history's), and copying, through
`Emulator::selection_to_string` (`line_size::selection_to_string`, which
hands single lines to alacritty's `bounds_to_string`, since its own
`selection_to_string` reads a wrapped row to the screen's edge, hidden half
and all). Pictures and models placed on a double line keep to its half too
(`place_image`, `place_model`, the room in `graphics::Context`), and
`regrow_pictures` fills one out no further. A new width would cut the mark
off a narrowed row or leave it short of a widened one, so `Emulator::resize`
lifts every mark onto its row's first cell as a link (`line_size::lift`)
and settles it back on the last (`settle`), on the screen in use and, under
a program's alternate screen, on the main one, reached through
`graphics::on_main_screen`. alacritty's rewrap of the main screen counts
cells by the screen's width, so `lift` lays out each double line whose text
flows for it (`lay_out`): one that wrapped at its half becomes the first
row of one line with what wrapped from it, a `Row` of whatever length makes
the rewrap give it as many cells as the screen is wide. Narrower
(`Flow::Narrower`), it's the letters its new half holds (a wide letter
across it goes on whole, leaving padding), blanks to the new width, then
the rest, which the rewrap carries on ahead of the next line's text; wider
(`Flow::Wider`), it's short of the width by the letters it now has room
for, which the rewrap pulls back from the next line (dropping a line left
empty), and `settle` moves them up beside its own (`take_back`). So the
rewrap, not Flow, adds and drops the rows, through the scrollback and
`resize_as_conpty`'s two rewraps alike. A double line's text flows once
text wraps at its half (`Margins::wrap`), which the mark's second set of
colors says (`line_size::flowing`, kept through erases, cleared by
`make`): so one a widening left holding all its text, no longer wrapped,
still gives what a narrowing leaves no room for a line of its own, and so
does one wrapped onto another double line, whose letters' line then goes
on into it again. One whose text never flowed (a title, a double height's
halves) is kept out of the rewrap and cut past the new width, its hidden
half shown again when widened (xterm rewraps nothing), as is every line on
the alternate screen, which alacritty cuts; a wrapped double line there
that widened marks the blanks it newly shows as padding, so it still reads
straight on. A single line that wrapped onto a double one is rewrapped on
its own (`end_before_double`; a blank ending its text rides the rewrap,
which drops trailing blanks, as `END`) and goes on into the double line
again (`rejoin`), the cells after its text padding. A letter written after
padding turns it back into blanks (`Margins::follow_padding`), and `make`
clears padding and wrap when the line goes single. `crop_pictures` cuts a
flowing double line's pictures at its new half (`line_size::flows`), so
they aren't carried onto the next line, and `regrow_pictures` grows them
back from its old half or its old edge. The cursor on a double line goes
where its next letter would go (`along` the line if that letter moved on),
as alacritty moves it through a rewrap. On Windows `resize_as_conpty`
rewraps the scrollback and the screen apart, so a double line at the
scrollback's end parts there from what wrapped from it, as a single line
does. `ui::draw_terminal` reads each row's size: a
double line's first `cols / 2` cells are drawn twice as wide (`CharCell`),
glyphs from the atlas at twice the font size (`Canvas::glyph_scaled`),
squashed for double width and clipped to the row for each half of a double
height, which don't ride the liquid so the halves stay joined. A picture's
cells there are drawn the same way (`CharCell::picture`: each piece scaled
with its cell and cut to the half its row shows), and they ride the liquid
by their corners as other pictures do, which keeps the halves joined. The
cursor (`cursor_target`), the mouse (`App::cell_under`,
`Layout::cell_on_line`) and clicks on models (`session::on_double_line`) go
by the doubled cells. Block elements and teletext's sextants (`mosaic.rs`)
aren't taken from the font: `draw_char` draws them as rectangles whose every
edge is rounded to a whole pixel from the cell's own position, so
neighbours join in any font; cells' backgrounds are filled out to whole
pixels too (`fill_cell`), so a band of them shows no seams. Both take the
cell's edges (left, top, right, bottom) as `ui::Grid` works them out from
its column and row (`CharCell::edges`), so the edge two cells share is the
same number for both and rounds to the same pixel. Nor are box drawing's
lines (U+2500–U+257F) and the squares, black and white in four sizes
(■ □, ◼ ◻, ◾ ◽, ▪ ▫), taken from the font (`box_drawing.rs`): most of the
themes' fonts lack the lines, so fallback fonts of other sizes and middles
drew them and a frame's sides and corners didn't meet, and a square drawn
here beside one from a fallback font wouldn't match. `box_drawing::of`
gives each a `Shape` (arms to the left, right, top and bottom, each light,
heavy or double, from the `ARMS` table; dashes; a rounded corner;
diagonals; a square), and `Shape::each_piece` lays it out as `Piece`s from
the cell's edges (on a double line the doubled cell's, `Pen::scale`
widening its strokes as a glyph's), with no allocation (`Few`, a few kept
in place): each stroke `band`ed in the middle on whole pixels, light as
heavy as the font's letters (`stroke`: the stem of its l,
`CellMetrics::stem`, measured as the x-height is, or an eighth of the cell
where that's more, and at most a quarter of it), heavy twice that, a
double's two a light stroke apart; a junction's arms reach across the
strokes they meet or stop at the nearer (`arms_bars`, which says when),
and arms in line are `join`ed into one bar. An arc is a quarter ring
between the straights' bands, its quad cut in up to four where theirs
end. `Canvas::stroke` draws each piece as kind 9 in `ui.wgsl`
(`KIND_STROKE`: a bar, a ring or a line, smoothed over a pixel). A side
on the cell's edge, or where an arc meets its straight, is hard and has
the same corners as the piece it meets, so, bending with the liquid by
their corners as pictures do, they stay joined, the halves of a double
height too. A square is a share of the font's x-height
(`CellMetrics::x_height`, measured from its x: ■ all of it, ◼ four
fifths, ◾ seven tenths, ▪ three fifths) with its room taken from the
cell's own size, not its whole pixels, which take turns a pixel apart on
cells a fraction of a pixel wide, so every square in a row is the same
size, never touching the next cell; a white one is its black one's
outline, a light stroke thick (a third of the side at most, so a small
one's hole is at least as wide as its outline). The tests lay pieces out on a copy of the
shader's coverage (`coverage`, `picture`).

**Combining marks.** A shell's line editor measures the line it edits
with the system's `wcwidth`, and the emulator has to give each letter as
many cells, or a long line is put back in the wrong place (Up over a
wrapped command garbled it). alacritty_terminal goes by unicode-width,
which, as glibc does, gives a spacing mark (Mc, Devanagari's vowel sign
ा) a cell; the Mac's `wcwidth` gives it none. So every letter goes
through `line_size::Margins` (`Emulator::run`, `Margins::new(term,
double)`, which with `double` false does nothing else), and on the Mac
one that `combining::joins_letter_before` (`MAC_SPACING_MARKS`, 399
code points read from macOS 27.2's libc, which `macos/tests/widths.rs`
holds the table to) goes on the letter before as alacritty puts a mark
of no width (`combining::join`). `macos/tests/shells.rs` types and
recalls a long Hindi command in zsh and bash, as `linux/tests/shells.rs`
does (not BusyBox's ash, which Ubuntu builds to show `?` past U+02FF).
zsh writes a mark of no width as `<094d>` unless COMBINING_CHARS is on,
which macOS's `/etc/zshrc` sets and Ubuntu's doesn't, so Flow's zsh hook
sets it before the user's files (`ZSH_RC`). A cell's marks are drawn
with its letter (`ui::marks_of`, none for a picture's cell; `draw_char`
takes them): `Canvas::marks` interns the letter and those of its marks
`combining::drawn` keeps (not variation selectors or the joiners, which
would turn ❤ into a squeezed emoji) as an id in `GlyphKey::marks`
(`Atlas::marked`, at most `MOST_MARKED`), and `Atlas::rasterize_marked`
shapes them together (`draw_shaped`, every glyph of the run in one
picture), squeezed into the letter's cells where it's wider. Each cell is
shaped alone, so a conjunct across cells shows its virama.
`FLOW_GLYPH_PICTURES=<dir>` has an atlas test save a Hindi line drawn so.

**Linux.** `linux/` names the window `com.arcanii.Flow` (Wayland's app id and
X11's WM_CLASS), which `linux/packaging/com.arcanii.Flow.desktop` and the
installed icons match, and hands winit the launcher's activation token, then
clears it from the environment so the shells don't inherit it. The window
has no decorations at all (winit's Adwaita frame is left out of the build);
a right click on Flow's title bar opens the window manager's menu on Wayland
(`Platform::show_window_menu`). Which of Wayland and X11 winit took
(`window_created` reads the window's handle into `hotkey::ON_WAYLAND`) is
said after About's version (`Platform::windowing`, `ui::about_version`,
`about.version_on`, for the card and screen readers alike), its logo
before its name on the card (`logos::windowing`: `assets/logos/`'s
`wayland.png`, and `x11.png` drawn in the text's color, shrunk to a line
of text and kept in `App::windowing_logo` beside About's cube, over a
`steady_glow` in its color, `logos::windowing_glow`; their
sources and notices in that folder's README, `NOTICES.txt`'s Logos and
the Legal page). Blur is `set_blur`, which only KDE Plasma
honors. A mouse selection becomes the primary selection when the button
comes up, and a middle click pastes it (`linux/src/selection.rs`, arboard,
through XWayland on a Wayland desktop). The "system" startup logo is the
distribution's PNG (`linux/src/system.rs`), and the games' folder is
`<prefix>/lib/flow-terminal/games` beside `bin`. There's no Sparkle, so
`linux/src/updater.rs` is Flow's own: for a copy `install.sh` put in place, it
reads `appcast-linux-<arch>.xml` (Sparkle's form) from Flow-releases (a
release whose version isn't numbers and dots passed over, `plain_version`,
since the feed isn't signed and the version names the .deb and the apt
command typed in a tab), checks
the tarball against `UPDATE_PUBLIC_KEY` and that its `flow` is the feed's
build, and runs its `install.sh` into Flow's prefix; a Flow from the .deb (in
`/usr`, dpkg's) can't, so where the feed carries the release's .deb (a
second enclosure, `DEB_TYPE`, after the tarball's, which a Flow before
0.1.37.1 reads alone; `make-appcast.sh --deb`), its Install Update
downloads it into the downloads folder, checked with `UPDATE_PUBLIC_KEY`
and that it's `flow-terminal` at the feed's build (`fetch_deb`,
`deb_is_build`, through `dpkg-deb -f`), and the state becomes
`UpdateState::InTab`, whose command (`apt_command`) `App::run_update_in_tab`
runs in a tab of its own in Flow's look, once its shell waits at its prompt
(`tend_update_tab`, `UpdateTab`), sudo asking for the password there; the
updater looks each second for the new Flow in place (`wait_for_apt`,
`replaced`) and goes on to `Installed`, and About's button is Show Its Tab
meanwhile (`show_update_tab`). An install the user started (`Installing`
or `InTab`) that becomes `Installed` while About is closed opens a card
asking whether to restart now (`App::restart_ask`, `ui::draw_restart_ask`,
`RestartAsk`, `Hit::RestartAsk*`, help's updates page, a `DialogKind::Restart`
for screen readers, `restart.png` in snapshots): Restart Now (Enter) is
About's Restart Flow (`answer_restart_ask`, `hand_over`), and it says when
a tab isn't at its prompt (`a_tab_is_busy`); Later (Esc) leaves the pill
and About. A feed without the .deb has Install Update
open the release page and About give the .deb's name and the command
(`UpdateState::ByHand`), which its button copies. After either, Restart
starts the new Flow (`updater::replaced`). A Flow pacman put in `/usr`
(the AUR's `flow-terminal-bin`, `from_pacman`, `pacman_has_flow` reading
pacman's database) only says a release is out (`UpdateState::ByPackage`),
its AUR helper installing it; `linux/scripts/aur.sh <ver>` writes the
package's PKGBUILD and `.SRCINFO` from `linux/packaging/aur/` for each
release (`docs/design/aur.md`).
Flow shows it, through `Platform::update_state`, `UpdateState` and `UserEvent::Update`: About's line
and button (check, install, restart), a pill in the title bar, and a question
asked once whether to check daily (`config::UpdateChecks`, kept in
`updates.json` beside config.toml, since Flow writes it unasked and must never
rewrite the user's config; checked at start and on focus in
`App::check_updates_if_due`). The pill shows only where the tabs keep room. Without Vulkan, wgpu's OpenGL
draws only to windows of the display the instance was given, so
`Renderer::new` gives it the window's; its surfaces have no alpha, so the
glass is opaque there. Flow uses no compute shaders or storage buffers, so
it asks for no more than OpenGL ES 3.0's limits (`render::limits`, which
the GPU test of models uses too): an OpenGL without compute (VirtualBox's)
offers no compute limits, and wgpu refuses a device asked for any.

**Input methods.** `ui::draw` hands back the caret where typed text goes
(`Drawn::caret`: the terminal's cursor cell, the assistant's input, a preset's
name, a Settings text row being typed in). `App::place_ime` allows the input method only while there is one, so
Settings' rows and the key dialog get plain keys, and puts its candidates there,
through `crt::unwarp` when the CRT is curved. `WindowEvent::Ime` preedits are
drawn at the caret (`ui::Preedit`), and commits are typed by `App::type_text`;
the keys the input method uses never reach `on_key`. On macOS,
`Platform::prepare_input` registers `ApplePressAndHoldEnabled = NO` so held
letters repeat instead of opening the accent picker. Text macOS inserts
without a key (Emoji & Symbols, which Edit opens with ⌃⌘Space, or dictation)
winit drops unless something is being composed, so `macos/src/insert.rs`
swaps in its own `insertText:replacementRange:` and `keyDown:` on winit's
`WinitView`: after winit's own, text that came neither from a key nor from a
composition goes to the app as `UserEvent::Insert` and is typed by
`App::type_text`.

**Screen readers.** VoiceOver, Narrator and Orca read a tree of what's on
screen, made for AccessKit (`accesskit`) and handed to each system by
`accesskit_winit`'s adapter, which `App::create_window` makes between
creating the window (invisible) and showing it, through the event loop's
proxy (`App::proxy`, `Reader`; snapshots and recordings make none). Its
events come back as `UserEvent::Access`, and `window_event` hands it every
`WindowEvent` first (`process_event`). `access.rs` makes the tree as a pure
function of an `AccessView` (`access::tree`), so it's tested without a
window, its tests reading it back with `accesskit_consumer` as the
adapters do: `App::access_view` gathers it (the title bar's buttons and
tabs by their `Hit`s' rectangles, `rect_of`; each pane's rows in view,
`Emulator::rows_in_view`, each `Letter` its bytes, column and cells, the
cursor's row keeping the blanks before the cursor; the panel's
`PanelLaid` rows and its input; Settings' `entries` with their rooms,
`access::settings_rooms`, each `Control` its role; Flow's own screens as
`Dialog`s, `access_dialogs`; the toast, and the last reply once done, as
polite live regions), `access_focus` names what has the keys in
`Page::in_front`'s order, and node ids are a `Part` and its places
(`access::id`). Text is runs of at most `RUN` letters, since AccessKit
counts a run's words in bytes. `App::tell_readers`, from `draw_frame`,
sends the whole tree with `update_if_active` only while a reader has asked
(`InitialTreeRequested`, answered at once) and only when the view differs
from the last sent. What a node does is a `Target`; `access::what_to_do`
turns a reader's `ActionRequest` into a `Do`, which `App::do_for_reader`
does: a button's Click is a click at its middle (`click_for_reader`, the
pointer, its keys and its clicks put back after), Focus moves only what
takes the keys (a pane, the panel's input, the find bar, Settings' row,
`select_for_reader`, a list's item, `choose_for_reader`) and never presses
anything, and a slider's Increment, Decrement and SetValue are Settings'
own `Change`s. Labels drop the symbols Flow draws beside its words
(`access::spoken`), and its words are `access.*` and the menus' own.

**Themes.** `looks.rs` holds Flow's look and ten old machines'. Settings'
Theme row puts one on the config (`Look::apply`: colors, `colors.border`,
the cursor, font families of the era before Flow's own, plain glass and a
built-in CRT preset), through `App::take_look` (which saves) or
`App::wear_look` (which doesn't, for snapshots). A border widens the
layout's margin (`Layout::compute`) and `ui::draw_border` fills it. A tab
may wear a theme of its own (`Tab::look`, an id of `looks::ids`), picked
in the new-tab picker (the … after the +, `Hit::NewTabChoose`,
`ui::TabPicker`: shells and themes side by side, `App::open_picked`,
`open_tab_in`); the + and the
new-tab shortcut open the default shell in Settings' theme. Each tab behind
the one in front keeps the grid its own theme gives it (`App::own_grid`,
`grid_wearing`: its font's cell from `Atlas::metrics_for`, its border), so
`relayout` sizes only the front tab to the window's layout and bringing
another forward lays nothing out again. Before each
frame `App::fit_look` wears the front tab's config (`App::tab_config`,
`tab_wearing`: the user's with the tab's own whole look on, `Tab::own`,
else its theme's, `looks::wear`, else the user's as it is), keeping the
user's own config in `App::base` and the tab it wears for in `worn_for`,
or the user's itself while Settings is on New tabs (`SettingsMenu::new_tabs`).
When that changes, `App::keep_worn` first puts what was worn where it
belongs: the tab it was for keeps its whole look (`looks::TAB_KEYS`,
`tab_part`, `put_tab_part`: the look's Settings sections, the font's size
among them) as its `own` where it changed, and everything else goes to the
user's config (`looks::keep_look`), which `App::save_config` saves. So
Settings on a tab (its row of tabs under the title, `ui::SettingsView::tabs`,
`Hit::SettingsOnTab`, `App::settings_on`, the tab shortcuts through
`settings_tab_key`) changes the look's sections for that tab alone
(`Section::follows_tab`, whose headers say so, `Control::Section`'s
`scope`) and Flow's for all, and a tab's own look goes with it into
`tabs.json` (`OpenTab::own`), `restart_tab` and its grid. Each session answers
programs' color questions from its own tab's colors (`Session::set_ink`,
`give_inks`), not the window's ink.

**What each tab uses.** `resources.rs` reads a tab's programs (its shell,
`Session::pid`, and every process under it) through
`Platform::processes`: the Mac's libproc (`macos/src/processes.rs`:
`proc_listpids` by parent, `proc_pid_rusage`'s footprint and CPU time in
the Mach clock's ticks, `proc_name`), Linux's `/proc` (read in core,
`resources::linux`, so it's tested anywhere) and Windows' Toolhelp
snapshot with `K32GetProcessMemoryInfo` and `GetProcessTimes`
(`windows/src/processes.rs`). `App::ask_usage` reads them on a
`flow-usage` thread at most every `USAGE_EVERY` while the card under a
tab (the pointer resting on it `TAB_CARD_WAIT`, or the tab shortcuts
bringing it forward, `App::tab_by_key`, for `TAB_CARD_SHOWN` or until a
key that isn't a modifier; `ui::draw_tab_card`) or
About's Resources (`Hit::AboutResources`, `ui::draw_resources`) shows,
never otherwise; `UserEvent::Usage` comes back, and `resources::programs`
works out each one's CPU share from the reading before. Flow's own for a
tab is `Session::held` (its rows' cells, its pictures decoded) and
`Renderer::images_gpu_bytes` (their textures on the GPU, and a 3D model's
multisampled picture, depth, buffers and mesh, `Models::gpu_bytes`), and the
window's GPU memory `Renderer::own_gpu_bytes` (the atlas, the liquid's,
the CRT's and the rain's textures); `App::resources` also counts the
frames drawn each second, and while About's Resources shows,
`render/timing.rs`'s `GpuClock` times them on the GPU where the adapter
has `TIMESTAMP_QUERY` (asked for in `Renderer::new`): every pass of a
frame, the liquid's, the rain's, models', the main one and the CRT's,
takes its `timestamp_writes` from `GpuClock::writes`, and their spans,
read back without waiting a frame or so later (`poll`), are a frame's
time, the mean of the last second's shown (`Renderer::gpu_frame_ms`). A
machine's start screen is typed into a new tab by its reader thread before
the shell's output (`Session::start_typing`), at the look's baud, below the
startup logo, which every tab shows (`App::startup_logo` and `banner` for
the tab's own worn config, its text color and its theme's logo), unless the
start screen shows the logo itself (`{logo}`, `looks::LOGO_HERE`, placed by `session::place_logo` as
it's typed, with `{beside}` lines past it), as the Workstation's and the
Graphics workstation's PROM boots do, the memory banks and the buses' slots
probed (`{banks}`, `{probes}`: `hardware.rs`'s `Hardware`, which
`Platform::hardware` reads, the banks once a run and the buses afresh
each time, so a tab opened after a device is plugged in shows it; the
first tab's on the `flow-hardware` thread `App::new` starts when the look
will need them, which it takes from `warmed_hardware`; its parsers for each OS,
SMBIOS and ACPI tables, udev's database and sysfs links, the Mac's device
tree, are in core so they're tested everywhere; banks count only where
`Hardware::banks_agree` finds them adding up to the system's total, never
short of it but up to `MORE_THAN_SEEN` times over, since firmware and an
APU's graphics take theirs first; `looks.rs` keeps each list to eight lines
of at most 79 columns, saying what it cut, and shows only what
`hardware::shown_name` and the like allow; without them a SPARCstation
5's, and snapshots probe made-up hardware, `Facts::made_up`) and
numbers (`{serial}`, `{ether}`, `{hostid}`, `{sgi_ether}`), made up from
`Facts::seed`, the same on one computer (`identity::seed`: the OS's id for
it, `Platform::machine_id`, hashed with SHA-256, or a seed kept in
`machine-seed` beside the config), or with Theme details' Its real numbers
(`ui.look_real_numbers`, off by default) the computer's own
(`Platform::real_numbers`, `Facts::real`; snapshots never read them); its prompt goes to new shells as `FLOW_PROMPT`
(`shell::set_prompt`, written per shell by `looks::prompt_for`), which the
bash, zsh, ash and PowerShell hooks take in place of the user's, and cmd's
`PROMPT` holds. BusyBox turns the backslashes in the variables it starts
with into slashes, so its prompt goes with them as ^A, which the ash hook
turns back. A theme's prompt is text, never shell code, whatever the
user's options and locale: `prompt_for` leaves out the controls that
don't show (a NUL, the ^A) and every escape but a color's (`is_color`,
`whole_escape_len`), cuts it at `MOST_PROMPT`, and writes bash's `$`,
`` ` ``, `\` and `!` as `\D{…}` (bash puts a backslash before each `$`
and `` ` `` an escape gave, so it expands none, promptvars on or off, in
posix mode too; after a letter past ASCII or the folder, `\001\002`
first, since in Shift JIS or GB18030 the byte before could take that
backslash as a letter's second), ash's `$` and `` ` `` as octal escapes
its line editor alone reads, PowerShell's `$`, `` ` `` and every kind of
double quote after a backtick, cmd's `$` as `$$`, and zsh's `$`, `` ` ``,
`\` and `!` (`shell::ZSH_LITERALS`, what prompt_subst and prompt_bang
expand) as `%-4v` to `%-1v`, which `__flow_precmd` (in `emulate -L zsh`,
as `__flow_first` and `__flow_preexec` are) fills by putting them last in
`psvar` (the four from the prompt before taken away first), and zsh never
expands what `%v` brings: so zsh's PS1 holds none of them, whatever
options are on then or come on before it's drawn (a widget's), and a
shell started in the tab that takes an exported PS1 finds nothing in it to
expand. bash's `__flow_prompt` and the ash hook unexport PS1 last (the
user's files may export it, and `set -a` again with each assignment),
since bash's form still holds `$` and `` ` ``, which dash or ksh would
expand, and bash reads ash's octal escapes as `$` and `` ` `` (the ash
hook unexports it before setting it too, as busybox-w32 gives a variable
from Windows' environment its value from there again the first time)
(`macos/tests/prompt_text.rs`, and Linux's, with the system's BusyBox,
and Windows' of the same name). Snapshots wear the Home computer, Green
screen, School computer, Personal computer, Teletype, Flow Instruments
Home Computer and Flow notanAmiga CLI in tabs of their own (`theme-*.png`; the notanAmiga is
given the key its start screen waits for, `SNAPSHOT_KEYED`, so it shows
its shell) and go back. The user's own theme (`looks::OWN`, Settings' last choice) keeps the
colors, fonts and CRT set and takes its start screen, prompt and `Sounds`
from `[theme]` (`ThemeConfig`), begun from the last machine
(`Look::as_theme`, `App::take_own_look`); `looks::InUse` is the look new
tabs and the machines' sounds go by, built-in or own. `theme` is one of
`PRESET_SECTIONS`, and a preset with a start screen or a prompt chooses it.
While it's chosen, Settings shows its rows under Theme (`settings::own_rows`:
its start screen, prompt, speed, sounds and window frame)
in place of `ui.look_start` and `ui.look_prompt`'s switches, which it doesn't
use (an empty start screen or prompt is none): the start screen and prompt
are `Control::Text` rows typed in place. Enter or a click puts a
`settings::Editing` in `SettingsMenu::editing`, whose `TextField` is the
text and caret alone (tested without drawing, word jumps included); a click
puts the caret on the place under it (`Hit::SettingCaret`, laid over each
character's halves by `ui::caret_hits`); `App::edit_key` takes every
key while it's there, paste and the input method's text go into it too, and
each change goes to the config at once (`settings::set_row_text`; the
prompt's line writes a new line as `\n`). Esc puts back `Editing::was`. A box
of lines is a taller row (`SettingsGeometry::height`, `box_lines`), drawn by
`ui::draw_text_field`, which hands back the caret. Snapshots show it as
`settings-theme.png` (`App::show_own_theme`).
Save as theme… (Settings' `SettingId::SavedTheme` row, the picker's
`Hit::TabPickerSave`, the Mac's View menu's `Command::SaveTheme`, all
`App::ask_theme_name`) keeps what's worn under a name in
`[[themes.saved]]` (`config::ThemesConfig`, `SavedTheme`: a name, its own
start screen, prompt and sounds as a typed `theme`, and the rest of the
look's sections as a preset file holds them, flattened, with only
`looks::THEME_KEYS`, which `looks::save_as` takes from what's worn, the
theme of the look in use as Theme details leave it, and `keep_look` puts
back). Its id is `saved:<name>` (`looks::SAVED`, `saved_id`,
`saved_name`), after the built-in ones and your own in `looks::ids`, named
by `looks::name`; `wear` puts on what it holds (`put_on`, merged as a
preset is, so one written by hand may hold a few) and never touches
`[theme]`, your own's; `InUse::Own` carries its id with its own `theme`
(`Config::worn_theme`, which the rows under Theme type into through
`worn_theme_mut` while it's chosen, and a preset file saves), so it's worn
as your own is, and its sounds are muted under its id (`carry_mutes`
brings a machine's, `muted_part` telling a look's by all before the last
dot). One no longer kept is Flow's look. The name is asked over Settings, as a CRT preset's is:
`SettingsMenu::naming` is a `settings::Naming` (`Named::Crt`, or
`Named::Theme` with the config worn when it was asked, a tab's own theme
from the picker or the menu, which opens Settings with the user's config
put back), whose names `taken` make the box say it saves over
(`Naming::over`); `ask_theme_name` first ends a text row's typing and
closes the color picker, which would keep the keys. `App::finish_naming`
calls `save_theme`, which chooses the theme in Settings where it's what
Settings wears and that's your own or a saved one; Delete
(`delete_saved_theme`) leaves its colors, fonts and CRT on under your own
theme and takes it off the tabs that wore it. Export…, between them
(`App::export_saved_theme`, the system's save panel through `rfd`), writes
the one chosen as a theme package: `packaged::export` makes its
`ThemeFile` (its colors, cursor, fonts less Flow's and a built-in CRT
preset's name as the theme's own, so Theme details' switches work on them,
the rest of its `THEME_KEYS` in `keys`; its id from `exported_id`, never a
built-in look's), and `packaged::pack` reads it back as `read` does and
packs it with the system's tar owned by 0 with no names, as
`examples/theme_package.rs` does. `flow-settings` never sees
`themes`, a `ui.look` naming one (`names_saved_theme`) or their muted
sounds (`hide_saved_mutes`, `put_back_saved_mutes`). The picker shows the
rows the window has room for, each list kept around its choice
(`TabPicker::tops`, `keep_in_view`), and the wheel moves the choice
there. Snapshots show them as `settings-saved-theme.png`,
`settings-save-theme.png` and `new-tab-saved.png`, a dozen saved
(`App::show_saved_theme`).
A start screen's `{wait}` alone on a line (`looks::KEY_WAIT`) has
`type_out` stop there, the cursor hidden, until the tab's first key
(`ShellState::awaiting_key`, which `Session::write` takes in place of
sending it; what goes to the shell untyped, `report` and `run_command`,
ends the wait without one, and closing the tab, `ShellState::closing`,
stops it), then go on, the machine's boot sound again (`booting`), as the
Flow Instruments Home Computer's "press any key" does before `{clear}` and
`{foot}` put its BASIC at the screen's foot, and the Flow notanAmiga CLI's
white first screen does, Kickstart 1.3's own picture of a hand holding out
its Workbench disk (`{disk}`, `looks::PICTURE_HERE`: `assets/logos/kickstart.png`
at its 130 × 131 pixels, which `logos::kickstart` scales by whole pixels to
as tall as it's shown and `session::place_picture` puts in the middle,
`PICTURE_ROWS` tall or what the screen has room for, the cursor on its last
row; `Session::start_typing` takes it beside the logo), before
`{clear}` brings its shell's copyright beside the Boing ball, its empty
drive clicking meanwhile (`Sounds::empty_drive`: `App::drive_clicks`
plays `Cue::Step` every `DRIVE_EVERY` while the tab in front waits and the
window is, from `about_to_wait`, muted as `Part::Drive`).
Start screens' lines take teletext's colors (`{red}`, `{on-blue}`,
`{plain}`, as truecolor SGR), any other as `{#d5524d}` or `{on-#d5524d}`
(the Flow Instruments Home Computer's bars, its video chip's colors,
`looks::BARS`), `{paper-white}` for the page's color (each line after it
starts on it, its centering's spaces too, and goes back to it at its end
and `{plain}`, so a `{clear}` fills the screen with it, `colored`'s
`paper`; `{paper-plain}` or the start screen's end takes it off), and
`{tall}` / `{wide}` for double lines
(`looks::start_screen`, which centers by what shows); a centered line of
`{logo}` alone is `looks::LOGO_CENTERED`, which `session::place_logo` puts
in the middle of its line with nothing beside it (`looks::shows_logo`
finds either); `Session::start_typing`
types them through an `OscScanner`, so their escapes are handled as a
program's. The School computer's is a page of teletext: its name double
height over a boat in mosaics.
Theme details (`Section::ThemeDetails`, `settings::theme_details_rows`,
shown only while a machine or your own theme is worn) holds a machine's
switches (`ui.look_start`, `look_prompt`, `look_font`, `look_crt`,
`look_colors`, and `look_frame` where the look has a frame): `Look::apply` is `apply_colors`, `apply_font` and
`apply_crt` with the cursor (its shape, blinking and speed,
`Look::cursor_speed`, which `THEME_KEYS` keep with a saved theme) and
glass, so a switch changes its part alone,
Flow's in its place when off. A look whose desktop drew its shell window's
frame (`Look::frame`, the notanAmiga's; a package's `ThemeEntry::frame`, and
one taking a built-in's place keeps the built-in's, `builtin_frame`) has it
drawn round each pane wearing it while `looks::framed` (Its window frame,
`ui.look_frame`, on at first; for your own or a saved theme, `InUse::Own`,
its own `[theme] frame`, `ThemeConfig::frame`, begun from its machine's as
Theme details left it by `Look::as_theme`, `wear` and `save_as`, switched
by the Window frame row among its rows, `SettingId::OwnFrame`, carried by
`TAB_KEYS` and Export…, `docs/design/window-frame.md`): `App::pane_layouts_in` takes which panes are
framed (`App::frames`: the one in front by the config worn, those behind by
`PaneLook::framed`, a tab behind's by `pane_config`) and gives each a
`Layout` inside a `ui::Frame` (`Frame::round`: an outline two of the scale's
pixels wide, a gap of half a cell, a title bar a cell tall and the outline
more, the text ending a line above the sizing gadget, on whole pixels), its
`frame` set, so the grid, the mouse, Fit window
to and Fit text to (`front_part`) and a new tab's first grid
(`grid_wearing`) count the room it leaves; `ui::draw_frame` draws it as
Kickstart 1.3 drew AmigaShell's window (from a picture of it in WinUAE,
2026-11-17), counted in the machine's lines (`Frame::unit`, a tenth of the
bar) and pixels (half a line): a white bar (the pane's text color) with the
background's color between its parts. At its left a close box of Flow's own
(`Frame::close_box`, a cross of whole-pixel squares, not the machine's
gadget, `Hit::FrameClose`, which closes that pane, or its tab where it's
the only one, as ⌘W does; readers get it as a button, `AccessView::closes`),
then the tab's name in the pane's letters, in the background's color in a
box of white, three white lines (`Frame::stripes`: the bar's top, middle
and foot), and at its right the depth gadgets (`Frame::gadgets`, none on
a narrow pane), `Hit::FrameFront`, which brings Flow's window in front of
the others (`Platform::bring_forward`), and `Hit::FrameBack`, which sends
it behind them, the keys going to the window then in front
(`Platform::send_to_back`, Windows' `window::send_to_back` as Alt+Esc does,
the Mac's `orderBack:` with the keys given to the app whose window
CoreGraphics lists in front of the rest; Linux's `hotkey::send_to_back`
on X11, XWayland's too, asking the window manager as a pager does,
`_NET_RESTACK_WINDOW`, which mutter honors, and as a window does,
ConfigureWindow's stack mode, then `_NET_ACTIVE_WINDOW` for the window
`_NET_CLIENT_LIST_STACKING` has in front, `first_in_front`, but not under
XWayland, whose list has no Wayland window; `sends_to_back` false on
Wayland, which lets no program lower its window; a window dropped down
is put away instead); in the bottom right corner the sizing gadget
(`Frame::size_box`, `Hit::FrameSize`), which drags the window's corner
where `ui::frame_sizes` says (`View::sizes`: not maximized nor full
screen), by `drag_resize_window`, or where winit can't start it (the
Mac) by Flow's own drag (`App::sizing_window`, `size_window_to`, the
pointer's travel on the screen added to the window's size). The gadgets' pictures are `TO_FRONT`, `TO_BACK` and `SIZING`,
pieces in the machine's pixels painted white, blue or black (`Pen`), which
a test holds to the picture's pixels; one under the pointer that would act
is drawn pressed, its colors turned over as the machine's were (white and
black swapped, the blue the cursor's orange). A frame whose pane hasn't
the keys, or while Flow hasn't (`View::focused`), has its name and its
lines' gaps ghosted (`ghost`: a dot of white in every four pixels), as the
machine's inactive windows had. Readers get only the close box: the
system arranges windows for them its own way. Each machine has programs' sixteen colors
(`Look::ansi`, written to `colors.ansi`, carried by `palette::Ink::ansi`
into `color_for_index`, so programs' color queries get them too; an empty
list is Flow's own, while a config without one, from before them, gets its
machine's from `looks::fill_in_colors`). The green and amber screens also
have a phosphor (`Look::phosphor`, written to `colors.phosphor`, empty for
none, a `THEME_KEYS` color so your own and saved themes keep one, and
Settings' Phosphor row, `SettingId::Phosphor`, a `Control::Colors` whose
first option is Off, `none`): `palette::Ink::phosphor` goes to the frame's
uniform (`FrameStyle::phosphor`), and `ui.wgsl`'s `on_phosphor` draws the
glass and every quad through it (`fs_quad` wraps `quad_color`; the color
picker's field and hue bar, `KIND_PICKER`, are left as they are), so cells
of any color, pictures, models, effects and Flow's own interface all come
out as shades of it as a monochrome monitor shows a video signal: as
bright as its luma (Rec. 601's, `lum`, as composite video and mono VGA
weighed it; `palette::luma`), from black to the phosphor at full for white
and never paler, since one phosphor can't show white, with a floor for
saturated colors (`PHOSPHOR_FLOOR`), which a real monitor lacked. So the
green and amber screens' text, sent in the phosphor's own color, shows a
little below full, and their paler shades above it. The tube's last pass
(`crt.wgsl`'s `tube`, handed it as `CrtFrame::phosphor`) brings what its
glow adds back onto the phosphor, no brighter than it at full, as it'd
otherwise whiten bright text, and draws no grille under it
(`CrtLook::grille_over`), a monochrome tube's phosphor coating it whole.
Without the tube, the frame is drawn into the CRT layer's frame texture
alone and `CrtLayer::onto_phosphor` (`crt.wgsl`'s `fs_onto_phosphor`,
`docs/design/phosphor-glow.md`) brings it back onto the phosphor the same
way, so a glow quad that clipped channel by channel over bright cells in
the 8-bit frame is the phosphor at full, not paler; the floor for
saturated colors stays, decided 2026-11-27. Its twin
`palette::on_phosphor` is tested, and `palette::answered` tells programs
asking for colors (`EventProxy`, `color_answers`) the shade they get.
Cells' colors themselves (`palette::resolve`) stay full, so nothing is
mapped twice. Its tube shows only where the machine had one
(`Look::has_crt`).
Themes also come as packages (`looks/packaged.rs`, re-exported as
`flow_core::themes`): `flowapp` installs one as an app with no programs
(`Catalog::themes`, its one download `apps::ANY`, `Kept::kind` `themes`,
recognized by its `themes.json`, whose every theme must read before
`install_bytes` puts it in place; `apps::install_file` takes one of the
user's own from a `.tar.gz`, unsigned, refusing any with a `bin/` or a name
that's Flow's, `own_name_refused`: flowapp's, one the saved catalog lists
(`flowapp install` reads it afresh first, `catalog_afresh`, so a name it
has added since counts), or one installed from it, `in_the_way`; or a theme
whose id is a built-in look's or another installed package's,
`own_themes_refused`, and `packaged::installed` reads the catalog's
packages first, never letting one of the user's own, `Kept::own`, stand in
for a built-in look, as only the catalog's packages may). Each
`ThemeEntry` is read into a `Look` leaked for good (`packaged::read`;
`Look::keys` holds more of `THEME_KEYS` put on after `apply`), so it's
worn as a built-in one is; `looks::find` takes a packaged one first, in a
built-in's place where the id is one, and `looks::ids` lists the rest after
the built-in ones (`Packaged::name` by language). `App::new` reads them
(`read_packaged_themes`, not for snapshots and recordings, which wear only
Flow's own) before the config, and focus reads them again once
`installed.json`'s time changes (`App::reread_themes`); the atlas loads
their font files (`font_system`, `Atlas::load_fonts`) and `startup_logo`
shows a theme's PNG (`picture_logo`). The registry (`packaged::set`,
`all`) is per thread in core's tests. `packaged::core_themes` is Flow's
own machines as the `flow-core-themes` package, which
`examples/theme_package.rs` writes and packs. Settings' Theme gallery
(`SettingId::ThemeGallery`, `SettingsMenu::gallery`, `gallery.rs`'s
`Gallery`, drawn by `ui::draw_gallery` over Settings, `Hit::Gallery*`,
`App::gallery_key`, `gallery_click`, help's `Page::Gallery`) lists the
catalog's packages of themes and those installed; `App::read_gallery`
reads the one chosen on a `flow-apps` thread, from its folder if
installed, else fetched, checked (`apps::check`) and unpacked aside
(`apps::peek_themes`), with `packaged::peek_shown` (`peek`, read and
checked as `read` does, nothing registered or leaked, which installs check
with too, and the font files' and logos' bytes it brings, `Peeked`, read
before the folder goes), into `UserEvent::GalleryRead` as a
`gallery::Preview` (its logos decoded by `gallery::logo_of`, as the startup
logo's `picture_logo` is); the atlas loads its fonts from memory
(`Atlas::load_font_data`, once each by a hash of their bytes), and the
preview is drawn from its `ThemeEntry` (`gallery::plain` takes a start
screen's markup out), its lines and prompt in the theme's own letters
(`Canvas::use_font` and back) under its logo (`Gallery::logo`,
`Canvas::image_over`). Install and Upgrade
go through `App::start_app_work` as Settings' Apps do, and Wear through
`choose_theme`. Snapshots show it as `theme-gallery.png`
(`gallery::made_up`).
Each sound a machine makes is a `looks::Part` of its `Sounds`
(`Sounds::parts`), muted per theme in `sound.muted` as `<look>.<part>`
(`Part::key`, `App::part_heard`).

**3D models.** Programs place them with the Ratty graphics protocol (APC
`_ratty;g;…`, `osc.rs` `Marker::Model`), as the Ratty terminal reads it:
`model.rs` parses the commands, loads OBJ and GLB (`gltf`) into a mesh
centered in a unit box, and keeps each tab's objects (`ModelStore`).
`session::handle_model` places one as an `Image` whose `model` is a `Shown`
(its object and style), written as placeholder cells centered on the cell
named, without moving the cursor; placing it again clears its old cells. So
it scrolls and clears like an image, and `ui::draw_terminal` tiles it as one.
The renderer draws its mesh into that image's texture (`render/models.rs`,
`shaders/model.wgsl`: multisampled, lit, premultiplied) when the style's
version or the mesh changes, or each frame while it's animated
(`App::models_moving`). A placement whose object is registered again (the
demos send a whole scene that way each frame) draws the new mesh, and
`Models::draw` drops the old one's buffers at once rather than after `KEEP`.
A part's `gloss` (one minus a glTF's roughness, or Flow's own scenes') gives
it a highlight and a sheen in `model.wgsl`, with metallic-roughness, normal,
emissive (times `KHR_materials_emissive_strength`) and occlusion textures
(`model/glb.rs`, tangents worked out where missing, from the uvs the normal
map reads); occlusion shades only the room's light (ambient, rim,
reflections), so a part without it is lit as before. Each texture reads
`Vertex::uv` or `uv1` (`TEXCOORD_1`) through a `UvMap`
(`KHR_texture_transform`, typed by the gltf crate on a plain texture's info
and read as JSON on normal and occlusion maps, hence its `extensions`
feature), sixteen of them (`model::Tex`, `TEXTURES`) in `Draw::uv`, bound
at 1 and 3 to 17 (`texture_binding`), every one a stage may sample on
downlevel GPUs. The material extensions are layers the shader
adds only where `Draw::layers` (or `alpha.w`, unlit) says the part has
them, sampling their textures with `textureSampleLevel` (one mip level) so
they can sit in branches, and a part without them is drawn as before:
unlit (the color alone), specular and ior (`Draw::specular`, scaling the
non-metal glaze's reflections head on and at the edges), sheen (a rim in
its color, wider when rough), clearcoat (Flow's glaze again over the rest,
on the surface's own normal or its own normal map, which shares the
tangents), transmission with volume (see-through, as `Material::see_through`
says: the picture can't tint or bend what's behind it, so the light that
passes is clear under a film as deep as the color it takes absorbs, frosted
when rough, the reflections kept, and the film left off a volume's far
side), and from 0.1.48 (`docs/design/gltf-materials.md`; `Draw::more`)
anisotropy (`model::Anisotropy`: Flow's highlights stretched along its way,
the tangent turned by its rotation and its texture, `stretched`'s half
vector with its part along the way taken down, the tangents worked out
from the first uvs where nothing else needed them, none without uvs) and
iridescence (`model::Iridescence`: the glaze's reflections and a metal
times a thin film's color, `thin_film`, its two maps one texture,
`buffers::Images::film`, `film_of`); dispersion is read and draws nothing,
and `COLOR_1` and on are passed over. Clearcoat, sheen, anisotropy and
iridescence are JSON to the gltf crate (`named` in `read_material` finds
their textures); the rest are its typed features.
KHR_materials_pbrSpecularGlossiness (the crate's feature) is read as
glTF-Transform's `metalRough()` turns it into metal and roughness: the
diffuse the base color, no metal, the glossiness the gloss, the specular
color KHR_materials_specular's at full strength, held as how much more
than Flow's own 0.04 glaze it reflects in display values at Flow's ior of
1.5 (glTF-Transform's ior of 1000 would scale the glaze 25 times, as
`render/models.rs` scales it linearly by the ior's reflection while the
colors are display values), the spec-gloss texture's color the specular
color's map and its alpha baked into a roughness map
(`Images::roughness_from_gloss`, made once per texture and factor).
The crate takes as required (`extensionsRequired`) only those it reads
itself, which leaves out clearcoat, sheen, specular, volume, anisotropy,
iridescence, dispersion and instancing, so `glb::import` checks that list
against what Flow reads (`REQUIRABLE`), refusing a file by the names of
those it doesn't (diffuse transmission, say), and lets the crate check the
rest of the file.
EXT_mesh_gpu_instancing is JSON on the node to the crate:
`glb::instances` reads each instance's TRANSLATION, ROTATION and SCALE
(through `buffers::floats`, so quantized too) as a matrix. From 0.1.48
(`docs/design/gpu-instancing.md`) a part not skinned and not see-through
is drawn once per instance on the GPU: `glb::load` keeps one set of its
vertices, in its node's own space (`Rest::drawn`, which `Rig::pose_part`
leaves there, morphs alone moving it), and `Part::instances` the matrices
that draw each copy in the unit box (`Rig::instances_at`: the box's, the
node's as posed, the instance's; `Pose::instances` as an animation has
them), which the fit and the box's sweep take through `model::each_drawn`
(every point within `EXACT`, else each instance's corners of the part's
box), `pick::cast` takes by casting in each instance's space, `render/models.rs`
draws as its second vertex buffer (`GpuPart::places`, a placement's
`placed` as posed, one identity for a part drawn once) with as many
instances, and `model.wgsl`'s `vs_model` puts through it (normals by its
cofactors, its `mirror` flag turning `fs_model`'s front about);
`model::expand` and `Part::expanded` make the copies on the CPU, and such
flat artwork isn't made thick for `depth`. A skinned or see-through part
is still copied (`Part::vertices` one copy after another, each copy's
triangles numbering its own, wound round where the node and the instance
mirror it differently), its matrices in `Rest::instances`, which
`Rig::pose_part` puts between the morph targets and the node (or the
skin's joints), so posing at rest, the clips, the box's sweep, the
see-through cuts and `pick::cast` all take the copies as they take any
vertices. A part on the GPU counts its triangles once toward
`MAX_TRIANGLES`, and every instance's toward `glb::MAX_DRAWN` (32
million); copies count their triangles toward `MAX_TRIANGLES` and their
vertices toward `glb::MAX_INSTANCED` (three a triangle) before they're
made, refused past any. Its bytes are
Flow's to read (`model/buffers.rs`): `buffers::read` takes the binary chunk
and data URIs (no files) and gives meshopt's fallback buffers zeros, then
unpacks every view EXT_meshopt_compression (or KHR_meshopt_compression)
packed into its own place before anything reads it (`model/meshopt.rs`,
written from the spec and checked against meshoptimizer 1.3's own decoder:
the vertex codec's versions 0 and 1, version 1's per-block control of how
each byte's column is kept, its one-bit groups low bit first, and its
channels of byte, 16-bit and xored-and-turned 32-bit deltas, the index
codecs' 0 and 1, the octahedral, quaternion, exponential and color
filters; tested against meshoptimizer's own streams and an encoder in its
tests written after meshoptimizer's, which the GLBs the tests build use
too). It reads from the JSON, every index checked, since a Draco file's
accessors have no view, which the gltf crate's checks refuse: so
`draco::decode` (`model/draco.rs`, draco-core, Google's codec in Rust, its
decoder alone, held to `MAX_TRIANGLES` and `buffers::MAX_UNPACKED`)
decodes each KHR_draco_mesh_compression primitive into a buffer of its
own, each attribute in its accessor's component type and the triangles as
32-bit indices, and points the accessors (in place, or a copy where
another primitive took one already) at views over it, and only then is
the document made and checked (`gltf::Document::from_json`).
`model/testdata` holds Draco meshes and KTX2 textures that Google's and
Binomial's own encoders made (its README says with which versions and
how), which the tests wrap in GLBs; draco-core's encoder, a
dev-dependency, makes the rest. From 0.1.48 it holds whole GLBs too, as
gltfpack and gltf-transform wrote them from Khronos's samples and Flow's
own forest (`forest_gltf.py`; `docs/design/gltf-tool-files.md`):
`glb::tests::FOXES` (skinned, three clips), `FORESTS` (plain and made GPU
instances by each tool), `SIMPLE_INSTANCING`, `ANISOTROPY` and
`IRIDESCENCE`, which `glb`'s tests read and the GPU test
`whole_files_from_gltf_tools_draw_on_the_gpu` draws. A primitive without
normals is drawn flat, as glTF has a viewer draw it: where its triangles
share a vertex (`shares_vertices`), each corner gets one of its own,
every attribute spread to them as it's read (`spread`: positions, uvs,
colors, joints, weights, morph targets), once its triangles are counted
against `MAX_TRIANGLES`, and the file's tangents are passed over.
`buffers::floats` reads positions, normals, tangents, uvs and morph
targets of any component type, normalized or not, sparse too
(KHR_mesh_quantization, the nodes scaling them back), where the gltf
crate's readers take floats; and `Images` decodes each image the first
time a texture needs it, a texture's KHR_texture_basisu source first (a
KTX2, known by its identifier, its largest level transcoded to RGBA by
basisu, Binomial's transcoder in Rust), then its EXT_texture_webp source
(`image-webp`) and its own (PNG or JPEG, the crate's) after, an image that
won't decode leaving the texture out rather than the model. A primitive's
COLOR_0 (VEC3 or VEC4, read through all of that) is each vertex's
`Vertex::color`, RGBA bytes in display values as a texel is
(`glb::vertex_color`), white where there's none, which the GPU reads as
`Unorm8x4` and `fs_model` multiplies into the base color, alpha too, so
the alpha modes take it as they take the texture's; posing carries it
over with the uvs, and a see-through cut mixes it (`order::part_way`).
A glTF's `KHR_lights_punctual` lights (`Mesh::lights`, at most
`MAX_LIGHTS`, made shares of 1 on the model's middle by `settle_lights`,
since their physical units say nothing of the picture's brightness, and
then by `settle_over_clips` shares of 1 at the brightest moment of the
clips that move them, sampled as the box is swept, so one a clip carries
nearer never washes the model out) take
the place of Flow's `LIGHT` in `lighting()`, held in the view the model is
first seen in rather than turned with the style (`light_rows`). Each keeps
its node (`Light::node`): where a clip moves it (`Rig::moves_lights`, from
`Rig::moved`), `Rig::place_lights` places and aims it from that frame's
pose (`Pose::lights`), its share left as settled at rest, so one brought
nearer brightens (`light_rows` keeps its distance at rest as where it gives
its share). The file's camera (the scene's first) is `Rig::view`, a turn
put before every root node, so posing bakes it into the mesh, joints and
lights too, and the unit box and its fit are as the camera sees it. Where
a clip moves the camera, it's `Rig::camera` instead, and `Rig::pose` puts
`seen_from` (its turn, and its place taken away) before every node from
each pose, so the scene turns and slides as the camera does, what it looks
at staying put, and the unit box is swept over all of it; a still camera's
place would only shift the whole box, so `view` leaves it out. A
perspective camera's projection is used too (`Mesh::lens`, a `Lens`: its
eye where the rest pose puts it, so the middle for a moving one, gone into
the unit box as the lights do, its `yfov` and `znear`; an orthographic one
has none and draws as before). `Shown::transform` then scales the model by
`Lens::scale` (the pixels a unit takes at the model's middle) rather than
fitting it, and moves it so the eye is `Lens::focal` pixels in front of the
picture's middle; `Shown::view` gives the renderer that and the projection,
a perspective one from the eye whose near and far hug the model (no nearer
than `znear`), or the flat one as before, and `View::eye` is where the
shader's highlights are seen from, and through a camera what its rim of
light faces (`Draw::view`; straight on the rim keeps to the normal's z,
as it always did). Turned (`rx`, `ry`, `rz`, `depth`'s oblique view or
`animate=1`), it's kept on the picture by `kept_in_view`: scaled down
about its middle, in closed form, just far enough that it reaches no
further out on any side than the picture's edge, or than it reached
unturned where the camera has it reach past that, and stays in front of
`znear`: the larger of what its ball allows (which bounds any turn, so
`animate=1` keeps one size) and, for a fixed turn, its box turned (which,
square to the view, can reach past all of the model). `Shown::hit` casts from the eye through
the point, from the near plane on (`pick::cast`'s `from`), and the
see-through triangles' sort by view depth stands as it was. A material's `Alpha` is glTF's
`alphaMode`: solid parts ignore alpha, cut-outs discard below the cutoff,
and see-through parts (`BLEND`, as the SDK marks its invisible corners and
shadows, `Part::plain` with alpha under 1, or light passing through) are
drawn last by a second pipeline that blends premultiplied and doesn't write
depth, triangle by triangle, farthest first (`model/order.rs`: a radix sort
of their middles' depth each draw, written to the placement's own index
buffer, drawn in runs of one part each, `MAX_RUNS` at most). Every mesh
has them cut where they cross when it's made (`SeeThrough::new`: crossings
found through a grid, each triangle cut by the planes of those it passes
through, the new vertices in `SeeThrough::added`, which only the GPU's
copy of each part holds, and the two each lies between in `between`), so
no two cross. A posed one (`Mesh::posed`, a `Pose`) is cut afresh each
frame by `SeeThrough::pose` where it has at most `EACH_FRAME` see-through
triangles and the work stays within `PER_FRAME` (pairs looked at and
tested, pieces cut: a millisecond or two at most); otherwise the cuts made
at rest are posed with the two vertices each lies between, right while the
triangles that crossed move together. Either way the cuts' vertices follow
each part's own in `Pose::vertices`, written to the placement's posed
buffers, which grow as a frame needs. An ignored test in
`render/models.rs` draws models on the GPU and reads them back
(`FLOW_MODEL_PICTURES=<dir>` saves the pictures), to look at a model's
materials without the app. A
glTF's animations play (`model/anim.rs`: TRS channels sampled step, linear
or cubic, skins posed on the CPU, morph targets), looped, picked by `clip=`;
each placement has its own posed buffers, and the unit box is fitted over
the clip's poses so a moving model stays in its cells. Flat artwork given
`depth` is a mesh made thick (`Mesh::extruded`, cached in `Shown::mesh`);
where the flat one moves, its `Extrusion` keeps it and its outlines, and
`Mesh::posed` poses the flat mesh and makes it thick again (`thicken`), so
the animation plays on it too. `clicks=1` asks for
clicks on a placement: `App::mouse_free_at` decides as for mouse reports,
and `model/pick.rs` casts a ray against the posed mesh and answers
`_ratty;g;click;…`. After a font size change `Session::resize` has each
model drawn again at the new size (`ImageStore::resize_model`). Flow draws a scene of its own the same
way: `about_cube.rs` lays out the About card's rolling cube each frame as a
`Mesh::scene` (already in the picture's pixels) for a `Shown` with a scale
of 1/0.9, and the card shows that picture over its glass with
`Canvas::image_over`, which draws a picture in the UI's own order rather
than under it.
A model named by a path is an `Ask` that waits for the
user's yes (`App::answer_model_ask`, `ui::ModelAsk`), then loads on a thread
of its own (`Ask::load`: a regular file on this computer alone, through
graphics' `is_local_path` and `is_sensitive`, read no further than
`MAX_BYTES`). Snapshots place a cut gem (`App::show_demo_model`) for
`model.png`.

**The command palette.** `command_palette.rs` builds its items when it
opens (`App::toggle_palette`, ⇧⌘P, Ctrl+Shift+P, `Command::Palette` in the
Mac's View menu): `COMMANDS` (each `Command` with its `menu.*` words, and
its key as it's bound, `keys::bound`), every Settings row with all sections
open but Keys' own (each `Entry::Setting` with its section's header as
`detail`), every theme
(`looks::ids`, `name`), every help page and a new tab in each shell, each
an `Act`. `score` matches letters in order over every alignment (word
starts and runs count more, gaps less), and far more what's found as typed
(at a word's start; inside one from three letters); a setting is found by
its words too, through Settings' search's `settings_search::Index`
(`Palette::words`, built in both languages as the palette opens), scored
by `by_words` from `Found::weight` below a name found as typed;
`Palette::refilter` sorts by it, `App::palette_recent` (a `command_palette::Recent`: the
last `RECENT` chosen, by `Item::key`, which doesn't change with the
language, kept in `palette.json` beside config.toml, never by snapshots)
breaking ties. `App::run_palette` does the `Act`: `on_command`,
`open_settings_at`, `choose_theme` (Settings' Theme row's path),
`open_help` or `open_tab_in`. It's modal, as the history's search is
(`palette_key`, clicks outside close it, the wheel moves the choice);
`ui::draw_palette` draws it, and snapshots show it as `palette.png`.

**Recording the terminal.** `Command::Record` (⇧⌘R, and `keys.rs`'s
`RECORD_ELSEWHERE`: Ctrl+Alt+Shift+R on Windows, Ctrl+Alt+Shift+V on
Linux, where GNOME Shell takes Ctrl+Alt+Shift+R for its own screen
recorder; the Mac's View menu, the palette) first counts down `record.countdown`
seconds (`App::count_down_to_recording`, a `Countdown` whose toast
`tend_recording` changes each second and clears as it starts, so the
toast isn't in the file; the key again calls it off; with none it starts
at once and says so), then toggles `App::recording`, a `Recording`
over `record::Recorder`: a `flow-record` thread taking `record::Frame`s
over a bounded channel (`QUEUE`; a full one drops the frame), each made
straight RGBA (`straighten`), shrunk to the canvas (`shrink`, an area
average, or squares by a whole factor; `canvas_for`: the terminal area in
points, at most `MAX_WIDTH`; a frame of another size `fit` in the middle)
and added at once to libwebp's `WebPAnimEncoder` through libwebp-sys
(`record/webp.rs`, `AnimEncoder`, looping, libwebp's second thread on,
its whole-frame limits kmin one short of kmax, since libwebp encodes each
frame between them twice, as a whole frame and as its changes), so
no raw frames pile up; `finish` ends it and the file's bytes come on a
channel. How it's encoded is Settings' Recording section
(`Section::Recording`, `config::RecordConfig`, open to `flow-settings`):
`record.quality` through `record::encoding` (`QUALITIES`: lossy at 70, 85
and 95 with a whole frame every 48, 24 and 12, since between them libwebp
keeps only changed rectangles and takes a change under its quality's
threshold as none, which a slowly moving liquid drifts into a patchwork;
or lossless), each timed on `--record`'s oil frames by an ignored test
(`encoding_keeps_up_with_frames_that_change_all_over`,
`FLOW_RECORD_FRAMES`; libwebp's `use_sharp_yuv` more than doubled them,
so it's off) and measured there (its doc comment); where a frame at the
chosen method takes longer than the interval, the thread hurries
(`record::Pace`: `AnimEncoder::add`'s `hurry`, libwebp's method 0, which
its settings take frame by frame, tried at the chosen method again every
`RETRY` frames), and `record.size` through `canvas_for` (`SIZES`: points, or
every pixel up to `MAX_FULL_WIDTH`). The frames
are what's shown: `Renderer::set_recording` adds `COPY_SRC` to the
surface's usage (only while recording; `can_record` where the surface's
caps allow it, else a toast says it can't), and `render`, handed a
`render::capture::Crop` (`App::record_crop`: the window below the title
bar, left of the panel) and the time, copies that rectangle into one of
`capture::Capture`'s three staging buffers before presenting, maps it
after the submit and hands frames back from `take` without waiting
(`App::keep_recorded`, each frame and from `about_to_wait`). Frames are
taken at most `record::FPS` a second (`record::due`, `recording_take`),
none without a free buffer or while frames wait for the encoder
(`Recorder::ready`), so those kept are evenly spaced when it can't keep
up; one passed over is owed, so `tend_recording` draws it at the next
interval (or, the encoder busy, looks again in 8 ms, drawing nothing), as it does the title bar's clock each
second (`View::recording`, a pill in `BAD`, `Hit::StopRecording`) and
stops at `MAX_LENGTH`. A frame not drawn just lasts longer in the file.
Stopping (`stop_recording`, through `finish_recording`, which waits for
the frames still coming back, `recorded_last`) asks where with the
system's save panel, the kind that doesn't wait (`rfd::AsyncFileDialog`,
a sheet on the Mac; `dirs::video_dir`, `record::file_name` by jiff's
local time): a click on the title bar's clock stops it, and AppKit hands
the title bar's held mouse-down to a waiting panel's modal loop, which
brings it to winit while Flow is still in that click, and winit aborts.
`flow-record-save` waits for the answer (`pollster`) and the thread,
writes the file and comes back as `UserEvent::RecordingSaved` (no path:
cancelled), `hotkey.dialog` held meanwhile as `in_dialog` holds it. As it starts, `prompt_shows_names` reads each
pane's prompt (its `input_start` line and the one above) for
`record::user_name` and `Platform::host_name` (`shows_names`), and the
toast says so. Snapshots end with `RECORDING_STAGE`: the split tab drawn
offscreen (`Renderer::record_offscreen`) through the same capture and
encoder for `SNAPSHOT_RECORDING` seconds, as `recording.webp`.
`examples/third_party_notices.rs` reads a crate's `vendor/` too, for
libwebp's COPYING and PATENTS.

**Find in the scrollback.** `find.rs` searches a tab's grid and scrollback
with alacritty_terminal's own `RegexSearch` and `RegexIter` (what's typed made
literal by `find::literal`, or as written with the bar's pattern switch,
`Find::pattern`, `Hit::FindPattern`, ⌥⌘R or Alt+R, a pattern that isn't one
`Find::bad`; case-insensitive unless it has a capital), at
most `find::MOST` matches; `Find` keeps the one chosen by its line counted
from the top of the scrollback (`history_size` plus its line), so it stays
chosen as output comes, and `shown` gives the matches in view. `App::find`
is the bar (`open_find`, `search_find`, `step_find`, `find_key`, which
takes every key but the shortcuts and Page Up and Down while `Find::focus`);
`find_again`, from `prepare_frame`, searches afresh in another tab and again
as output comes (`find_stale`, set by `Wake`) at most every `FIND_EVERY`.
`ui::draw_find` draws it at the terminal's top right (`Hit::FindBar`,
`FindPrev`, `FindNext`, `FindClose`), and `draw_terminal` lights the
matches as it does a selection, in `ACCENT2`. A click in the terminal takes
the keys from it (`focus`) and leaves it open. `Command::Find`, `FindNext`
and `FindPrevious` are the Mac's Edit menu's (⌘F, ⌘G, ⇧⌘G); elsewhere
Ctrl+Shift+F, F3 and Shift+F3. Snapshots show it as `find.png`.

**Scrolling back.** While a pane is scrolled back (its grid's
`display_offset`), `ui::draw_terminal` draws a pill at its top right
saying how many lines are below (`terminal.scrolled`, `scrolled_one`),
`Hit::ScrollBottom` (the pane's place in its tab, `PaneView::index`),
which scrolls that pane to its foot; ⌘End and ⌘Home on the Mac,
Ctrl+Shift+End and Ctrl+Shift+Home elsewhere, go to the foot and the top
outside a program's alternate screen (`on_key`, beside the Page keys).
Each pane's scrollbar is `scrollbar.rs`: `Bar::place` lays out its track
and thumb along the pane's right edge for the scrollback's lines, the
screen's and the offset, and the strip that takes the pointer
(`Bar::zone`, `Hit::Scrollbar`), wider than the thumb, which is drawn thin
and widens under the pointer or held; `offset_at` and `grab_at` turn a
point on it back into an offset, a press on the track jumping the thumb's
middle there (`App::scrollbar_of`, `drag_scrollbar`, `dragging_scrollbar`,
let go as a divider's drag is). `shell.scrollbar` (Settings' Scrollbar,
under Terminal: `scrollbar::MODES`) shows it always, never, or while
scrolling: `App::light_scrollbars`, from `prepare_frame`, gives each pane
of the tab in front a `scrollbar::Light` by its session, lit while it's
scrolled back, the pointer is at its edge (`scroll_near`, from
`scrollbar_near` as the pointer moves) or its thumb is held, the wheel
`nudge`s it, and it holds `HOLD` after and fades over `FADE`, keeping
frames coming (`moving`) only then; `scrollbar_shown` is none on the
alternate screen or without a scrollback. A program that takes the mouse
gets no report over either, as over any hit (`mouse_free_at`). While the
find bar has found something, the pane in front's bar is lit and marks
its matches across the track (`Bar::mark_y`: the thumb's middle with that
line in the middle of the screen, so a press on a mark, a press on the
track, brings it into view; marks on one pixel row drawn once, the one
chosen taller), its track starting below the find bar
(`ui::scrollbar_room`, which `App::scrollbar_of` lays it out by too); a
snapshot shows them as `find-marks.png`. Screen
readers get the pill as a button (`AccessView::scrolled`,
`access.to_bottom`). Snapshots show both in the split tab's green pane,
scrolled back through a count, as `scrolled.png` (`SCROLLED_STAGE`).
Each command run at a prompt is marked in the first cell of the line its
prompt began on (`marks.rs`: `Mark` as an underline color nothing draws,
apart from `line_size`'s, so it scrolls, rides a rewrap and leaves with
its line): `ShellState::prompt_line` notes 133;A's line, `marks::start`
marks it at Enter (`Session::write`, `run_command`) and again at 133;C
(after zsh's transient prompts draw the line again), and 133;D's
`marks::finish` turns the nearest running mark above the cursor done or
failed; none on the alternate screen. `ui::draw_terminal` marks each
(`marks::all`, a glance at every line's first cell) at the track's left
under the find bar's marks, and lights the line Previous and Next Command
went to (`Command::PreviousCommand`, `NextCommand`, ⌘↑ and ⌘↓,
Ctrl+Shift+↑ and ↓; `App::jump_command`, `marks::beside`,
`App::jumped`, `JUMP_LIT`), which puts it at the screen's top and goes
on from it while it's in view. Snapshots show them as
`command-marks.png`, the count failing on Unix.

**Caps Lock.** `caps_lock.rs`'s `CapsLock` keeps whether it's on and when
its notice came: after each key (`App::caps_lock_key`, from the
`KeyboardInput` handler), one that typed text into a pane
(`key_to_shell`, `caps_lock::types`) with no Control, Alt or ⌘ held asks
`Platform::caps_lock` (the Mac's `NSEvent::modifierFlags_class`, Windows'
`GetKeyState(VK_CAPITAL)`, X11's Lock modifier in `QueryPointer`'s mask
on a connection of its own, `linux/src/caps_lock.rs`, whose press that
turns it off still finds it on, the server unlocking it as the key comes
up; none on Wayland, since winit's modifiers leave it out and Wayland
gives it to winit alone), else `caps_lock::guess`es from
the letter (a capital without Shift, a small one with it; a capital with
Shift says nothing, as the Mac types one with Caps Lock on too), and the
Caps Lock key itself asks again or turns over what was known
(`CapsLock::pressed`). On, the notice shows for `caps_lock::SHOWN` after
the last key; `about_to_wait` takes it away then (`CapsLock::tend`, which
asks the system again only while it shows, and `due` in its wake), and
losing the focus forgets it all (`left`). `ui::draw_caps_lock` draws it at
the middle of the terminal's foot in `ORANGE` with a `steady_glow`, above
the pane in front's cursor where its row would be covered, and readers
get it as an assertive alert (`AccessView::caps_lock`). Snapshots show it
as `caps-lock.png`.

**Flow's keys.** `keys.rs` is every shortcut in one table (`TABLE`: an
`Entry` each, its name in `[keys]`, what it `Does`, a `Command` or one of
two families, going to tab 1 to 9 and moving between panes, whose digits
and arrows are fixed and whose modifiers are bound, its `menu.*` words, its
`Group`, and its default chords on the Mac and elsewhere, the keys Flow had
before they could be changed, which a test holds them to, as it holds that
no two share one). A `Chord` is a `Key` (a character or a `Named` key) and
its `Mods`, written as the hotkey's (`hotkey::modifier_named`: `cmd+t`,
`ctrl+shift+t`, `cmd+plus`) and shown as each system writes it (⇧⌘T,
Ctrl+Alt+Shift+D). A key pressed is a `Pressed` (`key_without_modifiers`,
what it typed, the modifiers), matched by `Bindings::action_for` against
its candidate chords: the key with its modifiers exactly, and where Shift
made another symbol, that symbol with and without Shift, so `cmd+plus` is
⇧⌘= in the US and ⌘+ in Germany, and `cmd+?` ⇧⌘ß, as the menus match.
`Bindings` are the defaults with `[keys]`' changes (`Config::keys`, a
`KeySetting` each, one, a list or anything else, which is kept and passed
over so the config still reads), made process-wide by `keys::set` (per
thread in tests) and read by `keys::bound`: `App::on_key` (one lookup,
`do_bound`, which lets a key go on where it isn't Flow's just now: Copy
without a selection on the Mac, Find Next with the find bar closed, the
scroll keys over a full-screen program or the panel; help's, Settings',
the history's, the palette's and `settings_tab_key` read it too, and
`find_key`), the Mac's menus (`Menus::bound`, `equivalent`: AppKit's
function-key characters for named keys, the rest of a command's chords as
hidden items; `install_menus` again when they change), the palette, help
and the words that say a key (`keys::label`: the title bar's Ask pill, the
panel's hint, Settings' hints, the toasts), and `flow-help`, which reads
`[keys]` alone (`from_config_file`). Settings' **Keys**
(`Section::Keys`, closed at first, and closed once for a config from
before it at the first start of 0.1.39, whose closed sections don't name
it: `settings::first_start_since_keys`, `close_keys_section`, as
`looks::bring_glass_up` was 0.1.36's; `settings::key_rows`, `SettingId::Key`,
`KeysReset`, each a `Control::Item` whose buttons are `KeyAct`s) waits for a
key (`App::key_wait`, a `keys::Wait`, taken at the top of `on_key` by
`press_bound_key` as `press_hotkey` is; `MenuState::capturing` has the
Mac's menu items give their keys up meanwhile): `Bindings::offer` refuses
(`Why`: typed, F1, the system's, the Hotkey, a family's wrong key), finds
another's (`Wait::Taken`, swapped or taken, `swap`, `take_from`), or binds
it (Add Key, `KeyAct::Add`, waits in `keys::Mode::Add`, and
`Bindings::offer_as` adds the key to a command's others, taken from
another but never swapped), a toast saying where the shell would have had it (`shells`);
`App::rebind` writes what changed into `[keys]` (`Bindings::write`, only
what differs from the defaults) and saves as Settings closes. Quit's ⌘Q is
the Mac's, fixed (`keys::fixed`). Windows' Alt+F4 never comes as a key
(winit leaves it to the system), but as `WindowEvent::CloseRequested` with
Alt held, which `App::refuse_alt_f4` refuses as the system's key while
either wait is on, rather than quitting. Snapshots show Flow's own keys only, and
the section with a made-up change and a key awaited as
`settings-keys.png` (`App::show_keys`).

**Command history.** `history.rs` keeps each command run in a tab in SQLite
(`history.sqlite3` beside `config.toml`, a trigram FTS5 index over the text),
and finds commands by any words in them. The text comes from the grid at
Enter: the prompt's end mark (`133;B`) notes where typing starts
(`ShellState::input_start`), and `Session::write` reads from there to the end
of the line (`Emulator::input_from`). zsh's `preexec`, PowerShell's prompt
and bash 4.4's `PS0` (`__flow_line`) also send the line itself (OSC
633;E, every control character escaped as VS Code does), which replaces the grid's, since that can
hold a suggestion drawn after the cursor. bash's is its history's newest
line, which is this one where it has the number HISTCMD had at the prompt
(noted last before it, `__flow_prompt`'s `__flow_histcmd`, after the
user's `history -n`). Any other goes as only the newest
(`633;E;<line>;<nonce>;newest`, `Marker::CommandLine`'s `newest`), as
3.2's trap sends the newest it saw at the prompt: HISTCONTROL or
HISTIGNORE left this line out (the newest is the line before), or
erasedups took an older copy out (this one, a number lower). Then
`handle_marker` keeps the grid's text (`ShellState::command`) only where
it's that line (a copy ignoredups left out, erasedups' line), else drops
it and notes `HistoryNote::LeftOut`, which takes away the row the grid's
text made: a background job's output at the prompt can draw a prompt
with words of its own there, which no nonce guards
(`a_line_bashs_history_left_out_names_nothing_a_background_job_drew`,
`macos/tests/shells.rs`'s background job). Only text read off the screen
is weighed (`ShellState::command_read`, set by `Session::write`, cleared
by the shell's own word): a paste's later lines, which bash 5 reads one by
one with no prompt between them, come as only the newest and leave the
first's exact word (where the first runs nothing, a comment, no word came
before them, and the paste names nothing). And once the hook has said a
line with the nonce (`ShellState::lines_said`), a "done" with screen text
that no word came for (Enter alone, blanks, a comment, a syntax error, a
line of 4,000 characters or more, bash 3.2's that begins in a subshell or
runs no simple command, a function's definition, for which no DEBUG trap
runs) drops it and
notes `LeftOut` too, so Enter alone after a background job's prompt names
nothing for Ask why (`an_enter_that_runs_nothing_names_nothing_a_background_job_drew`,
in core and both OSes' `shells.rs`); an exact empty line is no command.
An older bash (macOS's 3.2) reads
HISTCMD as 1 in `PROMPT_COMMAND`, so `__flow_old_line` sends it from a
`DEBUG` trap that `__flow_prompt` arms (or bash-preexec's
`preexec_functions`) when the newest history entry isn't the one at the last
prompt (`__flow_seen`). Anything printed in a tab can send 633;E, so each
pane has a nonce (`shell::new_nonce`, 128 bits from getrandom, apart from
`Session::key`, since the hook sends it back through the output), given as
`FLOW_NONCE` to the shells whose hooks say their lines (`shell::give_nonce`,
named in WSL's `WSLENV`; removed for cmd, ash and sh): the hooks keep it in
a variable of their own, never exported (`__flow_nonce`, PowerShell's
`$global:__flowNonce`), and unset it before the user's files run
(PowerShell's after its profile), so no program started from the prompt
has it, and send `633;E;<line>;<nonce>` as VS Code's do (and the "done"
and start marks with it, see Shell integration). `osc.rs` hands the nonce
on (`Marker::CommandLine`), and `handle_marker` passes over a line without
the pane's (`ShellState::forged`, `ShellState::nonce`,
`flow_command::same_key`; an empty one takes none), changing nothing, so
the grid's text stands for the history, `Finished` (the notification) and
Ask why's `Failure`. A shell started inside another (or on a host reached
with ssh) runs none of Flow's hooks, and Flow notes nothing typed in it.
The assistant's `search_history`
tool (`ai/tools.rs`, run in `App::start_job`) finds a few commands by their
words, only when the user asks after one; guide mode asks first.
The session queues `HistoryNote`s (entered, exact, left out, done), and
`App::watch_history` records them, leaving out what `history::keep` refuses
(a leading space, a secret). The search is `HistoryPanel` (`ui.rs`),
`⌘R` / `Ctrl+Shift+R`; Enter puts the command at the prompt
(`Session::put_at_prompt`), less any control character but its line
breaks (`shell::without_keys`), which a forged line could hold and the
line editor or the terminal's driver would take as a key, though a tab
stays in a bracketed paste, where it's a letter.

**Apps (`flowapp`).** `src/bin/flowapp.rs`, the `flow-terminal` package's
fourth binary, carried with `flow-help` and `flow-settings` in every
release (`release.sh`'s `games`, `package.sh`, `release.ps1`,
`installer.iss`), keeps Flow's apps as Homebrew does (`update`, `upgrade`,
`install`, `remove`, `list`, `outdated`, `info`, `search`), its words in
`flowapp.*`. What it reads and does is `core/src/apps.rs`: the catalog
(`apps::Catalog`, `apps.json` in Flow-releases, `CATALOG_URL`, or
`FLOWAPP_CATALOG`; `format` 1), each app's `programs`, the one Flow's
releases carried until 0.1.33 (`bundled`, whose `--version` gives the
version it's installed at: `apps::standing`, `version_of`, found in Flow's
own folder of programs, so from 0.1.34, which carries none, only in a copy
an older Flow left there: a Windows portable zip unpacked over an older
one; the Mac's bundle is replaced whole, the Windows setup's
`[InstallDelete]`, `install.sh` and the .deb take them away; so that an
update takes no game away, `App::keep_carried_apps` has
`apps::keep_carried` install the catalog's `bundled` apps on a `flow-apps`
thread at the first start after one, where a config was there before
`App::new` loaded it (`older_flow`), toasting `UserEvent::AppsKept`, once:
`apps/carried.json`, written then, or at once by a Flow that began with
none, `carried_settled`; offline nothing is written and the next start
tries again; `carried_to_keep` passes over one something installed is in
the way of, `in_the_way`, an older `Flow-Rogue`, so the rest settle, and
`carried_in_the_way` names it with that one, `CarriedKept::passed_over`,
for the log and the toast, `CarriedKept::words`, `toast.apps_passed_over`;
release builds only, or `FLOW_UPDATER=1`), and a `Download` for each
system and chip (`apps::platform`, `macos-aarch64`), each with its own
version, URL, size, SHA-256 and Ed25519 signature by `UPDATE_PUBLIC_KEY`,
all checked (`apps::check`) before it's unpacked with the system's `tar`,
and the oldest Flow it works in (`Download::flow`, `--flow` to
`app_catalog` and Flow's `app-release` scripts, `--needs-flow` to
Flow-Games', kept from the download
before where it isn't given): `Download::needs_newer_flow` against
`version::VERSION` holds back one that needs more, which `install_with`
refuses before fetching (`apps::needs_flow`), `flowapp install` and a
named `upgrade` give as an error, a bare `upgrade` and `outdated` say
under the table (`table::held_back`; `Shown::held`, `needs_flow`, not
green, and `info`'s Needs row), and `keep_carried` installs at a later
start, nothing settled meanwhile.
`update` keeps the catalog it read as `apps/catalog.json`
(`apps::read_catalog`), which `list`, `outdated`, `info` and `search`
go by (`catalog_as_saved`, a `Read` with its age by the file's time:
`info`'s Catalog row, and under the others in a terminal past `STALE`,
`table::stale`); `install` (a file of the user's own too) and `upgrade`
read it again first (`catalog_afresh`; `tests/flowapp.rs` runs flowapp in a
scratch home to hold a file to it), falling back to the kept one with
`table::offline`'s note. A fetch that finds nothing (404, 410, a file that
isn't there) is `apps::Failed::Missing`, which `table::failure` says may be
a catalog out of date. A gateway's trouble (502 to 504: GitHub's edge has
answered every page of github.com with 504 for minutes) is tried again
after each of `AGAIN_AFTER`'s waits (`fetch_routed`), and a release's file
on github.com that still won't come is fetched through GitHub's API
(`api_route`, `from_api`: the release's tag lists its files, each asked
for as `application/octet-stream`), checked as any download is.
Settings' Apps (`Section::Apps`) is the same in Flow: `apps::shelf`
works out a `Line` for each app and package of themes installed or in
the saved catalog (its words and its `Doing`s, Install, Upgrade, Remove;
its hint ending in who signed it, `signed_hint`, from what the install
noted, `Shown::noting`, and a row Flow didn't sign marked, `off_mark`),
which `App::see_apps` caches in `apps_view` when Settings opens and after
a change (Settings' entries are built every frame, so never there; a
made-up shelf, `shelf::made_up`, for snapshots, `settings-apps.png`),
handed to `settings::entries` in `Tally::apps` as `SettingId::App(i)` rows
of `Control::Item` (words, then buttons at the row's end; labels kept
once by `interned`). `App::change_app` installs, upgrades or removes on a
`flow-apps` thread (Remove pressed twice, `app_remove_at`), coming back as
`UserEvent::AppsDone`, a toast and `reread_themes`, and where an Install
brought an app with a hello (below), `App::introduce_in_tab`: Settings
closes and a tab in Flow's look types `flowapp hello <app>` once at its
prompt (`hello_tab`, typed as the .deb's update tab is, `tend_typed_tab`).
`apps.check_at_start`
and `apps.check` (`config::AppsConfig`, `settings::APPS_CHECK`) have
`App::check_apps` read the catalog (`shelf::check_due`; at start, and on
focus, release builds or `FLOW_UPDATER=1`; Check now, `SettingId::AppsCatalog`,
any time) into `UserEvent::AppsChecked` with `shelf::upgradable`, a
toast saying what's newer; nothing upgrades by itself. `updates.json`
keeps the version that last ran (`UpdateChecks::ran`), and
`App::remind_of_apps` toasts at the first start of a newer one with apps
installed (`shelf::first_start_of_newer_flow`).
Installed apps live in `apps/` beside config.toml (`apps::dir`): each
version in `<name>/<version>/` (unpacked beside it in
`.<version>.unpacking` by `untar`, which on a Mac takes nothing from the
tarball's AppleDouble entries or file flags, `--no-mac-metadata`,
`--no-acls`, `--no-fflags`, since the Mac's tar would give a folder an ACL
denying delete from them, as a user too, and renamed into place; a refusal
takes that away, and `<name>/` too where it's left empty: `take_away`,
which every folder flowapp unpacks into and every version it deletes go
through, and which, where `remove_dir_all` fails, makes all in it its
owner's to change, `let_owner_change_all` (each folder's mode, as `tar`
gives one the tarball's `0555`, and on a Mac each ACL an older flowapp's
tar gave, `take_acls_off`), following no link, and tries again, an error,
`flowapp.cant_take_away`, where it's still there, so nothing is unpacked
over what an install before left), its programs copied into `bin/`
(`apps::bin_dir`, renamed into place, an old one put aside first, as
Windows won't rename over a running program), and `installed.json`.
A name, version and program become folders and files there, so
`install_kept` takes only those `fit_name`, `fit_version` and
`fit_program` allow (lowercase letters, digits, `-` and `_` for a name, so
two can't share a folder on a Mac or Windows; a version as Flow's SDK
makes them, capitals too, but no trailing dot, and one differing only in
case from the version before keeps their folder; no path, drive, device
or flowapp's own folder, `RESERVED`; a refused one printed through
`printable`, and flowapp's errors through `apps::plain`), which
`examples/app_catalog.rs` holds the catalog's to as well, and
`in_the_way`: a package of the user's own (`Kept::own`: marked
`own_file`, or from a flowapp before 0.2.0 a package of themes or skills
with no programs nor download that the saved catalog doesn't name) never
takes a catalog app's place, nor the catalog's one of the user's own
(`flowapp remove` first; `Shown::newer`, `shelf::upgradable` and the
gallery's `Package::own` offer no upgrade of it), nor either a name
installed but for its case, where none is installed by that very name.
`remove` leaves the folder of a name from before that check where it may
be another's (`own_folder`), and an upgrade that of such a version.
How it prints is `apps::table` (`core/src/apps/table.rs`), laid out
without a terminal for the tests: in one, tables ruled in box drawing
under a bold header, a newer version green and a bundled game dim, each
name an OSC 8 link to its release page (`Download::release_page`), cut
with … to the width `flowapp` reads as `flow-help` does (at most 100
columns); piped or with `NO_COLOR`, plain columns lined up with spaces (a
dash for an empty cell), and piped nothing cut; with `--json` (`Shape`
in `flowapp`) the listings are JSON instead (`table::json`, `json_one`,
English keys whatever the language). A download in a terminal is a line
redrawn (`table::fetching`, `checked`, `failed`) as
`apps::install_with` reports its `Step`s, and in Flow `flowapp`'s
`splash` sets a burst off at the cursor's row (asked with crossterm,
down Flow's pipe on Windows). Who signed a download is
`apps/signing.rs` (`docs/design/flowapp-signatures.md`): `KEYS`, Flow's
own table of keys and whose each is (`signer`, `signer_of` by
fingerprint; never a name from the catalog, which nothing signs),
`fingerprint` (the SHA-256 of a key's DER, as OpenSSL works it out from
`pem`), and `code_signature` reading `codesign -dvv`'s report
(`CodeSigned`, `group`, `is_mach_o`), which `flowapp`'s
`code_signatures` runs on the Mac for `info`. A catalog `Download` has
its `signed` date, which `app_catalog` writes (`apps::today`, or
`--signed`, read by `apps::date`) with `<file>.sig` beside the tarball;
`install_with` keeps the download, the key's fingerprint and the day it
was `checked` in `Kept`, and `install_file` marks `own_file`.
`table::Shown` holds the catalog's `offered` signature and what's
installed's `signing` (`Shown::noting`: what the install noted, else the
catalog's of that version, `unnoted` and dim; `OwnFile`; `WithFlow`),
shown in `list`'s last column (which stands for its From) and
`search`'s before About (`Column::cut_last`), as `info`'s rows, its
`check_yourself` lines (`signing::KEY_URL`) and the JSON's keys.
`flowapp verify` is `apps/verify.rs`: `download_of` (the one kept, else
the catalog's of that version, `Against`), fetched, `check`ed,
`untar`red aside in the temp folder, and `compare`d (`Diff`: a file
changed or missing fails, one added is said). An app's download may hold
`flowapp.json` (`apps::APP_FILE`, `docs/design/app-hello.md`), whose
`hello` names one of its programs and its words (`read_hello`, checked
as it's unpacked, a `flowapp.json` that doesn't read refusing the
install; `apps::hello`, a `Hello` by the program's path in its version):
`flowapp install` runs it after a fresh install in a terminal
(`hello_runs`), `flowapp hello <app>` again. flowapp is an app of its
own catalog too (`apps/itself.rs`, `docs/design/flowapp-updates.md`): its
own version, `version::FLOWAPP` (`flowapp` under
`[workspace.metadata.flow]`, stamped by `build_number::flowapp_version`),
the catalog's `flowapp` entry `bundled`, so `standing` reads the copy Flow
carries (`apps::version_in`, a `--version`'s first word of numbers, since
the flowapps of 0.1.45 and before read their own as `737)`) and
`upgrade` offers the newer, which goes in `apps/bin`, ahead of Flow's on
the PATH. Such a copy finds Flow's own folder by `itself::flow_dir`
(`flow_dirs`: its own folder unless it's in the apps folder, then
`FLOW_GAMES`' others, `games_entries` reading each shell's spelling, then
`usual_flow_dirs`), which flowapp's `bundled_dir` is now; the Flow it runs
in is `itself::flow_version` (`running_flow`: the tab's
`TERM_PROGRAM_VERSION`, else `flow_program`'s `--version`, else its own
build's, set by `main`), which every check of a download's `flow` and its
words go by; and `main` first hands over to the other copy where it's
newer (`hand_over`, `itself::other_copy`, `hand_to`, `FLOWAPP_HANDED`:
`exec` on Unix, waited for on Windows). `flowapp first` and `last` keep what goes
ahead of the system's commands as choices in `apps/first.json`
(`apps/first.rs`'s `Record`, a `Choice` each: a program; its app, where
it's named as `<app>/<program>` or another app has one of its name,
`first::resolve`, `whose`; its `Shells`, all, `Only` or `Except` by
`first::SHELLS`' names, `bash` every flavor of it and `busybox` the
bundled ash; its `How`, the PATH or the prompt alone; and what it takes
away, a `Take`, none for the default of PowerShell's alias and BusyBox's
applet, `Choice::takes_in`), a later choice taking an earlier one's place
on the shells it names (`Record::put`, `put_last`, `Shells::without`,
`meet`). Each pane has a folder of its own,
`apps/first/panes/<flow pid>-<session id>/`, its `FLOW_FIRST`:
`Session::launch` opens it (`first::open_pane`, a `PaneFolder` the
session keeps, which takes it away when dropped) for its shell (`.shell`)
from the record and the pane's own choices (`--here`, `Here`, in
`.here`): a link of each program first on the PATH (a relative symlink
on Unix, to `bin/` or the app's version; a hard link or copy on Windows,
the old one put aside first), `.prompt/` holding those first at the
prompt alone, and `.take`, a `<what> <name>` a line. The first a Flow
opens brings an older Flow's `first/` links into the record (`migrate`;
the links stay for an older Flow still running until flowapp next
changes something, `forget_old_links`) and sweeps the folders of Flows
that are gone on a thread (`sweep`: `/proc`, `kill -0`, `tasklist`).
flowapp's `first`, `last`, `remove` and an install (`follow_firsts`,
which forgets the choices of programs gone) write every pane's folder
afresh (`rewrite_panes`; `--here` its own, found by `FLOW_FIRST`'s last
part, `rewrite_pane`). `first::ahead` says from tables of each shell's
own (bash's and zsh's builtins, PowerShell's aliases and functions,
BusyBox's applets and ash's builtins, cmd's own commands) what still
comes ahead, which `flowapp first` alone lists by shell (`first::rows`,
`table::firsts`, `firsts_json`), and `--take builtin` is refused for one
no program can do the work of (`STANDS_IN`). The hooks look at the
folder before each prompt with no process started (bash's `read -d ''`,
zsh's `$(<file)`), and when its listing, `.prompt/`'s or `.take` changes
they forget remembered commands (`hash -r`, `rehash`), give back what
they took and take what's asked: bash's `enable -n` and zsh's `disable`,
the user's aliases (kept, and put back), and an alias of each name in
`.prompt/`; PowerShell's `__flowFirst` sets a global alias of the name to
the program over its own alias or function (`Set-Alias -Scope Global`,
the alias it had put back after), passing over the dotfiles
`Get-ChildItem` lists. BusyBox runs its applets ahead of the PATH but
for the names in `BB_OVERRIDE_APPLETS`, which it then looks for on the
PATH alone (not found, not the applet), so the ash hook's `__flow_first`
puts there the names `.take` gives `applet`, after the user's own, as the
tab opens and after `flowapp` (a function around it) runs in it: ash has
no PROMPT_COMMAND, and its `command_not_found_handle` runs in the child
that looked, which can't change the shell's variables. cmd has no hook:
its PATH has the folder first, and its own commands come ahead of
everything. zsh's `which` is a builtin, so `ZSH_RC` defines a `which`
(unless the user's files made one) that hands a plain `which` of
programs to `$FLOW_FIRST/which` while there is one, and anything zsh
knows itself (`whence -w` isn't `command`) or another flag to `builtin
which`; with the builtin put aside, every `which` goes to it.
`macos/tests/shells.rs` tries zsh, Homebrew's bash and the Mac's 3.2 in
a pty with stand-ins (`cal`, `pwd`, `which`), its tests going by an
apps' folder of their own (`first::set_root`; a tab opened in the
user's own would change it), `linux/tests/shells.rs` zsh and bash the
same way with `factor` for `cal`, and
`windows/tests/first.rs` (written on Windows) PowerShell, cmd, BusyBox
and Git Bash with copies of `hostname.exe`. `shell::path_with` puts the
pane's folder first on its PATH (`build_command`'s `first`; none for a
session built without one), and the bash, zsh and ash hooks first again
after the user's files. `apps::man_dirs` (each installed app's
`share/man` in its version's folder) go on every tab's `MANPATH` after
the system's (`shell::man_path`: a leading colon, which the Mac's man
and man-db both read as the system's path first, so an app's `ls.1`
never hides the system's; an inherited `MANPATH` keeps its folders
first; none on Windows). `apps::system_command` finds the system's command a
program goes ahead of, for flowapp's words. `shell::games_dirs` puts `apps/bin` on every tab's PATH ahead of the
folder of Flow's own programs (`FLOW_GAMES` lists both, parted as each shell's PATH is,
`games_paths`). `macos/scripts/app-release.sh flow-sdk` makes flow-sdk's
download on the Mac (its C library, header, Python package and flow-demo
from Flow-SDK's tag), signs it with the Developer ID and, with `--notarize`,
has Apple notarize it, packs the tarball owned by 0 with no names, signs
it with Flow's key and writes its entry with `examples/app_catalog.rs`,
which checks the signature first; `examples/third_party_notices.rs` takes
`--manifest-path` for the SDK's workspace. `app-release.sh flowapp`
(and the Windows and Linux scripts' `flowapp`) makes flowapp's own: built
from this checkout, clean unless `--allow-dirty` (`-AllowDirty`), at
`version::FLOWAPP`, `bin/flowapp` and its notices, written into the
catalog `--bundled flowapp` needing Flow 0.1.44 unless `--flow` says.
`windows/scripts/app-release.ps1 flow-sdk` makes the `windows-x86_64` one
the same way (x64 always, as `release.ps1` builds; not code-signed), packs
it with System32's `tar.exe`, unpacks it into a scratch folder to run each
program's `--version` and have the SDK's Python package load its own DLL,
then signs it with `update_key` and the key file, as `make-appcast.ps1`
does; the remap's flags go to the programs' builds alone, so the tools
it runs aren't built again. `linux/scripts/app-release.sh flow-sdk` makes
the `linux-<arch>` one for glibc 2.31 with cargo-zigbuild (as
`package.sh`; `--native` for this computer's), packs it with GNU tar
owned by 0, and tries it unpacked (each program's `--version`, the glibc
each program and the SDK's `.so` needs, no build folder named anywhere in
it, the Python package loading its own library); the key isn't on Linux,
so it's signed on the Mac from its release (`sign-release.sh`) and
`app-release.sh flow-sdk --signature <sig>` (or `--key-file`) writes the
entry for the tarball as made. The games' downloads are made the same
way in Flow-Games, by scripts of its own, as the other repositories
whose apps flowapp installs make theirs: each builds its own programs
from its own workspace and lock, and takes a checkout of Flow-Terminal
(named with `--flow`) for `app_catalog`, `third_party_notices` (with
`--manifest-path` to its own workspace), `update_key` and Sparkle's
`sign_update` in `target/sparkle-*`, so a change to these tools reaches
them all. Flow's own scripts turn a game's name away, saying where its
download is made.

**Outside apps (MCP).** `flow-mcp` (`src/bin/flow-mcp.rs`, the
`flow-terminal` package's fifth binary, carried with `flowapp`) is Flow as
an MCP server for outside apps, which an app's MCP config runs: MCP's
JSON-RPC over its stdin and stdout, by hand (`mcp::protocol`: `initialize`
with `VERSIONS`, `tools/list` from `mcp::tools::TOOLS`, each a `Kind`,
`tools/call` handed on as `Outcome::Call`, answered by `protocol::answer`),
each call sent to the running Flow over the bridge (`mcp::bridge`: a JSON a
line, `ToFlow::Hello` then `Call`, `FromFlow`; `bridge::address`, the
socket `<config_dir>/mcp/flow.sock`, its folder 0700, or Windows' pipe
`\\.\pipe\flow-mcp-<user>`, `FLOW_MCP_SOCKET` overriding), starting Flow
where nothing answers and none runs (`flow_program`: the bundle, `games\`'s
parent, the prefix's `bin/flow`, or a build folder's `flow`, in that
order, since a release's `Contents/Helpers/flow` and `games\flow.exe`
beside it are the flow command), without a
tab's variables (`flow_command::leave_the_tab`, since Linux's `flow` with
a tab's key is the flow command), and on Windows without its own standard
handles (`flow_windows::keep_std_handles`, which the flow command calls
too: Windows hands a child every inheritable handle, so the Flow started
held the app's pipes open). Flow listens
only while `mcp.enabled` (Settings' Outside apps) is on, never for
snapshots or elevated (`App::follow_outside`, `Platform::mcp_listen`: the
Mac's `macos/src/mcp.rs` and Linux's through `bridge::listen_unix`,
`bridge::linux_peer`; Windows' `windows/src/mcp.rs` over `pipe.rs`'s
instances, the user's alone, `pipe::UserOnly`). Each end makes sure the
other is the same user before a word goes: on Unix by the system's word
(`bridge::peer_user`: the Mac's `getpeereid`, Linux's `SO_PEERCRED`),
`listen_unix` closing another user's connection unread and `flow-mcp`
connecting through `bridge::connect_unix`, which refuses a socket another
user listens at (`FLOW_MCP_SOCKET` and the /tmp folder of a deep config
folder may be anywhere, so `listen_as` listens in no folder that's a link
or another's); on Windows, whose pipes' names every user shares,
`flow-mcp` opens the pipe through `flow_windows::mcp::open` at the level
of identification (`as_client`, std's `security_qos_flags` with
`SECURITY_IDENTIFICATION`: by default a pipe's client lets whatever serves
it act as them once it has read what they wrote, which a squatter holding
SeImpersonatePrivilege could use, the open coming before any check; the
image pipe's clients, through Flow-SDK's `Pipe::open`, ask the same level
and check the pipe's owner from the SDK's 0.1.4, `docs/design/mcp-server.md`), and `served_by_flow` takes only a pipe this
user owns (`pipe::owned_by`, read
from flow-mcp's own handle, which no process id can fool) served by a
`flow.exe` whose token is this user's at medium integrity or above
(`pipe::User::of_process`, `GetNamedPipeServerProcessId`). Either refusal
is `bridge::NotUsersFlow`, which `flow-mcp` tells the model as what answers
not being the user's own Flow (`not_flows`), and the system's own
`PermissionDenied` as its not letting `flow-mcp` in (`not_let_in`: another
user's, or a sandbox), both through `kept_out`, starting no Flow.
Each connection is served on a thread of its own
(`bridge::serve`) handing each call to the window as `UserEvent::Mcp`
(`bridge::Event`, a `Request` with its reply channel) and waiting for its
answer however long the user takes, looking every `LOOK_EVERY` meanwhile
whether its app has gone (`bridge::gone`: a read that doesn't wait,
through `Duplex::set_waiting`, kept in the reader's buffer; never on
Windows' pipe), so an app gone while its card waits is `Event::Gone` at
once and its cards go; `Peer` is the system's word on who
connected (the peer's pid, its parent the app, `bridge::peer_from`). The
window's side is `core/src/app/outside.rs` (`App::outside`, apart from the
assistant's tools): an app is an `AppKey` (its client's name and its host
program's path), its `mcp::grants::Grant` (`read`, `tabs`, `settings`,
`commands` Off, Ask or Allowed, `Grant::command`'s `May`, high risk and
`tools::always_asks` asking whatever) kept in `mcp.json` (`Grants`). The
first call puts up a card (`ui::McpCard`, `McpAsk::Use`, `draw_mcp_card`,
`Hit::McpAllow`, `McpDeny`, `McpKind`, Enter, Esc and 1 to 4, a
`DialogKind::Outside` for readers, a notification with Flow behind), the
app's calls waiting behind it; a command to ask about is `McpAsk::Command`,
one holding a control character refused first (`shell::key_in`).
`outside_tool` carries the rest out; commands run in the app's own panes
(`PaneFor::Outside`, its name kept by `ui::outside_slot`; `outside_side`
opens one beside the user's, `Outside::own` lists those it opened), one a
pane at a time, moved on by `advance_outside` from `about_to_wait`
(`outside_due` waking it each second), and a tab it closes goes there too
(`Outside::closing`). Settings' Outside apps (`Section::OutsideApps`,
`outside_rows`, `settings::OutsideView`, `App::see_outside`,
`outside_setting`): the switch, Connect an app (Copy for Claude Code, Copy
JSON), and each app with Forget and its kinds. Help's `Page::Outside`.
`tests/mcp.rs` runs `flow-mcp` against a stand-in Flow, and on Windows
`tests/mcp_windows.rs` against Flow's own pipe, served by a copy of the
test program named `flow.exe` (`stand_in_flow`), since `flow-mcp` answers
no other; Flow's client starts it there from a folder with spaces, and
through a batch file found on the path. Snapshots show
`mcp-ask.png`, `mcp-command.png` and `settings-mcp.png`.

**The assistant's MCP servers.** The other way round, Flow is an MCP
client (`core/src/mcp/client/`): `servers` reads `mcp-servers.json` beside
config.toml (`file`, `load`, `read`: Claude Desktop's `mcpServers` form, a
`Server` a name, its `How`, `Stdio` or `Http`, `type` `sse` refused, and
its `OAuthFile`, Claude Code's `oauth.clientId` and `callbackPort`;
`expand` for `${env:NAME}`; `save`, keeping keys Flow doesn't read;
`Approval`), and each server switched on has a thread of its own
(`Link::start`, `run`: `initialize` with `VERSIONS`, 2025-11-25 first,
declaring elicitation's form and url modes and keeping the server's
capabilities, `Offers`; `list` of each `Kind`, tools, resources, templates
and prompts, every page, again on its `list_changed`, into a `Listing`;
then a `select!` over Flow's asks and the server's `Incoming`). Each
request (`Live::request_for`: a call, `resources/read`, `prompts/get`)
runs on a `flow-mcp-call` thread of its own with an id and a cancel flag
(`Live::underway`), so `Link::cancel` sends `notifications/cancelled` and
lets it go, a late answer to it let go too, and one given up never holds
up the next. What the server sends unasked (`route_unasked`) goes to the
link's thread: `ping` answered, `elicitation/create` read by
`elicit::Ask::read` and handed to the window, anything else refused
(-32601); `result_of` reads -32042 as `Fail::Links`. A request's failure is
a `Fail` (`Said`, `Gone`, `SignIn`, `Links`, `Stopped`; `Fail::text` for
the model). Two `Transport`s, which take `&self` (several threads use one):
`stdio` (the program started, on Windows found first as cmd would find it,
`npx` as its `npx.cmd`, and started as itself, `find_program`, since
through `cmd /C` a path with spaces given arguments lost its quotes;
`%NAME%` in its command and arguments expanded as cmd did,
`expand_percent`; a reader thread handing each answer to its waiting
request by id, `deliver`, its stderr's last lines kept for the error) or
`http` (Streamable HTTP through an agent of its own, `Mcp-Session-Id` and
`MCP-Protocol-Version`, a JSON answer or an event stream read by `sse`'s
`Events`, which keeps each event's id and `retry:`, so a stream dropped
before its answer is picked up with a GET and `Last-Event-ID`,
`read_stream`; an `oauth::Auth`, a pasted token, a sign-in's `Session` or
none, whose 401 is refreshed and tried again once, then, like a 403 for
scopes, `Fail::SignIn` with the `Challenge` of `WWW-Authenticate`, unless
the user gave the token). Events come back as `UserEvent::McpServer`
(`ServerEvent`). `names::for_model` names a tool for the model
(`mcp__<server>__<tool>`, cut with a hash past 64), and `for_gemini`
cleans a schema for Gemini. Signing in is `oauth` (the spec's
authorization of 2025-11-25): `sign_in` finds the protected resource
metadata (the challenge's URL, else `resource_metadata_urls`; none, the
server's origin signs in, as the spec's earlier versions had it), its
first authorization server's metadata (`server_metadata_urls`, RFC 8414's
and OIDC's orders; `check_server` refuses no S256, a wrong issuer, or an
endpoint neither HTTPS nor loopback), the client (the user's typed ID,
one registered before for the same issuer and redirect, else dynamic
registration, `register`), a loopback listener on `CALLBACK_PORT` (else a
free port), PKCE (`pkce_challenge`, `random` from `getrandom`), `state`,
`resource` (`canonical`) and the scope (the challenge's, else
`scopes_supported`), opens it through the window (`OpenToSignIn`), waits
for the browser (`wait_for_code`, `answer_callback`, a page in the user's
words) and trades the code (`tokens`, the client's secret as it
registered). `Kept` (the client, the token endpoint, the resource and the
refresh token) goes in the credential store as JSON through
`secrets::store_long` (split into 1,000-character pieces, `mcp-oauth-<server>`,
`.2` and on, for Windows' 2,560 bytes); the access token stays in memory
(`Session`, refreshed within `EARLY` of running out, a rotated refresh
token saved, a refused one forgotten). The window's side is
`core/src/app/servers.rs` (`App::servers`: `read_servers` as Flow starts
and Settings opens, `follow_servers` from `ai.mcp_servers_on`, leaving a
server failed or waiting for a sign-in as it is while it's on;
`connect_server`'s credentials read on the server's thread, a pasted token
first, `mcp::client::Credentials`; `Standing` `Connecting`, `Connected`
with its `Listing` and whether it's signed in, `Failed`, `SignIn` with the
challenge, `SigningIn`; `sign_in_server` on a `flow-mcp-sign-in` thread,
`cancel_sign_in`, `sign_out_server`, `keep_client_id`; `server_tools`,
each tool in `ai.mcp_tools_on` of each server connected, and
`resource_servers`, those in `ai.mcp_resources_on`, which `agent_settings`
hands the agent as `AiSettings::server_tools` and `server_resources`, and
`AiSettings::offered` offers the assistant alone, `tools::resource_functions`
with them, with `SERVERS_PROMPT`). A call is `ToolCall::Server`;
`ToolCall::ServerResources` (`mcp_resources`, `list_resources`, Flow's own
list) and `ReadResource` (`mcp_read_resource`, `start_read_resource`, asking
as `resource_read_asks`, a read-only tool's). `needs_approval` and
`own_asks` (`own::Why::Server`) ask as `ai.mcp_approval` says
(`server_call_asks`), its card is `server_card`, `start_server_call` sends
it and the job waits as `JobState::Serving` with its server (which
`ToolBatch::waiting` counts, so no frames) for `server_answered`, matched
by job id; `cancel_tools` and the timeout in `advance_tools` call
`give_up_serving`, which tells the server. That timeout is the call's
only clock (the command timeout, `server_wait`, kept a whole timeout
ahead while its server's question shows, `hold_serving`): the link waits
`SERVING_LONGEST`, an hour, so its own wait never fails a call first. `Fail::Links` puts the links on
cards (`links_needed`, a `LinkWait`), the call's card `CardStatus::InBrowser`
with Try Again and Give Up (`Hit::CardRetry`, `CardGiveUp`, `call_again`,
`give_up_link`), made again once every link's `LinkDone` comes, given up
after `LINK_WAIT`. Edit edits a call's arguments as JSON. The server's
questions are `core/src/app/server_ask.rs`: `server_asks` (refused past
`ASKS_WAITING` waiting or `ASKS_A_MINUTE` a minute, `hold_serving` keeping
its call waiting, `tell_of_ask` notifying where Flow is behind) puts an
`AskCard` (`Purpose`: the server's request, a call's link, or a prompt's
arguments) on `Servers::asks`, the first drawn by `ui::draw_server_ask`
(`ui::ServerAsk`, its `AskBody` a form, with each field's `TextField` and
choice, or a link with its host lit, `LinkAsk::around_host`, and
`warning`; `Hit::AskCard`, `AskField`, `AskOption`, `AskSend`,
`AskDecline`), taking every key (`server_ask_key`), typing and pastes
(`server_ask_type`) and clicks (`server_ask_click`) while it's in front
(`server_ask_in_front`); Send checks the form (`elicit::Form::content`, its
`Field`s and `Kind`s read from `requestedSchema`, kept in the server's
order by serde_json's `preserve_order`, which core turns on, defaults in
`Form::start`, `regex` for `pattern`), and `answer_server_ask` sends
accept, decline or cancel (`Link::answer`), opens a link (`Platform::open_url`)
or gets a prompt. A server's `notifications/cancelled` withdraws its card
(`AskWithdrawn`). Readers get it as `DialogKind::ServerAsk`
(`server_ask_dialog`), and F1 its own help page (`help::Page::ServerAsk`,
`Front::server_ask`). Prompts: `submit_ai` hands `/<server>:<prompt>` to
`prompt_command` (`prompt_args` from the words after it, an `AskCard`
for required ones missing), `get_prompt` asks the link (`Link::prompt`) and
`prompt_got` sends its text (`mcp::client::prompt_text`) through
`send_to_assistant`; `/mcp` is `servers_notice`. Settings' MCP servers
(`Section::McpServers`, `servers_rows`, `ServersView`, `server_acts`,
`server_auth_acts`, `App::see_servers`, `server_setting`): Add server…
(`Named::Server`, a draft row until its command or URL is typed,
`server_line_typed`), Open file, Connect or Disconnect and Remove pressed
twice on its row (its hint naming its prompts), a remote one's
Sign-in row (`SettingId::ServerAuth`: Sign in, Cancel or Sign out, Client
ID…, `Named::ClientId`, and Token…, `KeyDialog::server_token`,
`keep_server_token`), Its resources (`ServerResources`), Approval, All on
and All off, and a switch per tool. Snapshots show
`settings-mcp-servers.png`, `mcp-call.png`, `mcp-ask-form.png` and
`mcp-ask-link.png` (`App::show_servers`). Left out on purpose: sampling,
roots, tasks, resource subscriptions, `completion/complete` and the GET
stream outside a request.

**Untrusted output.** Anything printed in a tab (a `cat`ed file, an ssh
host, a theme's start screen and prompt, a package) and what outside apps
and the model hand Flow can make Flow do only what its sender could do
anyway: OSC 7732 needs the pane's key and goes unanswered without it, a
theme's prompt is text in every shell, links and folders count only on
this computer, kitty's files answer alike, the bridge and the pipes check
their peers, the assistant's commands ask where Flow can't read their
program, flowapp's names are held to one rule, control characters are
never typed, and the history takes lines only with the pane's nonce.
`docs/design/untrusted-output.md` says why each is so and what it leaves;
the paragraphs of each part say where it lives.

**Settings from the shell.** `flow-settings` (`src/bin/flow-settings.rs`,
the `flow-terminal` package's third binary, shipped with `flow-help`) asks
Flow with OSC 7732 through the SDK (`flow_sdk::settings`: base64 of a JSON
request, answered the same way; down the pipe on Windows, and on Unix to
the tab's terminal itself, raw for the wait, so its output can be piped or
captured). `osc.rs` finds it (`Marker::Settings`),
the session queues it (`ShellState::settings`, `SettingsAsk`; from the pipe
with a channel `serve_pipe` waits on, a few seconds at most), and
`App::watch_settings` answers each frame (finding the asking pane's tab
by its session then, as an earlier ask may have moved it):
`settings_act` decides against
what the asking pane wears (`App::pane_config`, as Settings on that pane
reads it; `list`, `describe`, `get`, `set`, `reset`,
`reset-terminal`, `restart`), for a `SettingsAsker::Program` (a pane's,
OSC 7732 and the pipe alike) only where the request's `pass` is that
pane's `Session::key` (`flow_command::same_key`, the flow command's
secret, which `flow-settings` sends from `FLOW_TAB_KEY`, `with_pass`),
else `SettingsAct::Unanswered` whatever it asks, and so for one that
doesn't read: anything printed in a tab can ask, and an answer, even a
refusal, is typed into the shell's input, which a line editor in vi
mode reads as commands. So `watch_settings` writes nothing then
(`settings_answer` and `open_tab_asked` give none; a line in the log at
debug; down the pipe the reply dropped, which ends `serve_pipe`'s wait
at once), and the programs say why: `flow-settings` with no
`FLOW_TAB_KEY` asks nothing (`flow_settings.no_key`, exit 2), and its
and `flow`'s words for no answer (`flow_settings.no_answer`,
`flow_command.no_answer`) say the key may be another pane's. The
assistant's and outside apps' `flow_settings`
(`SettingsAsker::Tool`, allowed by the user already) need none, are
always answered in words, and may
neither `restart` nor `reset-terminal` (`program_settings.not_offered`). It takes only keys `open_to_programs` lets through (the look's, sound's,
the window's, the shell's and a few of `ui`'s; never `ai`, `history`, the
user's own `theme` or the saved `themes`, whose prompts every shell shows, or the real numbers),
values of the kind already there, within what Settings offers
(`settings::takes`: its choices, a slider's range, a `#RRGGBB` color;
`allows` checks a `set`, and `describe` answers them with their words,
`takes_in_words`, which `flow-settings --help` prints) and that `Config`
reads back; a change is split as Settings on that pane splits it
(`pane_change`): a theme chosen put on whole (`looks::wear`), the look's
keys (`looks::TAB_KEYS`) the pane's own (`look`, `own`, so `tabs.json`
keeps them; none again where they're the user's), the rest the user's
config, through `follow_config` and `save_config`, with a toast naming the
tab. `restart` restarts at once only while every other pane waits at its
prompt, its own tab's too (`restart_for_program`, `others_wait`), and
otherwise answers `program_settings.restart_busy` with a toast, never
taking or setting the user's `restart_asked`, so a program asking twice is
never the user's yes (`restart_by_key` alone confirms, within
`CONFIRM_WINDOW`); `reset-terminal` plays a full reset into the tab
(`Session::reset_terminal`). `tests/settings.rs` runs `flow-settings` in a
real PTY and checks each request it sends carries the pane's key, that
without one it asks nothing, and that `flow-settings` and `flow-command`
left unanswered say why.

**The flow command.** `flow <command>` in a tab opens a tab like it in
the running Flow and types the command there (`docs/design/flow-command.md`).
What it does is `core/src/flow_command.rs`: `parse` reads its options
(`--split`, `--behind`, `--close`, `--shell`, `--theme`, `--window`, `--`)
into an `Open`, `request` makes an `open-tab` (OSC 7732's `do`, beside
`flow_sdk::settings`' own, which the programs hand in as `ask`, core
taking the SDK only for its tests), and `run` is the whole program:
`src/bin/flow-command.rs` (`Contents/Helpers/flow` through `bundle.sh`,
`games\flow.exe` through `release.ps1` and `installer.iss`), and on Linux
`src/main.rs`, where `flow` is Flow itself and `is_command` says whether
its arguments are the command's (none outside a tab, `--snapshot`,
`--record`, `--carry-on` and a guest's are Flow's). Each pane has a
secret, `Session::key` (`new_key`, 128 bits from `getrandom`), given to
its shell as `FLOW_TAB_KEY` (`flow_command::TAB_KEY`), since Flow takes
OSC 7732 from anything printed in a tab; `watch_settings` hands an
`open-tab` (`open_tab_asked`) to `App::open_tab_asked`
(`core/src/app/flow_tabs.rs`), which `keep_worn`s, makes the asking
pane's `Asker` (its key, shell, `Tab::look_of`, folder) and has the pure
`tab_act` decide (the pass compared by `same_key`, no answer at all for
none and a wrong one alike, as for settings; `opening_for` the rest:
`shell_named`, `theme_named` by id or name; the folder
`flow` ran in where it's one; the words quoted by `shell::quote`:
`posix_word`, `powershell_word`, `windows_word` and `cmd_carets` over the
whole line, cmd's quotes running from word to word, a word holding a `~`
quoted, which bash expands after a `=` or `:` (`--x=~/y`), and letters
past ASCII as letters, since the new shell is one Flow starts, whose
bash hook has readline take them as letters; `Opening::typed_ascii`
(`shell::quote_ascii_for`, zsh's and bash's letters as their bytes in
`$'\xHH'`) is what `tend_flow_tabs` types instead where the shell
hasn't shown the hook's prompt (`FLOW_TAB_WAIT` passed, one its startup
files `exec`ed) or bash's hook said it may take them as keys
(`Session::types_letters`); a word holding a
control character refused, `shell::typable`'s `Untypable`, since no quote
keeps a Ctrl+C, Ctrl+U, Tab or line break from being a key at the prompt,
`parse` refusing it first and `open_guest_tab` too, in
`flow_command::untypable`'s words, a name echoed back with its control
characters `written_out`). `tab_asked` opens it
with `open_tab_sized` and moves it right after the asking tab, the front
kept for `--behind`; `split_asked` splits the asking pane through
`split_front_pane` (`split_pane`'s, with a shell and whether it comes in
front). A `FlowTab` waits in `FlowTabs::waiting` until `tend_flow_tabs`
(each frame and in `about_to_wait`, after `watch_commands`, waking at
`flow_tab_due`) types it with `Session::run_typed` once `at_prompt`, or
after `FLOW_TAB_WAIT`, a start screen's `{wait}` going on first
(`go_on_from_start`); with `--close` it's armed once typed, and
`FlowTabs::ended` (from `watch_commands`) puts a pane whose command ended
with 0 (or cmd's none) in `to_close` for `close_flow_tabs`. `--window`, or
no `FLOW_TAB_KEY`, starts a Flow apart (`start_apart`: `open -n -a` on
the bundle from `Helpers`, `flow.exe` above `games\` detached, Linux's
itself in a session of its own, else the `flow` beside it, with
`guest_args` and without the tab's own variables, `leave_the_tab`:
`TAB_ONLY`, ZDOTDIR given back from `FLOW_USER_ZDOTDIR`); `flow_core::run` reads it back
(`read_guest`), takes `FLOW_TAB_KEY` out of its own environment, logs to
`flow-window.log` and makes the `App` a guest (`as_guest`): its one tab
(`open_guest_tab`), no `tabs.json`, hotkey, outside apps, updates, carried
apps or reminders. Help's `Page::FlowCommand` (`flow-help command`, `flow`
being Flow's own page). `macos/tests/shells.rs` types quoted words into
zsh and bash in a real PTY and reads them back, and prints a request with
the pane's key; `linux/tests/shells.rs` does the same in zsh, bash and
BusyBox's ash (`base64 -w0`, as GNU's wraps).

**Help.** `help.rs` has a page for each of Flow's own screens (`Page`:
Flow's keys over the terminal, the command history, the assistant's panel,
Settings, the color picker, the key dialog, the model question, the new-tab
picker, Flow's apps (`flowapp`, a page of no screen, reached from the
others' links and the menu), About, the update question, and the logos' legal brief that
About's Legal button opens; `Page::ALL`, which the Mac's Help menu
lists as `Command::HelpPage`), its title
and markdown in `help.*` in the locales, with Flow's keys put in as
they're bound (`help::BOUND`, `{help}` and the like, by their names in
`keys::TABLE`, and `FIXED`, the keys inside Flow's screens; the start
screen's markup in the Settings page, such as `{tall}`, isn't among them
and stays as it's written). `Page::Keys`, second, is made from the table
(`keys_page`: each `keys::Group`'s keys, a changed one with its default,
then `help.keys.fixed`), About's Keys link (`Hit::AboutKeys`) opening it
over the card; snapshots show it as `keys.png`.
`App::on_key` opens one before anything else takes the key: F1 for the page
`Page::in_front` picks from what's open (`App::front`, in the order the
screens take keys; none over the terminal, where F1 is the program's), and
`⌘?` / `Ctrl+Shift+?`, or the Mac's Help menu (`Command::Help`), for that
or Flow's own. While `App::help` is open it takes the keys, the clicks and
the wheel, and `ui::draw_help` draws it over the screen it's about, with
`text::layout_linked` (`Page::laid_out`), scrolled by `Help::top`, which the
drawing keeps within the page. Pages link to each other as
`[words](help:<name>)` (`Page::name`, the same as its keys' section;
`Page::linked`): the layout makes a link's words `text::Style::Link(i)`
spans, wrapped like any others, and `help::Laid` keeps those naming a page
(one naming none reads as plain words; a test checks every link in every
language). The assistant's `layout_markdown` reads no links. The drawing
measures each link's pieces on the lines shown, hands them back as
`Hit::HelpLink(i)` and underlines the one under the pointer; Tab and
Shift+Tab move `Help::chosen` (starting from the links `Help::shown`, and
`reveal` scrolls to it once), Enter or a click `Help::follow`s it, and Esc,
Backspace or the ← (`Hit::HelpBack`) go `Help::back` to the page before,
as it was scrolled; Esc on the first page, or the key that opened help,
closes it. Flow's own page ends with a link to every other page, which is
how Windows and Linux, with no Help menu, reach them all. Snapshots show the
history's as `help.png`. The same pages print in a tab with `flow-help`
(`src/bin/flow-help.rs`, the `flow-terminal` package's second binary):
`help::for_terminal` lays them out as help does, to the tab's width (from
crossterm, at most 100 columns), its styles as SGR unless piped or
`NO_COLOR`, and the links alone on their lines (Flow's list of pages) with
the command that prints each. It reads the language from `FLOW_LANGUAGE`,
which `shell::build_command` gives every shell, then `LANG` and the like,
and ships with `flow-settings` and `flowapp` in the folder on the tabs'
PATH (each OS's release scripts, `bundle.sh --game`), so only Flow's tabs
have it.

**The mouse.** A program that asks for the mouse (`?1000h`, `?1002h` or
`?1003h`, which alacritty_terminal keeps in `TermMode`) gets its clicks, drags
and wheel over the terminal, unless Shift is held (Flow selects, opens its
menu and scrolls back then) or `shell.mouse_reporting` is off.
`App::mouse_mode_at` decides that per event, and `mouse.rs` encodes the
reports (SGR, or xterm's bytes) and keeps the buttons held, so a release goes
to the program wherever the pointer is. Reports go through `Session::report`,
not `write`, so a prompt doesn't count them as typing, and motion through
`Session::report_motion`, which a program behind on reading goes without.
Where no program has the mouse, a single click with no key held that
comes up having selected nothing (`App::click_to_move`, noted at the
press) moves the shell's cursor to it, with `shell.click_moves_cursor`
on: `Session::click_to_move` sends ← or → (in the cursor-key mode the
shell set) as many times as `Emulator::letters_to` counts letters
between, the line being typed at the prompt alone (the cursor's line, as
its wraps join its rows, and the one the prompt's `input_start` is on,
while the shell's marks say it's at its prompt), a click on the prompt
going to the input's start and one past the text to its end, a wide
letter's right half to the letter (`macos/tests/shells.rs` clicks in zsh
and bash, `linux/tests/shells.rs` in zsh, bash and BusyBox's ash, and
`linux/tests/theme.rs` behind prompt themes and bash's array
`PROMPT_COMMAND`).
A link a program writes with OSC 8, which alacritty_terminal keeps on each
cell under it, is Flow's whatever the program asks of the mouse: `link.rs`
says what following a link does (`link::target`: http, https and mailto
open, `link::openable`, `web_address` with a host, written only in what an
address holds, `address_letters`: no space, control character or
backslash, and a quote, angle bracket, brace, `|`, `^` or backtick, which
an opener or a browser's command line could read as more, written `%XX`
first, `escaped`; a mailto's fields only `MAIL_FIELDS`, `mail_address`, so
no `attach=`; a `file:`
URL with no host or `localhost` is a `Target::Reveal`, shown selected in
its folder by `Platform::reveal`, the Mac's `open -R`, Explorer's
`/select,` (no quote in the path), Linux's FileManager1 over D-Bus or its
folder through `xdg-open`, so a `file:` link dressed in other words never
starts a program; only where `link::on_this_computer` says, never a path
starting with two separators, `/` or `\` either way round, and on Windows
only a drive's, `C:\…`, or one verbatim as `canonicalize` writes it,
`\\?\C:\…` exactly, in backslashes, with no `.` or `..` (Windows tidies
any other `?` or `.` start as a device's path, whose `..` climbs from the
drive, so `//?/C:/../UNC/host/share` is a share; `local` reads the text as
each system does, so both are tested anywhere, and `file_path` a `file:`
URL), since merely looking at
`\\host\share` there hands the host the user's NTLM hash) and finds the
one at a cell (`link::at`, a wide letter's second half its letter's).
`on_this_computer` also keeps
what any program can name from being looked at before the user's yes: the
folder a shell says it's in (OSC 7, 633;P) for new tabs, splits, Restart
Tab, `tabs.json` and the Attach panel (`App::folder_of`, `reopen`,
`attach_dialog`, the flow command's `Asker`), a 3D model's file
(`session::handle_model`, never asked about), and the assistant's and its
helpers' `read_file`, `write_file` and a wait's file
(`tools::file_elsewhere`, refused unasked in `advance_tools` before
`needs_approval`, in `start_job` and in `helper_refusal`, each judging by
the one read of the pane's folder that the call then opens from, which
`start_job`, `needs_approval`, `own_asks` and `file_call_asks` are
handed, kept with a card while it waits (`Place`), so a program saying
the shell is elsewhere at the right moment can't slip a call past guide
mode's asking; the card says
`card.elsewhere`, the model is told `tools::ELSEWHERE_TOLD`, not to reach
it another way). On Windows a
command naming such a path (`tools::reaches_another_computer`: two
separators where a word or a value starts, a value after `=`, a
parameter's bare name, `-Path:`, or PowerShell's provider, `::`, but not
a scheme's colon, `--index-url=https://`, then a host and a separator,
or a host with a dot) is high risk in `command_risk` and `always_asks`.
`Platform::open_url`
takes only what `link::for_the_opener` lets (a web or mail address, a
`file:` URL with no host (or `localhost`), which Flow makes only for its
own files and so trusts): on
Windows through `ShellExecuteExW`'s "open" (`windows/src/lib.rs`'s
`shell_open`), never Explorer's command line, which read commas and
quotes in an address as its switches; on Linux never a word starting with
a dash. Core calls it, and `reveal`, only through `App::open_url` and
`App::reveal`, which open nothing while `Platform::elevated` (a toast,
`toast.elevated_opens_nothing`), since the program opened would run as
administrator too. A server's link to open (`elicit::LinkAsk::read`) and its page to
sign in on (`oauth::secure`, `loopback` http and https alone) are
`web_address`es too. An address no program marked is a
link too (`link::plain`): on the logical line under the pointer
(`line_search_left`, `line_search_right`, so a wrapped one is found whole)
alacritty's `RegexIter` looks with `link::addresses` (an `http`, `https`,
`mailto` or `file` address up to a space, a control character, a picture's
cell or a quote; kept in `App::addresses`, made once, all lowercase since
alacritty's search heeds case once a pattern has a capital), keeps the
match holding the pointer, takes off a sentence's punctuation and an
unmatched closing bracket (`trimmed`), and only where `target` follows it,
never on a cell a program linked. Either is a `link::Link` (`Marked`, its
cells those carrying it; `Plain`, its uri and the `Match` of its cells,
`Link::covers`). `App::link_under` keeps the one under the pointer in
`App::link`, on each move and each frame (output and scrolling move it),
none once the pointer has left the window; `ui::draw_terminal` underlines
that link's cells in the accent color, `draw_link_target` shows where it
goes (or the file it reveals, `link.reveals`) at the terminal's foot (at its right while the pointer is where the
left would be), and `App::open_link` opens it on ⌘-click (Ctrl+click
elsewhere, `link_modifier`, where the pointer is a hand) through
`Platform::open_url` before the click reaches a program.

**Always on top** (`docs/design/on-top.md`). `window.on_top` keeps the
window on top of every other through winit's `set_window_level`
(`App::follow_on_top`, after the window is made and in `follow_config`;
dropped down too, which floats only by it, as decided 2026-11-18,
since `set_dropped_down` leaves the level alone), where
`Platform::on_top_possible` says (not Linux's Wayland, `hotkey::ON_WAYLAND`,
where Settings' row says the window manager's menu does it, `Tally::no_on_top`,
and the title bar has no pill). It's turned by `App::set_on_top` (a toast
each way) from the title bar's **on-top** pill (`Hit::ToggleOnTop`, beside
the gear, never giving way, `View::on_top`), `Command::ToggleOnTop` (no key
at first, the palette, the Mac's Window menu's Float on Top checked by
`MenuState::on_top`), and Settings' Always on top (`SettingId::OnTop`, in
the Window section above the Hotkey); readers get the pill as a toggle.

**Notifications and the hotkey.** A command that ran at least
`ui.notify_after` seconds (Settings' Notify when done: Off, 10 s, 30 s,
1 min, 5 min; 30 at first) and finishes while `!App::focused`, not
stopped with Ctrl+C, is told of by the system: `notify::wanted` decides
(pure, tested), `notify::body` says the command (`Finished::command`,
cut at `COMMAND_SHOWN`, none where `history::looks_secret`), how it
ended and how long it took (`notify::took`), and `App::notify_done`,
called from `watch_commands` for each pane's `Finished` it asks of
(playing no cue of Flow's for one that notifies), hands
`Platform::notify` the tab's label, the words and the pane's session id
as the tag. `watch_commands` runs from
`about_to_wait` as well as each frame, so a window that gets no frames
(hidden, on Windows) still notifies. The Mac posts to Notification
Center (`macos/src/notify.rs`, objc2-user-notifications and block2):
authorization asked with each, which macOS asks the user the first time
only; a `define_class!` delegate (`FlowNotificationDelegate`) shows it
while Flow is in front too and sends a click's tag as
`UserEvent::NotificationClicked` through a waker kept from
`install_menus`, and `App::notification_clicked` brings forward the tab
holding that session (`Tab::holds`) and focuses its pane (tabs and panes
move). A bare binary has no bundle, without
which UNUserNotificationCenter raises, so it bounces the Dock icon
(`request_user_attention`). Windows shows a toast
(`windows/src/notify.rs`: WinRT's `ToastNotificationManager` through the
`windows` crate, its XML's words escaped, tagged by the pane so a pane's
next takes its place) under Flow's own AppUserModelID,
`com.arcanii.Flow`, which `install_menus` registers for the user
(`HKCU\Software\Classes\AppUserModelId`, its name and an icon written
beside the config as `flow-notification.png`) rather than with a Start
menu shortcut, so the portable zip and a build run from its folder have
toasts and the taskbar's grouping is left as it was; the installer's
`[Registry]` only takes the key away at uninstall. A click comes on
WinRT's thread to the toast's `Activated` (the last `KEPT` toasts held
for it) and goes through the waker as the Mac's does; where Windows has
toasts off for Flow (`ToastNotifier::Setting`) or WinRT fails, the
taskbar button flashes instead (winit's informational attention,
FlashWindowEx with FLASHW_TIMERNOFG, which only pauses while Flow is in
front and flashes again once it isn't), stopped for good by
`Platform::attended` as the window gets the focus (`FLASHW_STOP`, which
winit's own stop skips while the window is the active one). Linux asks for a desktop notification with gdbus, notify-send or
dbus-send in turn on a `flow-notify` thread (`linux/src/notify.rs`;
gdbus's arguments follow a `--`, or the timeout's -1 reads as an option;
dbus-send can't write the hints' empty a{sv}, so checking daemons may
refuse it), and marks the window, without following a click.
The drop-down hotkey is `window.hotkey` (`hotkey::Hotkey::parse`:
modifiers and a key joined by `+`; Settings offers `hotkey::CHOICES`,
none, ⌃`, ⌥Space, F12 (not on Windows, whose RegisterHotKey refuses F12,
kept for debuggers, as already taken, with Shift or Alt too) and ⌃Space,
which is then no longer the
assistant's, then Press a key…: `App::start_pressing_hotkey` puts
`HotkeyHeld::pressing`, which `follow_hotkey` lets the hotkey go for and
`Tally::hotkey_pressing` shows, and `App::press_hotkey`, ahead of every
other key in `on_key`, F1 too, takes winit's `physical_key` as
`Hotkey::pressed` allows, `Key::of` mapping its `KeyCode`, refusing the
system's (`keys::refused`, `Pressing::System`) and taking one of Flow's
commands' keys from it once `follow_hotkey` holds it (`Bindings::holder`,
`toast.hotkey_took`; where it fails, Wayland's or XWayland's, the command
keeps it), the
Mac's menus giving theirs up meanwhile (`MenuState::capturing`), written back
with `Hotkey::to_config`, which `parse` reads as itself; Esc, a click or
Settings closing, `tend_drop_down`, end the wait), none by default, and
`window.drop_down` (on). `App::follow_hotkey` takes it through
`Platform::register_hotkey` when it changes (after the window, in
`create_window`, and from Settings and `follow_config`), noting a failure
for Settings' hint (`Tally::hotkey_failed`) and a toast.
`UserEvent::Hotkey` comes to `App::on_hotkey`: in front, the window is
put away (`put_away_window`); otherwise `drop_down` puts it at
`Platform::drop_down_rect` (by default the window's monitor's top part,
`hotkey::top_part` with `window.drop_down_height`'s `Share`), size
first, then `set_dropped_down(true, over_full_screen)` (nothing but on
the Mac: never the level, which stays `window.on_top`'s), then place,
and `bring_forward` shows and focuses it; no
hotkey, or Drop down off, ends being dropped (`App::undrop`). With
`window.drop_down_slide`, outside efficient mode and where winit can
place the window (not Wayland: `App::slides`), it's placed right above
its screen (`hotkey::slide_from`, none where another screen,
`Platform::screens`, is there) and a `hotkey::Slide` eases its origin
down over `SLIDE` with `set_outer_position` (a move, no resize), moved
from `about_to_wait` by `App::tend_drop_down` at most every
`SLIDE_STEP` (each move comes back as an event), `moving` true
meanwhile; put away, it slides up from where it is and is put away
there (`Platform::put_away`), then put back where it drops, out of
sight, for whatever brings it forward next. The hotkey while it goes
turns it back. `window.drop_down_hides` puts it away once Flow has been
without the keys `HIDE_AFTER` (`hotkey::Unfocused`, from
`App::drop_down_focus` in `WindowEvent::Focused`; X11 takes them a
moment as its grab begins, so `on_hotkey` counts Flow that lost them
that recently as in front too, `Unfocused::just_lost`), not while one of
Flow's own file panels has them (`App::in_dialog` around each `rfd`
call, which starts the wait again as it closes). The Mac takes the key with Carbon's
`RegisterEventHotKey` and an `InstallEventHandler` on the application's
target, hand-written `extern "C"` linking Carbon (`macos/src/hotkey.rs`,
`kVK_*` codes in `key_code`), drops onto the room for windows of the
screen the pointer is on (`flipped`, AppKit's points up from the main
screen's foot into winit's pixels down from its top, at the window's
scale, which winit converts back with), joins every Space there and
full-screen apps' (`CanJoinAllSpaces | FullScreenAuxiliary`, less
`MoveToActiveSpace` and `FullScreenPrimary`, which AppKit won't pair
with them; the behavior from before put back after), placed by Flow
alone meanwhile (a `constrainFrameRect:toScreen:` of its own added to
winit's `WinitWindow`, which leaves the frame as it is while dropped
down and is NSWindow's otherwise, so a window above its screen isn't
moved onto it as it's shown), puts away with `NSApplication hide:` and
brings forward with `unhide:`. With `window.drop_down_over_full_screen`
(Settings shows it on the Mac alone) Flow is an accessory app while
it's down (`setActivationPolicy`, no Dock icon or menu bar), which a
full-screen app's Space lets in, and Regular again as it's put away or
undropped (`set_accessory`). Windows holds
the key with `RegisterHotKey` and no window on a `flow-hotkey` thread
whose queue WM_HOTKEY comes to, ended with WM_QUIT for the next
(`windows/src/hotkey.rs`), and while dropped down DWM's fades as the
window is shown and hidden are off (`window::set_fades`,
`DWMWA_TRANSITIONS_FORCEDISABLED`, from `set_dropped_down`), since a window
hidden at its slide's top and put back where it drops faded out there. Linux grabs it on X11's root window through
x11rb on a connection of its own, with Caps Lock's and Num Lock's
modifiers too, a `flow-hotkey` thread reading presses and skipping a
held key's repeats and keeping each press's time, and brings the window
forward by asking for the keys again as a pager does (EWMH's source 2,
with that time: `hotkey::bring_forward`), since winit asks as an
application, with no time, which GNOME's and KDE's window managers turn
down, and not at all for a window still being mapped again; on Wayland
(`hotkey::ON_WAYLAND`, from `window_created`) it says false, and so it
does under XWayland (the X server has the `XWAYLAND` extension,
`Display::xwayland`), whose grab hears keys only while an X11 window has
them, so the key that put the window away would never bring it back.
Neither has a hotkey at all (`Platform::hotkey_possible`, Linux's
`hotkey::possible`): Settings' Hotkey row is a `Control::Item` saying so
(`Tally::no_hotkey`, `hotkey.none_here` and `hint_none_here`), without the
drop-down's rows, `apply_setting` leaves it, and `follow_hotkey` keeps a
hotkey set elsewhere in the config without trying it, so no toast comes
at each start.
A Flow put away is hidden, with no taskbar button, so it can be found
again (`docs/design/put-away.md`; Windows alone, 0.1.44.3). `App::put_away_now`
follows `Platform::put_away` with `Platform::away_icon(Some(tip))`, and
`App::bring_forward` (every bring forward), the window getting the keys
and `Platform::exiting` take it away with `away_icon(None)`. On Windows
that's an icon in the notification area (`windows/src/tray.rs`: a
`flow-tray` thread with a hidden window of its own, not message-only so it
hears `TaskbarCreated`, `Shell_NotifyIconW` at `NOTIFYICON_VERSION_4`,
flow.exe's icon resource 1), whose click is `UserEvent::AwayIconShow` and
whose menu gives Show Flow and Quit Flow (`AwayIconQuit`). Starting
this Flow again brings it forward, first thing in `flow_core::run` before
the logger (which would empty its `flow.log`): a start that
`instance::takes_part` (no guest, `--snapshot`, `--record`, `--carry-on`
or elevated Flow) asks `Platform::hand_to_running` with `instance::id`
(a hash of the program and config folder, lowercased on Windows, so
another copy or a scratch `APPDATA` starts apart), a `Start`: Windows'
(`windows/src/instance.rs`) holds the mutex `Local\Flow.Instance.<id>`
for the first start, and a later one sets `Flow.Again.<id>`
(`AllowSetForegroundWindow` first) and waits `AGAIN_WAIT` for
`Flow.Forward.<id>`, quitting once it comes (`Handed`), else running
beside it (`Beside`, logging to `flow-window.log`). The first's
`flow-again` thread (`Platform::listen_for_starts`) sends
`UserEvent::StartedAgain`, `App::come_forward` brings the window back as
the hotkey does (dropped down where it was or the hotkey would drop it),
and `Platform::came_forward` sets the answer. The Mac's Dock unhides
Flow and Launch Services hands it a second start; Linux has neither yet.

**Sound.** `sound.rs` makes every sound in code: a `Cue` plays as a
`Voice` (oscillators, filtered noise, envelopes, an equal-power pan), and
`Mixer` sums the voices with a soft limit. `Sound`, which `App` keeps, sends
voices to a thread of its own (`flow-sound`, started at the first sound),
which opens the default output with cpal when one comes, hands voices to the
output's callback over a channel (the callback allocates nothing) and closes
the output after `IDLE` with nothing playing. Snapshots and recordings make
none. cpal is a dependency on macOS and Windows only, since on Linux it
would need ALSA's headers to build: there `output` loads ALSA's library as
Flow runs (`core/src/sound/alsa.rs`, a copy of the SDK's), and a
thread of its own (`flow-sound-out`) writes the mix to ALSA's default
device a block at a time, which carries it on to PipeWire or PulseAudio;
the SDK's audio does the same for Flow Rogue's music. What plays it, all in `App`: `watch_bells` (alacritty's bell sets
`ShellState::bell_at`; `sound.bell` picks `Platform::beep`, the system's
alert sound, or Flow's `Cue::Bell`), `cue` from `watch_commands`, the
assistant's approval and a task done (`sound.cues`), `watch_effects` for the
effects programs ask for (`sound.effects`; heard even with the liquid off),
and `watch_sounds` for `osc::SoundRequest`s (OSC 7731's `fx=sound`, and
DECPS, `CSI … , ~`, which `osc.rs` reads; `sound.programs`). An effect's
`FxSound` is its kind's cue, none or a named one, played at the request's
`volume` (OSC 7731's `volume=`, a program's own level for its effects'
sounds, which leaves how they look). OSC 7731's `fx=tint` is a
`Marker::Tint`: the tab keeps its `Wash` (`ShellState::wash`) while it
holds the alternate screen, and `App::step_wash` eases the glass's tint and
the liquid's dye gain toward it. The liquid hands back what
can be heard (`FrameOutput::heard`: its own bolts, bubbles surfacing,
raindrops landing) for `hear_liquid` (`sound.liquid`); a look's `Sounds`
says what its machine sounded like for `hear_machines` (a tube's `Cue::Hum`
laid every `HUM_EVERY`, its level, harmonics and pitch drifting with the
mixer's clock, `Mixer::add` stamping each voice where it starts on it,
`Voice::clock`, so one laid on the last is in step: `hum_turns` and
`fan_turns` are the pitch's closed-form sums, in f64; a teletype's `Cue::Clack` for each letter its start
screen types and `Cue::Return` for each line, `ShellState::typed_out` and
`typed_lines`, and a disk drive's `Cue::Seek` as a start screen begins,
`booting`, through `machine_boot`) and `machine_key` (`sound.machines`; a
teletype's Return runs its carriage back, and a letter struck
`MARGIN_BELL` columns from the line's end rings `Cue::Margin`,
`App::at_margin`). Lines a running command writes (`ShellState::output_lines`,
not while the alternate screen is up) run a teletype's `Cue::Punch` and
the workstation's `Cue::Print` (`Sound::play_run`, queued at most a second
ahead), and the PC's `Cue::Fan` is laid with its hum and `Cue::FanAir`, the
other half of its air, on a spacing of its own (`sound::FAN_LAYERS`,
`App::fan_air_at`): each layer's air fades at equal power
(`sound::equal_power`), its noise being each voice's own, and the
blades' tone in a straight line, in step on the mixer's clock, so no
crossing dips and none comes at a beat. The green and amber
terminals click with `Cue::Tick`, the graphics workstation starts with
`Cue::Startup` (`machine_boot`), the Flow Instruments Home Computer
(`Sounds::Tones`) with its sound chip's high beep (`Sounds::start_tone`, a
`Cue::Note`, muted as `Part::Tone`), and the Flow notanAmiga CLI
(`Sounds::Floppy`) with its floppy drive's seek, as it starts and again as
its disk goes in. With `sound.bell` at "flow" and
Machines' sounds on, `ring_bell` rings a machine's own bell where it had
one (`Sounds::bell`: the teletype's `Cue::Margin`, a terminal's or the PC's
`Cue::Beep`, the sound chip's low honk, a `Cue::Note` too). The machines' cues aren't among the names programs ask for
(`sound::NAMES`).
Only `sound.enabled` gates Flow's own; the bell follows `sound.bell` alone,
and `sound.bell_flash` washes the terminal with the text's color
(`ui::draw_bell_flash`).

**Words.** Everything the user reads comes from `core/locales/<code>.json`
through `i18n::t(key)` / `tf(key, &[("name", &value)])` (dotted keys, `{name}`
placeholders), never a literal in the code; menus in the OS crates too. What
goes to the model (tool results, the system prompt, `terminal_context`) stays
English, and a message that goes to both uses `i18n::english` for the model
(see `App::fail_job`). A new string needs its key in every locale file: the
i18n tests check that the files match, placeholders included, and that every
`"section.key"` literal in the sources exists. The language is process-wide
(`ui.language`, "auto" following `Platform::languages`), but per thread in
core's own tests, so they can switch it. In the interface's text, Chinese and
Japanese characters advance 1em rather than two cells (`Canvas::text`), and
wrapping breaks between them with kinsoku (`text.rs`).

**Config and settings.** `config.rs` is the TOML config (`#[serde(default)]`
structs, written on first run). `settings.rs` is the in-app settings menu: its
rows, ranges, hints and how each change maps onto the config. Changes apply
live and are saved when the menu closes. The rows come in sections
(`settings::Section`), each a header that's a row too
(`Entry::Header`, `Control::Section`), so the keys reach it: a change
opens or closes it, and a closed one's rows aren't built
(`ui.closed_sections`; `App::open_settings_at` opens the section of the
setting it shows, found by `settings::section_of`; Collapse all and Expand
all beside the card's ✕, `Hit::SettingsCollapseAll`, `SettingsExpandAll`,
`App::open_all_sections` through `settings::open_all` over `Section::ALL`,
dim where `sections_shown` says they'd change nothing). Only the drawing knows the rows'
heights, so it keeps the list's scroll in `SettingsMenu::scroll` (a `Cell`),
moving it just far enough to show a row the keys select
(`settings::reveal`); clicks leave it, and the wheel scrolls it. The color
rows open a `ColorPicker` over the menu, whose field and hue bar are a quad
kind of their own (`KIND_PICKER`, the `hsv` function in `ui.wgsl`);
`palette::from_hsv` works out colors the same way, so the color picked is the
one under the marker. A preset file is the look's sections of the config
(`config::PRESET_SECTIONS`) as JSON (`Config::preset_json`); loading one
merges only the keys it has (`Config::apply_preset_json`), and
`App::take_look` makes the rest of the app follow. The panels for picking the
file are the system's own, through `rfd`. `settings::entries` is handed what
the window holds as it is the font choices (a `TextGrid` from
`App::text_grid`: the layout's grid and cell, which follows the font's
metrics at the size picked, and the display's scale), for the line under
Font size's slider (`Control::Slider`'s `note`, a taller row; `⌘+` and
`⌘-` say it in a toast, `App::step_font_size`) and for Fit window to
(`SettingId::FitScreen`, `settings::screens`: the worn machine's
`Look::screen` first, then `SCREENS`, then the user's own, `window.sizes`,
which Your sizes, `SettingId::OwnSizes`, a text row under the Fit rows,
types in place: `settings::parse_sizes` reads columns × rows however
they're written, within `SIZE_COLS` and `SIZE_ROWS`, and
`App::finish_editing` writes them alike, `sizes_text`, as Enter keeps
them, `read_sizes` keeping what it left out (`SizesRead`: sizes outside
the limits, numbers that made no size) for `sizes_note`'s toast, which
`ui::draw` puts over Settings, as it does every toast while Settings is
open; `settings::Was` is what Esc puts back, a theme or a row's own
text), a choice lit where the window's grid
matches one (none, past its end, otherwise; the arrows a narrow card draws
step from it as the keys do, `settings::choice_index`) and acted on once,
never saved: `App::fit_window` puts a maximized window back to its own size
first (not on the Mac, where a zoomed window is only a frame) and sizes it
at the first resize that leaves it unmaximized (`pending_fit`), since
Wayland only asks the compositor and drops a size asked for meanwhile, or
once `FIT_WAIT` has passed, saying so if it's still maximized
(`fit_after_wait`); `App::size_window` asks, in whole points rounded up
(Wayland rounds pixels to the nearest point), for the window
`Layout::size_for_part` gives (the smallest that `Layout::compute` lays out
with that grid in the part it's asked about, `App::front_part`: a split
tab's pane in front, else the whole terminal; found by halving, since a
border's margins and the panel grow with the window),
within `MIN_WINDOW` and the screen's room (`App::window_room`), and says
what it holds: from the size `request_inner_size` hands back where it does
(Wayland's, which keeps a tiled window's: `toast.fit_kept`), and for a grid
the least window holds too much of, from what text size it would fit
(`Layout::text_size_within_part`). Fit text to (`SettingId::FitText`, `App::fit_text`)
is the other way round: the largest whole size at which the window as it
is holds the screen in that part (`settings::text_size_for`, from a guess by
the cell in use, the atlas measuring it and the sizes beside it through
`layout_wearing` and `front_part`), set as Font size sets it. `text_grid`,
what Font size's note and the Fit rows name, is the pane in front's too.

**Searching Settings.** `settings_search.rs`'s `Index` keeps each row's
words, lowercase, each with what a word found there counts: its label, its
section's header, its hint, its options' names (`options`: a choice's, a
color row's), and its key in config.toml (`settings::config_key`, every
row's, which a test finds in the config), in the language in use and in
English (`i18n::in_english`, a thread's own override, so the rows are
built twice as the field opens, every section open). `Index::find` takes
every word typed, each the start of a word there (`places`), or anywhere
for one with letters past ASCII (Japanese), a label above a hint, the
palette's `score` breaking ties, and gives each `Found`'s section and the
ranges of its label to light. `SettingsMenu::search` (a
`settings::SettingsSearch`) is the field while it has the keys:
`App::start_settings_search` (Find's key through `settings_search_key`,
and `on_command`'s `Command::Find` while Settings is open, since the Mac's
menu takes ⌘F first; a click on `Hit::SettingsSearch`; a letter typed
where no row takes it), `settings_search_changed`, `paste_text` and
`type_text` (the input method's, the field handing back its caret).
While something's typed, `App::settings_entries` is the rows found, in
order, from every section, so the keys, clicks, `apply_setting` and
readers all go by them; Enter or a click on a found row's label
(`go_to_found`) opens its section and chooses it in place, and Esc
empties the field. `ui::draw_settings` draws the field at the end of the
row of tabs (the chips stop short of it), a found row's label through
`draw_lit` with its section beside it, and `settings.search.none` for
nothing found; readers get it as a `SearchInput` (`Field::search`,
`Focus::SettingsSearch`, `Do::FocusSettingsSearch`). Snapshots show
"glow" found as `settings-search.png` (`App::show_settings_search`).

**Adding a liquid style** touches: `Style` in `fx.rs` (`from_config`,
`material`, `shader_mode`, and its arm in `Effects::frame`), `Material` in
`render/fluid.rs`, a branch of `fs_glass`, `STYLES` and the hints in
`settings.rs`, `SHOWCASE` in `app.rs`, and the style table in
`docs/manual/liquid-styles.md` and the `fx.style` list in
`docs/manual/configuration.md`, with a new `docs/screenshots/style-<name>.png`.

**Games.** Flow's games, Flow Rogue (`flow-rogue`), Flow Mines
(`flow-mines`) and Flow Invaders (`flow-invaders`, being built), are
programs that run in its tabs to show off what it draws. They live in a
private repository of their own, arcanii/Flow-Games, each game with its
own version; they were in `games/` here until after 0.1.32, and their
history went with them. They
use none of Flow's crates, only Flow's SDK, and this workspace takes
nothing of theirs: Flow-Games builds each game's own bin from its own
workspace and lock, and makes its downloads for `flowapp` with its own
`scripts/` (see Apps), so Flow builds without access to that private
repository. From 0.1.34 Flow's own releases carry no game, and `flowapp`
installs them. (Until 0.1.34, Flow took the games by a tag of Flow-Games
and built them through a `carried/` package, the `flow-games` package,
for those scripts; every computer building Flow then had to reach the
private repository, through `.cargo/config.toml`'s `git-fetch-with-cli`.)
A change to a game is tried in Flow by running it from a checkout of
Flow-Games in a Flow tab. `third_party_notices` takes a game's crates by
name (`--manifest-path <Flow-Games>/Cargo.toml --package flow-rogue`), so
what it's built from is listed and it, that workspace's own, isn't.
Flow.app carries `flow-help`, `flow-settings` and
`flowapp` in `Contents/Helpers` (`bundle.sh --game`, which `release.sh`
passes for each), and the Windows setup and zip in `games\` beside flow.exe
(`release.ps1`, `installer.iss`), as Flow carried the games and the demos
until 0.1.33 (the folder keeps its name); each OS's
`Platform::games_dir` finds that folder, and `shell::set_games_dir` puts it
last on every shell's PATH (without the games of a Flow this one was opened
from, its inherited `FLOW_GAMES`, which would shadow them) and in
`FLOW_GAMES`, which the bash, zsh and ash
hooks add again after the user's startup files. Git Bash's PATH is Unix-style
and BusyBox's parted by semicolons, so `FLOW_GAMES` is spelled for each
(`shell::games_path`) and the ash hook looks for `;`. Under
`cargo run` there is no games folder: run a game from a checkout of
Flow-Games (`cargo run --release -p flow-rogue` there), or as `flowapp`
installs it.

**Demos.** `flow-demo` and its demos live in Flow-SDK (see the
Workspace); `demos/` here is the `flow-demo` package that calls its
program, built with Flow's lock and release profile. Flow's releases
carried it until 0.1.33; from 0.1.34 it's in flow-sdk's download, which
`flowapp install flow-sdk` puts on the tabs' PATH.

**`examples/` are tools**, not samples: `theme_package` writes and packs
`flow-core-themes` (see Themes); `skill_package` packs `flow-skills`
from `assets/skills/` (a folder per skill, its `SKILL.md`), each skill
read back as Flow reads them, for `app_catalog --skills`; `show_model` sends a model to the
terminal with the Ratty protocol, to try 3D models by hand (on Windows down
the tab's pipe, Flow-SDK's `Pipe`, with the cursor from the console); `sounds` writes
each of Flow's sounds to a WAV file, to listen to them away from Flow; `make_icon` redraws `assets/icon/`,
`make_movie` turns one style's `--record` frames into a looping animated WebP
(like those in the public Flow-releases README),
`update_key` makes the update key and signs and verifies with it, and
`third_party_notices` lists the crates for the macOS and Linux releases
(`release.sh`, `package.sh`).

## Versions and releases

- The marketing version is `version` under `[workspace.package]` in the root
  `Cargo.toml`. The build number is the git commit count plus
  `build_offset` under `[workspace.metadata.flow]`, which keeps it rising in
  arcanii/Flow-Terminal, whose history began afresh from 0.1.46.DEV (2000
  there; 0 in the private repository it was made from, whose commit count
  must stay below that while it still releases). `core/build.rs`
  stamps both (`flow --version` prints them), so `macos/scripts/release.sh`
  refuses a dirty tree unless given `--allow-dirty`.
- **The version says what's being worked on** (a rule since
  2026-11-15): as soon as a version is released, `version` goes up to the
  next (0.1.46 after 0.1.45) with `dev = true` under
  `[workspace.metadata.flow]`, so every build of the work toward it shows
  as `0.1.46.DEV (build N)` (`build_number::dev`, `FLOW_DEV`,
  `version::DEV`, `version::shown`, `display`: About, Settings, `flow
  --version`, `flowapp --version`, the log, the Windows exe's product
  version). `version::VERSION` stays numbers alone, as programs
  (`TERM_PROGRAM_VERSION`, XTVERSION) and flowapp's catalog compare it,
  and a DEV build's About links the list of releases. Releasing sets
  `dev = false` in the version's commit ("Make Flow's version 0.1.46");
  `release.sh`, `release.ps1` and `package.sh` refuse a DEV build unless
  it's a test build (`--allow-dirty`, `-AllowDirty`), whose files say
  `0.1.46.DEV`. Once every system's release is out, the next commit
  bumps `version` and sets `dev = true` again.
- **flowapp has a version of its own** (`flowapp` under
  `[workspace.metadata.flow]`, `version::FLOWAPP`, from 0.2.0, decided
  2026-11-20): it goes up with each download of flowapp published
  (0.2.1 for a fix), never DEV, so it can go out between Flow's releases
  as an app of the catalog, made by the `app-release` scripts' `flowapp`.
  Flow's releases carry whatever flowapp master holds. `flowapp
  --version` says it and the build: `flowapp 0.2.0 (build 750)`.
- A patch release adds a fourth number to what people see: `patch` under
  `[workspace.metadata.flow]` (`build_number::shown_version`: 0.1.25 with
  `patch = 1` was `version::VERSION` `0.1.25.1`), back to 0 when `version`
  goes up. It was 0 for 0.1.48, on every system, which brought `/model`
  listing each provider's own models (`docs/design/live-models.md`), a
  phosphor's glow kept in its color without the tube, your own and saved
  themes wearing the notanAmiga's frame, and more of glTF (anisotropy,
  iridescence, instances on the GPU, whole files from the tools; decided
  2026-11-27), the Mac's first. It was 0 for 0.1.47, on every system,
  the first released from arcanii/Flow-Terminal, which brought the
  assistant's input (editing,
  its history across restarts and `/model`,
  `docs/design/assistant-input.md`), the Ask item named by the provider
  and Flow's source opened (decided 2026-11-26), the Mac's first. It was 0
  for 0.1.46, on every system,
  which brought the security pass before Flow's source opened
  (`docs/design/untrusted-output.md`; decided 2026-11-24), the Mac's
  first. It was 0 for 0.1.45, on every system,
  which brought flowapp showing who signed each download and `flowapp
  verify` (decided 2026-11-20), after 3 for 0.1.44.3, Windows alone,
  which brought a put-away Flow found again, its icon in the
  notification area and a second start bringing it forward (decided
  2026-11-18), after 2 for 0.1.44.2, on every system, which brought the
  drop-down following on-top and the assistant's MCP calls in its
  journal (decided 2026-11-18), after 1 for 0.1.44.1, on every system,
  which brought the Mac the notanAmiga frame's gadgets and every system
  e-mode and on-top (decided 2026-11-17; Windows and Linux skipped
  0.1.44), after 0 for 0.1.44 (the Mac's alone). It was 0 for 0.1.43
  and 0.1.42, and 1 for 0.1.41.1, on every system, which brought the
  Mac and Windows the copy fixes Linux's 0.1.41 had (decided
  2026-11-15), after 0 for 0.1.41, 0.1.40, 0.1.39 and 0.1.38. It was 4 for 0.1.37.4, a Linux release for 0.1.37.3's card to
  be seen on, after 3 for 0.1.37.3, Linux's card asking to restart once an
  update is in place, 2 for 0.1.37.2, a release for 0.1.37.1's .deb
  to update to through apt, and 1 for 0.1.37.1, which taught it to
  (0 for 0.1.37, 0.1.36, 0.1.35, 0.1.34, 0.1.33 and 0.1.32; it was 3 for 0.1.31.3, after 0.1.31.1 on
  every OS and 0.1.31.2, which Windows alone released). The updaters still compare the marketing
  version and the build number (`version::FULL`, `0.1.25.N`). Every OS's
  scripts name the release, its tag and its files by it (`v0.1.25.1`,
  `v0.1.31.1-macos`, `Flow-0.1.31.1-macos.dmg`, the bundle's
  `CFBundleShortVersionString`, `v0.1.31.1-linux`,
  `Flow-0.1.31.1-linux-x86_64.tar.gz`), as About's release link does
  (`version::release_page`); the .deb's own version is the marketing one
  and the build (`0.1.31.<build>`), which dpkg sorts as they rise.
- Every release script (`release.sh`, both `app-release.sh`, `package.sh`,
  `release.ps1` and `app-release.ps1`, and those of the repositories whose
  apps flowapp installs) sets `CARGO_ENCODED_RUSTFLAGS` to
  `--remap-path-prefix` the home folder as `~` and the repo as `flow` (each
  other repository by its own name; parted by 0x1f), so panic
  messages and `file!()` name no build folder; nothing in a release may
  take a path with `env!("CARGO_MANIFEST_DIR")` but a debug build's.
  `strings <binary> | grep -c /Users/` is 0 for every program so built.
- **Updates only in Flow's own releases** (`docs/design/updates-for-releases.md`,
  decided 2026-11-23): Flow starts its updater only where core was
  built with `FLOW_UPDATES=1` (`version::UPDATES`, `version::updater_on`,
  from `App::create_window`; `FLOW_UPDATER=1` still forces it), so a plain
  `cargo build --release`, a fork's, never offers Flow's release in its
  place. `release.sh` sets it only when the Keychain's `Flow` key (Sparkle's
  `generate_keys --account Flow -p`, `GENERATE_KEYS` for another) is
  `UPDATE_PUBLIC_KEY`, `release.ps1` when `-KeyFile`'s is (`update_key
  public`), and `package.sh` with `--updates`, Linux having no key; a
  release without stops unless given `--no-updates` (`-NoUpdates`), and a
  test build is made without. `FLOW_SAY_UPDATES=1 flow --version` says
  `updates: on` or `off` on a second line (`version::answer`, from
  `run`'s `--version` and the flow command's), which each script checks of
  what it packages and Linux's `make-appcast.sh` of the tarball it
  publishes; asked without it, a Flow says only its version, as the
  updaters out already read it. flowapp's catalog checks and the carried
  apps (`check_apps`, `keep_carried_apps`) still run in every release
  build: they install other programs, never Flow.
- Releases and the signed update feeds (`appcast.xml` for
  Windows, `appcast-macos.xml` for macOS, `appcast-linux-<arch>.xml` for
  Linux) go to the public `arcanii/Flow-releases`, apart from the source. macOS and Linux releases
  are tagged `v<ver>-macos` and `v<ver>-linux` and created with
  `--latest=false`. Follow `windows/RELEASING.md`, `macos/RELEASING.md` and
  `linux/RELEASING.md`; publishing an appcast updates every installed copy, so
  publish only when asked.
- Updates are signed with Flow's own Ed25519 key. The public half is
  `UPDATE_PUBLIC_KEY` in `core/src/lib.rs` (macOS's `SUPublicEDKey` must
  match). The private half is kept off the repository, in the Mac's
  Keychain or a key file the Windows scripts take as `-KeyFile`, so Linux
  tarballs are signed on the Mac (`windows/scripts/sign-release.sh` signs any
  file) and `linux/scripts/make-appcast.sh --signature` takes the result.
- `macos/scripts/release.sh --adhoc --allow-dirty` runs the whole macOS release
  (Sparkle, notices, signing, DMG) without a certificate, for this Mac only.
- Linux 0.1.47 (x86-64, the tarball and the .deb, each signed in the
  feed) is published as `v0.1.47-linux` (build 2008, the first from
  Flow-Terminal), after `v0.1.46-linux` (build 766), `v0.1.45-linux`, `v0.1.44.2-linux`, `v0.1.44.1-linux`, `v0.1.43-linux`, `v0.1.42-linux`, `v0.1.41.1-linux`, `v0.1.41-linux`, `v0.1.40-linux`, `v0.1.39-linux`, `v0.1.38-linux`, `v0.1.37.4-linux`, `v0.1.37.3-linux`,
  `v0.1.37.2-linux`, `v0.1.37.1-linux` (the first feed with the .deb's
  signature), `v0.1.37-linux`, `v0.1.36-linux`, `v0.1.35-linux`, `v0.1.34-linux`, `v0.1.33-linux`, `v0.1.32-linux`, `v0.1.31.3-linux`, `v0.1.31.1-linux` (the first Linux tag with a
  patch's number), `v0.1.30-linux`, `v0.1.29-linux` (built from the commit
  Windows' 0.1.29 came to), `v0.1.27-linux`, `v0.1.25-linux` and
  `v0.1.23-linux`, made by `linux/scripts/package.sh` and signed on the
  Mac.

## Conventions

- Comments and docs are plain English that say why; each module starts with a
  `//!` summary. Tests are named as sentences
  (`touching_drops_merge_and_keep_their_water`).
- Commit messages have an imperative subject ("Add a rain-on-glass liquid
  style") and a body that explains the change, with bullets when it has several
  parts. They end with what was checked, on which OS, and what wasn't built
  there.
