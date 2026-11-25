//! Flow's pipe for outside apps on Windows (`flow_core::mcp::bridge`):
//! `\\.\pipe\flow-mcp-<user>`, served as the tabs' image pipe is (its
//! instances, waits and connections, `pipe.rs`), but under a fixed name
//! and the user's alone: owned by them, its DACL grants their SID and no
//! one else, administrators and the system included (`pipe::UserOnly`), and
//! programs on other machines are turned away. Its first instance claims
//! the name, so a second Flow, or anything that took the name first, gets
//! none; no instance goes before the next is made (`pipe::serve_each`), so
//! the name is never free while Flow serves it, and each later one is
//! checked to be of the user's own pipe. Pipes' names are one namespace
//! for every user, so another could make this one first: `flow-mcp` opens
//! it (`open`) letting what serves it know who opened it but never act as
//! them, and checks that it's owned by its own user and served by a
//! `flow.exe` of theirs (`served_by_flow`) before it sends a word, and
//! Flow reads who connected from the pipe itself
//! (`GetNamedPipeClientProcessId`).

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::{AsRawHandle, OwnedHandle};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use flow_core::mcp::bridge::{self, Conns, Duplex, Handler, Peer};
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::SECURITY_IDENTIFICATION;
use windows_sys::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS};
use windows_sys::Win32::System::Pipes::{DisconnectNamedPipe, GetNamedPipeClientProcessId, GetNamedPipeServerProcessId};
use windows_sys::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, SetEvent, PROCESS_QUERY_LIMITED_INFORMATION};
use windows_sys::Win32::System::IO::CancelIoEx;

use crate::pipe;

/// The pipe served, until dropped.
struct Listening {
    stop: Arc<OwnedHandle>,
    conns: Conns,
    thread: Option<JoinHandle<()>>,
}

impl bridge::Listening for Listening {}

impl Drop for Listening {
    fn drop(&mut self) {
        // SAFETY: an event handle this listener owns.
        unsafe { SetEvent(self.stop.as_raw_handle()) };
        self.conns.end_all();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Listens at Flow's pipe, each `flow-mcp` served on a thread of its own.
/// The security is moved to the listening thread, which makes each
/// instance with it.
pub fn listen(handler: Handler) -> Result<Box<dyn bridge::Listening>, String> {
    let name: Vec<u16> = bridge::address().as_os_str().to_string_lossy().encode_utf16().chain([0]).collect();
    let security = pipe::UserOnly::new()?;
    let first = pipe::instance(&name, true, &security).ok_or("another Flow already lets outside apps connect, or the pipe's name is taken")?;
    let stop = Arc::new(pipe::event().ok_or("Flow couldn't make an event")?);
    let conns = Conns::default();
    let (stopped, kept) = (stop.clone(), conns.clone());
    let thread = std::thread::Builder::new()
        .name("flow-mcp".into())
        .spawn(move || {
            pipe::serve_each("outside apps' pipe", &name, first, &security, &stopped, |instance| {
                let peer = peer_of(&instance);
                let handle = instance.as_raw_handle() as usize;
                let Some(connection) = pipe::Connection::new(instance) else { return };
                let conn = McpConn { inner: Arc::new(Mutex::new(connection)), handle };
                kept.add(&conn);
                let (handler, id) = (handler.clone(), bridge::next_conn());
                let _ = std::thread::Builder::new().name("flow-mcp-conn".into()).spawn(move || bridge::serve(id, Box::new(conn), peer, handler));
            });
        })
        .map_err(|e| e.to_string())?;
    Ok(Box::new(Listening { stop, conns, thread: Some(thread) }))
}

/// A connection, shared by the thread reading and writing it (one after
/// the other) and whoever ends it.
struct McpConn {
    inner: Arc<Mutex<pipe::Connection>>,
    /// The pipe's handle, for ending it while a read waits.
    handle: usize,
}

impl Read for McpConn {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).read(buf)
    }
}

impl Write for McpConn {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Duplex for McpConn {
    fn try_clone_box(&self) -> io::Result<Box<dyn Duplex>> {
        Ok(Box::new(McpConn { inner: self.inner.clone(), handle: self.handle }))
    }

