# Releasing Flow (Linux)

Linux has no Sparkle, so Flow updates itself there with an updater of its
own (`linux/src/updater.rs`). It reads an **EdDSA-signed appcast** in
Sparkle's form, downloads the newer tarball, checks it with **Flow's own
Ed25519 key** (the one Sparkle and WinSparkle check with), and runs the
tarball's `install.sh` into the folder Flow runs from. Then it offers to
restart into the new Flow. Flow shows all of it in About, since there's no
updater window.

All commands below run from the repo root. The scripts are in
`linux/scripts`, and the menu entry is in `linux/packaging`. The macOS and
Windows releases are in [macos/RELEASING.md](../macos/RELEASING.md) and
[windows/RELEASING.md](../windows/RELEASING.md).

- Feeds: one per architecture, since the tarballs differ:
  `https://raw.githubusercontent.com/arcanii/Flow-releases/main/appcast-linux-<arch>.xml`,
  `<arch>` being `x86_64` or `aarch64` (Rust's `std::env::consts::ARCH`,
  which is also the start of the tarball's target triple). Without a feed for
  its architecture, Flow says there's no release for it yet.
- Key: Flow's update key. Its private half is kept off the repository, in
  the Mac's login Keychain (account `Flow`) or in a key file given to the
  Windows scripts as `-KeyFile`
  ([windows/RELEASING.md](../windows/RELEASING.md)), not on a Linux
  computer, so a Linux release is signed on the Mac.
- Version: the appcast's `sparkle:version` is the build number (the git
  commit count plus `build_offset`) and `sparkle:shortVersionString` the version shown (with a
  patch's fourth number, as the tag has it). Flow
  compares the build number with its own (`flow --version`), and after the
  download checks that the tarball's `flow` reports that build.
- Who updates: only a Flow that `install.sh` put in place (`<prefix>/bin/flow`
  beside `<prefix>/lib/flow-terminal` and its menu entry). Where Flow can't
  write there (an install for every user, by root), it says a release is out
  and leaves the installing to the user. Copies run from `target/` have no
  updater.
- Tags: Linux releases are `v<ver>-linux` in `arcanii/Flow-releases`, one
  release holding the tarball and the .deb for each architecture, created
  with `--latest=false`, so the Windows release keeps the "Latest" badge.
  `<ver>` is the version `flow --version` shows, so a patch release has its
  fourth number (`patch` under `[workspace.metadata.flow]`) in the tag and
  the files' names (`v0.1.31.1-linux`, `Flow-0.1.31.1-linux-x86_64.tar.gz`),
  as About's Release notes link expects (`version::release_page`). The
  .deb's own version leaves it out (`0.1.31.<build>`), since dpkg would
  sort a fourth number's version below the release before it.
- The .deb installs under `/usr` (package `flow-terminal`), where Flow can't
  write, so it can't replace itself. From 0.1.37.1, where the feed carries
  the release's .deb (a second `<enclosure>`, after the tarball's, of type
  `application/vnd.debian.binary-package`, signed with Flow's key as the
  tarball is), its **Install Update** downloads the .deb into the
  downloads folder, checks it with Flow's key and that it's
  `flow-terminal` at the feed's build (`dpkg-deb -f`), and opens a tab
  that runs `sudo apt install ~/Downloads/<file>`, where sudo asks for the
  password (`UpdateState::InTab`); once apt has put the new Flow in place,
  About's **Restart Flow** starts it. A Flow before 0.1.37.1 reads the
  first enclosure alone, and a feed without the .deb's has a .deb's Flow
  open the release's page (the feed's `sparkle:releaseNotesLink`) instead,
  About naming the file and giving the command, which **Copy Command**
  copies (`UpdateState::ByHand`). The .deb's `.sha256` is published beside
  it, as the tarball's is.

## One-time setup

- Rust and a C compiler (see [docs/development.md](../docs/development.md)), and `gh`
  signed in to the arcanii account, on the computer that publishes.
