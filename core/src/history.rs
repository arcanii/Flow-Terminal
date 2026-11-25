//! The command history: every command run in Flow's tabs, whatever the
//! shell, kept with its folder, exit code, shell and when it ran, in SQLite
//! beside `config.toml`, and found by any words in it. The text is the
//! shell's own where its hook sends it (zsh, bash and PowerShell, OSC
//! 633;E with the pane's nonce, `shell::NONCE`, so no program's output can
//! say it), else the grid's at Enter (`Session::write`); a line bash's own
//! history left out keeps the grid's only where that's the newest line
//! bash kept, and one the hook said nothing of, though it says each line
//! it runs, none (`HistoryNote::LeftOut`). Lines that start with a
//! space, or look like they set a password, a token or a key, are never
//! kept ([`keep`]), and nothing here leaves the computer.
//!
//! Each run is a row of `runs`; `runs_text` indexes their text in trigrams
//! (FTS5), so any three letters in a row find a command, in any script. A
//! search shows each command once, at its latest run.

use std::path::Path;

use rusqlite::{params, params_from_iter, Connection, ToSql};

/// The most runs kept: past it, the oldest go.
const MAX_RUNS: i64 = 100_000;
/// How many results a search shows at most.
pub const SHOWN: usize = 200;

/// One run of a command, as it's recorded.
#[derive(Debug, Clone, PartialEq)]
pub struct Run<'a> {
    pub command: &'a str,
    pub cwd: Option<&'a str>,
    /// The shell profile's id (`zsh`, `powershell`, `cmd`, …).
    pub shell: &'a str,
    /// When it started, in milliseconds since 1970.
    pub started: i64,
    /// The assistant ran it, not the user.
    pub assistant: bool,
}

/// A command found, as its latest run (within the filter) left it.
#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    pub command: String,
    pub cwd: Option<String>,
    pub exit: Option<i32>,
    pub started: i64,
    pub assistant: bool,
    /// How many times it ran (within the filter).
    pub runs: u32,
}

/// Which runs a search looks at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Filter {
    #[default]
    All,
    /// Run in the folder the tab in front is in.
    Folder,
    /// Ended with an exit code other than 0.
    Failed,
    /// Run in the shell the tab in front runs.
    Shell,
}

impl Filter {
    pub const ALL: [Filter; 4] = [Filter::All, Filter::Folder, Filter::Failed, Filter::Shell];
}

/// A search: words that must all be in the command, and a filter, with the
/// folder and shell it means.
#[derive(Debug, Clone, Copy)]
pub struct Query<'a> {
    pub words: &'a str,
    pub filter: Filter,
    pub cwd: Option<&'a str>,
    pub shell: &'a str,
}

/// The history's database, or none if it couldn't be opened (then nothing
/// is kept, and searches find nothing).
pub struct History {
    db: Option<Connection>,
}

impl History {
    /// Opens (or makes) the history at `path`.
    pub fn open(path: &Path) -> Self {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let db = Connection::open(path).and_then(|db| {
            prepare(&db)?;
            Ok(db)
        });
        // Only its owner reads it, as shells keep their history files.
        // SQLite gives its journal files the same.
        #[cfg(unix)]
        if db.is_ok() {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
        }
        match db {
            Ok(db) => Self { db: Some(db) },
            Err(err) => {
                log::warn!("command history unavailable ({}): {err}", path.display());
                Self { db: None }
            }
        }
    }

    /// A history kept in memory only, gone when Flow quits: for snapshots
    /// and recordings, whose commands aren't the user's, and for tests.
    pub fn in_memory() -> Self {
        let db = Connection::open_in_memory().and_then(|db| {
            prepare(&db)?;
            Ok(db)
        });
        Self { db: log_err(db) }
    }

    /// Keeps a run that started; its row, to finish later.
    pub fn record(&self, run: &Run<'_>) -> Option<i64> {
        let db = self.db.as_ref()?;
        let added = db.execute(
            "INSERT INTO runs (command, cwd, shell, started, assistant) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![run.command, run.cwd, run.shell, run.started, run.assistant],
        );
        let id = log_err(added).map(|_| db.last_insert_rowid())?;
        if id % 500 == 0 {
            log_err(db.execute("DELETE FROM runs WHERE id <= ?1", params![id - MAX_RUNS]));
        }
        Some(id)
    }

