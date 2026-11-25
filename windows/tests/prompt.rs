//! An old machine's look gives the shells its prompt (`shell::set_prompt`):
//! PowerShell's, bash's and BusyBox ash's hooks take it from `FLOW_PROMPT`,
//! and cmd has it in its `PROMPT`. Each in a real PTY (ConPTY) shows it, its
//! folder and backslashes too (which BusyBox would turn into slashes), and
//! still marks it, so commands are timed, their exit codes read and their
//! lines kept for the history. In a test binary of its own, since the prompt
//! is set for every shell started.
#![cfg(windows)]

use std::time::{Duration, Instant};

use flow_core::session::{GridSize, HistoryNote, Session, Waker};
use flow_core::shell::{BashFlavor, ShellKind};
use flow_core::Platform;

fn wait_for(what: &str, mut f: impl FnMut() -> bool) {
    let start = Instant::now();
    while !f() {
        assert!(start.elapsed() < Duration::from_secs(20), "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Has the tabs opened here keep their panes' folders (and any `flowapp
/// first` record) in an apps' folder of the tests' own, never the user's,
/// which a tab opened there would change. Each test calls it before its
/// first shell; the folder is the process's, so they all name the same.
fn own_apps() {
    flow_core::apps::first::set_root(std::env::temp_dir().join(format!("flow-prompt-apps-{}", std::process::id())));
}

#[test]
fn a_looks_prompt_shows_and_each_shell_still_marks_it() {
    own_apps();
    flow_core::shell::set_prompt(Some("{n}% READY. {cwd} C:\\>\n"));
    let busybox = flow_windows::bundled_busybox().expect("windows/third_party/busybox/busybox64u.exe");
    for profile in flow_windows::PLATFORM.shells().into_iter().chain([busybox]) {
        let label = profile.label.clone();
        let (kind, flavor) = (profile.kind, profile.flavor);
        let session = Session::spawn(4, profile, GridSize { cols: 100, rows: 20 }, (9.0, 18.0), 2000, None, Waker::from_fn(|_| {}))
            .unwrap_or_else(|e| panic!("{label}: {e:#}"));
        wait_for(&format!("{label} first prompt"), || {
            let st = session.state.lock();
            st.integrated && st.at_prompt
        });
        // The prompt's first line, the history number (cmd and BusyBox have
        // none), READY., the folder and the backslash, is the line above the
        // cursor's.
        let above = |s: &Session| {
            let emu = s.emu.lock();
            let line = emu.cursor_abs_line();
            emu.text_between(line - 1, line)
        };
        wait_for(&format!("{label}: READY."), || above(&session).contains("% READY."));
        let first = above(&session);
        let numbered = first.trim_start().chars().next().is_some_and(|c| c.is_ascii_digit());
        assert_eq!(numbered, !(kind == ShellKind::Cmd || flavor == BashFlavor::BusyBox), "{label}: {first:?}");
        let folder = first.split_once("% READY. ").and_then(|(_, rest)| rest.trim_end().strip_suffix(" C:\\>"));
        assert!(folder.is_some_and(|f| !f.is_empty() && f != "\\w" && f != "/w"), "{label}: {first:?}");

        // A command typed a key at a time still runs, ends and is kept.
        session.state.lock().history.clear();
        for ch in "echo flow-prompt-ok".chars() {
            session.write(ch.to_string().as_bytes());
            std::thread::sleep(Duration::from_millis(15));
        }
        wait_for(&format!("{label}: the echo"), || session.cursor_line().1.contains("echo flow-prompt-ok"));
        session.write(b"\r");
        wait_for(&format!("{label}: done"), || session.state.lock().history.iter().any(|n| matches!(n, HistoryNote::Done(_))));
        let notes = std::mem::take(&mut session.state.lock().history);
        assert!(notes.iter().any(|n| matches!(n, HistoryNote::Entered { command, .. } if command == "echo flow-prompt-ok")), "{label}: {notes:?}");
        let ok = if kind == ShellKind::Cmd { None } else { Some(0) };
        assert_eq!(notes.last(), Some(&HistoryNote::Done(ok)), "{label}: {notes:?}");
        wait_for(&format!("{label}: READY. again"), || session.state.lock().at_prompt && above(&session).contains("% READY."));
        eprintln!("{label}: {:?} then the cursor", above(&session).trim_end());
    }
    flow_core::shell::set_prompt(None);
}
