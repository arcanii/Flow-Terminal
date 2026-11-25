[← Flow's documentation](../README.md#documentation)

# Status

What's planned next is in [BACKLOG.md](../BACKLOG.md).

**Tested on Windows 11:**
- build and unit tests
- command capture with exit codes in cmd, PowerShell 7, Git Bash and the bundled BusyBox,
  including output that doesn't end in a newline
- the agent loop with Claude, GPT and Gemini and the token cap against mock
  servers, and each provider's real API turning down a bad key
- offscreen rendering, the CRT included, and the interface in Japanese
- the display languages Windows reports (`en-GB`, `en-US` here), which
  Automatic follows; not yet on a Windows set to Japanese
- the installer (a silent per-user install and uninstall)
- on Windows 11 on Arm (in Parallels on a Mac): the x64 release cross-built
  there, installed, and rendering offscreen under x64 emulation; and 0.1.8
  installed for all users from its setup.exe, with the None style in the live
  window; typing Japanese through Microsoft's IME (Romaji) in the live window;
  and, since Parallels offers no Direct3D 12 there, drawing on the CPU
  (Microsoft Basic Render Driver)
- resizing the live window, with the startup logo: a line that wraps shows
  once, and the scrollback keeps every line when the window grows wider or
  taller (up to 0.1.17, narrowing could show a line twice, and growing lost
  the lines just above the screen)
- the startup logo in the live window: still there after Flow Rogue drew over
  its rows on the alternate screen, and after resizing then, and gone with
  `cls` (up to 0.1.19, a full-screen program that drew over the logo's rows
  took it away)
- Flow Rogue (x64): the installer putting it in `games` and removing it again;
  from the portable zip, `flow-rogue` typed in a live Flow tab and found in
  that folder, drawn in letters, played and quit back to the prompt; and in
  cmd, PowerShell, Git Bash and BusyBox through ConPTY, found on the PATH
  after the user's startup files
- Flow Rogue in sprites (an Arm64 build), through the pipe for images: in a
  live window, played, the window resized mid-game, started from BusyBox,
  and quit back to the prompt with the startup logo still there; and
  through ConPTY, every placeholder cell of its screens in its column and
  its picture found
- Flow Rogue 0.1.10 and Flow Mines 0.1.1 from an x64 build, on an x64 PC
  drawing with Direct3D 12: the installer putting both in `games` and
  removing them again; Flow Rogue's sprites drawn by Flow offscreen under
  each liquid style and the CRT; through ConPTY, every placeholder cell of
  its screens (from PowerShell and from BusyBox) with its picture found,
  its effects coming down the pipe, and quitting back to the prompt; and
  Flow Mines from PowerShell, its field drawn, the first tile opened, its
  effects coming down the pipe, and `q` back to the prompt
- real updates through WinSparkle: from 0.1.0 to 0.1.1 (a check from About,
  the signed download, the installer for all users, and the relaunch), and
  from 0.1.20 to 0.1.21 on an x64 PC (a check from About, and both games put
  in the `games` folder of the install for all users), from 0.1.21 to
  0.1.25 there (Flow and both games installed byte for byte as built), and
  from 0.1.25 to 0.1.25.1 (WinSparkle going by the build, 0.1.25.192 over
  0.1.25.185, with `flow-demo` put in `games` too, all byte for byte as
  built)
- for 0.1.25, from an x64 build on an x64 PC drawing with Direct3D 12:
  - build, unit tests and clippy, the GPU's model drawing and the sound
    output (WASAPI) opened and played through at no volume
  - the command history through ConPTY in cmd, PowerShell 7, Git Bash and
    BusyBox: a command typed a key at a time noted with its line, its folder
    and its exit code (cmd has none), PowerShell's and bash's own word for
    the line, a line that starts with a space kept so, and the assistant's
    commands
  - a machine's prompt in cmd, PowerShell 7, Git Bash and BusyBox, with its
    folder and a backslash, still marked, timed and kept (BusyBox has no
    history number to show)
  - mouse reports through ConPTY: Flow Mines taking the mouse, ConPTY asking
    for it (any motion, SGR), and a click opening the field
  - a model turning in 3D (Flow Rogue 0.1.19's): sent down the pipe for
    images after the screen it's over, and its cells kept through ConPTY's
    paints
  - DEC's double lines (`ESC # 3`, `4`, `6`) and DECPS's notes through
    ConPTY
  - offscreen: every style and the CRT, Settings with your own theme's
    rows, the history's search, the About card's cube, and the Home
    computer, Green screen, School computer (its teletext page) and
    Personal computer in PowerShell tabs, with the computer's memory in
    their start screens
  - the computer named for the startup logo and the start screens (the
    edition of Windows, the processor, the memory installed), in a unit test
  - the SDK's Python package's tests run against the C library's DLL with no
    terminal
- for 0.1.25.1, on the same PC:
  - Flow Rogue 0.1.20 quitting through ConPTY from Git Bash, BusyBox and
    PowerShell 7: the tab back on the main screen as it was, with nothing of
    the game left (up to 0.1.19 on Windows, the game's last screen stayed,
    and the tab on the alternate screen, under the shell's prompt); and a
    full-screen program built on flow-sdk without the mouse doing the same,
    in a real-PTY test
  - `flow-demo boing` and `flow-demo cube` from PowerShell through ConPTY:
    each's picture placed down Flow's pipe and kept, every cell, while its
    scene came down the pipe each frame; its effects with it; a click taken;
    and `q` deleting the model and back at the prompt
  - Boing in a Git Bash tab, drawn offscreen on Direct3D 12
  - both demos played in a live window of 0.1.25.1, their sounds heard with
    Sound on
  - Flow Rogue quit in a live window of the installed 0.1.25.1, with nothing
    of the game left on the screen
  - the patch's version: About showing `0.1.25.1 (build N)`, and `flow.exe`'s
    product version saying so while its file version stays `0.1.25.N`
- for 0.1.27, on the same PC:
  - build, unit tests and clippy, with the two double-line tests that count
    on alacritty's resize left to macOS and Linux (Windows resizes as ConPTY
    does, and its own test of double lines through that passes)
  - the six new demos from PowerShell through ConPTY: plasma's picture down
    Flow's pipe in bare cells, teletext's double lines and mosaics,
    vector's scene, the sounds demo's tune and keys reaching Flow as
    sounds, tint washing the glass and washing it off on `q`, and model
    asking to read its file by its Windows path and shown once allowed;
    each gone on `q`, back at the prompt
  - a resize under Boing, Vector and Tint, each placed again at the new
    size and still quitting on `q` (Vector, whose frames take longer than
    it allows them here, heard no keys at all before the SDK's fix)
  - an old machine's start screen through the window settling as Flow
    starts (sized to the screen and back while it types), kept whole at the
    top, in a real-PTY test with PowerShell, and with Git Bash and other
    sizes by script, each shell sized right after; up to the test build,
    ConPTY scrolled it off the top
  - offscreen on Direct3D 12: the Workstation's boot with Sun's emblem, the
    Graphics workstation's with SGI's cube turning, Settings in sections
    with Restart Flow at the end, About's Restart Flow and Legal, and help
    with Windows' keys; and the GPU test's models, their new materials
    among them
  - the sound output opened through WASAPI, in the ignored test
- for 0.1.28, on the same PC:
  - build, unit tests and clippy, nothing to change but three warnings
    only Windows' build gives
  - the Workstation's PROM reading this PC: SMBIOS's memory slots, the
    empty ones too, and the PCI slots under its IOMMU named by their
    drivers, in 7 ms, nothing identifying among them
  - the assistant's Run in cmd, PowerShell 7, Git Bash and BusyBox through
    ConPTY, one line and two: Git Bash takes the two as a bracketed paste,
    the others joined (`&` in cmd, `;` elsewhere), each run once and
    noted for the history
  - the pipe for images answering OSC 11, 10 and 4, each ended as asked,
    before DA1's answer
  - every demo and `flow-invaders` from PowerShell through ConPTY: the
    cube's four materials, the reveal and a resize; Boing, the plasma,
    Vector and Tint placed again after a resize; teletext, the sounds demo
    and the balloon as in 0.1.27; and a wave of `flow-invaders` with a
    ship locked, a missile fired and a ship down; each gone on `q`
  - offscreen on Direct3D 12 with the lower limits asked of the GPU: the
    Efficient pill, About's Release notes, Settings' new rows, and the GPU
    test's models; `flow-help` printing a page and turning away one there
    isn't
- for 0.1.29, on the same PC:
  - build, unit tests and clippy, with nothing to change
  - the power read from Windows (`GetSystemPowerStatus`): this desktop
    plugged in, with no system battery and energy saver off, in a test that
    a computer without a battery is never on one; not tried on a laptop
    on battery or with energy saver on
  - box drawing's lines and squares drawn on Direct3D 12 (light boxes
    with rounded corners, double and heavy ones with their junctions,
    diagonals, dashes, and ■ □ ▪ ▫ ◼ ◻ ◾ ◽), offscreen in a scratch build
    whose demo command printed them
  - offscreen: Font size's readout and **Fit window to**, and the made-up
    conversation's **Copy** on its lines and command cards, which made the
    README's `main.png` again
  - Flow Rogue 0.1.28's scenes shown from PowerShell through ConPTY, its
    sprites down Flow's pipe and drawn again after a resize; and the
    reworked cube as for 0.1.28
- for 0.1.30, on the same PC:
  - build, unit tests and clippy: the Python package's test now lets its
    two Windows-only skips be (settings answered as keys in a PTY, which
    on Windows come down Flow's pipe)
  - what each tab uses read through Toolhelp: the test's own process and
    a child it started, with their memory and CPU, and offscreen the tab's
    card and About's Resources (a PowerShell tab's one program, 99 MB;
    Flow's own memory, CPU and GPU)
  - this PC's id for the workstations' numbers (its shape only), and its
    real numbers read without being shown
  - `flow-settings` answered down Flow's pipe by a whole Flow, offscreen
    in a scratch build: `get fx.style` and the list piped through
    PowerShell, exit 0
  - `flow-demo model`'s file and folder deleted on `q` and when its tab
    closes under it (the console's close event), through ConPTY
  - offscreen: the new-tab picker, saving a theme and its naming box, which
    made the README's `main.png` again for the title bar's new …
- for 0.1.31.1, on the same PC:
  - build, unit tests and clippy, with nothing to change, and the ignored
    GPU tests on Direct3D 12 (SGI's cube standing as its logo, a font
    switched back drawing from its old glyphs, the models' materials)
  - a tab greeting with the startup logo and then an old machine's start
    screen typed below it, in PowerShell and Git Bash through ConPTY,
    with and without the window's first sizing: the logo at the top, the
    start screen under it, the prompt after, nothing scrolled off
  - About's Resources timing the GPU with Direct3D 12's timestamps,
    offscreen: 8.3 ms a frame on this PC's integrated GPU, which Flow
    draws with
  - not tried here: tabs reopened at a start, Ctrl+Shift+B's restart of
    one tab, a tab's card from the keys, and Flow Rogue 0.1.45 played
- for 0.1.31.2, on the same PC, made there and on Windows alone:
  - the Flow Instruments Home Computer theme: build, unit tests (its title
    screen, its colors in hex, the centered logo, its sounds) and clippy
  - offscreen, its title screen between its bars with the logo in the
    middle, and its name in the new-tab picker
- for 0.1.31.3, on the same PC:
  - build, unit tests and clippy, with nothing to change
  - offscreen, Settings on the tab in front: its row of tabs and New tabs,
    and each section's header saying whose its settings are
  - not tried here: moving Settings between tabs by click or by key,
    Ctrl+= on one tab, a tab's own look across quitting
- for 0.1.32, on the same PC:
  - build, unit tests and clippy, with nothing to change, the SDK and
    the demos taken from Flow-SDK's tag
  - the release script's first run with them: `flowapp` and flow-demo
    0.1.10 in `games`, flow-sdk and flow-demos in the notices, and no
    program naming the account or the folders it was built in
  - the Flow Instruments title screen waiting for a key through ConPTY
    in PowerShell, with the window's first sizing, then clearing for its
    BASIC at the foot, the key not typed into the shell
  - `flowapp` installing, upgrading and removing an app in a scratch
    folder with Windows' own `tar`, which it finds before Git's; and
    `list`, `outdated` and `install` as built, the catalog having no
    Windows downloads yet
  - offscreen, the theme's own letters and the rain's streams
  - not tried here: a key pressed on the title screen in a live window,
    the rain's rates and storm, Fit text to
- for 0.1.33, on the same PC:
  - build, unit tests and clippy, the games taken from Flow-Games' tag;
    Windows' shell test given the start screen's new picture argument
  - a link a program writes (OSC 8) coming through ConPTY onto the cells
    of its words and no others, in PowerShell
  - `flowapp search` through ConPTY: its ruled table and header, cut to
    100 columns, with no links on the names while the catalog has no
    Windows downloads
  - offscreen, the Flow notanAmiga CLI's white page with the Kickstart's
    picture waiting for its key, and its shell after it, its prompt at the
    drive's root from PowerShell
  - not tried here: Ctrl+click opening a link in a live window, the
    drive's sounds
- for `flowapp`'s first Windows downloads, Flow Invaders 0.1.20 and
  `flow-sdk` 0.1.2, made by `app-release.ps1` on the same PC:
  - the SDK's own tests (Flow-SDK's `v0.1.2`, its C library's and Python
    package's among them), all passing
  - each tarball unpacked and its programs run, and installed with
    `flowapp`'s own code from a local copy of the catalog into a scratch
    folder, a copy with one byte changed refused and nothing of it left
  - the SDK's Python package loading its own DLL from the install, and
    its `hello.c` built with MSVC against the header and
    `flow_sdk_c.dll.lib` and run beside the DLL
  - no program or library naming the account or the folders it was
    built in
  - not tried here: `flowapp install` against the published catalog, the
    static `flow_sdk_c.lib` linked into a program, the SDK's audio heard
- for `flowapp`'s Windows Flow Rogue 0.1.46 and Flow Mines 0.1.6, the
  versions Flow 0.1.33 carries, made the same way:
  - each tarball unpacked and its program's `--version` the catalog's,
    and installed with `flowapp`'s own code from a local copy of the
    catalog into a scratch folder, a copy with one byte changed refused
    and nothing of it left
  - both installed the same way from the published catalog and GitHub
  - neither program naming the account or the folders it was built in
  - not tried here: the games played from that install
- for `flowapp`'s Windows Flow Invaders 0.1.21, the same as the Mac's,
  made the same way:
  - the tarball unpacked and its program's `--version` the catalog's,
    and installed with `flowapp`'s own code from a local copy of the
    catalog into a scratch folder, a copy with one byte changed refused
    and nothing of it left
  - installed the same way from the published catalog and GitHub
  - the program naming neither the account nor the folders it was built
    in
  - not tried here: the game played from that install
- for 0.1.34, on the same PC:
  - build, unit tests and clippy, with the Mac's new platform code (the
    taskbar's flash for a notification, the drop-down hotkey's
    `RegisterHotKey` thread) built here for the first time
  - pictures through ConPTY, from cmd, PowerShell, Git Bash and BusyBox:
    an iTerm2 PNG shown, and from all but cmd the text after it where
    iTerm2 leaves it; a sixel dropped by ConPTY
  - offscreen, split panes, the command palette, the find bar and the
    green screen's phosphor
  - the release script with its smoke test (no game or demo installed),
    and its setup over 0.1.33's in a scratch folder, which took
    `flow-rogue.exe`, `flow-mines.exe` and `flow-demo.exe` away
  - not tried here: any of 0.1.34's new screens and keys in a live window,
    the hotkey and the taskbar's flash, and the first start after an
    update installing Flow Rogue and Flow Mines with `flowapp`
- for `flowapp`'s Windows Flow Invaders 0.1.26 and Flow-Utils 0.2.0, made
  the same way:
  - each tarball unpacked and its programs' versions the catalog's
  - both installed with `flowapp`'s own code from the published catalog
    and GitHub into a scratch folder
  - not tried here: the game played, Flow-Utils in a live Flow window
- for 0.1.35, on the same PC:
  - build, unit tests and clippy, and the shells through ConPTY: a
    command Flow types over something half-typed, a PowerShell one
    beginning with `[`, and what `flowapp first` puts ahead in
    PowerShell, cmd, BusyBox and Git Bash
  - offscreen, Settings' Collapse all and Expand all, the theme gallery,
    Settings' Apps and a split tab with a pane in the green screen's theme
  - Flow Invaders' download made, unsigned, by its own repository's
    script with Flow's tools
  - the release script with its smoke test
  - not tried here: 0.1.35's new screens and keys in a live window, the
    drop-down's slide and Press a key…
- for 0.1.36, on the same PC:
  - build, unit tests and clippy, and pictures through ConPTY: an iTerm2
    PNG whole with the next command below it in cmd, PowerShell, Git Bash
    and BusyBox
  - in a live window: the glass taken opaque at the first start over a
    0.1.35 config at 55%, the Flow notanAmiga CLI's frame (a split tab of
    two framed panes too), the theme gallery's preview, Ctrl+click on an
    `https://` address in plain text, a moving GIF and WebP sent as
    iTerm2's, and Narrator reading the terminal and Settings
  - the release script with its smoke test
  - not tried here: 3D models' new files, DECSDM, WinSparkle updating
    0.1.35 to it
- recording the terminal (after 0.1.36), on the same PC: libwebp built by
  MSVC, the unit tests, and in a live window its countdown from
  `Ctrl+Shift+Alt+R`, the clock, stopping from the clock and the keys,
  the save panel and the file played in a browser, at Balanced and at
  Sharp, with the window resized while recording; not tried here: Smaller
  file, Lossless, Every pixel, a display scaled above 100%
- Your sizes for the Fit rows (after 0.1.36), on the same PC: the unit
  tests, and in a live window sizes typed and kept, each picked in Fit
  window to and Fit text to, a messy line tidied, and Esc putting back
  what was there; not tried here: the note of what was left out, a size
  larger than the screen
- for 0.1.37, on the same PC: build, unit tests and clippy at the release's
  commit, the release script with its smoke test, and WinSparkle updating
  an installed 0.1.36 to it, every file the same as the release's
- no white on a phosphor (after 0.1.37), on the same PC: the unit tests;
  offscreen, every pixel of the green screen a shade of its phosphor; and
  in a live window the green and amber screens, a BBS's welcome through
  `dial` among them, and bright text under the tube's glow
- toward 0.1.38 (after 0.1.37.4), on the same PC: the unit tests, and a
  kitty animation down a tab's pipe for images in a test; in a live
  window a kitty animation four rows tall sent down the pipe, in step and
  with no gap between its rows on the green screen too, a recording at
  every pixel with a tube keeping 23.6 frames a second, the green and
  amber screens with no grille, and a BMP with alpha and a 16-bit TIFF
  from PowerShell and Git Bash; the GPU tests' vertex colors and
  instances on Direct3D 12; not tried here: `kitten icat`, which ConPTY
  can't carry, and a glTF tool's own file
- a toast for a long command done and Fit window to's room (toward
  0.1.38), on the same PC: the unit tests (the toast's XML read by
  Windows' own, the notification switches read from the registry), and in
  a live window a toast shown for a command done, the taskbar button's
  flash (with Windows' notifications off) stopping for good once Flow is
  in front, and a window at the screen's bottom right fitted to more than
  the screen holds, filling the work area down to the taskbar; not tried
  here: a toast clicked from the notification center long after, the
  setup's uninstall taking the registration away, a taskbar at a side or
  hidden, and a second screen
- for 0.1.38, on the same PC: build, unit tests and clippy at the
  release's commit, the release script with its smoke test, its
  uninstall taking the toast's registration away, and WinSparkle
  updating an installed 0.1.37 to it, every file the same as the
  release's
- the PC's fan steady (toward 0.1.39), on the same PC: the unit tests (a
  minute of its two layers laid as Flow lays them, no quieter where a
  voice crosses to the next, at either spacing) and `examples/sounds`'
  `fan-minute.wav` measured the same way with the hum: 0.76 dB quieter
  every 2.5 s before, 0.01 dB after, and listened to: steady; not yet
  heard in a live window
- the items toward 0.1.39 (after 0.1.38), on the same PC: the unit tests
  and clippy, a snapshot's About with its links on a line
  of their own, and in a live window (build 580, 2026-11-14), all as they
  should be: every default key (Ctrl+Shift with T, W, D, R, F, P, K, E
  and B, Ctrl+Alt+Shift+D, Ctrl+Tab and Ctrl+Shift+Tab, Ctrl+Shift+1 to
  9, Ctrl+Alt and the arrows, F3 and Shift+F3 in the find bar and F3
  reaching cmd without it, Ctrl+Shift+?, Ctrl+, Ctrl+=, Ctrl+- and
  Ctrl+0, Ctrl+Space, Ctrl+Alt+Shift+R, Ctrl+Shift+End and Home), Ctrl+C
  copying only with a selection, Ctrl+Insert, Shift+Insert and Ctrl+V;
  Settings' Keys (New Tab rebound and reset, Find given the palette's key
  with Swap and Take It, a system key refused, Backspace leaving none,
  Ctrl+L taken with a toast, Reset All); searching Settings in English
  and in Japanese through Microsoft's IME, Enter going to the setting and
  Esc emptying the field; the scrollbar dragged and its track clicked in
  its three modes, through ConPTY's resizes too, and its pill; the find
  bar's marks in a long scrollback, one clicked; a split tab's ✕ and the
  notanAmiga's close box; About in Japanese; and Narrator reading the
  search box, the panes' close buttons and the pill
- toward 0.1.40 (after 0.1.39), on the same PC: the unit tests and
  clippy; Caps Lock's notice in a snapshot; each command marked
  through ConPTY from cmd, PowerShell 7, Git Bash and BusyBox, done or
  failed, those in the scrollback outlasting a resize and the one on the
  screen lost to ConPTY's repaint; Previous Command's jump in a snapshot;
  and sixels sent down the pipe for images from each shell, the picture
  whole between the command's lines, by script; not then in a live
  window. Later: release.ps1 refusing the 0.1.40.DEV build, and with
  `-AllowDirty` making one whose files say so; and Git Bash not taken as
  at its prompt before its first is drawn, nor while a command typed ahead
  runs, through ConPTY. Then in a live window (a test build, 0.1.40.DEV
  build 598, 2026-11-15), all as they should be: Caps Lock's
  notice; Previous and Next Command in PowerShell, cmd, Git Bash and
  BusyBox with the marks on the scrollbar; Add Key; the Hotkey taking
  Ctrl+Shift+T from New Tab and refusing Alt+F4 while it waits, and its
  row without F12, which Windows keeps for debuggers; "amber" in the
  palette finding Phosphor; a sixel sent down the pipe from cmd; and the
  banner ending with DirectX's wordmark. Not tried by hand: kitty's
  `d=f`
- toward 0.1.44.3 (after 0.1.44.2), on the same PC: the unit tests and
  clippy, and a second start handed to the first through the named
  objects, or run beside it when unanswered. In a live window (a test
  build, 0.1.44.3 build 726 on a scratch profile, 2026-11-18), all as
  they should be: the icon in the notification area while put away and
  its click; a second start bringing the Flow running forward, dropped
  down or where it was, with no second Flow and no toast; Show Flow and
  Quit Flow from its menu; the icon back after Explorer restarts; and
  none left after quitting

**Tested on macOS 27 (Apple silicon):**
- build and unit tests
- screen readers' tree (for 0.1.36): the terminal's rows (a wrapped one
  running on, a wide letter, marks on a letter, a double line) and the
  caret, the tabs, every kind of Settings row, dialogs, the API key read as
  dots, a toast as a live region, and a reader's clicks, steps and choices,
  in unit tests that read the tree back as AccessKit's adapters do; a test
  Flow.app built, not yet tried with VoiceOver
- 0.1.34's notification of a long command done and drop-down hotkey: the
  settings, their config keys, the hotkey's names and labels, the
  drop-down's rectangle (AppKit's room for windows turned into winit's
  pixels) and when a notification is due, in unit tests; and from
  Flow.app, a long command's notification shown live, with no sound of
  Flow's besides; not yet tried live: its click, the hotkey pressed in
  another app, over a full-screen one, on a second screen
- 0.1.35's drop-down, finished: the heights, a key pressed written for
  the config and read back, winit's keys as the hotkey's, the slide's
  easing, steps and end, where it starts with another screen above, the
  wait before a window that lost the keys is put away, and the rows in
  Settings, in unit tests; built into a test Flow.app; not yet tried
  live: the slide, the heights, putting away on losing the keys, a key
  pressed in Settings, Over full-screen apps, and whether AppKit leaves
  the window above the screen to slide (Flow's own
  `constrainFrameRect:toScreen:`)
- command capture with exit codes in zsh and bash, and a UTF-8 `LANG` for
  shells when none is set
- a long command with Hindi in it typed whole and put back whole by ↑ and
  ↓, in zsh and bash in a real PTY (for 0.1.44; up to 0.1.43, the vowel
  sign `ा` took a cell the Mac's shells don't count, and zsh lost letters
  as the line was typed); the Mac's spacing marks held to its libc's
  `wcwidth` by a test; in the live window (a test copy), the Hindi line
  typed and recalled whole, its signs on their letters, and accents
  written as marks drawn on them
- offscreen rendering (Metal)
- the release pipeline: Flow.app with Sparkle and the notices, inside-out
  signing with Developer ID, the DMG, notarization and stapling, and the
  appcast script's checks
- Sparkle loading in the packaged app
- the agent loop with Claude, GPT and Gemini against mock servers, and each
  provider's real API turning down a bad key
- typing Japanese through the input method (Romaji), in the live window
- text inserted the way Emoji & Symbols inserts it, reaching the prompt
  (offscreen)
- the startup logo with Flow's icon and with the Apple logo, offscreen
- images through the kitty graphics protocol, offscreen: sent in chunks by a
  script, by `timg -pk`, and by `viu`, which found the support itself; the
  replies to kitty's query, XTVERSION and the pixel sizes; and the CRT and the
  liquid over them
- Sixel pictures and iTerm2's inline PNGs and JPEGs, offscreen: decoded and
  placed by tests, and a sixel and a PNG printed by zsh and bash in a real
  PTY, the cursor after each where xterm and iTerm2 leave it; DA1's and
  XTSMGRAPHICS's answers; and in a live window, a sixel from `printf`,
  `img2sixel`'s and `timg -pi`'s pictures, and a PNG sent by `printf` as
  iTerm2's, 20 cells wide
- DECSDM and sixelScrollsRight (with DECRQM and RIS), iTerm2's files in
  parts, GIFs and WebPs (animated ones' frames and timing), pictures wrapped
  for tmux and pictures let go once no cell shows them, offscreen: by
  tests, and a PNG in parts and an animated GIF printed by zsh and bash in
  a real PTY; and in a live window, an animated GIF from ffmpeg's test
  pattern sent as iTerm2's, playing
- kitty's animation commands (`a=f`, `a=a`, `a=c`, and `d=f` and `d=F`
  deleting frames after 0.1.39) and an animation's one
  texture, offscreen: by tests, down the pipe too, a kitty animation
  printed by zsh and bash in a real PTY, and the texture written frame by
  frame on the Mac's GPU
- Flow Rogue's screens in sprites, offscreen, under each liquid style and
  the CRT; and played in a live window
- Flow.app carrying `flow-rogue` (an ad hoc build of the release pipeline):
  signed with the app, and found on the PATH of zsh and bash tabs, after the
  user's own startup files
- Flow Mines played in a live window, from a build before the 0.1.21 release
- real updates through Sparkle, from 0.1.19 to 0.1.20 and from 0.1.20 to
  0.1.21
- the command history: zsh and bash in a real PTY, with a command typed a key
  at a time, noting its line (zsh's own word for it too), its folder and its
  exit code, a line that starts with a space, and the assistant's commands;
  macOS's bash 3.2 and Homebrew's bash 5.3 sending a command continued over
  two lines whole, once, keeping `$?` and `$_`, and nothing for a line
  HISTCONTROL leaves out; the assistant's search tool's
  input and report in unit tests;
  the search, its filters and keeping secrets out in unit tests; and the
  search panel, offscreen
- mouse reports' forms in unit tests, and the Mouse reporting setting,
  offscreen
- 3D models: the protocol, OBJ and glTF loading, placing, moving, updating
  and deleting in unit tests; `show_model` in a real PTY with bash, sending a
  cube and a 7,200-triangle sphere in 61 chunks; and a model drawn,
  offscreen. Occlusion, the second set of uvs, texture transforms, emissive
  strength, the alpha modes and the see-through parts' order, from glTFs made
  in the tests, and flat artwork with `depth` turning and playing its
  animation, in unit tests; the see-through parts blended, the occlusion and
  the moved uvs drawn on the GPU in an ignored test, not yet in the app
- pictures and models cut at the edge as the grid narrows and filled out
  again as it widens, the alternate screen held or not, and through the
  rewrap Windows' resize does, in unit tests; not with ConPTY itself
- the themes: their looks, start screens and prompts in unit tests; a
  machine's prompt in zsh and bash in a real PTY, still marked, timed and
  kept; and the Home computer, Green screen, School computer and Personal
  computer, with their start screens, offscreen; your own theme, and a
  preset carrying it, in unit tests
- double-width and double-height lines: read from the stream, kept through
  scrolling, the scrollback, erases within them and resizes (under a
  program's alternate screen too, and through ConPTY's rewrap), made
  single by a clear, losing their right half when made double, copied as
  their text alone, wrapping at their half with the cursor stopping there
  (and what wrapped copied, and kept in the history, as one line with
  them), inserting and deleting characters within it, and keeping pictures
  to it; a resize rewrapping text that wrapped at the half with what it
  wrapped onto (narrower and wider, with wide letters, the cursor by its
  letter, a picture cut there growing back, read, copied and typed the
  same), keeping a single line's wrap onto a double one, and cutting the
  alternate screen's and lines a program laid out,
  in unit tests, with the pointer's cells, where a picture's piece goes on
  them, and the start screens' colors and double lines; the School
  computer's teletext page, and a tab given double lines, sextants,
  separated sextants and the block elements, drawn offscreen
- the demos: their scenes, their glTF read back, their stage within Flow's
  limits and their effects in unit tests; and `flow-demo boing` and
  `flow-demo cube` typed into a bash tab and drawn, offscreen, from the first
  landings to the cube's reveal
- `flow-demo` 0.1.4's plasma, teletext and vector: their pictures, pages,
  mosaics, layout and effects in unit tests; what each writes, recorded in a
  pty that answers as Flow does (the plasma's picture and the vector
  display's scenes drawn from the recording); and each typed into a bash tab
  and drawn offscreen, over every liquid style and under the CRT, teletext's
  index, clock and sea among them
- `flow-demo` 0.1.5's sounds, tint and model: their tunes, Flow's queue of
  notes and its allowance kept to, the wash sent only when it changes, the
  model's file named, placed, turned and deleted, layouts and effects, in
  unit tests; what each writes, recorded in a pty that answers as Flow does
  (and as a terminal that shows models but isn't Flow, and one that shows
  none), its text screen rendered from the recording, and the tint demo's
  scenes through a day and the balloon drawn from it by a small software
  rasterizer lit as Flow lights models; not yet in Flow itself
- `flow-demo` 0.1.9's model leaving nothing behind: the sweep's choice (a
  gone run's folders, this run's own, names not the demo's, links, and
  folders open to others or holding something else), the hang-up's flag
  and the thread that ends a demo stuck reading a hung-up terminal, in
  unit tests; and in a pty that answers as Flow does, its file deleted on
  a hang-up (the pty closed, as closing a tab does), on SIGTERM (the model
  taken away and the main screen back) and on q, and a run killed with
  SIGKILL swept by the next; Boing ending on a hang-up and on SIGTERM too;
  and SIGTERM ending a demo whose terminal has stopped reading, its file
  deleted, in bash and zsh, which then give the tty's modes back
- the About card's cube, offscreen: settled, and its roll, bounces and jump
  captured at eight moments; each frame inside its picture and wound right
  in unit tests
- a game's models in Flow: Flow Rogue's dragon drawn on the GPU from four
  sides by Flow's model renderer, and its crown and gem read by Flow's
  model loader and drawn in a snapshot
- the About card's cube tried in a live window of an app built from this
  tree before the 0.1.24 release, with Flow Rogue 0.1.13 played there, and
  found good
- the new sounds (patter, bubble, clack, a tube's hum laid without a seam),
  the liquid reporting its rain, bubbles and lightning, a program's wash
  read and kept, the bell's flash, the School and Personal computers, your
  own theme and presets carrying it, in unit tests; the two machines drawn
  in a snapshot
- 3D models: a glTF's animations (step, linear and cubic), skins and morph
  targets posed, its materials, the click's ray cast and answer, and
  repicturing after a font change, in unit tests on GLBs built in code;
  balls of gold, a normal-mapped one, an emissive one and a skinned bar
  drawn in snapshots, and the gem and the About cube unchanged
- 3D models: the unlit, specular, ior, clearcoat, sheen, transmission and
  volume extensions, lights and a camera read from GLBs built in code, and
  see-through triangles cut where they cross and drawn farthest first, in
  unit tests; each drawn on the GPU in the ignored test, whose pictures from
  before came out the same byte for byte; not yet in the app
- 3D models: files requiring the extensions Flow reads loaded, and those
  requiring others refused by name; lights and a camera moved by an
  animation, and a moving model's crossing see-through triangles cut as each
  frame poses them (or the cuts made at rest moved with it), in unit tests
  on GLBs built in code; a light carried across and two squares crossing as
  one slides drawn on the GPU, the pictures from before the same byte for
  byte; the cutting timed at under a millisecond a frame; not yet in the app
- 3D models: meshopt's codecs and filters against meshoptimizer's own test
  streams and values worked out by hand, and round trips through an encoder
  written to its spec; meshopt-compressed, quantized and WebP-textured GLBs
  (a ball packed as gltfpack packs one, a triangle quantized every way,
  filtered animations) read as their plain twins, and a WebP's fallback
  taken where it won't decode; a perspective camera's picture matched
  against a pinhole camera's, clicks through it, and a camera coming nearer,
  in unit tests on GLBs built in code; squares going off into the distance,
  a cube close up and the packed ball drawn on the GPU, the pictures of
  models without a perspective camera the same byte for byte; no real
  gltfpack or meshoptimizer output tried, as none was at hand; not yet in
  the app
- 3D models (0.1.36): meshopt's vertex codec version 1 and color filter,
  written from KHR_meshopt_compression's text and checked against
  meshoptimizer 1.3's own decoder, in streams worked out by hand and round
  trips through a test encoder written after meshoptimizer's (every column
  mode and channel); Draco meshes made by Google's own encoder (1.5.9, at
  three levels) and by draco-core's (colors and all) read as the ball they
  were made from; Basis textures made by Binomial's own encoder (basisu
  2.50: ETC1S, and UASTC with Zstandard and mipmaps) read as the checker
  they were made from, before their fallback; the specular-glossiness
  material read as glTF-Transform converts it; a model turned in front of
  its camera kept on the picture; lights a clip carries nearer settled
  over it; in unit tests, and the packed, Draco and KTX2-textured balls,
  the specular-glossiness balls, a stood-up plank and a light passing in
  front drawn on the GPU and looked at; no whole file from a glTF tool
  (gltf-transform, gltfpack, Blender) tried; not yet in the app
- 3D models (after 0.1.37): vertex colors (`COLOR_0`) of every packing
  (floats, normalized bytes and shorts, RGB and RGBA, meshopt's color
  filter, Draco) reaching the vertices, a model without them white at
  every one, and see-through cuts taking the color between their corners,
  at rest, cut afresh and posed; GPU instancing placing each instance in
  its node's space before the node, mirrored ones wound round, posed with
  the node's animation and its skin, the box fitted over all of them, a
  click on each naming the node, and files past the limits or with
  instances that can't be read refused saying why; all in unit tests on
  GLBs built in code; a vertex-colored triangle (solid and see-through),
  the colored meshopt and Draco balls and the instances, still and moving,
  drawn on the GPU and looked at, white vertex colors drawing the very
  pixels none do, and the gem and the About cube unchanged in a snapshot;
  no file from a glTF tool tried; not yet in a live tab
- the SDK: detection, pictures, models, meshes, scenes, effects and its
  audio's mixer in unit tests; the demos ported onto it sending the very
  same bytes as before for a scripted session, compared byte for byte; its
  examples run in a fake terminal
- the SDK from C and Python: every function the C library exports declared
  in its header and no other, the header's numbers and structs the crate's,
  each function's escapes the Rust SDK's and its refusals (NULL handles,
  pixels that don't fit, names that would end an escape) in unit tests;
  scenes, cameras and stages from C sending the Rust SDK's very bytes, and
  tones, tunes and songs from C mixed sample for sample as the SDK mixes
  them, in unit tests, with no sound played; `hello.c` and `scene.c` built
  with `cc` and the Python package's tests run against the library, with
  its audio and without, with no terminal; and both of each language's
  examples, and a picture below the prompt from Python, run in a PTY that
  answers as Flow does (the scene examples' frames, placement, click and
  resize, from a library without audio, so nothing played)
- Flow Rogue and Flow Mines 0.1.4 on the SDK: every effect, wash,
  picture, cell and escape each sends, and the music's samples, compared
  with what the games sent before, byte for byte and bit for bit, in unit
  tests; not played live
- sound: each cue made in unit tests (heard, not clipping, dying away, panned),
  the mixer, DECPS and `sound=` read from the stream, every cue written to
  WAV files by `examples/sounds.rs`, and the output opened and played through
  at no volume; and listened to in a live window of an app built from this
  tree, with Sound turned on in Settings
- in the tree, for 0.1.28, on this Mac: `flow-help` printing every page,
  one, the list and a page there's none of, piped and in a pty, in English
  and Japanese, and run from a Flow.app's `Contents/Helpers`; About's
  **Release notes** beside the version, in a snapshot (its link not
  clicked); the assistant panel's **Copy** under a block of code, in a
  snapshot, and finding blocks, leaving out a session's prompts and the text
  a drag selects in unit tests (**Run**, **Copy reply** and dragging not
  tried in a live window); **Efficient** in the title bar and Settings'
  **Efficient mode** and **Cursor speed** rows, in snapshots, and the
  cursor's speed in unit tests (the mode itself not switched on in a live
  window); Theme details' rows, each machine's sixteen colors and the
  switches in unit tests, not looked at in a live window; the cube demo's
  rounded blocks in glass, metal, rock and jelly (m and 1 to 4 switching
  them as it plays), its notch, fit and F on every face, drawn offscreen
  with Flow's model renderer and in unit tests, and every frame of a play
  of each loaded, cut and sorted as Flow does it (at most 2,043
  see-through triangles, under the 2,048 Flow cuts afresh each frame for a
  moving model, none crossing, and a millisecond and a half a frame at
  most), not watched in Flow; and the Workstation's PROM probing this
  Mac's own memory and buses (its reader timed at a few milliseconds, its
  lines printed), with Linux's and Windows' readers
  written from the kernel's, systemd's and Microsoft's documentation, their
  parsers tested here on made-up data, the readers themselves not compiled
  or run; programs' effects drawn over cells' backgrounds and under their
  letters, on a field laid as Flow Mines lays it over Flow's dark glass and
  a light one, in offscreen pictures (made by code since taken out), and
  the order in a unit test, not watched in a live window; and Flow's pipe
  for images answering the colors OSC 4, 10, 11 and 12 ask for as the
  terminal does, in unit tests (bytes for a pipe, and the SDK reading what
  it answers), not built for Windows or run there
