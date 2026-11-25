//! What the assistant changed, kept so it can be put back: before
//! `write_file` changes a file, the file as it was (or that there was
//! none) goes into a folder of the task's own under [`DIR`] beside
//! `config.toml`, listed in its `changes.json`. Only a file's first change
//! in a task is kept, so putting it back puts back what was there before
//! the task touched it. A task keeps at most [`TASK_BYTES`], and the
//! oldest tasks' go once all of them pass [`ALL_BYTES`].

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The folder of what's kept, beside `config.toml`.
pub const DIR: &str = "assistant-undo";
/// The most a task keeps, in bytes.
pub const TASK_BYTES: u64 = 20 * 1024 * 1024;
/// The most all tasks keep, in bytes, the oldest going past it.
pub const ALL_BYTES: u64 = 200 * 1024 * 1024;
/// A task's list of what it changed.
const LIST: &str = "changes.json";

/// A file the assistant changed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Change {
    pub path: PathBuf,
    /// Its copy as it was, in the task's folder; none where there was no
    /// file, which putting back takes away.
    pub kept: Option<String>,
    /// When, in milliseconds since 1970.
    pub at: i64,
}

/// Where changes are kept; none keeps nothing (snapshots).
pub struct Undo {
    dir: Option<PathBuf>,
}

impl Undo {
    pub fn open(config_dir: &Path) -> Self {
        Undo { dir: Some(config_dir.join(DIR)) }
    }

    pub fn none() -> Self {
        Undo { dir: None }
    }

    fn task_dir(&self, task: &str) -> Option<PathBuf> {
        self.dir.as_ref().map(|d| d.join(task))
    }

    /// What task `task` changed, oldest first.
    pub fn changes(&self, task: &str) -> Vec<Change> {
        self.task_dir(task)
            .and_then(|d| std::fs::read(d.join(LIST)).ok())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    fn save(&self, task: &str, changes: &[Change]) -> Result<(), String> {
        let dir = self.task_dir(task).ok_or("nothing is kept here")?;
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let json = serde_json::to_vec_pretty(changes).map_err(|e| e.to_string())?;
        std::fs::write(dir.join(LIST), json).map_err(|e| e.to_string())
    }

    /// Keeps `path` as it is now, before task `task` changes it, unless
    /// the task changed it before. Err where it can't be kept (too big, or
    /// no room left), which leaves the change to go ahead unkept.
    pub fn keep(&self, task: &str, path: &Path, at: i64) -> Result<(), String> {
        let Some(dir) = self.task_dir(task) else { return Ok(()) };
        let mut changes = self.changes(task);
        if changes.iter().any(|c| c.path == path) {
            return Ok(());
        }
        let kept = match std::fs::metadata(path) {
            Ok(meta) if meta.is_file() => {
                let used: u64 = changes.iter().filter_map(|c| c.kept.as_ref()).filter_map(|k| std::fs::metadata(dir.join(k)).ok()).map(|m| m.len()).sum();
                if used + meta.len() > TASK_BYTES {
                    return Err(format!("{} is too big to keep a copy of", path.display()));
                }
                std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                let name = format!("{}-{}", changes.len(), path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "file".into()));
                std::fs::copy(path, dir.join(&name)).map_err(|e| e.to_string())?;
                Some(name)
            }
            Ok(_) => return Err(format!("{} isn't a file", path.display())),
            Err(_) => None,
        };
        changes.push(Change { path: path.to_path_buf(), kept, at });
        self.save(task, &changes)?;
        self.prune();
        Ok(())
    }

