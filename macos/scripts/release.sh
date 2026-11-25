#!/usr/bin/env bash
#
# Build a macOS release of Flow into target/dist:
#   Flow-<ver>-macos.dmg   Flow.app signed with Developer ID (and notarized with
#                          --notarize): the download, and what Sparkle installs.
#                          It carries flow-help, flow-settings, flowapp, flow-mcp
#                          and the flow command (flow-command, as `flow`) in
#                          Contents/Helpers; from 0.1.34 the games and the demos
#                          come from flowapp, not with Flow.
#   SHA256SUMS-macos.txt
# Commit first: the build number is the commit count (plus build_offset), so it
# has to name HEAD.
# Run from anywhere; paths are relative to the repo root.
#
#   macos/scripts/release.sh --notarize               a release
#   macos/scripts/release.sh                          the same without notarizing
#   macos/scripts/release.sh --adhoc --allow-dirty    a local test build, no certificate
#
# Options:
#   --notarize          submit the DMG to Apple and staple the ticket to it
#   --identity <name>   codesign identity; default: the keychain's first
#                       "Developer ID Application" certificate
#   --adhoc             sign ad hoc instead; such a build only runs on this Mac
#   --arch <a>          arm64, x86_64 or universal; default: universal when the
#                       x86_64-apple-darwin target is installed, else arm64
#   --skip-build        package the binaries already in target/<triple>/release
#   --allow-dirty       allow uncommitted changes, for test builds only
#   --no-updates        a release that never updates itself, made without
#                       Flow's update key (a fork's, until it has its own)
#
# Environment:
#   NOTARY_PROFILE      notarytool keychain profile (default Flow-notarize;
#                       macos/RELEASING.md has the setup)
#   SPARKLE_VERSION     Sparkle release to bundle, with SPARKLE_SHA256 its
#                       tarball's checksum (defaults below)
#   GENERATE_KEYS       Sparkle's generate_keys, which reads the update key's
#                       public half (default: the one from that release)
#
# Flow updates itself only in a build made with its update key at hand
# (docs/design/updates-for-releases.md): this builds with FLOW_UPDATES=1
# where the login Keychain's "Flow" key, which make-appcast.sh signs with,
# is the one the build trusts (UPDATE_PUBLIC_KEY), and a release made
# without it stops unless given --no-updates.

set -euo pipefail

SPARKLE_VERSION="${SPARKLE_VERSION:-2.6.4}"
SPARKLE_SHA256="${SPARKLE_SHA256:-50612a06038abc931f16011d7903b8326a362c1074dabccb718404ce8e585f0b}"
NOTARY_PROFILE="${NOTARY_PROFILE:-Flow-notarize}"
MIN_MACOS="11.0" # keep in step with LSMinimumSystemVersion in macos/packaging/Info.plist

die() { echo "error: $*" >&2; exit 1; }

notarize=""; identity=""; adhoc=""; arch=""; skip_build=""; allow_dirty=""; no_updates=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --notarize) notarize=1; shift ;;
    --identity) identity="${2:-}"; shift 2 ;;
    --adhoc) adhoc=1; shift ;;
    --arch) arch="${2:-}"; shift 2 ;;
    --skip-build) skip_build=1; shift ;;
    --allow-dirty) allow_dirty=1; shift ;;
    --no-updates) no_updates=1; shift ;;
    *) die "unknown option $1 (see the top of $0)" ;;
  esac
done
[[ -z "$adhoc" || -z "$notarize" ]] || die "an ad hoc build can't be notarized"
[[ -z "$adhoc" || -z "$identity" ]] || die "pass --identity or --adhoc, not both"

# This script is in macos/scripts; the repo root is two levels up.
repo="$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)"
cd "$repo"
dist="$repo/target/dist"