- in the tree, for 0.1.29, on this Mac: the small **Copy** at the end of a
  line of nothing but backticks and on a command's card, in a snapshot of a
  made-up conversation, and which lines and cards have it, and a reply's
  last line having it only once it's whole, in unit tests (not clicked in a
  live window); **Efficient on battery**: its row and the pill's **Efficient
  · battery**, in a snapshot made to think it was on battery, the Mac's
  power read (on a MacBook plugged in, not tried unplugged or in Low Power
  Mode), Linux's supplies, Windows' status and the setting's rules in unit
  tests (Windows' and Linux's readers not built on their own systems); the
  Graphics workstation's logo held still and turning on from where it
  stopped, in unit tests, not looked at in a live window; Font size's
  readout and **Fit window to** in Settings, in snapshots in English and
  Japanese (the Workstation's 80 × 34 first), and the window's size for each
  grid, with and without the panel and a border, and the text size a grid
  too small for the least window needs, in unit tests (fitting the live
  window, a maximized one, and keeping it in the screen's room, not tried
  by hand)
- box drawing's lines (U+2500–U+257F) and the squares (■ □, ◼ ◻, ◾ ◽,
  ▪ ▫) drawn by Flow itself: every set of arms, strokes meeting the next
  cell's at the same pixels, arcs meeting their straights, nothing past
  the cell, white squares the black ones' outlines, every square in a row
  of cells a fraction of a pixel wide the same size, cells' shared edges
  one number, and the light stroke from the font's stems (VT323's and
  Gallant's measured), in unit tests; a pattern of them, progress bars and
  checkboxes, Flow Mines 0.1.6 and DEC's double lines drawn offscreen in
  the Graphics workstation's, the Workstation's, the Green screen's and
  Flow's own looks at sizes 15 and 22, and under the CRT; not looked at
  in a live window, nor built for Windows or Linux