    /// Puts back what task `task` changed, newest first: `path` alone, or
    /// all of it. Each file goes back as it was before the task, or goes
    /// where the task made it. The files put back.
    pub fn undo(&self, task: &str, path: Option<&Path>) -> Result<Vec<PathBuf>, String> {
        let Some(dir) = self.task_dir(task) else { return Err("Nothing is kept here.".into()) };
        let mut changes = self.changes(task);
        let mut done = Vec::new();
        let mut errors = Vec::new();
        for i in (0..changes.len()).rev() {
            if path.is_some_and(|p| p != changes[i].path) {
                continue;
            }
            let change = &changes[i];
            let put = match &change.kept {
                Some(kept) => std::fs::copy(dir.join(kept), &change.path).map(|_| ()),
                None => match std::fs::remove_file(&change.path) {
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                    other => other,
                },
            };
            match put {
                Ok(()) => {
                    if let Some(kept) = &change.kept {
                        let _ = std::fs::remove_file(dir.join(kept));
                    }
                    done.push(change.path.clone());
                    changes.remove(i);
                }
                Err(e) => errors.push(format!("{}: {e}", change.path.display())),
            }
        }
        self.save(task, &changes)?;
        match (done.is_empty(), errors.is_empty()) {
            (true, true) => Err(match path {
                Some(p) => format!("Nothing kept for {} in this task.", p.display()),
                None => "This task changed no file that's kept.".into(),
            }),
            (_, false) => Err(format!("Couldn't put back: {}", errors.join("; "))),
            _ => Ok(done),
        }
    }

    /// The oldest tasks' copies go, while all of them pass [`ALL_BYTES`].
    fn prune(&self) {
        let Some(dir) = &self.dir else { return };
        let Ok(read) = std::fs::read_dir(dir) else { return };
        let mut tasks: Vec<(std::time::SystemTime, PathBuf, u64)> = read
            .flatten()
            .filter(|e| e.path().is_dir())
            .map(|e| {
                let size = std::fs::read_dir(e.path()).map(|r| r.flatten().filter_map(|f| f.metadata().ok()).map(|m| m.len()).sum()).unwrap_or(0);
                (e.metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH), e.path(), size)
            })
            .collect();
        tasks.sort();
        let mut total: u64 = tasks.iter().map(|t| t.2).sum();
        for (_, path, size) in tasks {
            if total <= ALL_BYTES {
                break;
            }
            if std::fs::remove_dir_all(&path).is_ok() {
                total = total.saturating_sub(size);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("flow-undo-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_file_changed_goes_back_as_it_was_before_the_task_and_one_made_goes() {
        let dir = scratch("back");
        let undo = Undo::open(&dir.join("config"));
        let (old, new) = (dir.join("hosts"), dir.join("script.sh"));
        std::fs::write(&old, "127.0.0.1 localhost\n").unwrap();
        undo.keep("7", &old, 1).unwrap();
        std::fs::write(&old, "changed once\n").unwrap();
        // A second change keeps nothing more: back is before the task.
        undo.keep("7", &old, 2).unwrap();
        std::fs::write(&old, "changed twice\n").unwrap();
        undo.keep("7", &new, 3).unwrap();
        std::fs::write(&new, "#!/bin/sh\n").unwrap();
        assert_eq!(undo.changes("7").len(), 2);
        assert_eq!(undo.undo("7", Some(&new)).unwrap(), [new.clone()]);
        assert!(!new.exists(), "a file the task made goes");
        assert_eq!(undo.undo("7", None).unwrap(), [old.clone()]);
        assert_eq!(std::fs::read_to_string(&old).unwrap(), "127.0.0.1 localhost\n");
        assert!(undo.changes("7").is_empty());
        assert!(undo.undo("7", None).is_err(), "nothing left to put back");
        assert!(Undo::none().keep("7", &old, 1).is_ok() && Undo::none().undo("7", None).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_past_the_tasks_room_isnt_kept() {
        let dir = scratch("room");
        let undo = Undo::open(&dir.join("config"));
        let big = dir.join("big.bin");
        let file = std::fs::File::create(&big).unwrap();
        file.set_len(TASK_BYTES + 1).unwrap();
        assert!(undo.keep("1", &big, 1).is_err());
        assert!(undo.changes("1").is_empty());
        assert!(undo.keep("1", &dir, 1).is_err(), "a folder isn't a file");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