# Marketing version from [workspace.package] in Cargo.toml; build number =
# commit count plus `build_offset` under [workspace.metadata.flow], as
# core/build_number.rs has it.
ver="$(sed -n 's/^version = "\([^"]*\)"$/\1/p' Cargo.toml | head -1)"
# A patch release's number (`patch` under [workspace.metadata.flow]) goes
# after it as a fourth, as Flow shows it (0.1.31.1), and names the release,
# its tag and the DMG, as Windows' scripts do; Sparkle compares the build
# number, which only rises.
patch="$(awk '/^\[workspace\.metadata\.flow\]/ { f = 1; next } /^\[/ { f = 0 } f && /^patch *=/ { gsub(/[^0-9]/, ""); print; exit }' Cargo.toml)"
shown="$ver"
[[ -n "$patch" && "$patch" != 0 ]] && shown="$ver.$patch"
# Work toward a version (`dev = true` there) isn't released: Flow says
# 0.1.40.DEV until releasing sets it to false. A test build keeps it.
if awk '/^\[workspace\.metadata\.flow\]/ { f = 1; next } /^\[/ { f = 0 } f && /^dev *= *true/ { found = 1 } END { exit !found }' Cargo.toml; then
  [[ -n "$allow_dirty" ]] || die "Flow is $shown.DEV, still worked toward: set dev = false under [workspace.metadata.flow] and commit to release it (or pass --allow-dirty for a test build)."
  shown="$shown.DEV"
fi
offset="$(awk '/^\[workspace\.metadata\.flow\]/ { f = 1; next } /^\[/ { f = 0 } f && /^build_offset *=/ { gsub(/[^0-9]/, ""); print; exit }' Cargo.toml)"
build="$(( $(git rev-list --count HEAD) + ${offset:-0} ))"
echo "Flow $shown (build $build)"
if [[ -z "$allow_dirty" && -n "$(git status --porcelain)" ]]; then
  die "uncommitted changes. Commit first, since the build number names HEAD (or pass --allow-dirty for a test build)."
fi

has_target() {
  local libdir
  libdir="$(rustc --print target-libdir --target "$1" 2>/dev/null)" && compgen -G "$libdir/libstd-*" >/dev/null
}
if [[ -z "$arch" ]]; then
  if has_target x86_64-apple-darwin; then
    arch=universal
  else
    arch=arm64
    echo "note: building for Apple silicon only; for a universal app, use rustup's cargo with the"
    echo "      x86_64-apple-darwin target (rustup target add x86_64-apple-darwin)."
  fi
fi
case "$arch" in
  arm64) triples=(aarch64-apple-darwin) ;;
  x86_64) triples=(x86_64-apple-darwin) ;;
  universal) triples=(aarch64-apple-darwin x86_64-apple-darwin) ;;
  *) die "--arch is arm64, x86_64 or universal, not $arch" ;;
esac

