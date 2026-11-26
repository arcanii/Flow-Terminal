# Releasing Flow (Windows x64)

For the macOS app, see [macos/RELEASING.md](../macos/RELEASING.md).

Flow updates itself with **WinSparkle** (0.9.3, vendored in
`windows/third_party/winsparkle`). WinSparkle reads an **EdDSA-signed
appcast**, then downloads and runs the new installer.

All commands below run from the repo root. The scripts are in
`windows/scripts`, and the installer script is `windows/packaging/installer.iss`.

Releases go apart from the source, to the **public**
[`arcanii/Flow-releases`](https://github.com/arcanii/Flow-releases) repo. It holds
only `appcast.xml` (on `main`) and the GitHub Releases with the installers.

- Feed: `https://raw.githubusercontent.com/arcanii/Flow-releases/main/appcast.xml`
  (`APPCAST_URL` in `windows/src/updater.rs`).
- Version: the About box shows `0.1.0 (build N)`. WinSparkle compares the
  four-part `0.1.0.N` (`version::FULL`) against the appcast's
  `sparkle:version`.
  - The marketing version is `version` under `[workspace.package]` in the root
    `Cargo.toml`. A patch release sets `patch` under
    `[workspace.metadata.flow]`, and then shows as `0.1.0.1 (build N)`; its
    tag, its files and the feed's `sparkle:shortVersionString` say
    `0.1.0.1`, while WinSparkle still compares `0.1.0.N`. Set `patch` back to
    0 when the marketing version goes up.
  - The build number is the commit count (`git rev-list --count HEAD`) plus
    `build_offset` under `[workspace.metadata.flow]`.
    `core/build.rs` stamps it into the app, and the root `build.rs` into the
    exe's VERSIONINFO.
  - A rebuild of the same commit keeps its number, and two clones agree on it.

## The update key

Flow has **its own Ed25519 key pair**, made by `examples/update_key.rs`.

- The **public key** is `UPDATE_PUBLIC_KEY` in `core/src/lib.rs`. The macOS
  build's `SUPublicEDKey` must be the same string.
- The **private key** is kept off the repository: in the Mac's login
  Keychain under the account `Flow`, or in a key file given to the Windows
  scripts as `-KeyFile`. The key file is in Sparkle's format, so Sparkle's
  tools on the Mac take the same key.

Ed25519 signatures are deterministic, so signing the same installer on either
machine gives the same signature.

> ⚠️ **Don't lose the private key.** Every installed copy of Flow accepts
> updates signed with this key only. Without the key, those copies can't
> update; their users would have to download a build with a new key by hand.

## Installer vs portable zip

WinSparkle updates by running the installer, so **only installed copies update
themselves**. The portable zip leaves `WinSparkle.dll` out. Without the DLL, Flow
hides *Check for Updates…*, and portable users update by hand. Shipping the DLL
there would make an accepted update install a second copy in Program Files and
leave the portable one stale.

## One-time setup

- Inno Setup 6: `winget install JRSoftware.InnoSetup`
- PowerShell 7 (`pwsh`), Rust, and `gh` signed in to the arcanii account.
- The x64 Rust target: `rustup target add x86_64-pc-windows-msvc`. The release
  is always built for it, so on an ARM64 PC it's a cross build, which also
  needs Visual Studio's x64 build tools (the MSVC x64/x86 build tools
  component).
- The key, to sign on the Mac: import the key file into the login
  Keychain and check it,
  ```sh
  generate_keys --account Flow -f <key file>
  generate_keys --account Flow -p
  ```
  The second must print the public key from `core/src/lib.rs`.
  `generate_keys` and `sign_update` ship in Sparkle's `bin/` folder, which
  `macos/scripts/release.sh` fetches, checks and unpacks into
  `target/sparkle-2.6.4/bin`.
- A new key pair is only for a new app, since installed copies of Flow trust
  this one. `cargo run --example update_key -- new <file>` writes the private
  key and prints the public key. If Windows says this "requires elevation",
  its installer detection took `update_key.exe` for an installer because of
  its name: run `$env:__COMPAT_LAYER = 'RunAsInvoker'` first, as
  `make-appcast.ps1` does for its own runs.

## Per release

