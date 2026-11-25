#!/usr/bin/env bash
#
# Make Flow's Linux release: a tarball holding flow, flow-help, flow-settings,
# flowapp, the menu entry, the icons, the third-party notices and install.sh,
# and the same as a .deb for Debian and Ubuntu. From 0.1.34 the games and the
# demos come from flowapp, not with Flow.
#
#   linux/scripts/package.sh [--out <dir>] [--glibc <version> | --native] [--no-build] [--allow-dirty]
#       [--updates | --no-updates]
#
# Run from anywhere in the repo. It builds flow, flow-help, flow-settings and
# flowapp in release mode (unless --no-build, which takes what the build
# folder holds), then writes <out>/Flow-<version>-linux-<arch>.tar.gz and its .sha256
# (<out> is target/linux unless given). The tarball unpacks to
# flow-<version>/, laid out as it installs:
#
#   bin/flow
#   lib/flow-terminal/games/flow-help, flow-settings, flowapp
#   share/applications/com.arcanii.Flow.desktop
#   share/icons/hicolor/<size>/apps/com.arcanii.Flow.png
#   THIRD-PARTY-NOTICES.txt
#   install.sh
#
# Where dpkg-deb is, it also writes <out>/Flow-<version>-linux-<Debian arch>.deb
# (amd64, arm64) and its .sha256: package flow-terminal, version
# <marketing version>.<build>, the same files under /usr (the notices in
# /usr/share/doc/flow-terminal), for `sudo apt install ./<file>`. Flow
# installed so can't update itself: its updater opens the release page for
# the next .deb, and About gives the `sudo apt install ~/Downloads/<file>`
# command for it, naming the file as this script does.
#
# A program linked on a new Linux needs that Linux's glibc (Ubuntu 26.04's
# 2.43, for one), and older ones refuse to start it. So it builds with
# cargo-zigbuild and zig against glibc 2.31 (--glibc picks another), which
# Ubuntu 20.04, Debian 11 and everything since have: the result goes in
# target/<triple>/release. --native builds with plain cargo into
# target/release instead, for a tarball that runs on this computer's Linux
# and newer only. Install zig and cargo-zigbuild as linux/RELEASING.md says.
#
# The build number is the commit count (plus build_offset), so a tree with
# changes is refused unless --allow-dirty: its build would claim a commit it
# doesn't match.
#
# Flow updates itself only in a build made as one of its releases
# (docs/design/updates-for-releases.md), FLOW_UPDATES=1 as it's built. On the
# Mac and Windows the release scripts tell by Flow's update key, which isn't
# here (Linux's tarballs are signed on the Mac afterwards), so a release says
# which it is: --updates for Flow's own, --no-updates for one that never
# updates itself (a fork's, until it has a key and feeds of its own). A test
# build (--allow-dirty) is made without updates unless given --updates.
# Nothing is published.

set -euo pipefail

usage() {
  echo "usage: $(basename "$0") [--out <dir>] [--glibc <version> | --native] [--no-build] [--allow-dirty] [--updates | --no-updates]" >&2
  exit 1
}
die() { echo "error: $*" >&2; exit 1; }

# This script is in linux/scripts; the repo root is two levels up.
repo="$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)"
out="$repo/target/linux"; build=1; allow_dirty=0; glibc=2.31; updates=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --out) out="${2:-}"; [[ -n "$out" ]] || usage; shift 2 ;;
    --glibc) glibc="${2:-}"; [[ "$glibc" =~ ^2\.[0-9]+$ ]] || usage; shift 2 ;;
    --native) glibc=""; shift ;;
    --no-build) build=0; shift ;;
    --allow-dirty) allow_dirty=1; shift ;;
    --updates) updates=1; shift ;;
    --no-updates) updates=0; shift ;;
    *) usage ;;
  esac
done
mkdir -p "$out"
out="$(CDPATH= cd -- "$out" && pwd)"
cd "$repo"

if [[ "$allow_dirty" = 0 && -n "$(git status --porcelain)" ]]; then
  die "the tree has changes; commit them, or pass --allow-dirty for a build that's only for this computer"
fi
# Work toward a version (`dev = true` under [workspace.metadata.flow]) isn't
# released: Flow says 0.1.40.DEV until releasing sets it to false. A test
# build keeps it, and its files say so.
if [[ "$allow_dirty" = 0 ]] && awk '/^\[workspace\.metadata\.flow\]/ { f = 1; next } /^\[/ { f = 0 } f && /^dev *= *true/ { found = 1 } END { exit !found }' Cargo.toml; then
  die "Flow is still worked toward (dev = true under [workspace.metadata.flow]): set it to false and commit to release it, or pass --allow-dirty for a test build"
