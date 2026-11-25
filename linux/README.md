# Flow on Linux

`src/` implements `flow_core::Platform` for Linux and the other Unix systems
apart from macOS:
- `window.rs`: the name desktops know the window by, `com.arcanii.Flow`
  (Wayland's app id, X11's WM_CLASS), which finds the menu entry and icon;
  Flow's icon for X11; the launcher's activation token, so the window takes
  focus and the busy cursor ends; the window manager's menu for a right click
  on Flow's title bar (Wayland only: winit has no way to ask on X11); and an
  error dialog through rfd (zenity), for when Flow can't start.
- `system.rs`: the user's languages (`LANGUAGE`, then `LC_ALL`, `LC_MESSAGES`
  or `LANG`), `LANG=C.UTF-8` for the shells when Flow has no locale, the
  computer for the startup logo's lines (`/etc/os-release`, `/proc/cpuinfo`,
  the online memory blocks in `/sys/devices/system/memory`, else
  `/proc/meminfo`), and the distribution's logo for the "system" startup logo:
  the PNG of the icon `os-release` names in `LOGO`, else the icon theme's
  `distributor-logo`.
- `updater.rs`: Flow's own updater, since Linux has no Sparkle. For a copy
  `install.sh` put in place, it reads the appcast for this architecture from
  `arcanii/Flow-releases`, checks a newer tarball against Flow's update key,
  and runs its `install.sh` into Flow's prefix; the core shows it in About
  (see RELEASING.md). A Flow from the .deb (in `/usr`, dpkg's) can't replace
  itself, so Install Update opens the release's page and About gives the
  `sudo apt install` command for the new .deb in the downloads folder
  (`apt_command`). Either way, once the program on disk is no longer the one
  running (`replaced`, from `/proc/self/exe`), Restart starts the new one.
- `selection.rs`: the primary selection, which a mouse selection fills and a
  middle click pastes. Its clipboard is let go as Flow exits, before the
  core's, which then hands what was copied to a clipboard manager.
- `lib.rs`: the rest. Blur behind the glass is `Window::set_blur`, which KDE
  Plasma honors and other compositors ignore. The shells are the shared Unix
  ones (zsh, bash, or else `/bin/sh`). The games' folder is
  `lib/flow-terminal/games` beside the `bin` folder Flow runs from.

The API key goes to the Secret Service (GNOME Keyring, KWallet) through the
shared `keyring` code in `core`. Sound isn't here either: core's `sound.rs`,
and the SDK's audio for Flow Rogue's music, load ALSA's `libasound.so.2` at
the first sound (`core/src/sound/alsa.rs`, as the SDK's programs do) and write to its default device, which
carries it on to PipeWire or PulseAudio. Without the library Flow is
silent; the .deb recommends it. There's no system bell, so the bell's
**Alert sound** rings Flow's own.

`tests/` drive zsh and bash in a real PTY: `shells.rs` and `prompt.rs` as on
macOS, and `theme.rs` with startup files that set the prompt before each one,
as prompt themes do, with nounset on and bash's `PROMPT_COMMAND` as a string
and as an array.

## Packaging

`packaging/com.arcanii.Flow.desktop` is the menu entry. `scripts/package.sh`
builds Flow, `flow-help`, the games and `flow-demo` against glibc 2.31 (with zig and
cargo-zigbuild, so they run on Linuxes older than the one building them) and
makes `target/linux/Flow-<version>-linux-<arch>.tar.gz`
with the menu entry, the icons from `assets/icon/`, the third-party notices
and `scripts/install.sh`, which installs into `~/.local` (or `--prefix`) and
takes it away again with `--uninstall`. The tarball is for the architecture
it was built on. Where `dpkg-deb` is, it makes the same files under `/usr`
into a .deb too (package `flow-terminal`). `scripts/make-appcast.sh` writes the feed the updater reads
for it, once the tarball is signed; RELEASING.md has the steps.

## Still to do

- **Other packages**: an apt repository, so the .deb updates through apt;
  an `.rpm`, an AppImage or a Flatpak; and an AppStream metainfo file for
  software centers.
- **The window manager's menu on X11** (`_GTK_SHOW_WINDOW_MENU`), which winit
  doesn't send.
- **Blur on GNOME**, which Mutter doesn't offer to other programs.
- **The primary selection without XWayland**: arboard reaches it through X11
  only; its `wayland-data-control` feature would reach compositors with the
  data-control protocol (KDE Plasma, wlroots), but not GNOME.
