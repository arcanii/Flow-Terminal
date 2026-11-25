#!/usr/bin/env bash
# Make one of flowapp's apps for Linux, as macos/scripts/app-release.sh does
# on the Mac and windows/scripts/app-release.ps1 on Windows: build it against
# glibc 2.31 with cargo-zigbuild (as package.sh builds Flow), pack it as
# target/apps/<app>-<version>-linux-<arch>.tar.gz, unpack that into a scratch
# folder and run each program's --version (and have the SDK's Python package
# load its own library). Then, given its signature by Flow's update key,
# write its entry into flowapp's catalog (apps.json, core/src/apps.rs;
# examples/app_catalog.rs writes the entry, checking the signature first).
#
#   linux/scripts/app-release.sh <app> [--glibc <version> | --native] [--skip-build]
#   linux/scripts/app-release.sh <app> (--signature <base64> | --key-file <file>) [--catalog <file>] [--flow <version>]
#
# <app>: flow-sdk, the SDK's C library (libflow_sdk_c.so and the static
# libflow_sdk_c.a) with its header, its Python package (with the library in
# it, where it looks first) and flow-demo, built from Flow-SDK's tag (the one
# Cargo.toml's flow-sdk names), cloned into target/flow-sdk-<tag>; or
# flowapp itself, built from this checkout at flowapp's own version
# (`flowapp` under [workspace.metadata.flow]), which runs in Flow's tabs
# ahead of the copy Flow carries (docs/design/flowapp-updates.md), from a
# clean tree, as its build number names the commit. The games' downloads are made in their own repository, arcanii/Flow-Games, by
# its scripts/app-release-linux.sh, in the same two steps.
#
# The update key's private half isn't on a Linux computer
# (linux/RELEASING.md), so this goes in two steps, as a Linux release of Flow
# does. The first makes the tarball and says what's next: it goes on the
# app's release in arcanii/Flow-releases, and is signed on the Mac from there
# (windows/scripts/sign-release.sh), which proves the published file is the
# one signed. The second, given that signature, writes the entry for the
# tarball as the first made it into the catalog (--catalog, by default
# target/apps/apps.json, begun from the published one); with --key-file (the
# key's base64 seed, as examples/update_key.rs reads it) it signs here
# instead. The catalog then goes to that repo's main as apps.json, where
# flowapp update reads it.
#
# Options:
#   --glibc <version>   the glibc to build for (default 2.31, as package.sh)
#   --native            build with plain cargo, for this computer's glibc and newer
#   --skip-build        pack what target/ has already
#   --signature <sig>   the tarball's signature, from sign-release.sh on the Mac
#   --key-file <file>   sign here with the key's file instead
#   --catalog <file>    the catalog to write into
#   --flow <version>    the oldest Flow the app works in (0.1.34), which an
#                       older Flow's flowapp holds it back for; none for
#                       any (without it, what the catalog said before;
#                       flowapp's is 0.1.44 unless given)
#   --allow-dirty       build flowapp from a tree with changes, for a test

set -euo pipefail

RELEASES="arcanii/Flow-releases"
CATALOG_URL="https://raw.githubusercontent.com/$RELEASES/main/apps.json"
die() { echo "error: $*" >&2; exit 1; }
usage="usage: $(basename "$0") <app> [--glibc <version> | --native] [--skip-build], then $(basename "$0") <app> (--signature <base64> | --key-file <file>) [--catalog <file>] [--flow <version>]"

[[ $# -ge 1 ]] || die "$usage"
app="$1"; shift
glibc=2.31; skip_build=""; sig=""; key_file=""; catalog=""; flow=""; allow_dirty=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --glibc) glibc="${2:-}"; [[ "$glibc" =~ ^2\.[0-9]+$ ]] || die "$usage"; shift 2 ;;
    --native) glibc=""; shift ;;
    --skip-build) skip_build=1; shift ;;
    --signature) sig="${2:-}"; [[ -n "$sig" ]] || die "$usage"; shift 2 ;;
    --key-file) key_file="${2:-}"; [[ -n "$key_file" ]] || die "$usage"; shift 2 ;;
    --catalog) catalog="${2:-}"; [[ -n "$catalog" ]] || die "$usage"; shift 2 ;;
    --flow) flow="${2:-}"; [[ -n "$flow" ]] || die "$usage"; shift 2 ;;
    --allow-dirty) allow_dirty=1; shift ;;
    *) die "unknown option $1 (see the top of $0)" ;;
  esac
done
[[ -z "$sig" || -z "$key_file" ]] || die "give --signature or --key-file, not both"

# This script is in linux/scripts; the repo root is two levels up.
repo="$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)"
cd "$repo"
triple="$(rustc -vV | sed -n 's/^host: //p')"
[[ -n "$triple" ]] || die "rustc names no host target"
# As flowapp names this system and chip (apps::platform: Rust's OS and ARCH).
platform="linux-${triple%%-*}"
out="$repo/target/apps"
mkdir -p "$out"
[[ -n "$catalog" ]] || catalog="$out/apps.json"

