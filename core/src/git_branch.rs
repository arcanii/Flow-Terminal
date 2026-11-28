//! The git branch a folder is on, for the line under a tab in the sidebar
//! (`docs/design/tab-groups.md`'s seventh step): read from the checkout's
//! own files, never by running `git`, which in a large checkout takes
//! seconds and runs the checkout's hooks and settings. It's read on a
//! thread of its own (`App::ask_branch`), only for a folder not read since
//! it last changed.
//!
//! The folder comes from what a shell said, which any program printed in a
//! tab can say, so it's read only where it's on this computer, and so is
//! a `.git` file's `gitdir:` (`link::on_this_computer`): merely looking at
//! a share on Windows hands its host the user's NTLM hash. Only regular
//! files are read, and only their first few kilobytes, so a FIFO or a
//! device named `HEAD` never holds the thread.

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};

/// What a checkout's `HEAD` names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Head {
    /// A branch, by its name (`refs/heads/` taken off).
    Branch(String),
    /// No branch: the commit it's on, by its id's first seven.
    Detached(String),
}

/// The most of `HEAD` or a `.git` file read: git's are a line long.
const MOST: u64 = 4096;

/// The most of a branch's name kept, in letters.
const MOST_NAME: usize = 200;

/// The most folders whose answers are kept: past it they're all forgotten
/// and read again as they're shown.
const MOST_KEPT: usize = 256;

/// Each folder's branch as it was last read, and the folders being read:
/// a folder is read once, and again only once it's forgotten, as a command
/// ends in it (which may have checked out another branch).
#[derive(Debug, Default)]
pub struct Branches {
    known: HashMap<PathBuf, Option<Head>>,
    reading: HashSet<PathBuf>,
}

impl Branches {
    /// Folder `folder`'s branch, where it's been read: none outside a
    /// checkout. None where it hasn't been read yet.
    pub fn of(&self, folder: &Path) -> Option<Option<&Head>> {
        self.known.get(folder).map(Option::as_ref)
    }

    /// Whether `folder` is to be read now: neither known nor being read. It
    /// is being read from then on.
    pub fn to_read(&mut self, folder: &Path) -> bool {
        if self.known.contains_key(folder) || self.reading.contains(folder) {
            return false;
        }
        self.reading.insert(folder.to_path_buf());
        true
    }

    /// Folder `folder` read: its branch, or none.
    pub fn read(&mut self, folder: PathBuf, head: Option<Head>) {
        self.reading.remove(&folder);
        if self.known.len() >= MOST_KEPT {
            self.known.clear();
        }
        self.known.insert(folder, head);
    }

    /// Folder `folder` forgotten, to be read again as it's next shown.
    pub fn forget(&mut self, folder: &Path) {
        self.known.remove(folder);
    }

    /// Nothing being read after all (a thread that couldn't start).
    pub fn give_up_reading(&mut self) {
        self.reading.clear();
    }
}

/// The `HEAD` of the checkout `folder` is in, found by looking up from it
/// for `.git`: a folder, or, in a worktree or a submodule, a file naming
/// the folder (`gitdir:`). None outside a checkout, where what's there
/// isn't git's, or where the folder or the one named isn't on this
/// computer.
pub fn head_of(folder: &Path) -> Option<Head> {
    if !crate::link::on_this_computer(folder) {
        return None;
    }
    for dir in folder.ancestors() {
        let dot = dir.join(".git");
        let Ok(meta) = std::fs::metadata(&dot) else { continue };
        let git_dir = if meta.is_dir() {
            dot
        } else if meta.is_file() {
            git_dir_named(&read_small(&dot)?, dir)?
        } else {
            return None;
        };
        if !crate::link::on_this_computer(&git_dir) {
            return None;
        }
        return parse_head(&read_small(&git_dir.join("HEAD"))?);
    }
    None
}

/// The folder a `.git` file in `dir` names (`gitdir: <path>`), from `dir`
/// where it's relative, as a submodule's is.
fn git_dir_named(text: &str, dir: &Path) -> Option<PathBuf> {
    let named = text.lines().find_map(|l| l.strip_prefix("gitdir:"))?.trim();
    (!named.is_empty()).then(|| dir.join(named))
}

/// A regular file's first [`MOST`] bytes as text; none for anything else.
fn read_small(path: &Path) -> Option<String> {
    if !std::fs::metadata(path).ok()?.is_file() {
        return None;
    }
    let mut text = String::new();
    std::fs::File::open(path).ok()?.take(MOST).read_to_string(&mut text).ok()?;
    Some(text)
}