- a Flow.app started in Efficient mode drawing its first frame as soon as
  macOS shows the window (0.1.28's stayed empty, but for the window's
  buttons, until a click): three launches each, before and after, timed in
  logs, every frame before the window was on screen turned down and the
  one after it shown, then none for four seconds; not looked at, nor built
  for Windows or Linux
- what goes to the shell written by a thread of each tab's own, so a
  program that turns on the mouse and stops reading no longer freezes
  Flow: in unit tests, one with a writer that waits until let go (every
  send returning at once, motion past its room dropped, keys kept in
  order) and one with a real pty whose raw program never reads (it fills,
  and the sends still return at once), and zsh and bash in a real pty as
  before; not built for Windows (ConPTY) or Linux
- a new tab's shell and theme from the **…** after the +: the picker and a
  tab in the Teletype's theme of its own in a snapshot (`new-tab.png`), the
  window wearing it while that tab is in front and Flow's own once it
  closed, and every theme taken off again before saving in a unit test;
  not tried by hand in a live window, nor built for Windows or Linux
- what each tab uses: the card under a tab and About's Resources in a
  snapshot (`tab-card.png`, `resources.png`), this Mac's processes read
  (a child found with its name, memory and CPU) and Linux's `/proc` from
  made-up files in unit tests, and the CPU's shares between two readings;
  Windows' and Linux's readers not built or run on their own systems
