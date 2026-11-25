//! What each tab uses, for the card under a tab and About's Resources: its
//! programs (the shell and everything started under it, which each OS crate
//! reads, `Platform::processes`), and Flow's own for it (its scrollback, its
//! pictures, and what they hold on the GPU). CPU is a share of one core, from
//! two readings of each program's time on the CPU about a second apart.
//! Linux's `/proc` is read here, so it's tested on any OS.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

/// One process, as the OS tells it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Process {
    pub pid: u32,
    pub parent: u32,
    pub name: String,
    /// Its memory: the Mac's footprint, the resident set elsewhere.
    pub memory: u64,
    /// Its time on the CPU so far, its own and the kernel's for it.
    pub cpu: Duration,
}

/// A tab's programs, or Flow's own process, read once.
#[derive(Debug, Clone)]
pub struct Reading {
    pub at: Instant,
    pub processes: Vec<Process>,
}

/// What a tab's programs use, from two readings.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Programs {
    /// The shell and everything under it.
    pub count: usize,
    pub memory: u64,
    /// Shares of one core: 1.0 is a whole core.
    pub cpu: f32,
    /// The program under the shell working hardest, or the largest when
    /// none is working; none with only the shell.
    pub busiest: Option<String>,
}

/// What `now` says the programs use, their CPU from what each did since
/// `before`: a program that wasn't there then counts from now on.
pub fn programs(before: Option<&Reading>, now: &Reading, shell: u32) -> Programs {
    let then: HashMap<u32, Duration> = before.map(|b| b.processes.iter().map(|p| (p.pid, p.cpu)).collect()).unwrap_or_default();
    let wall = before.map(|b| now.at.saturating_duration_since(b.at).as_secs_f32()).unwrap_or(0.0);
    let share = |p: &Process| match then.get(&p.pid) {
        Some(was) if wall > 0.0 => p.cpu.saturating_sub(*was).as_secs_f32() / wall,
        _ => 0.0,
    };
    let busiest = now
        .processes
        .iter()
        .filter(|p| p.pid != shell)
        .map(|p| (share(p), p))
        .max_by(|(a, p), (b, q)| a.total_cmp(b).then(p.memory.cmp(&q.memory)))
        .map(|(_, p)| p.name.clone());
    Programs {
        count: now.processes.len(),
        memory: now.processes.iter().map(|p| p.memory).sum(),
        cpu: now.processes.iter().map(share).sum(),
        busiest,
    }
}

/// Linux's: `root` and every process under it, from `proc` (`/proc`), with
/// the system's page size in bytes and its clock's ticks a second.
pub fn linux(proc: &Path, root: u32, page: u64, ticks: u64) -> Vec<Process> {
    let Ok(entries) = fs::read_dir(proc) else { return Vec::new() };
    let all: Vec<Process> = entries
        .flatten()
        .filter_map(|e| e.file_name().to_str()?.parse::<u32>().ok())
        .filter_map(|pid| linux_stat(&fs::read_to_string(proc.join(pid.to_string()).join("stat")).ok()?, page, ticks))
        .collect();
    under(all, root)
}

/// `root` and its descendants among `all`, `root` first.
pub fn under(all: Vec<Process>, root: u32) -> Vec<Process> {
    let mut kids: HashMap<u32, Vec<usize>> = HashMap::new();
    for (i, p) in all.iter().enumerate() {
        kids.entry(p.parent).or_default().push(i);
    }
    let Some(top) = all.iter().position(|p| p.pid == root) else { return Vec::new() };
    let mut order = vec![top];
    let mut i = 0;
    while i < order.len() {
        let pid = all[order[i]].pid;
        // A process can't be its own parent, but pid 0's children (the
        // first processes) say 0.
        order.extend(kids.get(&pid).into_iter().flatten().filter(|&&k| all[k].pid != pid));
        i += 1;
    }
    let mut all: Vec<Option<Process>> = all.into_iter().map(Some).collect();
    order.into_iter().filter_map(|i| all[i].take()).collect()
}

/// One `/proc/<pid>/stat`: `pid (name) state ppid … utime stime … rss …`,
/// the name in brackets and able to hold anything, spaces and brackets too.
fn linux_stat(stat: &str, page: u64, ticks: u64) -> Option<Process> {
    let open = stat.find('(')?;
    let close = stat.rfind(')')?;
    let pid = stat[..open].trim().parse().ok()?;
    let name = stat[open + 1..close].to_string();
    // After the name: state (field 3), ppid (4), … utime (14), stime (15),
    // … rss (24), counted from 1 with the pid.
    let rest: Vec<&str> = stat[close + 1..].split_whitespace().collect();
    let field = |n: usize| rest.get(n - 3).and_then(|v| v.parse::<u64>().ok());
    let time = field(14)? + field(15)?;
    Some(Process {
        pid,
        parent: field(4)? as u32,
        name,
        memory: field(24)? * page,
        cpu: Duration::from_secs_f64(time as f64 / ticks.max(1) as f64),
    })
}

