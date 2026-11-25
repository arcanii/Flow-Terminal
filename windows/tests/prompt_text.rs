//! A theme's prompt is text, never shell code (`looks::prompt_for` and the
//! hooks): PowerShell, cmd, bash and the bundled BusyBox ash, each in a real
//! PTY (ConPTY), show every letter of one full of what shells expand as
//! itself, and run none of it. In a test binary of its own, since the
//! prompt is set for every shell started.
#![cfg(windows)]

use std::time::{Duration, Instant};

use flow_core::session::{GridSize, Session, Waker};
use flow_core::Platform;

fn wait_for(what: &str, mut f: impl FnMut() -> bool) {
    let start = Instant::now();
    while !f() {
        assert!(start.elapsed() < Duration::from_secs(20), "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Each would make a file in the tab's folder if a shell ran it (`ni` is
/// PowerShell's New-Item), or show something else if it expanded it. The
/// history number goes before it and nothing after, so the line reads the
/// same in every shell.
const PROMPT: &str = "{n}% $(touch ran-1) `touch ran-2` $(ni ran-3) ${HOME} $((6*7)) %PATH% !! ! \\ \\$ \" 'q' END\n";

#[test]
fn a_themes_prompt_shows_as_written_and_runs_nothing_in_any_shell() {
    let dir = std::env::temp_dir().join(format!("flow-prompt-text-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // The tabs' panes keep their folders in an apps' folder of the test's
    // own, never the user's.
    flow_core::apps::first::set_root(dir.join("apps"));
    flow_core::shell::set_prompt(Some(PROMPT));
    let shown = PROMPT.split_once("{n}").map_or(PROMPT, |(_, rest)| rest).trim_end();
    let busybox = flow_windows::bundled_busybox().expect("windows/third_party/busybox/busybox64u.exe");
    for (n, profile) in flow_windows::PLATFORM.shells().into_iter().chain([busybox]).enumerate() {
        let label = profile.label.clone();
        let session = Session::spawn(30 + n as u32, profile, GridSize { cols: 120, rows: 20 }, (9.0, 18.0), 2000, Some(dir.clone()), Waker::from_fn(|_| {}))
            .unwrap_or_else(|e| panic!("{label}: {e:#}"));
        wait_for(&format!("{label} first prompt"), || {
            let st = session.state.lock();
            st.integrated && st.at_prompt
        });
        // The prompt's line is the one above the cursor's.
        let above = |s: &Session| {
            let emu = s.emu.lock();
            let line = emu.cursor_abs_line();
            emu.text_between(line - 1, line)
        };
        let start = Instant::now();
        while !above(&session).contains(shown) {
            assert!(start.elapsed() < Duration::from_secs(20), "{label}: no {shown:?} in {:?}", above(&session));
            std::thread::sleep(Duration::from_millis(50));
        }
        for ran in ["ran-1", "ran-2", "ran-3"] {
            assert!(!dir.join(ran).exists(), "{label}: the prompt ran {ran}");
        }
        eprintln!("{label}: {:?} then the cursor", above(&session).trim_end());
    }
    flow_core::shell::set_prompt(None);
    let _ = std::fs::remove_dir_all(&dir);
}
