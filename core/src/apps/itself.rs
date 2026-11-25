//! flowapp as an app of its own catalog (docs/design/flowapp-updates.md):
//! a copy of it installed in the apps folder runs in Flow's tabs ahead of
//! the one Flow carries, since `apps/bin` comes first on their PATH. Such a
//! copy has to find Flow's own folder ([`flow_dir`]), whose programs aren't
//! beside it, and the Flow it runs in ([`flow_version`]), which isn't the
//! one it was built with; and of the two copies, the newer runs
//! ([`hand_to`], [`other_copy`]).

use std::path::{Path, PathBuf};

use super::{exe, version_of, Installed};

/// The catalog's name for flowapp itself, and its program's.
pub const NAME: &str = "flowapp";

/// Set in the copy a flowapp hands over to, so it never hands over again.
pub const HANDED: &str = "FLOWAPP_HANDED";

/// Whether `dir` is in the apps folder `apps`, where an installed copy of
/// flowapp lives (`apps/bin`, or its version's own `bin/`).
pub fn in_apps(dir: &Path, apps: &Path) -> bool {
    dir.starts_with(apps) || matches!((dir.canonicalize(), apps.canonicalize()), (Ok(dir), Ok(apps)) if dir.starts_with(&apps))
}

/// The folders `FLOW_GAMES` names, as the shell that gives it spells them:
/// parted by `;` on Windows (PowerShell, cmd, BusyBox), or there by `:` with
/// Git Bash's `/c/...` turned back into `C:\...`; by `:` elsewhere.
pub fn games_entries(games: &str, windows: bool) -> Vec<PathBuf> {
    let games = games.trim();
    if games.is_empty() {
        return Vec::new();
    }
    if !windows {
        return games.split(':').filter(|p| !p.is_empty()).map(PathBuf::from).collect();
    }
    if games.contains(';') || !games.starts_with('/') {
        return games.split(';').filter(|p| !p.is_empty()).map(PathBuf::from).collect();
    }
    games.split(':').filter_map(from_git_bash).collect()
}

/// `/c/Program Files/Flow/games` as Windows writes it, `C:\Program
/// Files\Flow\games`; none for a path that names no drive.
fn from_git_bash(path: &str) -> Option<PathBuf> {
    let rest = path.strip_prefix('/')?;
    let (drive, tail) = rest.split_once('/').unwrap_or((rest, ""));
    let drive = drive.chars().next().filter(|c| c.is_ascii_alphabetic() && drive.len() == 1)?;
    Some(PathBuf::from(format!("{}:\\{}", drive.to_ascii_uppercase(), tail.replace('/', "\\"))))
}

/// Where Flow's own programs may be, in the order to look: the running
/// flowapp's folder `exe_dir`, unless it's in the apps folder `apps`; then
/// `games`' (`FLOW_GAMES`) folders outside the apps folder; then `usual`,
/// the places Flow is installed to.
pub fn flow_dirs(exe_dir: Option<&Path>, games: &[PathBuf], apps: &Path, usual: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = exe_dir.filter(|d| !in_apps(d, apps)).map(Path::to_path_buf).into_iter().collect();
    dirs.extend(games.iter().filter(|d| !in_apps(d, apps)).cloned());
    dirs.extend(usual);
    dirs.dedup();
    dirs
}

/// The places each system's Flow is installed to, its folder of programs:
/// the Mac's `Flow.app/Contents/Helpers` in `/Applications` or
/// `~/Applications`; Windows' `games\` beside flow.exe, per user or for
/// everyone (the installer's `{autopf}`); Linux's
/// `lib/flow-terminal/games` in `~/.local` (`install.sh`'s) or `/usr`
/// (the .deb's).
pub fn usual_flow_dirs() -> Vec<PathBuf> {
    let home = dirs::home_dir();
    if cfg!(target_os = "macos") {
        let helpers = |apps: PathBuf| apps.join("Flow.app").join("Contents").join("Helpers");
        std::iter::once(helpers(PathBuf::from("/Applications"))).chain(home.map(|h| helpers(h.join("Applications")))).collect()
    } else if cfg!(windows) {
        let local = std::env::var_os("LOCALAPPDATA").map(|l| PathBuf::from(l).join("Programs").join("Flow").join("games"));
        let everyone = std::env::var_os("ProgramFiles").map(|p| PathBuf::from(p).join("Flow").join("games"));
        local.into_iter().chain(everyone).collect()
    } else {
        let games = |prefix: PathBuf| prefix.join("lib").join("flow-terminal").join("games");
        home.map(|h| games(h.join(".local"))).into_iter().chain(std::iter::once(games(PathBuf::from("/usr")))).collect()
    }
}

