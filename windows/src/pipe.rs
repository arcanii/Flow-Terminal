//! The named pipe each tab's programs send images by (`flow_core::pipe`):
//! ConPTY drops kitty's image uploads on their way through the terminal.
//!
//! The pipe is opened for overlapped I/O, so waiting for the next program can
//! end when the tab closes, and each connection's reads and writes wait for
//! themselves. Its name is random, the first instance claims it (nobody can
//! have made it first), and programs on other machines are turned away.
//!
//! Pipes' names are no secret (anyone can list `\\.\pipe\`), so each
//! instance is the user's alone (`UserOnly`, which Flow's pipe for outside
//! apps is made with too): owned by them, its access list naming them and
//! no one else, where Windows' default lets everyone and anonymous read,
//! and in an elevated Flow labelled at its high integrity, so the user's
//! programs that aren't elevated can't write to it. A later instance joins
//! whatever pipe has the name, which, were every instance of Flow's closed
//! a moment, could be one someone else made meanwhile under an access list
//! of theirs: so none is ever let go before the next is made (`serve_each`),
//! and each is checked to be of a pipe the user owns before it's served.

use std::hash::{BuildHasher, Hasher};
use std::io::{self, Read, Write};
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::sync::Arc;
use std::thread::JoinHandle;

use flow_core::pipe::{Pipe, Serve};
use windows_sys::core::BOOL;
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, LocalFree, ERROR_BROKEN_PIPE, ERROR_IO_PENDING, ERROR_PIPE_CONNECTED, ERROR_PIPE_NOT_CONNECTED,
    ERROR_SUCCESS, HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo, SDDL_REVISION_1, SE_KERNEL_OBJECT,
};
use windows_sys::Win32::Security::{
    EqualSid, GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation, TokenIntegrityLevel, TokenUser,
    OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID, SECURITY_ATTRIBUTES, TOKEN_INFORMATION_CLASS, TOKEN_MANDATORY_LABEL,
    TOKEN_QUERY, TOKEN_USER,
};
use windows_sys::Win32::Storage::FileSystem::{
    ReadFile, WriteFile, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED, PIPE_ACCESS_DUPLEX,
};
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE,
    PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
};
use windows_sys::Win32::System::Threading::{
    CreateEventW, GetCurrentProcess, OpenProcess, OpenProcessToken, SetEvent, WaitForMultipleObjects, INFINITE,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};

/// The size Windows is asked to buffer each way.
const BUFFER: u32 = 64 << 10;

/// The integrity level a user's programs run at, as a token's label names
/// it (the RID of its SID, `SECURITY_MANDATORY_MEDIUM_RID`, which
/// windows-sys keeps in a module Flow doesn't build); a sandbox's (an
/// AppContainer's) is below it.
const MEDIUM: u32 = 0x2000;
/// An elevated program's (`SECURITY_MANDATORY_HIGH_RID`).
const HIGH: u32 = 0x3000;

/// A tab's pipe, taking programs on a thread of its own until dropped.
struct Server {
    name: String,
    /// Set when the tab closes, to stop taking programs.
    stop: Arc<OwnedHandle>,
    thread: Option<JoinHandle<()>>,
}