1. **Set `dev = false`** under `[workspace.metadata.flow]` in the root
   `Cargo.toml` (the work toward the version showed as 0.1.40.DEV; its
   `version` went up when that work began), or set `patch` there for a patch
   release, and **commit everything**. `release.ps1` refuses a DEV build and
   a dirty tree, because the build number has to name the commit it was
   built from. Below, `<ver>` is the version people see, a patch's number
   and all. Once every system's release is out, bump `version` to the next
   and set `dev = true` again.
2. **Build**:
   ```powershell
   pwsh windows\scripts\release.ps1 -SmokeTest
   ```
   Only with Flow's update key in `-KeyFile` (the key file
   `make-appcast.ps1` signs with) does the build update itself (`FLOW_UPDATES=1`,
   docs/design/updates-for-releases.md): its public half, from `update_key
   public`, must be `UPDATE_PUBLIC_KEY`. A release without it stops, unless
   given `-NoUpdates` for one that never updates itself (which mustn't go in
   Flow's appcast). `games\flow.exe` says which the build is, with
   `FLOW_SAY_UPDATES=1` and `--version`.
   This builds `flow.exe`, `flow-help.exe`, `flow-settings.exe` and
   `flowapp.exe` in `target\x86_64-pc-windows-msvc\release`,
   x64 on any PC, and writes four files to `target\dist`:
   - `Flow-<ver>-setup.exe`
   - `Flow-<ver>-x64-portable.zip`
   - `busybox-w32-FRP-<n>.tgz`, the source of the bundled BusyBox
   - `SHA256SUMS.txt`

   The setup and the zip both carry `busybox.exe`, the built-in bash;
   `games\flow-help.exe`, `games\flow-settings.exe`, `games\flowapp.exe`,
   `games\flow-mcp.exe` and `games\flow.exe` (the `flow` command,
   `flow-command.exe` renamed), which Flow puts at the end of its tabs' PATH;
   `README.md` and `LICENSE`, with the manual and the pages beside it
   (`docs\*.md`, `docs\manual\*.md`), which the README's Documentation
   links, so they open offline (their pictures stay online); and a generated
   `THIRD-PARTY-NOTICES.txt` for the crates in those programs. From 0.1.34
   the games and the demos come from `flowapp` instead, and the setup's
   `[InstallDelete]` takes an older Flow's `flow-rogue.exe`, `flow-mines.exe`
   and `flow-demo.exe` away; the portable zip, unpacked over an older one,
   leaves them, and they count as Flow's in `flowapp list` until deleted.

   `-SmokeTest` installs the build per-user into a temp dir, checks the files,
   the version, that no game or demo is there, that `flowapp` starts
   (answers `--version`) and that `flow-help --list` names the pages, and uninstalls it. Close Flow first: setup won't run while
   Flow is open (`AppMutex=Flow.Running`). Note the `0.1.0.N` version it prints.
3. **Publish the release**:
   ```powershell
   gh release create v<ver> --repo arcanii/Flow-releases --title "Flow <ver>" `
      --notes "..." target\dist\Flow-<ver>-setup.exe `
      target\dist\Flow-<ver>-x64-portable.zip target\dist\busybox-w32-FRP-<n>.tgz `
      target\dist\SHA256SUMS.txt
   ```
   **Always upload the BusyBox source.** BusyBox is GPLv2, and offering its
   source from the same place as the binaries is what lets Flow ship it
   (`windows/third_party/busybox/README.md`). `release.ps1` prints the exact
   command.

   Installed copies don't see the release until the appcast changes in step 5.
4. **Sign and write the appcast.** On Windows it's one step:
   ```powershell
   pwsh windows\scripts\make-appcast.ps1 -Version <ver>.<build> -Tag v<ver> `
        -SetupExe target\dist\Flow-<ver>-setup.exe
   ```
   This signs with the key file `-KeyFile` names.

   To sign on the Mac instead, run these from a checkout of this repo:
   ```sh
   curl -LO https://github.com/arcanii/Flow-releases/releases/download/v<ver>/Flow-<ver>-setup.exe
   windows/scripts/sign-release.sh Flow-<ver>-setup.exe
   ```
   Then pass the signature it prints as `-Signature <sig>`.

   Either way, before writing `target\dist\appcast.xml` the script checks that
   the installer's version is `-Version` and that the signature verifies
   against the updater's public key. A wrong key, or a signature of a
   different build, stops here instead of breaking updates. `-Tag` is required
   and must be the release tag (`v0.1.0`), not the full version.
5. **Publish the appcast**: in a `Flow-releases` checkout, pull first, since
   its README also gets edited on GitHub. Then copy `target\dist\appcast.xml`
   to its root, commit, and push to `main`. WinSparkle sees the update on its
   next check, or when someone picks *Check for Updates…* in About.
6. **Update the README's Windows version**: in this repository's own
   `README.md`, change the version under "Download for Windows", at the
   top and under Install, to `<ver>`. Its link opens Flow-releases' latest
   release and needs no change.

## flowapp's apps

`flowapp`'s catalog (`apps.json` in Flow-releases) gives each app a
download for each system and chip; Windows' are `windows-x86_64`, which
every Flow for Windows asks for, x64 or ARM64. An app goes out apart from
Flow, when it changes. The steps below make `flow-sdk`, and the same
steps with `flowapp` in its place make flowapp's own download
(docs\design\flowapp-updates.md): built from this checkout, which must be
committed (`-AllowDirty` for a try), at flowapp's own version (`flowapp`
under `[workspace.metadata.flow]`), bundled with Flow and needing Flow
0.1.44 (`-Flow` for another), on the release `flowapp-v<version>` beside
the Mac's. The games
(`flow-rogue`, `flow-mines`, `flow-invaders`) are built and made in their
own repository, arcanii/Flow-Games, by its `scripts\app-release.ps1
<game>`, in the same steps from its own workspace and lock, with this
checkout beside it (`-Flow <dir>`, by default `..\Flow_Terminal`, so a
checkout of Flow-Terminal is named with `-Flow`) for
`app_catalog`, the notices and `update_key` (`-Unsigned` makes and tries
one without the key); Flow-Utils makes its own the same way. Flow itself no
longer needs Flow-Games to build.