/// Flow's own folder of programs, where the copy of flowapp it carries is:
/// the first of [`flow_dirs`] that holds one.
pub fn flow_dir() -> Option<PathBuf> {
    let exe_dir = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf));
    let games = std::env::var("FLOW_GAMES").map(|g| games_entries(&g, cfg!(windows))).unwrap_or_default();
    flow_dirs(exe_dir.as_deref(), &games, &super::dir(), usual_flow_dirs()).into_iter().find(|d| d.join(exe(NAME)).is_file())
}

/// Flow's own program beside its folder of programs `dir`, whose
/// `--version` names Flow's version: the flow command in the Mac's
/// `Helpers` and Windows' `games\`, Flow itself in Linux's `bin/`, three
/// folders up from `lib/flow-terminal/games`.
pub fn flow_program(dir: &Path) -> PathBuf {
    if cfg!(any(target_os = "macos", windows)) {
        dir.join(exe("flow"))
    } else {
        dir.join("..").join("..").join("..").join("bin").join("flow")
    }
}

/// The Flow a flowapp runs in: the one whose tab it's in, by
/// `TERM_PROGRAM` (Flow's [`crate::APP_ID`]) and `TERM_PROGRAM_VERSION`,
/// else what Flow's own program said it is (`said`, asked only where
/// there's no tab to go by); none where neither says.
pub fn running_flow(term_program: Option<&str>, term_version: Option<&str>, said: impl FnOnce() -> Option<String>) -> Option<String> {
    let in_tab = term_program == Some(crate::APP_ID);
    match term_version.map(str::trim).filter(|v| in_tab && !v.is_empty()) {
        Some(version) => Some(version.to_string()),
        None => said(),
    }
}

#[cfg(not(test))]
static FLOW: std::sync::OnceLock<String> = std::sync::OnceLock::new();