impl Pipe for Server {
    fn name(&self) -> &str {
        &self.name
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        // SAFETY: an event handle this server owns.
        unsafe { SetEvent(self.stop.as_raw_handle()) };
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Opens a pipe with a fresh name and hands each program that connects to
/// `serve`, on a thread of its own.
pub fn open(serve: Serve) -> Option<Box<dyn Pipe>> {
    let name = format!(r"\\.\pipe\flow-images-{}-{:016x}", std::process::id(), random());
    let wide: Vec<u16> = name.encode_utf16().chain([0]).collect();
    let security = match UserOnly::new() {
        Ok(security) => security,
        // No pipe rather than one anyone may open.
        Err(err) => {
            log::warn!("image pipe: {err}");
            return None;
        }
    };
    // The first instance is made now, so the name exists before the shell
    // starts and hands it on.
    let first = instance(&wide, true, &security)?;
    let stop = Arc::new(event()?);
    let stopped = stop.clone();
    let thread = std::thread::Builder::new()
        .name("image-pipe".into())
        .spawn(move || {
            serve_each("image pipe", &wide, first, &security, &stopped, |pipe| {
                let serve = serve.clone();
                let started = std::thread::Builder::new().name("image-pipe-client".into()).spawn(move || {
                    if let Some(connection) = Connection::new(pipe) {
                        serve(Box::new(connection));
                    }
                });
                if let Err(err) = started {
                    log::debug!("image pipe: {err}");
                }
            });
        })
        .ok()?;
    Some(Box::new(Server { name, stop, thread: Some(thread) }))
}

/// Serves pipe `name` (`what`, for the log) from its `first` instance until
/// `stop` is set, handing each instance a program opens to `connected` once
/// the next is made, so the next program finds one waiting. An instance of
/// Flow's always holds the name: one whose wait failed (a program opened it
/// and closed it again before the wait began, `ERROR_NO_DATA`) goes only
/// once another has taken its place, since in a moment with none anyone
/// could make the name theirs, and where no other can be made the pipe
/// serves no more.
pub(crate) fn serve_each(what: &str, name: &[u16], first: OwnedHandle, security: &UserOnly, stop: &OwnedHandle, mut connected: impl FnMut(OwnedHandle)) {
    let mut waiting = first;
    loop {
        match connect(&waiting, stop) {
            Ok(true) => {}
            Ok(false) => return,
            Err(err) => {
                log::debug!("{what}: {err}");
                // The failed one is dropped as its place is taken.
                let Some(next) = instance(name, false, security) else { return };
                waiting = next;
                continue;
            }
        }
        let next = instance(name, false, security);
        connected(waiting);
        match next {
            Some(next) => waiting = next,
            None => {
                log::warn!("{what}: no next instance, so it serves no more");
                return;
            }
        }
    }
}

/// 64 bits no other process can guess: std seeds its hashers from the
/// system's random numbers.
fn random() -> u64 {
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos()));
    hasher.finish()
}

/// One instance of a pipe, the user's alone under `security`: each
/// connection takes one. The `first` claims the name, failing where it's
/// taken; a later one is served only where the pipe it joined is the
/// user's own (`UserOnly::owns`), else none is made and the pipe serves no
/// more (which `serve_each`, never letting the name go, keeps from
/// happening but for a pipe someone else made all the same).
pub(crate) fn instance(name: &[u16], first: bool, security: &UserOnly) -> Option<OwnedHandle> {
    let mut open_mode = PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED;
    if first {
        open_mode |= FILE_FLAG_FIRST_PIPE_INSTANCE;
    }
    let pipe_mode = PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS;
    let attributes = security.attributes();
    // SAFETY: a NUL-terminated name, and security attributes, and the
    // descriptor they point to, that outlive the call.
    let handle = unsafe { CreateNamedPipeW(name.as_ptr(), open_mode, pipe_mode, PIPE_UNLIMITED_INSTANCES, BUFFER, BUFFER, 0, &attributes) };
    if handle == INVALID_HANDLE_VALUE {
        log::debug!("pipe: {}", io::Error::last_os_error());
        return None;
    }
    // SAFETY: a handle just made, owned from here on.
    let pipe = unsafe { OwnedHandle::from_raw_handle(handle) };
    if !security.owns(&pipe) {
        log::warn!("pipe: an instance of a pipe the user doesn't own, so it's served no more");
        return None;
    }
    Some(pipe)
}

/// A user, as a process's token names them: the `TOKEN_USER`
/// GetTokenInformation fills in, kept in words so it's aligned as the
/// struct is, its SID pointing into it.
pub(crate) struct User(Vec<u64>);

impl User {
    /// This process's.
    pub(crate) fn own() -> Option<User> {
        // SAFETY: this process's pseudo-handle, which needs no closing.
        Token::of(unsafe { GetCurrentProcess() })?.user()
    }

    /// Process `pid`'s, where it runs as them in full: none for a process
    /// gone, or one this user can't open or read the token of, which is
    /// another user's; none either for one held below medium integrity (an
    /// AppContainer's, a sandbox's), which the user's own programs aren't,
    /// so a program they let run boxed in can't pass for them.
    pub(crate) fn of_process(pid: u32) -> Option<User> {
        // SAFETY: opened to read its token only, closed below.
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if process.is_null() {
            return None;
        }
        let token = Token::of(process);
        // SAFETY: the process opened above; its token, if opened, stays open.
        unsafe { CloseHandle(process) };
        let token = token?;
        if token.integrity()? < MEDIUM {
            return None;
        }
        token.user()
    }