fi
if [[ "$allow_dirty" = 0 && -z "$updates" ]]; then
  die "say whether this release updates itself: --updates for Flow's own (its tarball signed with Flow's key on the Mac), or --no-updates for one that never does"
fi
said_updates="updates: off"
[[ "$updates" = 1 ]] && said_updates="updates: on"
echo "This build: $said_updates"
export FLOW_UPDATES="${updates:-0}"
triple="$(rustc -vV | sed -n 's/^host: //p')"
[[ -n "$triple" ]] || die "rustc names no host target"
packages=(-p flow-terminal)
# The build's own folders out of the programs (panics name their source
# files): the home folder as ~ (Cargo's crates are under it) and the repo as
# flow, so a release names no one's account. Encoded, parted by 0x1f, as a
# folder may hold spaces.
export CARGO_ENCODED_RUSTFLAGS="--remap-path-prefix=$HOME=~"$'\x1f'"--remap-path-prefix=$repo=flow"
if [[ -n "$glibc" ]]; then
  bin="$repo/target/$triple/release"
  if [[ "$build" = 1 ]]; then
    command -v zig >/dev/null && cargo zigbuild --help >/dev/null 2>&1 \
      || die "zig and cargo-zigbuild are needed to build for glibc $glibc (linux/RELEASING.md), or pass --native"
    cargo zigbuild --release --locked "${packages[@]}" --target "$triple.$glibc"
  fi
else
  bin="$repo/target/release"
  if [[ "$build" = 1 ]]; then
    cargo build --release --locked "${packages[@]}"
  fi
fi
programs=(flow flow-help flow-settings flowapp flow-mcp)
for b in "${programs[@]}"; do
  [[ -x "$bin/$b" ]] || die "no $bin/$b; build it, or leave out --no-build"
done
# No program may need a newer glibc than the one built for.
if [[ -n "$glibc" ]]; then
  for b in "${programs[@]}"; do
    need="$(objdump -T "$bin/$b" | grep -oE 'GLIBC_2\.[0-9]+' | sort -t. -k2 -n -u | tail -1)"
    [[ "${need#GLIBC_2.}" -le "${glibc#2.}" ]] || die "$b needs $need, newer than glibc $glibc"
  done
fi

# "Flow 0.1.0 (build 42)" -> 0.1.0 and 42; a patch release says 0.1.0.1,
# which names the files as its tag does.
reported="$("$bin/flow" --version)"
version="$(printf '%s\n' "$reported" | sed -n 's/^[^ ]* \([^ ]*\) (build \([0-9]*\))$/\1/p')"
build_number="$(printf '%s\n' "$reported" | sed -n 's/^[^ ]* \([^ ]*\) (build \([0-9]*\))$/\2/p')"
[[ -n "$version" && -n "$build_number" ]] || die "unexpected version output: $reported"
said="$(FLOW_SAY_UPDATES=1 "$bin/flow" --version | sed -n 2p)"
[[ "$said" == "$said_updates" ]] || die "$bin/flow says \"$said\", not \"$said_updates\"; build it again, without --no-build"
# dpkg's version is the marketing version and the build, which only rise, as
# the updaters compare: with a patch's fourth number, 0.1.31.1.330 would sort
# below an earlier 0.1.31.325, and apt would take it for a downgrade.
deb_version="$(printf '%s\n' "$version" | cut -d. -f1-3).$build_number"
# The newest glibc any of them needs, which the .deb depends on.
glibc_need="$(for b in "${programs[@]}"; do objdump -T "$bin/$b"; done | grep -oE 'GLIBC_2\.[0-9]+' | sort -t. -k2 -n -u | tail -1)"
glibc_need="${glibc_need#GLIBC_}"
# What the binaries were built for, which a 32-bit system on a 64-bit kernel
# tells apart and `uname -m` doesn't.
arch="${triple%%-*}"

id=com.arcanii.Flow
name="flow-$version"
stage="$(mktemp -d "${TMPDIR:-/tmp}/flow-package.XXXXXX")"
trap 'rm -rf "$stage"' EXIT
dir="$stage/$name"
mkdir -p "$dir/bin" "$dir/lib/flow-terminal/games" "$dir/share/applications"

install -m 755 "$bin/flow" "$dir/bin/flow"
install -m 755 "$bin/flow-help" "$bin/flow-settings" "$bin/flowapp" "$bin/flow-mcp" "$dir/lib/flow-terminal/games/"
install -m 644 "$repo/linux/packaging/$id.desktop" "$dir/share/applications/$id.desktop"
# The sizes the hicolor theme lists; assets/icon has a few others for Windows.
for size in 16 24 32 48 64 128 256 512; do
  mkdir -p "$dir/share/icons/hicolor/${size}x${size}/apps"
  install -m 644 "$repo/assets/icon/flow-$size.png" "$dir/share/icons/hicolor/${size}x${size}/apps/$id.png"
