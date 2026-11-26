# Flow in the AUR (2026-11-25)

Arch users, Omarchy's among them (the README's pitch leads with it), install
what Arch doesn't ship from the AUR, through a helper such as `yay`, which
Omarchy comes with. The tarball already installs on Arch without root, but a
package there is how they look for a program, and their helper keeps it up
to date with the rest of the system. This is the plan for one, to be agreed
before it's built.

## What it is

**`flow-terminal-bin`**, a PKGBUILD that repackages each Linux release's
tarball: `-bin`, as the AUR names a package of upstream's own build. A
source package (`flow-terminal`, built with cargo from a tag of
Flow-Terminal) can follow, Flow's source being public from 2026-11-25; this
one comes first, since it builds nothing on the user's computer and holds
the very files that were released.

It installs what the .deb does, in the same places: `bin` to `/usr/bin`,
`lib/flow-terminal/games/` to `/usr/lib/flow-terminal/games/` (where
`Platform::games_dir` looks beside `bin`), the menu entry and icons under
`/usr/share`, and the notices and Flow's MIT license under
`/usr/share/licenses/flow-terminal-bin/`. The tarball doesn't hold Flow's own
`LICENSE` yet, so the AUR repository carries it beside the PKGBUILD.

```bash
# Maintainer: arcanii <arcanii at users dot noreply dot github dot com>
pkgname=flow-terminal-bin
pkgver=0.1.46
pkgrel=1
pkgdesc='A liquid-glass terminal with Claude built in'
arch=('x86_64')
url='https://github.com/arcanii/Flow-Terminal'
license=('MIT')
depends=('glibc' 'libxkbcommon' 'hicolor-icon-theme')
optdepends=(
  'vulkan-icd-loader: drawing with Vulkan, for glass you can see through'
  'wayland: a Wayland window'
  'libx11: an X11 window, with libxcursor, libxi, libxrandr and libxkbcommon-x11'
  "alsa-lib: Flow's sounds"
  "gnome-keyring: keeping the assistant's API key (or any Secret Service)"
  'xdg-desktop-portal: the panels for picking a file'
  "zenity: the error dialog when Flow can't start"
)
provides=('flow-terminal')
conflicts=('flow-terminal' 'flow' 'flow-bin')
options=('!strip' '!debug')
source=('LICENSE')
source_x86_64=("https://github.com/arcanii/Flow-releases/releases/download/v${pkgver}-linux/Flow-${pkgver}-linux-x86_64.tar.gz")
sha256sums=('f740518a6a7cc61f522c466a4829c7ccce8129a42d732418d7b19bd734565978')
sha256sums_x86_64=('b7bfea7f2f9b23e7c8291ecc776811f39b2758bac3e129112d61b5a4c379b051')

package() {
  cd "flow-${pkgver}"
  install -d "$pkgdir/usr"
  cp -R bin lib share "$pkgdir/usr/"
  install -Dm644 THIRD-PARTY-NOTICES.txt "$pkgdir/usr/share/licenses/$pkgname/THIRD-PARTY-NOTICES.txt"
  install -Dm644 "$srcdir/LICENSE" "$pkgdir/usr/share/licenses/$pkgname/LICENSE"
}
```

**Why these choices:**
- **Dependencies:** Flow's programs link glibc alone; winit, wgpu and the
  sound output load the rest as Flow starts, as the .deb's Depends and
  Recommends say. xkbcommon is needed always, so it's a dependency; the
  others are optional, since a desktop has the ones its session uses.
  pacman has no "this or that", so Vulkan is optional too: without it Flow
  draws with OpenGL and its glass is opaque.
- **`!strip` and `!debug`:** the binaries are installed exactly as
  released, so their SHA-256 is the release's.
- **The checksum** is the release's `.sha256`, written in when the package
  is bumped. Flow's Ed25519 signature isn't checked by makepkg, which knows
  PGP only; the checksum, pinned by whoever bumps it after the release is
  checked, is the AUR's usual way.
- **The `flow` conflict:** the AUR's `flow` and `flow-bin` are Facebook's
  Flow, a JavaScript type checker, which also installs `/usr/bin/flow`. So
  the two can't be installed together, and the package says so, as pacman
  would refuse anyway. Renaming Flow's program there (`flow-terminal`) would
  part it from the .deb, the tarball and the docs, and the flow command in
  a tab would be the type checker's. Kept, with the conflict, on the
  user's word (2026-11-25: "Keep it, conflict").

## Updates