    fn sid(&self) -> PSID {
        // SAFETY: `Token::user` checked that a TOKEN_USER fills the start
        // of the buffer, which lives as long as `self`.
        unsafe { (*self.0.as_ptr().cast::<TOKEN_USER>()).User.Sid }
    }

    /// Whether `other` is the same user.
    pub(crate) fn is(&self, other: &User) -> bool {
        self.is_sid(other.sid())
    }

    /// Whether `sid` is this user's.
    fn is_sid(&self, sid: PSID) -> bool {
        // SAFETY: two SIDs alive here.
        !sid.is_null() && unsafe { EqualSid(self.sid(), sid) } != 0
    }

    /// Its SID written out (`S-1-5-21-…`), as an access list in SDDL names it.
    fn text(&self) -> Option<String> {
        let mut text: *mut u16 = std::ptr::null_mut();
        // SAFETY: the SID in the buffer, alive here; the string is freed below.
        if unsafe { ConvertSidToStringSidW(self.sid(), &mut text) } == 0 || text.is_null() {
            return None;
        }
        // SAFETY: a NUL-terminated string LocalAlloc made.
        let len = (0..).take_while(|&i| unsafe { *text.add(i) } != 0).count();
        let sid = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(text, len) });
        unsafe { LocalFree(text.cast()) };
        Some(sid)
    }
}

/// A process's token, opened to be read, closed when dropped.
struct Token(HANDLE);

impl Drop for Token {
    fn drop(&mut self) {
        // SAFETY: the token `of` opened.
        unsafe { CloseHandle(self.0) };
    }
}

impl Token {
    /// `process`'s, `process` a handle that may query it.
    fn of(process: HANDLE) -> Option<Token> {
        let mut token: HANDLE = std::ptr::null_mut();
        // SAFETY: the process's token, opened to read and closed on drop.
        if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) } == 0 {
            return None;
        }
        Some(Token(token))
    }

    /// What it says of `class`, at least `least` bytes, kept in words so
    /// the struct at its start is aligned.
    fn info(&self, class: TOKEN_INFORMATION_CLASS, least: usize) -> Option<Vec<u64>> {
        let mut size = 0u32;
        // SAFETY: asks the size first, then fills a buffer at least that big.
        unsafe { GetTokenInformation(self.0, class, std::ptr::null_mut(), 0, &mut size) };
        let mut buf = vec![0u64; (size as usize).div_ceil(8)];
        let got = unsafe { GetTokenInformation(self.0, class, buf.as_mut_ptr().cast(), size, &mut size) };
        (got != 0 && size as usize >= least).then_some(buf)
    }

    /// Its user.
    fn user(&self) -> Option<User> {
        self.info(TokenUser, std::mem::size_of::<TOKEN_USER>()).map(User)
    }

    /// Its integrity level: the last part (RID) of its label's SID.
    fn integrity(&self) -> Option<u32> {
        let buf = self.info(TokenIntegrityLevel, std::mem::size_of::<TOKEN_MANDATORY_LABEL>())?;
        // SAFETY: `info` checked that a TOKEN_MANDATORY_LABEL fills the
        // buffer's start; its SID points into the buffer, alive here.
        let sid = unsafe { (*buf.as_ptr().cast::<TOKEN_MANDATORY_LABEL>()).Label.Sid };
        if sid.is_null() {
            return None;
        }
        // SAFETY: a SID's count of parts, and its last part where it has one.
        let parts = unsafe { *GetSidSubAuthorityCount(sid) };
        (parts > 0).then(|| unsafe { *GetSidSubAuthority(sid, u32::from(parts) - 1) })
    }
}

/// Whether the pipe `handle` is an end of (a server's instance, or a
/// program's end it opened) is owned by `user`. Only the user can make
/// themselves a pipe's owner (anyone else would need to be the system's
/// own), and every pipe Flow makes is owned by its user (`UserOnly`), so a
/// pipe someone else made never passes, whoever serves it and whatever
/// process id it says it's served by. The handle needs READ_CONTROL, as
/// GENERIC_READ gives.
pub(crate) fn owned_by(handle: HANDLE, user: &User) -> bool {
    let mut owner: PSID = std::ptr::null_mut();
    let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    // SAFETY: a pipe's handle; `owner` points into the descriptor made,
    // which is freed below.
    let got = unsafe {
        GetSecurityInfo(handle, SE_KERNEL_OBJECT, OWNER_SECURITY_INFORMATION, &mut owner, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut(), &mut descriptor)
    };
    let owned = got == ERROR_SUCCESS && user.is_sid(owner);
    if !descriptor.is_null() {
        // SAFETY: memory GetSecurityInfo gave, no longer read.
        unsafe { LocalFree(descriptor) };
    }
    owned
}

