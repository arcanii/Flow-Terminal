#!/usr/bin/env bash
# Make one of flowapp's apps for the Mac: build it, sign it with the
# Developer ID (and with --notarize have Apple notarize it), pack it as
# target/apps/<app>-<version>-macos-aarch64.tar.gz, sign that with Flow's
# update key, and write its entry into flowapp's catalog (apps.json,
# core/src/apps.rs; examples/app_catalog.rs writes the entry).
#
#   macos/scripts/app-release.sh <app> [options]
#
# <app>: flow-sdk, the SDK's C library (shared and static) with its header,
# its Python package (with the library in it, where it looks first) and
# flow-demo, built from Flow-SDK's tag (the one Cargo.toml's flow-sdk names);
# or flowapp itself, built from this checkout at flowapp's own version
# (`flowapp` under [workspace.metadata.flow]), which runs in Flow's tabs
# ahead of the copy Flow carries (docs/design/flowapp-updates.md). The
# checkout must be clean for it, as its build number names the commit.
# The games' downloads (flow-rogue, flow-mines, flow-invaders) are made in
# their own repository, arcanii/Flow-Games, by its scripts/app-release.sh,
# which runs this repository's examples as this does.
#
# Options:
#   --catalog <file>    the catalog to write into (default target/apps/apps.json,
#                       begun from the one published in Flow-releases)
#   --notarize          have Apple notarize the programs (a zip of them)
#   --identity <name>   the codesigning identity (default: the keychain's
#                       "Developer ID Application" certificate)
#   --adhoc             sign ad hoc, for a test on this Mac only
#   --skip-build        pack what target/ has already
#   --flow <version>    the oldest Flow the app works in (0.1.34), which an
#                       older Flow's flowapp holds it back for; none for
#                       any (without it, what the catalog said before;
#                       flowapp's is 0.1.44 unless given)
#   --allow-dirty       build flowapp from a tree with changes, for a test
#
# Environment: NOTARY_PROFILE, as macos/scripts/release.sh takes it.
#
# Then, as it prints: the tarball goes on a release <app>-v<version> in
# arcanii/Flow-releases (--latest=false), and the catalog to that repo's
# main as apps.json. flowapp update reads it from there.

set -euo pipefail

NOTARY_PROFILE="${NOTARY_PROFILE:-Flow-notarize}"
RELEASES="arcanii/Flow-releases"
CATALOG_URL="https://raw.githubusercontent.com/$RELEASES/main/apps.json"
die() { echo "error: $*" >&2; exit 1; }

[[ $# -ge 1 ]] || die "usage: $(basename "$0") <app> [--catalog <file>] [--notarize] [--identity <name>] [--adhoc] [--skip-build] [--flow <version>] [--allow-dirty]"
app="$1"; shift
catalog=""; notarize=""; identity=""; adhoc=""; skip_build=""; flow=""; allow_dirty=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --catalog) catalog="${2:-}"; shift 2 ;;
    --notarize) notarize=1; shift ;;
    --identity) identity="${2:-}"; shift 2 ;;
    --adhoc) adhoc=1; shift ;;
    --skip-build) skip_build=1; shift ;;
    --flow) flow="${2:-}"; [[ -n "$flow" ]] || die "--flow needs a version, or none"; shift 2 ;;
    --allow-dirty) allow_dirty=1; shift ;;
    *) die "unknown option $1 (see the top of $0)" ;;
  esac
done
[[ -z "$adhoc" || -z "$notarize" ]] || die "an ad hoc build can't be notarized"

repo="$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)"
cd "$repo"
triple="aarch64-apple-darwin"
platform="macos-aarch64"
out="$repo/target/apps"
mkdir -p "$out"
[[ -n "$catalog" ]] || catalog="$out/apps.json"

# The build's own folders out of the programs (panics name their source
# files): the home folder as ~ (Cargo's crates are under it) and the repo as
# flow, so a download names no one's account. Encoded, parted by 0x1f, as a
# folder may hold spaces.
export CARGO_ENCODED_RUSTFLAGS="--remap-path-prefix=$HOME=~"$'\x1f'"--remap-path-prefix=$repo=flow"