/// What a `HEAD` file says: `ref: refs/heads/<branch>`, or a commit's id
/// (SHA-1's 40 letters, SHA-256's 64) where it's detached. A name's
/// control characters are left out and it's cut at [`MOST_NAME`].
pub fn parse_head(text: &str) -> Option<Head> {
    let line = text.lines().next()?.trim();
    if let Some(named) = line.strip_prefix("ref:") {
        let named = named.trim();
        let name: String = named.strip_prefix("refs/heads/").unwrap_or(named).chars().filter(|c| !c.is_control()).take(MOST_NAME).collect();
        return (!name.is_empty()).then_some(Head::Branch(name));
    }
    let id = matches!(line.len(), 40 | 64) && line.chars().all(|c| c.is_ascii_hexdigit());
    id.then(|| Head::Detached(line[..7].to_ascii_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn head_names_a_branch_or_a_detached_commit() {
        assert_eq!(parse_head("ref: refs/heads/main\n"), Some(Head::Branch("main".into())));
        assert_eq!(parse_head("ref: refs/heads/feature/tab-groups"), Some(Head::Branch("feature/tab-groups".into())));
        assert_eq!(parse_head("ref: refs/remotes/origin/main"), Some(Head::Branch("refs/remotes/origin/main".into())), "not a branch's: as it's named");
        assert_eq!(parse_head("3c128fd1c9e0d2f7a6b5e4d3c2b1a09f8e7d6c5b\n"), Some(Head::Detached("3c128fd".into())));
        assert_eq!(parse_head(&"ab".repeat(32)), Some(Head::Detached("abababa".into())), "SHA-256's");
        assert_eq!(parse_head("ref: refs/heads/a\u{1b}[31mb"), Some(Head::Branch("a[31mb".into())), "no control characters");
        assert_eq!(parse_head(&format!("ref: refs/heads/{}", "x".repeat(500))).map(|h| matches!(h, Head::Branch(n) if n.len() == MOST_NAME)), Some(true));
        for not in ["", "ref:", "ref: refs/heads/", "main", "3c128fd", "zz128fd1c9e0d2f7a6b5e4d3c2b1a09f8e7d6c5b"] {
            assert_eq!(parse_head(not), None, "{not:?}");
        }
    }

    #[test]
    fn a_folder_s_branch_is_read_up_from_it_from_a_checkout_a_worktree_or_a_submodule() {
        let root = std::env::temp_dir().join(format!("flow-git-branch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let write = |path: PathBuf, text: &str| {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        // A checkout, read from a folder deep inside it.
        let repo = root.join("repo");
        write(repo.join(".git/HEAD"), "ref: refs/heads/main\n");
        std::fs::create_dir_all(repo.join("src/app")).unwrap();
        assert_eq!(head_of(&repo.join("src/app")), Some(Head::Branch("main".into())));
        // A worktree's `.git` names its folder in the checkout's.
        write(repo.join(".git/worktrees/fix/HEAD"), "ref: refs/heads/fix\n");
        let tree = root.join("fix");
        write(tree.join(".git"), &format!("gitdir: {}\n", repo.join(".git/worktrees/fix").display()));
        assert_eq!(head_of(&tree), Some(Head::Branch("fix".into())));
        // A submodule's names it from where it is, and is detached.
        write(repo.join(".git/modules/lib/HEAD"), &"0123456789abcdef".repeat(2).chars().chain("01234567".chars()).collect::<String>());
        write(repo.join("lib/.git"), "gitdir: ../.git/modules/lib\n");
        assert_eq!(head_of(&repo.join("lib")), Some(Head::Detached("0123456".into())));
        // No checkout above it, and a `.git` file that names nothing.
        let plain = root.join("plain");
        std::fs::create_dir_all(&plain).unwrap();
        assert_eq!(head_of(&plain), None);
        write(root.join("odd/.git"), "nothing here\n");
        assert_eq!(head_of(&root.join("odd")), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn each_folder_is_read_once_until_it_s_forgotten() {
        let mut b = Branches::default();
        let (a, c) = (Path::new("/src/a"), Path::new("/src/c"));
        assert_eq!(b.of(a), None);
        assert!(b.to_read(a));
        assert!(!b.to_read(a), "being read");
        b.read(a.to_path_buf(), Some(Head::Branch("main".into())));
        assert_eq!(b.of(a), Some(Some(&Head::Branch("main".into()))));
        assert!(!b.to_read(a), "known");
        b.read(c.to_path_buf(), None);
        assert_eq!(b.of(c), Some(None), "no checkout");
        b.forget(a);
        assert_eq!(b.of(a), None);
        assert!(b.to_read(a), "forgotten: read again");
        b.give_up_reading();
        assert!(b.to_read(a));
        for k in 0..MOST_KEPT + 1 {
            b.read(PathBuf::from(format!("/src/{k}")), None);
        }
        assert!(b.known.len() <= MOST_KEPT);
    }

    #[test]
    fn a_folder_or_a_gitdir_elsewhere_is_never_looked_at() {
        assert_eq!(head_of(Path::new("//server/share/repo")), None);
        let root = std::env::temp_dir().join(format!("flow-git-elsewhere-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join(".git"), "gitdir: //server/share/repo/.git\n").unwrap();
        assert_eq!(head_of(&root), None);
        let _ = std::fs::remove_dir_all(&root);
    }
}
