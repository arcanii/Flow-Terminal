//! Spawns the Windows shells in a real PTY (ConPTY), runs commands the way the
//! assistant does, and checks output capture, exit codes, timeouts and the
//! reported folder through Flow's shell-integration markers, what each notes
//! for the command history as a person types (none of it a program's output
//! that only looks like the hook's marks), the games' folder on each
//! shell's PATH, the screen after a resize, the startup logo through ConPTY's
//! repaints, a start screen through the window settling as Flow starts, a
//! full-screen program going back to the main screen, pictures sent down
//! a tab's pipe for images, a command Flow types clearing what's
//! half-typed, a PowerShell one beginning with `[` arriving whole, and
//! (ignored while it fails) a long line with Devanagari's marks recalled.
#![cfg(windows)]

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::TermMode;
use alacritty_terminal::vte::ansi::{Color, Rgb};
use flow_core::ai::tools::resolve_path;
use flow_core::session::{CommandResult, GridSize, HistoryNote, Image, Session, Waker};
use flow_core::shell::{BashFlavor, ShellKind, ShellProfile};
use flow_core::Platform;

fn wait_for(what: &str, timeout: Duration, mut f: impl FnMut() -> bool) {
    let start = Instant::now();
    while !f() {
        assert!(start.elapsed() < timeout, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Types `command` at the prompt and waits for its result.
fn run(session: &Session, label: &str, command: &str) -> CommandResult {
    wait_for(&format!("{label} prompt"), Duration::from_secs(10), || session.state.lock().at_prompt);
    session.run_command(command);
    let mut result = None;
    wait_for(&format!("{label}: {command}"), Duration::from_secs(20), || {
        result = session.poll_command(Duration::from_secs(20));
        result.is_some()
    });
    result.unwrap()
}

/// Has the tabs opened here keep their panes' folders (and any `flowapp
/// first` record) in an apps' folder of the tests' own, never the user's,
/// which a tab opened there would change. Each test calls it before its
/// first shell; the folder is the process's, so they all name the same.
fn own_apps() {
    flow_core::apps::first::set_root(std::env::temp_dir().join(format!("flow-shells-apps-{}", std::process::id())));
}

/// Spawns each installed shell and checks it with [`check_shell`].
#[test]
fn shells_report_output_and_exit_codes() {
    own_apps();
    for profile in flow_windows::PLATFORM.shells() {
        check_shell(profile, None);
    }
}

/// The bundled BusyBox gets the same checks even where a real bash is
/// installed, and must report the folder it starts in and moves to. It also
/// stands in for every shell in checking how a command that outlasts its
/// timeout is reported.
#[test]
fn bundled_busybox_reports_output_exit_codes_cwd_and_timeouts() {
    own_apps();
    let canon = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
    let profile = flow_windows::bundled_busybox().expect("windows/third_party/busybox/busybox64u.exe");
    let start = std::env::temp_dir();
    let session = check_shell(profile, Some(start.clone()));
    let reported = |s: &Session| resolve_path(&s.cwd().unwrap_or_default(), None);
    assert_eq!(canon(&reported(&session)), canon(&start));

    let target = start.join("flow-busybox-cwd-test");
    std::fs::create_dir_all(&target).unwrap();
    session.run_command(&format!("cd '{}'", target.to_string_lossy().replace('\\', "/")));
    wait_for("busybox cd", Duration::from_secs(20), || {
        session.state.lock().at_prompt && reported(&session).is_dir() && canon(&reported(&session)) == canon(&target)
    });
    let _ = std::fs::remove_dir(&target);

    // The timeout counts from when the command was sent, so a command that
    // outlasts it is reported as still running only once it has passed.
    let timeout = Duration::from_millis(1500);
    let sent = Instant::now();
    session.run_command("sleep 30");
    let mut result = None;
    wait_for("busybox timeout", Duration::from_secs(10), || {
        result = session.poll_command(timeout);
        result.is_some()
    });
    assert!(sent.elapsed() >= timeout, "reported after {:?}", sent.elapsed());
    assert!(result.unwrap().still_running);
}

/// Types `command` at the prompt a key at a time, as a person does, waits
/// for its echo, then presses Enter and waits for the shell to finish it.
fn type_and_run(session: &Session, label: &str, command: &str) -> Vec<HistoryNote> {
    wait_for(&format!("{label} prompt"), Duration::from_secs(10), || session.state.lock().at_prompt);
    session.state.lock().history.clear();
    for ch in command.chars() {
        session.write(ch.to_string().as_bytes());
        std::thread::sleep(Duration::from_millis(15));
    }
    wait_for(&format!("{label}: the echo of {command:?}"), Duration::from_secs(10), || session.cursor_line().1.contains(command.trim()));
    session.write(b"\r");
    wait_for(&format!("{label}: {command:?} done"), Duration::from_secs(20), || {
        session.state.lock().history.iter().any(|n| matches!(n, HistoryNote::Done(_)))
    });
    std::mem::take(&mut session.state.lock().history)
}

/// A program printing what looks like the hook's word for the command line
/// (OSC 633;E), without the pane's nonce or with another, names nothing:
/// the hook's own word for the line typed stands, in the history and for
/// the notification. Where the hook says its marks with the nonce
/// (`own_line`: PowerShell's and bash's), nor does a prompt a program draws
/// while it runs (133;A, 133;B), and its "done" (133;D): the answer typed
/// at its question isn't a command, and the line typed ends when the shell
/// says. No shell's programs have the nonce, cmd's and BusyBox's neither.
fn lines_a_program_says_name_nothing(session: &Session, label: &str, kind: ShellKind, own_line: bool) {
    let (forged, prompt, unset) = match kind {
        ShellKind::Cmd => (None, None, "if defined FLOW_NONCE (echo has) else (echo flow-nonce-4^3)"),
        // ESC and BEL in variables of the prompt's, so the lines typed fit
        // on one of the screen's.
        ShellKind::PowerShell => (
            Some(r#"Write-Host -NoNewline "$e]633;E;flow-forged$b$e]633;E;flow-forged;00$b""#),
            Some(r#"Write-Host -NoNewline "$e]133;A$b$e]133;B${b}flow-forged-prompt $e]133;D;7$b"; $x=Read-Host; cmd /c exit 1"#),
            r#"if ($env:FLOW_NONCE) { 'has' } else { "flow-nonce-$(40+3)" }"#,
        ),
        _ => (
            Some(r"printf '\033]633;E;flow-forged\007\033]633;E;flow-forged;00\007'"),
            Some(r"printf '\033]133;A\007\033]133;B\007flow-forged-prompt \033]133;D;7\007'; read x; false"),
            r"env | grep -E '^(FLOW_NONCE|__flow_nonce)=' || echo flow-nonce-$((40 + 3))",
        ),
    };
    if kind == ShellKind::PowerShell {
        run(session, label, "$e = [char]27; $b = [char]7");
    }
    if let Some(forged) = forged {
        let notes = type_and_run(session, label, forged);
        assert_eq!(notes.contains(&HistoryNote::Exact(forged.into())), own_line, "{label}: {notes:?}");
        assert!(!notes.iter().any(|n| matches!(n, HistoryNote::Exact(c) if c.starts_with("flow-forged"))), "{label}: {notes:?}");
        let finished = session.state.lock().finished.clone().and_then(|f| f.command);
        assert!(!finished.as_deref().is_some_and(|c| c.starts_with("flow-forged")), "{label}: {finished:?}");
    }
    if let Some(prompt) = prompt.filter(|_| own_line) {
        let notes = type_and_answer(session, label, prompt, "flow-forged-prompt", "y");
        assert!(!notes.iter().any(|n| matches!(n, HistoryNote::Entered { command, .. } | HistoryNote::Exact(command) if command.starts_with("flow-forged"))), "{label}: {notes:?}");
        assert_eq!(notes.iter().filter(|n| matches!(n, HistoryNote::Done(_))).collect::<Vec<_>>(), [&HistoryNote::Done(Some(1))], "{label}: {notes:?}");
        let (finished, failed) = {
            let st = session.state.lock();
            (st.finished.clone().and_then(|f| f.command), st.failed.clone().map(|f| f.command))
        };
        assert_eq!((finished.as_deref(), failed.as_deref()), (Some(prompt), Some(prompt)), "{label}: the notification's and Ask why's");
    }
    let output = run(session, label, unset).output;
    assert!(output.contains("flow-nonce-43"), "{label}: {output:?}");
}

/// Types `command` at the prompt as `type_and_run` does, waits for `shown`
/// on a line below it, then types `answer` and Enter to it, and waits for
/// the shell to finish the command.
fn type_and_answer(session: &Session, label: &str, command: &str, shown: &str, answer: &str) -> Vec<HistoryNote> {
    wait_for(&format!("{label} prompt"), Duration::from_secs(10), || session.state.lock().at_prompt);
    session.state.lock().history.clear();
    for ch in command.chars() {
        session.write(ch.to_string().as_bytes());
        std::thread::sleep(Duration::from_millis(15));
    }
    // Whole, however the line editor and ConPTY wrapped it.
    wait_for(&format!("{label}: the echo of {command:?}"), Duration::from_secs(10), || session.emu.lock().recent_text(4).replace('\n', "").contains(command));
    let typed_on = session.cursor_line().0;
    session.write(b"\r");
    wait_for(&format!("{label}: {shown:?}"), Duration::from_secs(10), || {
        let (line, text) = session.cursor_line();
        line > typed_on && text.contains(shown)
    });
    assert!(!session.state.lock().at_prompt, "{label}: at a prompt the program drew");
    for ch in answer.chars() {
        session.write(ch.to_string().as_bytes());
        std::thread::sleep(Duration::from_millis(15));
    }
    session.write(b"\r");
    wait_for(&format!("{label}: {command:?} done"), Duration::from_secs(20), || {
        session.state.lock().history.iter().any(|n| matches!(n, HistoryNote::Done(_)))
    });
    std::mem::take(&mut session.state.lock().history)
}

/// What each shell, the bundled BusyBox too, notes for the command history as
/// a person types: the line as the grid shows it, the folder it ran in, and
/// how it ended (cmd can't say). PowerShell and bash also send the line itself
/// (OSC 633;E), which beats the grid's; cmd and BusyBox's ash don't.
#[test]
fn shells_note_each_command_for_the_history() {
    own_apps();
    let busybox = flow_windows::bundled_busybox().expect("windows/third_party/busybox/busybox64u.exe");
    for profile in flow_windows::PLATFORM.shells().into_iter().chain([busybox]) {
        let label = profile.label.clone();
        let kind = profile.kind;
        let own_line = kind == ShellKind::PowerShell || (kind == ShellKind::Bash && profile.flavor != BashFlavor::BusyBox);
        let session = Session::spawn(2, profile, GridSize { cols: 120, rows: 30 }, (9.0, 18.0), 2000, None, Waker::from_fn(|_| {}))
            .unwrap_or_else(|e| panic!("{label}: {e:#}"));
        wait_for(&format!("{label} first prompt"), Duration::from_secs(30), || {
            let st = session.state.lock();
            st.integrated && st.at_prompt
        });
        let cwd = session.cwd();

        // Not `echo` in PowerShell, which a profile may turn into a program's
        // path before it runs, and so in what PowerShell's history keeps.
        let (line, failing, quiet) = match kind {
            ShellKind::Cmd => ("echo flow-history-1 & echo two", None, " echo flow-history-quiet"),
            ShellKind::PowerShell => ("Write-Output flow-history-1; $null", Some("cmd /c exit 4"), " Write-Output flow-history-quiet"),
            _ => ("echo flow-history-1; true", Some("(exit 4)"), " echo flow-history-quiet"),
        };
        let notes = type_and_run(&session, &label, line);
        let entered = notes.iter().find_map(|n| match n {
            HistoryNote::Entered { command, cwd, assistant, .. } => Some((command.clone(), cwd.clone(), *assistant)),
            _ => None,
        });
        assert_eq!(entered, Some((line.to_string(), cwd.clone(), false)), "{label}: {notes:?}");
        assert_eq!(notes.contains(&HistoryNote::Exact(line.into())), own_line, "{label}: {notes:?}");
        let ok = if kind == ShellKind::Cmd { None } else { Some(0) };
        assert_eq!(notes.last(), Some(&HistoryNote::Done(ok)), "{label}: {notes:?}");
        lines_a_program_says_name_nothing(&session, &label, kind, own_line);
        if let Some(failing) = failing {
            let notes = type_and_run(&session, &label, failing);
            assert_eq!(notes.last(), Some(&HistoryNote::Done(Some(4))), "{label}: {notes:?}");
        }

        // A line that starts with a space keeps it, so the history leaves it out.
        let notes = type_and_run(&session, &label, quiet);
        for n in &notes {
            if let HistoryNote::Entered { command, .. } | HistoryNote::Exact(command) = n {
                assert!(command.starts_with(' '), "{label}: {notes:?}");
            }
        }

        // The assistant's command, sent whole.
        session.state.lock().history.clear();
        run(&session, &label, "echo flow-history-2");
        let notes = std::mem::take(&mut session.state.lock().history);
        assert!(
            notes.iter().any(|n| matches!(n, HistoryNote::Entered { command, assistant: true, .. } if command == "echo flow-history-2")),
            "{label}: {notes:?}"
        );
        eprintln!("{label}: history notes ok");
    }
}

/// Every shell, the bundled BusyBox too, finds the games that come with Flow
/// after the user's startup files, and has their folder on its PATH once:
/// spelled as that shell spells its PATH, so the hooks don't add it again.
#[test]
fn every_shell_finds_the_games_once_on_its_path() {
    own_apps();
    let busybox = flow_windows::bundled_busybox().expect("windows/third_party/busybox/busybox64u.exe");
    // Not under %TEMP%, which Git Bash also calls /tmp: its PATH would hold
    // the folder under that name, and the hook's under the drive's.
    let games = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("flow-games-test-{}", std::process::id()));
    std::fs::create_dir_all(&games).unwrap();
    // Any program stands in for the game: the shells only have to find it.
    std::fs::copy(&busybox.program, games.join("flow-games-probe.exe")).unwrap();
    flow_core::shell::set_games_dir(games.clone());
    // In what the shells print, not in the commands they're sent.
    let folder = games.file_name().unwrap().to_string_lossy().into_owned();

    for profile in flow_windows::PLATFORM.shells().into_iter().chain([busybox]) {
        // WSL sees Windows' PATH through its own interop, as /mnt/c.
        if profile.flavor == BashFlavor::Wsl {
            continue;
        }
        let label = profile.label.clone();
        let (find, path) = match profile.kind {
            ShellKind::Cmd => ("where flow-games-probe", "echo %PATH%"),
            ShellKind::PowerShell => ("(Get-Command flow-games-probe).Source", "$env:Path"),
            _ => ("command -v flow-games-probe", r#"echo "$PATH""#),
        };
        let session = Session::spawn(1, profile, GridSize { cols: 120, rows: 30 }, (9.0, 18.0), 2000, None, Waker::from_fn(|_| {}))
            .unwrap_or_else(|e| panic!("{label}: {e:#}"));
        wait_for(&format!("{label} prompt"), Duration::from_secs(30), || {
            let st = session.state.lock();
            st.integrated && st.at_prompt
        });
        let found = run(&session, &label, find).output;
        assert!(found.contains(&folder), "{label}: {found:?}");
        let output = run(&session, &label, path).output;
        assert_eq!(output.matches(&folder).count(), 1, "{label}: {output:?}");
        eprintln!("{label}: finds the games");
    }
    let _ = std::fs::remove_dir_all(&games);
}

/// ConPTY repaints its whole screen after a resize, keeping its top row where
/// it was. Flow's grid has to agree, or a line that wraps shows twice and the
/// scrollback loses the lines the repaint writes over.
#[test]
fn resizing_shows_each_line_once_and_keeps_the_scrollback() {
    own_apps();
    let profile = flow_windows::bundled_busybox().expect("windows/third_party/busybox/busybox64u.exe");
    let cell = (9.0, 18.0);
    let mut session = Session::spawn(1, profile, GridSize { cols: 100, rows: 30 }, cell, 2000, Some(std::env::temp_dir()), Waker::from_fn(|_| {}))
        .unwrap();
    wait_for("busybox prompt", Duration::from_secs(30), || {
        let st = session.state.lock();
        st.integrated && st.at_prompt
    });

    // Waits for ConPTY's repaint to come and finish.
    let resize = |session: &mut Session, cols: usize, rows: usize| {
        let sent = Instant::now();
        session.resize(GridSize { cols, rows }, cell);
        wait_for(&format!("the repaint at {cols}x{rows}"), Duration::from_secs(10), || {
            let last = session.state.lock().last_output_at;
            last.is_some_and(|t| t > sent && t.elapsed() > Duration::from_millis(400))
        });
    };
    // The terminal's text from the top of the scrollback, a line per line
    // however it's wrapped.
    let text = |session: &Session| {
        let emu = session.emu.lock();
        emu.text_between(0, emu.cursor_abs_line() + 100)
    };

    // A line of 95 columns under a short command at the top of the screen:
    // one row at 100, two at 90. The command and the line must show once.
    let echo = r#"echo "dup-$x-end""#;
    run(&session, "busybox", r#"x=$(awk 'BEGIN { s = sprintf("%88s", ""); gsub(/ /, "x", s); print s }')"#);
    run(&session, "busybox", "clear");
    run(&session, "busybox", echo);
    resize(&mut session, 90, 30);
    let once = |text: &str, what: &str| assert_eq!(text.matches(what).count(), 1, "{what:?} in:\n{text}");
    once(&text(&session), echo);
    once(&text(&session), "dup-x");

    // Forty 94-column lines fill the screen and the scrollback above it.
    run(&session, "busybox", r#"awk 'BEGIN { s = sprintf("%85s", ""); gsub(/ /, "x", s); for (i = 1; i <= 40; i++) printf "line-%03d-%s\n", i, s }'"#);
    for (cols, rows) in [(100, 30), (100, 40), (90, 40), (100, 30)] {
        resize(&mut session, cols, rows);
        let text = text(&session);
        let numbers: Vec<usize> = text.lines().filter_map(|l| l.strip_prefix("line-")?.get(..3)?.parse().ok()).collect();
        assert_eq!(numbers, (1..=40).collect::<Vec<_>>(), "at {cols}x{rows}:\n{text}");
        once(&text, echo);
        once(&text, "dup-x");
    }
}

/// Flow draws the startup logo itself, so ConPTY doesn't know it's there and
/// blanks its rows when it repaints the screen from the top: after a program
/// that drew there leaves the alternate screen, and after each resize from
/// then on. The logo has to outlast those, and still go with `clear`.
#[test]
fn the_startup_logo_outlasts_full_screen_programs_and_resizes() {
    own_apps();
    let profile = flow_windows::bundled_busybox().expect("windows/third_party/busybox/busybox64u.exe");
    let cell = (9.0, 18.0);
    let mut session = Session::launch(1, profile, GridSize { cols: 100, rows: 30 }, cell, 2000, Some(std::env::temp_dir()), Waker::from_fn(|_| {}))
        .unwrap();
    // Direct3D 12's logo, DirectX's wordmark, after the last line, as
    // `App::banner` gives it.
    let api = flow_core::logos::api("Direct3D 12").map(|a| a.in_ink([0xE4, 0xE9, 0xF2]));
    let api_cols = api.as_ref().map(|a| a.in_row(cell).1 as usize).expect("DirectX's wordmark");
    session.show_banner(Image::new(2, 2, vec![255; 16]), &["flow-logo-line".into(), "BusyBox ·".into()], api);
    session.start();
    wait_for("busybox prompt", Duration::from_secs(30), || {
        let st = session.state.lock();
        st.integrated && st.at_prompt
    });
    // Whether the logo's line is on screen, once ConPTY has gone quiet, and
    // the API's logo after the last line, whole.
    let shown = |session: &Session| {
        wait_for("ConPTY to go quiet", Duration::from_secs(10), || {
            session.state.lock().last_output_at.is_some_and(|t| t.elapsed() > Duration::from_millis(400))
        });
        let emu = session.emu.lock();
        let grid = emu.term.grid();
        // Its cells after the line's words (the startup logo's are at the
        // row's left).
        let api_shown = (0..grid.screen_lines() as i32).any(|l| {
            let row: Vec<char> = (0..grid.columns()).map(|c| grid[Line(l)][Column(c)].c).collect();
            let text: String = row.iter().collect();
            text.find("BusyBox ·").is_some_and(|at| {
                let from = text[..at].chars().count();
                row[from..].iter().filter(|&&c| c == '\u{10EEEE}').count() == api_cols
            })
        });
        let line = emu.text_between(0, emu.cursor_abs_line() + 1).contains("flow-logo-line");
        assert_eq!(line, api_shown, "the banner's line and its API logo go together");
        line
    };
    assert!(shown(&session), "no logo to begin with");

    run(&session, "busybox", r"printf '\033[?1049h\033[H%s' '############################'; sleep 1; printf '\033[?1049l'");
    assert!(shown(&session), "a full-screen program took the logo away");
    for (cols, rows) in [(90, 30), (100, 30)] {
        let sent = Instant::now();
        session.resize(GridSize { cols, rows }, cell);
        wait_for(&format!("the repaint at {cols}x{rows}"), Duration::from_secs(10), || {
            session.state.lock().last_output_at.is_some_and(|t| t > sent)
        });
        assert!(shown(&session), "resizing to {cols}x{rows} took the logo away");
    }
    run(&session, "busybox", "clear");
    assert!(!shown(&session), "clear left the logo");
}

/// As Flow starts, Windows sizes its window to the screen and back, all
/// before the first tab's shell has said a word, while an old machine's
/// start screen is still typing. ConPTY resized before it hears where its
/// cursor is paints down to the foot of the taller screen, which scrolled
/// the start screen off the top. So the pty keeps its size until then.
/// (BusyBox's ash doesn't show it; PowerShell, Windows' own, does.)
#[test]
fn a_start_screen_stays_on_screen_through_the_window_settling_as_flow_starts() {
    own_apps();
    let profile = flow_windows::PLATFORM.shells().into_iter().find(|p| p.kind == ShellKind::PowerShell).expect("PowerShell");
    let cell = (9.0, 18.0);
    let mut session = Session::launch(1, profile, GridSize { cols: 140, rows: 40 }, cell, 2000, Some(std::env::temp_dir()), Waker::from_fn(|_| {}))
        .unwrap();
    let lines: Vec<String> = (1..=20).map(|i| format!("flow-boot-line-{i}")).collect();
    session.start_typing(lines.join("\r\n") + "\r\n", 2400, None, None);
    // As the window shows, once Flow has drawn its first frame.
    std::thread::sleep(Duration::from_millis(50));
    session.resize(GridSize { cols: 248, rows: 62 }, cell);
    session.resize(GridSize { cols: 140, rows: 40 }, cell);
    wait_for("PowerShell's prompt", Duration::from_secs(30), || {
        let st = session.state.lock();
        st.integrated && st.at_prompt
    });
    wait_for("ConPTY to go quiet", Duration::from_secs(10), || {
        session.state.lock().last_output_at.is_some_and(|t| t.elapsed() > Duration::from_millis(400))
    });
    let emu = session.emu.lock();
    let shown: Vec<String> = (0..lines.len() as i64).map(|l| emu.text_between(l, l + 1).trim_end().to_string()).collect();
    assert_eq!((shown, emu.term.grid().history_size()), (lines, 0), "the start screen at the top, none of it scrolled off");
}

/// Set for this test binary when the test below starts it in a tab as a
/// full-screen program.
const FULL_SCREEN: &str = "FLOW_TEST_FULL_SCREEN";

/// A full-screen program built on flow-sdk that doesn't take the mouse, as
/// Flow Rogue doesn't, leaves the alternate screen when it's done, so the
/// main screen shows what it showed before and nothing of the program's. On
/// Windows crossterm turns the mouse off through the console, which fails if
/// it was never on, and `term::leave` used to stop there: the program's last
/// screen stayed, and the tab on the alternate screen. Started in a tab with
/// `FLOW_TEST_FULL_SCREEN` set, this test binary is that program.
#[test]
fn a_full_screen_program_leaves_nothing_on_the_main_screen() {
    own_apps();
    if std::env::var_os(FULL_SCREEN).is_some() {
        flow_sdk::term::enter(false).expect("the console");
        let mut out = std::io::stdout();
        out.write_all(b"\x1b[3;3Hflow-full-screen-marker").unwrap();
        out.flush().unwrap();
        std::thread::sleep(Duration::from_millis(300));
        flow_sdk::term::leave();
        return;
    }
    let profile = flow_windows::bundled_busybox().expect("windows/third_party/busybox/busybox64u.exe");
    let session = Session::spawn(1, profile, GridSize { cols: 100, rows: 30 }, (9.0, 18.0), 2000, None, Waker::from_fn(|_| {}))
        .unwrap();
    wait_for("busybox prompt", Duration::from_secs(30), || {
        let st = session.state.lock();
        st.integrated && st.at_prompt
    });
    let exe = std::env::current_exe().unwrap().to_string_lossy().replace('\\', "/");
    run(&session, "busybox", &format!("{FULL_SCREEN}=1 '{exe}' a_full_screen_program_leaves_nothing_on_the_main_screen --exact --nocapture"));
    wait_for("ConPTY to go quiet", Duration::from_secs(10), || {
        session.state.lock().last_output_at.is_some_and(|t| t.elapsed() > Duration::from_millis(400))
    });
    let emu = session.emu.lock();
    assert!(!emu.term.mode().contains(TermMode::ALT_SCREEN), "the tab is still on the alternate screen");
    let text = emu.text_between(0, emu.cursor_abs_line() + 1);
    assert!(!text.contains("flow-full-screen-marker"), "the program's screen is still there:\n{text}");
}

/// ConPTY drops kitty's image uploads, and gives each combining mark a column
/// of its own, so a tab's programs send pictures down its pipe for images,
/// which it names in FLOW_IMAGE_PIPE, and write placeholder cells bare: the
/// image's id in the color and nothing else. Those come through ConPTY one
/// to a column, with their color, and find the picture.
#[test]
fn pictures_sent_down_the_pipe_show_in_bare_placeholders() {
    own_apps();
    flow_core::pipe::set_platform(&flow_windows::PLATFORM);
    let profile = flow_windows::PLATFORM.shells().into_iter().find(|p| p.id == "powershell").expect("Windows PowerShell");
    let session = Session::spawn(1, profile, GridSize { cols: 100, rows: 30 }, (16.54, 38.0), 2000, None, Waker::from_fn(|_| {}))
        .unwrap();
    wait_for("PowerShell prompt", Duration::from_secs(30), || {
        let st = session.state.lock();
        st.integrated && st.at_prompt
    });
    let output = run(&session, "PowerShell", "$env:FLOW_IMAGE_PIPE").output;
    let name = output.lines().map(str::trim).find(|l| l.starts_with(r"\\.\pipe\flow-images-")).unwrap_or_else(|| panic!("{output:?}"));
    // BusyBox hands it on with forward slashes, which open it too.
    let mut pipe = std::fs::OpenOptions::new().read(true).write(true).open(name.replace('\\', "/")).expect("the pipe opens");

    // A picture two cells wide, as Flow Rogue sends its tiles, then DA1,
    // whose answer comes once Flow has it.
    let id = 0x10_0011;
    pipe.write_all(format!("\x1b_Ga=T,U=1,f=24,s=2,v=1,i={id},p=1,c=2,r=1,q=2;AAAAAAAA\x1b\\\x1b[c").as_bytes()).unwrap();
    let mut answer = Vec::new();
    while !answer.ends_with(b"c") {
        let mut buf = [0u8; 64];
        let n = pipe.read(&mut buf).unwrap();
        assert!(n > 0, "the pipe closed");
        answer.extend_from_slice(&buf[..n]);
    }
    assert_eq!(answer, b"\x1b[?62;4;22c");
    assert!(session.images.lock().lookup(id, 0).is_some());

    // The picture, a bar, and the picture again, in color even where NO_COLOR
    // would have PowerShell 7 leave Write-Host's colors out.
    run(&session, "PowerShell", r#"if ($PSStyle) { $PSStyle.OutputRendering = 'Ansi' }; $e = [char]27; $p = [char]::ConvertFromUtf32(0x10EEEE); Write-Host "$e[38;2;16;0;17m$p$p$e[0m|$e[38;2;16;0;17m$p$p$e[0m<-""#);
    let emu = session.emu.lock();
    let grid = emu.term.grid();
    let row = (0..grid.screen_lines() as i32).find(|&l| grid[Line(l)][Column(5)].c == '<').expect("the line with the pictures");
    let cells: Vec<_> = (0..5).map(|c| &grid[Line(row)][Column(c)]).collect();
    let color = Color::Spec(Rgb { r: 0x10, g: 0, b: 0x11 });
    for c in [0, 1, 3, 4] {
        assert_eq!((cells[c].c, cells[c].fg), ('\u{10EEEE}', color), "column {c}");
        assert!(cells[c].zerowidth().is_none(), "column {c}");
    }
    assert_eq!(cells[2].c, '|');
}

/// Git Bash draws its first prompt slowly, a while after PROMPT_COMMAND says
/// its folder: the tab isn't at its prompt until it's drawn. And a command
/// typed ahead while another runs starts once the prompt after that one is
/// drawn, which bash says (133;C from PS0), so the tab isn't taken for one
/// at its prompt while the queued command runs.
#[test]
fn git_bash_is_at_its_prompt_only_while_one_waits() {
    own_apps();
    let Some(profile) = flow_windows::PLATFORM.shells().into_iter().find(|p| p.flavor == BashFlavor::GitBash) else {
        eprintln!("no Git Bash here");
        return;
    };
    let session = Session::spawn(1, profile, GridSize { cols: 100, rows: 30 }, (9.0, 18.0), 2000, None, Waker::from_fn(|_| {})).unwrap();
    // Each time it's at a prompt, one has been drawn.
    wait_for("Git Bash's first prompt", Duration::from_secs(30), || {
        let st = session.state.lock();
        assert!(!st.integrated || !st.at_prompt || st.input_start.is_some(), "at a prompt before one was drawn");
        st.integrated && st.at_prompt
    });
    let typed = |text: &str| {
        for ch in text.chars() {
            session.write(ch.to_string().as_bytes());
            std::thread::sleep(Duration::from_millis(15));
        }
    };
    typed("sleep 2\r");
    std::thread::sleep(Duration::from_millis(300));
    // Ahead, while the first still runs.
    typed("sleep 2\r");
    let entered = Instant::now();
    // Halfway through the second: the first's prompt drawn, then left.
    std::thread::sleep(Duration::from_millis(3000).saturating_sub(entered.elapsed()));
    assert!(!session.state.lock().at_prompt, "at its prompt while the command typed ahead runs");
    wait_for("the prompt after both", Duration::from_secs(10), || session.state.lock().at_prompt);
}

/// A click on the line typed at the prompt moves each shell's cursor there
/// (`Session::click_to_move`), through ConPTY: what's typed after it lands
/// at the click, and a click on the line above does nothing. ConPTY
/// repaints its screen after a resize, which rewraps a long line above, and
/// the prompt's input start (its 133;B mark) must stay on the cursor's line
/// through that. PowerShell, cmd and BusyBox say nothing as a command
/// starts (no 133;C), so they're taken to be at their prompt while one
/// runs: a click on what it prints, or on the line the cursor waits on,
/// sends it nothing.
#[test]
fn a_click_on_the_prompts_line_moves_each_shells_cursor_there() {
    use alacritty_terminal::index::Point;
    own_apps();
    let busybox = flow_windows::bundled_busybox().expect("windows/third_party/busybox/busybox64u.exe");
    let cell = (9.0, 18.0);
    // Windows PowerShell 5.1 and its own PSReadLine too, where PowerShell 7
    // took its place, as a computer without PowerShell 7 has it.
    let windows_ps = PathBuf::from(std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into())).join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
    let mut shells: Vec<ShellProfile> = flow_windows::PLATFORM.shells().into_iter().chain([busybox]).collect();
    if windows_ps.is_file() && !shells.iter().any(|p| p.program == windows_ps) {
        shells.push(ShellProfile { id: "powershell", label: "Windows PowerShell".into(), kind: ShellKind::PowerShell, program: windows_ps, flavor: BashFlavor::Native });
    }
    for profile in shells {
        if profile.flavor == BashFlavor::Wsl {
            continue;
        }
        let label = format!("{} ({:?})", profile.label, profile.flavor);
        let kind = profile.kind;
        let mut session = Session::spawn(8, profile, GridSize { cols: 120, rows: 30 }, cell, 2000, None, Waker::from_fn(|_| {}))
            .unwrap_or_else(|e| panic!("{label}: {e:#}"));
        wait_for(&format!("{label} first prompt"), Duration::from_secs(30), || {
            let st = session.state.lock();
            st.integrated && st.at_prompt
        });
        // Nothing typed here goes into the user's own PowerShell history,
        // whose predictions the next PowerShell draws: the long line below,
        // drawn as one past the screen's edge, would leave a row of blanks
        // joining a later command's line to its output.
        if kind == ShellKind::PowerShell {
            run(&session, &label, "Set-PSReadLineOption -HistorySaveStyle SaveNothing");
        }
        // A line of 110 columns above, which a narrower screen wraps.
        run(&session, &label, &format!("echo {}", "w".repeat(110)));
        wait_for(&format!("{label} prompt"), Duration::from_secs(10), || session.state.lock().at_prompt);
        for ch in "echo flow-click-ab".chars() {
            session.write(ch.to_string().as_bytes());
            std::thread::sleep(Duration::from_millis(15));
        }
        wait_for(&format!("{label}: the echo"), Duration::from_secs(10), || session.cursor_line().1.contains("flow-click-ab"));
        let agree = |session: &Session| {
            let start = session.state.lock().input_start.map(|s| s.0);
            (start, session.cursor_line().0)
        };
        let (start, line) = agree(&session);
        assert_eq!(start, Some(line), "{label}: the input's start and the cursor's line");
        for cols in [100, 120] {
            let sent = Instant::now();
            session.resize(GridSize { cols, rows: 30 }, cell);
            wait_for(&format!("{label}: the repaint at {cols} columns"), Duration::from_secs(10), || {
                session.state.lock().last_output_at.is_some_and(|t| t > sent && t.elapsed() > Duration::from_millis(400))
            });
            let (start, line) = agree(&session);
            assert_eq!(start, Some(line), "{label}: the input's start and the cursor's line after ConPTY's repaint at {cols} columns");
        }
        // The b of "ab", on the cursor's own line.
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
        wait_for(&format!("{label}: the line run"), Duration::from_secs(10), || session.emu.lock().recent_text(30).lines().any(|l| l.trim() == "flow-click-aXb"));

        // A program running, which prints a line and waits.
        let running = match kind {
            ShellKind::Cmd => "echo flow-click-running& ping -n 4 127.0.0.1 >nul",
            ShellKind::PowerShell => "Write-Output flow-click-running; Start-Sleep 3",
            _ => "echo flow-click-running; sleep 3",
        };
        wait_for(&format!("{label} prompt"), Duration::from_secs(10), || session.state.lock().at_prompt);
        session.state.lock().history.clear();
        session.write(format!("{running}\r").as_bytes());
        let printed = || {
            let emu = session.emu.lock();
            let grid = emu.term.grid();
            (0..grid.screen_lines() as i32).find(|&l| (0..grid.columns()).map(|c| grid[Line(l)][Column(c)].c).collect::<String>().trim() == "flow-click-running")
        };
        wait_for(&format!("{label}: the program's line"), Duration::from_secs(10), || printed().is_some());
        let output = Point::new(Line(printed().unwrap()), Column(5));
        let waiting = Point::new(session.emu.lock().term.grid().cursor.point.line, Column(5));
        let at_prompt = session.state.lock().at_prompt;
        assert!(!session.click_to_move(output), "{label}: a click on a running program's line (at_prompt {at_prompt})");
        assert!(!session.click_to_move(waiting), "{label}: a click where a running program's cursor waits (at_prompt {at_prompt})");
        wait_for(&format!("{label}: the program done"), Duration::from_secs(20), || {
            session.state.lock().history.iter().any(|n| matches!(n, HistoryNote::Done(_)))
        });

        // A line typed ahead while another runs starts once that one's
        // prompt is drawn, which PowerShell, cmd and BusyBox don't say: the
        // tab is taken to be at its prompt while it runs.
        let (first, ahead) = match kind {
            ShellKind::Cmd => ("ping -n 2 127.0.0.1 >nul", "echo flow-click-ahead& ping -n 4 127.0.0.1 >nul"),
            ShellKind::PowerShell => ("Start-Sleep 1", "Write-Output flow-click-ahead; Start-Sleep 3"),
            _ => ("sleep 1", "echo flow-click-ahead; sleep 3"),
        };
        wait_for(&format!("{label} prompt"), Duration::from_secs(10), || session.state.lock().at_prompt);
        session.write(format!("{first}\r").as_bytes());
        std::thread::sleep(Duration::from_millis(300));
        for ch in format!("{ahead}\r").chars() {
            session.write(ch.to_string().as_bytes());
            std::thread::sleep(Duration::from_millis(5));
        }
        let ahead_line = || {
            let emu = session.emu.lock();
            let grid = emu.term.grid();
            (0..grid.screen_lines() as i32).find(|&l| (0..grid.columns()).map(|c| grid[Line(l)][Column(c)].c).collect::<String>().trim() == "flow-click-ahead")
        };
        wait_for(&format!("{label}: the line typed ahead running"), Duration::from_secs(15), || ahead_line().is_some());
        let output = Point::new(Line(ahead_line().unwrap()), Column(5));
        let waiting = Point::new(session.emu.lock().term.grid().cursor.point.line, Column(5));
        let ahead_at_prompt = session.state.lock().at_prompt;
        assert!(!session.click_to_move(output), "{label}: a click on what the line typed ahead prints (at_prompt {ahead_at_prompt})");
        assert!(!session.click_to_move(waiting), "{label}: a click where its cursor waits (at_prompt {ahead_at_prompt})");
        // On the line it was typed on, too, which the input's start is on.
        let typed_on = session.state.lock().input_start.map(|(abs, _)| abs - session.emu.lock().term.grid().history_size() as i64);
        if let Some(row) = typed_on.filter(|&r| r >= 0) {
            assert!(!session.click_to_move(Point::new(Line(row as i32), Column(8))), "{label}: a click on the line typed ahead, while it runs");
        }
        std::thread::sleep(Duration::from_secs(4));
        wait_for(&format!("{label} prompt after"), Duration::from_secs(15), || {
            let st = session.state.lock();
            st.at_prompt && st.last_output_at.is_some_and(|t| t.elapsed() > Duration::from_millis(400))
        });
        eprintln!("{label}: a click moves the cursor (at_prompt while running: {at_prompt}, typed ahead: {ahead_at_prompt})");
    }
}

/// Up and Down over a command long enough to wrap, with Devanagari's vowel
/// sign ा in it, a spacing mark that unicode-width, and so the emulator,
/// gives a cell of its own here (the Mac puts it on the letter before, as
/// its shells measure it, `flow_core::combining`): each shell types it,
/// and puts it back, whole, through ConPTY. Where the console or a line
/// editor measured the mark otherwise, the line would come back cells off,
/// as zsh's did on the Mac (letters lost as it was typed, and Down leaving
/// `echo "नमshort`). In PowerShell, what the console measures each piece
/// as (its cursor after writing it) is printed beside the emulator's cells.
///
/// Ignored while it fails: Windows' own ConPTY gives every
/// letter that isn't wide a column, ा and the marks of no width (े, ्)
/// alike, so it lays the sentence out in 24 columns where the emulator
/// lays it out in 20; PSReadLine measures as the console does, Git Bash's
/// C library as the emulator does. cmd, whose line the console edits
/// itself, comes back whole; PowerShell 7 and 5.1 put a recalled line
/// back with blanks where its first row ends, BusyBox does so as the line
/// is typed and loses letters, and Git Bash's Down leaves `echo "नमस्short`.
#[test]
#[ignore = "ConPTY measures marks of no width as a column"]
fn a_long_command_with_spacing_marks_comes_back_whole_with_up_and_down() {
    own_apps();
    let busybox = flow_windows::bundled_busybox().expect("windows/third_party/busybox/busybox64u.exe");
    let windows_ps = PathBuf::from(std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into())).join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
    let mut shells: Vec<ShellProfile> = flow_windows::PLATFORM.shells().into_iter().chain([busybox]).collect();
    if windows_ps.is_file() && !shells.iter().any(|p| p.program == windows_ps) {
        shells.push(ShellProfile { id: "powershell", label: "Windows PowerShell".into(), kind: ShellKind::PowerShell, program: windows_ps, flavor: BashFlavor::Native });
    }
    let sentence = "नमस्ते, मेरा नाम साई है।";
    let long = format!("echo \"{sentence}\" {}", "b".repeat(40));
    let mut failed = Vec::new();
    for profile in shells {
        if profile.flavor == BashFlavor::Wsl {
            continue;
        }
        let label = format!("{} ({:?})", profile.label, profile.flavor);
        let kind = profile.kind;
        let session = Session::spawn(8, profile, GridSize { cols: 60, rows: 20 }, (9.0, 18.0), 2000, None, Waker::from_fn(|_| {}))
            .unwrap_or_else(|e| panic!("{label}: {e:#}"));
        wait_for(&format!("{label} first prompt"), Duration::from_secs(30), || {
            let st = session.state.lock();
            st.integrated && st.at_prompt
        });
        // Nothing typed here goes into the user's own history, nor do the
        // user's commands come up as predictions after the cursor.
        match kind {
            ShellKind::PowerShell => {
                run(&session, &label, "Set-PSReadLineOption -HistorySaveStyle SaveNothing; try { Set-PSReadLineOption -PredictionSource None } catch {}");
                // Each piece written by the console, which says where its
                // cursor went: रा (a letter and ा), मे (a letter and a mark
                // of no width), स्त (a conjunct) and the whole sentence,
                // given by their code points so none is typed.
                let pieces: Vec<String> = ["रा", "मे", "स्त", sentence]
                    .iter()
                    .map(|p| format!("({})", p.chars().map(|c| format!("0x{:X}", c as u32)).collect::<Vec<_>>().join(",")))
                    .collect();
                // The host writes through the console's own Unicode calls,
                // where [Console]::Write would go through its code page.
                run(&session, &label, &format!("foreach ($p in @({})) {{ $Host.UI.Write((-join [char[]]$p)); $x = $Host.UI.RawUI.CursorPosition.X; $Host.UI.WriteLine(\"|$x\") }}", pieces.join(",")));
                let emu = session.emu.lock();
                let grid = emu.term.grid();
                for l in 0..grid.screen_lines() as i32 {
                    let row = &grid[Line(l)];
                    let text: String = (0..grid.columns()).map(|c| row[Column(c)].c).collect();
                    let Some(bar) = (0..grid.columns()).rev().find(|&c| row[Column(c)].c == '|') else { continue };
                    let after = text.chars().skip(bar + 1).collect::<String>();
                    if let Ok(console) = after.trim().parse::<usize>() {
                        let piece: String = (0..bar).map(|c| format!("{}{}", row[Column(c)].c, row[Column(c)].zerowidth().unwrap_or(&[]).iter().collect::<String>())).collect();
                        eprintln!("{label}: {:?} is {console} columns to the console, {bar} cells to the emulator", piece.trim_end());
                    }
                }
            }
            ShellKind::Bash | ShellKind::Sh => {
                run(&session, &label, "unset HISTFILE");
            }
            _ => {}
        }
        let typed = |text: &str| {
            for ch in text.chars() {
                session.write(ch.to_string().as_bytes());
                std::thread::sleep(Duration::from_millis(15));
            }
        };
        let key = |up: bool| {
            let app = session.emu.lock().term.mode().contains(TermMode::APP_CURSOR);
            session.write(match (app, up) {
                (true, true) => b"\x1bOA",
                (true, false) => b"\x1bOB",
                (false, true) => b"\x1b[A",
                (false, false) => b"\x1b[B",
            });
        };
        // Whether `short` was printed just above, a prompt's own line apart
        // (Git Bash's), and the line being edited.
        let last_two = || {
            let text = session.emu.lock().recent_text(8);
            let lines: Vec<String> = text.lines().map(|l| l.trim_end().to_string()).filter(|l| !l.is_empty()).collect();
            let n = lines.len();
            let short = lines[..n.saturating_sub(1)].iter().rev().take(2).any(|l| l == "short");
            (short, lines.last().cloned().unwrap_or_default())
        };
        // Waits as `wait_for` does, but says what the screen held instead.
        let settles = |what: &str, f: &dyn Fn() -> bool| -> Result<(), String> {
            let start = Instant::now();
            while !f() {
                if start.elapsed() > Duration::from_secs(10) {
                    return Err(format!("{what}: the screen ends {:?}", session.emu.lock().recent_text(6)));
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Ok(())
        };
        let outcome = (|| -> Result<(), String> {
            typed(&long);
            settles("the long line typed whole", &|| last_two().1.ends_with(&long))?;
            typed("\r");
            // PowerShell's echo prints each word on a line of its own.
            settles("its output", &|| session.emu.lock().recent_text(8).lines().any(|l| !l.contains("echo") && l.contains(sentence)))?;
            std::thread::sleep(Duration::from_millis(500));
            typed("echo short\r");
            settles("short", &|| last_two().0 && session.state.lock().at_prompt)?;
            std::thread::sleep(Duration::from_millis(300));
            for (up, want) in [(true, "echo short"), (true, long.as_str()), (false, "echo short"), (true, long.as_str())] {
                key(up);
                settles(&format!("{} to {want}", if up { "Up" } else { "Down" }), &|| {
                    let (short, line) = last_two();
                    short && line.ends_with(want) && !line.contains(&format!("नम{want}"))
                })?;
                std::thread::sleep(Duration::from_millis(300));
            }
            Ok(())
        })();
        match outcome {
            Ok(()) => eprintln!("{label}: ok"),
            Err(why) => {
                eprintln!("{label}: {why}");
                failed.push(format!("{label}: {why}"));
            }
        }
    }
    assert!(failed.is_empty(), "{failed:#?}");
}

/// Runs commands the way the assistant does and checks output capture and
/// exit codes through the integration markers. Returns the session at a prompt.
fn check_shell(profile: ShellProfile, cwd: Option<PathBuf>) -> Session {
    let label = profile.label.clone();
    let kind = profile.kind;
    let session = Session::spawn(
        1,
        profile,
        GridSize { cols: 120, rows: 30 },
        (9.0, 18.0),
        2000,
        cwd,
        Waker::from_fn(|_| {}),
    )
    .unwrap_or_else(|e| panic!("{label}: {e:#}"));
    wait_for(&format!("{label} prompt"), Duration::from_secs(30), || {
        let st = session.state.lock();
        st.integrated && st.at_prompt
    });

    let result = run(&session, &label, "echo flow-marker-42");
    assert!(!result.still_running, "{label}: {result:?}");
    assert!(result.output.contains("flow-marker-42"), "{label}: {:?}", result.output);
    if kind != ShellKind::Cmd {
        assert_eq!(result.exit_code, Some(0), "{label}: {result:?}");
    }
    assert!(session.cwd().is_some(), "{label}: no cwd reported");

    // Output with no final newline: bash and BusyBox leave the cursor after
    // it when the done marker comes, while cmd and PowerShell start a new line
    // first. The command line contains the text too, so look for it on its own.
    let unterminated = match kind {
        ShellKind::Cmd => "<nul set /p =flow-tail-7",
        ShellKind::PowerShell => "Write-Host -NoNewline flow-tail-7",
        _ => "printf flow-tail-7",
    };
    let output = run(&session, &label, unterminated).output;
    assert!(output.lines().any(|l| l.trim() == "flow-tail-7"), "{label}: {output:?}");

    // What's half-typed at the prompt goes before a command Flow types, and
    // a command beginning with `[` arrives whole: cmd and PowerShell are
    // cleared with End and Ctrl+Home, as an Esc there would make the `[` a
    // key's sequence.
    session.write(b"flow-junk");
    std::thread::sleep(Duration::from_millis(300));
    let output = run(&session, &label, "echo flow-clean-5").output;
    assert!(output.lines().any(|l| l.trim() == "flow-clean-5") && !output.contains("flow-junk"), "{label}: {output:?}");
    if kind == ShellKind::PowerShell {
        let result = run(&session, &label, "[Console]::WriteLine('flow-bracket-9')");
        assert!(result.output.lines().any(|l| l.trim() == "flow-bracket-9"), "{label}: {result:?}");
        assert_eq!(result.exit_code, Some(0), "{label}: {result:?}");
    }

    // A PowerShell prompt that draws itself with Write-Host prints before it
    // returns. Its text must come after the done marker, not end the output.
    if kind == ShellKind::PowerShell {
        run(&session, &label, "$global:__flowOrigPrompt = { Write-Host -NoNewline 'flow-prompt>'; ' ' }");
        let output = run(&session, &label, "echo flow-marker-43").output;
        assert!(output.lines().any(|l| l.trim() == "flow-marker-43"), "{label}: {output:?}");
        assert!(!output.trim_end().ends_with("flow-prompt>"), "{label}: {output:?}");
    }

    let failing = match kind {
        ShellKind::Cmd => None,
        ShellKind::PowerShell => Some("cmd /c exit 3"),
        _ => Some("(exit 3)"),
    };
    if let Some(cmd) = failing {
        assert_eq!(run(&session, &label, cmd).exit_code, Some(3), "{label}");
    }
    wait_for(&format!("{label} final prompt"), Duration::from_secs(10), || session.state.lock().at_prompt);
    eprintln!("{label}: ok (cwd {:?})", session.cwd());
    session
}