# The build's own folders out of the programs (panics name their source
# files): the home folder as ~ (Cargo's crates are under it) and the repo as
# flow, so a download names no one's account. Encoded, parted by 0x1f, as a
# folder may hold spaces. The same as package.sh's, so the two share builds.
export CARGO_ENCODED_RUSTFLAGS="--remap-path-prefix=$HOME=~"$'\x1f'"--remap-path-prefix=$repo=flow"

# What each app is: its programs and its line in the catalog, the same as
# app-release.sh's.
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
    die "$app's download is made in Flow-Games: scripts/app-release-linux.sh $app there" ;;
  *) die "no app called $app here: flow-sdk or flowapp (the games' are made in Flow-Games)" ;;
esac

if [[ "$app" == flowapp ]]; then
  version="$(sed -n 's/^flowapp = "\([^"]*\)"$/\1/p' Cargo.toml)"
  [[ -n "$version" ]] || die "Cargo.toml names no flowapp version under [workspace.metadata.flow]"
else
  tag="$(sed -n 's/^flow-sdk = { git = "[^"]*", tag = "\([^"]*\)" }$/\1/p' Cargo.toml)"
  [[ -n "$tag" ]] || die "Cargo.toml names no tag for flow-sdk"
  version="${tag#v}"
fi
file="$app-$version-$platform.tar.gz"
tarball="$out/$file"
url="https://github.com/$RELEASES/releases/download/$app-v$version/$file"

# The second step: the signed tarball's entry, into the catalog.
if [[ -n "$sig" || -n "$key_file" ]]; then
  [[ -f "$tarball" ]] || die "no $tarball: make it first, without --signature"
  if [[ -n "$key_file" ]]; then
    sig="$(cargo run --quiet --locked --example update_key -- sign "$tarball" "$key_file")"
  fi
  # Begun from the published catalog: one begun empty would drop the other
  # systems' downloads when it's published.
  if [[ ! -f "$catalog" ]]; then
    curl -fsSL "$CATALOG_URL" -o "$catalog" || { rm -f "$catalog"; die "couldn't read the published catalog ($CATALOG_URL); pass --catalog with a copy of it"; }
  fi
  extra=()
  [[ -z "$flow" ]] || extra=(--flow "$flow")
  [[ -z "$bundled" ]] || extra+=(--bundled "$bundled")
  cargo run --quiet --locked --example app_catalog -- --catalog "$catalog" --app "$app" --version "$version" \
    --summary "$summary" --programs "$(IFS=,; echo "${programs[*]}")" ${extra[@]+"${extra[@]}"} \
    --platform "$platform" --url "$url" --file "$tarball" --signature "$sig"
  echo
  echo "Next:"
  echo "  1. gh release upload $app-v$version --repo $RELEASES $tarball.sig"
  echo "  2. In a Flow-releases checkout, pull, copy $catalog to apps.json, check with"
  echo "     git diff that only the apps you made changed, commit and push to main."
  exit 0
fi

# The first step: build, stage, pack and try it.
if [[ -n "$glibc" ]]; then
  if [[ -z "$skip_build" ]]; then
    command -v zig >/dev/null && cargo zigbuild --help >/dev/null 2>&1 \
      || die "zig and cargo-zigbuild are needed to build for glibc $glibc (linux/RELEASING.md), or pass --native"
  fi
  build=(cargo zigbuild --release --locked --target "$triple.$glibc")
  dir="$triple/release"
else
  build=(cargo build --release --locked)
  dir="release"
fi

stage="$out/stage/$app"
rm -rf "$stage"
mkdir -p "$stage/bin"

libraries=()
if [[ "$app" == flowapp ]]; then
  # flowapp, from this checkout, at its own version; the build number is
  # the commit count plus build_offset, so the tree must be the commit's.
  [[ -n "$allow_dirty" || -z "$(git status --porcelain)" ]] || die "the tree has changes: commit them first (or --allow-dirty for a test)"
  [[ -n "$skip_build" ]] || "${build[@]}" -p flow-terminal --bin flowapp
  install -m 755 "$repo/target/$dir/flowapp" "$stage/bin/"
  cargo run --quiet --locked --example third_party_notices -- --target "$triple" \
    --binary "flowapp" --out "$stage/THIRD-PARTY-NOTICES.txt"
else
  # The SDK, from the tag Flow takes it by, used only as it was tagged.
  sdk="$repo/target/flow-sdk-$tag"
  if [[ ! -d "$sdk" ]]; then
    git clone --quiet --depth 1 --branch "$tag" https://github.com/arcanii/Flow-SDK "$sdk"
  fi
  [[ "$(git -C "$sdk" rev-parse HEAD)" = "$(git -C "$sdk" rev-parse "$tag^{commit}")" && -z "$(git -C "$sdk" status --porcelain)" ]] \
    || die "$sdk isn't Flow-SDK's $tag as it was tagged. Delete it to clone it again."
  built="$repo/target/flow-sdk-build"
  [[ -n "$skip_build" ]] || "${build[@]}" --manifest-path "$sdk/Cargo.toml" -p flow-sdk-c -p flow-demos --target-dir "$built"
  rel="$built/$dir"
  install -m 755 "$rel/flow-demo" "$stage/bin/"
  mkdir -p "$stage/lib" "$stage/include" "$stage/python"
  install -m 755 "$rel/libflow_sdk_c.so" "$stage/lib/"
  install -m 644 "$rel/libflow_sdk_c.a" "$stage/lib/"
  install -m 644 "$sdk/c/include/flow_sdk.h" "$stage/include/"
  cp -R "$sdk/python/flow_sdk" "$stage/python/"
  find "$stage/python" -name "__pycache__" -prune -exec rm -rf {} +
  # Python finds the library in its own folder first.
  install -m 755 "$rel/libflow_sdk_c.so" "$stage/python/flow_sdk/"
  install -m 644 "$sdk/LICENSE-MIT" "$sdk/LICENSE-APACHE" "$stage/"
  cargo run --quiet --locked --example third_party_notices -- --manifest-path "$sdk/Cargo.toml" --target "$triple" \
    --package flow-sdk-c --package flow-demos --binary "the Flow SDK's library and flow-demo" --out "$stage/THIRD-PARTY-NOTICES.txt"
  libraries=(lib/libflow_sdk_c.so)
fi
echo "$app $version for $platform"

# The download: what's staged, at the tarball's top, owned by 0 with no
# names (else each entry would carry the builder's account), in an order
# that doesn't hang on the disk's.
tar -C "$stage" --sort=name --owner=0 --group=0 --numeric-owner --mode=u+rwX,go+rX,go-w -czf "$tarball" .

# Unpacked as flowapp unpacks it, and tried, before anyone signs it.
tried="$(mktemp -d "${TMPDIR:-/tmp}/flow-app.XXXXXX")"
trap 'rm -rf "$tried"' EXIT
tar -xzf "$tarball" -C "$tried"
for p in "${programs[@]}"; do
  said="$("$tried/bin/$p" --version)" || die "bin/$p --version failed"
  [[ "$said" == "$p "* ]] || die "bin/$p --version printed '$said'"
  echo "  $said"
done
# No program or library may need a newer glibc than the one built for.
if [[ -n "$glibc" ]]; then
  for f in "${programs[@]/#/bin/}" ${libraries[@]+"${libraries[@]}"}; do
    need="$(objdump -T "$tried/$f" | grep -oE 'GLIBC_2\.[0-9]+' | sort -t. -k2 -n -u | tail -1)"
    [[ "${need#GLIBC_2.}" -le "${glibc#2.}" ]] || die "$f needs $need, newer than glibc $glibc"
    echo "  $f needs $need at most"
  done
fi
# Nothing in it may name the build's folders.
if grep -rlF --binary-files=text -e "$HOME" -e "$repo" "$tried" >/dev/null; then
  die "$(grep -rlF --binary-files=text -e "$HOME" -e "$repo" "$tried" | sed "s|^$tried/||" | tr '\n' ' ')name the build's folders"
fi
# The package loads its own copy of the library, whatever Cargo has built.
if [[ ! -d "$tried/python" ]]; then
  :
elif command -v python3 >/dev/null; then
  loaded="$(env -u FLOW_SDK_LIB -u CARGO_TARGET_DIR python3 -I -B -c 'import sys; sys.path.insert(0, sys.argv[1]); from flow_sdk import _native as n; n.lib(); print(next(p for p in n.candidates() if p.is_file()), n.ABI)' "$tried/python")" \
    || die "the Python package didn't load its library"
  [[ "$loaded" = "$tried/python/flow_sdk/libflow_sdk_c.so 1" ]] || die "the Python package loaded '$loaded', not its own library"
  echo "  Python's flow_sdk loads its own libflow_sdk_c.so"
else
  echo "warning: no python3 here, so the Python package went untried" >&2
fi

sha="$(sha256sum "$tarball" | cut -d' ' -f1)"
echo
echo "Made $tarball ($(stat -c %s "$tarball") bytes, SHA-256 $sha)."
echo "Next:"
echo "  1. Where the release $app-v$version is (the Mac's download of it):"
echo "        gh release upload $app-v$version --repo $RELEASES $tarball"
echo "     else:"
echo "        gh release create $app-v$version --repo $RELEASES --title \"$app $version\" --latest=false --notes \"…\" $tarball"
echo "  2. On the Mac, sign what was published:"
echo "        curl -fLO $url"
echo "        windows/scripts/sign-release.sh $file"
echo "  3. Here: $0 $app --signature <its signature>"
