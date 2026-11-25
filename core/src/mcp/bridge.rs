//! The bridge between `flow-mcp` and the running Flow: a socket only the
//! user can open, `<config_dir>/mcp/flow.sock` (a named pipe on Windows,
//! `\\.\pipe\flow-mcp-<user>`, which the Windows crate serves), carrying a
//! JSON a line: [`ToFlow`] one way, [`FromFlow`] the other.
//!
//! Flow listens only while Settings' Outside apps is on. Each connection is
//! served on a thread of its own ([`serve`]), which hands every call to
//! Flow's window as an [`Event`] and writes back its answer, however long
//! the user takes to say yes. Who's at the other end ([`Peer`]) comes from
//! the system, not from what `flow-mcp` says: its process, and the app that
//! started it. Each end first makes sure the other is the same user, as the
//! system says (`peer_user`): Flow serves only the user's own `flow-mcp`
//! (`listen_unix`), and `flow-mcp` talks only to the user's own Flow
//! (`connect_unix`), since `FLOW_MCP_SOCKET`, or the folder in /tmp that a
//! deep config folder has the socket in, may be where another user can
//! listen first. Windows' pipe does the same its own way (the Windows
//! crate's `mcp.rs`).

use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::ClientInfo;

/// Where `flow-mcp` finds Flow: `FLOW_MCP_SOCKET` where it's set (tests, a
/// Flow of one's own), else the socket in Flow's config folder, or on
/// Windows the user's pipe.
pub fn address() -> PathBuf {
    if let Some(set) = std::env::var_os("FLOW_MCP_SOCKET").filter(|s| !s.is_empty()) {
        return PathBuf::from(set);
    }
    let user: String = std::env::var(if cfg!(windows) { "USERNAME" } else { "USER" }).unwrap_or_default().chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')).collect();
    if cfg!(windows) {
        return PathBuf::from(format!(r"\\.\pipe\flow-mcp-{user}"));
    }
    socket_in(&crate::config::config_dir(), &user)
}

/// The most bytes a socket's path may have (the Mac's is 104 with its end).
const SOCKET_PATH: usize = 100;

/// The socket in config folder `config`, or, where that path is too long
/// for a socket, `/tmp/flow-mcp-<user>/flow.sock`, whose folder is made the
/// user's alone, or refused where someone else has it already.
fn socket_in(config: &std::path::Path, user: &str) -> PathBuf {
    let socket = config.join("mcp").join("flow.sock");
    if socket.as_os_str().len() < SOCKET_PATH {
        socket
    } else {
        PathBuf::from(format!("/tmp/flow-mcp-{user}/flow.sock"))
    }
}

/// What `flow-mcp` sends Flow.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToFlow {
    /// First, what the MCP client said it is.
    Hello { client: ClientInfo },
    /// A tool to call, numbered for its answer.
    Call { id: u64, tool: String, args: Value },
}

/// Flow's answer to call `id`: what it says, or why it couldn't.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FromFlow {
    pub id: u64,
    pub ok: bool,
    pub text: String,
}

/// Writes `frame` as one line.
pub fn write_frame<T: Serialize>(to: &mut impl Write, frame: &T) -> io::Result<()> {
    let mut line = serde_json::to_vec(frame).map_err(io::Error::other)?;
    line.push(b'\n');
    to.write_all(&line)?;
    to.flush()
}

/// The next frame, none once the other end has gone; a line that isn't
/// one is an error.
pub fn read_frame<T: for<'de> Deserialize<'de>>(from: &mut impl BufRead) -> io::Result<Option<T>> {
    let mut line = String::new();
    // A line far past any call's is refused rather than read without end.
    if from.take(MOST_LINE as u64).read_line(&mut line)? == 0 {
        return Ok(None);
    }
    serde_json::from_str(line.trim_end()).map(Some).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// The longest frame read, in bytes.
const MOST_LINE: usize = 4 << 20;

/// The process at the other end of a connection, and the app that started
/// it (`flow-mcp`'s parent), as the system says.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Peer {
    pub pid: Option<u32>,
    pub host: Option<Host>,
}

