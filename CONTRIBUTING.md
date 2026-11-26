# Contributing to Flow

Thank you for helping. This page says how to build Flow and test it, the
house style its code and words keep, where a design goes, and how to file an
issue or open a pull request. GitHub links this page from every new issue and
pull request.

[CLAUDE.md](CLAUDE.md) holds the architecture notes: how a frame is drawn,
how the sessions, the assistant and the platforms fit together, and where
each feature lives. It's written for Claude Code, but it reads as a guide
for anyone. [How it is built](docs/how-it-is-built.md) and
[Development](docs/development.md) say the same more briefly.

## Building and testing

Flow is one Cargo workspace: `core/` (`flow-core`, everything shared, which
never calls an OS API), a crate for each OS (`windows/`, `macos/`, `linux/`)
and the `flow-terminal` package at the root, whose `flow` binary picks the
OS crate it was built for.

**What each OS needs:**

- **Every OS:** stable Rust (`rustup`, or your system's packages) and a C
  compiler, for SQLite, `ring` and libwebp, which are built from source.
  Flow takes its SDK from the public
  [arcanii/Flow-SDK](https://github.com/arcanii/Flow-SDK) by a git tag, so
  the first build needs the network.
- **macOS:** Xcode's command line tools (`xcode-select --install`).
- **Windows:** Visual Studio's C++ build tools (MSVC). `build.rs` embeds the
  icon and version info into `flow.exe` only where `rc.exe` from the Windows
  SDK is there, and skips it otherwise.
- **Linux:** nothing more. winit, wgpu, the clipboard and the sound output
  load the system's Wayland, X11, xkbcommon, Vulkan, OpenGL and ALSA
  libraries as Flow runs, so no development packages are needed.

**Run and test:**

```bash
cargo run --release
```

```bash
cargo test --workspace
```

```bash
cargo clippy --workspace
```

- `cargo test --workspace` runs core's tests and the OS crate's, among them
  a test that drives the real shells in a real PTY: zsh and bash on macOS
  and Linux (`macos/tests/shells.rs`, `linux/tests/shells.rs`), cmd,
  PowerShell, Git Bash and BusyBox through ConPTY on Windows
  (`windows/tests/shells.rs`).
- One module, or one test, by name:

  ```bash
  cargo test -p flow-core fx::rain
  ```

  ```bash
  cargo test -p flow-core the_cursor_wipes_a_path_clear
  ```

- The ignored tests reach outside: each provider's real API is sent a fake
  key, the sound output is opened silently, 3D models are drawn on the GPU,
  a release's file is fetched through GitHub's API, and on Linux the
  published release is installed in a scratch folder.

  ```bash
  cargo test --workspace -- --ignored
  ```

- `cargo clippy --workspace` reports 9 warnings in older code. New code
  should add none.
- Each OS crate is `#![cfg(...)]`-gated and compiles to nothing elsewhere,
  so a build on one OS never checks another's. Say in your pull request
  which systems you built and tested on.
- Debug builds log to stderr (`RUST_LOG` works), and never start the
  updater unless `FLOW_UPDATER=1` is set. `FLOW_SHELL` picks the first
  tab's shell, and `FLOW_SKIP_PROFILE=1` keeps the bash and BusyBox hooks
  from reading your profile. CLAUDE.md's Commands section lists the rest.
- On Linux, a bare `cargo run --release` typed in a Flow tab opens a tab in
  that Flow (`flow` is the flow command there). Run it as
  `env -u FLOW_TAB_KEY cargo run --release` in a Flow tab, or from another
  terminal.

**Checking the look.** Rendering has no unit tests, and the WGSL shaders are
compiled when Flow starts, so `cargo build` doesn't catch a shader error.
After touching rendering, run Flow and look, or take snapshots:

```bash
cargo run --release -- --snapshot snapshots
```

That plays a scripted session in a hidden window and writes a picture of
each screen (`/snapshots` is in `.gitignore`). The README's and the
manual's pictures in `docs/screenshots/` are made the same way, and no user
or host name may appear in them. Snapshots run on Flow's default config and
plain shells of their own (PowerShell on Windows, else bash, started without
your startup files in a folder they make), so their prompts name neither.
[Development](docs/development.md) has more: recordings, the Mac's app
bundle, the Linux package and the icon.

## House style

- **Don't run `cargo fmt`.** There is no rustfmt config and the code isn't
  rustfmt-formatted (lines often pass 100 columns), so it would rewrite the
  whole repository. Match the surrounding style by hand.
- **Comments and docs are plain English that say why**, not what the code
  already says. Each module starts with a `//!` summary.
- **Tests are named as sentences**, saying what holds:
  `touching_drops_merge_and_keep_their_water`.
- **Every string a user reads is in the locales.** Words come from
  `core/locales/en.json` and `core/locales/ja.json` through `i18n::t` and
  `i18n::tf`, never a literal in the code, the OS crates' menus included. A
  new string needs its key in both files, with the same `{name}`
  placeholders; `cargo test -p flow-core i18n` checks that the files match
  and that every key the code names exists. What goes to the assistant's
  model (tool results, the system prompt) stays in English. Adding a
  language is in [Development](docs/development.md#languages).
- **Commit messages** have an imperative subject ("Add a rain-on-glass
  liquid style") and a body that explains the change, with bullets when it
  has several parts. They end with what was checked, on which OS, and what
  wasn't built there.
- **The manual follows the code.** A change that adds or alters a setting,
  shortcut, config key, liquid style or platform behavior updates the
  matching page in [`docs/manual/`](docs/manual/) in the same pull request.
  A new page goes in the README's [Documentation](README.md#documentation)
  index.

## Designs

A feature's design, the plan agreed before it's built, goes in
`docs/design/<feature>.md` and is kept up to date as the design changes.
For anything larger than a fix, open an issue first and agree the design
there; [`docs/design/`](docs/design/) has examples of what one holds: what
it's for, what it does, what it leaves out, and how it's tested.

## Filing an issue

Use the issue forms: **Bug report** or **Feature request**.

- Search the open issues first, in case it's there already.
- For a bug, give Flow's version (**About**, or `flow --version` in a tab),
  your system and its version, how you installed Flow, the shell, and the
  steps that show it. A release build logs to `flow.log` beside
  `config.toml` (the folders are in
  [Configuration](docs/manual/configuration.md)); attach the lines around
  the problem.
- Leave out API keys, tokens and anything else secret, and check that
  screenshots and logs show no user name, computer name or folder you'd
  rather keep to yourself.
- **Don't report a security hole in a public issue.** Report it privately
  through the repository's **Security** tab (**Report a vulnerability**), as
  [SECURITY.md](SECURITY.md) says.

## Opening a pull request

1. Fork the repository and branch from `master`.
2. Keep each pull request to one change, with its tests, its words in both
   locale files and its page of the manual.
3. Run `cargo test --workspace` and `cargo clippy --workspace` on every
   system you can, and say which in the pull request, so the change can be
   tried on the others.
4. For a change to what Flow draws, add a picture: a screenshot or a
   snapshot.
5. Fill in the pull request's template, which asks for the same.

Flow is licensed under the [MIT License](LICENSE), and a contribution
is taken under the same license: opening a pull request says you have the
right to offer it so.

Flow's releases, their signing and their update feeds are made by the
maintainer from `windows/`, `macos/` and `linux/`'s `RELEASING.md`; a pull
request never needs to touch them.

## Forks

A fork is welcome, under the MIT License, but Flow's name, its icon and its
startup logo aren't covered by that license: they're arcanii's marks, and
[Flow's brand license](docs/brand/LICENSE-brand.txt) says how they may be
used.

### Branding

- **A pristine fork keeps Flow's branding.** That's a fork that's an
  unchanged copy of this repository, or one kept only to propose changes
  back to it, its changes on branches for pull requests, never released or
  handed on as a program of its own.
- **Every other fork changes it.** A fork that's built and handed to
  anyone, published or released takes a name of its own (one that neither
  is nor contains Flow), an icon and startup logo of its own, and an
  identity of its own, and doesn't present itself as Flow, as arcanii's,
  or as an official release. It may say in plain text that it's based on
  Flow by arcanii, with a link to this repository.

Where each is in the source:

- **The name:** `APP_NAME` in `core/src/lib.rs`, which the interface's words
  name the app by; `CFBundleName` and `CFBundleDisplayName` in
  `macos/packaging/Info.plist`; `MyApp` in `windows/packaging/installer.iss`;
  `Name` in `linux/packaging/com.arcanii.Flow.desktop`; and the words in
  `core/locales/` and `docs/` that say Flow outright.
- **The icon and the startup logo:** `assets/icon/` (`flow-*.png`,
  `flow.icns`, `flow.ico`; `examples/make_icon.rs` draws Flow's), which the
  bundle, the Windows program and installer, Linux's icons and the startup
  logo all come from.
- **The identity:** `com.arcanii.Flow` as `CFBundleIdentifier` in
  `Info.plist`, `APP_ID` in `linux/src/window.rs` (and the `.desktop` file's
  name), and `APP_USER_MODEL_ID` in `windows/src/notify.rs`; `AppId` and
  `AppPublisher` in `installer.iss`; and About's links to Flow's releases
  (`RELEASES` in `core/src/version.rs`, `RELEASES_URL` in
  `macos/src/menu.rs`). So a fork's installs, notifications and windows
  never take Flow's place, nor send its users to Flow's releases.
- **Updates:** the key, feeds and catalog below.

Names inside the program that are how it works (the `flow` and `flowapp`
commands, `FLOW_*` variables, config keys) may stay.

### Updates

Only Flow's own releases update themselves: a Flow built with plain
`cargo build --release` has no updater, and the release scripts build with
updates (`FLOW_UPDATES=1`) only with the update key the build trusts at
hand ([Version and updates](docs/manual/updates.md)). So a fork's builds
never update themselves to Flow's releases. A fork that wants updates of
its own gives its builds its own key and feeds, and its `flowapp`, which
reads Flow's catalog of apps, a catalog of its own:

- **The update key:** `UPDATE_PUBLIC_KEY` in `core/src/lib.rs`, and the
  same key as `SUPublicEDKey` in `macos/packaging/Info.plist`.
  `cargo run --example update_key -- new <private-key-file>` makes a pair,
  and `public` prints the public half. With the private half where the
  release scripts look (the Mac's login Keychain, account `Flow`; the key
  file on Windows) they build with updates; on Linux, `package.sh
  --updates`.
- **The feeds:** `APPCAST_URL` in `windows/src/updater.rs`, `SUFeedURL` in
  `macos/packaging/Info.plist`, and the feed's URL in `linux/src/updater.rs`.
- **The catalog:** `CATALOG_URL` in `core/src/apps.rs` (`FLOWAPP_CATALOG`
  names another while you try one).

A build of your own starts no updater unless `FLOW_UPDATER=1` is set, so
none of this matters for building and testing.