/// A security descriptor that makes the user a pipe's owner and grants
/// them alone everything (no one else, administrators and the system
/// included, gets in), freed when dropped; Flow's pipes make each instance
/// with it.
pub(crate) struct UserOnly {
    descriptor: PSECURITY_DESCRIPTOR,
    user: User,
}

// SAFETY: the descriptor is plain memory, only read once made.
unsafe impl Send for UserOnly {}

impl Drop for UserOnly {
    fn drop(&mut self) {
        // SAFETY: memory ConvertStringSecurityDescriptorToSecurityDescriptorW gave.
        unsafe { LocalFree(self.descriptor) };
    }
}

impl UserOnly {
    pub(crate) fn new() -> Result<Self, String> {
        let user = User::own().ok_or("Flow couldn't read its user's SID")?;
        let sid = user.text().ok_or("Flow couldn't read its user's SID")?;
        // SAFETY: this process's pseudo-handle, which needs no closing.
        let high = Token::of(unsafe { GetCurrentProcess() }).and_then(|token| token.integrity()).is_some_and(|level| level >= HIGH);
        let sddl: Vec<u16> = user_only(&sid, high).encode_utf16().chain([0]).collect();
        let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        // SAFETY: a NUL-terminated string; the descriptor is freed on drop.
        let made = unsafe { ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl.as_ptr(), SDDL_REVISION_1, &mut descriptor, std::ptr::null_mut()) };
        if made == 0 || descriptor.is_null() {
            return Err(format!("Flow couldn't make the pipe's security: {}", io::Error::last_os_error()));
        }
        Ok(UserOnly { descriptor, user })
    }

    fn attributes(&self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES { nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32, lpSecurityDescriptor: self.descriptor, bInheritHandle: 0 }
    }

    /// Whether the pipe `pipe` is an instance of is owned by this user, as
    /// those made under this descriptor are (`owned_by`).
    fn owns(&self, pipe: &OwnedHandle) -> bool {
        owned_by(pipe.as_raw_handle(), &self.user)
    }
}

/// The SDDL of `UserOnly`'s descriptor for the user whose SID is `sid`:
/// owned by them, protected (no entries inherited), full access for them
/// alone. A Flow at `high` integrity (elevated) labels it so, which keeps
/// the user's programs that aren't elevated from writing to it, as they
/// couldn't to the pipe Windows would make by default; unlabelled, it would
/// count as medium, as everything unlabelled does. No higher than its own,
/// which needs no privilege.
fn user_only(sid: &str, high: bool) -> String {
    let label = if high { "S:(ML;;NW;;;HI)" } else { "" };
    format!("O:{sid}D:P(A;;GA;;;{sid}){label}")
}

/// A manual-reset event, not set.
pub(crate) fn event() -> Option<OwnedHandle> {
    // SAFETY: no name and default security.
    let handle = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
    // SAFETY: a handle just made, owned from here on.
    (!handle.is_null()).then(|| unsafe { OwnedHandle::from_raw_handle(handle) })
}

/// Waits for a program to open `pipe`: true once one has, false once `stop`
/// is set.
pub(crate) fn connect(pipe: &OwnedHandle, stop: &OwnedHandle) -> io::Result<bool> {
    let done = event().ok_or_else(io::Error::last_os_error)?;
    let handle = pipe.as_raw_handle();
    // SAFETY: zeroes are a valid OVERLAPPED.
    let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
    overlapped.hEvent = done.as_raw_handle();
    // SAFETY: `overlapped` and its event outlive the operation: it either
    // finishes here, or is waited for below, or cancelled and waited for.
    if unsafe { ConnectNamedPipe(handle, &mut overlapped) } == 0 {
        match unsafe { GetLastError() } {
            ERROR_PIPE_CONNECTED => return Ok(true),
            ERROR_IO_PENDING => {}
            err => return Err(io::Error::from_raw_os_error(err as i32)),
        }
    }
    let events = [overlapped.hEvent, stop.as_raw_handle()];
    // SAFETY: two valid event handles.
    let woke = unsafe { WaitForMultipleObjects(2, events.as_ptr(), 0, INFINITE) };
    let mut moved = 0;
    if woke == WAIT_OBJECT_0 {
        // SAFETY: the operation `overlapped` belongs to has finished.
        if unsafe { GetOverlappedResult(handle, &overlapped, &mut moved, 0) } == 0 {
            return Err(io::Error::last_os_error());
        }
        return Ok(true);
    }
    // The tab closed: the wait is called off, and over, before `overlapped` goes.
    // SAFETY: as above.
    unsafe {
        CancelIoEx(handle, &overlapped);
        GetOverlappedResult(handle, &overlapped, &mut moved, 1);
    }
    Ok(false)
}

