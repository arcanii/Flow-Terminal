#!/usr/bin/env bash
#
# Sign a Flow Windows installer (or a Linux tarball) on macOS with Flow's
# update key, using Sparkle's `sign_update`. Prints the sparkle:edSignature for
# windows/scripts/make-appcast.ps1 (or linux/scripts/make-appcast.sh). On
# Windows, make-appcast.ps1 can sign by itself.
#
# Usage:
#   windows/scripts/sign-release.sh path/to/Flow-0.1.0-setup.exe
#
# Notes:
#   * The signature covers the exact bytes, so publish the SAME file you sign.
#     Signing the copy downloaded from the GitHub release proves exactly that.
#   * sign_update reads the private key from the login Keychain; macOS may ask
#     to allow access. Flow's key goes under the Keychain account "Flow",
#     imported once from the key file (windows/RELEASING.md):
#         generate_keys --account Flow -f flow-update.key
#     so --account Flow is passed by default. SIGN_UPDATE_ARGS replaces it,
#     e.g. to sign straight from the key file:
#         SIGN_UPDATE_ARGS="--ed-key-file /path/to/flow-update.key" windows/scripts/sign-release.sh <file>
#   * sign_update is the one named in $SIGN_UPDATE, else Sparkle's own, which
#     macos/scripts/release.sh fetches and checks against its pinned checksum
#     into target/sparkle-<version>/bin (that script's SPARKLE_VERSION). No
#     other copy is looked for, since whatever runs here is handed Flow's key.
#   * stdout is only the base64 signature, so it can be captured:
#         SIG=$(windows/scripts/sign-release.sh Flow-0.1.0-setup.exe)

set -euo pipefail

file="${1:-}"
if [[ -z "$file" || ! -f "$file" ]]; then
  echo "usage: $(basename "$0") <path-to-Flow-x.y.z-setup.exe>" >&2
  exit 1
fi

# The repo root (this script is in windows/scripts), through any symlink to it
# (macOS readlink has no -f).
src="$0"
while [[ -L "$src" ]]; do
  dir="$(CDPATH= cd -- "$(dirname -- "$src")" && pwd)"
  src="$(readlink "$src")"
  [[ "$src" == /* ]] || src="$dir/$src"
done
repo="$(CDPATH= cd -- "$(dirname -- "$src")/../.." && pwd)"

tool="${SIGN_UPDATE:-}"
if [[ -z "$tool" ]]; then
  sparkle_version="${SPARKLE_VERSION:-$(sed -n 's/^SPARKLE_VERSION="\${SPARKLE_VERSION:-\([^}]*\)}"$/\1/p' "$repo/macos/scripts/release.sh")}"
  c="$repo/target/sparkle-$sparkle_version/bin/sign_update"
  if [[ -n "$sparkle_version" && -x "$c" ]]; then tool="$c"; fi
fi
if [[ -z "$tool" ]]; then
  echo "error: sign_update not found. Run macos/scripts/release.sh once (it fetches" >&2
  echo "       Sparkle into target/sparkle-<version>), or set SIGN_UPDATE=/path/to/sign_update." >&2
  exit 1
fi

args="${SIGN_UPDATE_ARGS-"--account Flow"}"
case " $args " in
  *" --account "* | *" --ed-key-file "* | *" -f "*) ;;
  *) echo "warning: no --account or key file in SIGN_UPDATE_ARGS, so sign_update uses its DEFAULT" >&2
     echo "         Keychain key, not Flow's. make-appcast.ps1 will reject a" >&2
     echo "         signature from the wrong key." >&2 ;;
esac

echo "Signing $file" >&2
echo "  with $tool${args:+ $args}" >&2
# shellcheck disable=SC2086
out="$("$tool" $args "$file")"
echo "  -> $out" >&2

# Sparkle prints:  sparkle:edSignature="BASE64==" length="12345"
sig="$(printf '%s\n' "$out" | sed -n 's/.*sparkle:edSignature="\([^"]*\)".*/\1/p')"
if [[ -z "$sig" ]]; then
  echo "error: could not parse edSignature from the sign_update output above." >&2
  exit 1
fi

echo >&2
echo "Signature (pass to make-appcast.ps1 -Signature):" >&2
printf '%s\n' "$sig"