- **zig** and **cargo-zigbuild**, with which `package.sh` builds against
  glibc 2.31. A program linked on a new Linux needs that Linux's glibc
  (Ubuntu 26.04's 2.43 for one), and older ones refuse to start it; built
  this way it runs on Ubuntu 20.04, Debian 11 and everything since. Neither
  needs root:
  ```sh
  # zig 0.16.0 from https://ziglang.org/download/ (check the SHA-256 its
  # index.json gives), unpacked under ~/.local/share/zig, on the PATH:
  ln -s ~/.local/share/zig/zig-x86_64-linux-0.16.0/zig ~/.cargo/bin/zig
  cargo install cargo-zigbuild --version 0.24.0 --locked
  ```
- On the Mac, Sparkle's `sign_update` and Flow's key in the Keychain, as for
  a macOS release: `windows/scripts/sign-release.sh` signs any file with it.

## Per release

1. **Commit everything** at the version to release (the marketing version is
   `version` under `[workspace.package]` in the root `Cargo.toml`), with
   `dev = false` under `[workspace.metadata.flow]`: the work toward it
   showed as 0.1.40.DEV. `package.sh` refuses a DEV build and a dirty tree,
   because the build number has to name the commit it was built from.
2. **Build** on each architecture, from the same commit:
   ```sh
   linux/scripts/package.sh --updates
   ```
   `--updates` makes it a build that updates itself (`FLOW_UPDATES=1`,
   docs/design/updates-for-releases.md), as Flow's releases are: the key
   isn't here to tell it so, as it tells `release.sh` on the Mac. A release
   must say `--updates` or `--no-updates` (a release that never updates
   itself, which mustn't go in Flow's appcast), and `make-appcast.sh`
   refuses a tarball whose `flow` doesn't say `updates: on`.
   This writes `target/linux/Flow-<ver>-linux-<arch>.tar.gz` and
   `Flow-<ver>-linux-<amd64|arm64>.deb`, each with its `.sha256`, built
   against glibc 2.31; it stops if any program in it needs a newer one.
   Build the `aarch64` one on an Arm64 Linux computer.
3. **Publish the release** from the first computer, then add the other
   architecture's tarball from its own:
   ```sh
   gh release create v<ver>-linux --repo arcanii/Flow-releases --title "Flow <ver> for Linux" \
      --latest=false --notes "..." target/linux/Flow-<ver>-linux-x86_64.tar.gz* target/linux/Flow-<ver>-linux-amd64.deb*
   ```
   ```sh
   gh release upload v<ver>-linux --repo arcanii/Flow-releases \
      target/linux/Flow-<ver>-linux-aarch64.tar.gz* target/linux/Flow-<ver>-linux-arm64.deb*
   ```
   Until the appcasts change in step 5, no installed copy sees it.
4. **Sign and write the appcasts.** Sign each tarball and each .deb on the
   Mac, from the release (which proves the published file is the one
   signed; the Mac has no `gh`, so fetch each with `curl -fLO
   https://github.com/arcanii/Flow-releases/releases/download/v<ver>-linux/<file>`):
   ```sh
   windows/scripts/sign-release.sh Flow-<ver>-linux-x86_64.tar.gz
   windows/scripts/sign-release.sh Flow-<ver>-linux-amd64.deb
   windows/scripts/sign-release.sh Flow-<ver>-linux-aarch64.tar.gz
   windows/scripts/sign-release.sh Flow-<ver>-linux-arm64.deb
   ```
   Then, on each architecture's computer, with those signatures:
   ```sh
   linux/scripts/make-appcast.sh --tarball target/linux/Flow-<ver>-linux-<arch>.tar.gz \
       --tag v<ver>-linux --signature <sig> \
       --deb target/linux/Flow-<ver>-linux-<debarch>.deb --deb-signature <deb sig>
   ```
   Before writing `target/linux/appcast-linux-<arch>.xml`, it checks that the
   tarball's `flow` reports the version the tag names and gives its build
   number, that the tarball holds `install.sh`, that the .deb is named as
   `package.sh` names it and is `flow-terminal` at that build, and that both
   signatures verify against `UPDATE_PUBLIC_KEY` (`cargo run --example
   update_key -- verify`). With the key's file at hand, `--key-file <file>`
   signs both there instead. Without `--deb` the feed offers the tarball
   alone, and a .deb's Flow opens the release's page.
5. **Publish the appcasts**: in a `Flow-releases` checkout, pull first, then
   copy each `appcast-linux-<arch>.xml` to its root, commit, and push to
   `main`. Flow sees the update on its next daily check, or when someone
   picks **Check for Updates…** in About or Settings.
6. **Update the README's Linux links** in that checkout, to the
   `v<ver>-linux` release, and the same in this repository's own `README.md`,
   whose "Download for Linux" link and the logo over it, at the top and under
   Install, open the release by its tag too.
7. **Check the live feed**: `cargo test -p flow-linux updater -- --ignored`
   downloads what `appcast-linux-<arch>.xml` now offers, checks it against
   Flow's key and installs it into a scratch folder, as an update would,
   and fetches and checks its .deb where the feed carries one, as a .deb's
   Flow would.

## flowapp's apps

`flowapp`'s catalog (`apps.json` in Flow-releases) gives each app a
download for each system and chip; Linux's are `linux-x86_64` (and
`linux-aarch64` once there's an Arm64 build), as `apps::platform` names
them. An app goes out apart from Flow, when it changes, and like Flow's
tarball it's signed on the Mac. The steps below make `flow-sdk`, and the
same steps with `flowapp` in its place make flowapp's own download
(docs/design/flowapp-updates.md): built from this checkout, which must be
committed (`--allow-dirty` for a try), at flowapp's own version (`flowapp`
under `[workspace.metadata.flow]`), bundled with Flow and needing Flow
0.1.44 (`--flow` for another), on the release `flowapp-v<version>` beside
the Mac's. The games
(`flow-rogue`, `flow-mines`, `flow-invaders`) are built and made in their
own repository, arcanii/Flow-Games, by its `scripts/app-release-linux.sh
<game>`, in the same steps from its own workspace and lock, with this
checkout beside it (`--flow <dir>`, by default `../Flow_Terminal`, so a
checkout of Flow-Terminal is named with `--flow`) for
`app_catalog`, the notices and `update_key`; Flow-Utils makes its own the
same way. Flow itself no longer needs Flow-Games to build.