#[cfg(test)]
thread_local! {
    static FLOW: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

/// Sets the Flow flowapp runs in, once, as it starts (per thread in tests).
pub fn set_flow_version(version: String) {
    #[cfg(not(test))]
    let _ = FLOW.set(version);
    #[cfg(test)]
    FLOW.with(|f| *f.borrow_mut() = Some(version));
}

/// The version of the Flow this runs in, which a download's `flow` is
/// checked against: what flowapp set as it started, else this build's own
/// ([`crate::version::VERSION`]), as in Flow itself, which never sets it.
pub fn flow_version() -> String {
    #[cfg(not(test))]
    let set = FLOW.get().cloned();
    #[cfg(test)]
    let set = FLOW.with(|f| f.borrow().clone());
    set.unwrap_or_else(|| crate::version::VERSION.to_string())
}

/// Whether a flowapp of version `own` hands over to the other copy, of
/// version `other`: only where that one is newer, and never twice
/// (`handed`, [`HANDED`] set).
pub fn hand_to(own: &str, other: &str, handed: bool) -> bool {
    !handed && super::newer(other, own)
}

/// The other copy of flowapp and its version, for the one running from
/// `exe_dir`: Flow's own, as its `--version` says, for a copy in the apps
/// folder `apps`; else the one installed there (`apps/bin`), as
/// `installed.json` says, asking no program.
pub fn other_copy(exe_dir: &Path, apps: &Path) -> Option<(PathBuf, String)> {
    if in_apps(exe_dir, apps) {
        let other = flow_dir()?.join(exe(NAME));
        let version = version_of(&other)?;
        Some((other, version))
    } else {
        let other = apps.join("bin").join(exe(NAME));
        let version = Installed::load(apps).apps.get(NAME)?.version.clone();
        other.is_file().then_some((other, version))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_newer_copy_is_handed_to_and_never_twice() {
        assert!(hand_to("0.2.0", "0.2.1", false));
        assert!(!hand_to("0.2.0", "0.2.0", false), "the same runs itself");
        assert!(!hand_to("0.2.1", "0.2.0", false), "an older one never");
        assert!(!hand_to("0.2.0", "0.1.45", false), "a Flow's old flowapp, by Flow's version, is older");
        assert!(!hand_to("0.2.0", "0.3.0", true), "a copy handed to hands over no more");
    }

    #[test]
    fn flow_games_is_read_as_each_shell_spells_it() {
        assert_eq!(games_entries("/a/apps/bin:/Applications/Flow.app/Contents/Helpers", false), [PathBuf::from("/a/apps/bin"), PathBuf::from("/Applications/Flow.app/Contents/Helpers")]);
        assert_eq!(games_entries(r"C:\Users\me\AppData\Roaming\flow-terminal\apps\bin;C:\Program Files\Flow\games", true), [PathBuf::from(r"C:\Users\me\AppData\Roaming\flow-terminal\apps\bin"), PathBuf::from(r"C:\Program Files\Flow\games")]);
        assert_eq!(games_entries("/c/Users/me/AppData/Roaming/flow-terminal/apps/bin:/c/Program Files/Flow/games", true), [PathBuf::from(r"C:\Users\me\AppData\Roaming\flow-terminal\apps\bin"), PathBuf::from(r"C:\Program Files\Flow\games")]);
        assert_eq!(games_entries(r"C:\Program Files\Flow\games", true), [PathBuf::from(r"C:\Program Files\Flow\games")], "one folder, no separator");
        assert!(games_entries("  ", false).is_empty());
    }

    #[test]
    fn flows_folder_is_the_programs_own_unless_its_an_installed_copy() {
        let apps = Path::new("/home/me/.config/flow-terminal/apps");
        let helpers = PathBuf::from("/Applications/Flow.app/Contents/Helpers");
        let usual = vec![PathBuf::from("/usr/lib/flow-terminal/games")];
        // Flow's own copy: its folder first.
        assert_eq!(flow_dirs(Some(&helpers), &[], apps, usual.clone())[0], helpers);
        // A copy in the apps folder: FLOW_GAMES' other folders, then the usual.
        let games = [apps.join("bin"), helpers.clone()];
        assert_eq!(flow_dirs(Some(&apps.join("bin")), &games, apps, usual.clone()), [helpers.clone(), usual[0].clone()]);
        assert_eq!(flow_dirs(Some(&apps.join("flowapp").join("0.2.0").join("bin")), &[], apps, usual.clone()), usual, "outside a tab, the usual places");
    }

    #[test]
    fn the_flow_it_runs_in_is_the_tabs_then_what_flow_says() {
        let said = || Some("0.1.44.3".to_string());
        assert_eq!(running_flow(Some(crate::APP_ID), Some("0.1.45"), said).as_deref(), Some("0.1.45"));
        assert_eq!(running_flow(Some("iTerm.app"), Some("3.5.0"), said).as_deref(), Some("0.1.44.3"), "another terminal's version is no Flow's");
        assert_eq!(running_flow(Some(crate::APP_ID), Some(""), said).as_deref(), Some("0.1.44.3"));
        assert_eq!(running_flow(None, None, || None), None);
    }

    #[test]
    fn a_download_is_held_back_by_the_flow_flowapp_runs_in() {
        assert_eq!(flow_version(), crate::version::VERSION, "unset, this build's own");
        let download = super::super::Download { flow: "0.1.44".into(), ..Default::default() };
        set_flow_version("0.1.40".into());
        assert_eq!(download.needs_newer_flow(&flow_version()), Some("0.1.44"));
        set_flow_version("0.1.44.2".into());
        assert_eq!(download.needs_newer_flow(&flow_version()), None);
    }

    #[test]
    fn flows_own_program_names_flows_version_beside_its_folder() {
        let dir = Path::new("/opt/flow/lib/flow-terminal/games");
        let program = flow_program(dir);
        if cfg!(any(target_os = "macos", windows)) {
            assert_eq!(program, dir.join(exe("flow")));
        } else {
            assert_eq!(program, dir.join("../../../bin/flow"));
        }
    }

    #[cfg(unix)]
    #[test]
    fn the_installed_copy_is_the_other_for_flows_own_and_flows_for_the_installed() {
        use std::os::unix::fs::PermissionsExt;
        let root = super::super::tests::tempdir();
        let apps = root.join("apps");
        let flows = root.join("Helpers");
        std::fs::create_dir_all(apps.join("bin")).unwrap();
        std::fs::create_dir_all(&flows).unwrap();
        // Flow's own copy looks at the installed one, by installed.json.
        assert_eq!(other_copy(&flows, &apps), None, "none installed");
        std::fs::write(apps.join("bin").join(NAME), "#!/bin/sh\n").unwrap();
        std::fs::write(apps.join("installed.json"), r#"{"apps":{"flowapp":{"version":"0.2.1","programs":["flowapp"]}}}"#).unwrap();
        assert_eq!(other_copy(&flows, &apps), Some((apps.join("bin").join(NAME), "0.2.1".into())));
        // A stand-in for Flow's own copy says its version as flowapp does.
        let own = flows.join(NAME);
        std::fs::write(&own, "#!/bin/sh\necho 'flowapp 0.2.3 (build 9)'\n").unwrap();
        std::fs::set_permissions(&own, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(version_of(&own).as_deref(), Some("0.2.3"));
    }
}
