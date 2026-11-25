# Updates only in Flow's own releases

Asked for on 2026-11-23: the updaters should run only in Flow's own
releases, before Flow's source opens and anyone can build and ship it.
The design below was agreed the same day.

## Why

Today any release build of Flow starts its updater wherever the package
carries one: Sparkle in the Mac's bundle, `WinSparkle.dll` beside
`flow.exe`, or an `install.sh` layout on Linux (`App::create_window`:
`!cfg!(debug_assertions)`, or `FLOW_UPDATER=1`). Each reads Flow's own feed
and installs what Flow's update key signed. So a fork that runs Flow's
release scripts unchanged makes builds that offer Flow's next release and,
once it's installed, are Flow again: their users lose the fork without
being asked. Nothing unsafe gets in, since only Flow's key is trusted, but
the build isn't the fork's to update.

A build should update itself only from whoever made it. Only Flow's
maintainer holds Flow's update key, kept off the repository (in the Mac's
Keychain, or a key file the Windows scripts take as `-KeyFile`), and the
key is what signs every update. So holding it is what makes a build one
of Flow's own.

## What it does

- **One switch, in the build:** `FLOW_UPDATES=1` in the environment while
  core builds makes `version::UPDATES` true (`core/build.rs`, with
  `rerun-if-env-changed`). Flow starts its updater only where it's true, or
  where `FLOW_UPDATER=1` forces it at run time for testing, as now. A plain
  `cargo build --release`, a fork's, a distribution's or a contributor's,
  has no updater: About and the menus show none, as a bare build's do now
  (they follow `Platform::updater_available`).
- **The release scripts turn it on only with the key at hand:**
  - **macOS:** `release.sh` reads the public half of the Keychain's `Flow`
    key (Sparkle's `generate_keys --account Flow -p`, which only looks; the
    Keychain may ask once) and builds with `FLOW_UPDATES=1` where it's
    `UPDATE_PUBLIC_KEY` (`core/src/lib.rs`), which `SUPublicEDKey` must be
    too.
  - **Windows:** `release.ps1` reads the public half of the key file
    `-KeyFile` names (`update_key public`, the file `make-appcast.ps1`
    signs with) and does the same.
  - **Linux** has no key: its tarballs are signed on the Mac afterwards. So
    `package.sh` takes `--updates`, which Flow's releases pass
    (`linux/RELEASING.md`).
- **A release can't go out without it by mistake:** a release build (no
  `--allow-dirty`, `-AllowDirty` or `--adhoc`) without the key, or on Linux
  without `--updates`, stops and says why, unless given `--no-updates`
  (`-NoUpdates`), which makes a release that never updates itself. A test
  build without the key is made without updates and says so.
- **Kept as it is:** Sparkle stays in the Mac's bundle and `WinSparkle.dll`
  in the Windows setup either way (without the switch they're never
  loaded), and flowapp's catalog checks and the apps an older Flow carried
  (`check_apps`, `keep_carried_apps`) still run in every release build:
  they install other programs, signed by Flow's key, never Flow itself.
- **A fork** that wants its own updates gives its builds its own key and
  feeds (CONTRIBUTING's Forks): with its key in the Keychain or the key
  file, its release scripts turn updates on for its builds, reading its
  own feeds.

- **What a build says:** a release built with `--skip-build` or
  `--no-build` packages a build made before, so each script asks the
  binary what it was built for: `FLOW_SAY_UPDATES=1 flow --version` adds
  `updates: on` or `updates: off` on a line of its own
  (`version::answer`), and the script stops where that isn't what it
  decided. Asked without it, a Flow says only its version, as the
  updaters out already read it (Linux's takes the whole answer). On
  Windows `flow.exe` is a window's program, so `release.ps1` asks the
  flow command beside it, built from the same core. Linux's
  `make-appcast.sh` asks the tarball's `flow` too, since nothing else
  stops a feed from naming a build without updates, which every Flow
  would update to and then stop.

## Tried

- Core's tests of the decision (`version::updater_on`: the build's switch,
  a debug build, `FLOW_UPDATER`) and of `--version`'s answer with and
  without `FLOW_SAY_UPDATES`.
- A debug build with and without `FLOW_UPDATES=1`: cargo builds core again
  when it changes, and `--version` says `updates: on` or `off`.
- `release.sh` on the Mac against stand-in `generate_keys` (`GENERATE_KEYS`,
  as `SIGN_UPDATE` names `sign_update`) and a stand-in `cargo`, which ends
  the run before it builds: Flow's key turns updates on, another key or none
  leaves a test build without, and `--no-updates` leaves them off; its
  decision run alone as a release stops without the key, and `--no-updates`
  lets it through.
- `package.sh`'s decision run alone: a release without `--updates` or
  `--no-updates` stops, and a test build is without updates unless given
  `--updates`.
- Not run: a whole release on any system, Sparkle's real `generate_keys`
  against the Keychain (which may ask the user), and `release.ps1`, which
  needs Windows: to be tried before the next Windows release.

## Left out

- Signing or checking anything new at run time: a build's key is already
  the only one its updater trusts.
- Gating flowapp's catalog, which a fork's flowapp still reads: apps aren't
  Flow, and `FLOWAPP_CATALOG` and `CATALOG_URL` already name another.