    fn shutdown(&self) {
        let handle = self.handle as HANDLE;
        // SAFETY: the pipe's handle, alive while `inner` holds it; a read
        // waiting on it is called off, and the client let go.
        unsafe {
            CancelIoEx(handle, std::ptr::null());
            DisconnectNamedPipe(handle);
        }
    }
}

/// Who connected: `flow-mcp`'s process, and the app that started it.
fn peer_of(pipe: &OwnedHandle) -> Peer {
    let mut pid = 0u32;
    // SAFETY: a connected pipe's handle.
    let got = unsafe { GetNamedPipeClientProcessId(pipe.as_raw_handle(), &mut pid) };
    bridge::peer_from((got != 0 && pid != 0).then_some(pid), process)
}

/// Opens Flow's pipe at `address` for `flow-mcp` (`as_client`), and gives
/// it only where it's Flow's (`served_by_flow`): anything else that serves
/// it is refused as `bridge::NotUsersFlow`, before a word is sent. One the
/// system won't open is the system's own error, `PermissionDenied` among
/// them.
pub fn open(address: &Path) -> io::Result<File> {
    let pipe = as_client(address)?;
    if !served_by_flow(&pipe) {
        return Err(bridge::NotUsersFlow::error());
    }
    Ok(pipe)
}

/// Opens the pipe at `address` as `flow-mcp` does, checking nothing of
/// who serves it: to read and write, at the level of identification, so
/// whatever serves it may learn who opened it (which the pipe's name says
/// already) but never act as them. Asked for no level, as std's
/// `OpenOptions` asks by default, a pipe's client lets whatever serves it
/// act as them (once it has read what they wrote), and the server isn't
/// yet known to be Flow when it opens: a program that took the name first
/// under an account that may impersonate (a service's, holding
/// SeImpersonatePrivilege) could take on the user's token and start
/// programs as them. Flow itself never acts as its clients, and the
/// system names the process that opened a pipe
/// (`GetNamedPipeClientProcessId`) at any level. std adds the
/// SECURITY_SQOS_PRESENT the level needs. The test of the level opens
/// through here, so it measures what `open` asks.
fn as_client(address: &Path) -> io::Result<File> {
    OpenOptions::new().read(true).write(true).security_qos_flags(SECURITY_IDENTIFICATION).open(address)
}

/// Whether `pipe`, opened by `flow-mcp`, is Flow's: owned by this user, as
/// every pipe Flow makes is and none another user makes can be
/// (`pipe::owned_by`), and served by a `flow.exe` running as them in full,
/// so nothing that took Flow's pipe's name first is answered: not another
/// user's program, whatever it's named or whatever process id its pipe
/// gives (the one that made it, which may have gone and its id gone to
/// another program), nor a program of this user's that isn't Flow.
fn served_by_flow(pipe: &File) -> bool {
    let Some(me) = pipe::User::own() else { return false };
    if !pipe::owned_by(pipe.as_raw_handle(), &me) {
        return false;
    }
    let mut pid = 0u32;
    // SAFETY: an open pipe's handle.
    if unsafe { GetNamedPipeServerProcessId(pipe.as_raw_handle(), &mut pid) } == 0 {
        return false;
    }
    let users = pipe::User::of_process(pid).is_some_and(|server| me.is(&server));
    users && image(pid).is_some_and(|path| path.file_name().is_some_and(|n| n.eq_ignore_ascii_case("flow.exe")))
}

/// A process's parent and program.
fn process(pid: u32) -> Option<(u32, PathBuf)> {
    Some((parent(pid)?, image(pid).unwrap_or_default()))
}

/// A process's program, as the system names it.
fn image(pid: u32) -> Option<PathBuf> {
    // SAFETY: opened to read its name only, closed below.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return None;
    }
    let mut buf = vec![0u16; 32_768];
    let mut len = buf.len() as u32;
    // SAFETY: writes at most `len` characters, which it then says.
    let got = unsafe { QueryFullProcessImageNameW(process, 0, buf.as_mut_ptr(), &mut len) };
    unsafe { CloseHandle(process) };
    (got != 0).then(|| PathBuf::from(String::from_utf16_lossy(&buf[..len as usize])))
}