/// This Linux's: `/proc`, with its page size and clock ticks.
#[cfg(all(unix, not(target_os = "macos")))]
pub fn linux_live(root: u32) -> Vec<Process> {
    // SAFETY: sysconf only reads the system's settings.
    let (page, ticks) = unsafe { (libc::sysconf(libc::_SC_PAGESIZE), libc::sysconf(libc::_SC_CLK_TCK)) };
    linux(Path::new("/proc"), root, page.max(1) as u64, ticks.max(1) as u64)
}

/// Bytes as people read them: "812 KB", "142 MB", "1.4 GB".
pub fn bytes(n: u64) -> String {
    const K: f64 = 1024.0;
    let n = n as f64;
    if n < K * K {
        format!("{:.0} KB", (n / K).ceil())
    } else if n < K * K * K {
        format!("{:.0} MB", n / K / K)
    } else {
        format!("{:.1} GB", n / K / K / K)
    }
}

/// A share of a core as a percent: "0%", "12%", "230%" for more than one.
pub fn percent(share: f32) -> String {
    format!("{:.0}%", (share * 100.0).max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(pid: u32, parent: u32, name: &str, memory: u64, cpu_ms: u64) -> Process {
        Process { pid, parent, name: name.into(), memory, cpu: Duration::from_millis(cpu_ms) }
    }

    #[test]
    fn a_tabs_programs_are_its_shell_and_everything_under_it() {
        let all = vec![p(1, 0, "launchd", 9, 0), p(40, 1, "flow", 9, 0), p(41, 40, "zsh", 3, 0), p(50, 41, "vim", 7, 0), p(51, 50, "rg", 2, 0), p(60, 40, "bash", 4, 0)];
        let pids: Vec<u32> = under(all.clone(), 41).iter().map(|p| p.pid).collect();
        assert_eq!(pids, [41, 50, 51]);
        assert!(under(all, 99).is_empty(), "a shell that's gone");
    }

    #[test]
    fn cpu_is_what_each_program_did_since_the_last_reading() {
        let t0 = Instant::now();
        let before = Reading { at: t0, processes: vec![p(41, 1, "zsh", 3 << 20, 100), p(50, 41, "vim", 7 << 20, 1000)] };
        // A second on: vim worked half of it, a new rg counts from now.
        let now = Reading { at: t0 + Duration::from_secs(1), processes: vec![p(41, 1, "zsh", 3 << 20, 100), p(50, 41, "vim", 8 << 20, 1500), p(51, 50, "rg", 1 << 20, 900)] };
        let use_ = programs(Some(&before), &now, 41);
        assert_eq!((use_.count, use_.memory), (3, 12 << 20));
        assert!((use_.cpu - 0.5).abs() < 0.01, "{}", use_.cpu);
        assert_eq!(use_.busiest.as_deref(), Some("vim"));
        // At the first reading nothing has a share yet; the largest is named.
        let first = programs(None, &now, 41);
        assert_eq!((first.cpu, first.busiest.as_deref()), (0.0, Some("vim")));
        // The shell alone names none.
        let alone = Reading { at: t0, processes: vec![p(41, 1, "zsh", 3, 0)] };
        assert_eq!(programs(None, &alone, 41).busiest, None);
    }

    #[test]
    fn linuxs_proc_is_read_with_names_that_hold_anything() {
        let dir = std::env::temp_dir().join(format!("flow-proc-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let stat = |pid: u32, rest: &str| {
            fs::create_dir_all(dir.join(pid.to_string())).unwrap();
            fs::write(dir.join(pid.to_string()).join("stat"), format!("{pid} {rest}\n")).unwrap();
        };
        // pid (comm) state ppid pgrp session tty tpgid flags minflt cminflt
        // majflt cmajflt utime stime cutime cstime priority nice threads
        // itrealvalue starttime vsize rss …
        stat(41, "(bash) S 40 41 41 34816 50 4194304 1 0 0 0 30 20 0 0 20 0 1 0 100 9000000 1000 0");
        stat(50, "(my (odd) prog) R 41 50 41 34816 50 4194304 1 0 0 0 250 50 0 0 20 0 1 0 200 9000000 2000 0");
        stat(60, "(sshd) S 1 60 60 0 -1 4194304 1 0 0 0 1 1 0 0 20 0 1 0 50 9000000 300 0");
        fs::create_dir_all(dir.join("self")).unwrap();
        let found = linux(&dir, 41, 4096, 100);
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(found.len(), 2);
        assert_eq!((found[0].name.as_str(), found[0].memory, found[0].cpu), ("bash", 1000 * 4096, Duration::from_millis(500)));
        assert_eq!((found[1].name.as_str(), found[1].parent, found[1].cpu), ("my (odd) prog", 41, Duration::from_secs(3)));
    }

    #[test]
    fn sizes_and_shares_read_as_people_write_them() {
        assert_eq!(bytes(812 * 1024), "812 KB");
        assert_eq!(bytes(142 << 20), "142 MB");
        assert_eq!(bytes(1503 << 20), "1.5 GB");
        assert_eq!((percent(0.0), percent(0.123), percent(2.3)), ("0%".to_string(), "12%".to_string(), "230%".to_string()));
    }
}