- the workstations' numbers the same on one computer and, switched on,
  its own: this Mac's id, serial and address read (their shape only),
  Linux's from made-up files and SMBIOS's serial from a made-up table, the
  seed's hash and the one kept where there's no id, and both start
  screens with real numbers, in unit tests; Windows' readers written
  against windows-sys's declarations, not built or run there
- `flow-settings`: the rules for what a program sees and changes (closed
  keys, wrong kinds, set, reset and reset all) and OSC 7732 scanned and
  answered down the pipe, in unit tests; the program itself in a pty
  answered as Flow would (its questions, what it prints and its exit
  codes, and nothing answering), and since its output went to the tab
  itself, with its output sent to a file, piped, captured by `$(…)` and its
  input from `/dev/null`, the tab's modes back after, and nothing asked
  with all three streams taken; not run in a live Flow window, nor built
  for Windows or Linux
- Flow's settings from the SDK's C library and Python package: the
  request's bytes and the answers read back, from C built with `cc` and
  from Python, in unit tests, with and without audio, nothing asked and
  nothing waited for outside a terminal; Python's `get`, `set` and `list`
  and `settings.c` in a PTY answered as Flow would, raw mode taken for the
  wait and put back (kept where the program had it on), a refusal's reason
  and a terminal that answers nothing, and an answer too late for its wait
  thrown away rather than taken for the next request's (a test in a PTY);
  the DA1 that ends a settings question down Windows' pipe, against Flow's
  pipe server on the Mac; not in a live Flow window, nor on Windows itself
