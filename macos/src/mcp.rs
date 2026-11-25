//! Flow's socket for outside apps on the Mac (`flow_core::mcp::bridge`):
//! who's at the other end from the socket itself (`LOCAL_PEERPID`), each
//! process's parent from `proc_pidinfo` and its program from
//! `proc_pidpath`, so the card names the app that started `flow-mcp`.

use std::ffi::c_int;
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

use flow_core::mcp::bridge::{self, Handler, Listening, Peer};

/// Listens at Flow's socket.
pub fn listen(handler: Handler) -> Result<Box<dyn Listening>, String> {
    bridge::listen_unix(&bridge::address(), peer_of, handler).map(|l| Box::new(l) as Box<dyn Listening>)
}

fn peer_of(stream: &UnixStream) -> Peer {
    let mut pid: libc::pid_t = 0;
    let mut len = std::mem::size_of::<libc::pid_t>() as libc::socklen_t;
    // SAFETY: fills one pid_t, whose size is passed.
    let got = unsafe { libc::getsockopt(stream.as_raw_fd(), libc::SOL_LOCAL, libc::LOCAL_PEERPID, (&mut pid as *mut libc::pid_t).cast(), &mut len) };
    bridge::peer_from((got == 0 && pid > 0).then_some(pid as u32), process)
}

/// A process's parent and program.
fn process(pid: u32) -> Option<(u32, PathBuf)> {
    // SAFETY: a zeroed proc_bsdinfo is valid; proc_pidinfo fills it, its
    // size passed.
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_bsdinfo>() as c_int;
    let got = unsafe { libc::proc_pidinfo(pid as c_int, libc::PROC_PIDTBSDINFO, 0, (&mut info as *mut libc::proc_bsdinfo).cast(), size) };
    if got != size {
        return None;
    }
    let mut buf = vec![0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    // SAFETY: writes at most the buffer's length, which is passed.
    let n = unsafe { libc::proc_pidpath(pid as c_int, buf.as_mut_ptr().cast(), buf.len() as u32) };
    let path = if n > 0 { PathBuf::from(String::from_utf8_lossy(&buf[..n as usize]).into_owned()) } else { PathBuf::new() };
    Some((info.pbi_ppid, path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_connection_names_this_process_and_the_one_that_started_it() {
        let (a, _b) = UnixStream::pair().unwrap();
        let peer = peer_of(&a);
        assert_eq!(peer.pid, Some(std::process::id()));
        let host = peer.host.expect("the test runner's parent");
        // SAFETY: getppid only reads.
        assert_eq!(host.pid, unsafe { libc::getppid() } as u32);
        assert!(host.path.is_absolute(), "{}", host.path.display());
        let (_, own) = process(std::process::id()).unwrap();
        assert_eq!(own, std::env::current_exe().unwrap().canonicalize().unwrap());
    }
}
