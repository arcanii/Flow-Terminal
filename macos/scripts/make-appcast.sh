#!/usr/bin/env bash
#
# Sign a macOS release of Flow and write its Sparkle appcast. The appcast and
# the DMGs are published in the public arcanii/Flow-releases repo, apart from
# the source: commit the file this writes as appcast-macos.xml on that
# repo's main branch (the SUFeedURL in macos/packaging/Info.plist).
#
#   macos/scripts/make-appcast.sh --dmg target/dist/Flow-0.1.0-macos.dmg --tag v0.1.0-macos
#
# It signs the DMG with Sparkle's sign_update and Flow's key in the login
# Keychain (account "Flow"; macos/RELEASING.md covers importing it). macOS may
# ask to allow access. To sign elsewhere, pass the signature instead.
#
# Options: --signature <base64> (skip signing here), --repo <owner/name>
# (default arcanii/Flow-releases), --notes <url> (default: the release page),
# --out <file> (default target/dist/appcast-macos.xml), --allow-unnotarized
# (for a test feed only).
#
# Checks first that the DMG holds a signed, notarized Flow.app whose version
# matches --tag, and that the signature verifies against the updater's public
# key, because each mistake would only show up as failed updates on users' Macs.

set -euo pipefail

die() { echo "error: $*" >&2; exit 1; }

dmg=""; tag=""; sig=""; repo_name="arcanii/Flow-releases"; notes=""; out=""; allow_unnotarized=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --dmg) dmg="${2:-}"; shift 2 ;;
    --tag) tag="${2:-}"; shift 2 ;;
    --signature) sig="${2:-}"; shift 2 ;;
    --repo) repo_name="${2:-}"; shift 2 ;;
    --notes) notes="${2:-}"; shift 2 ;;
    --out) out="${2:-}"; shift 2 ;;
    --allow-unnotarized) allow_unnotarized=1; shift ;;
    *) die "unknown option $1 (see the top of $0)" ;;
  esac
done
[[ -n "$dmg" && -n "$tag" ]] || die "usage: $(basename "$0") --dmg <Flow-x.y.z-macos.dmg> --tag v<x.y.z>-macos [--signature <base64>]"
[[ -f "$dmg" ]] || die "no DMG at $dmg"
[[ "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+(\.[0-9]+)?-macos$ ]] || die "--tag should look like v0.1.0-macos (or v0.1.0.1-macos for a patch release), not $tag"

# This script is in macos/scripts; the repo root is two levels up.
repo="$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)"
dmg="$(CDPATH= cd -- "$(dirname -- "$dmg")" && pwd)/$(basename -- "$dmg")"
[[ -n "$out" ]] || out="$repo/target/dist/appcast-macos.xml"

# Read the app's version out of the DMG.
mnt="$(mktemp -d)"
hdiutil attach -quiet -nobrowse -readonly -noautoopen -mountpoint "$mnt" "$dmg"
trap 'hdiutil detach -quiet "$mnt" || true; rmdir "$mnt" 2>/dev/null || true' EXIT
plist="$mnt/Flow.app/Contents/Info.plist"
[[ -f "$plist" ]] || die "$dmg has no Flow.app"
short="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$plist")"
build="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleVersion' "$plist")"
min_os="$(/usr/libexec/PlistBuddy -c 'Print :LSMinimumSystemVersion' "$plist")"
[[ "$tag" == "v$short-macos" ]] || die "the DMG holds Flow $short, but the tag is $tag"
codesign --verify --deep --strict "$mnt/Flow.app" || die "Flow.app in the DMG isn't validly signed"
if ! spctl --assess --type execute "$mnt/Flow.app" 2>/dev/null; then
  [[ -n "$allow_unnotarized" ]] || die "Flow.app isn't notarized. Build with macos/scripts/release.sh --notarize."
  echo "warning: Flow.app isn't notarized; this feed is for testing only" >&2
fi
hdiutil detach -quiet "$mnt"
rmdir "$mnt" 2>/dev/null || true
trap - EXIT

if [[ -z "$sig" ]]; then
  # The sign_update named in SIGN_UPDATE, else Sparkle's own, which
  # release.sh fetched and checked against its pinned checksum into
  # target/sparkle-<version> (its SPARKLE_VERSION); no other copy, since
  # whatever runs here is handed Flow's key.
  tool="${SIGN_UPDATE:-}"
  if [[ -z "$tool" ]]; then
    sparkle_version="${SPARKLE_VERSION:-$(sed -n 's/^SPARKLE_VERSION="\${SPARKLE_VERSION:-\([^}]*\)}"$/\1/p' "$repo/macos/scripts/release.sh")}"
    c="$repo/target/sparkle-$sparkle_version/bin/sign_update"
    if [[ -n "$sparkle_version" && -x "$c" ]]; then tool="$c"; fi
  fi
  [[ -n "$tool" ]] || die "sign_update not found. Run macos/scripts/release.sh first (it fetches Sparkle), or set SIGN_UPDATE."
  echo "Signing $(basename -- "$dmg") with Flow's key (Keychain account Flow)"
  signed="$("$tool" --account Flow "$dmg")"
  # Sparkle prints:  sparkle:edSignature="BASE64==" length="12345"
  sig="$(printf '%s\n' "$signed" | sed -n 's/.*sparkle:edSignature="\([^"]*\)".*/\1/p')"
  [[ -n "$sig" ]] || die "could not read the signature from sign_update: $signed"
fi
sig="$(printf '%s' "$sig" | tr -d '[:space:]')"

cargo run --quiet --locked --example update_key --manifest-path "$repo/Cargo.toml" -- verify "$dmg" "$sig" \
  || die "the signature check failed. Sign this exact file with Flow's key."

len="$(stat -f %z "$dmg")"
file="$(basename -- "$dmg")"
url="https://github.com/$repo_name/releases/download/$tag/$file"
[[ -n "$notes" ]] || notes="https://github.com/$repo_name/releases/tag/$tag"
pub="$(LC_ALL=C date -u '+%a, %d %b %Y %H:%M:%S +0000')"
mkdir -p "$(dirname -- "$out")"
cat > "$out" <<EOF
<?xml version="1.0" encoding="utf-8"?>
<rss version="2.0" xmlns:sparkle="http://www.andymatuschak.org/xml-namespaces/sparkle">
  <channel>
    <title>Flow (macOS) Updates</title>
    <link>https://raw.githubusercontent.com/$repo_name/main/appcast-macos.xml</link>
    <description>Most recent updates to Flow for macOS</description>
    <language>en</language>
    <item>
      <title>Version $short</title>
      <sparkle:releaseNotesLink>$notes</sparkle:releaseNotesLink>
      <pubDate>$pub</pubDate>
      <sparkle:minimumSystemVersion>$min_os</sparkle:minimumSystemVersion>
      <enclosure url="$url"
                 sparkle:version="$build"
                 sparkle:shortVersionString="$short"
                 sparkle:edSignature="$sig"
                 length="$len"
                 type="application/octet-stream"/>
    </item>
  </channel>
</rss>
EOF
echo "Wrote $out"
echo "  version=$short build=$build length=$len"
echo "  url=$url"
echo "Next: upload $file to the $tag release in $repo_name, then commit this file there as appcast-macos.xml on main."
