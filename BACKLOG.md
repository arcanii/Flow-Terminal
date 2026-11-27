# Flow backlog

What's planned for Flow itself, roughly in the order asked for. Each item
notes what's known about doing it. The games keep their own, in their
repository, arcanii/Flow-Games, which is private.

## 3D models: what's left

Flow shows 3D models through the Ratty graphics protocol (the docs, [3D
models](docs/manual/images.md#3d-models)): OBJ and GLB, still or animated, a glTF's own
animations, skins and morph targets played, its materials (metal, roughness,
normal, occlusion and emissive maps, a second set of texture coordinates,
texture transforms, emissive strength, cut-outs and see-through parts,
unlit, specular, ior, clearcoat, sheen, transmission and volume as
see-through films, and the old specular-glossiness material), vertex
colors (`COLOR_0`, however they're packed), GPU instancing
(`EXT_mesh_gpu_instancing`, each instance a copy of its node's mesh), the
file's lights (settled over the clips that move them) and first camera (a
perspective one looked through, a turned model kept on its picture), files
packed with meshopt (both vertex codecs, every filter), Draco, quantized
attributes, WebP and Basis (KTX2) textures, clicks answered on request,
each drawn again at a new font size, and pictures cut rather than split
when the grid narrows. Still to do:

- **More of glTF:** anisotropy and iridescence are drawn and dispersion
  read for 0.1.48 (`docs/design/gltf-materials.md`), and `COLOR_1` and on,
  and instances' own attributes (`_ID`, say), stay passed over, as glTF
  gives them no meaning (decided 2026-11-27). Instancing draws solid and
  cut-out parts once per instance on the GPU for 0.1.48, counted once
  toward the 2 million triangles a model may have, up to 32 million drawn
  (`docs/design/gpu-instancing.md`), so a forest of big trees fits;
  skinned and see-through parts are still copied on the CPU, within the 2
  million. Whole files as gltfpack and gltf-transform write them
  (meshopt, quantized, Draco, WebP, instances, the newer materials) are
  read and drawn by the tests for 0.1.48 (`core/src/model/testdata`,
  `docs/design/gltf-tool-files.md`), which found a mesh without normals
  drawn smooth where glTF has it flat, now flat; KTX2 textures as gltfpack
  writes them (`-tc`, which npm's gltfpack hasn't) and Blender's own
  exports aren't among them. 0.1.36's (Draco, KTX2, meshopt's
  version 1, spec-gloss, a camera turned) were drawn by the GPU tests on the
  Mac and Linux, and vertex colors and instancing by them on the Mac,
  Linux and Windows, but none has been placed in a live tab on any
  system.
- **Lights a clip carries nearer** are settled so the brightest moment
  lights the model as Flow's own light does, which leaves the rest of the
  clip (rest included) the dimmer; a clip whose light passes very near the
  middle may leave it too dim at rest.
- **Cutting see-through triangles each frame:** an animated model past
  2,048 see-through triangles moves the cuts made at rest rather than
  cutting each frame (large).
- **The glaze and its ior:** `render/models.rs` scales a glaze's reflection
  by its ior's head-on reflection linearly while its colors are display
  values, so a very high ior (a spec-gloss file read with glTF-Transform's
  1000, which Flow sidesteps) reflects far too brightly.
- **Windows:** a model's cells are Flow's own, so ConPTY's repaint after a
  resize (or on leaving the alternate screen) goes over them, as it did the
  startup logo's before `Banner`; a placement down the pipe already waits
  for ConPTY to paint what came before it. `show_model` writing down the
  pipe for images, and clicks answered through ConPTY's input, are untried.
- **Rewrapping on Windows:** pictures are cut and filled out again around
  Flow's own resize, but ConPTY repaints from its own rewrapped buffer, so a
  picture that came through ConPTY can still split there.

## The SDK and the demos

They live in their own public repository, arcanii/Flow-SDK, with a
backlog of their own; Flow takes them by tag (`flow-sdk` and `flow-demos`
in the root Cargo.toml's workspace dependencies), and `demos/` here is only
`flow-demo` as Flow's releases carry it.

## Games to come

- **Flow Invaders** (`flow-invaders`) plays: invaders come down in
  waves, fought from a truck, with a shop between the waves. `flowapp
  install flow-invaders` brings it (0.1.26 in the catalog, for the Mac,
  Windows and Linux); what's left is in its own backlog.

## Flow apps on their own, and `flowapp`

`flowapp` is in (the docs, [Apps: flowapp](docs/manual/flowapp.md)):
`update`, `upgrade`, `install`, `remove`, `list`, `outdated`, `info` and
`search`, from a catalog in Flow-releases (`apps.json`) of signed
downloads, a game an older Flow carried counted as installed, and the
apps it installs in `apps/` beside config.toml, on the tabs' PATH. From
0.1.34 Flow carries no game or demo: `flowapp install` brings them.
In a terminal it prints tables (`apps::table`) and each download on a
line redrawn as it comes, and in Flow the liquid splashes as an app lands;
piped, plain columns, and with `--json` JSON. The first apps: Flow Rogue,
Flow Mines, Flow Invaders and `flow-sdk` (the SDK's C library, header and
Python package, with `flow-demo`); since then Flow-Utils and Flow
Phonebook, each from a repository of its own, and the package of themes
`flow-core-themes`, all for the Mac, Windows and x86-64 Linux. What's
left:

- **Arm64 Linux downloads** (`linux-aarch64`), once there's an Arm64
  Linux computer to build Flow on: the catalog has every app for the
  Mac, Windows and x86-64 Linux, which Flow 0.1.32 and later carry
  `flowapp` on.
- **Its words read by someone who reads Japanese**, its help page's
  (`flow-help apps`, in 0.1.32) among them.
- **The games' downloads made from their own repository:** from 0.1.35
  Flow-Games makes them itself, with a checkout of Flow for its tools,
  and Flow dropped `carried/` and the Flow-Games tags, so it builds
  without that repository. Each system's script has run, nothing
  published. Left: the first published download made by them (Flow
  Invaders 0.1.26 went out through Flow's old scripts and `carried/`).
- **The move, its last part:** 0.1.33 carried the games while `flowapp`
  installed them, and from 0.1.34 Flow carries none (the Mac's update
  replaces the bundle whole; the Windows setup's `[InstallDelete]`,
  `install.sh` and the .deb take an older Flow's copies away), and the
  first start after an update from an older Flow installs the two it
  carried with flowapp, once (`apps::keep_carried`), so an update takes no
  game away. Seen on Linux: a .deb updated to 0.1.34 kept the
  games. Left: the same seen on the Mac and on Windows.
- **Ahead of the system's commands:** `flowapp first` and `last` are in
  for 0.1.34 (a folder first on the tabs' PATH, which open bash and zsh
  tabs follow at their next prompt). Tried on Windows through ConPTY by
  `windows/tests/first.rs` (PowerShell, cmd, BusyBox, Git Bash; on
  Windows for 0.1.35), and Windows' hard links with Flow-Utils' download put
  first. BusyBox has no prompt hook to forget remembered commands, so its
  open tabs follow only for commands not yet run, or once `flowapp` runs
  in them.
- **Finer control over what goes first** (asked for after 0.1.34, with
  Flow-Utils in mind): in for 0.1.35. Choices are kept in
  `apps/first.json` (which shells, `--shell`/`--except`; the PATH or
  `--prompt` alone; what each takes away, `--take
  alias,builtin,function,applet,none`; `--here` for one tab;
  `<app>/<program>` where two apps have one of a name), each pane has a
  folder of its own built from them for its shell, and `flowapp first`
  alone lists by shell what's first, what it took away and what still
  comes ahead (`--json` too). Tried by tests in a pty on each system: zsh
  and bash (5.3 and 3.2) on the Mac, zsh 5.9 and bash 5.3 on Linux, and
  PowerShell, cmd, BusyBox and Git Bash through ConPTY on Windows
  (`windows/tests/first.rs`). Left: in live tabs, Windows' and Linux's
  above all.
- **Apps in Settings** are in 0.1.35, as decided: Settings' Apps lists
  them with Install, Upgrade and Remove; apps don't upgrade when Flow
  does, but the first start of a newer Flow reminds you; Check apps at
  start and Check apps (daily, weekly) read the catalog by themselves and
  toast what's newer. A test Flow.app with them was tried on the Mac
  (2026-11-09). Left: Windows and Linux in a live window, installing and
  removing for real; whether the new-tab picker lists apps too.
- **Shells from apps** (asked for 2026-11-15, with dash in mind): flowapp
  can carry a shell's program as it does any app's, on every tab's PATH,
  but a tab opens only the shells `shell::unix_profiles` (zsh, bash, `sh`
  where there's neither) and `windows/src/shells.rs` find, each with its
  hook. A shell typed at another's prompt runs with no hook, so its
  prompts go unmarked and the assistant's commands, the glass's timing
  and the tab's folder lose track of it until it exits. Wanted: an app's
  catalog entry naming a program as a shell, and of which kind, which the
  new-tab picker and `shell.default` then offer, with a hook for each
  kind. dash first, tried against the Mac's own `/bin/dash` with nothing
  downloaded: it expands `$PWD` and `$(…)` in `PS1`, so its prompts can
  be marked as ash's are, and like ash it has no hook before each prompt,
  so `flowapp first` reaches its open tabs as it does BusyBox's. Against
  it as a shell to type in: the Mac's (linking libSystem alone) and
  Debian's are built without libedit, so no arrow keys, history or
  completion; on Windows it needs Cygwin or MSYS2, where the bundled
  BusyBox ash is of its family already. Its license is BSD-3-Clause but
  for `src/mksignames.c` (GPL-2+), whose output is compiled in, as
  Debian's copyright file says: to settle before Flow ships one.
- **uutils as an app** (asked for with dash): coreutils in Rust (MIT),
  about a hundred commands, native on Windows too; an app of commands, as
  Flow-Utils is, not a shell. flowapp copes with the names they share
  (`ls`, `cat`, `du`, `df`, `wc`, `head`, `tail`, `env`, `date`…): the
  app installed last holds `apps/bin`'s copy, `flowapp first uutils/ls`
  picks one. Its commands are one program by default, which flowapp
  would copy into `bin/` once for each name: to be built as programs of
  their own, or `bin/` to link where it now copies.
- **Who signed each download, with which key and when** (asked for
  2026-11-20, built the same day on the Mac;
  `docs/design/flowapp-signatures.md`).
  - **What's in:**
    - A Signed column in `list` and `search`: `Flow f186 6b49 ·
      2026-11-18`, Flow's key named by Flow's own table of keys, never
      by the catalog.
    - `info`'s Signed by, Signed on and Checked rows, and on the Mac
      Code signed, from `codesign`.
    - Under `info`'s table, the lines that check the signature with
      OpenSSL alone.
    - `flowapp verify`, which fetches each app's download again, checks
      it and compares every installed file with it.
    - `installed.json` keeping the download, the key and the day it was
      checked.
    - `app_catalog` writing the catalog's `signed` date and a `<file>.sig`
      beside the tarball, and the `app-release` scripts naming the
      `.sig` in the upload they print: Flow's own, and those of each
      repository whose apps flowapp installs.
  - **Tried on the Mac** (2026-11-20):
    - `list`, `search` and `info` on an apps folder in everyday use.
    - `flowapp verify flow-say` there: 163 files as downloaded.
    - The same on a scratch copy with a file changed, one removed and
      one added: the first two failed it, the third was only said.
    - An install in a scratch HOME keeping its signature.
    - `info`'s OpenSSL lines run as printed, with Homebrew's OpenSSL
      4.1.0.
  - **Left:**
    - **A Flow release with it.** Flow-releases has everything it needs
      (published 2026-11-20, `843dc79`): `flow-update.pem`, the README
      section "Checking a download yourself", a date on every download
      in `apps.json`, and a `.sig` beside each.
    - **Live tries** on Windows and Linux. Windows and Linux have no
      Code signed row: Authenticode waits for Flow's Windows downloads to
      be code-signed.
    - **The same in Settings' Apps** is built for 0.1.46 (below; decided
      2026-11-23): the hint of the app
      chosen ends saying who signed it, and a row adds "not signed" or
      "unknown key" where Flow didn't sign it. Tested in `shelf`'s unit
      tests and seen in a snapshot (`settings-apps.png`); not yet in a
      live window.
    - **Still untried:** OpenSSL 3 on Linux and in Git Bash, and the
      same check on Flow's own downloads (Sparkle's `edSignature`).
    - **The date is the catalog's word,** as decided: nothing signs it.
- **flowapp updating itself, without a Flow release** (asked for
  2026-11-20, perhaps through a helper program; built the same day on the
  Mac; `docs/design/flowapp-updates.md`). No helper: flowapp
  is an app of its own catalog, whose installed copy runs in Flow's tabs
  ahead of Flow's, `apps/bin` coming first on their PATH.
  - **What's in:**
    - flowapp's own version, from 0.2.0 (`flowapp` under
      `[workspace.metadata.flow]`).
    - `version_in`, reading `flowapp 0.2.0 (build 750)` as 0.2.0.
    - Flow's own folder and the Flow it runs in, found from the copy in
      the apps folder.
    - The newer of the two copies running.
    - The `app-release` scripts' `flowapp`, on all three systems.
  - **Out:** the Mac's 0.2.0 (2026-11-20, Flow-releases `866f7c0`), and
    Windows' 0.2.1 (`flowapp-v0.2.1`, Flow-releases `e380d91`, with
    Windows' 0.1.45), and Linux's 0.2.1 (the same release, Flow-releases
    `5228bb2`, with Linux's 0.1.45), and the Mac's 0.2.1 (the same
    release, build 745, Flow-releases `abc6433`, 2026-11-21). Tried live
    on Windows: the hand-over both ways, waiting for the other copy and
    giving back its exit code, in pwsh, cmd, Git Bash and BusyBox. And on
    Linux: the hand-over both ways with `exec`, in bash, zsh and ash, and
    the upgrade from 0.1.44.2's flowapp against the live catalog. And on
    the Mac: the hand-over both ways with `exec`, and the upgrade from
    0.1.45's flowapp against the live catalog, in zsh and both bashes.
  - **Left:**
    - **Linux's unused 0.2.0 tarball** is still on `flowapp-v0.2.0`,
      with no catalog entry or `.sig`; taking it off is still to decide.
    - **bash's `checkhash`** has its pty test on Linux and on the Mac
      (zsh, Homebrew's bash and the Mac's 3.2), and Windows' Git Bash was
      tried live. It reaches the Mac with
      0.1.46, as the Mac's 0.1.45 came before it.
    - **The flowapps already out** (0.1.45 and before) show the copy Flow
      carries as `737)` until flowapp is upgraded, and don't hand over.
    - **Settings' Apps** is Flow's own code, so it changes only with
      Flow.
    - **Flow's other programs** (flow-help, flow-settings, flow-mcp, the
      flow command) could go the same way.

## Sound: what's left

Flow has sound (the docs, [Sound](docs/manual/sound.md)): the bell as the
system's alert sound or Flow's, and, once turned on, cues for its own
events, sounds for the effects programs ask for, programs' DECPS notes and
named sounds, the liquid's own and the old machines' own, all made in code,
and a bell that's seen. Still to do:

- **Linux:** a system bell (the desktop's sound theme's), where Flow's own
  rings for now.
- **Windows:** not yet heard: cpal's WASAPI output opens and plays, and
  ConPTY passes DECPS on, but neither they nor `MessageBeep` have been
  listened to.
- **The machines' long sounds less alike each time** are in for 0.1.34:
  the tube's hum and the PC's fan drift with the mixer's clock. Left:
  listening to them on each system (`examples/sounds` writes a minute of
  each).
- **The PC's fan steady, not loud and soft by turns** is in for 0.1.39
  (heard on Windows, 2026-11-13: it swelled and sank at a beat). Its air
  was each voice's own noise faded in a straight line where one was laid
  on the last every 2.5 s, a dip of 3 dB in the air there; now it fades
  at equal power, and half of it is `Cue::FanAir`, 4.2 s laid every
  3.7 s, so the two layers' crossings never come together but every
  92.5 s, while the blades' tone keeps its straight fade, in step. Left:
  hearing it in a live window on each system.

## Voices: a `say` app

flow-say is an app flowapp installs (`flowapp install flow-say`), made in
a repository of its own, arcanii/Flow-Say: a `say` program that speaks
aloud, with a skill for the assistant, off until turned on. It's in the
catalog for the Mac (Apple silicon, macOS 14 or later), Windows (x64)
and Linux (x86-64). Flow's own part:

- **Introducing itself when it's installed:** built for 0.1.46 (decided
  2026-11-23; 0.1.46, below; `docs/design/app-hello.md`): an app's
  download may name one of its programs in a `flowapp.json`, which
  flowapp runs after a fresh install in a terminal, and Settings in a tab
  of its own, so flow-say greets you aloud once it's in.
- **An app's note,** which flowapp would print after installing it: not
  built yet.

## Hearing: speech to text

Asked for on 2026-11-27: Flow hears the user, speech to text with
Whisper, so a request can be said rather than typed. Nothing is started;
what's known:

- **Where the words go:** into the assistant's input as typed text,
  never sent by themselves, so they can be read and changed first. A
  microphone button beside Attach, and a key held while talking. Later,
  perhaps, typed into a pane as well.
- **Whisper here, or OpenAI's:** run on this computer (whisper.cpp,
  through a crate such as `whisper-rs`; Metal on the Mac, the CPU or
  Vulkan elsewhere), with a model of about 75 MB (tiny) to about 3 GB
  (large), so nothing said leaves the computer; or OpenAI's transcription
  API with the OpenAI key, which sends the audio. On this computer first,
  as the journal and the history keep everything here.
- **An app of its own?** As flow-say speaks (Voices, above), hearing could
  be an app flowapp installs, carrying whisper.cpp and fetching its model
  the first time, a download the user agrees to, so Flow stays small and
  no release carries a model.
- **The microphone:** cpal records on the Mac and Windows; on Linux,
  ALSA, loaded as Flow runs, as its sound output is. The Mac asks the
  user once (`NSMicrophoneUsageDescription` in Info.plist, and the
  hardened runtime's audio-input entitlement); Windows' privacy settings
  may refuse it.
- **Languages:** Whisper finds the language itself, Japanese among them;
  a setting could hold it to `ui.language`.
- **Snapshots** show it made up, hearing nothing.

## Linux: what's left

Flow builds, runs and packs into a tarball and a .deb on Linux (the docs, [Install](docs/manual/install.md)
and [Linux release](docs/development.md#linux-release)). Still to do:

- **Seeing the live window** on GNOME, KDE Plasma and a wlroots compositor:
  the glass over the desktop, moving, resizing and maximizing by Flow's own
  title bar, its right-click menu, middle-click paste with other programs,
  the key in GNOME Keyring and KWallet, and typing through IBus and Fcitx.
- **Publishing more of it:** every release since 0.1.23 has gone out for
  x86-64 Linux, the latest `v0.1.47-linux` (the tarball and the .deb, with
  `appcast-linux-x86_64.xml` for Flow's own updater). From 0.1.37.1 a
  .deb's Flow downloads the next .deb, checks it with Flow's key and runs
  `sudo apt install` on it in a tab (the manual's Install).
  - **The AUR** (2026-11-25, the user's "AUR package first", ahead of a
    Flatpak): `flow-terminal-bin`, repackaging each release's tarball
    (`docs/design/aur.md`), built, linted and installed in a scratch Arch;
    `linux/scripts/aur.sh` writes it for each release, and from 0.1.48
    Flow leaves pacman's copy to the AUR helper. Left:
    - the user's AUR account, and publishing the package;
    - seeing Flow's window on Arch (Hyprland, Omarchy's).
  - Still to come: an Arm64 tarball, an apt repository, so the .deb
    updates with the rest of the system's packages, and other packages
    (an `.rpm`, an AppImage or a Flatpak, with AppStream metadata), which
    would update through their own.
- **The window manager's menu on X11**, which winit doesn't ask for
  (`_GTK_SHOW_WINDOW_MENU`); on Windows too (`Window::show_window_menu`),
  untried there.
- **The primary selection without XWayland**, through arboard's
  `wayland-data-control` feature (KDE Plasma and wlroots; GNOME has no such
  protocol).

## The Mac App Store

Asked on 2026-11-13: what it would take to put Flow on the Mac App
Store, as a fourth release beside the Mac's DMG, Windows' and Linux's,
updated by the App Store rather than Sparkle. Nothing is started; what's
known:

- **The sandbox is the hard part, not Sparkle.** Every app in the Mac App
  Store runs in the App Sandbox (`com.apple.security.app-sandbox`; App
  Review Guidelines 2.4.5 and 2.5.2 as they read today), and every program
  it starts inherits it: the shell, and everything run in it. A sandboxed
  app's `HOME` is its container (`~/Library/Containers/<id>/Data`), so the
  shell would start there without the user's `~/.zshrc`, and couldn't read
  or write the user's own folders, or run what lies outside the system's
  own (Homebrew's `/opt/homebrew`), unless the user grants a folder through
  the system's open panel (the user-selected read-write entitlement, kept
  as a security-scoped bookmark). Apple's temporary exceptions for paths
  seldom pass review. That's why most well-known terminals (iTerm2,
  Ghostty, kitty, WezTerm, Warp) are distributed outside the store.
- **So the first step is a try, not a build:** a sandboxed Flow.app signed
  for development, and what the pty, zsh, `HOME`, `~/.zshrc`, `ls
  ~/Documents`, `/opt/homebrew` and `git` do in it, and whether a folder
  the user grants stays granted to the shell and its children across
  `exec`. Then decide whether a store Flow, a terminal for its own
  container and the folders granted it, is worth a fourth release.
- **flowapp couldn't install programs there.** The guidelines don't let a
  Mac app download or install code that adds to what it does, so the
  store's Flow would install no games, Flow-Utils, SDK or Phonebook: at
  most packages of themes, which are data. Settings → Apps and flowapp's
  help page would say so.
- **Private functions.** winit's macOS `set_blur` calls CoreGraphics'
  private `CGSSetWindowBackgroundBlurRadius` and `CGSMainConnectionID`,
  which Flow's binary imports (`nm -u` on 0.1.38's), and App Review scans
  for private symbols. The store's build needs the blur through an
  `NSVisualEffectView` behind the Metal layer instead, and a winit without
  those calls (patched in `[patch.crates-io]`, or changed upstream),
  checked with `nm -u` before each upload.
- **No Sparkle, and no updater in sight.** The App Store updates it: built
  without `--sparkle` (`bundle.sh` already leaves it out), `updater.rs`
  not started, About's Check for Updates, the title bar's pill and the
  question about checking daily gone, and About saying the App Store keeps
  Flow up to date; a cargo feature (`app-store`) would choose it.
- **Its version:** the store takes a `CFBundleShortVersionString` of three
  numbers at most, so a patch release (0.1.37.4) can't go there as such;
  the store's Flow would skip patches or take the next version.
  `CFBundleVersion`, the commit count, rises with each upload as it must.
- **An identity of its own:** a bundle id of its own (such as
  `com.arcanii.Flow.appstore`), since one id from two places would muddle
  the updaters, Launch Services and Notification Center. Its config,
  history, tabs and the API keys in the Keychain then live in its
  container, apart from the DMG Flow's, so moving from one to the other
  starts afresh (a preset file carries the look over).
- **Entitlements to ask for and explain to review:** the network (the
  assistant, the catalog for themes), and for programs run in it, the
  camera, the microphone, Apple Events, contacts, calendars, location and
  photos that `Flow.entitlements` grants today; the helpers (`flow-help`,
  `flow-settings`, `flowapp`) sandboxed too, with
  `com.apple.security.inherit`.
- **The rest of Flow to try in the sandbox:** the Workstation's probe of
  the memory banks and buses (IOKit's registry), the machine's id, the
  Carbon hotkey, Notification Center, the drop-down over full-screen apps
  (the accessory activation policy), reading the tab's programs' usage
  (libproc), the save panels (recording, Export…), and a `file:` link's
  `open -R` (NSWorkspace instead).
- **Signing and sending it:** an Apple Distribution and a Mac Installer
  Distribution certificate, a provisioning profile for its id embedded in
  the bundle, a `.pkg` from `productbuild --sign`, uploaded with
  `xcrun altool --upload-app` or Transporter. App Store Connect's record
  wants a description, screenshots (from `--snapshot`), the privacy label
  (the assistant sends what's on screen to the provider the user picks,
  with their own key; Flow collects nothing), a privacy policy page, an age
  rating, export compliance (standard TLS only) and notes for the reviewer
  on how a terminal and its assistant work. A `release.sh --app-store`, or
  a script of its own, and a fourth part in `macos/RELEASING.md` and the
  manual's Install.
- **The assistant** uses the user's own key with the provider they pick,
  which the store allows, and Flow sells nothing inside, so there's no
  in-app purchase to set up. Whether the store's Flow is free or paid is
  still to decide.

## Opening Flow's source

Flow's source opens as arcanii/Flow-Terminal. Still to do:

- **For contributors:** done. CONTRIBUTING.md and `.github`'s issue
  forms and pull request template (2026-11-23); SECURITY.md, with private
  vulnerability reporting on since the repository opened (2026-11-25);
  CODE_OF_CONDUCT.md (2026-11-27), its reports through the same private
  report, titled "Conduct:" (the user's choice). No CLA and no DCO
  (decided 2026-11-27): a contribution comes under the MIT License, as
  CONTRIBUTING says, which lets a later Flow carry it under other terms
  as long as its MIT notices stay.
- **CI:** GitHub Actions building and testing on macOS, Windows and Linux
  (`cargo test --workspace`, clippy), with no signing secrets in it;
  releases stay with the release scripts, run by hand.
- **A review before it opens:** done on 2026-11-24 on the Mac, as the
  security pass in `docs/design/untrusted-output.md`. It reviewed what
  reads programs' output, the pipes and the bridge, the updaters' and
  flowapp's checks, and the assistant's approvals. Each fix was attacked
  and fixed again until its checks passed. What was known was fixed, and
  what needs checking was left for later (decided 2026-11-24):
  - **On Windows: done** (2026-11-25, Windows 11, x64, rustc 1.92),
    before Windows' 0.1.46. The Windows half (the pipes' access lists and
    owner checks, flow-mcp's open at identification level, the opener
    through `ShellExecuteExW`, the PowerShell and cmd hooks' nonce) built
    with no warnings, and `cargo test --workspace` passed, then `cargo
    test -p flow-windows` and `--test mcp_windows` elevated too, once two
    things were fixed:
    - a theme's prompt in PowerShell stopped at its first straight `"`,
      since `ExpandString` drops all after a backticked one, which
      `windows/tests/prompt_text.rs` found (the design now says so);
    - clippy 1.92 flagged two of the pass's expressions in `ai/tools.rs`,
      so `cargo clippy --workspace --tests` is at flow-core's 8 and 13.

    The shells test's click case flakes when built on a network share
    (3 of 3 on local disk, before and after the pass). The pass was
    tried live in a scratch Flow too.
  - **On Linux: done** (2026-11-25, Ubuntu 26.04, x86-64), before Linux's
    0.1.46. `cargo test --workspace` and `cargo clippy --workspace
    --tests` passed once two of the shell tests' checks were fixed for
    Linux:
    - the nonce check took the machine's id in systemd's prompt trace
      for it;
    - zsh with glibc makes letters past ASCII a `?` in a locale that
      isn't UTF-8, which the design now says.

    The pass was tried live in a scratch Flow too.
  - **The whole change, reviewed once more.** Each part was attacked on
    its own, and the merged whole passed the tests, but the two reviews
    of the whole were left for later: one for holes across the parts,
    one for everyday use that now asks or breaks.
  - **The assistant's command rules, fail closed.** They read the shell's
    words and ask where they can't tell what runs, but every review
    found lines they misread, so a reader of the shell's grammar always
    leaves some. Agreed (2026-11-24) for after the pass: a line passes
    without asking only when every byte of it is read within a small
    grammar, so a mistake in the reader costs a question, not a hole. Its
    cost, more questions in auto mode for complicated one-liners, is to
    be measured.
  - **What bash's hook costs:** bash 5.1 and later start one process
    before each prompt; measure it in Git Bash and MSYS2. On Linux (bash
    5.3.9) it's about 0.35 ms a prompt.
  - **Flow-SDK:** done in its 0.1.4, which Flow takes from 0.1.46.
    Its `ask` sends the pane's `FLOW_TAB_KEY`, Windows' `Pipe::open`
    opens at identification level and checks the pipe's owner, and its
    theme checks match flowapp's. Its Windows code was built and run on
    Windows (2026-11-25): its tests and clippy pass, with two Windows-only
    test faults fixed on its main after the tag (`be2347d`: the theme
    fixture's line endings under autocrlf, and the Python example's
    output read as UTF-8).
  - **Still to decide:**
    - whether to reserve the `flow-` prefix for Flow's own packages;
    - that `FLOW_TAB_KEY` stays the same for a pane's life;
    - whether OSC 52 may set the clipboard unasked, as most terminals
      allow.
- **Where releases live:** Flow-releases keeps the releases and the feeds,
  since every installed Flow reads its feeds and flowapp its catalog from
  there. Done (2026-11-27): its README links the source, its issues go to
  Flow-Terminal's, and its badges are files of its own, as GitHub's image
  proxy stopped fetching shields.io's.
- **The other repositories' scripts:** done (2026-11-27). Flow-Games,
  Flow-Utils, Flow-Say, Flow-Phonebook and Flow-BBS-List take a checkout
  of Flow-Terminal beside them by default (`--flow` or `FLOW_TERMINAL`
  for another), and their docs name it as public.

## Context help

Each of Flow's own screens has a page of help (the docs, [Help](docs/manual/terminal.md#help)):
F1 over the screen, `⌘?` or `Ctrl+Shift+?` anywhere (Flow's own keys over
the terminal), in English and Japanese. Pages link to each other, and Flow's
own ends with a link to every page, so every OS reaches them all from help
itself; the Mac's Help menu lists them too.

- **Links to the manual's page on each page** were left out while
  Flow's source, which holds the manual, wasn't public. A page could link
  to its page of the manual now.

## Flow's keys

Asked for on 2026-11-13: a button in About that shows Flow's keyboard
shortcuts, and every shortcut rebound to whatever key the user wants. In
0.1.39 (the docs, [Changing Flow's
keys](docs/manual/keys.md#changing-flows-keys); `keys.rs`):

- **One table** (`keys::TABLE`) every place reads: `App::on_key`, the
  Mac's menus (built again when the keys change), the palette, help's
  pages and the words that say a key (the Ask pill, the panel's hint,
  Settings' hints, the toasts). Its defaults are the keys Flow had, which
  a test holds them to, as it holds that no two share one. Going to tab 1
  to 9 and moving between panes are one entry each, their modifiers
  bound. Keys follow the layout, as the menus do (⌘? is ⇧⌘ß on a German
  keyboard). Scroll to Bottom and Scroll to Top are commands now, in the
  Mac's View menu.
- **The sheet:** About's **Keys** link opens help's Flow's keys
  (`Page::Keys`, `flow-help keys`), made from the table as the keys are
  bound, a changed one with its default, then the keys inside Flow's
  screens.
- **Rebinding:** Settings' **Keys** (closed at first): Enter or Change,
  then the key; Esc keeps it, Backspace leaves none, Reset and Reset All
  put Flow's back. A key another has is asked about (Swap, Take It, Keep
  Mine); a typed one, F1, the system's and the Hotkey are refused; one the
  shell would get is taken with a toast. The Mac's menus give their keys
  up while it waits, so ⌘T can be pressed for another command. `[keys]`
  in config.toml keeps only what's changed; not open to `flow-settings`.
- Tried by core's tests and a snapshot (`keys.png`,
  `settings-keys.png`), and in a live window on all three (the Mac's
  test Flow.app, build 579; Linux's and Windows' test Flows, build 580),
  where all worked. GNOME keeps Ctrl+Alt and the arrows for its
  workspaces, so Move Between Panes' default never reaches Flow there;
  Settings' Keys can give it another. Left:
  - a second key for a command from Settings is in for 0.1.40 (decided
    2026-11-15): **Add Key** on a command's row
    (`KeyAct::Add`, `keys::Mode::Add`, `Bindings::offer_as`), another's
    key taken but never swapped;
  - Settings' Hotkey row taking a menu's key is in for 0.1.40 too: the
    Mac's menus give their keys up while it waits, a command's key is
    taken from it with a toast, and the system's (⌘Q, Alt+F4) are
    refused. Both tried on the Mac (build 589).
    On Windows, where winit leaves Alt+F4 to the system, which asks the
    window to close, a close request with Alt held while a key is awaited
    is refused as Alt+F4 (`App::refuse_alt_f4`), Keys' wait
    too. Left: Windows and Linux in a live window, and on Linux whether
    the window manager's Alt+F4 comes the same way.
## Searching Settings

Asked for on 2026-11-13: a search field in Settings itself that finds a
setting by any words about it, full-text, not only by its name. In
0.1.39 (the manual's Settings; `settings_search.rs`):

- **What's searched:** each row's label, hint, section, options' names
  ("amber" finds Phosphor) and its key in config.toml (`fx.crt`,
  `settings::config_key`), in the language in use and in English.
- **How it matches:** every word, in any order, each the start of a word;
  Japanese by any run of its letters. A label above a hint, the palette's
  score breaking ties. Kept in memory, made as the field opens.
- **How it looks:** a field at the end of the row of tabs, reached with
  Find's key (⌘F, Ctrl+Shift+F), a click, or a letter typed where no row
  takes it. The list shows the rows found, closed sections too, each with
  its section beside it and the words typed lit; ←→ change the one chosen,
  Enter or a click on its name goes to it in its place, and Esc empties
  the field, then closes Settings. Read as a search box; help's Settings
  page and the manual say how; a snapshot (`settings-search.png`).
- Tried in a live window on all three (the Mac's build 579, Linux's and
  Windows' build 580), where it worked, Japanese through Microsoft's IME
  on Windows. Left:
  - Japanese typed through an input method on Linux (IBus with Mozc,
    Fcitx), which the Ubuntu computer it was tried on has none of;
  - rows Settings shows only in a state (the CRT's parts with the tube
    off, Theme details under Flow's look) aren't found until they show.
- **The palette finds them the same way** (for 0.1.40, decided
  2026-11-15): a setting by its hint, its choices and its key, through the
  same `settings_search::Index`, after what's found by name
  (`command_palette::by_words`). Tried on the Mac (build 589).
## Settings from the shell

`flow-settings` sees and changes Flow's settings from a tab (0.1.30), and
any program can through the SDK's `settings`, in Rust, C or Python. What's
left:

- **What each setting takes** came in 0.1.32: Flow answers `describe`,
  `flow-settings --help` lists it, and `set` keeps to it. The SDK's
  `flow_sdk::settings` sends it as it does any question; its own docs
  (in Flow-SDK) don't name `describe` yet.
- **Tried in a live window,** on each system: through ConPTY and the pipe
  on Windows above all, and with a program changing settings while
  Settings is open.

## Themes to come

Settings' Theme row has Flow's look and ten old machines' (the docs,
[Settings](docs/manual/settings.md)' Theme table; `core/src/looks.rs`). The
TI-99/4A one went out as the **Flow Instruments Home Computer** in
Windows 0.1.31.2 and on every system in 0.1.31.3: its cyan, the television tube, its title screen
with the chip's color bands and the startup logo in the middle, start
screens' hex colors and a centered `{logo}`, and its beep and honk; its
own letters (Flow Instruments) and a "press any key" that waits for one
(`{wait}`) followed on the Mac for 0.1.32, and the Flow notanAmiga CLI
came in 0.1.33.

**One phosphor for everything on the green and amber screens** is in
for 0.1.34 (the manual's Settings, **Phosphor**; `colors.phosphor`):
everything Flow draws comes out in the phosphor, its own interface
included, and your own and saved themes keep one of their own. The green
and amber screens' phosphor was tried in a live window (2026-11-10) with
nothing wrong seen, and again on Windows and Linux (2026-11-12, with the
change below).

**No white on a phosphor** (asked for 2026-11-12, with pictures of a
BBS's welcome on both screens: its white PNG logo and its menu's pale
selection bar came out white) is in for 0.1.38 (the manual's Settings,
**Phosphor**): a monochrome monitor has one phosphor, so its brightest
is that color at full and never white. It was chosen to be as close to a
real monochrome monitor as can be, over capping at the
phosphor (which kept the screens' text at full but merged everything
brighter into it, text on white and a picture's pale detail lost):
`on_phosphor` (`ui.wgsl`, and its twin `palette::on_phosphor`) now shows
each color as bright as its signal's luma, weighed as composite video and
mono VGA did (Rec. 601, where it was 709), from black to the phosphor at
full for white, where it went on toward white past the phosphor; and the
tube's last pass brings what its glow and grille add back onto the
phosphor, no brighter than it (`crt.wgsl`'s `tube`, handed
`CrtFrame::phosphor`), as the glow whitened bright text too. For 0.1.38,
a monochrome tube draws no grille (`CrtLook::grille_over`),
as a real one had none (and on Linux, seen live). So the
screens' text, sent in the phosphor's own color, shows a little below
full (about 71% on both), their bright text and white above it, and
nothing paler than the phosphor. A live look on each system, the BBS's
welcome among them, found nothing wrong. For 0.1.48
(`docs/design/phosphor-glow.md`): the floor for saturated colors
(`PHOSPHOR_FLOOR`) stays, decided 2026-11-27, so blue on black reads; and
without the tube, a last pass brings the frame back onto the phosphor as
the tube's does, so a glow clipped over bright cells is no longer paler.
Left: that pass in a live window on each system.

**Themes as packages** is in for 0.1.34 (the docs, [Apps:
flowapp](docs/manual/flowapp.md)): `flowapp` installs packages of themes,
from the catalog (`themes`) or from a file of the user's own, and Flow
wears their themes as built-in ones, in a built-in one's place where the id
is one. As decided, the built-in themes stay in Flow, and a start
screen only types text. Settings' Export…, under Your themes, writes a
saved theme as a package (`packaged::export`, `pack`), and programs write
them with Flow-SDK 0.1.3's `flow_sdk::theme` (C's `flow_theme_*`, Python's
`flow_sdk.theme`), whose packages `looks/packaged.rs`'s tests read back as
written. What's left:
- **`flow-core-themes` is published:** 0.1.0 went out on 2026-11-08
  (`flow-core-themes-v0.1.0` in Flow-releases, in the catalog's
  `themes`), written and packed by `examples/theme_package.rs`, signed on
  the Mac and entered with `app_catalog --themes --platform any`. A new
  version goes out the same way when Flow's machines change.
- **Tried in a live window:** a package installed while Flow runs showing
  in Settings and the picker at the next focus, its font and its logo,
  and Export…'s save panel; snapshots wear no packages and open no
  panels, so only unit tests and `flowapp` in a scratch home have run
  them. Exporting packs with each system's tar, tried only on the Mac.

**A window frame for the Flow notanAmiga CLI** (asked for 2026-11-10,
with a picture of the shell's window on its desktop) is in
for 0.1.36 (the manual's Themes, Theme details, **Its window frame**,
`ui.look_frame`): round each pane wearing the theme, an outline and a
title bar with the tab's name and three lines across it, in the theme's
own letters and text color, on at first, none of the machine's window
controls (as decided), the text's room inset so the grid, the
mouse and the Fit rows count it. `flow-core-themes` carries it from its
next version (`ThemeEntry::frame`), and 0.1.0's notanAmiga keeps the
built-in's. Tried in a live window on Windows and Linux for 0.1.36, a
split tab of two framed panes too. Your own and saved themes keep a frame
for 0.1.48 (`[theme] frame`, a Window frame row under Theme, kept by
Export…), and `flow-core-themes` 0.1.1, which writes the notanAmiga's
`frame`, is packed on the Mac. What's left: the Mac's live window (only
snapshots there, `theme-amiga.png`), publishing `flow-core-themes` 0.1.1
(signed on the Mac and entered with `app_catalog --themes --platform
any`), and Flow-SDK's theme entry writing `frame` in its next version
(until then `packaged.rs`' test of the SDK's package compares without
it).

- **As the machine drew it** (2026-11-17, from a picture of the real
  AmigaShell's window in WinUAE under Kickstart 1.3;
  `docs/design/window-frame.md`): the bar is white with three white
  lines, the bar's top, middle and foot (it drew four), the name in a box
  of white in the background's color, depth gadgets at its right that
  bring Flow's window in front of the others and send it behind them, a
  sizing gadget in the bottom right corner that drags the window's corner,
  and a frame without the keys ghosted. Built on Windows toward 0.1.44,
  its gadgets pixel for pixel the picture's. What's left:
  - **Sending the window back** is built on every system but Wayland,
    which lets no program do it. The Mac's (`orderBack:`, the keys to the
    app then in front) and a drag of Flow's own for its sizing gadget
    came after 0.1.44 went out without them (found 2026-11-17: the
    sizing gadget didn't resize and the depth gadgets did nothing).
    Linux's X11 came for 0.1.44.1 after its depth gadgets were found
    doing nothing (2026-11-17), the back gadget first:
    `_NET_RESTACK_WINDOW`, which mutter honors for an
    XWayland window, and the keys to the window the manager stacks in
    front. Left: under XWayland the X server lists no Wayland window, so
    the keys are mutter's to give; and on Wayland the gadget stays
    still.
  - **The Mac's and Linux's live window** (the Mac's snapshot,
    `theme-amiga.png`, is made again for 0.1.44).

## Start screens: what's left

- **The Workstation's real probe:** each Mac port shows the DART it goes
  through as its `/iommu` (0.1.32, found by the handles its devices
  name), and the buses are read again for each new tab (about 4 ms on
  the Mac). Linux's and Windows' readers are built in their releases but
  haven't been looked at in a live tab since.

## The assistant in 0.1.41

0.1.41's work (asked for 2026-11-15). Today the conversation lives
only in the assistant's thread (`Worker::messages`, `ai/mod.rs`): quitting,
Restart Flow or a reboot loses it, and nothing records what was tried. Each
provider keeps its own form of the history (OpenAI's encrypted reasoning,
Gemini's thought signatures), so what's kept across sessions has to be
plain text that any provider can read.

- **Memory that outlasts the session**, above all across a reboot, which
  Windows asks for often (drivers, updates), with a record of the
  diagnostics already run, so they aren't run again. **Built on the Mac**
  (the docs, [Memory](docs/manual/assistant.md#memory)), as below, and tried with a
  real key (2026-11-15), all working; still to do: on Windows (a real
  reboot through `RunOnce`, which `windows/src/start_once.rs` writes,
  built only on Windows) and Linux (the autostart entry).
  - A journal of each task: what was asked, its steps and the one it's on,
    and each command run with its folder, exit code and the end of its
    output, in SQLite beside `history.sqlite3`, written as each tool result
    comes back, so a crash or a reboot loses at most the step underway. A
    tool finds earlier entries by their words, as `search_history` does.
  - Facts kept for good, which the assistant adds with a tool and the user
    reads, edits and clears in Settings (this PC's GPU and its driver, that
    winget is how the user installs): a short file beside config.toml, sent
    with each conversation.
  - Going on after a restart: the panel offers to carry on with a task left
    unfinished. A conversation can't go to another provider, and an old one
    fills the context, so it carries on from the journal's summary rather
    than the transcript.
  - A reboot: Flow has to run again after it. Before a reboot the assistant
    asks for, Flow puts itself where the system starts a program once at
    the next login (Windows' `RunOnce` key, a LaunchAgent on the Mac, an
    autostart .desktop file on Linux), and `tabs.json` reopens the tabs.
  - What's kept is the user's: secrets left out as the history leaves them
    (`history::looks_secret`), a switch to keep nothing, and Clear in
    Settings.
- **Working on its own, with the user's permission, root and
  administrator included.** **Built on the Mac** (the docs, [Working on its
  own](docs/manual/assistant.md#working-on-its-own)), as below, with sudo's password
  prompt waiting and telling the user, and tried with a real key, all
  working; still to do: Windows' elevated window
  (`windows/src/elevate.rs`, built only on Windows) tried there.
  - A third mode past guide and auto: the user approves a task's plan once,
    with what it may do (admin rights, a reboot, for how long, how many
    tokens), and the assistant works through it without asking again,
    stopping where the plan didn't reach. Esc stops it at any time.
  - Admin rights stay the system's to give. On the Mac and Linux, sudo asks
    for the password in the terminal, where the user types it, the model
    never seeing it, and sudo keeps it a while (five minutes by default). On
    Windows a program that isn't elevated can't drive an elevated console
    safely (Windows Terminal keeps elevated tabs in a window of their own),
    so an admin task runs in a Flow window started as administrator, whose
    UAC prompt is the user's yes. A pipe from Flow into an elevated shell
    would let any program running as the user past UAC, so not that.
  - Rights make prompt injection matter more: what the tools read (pages,
    logs, files) is data, never instructions, and `command_risk`'s high
    commands still ask unless the plan named them.
- **Helpers, if the user approves** (**built on the Mac**, the docs,
  [Helpers](docs/manual/assistant.md#helpers); tried with a real key, all working):
  the assistant hands parts of a task to
  helpers, each with a conversation and a pane of its own in the tab
  (Flow's split panes), so the user sees what each does, and reads back
  what each found. Read-only unless the plan says otherwise, a few at once
  at most, their tokens counted toward `ai.token_cap` and shown for each,
  and a cheaper model for them as a setting.
- **Undo**: a file is kept before it's overwritten, and on Windows a
  restore point is made before a driver or a system change, which matters
  more once it has root or administrator rights.
- **Asking when a command waits for input**: a command that stops at a
  question (a password, `[Y/n]`) is noticed, its tab brought forward and
  the user told (`Platform::notify`), rather than left hanging.
- **Its own pane**, so it never types into the user's shell while they're
  in the middle of a command.
- **Waiting without spending tokens**, for a long build to finish or a
  service to come up, rather than asking the model again and again.
- **Knowing Flow itself**: it reads Flow's help pages (`help::Page`) and
  changes settings through what `flow-settings` may change
  (`open_to_programs`), for asks such as "make the text bigger" or "wear
  the green screen".
- **Summing up older turns** as a long task fills the context.

These six were agreed on 2026-11-15; all six are **built on the Mac**
(the docs, [Where it works, and what it asks](docs/manual/assistant.md#where-it-works-and-what-it-asks)),
and tried with a real key, all working.
- **The user's standing instructions and skills** (asked for
  2026-11-15), **built on the Mac** (the docs, [Your instructions and
  skills](docs/manual/assistant.md#your-instructions-and-skills)): a text file of the
  user's own sent with every conversation, and a folder of skills, read
  when a task matches one, which the assistant can add to; tried with a
  real key, all working. Skills from `flowapp` packages, as
  themes come, are 0.1.42's (below).

- **A task's steps shown in the panel** (asked for 2026-11-15), **built
  on the Mac**
  (the docs, [The task's steps](docs/manual/assistant.md#the-tasks-steps)): a strip under the
  panel's header, kept with the task. Still to do: tried live.

## 0.1.42

The plan for 0.1.42 (2026-11-16).

- **Skills from flowapp packages** (the docs, [Your instructions and
  skills](docs/manual/assistant.md#your-instructions-and-skills)), **built on the Mac**:
  an app's download may carry a `skills/` folder, and a package of nothing
  but skills (`skills.json`) installs as a package of themes does, from
  the catalog's `skills` map or a file of one's own. Settings' Skills
  section, after the Assistant's (asked for 2026-11-16, with each skill
  turned on and off on its own), lists every skill:
  the user's own, each on until switched off, and each installed
  package's, each off until turned on (with All on and All off for the
  package), listed for the assistant as `<package>:<name>` after the
  user's own; a hint is shown whole however long. `flow-skills`, Flow's own
  package (making a package of themes, writing a program with the SDK,
  `flowapp`, `flow-settings`), is made by `examples/skill_package.rs` from
  `assets/skills/`, and published as flow-skills 0.1.0 with macOS 0.1.42.
  Still to do: tried live with a key, on Windows and Linux.
- **Flow as an MCP server**, so outside apps (Claude Desktop, Claude Code,
  Cursor) can use it, with the user's permission (asked for 2026-11-16).
  As decided:
  - What they may do: read tabs (list them, read a pane's screen and
    scrollback), run commands (in a tab or a pane of their own, with the
    output and exit code), open and close tabs and panes, and change what
    `flow-settings` may (never the assistant's settings or keys).
  - Permission per app, asked once: the first time an app connects, Flow
    shows a card naming it and what it asks for, allowed or denied by
    kind, remembered, and listed in Settings to change or revoke.
    Commands still ask each time unless that app is allowed to run them.
  - A `flow-mcp` program shipped with Flow (beside `flow-settings`), which
    an app's MCP config runs as a stdio server, talking to the running
    Flow over a socket only the user can open (a named pipe on Windows);
    nothing listens on the network.
  **Built on the Mac** (the docs, [Outside apps: MCP](docs/manual/outside-apps.md);
  the design in `docs/design/mcp-server.md`), tried with Claude Code.
  Still to do: Claude Desktop; Windows' pipe built and tried on Windows
  (`windows/src/mcp.rs` isn't built on the Mac); Linux's socket tried
  there.
- **The assistant using MCP servers** (asked for 2026-11-16): Flow as an
  MCP client, the
  other way round from `flow-mcp`, so the assistant can drive an app that
  has an MCP server. **Built on the Mac** (the docs, [MCP
  servers](docs/manual/assistant.md#mcp-servers); the design in
  `docs/design/mcp-client.md`): servers from `mcp-servers.json` in the
  `mcpServers` form, stdio and Streamable HTTP, each connected only once
  switched on and each tool off until turned on, an approval per server,
  tokens in the credential store. Tried with Claude Code and Flow's own
  `flow-mcp`; Windows and Linux built it for 0.1.42. Its
  still-to-dos are 0.1.43's (below).

## 0.1.43

The plans for 0.1.43 (2026-11-16): finishing MCP, first, and the `flow`
command. That's the whole of it (decided 2026-11-16): once both are
built and tried on every system, 0.1.43 is released. A click that moves
the cursor on the prompt's line (asked for the same day) was built on
the Mac meanwhile and goes with it, tried live there; tested in a real
PTY on Linux too (zsh, bash, BusyBox's ash, prompt themes, bash's array
`PROMPT_COMMAND`) and on Windows (cmd, PowerShell 7 and 5.1, Git Bash
and BusyBox through ConPTY, which found a resize losing the prompt's
place, fixed), and tried live in the Windows test copy; it's still to
try live on Linux. The `say` app (Voices, above) comes after, as an app
of flowapp's, made in arcanii/Flow-Say.

**Finishing MCP**, its design in `docs/design/mcp-client.md`. **Built on
the Mac** (the docs, [MCP servers](docs/manual/assistant.md#mcp-servers)):

- **Signing in to remote servers** (OAuth 2.1 with PKCE, as MCP's
  authorization of 2025-11-25 has it): dynamic client registration, or a
  client ID the user typed (Client ID…); Flow publishes no client
  metadata document (as decided). Tokens in the credential store,
  refreshed as they run out; Sign in, Cancel and Sign out in Settings.
- **Resources and prompts:** the assistant lists and reads a server's
  resources (`mcp_resources`, `mcp_read_resource`, each server's off until
  turned on); a server's prompts run from the panel as
  `/<server>:<prompt>`, `/mcp` listing them.
- **A server asking the user** (elicitation): a card with a form, or a
  link to open, which Flow sends to the server alone; a call that needs a
  link opened first waits on its card (Try Again, Give Up).
- **MCP 2025-11-25, both ways:** a stream dropped before its answer picked
  up again; `notifications/cancelled` when a call is given up, calls
  running apart; `flow-mcp` at 2025-11-25.

Tried live against the MCP project's `server-everything`. Still to do:
a real OAuth server signed in to (Linear, set up in the Windows and
Linux test copies), to be tried on the released versions (decided
2026-11-16); Try Again after a link is opened; on Windows (where
`server-everything`'s test passes and Flow starts it through `npx`) the
cards, resources, a prompt and Esc in a live window; on Linux the
clicked parts and a sign-in, `server-everything`'s test and
`flow-mcp` starting Flow being tried there, with a fix for an app in a
Flow tab; Claude Desktop with `flow-mcp`. Left
out on purpose: sampling (a server spending the user's tokens), roots,
tasks, resource subscriptions, a server's `completion/complete`, the GET
stream outside a request, and a client ID metadata document (which needs
a page Flow publishes).

- **The `flow` command** ([Starting Flow from a tab](#starting-flow-from-a-tab),
  below): `flow <command>` typed in a tab opens a new tab in the running
  Flow, like the asking one (its shell, theme and folder), and types the
  command at its prompt. `--split`, `--behind`, `--close`, `--shell` and
  `--theme` change that, and `flow --window <command>`, or `flow` with
  no tab to ask, starts a new Flow with that one tab, as a guest beside
  the one running. On every system, in every shell. Planned on
  2026-11-16 in `docs/design/flow-command.md`. **Built on the Mac**
  (the docs, [A tab from a tab](docs/manual/from-the-shell.md#a-tab-from-a-tab-the-flow-command)):
  every option, odd words, refusals and guests tried live in a test
  copy. **Tried on Linux:** `bin/flow` as
  the command and as Flow, every option, odd words, refusals and guests
  in a scratch install, on Wayland and X11, and the real-PTY test in
  zsh, bash and BusyBox's ash. **On Windows:** the words given back in
  cmd, PowerShell 7
  and 5.1, Git Bash and BusyBox through ConPTY, and every option, guest
  and refusal tried live in a test copy with a scratch profile; cmd's
  `%` and Windows PowerShell 5.1's quoting fixed; the release script's
  smoke test passed, and `flow-mcp` starting Flow, which started the
  flow command beside it and held the app's pipes open, fixed (the Mac's
  `flow-mcp` likely started a guest too, not tried there). Still to do:
  Windows' Run box by hand.

## 0.1.44

A small release (decided 2026-11-17) with what master held since
0.1.43: the fix for combining marks ([Combining
marks](#combining-marks), below), built and tried live on the Mac, with
zsh's COMBINING_CHARS turned on by the hook and the recall test passing
on Linux; and the notanAmiga's frame as the machine drew it (above),
built on Windows and put in as decided. It went out on the Mac alone.
Windows and Linux came with 0.1.44.1, on every system, which also
brought Always on top and e-mode, and the Mac the frame's depth and
sizing gadgets; then 0.1.44.2, on every system, and Windows' 0.1.44.3.
Of what was to be tried before Windows' and Linux's releases: on
Windows, what ConPTY and PSReadLine measure ा as was checked (its
console counts the marks of no width too, so recalling a long line
still garbles there: [Combining marks](#combining-marks), below), and
the marks were drawn in a snapshot, with a test copy left for a look at
the live window; on Linux, zsh's hook passed the recall test, and the
marks and the frame were seen in a scratch install's live window.

## 0.1.45

Made 0.1.45 (decided 2026-11-20), for flowapp showing who signed each
download, the check with OpenSSL and `flowapp verify` (Who signed each
download, above). It went out on the Mac first (build 737), then on
Windows (build 741), carrying flowapp 0.2.1, which updates itself
between Flow's releases, and on Linux (build 741 too), carrying the same
flowapp 0.2.1 and bash's `checkhash`. Every system's 0.1.45 is out, and
master went to 0.1.46.DEV on 2026-11-23.

## 0.1.46

Two small items the backlog left, chosen on 2026-11-23: who signed each
app in Settings' Apps, finishing 0.1.45's signatures (Who signed each
download, above; `docs/design/flowapp-signatures.md`), and an app
introducing itself as it's installed (Voices, above;
`docs/design/app-hello.md`). Both built on the Mac:
- **Signatures in Settings' Apps:** in `shelf`, with its unit tests.
- **The hello:** flowapp 0.2.2 runs a download's `flowapp.json` hello
  after a fresh install in a terminal, and `flowapp hello <app>`; Settings'
  Install opens a tab for it; flow-say's downloads carry one. Tried on
  the Mac in a scratch HOME with an ad hoc flow-say 0.1.1 download signed
  with Flow's key: its hello ran once after the install, and nothing ran
  piped or installed again; and in a test copy installed from Settings,
  whose tab worked.
- **Noticed:** `flowapp remove` takes `apps/<app>/` whole, so removing an
  app takes what it fetched and kept there too, though help's apps page
  says your saves and settings stay. Whether an app's own files should
  stay (and how they'd go), to decide.
- **flow-say 0.1.2 is out for the Mac** (`flow-say-v0.1.2`,
  Flow-releases `71df973`, 2026-11-23), and so is **flowapp 0.2.2 for
  the Mac** (`flowapp-v0.2.2`, build 749, Flow-releases `ee5a6c0`).
- **flow-say 0.1.2 is out for Windows** (the same release, Flow-releases
  `80c9fdb`, 2026-11-23). Tried in a Flow tab: its hello ran.
- **flow-say 0.1.2 is out for Linux** (the same release, Flow-releases
  `b52edde`, 2026-11-23). Tried in a Flow tab: its hello ran.
- **flowapp 0.2.2 is out for Linux** (build 752, Flow-releases `b9d7ad8`)
  **and Windows** (build 752, Flow-releases `4730816`, 2026-11-23), so
  the catalog's flowapp is 0.2.2 everywhere. Windows' was tried with the
  released 0.1.45's flowapp upgrading to it, in pwsh, cmd, Git Bash and
  BusyBox tabs, and with flow-say's hello.
- **The security pass** (2026-11-24, put first before Flow's source
  opens): Flow treats what's printed in a tab as untrusted, with Flow-SDK
  0.1.4 taken in, and its design is `docs/design/untrusted-output.md`.
  What's left to check is under Opening Flow's source.
- **macOS 0.1.46 is out** (2026-11-25):
  - `v0.1.46-macos`, build 764, Apple silicon only;
  - the feed and the README's links in Flow-releases `2a5b8b5`.

  So is **flow-sdk 0.1.4 for the Mac** (`flow-sdk-v0.1.4`, Flow-releases
  `7571c86`), installed from the live catalog by 0.1.46's flowapp.
- **Linux 0.1.46 is out** (2026-11-25):
  - `v0.1.46-linux`, build 766, x86-64, the tarball and the .deb;
  - the feed and the README's links in Flow-releases `7532c27`.

  So is **flow-sdk 0.1.4 for Linux** (`flow-sdk-v0.1.4`, Flow-releases
  `718d1af`), installed from the live catalog by 0.1.46's flowapp.
- **Windows 0.1.46 is out** (2026-11-25), once its half of the pass was
  built and tested on Windows (Opening Flow's source, the review item):
  - `v0.1.46`, the Latest, build 768, which fixes a theme's prompt in
    PowerShell;
  - the feed in Flow-releases `7342233` (the README's Windows link goes
    to the Latest).

  So is **flow-sdk 0.1.4 for Windows** (`flow-sdk-v0.1.4`, Flow-releases
  `45e7980`), installed from the live catalog by 0.1.46's flowapp. Every
  system's 0.1.46 is out.
- **Noticed:** WinSparkle in a per-user copy of Flow ran the new setup,
  which upgraded the per-machine install in Program Files rather than the
  per-user one it ran from. Whether the setup should keep to the install
  that started it, to decide.

## 0.1.48

The plan for 0.1.48 (2026-11-27), beside what master holds for it
already: `/model` listing each provider's own models
(`docs/design/live-models.md`), and Linux leaving pacman's Flow to the
AUR helper.

- **Themes: some polish** (Themes to come, above):
  - the phosphor's floor for saturated colors kept (decided 2026-11-27),
    and the glow without the tube, clipped channel by channel, brought back
    by a last pass of the phosphor's own (`docs/design/phosphor-glow.md`);
  - your own and saved themes keeping a window frame: built
    (`[theme] frame`, `docs/design/window-frame.md`), and
    `flow-core-themes` 0.1.1 packed, to publish on the user's word;
  - a package of themes installed while Flow runs, and Export…, tried in
    a live window;
  - the notanAmiga's frame in the Mac's and Linux's live windows.
- **Efficient mode, tried for real on battery:** unplugged and in Low
  Power Mode on a Mac, and on Windows' and Linux's laptops (Efficient
  mode: what's left, below).
- **3D models: more of glTF, and Windows' checks** (3D models: what's
  left, above):
  - whole files from glTF tools (gltf-transform, gltfpack, Blender),
    meshopt and quantized ones as gltfpack writes them, placed in a live
    tab on each system: made from Khronos's samples and a forest of
    Flow's own, read and drawn by the tests on the Mac, a mesh without
    normals now flat (`docs/design/gltf-tool-files.md`); placing them in
    a live tab, and the GPU test on Windows and Linux, still to do;
  - the newer materials: anisotropy and iridescence drawn, dispersion
    read (`docs/design/gltf-materials.md`), and `COLOR_1` and on left
    passed over (decided 2026-11-27);
  - instancing drawn on the GPU rather than copied on the CPU, so a
    forest of big trees fits: built for solid and cut-out parts
    (`docs/design/gpu-instancing.md`);
  - on Windows: a model's cells kept through ConPTY's repaints, as the
    startup logo's are (`Banner`), `show_model` down the pipe for images,
    and clicks answered through ConPTY's input.

## The assistant's panel: what's left

The panel's replies have Copy and Run under each block of code, a small Copy
at the end of a line of nothing but backticks and on a command's card, Copy
reply over a reply, and a drag selects their words (the docs, [Using
it](docs/manual/keys.md)). Still to do:

- **Tried in a live window:** Run, Copy reply and dragging, and the small
  Copy on a line in backticks and on a command's card, have been checked in
  unit tests and snapshots only.

## Files dropped on the assistant

Asked for on 2026-11-16: a file dropped on the assistant for it to use,
such as a document or a picture. Not in 0.1.43's scope as first fixed;
**planned** on 2026-11-16 in `docs/design/assistant-files.md` and
**built on the Mac** the same day, on master, so it goes with 0.1.43,
and tried live there with Claude. OpenAI and Gemini with files are left
to Flow's users to try (decided 2026-11-16). On Linux it builds as
written and its tests pass; drops (under XWayland: there was no Xorg
session to try, and winit gives Wayland no drops), Attach, pasting and
sending are still to try live in a scratch copy. On Windows it builds
and its tests and the test files' reading pass; the live tries in a
test copy are still to come.

Today a file dropped anywhere on the window types its path at the
prompt of the pane in front (`WindowEvent::DroppedFile` in `app.rs`),
in double quotes where it has a space, whatever the shell, and the
assistant only reads text files, through `read_file`.

What was decided:
- **Kinds:** pictures, PDFs, text files, and Word and the like (docx,
  xlsx, pptx: OpenAI takes them as they are, and for Claude and Gemini
  Flow takes their text out).
- **Ways in:** dropped on the panel, a picture pasted, the assistant's
  own `read_file` reading them, and an Attach button.
- **Where a drop goes:** where it lands, on the panel to the assistant,
  on a pane its path typed there. winit gives a drop no place, so Flow
  reads the pointer itself (`Platform::pointer`).
- **A secret** (a sensitive path, a private key's header) waits on its
  chip until it's confirmed with a click; anything else the user gives
  goes without asking.

## Efficient mode: what's left

Efficient mode, the cursor's speed, the turning logo held still, and
Efficient on battery are in (the docs, [Settings](docs/manual/settings.md)).
Still to do:

- **On battery, tried for real:** unplugged and in Low Power Mode on a
  Mac, and on Windows' and Linux's laptops.
- **A start screen typed faster than its baud** is left for now (each
  theme's own cursor speed is in, 0.1.31).
- **A shorter word for its pill,** to give the title bar room for
  another: see [Always on top](#always-on-top).

## The liquid: what's left

- **Rain from a drizzle to a downpour** came for 0.1.32 (the docs,
  [Liquid styles](docs/manual/liquid-styles.md); `fx.rain`): drops strike in a splat and
  spray, a downpour runs in rivulets and sheets, and a hiss takes over
  from the taps. What's left of it:
  - **Without a GPU** (`Renderer::software`): a downpour's sheets and its
    thousand-odd drops weren't measured on a machine without one, where
    the drizzle should stay as cheap as it was.
  - **In a live window:** its hiss was checked in unit tests only, never
    listened to, and its motion seen in snapshots and a recording.

## New tabs

The … after the + picks a new tab's shell and theme (0.1.30), and the
window wears the theme of the tab in front; Save as theme… keeps what's
worn under a name, beside the built-in themes. Since 0.1.31 the atlas
keeps each font a tab wears, each tab keeps its own theme's grid, and
Restart and a start open the tabs again, each in its theme and folder.
What's left:

- **Tried in a live window:** switching between tabs in themes of
  different fonts and borders, timed by eye on each system, and
  reopening tabs on Windows (ConPTY sized to a tab's own grid) and Linux;
  quitting with tabs open was only read in the code, not quit live.

## What a tab uses

The card under a tab and About's Resources show each tab's programs, their
memory and CPU, and what Flow holds for it (0.1.30); since 0.1.31 the tab
shortcuts show the card for a moment, a 3D model's meshes count in its
tab's GPU memory, and Resources says how long a frame takes on the GPU.
What's left:

- **The GPU's time on Windows and Linux:** timestamp queries are asked
  for where the adapter has them and read on a Mac's GPU; Direct3D 12,
  Vulkan and OpenGL are untried, and where an adapter has none, About says
  so.
- **Windows and Linux:** Windows' Toolhelp reading works (its tests on
  Windows, and a PowerShell tab's card and About's Resources drawn
  offscreen), and Linux's `/proc` reading is core's, tested everywhere.
  Left: both in a live window, and the card's words for WSL, whose
  programs Windows can't see.

## Text size and the grid

Font size says what the window holds and each character's size, in Settings
and in the toast ⌘+ and ⌘- show, and **Fit window to** sizes the window to
40 × 25, 80 × 24 or 80 × 25 (the worn machine's own first) at the text size
in use, and **Fit text to** (0.1.32) the text size that fills the window
with one, leaving the window as it is. Still to do:

- **A grid typed in** is in 0.1.37 (the manual's Settings, Your
  sizes; `window.sizes`, `settings::parse_sizes`): as many sizes of the
  user's own as they like, typed in a row under the Fit rows (as
  decided), each a choice in Fit window to and Fit text to after the
  others; Enter notes what it left out and why. Tried live on the Mac,
  on Linux and on Windows (there before the note). Left: the note on
  Windows in a live window.
- **The room on Linux:** Flow takes the screen's size less 64 points for a
  taskbar, a guess (`Platform::screen_room`'s default). X11 says it
  (`_NET_WORKAREA`); Wayland doesn't, and moves no window, so
  `keep_on_screen` stays the Mac's and Windows' there. Windows takes its
  screen's work area for 0.1.38 (`MonitorFromWindow`, `GetMonitorInfoW`'s
  `rcWork`), and moves a window run off it back in, tried live.
- **Tried in a live window:** fitting it, on each system, in and out of a
  maximized window (sized at the resize that un-maximizes it), a tiled one
  on Wayland (whose size winit keeps, which the toast says), and kept in the
  screen's room on the Mac.

## The scrollback

Asked for on 2026-11-13, from a screenshot of a tab scrolled back 21
lines: the pill's arrow pointing at the lines below, a click on it
back to the bottom, and a scrollbar on the right to jump back through
the history, where scrolling up was the only way back. In 0.1.39
(the docs, [Scrolling back](docs/manual/terminal.md#scrolling-back);
`scrollbar.rs`):

- **The pill** reads `↓ 21 lines below` (`1 line below` for one), and a
  click on it takes its pane back to the bottom (`Hit::ScrollBottom`,
  each pane of a split tab its own), lit under the pointer. Screen
  readers read it as a button.
- **Keys:** ⌘End and ⌘Home on the Mac, as Terminal.app and iTerm2 have
  them, and Ctrl+Shift+End and Ctrl+Shift+Home elsewhere, as Windows
  Terminal has them (Shift+Home and Shift+End alone are PowerShell's,
  selecting at its prompt), go to the bottom and the top; on a program's
  alternate screen they're the program's.
- **The scrollbar** along each pane's right edge, over the grid: thin,
  widening under the pointer and while held; its thumb dragged, or the
  track clicked to jump the thumb's middle there. Settings' **Scrollbar**
  under Terminal (`shell.scrollbar`, open to `flow-settings`): **While
  scrolling** (scrolled back, the wheel turning or the pointer at the
  edge, held 0.9 s then fading), **Always** or **Never**. None on a
  program's alternate screen, and a program that takes the mouse gets no
  report over it.
- Tried by core's tests (the bar's layout, a drag giving back its
  offset, a press on the track, the hold and the fade, the pill as a
  reader's button) and a snapshot of the split tab's green pane scrolled
  back through a count (`scrolled.png`), and in a live window on all
  three (the Mac's test Flow.app, build 567; Linux's and Windows' test
  Flows, build 580), all good. Left:
  - the bar under the CRT's curve on Linux and Windows, which their
    tries didn't name;
  - **marks on the track:** the find bar's matches are in 0.1.39
    (marked in `ACCENT2` as it lights them, the chosen one
    taller, a press on one scrolling there). The commands run are in for
    0.1.40 (decided 2026-11-15; `marks.rs`): each marked at its
    prompt, done, failed in the error color or running, as VS Code marks
    its scrollbar, kept in its prompt line's first cell so it scrolls
    with it; ⌘↑ and ⌘↓ (Ctrl+Shift+↑ and ↓ elsewhere, Previous and Next
    Command) go from command to command, as Terminal.app does, the line
    lit a moment. Tried by core's tests and a snapshot
    (`command-marks.png`), and in a live window on the Mac (a test
    Flow.app, build 589). Left: Windows and Linux in a
    live window; on Windows, ConPTY's repaint after a resize takes the
    marks off the commands still on the screen;
  - the scrollbar for screen readers, as a scroll bar with its value
    that a reader can set (the pill takes them to the bottom already).

## Caps Lock

Asked for on 2026-11-15: something on the terminal that shows when Caps
Lock is on, and, as decided, how: a notice at the bottom center as the
user types with it on, an orange glow so there's no mistake. In for
0.1.40 (the docs, [Caps Lock](docs/manual/terminal.md#caps-lock);
`caps_lock.rs`):

- **The notice:** **⇪ Caps Lock is on** in the middle of the terminal's
  foot, orange, its glow steady whatever Glow is set to, brought back by
  each key typed into a pane (with no Control, Alt or ⌘ held) and gone
  2.5 s after the last one, at once when Caps Lock goes off, or as the
  window loses the keys. Over the cursor's row it sits above it. Screen
  readers hear it as an assertive alert.
- **Knowing it's on:** winit 0.30 doesn't say (its `ModifiersState` holds
  Shift, Control, Alt and Super alone). The Mac asks `NSEvent`'s
  `modifierFlags`, Windows `GetKeyState(VK_CAPITAL)`, and Linux under X11
  the Lock modifier (the mask of x11rb's `query_pointer`, on a connection
  of its own, as the hotkey has; `linux/src/caps_lock.rs`, after 0.1.39);
  on Wayland, which gives a client its locked modifiers only through
  `wl_keyboard`, which winit takes, Flow guesses from the letter typed (a
  capital without Shift, or a small one with it), and the Caps Lock key
  turns over what it knew.
- Tried by core's tests (the guess, the notice coming and going, the
  reader's alert) and a snapshot (`caps-lock.png`), and in a live window
  on the Mac (a test Flow.app, build 585), where it worked. Left:
  - Windows in a live window; Windows' answer is built only on Windows;
  - Linux in a live window: X11's answer and Wayland's guess, tried;
  - the notice over Settings' text rows and the assistant's input, which
    Caps Lock changes as much;
  - a switch in Settings, if anyone wants it off.

## Selecting text

Dragging over the terminal selects (alacritty_terminal's `Selection`), a
double click a word and a triple click a line, and on Linux the button
coming up makes it the primary selection. What's left:

- **The highlight following the drag** is in 0.1.35: it showed
  only when the button came up wherever nothing else kept frames coming
  (plain glass, Efficient mode), since a drag moved the selection's end
  without asking for a frame. Now `Emulator::drag_selection` says when
  the end reaches another cell or half of one, and `App::on_mouse_move`
  asks for a frame then, not at every pixel. A test Flow.app with it was
  tried on the Mac (2026-11-09). Left: dragged in a live window
  on Windows and Linux, and on a computer without a GPU.

## Links in the terminal

Links programs write with OSC 8 are in (the docs, [Using it](docs/manual/keys.md)):
underlined under the pointer, where they go shown at the terminal's foot,
and opened with ⌘-click or Ctrl+click, web and mail links only
(`link.rs`). What's left:

- **`file:` links** are in for 0.1.34: one to this computer shows the
  file selected in its folder (`Platform::reveal`), running nothing.
  Left: clicked in a live window on each system (Windows' and Linux's
  reveal are built since 0.1.34, untried).
- **Addresses in plain text**, which no program marked as a link, are
  in for 0.1.36 (`link::plain`): `http`, `https`, `mailto` and `file`
  addresses found under the pointer, wrapped rows and all, without the
  sentence's punctuation round them, underlined and opened the same way
  (as decided: no bare paths or `www.`). Ctrl+click on a plain
  `https://` address worked in a live window on Windows and Linux for
  0.1.36. Left: ⌘-click on the Mac live, and a mail or `file:` address
  and one wrapped over rows tried live.

## What other terminals have that Flow hasn't

Ten features found missing after 0.1.34's set, by looking through
Flow's code and README, first the one most worth doing.

- **Find in the scrollback** is in for 0.1.34 (the docs, [Find in the
  scrollback](docs/manual/terminal.md#find-in-the-scrollback); `find.rs`), and a search
  by pattern (the bar's `.*`, ⌥⌘R or Alt+R) in 0.1.35. Left: tried in a
  live window on each system.
- **Split panes** are in for 0.1.34 (the docs, [Split panes](docs/manual/terminal.md#split-panes);
  `panes.rs`), and in 0.1.35 each pane its own theme (its colors, font and
  background; the liquid, glass, tube and phosphor the window's, following
  the pane in front), the line between panes dragged, and Fit window to
  and Fit text to for the pane in front. A test Flow.app with them was
  tried on the Mac (2026-11-09). Left: a pane behind in a theme whose
  phosphor or tube differs shows the front one's; Windows (ConPTY resizes
  each pane) and Linux in a live window.
- **A notification when a long command finishes** while Flow isn't in
  front: in for 0.1.34. Past `ui.notify_after` (Settings' Notify when
  done, 30 s at first, or off), `App::notify_done` hands
  `Platform::notify` the tab's name, the command (where the shell's
  marks said what it was and it holds no secret), how it ended and how
  long it took: Notification Center on the Mac, whose click brings Flow
  forward with that tab and pane; on Windows a toast, for 0.1.38 (under
  Flow's own AppUserModelID, registered for the user in the registry
  rather than by a Start menu shortcut; where Windows has toasts off, the
  taskbar button flashing, now stopped once Flow is in front), tried
  live; a desktop notification on Linux. Left: a click followed
  on Linux (watching D-Bus for `ActionInvoked`); a toast clicked from the
  notification center long after, on Windows (the last 16 are kept for
  their clicks while Flow runs; after it quits, none is followed); and
  trying it live on Linux, where it's built since 0.1.34 (Linux's
  `gdbus` call fixed then).
- **"Ask why" on a failed command** is in for 0.1.34 (the docs, [Ask
  why](docs/manual/assistant.md#ask-why)). Left: tried in a live window with each
  provider, and on Windows and Linux.
- **Sixel and iTerm2's images** are in for 0.1.34 (the docs,
  [Images](docs/manual/images.md)): `sixel.rs` and `iterm.rs` decode them into
  `ImageStore` and its placeholder cells as kitty's are, the cursor left
  where xterm and iTerm2 leave it, DA1 saying sixel and XTSMGRAPHICS
  answered; tried by tests, in zsh and bash in a real PTY, and in a live
  window on macOS (a sixel from `printf`, `img2sixel`, `timg -pi`, and a
  PNG as iTerm2's from `printf`). Their extras are in for 0.1.36: DECSDM
  and sixelScrollsRight (with DECRQM, reset by RIS), iTerm2's multipart
  files, GIFs and WebPs in iTerm2's pictures and kitty's `f=100` (animated
  ones play, `frames.rs`), a picture wrapped for tmux (found already; now
  tested), and sixels and iTerm2's pictures that no cell shows let go
  before the 320 MB quota; tried by core's tests and in zsh and bash in a
  real PTY (a PNG in parts and an animated GIF), and in a live window on
  macOS an animated GIF (ffmpeg's test pattern, sent as iTerm2's) playing.
  For 0.1.38, kitty's own animation commands are in,
  played through the same `Frames` (`graphics/animation.rs`: `a=f` frames
  new or edited,
  over a base frame, a color or in place, `a=a` stop, loading, loop, loop
  count, current frame and gaps, `a=c` composing, answered as kitty
  answers, each frame in the quota), and an animation holds one texture,
  written with `queue.write_texture` only as the frame shown changes, so
  the GPU, About's Resources and the tab card count one frame's worth;
  tried by core's tests (the terminal and the pipe), a kitty animation
  printed by zsh and bash in a real PTY, and the texture read back on the
  Mac's GPU (an ignored test). On Linux a script sending a GIF's frames
  with kitty's own commands, as `kitten icat` does, played in a live
  window, About's Resources counting one frame's worth.
  Left:
  - **Windows**: Windows' ConPTY (build 26300) passes an OSC 1337 on and
    drops a sixel's DCS whole (`windows/tests/pictures.rs`, from cmd,
    PowerShell, Git Bash and BusyBox). ConPTY never learns an iTerm2
    picture's room and keeps its cursor on the row it began on, so from
    0.1.36 Flow has the picture rise from there over the lines above: the
    text after it goes where iTerm2 leaves it and the next line below it,
    from every shell (a GIF and a WebP seen playing so in a live window).
    Toward 0.1.40, sixels come down the pipe instead: Flow
    puts each where ConPTY's cursor is once ConPTY's output has stopped,
    in lines of its own above the cursor's line (or the prompt's first),
    so nothing ConPTY goes on drawing moves; mid-line rising as an iTerm2
    picture does, DECSDM at the top left, the alternate screen at the
    cursor. A sixel program's output redirected there shows from cmd,
    PowerShell (`[IO.File]::WriteAllBytes`), BusyBox and Git Bash (through
    `cmd //c`, as MSYS can't open a Windows pipe), by script in
    `windows/tests/pictures.rs`. Left: a live window, with `img2sixel` or
    `chafa -f sixels`; a picture taller than the rows above the cursor
    (just after `cls`) has its top in the scrollback, as an iTerm2
    picture's is; and a program writing text after a sixel has to ask DA1
    down the pipe first, or its text may come before the picture.
    Flow-SDK's `detect` doesn't read DA1's sixel.
  - Looking at them from `lsix`, gnuplot, chafa, yazi and `imgcat` (iTerm2
    3.5's, which sends big files in parts), an iTerm2 JPEG, a still WebP,
    DECSDM from a program that sets it, and a sixel through tmux with
    `allow-passthrough on`, in a live window. On Linux a moving GIF and
    WebP played through iTerm2's OSC 1337 and kitty's `f=100` for 0.1.36;
    left there, DECSDM, sixelScrollsRight and iTerm2's files in parts in a
    live window.
  - The other files iTerm2 shows (PDF, HEIC; AVIF too), which the log
    names as not shown. BMP and TIFF (its first page) are in for 0.1.38,
    through the `image` crate as JPEG is, and showed in a
    live window on Linux (24 and 32 bits a BMP, 8 and 16 a TIFF) and on
    Windows (a 32-bit BMP with alpha, a two-page 16-bit TIFF).
  - Kitty's animation commands seen in a live window from `kitten icat`
    with a GIF and a program building one frame by frame, on each system
    (on Linux so far, a script sending a GIF's frames as icat does). On
    Windows, where `kitten icat` can't reach Flow (ConPTY drops its
    APCs), a script's four rows down the pipe played in step; there a
    taller animation is one picture a row, each with its
    own clock from its `a=a`, so a program must start them in one write
    or they shear: the SDK could do that for it, or Flow could keep a
    picture's rows on one clock. Kitty's `d=f` and `d=F` (deleting
    frames) are in for 0.1.40 (decided 2026-11-15), as kitty's
    own code has them, tried by core's tests; left: a program deleting
    frames seen in a live window.
  - An animation's one texture seen playing in a live window on Windows;
    on Linux About's Resources counted one frame's worth while one
    played, and on the Mac an ignored test reads its
    writes back.
- **A command palette** is in for 0.1.34 (the docs, [The command
  palette](docs/manual/terminal.md#the-command-palette); `command_palette.rs`), and what
  was chosen lately kept between runs in 0.1.35 (`palette.json`). Left:
  tried in a live window on each system.
- **A drop-down terminal on a global hotkey:** in for 0.1.34, as Flow's
  own window: `window.hotkey` (Settings' Hotkey, none at first) brings it
  forward and puts it away, and with `window.drop_down` it drops over the
  top of the screen, in front of every other window (the Mac's
  `RegisterEventHotKey`, Windows' `RegisterHotKey`, X11's `XGrabKey`;
  Wayland has none). Finished for 0.1.35: it slides down and back up
  (`window.drop_down_slide`), is put away when Flow loses the keys
  (`window.drop_down_hides`), takes a third to all of the screen
  (`window.drop_down_height`), takes a key picked by pressing it (Press a
  key…), and on the Mac drops over a full-screen app as an accessory app
  (`window.drop_down_over_full_screen`), out in 0.1.35 on all three. Tried
  live on Linux on X11 (XWayland under GNOME, which shows it in place
  rather than sliding), and a test Flow.app with it was tried on the Mac
  (2026-11-09). Left: Windows in a live window, and on Linux a key
  pressed in Settings, an Xorg session and other window managers.
- **Recording the terminal** is in 0.1.37 (the docs,
  [Recording the terminal](docs/manual/terminal.md#recording-the-terminal); `record.rs`,
  `render/capture.rs`), as decided: an animated WebP, the
  terminal area only (the window below the title bar, left of the panel),
  ⇧⌘R, Ctrl+Shift+Alt+R on Windows or Ctrl+Shift+Alt+V on Linux (GNOME
  keeps Ctrl+Shift+Alt+R), the Mac's menu and the palette, a clock in the
  title bar, the system's save panel at stop, and a word as it starts when
  a prompt shows the user's or the computer's name. Tried by core's tests
  (a WebP written and read back with its frames' times) and a snapshot
  recording through the same capture and encoder, drawn offscreen. Left:
  - on Linux, an X11 session and desktops other than GNOME (each system
    has been tried in a live window: Metal, Direct3D 12 with libwebp built
    by MSVC, and Vulkan with libwebp built by zigbuild for glibc 2.31);
  - an OpenGL surface that can't be copied from, where Flow now says
    recording isn't available: drawing to a texture and showing that would
    record there too;
  - a GIF as well, for the places that won't play a WebP;
  - Settings' Quality and Size (in): each quality's look on real
    recordings, and how many frames Every pixel keeps on each system;
  - keeping up where every pixel changes, in for 0.1.38: on Linux at
    Balanced, a window of 1541 × 886, or a theme's tube, kept about 12
    frames a second, the rest dropped. Now
    libwebp's whole-frame limits are close together (kmin one short of
    kmax: from half way, half the frames were encoded twice, as a whole
    frame and as their changes), the encoder hurries at libwebp's fastest
    method while it falls behind (`record::Pace`), and frames are taken
    only as it's ready (`Recorder::ready`), so those kept are evenly
    spaced. On the Mac's oil frames at every pixel (2200 × 1400) Balanced
    went from 99 ms a frame to 59, and 29 hurried, the worst block as
    clean. On Linux at Balanced, Every pixel, 1650 × 990 and a tube, a
    recording kept 23.6 frames a second, nearly all 42 ms apart, 252 of
    258 hurried, and on Windows the same kept 23.6, 48 of 190 hurried.
    Left: how many a second Lossless keeps
    there (fewer than 24, as expected);
  - masking a prompt's names in the file, rather than only saying so.
- **A theme gallery in Settings** is in 0.1.35 (the manual's Themes,
  Theme gallery; `gallery.rs`): the catalog's packages of themes, each
  fetched, checked and read to show its themes' look before it's installed,
  with Install, Upgrade and Wear. A test Flow.app with it was tried on
  the Mac (2026-11-09). For 0.1.36 the preview shows a theme's logo and is
  drawn in its own letters, the package's fonts loaded from memory before
  it's installed. Windows and Linux previewed `flow-core-themes` from the
  catalog in a live window for 0.1.36. Left: a package with fonts of its
  own previewed for real (`flow-core-themes` brings none; snapshots show
  the notanAmiga's built-in letters and its Kickstart picture as a logo),
  and a package installed from the gallery on Windows and Linux. The
  gallery's row is the third under Theme, so with that section closed
  (Collapse all) it's out of sight; the palette's "gallery" finds it.
- **Accessibility** is in 0.1.36 (the manual's Accessibility; `access.rs`):
  AccessKit gives VoiceOver, Narrator and Orca a tree of the window, the
  title bar and tabs, each pane's rows in view with the cursor as the
  caret, the assistant's conversation, input and last reply, Settings'
  rows by their controls, Flow's own screens as dialogs and the toasts as
  live regions, the focus following the keys, and the readers' clicks,
  steps and choices working. Unit tests read the tree back as the
  adapters do. Narrator on Windows and Orca on Linux (GNOME's
  Super+Alt+S) read the terminal and Settings in a live window for
  0.1.36. Left: VoiceOver tried in a live window, and Narrator and Orca
  on the assistant's panel and Flow's dialogs;
  new output read out as it comes (a reader reads the rows when asked,
  and says what changed only as its own review of a terminal does); the
  scrollback above the screen; a selection made with the mouse as the
  text selection; and Settings' choices as lists of their options.

## Starting Flow from a tab

Asked for on 2026-11-16: `flow <command>` typed in a tab should run the
command in Flow, apart from the tab it's typed in. Before it, a Windows
tab's PowerShell answered "The term 'flow' is not recognized" (tried
with an `ssh -N -L …` tunnel).

**Decided** (2026-11-16): plain `flow <command>` asks the running Flow
for a new tab, and a new Flow comes only when asked for. **For 0.1.43**
(see [0.1.43](#0143), above). **Planned** in
`docs/design/flow-command.md` (2026-11-16), with a tab like the asking
one and all four options. **Built on the Mac** (2026-11-16), Windows and
Linux to build and try it.

- **`flow <command> [args…]` in a tab:** the running Flow opens a new
  tab beside the one it was typed in, with its shell, theme and folder.
  It types the command once the shell is at its prompt, quoted for that
  shell (as `Session::run_typed` runs a reply's), so the shell stays
  when the command ends. The request goes the way `flow-settings`
  already asks Flow: an OSC on the tab's terminal on the Mac and Linux,
  and down the tab's pipe on Windows, as a new kind of request beside
  `flow_sdk::settings`' ones. It carries the pane's own secret
  (`FLOW_TAB_KEY`), since a remote host's output can send that OSC too.
  `flow` returns once Flow has answered.
- **`flow --window <command>`, or `flow` with no tab to ask** (the Run
  box, a shortcut, a script, another terminal): a new Flow starts with
  that one tab.
- Running the command as the tab's own program (as `alacritty -e`
  does) could come later. `--close` closes the tab once its command
  ends well.

What's known for building it:

- **`flow` isn't on the tabs' PATH.** On Windows the PATH has `games\`
  (`flow-help`, `flow-settings`, `flowapp`, `flow-mcp`) but not the folder
  above it, where `flow.exe` is. The Mac's binary is in `Contents/MacOS`,
  not `Contents/Helpers`. So a small `flow` program goes in the folder
  of Flow's programs (`games\`, `Contents/Helpers`), beside
  `flow-settings`. On Linux `bin/flow` is Flow itself, found ahead of
  that folder where the user's PATH has the prefix's `bin`
  (`~/.local/bin`, or `/usr/bin` for the .deb). So there Flow reads the
  same arguments, and asks the running Flow the same way before it
  opens a window of its own.
- **Flow reads no command.** `flow_core::run` takes `--snapshot`,
  `--record` and `--carry-on` and passes over every other argument. So
  on Linux, `flow ssh …` opens Flow with its saved tabs and drops the
  command. A Flow started for a command must read it and the folder
  (say `flow --window [--cwd <dir>] -- <command> [args…]`) and open only
  that tab.
- **Two Flows at once** (only with `--window`, or with no Flow running
  where one starts while another is still coming up). A Flow started
  for a command shouldn't reopen `tabs.json` (`App::open_first_tabs`),
  nor save over the first Flow's as it quits (an elevated Flow already
  keeps none). Some things belong to one Flow only, and each needs a
  rule:
  - the drop-down hotkey: the second's registration fails, with a toast;
  - the MCP pipe or socket, while `mcp.enabled` is on;
  - the updater;
  - `flow.log`, which a second start empties, as the elevated Flow's
    does.
- **What the tab passes on** to a new Flow: the tab's environment.
  Flow already takes the folders of an inherited `FLOW_GAMES` and
  `FLOW_FIRST` off the PATH (`shell::path_for_games`). But
  `FLOW_IMAGE_PIPE`, `FLOW_PROMPT` and the other variables Flow gives a
  tab belong to the old tab and must go too.
- **Letting go of the tab,** for a new Flow. On Windows, PowerShell
  doesn't wait for a windowed program, so the prompt comes back at
  once. On the Mac and Linux, Flow started from a shell holds it, and
  would end with the tab's SIGHUP. So `flow` must start Flow apart and
  return at once: in a session of its own (`setsid`), or with `open -n`
  for Flow.app on the Mac.

## Combining marks

Reported on 2026-11-17: Up over a long command with Hindi in it came
back garbled in zsh. Fixed for 0.1.44 on the Mac: a
spacing mark such as the vowel sign ा, which the Mac's `wcwidth` gives no
width, goes on the letter before (`combining.rs`), and a cell's marks are
drawn with its letter, shaped together. What's left:

- **In the live window,** tried on the Mac in a test copy:
  the Hindi line typed and recalled whole, and accents written as marks
  drawn on their letters. Left: other sizes and the themes' fonts.
- **Conjuncts across cells:** each cell is shaped alone, so स्त shows its
  virama where a font would join it. Shaping a run of cells together and
  drawing it over their cells would join them.
- **Linux** keeps ा in a cell of its own, as glibc measures it.
  - Recall: zsh and bash bring a long Hindi line back whole with Up
    and Down (`linux/tests/shells.rs`), and so does BusyBox's ash, as
    the `?`s Ubuntu's build shows past U+02FF.
  - The other marks are now drawn on their letters (live, 2026-11-17).
  - ा alone shows only the font's dotted circle, as in 0.1.43. The
    shaper puts U+25CC before a lone sign and `Atlas::rasterize` keeps a
    cell's first glyph, so the sign itself is lost. Drawing it with the
    letter before, over both cells, would look right without moving it.
  - zsh at its prompt showed a mark of no width as `<0947>` markup:
    COMBINING_CHARS was off, which macOS's `/etc/zshrc` sets and
    Ubuntu's doesn't. Flow's zsh hook now sets it before the user's
    files (decided 2026-11-17), so its line looks like bash's.
- **Windows:** Windows' own ConPTY gives
  ा a column, as the emulator does, but the marks of no width (े, ्) one
  each too, so it measures the line 24 columns where the emulator lays it
  out in 20. PSReadLine measures as the console does, Git Bash's C
  library as the emulator does. cmd recalls the long line whole;
  PowerShell's Up puts it back with blanks at its first row's end,
  BusyBox does so as it's typed, and Git Bash's Down leaves `echo
  "नमस्short` (`windows/tests/shells.rs`, ignored while it fails). This
  was so before the Mac's fix. Ways out, none tried: measure as the
  console does in PowerShell's, cmd's and BusyBox's tabs (each mark a
  cell, drawn over the letter before), which leaves Git Bash's; or a
  ConPTY of Flow's own (Windows Terminal's OpenConsole) if its measuring
  can be the emulator's, which leaves PSReadLine's. ा itself is drawn
  alone with the font's dotted circle, as on Linux.
- **Over ssh** from the Mac, a Linux shell gives the mark a cell, which
  Flow on the Mac doesn't: its long lines can still come back garbled.

## Always on top

Asked for on 2026-11-17: a mode that keeps Flow's window in front of
every other window, from a pill in the title bar, and a shorter word than
"Efficient" for its pill. **Built for 0.1.44.1** as decided
(`docs/design/on-top.md`): `window.on_top`, Settings' Always
on top above the Hotkey, an **on-top** pill beside the gear that never gives
way, `Command::ToggleOnTop` (no key at first; the palette, Keys, the Mac's
Window menu's Float on Top), none on Wayland (its row says the window
manager's menu does it); and the Efficient pill reads **🌱e-mode** always.
Tried live on the Mac, where both worked. What's left:

- **The drop-down follows on-top** (decided 2026-11-18): it floated by itself with the pill unlit, found on the Mac and
  Windows; now it's on top only while on-top is on. To try on each system,
  and on the Mac over a full-screen app (Over full-screen apps on).
- **Tried live on Windows and Linux's X11**: the window kept on top, the
  drop-down putting it back on top after, and Flow's own panels (`rfd`: a
  recording's save panel, Export…, preset files), which might open behind
  a window on top there (on the Mac they're sheets on it).
- **The Mac's floating level** keeps to its own Space and doesn't follow
  into another app's full screen; only the drop-down's accessory app does.
- **The pictures**: every title bar has the pill and e-mode's word. The
  Mac's are made again on the Mac for 0.1.44.1; Windows' (CLAUDE.md's
  list) on Windows.
