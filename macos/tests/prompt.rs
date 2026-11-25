//! An old machine's look gives the shells its prompt (`shell::set_prompt`,
//! taken from `FLOW_PROMPT` by the zsh and bash hooks): zsh and bash in a
//! real PTY show it, and still mark their prompts, so commands are timed,
//! their exit codes read and their lines kept for the history. In a test
//! binary of its own, since the prompt is set for every shell started.
#![cfg(target_os = "macos")]

use std::time::{Duration, Instant};

use flow_core::session::{GridSize, HistoryNote, Session, Waker};
use flow_core::Platform;

fn wait_for(what: &str, mut f: impl FnMut() -> bool) {
    let start = Instant::now();
    while !f() {
        assert!(start.elapsed() < Duration::from_secs(20), "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn a_looks_prompt_shows_and_the_shell_still_marks_it() {
    let platform = &flow_macos::PLATFORM;
    flow_core::shell::set_shell_env(platform.shell_env());
    // An apps' folder of the test's own, never the user's, which a tab
    // opened there would change (its pane's folder, and an older Flow's
    // `flowapp first` links brought into a first.json).
    flow_core::apps::first::set_root(std::env::temp_dir().join(format!("flow-prompt-apps-{}", std::process::id())));
    flow_core::shell::set_prompt(Some("{n}% READY.\n"));
    for profile in platform.shells() {
        let label = profile.label.clone();
        let session = Session::spawn(4, profile, GridSize { cols: 100, rows: 20 }, (9.0, 18.0), 2000, None, Waker::from_fn(|_| {}))
            .unwrap_or_else(|e| panic!("{label}: {e:#}"));
        wait_for(&format!("{label} first prompt"), || {
            let st = session.state.lock();
            st.integrated && st.at_prompt
        });
        // The prompt's first line, the history number then READY., is the
        // line above the cursor's.
        let above = |s: &Session| {
            let emu = s.emu.lock();
            let line = emu.cursor_abs_line();
            emu.text_between(line - 1, line)
        };
        wait_for(&format!("{label}: READY."), || above(&session).contains("% READY."));
        let first = above(&session);
        assert!(first.trim_start().chars().next().is_some_and(|c| c.is_ascii_digit()), "{label}: {first:?}");

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
        assert_eq!(notes.last(), Some(&HistoryNote::Done(Some(0))), "{label}: {notes:?}");
        wait_for(&format!("{label}: READY. again"), || session.state.lock().at_prompt && above(&session).contains("% READY."));
        eprintln!("{label}: {:?} then the cursor", above(&session).trim_end());
    }
    flow_core::shell::set_prompt(None);
}
