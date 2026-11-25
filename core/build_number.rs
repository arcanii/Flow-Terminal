// Shared by core/build.rs and the root build.rs (via #[path]).
//
// The build number is the git commit count (as in the sibling apps) plus
// `build_offset` under `[workspace.metadata.flow]` in the root Cargo.toml (0
// where it isn't given), so it goes up with every commit and matches on every
// machine, which WinSparkle's update check relies on; the offset keeps it
// rising in a repository whose history started afresh. Outside a git
// checkout it is 0.

use std::path::Path;
use std::process::Command;

/// The build number for the checkout containing `dir`. Also asks Cargo to
/// rerun the build script when HEAD moves.
pub fn build_number(dir: &Path) -> u32 {
    let git = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
    };
    // The crate may sit below the repo root, so ask git where .git is.
    if let Some(git_dir) = git(&["rev-parse", "--absolute-git-dir"]) {
        for file in ["HEAD", "logs/HEAD", "packed-refs"] {
            let path = Path::new(&git_dir).join(file);
            if path.exists() {
                println!("cargo:rerun-if-changed={}", path.display());
            }
        }
    }
    let Some(count) = git(&["rev-list", "--count", "HEAD"]).and_then(|s| s.parse::<u32>().ok()) else {
        return 0;
    };
    // The workspace's root is the checkout's, wherever the crate sits in it.
    let offset = git(&["rev-parse", "--show-toplevel"])
        .and_then(|root| flow_metadata(Path::new(&root), "build_offset"))
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(0);
    count.saturating_add(offset)
}

/// `key` under `[workspace.metadata.flow]` in the root Cargo.toml in
/// `root`, as written. Also asks Cargo to rerun the build script when that
/// file changes.
fn flow_metadata(root: &Path, key: &str) -> Option<String> {
    let manifest = root.join("Cargo.toml");
    println!("cargo:rerun-if-changed={}", manifest.display());
    let mut in_table = false;
    for line in std::fs::read_to_string(&manifest).unwrap_or_default().lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.starts_with('[') {
            in_table = line == "[workspace.metadata.flow]";
        } else if let Some((k, value)) = line.split_once('=').filter(|_| in_table) {
            if k.trim() == key {
                return Some(value.trim().to_string());
            }
        }
    }
    None
}

/// The version people see: `version`, then a patch release's number, which
/// is `patch` under `[workspace.metadata.flow]` in the root Cargo.toml in
/// `root` (0.1.25.1); `version` alone when there's none.
pub fn shown_version(root: &Path, version: &str) -> String {
    match flow_metadata(root, "patch").and_then(|p| p.parse::<u32>().ok()).unwrap_or(0) {
        0 => version.to_string(),
        patch => format!("{version}.{patch}"),
    }
}

/// Whether the work toward `version` is still going on (`dev = true` under
/// `[workspace.metadata.flow]`), so Flow shows it as 0.1.40.DEV.
pub fn dev(root: &Path) -> bool {
    flow_metadata(root, "dev").as_deref() == Some("true")
}

/// flowapp's own version (`flowapp = "0.2.0"` under
/// `[workspace.metadata.flow]`), which goes out apart from Flow's; none
/// where it isn't given. Only core's build script asks for it.
#[allow(dead_code)]
pub fn flowapp_version(root: &Path) -> Option<String> {
    flow_metadata(root, "flowapp").map(|v| v.trim_matches('"').to_string()).filter(|v| !v.is_empty())
}
