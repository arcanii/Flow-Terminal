//! A server Flow starts as a program, speaking MCP's JSON-RPC a line at a
//! time over its standard input and output. A reader thread hands each
//! answer to the request waiting for it by id (several may wait at once),
//! and what the server sends unasked (its requests and notifications) to
//! the link's thread; its standard error's last lines say why, where it
//! fails.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio as Piped};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::Value;

use super::{request_message, result_of, route_unasked, Fail, Incoming, Transport, GONE};

/// How many of its standard error's last lines are kept.
const ERR_LINES: usize = 12;
/// How often a request waiting for its answer looks whether it's given up.
const LOOK_EVERY: Duration = Duration::from_millis(100);

type Waiting = Arc<Mutex<HashMap<u64, crossbeam_channel::Sender<Value>>>>;

pub struct Stdio {
    child: Mutex<Child>,
    to: Mutex<ChildStdin>,
    waiting: Waiting,
    errors: Arc<Mutex<VecDeque<String>>>,
}

impl Stdio {
    /// Starts `command` with `args`, its environment Flow's with `env` over
    /// it (each value's `${env:…}` expanded) and `path` first on its PATH;
    /// what it sends unasked goes to `incoming`.
    pub fn start(command: &str, args: &[String], env: &BTreeMap<String, String>, path: &[PathBuf], incoming: crossbeam_channel::Sender<Incoming>) -> Result<Stdio, String> {
        let mut paths: Vec<PathBuf> = path.to_vec();
        paths.extend(std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect::<Vec<_>>()).unwrap_or_default());
        // Windows starts only a program named whole, so `npx` is found as cmd
        // would find it (its `npx.cmd`), and started as itself: through `cmd
        // /C`, a path with spaces given arguments too lost its quotes. A
        // batch file's arguments the standard library escapes for cmd. What
        // cmd did besides, `%USERPROFILE%` and the like in the command and its
        // arguments, Flow does.
        let (program, args): (PathBuf, Vec<String>) = if cfg!(windows) {
            let var = |name: &str| std::env::var(name).ok();
            let command = expand_percent(command, var);
            let program = find_program(&command, &paths, &windows_extensions()).unwrap_or_else(|| PathBuf::from(&command));
            (program, args.iter().map(|a| expand_percent(a, var)).collect())
        } else {
            (PathBuf::from(command), args.to_vec())
        };
        let mut cmd = Command::new(program);
        cmd.args(&args);
        if let Ok(joined) = std::env::join_paths(paths) {
            cmd.env("PATH", joined);
        }
        for (key, value) in env {
            cmd.env(key, super::servers::expand(value));
        }
        cmd.stdin(Piped::piped()).stdout(Piped::piped()).stderr(Piped::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = cmd.spawn().map_err(|e| format!("{command} didn't start: {e}"))?;
        let (stdin, stdout, stderr) = (child.stdin.take(), child.stdout.take(), child.stderr.take());
        let (Some(stdin), Some(stdout), Some(stderr)) = (stdin, stdout, stderr) else { return Err("its pipes didn't open".into()) };
        let errors = Arc::new(Mutex::new(VecDeque::new()));
        let kept = errors.clone();
        let _ = std::thread::Builder::new().name("flow-mcp-client-err".into()).spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                let mut lines = kept.lock().unwrap_or_else(|e| e.into_inner());
                lines.push_back(line);
                if lines.len() > ERR_LINES {
                    lines.pop_front();
                }
            }
        });
        let waiting: Waiting = Arc::new(Mutex::new(HashMap::new()));
        let (answers, said) = (waiting.clone(), errors.clone());
        let _ = std::thread::Builder::new().name("flow-mcp-client-out".into()).spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                let Ok(message) = serde_json::from_str::<Value>(&line) else { continue };
                if !route_unasked(&message, &incoming) {
                    deliver(&answers, message);
                }
            }
            // It's gone: every request still waiting hears so, and the
            // link's thread too, once its standard error has had a moment.
            answers.lock().unwrap_or_else(|e| e.into_inner()).clear();
            std::thread::sleep(Duration::from_millis(100));
            let _ = incoming.send(Incoming::Gone(gone_text(&said)));
        });
        Ok(Stdio { child: Mutex::new(child), to: Mutex::new(stdin), waiting, errors })
    }

    fn write(&self, message: &Value) -> Result<(), Fail> {
        let mut to = self.to.lock().unwrap_or_else(|e| e.into_inner());
        writeln!(to, "{message}").and_then(|_| to.flush()).map_err(|_| Fail::Gone(gone_text(&self.errors)))
    }
}

/// Answer `message` to the request waiting for it, where one still does.
fn deliver(waiting: &Waiting, message: Value) {
    let Some(id) = message.get("id").and_then(Value::as_u64) else { return };
    if let Some(waiter) = waiting.lock().unwrap_or_else(|e| e.into_inner()).remove(&id) {
        let _ = waiter.send(message);
    }
}

/// Why it's gone: its standard error's last lines, where it said any.
fn gone_text(errors: &Mutex<VecDeque<String>>) -> String {
    let lines: Vec<String> = errors.lock().unwrap_or_else(|e| e.into_inner()).iter().cloned().collect();
    if lines.is_empty() {
        GONE.to_string()
    } else {
        format!("{GONE}. It said:\n{}", lines.join("\n"))
    }
}

