#!/usr/bin/env bash
#
# Write the AUR's flow-terminal-bin for a published Linux release
# (docs/design/aur.md): its PKGBUILD, .SRCINFO and LICENSE, from the
# templates in linux/packaging/aur/, the release's checksum of its tarball
# and Flow's license.
#
#   linux/scripts/aur.sh <version> [--pkgrel <n>] [--out <dir>]
#
# <version> is the release's as its tag has it (0.1.46, or 0.1.46.1 for a
# patch). The checksum is the one the release publishes beside its tarball
# (<file>.sha256), checked against target/linux's tarball of that version
# where there is one, so the package names the very file people download.
# --pkgrel (1 unless given) goes up for a change to the package alone, at the
# same version. <dir> is a checkout of the AUR's repository (git clone
# ssh://aur@aur.archlinux.org/flow-terminal-bin.git), by default
# target/aur/flow-terminal-bin. Where makepkg is (on Arch), the .SRCINFO
# written is compared with what makepkg makes of the PKGBUILD. Nothing is
# pushed: that's publishing, done by hand in <dir>.

set -euo pipefail

usage() {
  echo "usage: $(basename "$0") <version> [--pkgrel <n>] [--out <dir>]" >&2
  exit 1
}
die() { echo "error: $*" >&2; exit 1; }

# This script is in linux/scripts; the repo root is two levels up.
repo="$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)"
version=""; pkgrel=1; out="$repo/target/aur/flow-terminal-bin"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --pkgrel) pkgrel="${2:-}"; [[ "$pkgrel" =~ ^[1-9][0-9]*$ ]] || usage; shift 2 ;;
    --out) out="${2:-}"; [[ -n "$out" ]] || usage; shift 2 ;;
    -*) usage ;;
    *) [[ -z "$version" ]] || usage; version="$1"; shift ;;
  esac
done
# Numbers and dots alone, as the updater takes them: the version goes into
# file names and a URL.
[[ "$version" =~ ^[0-9]+(\.[0-9]+){2,3}$ ]] || usage

file="Flow-$version-linux-x86_64.tar.gz"
url="https://github.com/arcanii/Flow-releases/releases/download/v$version-linux/$file"
published="$(curl -fsSL "$url.sha256")" || die "couldn't read $url.sha256: is v$version-linux published?"
sha="$(printf '%s\n' "$published" | awk '{print $1}')"
[[ "$sha" =~ ^[0-9a-f]{64}$ ]] || die "$url.sha256 holds no checksum"
if [[ -f "$repo/target/linux/$file" ]]; then
  built="$(sha256sum "$repo/target/linux/$file" | cut -d' ' -f1)"
  [[ "$built" == "$sha" ]] || die "target/linux/$file isn't the published one ($built, not $sha)"
fi
license_sha="$(sha256sum "$repo/LICENSE" | cut -d' ' -f1)"

mkdir -p "$out"
fill() {
  sed -e "s/@PKGVER@/$version/g" -e "s/@PKGREL@/$pkgrel/g" \
      -e "s/@SHA256@/$sha/g" -e "s/@LICENSE_SHA256@/$license_sha/g" "$1"
}
fill "$repo/linux/packaging/aur/PKGBUILD" > "$out/PKGBUILD"
fill "$repo/linux/packaging/aur/SRCINFO" > "$out/.SRCINFO"
install -m 644 "$repo/LICENSE" "$out/LICENSE"
! grep -q '@[A-Z_0-9]*@' "$out/PKGBUILD" "$out/.SRCINFO" || die "a template's field went unfilled"

# makepkg's own reading of the PKGBUILD, where there is one, so the two
# templates can't drift apart unseen.
if command -v makepkg >/dev/null; then
  (cd "$out" && makepkg --printsrcinfo) | cmp -s - "$out/.SRCINFO" \
    || die ".SRCINFO isn't what makepkg makes of the PKGBUILD: change linux/packaging/aur/SRCINFO to match"
  echo "makepkg --printsrcinfo agrees with .SRCINFO"
fi

echo "Wrote $out: PKGBUILD, .SRCINFO and LICENSE for flow-terminal-bin $version-$pkgrel"
echo "  $file, SHA-256 $sha (the release's)"
echo "Next, when asked to publish, in a checkout of ssh://aur@aur.archlinux.org/flow-terminal-bin.git:"
echo "  git add PKGBUILD .SRCINFO LICENSE && git commit -m 'Update to $version-$pkgrel' && git push"