/// A process's parent, from a Toolhelp snapshot.
fn parent(pid: u32) -> Option<u32> {
    // SAFETY: a snapshot of every process, closed below.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE || snapshot.is_null() {
        return None;
    }
    let mut entry = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
    let mut found = None;
    // SAFETY: the entry's size is set, as the walk asks.
    let mut more = unsafe { Process32FirstW(snapshot, &mut entry) } != 0;
    while more {
        if entry.th32ProcessID == pid {
            found = Some(entry.th32ParentProcessID);
            break;
        }
        more = unsafe { Process32NextW(snapshot, &mut entry) } != 0;
    }
    unsafe { CloseHandle(snapshot) };
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::Security::{
        GetTokenInformation, RevertToSelf, SecurityIdentification, SecurityImpersonation, TokenImpersonationLevel,
        SECURITY_IMPERSONATION_LEVEL, TOKEN_QUERY,
    };
    use windows_sys::Win32::System::Pipes::ImpersonateNamedPipeClient;
    use windows_sys::Win32::System::Threading::{GetCurrentThread, OpenThreadToken};

    #[test]
    fn this_process_has_a_parent_and_a_program() {
        let (parent, path) = process(std::process::id()).unwrap();
        assert!(parent != 0);
        assert_eq!(path.canonicalize().unwrap(), std::env::current_exe().unwrap().canonicalize().unwrap());
    }

    /// The level at which whatever serves a pipe may act as the program
    /// that opened it by `opening`, as the server finds it: taking on the
    /// program's token once it has read what the program wrote, as
    /// impersonating a pipe's client asks, and reading the token's level.
    fn level_given(opening: impl FnOnce(&Path) -> io::Result<File>, which: &str) -> SECURITY_IMPERSONATION_LEVEL {
        let security = pipe::UserOnly::new().unwrap();
        let text = format!(r"\\.\pipe\flow-test-level-{which}-{}", std::process::id());
        let name: Vec<u16> = text.encode_utf16().chain([0]).collect();
        let instance = pipe::instance(&name, true, &security).expect("an instance");
        let mut client = opening(Path::new(&text)).expect("opens");
        client.write_all(b"x").unwrap();
        let stop = pipe::event().unwrap();
        assert!(pipe::connect(&instance, &stop).unwrap(), "connected");
        let handle = instance.as_raw_handle();
        let mut server = pipe::Connection::new(instance).unwrap();
        let mut one = [0u8; 1];
        server.read_exact(&mut one).unwrap();
        // SAFETY: the connected pipe's handle, alive while `server` holds it.
        assert!(unsafe { ImpersonateNamedPipeClient(handle) } != 0, "{}", io::Error::last_os_error());
        let mut token: HANDLE = std::ptr::null_mut();
        let mut level: SECURITY_IMPERSONATION_LEVEL = -1;
        let mut size = 0u32;
        // SAFETY: this thread's token, taken on above, opened as this
        // process (one at identification can't be opened as the program it
        // names), read into `level` and closed; the thread is itself again
        // before anything here could unwind.
        let read = unsafe {
            let opened = OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, 1, &mut token) != 0;
            let len = std::mem::size_of::<SECURITY_IMPERSONATION_LEVEL>() as u32;
            let read = opened && GetTokenInformation(token, TokenImpersonationLevel, std::ptr::from_mut(&mut level).cast(), len, &mut size) != 0;
            let err = io::Error::last_os_error();
            if opened {
                CloseHandle(token);
            }
            RevertToSelf();
            if read { Ok(level) } else { Err(err) }
        };
        drop(client);
        read.expect("the level of the token taken on")
    }

    #[test]
    fn what_serves_flows_pipe_may_know_who_opened_it_but_never_act_as_them() {
        assert_eq!(level_given(as_client, "flow-mcp"), SecurityIdentification);
        // A pipe opened as std opens one by default, as flow-mcp opened
        // Flow's before 0.1.46, lets its server act as the program that
        // opened it.
        let by_default = |address: &Path| OpenOptions::new().read(true).write(true).open(address);
        assert_eq!(level_given(by_default, "default"), SecurityImpersonation);
    }
}