impl Transport for Stdio {
    fn request(&self, id: u64, method: &str, params: Value, wait: Duration, cancel: &AtomicBool) -> Result<Value, Fail> {
        let (waiter, answer) = crossbeam_channel::bounded(1);
        self.waiting.lock().unwrap_or_else(|e| e.into_inner()).insert(id, waiter);
        let forget = || {
            self.waiting.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
        };
        if let Err(fail) = self.write(&request_message(id, method, params)) {
            forget();
            return Err(fail);
        }
        let until = Instant::now() + wait;
        loop {
            if cancel.load(Ordering::Relaxed) {
                forget();
                return Err(Fail::Stopped);
            }
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                forget();
                return Err(Fail::Said(format!("It didn't answer {method} within {} s.", wait.as_secs())));
            }
            match answer.recv_timeout(left.min(LOOK_EVERY)) {
                Ok(message) => return result_of(&message),
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                    // Its standard error may still be coming.
                    std::thread::sleep(Duration::from_millis(100));
                    return Err(Fail::Gone(gone_text(&self.errors)));
                }
            }
        }
    }

    fn send(&self, message: &Value) -> Result<(), Fail> {
        self.write(message)
    }
}

impl Drop for Stdio {
    fn drop(&mut self) {
        let child = self.child.get_mut().unwrap_or_else(|e| e.into_inner());
        let _ = child.kill();
        let _ = child.wait();
    }
}

/// The extensions of the programs Windows starts, as `PATHEXT` orders them:
/// those the system starts itself, and batch files, which the standard
/// library hands to cmd.
fn windows_extensions() -> Vec<String> {
    let listed = std::env::var("PATHEXT").unwrap_or_default();
    let mut exts: Vec<String> = listed.split(';').map(|e| e.trim().to_ascii_lowercase()).filter(|e| matches!(e.as_str(), ".com" | ".exe" | ".bat" | ".cmd")).collect();
    if exts.is_empty() {
        exts = [".com", ".exe", ".bat", ".cmd"].map(String::from).to_vec();
    }
    exts
}

/// `text` with each `%NAME%` in it what `var` gives for `NAME`, as cmd
/// expands it; one `var` gives nothing for stays as written, as in cmd.
fn expand_percent(text: &str, var: impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%').map(|end| (&after[..end], end)) {
            Some((name, end)) if !name.is_empty() => match var(name) {
                Some(value) => {
                    out.push_str(&value);
                    rest = &after[end + 1..];
                }
                // Not a variable: its first % stays, and the second may
                // begin one.
                None => {
                    out.push('%');
                    rest = after;
                }
            },
            _ => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Where `command` is, as cmd would find it: a path names its own file,
/// a bare name the first in `paths`' folders, each with one of `exts` added
/// unless it ends with one already.
fn find_program(command: &str, paths: &[PathBuf], exts: &[String]) -> Option<PathBuf> {
    let named = std::path::Path::new(command);
    let has_ext = named.extension().is_some_and(|e| exts.iter().any(|x| x.trim_start_matches('.').eq_ignore_ascii_case(&e.to_string_lossy())));
    let names: Vec<String> = if has_ext { vec![command.to_string()] } else { exts.iter().map(|e| format!("{command}{e}")).collect() };
    let in_place = named.is_absolute() || command.contains(['/', '\\']);
    let dirs: Vec<PathBuf> = if in_place { vec![PathBuf::new()] } else { paths.to_vec() };
    dirs.iter().flat_map(|dir| names.iter().map(move |name| dir.join(name))).find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_variables_are_expanded_as_cmd_expands_them() {
        let var = |name: &str| match name {
            "USERPROFILE" => Some(r"C:\Users\me".to_string()),
            "A" => Some("1".to_string()),
            _ => None,
        };
        assert_eq!(expand_percent(r"%USERPROFILE%\Flow\games\flow-mcp.exe", var), r"C:\Users\me\Flow\games\flow-mcp.exe");
        assert_eq!(expand_percent("%A%%A%-%A%", var), "11-1");
        // Unset, empty or unclosed, it stays as written, as in cmd.
        assert_eq!(expand_percent("50% off %NOPE% and %", var), "50% off %NOPE% and %");
        assert_eq!(expand_percent("%NOPE%A%", var), "%NOPE1", "the second % began one that is set");
        assert_eq!(expand_percent("%%", var), "%%");
        assert_eq!(expand_percent("plain", var), "plain");
    }

    #[test]
    fn a_program_is_found_as_cmd_finds_it_and_named_whole() {
        let dir = std::env::temp_dir().join(format!("flow find program {}", std::process::id()));
        let (bin, other) = (dir.join("bin"), dir.join("other bin"));
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::create_dir_all(&other).unwrap();
        for file in [bin.join("npx.cmd"), other.join("npx.exe"), other.join("server.exe"), other.join("tool.py")] {
            std::fs::write(file, b"").unwrap();
        }
        let exts = [".com", ".exe", ".bat", ".cmd"].map(String::from).to_vec();
        let paths = [bin.clone(), other.clone()];
        // The first folder that has it, with whatever extension it has there.
        assert_eq!(find_program("npx", &paths, &exts), Some(bin.join("npx.cmd")));
        assert_eq!(find_program("server", &paths, &exts), Some(other.join("server.exe")));
        assert_eq!(find_program("server.exe", &paths, &exts), Some(other.join("server.exe")));
        // A path, spaces and all, is its own file, not looked for elsewhere.
        let named = other.join("server.exe").display().to_string();
        assert_eq!(find_program(&named, &[], &exts), Some(other.join("server.exe")));
        assert_eq!(find_program(&other.join("server").display().to_string(), &[], &exts), Some(other.join("server.exe")));
        // Nothing Windows would start: left as given.
        assert_eq!(find_program("tool", &paths, &exts), None);
        assert_eq!(find_program("nowhere", &paths, &exts), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