1. **Make it**:
   ```powershell
   pwsh windows\scripts\app-release.ps1 flow-sdk
   ```
   This builds the app for x64 as `release.ps1` builds (`flow-sdk` from the
   tag the root `Cargo.toml` names, cloned into `target\flow-sdk-<tag>`: its
   DLL, import and static libraries, header, Python package and
   `flow-demo.exe`), packs
   it with Windows' own `tar.exe` as
   `target\apps\<app>-<version>-windows-x86_64.tar.gz`, unpacks it into a
   scratch folder and runs each program's `--version` (and has the Python
   package load its own DLL), signs it with the key file `-KeyFile`
   names and writes its entry into
   `target\apps\apps.json`, begun from the published catalog, which
   `examples\app_catalog.rs` checks the signature against before writing.
   Make several and they go into the same catalog. The programs aren't
   code-signed, as Flow's aren't.
2. **Upload it** to the app's release, `<app>-v<version>` in Flow-releases.
   Where the Mac's download of that version is there already:
   ```powershell
   gh release upload <app>-v<version> --repo arcanii/Flow-releases target\apps\<app>-<version>-windows-x86_64.tar.gz
   ```
   Otherwise `gh release create <app>-v<version> … --latest=false`, so
   Flow's own release stays Latest. Check that GitHub's digest of the upload
   is the catalog's `sha256`.
3. **Publish the catalog**: in a `Flow-releases` checkout, pull, copy
   `target\apps\apps.json` to its root, check with `git diff` that only the
   apps you made changed, commit and push to `main`. Every `flowapp update`
   reads it from there, so publish only when asked.

## Not covered yet

- **Authenticode.** Neither `flow.exe` nor the installer is code-signed, so
  SmartScreen warns on first run ("Windows protected your PC" → More info → Run
  anyway). The EdDSA signature above protects only updates; it isn't
  Authenticode. With a certificate, `signtool sign` the exe before step 2
  packages it, then the installer, then do the EdDSA signing, so it covers the
  final bytes.
- **ARM64.** There's only an x64 build, which also runs emulated on ARM64.
  A native build would need its own feed (`appcast-arm64.xml`) and an ARM64
  WinSparkle.dll: the vendored one is x64, so a native
  ARM64 flow.exe can't load it and runs without the updater.
