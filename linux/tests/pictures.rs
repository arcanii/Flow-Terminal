//! Has a shell in a real PTY print kitty pictures named by their paths, as
//! `kitten icat` sends a PNG, and checks that files its programs failed to
//! read stay counted through a terminal reset, which a program holding the
//! tab's key may ask for, as `macos/tests/pictures.rs` does on the Mac.
#![cfg(all(unix, not(target_os = "macos")))]

use std::path::Path;
use std::time::{Duration, Instant};

use flow_core::session::{CommandResult, GridSize, Session, Waker};
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

/// Base64, as kitty's icat sends a path.
fn base64(bytes: &[u8]) -> String {
    const LETTERS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, &b)| n | (b as u32) << (16 - 8 * i));
        for i in 0..4 {
            out.push(if i <= chunk.len() { LETTERS[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' });
        }
    }
    out
}

/// A program on another computer can name files on this one and time the
/// failures, so past 8 failures in 10 seconds a tab reads no file it names.
/// A program holding the tab's key (`FLOW_TAB_KEY`, which a remote one may
/// be handed) may ask for a terminal reset (`flow-settings reset
/// terminal`), which takes the tab's pictures away; were the failures
/// forgotten with them, a program could reset after each 8 and go on.
#[test]
fn files_that_failed_to_be_read_stay_counted_through_a_terminal_reset() {
    let platform = &flow_linux::PLATFORM;
    flow_core::shell::set_shell_env(platform.shell_env());
    // An apps' folder of the test's own, never the user's, which a tab
    // opened there would change.
    flow_core::apps::first::set_root(std::env::temp_dir().join(format!("flow-pictures-apps-{}", std::process::id())));
    let dir = std::env::temp_dir().join(format!("flow-pictures-reset-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // Kickstart's picture, 130 × 131 pixels, named by its path, quietly,
    // and eight files that aren't there named the same way.
    let png = dir.join("disk.png");
    std::fs::copy(concat!(env!("CARGO_MANIFEST_DIR"), "/../assets/logos/kickstart.png"), &png).unwrap();
    let named = |path: &Path| format!("\x1b_Ga=T,q=2,t=f,f=100;{}\x1b\\", base64(path.as_os_str().as_encoded_bytes()));
    let show = dir.join("show.kitty");
    std::fs::write(&show, named(&png)).unwrap();
    let fail = dir.join("fail.kitty");
    std::fs::write(&fail, (0..8).map(|n| named(&dir.join(format!("missing-{n}.png")))).collect::<String>()).unwrap();

    let profile = platform.shells().into_iter().next().expect("a shell");
    let label = profile.label.clone();
    let session = Session::spawn(8, profile, GridSize { cols: 120, rows: 30 }, (9.0, 18.0), 2000, None, Waker::from_fn(|_| {}))
        .unwrap_or_else(|e| panic!("{label}: {e:#}"));
    wait_for(&format!("{label} first prompt"), Duration::from_secs(30), || {
        let st = session.state.lock();
        st.integrated && st.at_prompt
    });
    let cat = |file: &Path| {
        let result = run(&session, &label, &format!("cat '{}'", file.display()));
        assert_eq!(result.exit_code, Some(0), "{label}: {result:?}");
    };
    let held = || session.held().1;
    // The picture shows, and a reset takes it away.
    cat(&show);
    assert_eq!(held(), 130 * 131 * 4, "{label}: the picture shows");
    session.reset_terminal();
    assert_eq!(held(), 0, "{label}: a reset takes the picture away");
    // Past eight failures the picture is refused unread, and still after
    // a reset. (Were this to take ten seconds, the failures would be
    // forgotten and the picture shown: how long it took says so.)
    let failed = Instant::now();
    cat(&fail);
    cat(&show);
    assert_eq!(held(), 0, "{label}: refused past eight failures, {:?} after them", failed.elapsed());
    session.reset_terminal();
    cat(&show);
    assert_eq!(held(), 0, "{label}: refused after a reset too, {:?} after the failures", failed.elapsed());
    let _ = std::fs::remove_dir_all(&dir);
}