done
install -m 755 "$repo/linux/scripts/install.sh" "$dir/install.sh"

# THIRD-PARTY-NOTICES.txt: every crate compiled into flow and the programs
# beside it.
cargo run --quiet --locked --example third_party_notices -- --target "$triple" \
  --package flow-terminal \
  --binary "flow, flow-help, flow-settings, flowapp and flow-mcp" --out "$stage/crates.txt"
{
  cat <<EOF
Flow - third-party notices
==========================

Flow and the programs it carries, flow-help, flow-settings and flowapp, are
built from the open-source Rust crates listed below, and Flow
carries the fonts for its themes. Their copyright notices and licenses follow.

EOF
  cat "$repo/assets/fonts/NOTICES.txt"
  echo
  cat "$stage/crates.txt"
} > "$dir/THIRD-PARTY-NOTICES.txt"
rm "$stage/crates.txt"

tarball="Flow-$version-linux-$arch.tar.gz"
tar -C "$stage" --sort=name --owner=0 --group=0 --numeric-owner --mode=u+rwX,go+rX,go-w -czf "$out/$tarball" "$name"
(cd "$out" && sha256sum "$tarball" > "$tarball.sha256")
if [[ -n "$glibc" ]]; then runs="glibc $glibc and newer"; else runs="this computer's glibc and newer"; fi
echo "$out/$tarball ($reported, $triple, for $runs)"

# The .deb: the same files under /usr.
if ! command -v dpkg-deb >/dev/null; then
  echo "No dpkg-deb here, so no .deb."
  exit 0
fi
case "$arch" in
  x86_64) deb_arch=amd64 ;;
  aarch64) deb_arch=arm64 ;;
  *) die "no Debian name for $arch" ;;
esac
deb="$stage/deb"
mkdir -p "$deb/DEBIAN" "$deb/usr/share/doc/flow-terminal"
cp -R "$dir/bin" "$dir/lib" "$dir/share" "$deb/usr/"
install -m 644 "$dir/THIRD-PARTY-NOTICES.txt" "$deb/usr/share/doc/flow-terminal/THIRD-PARTY-NOTICES.txt"
cat > "$deb/usr/share/doc/flow-terminal/copyright" <<COPYRIGHT
Flow, a liquid-glass terminal with Claude built in, by arcanii:
https://github.com/arcanii/Flow-releases

The Rust crates built into flow, flow-help, flow-settings and flowapp,
with their copyright notices and licenses, are listed in THIRD-PARTY-NOTICES.txt
beside this file.
COPYRIGHT
# winit, wgpu and arboard load the display's and the GPU's libraries when
# Flow starts: xkbcommon always, Vulkan (or EGL for OpenGL), and Wayland's
# or X11's. The sound output loads ALSA's at the first sound, and Flow is
# silent without it.
cat > "$deb/DEBIAN/control" <<CONTROL
Package: flow-terminal
Version: $deb_version
Architecture: $deb_arch
Maintainer: arcanii <arcanii@users.noreply.github.com>
Installed-Size: $(du -sk --apparent-size "$deb/usr" | cut -f1)
Depends: libc6 (>= $glibc_need), libxkbcommon0, libvulkan1 | libegl1
Recommends: libwayland-client0, libx11-6, libxcursor1, libxi6, libxrandr2, libxkbcommon-x11-0, mesa-vulkan-drivers | vulkan-icd, libasound2t64 | libasound2
Suggests: zenity
Section: x11
Priority: optional
Homepage: https://github.com/arcanii/Flow-releases
Description: liquid-glass terminal with Claude built in
 Flow is a terminal emulator drawn on the GPU, with a liquid behind its glass
 that moves as you work, and an assistant (Claude, GPT or Gemini) that
 suggests or runs commands in your shell. It shows images and 3D models that
 programs send, and installs its games and demos with flowapp.
CONTROL
chmod -R u+rwX,go+rX,go-w "$deb"
debfile="Flow-$version-linux-$deb_arch.deb"
dpkg-deb --root-owner-group -Zxz --build "$deb" "$out/$debfile" >/dev/null
(cd "$out" && sha256sum "$debfile" > "$debfile.sha256")
echo "$out/$debfile (flow-terminal $deb_version, $deb_arch, needs glibc $glibc_need)"
