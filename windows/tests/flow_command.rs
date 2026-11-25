//! The `flow` command in Windows' shells, through ConPTY: its words quoted
//! for each shell (`shell::quote_for`) and typed as Flow types them
//! (`Session::run_typed`) reach a program as they were given, none expanded
//! or split again; and the ask `flow` sends down the tab's pipe
//! (`FLOW_IMAGE_PIPE`) carries the pane's own secret (`FLOW_TAB_KEY`) to the
//! session as a settings ask, whose answer comes back to it. The program is
//! a copy of this test program ([`stand_in`]), since cmd's `echo` prints
//! its line as typed rather than the words a program gets.
#![cfg(windows)]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use base64::Engine as _;
use flow_core::session::{GridSize, HistoryNote, Session, Waker};
use flow_core::shell::{BashFlavor, ShellKind, ShellProfile};
use flow_core::Platform;

fn wait_for(what: &str, timeout: Duration, mut f: impl FnMut() -> bool) {
    let start = Instant::now();
    while !f() {
        assert!(start.elapsed() < timeout, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// The stand-in's name on the shells' PATH.
const STAND_IN: &str = "flow-args";

/// What the shells type before the stand-in's own words: this program run
/// as the test below and nothing else, past `--` the words its own.
const AS_STAND_IN: [&str; 6] = ["--exact", "--ignored", "--nocapture", "--test-threads=1", "stand_in", "--"];

/// The stand-in, run by the tests below from a shell as `flow-args`, after
/// [`AS_STAND_IN`]: `print` then words prints each between `<` and `>` on
/// a line of its own; `ask` then `flow`'s arguments does what the `flow`
/// program does with them (asks the tab's Flow down its pipe) and prints
/// its exit code. Run any other way, it does nothing.
#[test]
#[ignore = "the stand-in program, which the tests below run in a shell"]
fn stand_in() {
    let args: Vec<String> = std::env::args().collect();
    let Some(at) = args.iter().position(|a| a == "--") else { return };
    let words = &args[at + 1..];
    let mut out = std::io::stdout();
    match words.first().map(String::as_str) {
        Some("print") => {
            // A line of its own: libtest's "test stand_in ..." has none.
            let mut text = String::from("\n");
            for word in &words[1..] {
                text.push_str(&format!("<{word}>\n"));
            }
            out.write_all(text.as_bytes()).unwrap();
        }
        Some("ask") => {
            let flow_command = flow_core::flow_command::run;
            let code = flow_command(&words[1..], &|request| flow_sdk::settings::ask(request, flow_core::flow_command::WAIT), false);
            out.write_all(format!("\nflow-ask-exit-{code}\n").as_bytes()).unwrap();
        }
        _ => {}
    }
    out.flush().unwrap();
}

/// Puts a copy of this program on every shell's PATH as [`STAND_IN`]
/// (Flow's games' folder, which each shell's hook keeps on it), and has the
/// tabs keep their panes' folders in an apps' folder of the tests' own.
fn set_up() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        flow_core::apps::first::set_root(std::env::temp_dir().join(format!("flow-command-apps-{}", std::process::id())));
        flow_core::pipe::set_platform(&flow_windows::PLATFORM);
        // Not under %TEMP%, which Git Bash also calls /tmp.
        let games = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("flow-command-test-{}", std::process::id()));
        std::fs::create_dir_all(&games).unwrap();
        std::fs::copy(std::env::current_exe().unwrap(), games.join(format!("{STAND_IN}.exe"))).unwrap();
        flow_core::shell::set_games_dir(games);
    });
}

/// Every shell the tests try: the platform's (cmd, PowerShell 7 where it's
/// installed, a bash), the bundled BusyBox, and Windows PowerShell 5.1
/// where PowerShell 7 took its place, since a computer without PowerShell 7
/// gets it.
fn shells() -> Vec<ShellProfile> {
    let mut shells = flow_windows::PLATFORM.shells();
    let busybox = flow_windows::bundled_busybox().expect("windows/third_party/busybox/busybox64u.exe");
    if !shells.iter().any(|p| p.flavor == BashFlavor::BusyBox) {
        shells.push(busybox);
    }
    let windows_ps = PathBuf::from(std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into())).join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
    let has_it = shells.iter().any(|p| p.program.eq(&windows_ps));
    if windows_ps.is_file() && !has_it {
        shells.push(ShellProfile { id: "powershell", label: "Windows PowerShell".into(), kind: ShellKind::PowerShell, program: windows_ps, flavor: BashFlavor::Native });
    }
    // WSL's programs see Windows' PATH through its own interop.
    shells.retain(|p| p.flavor != BashFlavor::Wsl);
    shells
}

fn start(profile: ShellProfile, id: u32) -> (String, Session) {
    let label = format!("{} ({:?})", profile.label, profile.flavor);
    let kind = profile.kind;
    let session = Session::spawn(id, profile, GridSize { cols: 300, rows: 60 }, (9.0, 18.0), 2000, None, Waker::from_fn(|_| {}))
        .unwrap_or_else(|e| panic!("{label}: {e:#}"));
    wait_for(&format!("{label} first prompt"), Duration::from_secs(30), || {
        let st = session.state.lock();
        st.integrated && st.at_prompt
    });
    if kind == ShellKind::PowerShell {
        keep_out_of_history(&session, &label);
    }
    (label, session)
}

