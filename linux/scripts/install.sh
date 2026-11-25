#!/bin/sh
#
# Install Flow from the folder this script is in, as the release tarball
# unpacks it, or take it away again.
#
#   ./install.sh [--prefix <dir>]
#   ./install.sh --uninstall [--prefix <dir>]
#
# The prefix is ~/.local unless given, which needs no root: Flow goes in
# <prefix>/bin, flow-help, flow-settings and flowapp in
# <prefix>/lib/flow-terminal/games (Flow puts that folder on its shells'
# PATH), and its menu entry and icons in
# <prefix>/share. For every user, run it as root with --prefix /usr/local.
# Flow's settings, history and keys stay where they are either way.

set -eu
# Folders readable by everyone, as an install for every user needs, whatever
# the umask of the root shell running it; files get their modes from install.
umask 022

usage() {
  echo "usage: $(basename "$0") [--uninstall] [--prefix <dir>]" >&2
  exit 1
}

prefix=""
uninstall=0
while [ $# -gt 0 ]; do
  case "$1" in
    --prefix) [ $# -ge 2 ] || usage; prefix="$2"; shift 2 ;;
    --uninstall) uninstall=1; shift ;;
    *) usage ;;
  esac
done
[ -n "$prefix" ] || prefix="${HOME:?}/.local"
case "$prefix" in
  /*) ;;
  *) echo "error: the prefix must be an absolute path: $prefix" >&2; exit 1 ;;
esac
# The menu entry names Flow by its path, where these would need escaping
# (and sed below would read & and | as its own).
nl='
'
case "$prefix" in
  *[\"\`\$\\%\&\|]* | *"$nl"*) echo "error: pick a prefix without \" \` \$ \\ % & | or a new line: $prefix" >&2; exit 1 ;;
esac

id=com.arcanii.Flow
here="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
games="$prefix/lib/flow-terminal/games"
apps="$prefix/share/applications"
icons="$prefix/share/icons/hicolor"

# Asks the desktop to read the menu entries and icons again, where it can.
refresh() {
  if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database -q "$apps" 2>/dev/null || true
  fi
  if [ -d "$icons" ]; then
    touch "$icons" 2>/dev/null || true
  fi
  if command -v gtk-update-icon-cache >/dev/null 2>&1 && [ -f "$icons/index.theme" ]; then
    gtk-update-icon-cache -q -t "$icons" 2>/dev/null || true
  fi
}

if [ "$uninstall" = 1 ]; then
  rm -f "$prefix/bin/flow" "$games/flow-rogue" "$games/flow-mines" "$games/flow-demo" "$games/flow-help" "$games/flow-settings" "$games/flowapp" "$games/flow-mcp" "$apps/$id.desktop"
  for icon in "$icons"/*/apps/"$id".png; do
    rm -f "$icon"
  done
  rmdir "$games" "$prefix/lib/flow-terminal" 2>/dev/null || true
  refresh
  echo "Flow is uninstalled from $prefix."
  exit 0
fi

[ -x "$here/bin/flow" ] || { echo "error: no bin/flow beside this script" >&2; exit 1; }
mkdir -p "$prefix/bin" "$games" "$apps"
# Copied to a new name and moved into place, so a running Flow keeps its file.
install -m 755 "$here/bin/flow" "$prefix/bin/.flow.new"
mv -f "$prefix/bin/.flow.new" "$prefix/bin/flow"
for game in "$here"/lib/flow-terminal/games/*; do
  [ -f "$game" ] || continue
  name="$(basename "$game")"
  install -m 755 "$game" "$games/.$name.new"
  mv -f "$games/.$name.new" "$games/$name"
done
# Flow carried the games and the demos itself until 0.1.33; flowapp installs
# them now (in Flow's own folder, apps/bin), so an older Flow's copies go,
# unless this tarball still has them.
for gone in flow-rogue flow-mines flow-demo; do
  [ -f "$here/lib/flow-terminal/games/$gone" ] || rm -f "$games/$gone"
done
for icon in "$here"/share/icons/hicolor/*/apps/"$id".png; do
  [ -f "$icon" ] || continue
  size="$(basename "$(dirname "$(dirname "$icon")")")"
  mkdir -p "$icons/$size/apps"
  install -m 644 "$icon" "$icons/$size/apps/$id.png"
done
# The menu runs Flow by its full path: a launcher's PATH may lack <prefix>/bin.
sed "s|^Exec=.*|Exec=\"$prefix/bin/flow\"|" "$here/share/applications/$id.desktop" > "$apps/.$id.desktop.new"
mv -f "$apps/.$id.desktop.new" "$apps/$id.desktop"
refresh

echo "Flow is installed in $prefix: it's in your applications as Flow."
case ":$PATH:" in
  *":$prefix/bin:"*) ;;
  *) echo "To start it as 'flow' from a shell, add $prefix/bin to your PATH." ;;
esac