/// A program connected to the pipe. Each read and write waits for itself.
pub(crate) struct Connection {
    pipe: OwnedHandle,
    done: OwnedHandle,
}

impl Connection {
    pub(crate) fn new(pipe: OwnedHandle) -> Option<Self> {
        Some(Self { pipe, done: event()? })
    }

    /// Starts an operation with `start` and waits for it; returns the bytes
    /// it moved.
    fn wait(&mut self, start: impl FnOnce(HANDLE, *mut OVERLAPPED) -> BOOL) -> io::Result<usize> {
        let handle = self.pipe.as_raw_handle();
        // SAFETY: zeroes are a valid OVERLAPPED.
        let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
        overlapped.hEvent = self.done.as_raw_handle();
        if start(handle, &mut overlapped) == 0 {
            let err = unsafe { GetLastError() };
            if err != ERROR_IO_PENDING {
                return Err(io::Error::from_raw_os_error(err as i32));
            }
        }
        let mut moved = 0;
        // SAFETY: waits for the operation `overlapped` belongs to, which
        // outlives it.
        if unsafe { GetOverlappedResult(handle, &overlapped, &mut moved, 1) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(moved as usize)
    }
}

impl Read for Connection {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let len = buf.len().min(u32::MAX as usize) as u32;
        let ptr = buf.as_mut_ptr();
        // SAFETY: `buf` outlives the read, which `wait` waits for.
        match self.wait(|handle, overlapped| unsafe { ReadFile(handle, ptr, len, std::ptr::null_mut(), overlapped) }) {
            // The program hung up.
            Err(err) if [ERROR_BROKEN_PIPE, ERROR_PIPE_NOT_CONNECTED].map(|e| Some(e as i32)).contains(&err.raw_os_error()) => Ok(0),
            other => other,
        }
    }
}

