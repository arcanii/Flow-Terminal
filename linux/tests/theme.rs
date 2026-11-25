//! A prompt theme sets the prompt afresh before each one (zsh's promptinit
//! themes in `precmd`, bash's in `PROMPT_COMMAND`), after Flow's hook has
//! marked it once. zsh and bash in a real PTY, with startup files that do
//! that, should still mark every prompt, so commands are kept as typed and
//! their exit codes read, an old machine's look should still give its
//! prompt, and a click on the line typed at the prompt should move the
//! cursor there. In a test binary of its own, since it points HOME at those
//! files.
#![cfg(all(unix, not(target_os = "macos")))]

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

/// Waits for the cursor's line to hold `want`, and says what it held if not.
fn wait_for_line(session: &Session, label: &str, want: &str) {
    let start = Instant::now();
    while !(session.state.lock().at_prompt && session.cursor_line().1.contains(want)) {
        assert!(start.elapsed() < Duration::from_secs(20), "{label}: no {want:?} in {:?}", session.cursor_line().1);
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Types `command` a key at a time, presses Enter, and returns what the
/// session noted for the history.
fn type_and_run(session: &Session, label: &str, command: &str) -> Vec<HistoryNote> {
    wait_for(&format!("{label} prompt"), || session.state.lock().at_prompt);
    session.state.lock().history.clear();
    for ch in command.chars() {
        session.write(ch.to_string().as_bytes());
        std::thread::sleep(Duration::from_millis(15));
    }
    wait_for(&format!("{label}: the echo of {command:?}"), || session.cursor_line().1.contains(command));
    session.write(b"\r");
    wait_for(&format!("{label}: {command:?} done"), || session.state.lock().history.iter().any(|n| matches!(n, HistoryNote::Done(_))));
    std::mem::take(&mut session.state.lock().history)
}

/// Types `echo flow-click-ab`, clicks on its `b` (and first on the line
/// above, which does nothing), types `X` and Enter, and waits for
/// `flow-click-aXb`, as `tests/shells.rs` does.
fn click_moves_the_cursor(session: &Session, label: &str) {
    use alacritty_terminal::index::{Column, Point};
    for ch in "echo flow-click-ab".chars() {
        session.write(ch.to_string().as_bytes());
        std::thread::sleep(Duration::from_millis(15));
    }
    wait_for(&format!("{label}: the echo"), || session.cursor_line().1.contains("flow-click-ab"));
    let target = {
        let emu = session.emu.lock();
        let grid = emu.term.grid();
        let line = grid.cursor.point.line;
        let row = &grid[line];
        let col = (1..grid.cursor.point.column.0).rev().find(|&c| row[Column(c - 1)].c == 'a' && row[Column(c)].c == 'b').unwrap_or_else(|| panic!("{label}: no ab"));
        Point::new(line, Column(col))
    };
    assert!(!session.click_to_move(Point::new(target.line - 1, target.column)), "{label}: a line above isn't the prompt's");
    assert!(session.click_to_move(target), "{label}");
    session.write(b"X\r");
    wait_for(&format!("{label}: the line run"), || session.emu.lock().recent_text(30).lines().any(|l| l.trim() == "flow-click-aXb"));
}

/// Whether `bash` runs each command of an array PROMPT_COMMAND (5.1 on).
fn runs_prompt_command_arrays(bash: &std::path::Path) -> bool {
    let out = std::process::Command::new(bash).args(["-c", "echo ${BASH_VERSINFO[0]} ${BASH_VERSINFO[1]}"]).output();
    let text = out.map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
    let v: Vec<u32> = text.split_whitespace().filter_map(|n| n.parse().ok()).collect();
    matches!(v[..], [major, minor] if (major, minor) >= (5, 1))
}

#[test]
fn a_theme_that_sets_the_prompt_each_time_still_gets_it_marked() {
    let home = std::env::temp_dir().join(format!("flow-theme-test-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    // With nounset, as some set it: Flow's hooks mustn't stumble on a
    // variable of theirs that isn't set.
    let themed_zshrc = "setopt nounset\nprecmd() { PS1='themed-%h%# ' }\n";
    // A theme of zsh's own, as Ubuntu's zshrc chooses it, which sets PS1 in
    // a precmd it adds with add-zsh-hook.
    let adam1_zshrc = "autoload -Uz promptinit\npromptinit\nprompt adam1\n";
    // A string ending in "; ", as `PROMPT_COMMAND="history -a; $PROMPT_COMMAND"`
    // leaves it; unset first, since Ubuntu's /etc/profile.d makes it an array.
    let string_rc = "__theme() { PS1='themed-\\#\\$ '; }\nunset PROMPT_COMMAND\nPROMPT_COMMAND='__theme; true; '\nset -u\n";
    // bash 5.1's form, after a command that's there already, as Ubuntu's
    // /etc/profile.d leaves it.
    let array_rc = "__theme() { PS1='themed-\\#\\$ '; }\nPROMPT_COMMAND=(true)\nPROMPT_COMMAND+=(__theme)\n";
    std::env::set_var("HOME", &home);
    // None of what the terminal running the tests may have set.
    for name in ["ZDOTDIR", "XDG_CONFIG_HOME", "FLOW_PROMPT", "FLOW_SKIP_PROFILE"] {
        std::env::remove_var(name);
    }
    let platform = &flow_linux::PLATFORM;
    flow_core::shell::set_shell_env(platform.shell_env());

    let passes = [
        (None, "themed", string_rc, themed_zshrc),
        (None, "themed, an array", array_rc, themed_zshrc),
        (Some("{n} OK> "), "with a look", string_rc, themed_zshrc),
        (None, "promptinit's adam1", string_rc, adam1_zshrc),
    ];
    for (n, (look, what, bashrc, zshrc)) in passes.into_iter().enumerate() {
        std::fs::write(home.join(".bashrc"), bashrc).unwrap();
        std::fs::write(home.join(".zshrc"), zshrc).unwrap();
        flow_core::shell::set_prompt(look);
        for profile in platform.shells().into_iter().filter(|p| p.id == "zsh" || p.id == "bash") {
            if bashrc == array_rc && (profile.id != "bash" || !runs_prompt_command_arrays(&profile.program)) {
                continue;
            }
            if zshrc == adam1_zshrc && profile.id != "zsh" {
                continue;
            }
            let label = format!("{} {what}", profile.label);
            let session = Session::spawn(10 + n as u32, profile, GridSize { cols: 100, rows: 20 }, (9.0, 18.0), 2000, None, Waker::from_fn(|_| {}))
                .unwrap_or_else(|e| panic!("{label}: {e:#}"));
            wait_for(&format!("{label} first prompt"), || {
                let st = session.state.lock();
                st.integrated && st.at_prompt
            });
            let want = match (look, zshrc == adam1_zshrc) {
                (Some(_), _) => " OK>",
                (None, true) => "%",
                (None, false) => "themed-",
            };
            wait_for_line(&session, &label, want);
            // Twice, since the theme sets the prompt again for the second.
            for (command, code) in [("echo flow-theme-1", 0), ("(exit 5)", 5)] {
                let notes = type_and_run(&session, &label, command);
                assert!(
                    notes.iter().any(|n| matches!(n, HistoryNote::Entered { command: c, .. } if c == command)),
                    "{label}: {notes:?}"
                );
                assert_eq!(notes.last(), Some(&HistoryNote::Done(Some(code))), "{label}: {notes:?}");
            }
            wait_for_line(&session, &label, want);
            // A click on the line typed at the prompt goes by where the
            // prompt's end mark is, which Flow's hook puts on after the
            // theme has set the prompt: the cursor moves there, and a click
            // on the line above does nothing.
            click_moves_the_cursor(&session, &label);
            wait_for_line(&session, &label, want);
            let screen = {
                let emu = session.emu.lock();
                emu.text_between(0, emu.cursor_abs_line() + 1)
            };
            assert!(!screen.contains("unbound variable") && !screen.contains("parameter not set"), "{label}: {screen}");
            eprintln!("{label}: {:?}", session.cursor_line().1.trim_end());
        }
    }
    flow_core::shell::set_prompt(None);
    let _ = std::fs::remove_dir_all(&home);
}