- a click on the Graphics workstation's cube: its turn back the shorter
  way, its five seconds' rest and turning on from the logo's stance (held
  still in efficient mode, standing so at once), and finding the logo
  under a cell but not a program's model, in unit tests; not clicked in a
  live window. After 0.1.31, which stood it upside down, it stands the
  right way up: the same turned half round in the picture, in a unit test
  and drawn on this Mac's GPU
- **Save as theme…**: saved themes written to `config.toml` and read back
  (and a config with none), saved, saved over by name and deleted, worn in
  Settings and by a tab of its own and taken off whole before saving, your
  own theme left as it is by choosing one and typing in its rows, a preset
  file saved while one is worn holding its start screen, its muted sounds
  kept apart from a name that's its own and more, and `flow-settings`
  neither seeing nor choosing them, their muted sounds included, in unit
  tests; offscreen, the naming box over Settings saying it saves over a
  name taken, a saved theme chosen in the Theme row with its start screen
  and prompt, and the new-tab picker with more saved themes than the
  window has room for, cut to fit around the last, chosen, with its bar
  and its **Save as theme…**; not clicked, scrolled or typed in a live
  window, nor the Mac's View menu item tried, nor built for Windows or
  Linux
- **Export…** (a saved theme as a theme package): one saved from an old
  machine, from Flow's look in the rain and from your own with a tube of
  no preset's, its own fonts and a start screen, each exported, read back
  and worn as it was saved; ids made from names, never a built-in one's;
  packed owned by no one and installed by `flowapp`'s own code in a
  scratch folder; and the SDK taking what's exported, in unit tests on
  macOS; offscreen, the row's three buttons; its save panel not clicked in
  a live window, nor packed on Windows or Linux
- tabs opened again (0.1.31): `tabs.json` written and read back (and one
  with less in it), each tab's shell found by its id and label, in unit tests; in a live
  window with a home of its own, a start opening a saved bash tab in the
  Green screen in its folder (its prompt the theme's) and a zsh tab in
  another, `flow-settings restart` from the first opening both again as
  they were, and the file gone once both shells exited; quitting with tabs
  open, which writes it, not tried live
- the glyph atlas keeping each font: a font switched back to drawing its
  glyphs from where they were, after a size and back and after the atlas
  grew, and another's cell measured without drawing in it, on this Mac's
  GPU (an ignored test); each tab behind keeping its own theme's grid, in
  the snapshots' themed tabs; not timed by eye in a live window
- a tab's card from the keys, built and its timing read in the code; not
  pressed in a live window
- the frame's time on the GPU: timestamps asked for and read back on this
  Mac's GPU (1.5 ms a frame in the snapshot's About), their spans added up
  in unit tests; a 3D model's mesh and drawing counted in its tab's GPU
  memory, built but not looked at with a model placed
- each theme's own cursor speed (0.1.31), on the slider's steps, worn by
  a tab of its own and taken off before saving, in unit tests; not watched
  gliding in a live window
- Settings on the tab in front, after 0.1.31.1: a tab's whole look taken
  off one config and put on another, a change split into the tab's look
  and Flow's settings, and `tabs.json` keeping a tab's own look, in unit
  tests; in snapshots, the row of tabs under the title with **New tabs**,
  the sections saying **this tab** or **all tabs**, and the machines' tabs
  wearing their own looks; not clicked through or keyed in a live window
- the Flow Instruments Home Computer's own letters and its "press any key"
  (for 0.1.32): the font's glyphs, metrics and the config from before
  taking it, and a start screen waiting for a key with its cursor hidden,
  going on after it and stopping when its tab closes, in unit tests; its
  title screen waiting, in its letters, in the snapshot; no key pressed in
  a live window
- rain from a drizzle to a downpour (for 0.1.32): the rate, a drop's
  strike and spray, a heavy one bursting, taps thinning out, rivulets,
  paths fogging over sooner and the drops kept in bounds, and the hiss
  growing with the rain, in unit tests; ten seconds of downpour in about
  a millisecond of the CPU in a release build; its splats, rivulets and
  sheets in the snapshot and a recording; the hiss never listened to, and
  nothing tried without a GPU
- the Flow notanAmiga CLI theme (for 0.1.33): its start screen (the
  white page, the Kickstart's picture in its four colors and as tall as each
  screen has room for, the wait for its key, the copyright beside the Boing
  ball), its prompt as each shell writes it, its colors' contrast on its
  blue, its font's glyphs and metrics, the ball's checks and its spin about
  its leaning axis, and the empty drive's two knocks, in unit tests; the
  page waiting for its disk and the shell after the key in snapshots, and
  worn in a live window of a test build; the drive's clicks and seek not
  checked by ear; on Linux, its unit tests and the shell after the key in
  a snapshot, in the release built against glibc 2.31; on Windows, see
  above
- `flowapp`'s tables (for 0.1.33): `list`, `outdated`, `update`, `search`
  and `info` in English and Japanese at 70 to 100 columns, in a pty, piped
  and under `NO_COLOR`, and each listing's `--json`, against a test
  catalog in a scratch home; install and upgrade of the real downloads, a
  spoiled checksum and a missing download; `flowapp list` in a Flow tab of
  a test build; on Windows, see above; on Linux, `list` ruled in a pty
  and plain piped, and `info`, from the release against the live catalog
- links programs write (OSC 8, for 0.1.33): the underline and the pill
  under the pointer in a snapshot; one printed with `printf` and opened
  with ⌘-click in a live window of a test build; Ctrl+click on Windows and
  Linux, through `explorer.exe` and `xdg-open`, not tried (built on Linux
  for its 0.1.33; on Windows, see above)
- an update keeping the games an older Flow carried (for 0.1.34):
  `apps::keep_carried` installing Flow Rogue and Flow Mines once and
  nothing else, offline trying again, a game installed already left be,
  and a download failing its checks keeping nothing, in unit tests; a
  release build started over a scratch home with a config (offline: noted
  for the next start) and without one (nothing to keep); the first start
  of a test build (build 400) over a real config, installing Flow Mines
  0.1.6 and Flow Rogue 0.1.46 from the live catalog beside a Flow Invaders
  installed already; not tried on Windows or Linux
- Flow carrying no game or demo (for 0.1.34): an ad hoc release
  (`release.sh --adhoc`) whose `Contents/Helpers` hold `flow-help`,
  `flow-settings` and `flowapp` alone, signed, its notices naming no game's
  crates, and its `flowapp` listing every app as not installed against the
  live catalog; `install.sh` over an older Flow's folder in a scratch
  prefix, taking `flow-rogue`, `flow-mines` and `flow-demo` away and
  leaving a program of the user's own; Windows' `release.ps1`, its
  notices and its setup's `[InstallDelete]` (over 0.1.33's setup in a
  scratch folder) for Windows 0.1.34; an update from 0.1.33 through
  Sparkle or WinSparkle not tried
- the green and amber screens' phosphor (for 0.1.34): snapshots of a
  green screen printing the 256 colors, a truecolor ramp and blue text, the
  liquid, the assistant's panel, Settings and a 3D model all in its green,
  a tab in the Flow Instruments theme beside it in color, and an amber
  phosphor picked over Flow's own look; the mapping, the Phosphor row, a
  saved theme keeping it and a config from before it in unit tests; not
  tried in a live window, nor on Windows or Linux
