//! A tab's programs on Windows, for what each tab uses: every process from a
//! Toolhelp snapshot, the shell and those under it picked out by parent, each
//! one's working set (`K32GetProcessMemoryInfo`) and time on the CPU
//! (`GetProcessTimes`). A program in WSL runs in WSL's own Linux, out of
//! sight here.

use std::time::Duration;

use flow_core::resources::{self, Process};
use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS};
use windows_sys::Win32::System::ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
use windows_sys::Win32::System::Threading::{GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ};

/// Process `root` and every one under it, `root` first; empty if it's gone.
pub fn processes(root: u32) -> Vec<Process> {
    // SAFETY: a snapshot of every process, closed below.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE || snapshot.is_null() {
        return Vec::new();
    }
    let mut all = Vec::new();
    let mut entry = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
    // SAFETY: the entry's size is set, as the snapshot's walk asks.
    let mut more = unsafe { Process32FirstW(snapshot, &mut entry) } != 0;
    while more {
        let end = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
        let name = String::from_utf16_lossy(&entry.szExeFile[..end]);
        let name = name.strip_suffix(".exe").unwrap_or(&name).to_string();
        all.push(Process { pid: entry.th32ProcessID, parent: entry.th32ParentProcessID, name, memory: 0, cpu: Duration::ZERO });
        // SAFETY: as above, the same entry.
        more = unsafe { Process32NextW(snapshot, &mut entry) } != 0;
    }
    // SAFETY: the snapshot taken above.
    unsafe { CloseHandle(snapshot) };
    let mut found = resources::under(all, root);
    for process in &mut found {
        read_use(process);
    }
    found
}

/// Its memory and time on the CPU, where Windows lets them be read.
fn read_use(process: &mut Process) {
    let open = |access| -> HANDLE {
        // SAFETY: opened to read its counters, closed below; null if refused.
        unsafe { OpenProcess(access, 0, process.pid) }
    };
    let mut handle = open(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ);
    if handle.is_null() {
        handle = open(PROCESS_QUERY_LIMITED_INFORMATION);
    }
    if handle.is_null() {
        return;
    }
    let mut counters = PROCESS_MEMORY_COUNTERS { cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32, ..Default::default() };
    // SAFETY: fills one PROCESS_MEMORY_COUNTERS of the size given.
    if unsafe { K32GetProcessMemoryInfo(handle, &mut counters, counters.cb) } != 0 {
        process.memory = counters.WorkingSetSize as u64;
    }
    let (mut created, mut exited, mut kernel, mut user) = (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
    // SAFETY: fills four FILETIMEs.
    if unsafe { GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user) } != 0 {
        // In hundreds of nanoseconds.
        let hundreds = |t: FILETIME| ((t.dwHighDateTime as u64) << 32) | t.dwLowDateTime as u64;
        process.cpu = Duration::from_nanos(hundreds(kernel).saturating_add(hundreds(user)).saturating_mul(100));
    }
    // SAFETY: the handle opened above.
    unsafe { CloseHandle(handle) };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_process_and_a_child_it_started_are_found_with_their_use() {
        let mut child = std::process::Command::new("cmd").args(["/c", "ping -n 3 127.0.0.1 >nul"]).spawn().unwrap();
        let me = std::process::id();
        let found = processes(me);
        let _ = child.kill();
        let _ = child.wait();
        assert_eq!(found.first().map(|p| p.pid), Some(me));
        assert!(found[0].memory > 1 << 20, "{:?}", found[0]);
        let cmd = found.iter().find(|p| p.pid == child.id()).expect("the child");
        assert_eq!((cmd.name.as_str(), cmd.parent), ("cmd", me));
        assert!(processes(u32::MAX - 7).is_empty(), "no such process");
    }
}
