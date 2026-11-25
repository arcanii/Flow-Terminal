# WinSparkle

`x64/WinSparkle.dll` and `winsparkle.h` are WinSparkle 0.9.3. Source and
releases: <https://github.com/vslavik/winsparkle>. MIT licensed: `COPYING` is its
license as WinSparkle 0.9.3 has it, and the same notice ships in each
release's `THIRD-PARTY-NOTICES.txt`.

Flow loads the DLL at run time from next to `flow.exe` (see `windows/src/updater.rs`),
so nothing links against it. `windows/scripts/release.ps1` copies it into the
installer and the portable zip.
