# BusyBox (busybox-w32)

Flow's built-in bash on Windows machines that have no Git Bash, MSYS2 or WSL.
`windows/src/shells.rs` falls back to it and labels the tab "bash"; it is BusyBox ash,
a POSIX shell with some bash features, plus 100+ Unix tools in one exe.

- `busybox64u.exe`: the 64-bit Unicode build of busybox-w32 **FRP-6075-g169694ebd**
  (BusyBox v1.38.0, 2026-05-06), downloaded as `busybox-w64u-FRP-6075-g169694ebd.exe`
  from <https://frippery.org/files/busybox/>. It needs Windows 10 1903 or later.
  - SHA-256 `6e263d154d8548d1eb936f65d1d8312c80df31c45974e48d6335e4dcc0f4f34c`
    (matches upstream's `SHA256SUM`).
  - The GPG signature checks out with Ron Yorston's key `690E10A0513DA84B`,
    fetched from <https://github.com/rmyorston.gpg>.
- `busybox-w32-FRP-6075-g169694ebd.tgz`: the matching source, from the same
  folder. SHA-256 `44401413c86a839deeec3eba088af244a1594f18ff9fd0622811100e4cc2e7b4`.
- `LICENSE`: BusyBox's license, GPL version 2, copied from that source.

## The GPL

BusyBox is GPLv2. Flow runs it as a separate program, so Flow's own code
isn't affected, but every release that ships `busybox.exe` must do two things:
- include the license text: `windows/scripts/third-party-notices.ps1` adds it to
  `THIRD-PARTY-NOTICES.txt`;
- offer the corresponding source from the same place as the download.
  `windows/scripts/release.ps1` copies this tarball into `target\dist`, and it goes up
  with the installer as a release asset (GPLv2 §3).

Keep the binary and the tarball at the same FRP version.

## Updating

1. Download the new `busybox-w64u-FRP-*.exe`, its `.sig`, `SHA256SUM`, and
   `busybox-w32-FRP-*.tgz` from <https://frippery.org/files/busybox/>.
2. Check the checksum and the signature.
3. Replace both files here, and update the version and hashes above. The
   release scripts read the version from the tarball's name.