/// The app that started `flow-mcp`: its process and its program's path.
#[derive(Debug, Clone, PartialEq)]
pub struct Host {
    pub pid: u32,
    pub path: PathBuf,
}

/// What `flow-mcp` finds where its own check of the other end says that
/// what answers at Flow's address isn't the user's own Flow (`connect_unix`;
/// on Windows, `served_by_flow`), as a `PermissionDenied` (`error`): told
/// apart (`is`) from the system's own refusal to let it in, which says less
/// of who's there, since a sandbox the app runs it in gives that too.
#[derive(Debug)]
pub struct NotUsersFlow;

impl std::fmt::Display for NotUsersFlow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("what answers isn't the user's own Flow")
    }
}

impl std::error::Error for NotUsersFlow {}

impl NotUsersFlow {
    /// The refusal, as an `io::Error` of kind `PermissionDenied`.
    pub fn error() -> io::Error {
        io::Error::new(io::ErrorKind::PermissionDenied, NotUsersFlow)
    }

    /// Whether `e` is the refusal, not some other error of its kind.
    pub fn is(e: &io::Error) -> bool {
        e.get_ref().is_some_and(|inner| inner.is::<NotUsersFlow>())
    }
}

/// The [`Peer`] whose process is `pid`, by `info`, which gives a process's
/// parent and program's path as the system says: the app that started it is
/// its parent.
pub fn peer_from(pid: Option<u32>, info: impl Fn(u32) -> Option<(u32, PathBuf)>) -> Peer {
    let host = pid.and_then(&info).and_then(|(parent, _)| info(parent).map(|(_, path)| Host { pid: parent, path }));
    Peer { pid, host }
}

/// Linux's [`Peer`] of a connection: its process from `SO_PEERCRED`, each
/// process's parent from `/proc/<pid>/stat` and program from
/// `/proc/<pid>/exe`.
#[cfg(all(unix, not(target_os = "macos")))]
pub fn linux_peer(stream: &std::os::unix::net::UnixStream) -> Peer {
    let pid = linux_cred(stream).filter(|cred| cred.pid > 0).map(|cred| cred.pid as u32);
    peer_from(pid, |pid| linux_process(std::path::Path::new("/proc"), pid))
}

/// What Linux says of the other end of a connection (`SO_PEERCRED`): its
/// process and effective user when it connected, or, for the end that
/// connected, when the other began to listen.
#[cfg(all(unix, not(target_os = "macos")))]
fn linux_cred(stream: &std::os::unix::net::UnixStream) -> Option<libc::ucred> {
    use std::os::fd::AsRawFd;
    let mut cred = libc::ucred { pid: 0, uid: 0, gid: 0 };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: fills one ucred, whose size is passed.
    let got = unsafe { libc::getsockopt(stream.as_raw_fd(), libc::SOL_SOCKET, libc::SO_PEERCRED, (&mut cred as *mut libc::ucred).cast(), &mut len) };
    (got == 0).then_some(cred)
}

/// The effective user at the other end of a connection, as the system says
/// (the Mac's `getpeereid`, Linux's `SO_PEERCRED`): for the end that
/// listens, who connected; for the end that connected, who listens. None
/// where the system says nothing.
#[cfg(unix)]
pub fn peer_user(stream: &std::os::unix::net::UnixStream) -> Option<u32> {
    #[cfg(target_os = "macos")]
    {
        use std::os::fd::AsRawFd;
        let (mut uid, mut gid): (libc::uid_t, libc::gid_t) = (0, 0);
        // SAFETY: fills two integers.
        let got = unsafe { libc::getpeereid(stream.as_raw_fd(), &mut uid, &mut gid) };
        (got == 0).then_some(uid)
    }
    #[cfg(not(target_os = "macos"))]
    {
        linux_cred(stream).map(|cred| cred.uid)
    }
}