- the hum and the fan drifting (for 0.1.34): the drift's bounds, a hum
  laid on the last in step on the mixer's clock, and a voice taking that
  clock however late it comes, in unit tests; a minute of each written by
  `examples/sounds` and measured (the level wandering 0.85 to 1.15, no
  dip where one is laid on the last); not listened to here
- split panes (for 0.1.34): the split tree's layout, closing and moving
  between panes, `tabs.json` with a split tab and one from before, and a
  pane's layout, in unit tests; a snapshot of a tab split in two
  (`split.png`); not tried in a live window (the keys, the mouse, a shell
  exiting in a pane, reopening at a start), nor on Windows or Linux
- find in the scrollback, the command palette and Ask why (for 0.1.34):
  each in unit tests and a snapshot (`find.png`, `palette.png`, and Ask
  why's pill over a made-up failure), and Ask why's capture in a real zsh
  and bash in a pty (`ls` of a folder that isn't there, typed as a person
  does, kept with its output, gone when the next command starts); not
  tried in a live window, nor on Windows or Linux
- `flowapp first` and `last` (for 0.1.34): in a real zsh and bash in a
  pty, the first folder leading the PATH after the startup files, and a
  stand-in `cal` put there while the tab is open taking the place of
  `/usr/bin/cal` at its next prompt, and taken out, giving it back;
  linking, following an upgrade, unlinking and an app's removal in unit
  tests; `first`, `last` and `last --all` in a scratch home; PowerShell's
  aliases, BusyBox and Windows' hard links not tried (not built on Windows)
- finer `flowapp first` (for 0.1.35): the record of choices, a choice in
  some shells taking an earlier one's place there alone, the takes by
  default and asked, each pane's folder (its links, `.prompt/`, `.take`,
  its own `--here` choices kept when the others are written afresh), an
  upgrade and a removal followed, an older Flow's `first/` brought in, the
  sweep of a gone Flow's folders, what still comes ahead by each shell's
  tables, and `flowapp first`'s table and JSON, in unit tests; in a real
  zsh, Homebrew's bash 5.3 and macOS's bash 3.2 in a pty: the pane's folder
  leading the PATH, a stand-in `cal` put first while the tab is open (and
  in a script), in the other shell alone, at the prompt alone (a script
  keeping `/usr/bin/cal`), behind a user's alias until `--take alias` and
  the alias given back, the builtin `pwd` put aside and given back, in one
  tab alone, zsh's `which` put aside, and the folder gone with the tab; the
  bash hook by hand under `set -u` with `failglob` and `nullglob`; `flowapp
  first` (each option, its refusals, the table, `--json`), `--here` and
  `last` in a scratch home; PowerShell, cmd, BusyBox and Git Bash written
  in `windows/tests/first.rs` but not run (not built on Windows); not
  tried in a live window, nor on Linux
- zsh's `which` handed to one put first (after 0.1.34): in a real zsh and
  bash in a pty, a stand-in `which` put first answering `which cksum`, zsh
  keeping an alias's `which` and its own `-p` to itself, and both shells'
  own again once it's taken out; with Flow-Utils' `which` in zsh in a pty
  that says it's Flow; zsh on Linux not tried
- themes as packages (for 0.1.34): `flow-core-themes` written, packed and
  read back as the built-in looks it holds; a packaged theme in a built-in
  one's place and one of its own id after them, named in English and
  Japanese, worn with its screen and its more of the look's settings, and
  packages with a bad id, color, file or CRT refused, in unit tests;
  `flowapp install` of a package of one's own from its file, `list` and
  `list --json` in a scratch home; not tried in a live window (snapshots
  wear no packages), nor published to the catalog, nor on Windows or Linux
- Your sizes for the Fit rows (after 0.1.36): sizes typed however
  they're written read back, kept within 10 × 3 to 500 × 300 and each
  once, and each a choice in Fit window to and Fit text to, in unit
  tests, with the note naming what was left out; in a snapshot
  (`settings-font.png`); and in a live window, sizes typed, kept and
  fitted to
- recording the terminal (after 0.1.36): frames written to an animated
  WebP and read back with each frame's time, a pause kept as one long
  frame, a Retina frame shrunk to its points, one of another size fitted
  in the middle, straight alpha from the GPU's premultiplied BGRA, the
  pacing, a prompt's user and computer names found and the file's name, in
  unit tests; and a snapshot's split tab recorded for three seconds
  through the same capture and encoder, drawn offscreen (`recording.webp`:
  the terminal area alone, the green screen's tube, the command typed and
  its output, looping), with libwebp's notices in the Mac's
  THIRD-PARTY-NOTICES; and in a live window, btop recorded, stopped from
  the title bar's clock and with `⇧⌘R`, saved through the save panel's
  sheet and played in Chrome, at Balanced and at Sharp with the liquid
  behind it clean, and its countdown's note gone before the first frame
- scrolling back (after 0.1.38): the scrollbar's layout, a drag giving
  back the offset it was drawn at, a press on the track jumping the
  thumb's middle there, its hold and fade, and the pill as a screen
  reader's button, in unit tests; and a snapshot's green pane scrolled
  back 30 lines through a count (`scrolled.png`: the pill saying
  **↓ 30 lines below** left of a thin thumb, the pane beside it, with no
  scrollback, showing none). Tried by hand in a test Flow.app (build
  567, 2026-11-13): all good
- The find bar's matches marked on the scrollbar (after 0.1.38): a mark's
  place, and a press on it bringing its line into view, in unit tests;
  and a snapshot's `find-marks.png` (7 found through a count to 120, its
  marks down the green pane's bar, the one chosen taller). Tried by hand in a test Flow.app (build 579, 2026-11-14): works
- A window frame's close box, and a split tab's ✕ on the pane under the
  pointer (after 0.1.38): their places, and a reader's button closing
  each pane whether the ✕ shows or not, in unit tests; seen in a
  snapshot's `theme-amiga.png` and `scrolled.png` (the ✕ right of the
  pill). The pane's ✕ tried by hand in a test Flow.app (build 575,
  2026-11-13): worked, and the frame's close box in build 579
  (2026-11-14): works
- Searching Settings (after 0.1.38): the words each row is found by
  (its name, hint, choices and config key, in both languages), a name
  before a hint, Japanese by a run of its letters, the words lit, and the
  field as a reader's search box, in unit tests; and a snapshot's
  `settings-search.png` ("glow" finding Glow first, then rows by their
  hints). Tried by hand in a test Flow.app (build 579, 2026-11-14): works
- Flow's keys (after 0.1.38): every shortcut in one table, the defaults
  held to the keys Flow had on the Mac and elsewhere and no two sharing
  one, matching by the layout (⇧⌘= and a German ⌘+ both Bigger, ⇧⌘ß
  Help), refusals, swaps and takes, `[keys]` kept only where changed and
  read past a key it can't, Settings' Keys rows, the Keys help page and
  the menus' key equivalents, in unit tests; `flow-help keys`; and a
  snapshot's `keys.png` and `settings-keys.png` (a key changed, its
  default beside it, another waiting). Tried by hand in a test Flow.app (build 579, 2026-11-14): works
- Toward 0.1.40: the command palette finding a setting by its hint, its
  choices and its key; kitty's `d=f` and `d=F` deleting an animation's
  frames; Settings' Add Key and the Hotkey taking a command's key (the
  system's refused); and each command marked on the scrollbar where its
  prompt was (done, failed, running), Previous and Next Command going from
  one to the next: in unit tests (the marks scrolling with their lines into
  the scrollback and off its top, a two-line prompt marked on its first,
  the alternate screen marking nothing), and a snapshot's
  `command-marks.png`. Tried by hand in a test Flow.app (build 589,
  2026-11-15): the marks, the jumps (a full-screen program keeping the
  keys), the palette, Add Key and the Hotkey all perfect; kitty's frame
  deletion not tried by hand
- The graphics API's logo in the startup logo's banner (toward 0.1.40):
  each logo fitted to one row in whole cells, unstretched, and put after
  the line, the name alone without room, in unit tests; Metal's seen in a
  test Flow.app (build 591, 2026-11-15): looks good, then its icon
  without the word after it, as Linux shows Vulkan's
- Caps Lock's notice (after 0.1.39): the guess from a letter typed, the
  notice coming with each key and going after, or once Caps Lock is off,
  and the screen reader's alert, in unit tests; and a snapshot's
  `caps-lock.png`. Tried by hand in a test Flow.app (build 585,
  2026-11-15): works
- The `flow` command (toward 0.1.43): its options, the request, each
  pane's key and every shell's quoting in unit tests, and odd words typed
  into zsh and bash in a real PTY and given back as they were. In a test
  Flow.app (build 662, 2026-11-16) in a scratch home, driven through its
  own `flow-mcp`: a tab like the asking one, `--split`,
  `--behind`, `--close`, `--shell`, `--theme`, start screens that wait for
  a key, requests without the pane's own key refused (one `cat`ed from a
  file among them), and `--window` from a tab and `flow` from outside
  Flow starting a guest that saved no tabs

**Tested on Linux (Ubuntu 26.04, x86-64, GNOME on Wayland):**
- build, unit tests and clippy, with nothing installed but Rust and a C
  compiler
- command capture with exit codes, the command history's notes and a
  machine's prompt in zsh and bash in a real PTY, zsh with a `prompt` theme
  set in a `.zshrc`; and prompt themes that set the prompt before each one,
  in both, with nounset on, and bash's `PROMPT_COMMAND` as a string and as
  an array
