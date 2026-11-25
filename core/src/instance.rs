//! One Flow per program and config folder (`Platform::hand_to_running`,
//! docs/design/put-away.md): Flow started again while it runs brings the
//! running one forward, put away or not, rather than opening a second
//! that can't take the outside apps' pipe or the hotkey and saves its
//! whole config over the first's. Here are the parts every system shares:
//! which Flow is the same, and which starts take part.

use std::path::Path;

use sha2::{Digest, Sha256};

/// The name a start of this Flow goes by: its program and its config
/// folder, hashed. So another build, a copy in another folder or a copy
/// with a scratch config folder starts on its own. Windows' paths don't
/// heed case, so there they're lowercased first.
pub fn id(program: &Path, config: &Path) -> String {
    let said = |path: &Path| {
        let path = path.to_string_lossy();
        let path = path.trim_end_matches(['\\', '/']);
        if cfg!(windows) {
            path.to_lowercase()
        } else {
            path.to_string()
        }
    };
    let mut hash = Sha256::new();
    hash.update(said(program).as_bytes());
    hash.update([0]);
    hash.update(said(config).as_bytes());
    hash.finalize()[..8].iter().map(|b| format!("{b:02x}")).collect()
}

/// Whether a start takes part: a plain start of Flow. A guest started for
/// a command, a snapshot or recording, the Flow opened as administrator
/// to carry on a task (`--carry-on`) and any Flow run as administrator
/// each run beside the user's own, as they always have.
pub fn takes_part(guest: bool, snapshot: bool, carry_on: bool, elevated: bool) -> bool {
    !guest && !snapshot && !carry_on && !elevated
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_program_and_config_folder_are_the_same_flow() {
        let a = id(Path::new(r"C:\Program Files\Flow\flow.exe"), Path::new(r"C:\Users\me\AppData\Roaming\flow-terminal"));
        let b = id(Path::new(r"C:\Program Files\Flow\flow.exe"), Path::new(r"C:\Users\me\AppData\Roaming\flow-terminal\"));
        assert_eq!(a, b, "a trailing separator is the same folder");
        assert_eq!(a.len(), 16);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn another_program_or_config_folder_is_another_flow() {
        let config = Path::new(r"C:\Users\me\AppData\Roaming\flow-terminal");
        let installed = id(Path::new(r"C:\Program Files\Flow\flow.exe"), config);
        assert_ne!(installed, id(Path::new(r"C:\Users\me\Flow-test\flow.exe"), config), "a copy elsewhere");
        assert_ne!(installed, id(Path::new(r"C:\Program Files\Flow\flow.exe"), Path::new(r"C:\scratch\flow-terminal")), "a scratch profile");
        // The two parts are kept apart, so moving a letter across doesn't match.
        assert_ne!(id(Path::new("ab"), Path::new("c")), id(Path::new("a"), Path::new("bc")));
    }

    #[test]
    fn case_is_heeded_only_where_paths_heed_it() {
        let lower = id(Path::new(r"c:\program files\flow\flow.exe"), Path::new(r"c:\users\me\appdata\roaming\flow-terminal"));
        let upper = id(Path::new(r"C:\Program Files\Flow\FLOW.EXE"), Path::new(r"C:\Users\Me\AppData\Roaming\flow-terminal"));
        assert_eq!(lower == upper, cfg!(windows));
    }

    #[test]
    fn only_a_plain_start_takes_part() {
        assert!(takes_part(false, false, false, false));
        assert!(!takes_part(true, false, false, false), "a guest");
        assert!(!takes_part(false, true, false, false), "a snapshot or recording");
        assert!(!takes_part(false, false, true, false), "carrying on as administrator");
        assert!(!takes_part(false, false, false, true), "run as administrator");
    }
}