/// Has PowerShell keep what's typed next out of the user's own history
/// file, whose lines its next sessions predict from: a long line there
/// drawn as a prediction past the screen's edge leaves a row of blanks
/// that joins a later command's line to its output.
fn keep_out_of_history(session: &Session, label: &str) {
    session.run_command("Set-PSReadLineOption -HistorySaveStyle SaveNothing");
    wait_for(&format!("{label}: history kept in"), Duration::from_secs(20), || session.poll_command(Duration::from_secs(20)).is_some());
}

/// The stand-in's words in a shell's own command line: `words` after it.
fn stand_in_line(mode: &str, words: &[&str]) -> Vec<String> {
    [STAND_IN].iter().chain(AS_STAND_IN.iter()).chain([mode].iter()).chain(words.iter()).map(|w| w.to_string()).collect()
}

/// The `flow` command's words, quoted for each shell and typed as Flow
/// types them, reach the program as they were given: none expanded,
/// split, joined or dropped by the shell, nor by how it hands them to a
/// Windows program.
#[test]
fn the_flow_commands_words_come_back_as_they_were_given_in_each_shell() {
    set_up();
    let odd = [
        "it's $HOME", "*", "a b  c", "\"\"", "a\"b&c", "x&y", "%PATH%", "$env:HOME", "a,b", "@x", "-D.x", r"C:\my dir\", "", "--", "ünï", "ssh", "-N",
        "7445:127.0.0.1:7445", "user@host",
    ];
    let words = stand_in_line("print", &odd);
    let mut failed = Vec::new();
    for (i, profile) in shells().into_iter().enumerate() {
        let typed = flow_core::shell::quote_for(&profile, &words).unwrap();
        let (label, session) = start(profile, 10 + i as u32);
        session.state.lock().history.clear();
        session.run_typed(&typed);
        wait_for(&format!("{label}: the words printed"), Duration::from_secs(30), || {
            session.state.lock().history.iter().any(|n| matches!(n, HistoryNote::Done(_)))
        });
        let screen = session.emu.lock().recent_text(60);
        let back: Vec<&str> = screen.lines().map(str::trim_end).filter(|l| l.starts_with('<') && l.ends_with('>')).map(|l| &l[1..l.len() - 1]).collect();
        if back != odd {
            eprintln!("{label}: typed {typed}\n{screen}\n");
            failed.push(format!("{label}: {back:?}"));
        } else {
            eprintln!("{label}: the words come back");
        }
    }
    assert!(failed.is_empty(), "{failed:#?}");
}

/// `flow` in a tab asks the Flow it belongs to down the tab's pipe, as
/// `flow-settings` does, with the pane's own secret, which each shell gives
/// its programs as `FLOW_TAB_KEY`: the ask reaches the session as a
/// settings ask carrying that key, and Flow's answer goes back to `flow`,
/// which exits by it. Left unanswered, as Flow leaves a request without
/// the pane's own key, the pipe ends its wait at once and `flow` says the
/// key may be another pane's.
#[test]
fn the_flow_commands_ask_goes_down_the_pipe_with_the_panes_key() {
    set_up();
    for (i, profile) in shells().into_iter().enumerate() {
        let shell = profile.clone();
        let (label, session) = start(profile, 30 + i as u32);
        assert_eq!(session.key().len(), 32, "{label}");
        for (answer, code) in [(Some(r#"{"ok":true,"pane":9}"#), 0), (Some(r#"{"ok":false,"error":"flow-test-no-room"}"#), 1), (None, 2)] {
            let typed = flow_core::shell::quote_for(&shell, &stand_in_line("ask", &["--behind", "--theme", "green", "ls", "a b"])).unwrap();
            wait_for(&format!("{label} prompt"), Duration::from_secs(10), || session.state.lock().at_prompt);
            session.run_command(&typed);
            let mut asks = Vec::new();
            wait_for(&format!("{label}: the ask"), Duration::from_secs(20), || {
                asks.extend(session.take_settings_asks());
                !asks.is_empty()
            });
            let ask = asks.remove(0);
            let asked = flow_core::flow_command::open_tab_asked(&ask.request).unwrap_or_else(|| panic!("{label}: {:?}", ask.request));
            assert_eq!(asked["pass"], session.key(), "{label}");
            assert_eq!(asked["command"].to_string(), r#"["ls","a b"]"#, "{label}");
            assert_eq!((asked["behind"].as_bool(), asked["theme"].as_str()), (Some(true), Some("green")), "{label}");
            assert!(asked["cwd"].as_str().is_some_and(|c| Path::new(c).is_dir()), "{label}: {asked}");
            // As Flow answers it, or leaves it unanswered: the reply dropped.
            let reply = ask.reply.unwrap_or_else(|| panic!("{label}: an ask from the pipe waits for its answer"));
            match answer {
                Some(answer) => reply.send(format!("\x1b]7732;{}\x07", base64::engine::general_purpose::STANDARD.encode(answer))).unwrap(),
                None => drop(reply),
            }
            let exited = format!("flow-ask-exit-{code}");
            wait_for(&format!("{label}: {exited}"), Duration::from_secs(20), || session.emu.lock().recent_text(60).lines().any(|l| l.trim() == exited));
            if code == 1 {
                assert!(session.emu.lock().recent_text(60).contains("flow: flow-test-no-room"), "{label}");
            }
            if code == 2 {
                assert!(session.emu.lock().recent_text(60).contains("FLOW_TAB_KEY"), "{label}: why nothing answered");
            }
        }
        eprintln!("{label}: the ask ok");
    }
}
