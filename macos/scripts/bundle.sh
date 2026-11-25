#!/usr/bin/env bash
#
# Wrap a built flow binary in Flow.app.
#
#   macos/scripts/bundle.sh <flow binary> <output dir> [--sparkle <Sparkle.framework>] [--notices <file>]
#                           [--game <binary>]...
#
# Info.plist (macos/packaging/Info.plist) gets the version and build number the
# binary reports (flow --version) and the update key from core/src/lib.rs
# (UPDATE_PUBLIC_KEY). The bundle is left unsigned; macos/scripts/release.sh
# signs it. For a quick local app, from the repo root:
#
#   cargo build --release -p flow-terminal
#   macos/scripts/bundle.sh target/release/flow target/release --game target/release/flow-help \
#       --game target/release/flow-settings --game target/release/flowapp \
#       --game target/release/flow-mcp --game target/release/flow-command
#   open target/release/Flow.app
#
# Without --sparkle the app has no updater, like `cargo run`. Each --game goes
# into Contents/Helpers, which Flow puts at the end of its shells' PATH:
# flow-help, which prints Flow's help pages in a tab, flow-settings, which
# sees and changes Flow's settings from one, flowapp, which installs the
# games and the SDK's demos (Flow carried those itself until 0.1.33; a test
# build may still take them with --game), flow-mcp, and flow-command, which
# goes in as `flow`, the command that opens a tab in the running Flow
# (docs/design/flow-command.md).

set -euo pipefail

usage() {
  echo "usage: $(basename "$0") <flow binary> <output dir> [--sparkle <Sparkle.framework>] [--notices <file>] [--game <binary>]..." >&2
  exit 1
}

bin=""; out=""; sparkle=""; notices=""; games=()
while [[ $# -gt 0 ]]; do
  case "$1" in
    --sparkle) sparkle="${2:-}"; shift 2 ;;
    --game) games+=("${2:-}"); shift 2 ;;
    --notices) notices="${2:-}"; shift 2 ;;
    -*) usage ;;
    *) if [[ -z "$bin" ]]; then bin="$1"; elif [[ -z "$out" ]]; then out="$1"; else usage; fi; shift ;;
  esac
done
[[ -n "$bin" && -n "$out" ]] || usage
[[ -x "$bin" ]] || { echo "error: $bin is not an executable" >&2; exit 1; }
[[ -z "$sparkle" || -d "$sparkle" ]] || { echo "error: no Sparkle.framework at $sparkle" >&2; exit 1; }
[[ -z "$notices" || -f "$notices" ]] || { echo "error: no notices file at $notices" >&2; exit 1; }
for game in ${games[@]+"${games[@]}"}; do
  [[ -x "$game" ]] || { echo "error: game $game is not an executable" >&2; exit 1; }
done

# This script is in macos/scripts; the repo root is two levels up.
repo="$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)"

# "Flow 0.1.0 (build 42)" -> 0.1.0 and 42, so the plist always matches the binary.
reported="$("$bin" --version)"
version="$(printf '%s\n' "$reported" | sed -n 's/^[^ ]* \([^ ]*\) (build \([0-9]*\))$/\1/p')"
build="$(printf '%s\n' "$reported" | sed -n 's/^[^ ]* \([^ ]*\) (build \([0-9]*\))$/\2/p')"
[[ -n "$version" && -n "$build" ]] || { echo "error: unexpected version output: $reported" >&2; exit 1; }

# pub const UPDATE_PUBLIC_KEY: &str = "<base64>";
key="$(sed -n 's/^pub const UPDATE_PUBLIC_KEY: &str = "\([A-Za-z0-9+\/=]*\)";$/\1/p' "$repo/core/src/lib.rs")"
[[ -n "$key" ]] || { echo "error: no UPDATE_PUBLIC_KEY in core/src/lib.rs" >&2; exit 1; }

app="$out/Flow.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
sed -e "s/@VERSION@/$version/" -e "s/@BUILD@/$build/" -e "s|@UPDATE_PUBLIC_KEY@|$key|" \
  "$repo/macos/packaging/Info.plist" > "$app/Contents/Info.plist"
plutil -lint -s "$app/Contents/Info.plist"
printf 'APPL????' > "$app/Contents/PkgInfo"
cp "$bin" "$app/Contents/MacOS/flow"
cp "$repo/assets/icon/flow.icns" "$app/Contents/Resources/flow.icns"
# Info.plist's words in other languages (Resources/<lang>.lproj/InfoPlist.strings).
for lproj in "$repo"/macos/packaging/*.lproj; do
  plutil -lint -s "$lproj/InfoPlist.strings"
  cp -R "$lproj" "$app/Contents/Resources/"
done
[[ -z "$notices" ]] || cp "$notices" "$app/Contents/Resources/THIRD-PARTY-NOTICES.txt"
for game in ${games[@]+"${games[@]}"}; do
  mkdir -p "$app/Contents/Helpers"
  name="$(basename "$game")"
  # The flow command is `flow` on the tabs' PATH; Flow itself is in MacOS/.
  [[ "$name" == flow-command ]] && name=flow
  cp "$game" "$app/Contents/Helpers/$name"
done
if [[ -n "$sparkle" ]]; then
  mkdir -p "$app/Contents/Frameworks"
  # ditto keeps the framework's symlinks and its nested helpers intact.
  ditto "$sparkle" "$app/Contents/Frameworks/Sparkle.framework"
fi

echo "Bundled $app ($version, build $build$([[ -n "$sparkle" ]] && echo ', with Sparkle')$([[ ${#games[@]} -gt 0 ]] && echo ", with ${#games[@]} game(s)"))"
