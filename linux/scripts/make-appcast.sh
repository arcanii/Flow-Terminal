#!/usr/bin/env bash
#
# Write the appcast for one Linux tarball of a Flow release, which Flow's own
# updater (linux/src/updater.rs) reads. The appcasts and the tarballs are
# published in the public arcanii/Flow-releases repo, apart from the
# source: commit the file this writes as appcast-linux-<arch>.xml on that
# repo's main branch, one per architecture.
#
#   linux/scripts/make-appcast.sh --tarball target/linux/Flow-0.1.23-linux-x86_64.tar.gz \
#       --tag v0.1.23-linux --signature <base64> \
#       [--deb target/linux/Flow-0.1.23-linux-amd64.deb --deb-signature <base64>]
#
# The signature is Flow's update key's over the tarball's exact bytes, as
# Sparkle's: made on the Mac with windows/scripts/sign-release.sh <tarball>,
# or here with --key-file <file> (the key's base64 seed, as
# examples/update_key.rs reads it). The private key never goes in the repo.
#
# With --deb, the release's .deb goes in the feed too, as a second enclosure
# after the tarball's (a Flow before 0.1.37.1 reads the first alone): a Flow
# from the .deb downloads it, checks it with Flow's key and runs apt on it in
# a tab. Its signature is made as the tarball's is (sign-release.sh <deb>),
# or with --key-file, which then signs both.
#
# Options: --repo <owner/name> (default arcanii/Flow-releases), --notes <url>
# (default: the release page), --out <file> (default
# target/linux/appcast-linux-<arch>.xml).
#
# Checks first that the tarball's flow reports the version the tag names
# (so run it on the architecture the tarball is for) and updates itself
# (package.sh --updates), and that the signature
# verifies against the updater's public key, because each mistake would only
# show up as failed updates on users' computers.

set -euo pipefail

die() { echo "error: $*" >&2; exit 1; }

tarball=""; tag=""; sig=""; key_file=""; repo_name="arcanii/Flow-releases"; notes=""; out=""; deb=""; deb_sig=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --tarball) tarball="${2:-}"; shift 2 ;;
    --deb) deb="${2:-}"; shift 2 ;;
    --deb-signature) deb_sig="${2:-}"; shift 2 ;;
    --tag) tag="${2:-}"; shift 2 ;;
    --signature) sig="${2:-}"; shift 2 ;;
    --key-file) key_file="${2:-}"; shift 2 ;;
    --repo) repo_name="${2:-}"; shift 2 ;;
    --notes) notes="${2:-}"; shift 2 ;;
    --out) out="${2:-}"; shift 2 ;;
    *) die "unknown option $1 (see the top of $0)" ;;
  esac
done
[[ -n "$tarball" && -n "$tag" ]] || die "usage: $(basename "$0") --tarball <Flow-x.y.z-linux-<arch>.tar.gz> --tag v<x.y.z>-linux (--signature <base64> | --key-file <file>)"
[[ -f "$tarball" ]] || die "no tarball at $tarball"
[[ "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+(\.[0-9]+)?-linux$ ]] || die "--tag should look like v0.1.0-linux (v0.1.0.1-linux for a patch release), not $tag"
[[ -n "$sig" || -n "$key_file" ]] || die "give --signature (from windows/scripts/sign-release.sh on the Mac) or --key-file"

# This script is in linux/scripts; the repo root is two levels up.
repo="$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)"
tarball="$(CDPATH= cd -- "$(dirname -- "$tarball")" && pwd)/$(basename -- "$tarball")"
file="$(basename -- "$tarball")"
# Flow-0.1.23-linux-x86_64.tar.gz -> 0.1.23 and x86_64.
short="$(printf '%s\n' "$file" | sed -n 's/^Flow-\([0-9.]*\)-linux-\([a-z0-9_]*\)\.tar\.gz$/\1/p')"
arch="$(printf '%s\n' "$file" | sed -n 's/^Flow-\([0-9.]*\)-linux-\([a-z0-9_]*\)\.tar\.gz$/\2/p')"
[[ -n "$short" && -n "$arch" ]] || die "$file isn't named as package.sh names a tarball"
[[ "$tag" == "v$short-linux" ]] || die "the tarball is Flow $short, but the tag is $tag"
[[ -n "$out" ]] || out="$repo/target/linux/appcast-linux-$arch.xml"

# The build number, from the tarball's own flow.
work="$(mktemp -d "${TMPDIR:-/tmp}/flow-appcast.XXXXXX")"
trap 'rm -rf "$work"' EXIT
tar -xzf "$tarball" -C "$work"
flow="$work/flow-$short/bin/flow"
[[ -x "$flow" ]] || die "$file holds no flow-$short/bin/flow"
reported="$("$flow" --version 2>/dev/null)" || die "its flow doesn't run here; make the $arch appcast on a $arch computer"
build="$(printf '%s\n' "$reported" | sed -n 's/^Flow \([0-9.]*\) (build \([0-9]*\))$/\2/p')"
version="$(printf '%s\n' "$reported" | sed -n 's/^Flow \([0-9.]*\) (build \([0-9]*\))$/\1/p')"
[[ -n "$build" && "$version" == "$short" ]] || die "its flow says \"$reported\", not Flow $short"
# Every Flow that reads the feed updates to it, so it must update itself
# too, or they'd all stop there (package.sh --updates).
said="$(FLOW_SAY_UPDATES=1 "$flow" --version 2>/dev/null | sed -n 2p)"
[[ "$said" == "updates: on" ]] || die "its flow says \"${said:-nothing}\" of updates, not \"updates: on\": make it with package.sh --updates"
[[ -x "$work/flow-$short/install.sh" ]] || die "$file holds no install.sh, which the updater runs"

# The .deb, where given: named as package.sh names it, and Flow's package at
# this build (its version the marketing one, then the build), as the
# updater checks before apt gets it.
if [[ -n "$deb" ]]; then
  [[ -f "$deb" ]] || die "no .deb at $deb"
  [[ -n "$deb_sig" || -n "$key_file" ]] || die "give --deb-signature (sign-release.sh <deb> on the Mac) or --key-file"
  deb="$(CDPATH= cd -- "$(dirname -- "$deb")" && pwd)/$(basename -- "$deb")"
  deb_file="$(basename -- "$deb")"
  case "$arch" in x86_64) deb_arch=amd64 ;; aarch64) deb_arch=arm64 ;; *) deb_arch="$arch" ;; esac
  [[ "$deb_file" == "Flow-$short-linux-$deb_arch.deb" ]] || die "$deb_file isn't Flow-$short-linux-$deb_arch.deb, as package.sh names this release's .deb"
  deb_package="$(dpkg-deb -f "$deb" Package)" || die "dpkg-deb can't read $deb_file"
  deb_version="$(dpkg-deb -f "$deb" Version)"
  [[ "$deb_package" == "flow-terminal" && "$deb_version" == *".$build" ]] || die "$deb_file is $deb_package $deb_version, not flow-terminal at build $build"