1. **Make it**:
   ```sh
   linux/scripts/app-release.sh flow-sdk
   ```
   This builds the app for glibc 2.31 with cargo-zigbuild, as `package.sh`
   builds Flow (`flow-sdk` from the tag the root `Cargo.toml` names, cloned
   into `target/flow-sdk-<tag>`: `libflow_sdk_c.so` and `libflow_sdk_c.a`,
   the header, the Python package with the library in it and `flow-demo`),
   packs it as `target/apps/<app>-<version>-linux-x86_64.tar.gz`, owned by
   0 with no names, then unpacks it into a scratch folder, runs each
   program's `--version`, checks that nothing in it needs a newer glibc or
   names the build's folders, and has the Python package load its own
   library. It prints the tarball's size and SHA-256, and what's next.
2. **Upload it** to the app's release, `<app>-v<version>` in Flow-releases,
   beside the Mac's and Windows' downloads of that version:
   ```sh
   gh release upload <app>-v<version> --repo arcanii/Flow-releases target/apps/<app>-<version>-linux-x86_64.tar.gz
   ```
   (or `gh release create <app>-v<version> … --latest=false` where there's
   none yet, so Flow's own release stays Latest). Until the catalog
   changes in step 4, no `flowapp` sees it.
3. **Sign it on the Mac**, from the release:
   ```sh
   curl -fLO https://github.com/arcanii/Flow-releases/releases/download/<app>-v<version>/<app>-<version>-linux-x86_64.tar.gz
   windows/scripts/sign-release.sh <app>-<version>-linux-x86_64.tar.gz
   ```
   Then, back here, with its signature:
   ```sh
   linux/scripts/app-release.sh <app> --signature <sig>
   ```
   This writes the entry for the tarball step 1 made into
   `target/apps/apps.json` (begun from the published catalog; `--catalog`
   for another), which `examples/app_catalog.rs` checks the signature
   against first. With the key's file at hand, `--key-file <file>` signs
   here instead. Make several and they go into the same catalog.
4. **Publish the catalog**: in a `Flow-releases` checkout, pull, copy
   `target/apps/apps.json` to its root, check with `git diff` that only the
   apps you made changed, commit and push to `main`. Every `flowapp update`
   reads it from there, so publish only when asked.
5. **Check it live** as `flowapp` reads it: GitHub's cache of
   `raw.githubusercontent.com` keeps the gzipped copy (which `flowapp`
   asks for) apart from the plain one, and either can serve the catalog
   from before the push for some minutes after the other has the new one.
   So look with `curl --compressed`, then `flowapp update` and `flowapp
   upgrade` in a scratch home (`HOME` and `XDG_CONFIG_HOME` pointed at a
   new folder) once that serves it.

## Trying a feed before publishing it

`FLOW_UPDATE_FEED=<url>` points a Flow at another feed, such as
`python3 -m http.server` serving an appcast from a folder. What it offers
must still be signed with Flow's key, so a test feed shows the update in
About, but installing it fails at the signature unless it's a release
signed for real. `cargo test -p flow-linux updater` runs a whole update
against a local server with a test key: a signed tarball installed into a
scratch prefix, and a tampered one, one signed by another key and one whose
`flow` isn't the feed's build all turned away.