/// This process's effective user, whom the other end must be.
#[cfg(unix)]
fn this_user() -> u32 {
    // SAFETY: geteuid only reads.
    unsafe { libc::geteuid() }
}

/// Whether the other end of `stream` is user `me`: never where the system
/// can't say.
#[cfg(unix)]
fn of_user(stream: &std::os::unix::net::UnixStream, me: u32) -> bool {
    peer_user(stream) == Some(me)
}

/// A Linux process's parent and program, read under `proc` (`/proc`).
#[cfg(any(test, all(unix, not(target_os = "macos"))))]
pub fn linux_process(proc: &std::path::Path, pid: u32) -> Option<(u32, PathBuf)> {
    let stat = std::fs::read_to_string(proc.join(pid.to_string()).join("stat")).ok()?;
    // After the name in brackets, which may hold anything: state, then ppid.
    let rest = &stat[stat.rfind(')')? + 1..];
    let parent = rest.split_whitespace().nth(1)?.parse().ok()?;
    let path = std::fs::read_link(proc.join(pid.to_string()).join("exe")).unwrap_or_default();
    Some((parent, path))
}

/// A connection: a socket, or a pipe on Windows.
pub trait Duplex: Read + Write + Send {
    /// Another handle on it, for whoever may end it.
    fn try_clone_box(&self) -> io::Result<Box<dyn Duplex>>;
    /// Ends it, so a thread reading it stops.
    fn shutdown(&self);
    /// Has reads wait for something to read (`wait`, as they do at first) or
    /// come back at once; false where it can't be changed.
    fn set_waiting(&self, _wait: bool) -> bool {
        false
    }
}

/// Whether the other end of `from` has gone, looked at without waiting:
/// what's read meanwhile stays in `from`'s buffer for the next frame. False
/// where the connection can't be read without waiting (Windows' pipe).
fn gone(from: &mut BufReader<Box<dyn Duplex>>) -> bool {
    if !from.get_ref().set_waiting(false) {
        return false;
    }
    let gone = match from.fill_buf() {
        Ok(read) => read.is_empty(),
        Err(e) => !matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted),
    };
    from.get_ref().set_waiting(true);
    gone
}

#[cfg(unix)]
impl Duplex for std::os::unix::net::UnixStream {
    fn try_clone_box(&self) -> io::Result<Box<dyn Duplex>> {
        Ok(Box::new(self.try_clone()?))
    }
    fn shutdown(&self) {
        let _ = std::os::unix::net::UnixStream::shutdown(self, std::net::Shutdown::Both);
    }
    fn set_waiting(&self, wait: bool) -> bool {
        // Its clones share it, but none reads or writes meanwhile: `serve`
        // looks only while a call waits for Flow's answer.
        self.set_nonblocking(!wait).is_ok()
    }
}

/// A call for Flow's window to answer, with where its answer goes.
pub struct Request {
    /// The connection it came on.
    pub conn: u64,
    pub client: ClientInfo,
    pub peer: Peer,
    pub tool: String,
    pub args: Value,
    pub reply: crossbeam_channel::Sender<Result<String, String>>,
}

/// What a connection tells Flow's window.
pub enum Event {
    Call(Request),
    /// The connection has ended: its app's waiting cards go.
    Gone(u64),
}

impl std::fmt::Debug for Event {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Event::Call(r) => write!(f, "Call({} from {} on {})", r.tool, r.client.name, r.conn),
            Event::Gone(conn) => write!(f, "Gone({conn})"),
        }
    }
}

/// What hands an [`Event`] to Flow's window.
pub type Handler = Arc<dyn Fn(Event) + Send + Sync>;

/// How often a call waiting for Flow's answer (a card up for the user)
/// looks whether its app has gone.
const LOOK_EVERY: std::time::Duration = std::time::Duration::from_millis(250);

