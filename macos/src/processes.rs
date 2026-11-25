//! A tab's programs on the Mac, for what each tab uses: the shell and every
//! process under it, found by parent through libproc (`proc_listpids`), each
//! one's memory footprint (as Activity Monitor shows it) and time on the CPU
//! from `proc_pid_rusage`, and its name from `proc_name`.

use std::ffi::{c_int, c_void};
use std::time::Duration;

use flow_core::resources::Process;

/// `proc_listpids`: the processes whose parent is the one given.
const PROC_PPID_ONLY: u32 = 6;
const RUSAGE_INFO_V2: c_int = 2;

/// `struct rusage_info_v2` from `<sys/resource.h>`; its times are in the
/// Mach clock's ticks (`mach_timebase_info`).
#[repr(C)]
#[derive(Default)]
struct RusageInfoV2 {
    uuid: [u8; 16],
    user_time: u64,
    system_time: u64,
    pkg_idle_wkups: u64,
    interrupt_wkups: u64,
    pageins: u64,
    wired_size: u64,
    resident_size: u64,
    phys_footprint: u64,
    proc_start_abstime: u64,
    proc_exit_abstime: u64,
    child_user_time: u64,
    child_system_time: u64,
    child_pkg_idle_wkups: u64,
    child_interrupt_wkups: u64,
    child_pageins: u64,
    child_elapsed_abstime: u64,
    diskio_bytesread: u64,
    diskio_byteswritten: u64,
}

#[repr(C)]
#[derive(Default)]
struct Timebase {
    numer: u32,
    denom: u32,
}

extern "C" {
    fn proc_listpids(kind: u32, info: u32, buffer: *mut c_void, size: c_int) -> c_int;
    fn proc_pid_rusage(pid: c_int, flavor: c_int, buffer: *mut RusageInfoV2) -> c_int;
    fn proc_name(pid: c_int, buffer: *mut c_void, size: u32) -> c_int;
    fn mach_timebase_info(info: *mut Timebase) -> c_int;
}

/// Process `root` and every one under it, `root` first; empty if it's gone.
pub fn processes(root: u32) -> Vec<Process> {
    let mut timebase = Timebase::default();
    // SAFETY: fills one struct.
    if unsafe { mach_timebase_info(&mut timebase) } != 0 || timebase.denom == 0 {
        timebase = Timebase { numer: 1, denom: 1 };
    }
    let mut found = Vec::new();
    let mut next = vec![(root, 0)];
    while let Some((pid, parent)) = next.pop() {
        let Some(process) = process(pid, parent, &timebase) else { continue };
        next.extend(children(pid).into_iter().map(|kid| (kid, pid)));
        found.push(process);
        // A runaway fork can't keep this going without end.
        if found.len() >= 4096 {
            break;
        }
    }
    found
}

fn process(pid: u32, parent: u32, timebase: &Timebase) -> Option<Process> {
    let mut info = RusageInfoV2::default();
    // SAFETY: fills one rusage_info_v2, which flavor 2 asks for.
    if unsafe { proc_pid_rusage(pid as c_int, RUSAGE_INFO_V2, &mut info) } != 0 {
        return None;
    }
    let ticks = info.user_time.saturating_add(info.system_time);
    let nanos = ticks as u128 * timebase.numer as u128 / timebase.denom as u128;
    let mut name = [0u8; 256];
    // SAFETY: writes at most `name.len()` bytes into it.
    let n = unsafe { proc_name(pid as c_int, name.as_mut_ptr().cast(), name.len() as u32) };
    let name = String::from_utf8_lossy(&name[..n.clamp(0, name.len() as c_int) as usize]).into_owned();
    Some(Process { pid, parent, name, memory: info.phys_footprint, cpu: Duration::from_nanos(nanos.min(u64::MAX as u128) as u64) })
}

/// The processes whose parent is `pid`.
fn children(pid: u32) -> Vec<u32> {
    let mut pids = vec![0 as c_int; 64];
    loop {
        let size = (pids.len() * std::mem::size_of::<c_int>()) as c_int;
        // SAFETY: writes at most `size` bytes into `pids`.
        let got = unsafe { proc_listpids(PROC_PPID_ONLY, pid, pids.as_mut_ptr().cast(), size) };
        if got <= 0 {
            return Vec::new();
        }
        // Filled: there may be more than room for them.
        if got >= size && pids.len() < 1 << 16 {
            pids.resize(pids.len() * 4, 0);
            continue;
        }
        let n = got as usize / std::mem::size_of::<c_int>();
        return pids[..n].iter().filter(|&&p| p > 0 && p as u32 != pid).map(|&p| p as u32).collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_process_and_a_child_it_started_are_found_with_their_use() {
        let mut child = std::process::Command::new("sleep").arg("2").spawn().unwrap();
        let me = std::process::id();
        // Spin a little, so this process has time on the CPU to show.
        let until = std::time::Instant::now() + Duration::from_millis(30);
        while std::time::Instant::now() < until {
            std::hint::black_box(0u64.wrapping_add(1));
        }
        let found = processes(me);
        let _ = child.kill();
        let _ = child.wait();
        assert_eq!(found.first().map(|p| p.pid), Some(me));
        assert!(found[0].memory > 1 << 20 && found[0].cpu >= Duration::from_millis(10), "{:?}", found[0]);
        let sleep = found.iter().find(|p| p.pid == child.id()).expect("the child");
        assert_eq!((sleep.name.as_str(), sleep.parent), ("sleep", me));
        assert!(processes(u32::MAX - 7).is_empty(), "no such process");
    }
}
