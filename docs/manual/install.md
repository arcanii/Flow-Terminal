[← Flow's manual](../../README.md#documentation)

# Install

**Windows 10/11 (x64):** download `Flow-<version>-setup.exe` from the
[latest release](https://github.com/arcanii/Flow-releases/releases/latest) and run it.

- The installer isn't code-signed yet, so SmartScreen may say "Windows
  protected your PC". Choose **More info → Run anyway**.
- Installed copies update themselves through WinSparkle. Flow asks once whether
  to check automatically, and **About → Check for Updates…** checks on demand.
  Updates are signed, and Flow refuses any that aren't.
- A portable zip comes with each release too. Unzip it and run `flow.exe`.
  Portable copies don't update themselves.
- On Windows 11 on Arm, the same x64 build runs through Windows' x64 emulation.

**macOS 11 or later:** download `Flow-<version>-macos.dmg` from the newest `-macos`
release on the [releases page](https://github.com/arcanii/Flow-releases/releases),
open it and drag Flow to Applications.

- Releases are signed with Developer ID and notarized by Apple, so Flow opens
  without a warning.
- Flow.app updates itself through Sparkle. Flow asks once whether to check
  automatically, and **Flow → Check for Updates…** checks on demand. Updates
  are signed with the same key as on Windows, and Flow refuses any that aren't.
- Programs you run in Flow ask for things like the camera or your Documents
  folder in Flow's name, as they would in Terminal.

**Linux (x86-64; Wayland or X11; Ubuntu 20.04, Debian 11 or anything
since, with glibc 2.31 or newer):** download from the newest `-linux`
release on the [releases page](https://github.com/arcanii/Flow-releases/releases).
On Debian or Ubuntu, `sudo apt install ./Flow-<version>-linux-amd64.deb`
installs it for every user, and `sudo apt remove flow-terminal` takes it
away. On any Linux, unpack `Flow-<version>-linux-x86_64.tar.gz` and run its
`install.sh`. There's no Arm64 release yet: `linux/scripts/package.sh` makes
one from the source on an Arm64 Linux (see [Linux release](../development.md#linux-release)).

- `install.sh` installs into `~/.local` and needs no root: Flow goes in
  `~/.local/bin`, `flow-help`, `flow-settings` and `flowapp` in
  `~/.local/lib/flow-terminal/games` (with an older Flow's games and demos
  taken out of it, which `flowapp` installs now), and Flow
  joins your applications with its icon. `--prefix /usr/local`, as root,
  installs it for every user, and `--uninstall` takes it away again.
- Flow draws with Vulkan (Mesa's drivers, or NVIDIA's). Where there's only
  OpenGL, as in some virtual machines, it draws with that, but its glass is
  opaque there. It needs no compute shaders, which VirtualBox's OpenGL
  lacks (0.1.27 wouldn't start there). Without a GPU it draws on the CPU,
  with Mesa's lavapipe.
- Sound plays through ALSA's library (`libasound2`, which desktops have),
  which carries it on to PipeWire or PulseAudio; the .deb recommends it, and
  without it Flow and its games are silent.
- Flow installed this way updates itself. It asks once whether to check for
  updates automatically (once a day), and **About → Check for Updates…**
  checks on demand. A newer release shows in the title bar and in About,
  whose **Install Update** downloads it, checks it, puts it in place with its
  own `install.sh` and offers to restart. Updates are signed with the same key
  as on Windows and macOS, and Flow refuses any that aren't. An install for
  every user (by root) says when there's a newer release, for you to install
  as you did this one. For the .deb (from 0.1.37.1), **Install Update**
  downloads the new .deb into your downloads folder, checks it with the same
  key, and opens a tab that runs
  `sudo apt install ~/Downloads/Flow-<version>-linux-amd64.deb`: type your
  password there when sudo asks, and once apt has put the new Flow in
  place, a card asks whether to restart into it: **Restart Now** (Enter)
  opens the new Flow with your tabs, **Later** (Esc) leaves it to About's
  **Restart Flow** or the title bar's ↻ (**Show Its Tab** brings apt's
  tab back meanwhile, or runs apt again where it's closed). The same card
  asks after an update Flow installed itself, if About isn't open. Where a
  release's feed doesn't carry its .deb, **Install Update** opens the
  release's page instead, and About names the file and gives that command,
  which **Copy Command** copies; then restart Flow. A Flow from the AUR
  (`flow-terminal-bin`, from 0.1.48) says when there's a newer release,
  in the title bar and About, with no **Install Update**: your AUR helper
  installs it (`yay -Syu`, say) once the AUR has it.