    /// The shell's own word for a run's command, which beats the grid's.
    pub fn set_command(&self, id: i64, command: &str) {
        if let Some(db) = &self.db {
            log_err(db.execute("UPDATE runs SET command = ?2 WHERE id = ?1 AND command != ?2", params![id, command]));
        }
    }

    /// How a run ended, and how long it took.
    pub fn finish(&self, id: i64, exit: Option<i32>, secs: f64) {
        if let Some(db) = &self.db {
            log_err(db.execute("UPDATE runs SET exit = ?2, secs = ?3 WHERE id = ?1", params![id, exit, secs]));
        }
    }

    /// Takes back a run, as when its command turns out to be one not to keep.
    pub fn remove(&self, id: i64) {
        if let Some(db) = &self.db {
            log_err(db.execute("DELETE FROM runs WHERE id = ?1", params![id]));
        }
    }

    /// The commands `query` finds, each once at its latest run, the most
    /// recently run first.
    pub fn search(&self, query: &Query<'_>) -> Vec<Found> {
        let Some(db) = &self.db else { return Vec::new() };
        let (cond, args) = conditions(query);
        let sql = format!(
            "SELECT r.command, r.cwd, r.exit, r.started, r.assistant, g.n FROM runs r \
             JOIN (SELECT MAX(id) AS last, COUNT(*) AS n FROM runs WHERE {cond} GROUP BY command) g ON r.id = g.last \
             ORDER BY r.started DESC, r.id DESC LIMIT {SHOWN}"
        );
        let found = db.prepare_cached(&sql).and_then(|mut st| {
            let rows = st.query_map(params_from_iter(args.iter()), |r| {
                Ok(Found { command: r.get(0)?, cwd: r.get(1)?, exit: r.get(2)?, started: r.get(3)?, assistant: r.get(4)?, runs: r.get(5)? })
            })?;
            rows.collect::<Result<Vec<_>, _>>()
        });
        log_err(found).unwrap_or_default()
    }

    /// Forgets every run of `command`; how many there were.
    pub fn forget_command(&self, command: &str) -> usize {
        let Some(db) = &self.db else { return 0 };
        log_err(db.execute("DELETE FROM runs WHERE command = ?1", params![command])).unwrap_or(0)
    }

    /// Forgets every run `query` finds; how many there were.
    pub fn forget(&self, query: &Query<'_>) -> usize {
        let Some(db) = &self.db else { return 0 };
        let (cond, args) = conditions(query);
        log_err(db.execute(&format!("DELETE FROM runs WHERE {cond}"), params_from_iter(args.iter()))).unwrap_or(0)
    }

    /// Forgets everything.
    pub fn clear(&self) {
        if let Some(db) = &self.db {
            log_err(db.execute_batch("DELETE FROM runs; INSERT INTO runs_text(runs_text) VALUES ('rebuild'); VACUUM;"));
        }
    }

    /// How many distinct commands are kept.
    pub fn count(&self) -> usize {
        let Some(db) = &self.db else { return 0 };
        log_err(db.query_row("SELECT COUNT(DISTINCT command) FROM runs", [], |r| r.get::<_, i64>(0))).unwrap_or(0) as usize
    }
}

fn prepare(db: &Connection) -> rusqlite::Result<()> {
    // Two copies of Flow may write at once; one waits for the other.
    db.busy_timeout(std::time::Duration::from_secs(2))?;
    db.pragma_update_and_check(None, "journal_mode", "WAL", |_| Ok(()))?;
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS runs (
            id INTEGER PRIMARY KEY,
            command TEXT NOT NULL,
            cwd TEXT,
            shell TEXT NOT NULL,
            started INTEGER NOT NULL,
            secs REAL,
            exit INTEGER,
            assistant INTEGER NOT NULL DEFAULT 0
        );
        CREATE INDEX IF NOT EXISTS runs_command ON runs(command);
        CREATE VIRTUAL TABLE IF NOT EXISTS runs_text USING fts5(command, content='runs', content_rowid='id', tokenize='trigram');
        CREATE TRIGGER IF NOT EXISTS runs_added AFTER INSERT ON runs BEGIN
            INSERT INTO runs_text(rowid, command) VALUES (new.id, new.command);
        END;
        CREATE TRIGGER IF NOT EXISTS runs_removed AFTER DELETE ON runs BEGIN
            INSERT INTO runs_text(runs_text, rowid, command) VALUES ('delete', old.id, old.command);
        END;
        CREATE TRIGGER IF NOT EXISTS runs_changed AFTER UPDATE OF command ON runs BEGIN
            INSERT INTO runs_text(runs_text, rowid, command) VALUES ('delete', old.id, old.command);
            INSERT INTO runs_text(rowid, command) VALUES (new.id, new.command);
        END;",
    )
}