The package goes up with each Linux release: `pkgver`, the checksum and
`.SRCINFO`, pushed to `ssh://aur@aur.archlinux.org/flow-terminal-bin.git`,
which is publishing, so only on the user's word.
`linux/scripts/aur.sh <version>` (`--pkgrel <n>` for a change to the
package alone) writes the PKGBUILD, `.SRCINFO` and `LICENSE` into a
checkout of that repository (`--out`, by default
`target/aur/flow-terminal-bin`):
- the templates are `linux/packaging/aur/PKGBUILD` and `SRCINFO`, the
  second taken from makepkg's own `--printsrcinfo`, so no Arch is needed
  to write it;
- the checksum is the one the release publishes beside its tarball,
  checked against `target/linux`'s tarball where it's there;
- where makepkg is, it compares `.SRCINFO` with what makepkg makes of the
  PKGBUILD, so the two templates can't drift apart unseen.

`linux/RELEASING.md`'s step 8 runs it after the feed.

Flow's own updater stays in the binary (the release is built with
`--updates`), but a copy in `/usr` that dpkg didn't put there can't be
updated by Flow: before 0.1.48, About says a release is out, and Install
Update says Flow can't write in `/usr` and to install it as before. From
0.1.48 (the user's word, 2026-11-25: "Yes, in 0.1.47") the updater knows
pacman's copy (`from_pacman`: in `/usr`, with `flow-terminal-bin` or
`flow-terminal` in pacman's database, `/var/lib/pacman/local`, as
`from_deb` reads dpkg's list) and says only that the release is out, for
the AUR helper to install once the AUR has it
(`UpdateState::ByPackage`, `update.aur`): the title bar's pill shows, and
About's button checks again rather than installing. It was held while
0.1.46 and then 0.1.47 went out (the user's "Packaging now, hold"), so
neither carried it, and went in toward 0.1.48 (2026-11-26: "Yes, bring it
back").

## Trying it

Tried on Linux (2026-11-25, the user's word: "Scratch Arch here") in a
scratch Arch, there being no Arch, docker, podman or distrobox on the
computer:
- **The Arch:** Arch's bootstrap image (128 MB, its SHA-256 and Arch's
  release key's signature checked), unpacked in user, mount and pid
  namespaces with the user's subordinate ids, nothing changing outside
  it. pacman 7.1.0 installed `base-devel`, `namcap` and
  `desktop-file-utils` from Arch's mirror.
- **The build:** makepkg, as an ordinary user there, fetched the
  0.1.46 tarball from the release, passed both checksums and built
  `flow-terminal-bin-0.1.46-1-x86_64.pkg.tar.zst`. `makepkg
  --printsrcinfo` wrote `.SRCINFO`.
- **namcap** found no errors:
  - on the PKGBUILD, one warning, the `x86_64` in the tarball's name,
    which an architecture's own source carries;
  - on the package, warnings that `libpthread` and `libdl` are linked
    but unused (built for glibc 2.31, where they're apart) and that
    `libxkbcommon` may not be needed, since Flow loads it as it starts.
- **Installed** with `pacman -U`:
  - the files are where the .deb puts them, the license and notices in
    `/usr/share/licenses/flow-terminal-bin/`;
  - `/usr/bin/flow` is byte for byte the release's, and says 0.1.46
    (build 766) with updates on;
  - flowapp, flow-mcp and flow-help run;
  - `desktop-file-validate` passes the menu entry, but notes that its
    Keywords repeat its GenericName, "terminal".

  `pacman -R` took it all away again.
- **`aur.sh 0.1.46`** wrote the same PKGBUILD and `.SRCINFO` as were
  built there (but for `url`, Flow-Terminal's since), and run in that
  Arch, makepkg agreed with its `.SRCINFO`, built from it, and pacman
  installed what it built. It refuses a version that isn't published, or
  isn't numbers and dots. Once 0.1.47 was out (2026-11-26), `aur.sh
  0.1.47` wrote its package there too, which makepkg built, namcap
  passed, and pacman put in place over 0.1.46-1, `/usr/bin/flow` the
  release's byte for byte.
- **The updater's word** (0.1.48): `pacman_has_flow` is tested against a
  made-up pacman database (Facebook's Flow, a `-git` package and the
  system's aren't Flow's). Not seen in a window.
- **Not tried:** the window, which needs a session (Hyprland's would be
  the first look at Flow on it).

## Publishing

The AUR needs an account and an SSH key, which are the user's to make.
Each push to the AUR is publishing, on the user's word.

`url` is Flow-Terminal, where Flow's source is public from 2026-11-25.
