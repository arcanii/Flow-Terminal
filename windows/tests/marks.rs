//! Each command typed at a prompt marks its prompt's line for the scrollbar
//! and Previous and Next Command (`marks.rs`): from cmd, PowerShell, Git
//! Bash and the bundled BusyBox through ConPTY, done or failed (cmd can't
//! say, so done). ConPTY repaints its whole screen after a resize, which
//! takes the marks still on the screen; those in the scrollback stay, and
//! the next command is marked again.
#![cfg(windows)]

use std::time::{Duration, Instant};

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::vte::ansi::{Color, Rgb};
use flow_core::session::{GridSize, HistoryNote, Session, Waker};
use flow_core::shell::{BashFlavor, ShellKind};
use flow_core::Platform;

fn wait_for(what: &str, timeout: Duration, mut f: impl FnMut() -> bool) {
    let start = Instant::now();
    while !f() {
        assert!(start.elapsed() < timeout, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Types `command` at the prompt a key at a time, as a person does, then
/// Enter, and waits for the prompt after it.
fn type_and_run(session: &Session, label: &str, command: &str) {
    wait_for(&format!("{label} prompt"), Duration::from_secs(10), || session.state.lock().at_prompt);
    session.state.lock().history.clear();
    for ch in command.chars() {
        session.write(ch.to_string().as_bytes());
        std::thread::sleep(Duration::from_millis(15));
    }
    let start = Instant::now();
    while !session.cursor_line().1.contains(command.trim()) {
        if start.elapsed() > Duration::from_secs(10) {
            let emu = session.emu.lock();
            let text = emu.text_between(0, emu.cursor_abs_line() + 1);
            let point = emu.term.grid().cursor.point;
            drop(emu);
            panic!("{label}: no echo of {command:?}; the cursor's line {:?} at {point:?}\n{text}", session.cursor_line());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    session.write(b"\r");
    wait_for(&format!("{label}: {command:?} done"), Duration::from_secs(20), || {
        session.state.lock().history.iter().any(|n| matches!(n, HistoryNote::Done(_)))
    });
    wait_for(&format!("{label}: the prompt after {command:?}"), Duration::from_secs(10), || session.state.lock().at_prompt);
}

/// Each mark from the scrollback's top: its line, what it says (its color,
/// as `marks.rs` writes it in the line's first cell), and the line's text.
fn marks(session: &Session) -> Vec<(usize, &'static str, String)> {
    let emu = session.emu.lock();
    let grid = emu.term.grid();
    let history = grid.history_size();
    let mut found = Vec::new();
    for abs in 0..history + grid.screen_lines() {
        let what = match grid[Line(abs as i32 - history as i32)][Column(0)].underline_color() {
            Some(Color::Spec(Rgb { r: 0xF1, g: 0x0C, b: 0xC1 })) => "running",
            Some(Color::Spec(Rgb { r: 0xF1, g: 0x0C, b: 0xC2 })) => "done",
            Some(Color::Spec(Rgb { r: 0xF1, g: 0x0C, b: 0xC3 })) => "failed",
            _ => continue,
        };
        found.push((abs, what, emu.text_between(abs as i64, abs as i64 + 1).trim_end().to_string()));
    }
    found
}

#[test]
fn commands_are_marked_from_each_shell_and_those_in_the_scrollback_outlast_a_resize() {
    flow_core::apps::first::set_root(std::env::temp_dir().join(format!("flow-marks-apps-{}", std::process::id())));
    let busybox = flow_windows::bundled_busybox().expect("windows/third_party/busybox/busybox64u.exe");
    let cell = (9.0, 18.0);
    for profile in flow_windows::PLATFORM.shells().into_iter().chain([busybox]) {
        if profile.flavor == BashFlavor::Wsl {
            continue;
        }
        let label = profile.label.clone();
        let kind = profile.kind;
        let mut session = Session::spawn(1, profile, GridSize { cols: 100, rows: 30 }, cell, 2000, Some(std::env::temp_dir()), Waker::from_fn(|_| {}))
            .unwrap_or_else(|e| panic!("{label}: {e:#}"));
        wait_for(&format!("{label} first prompt"), Duration::from_secs(30), || {
            let st = session.state.lock();
            st.integrated && st.at_prompt
        });
        // A command, one failing, forty lines of output that take the first
        // two into the scrollback, and one more.
        let (ok, failing, count) = match kind {
            ShellKind::Cmd => ("echo mark-one", "dir no-such-file-here", "for /l %i in (1,1,40) do @echo count-%i"),
            ShellKind::PowerShell => ("Write-Output mark-one", "cmd /c exit 3", "1..40 | % { \"count-$_\" }"),
            _ => ("echo mark-one", "(exit 3)", "seq 1 40 | sed 's/^/count-/'"),
        };
        for command in [ok, failing, count, ok] {
            type_and_run(&session, &label, command);
        }
        let before = marks(&session);
        let failed = if kind == ShellKind::Cmd { "done" } else { "failed" };
        assert_eq!(before.iter().map(|m| m.1).collect::<Vec<_>>(), ["done", failed, "done", "done"], "{label}: {before:?}");
        let history = session.emu.lock().term.grid().history_size();
        let kept: Vec<_> = before.iter().filter(|m| m.0 < history).map(|m| (m.1, m.2.clone())).collect();
        assert!(kept.len() >= 2, "{label}: the first two in the scrollback: {before:?}");

        for (cols, rows) in [(90, 30), (100, 30), (100, 40)] {
            let sent = Instant::now();
            session.resize(GridSize { cols, rows }, cell);
            wait_for(&format!("{label}: the repaint at {cols}x{rows}"), Duration::from_secs(10), || {
                let last = session.state.lock().last_output_at;
                last.is_some_and(|t| t > sent && t.elapsed() > Duration::from_millis(400))
            });
            let after: Vec<_> = marks(&session).into_iter().map(|m| (m.1, m.2)).collect();
            assert!(kept.iter().all(|m| after.contains(m)), "{label} at {cols}x{rows}: {kept:?} in {after:?}");
            eprintln!("{label} at {cols}x{rows}: {} of {} marks left, {} of them in the scrollback", after.len(), before.len(), kept.len());
        }
        // The next command is marked as before (Git Bash's prompt begins
        // with a blank line, which takes its mark). Git Bash drops the first
        // key typed after a resize, so a Ctrl+U goes first, which clears
        // nothing at an empty prompt.
        if kind == ShellKind::Bash {
            session.write(b"\x15");
            std::thread::sleep(Duration::from_millis(300));
        }
        let had = marks(&session).len();
        type_and_run(&session, &label, ok);
        let now = marks(&session);
        assert!(now.len() == had + 1 && now.last().is_some_and(|m| m.1 == "done"), "{label}: {now:?}");
    }
}