/// The `WHERE` of the runs `query` finds, with its arguments. Words of three
/// letters or more go to the trigram index; shorter ones, which trigrams
/// can't find, are matched as they are (LIKE, ignoring ASCII case).
fn conditions(query: &Query<'_>) -> (String, Vec<Box<dyn ToSql>>) {
    let mut cond = vec!["1".to_string()];
    let mut args: Vec<Box<dyn ToSql>> = Vec::new();
    let (long, short): (Vec<&str>, Vec<&str>) = query.words.split_whitespace().partition(|w| w.chars().count() >= 3);
    if !long.is_empty() {
        let phrases: Vec<String> = long.iter().map(|w| format!("\"{}\"", w.replace('"', "\"\""))).collect();
        cond.push("id IN (SELECT rowid FROM runs_text WHERE runs_text MATCH ?)".into());
        args.push(Box::new(phrases.join(" AND ")));
    }
    for w in short {
        cond.push("command LIKE ? ESCAPE '\\'".into());
        let escaped = w.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
        args.push(Box::new(format!("%{escaped}%")));
    }
    match query.filter {
        Filter::All => {}
        Filter::Folder => {
            cond.push("cwd = ?".into());
            args.push(Box::new(query.cwd.unwrap_or("").to_string()));
        }
        Filter::Failed => cond.push("exit IS NOT NULL AND exit != 0".into()),
        Filter::Shell => {
            cond.push("shell = ?".into());
            args.push(Box::new(query.shell.to_string()));
        }
    }
    (cond.join(" AND "), args)
}

fn log_err<T>(r: rusqlite::Result<T>) -> Option<T> {
    r.map_err(|err| log::warn!("command history: {err}")).ok()
}

/// Whether a command line goes in the history: not empty, not started with
/// a space (as bash's `ignorespace` leaves a line out), and not one that
/// looks like it holds a secret.
pub fn keep(command: &str) -> bool {
    !command.trim().is_empty() && !command.starts_with(char::is_whitespace) && !looks_secret(command)
}

/// Names that, set to something, mark a secret: `API_KEY=…`, `--password=…`.
const SECRET_NAMES: [&str; 8] = ["key", "token", "secret", "pass", "pwd", "credential", "auth", "cookie"];
/// Flags whose next word is a secret: `--password hunter2`, `-Token abc`.
const SECRET_FLAGS: [&str; 12] = [
    "password", "passwd", "pass", "token", "secret", "apikey", "api-key", "api_key", "accesskey", "secretkey", "auth", "credential",
];
/// The starts of keys and tokens in the forms their services give them.
const SECRET_PREFIXES: [(&str, usize); 11] = [
    ("sk-", 20),
    ("ghp_", 20),
    ("gho_", 20),
    ("ghu_", 20),
    ("ghs_", 20),
    ("github_pat_", 20),
    ("glpat-", 16),
    ("xoxb-", 10),
    ("xoxp-", 10),
    ("AKIA", 16),
    ("AIza", 30),
];

