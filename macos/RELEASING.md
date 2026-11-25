# Releasing Flow (macOS)

Flow.app updates itself with **Sparkle** (2.6.4, fetched by the release
script). Sparkle reads an **EdDSA-signed appcast**, then downloads the DMG,
checks it and replaces the app.

All commands below run from the repo root. The scripts are in
`macos/scripts`, and the bundle's `Info.plist` and entitlements are in
`macos/packaging`. The Windows release is in
[windows/RELEASING.md](../windows/RELEASING.md).

- Feed: `https://raw.githubusercontent.com/arcanii/Flow-releases/main/appcast-macos.xml`
  (`SUFeedURL` in `macos/packaging/Info.plist`). It is separate from the
  Windows `appcast.xml`, since the downloads and version numbers differ.
- Key: **Flow's own Ed25519 key**, the one the Windows updater uses. Its
  private half is kept off the repository, here in the login Keychain under
  the account `Flow`. `macos/scripts/bundle.sh` copies the public key
  from `core/src/lib.rs` (`UPDATE_PUBLIC_KEY`) into Info.plist
  (`SUPublicEDKey`), and Flow doesn't start Sparkle if the two differ.
- Version: `CFBundleShortVersionString` is the version Flow shows (`0.1.2`, or
  `0.1.31.1` for a patch release, `patch` under `[workspace.metadata.flow]`)
  and `CFBundleVersion` the build number (the git commit count plus
  `build_offset`). Both come from the
  binary itself (`flow --version`), so the bundle can't disagree with it.
  Sparkle compares the build number with the appcast's `sparkle:version`.
- Tags: macOS releases are `v<ver>-macos` in `arcanii/Flow-releases` (`<ver>`
  with a patch release's fourth number, `v0.1.31.1-macos`, as the DMG's name), created
  with `--latest=false`, so the Windows release keeps the "Latest" badge that
  the README's Windows download link relies on. That repo's README links the
  macOS release by its tag instead, so step 6 updates it.

## One-time setup

- Xcode (for `codesign`, `notarytool` and `stapler`), Rust, and `gh` signed in
  to the arcanii account.
- The **Developer ID Application** certificate (your team ID, `<TEAMID>`)
  in the login keychain. `release.sh` finds it by name; `--identity` picks
  another.
- A `notarytool` keychain profile. The scripts use `Flow-notarize`: store
  your credentials under that name once,
  ```sh
  xcrun notarytool store-credentials "Flow-notarize" --apple-id <you@example.com> \
      --team-id <TEAMID> --password <app-specific password>
  ```
  or set `NOTARY_PROFILE` to a profile of another name.
- Flow's update key in the login Keychain under the account `Flow`
  (`generate_keys --account Flow -f <key file>` imports one). Check it with
  `generate_keys --account Flow -p` (in Sparkle's `bin/`, which `release.sh`
  unpacks to `target/sparkle-2.6.4/bin`); it must print `UPDATE_PUBLIC_KEY`.
  `release.sh` reads it the same way: only with it does the build update
  itself (`FLOW_UPDATES=1`, docs/design/updates-for-releases.md), and a
  release without it stops, unless given `--no-updates` for one that never
  updates itself (which mustn't go in Flow's appcast).
- For a **universal** build (Apple silicon and Intel), rustup's toolchain first
  on `PATH` with the Intel target added: `rustup target add x86_64-apple-darwin`.
  Without it, the script builds for Apple silicon only and says so.

## Per release

1. **Set `dev = false`** under `[workspace.metadata.flow]` in the root
   `Cargo.toml` (the work toward the version showed as 0.1.40.DEV; its
   `version` went up when that work began), and **commit everything**.
   `release.sh` refuses a DEV build and a dirty tree, because the build
   number has to name the commit it was built from. Once every system's
   release is out, bump `version` to the next and set `dev = true` again.
2. **Build**:
   ```sh
   macos/scripts/release.sh --notarize
   ```
   This writes `target/dist/Flow-<ver>-macos.dmg` and `SHA256SUMS-macos.txt`.
   On the way it:
   - builds `flow`, `flow-help`, `flow-settings`, `flowapp`, `flow-mcp` and
     `flow-command` (the `flow` command) for macOS 11
     and later (universal when it can) and checks that `flow` reports this
     commit's version (from 0.1.34 the games and the demos aren't in the
     app: `flowapp` installs them, from downloads made as "flowapp's apps"
     below says);
   - fetches Sparkle and checks the tarball against the pinned SHA-256;
   - writes `THIRD-PARTY-NOTICES.txt` (Sparkle, then every Rust crate in
     any of the programs, from `examples/third_party_notices.rs`) into the
     app's Resources, and puts `flow-help`, `flow-settings`, `flowapp`, `flow-mcp`
     and `flow-command`, named `flow` there, in `Contents/Helpers`;
   - signs inside out (Sparkle's helpers, the framework, each helper as
     `com.arcanii.Flow.<name>`, then the app) with
     the hardened runtime, a secure timestamp and
     `macos/packaging/Flow.entitlements`;
   - makes the DMG (Flow.app and a link to Applications), signs it, and has
     Apple notarize it, then staples the ticket.

   Notarizing needs the Mac's screen unlocked: while it's locked,
   `notarytool` says its keychain profile isn't there, though signing
   with the Developer ID still works. A build made then can be notarized
   later without building again: `release.sh --notarize --skip-build`.
