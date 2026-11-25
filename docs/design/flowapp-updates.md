# flowapp updating itself

Asked for on 2026-11-20: flowapp should be able to update without a Flow
release, perhaps through a helper program.

flowapp ships inside Flow: the Mac's `Contents/Helpers`, Windows'
`games\` and Linux's `lib/flow-terminal/games`. So a fix to it waited
for a whole Flow release.

No helper is needed. Flow's tabs already put the apps folder's `bin/`
ahead of Flow's own programs on the PATH (`shell::games_dirs`), "so an
app it upgraded comes ahead of the copy Flow carries". So flowapp is an
app in its own catalog, and its installed copy runs in place of Flow's.

## What was decided

- **Approach:** flowapp as an app, not a launcher Flow carries.
- **Version:** its own number, from 0.2.0.
- **Who gets it:** `upgrade` offers it, even in the flowapp already out.

## flowapp's own version

- **Where it's set:** `flowapp = "0.2.0"` under
  `[workspace.metadata.flow]`. `core/build.rs` stamps it as
  `FLOWAPP_VERSION`, read as `version::FLOWAPP`. It starts above every
  Flow so far (0.1.45).
- **What prints it:**
  - `flowapp --version` says `flowapp 0.2.0 (build 750)`. The build is
    still Flow's build number (this repository's commit count plus
    `build_offset`).
  - flowapp's user agent sends it.
- **The rule:**
  - It goes up with each download of flowapp published: 0.2.1 for a
    fix, 0.3.0 for more.
  - Flow's releases carry whatever flowapp master holds.
  - It's never DEV, so it can go out while Flow's own version is being
    worked toward.

## Reading a program's version

- **The problem:** `apps::version_of` took the last word of a program's
  `--version`.
  - That works for `flow-rogue 0.1.45`.
  - For `flowapp 0.1.45 (build 737)` it gave `737)`.
- **The fix:** `apps::version_in` takes the first word that starts with
  a digit, without a closing bracket or comma after it. It reads
  `flowapp 0.2.0 (build 750)`, `Flow 0.1.45 (build 737)` and
  `flow-rogue 0.1.45` alike.
- **What the copies already out do:** the flowapps of 0.1.44.x and
  0.1.45 still read Flow's own copy as `737)`. In those versions alone,
  `list` and Settings' Apps show that until flowapp is upgraded. They
  still offer the upgrade, since `apps::newer` reads `737)` as 0.

## The catalog's flowapp

- **The entry:** `apps.flowapp`, with programs `["flowapp"]`, `bundled:
  "flowapp"` and `flow: "0.1.44"`.
  - `bundled` makes `standing` read Flow's own copy as
    `Standing::Bundled`.
  - `flow` is the oldest Flow out on any system: the Mac's 0.1.45,
    Windows' 0.1.44.3, Linux's 0.1.44.2.
- **What people see:**
  - `outdated` and a bare `upgrade` offer it in every flowapp, including
    those already out.
  - Nothing installs by itself, and Flow doesn't toast it:
    `shelf::upgradable` counts installed apps alone.
  - Settings' Apps shows "comes with Flow", with no button until it's
    installed, then Upgrade and Remove.
  - `keep_carried` installs `bundled` apps only on a first start that
    isn't settled, which every current install already is.
- **Installed:**
  - `apps/flowapp/<ver>/bin/flowapp` is copied to `apps/bin/flowapp`,
    first on a tab's PATH. Windows puts the running copy aside first.
  - `flowapp remove flowapp` goes back to Flow's own copy.
  - `verify`, `info` and the Signed column work on it as on any app.

## Flow's own folder, and the Flow it runs in

- **Flow's own folder** (`apps::flow_dirs`, pure, for the copy in
  `apps/bin`, whose own folder isn't Flow's), the first that holds a
  flowapp:
  1. **The running program's folder,** unless that's in the apps
     folder.
  2. **`FLOW_GAMES`'s other entries,** read as each shell spells them:
     `:` or `;`, and Git Bash's `/c/...` on Windows.
  3. **The usual places:**
     - the Mac: `/Applications` or `~/Applications`, then
       `Flow.app/Contents/Helpers`;
     - Windows: `%LOCALAPPDATA%\Programs\Flow\games` and
       `%ProgramFiles%\Flow\games` (the installer's `{autopf}`);
     - Linux: `~/.local/lib/flow-terminal/games` and
       `/usr/lib/flow-terminal/games`.

  `standing`, the folders where the system's commands are looked for,
  and Windows' busybox all go by it.
- **The Flow it runs in** (`apps::flow_version`, per process, per
  thread in tests; `version::VERSION` unless set). flowapp's `main` sets
  it, in this order:
  1. `TERM_PROGRAM_VERSION`, where `TERM_PROGRAM` is Flow's (every tab
     has both);
  2. else `<Flow's folder>/flow --version`;
  3. else its own build's.

  Every check of a download's `flow` goes by it, and so do the words
  that name "this Flow". Flow itself never sets it.

## The newer copy runs

At the top of `main`, a flowapp looks at the other copy and hands over to
it where that one is newer:
- **Run from the apps folder:** the other copy is Flow's own, its
  version read with `version_in` (one process).
- **Flow's own copy:** the other copy is `apps/bin/flowapp`, its
  version read from `installed.json` (no process).
- **Handing over:** it runs the other with the same arguments and
  `FLOWAPP_HANDED=1`, which stops a second hand-over.
  - On Unix it uses `exec`.
  - On Windows it waits and gives back its exit code.
- **The decision** is `apps::hand_to`: strictly newer only.
- **In practice:**
  - A newer Flow carrying a newer flowapp wins over an older installed
    copy.
  - An installed copy wins over an older Flow's.
  - The copies already out don't hand over. A tab whose shell hashed
    Flow's old copy before the upgrade runs it until a new tab or
    `hash -r`.

## Making its download

The `app-release` scripts take `flowapp` beside flow-sdk:
- `macos/scripts/app-release.sh flowapp`, `windows/scripts/app-release.ps1
  flowapp` and `linux/scripts/app-release.sh flowapp`.
- **What they build:** `flowapp` from this checkout. It must be clean,
  since the build number names the commit, unless `--allow-dirty` is
  given.
- **What they pack:** `bin/flowapp` with its `THIRD-PARTY-NOTICES.txt`.
- **Signing:**
  - The Mac's is signed with the Developer ID and notarized with
    `--notarize`.
  - Each tarball is signed with Flow's key, the `.sig` beside it, and
    written into the catalog as `flowapp`, bundled, needing Flow 0.1.44.
  - Linux's is signed from the Mac, as flow-sdk's is.
- **The release:** its tag is `flowapp-v<version>`.

## Left for later

- Settings' Apps is Flow's own code, so it changes only with Flow.
- Flow's other programs (flow-help, flow-settings, flow-mcp, the flow
  command) could go the same way.