fi

key_tool() {
  cargo run --quiet --locked --example update_key --manifest-path "$repo/Cargo.toml" -- "$@"
}
if [[ -z "$sig" ]]; then
  [[ -f "$key_file" ]] || die "no key file at $key_file"
  echo "Signing $file with the key in $key_file"
  sig="$(key_tool sign "$tarball" "$key_file")"
fi
sig="$(printf '%s' "$sig" | tr -d '[:space:]')"
key_tool verify "$tarball" "$sig" || die "the signature check failed. Sign this exact file with Flow's key."
deb_enclosure=""
if [[ -n "$deb" ]]; then
  if [[ -z "$deb_sig" ]]; then
    echo "Signing $deb_file with the key in $key_file"
    deb_sig="$(key_tool sign "$deb" "$key_file")"
  fi
  deb_sig="$(printf '%s' "$deb_sig" | tr -d '[:space:]')"
  key_tool verify "$deb" "$deb_sig" || die "the .deb's signature check failed. Sign this exact file with Flow's key."
  deb_len="$(stat -c %s "$deb")"
  deb_url="https://github.com/$repo_name/releases/download/$tag/$deb_file"
  deb_enclosure="
      <enclosure url=\"$deb_url\"
                 sparkle:version=\"$build\"
                 sparkle:shortVersionString=\"$short\"
                 sparkle:edSignature=\"$deb_sig\"
                 length=\"$deb_len\"
                 type=\"application/vnd.debian.binary-package\"/>"
fi

len="$(stat -c %s "$tarball")"
url="https://github.com/$repo_name/releases/download/$tag/$file"
[[ -n "$notes" ]] || notes="https://github.com/$repo_name/releases/tag/$tag"
pub="$(LC_ALL=C date -u '+%a, %d %b %Y %H:%M:%S +0000')"
mkdir -p "$(dirname -- "$out")"
cat > "$out" <<EOF
<?xml version="1.0" encoding="utf-8"?>
<rss version="2.0" xmlns:sparkle="http://www.andymatuschak.org/xml-namespaces/sparkle">
  <channel>
    <title>Flow (Linux, $arch) Updates</title>
    <link>https://raw.githubusercontent.com/$repo_name/main/appcast-linux-$arch.xml</link>
    <description>Most recent updates to Flow for Linux on $arch</description>
    <language>en</language>
    <item>
      <title>Version $short</title>
      <sparkle:releaseNotesLink>$notes</sparkle:releaseNotesLink>
      <pubDate>$pub</pubDate>
      <enclosure url="$url"
                 sparkle:version="$build"
                 sparkle:shortVersionString="$short"
                 sparkle:edSignature="$sig"
                 length="$len"
                 type="application/gzip"/>$deb_enclosure
    </item>
  </channel>
</rss>
EOF
echo "Wrote $out"
echo "  version=$short build=$build arch=$arch length=$len"
echo "  url=$url"
[[ -z "$deb" ]] || echo "  deb=$deb_url length=$deb_len"
echo "Next: upload $file${deb:+ and $deb_file} to the $tag release in $repo_name, then commit this file there as appcast-linux-$arch.xml on main."