# What each app is: its programs and its line in the catalog.
bundled=""
case "$app" in
  flow-sdk)
    programs=(flow-demo)
    summary="Flow's SDK for C and Python, its library, header and Python package, with flow-demo, its demos" ;;
  flowapp)
    programs=(flowapp)
    bundled=flowapp
    [[ -n "$flow" ]] || flow="0.1.44"
    summary="flowapp, which keeps Flow's apps: newer than the copy your Flow carries, it runs in Flow's tabs in that one's place" ;;
  flow-rogue|flow-mines|flow-invaders)
    die "$app's download is made in Flow-Games: scripts/app-release.sh $app there" ;;
  *) die "no app called $app here: flow-sdk or flowapp (the games' are made in Flow-Games)" ;;
esac

stage="$out/stage/$app"
rm -rf "$stage"
mkdir -p "$stage/bin"

if [[ "$app" == flowapp ]]; then
  # flowapp, from this checkout, at its own version; the build number is
  # the commit count plus build_offset, so the tree must be the commit's.
  [[ -n "$allow_dirty" || -z "$(git status --porcelain)" ]] || die "the tree has changes: commit them first (or --allow-dirty for a test)"
  version="$(sed -n 's/^flowapp = "\([^"]*\)"$/\1/p' Cargo.toml)"
  [[ -n "$version" ]] || die "Cargo.toml names no flowapp version under [workspace.metadata.flow]"
  [[ -n "$skip_build" ]] || cargo build --release --locked -p flow-terminal --bin flowapp --target "$triple"
  rel="$repo/target/$triple/release"
  said="$("$rel/flowapp" --version)"
  [[ "$said" == "flowapp $version "* ]] || die "the flowapp built says \"$said\", not $version"
  cp "$rel/flowapp" "$stage/bin/"
  cargo run --quiet --locked --example third_party_notices -- --target "$triple" \
    --binary "flowapp" --out "$stage/THIRD-PARTY-NOTICES.txt"
else
  # The SDK, from the tag Flow takes it by.
  tag="$(sed -n 's/^flow-sdk = { git = "[^"]*", tag = "\([^"]*\)" }$/\1/p' Cargo.toml)"
  [[ -n "$tag" ]] || die "Cargo.toml names no tag for flow-sdk"
  version="${tag#v}"
  sdk="$repo/target/flow-sdk-$tag"
  if [[ ! -d "$sdk" ]]; then
    git clone --quiet --depth 1 --branch "$tag" https://github.com/arcanii/Flow-SDK "$sdk"
  fi
  built="$repo/target/flow-sdk-build"
  [[ -n "$skip_build" ]] || cargo build --release --locked --manifest-path "$sdk/Cargo.toml" -p flow-sdk-c -p flow-demos --target "$triple" --target-dir "$built"
  rel="$built/$triple/release"
  cp "$rel/flow-demo" "$stage/bin/"
  mkdir -p "$stage/lib" "$stage/include" "$stage/python"
  cp "$rel/libflow_sdk_c.dylib" "$rel/libflow_sdk_c.a" "$stage/lib/"
  cp "$sdk/c/include/flow_sdk.h" "$stage/include/"
  cp -R "$sdk/python/flow_sdk" "$stage/python/"
  find "$stage/python" -name "__pycache__" -prune -exec rm -rf {} +
  # Python finds the library in its own folder first.
  cp "$rel/libflow_sdk_c.dylib" "$stage/python/flow_sdk/"
  cp "$sdk/LICENSE-MIT" "$sdk/LICENSE-APACHE" "$stage/"
  cargo run --quiet --locked --example third_party_notices -- --manifest-path "$sdk/Cargo.toml" --target "$triple" \
    --package flow-sdk-c --package flow-demos --binary "the Flow SDK's library and flow-demo" --out "$stage/THIRD-PARTY-NOTICES.txt"
fi
echo "$app $version for $platform"

# Signed, inside out: every Mach-O in it, each as Flow's.
if [[ -n "$adhoc" ]]; then
  sign=(codesign --force --sign -)
else
  if [[ -z "$identity" ]]; then
    identity="$(security find-identity -v -p codesigning | sed -n 's/.*"\(Developer ID Application: [^"]*\)".*/\1/p' | head -1)"
    [[ -n "$identity" ]] || die "no Developer ID Application certificate in the keychain. Pass --identity, or --adhoc for a test."
  fi
  sign=(codesign --force --sign "$identity" --options runtime --timestamp)
fi
while IFS= read -r -d '' f; do
  if file -b "$f" | grep -q "Mach-O"; then
    "${sign[@]}" --identifier "com.arcanii.Flow.$(basename "$f" | sed 's/\.dylib$//')" "$f"
    codesign --verify --strict "$f"
  fi
done < <(find "$stage" -type f \( -path "*/bin/*" -o -name "*.dylib" \) -print0)

if [[ -n "$notarize" ]]; then
  zip="$out/$app-notarize.zip"
  rm -f "$zip"
  ditto -c -k --keepParent "$stage" "$zip"
  log="$out/$app-notarize.log"
  echo "Notarizing (profile $NOTARY_PROFILE)"
  xcrun notarytool submit "$zip" --keychain-profile "$NOTARY_PROFILE" --wait | tee "$log"
  grep -q "status: Accepted" "$log" || die "notarization didn't succeed: see $log"
  rm -f "$zip"
fi

# The download: what's staged, at the tarball's top, no macOS metadata.
file="$app-$version-$platform.tar.gz"
tarball="$out/$file"
# The system's tar (bsdtar), not a GNU one first on the PATH. Its entries
# are owned by 0 and no name, or each would carry the builder's account and
# group; tar unpacking as a user makes them that user's anyway.
COPYFILE_DISABLE=1 /usr/bin/tar --no-mac-metadata --uid 0 --gid 0 --uname "" --gname "" -czf "$tarball" -C "$stage" .

# Signed with Flow's key, as the appcasts' downloads are: by the sign_update
# named in SIGN_UPDATE, else Sparkle's own, which release.sh fetched and
# checked against its pinned checksum into target/sparkle-<version> (its
# SPARKLE_VERSION), and no other copy.
tool="${SIGN_UPDATE:-}"
if [[ -z "$tool" ]]; then
  sparkle_version="${SPARKLE_VERSION:-$(sed -n 's/^SPARKLE_VERSION="\${SPARKLE_VERSION:-\([^}]*\)}"$/\1/p' "$repo/macos/scripts/release.sh")}"
  c="$repo/target/sparkle-$sparkle_version/bin/sign_update"
  if [[ -n "$sparkle_version" && -x "$c" ]]; then tool="$c"; fi
fi
[[ -n "$tool" ]] || die "sign_update not found. Run macos/scripts/release.sh once (it fetches Sparkle), or set SIGN_UPDATE."
signed="$("$tool" --account Flow "$tarball")"
sig="$(printf '%s\n' "$signed" | sed -n 's/.*sparkle:edSignature="\([^"]*\)".*/\1/p')"
[[ -n "$sig" ]] || die "could not read the signature from sign_update: $signed"

# Its entry, into the catalog, begun from the published one.
if [[ ! -f "$catalog" ]]; then
  curl -fsSL "$CATALOG_URL" -o "$catalog" 2>/dev/null || rm -f "$catalog"
fi
url="https://github.com/$RELEASES/releases/download/$app-v$version/$file"
extra=()
[[ -z "$flow" ]] || extra=(--flow "$flow")
[[ -z "$bundled" ]] || extra+=(--bundled "$bundled")
cargo run --quiet --locked --example app_catalog -- --catalog "$catalog" --app "$app" --version "$version" \
  --summary "$summary" --programs "$(IFS=,; echo "${programs[*]}")" ${extra[@]+"${extra[@]}"} \
  --platform "$platform" --url "$url" --file "$tarball" --signature "$sig"

echo
echo "Made $tarball ($(du -h "$tarball" | cut -f1 | tr -d ' '))."
echo "Next:"
echo "  1. gh release create $app-v$version --repo $RELEASES --title \"$app $version\" --latest=false \\"
echo "        --notes \"…\" $tarball $tarball.sig"
echo "  2. Commit $catalog as apps.json on main in $RELEASES."