/// Serves connection `conn` until its other end goes: each call handed on
/// with `handler` and answered when Flow has (or "Flow closed" where it
/// went first), one at a time, as `flow-mcp` sends them. An app that goes
/// while its call waits, the user not yet having answered its card, is
/// heard within [`LOOK_EVERY`], so its cards go then rather than when
/// answered.
pub fn serve(conn: u64, stream: Box<dyn Duplex>, peer: Peer, handler: Handler) {
    let Ok(mut out) = stream.try_clone_box() else { return };
    let mut from = BufReader::new(stream);
    let mut client = ClientInfo::default();
    loop {
        let frame = match read_frame::<ToFlow>(&mut from) {
            Ok(Some(frame)) => frame,
            Ok(None) => break,
            Err(e) => {
                log::warn!("outside app's connection {conn}: {e}");
                break;
            }
        };
        match frame {
            ToFlow::Hello { client: said } => client = said,
            ToFlow::Call { id, tool, args } => {
                let (reply, answer) = crossbeam_channel::bounded(1);
                handler(Event::Call(Request { conn, client: client.clone(), peer: peer.clone(), tool, args, reply }));
                let said = loop {
                    match answer.recv_timeout(LOOK_EVERY) {
                        Ok(said) => break Some(said),
                        Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break Some(Err("Flow closed before it answered.".into())),
                        Err(crossbeam_channel::RecvTimeoutError::Timeout) if gone(&mut from) => break None,
                        Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
                    }
                };
                // Its app went while it waited: Flow hears so below.
                let Some(said) = said else { break };
                let (ok, text) = match said {
                    Ok(text) => (true, text),
                    Err(text) => (false, text),
                };
                if write_frame(&mut out, &FromFlow { id, ok, text }).is_err() {
                    break;
                }
            }
        }
    }
    handler(Event::Gone(conn));
}

/// Flow listening for outside apps: dropped, it stops, ending the
/// connections and taking its socket away.
pub trait Listening: Send {}

/// The connections being served, for ending them all.
#[derive(Clone, Default)]
pub struct Conns(Arc<std::sync::Mutex<Vec<Box<dyn Duplex>>>>);

impl Conns {
    pub fn add(&self, conn: &dyn Duplex) {
        if let Ok(handle) = conn.try_clone_box() {
            self.0.lock().unwrap_or_else(|e| e.into_inner()).push(handle);
        }
    }

    pub fn end_all(&self) {
        for conn in self.0.lock().unwrap_or_else(|e| e.into_inner()).drain(..) {
            conn.shutdown();
        }
    }
}

/// The next connection's number.
pub fn next_conn() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

#[cfg(unix)]
pub use unix::{connect_unix, listen_unix, UnixListening};

#[cfg(unix)]
mod unix {
    use super::*;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::Path;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// Flow's socket, served on a `flow-mcp` thread.
    pub struct UnixListening {
        path: PathBuf,
        stop: Arc<AtomicBool>,
        conns: Conns,
    }

    impl Listening for UnixListening {}