/// A command line that looks like it holds a password, a token or a key:
/// a secret-sounding name set to something, a secret-sounding flag with a
/// value, a key in a service's own form, a header or URL with credentials,
/// or a private key.
pub fn looks_secret(command: &str) -> bool {
    let words: Vec<&str> = command.split_whitespace().collect();
    let secret_name = |name: &str| {
        let name = name.trim_start_matches(['-', '$']).to_ascii_lowercase();
        let name = name.strip_prefix("env:").unwrap_or(&name);
        // `--author=` is a name, not a login.
        SECRET_NAMES.iter().any(|s| name.contains(s) && !(*s == "auth" && name.contains("author")))
    };
    for (i, w) in words.iter().enumerate() {
        let next = words.get(i + 1).copied();
        // NAME=value, --name=value, and PowerShell's $env:NAME = value.
        if let Some((name, value)) = w.split_once('=') {
            if !name.is_empty() && secret_name(name) && !value.trim_matches(['"', '\'']).is_empty() {
                return true;
            }
        }
        if next == Some("=") && secret_name(w) && words.get(i + 2).is_some() {
            return true;
        }
        // A secret flag with the value after it.
        if w.starts_with('-') && next.is_some() {
            let flag = w.trim_start_matches('-').to_ascii_lowercase();
            if SECRET_FLAGS.contains(&flag.as_str()) {
                return true;
            }
        }
        let lower = w.to_ascii_lowercase();
        // setx NAME value (cmd), and curl's -u user:password.
        if lower == "setx" && next.is_some_and(secret_name) && words.get(i + 2).is_some() {
            return true;
        }
        if matches!(lower.as_str(), "-u" | "--user") && next.is_some_and(|n| n.contains(':')) {
            return true;
        }
        // mysql -pPASSWORD.
        if i > 0 && w.len() > 2 && w.starts_with("-p") && matches!(words[0], "mysql" | "mariadb" | "mysqldump" | "mysqladmin") {
            return true;
        }
    }
    let lower = command.to_ascii_lowercase();
    if lower.contains("authorization:") || lower.contains("bearer ") || lower.contains("-----begin") {
        return true;
    }
    // https://user:password@host
    if let Some(at) = command.find("://") {
        let rest = &command[at + 3..];
        let host_end = rest.find(['/', ' ', '"', '\'']).unwrap_or(rest.len());
        if rest[..host_end].split_once('@').is_some_and(|(cred, _)| cred.contains(':')) {
            return true;
        }
    }
    // A key in the form its service gives it, standing on its own.
    command.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-')).any(|w| {
        SECRET_PREFIXES.iter().any(|(prefix, min)| {
            w.strip_prefix(prefix).is_some_and(|rest| rest.len() >= *min && rest.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'))
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run<'a>(command: &'a str, cwd: &'a str, shell: &'a str, started: i64) -> Run<'a> {
        Run { command, cwd: Some(cwd), shell, started, assistant: false }
    }

    fn all(words: &str) -> Query<'_> {
        Query { words, filter: Filter::All, cwd: None, shell: "zsh" }
    }

    fn commands(found: &[Found]) -> Vec<&str> {
        found.iter().map(|f| f.command.as_str()).collect()
    }

    #[test]
    fn a_search_finds_commands_by_any_words_in_them_latest_first() {
        let h = History::in_memory();
        h.record(&run("cargo build --release", "/work", "zsh", 1));
        h.record(&run("git commit -m 'fix the build'", "/work", "zsh", 2));
        h.record(&run("ls -la", "/tmp", "bash", 3));
        assert_eq!(commands(&h.search(&all("build"))), ["git commit -m 'fix the build'", "cargo build --release"]);
        assert_eq!(commands(&h.search(&all("BUILD rel"))), ["cargo build --release"], "every word, any case, in any order");
        assert_eq!(commands(&h.search(&all("uil"))), ["git commit -m 'fix the build'", "cargo build --release"], "inside words");
        assert_eq!(commands(&h.search(&all("-l"))), ["ls -la"], "words too short for trigrams");
        assert_eq!(h.search(&all("")).len(), 3, "no words: everything");
        assert!(h.search(&all("nothing like it")).is_empty());
    }

    #[test]
    fn a_command_run_again_shows_once_at_its_latest_run() {
        let h = History::in_memory();
        let first = h.record(&run("make test", "/a", "zsh", 1)).unwrap();
        h.finish(first, Some(2), 1.5);
        h.record(&run("make", "/a", "zsh", 2));
        let again = h.record(&run("make test", "/b", "zsh", 3)).unwrap();
        h.finish(again, Some(0), 2.0);
        let found = h.search(&all("make test"));
        assert_eq!(found.len(), 1);
        assert_eq!((found[0].cwd.as_deref(), found[0].exit, found[0].runs), (Some("/b"), Some(0), 2));
        assert_eq!(commands(&h.search(&all("make"))), ["make test", "make"]);
        assert_eq!(h.count(), 2);
    }

    #[test]
    fn filters_keep_to_the_folder_the_failures_or_the_shell() {
        let h = History::in_memory();
        let a = h.record(&run("npm test", "/web", "zsh", 1)).unwrap();
        h.finish(a, Some(1), 3.0);
        let b = h.record(&run("npm start", "/web", "bash", 2)).unwrap();
        h.finish(b, Some(0), 1.0);
        h.record(&run("dir", "C:\\", "cmd", 3));
        let q = |filter| Query { words: "", filter, cwd: Some("/web"), shell: "bash" };
        assert_eq!(commands(&h.search(&q(Filter::Folder))), ["npm start", "npm test"]);
        assert_eq!(commands(&h.search(&q(Filter::Failed))), ["npm test"], "cmd's unknown exit isn't a failure");
        assert_eq!(commands(&h.search(&q(Filter::Shell))), ["npm start"]);
    }

    #[test]
    fn the_shells_word_replaces_the_grids_and_the_index_follows() {
        let h = History::in_memory();
        let id = h.record(&run("git stat  us --short   # suggestion", "/", "zsh", 1)).unwrap();
        h.set_command(id, "git status");
        assert_eq!(commands(&h.search(&all("status"))), ["git status"]);
        assert!(h.search(&all("suggestion")).is_empty());
    }

    #[test]
    fn forgetting_takes_a_command_or_all_a_search_finds() {
        let h = History::in_memory();
        h.record(&run("echo one", "/a", "zsh", 1));
        h.record(&run("echo one", "/b", "zsh", 2));
        h.record(&run("echo two", "/a", "zsh", 3));
        h.record(&run("pwd", "/a", "zsh", 4));
        assert_eq!(h.forget_command("echo one"), 2);
        assert_eq!(commands(&h.search(&all("echo"))), ["echo two"]);
        let folder = Query { words: "", filter: Filter::Folder, cwd: Some("/a"), shell: "zsh" };
        assert_eq!(h.forget(&folder), 2);
        assert_eq!(h.count(), 0);
        h.record(&run("cd", "/", "zsh", 5));
        h.clear();
        assert_eq!(h.count(), 0);
        assert!(h.search(&all("cd")).is_empty());
    }

    #[test]
    fn japanese_and_quotes_are_found_as_typed() {
        let h = History::in_memory();
        h.record(&run("echo \"こんにちは世界\"", "/", "zsh", 1));
        assert_eq!(h.search(&all("にちは")).len(), 1);
        assert_eq!(h.search(&all("\"こんにちは")).len(), 1, "a quote in the words is matched, not parsed");
        assert_eq!(h.search(&all("100%")).len(), 0);
    }

    #[test]
    fn a_history_that_cant_open_keeps_nothing_and_finds_nothing() {
        let dir = std::env::temp_dir().join(format!("flow-history-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // A folder where the file should be.
        let h = History::open(&dir);
        assert_eq!(h.record(&run("ls", "/", "zsh", 1)), None);
        assert!(h.search(&all("")).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn the_file_is_its_owners_alone() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("flow-history-mode-{}", std::process::id()));
        let path = dir.join("history.sqlite3");
        let h = History::open(&path);
        assert!(h.record(&run("ls", "/", "zsh", 1)).is_some());
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        drop(h);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn lines_with_secrets_or_a_leading_space_are_not_kept() {
        for secret in [
            " ls",
            "export OPENAI_API_KEY=sk-abc",
            "ANTHROPIC_API_KEY='x' flow",
            "set GITHUB_TOKEN=abc",
            "setx API_KEY 12345",
            "$env:GEMINI_API_KEY = \"abc\"",
            "$env:GEMINI_API_KEY=\"abc\"",
            "mytool --password hunter2",
            "mytool --password=hunter2",
            "Connect-Thing -Token abc123",
            "curl -H 'Authorization: Bearer abc' https://api.example.com",
            "curl -u alex:hunter2 https://example.com",
            "git clone https://me:hunter2@example.com/repo.git",
            "mysql -u root -phunter2",
            "echo sk-ant-api03-abcdefghijklmnopqrstuvwxyz",
            "gh auth login --with-token ghp_abcdefghijklmnopqrstuvwxyz0123",
            "aws configure set aws_access_key_id AKIAABCDEFGHIJKLMNOP",
            "echo '-----BEGIN OPENSSH PRIVATE KEY-----'",
        ] {
            assert!(!keep(secret), "{secret}");
        }
        for fine in [
            "ls -la",
            "git commit -m 'skip the passing tests'",
            "cargo build --release",
            "ssh-keygen -t ed25519",
            "export PATH=$PATH:/opt/bin",
            "cd ~/keys",
            "mkdir -p build",
            "git clone https://example.com/repo.git",
            "echo sk-short",
            "git log --author=alex",
            "KEY=",
        ] {
            assert!(keep(fine), "{fine}");
        }
        assert!(!keep(""));
        assert!(!keep("   "));
    }
}