- offscreen rendering on Vulkan: every liquid style, the CRT, the dialogs,
  Settings, a model and both snapshot themes, with the computer's memory in
  their start screens; and on OpenGL (Mesa's), on Wayland and X11, with
  opaque glass, and with Mesa made to offer OpenGL 4.1 and ES 3.0 without
  compute shaders (as VirtualBox's OpenGL does, where 0.1.27 couldn't
  start): every snapshot, and the GPU test of models, drawn as on Vulkan
- the live window starting on Wayland and on X11 (XWayland) with nothing
  amiss in the log
- the release tarball: made, installed into a prefix and uninstalled, its
  menu entry checked with `desktop-file-validate`, and the games found on
  the PATH of the installed Flow's shells
- the computer, the languages and the distribution's logo read from the
  system, in unit tests
- the release built against glibc 2.31 with zig (every program needing 2.30
  at most), drawing every snapshot, and the .deb: its files, owners and
  modes, apt resolving it (`apt-get install -s`), and its Flow run unpacked;
  and a .deb of 0.1.23, installed by dpkg, whose Install Update opened
  the release page
- Flow's own updater: the appcast read, and against a local server with a
  test key, a signed tarball installed over a Flow in a scratch prefix and a
  tampered one, one signed by another key and one whose `flow` isn't the
  feed's build turned away, in unit tests; the installed Flow starting its
  updater and its daily check finding a newer build on a local feed, live;
  and the About card, the title bar's sign, the question about checking
  automatically and the Settings rows, offscreen in a build that pretends
  there's an update
- 0.1.23, 0.1.25, 0.1.27, 0.1.29, 0.1.30, 0.1.31.1, 0.1.31.3, 0.1.32, 0.1.33, 0.1.34 and then 0.1.35, installed from the
  published feed into a scratch folder through the updater's own code; and a .deb
  of 0.1.25 upgraded to 0.1.27 by `apt install` from the release's download
- what 0.1.25 brought from the Mac and Windows: the workspace's tests (the
  SDK's C library built with `cc` and its Python package's tests among
  them), bash 5.3 sending a command continued over two lines whole, the GPU
  test of glTF materials, and snapshots of the School computer's teletext
  page, the Personal computer, your own theme's rows, the About cube and
  the gem, from the release built against glibc 2.31
- sound through ALSA (since 0.1.27): Flow's bell and a splash, the SDK's
  tune, and Flow Rogue's music from the release built against glibc 2.31,
  recorded from PipeWire's output as they played; and with no sound device
  (ALSA without PipeWire's settings, a card that isn't there), silence
  with nothing printed
- a .deb's update by hand (since 0.1.27): the release's .deb
  named as `package.sh` names it, and the `sudo apt install` command for it
  in the user's downloads folder, quoted where the path needs it, each run
  through `sh` in unit tests to check it names the downloaded file; and
  About with the command and **Copy Command**, offscreen, with a short and a
  long localized downloads folder
- what 0.1.26 and 0.1.27 brought from the Mac and Windows: the workspace's
  tests (core's double-line resize tests, which Windows leaves out, and the
  SDK's C library and Python package, with and without audio, among them),
  the GPU test of the models' new materials, lights, cameras and compressed
  glTF, and snapshots of help with Linux's keys, Settings in sections with
  Restart, About's Restart Flow and Legal, the teletype's font and the
  Personal computer's boot, from the release built against glibc 2.31; and
  a running program's file replaced as install.sh and apt replace Flow's,
  which Restart then finds, in unit tests
- what 0.1.28 and 0.1.29 brought from the Mac and Windows, in the release
  built against glibc 2.31 (made from the commit Windows' 0.1.29 came to):
  the workspace's tests, among them the Workstation's readers of udev's
  database and sysfs and the power reader (this computer has no battery,
  so plugged in); snapshots on Vulkan and on OpenGL without compute
  shaders of the assistant's Copy and Run, Theme details, Font size's
  readout and Fit window to, Efficient on battery's row and the machines'
  colors; box drawing and the squares joined in DejaVu Sans Mono; and
  `flow-help` on the installed Flow's PATH, printing its pages
- what 0.1.30 brought from the Mac and Windows, in the release built
  against glibc 2.31: the workspace's tests (Flow Rogue 0.1.30's among
  them, and the SDK's C library without audio); `flow-settings` in a
  snapshot's bash tab, a value read, a key Flow lacks and a closed one
  refused, and its answer reaching it redirected to a file, piped,
  captured by `$(…)` and through Python's `flow_sdk.settings`, and from
  the installed release on its tabs' PATH; `flow-demo model` in a pty
  answering as Flow does, its file and folder deleted on a hang-up, on
  SIGTERM and on `q`, and a killed run's swept by the next; and snapshots
  of Save as theme, the new-tab picker, a tab's card and About's
  Resources, on Vulkan and on OpenGL without compute shaders
- what 0.1.31 and 0.1.31.1 brought from the Mac, in the release built
  against glibc 2.31: the workspace's tests (Flow Rogue 0.1.45's among
  them) and the GPU tests of the atlas keeping each font, SGI's cube at
  rest and the models, on Vulkan and on OpenGL without compute shaders;
  tabs reopened in a live window on X11, quit as the window manager
  closes a window and started again in the folder they were quit in, and
  not with Reopen tabs at start off; About's Resources giving the GPU's
  time per frame in snapshots; and the release tag and files named by
  the patch's number, About's Release notes link with them
- what 0.1.31.2 and 0.1.31.3 brought from Windows and the Mac, in the
  release built against glibc 2.31: the workspace's tests, and snapshots
  on Vulkan and on OpenGL without compute shaders of the Flow
  Instruments Home Computer's title screen and of Settings' row of tabs
  with each section's scope
- what 0.1.32 brought from the Mac, in the release built against glibc
  2.31: the workspace's tests (Flow Rogue 0.1.46's, Flow Mines' and Flow
  Invaders' among them), with the SDK and the demos from Flow-SDK's tag;
  the programs naming no home folder, account or repo; `flowapp` against
  the live catalog, listing the games Flow carries as installed and
  saying there's no Linux download of the rest; and snapshots on Vulkan
  and on OpenGL without compute shaders of the rain's streams, the Flow
  Instruments Home Computer's own letters with its title screen waiting
  for a key, and Fit text to
- what 0.1.33 brought from the Mac, in the release built against glibc
  2.31: the workspace's tests, with the games from Flow-Games' tag and
  the SDK from Flow-SDK's; the programs naming no home folder, account or
  repo; `flowapp`'s ruled table in a pty and its plain columns piped,
  against the live catalog; and snapshots on Vulkan of the Flow
  notanAmiga CLI's shell beside the Boing ball and the new-tab picker
  listing it
- what 0.1.34 brought from the Mac and Windows: the workspace's tests;
  in a live window on X11 (XWayland, under GNOME on Wayland), a long
  command's desktop notification (a zsh line that slept 11 s and failed,
  with Notify when done at 10 s, while Flow wasn't in front), asked for
  through gdbus under the tab's name with how it ended, which took a
  `--` before gdbus's arguments; the F12 hotkey grabbed on X11's root
  window and, pressed through XTEST, dropping the window over the top
  half of the screen, above the others; GNOME declined to give it the
  keys, as it does any X11 window's ask while a Wayland one has them
  (the pager's ask Flow now sends after winit's reached the root too),
  so a second press dropped it down again rather than putting it away;
  and Ask why's pill after a command that failed
- 0.1.34's release, built against glibc 2.31: no game or demo in the
  tarball or the .deb, and none of their crates in the notices; its
  `install.sh` over 0.1.33's in a scratch prefix taking `flow-rogue`,
  `flow-mines` and `flow-demo` away and leaving a program of the user's
  own; its first start over a config from before, in a scratch home,
  installing Flow Rogue 0.1.46 and Flow Mines 0.1.6 from the live
  catalog and noting it in `carried.json`; snapshots on Vulkan of split
  panes, the find bar, the command palette with Linux's keys and the
  green screen's phosphor; and apt taking the .deb as an upgrade of an
  installed 0.1.33
- `flowapp`'s Linux downloads, made by `linux/scripts/app-release.sh`
  for glibc 2.31: the released `flowapp` 0.1.33 installing Flow Rogue,
  Flow Mines, Flow Invaders and `flow-sdk` from the releases into a
  scratch home, each checked against its signature; each program's
  `--version`; the SDK's Python package loading its own library from the
  install; and a C program built against its header and run, with the
  shared library and with the static one; and Flow Invaders 0.1.21
  upgraded over 0.1.20 by `flowapp upgrade`; the
  games not played from them in a Flow tab, nor the SDK's audio heard
- what 0.1.35 brought from the Mac: the workspace's tests (one of core's
  had counted on a folder listing names in order, as the Mac's APFS does
  and ext4 doesn't); zsh 5.9 and bash 5.3 in a real PTY putting ahead
  what `flowapp first` chose, as the Mac's test has them (in some shells,
  at the prompt alone, an alias or a builtin taken away, in one tab, zsh's
  `which`), each in a folder of its own that goes with its tab; in a live
  window on X11 (XWayland, under GNOME on Wayland), the F12 hotkey
  dropping the window over the top third of the screen, above the others,
  and, with the keys given it by hand, sliding it up and putting it away
  out of sight; the hotkey let go and taken again as `flow-settings`
  cleared it and set F12 again; GNOME held the window below its top bar
  throughout, so the slide's steps (one every 8 ms, from above the screen
  to its top and back) didn't show there, and drew it about once a second
  while it hadn't the keys; Flow Mines 0.1.6's download made by its own
  repository's script (glibc 2.30 at most, nothing naming the build's
  folders); and 0.1.35's release, built against glibc 2.31:
  its `install.sh` into a scratch prefix and out again, its programs
  saying 0.1.35, and apt taking the .deb as an upgrade of an installed
  0.1.34
- what 0.1.36 brought from the Mac: the workspace's tests, and the GPU
  tests drawing the models' new files (Draco, KTX2's ETC1S and UASTC,
  meshopt's version 1, specular-glossiness); in a live window on Wayland,
  a config at 0.1.35's 55% glass made opaque at the first start, the Flow
  notanAmiga CLI's frame on a split tab of two panes, the theme gallery's
  preview in a theme's own font and logo, a plain `https://` address
  opened with Ctrl+click, a moving GIF and WebP sent as iTerm2's and
  kitty's pictures, and Orca reading the terminal and Settings; and
  0.1.36's release, built against glibc 2.31: its `install.sh` into a
  scratch prefix and out again, apt taking the .deb as an upgrade of an
  installed 0.1.35, and the published feed installing it as an update would
- recording the terminal (after 0.1.36): libwebp built by zigbuild for
  glibc 2.31; a snapshot's split tab recorded offscreen through Vulkan's
  BGRA surface; and in a live window on Wayland, with `Ctrl+Shift+Alt+V`
  (GNOME keeps `Ctrl+Shift+Alt+R` for its own recorder), the countdown,
  the clock, stopping from it and from the keys, the save panel, the
  files played in a browser, Balanced and Sharp, and a resize while
  recording
- Your sizes for the Fit rows (after 0.1.36): in a live window on
  Wayland, sizes typed and kept, each fitted to from Fit window to and Fit
  text to, something messy tidied, Esc putting back what was there, and
  the note over Settings saying what was left out