    impl Drop for UnixListening {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::SeqCst);
            // Wakes the thread waiting in accept, which then sees it's to stop.
            let _ = UnixStream::connect(&self.path);
            self.conns.end_all();
            let _ = std::fs::remove_file(&self.path);
        }
    }

    /// Listens at `path` for `flow-mcp`, each connection served on a thread
    /// of its own, its peer worked out by `peer_of` (the system's word on
    /// who it is). A socket left by a Flow that's gone is taken away first;
    /// one that answers is another Flow's, and this one serves nothing. Its
    /// folder, which must be a folder of the user's own (not a link), is made
    /// theirs alone, and so is the socket; a connection from
    /// another user all the same (to a socket `FLOW_MCP_SOCKET` puts where
    /// others can reach it) is closed unread.
    pub fn listen_unix(path: &Path, peer_of: fn(&UnixStream) -> Peer, handler: Handler) -> Result<UnixListening, String> {
        listen_as(path, peer_of, handler, this_user())
    }

    /// `listen_unix`, serving only connections from user `me`.
    pub(super) fn listen_as(path: &Path, peer_of: fn(&UnixStream) -> Peer, handler: Handler, me: u32) -> Result<UnixListening, String> {
        let folder = path.parent().ok_or("no folder for the socket")?;
        std::fs::create_dir_all(folder).map_err(|e| format!("{}: {e}", folder.display()))?;
        // The folder is made the user's alone, and an old socket in it taken
        // away, so it must be a folder of theirs: not a link another user
        // left in its place (in /tmp, which a deep config folder's socket
        // goes in), which would have a folder of the user's elsewhere made
        // theirs alone, nor another's folder.
        let made = std::fs::symlink_metadata(folder).map_err(|e| format!("{}: {e}", folder.display()))?;
        if !made.is_dir() || made.uid() != this_user() {
            return Err(crate::i18n::tf("outside.folder_not_own", &[("folder", &folder.display().to_string())]));
        }
        std::fs::set_permissions(folder, std::fs::Permissions::from_mode(0o700)).map_err(|e| format!("{}: {e}", folder.display()))?;
        if path.exists() {
            if UnixStream::connect(path).is_ok() {
                return Err("another Flow already lets outside apps connect".into());
            }
            std::fs::remove_file(path).map_err(|e| format!("{}: {e}", path.display()))?;
        }
        let listener = UnixListener::bind(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
        let stop = Arc::new(AtomicBool::new(false));
        let conns = Conns::default();
        let (stopping, kept) = (stop.clone(), conns.clone());
        std::thread::Builder::new()
            .name("flow-mcp".into())
            .spawn(move || {
                for stream in listener.incoming() {
                    if stopping.load(Ordering::SeqCst) {
                        break;
                    }
                    let Ok(stream) = stream else { continue };
                    if !of_user(&stream, me) {
                        log::warn!("outside apps: turned away a connection from another user ({:?})", peer_user(&stream));
                        continue;
                    }
                    let peer = peer_of(&stream);
                    kept.add(&stream);
                    let (handler, conn) = (handler.clone(), next_conn());
                    let _ = std::thread::Builder::new().name("flow-mcp-conn".into()).spawn(move || serve(conn, Box::new(stream), peer, handler));
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(UnixListening { path: path.to_path_buf(), stop, conns })
    }

    /// Connects to Flow's socket at `path`, as `flow-mcp` does: one that
    /// another user listens at is refused ([`NotUsersFlow`]) before a word
    /// is sent, since it would hear every call and could answer anything.
    pub fn connect_unix(path: &Path) -> io::Result<UnixStream> {
        connect_as(path, this_user())
    }

    /// `connect_unix`, taking only a socket user `me` listens at.
    pub(super) fn connect_as(path: &Path, me: u32) -> io::Result<UnixStream> {
        let stream = UnixStream::connect(path)?;
        if !of_user(&stream, me) {
            log::warn!("outside apps: {} is listened at by another user ({:?})", path.display(), peer_user(&stream));
            return Err(NotUsersFlow::error());
        }
        Ok(stream)
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::net::UnixStream;

    fn scratch(name: &str) -> PathBuf {
        // Sockets' paths are short, so in /tmp rather than the temp folder.
        let dir = PathBuf::from(format!("/tmp/flow-mcp-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_config_folder_too_deep_for_a_socket_has_it_in_tmp() {
        let config = PathBuf::from("/Users/someone/Library/Application Support/flow-terminal");
        assert_eq!(socket_in(&config, "someone"), config.join("mcp/flow.sock"));
        let deep = PathBuf::from(format!("/private/tmp/{}/home/Library/Application Support/flow-terminal", "x".repeat(60)));
        assert_eq!(socket_in(&deep, "someone"), PathBuf::from("/tmp/flow-mcp-someone/flow.sock"));
    }

    #[test]
    fn the_app_that_started_flow_mcp_is_its_parent() {
        let info = |pid: u32| match pid {
            300 => Some((200, PathBuf::from("/usr/local/bin/flow-mcp"))),
            200 => Some((1, PathBuf::from("/Applications/Claude.app/Contents/MacOS/Claude"))),
            _ => None,
        };
        assert_eq!(peer_from(Some(300), info), Peer { pid: Some(300), host: Some(Host { pid: 200, path: PathBuf::from("/Applications/Claude.app/Contents/MacOS/Claude") }) });
        assert_eq!(peer_from(Some(999), info), Peer { pid: Some(999), host: None }, "a process gone says nothing");
        assert_eq!(peer_from(None, info), Peer::default());
        // Linux's /proc, a name with brackets and spaces in it.
        let proc = scratch("proc");
        std::fs::create_dir_all(proc.join("300")).unwrap();
        std::fs::write(proc.join("300/stat"), "300 (flow mcp (x)) S 200 300 300 0 -1 4194304 1 0 0 0 0 0 0 0 20 0 1 0 1 1 1\n").unwrap();
        std::os::unix::fs::symlink("/usr/bin/flow-mcp", proc.join("300/exe")).unwrap();
        assert_eq!(linux_process(&proc, 300), Some((200, PathBuf::from("/usr/bin/flow-mcp"))));
        assert_eq!(linux_process(&proc, 301), None);
        let _ = std::fs::remove_dir_all(&proc);
    }

    #[test]
    fn frames_go_a_line_each_and_end_with_the_stream() {
        let mut out = Vec::new();
        write_frame(&mut out, &ToFlow::Call { id: 7, tool: "read_pane".into(), args: serde_json::json!({ "lines": 3 }) }).unwrap();
        write_frame(&mut out, &ToFlow::Hello { client: ClientInfo { name: "c".into(), version: "1".into() } }).unwrap();
        assert_eq!(out.iter().filter(|b| **b == b'\n').count(), 2);
        let mut from = io::Cursor::new(out);
        assert_eq!(read_frame::<ToFlow>(&mut from).unwrap(), Some(ToFlow::Call { id: 7, tool: "read_pane".into(), args: serde_json::json!({ "lines": 3 }) }));
        assert!(matches!(read_frame::<ToFlow>(&mut from).unwrap(), Some(ToFlow::Hello { .. })));
        assert_eq!(read_frame::<ToFlow>(&mut from).unwrap(), None);
        assert!(read_frame::<ToFlow>(&mut io::Cursor::new(b"nope\n".to_vec())).is_err());
    }

    #[test]
    fn flow_answers_each_call_over_its_socket_and_hears_when_the_app_goes() {
        let dir = scratch("serve");
        let path = dir.join("flow.sock");
        let (seen, events) = crossbeam_channel::unbounded();
        let handler: Handler = Arc::new(move |event| {
            if let Event::Call(request) = &event {
                let _ = request.reply.send(if request.tool == "list_tabs" { Ok(format!("{} asked", request.client.name)) } else { Err("no".into()) });
            }
            let _ = seen.send(match event {
                Event::Call(r) => format!("call {}", r.tool),
                Event::Gone(_) => "gone".into(),
            });
        });
        let listening = listen_unix(&path, |_| Peer { pid: Some(42), host: None }, handler.clone()).unwrap();
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700, "the user's alone");
        // Another Flow finds this one and serves nothing.
        assert!(listen_unix(&path, |_| Peer::default(), handler.clone()).is_err());
        let stream = UnixStream::connect(&path).unwrap();
        let mut from = BufReader::new(stream.try_clone().unwrap());
        let mut to = stream;
        write_frame(&mut to, &ToFlow::Hello { client: ClientInfo { name: "claude-code".into(), version: String::new() } }).unwrap();
        write_frame(&mut to, &ToFlow::Call { id: 1, tool: "list_tabs".into(), args: Value::Null }).unwrap();
        assert_eq!(read_frame::<FromFlow>(&mut from).unwrap(), Some(FromFlow { id: 1, ok: true, text: "claude-code asked".into() }));
        write_frame(&mut to, &ToFlow::Call { id: 2, tool: "run_command".into(), args: Value::Null }).unwrap();
        assert_eq!(read_frame::<FromFlow>(&mut from).unwrap(), Some(FromFlow { id: 2, ok: false, text: "no".into() }));
        drop((to, from));
        // The second Flow's look, a connection with no calls, goes on a
        // thread of its own, so its going may be heard before this one's
        // calls or among them; this one's calls come in order, and both go.
        let said: Vec<String> = events.iter().take(4).collect();
        let calls: Vec<&str> = said.iter().map(String::as_str).filter(|s| s.starts_with("call")).collect();
        assert_eq!(calls, ["call list_tabs", "call run_command"], "{said:?}");
        assert_eq!(said.iter().filter(|s| *s == "gone").count(), 2, "{said:?}");
        // Stopped, its socket goes, and a stale one is taken over.
        drop(listening);
        assert!(!path.exists());
        std::fs::write(&path, b"").unwrap();
        let again = listen_unix(&path, |_| Peer::default(), handler).unwrap();
        assert!(UnixStream::connect(&path).is_ok());
        drop(again);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_app_gone_while_its_call_waits_for_the_user_is_heard_at_once() {
        let dir = scratch("gone");
        let path = dir.join("flow.sock");
        // Flow holds each call, as a card waiting for the user's answer does.
        let held = Arc::new(std::sync::Mutex::new(Vec::new()));
        let (seen, events) = crossbeam_channel::unbounded();
        let keep = held.clone();
        let handler: Handler = Arc::new(move |event| match event {
            Event::Call(request) => {
                let _ = seen.send(format!("call {}", request.tool));
                keep.lock().unwrap().push(request);
            }
            Event::Gone(_) => {
                let _ = seen.send("gone".into());
            }
        });
        let listening = listen_unix(&path, |_| Peer::default(), handler).unwrap();
        let mut to = UnixStream::connect(&path).unwrap();
        write_frame(&mut to, &ToFlow::Hello { client: ClientInfo { name: "claude-code".into(), version: String::new() } }).unwrap();
        write_frame(&mut to, &ToFlow::Call { id: 1, tool: "list_tabs".into(), args: Value::Null }).unwrap();
        let wait = std::time::Duration::from_secs(2);
        assert_eq!(events.recv_timeout(wait).unwrap(), "call list_tabs");
        // Still waiting, it isn't taken for gone.
        assert!(events.recv_timeout(LOOK_EVERY * 3).is_err(), "nothing while the app waits");
        drop(to);
        assert_eq!(events.recv_timeout(wait).unwrap(), "gone", "heard before the user answers");
        assert_eq!(held.lock().unwrap().len(), 1, "the call was never answered");
        drop(listening);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_system_names_the_user_at_the_other_end() {
        let (a, b) = UnixStream::pair().unwrap();
        let me = this_user();
        assert_eq!((peer_user(&a), peer_user(&b)), (Some(me), Some(me)));
        assert!(of_user(&a, me));
        assert!(!of_user(&a, me.wrapping_add(1)), "anyone else isn't this user");
    }

    #[test]
    fn flow_mcp_sends_nothing_to_a_socket_another_user_listens_at() {
        let dir = scratch("their");
        let path = dir.join("flow.sock");
        let (seen, events) = crossbeam_channel::unbounded();
        let handler: Handler = Arc::new(move |event| {
            let _ = seen.send(matches!(event, Event::Call(_)));
        });
        let listening = listen_unix(&path, |_| Peer::default(), handler).unwrap();
        // As flow-mcp of another user would find this Flow: refused at once.
        let refused = unix::connect_as(&path, this_user().wrapping_add(1)).unwrap_err();
        assert_eq!(refused.kind(), io::ErrorKind::PermissionDenied, "{refused}");
        assert!(NotUsersFlow::is(&refused), "told as not the user's Flow: {refused}");
        // The user's own flow-mcp is let through, and its call reaches Flow,
        // which answers it (here by dropping it).
        let mut to = connect_unix(&path).unwrap();
        let mut from = BufReader::new(to.try_clone().unwrap());
        write_frame(&mut to, &ToFlow::Call { id: 1, tool: "list_tabs".into(), args: Value::Null }).unwrap();
        assert_eq!(read_frame::<FromFlow>(&mut from).unwrap().map(|answer| answer.id), Some(1));
        // The refused one went before a word: the only call is this one.
        let heard: Vec<bool> = events.try_iter().collect();
        assert_eq!(heard.iter().filter(|call| **call).count(), 1, "{heard:?}");
        drop((to, from, listening));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn flow_closes_a_connection_from_another_user_unread() {
        let dir = scratch("theirs");
        let path = dir.join("flow.sock");
        let (seen, events) = crossbeam_channel::unbounded();
        let handler: Handler = Arc::new(move |event| {
            let _ = seen.send(format!("{event:?}"));
        });
        // A Flow serving only someone else, as this test's connection is
        // someone else to it.
        let listening = unix::listen_as(&path, |_| Peer::default(), handler, this_user().wrapping_add(1)).unwrap();
        let stream = UnixStream::connect(&path).unwrap();
        stream.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
        let mut from = BufReader::new(stream.try_clone().unwrap());
        let mut to = stream;
        // It may be closed before these go; either way none is read.
        let _ = write_frame(&mut to, &ToFlow::Hello { client: ClientInfo { name: "claude-code".into(), version: String::new() } });
        let _ = write_frame(&mut to, &ToFlow::Call { id: 1, tool: "list_tabs".into(), args: Value::Null });
        match read_frame::<FromFlow>(&mut from) {
            Ok(None) => {}
            Err(e) => assert!(!matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut), "still open: {e}"),
            Ok(Some(answer)) => panic!("answered: {answer:?}"),
        }
        assert!(events.recv_timeout(LOOK_EVERY * 2).is_err(), "Flow's window heard nothing");
        drop(listening);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_socket_the_system_wont_open_isnt_taken_for_another_users() {
        use std::os::unix::fs::PermissionsExt;
        let dir = scratch("shut");
        let path = dir.join("flow.sock");
        let listening = listen_unix(&path, |_| Peer::default(), Arc::new(|_: Event| {})).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
        match connect_unix(&path) {
            // Root is let in whatever the socket says: nothing to show.
            Ok(_) => eprintln!("skipped: the socket's permissions don't hold for this user"),
            Err(e) => {
                assert_eq!(e.kind(), io::ErrorKind::PermissionDenied, "{e}");
                assert!(!NotUsersFlow::is(&e), "the system's refusal, not flow-mcp's: {e}");
            }
        }
        drop(listening);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn flow_listens_in_no_folder_that_isnt_the_users_own() {
        use std::os::unix::fs::PermissionsExt;
        let dir = scratch("linked");
        let elsewhere = dir.join("elsewhere");
        std::fs::create_dir_all(&elsewhere).unwrap();
        std::fs::set_permissions(&elsewhere, std::fs::Permissions::from_mode(0o755)).unwrap();
        // A link in the socket's folder's place, as another user could leave
        // one in /tmp, to a folder of this user's.
        let link = dir.join("flow-mcp-them");
        std::os::unix::fs::symlink(&elsewhere, &link).unwrap();
        // A file there by the socket's name, as a socket a Flow left would be.
        std::fs::write(elsewhere.join("flow.sock"), b"kept").unwrap();
        let refused = listen_unix(&link.join("flow.sock"), |_| Peer::default(), Arc::new(|_: Event| {}));
        assert!(refused.is_err(), "listened in a link's folder");
        let mode = std::fs::metadata(&elsewhere).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o755, "the folder linked to is left as it was");
        assert_eq!(std::fs::read(elsewhere.join("flow.sock")).unwrap(), b"kept", "and what's in it");
        std::fs::remove_file(elsewhere.join("flow.sock")).unwrap();
        // The folder itself is listened in, and made the user's alone.
        let listening = listen_unix(&elsewhere.join("flow.sock"), |_| Peer::default(), Arc::new(|_: Event| {})).unwrap();
        assert_eq!(std::fs::metadata(&elsewhere).unwrap().permissions().mode() & 0o777, 0o700);
        drop(listening);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