impl Write for Connection {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let len = buf.len().min(u32::MAX as usize) as u32;
        let ptr = buf.as_ptr();
        // SAFETY: `buf` outlives the write, which `wait` waits for.
        self.wait(|handle, overlapped| unsafe { WriteFile(handle, ptr, len, std::ptr::null_mut(), overlapped) })
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    /// Opens the pipe as a program would: while Flow readies the next
    /// instance, every one is busy for a moment.
    fn connect_to(name: &str) -> io::Result<std::fs::File> {
        let mut tries = 0;
        loop {
            match std::fs::OpenOptions::new().read(true).write(true).open(name) {
                Err(err) if err.raw_os_error() == Some(231) && tries < 50 => tries += 1,
                other => return other,
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    #[test]
    fn programs_connect_one_after_another_and_closing_stops_the_pipe() {
        // Each program's lines come back upper-cased.
        let (tx, rx) = mpsc::channel();
        let tx = std::sync::Mutex::new(tx);
        let pipe = open(Arc::new(move |mut stream| {
            let mut buf = [0u8; 64];
            while let Ok(n) = stream.read(&mut buf) {
                if n == 0 {
                    break;
                }
                let _ = stream.write_all(&buf[..n].to_ascii_uppercase());
            }
            let _ = tx.lock().unwrap().send(());
        }))
        .expect("a pipe");
        let name = pipe.name().to_string();
        assert!(name.starts_with(r"\\.\pipe\flow-images-"));
        for word in [&b"first"[..], b"second"] {
            let mut client = connect_to(&name).expect("opens");
            client.write_all(word).unwrap();
            let mut back = vec![0u8; word.len()];
            client.read_exact(&mut back).unwrap();
            assert_eq!(back, word.to_ascii_uppercase());
            drop(client);
            rx.recv_timeout(std::time::Duration::from_secs(5)).expect("the program hung up");
        }
        // Two at once.
        let mut a = connect_to(&name).expect("opens");
        let mut b = connect_to(&name).expect("opens");
        b.write_all(b"b").unwrap();
        a.write_all(b"a").unwrap();
        let mut one = [0u8; 1];
        a.read_exact(&mut one).unwrap();
        assert_eq!(&one, b"A");
        b.read_exact(&mut one).unwrap();
        assert_eq!(&one, b"B");
        drop(pipe);
        assert!(std::fs::OpenOptions::new().read(true).write(true).open(&name).is_err(), "no instance waits");
        // Programs still connected keep their connection.
        a.write_all(b"z").unwrap();
        a.read_exact(&mut one).unwrap();
        assert_eq!(&one, b"Z");
    }

    #[test]
    fn a_process_is_this_users_only_where_its_token_says_so() {
        let me = User::own().expect("this process's user");
        let sid = me.text().expect("its SID");
        assert!(sid.starts_with("S-1-"), "{sid}");
        assert!(User::of_process(std::process::id()).is_some_and(|user| user.is(&me)));
        // The system's own process (4) is never this user's, whether or not
        // this user may read its token, unless the tests run as the system.
        if sid != "S-1-5-18" {
            assert!(!User::of_process(4).is_some_and(|user| user.is(&me)));
        }
    }

    #[test]
    fn this_users_programs_run_at_medium_integrity_or_above() {
        // SAFETY: this process's pseudo-handle.
        let token = Token::of(unsafe { GetCurrentProcess() }).expect("this process's token");
        assert!(token.integrity().is_some_and(|level| level >= MEDIUM), "{:?}", token.integrity());
    }

    #[test]
    fn every_instance_is_the_users_own_and_the_first_claims_the_name() {
        let security = UserOnly::new().unwrap();
        let text = format!(r"\\.\pipe\flow-test-owned-{}-{:016x}", std::process::id(), random());
        let name: Vec<u16> = text.encode_utf16().chain([0]).collect();
        let first = instance(&name, true, &security).expect("the first instance");
        assert!(security.owns(&first));
        assert!(instance(&name, true, &security).is_none(), "the name is taken");
        let later = instance(&name, false, &security).expect("a later instance of the user's own pipe");
        assert!(security.owns(&later));
        // A program's end of it says the same, as flow-mcp reads it.
        let client = std::fs::OpenOptions::new().read(true).write(true).open(&text).expect("opens");
        assert!(owned_by(client.as_raw_handle(), &User::own().unwrap()));
    }

    #[test]
    fn the_pipes_security_names_the_user_alone_and_labels_an_elevated_flows_high() {
        assert_eq!(user_only("S-1-5-21-1-2-3-1001", false), "O:S-1-5-21-1-2-3-1001D:P(A;;GA;;;S-1-5-21-1-2-3-1001)");
        assert_eq!(user_only("S-1-5-21-1-2-3-1001", true), "O:S-1-5-21-1-2-3-1001D:P(A;;GA;;;S-1-5-21-1-2-3-1001)S:(ML;;NW;;;HI)");
    }

    #[test]
    fn programs_that_open_the_pipe_and_close_it_at_once_never_leave_its_name_free() {
        // Each program's lines come back upper-cased.
        let pipe = open(Arc::new(|mut stream| {
            let mut buf = [0u8; 64];
            while let Ok(n) = stream.read(&mut buf) {
                if n == 0 {
                    break;
                }
                let _ = stream.write_all(&buf[..n].to_ascii_uppercase());
            }
        }))
        .expect("a pipe");
        let name = pipe.name().to_string();
        let wide: Vec<u16> = name.encode_utf16().chain([0]).collect();
        let security = UserOnly::new().unwrap();
        for _ in 0..200 {
            // Gone again before Flow waits on its instance, or after: either
            // way an instance of Flow's still has the name, which can't be
            // claimed afresh, and the next program finds one.
            drop(connect_to(&name).expect("opens"));
            assert!(instance(&wide, true, &security).is_none(), "the name was free a moment");
        }
        let mut client = connect_to(&name).expect("opens");
        client.write_all(b"still").unwrap();
        let mut back = [0u8; 5];
        client.read_exact(&mut back).unwrap();
        assert_eq!(&back, b"STILL");
        drop((client, pipe));
    }
}
