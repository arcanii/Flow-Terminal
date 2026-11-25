# Flow on macOS

`src/lib.rs` implements `flow_core::Platform` for macOS:
- `window.rs`: a transparent native title bar with the traffic lights centered
  in Flow's own 40 pt title bar, the title-bar double-click from System
  Settings, error alerts (`NSAlert`), and Flow's Dock icon for `cargo run`.
- `menu.rs`: the menu bar (Flow, Shell, Edit, View, Assistant, Window, Help)
  and the terminal's context menu. Their commands reach the core as
  `UserEvent::Command`.
- `updater.rs`: Sparkle, loaded from `Flow.app/Contents/Frameworks` at run
  time. Flow refuses an Info.plist key that isn't `flow_core::UPDATE_PUBLIC_KEY`.
- `shells.rs`: a UTF-8 `LANG` for the shells, which apps opened from the
  Finder don't get. The shells themselves are the shared Unix ones (zsh first,
  then bash, Homebrew's when installed).
- Blur behind the glass (`Window::set_blur`) and Option as Meta
  (`set_option_as_alt`).

The Mac keyboard conventions (⌥←/⌥→ by word, ⌘←/⌘→, ⌘⌫, fn+↑/↓ to scroll back)
and the macOS wording of hints are in `core`, behind `cfg!(target_os = "macos")`.
The API key is kept in the Keychain by the shared `keyring` code in `core`.

## Building and testing

On a Mac, from the repo root:

```sh
cargo test --workspace
```

`tests/shells.rs` runs zsh and bash in a real PTY and checks output, exit
codes, the reported folder, and the UTF-8 locale.

`cargo run --release` starts a bare binary. For the real app bundle:

```sh
cargo build --release && macos/scripts/bundle.sh target/release/flow target/release
open target/release/Flow.app
```

## Releases

`scripts/release.sh` builds the signed, notarized DMG with Sparkle inside, and
`scripts/make-appcast.sh` signs it with Flow's key and writes
`appcast-macos.xml`. [RELEASING.md](RELEASING.md) walks through a release.