3. **Publish the release**:
   ```sh
   gh release create v<ver>-macos --repo arcanii/Flow-releases --title "Flow <ver> for macOS" \
      --latest=false --notes "..." target/dist/Flow-<ver>-macos.dmg target/dist/SHA256SUMS-macos.txt
   ```
   Until the appcast changes in step 5, no installed copy sees it.
4. **Sign and write the appcast**:
   ```sh
   macos/scripts/make-appcast.sh --dmg target/dist/Flow-<ver>-macos.dmg --tag v<ver>-macos
   ```
   It signs the DMG with `sign_update --account Flow` (macOS may ask to allow
   Keychain access); pass `--signature <sig>` to use one made elsewhere.
   Before writing `target/dist/appcast-macos.xml`, it checks that the DMG holds
   a validly signed, notarized Flow.app whose version matches the tag, and that
   the signature verifies against `UPDATE_PUBLIC_KEY`
   (`cargo run --example update_key -- verify`).
5. **Publish the appcast**: in a `Flow-releases` checkout, pull first, then
   copy `target/dist/appcast-macos.xml` to its root, commit, and push to
   `main`. Sparkle sees the update on its next check, or when someone picks
   **Flow → Check for Updates…**.
6. **Update the README's macOS links**: in the same checkout, change
   `v<old>-macos` to `v<ver>-macos` in `README.md`, both in the "Download for
   macOS" link at the top and in the macOS part of the Download section. Commit
   and push to `main`. Until then the links still open the previous release,
   whose copies Sparkle then updates. Change them the same way in this
   repository's own `README.md`, whose "Download for macOS" link and the logo
   over it, at the top and under Install, open the release by its tag too.

## flowapp's apps

`flowapp`'s catalog (`apps.json` in Flow-releases) gives each app a
download for each system and chip; the Mac's are `macos-aarch64`. An app
goes out apart from Flow, when it changes:

- **flow-sdk**: `macos/scripts/app-release.sh flow-sdk` (`--notarize` for a
  release, `--adhoc` for a try) builds it from the Flow-SDK tag the root
  `Cargo.toml` names, signs it with the Developer ID, packs it, signs the
  tarball with Flow's key and writes its entry into `target/apps/apps.json`,
  begun from the published catalog.
- **flowapp** itself (docs/design/flowapp-updates.md): `macos/scripts/app-release.sh
  flowapp --notarize` builds it from this checkout, which must be committed
  (`--allow-dirty` for a try), at its own version (`flowapp` under
  `[workspace.metadata.flow]`, which goes up with each one published), and
  writes it into the catalog as bundled with Flow and needing Flow 0.1.44
  (`--flow` for another), the rest as flow-sdk's. Its release is
  `flowapp-v<version>`. Every flowapp's `upgrade` then offers it, and the
  copy installed runs in Flow's tabs ahead of the one Flow carries.
- **The games** (`flow-rogue`, `flow-mines`, `flow-invaders`) are built and
  made in their own repository, arcanii/Flow-Games, by its
  `scripts/app-release.sh <game>`, the same steps from its own workspace and
  lock, with this checkout beside it (`--flow <dir>`, by default
  `../Flow_Terminal`, so a checkout of Flow-Terminal is named with
  `--flow`) for `app_catalog`, the notices and Sparkle's
  `sign_update` from `target/sparkle-*`. `--unsigned` (with `--adhoc`,
  signed ad hoc) makes and tries one without Flow's key. Flow itself no
  longer needs that repository to build. Flow-Utils makes its own the same
  way.

Then the tarball goes on the app's release, `<app>-v<version>` in
Flow-releases (`gh release create … --latest=false`, so Flow's own release
stays Latest), and the catalog to that repo's `main` as `apps.json`. Every
`flowapp update` reads it from there, so publish only when asked.

## Local builds

- `macos/scripts/release.sh --adhoc --allow-dirty` runs the whole pipeline
  without a certificate. The DMG it makes runs on this Mac only; don't publish it.
- `macos/scripts/bundle.sh target/release/flow target/release` wraps a build in
  an unsigned Flow.app without Sparkle, for trying the app bundle (its menu
  bar, icon and Dock behavior) during development.

## Entitlements and privacy prompts

Flow itself needs no entitlements. But programs run in its terminal count as
Flow when they ask macOS for the camera, the microphone, contacts, other apps
(AppleScript), or files in Desktop, Documents and Downloads. Under the hardened
runtime those requests fail silently unless the app has the matching
entitlement, and macOS refuses them unless Info.plist says why. So, like
Terminal and iTerm2, Flow.app carries both (`macos/packaging/Flow.entitlements`
and the `NS…UsageDescription` keys); each request still asks the user first.