if [[ -z "$identity" && -z "$adhoc" ]]; then
  identity="$(security find-identity -v -p codesigning | sed -n 's/.*"\(Developer ID Application: [^"]*\)".*/\1/p' | head -1)"
  [[ -n "$identity" ]] || die "no Developer ID Application certificate in the keychain. Pass --identity, or --adhoc for a local build."
fi

# Sparkle, from its GitHub release, checked against the pinned checksum.
sparkle="$repo/target/sparkle-$SPARKLE_VERSION"
if [[ ! -d "$sparkle/Sparkle.framework" ]]; then
  tarball="$repo/target/Sparkle-$SPARKLE_VERSION.tar.xz"
  if [[ ! -f "$tarball" ]]; then
    echo "Downloading Sparkle $SPARKLE_VERSION"
    curl -fsSL --proto '=https' -o "$tarball.part" \
      "https://github.com/sparkle-project/Sparkle/releases/download/$SPARKLE_VERSION/Sparkle-$SPARKLE_VERSION.tar.xz"
    mv "$tarball.part" "$tarball"
  fi
  sum="$(shasum -a 256 "$tarball" | cut -d' ' -f1)"
  if [[ "$sum" != "$SPARKLE_SHA256" ]]; then
    rm -f "$tarball"
    die "Sparkle-$SPARKLE_VERSION.tar.xz has SHA-256 $sum, not $SPARKLE_SHA256"
  fi
  rm -rf "$sparkle"
  mkdir -p "$sparkle"
  tar -xJf "$tarball" -C "$sparkle"
fi

# Updates, only with Flow's update key at hand: its public half, which
# generate_keys -p only looks up (the Keychain may ask once), must be the one
# the build trusts. So a fork's release never offers Flow's in its place.
updates=0
if [[ -z "$no_updates" ]]; then
  trusted="$(sed -n 's/^pub const UPDATE_PUBLIC_KEY: &str = "\(.*\)";$/\1/p' core/src/lib.rs)"
  held="$("${GENERATE_KEYS:-$sparkle/bin/generate_keys}" --account Flow -p 2>/dev/null | grep -Eo '[A-Za-z0-9+/]{43}=' | tail -1 || true)"
  if [[ -n "$trusted" && "$held" == "$trusted" ]]; then
    updates=1
  elif [[ -n "$adhoc" || -n "$allow_dirty" ]]; then
    echo "note: the Keychain holds no Flow update key that's UPDATE_PUBLIC_KEY, so this test build won't update itself."
  else
    die "the Keychain holds no Flow update key that's UPDATE_PUBLIC_KEY (account Flow; macos/RELEASING.md), so this release couldn't update itself. Pass --no-updates for a release that never does."
  fi
fi
said_updates="updates: off"
[[ "$updates" == 1 ]] && said_updates="updates: on"
echo "This build: $said_updates"
export FLOW_UPDATES="$updates"

export MACOSX_DEPLOYMENT_TARGET="$MIN_MACOS"
# The build's own folders out of the programs (panics name their source
# files): the home folder as ~ (Cargo's crates are under it) and the repo as
# flow, so a release names no one's account. Encoded, parted by 0x1f, as a
# folder may hold spaces.
export CARGO_ENCODED_RUSTFLAGS="--remap-path-prefix=$HOME=~"$'\x1f'"--remap-path-prefix=$repo=flow"
if [[ -z "$skip_build" ]]; then
  if pgrep -f "target/(aarch64|x86_64)-apple-darwin/release/flow" >/dev/null; then
    die "Flow is running from target/. Quit it so the build can replace the binary."
  fi
  for t in "${triples[@]}"; do
    cargo build --release --locked -p flow-terminal --bins --target "$t"
  done
fi

rm -rf "$dist"
mkdir -p "$dist/stage"
bin="$dist/stage/flow"
games=(flow-help flow-settings flowapp flow-mcp flow-command)
for name in flow "${games[@]}"; do
  if [[ "${#triples[@]}" -gt 1 ]]; then
    lipo -create -output "$dist/stage/$name" "target/${triples[0]}/release/$name" "target/${triples[1]}/release/$name"
  else
    cp "target/${triples[0]}/release/$name" "$dist/stage/$name"
  fi
done
reported="$("$bin" --version)"
[[ "$reported" == "Flow $shown (build $build)" ]] || die "the binary reports \"$reported\", not \"Flow $shown (build $build)\". Rebuild without --skip-build."
reported="$(FLOW_SAY_UPDATES=1 "$bin" --version | sed -n 2p)"
[[ "$reported" == "$said_updates" ]] || die "the binary says \"$reported\", not \"$said_updates\". Rebuild without --skip-build."

# THIRD-PARTY-NOTICES.txt: Sparkle, then every crate compiled into flow and
# the programs beside it.
notices="$dist/stage/THIRD-PARTY-NOTICES.txt"
target_args=()
for t in "${triples[@]}"; do target_args+=(--target "$t"); done
cargo run --quiet --locked --example third_party_notices -- "${target_args[@]}" \
  --package flow-terminal --binary "flow, flow-help, flow-settings, flowapp, flow-mcp and the flow command" --out "$dist/stage/crates.txt"
{
  cat <<EOF
Flow - third-party notices
==========================

Flow and the programs it carries, flow-help, flow-settings, flowapp, flow-mcp
and the flow command, are
built from the open-source Rust crates listed below, and Flow
ships Sparkle.framework for automatic updates and the fonts for its themes. Their copyright notices and
licenses follow.

-------------------------------------------------------------------------------
Sparkle.framework
-------------------------------------------------------------------------------

Sparkle $SPARKLE_VERSION, the automatic-update framework
    Source:  https://github.com/sparkle-project/Sparkle
    Its license, and the licenses of the code it includes:

EOF
  cat "$sparkle/LICENSE"
  echo
  cat "$repo/assets/fonts/NOTICES.txt"
  echo
  cat "$dist/stage/crates.txt"
} > "$notices"
rm "$dist/stage/crates.txt"

"$repo/macos/scripts/bundle.sh" "$bin" "$dist/stage" --sparkle "$sparkle/Sparkle.framework" --notices "$notices" --game "$dist/stage/flow-help" --game "$dist/stage/flow-settings" --game "$dist/stage/flowapp" --game "$dist/stage/flow-mcp" --game "$dist/stage/flow-command"
app="$dist/stage/Flow.app"

# Sign inside out: Sparkle's helpers (keeping their own entitlements), the
# framework, the programs in Helpers, then the app with Flow's entitlements. Developer ID signing
# turns on the hardened runtime and a secure timestamp, which notarization
# requires; ad hoc signatures can have neither.
if [[ -n "$adhoc" ]]; then
  echo "Signing ad hoc"
  sign=(codesign --force --sign -)
else
  echo "Signing as $identity"
  sign=(codesign --force --sign "$identity" --options runtime --timestamp)
fi
fw="$app/Contents/Frameworks/Sparkle.framework"
fwv="$fw/Versions/B"
[[ -d "$fwv" ]] || fwv="$fw/Versions/Current"
for helper in "$fwv/XPCServices/"*.xpc "$fwv/Updater.app"; do
  [[ -e "$helper" ]] && "${sign[@]}" --preserve-metadata=entitlements "$helper"
done
"${sign[@]}" "$fwv/Autoupdate"
"${sign[@]}" "$fw"
bundle_id="$(/usr/libexec/PlistBuddy -c "Print :CFBundleIdentifier" "$app/Contents/Info.plist")"
for helper in "$app/Contents/Helpers/"*; do
  "${sign[@]}" --identifier "$bundle_id.$(basename "$helper")" "$helper"
done
"${sign[@]}" --entitlements "$repo/macos/packaging/Flow.entitlements" "$app"
codesign --verify --deep --strict "$app"
echo "Signature verified"

# The DMG: Flow.app next to a link to /Applications.
dmg="$dist/Flow-$shown-macos.dmg"
root="$dist/dmg"
mkdir -p "$root"
ditto "$app" "$root/Flow.app"
ln -s /Applications "$root/Applications"
hdiutil create -quiet -volname "Flow $shown" -srcfolder "$root" -fs HFS+ -format UDZO -ov "$dmg"
rm -rf "$root"
[[ -n "$adhoc" ]] || codesign --force --sign "$identity" --timestamp "$dmg"

if [[ -n "$notarize" ]]; then
  echo "Notarizing (profile $NOTARY_PROFILE); this usually takes a few minutes"
  log="$dist/notarize.log"
  xcrun notarytool submit "$dmg" --keychain-profile "$NOTARY_PROFILE" --wait | tee "$log"
  if ! grep -q "status: Accepted" "$log"; then
    id="$(sed -n 's/^ *id: \(.*\)$/\1/p' "$log" | head -1)"
    die "notarization didn't succeed. Details: xcrun notarytool log $id --keychain-profile $NOTARY_PROFILE"
  fi
  xcrun stapler staple "$dmg"
  spctl --assess --type open --context context:primary-signature -v "$dmg"
fi

(cd "$dist" && shasum -a 256 "Flow-$shown-macos.dmg" > SHA256SUMS-macos.txt)

echo
echo "Built in $dist:"
ls -lh "$dist" | awk 'NR > 1 && $1 !~ /^d/ { printf "  %-28s %s\n", $9, $5 }'
if [[ -n "$adhoc" ]]; then
  echo
  echo "This is an ad hoc build: it runs on this Mac only. Don't publish it."
  exit 0
fi
cat <<EOF

Next (macos/RELEASING.md has the details):
  1. gh release create v$shown-macos --repo arcanii/Flow-releases --title "Flow $shown for macOS" --latest=false \\
        target/dist/Flow-$shown-macos.dmg target/dist/SHA256SUMS-macos.txt
  2. macos/scripts/make-appcast.sh --dmg target/dist/Flow-$shown-macos.dmg --tag v$shown-macos
     (signs it with Flow's key from the Keychain and writes target/dist/appcast-macos.xml)
  3. Commit target/dist/appcast-macos.xml as appcast-macos.xml on main in arcanii/Flow-releases.
  4. In that repo's README.md, point the macOS links at v$shown-macos, then commit and push.
EOF
[[ -n "$notarize" ]] || echo "Not notarized: run again with --notarize before publishing."
[[ "$updates" == 1 ]] || echo "It never updates itself (--no-updates): don't put it in Flow's appcast, where every Flow would update to it and then stop."