- 0.1.37's release, built against glibc 2.31: its `install.sh` into a
  scratch prefix and out again, apt taking the .deb as an upgrade of an
  installed 0.1.36, and the published feed installing it as an update would
- 0.1.37.1's release, the same way, with the .deb's signature in the
  feed for the first time: the published feed installing the tarball and
  its .deb fetched and checked with Flow's key, as a .deb's Flow takes it
- a .deb's Flow updating itself (0.1.37.1 to 0.1.37.4): About's Install
  Update downloading the next .deb and checking it with Flow's key, apt
  run in its own tab with the password typed there, and, from 0.1.37.3,
  the card asking to restart as apt finished, Restart Now bringing up the
  new Flow with the tabs
- no white on a phosphor (after 0.1.37): the workspace's tests; a
  snapshot's green screen on Vulkan, nothing in it past its phosphor; and
  in a live window on Wayland, the green and amber screens in shades of
  their phosphor and never white, a BBS's welcome over `ssh` among them
  (its picture of a logo and its menu's pale bar)
- what's toward 0.1.38 from the Mac (after 0.1.37): the workspace's
  tests; the GPU test drawing vertex colors and GPU instancing on Vulkan,
  its pictures looked at; and in a live window on Wayland, a recording at
  Balanced and Every pixel of a 1650 × 990 window through a tube keeping
  23.6 frames a second, evenly spaced (it kept about 12 before), no
  grille on the green and amber screens, BMP and TIFF sent as iTerm2's
  pictures, and a GIF sent with kitty's own animation commands as `kitten
  icat` sends one, playing, About's Resources counting one frame's worth
- what's toward 0.1.39 from the Mac (after 0.1.38): the workspace's
  tests; release snapshots on Vulkan in English and Japanese (Flow's keys
  as written off the Mac, Settings' Keys and its search, the scrolled
  pane's pill and ✕, the find bar's marks, and About's links on a line of
  their own inside the card in both languages); and in a live window on
  Wayland, every default key (Ctrl+Shift and a letter, the tabs, the
  splits, F3 and Shift+F3 with the find bar open and F3 reaching a
  program with it closed, `Ctrl+Alt+Shift+V` recording, Ctrl+Shift+End
  and Home), Ctrl+C copying only with a selection, Ctrl+Insert,
  Shift+Insert, Ctrl+V and the primary selection's middle click, Settings'
  Keys (a key changed and reset, Swap and Take It, Super refused,
  Backspace leaving none, Ctrl+L taken with its toast, Reset All),
  searching Settings, the scrollbar in its three modes and the pill, the
  find bar's marks, the pane's ✕ and the notanAmiga's close box, About,
  and Orca reading the search box, the close buttons and the pill. GNOME
  keeps Ctrl+Alt and the arrows for its workspaces and Alt+Tab for its
  windows, so Move Between Panes' default never reaches Flow there;
  Settings' Keys can give it another
- what's toward 0.1.40 from the Mac (after 0.1.39): the workspace's
  tests, and in live windows on Wayland and on X11 (XWayland) side by
  side: Caps Lock's notice (on X11 the system's answer, so a digit brings
  it; on Wayland the guess, waiting for a letter), each command marked on
  the scrollbar with Previous and Next Command, Settings' Add Key, the
  palette finding Phosphor from "amber", and About's version line naming
  Wayland or X11 with its logo. A hotkey there put the window away for
  good under XWayland, which hears keys only while an X11 window has them,
  so from 0.1.40 Flow has none on Wayland or XWayland, and Settings says so

**Not tested yet:**
- searching Settings in Japanese through an input method on Linux (IBus
  with Mozc, Fcitx): the Ubuntu computer has none installed
- the PC's fan steady (toward 0.1.39) heard in a live window on Linux
- screen readers in a live window: VoiceOver on the Mac (Narrator on
  Windows and Orca on Linux read the terminal and Settings for 0.1.36)
- recording the terminal: on the Mac, a resize while recording, a cancel,
  Every pixel and Lossless; on Linux, Every pixel, Lossless, an X11
  session and other desktops
- 0.1.34's notification and drop-down hotkey on Windows (the taskbar
  button flashing, `RegisterHotKey`; built, not yet tried in a live
  window); on Linux, the notification through notify-send or dbus-send
  (gdbus answers on GNOME), the hotkey on an X11 session (Xorg) and other
  window managers, and whether they give the keys to the window it brings
  forward
- 0.1.35's drop-down on Windows (built on the Mac only): the slide,
  putting away when Flow loses the keys, the heights and a key pressed in
  Settings; on Linux, on an X11 session (Xorg) and under window managers
  other than GNOME's: the slide seen, putting away when Flow loses the
  keys (under XWayland, GNOME gives an X11 window no keys while a Wayland
  one has them, so with it on a window dropped down goes away as it
  lands), and a key pressed in Settings
- the SDK's C library and Python package on Windows (its pipe, a C
  compiler's flags there), and in a live window
- on Linux: the live window looked at (the glass over the desktop, moving,
  resizing and maximizing, the title bar's menu), middle-click paste with
  other programs, an API key kept in GNOME Keyring, typing through IBus or
  Fcitx, KDE Plasma's blur, other distributions and desktops, Arm64, NVIDIA's
  driver, drawing on the CPU, VirtualBox itself (Ubuntu 26.10 in 7.2.22,
  where 0.1.27 couldn't start) and the games in a live window; a running
  0.1.25, 0.1.27, 0.1.29, 0.1.30, 0.1.31.1, 0.1.31.3 or 0.1.32 updating
  itself through About; moving Settings between tabs, a tab's own look kept after
  quitting, the Flow Instruments theme's beep and honk and a key pressed
  at its title screen, the rain's hiss and thunder; the Flow notanAmiga
  CLI's drive heard;
  Ctrl+Shift+B restarting one tab and the tab card from the keys; the release on a
  Linux older than Ubuntu 26.04; on a Flow installed from the .deb, About
  naming the new .deb and its `sudo apt install` command (whose button
  copies it), which 0.1.27 is the first to do; Restart
  Flow, and after an update starting the new Flow; help's pages and keys
  (F1, and Ctrl+Shift+? on layouts where ? isn't over /), Ctrl+Insert, the
  chip maker's logo (set from DejaVu Sans or Liberation Sans), the
  Workstation's and Graphics workstation's boots, the fonts built in since
  0.1.25 and the machines' sounds through ALSA in a live window; and Flow
  Rogue 0.1.23 and Flow Mines 0.1.5 played there
- a real conversation with GPT or Gemini (so far only mock servers and their
  APIs' key check)
- images in the live window, and with yazi, ranger, Neovim's image plugins or
  tmux
- Sixel and iTerm2 pictures from `lsix`, gnuplot, chafa, yazi and `imgcat`,
  an iTerm2 JPEG, a WebP, DECSDM from a program and a sixel through tmux
  in a live window, and on Linux (where a moving GIF and WebP sent as
  iTerm2's and kitty's played in a live window); on Windows, all
  but a moving GIF and WebP sent as iTerm2's, which played in a live
  window with the next line below them (ConPTY drops a sixel's DCS, by
  script in `windows/tests/pictures.rs`)
- kitty's animation commands from `kitten icat` (with a GIF) in a live
  window on any system, and down the pipe from a program on Windows
- Flow Mines in a live Flow window on Windows (there so far through ConPTY,
  by script), and in a release on macOS; Flow Rogue's music through WASAPI
- mouse reports in a live window (clicks, drags, the wheel, Shift to select)
- the command history in a live window
- 3D models in a live window (an animated one, the question before reading
  a file), glTF files from other tools (their material extensions, and
  lights and cameras, still or animated, included), beside Ratty itself,
  and on Windows a click on one
  through ConPTY's input
- the themes in a live window, with the fonts of their time installed
- the demos in a live window beyond playing Boing and the cube on Windows
  (clicks, resizing, how smoothly frames come on a big screen), on macOS
  and Linux, and in Ratty; `flow-demo plasma`, `teletext` and `vector` on
  Linux, and teletext in other terminals; `flow-demo sounds`, `tint` and
  `model` in a live Flow window (heard, washed, and the balloon allowed and
  turning; on Windows so far through ConPTY, by script), and on Linux;
  the balloon's file deleted as its tab closes, and swept after a run
  killed outright, on Windows (the console's close, `OpenProcess`, not yet
  built) and Linux, and in a live Flow window anywhere
- on Windows: Restart Flow, the chip maker's logo (set from Windows'
  fonts), help's pages and Ctrl+Insert in a live window; Flow Mines
  leaving its opened ground clear on Flow's dark glass there, now that the
  pipe for images answers OSC 11; the Workstation's boot listing this PC's
  own memory and slots in a live window; Run, efficient mode and the
  theme details by hand; Ctrl+Shift+B's restart and its question, box
  drawing in a live window, Efficient on battery on a laptop, and Flow
  Rogue 0.1.25 to 0.1.30 played; `flow-settings set` and `restart` in a
  live window, the new-tab picker and saved themes by hand, and the
  Graphics workstation's cube clicked
- Settings' **Fit window to** on Linux at the screen's edge, where the
  room Flow takes for a taskbar is a guess (Windows takes its screen's
  work area, tried live with a size more than it holds), a maximized
  window, and Wayland's maximized and tiled windows (sized once put back to
  their own size, or said to be kept) haven't been tried
- in a live window, by hand: the assistant searching the history, the bell's
  flash, the liquid's and the machines' sounds, the School and Personal
  computers and your own theme, a program's wash, a glTF's animations and
  materials, clicking a model, a model after a font change, and Flow Rogue
  0.1.15 (its treasures turning among the rest)
- sound heard on Windows beyond the demos' (the liquid's, and the bell
  through MessageBeep); on Linux, in a live Flow window and Flow Rogue's
  music in a game, without PipeWire (PulseAudio, or ALSA alone), with no
  ALSA library, and the sound server restarting while Flow plays (the
  output opens again at the next sound); and with output devices that
  change while Flow runs
- Flow Rogue in other terminals with kitty images, and on Linux;
  `flow-rogue` in a notarized release; its sprites in a live window on
  Windows from an x64 build
- picking an emoji from Emoji & Symbols in the live window, and Windows'
  emoji panel (`Win+.`)
- on macOS: an Intel or universal build, macOS versions before 27, and the look of the live
  window (title bar, blur, menus) on screen
