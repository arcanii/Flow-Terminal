//! `flow-settings` run for real by a shell in a real PTY, as in one of
//! Flow's tabs: every request it sends, `--help`'s `describe` among them,
//! carries the pane's own secret (`FLOW_TAB_KEY`) as its `pass`, without
//! which Flow answers nothing at all, and the answer, written here as Flow
//! writes it, comes back to it. With the secret left out (`env -u`) it asks
//! nothing and says so at once; with another one, which Flow leaves
//! unanswered, it says after its wait that the key may be another pane's,
//! and so does the `flow` command (`flow-command`), which asks the same way.
#![cfg(unix)]

use std::time::{Duration, Instant};

use base64::Engine as _;
use flow_core::session::{GridSize, Session, SettingsAsk, Waker};
use flow_core::Platform;
use serde_json::{json, Value};

#[cfg(target_os = "macos")]
use flow_macos::PLATFORM;
#[cfg(not(target_os = "macos"))]
use flow_linux::PLATFORM;

fn wait_for(what: &str, timeout: Duration, mut f: impl FnMut() -> bool) {
    let start = Instant::now();
    while !f() {
        assert!(start.elapsed() < timeout, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// The programs that ask.
const SETTINGS: &str = env!("CARGO_BIN_EXE_flow-settings");
const FLOW: &str = env!("CARGO_BIN_EXE_flow-command");

/// A shell started as in one of Flow's tabs, at its first prompt, with an
/// apps' folder of the test's own, never the user's (`flowapp first`).
fn shell(id: u32) -> (String, Session) {
    flow_core::shell::set_shell_env(PLATFORM.shell_env());
    flow_core::apps::first::set_root(std::env::temp_dir().join(format!("flow-settings-apps-{}", std::process::id())));
    let profile = PLATFORM.shells().into_iter().next().expect("a shell");
    let label = profile.label.clone();
    let session = Session::spawn(id, profile, GridSize { cols: 200, rows: 40 }, (9.0, 18.0), 2000, None, Waker::from_fn(|_| {}))
        .unwrap_or_else(|e| panic!("{label}: {e:#}"));
    wait_for(&format!("{label}'s first prompt"), Duration::from_secs(30), || {
        let st = session.state.lock();
        st.integrated && st.at_prompt
    });
    (label, session)
}

/// Runs `program` with `args` (after `before`, words of the shell's own)
/// at the shell's prompt as the `n`th command, and gives the request it
/// sends, as JSON, with where its answer goes.
fn run_asking(session: &Session, n: usize, before: &str, program: &str, args: &str) -> (Value, SettingsAsk) {
    run(session, n, before, program, args);
    let mut asks = Vec::new();
    wait_for(&format!("{program} {args}'s ask"), Duration::from_secs(10), || {
        asks.extend(session.take_settings_asks());
        !asks.is_empty()
    });
    let ask = asks.remove(0);
    let bytes = base64::engine::general_purpose::STANDARD.decode(ask.request.trim()).unwrap();
    (serde_json::from_slice(&bytes).unwrap(), ask)
}

/// Types `program` with `args` (after `before`) at the shell's prompt as
/// the `n`th command, which says its exit code after it.
fn run(session: &Session, n: usize, before: &str, program: &str, args: &str) {
    wait_for("the prompt", Duration::from_secs(10), || session.state.lock().at_prompt);
    session.run_command(&format!("{before}'{program}' {args}; echo asked-{n}-exit-$?"));
}

/// Answers the `n`th command as Flow does, into the terminal, and waits
/// for the exit code its program ends with, said by that command alone;
/// gives what the command printed.
fn answer(session: &Session, n: usize, answer: Value, code: i32) -> String {
    let escaped = format!("\x1b]7732;{}\x07", base64::engine::general_purpose::STANDARD.encode(answer.to_string()));
    session.report(escaped.as_bytes());
    ended(session, n, code)
}

/// Waits for the `n`th command's program to end with `code`, said by
/// that command alone; gives what the command printed.
fn ended(session: &Session, n: usize, code: i32) -> String {
    let exited = format!("asked-{n}-exit-{code}");
    wait_for(&exited, Duration::from_secs(10), || session.emu.lock().recent_text(40).lines().any(|l| l.trim() == exited));
    let mut done = None;
    wait_for("the command's end", Duration::from_secs(10), || {
        done = session.poll_command(Duration::from_secs(20));
        done.is_some()
    });
    done.unwrap().output
}

#[test]
fn every_request_flow_settings_sends_carries_the_panes_own_key() {
    let (label, session) = shell(41);
    let key = session.key().to_string();
    assert_eq!(key.len(), 32, "{label}");

    let (asked, sent) = run_asking(&session, 1, "", SETTINGS, "get fx.style");
    assert_eq!(asked, json!({ "do": "get", "key": "fx.style", "pass": key }), "{label}");
    assert!(sent.reply.is_none(), "{label}: asked in the terminal");
    let printed = answer(&session, 1, json!({ "ok": true, "key": "fx.style", "value": "rain" }), 0);
    assert!(printed.lines().any(|l| l.trim() == "rain"), "{label}: {printed}");

    let (asked, _) = run_asking(&session, 2, "", SETTINGS, "set font.size 16");
    assert_eq!(asked, json!({ "do": "set", "key": "font.size", "value": 16, "pass": key }), "{label}");
    answer(&session, 2, json!({ "ok": true, "key": "font.size", "value": 16 }), 0);

    let (asked, _) = run_asking(&session, 3, "", SETTINGS, "restart");
    assert_eq!(asked, json!({ "do": "restart", "pass": key }), "{label}");
    let printed = answer(&session, 3, json!({ "ok": false, "error": "flow-test-busy" }), 1);
    assert!(printed.contains("flow-settings: flow-test-busy"), "{label}: {printed}");

    // Help asks what each setting takes, with the key too.
    let (asked, _) = run_asking(&session, 4, "", SETTINGS, "--help");
    assert_eq!(asked, json!({ "do": "describe", "pass": key }), "{label}");
    let printed = answer(&session, 4, json!({ "ok": true, "takes": {}, "words": { "fx.style": "flow-test-styles" } }), 0);
    assert!(printed.lines().any(|l| l.trim() == "fx.style  flow-test-styles"), "{label}: {printed}");

    // Without the key, as through ssh or sudo: nothing asked, since Flow
    // wouldn't answer, and why said at once. (Nothing asked is what says
    // there was no wait: the SDK's own lasts about a second, too short for
    // a clock here to tell apart.)
    run(&session, 5, "env -u FLOW_TAB_KEY ", SETTINGS, "get fx.style");
    let printed = ended(&session, 5, 2);
    assert!(session.take_settings_asks().is_empty(), "{label}: nothing asked");
    assert!(printed.contains(flow_core::i18n::t("flow_settings.no_key")), "{label}: {printed}");

    // Another pane's key, as a tmux session started in that pane keeps:
    // asked with it, left unanswered as Flow leaves it, and why said once
    // the wait is over.
    let other = "f".repeat(32);
    let (asked, _) = run_asking(&session, 6, &format!("FLOW_TAB_KEY={other} "), SETTINGS, "get fx.style");
    assert_eq!(asked, json!({ "do": "get", "key": "fx.style", "pass": other }), "{label}");
    let printed = ended(&session, 6, 2);
    assert!(printed.contains(flow_core::i18n::t("flow_settings.no_answer")), "{label}: {printed}");
}

#[test]
fn the_flow_command_left_unanswered_says_its_key_may_be_another_panes() {
    // Asked with another pane's key, so Flow, as here, leaves it
    // unanswered. (Without a key it would start a Flow of its own.)
    let (label, session) = shell(42);
    let other = "f".repeat(32);
    let (asked, _) = run_asking(&session, 1, &format!("FLOW_TAB_KEY={other} "), FLOW, "ls");
    assert_eq!((&asked["do"], &asked["pass"], &asked["command"]), (&json!("open-tab"), &json!(other), &json!(["ls"])), "{label}");
    let printed = ended(&session, 1, 2);
    assert!(printed.contains(&format!("flow: {}", flow_core::i18n::t("flow_command.no_answer"))), "{label}: {printed}");
}
