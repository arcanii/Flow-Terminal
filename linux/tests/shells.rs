//! Spawns the Unix shells (zsh, bash, or sh) in a real PTY, runs commands the way
//! the assistant does, and checks output capture, exit codes and the reported
//! folder through Flow's shell-integration markers, and that the shells get a
//! UTF-8 locale and the games' folder on their PATH, that what `flowapp
//! first` puts first comes ahead of the system's as it was chosen (in some
//! shells, at the prompt alone, a builtin or an alias taken away, in one tab,
//! zsh's builtin `which` too), what each notes for the command history as
//! a person types (none of it a program's output that only looks like the
//! hook's marks), that the `flow` command's words come back as
//! given and its ask carries the pane's key, that letters past ASCII Flow
//! types into a shell without a UTF-8 locale are letters, not keys, that a
//! click on the prompt's line
//! moves the cursor there, that a long command with Devanagari's
//! spacing marks comes back whole with Up and Down, and that a program
//! removed after it ran is found again further along the PATH.
#![cfg(all(unix, not(target_os = "macos")))]

use std::path::PathBuf;
use std::time::{Duration, Instant};

use flow_core::apps::first::{self, Choice, Here, How, Record, Shells, Take};
use flow_core::session::{CommandResult, GridSize, HistoryNote, Session, Waker};
use flow_core::shell::{BashFlavor, ShellKind, ShellProfile};
use flow_core::Platform;

/// The apps' folder this file's tabs go by for what's first, never the
/// user's: a tab opened in it would bring the user's `flowapp first` links
/// into a record of their own there and sweep its panes' folders. The same
/// for every test.
fn apps_root() -> PathBuf {
    std::env::temp_dir().join(format!("flow-shells-apps-{}", std::process::id()))
}

/// As the app does before any shell starts, and with the apps' folder of
/// the tests' own.
fn setup() -> &'static flow_linux::Linux {
    let platform = &flow_linux::PLATFORM;
    flow_core::shell::set_shell_env(platform.shell_env());
    first::set_root(apps_root());
    platform
}

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

#[test]
fn shells_report_output_exit_codes_and_a_utf8_locale() {
    let platform = setup();
    // A games' folder holding a stand-in game: it should be on the PATH after
    // the user's startup files have had their say.
    let games = std::env::temp_dir().join(format!("flow-games-test-{}", std::process::id()));
    std::fs::create_dir_all(&games).unwrap();
    let game = games.join("flow-games-probe");
    std::fs::write(&game, "#!/bin/sh\necho probe-$((40 + 2))\n").unwrap();
    std::fs::set_permissions(&game, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    flow_core::shell::set_games_dir(games.clone());
    // Like Flow started by a launcher that cleared the environment, with no
    // locale set, the shells should still get a UTF-8 one.
    let no_locale = ["LC_ALL", "LC_CTYPE", "LANG"].iter().all(|v| std::env::var(v).map_or(true, |s| s.is_empty()));

    for profile in platform.shells() {
        let label = profile.label.clone();
        let session = Session::spawn(
            1,
            profile,
            GridSize { cols: 120, rows: 30 },
            (9.0, 18.0),
            2000,
            None,
            Waker::from_fn(|_| {}),
        )
        .unwrap_or_else(|e| panic!("{label}: {e:#}"));
        wait_for(&format!("{label} first prompt"), Duration::from_secs(30), || {
            let st = session.state.lock();
            st.integrated && st.at_prompt
        });

        let result = run(&session, &label, "echo flow-marker-42");
        assert!(!result.still_running, "{label}: {result:?}");
        assert!(result.output.contains("flow-marker-42"), "{label}: {:?}", result.output);
        assert_eq!(result.exit_code, Some(0), "{label}: {result:?}");
        assert!(session.cwd().is_some(), "{label}: no cwd reported");

        // Output with no final newline: bash leaves the cursor after it when
        // the done marker comes, while zsh (PROMPT_SP) first ends the line with
        // a '%' mark ('#' as root) and starts a new one. The command line
        // contains the text too, so look for it on its own.
        let output = run(&session, &label, "printf flow-tail-7").output;
        assert!(
            output.lines().any(|l| l.trim().trim_end_matches(['%', '#']) == "flow-tail-7"),
            "{label}: {output:?}"
        );

        assert_eq!(run(&session, &label, "(exit 3)").exit_code, Some(3), "{label}");

        if no_locale {
            let output = run(&session, &label, "echo lang=$LANG").output;
            assert!(output.contains(".UTF-8"), "{label}: {output:?}");
        }

        let output = run(&session, &label, "flow-games-probe").output;
        assert!(output.contains("probe-42"), "{label}: {output:?}");
        eprintln!("{label}: ok (cwd {:?})", session.cwd());
    }
    let _ = std::fs::remove_dir_all(&games);
}

/// A stand-in program under `root`'s `bin/`, saying its name and 42 (worked
/// out, so a command line that echoes isn't taken for it) and what it's
/// given.
fn stand_in(root: &std::path::Path, name: &str) {
    let file = root.join("bin").join(name);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, format!("#!/bin/sh\necho flow-{name}-$((40 + 2)) \"$@\"\n")).unwrap();
    std::fs::set_permissions(&file, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
}

/// Makes `change` to the record under `root` and writes every pane's
/// folder afresh, as `flowapp first` and `last` do.
fn choose(root: &std::path::Path, change: impl FnOnce(&mut Record)) {
    let mut record = Record::load(root);
    change(&mut record);
    record.save(root).unwrap();
    first::rewrite_panes(root);
}

/// The Mac's test of the same name, with `factor` for the Mac's `cal`,
/// which a Linux may not have (Ubuntu's comes with ncal alone).
#[test]
fn what_flowapp_puts_first_comes_ahead_as_chosen_and_open_tabs_follow_it() {
    let platform = setup();
    let root = apps_root();
    // An app of stand-ins for commands the system has (`factor`, which no
    // other test runs, `which` and the builtin `pwd`), as flowapp installs one.
    for name in ["factor", "which", "pwd"] {
        stand_in(&root, name);
    }
    std::fs::write(root.join("installed.json"), r#"{"apps":{"flow-test":{"version":"0.1.0","programs":["factor","which","pwd"]}}}"#).unwrap();
    for (n, profile) in platform.shells().into_iter().enumerate() {
        let label = profile.label.clone();
        let shell = first::shell_name(profile.kind, profile.flavor);
        let other = if shell == "zsh" { "bash" } else { "zsh" };
        let id = 40 + n as u32;
        let session = Session::spawn(id, profile, GridSize { cols: 120, rows: 30 }, (9.0, 18.0), 2000, None, Waker::from_fn(|_| {})).unwrap_or_else(|e| panic!("{label}: {e:#}"));
        wait_for(&format!("{label} first prompt"), Duration::from_secs(30), || {
            let st = session.state.lock();
            st.integrated && st.at_prompt
        });
        // Its own folder, for its shell, leads the PATH.
        let pane = first::pane_dir(&root, std::process::id(), id);
        assert_eq!(first::pane_shell(&pane).as_deref(), Some(shell), "{label}");
        let output = run(&session, &label, "echo \"first=${PATH%%:*}\"").output;
        assert!(output.contains(&format!("first={}", pane.display())), "{label}: its folder leads the PATH: {output:?}");
        let system = |what: &str| {
            let output = run(&session, &label, "factor 2000").output;
            assert!(output.contains("2000: 2 2 2 2 5 5 5"), "{label}: the system's factor, {what}: {output:?}");
        };
        let ours = |command: &str, what: &str| {
            let output = run(&session, &label, command).output;
            assert!(output.contains("flow-factor-42"), "{label}: the factor put first, {what}: {output:?}");
        };
        system("while nothing's first");

        // Put first while the tab is open: the shell forgets where it found
        // `factor` at its next prompt, and a script run from the tab finds it too.
        choose(&root, |r| {
            r.put(Choice::new("factor"));
        });
        run(&session, &label, "true");
        ours("factor", "on the PATH");
        ours("sh -c factor", "in a script");
        // First in the other shell alone: not here. (A choice for some
        // shells leaves the rest as they were, so it goes from all first.)
        choose(&root, |r| {
            r.put_last("factor", &Shells::All);
            r.put(Choice { shells: Shells::Only(vec![other.into()]), ..Choice::new("factor") });
        });
        run(&session, &label, "true");
        system(&format!("first in {other} alone"));
        // At the prompt alone: here, but not in a script.
        choose(&root, |r| {
            r.put(Choice { how: How::Prompt, ..Choice::new("factor") });
        });
        run(&session, &label, "true");
        ours("factor", "at the prompt");
        let output = run(&session, &label, "sh -c 'factor 2000'").output;
        assert!(output.contains("2000: 2 2 2 2 5 5 5"), "{label}: a script keeps the system's: {output:?}");
        choose(&root, |r| {
            r.put_last("factor", &Shells::All);
        });
        run(&session, &label, "true");
        system("taken out again");

        // An alias of the user's stays ahead of what's first, unless taken
        // away, and comes back after.
        run(&session, &label, "alias factor='echo flow-alias-$((40 + 2))'");
        let alias = |what: &str| {
            let output = run(&session, &label, "factor").output;
            assert!(output.contains("flow-alias-42") && !output.contains("flow-factor"), "{label}: the user's alias, {what}: {output:?}");
        };
        choose(&root, |r| {
            r.put(Choice::new("factor"));
        });
        run(&session, &label, "true");
        alias("by default");
        choose(&root, |r| {
            r.put(Choice { take: Some(vec![Take::Alias]), ..Choice::new("factor") });
        });
        run(&session, &label, "true");
        ours("factor", "its alias taken away");
        choose(&root, |r| {
            r.put_last("factor", &Shells::All);
        });
        run(&session, &label, "true");
        alias("given back");
        run(&session, &label, "unalias factor");

        // A builtin stays ahead of what's first unless it's put aside.
        let builtin = |what: &str| {
            let output = run(&session, &label, "cd /tmp && pwd").output;
            assert!(output.lines().any(|l| l.trim() == "/tmp") && !output.contains("flow-pwd"), "{label}: the builtin pwd, {what}: {output:?}");
        };
        choose(&root, |r| {
            r.put(Choice::new("pwd"));
        });
        run(&session, &label, "true");
        builtin("by default");
        choose(&root, |r| {
            r.put(Choice { take: Some(vec![Take::Builtin]), ..Choice::new("pwd") });
        });
        run(&session, &label, "true");
        let output = run(&session, &label, "pwd").output;
        assert!(output.contains("flow-pwd-42"), "{label}: the pwd put first, the builtin put aside: {output:?}");
        choose(&root, |r| {
            r.put_last("pwd", &Shells::All);
        });
        run(&session, &label, "true");
        builtin("given back");

        // In this tab alone (`--here`), kept when another tab's flowapp
        // writes every folder afresh.
        let mut here = Here::load(&pane);
        here.put(Choice::new("factor"));
        here.save(&pane).unwrap();
        first::rewrite_pane(&root, &pane);
        run(&session, &label, "true");
        ours("factor", "in this tab");
        first::rewrite_panes(&root);
        run(&session, &label, "true");
        ours("factor", "in this tab, the others written afresh");
        let mut here = Here::load(&pane);
        here.put_last("factor");
        here.save(&pane).unwrap();
        first::rewrite_pane(&root, &pane);
        run(&session, &label, "true");
        system("put last in this tab");

        // A `which` put first: bash has none of its own, so the PATH's
        // answers; zsh's is a builtin, which hands it a plain `which` of
        // programs and keeps an alias, and its own flags, to itself, unless
        // it's put aside.
        choose(&root, |r| {
            r.put(Choice::new("which"));
        });
        run(&session, &label, "true");
        let output = run(&session, &label, "which cksum").output;
        assert!(output.contains("flow-which-42 cksum"), "{label}: the which put first: {output:?}");
        if shell == "zsh" {
            let output = run(&session, &label, "alias flow-ll=cksum; which flow-ll").output;
            assert!(output.contains("flow-ll: aliased to cksum") && !output.contains("flow-which"), "{label}: an alias is zsh's to say: {output:?}");
            let output = run(&session, &label, "which -p cksum").output;
            assert!(output.contains("/bin/cksum") && !output.contains("flow-which"), "{label}: a flag of zsh's own: {output:?}");
            choose(&root, |r| {
                r.put(Choice { take: Some(vec![Take::Builtin]), ..Choice::new("which") });
            });
            run(&session, &label, "true");
            let output = run(&session, &label, "which -p cksum").output;
            assert!(output.contains("flow-which-42 -p cksum"), "{label}: zsh's which put aside: {output:?}");
        }
        choose(&root, |r| {
            r.put_last("which", &Shells::All);
        });
        run(&session, &label, "true");
        let output = run(&session, &label, "which cksum").output;
        assert!(output.contains("/bin/cksum") && !output.contains("flow-which"), "{label}: the shell's own again: {output:?}");
        drop(session);
        assert!(!pane.exists(), "{label}: its folder goes with it");
        eprintln!("{label}: ok");
    }
    let _ = std::fs::remove_file(root.join("first.json"));
}

/// A program the shell ran from one folder and that's gone since, as an
/// app `flowapp remove` took away (its copy in the apps' `bin/` ahead of
/// Flow's own), is looked for again along the PATH: zsh and ash do so
/// themselves, and bash, which would run the path it remembered and fail,
/// because Flow's hook turns on its `checkhash`.
#[test]
fn a_program_removed_after_it_ran_is_found_again_further_along_the_path() {
    let platform = setup();
    let root = std::env::temp_dir().join(format!("flow-removed-test-{}", std::process::id()));
    let (first, then) = (root.join("first"), root.join("then"));
    std::fs::create_dir_all(&first).unwrap();
    std::fs::create_dir_all(&then).unwrap();
    let further = then.join("flow-removed-probe");
    std::fs::write(&further, "#!/bin/sh\necho probe-$((40 + 2))\n").unwrap();
    std::fs::set_permissions(&further, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    for profile in shells_and_ash(platform) {
        let label = profile.label.clone();
        // The one the shell finds first, put back after the last shell took it away.
        let program = first.join("flow-removed-probe");
        std::fs::write(&program, "#!/bin/sh\necho probe-$((40 + 1))\n").unwrap();
        std::fs::set_permissions(&program, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        let session = Session::spawn(1, profile, GridSize { cols: 120, rows: 30 }, (9.0, 18.0), 2000, None, Waker::from_fn(|_| {}))
            .unwrap_or_else(|e| panic!("{label}: {e:#}"));
        wait_for(&format!("{label} first prompt"), Duration::from_secs(30), || {
            let st = session.state.lock();
            st.integrated && st.at_prompt
        });
        // Linux's BusyBox parts its PATH with colons, though the hook (as on
        // Windows) puts semicolons in what it adds.
        run(&session, &label, &format!("PATH=\"{}:{}:$PATH\"; export PATH", first.display(), then.display()));
        let output = run(&session, &label, "flow-removed-probe").output;
        assert!(output.contains("probe-41"), "{label}: {output:?}");
        std::fs::remove_file(&program).unwrap();
        let result = run(&session, &label, "flow-removed-probe");
        assert!(result.output.contains("probe-42"), "{label}: {result:?}");
        assert_eq!(result.exit_code, Some(0), "{label}: {result:?}");
        eprintln!("{label}: ok");
    }
    let _ = std::fs::remove_dir_all(&root);
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
/// the hook's own word for the line typed stands (where `own_line`, else
/// the screen's), in the history and for the notification. Where the hook
/// says its marks with the nonce (`own_line`), nor does a prompt a program
/// draws while it runs (133;A, 133;B), with hidden words after it, and its
/// "done" (133;D): the answer typed at its `read` isn't a command, and the
/// line typed ends when the shell says. A control character in the line
/// comes whole there, nothing of the mark showing, and a trace (set -x)
/// shows nothing of the nonce either. Nor do the programs the shell starts
/// have the nonce.
fn lines_a_program_says_name_nothing(session: &Session, label: &str, own_line: bool) {
    let forged = r"printf '\033]633;E;flow-forged\007\033]633;E;flow-forged;00\007'";
    let notes = type_and_run(session, label, forged);
    assert_eq!(notes.contains(&HistoryNote::Exact(forged.into())), own_line, "{label}: {notes:?}");
    assert!(!notes.iter().any(|n| matches!(n, HistoryNote::Exact(c) if c.starts_with("flow-forged"))), "{label}: {notes:?}");
    let finished = session.state.lock().finished.clone().and_then(|f| f.command);
    assert_eq!(finished.as_deref(), Some(forged), "{label}");

    if own_line {
        let prompt = r"printf '\033]133;A\007\033]133;B\007\033[8mflow-forged-prompt \033[0m\033]133;D;7\007'; read x; false";
        let notes = type_and_answer(session, label, prompt, "flow-forged-prompt", "y");
        assert!(notes.iter().all(|n| match n {
            HistoryNote::Entered { command, .. } | HistoryNote::Exact(command) => command == prompt,
            HistoryNote::Done(_) => true,
            HistoryNote::LeftOut => false,
        }), "{label}: {notes:?}");
        assert_eq!(notes.iter().filter(|n| matches!(n, HistoryNote::Done(_))).collect::<Vec<_>>(), [&HistoryNote::Done(Some(1))], "{label}: {notes:?}");
        let (finished, failed) = {
            let st = session.state.lock();
            (st.finished.clone().and_then(|f| f.command), st.failed.clone().map(|f| f.command))
        };
        assert_eq!((finished.as_deref(), failed.as_deref()), (Some(prompt), Some(prompt)), "{label}: the notification's and Ask why's");

        // CAN, typed after the line editor's Ctrl+V: raw, it would end the
        // mark early, and the rest of it would show.
        wait_for(&format!("{label} prompt"), Duration::from_secs(10), || session.state.lock().at_prompt);
        session.state.lock().history.clear();
        for key in ["e", "c", "h", "o", " ", "f", "l", "o", "w", "c", "a", "n", "\x16", "\x18", "t", "a", "i", "l"] {
            session.write(key.as_bytes());
            std::thread::sleep(Duration::from_millis(15));
        }
        wait_for(&format!("{label}: the echo of the CAN"), Duration::from_secs(10), || session.cursor_line().1.contains("tail"));
        session.write(b"\r");
        wait_for(&format!("{label}: the CAN's line done"), Duration::from_secs(20), || {
            session.state.lock().history.iter().any(|n| matches!(n, HistoryNote::Done(_)))
        });
        let notes = std::mem::take(&mut session.state.lock().history);
        assert!(notes.contains(&HistoryNote::Exact("echo flowcan\x18tail".into())), "{label}: {notes:?}");
        let screen = session.emu.lock().recent_text(30);
        assert!(!screen.contains("tail;"), "{label}: {screen}");

        // The nonce is 32 letters of hex. So is the machine's id, which
        // systemd's prompt hook (Ubuntu 26.04's /etc/profile.d) traces too.
        for line in ["set -x", "true", "set +x"] {
            type_and_run(session, label, line);
        }
        let machine = std::fs::read_to_string("/etc/machine-id").unwrap_or_default();
        let screen = session.emu.lock().recent_text(60).replace(machine.trim(), "");
        assert!(!screen.split(|c: char| !c.is_ascii_hexdigit()).any(|w| w.len() >= 32), "{label}: {screen}");
    }

    let output = run(session, label, "printenv FLOW_NONCE || printenv __flow_nonce || echo flow-nonce-$((40 + 3))").output;
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
    // Whole, however the line editor wrapped it.
    wait_for(&format!("{label}: the echo of {command:?}"), Duration::from_secs(10), || session.emu.lock().recent_text(4).contains(command));
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

#[test]
fn shells_note_each_command_for_the_history() {
    let platform = setup();
    for profile in platform.shells() {
        let label = profile.label.clone();
        let own_line = matches!(profile.id, "zsh" | "bash");
        let profile_id = profile.id.to_string();
        let session = Session::spawn(2, profile, GridSize { cols: 120, rows: 30 }, (9.0, 18.0), 2000, None, Waker::from_fn(|_| {}))
            .unwrap_or_else(|e| panic!("{label}: {e:#}"));
        wait_for(&format!("{label} first prompt"), Duration::from_secs(30), || {
            let st = session.state.lock();
            st.integrated && st.at_prompt
        });
        let cwd = session.cwd();

        // The line as typed, where it ran, and how it ended; zsh and bash
        // also say the line itself, which beats what the grid shows.
        let notes = type_and_run(&session, &label, "echo flow-history-1; true");
        let entered = notes.iter().find_map(|n| match n {
            HistoryNote::Entered { command, cwd, assistant, .. } => Some((command.clone(), cwd.clone(), *assistant)),
            _ => None,
        });
        assert_eq!(entered, Some(("echo flow-history-1; true".to_string(), cwd.clone(), false)), "{label}: {notes:?}");
        if own_line {
            assert!(notes.contains(&HistoryNote::Exact("echo flow-history-1; true".into())), "{label}: {notes:?}");
        }
        assert_eq!(notes.last(), Some(&HistoryNote::Done(Some(0))), "{label}: {notes:?}");
        lines_a_program_says_name_nothing(&session, &label, own_line);

        let notes = type_and_run(&session, &label, "(exit 4)");
        assert_eq!(notes.last(), Some(&HistoryNote::Done(Some(4))), "{label}: {notes:?}");
        // zsh's line editor reads `#` as a command's start (no
        // interactive_comments), so only bash's comment runs nothing.
        match profile_id.as_str() {
            "zsh" => an_enter_that_runs_nothing_names_nothing_a_background_job_drew(&session, &label, &["", " ", "x\x7f"]),
            "bash" => an_enter_that_runs_nothing_names_nothing_a_background_job_drew(&session, &label, &["", " ", "x\x7f", " #c"]),
            _ => {}
        }

        // A line that starts with a space keeps it, so the history leaves it out.
        let notes = type_and_run(&session, &label, " echo flow-history-quiet");
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

/// bash 4.4 and later send the line as their history keeps it, through PS0.
/// A command continued over two lines comes whole, which the grid's first
/// line wouldn't.
#[test]
fn bash_sends_its_own_line_whole() {
    let platform = setup();
    let Some(profile) = platform.shells().into_iter().find(|p| p.id == "bash") else {
        eprintln!("no bash; skipped");
        return;
    };
    let label = profile.program.display().to_string();
    let session = Session::spawn(3, profile, GridSize { cols: 120, rows: 30 }, (9.0, 18.0), 2000, None, Waker::from_fn(|_| {})).unwrap();
    wait_for(&format!("{label}'s first prompt"), Duration::from_secs(30), || {
        let st = session.state.lock();
        st.integrated && st.at_prompt
    });
    session.state.lock().history.clear();
    for line in ["echo flow-cont-1 \\", "flow-cont-2"] {
        for ch in line.chars() {
            session.write(ch.to_string().as_bytes());
            std::thread::sleep(Duration::from_millis(15));
        }
        wait_for(&format!("{label}: the echo of {line:?}"), Duration::from_secs(10), || session.cursor_line().1.contains(line));
        session.write(b"\r");
    }
    wait_for(&format!("{label}: the continued command done"), Duration::from_secs(20), || {
        session.state.lock().history.iter().any(|n| matches!(n, HistoryNote::Done(_)))
    });
    let notes = std::mem::take(&mut session.state.lock().history);
    assert!(notes.contains(&HistoryNote::Exact("echo flow-cont-1 flow-cont-2".into())), "{label}: {notes:?}");
    // The line once, and nothing from the commands the prompt runs.
    let notes = type_and_run(&session, &label, "true; true");
    assert_eq!(notes.iter().filter(|n| matches!(n, HistoryNote::Exact(_))).count(), 1, "{label}: {notes:?}");
    // Exit codes and $_ are the line's own, whatever the hook ran before
    // its first command.
    let notes = type_and_run(&session, &label, "echo flow-last-arg >/dev/null; [ \"$_\" = flow-last-arg ]");
    assert_eq!(notes.last(), Some(&HistoryNote::Done(Some(0))), "{label}: {notes:?}");
    let notes = type_and_run(&session, &label, "false");
    assert_eq!(notes.last(), Some(&HistoryNote::Done(Some(1))), "{label}: {notes:?}");
    lines_a_program_says_name_nothing(&session, &label, true);
    a_paste_of_two_lines_names_its_first(&session, &label);
    a_line_left_out_names_nothing_a_background_job_drew(&session, &label);
    an_enter_that_runs_nothing_names_nothing_a_background_job_drew(&session, &label, &["", " ", "x\x7f", " #c"]);
    eprintln!("{label}: its own line ok");
}

/// The Mac's of the same name: a paste of two lines, which bash 5 reads
/// one by one with no prompt between them, names its first, as bash's
/// hook says it; the second's, said as only the newest of the history,
/// takes nothing away.
fn a_paste_of_two_lines_names_its_first(session: &Session, label: &str) {
    wait_for(&format!("{label} prompt"), Duration::from_secs(10), || session.state.lock().at_prompt);
    if !session.brackets_pastes() {
        return;
    }
    session.state.lock().history.clear();
    session.write(b"\x1b[200~echo flow-paste-1\nfalse\x1b[201~");
    wait_for(&format!("{label}: the paste's echo"), Duration::from_secs(10), || session.emu.lock().recent_text(5).contains("flow-paste-1"));
    std::thread::sleep(Duration::from_millis(200));
    session.write(b"\r");
    wait_for(&format!("{label}: the paste done"), Duration::from_secs(20), || {
        session.state.lock().history.iter().any(|n| matches!(n, HistoryNote::Done(_)))
    });
    std::thread::sleep(Duration::from_millis(300));
    let st = session.state.lock();
    assert!(st.history.contains(&HistoryNote::Exact("echo flow-paste-1".into())) && !st.history.contains(&HistoryNote::LeftOut), "{label}: {:?}", st.history);
    assert_eq!(st.history.last(), Some(&HistoryNote::Done(Some(1))), "{label}: {:?}", st.history);
    assert_eq!(st.finished.as_ref().and_then(|f| f.command.as_deref()), Some("echo flow-paste-1"), "{label}");
    assert_eq!(st.failed.as_ref().map(|f| f.command.as_str()), Some("echo flow-paste-1"), "{label}");
}

/// The Mac's of the same name: a line that runs nothing (Enter alone,
/// blanks, a letter taken back, a comment) has the hook say no word of it,
/// only its "done", with the last command's code; so what a job left
/// running in the background drew at the prompt, read off the screen at
/// Enter, names nothing, in the history, for the notification or for Ask
/// why, though the last command's code, 1, says it failed.
fn an_enter_that_runs_nothing_names_nothing_a_background_job_drew(session: &Session, label: &str, cases: &[&str]) {
    for keys in cases {
        let what = format!("{label}: {keys:?} and Enter");
        // It stays, since zsh says at once that a job is done, and draws
        // its prompt again.
        type_and_run(session, &what, r"true; (sleep 1; printf '\033]133;A\007\033]133;B\007flow-evil-e #'; sleep 30) & false");
        wait_for(&format!("{what}: the job's prompt"), Duration::from_secs(10), || session.cursor_line().1.contains("flow-evil-e #"));
        {
            let mut st = session.state.lock();
            st.history.clear();
            st.finished = None;
            st.failed = None;
        }
        for k in keys.chars() {
            session.write(k.to_string().as_bytes());
            std::thread::sleep(Duration::from_millis(40));
        }
        std::thread::sleep(Duration::from_millis(200));
        session.write(b"\r");
        wait_for(&format!("{what}: done"), Duration::from_secs(20), || session.state.lock().history.iter().any(|n| matches!(n, HistoryNote::Done(_))));
        std::thread::sleep(Duration::from_millis(200));
        let st = session.state.lock();
        assert!(
            matches!(&st.history[..], [HistoryNote::Entered { command, .. }, HistoryNote::LeftOut, HistoryNote::Done(Some(1))] if command.contains("flow-evil-e #")),
            "{what}: {:?}",
            st.history
        );
        assert!(st.finished.as_ref().is_none_or(|f| f.command.is_none()), "{what}: the notification's: {:?}", st.finished);
        assert!(st.failed.is_none(), "{what}: Ask why's: {:?}", st.failed);
    }
}

/// The Mac's of the same name: a line left out of bash's history
/// (HISTCONTROL) has the hook say only the newest line the history holds,
/// not this one, and certainly not as this one. While the shell waits at
/// its prompt, a job left running in the background can draw a prompt
/// there with words of its own, which Enter would read off the screen as
/// the line: such a line then names nothing, in the history, for the
/// notification or for Ask why. A line that's the newest (a copy
/// ignoredups leaves out, as Ubuntu's ignoreboth does, or one erasedups
/// keeps a number lower) and any other line still name themselves.
fn a_line_left_out_names_nothing_a_background_job_drew(session: &Session, label: &str) {
    let commands = |notes: &[HistoryNote]| -> Vec<String> {
        notes.iter().filter_map(|n| match n {
            HistoryNote::Entered { command, .. } | HistoryNote::Exact(command) => Some(command.clone()),
            _ => None,
        }).collect()
    };
    let named = || {
        let st = session.state.lock();
        (st.finished.clone().and_then(|f| f.command), st.failed.clone().map(|f| f.command))
    };
    run(session, label, "HISTCONTROL=ignorespace");
    let notes = type_and_run(session, label, " echo flow-quiet");
    assert!(!notes.iter().any(|n| matches!(n, HistoryNote::Exact(_))), "{label}: {notes:?}");
    assert!(notes.contains(&HistoryNote::LeftOut), "{label}: {notes:?}");

    type_and_run(session, label, r"true; (sleep 1; printf '\033]133;A\007\033]133;B\007flow-evil-c #') &");
    wait_for(&format!("{label}: the job's prompt"), Duration::from_secs(10), || session.cursor_line().1.contains("flow-evil-c #"));
    let notes = type_and_run(session, label, " false");
    assert_eq!(commands(&notes), ["flow-evil-c # false"], "{label}: as the screen read");
    assert_eq!(notes[1..], [HistoryNote::LeftOut, HistoryNote::Done(Some(1))], "{label}: {notes:?}");
    assert_eq!(named(), (None, None), "{label}: the notification's and Ask why's");

    // Lines the history keeps name themselves as ever.
    let notes = type_and_run(session, label, "echo flow-kept; false");
    assert!(notes.contains(&HistoryNote::Exact("echo flow-kept; false".into())) && !notes.contains(&HistoryNote::LeftOut), "{label}: {notes:?}");
    assert_eq!(named(), (Some("echo flow-kept; false".into()), Some("echo flow-kept; false".into())), "{label}");
    // A copy ignoredups leaves out is the newest line.
    run(session, label, "HISTCONTROL=ignoreboth");
    for _ in 0..2 {
        let notes = type_and_run(session, label, "echo flow-twice; false");
        assert!(!notes.contains(&HistoryNote::LeftOut) && commands(&notes).iter().all(|c| c == "echo flow-twice; false"), "{label}: {notes:?}");
    }
    assert_eq!(named(), (Some("echo flow-twice; false".into()), Some("echo flow-twice; false".into())), "{label}");
    // A line erasedups keeps a number lower, its older copy taken out.
    run(session, label, "HISTCONTROL=erasedups");
    for line in ["echo flow-erased-1; false", "echo flow-erased-2"] {
        type_and_run(session, label, line);
    }
    let notes = type_and_run(session, label, "echo flow-erased-1; false");
    assert!(!notes.contains(&HistoryNote::LeftOut) && commands(&notes).iter().all(|c| c == "echo flow-erased-1; false"), "{label}: {notes:?}");
    assert_eq!(named(), (Some("echo flow-erased-1; false".into()), Some("echo flow-erased-1; false".into())), "{label}");
}

/// Flow's shells here, and the system's BusyBox where it's installed, run
/// as Windows runs its own (`BashFlavor::BusyBox`: `busybox sh -i` with the
/// ash hook). Flow offers no BusyBox on Linux, and that hook parts the PATH
/// with semicolons, as Windows does, so only what's tried with it below
/// holds here: its quoting, its line editor and the hook's marks, with
/// BusyBox's own applets.
fn shells_and_ash(platform: &flow_linux::Linux) -> Vec<ShellProfile> {
    let mut shells = platform.shells();
    match ["/usr/bin/busybox", "/bin/busybox"].into_iter().map(PathBuf::from).find(|p| p.is_file()) {
        Some(program) => shells.push(ShellProfile { id: "bash", label: "BusyBox".into(), kind: ShellKind::Bash, program, flavor: BashFlavor::BusyBox }),
        None => eprintln!("no BusyBox; its ash skipped"),
    }
    shells
}

/// The Mac's test of the same name: the `flow` command's words, quoted for
/// each shell (`shell::quote`) and typed as Flow types them
/// (`Session::run_typed`), reach the program as they were given, none
/// expanded again; and each pane's shell has the pane's own secret, which a
/// request it prints carries back to the session as `flow` asks (OSC
/// 7732). GNU's and BusyBox's `base64` wrap at 76 columns unless `-w0`.
#[test]
fn the_flow_commands_words_come_back_as_they_were_given_and_its_ask_carries_the_panes_key() {
    let platform = setup();
    let odd = [
        "it's $HOME", "*", "~", "a\\b", "", "x=1", "=ls", "\"double\"", "back`tick`", ";rm", "&", "|", "$(echo no)", "!!", "ünï", "a b  c", "%1", "{a,b}", "#hash",
        "a=~", "--x=~/y", "ssh", "-N", "7445:127.0.0.1:7445", "user@host",
    ];
    let words: Vec<String> = ["printf", "<%s>\\n"].iter().chain(odd.iter()).map(|w| w.to_string()).collect();
    for profile in shells_and_ash(platform) {
        let label = profile.label.clone();
        let typed = flow_core::shell::quote(profile.kind, profile.flavor, &words).unwrap();
        let session = Session::spawn(7, profile, GridSize { cols: 250, rows: 40 }, (9.0, 18.0), 2000, None, Waker::from_fn(|_| {}))
            .unwrap_or_else(|e| panic!("{label}: {e:#}"));
        wait_for(&format!("{label} first prompt"), Duration::from_secs(30), || {
            let st = session.state.lock();
            st.integrated && st.at_prompt
        });
        session.state.lock().history.clear();
        session.run_typed(&typed);
        wait_for(&format!("{label}: the words printed"), Duration::from_secs(20), || {
            session.state.lock().history.iter().any(|n| matches!(n, HistoryNote::Done(_)))
        });
        let screen = session.emu.lock().recent_text(40);
        let back: Vec<&str> = screen.lines().map(str::trim_end).filter(|l| l.starts_with('<') && l.ends_with('>')).map(|l| &l[1..l.len() - 1]).collect();
        assert_eq!(back, odd, "{label}: typed {typed}\n{screen}");

        // The pane's secret is in its shell, and a request printed there
        // comes to the session with it.
        assert_eq!(session.key().len(), 32, "{label}");
        let ask = r#"printf '\033]7732;%s\007' "$(printf '{"do":"open-tab","pass":"%s","command":["ls"]}' "$FLOW_TAB_KEY" | base64 -w0)""#;
        wait_for(&format!("{label} prompt again"), Duration::from_secs(10), || session.state.lock().at_prompt);
        session.run_command(ask);
        let mut asks = Vec::new();
        wait_for(&format!("{label}: the ask"), Duration::from_secs(10), || {
            asks.extend(session.take_settings_asks());
            !asks.is_empty()
        });
        let asked = flow_core::flow_command::open_tab_asked(&asks[0].request).unwrap_or_else(|| panic!("{label}: {:?}", asks[0].request));
        assert_eq!(asked["pass"], session.key(), "{label}");
        assert!(asked["command"][0] == "ls", "{label}: {asked}");
        eprintln!("{label}: the flow command's words ok");
    }
}

/// The Mac's test of the same name: letters past ASCII in what Flow types
/// at a prompt (the flow command's words, the assistant's command, a
/// reply's block put at the prompt) reach the program as they were given,
/// and run nothing, in each shell started in a C or POSIX locale or none
/// at all, and with an .inputrc that has readline take each byte past
/// ASCII as Meta and a key. Flow's hook tells bash's readline before each
/// prompt to take every byte as a letter. A dropped file's line for a bash
/// that may not be Flow's (`shell::quote_ascii`) reads back the same. zsh
/// here, unlike the Mac's, makes each byte typed past ASCII a `?`.
#[test]
fn letters_past_ascii_typed_into_a_shell_without_a_utf8_locale_are_letters_not_keys() {
    use flow_core::shell::{quote, quote_ascii};
    let platform = setup();
    let work = std::env::temp_dir().join(format!("flow-c-locale-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    let executable = |path: &std::path::Path, text: String| {
        std::fs::write(path, text).unwrap();
        std::fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    };
    // Writes each of its words after the first, a line each in hex, into
    // the file its first names: in a C locale the grid can't be trusted to
    // read them back as letters.
    let hex_words = work.join("hex-words");
    executable(&hex_words, "#!/bin/sh\nout=$1; shift\nfor w in \"$@\"; do printf '%s' \"$w\" | od -An -tx1 | tr -d ' \\n'; echo; done > \"$out\"\n".into());
    // readline's 8-bit settings as a C locale has them, which the user's
    // own .inputrc may say too.
    let inputrc = work.join("inputrc");
    std::fs::write(&inputrc, "set convert-meta on\nset input-meta off\nset output-meta off\n").unwrap();
    let marker = work.join("ran");
    let odd = [format!("Ētouch {};#", marker.display()), format!("Ē$(touch {})ą", marker.display()), "ünï 日本".to_string(), "ļ'x'!!".to_string()];
    let hex = |w: &str| w.bytes().map(|b| format!("{b:02x}")).collect::<String>();
    let shells: Vec<ShellProfile> = platform.shells();
    let mut n = 0;
    for profile in shells {
        for (locale, set) in [("C", "LC_ALL=C"), ("POSIX", "LC_ALL=POSIX"), ("no", "unset LC_ALL LC_CTYPE LANG;")] {
            n += 1;
            let mut profile = profile.clone();
            let real = profile.program.display().to_string();
            let label = format!("{real} in {locale} locale");
            // The shell started through a script that sets the locale, which
            // the shell's own startup files then don't change.
            let wrapper = work.join(format!("shell-{n}"));
            executable(&wrapper, format!("#!/bin/sh\nexport INPUTRC='{}'\n{set} exec '{real}' \"$@\"\n", inputrc.display()));
            profile.program = wrapper;
            let session = Session::spawn(70 + n, profile.clone(), GridSize { cols: 250, rows: 40 }, (9.0, 18.0), 2000, None, Waker::from_fn(|_| {}))
                .unwrap_or_else(|e| panic!("{label}: {e:#}"));
            wait_for(&format!("{label} first prompt"), Duration::from_secs(30), || {
                let st = session.state.lock();
                st.integrated && st.at_prompt
            });
            let said = run(&session, &label, "echo \"locale=${LC_ALL-}:${LC_CTYPE-}:${LANG-}\"").output;
            let expected = match locale {
                "no" => "locale=::".to_string(),
                l => format!("locale={l}:"),
            };
            assert!(said.lines().any(|l| l.trim().starts_with(&expected)), "{label}: {said:?}");

            // The flow command's words, as it types them, and a dropped
            // file's for a bash that may not be Flow's.
            for bytes in [false, true] {
                let out = work.join(format!("out-{n}-{bytes}"));
                let words: Vec<String> = [hex_words.display().to_string(), out.display().to_string()].into_iter().chain(odd.iter().cloned()).collect();
                let typed = match bytes {
                    false => quote(profile.kind, profile.flavor, &words).unwrap(),
                    true => quote_ascii(profile.kind, profile.flavor, &words).unwrap(),
                };
                wait_for(&format!("{label} prompt again"), Duration::from_secs(10), || session.state.lock().at_prompt);
                session.state.lock().history.clear();
                session.run_typed(&typed);
                wait_for(&format!("{label}: the words written"), Duration::from_secs(20), || {
                    session.state.lock().history.iter().any(|n| matches!(n, HistoryNote::Done(_)))
                });
                std::thread::sleep(Duration::from_millis(300));
                assert!(!marker.exists(), "{label}: typed {typed}, which ran a command");
                // zsh's line editor keeps each byte past ASCII on the Mac,
                // but with glibc in a locale that isn't UTF-8 makes it a
                // `?`, since glibc's C locale has no letter past ASCII to
                // keep it as: the letter is lost, never a key.
                let lost = |w: &str| w.bytes().map(|b| if b.is_ascii() { format!("{b:02x}") } else { "3f".into() }).collect::<String>();
                let kept = |w: &String| if !bytes && profile.kind == ShellKind::Zsh { lost(w) } else { hex(w) };
                let written = std::fs::read_to_string(&out).unwrap_or_default();
                assert_eq!(written.lines().collect::<Vec<_>>(), odd.iter().map(kept).collect::<Vec<_>>(), "{label}: typed {typed}");
            }

            // The assistant's command, as `run_command` types it.
            let command = format!("echo 'Ētouch {} #'", marker.display());
            let result = run(&session, &label, &command);
            std::thread::sleep(Duration::from_millis(300));
            assert!(!marker.exists(), "{label}: {command} ran a command: {result:?}");
            assert_eq!(result.exit_code, Some(0), "{label}: {result:?}");

            // A reply's block put at the prompt, with no Enter: bash 3.2
            // brackets no paste, so it comes as typing.
            wait_for(&format!("{label} prompt again"), Duration::from_secs(10), || session.state.lock().at_prompt);
            session.put_at_prompt(&format!("'Ē$(touch {})ą'", marker.display()));
            std::thread::sleep(Duration::from_millis(800));
            assert!(!marker.exists(), "{label}: the block put at the prompt ran a command");
            assert_eq!(session.at_prompt(), Some(true), "{label}: nothing began");
            eprintln!("{label}: letters past ASCII ok");
        }
    }
    let _ = std::fs::remove_dir_all(&work);
}

/// The Mac's test of the same name: a file dropped on a pane while a bash
/// runs inside it, in a C locale, is typed as ASCII alone
/// (`shell::quote_ascii`, as `App::file_dropped` types it while the pane
/// isn't at the prompt Flow's hook drew), which runs nothing and reads
/// back as the file's name, whatever this bash's readline makes of bytes
/// past ASCII.
#[test]
fn a_file_dropped_while_a_bash_runs_inside_the_pane_runs_nothing() {
    use flow_core::shell::quote_ascii;
    let platform = setup();
    let work = std::env::temp_dir().join(format!("flow-drop-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    let marker = work.join("ran");
    let name = format!("Ē$(touch {})ą", marker.display());
    for (n, profile) in platform.shells().into_iter().filter(|p| p.id == "zsh" || p.id == "bash").enumerate() {
        let label = profile.label.clone();
        let (kind, flavor) = (profile.kind, profile.flavor);
        let session = Session::spawn(170 + n as u32, profile, GridSize { cols: 200, rows: 30 }, (9.0, 18.0), 2000, None, Waker::from_fn(|_| {}))
            .unwrap_or_else(|e| panic!("{label}: {e:#}"));
        wait_for(&format!("{label} first prompt"), Duration::from_secs(30), || {
            let st = session.state.lock();
            st.integrated && st.at_prompt
        });
        assert!(session.types_letters(), "{label}: at the hook's prompt");
        let inner = "LC_ALL=C PS1='flow-inner$ ' bash --norc --noprofile";
        for ch in inner.chars() {
            session.write(ch.to_string().as_bytes());
            std::thread::sleep(Duration::from_millis(10));
        }
        session.write(b"\r");
        wait_for(&format!("{label}: the inner bash's prompt"), Duration::from_secs(10), || session.cursor_line().1.contains("flow-inner$"));
        assert!(!session.types_letters(), "{label}: a program runs");
        let word = quote_ascii(kind, flavor, std::slice::from_ref(&name)).unwrap();
        assert!(word.is_ascii(), "{label}: {word}");
        session.write(format!("{word} ").as_bytes());
        std::thread::sleep(Duration::from_millis(800));
        assert!(!marker.exists(), "{label}: the drop's {word} ran a command");
        let out = work.join(format!("name-{n}"));
        session.write(b"\x05\x15");
        let line = format!("printf %s {word} > '{}'", out.display());
        session.write(format!("{line}\r").as_bytes());
        wait_for(&format!("{label}: the name written"), Duration::from_secs(10), || std::fs::metadata(&out).is_ok_and(|m| m.len() > 0));
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(std::fs::read(&out).unwrap(), name.as_bytes(), "{label}: {line}");
        assert!(!marker.exists(), "{label}");
        session.write(b"exit\r");
        wait_for(&format!("{label}: back at its prompt"), Duration::from_secs(10), || session.types_letters());
        eprintln!("{label}: a drop into a bash inside it ok");
    }
    let _ = std::fs::remove_dir_all(&work);
}

/// The Mac's test of the same name: readline 8.1 and later (bash 5.1 on)
/// put a C or POSIX locale's 8-bit settings back over the hook's at the
/// first prompt after a change into one, which bash's hook says (having
/// brought the environment a readline of its own reads the locale from up
/// to date), and there Flow types no letter past ASCII
/// (`Session::types_letters`, `letter_as_keys`) until a line has run.
/// `bind -v`, read at each prompt, says readline's settings change nowhere
/// else, whether this bash has a readline of its own or its own copy, which
/// reads its variables (exported or not) at once; zsh says nothing. A line
/// with Ē typed where they went back, past Flow's care, says whether this
/// readline takes its bytes as keys there, which is what the care is for.
#[test]
fn readline_puts_a_c_locales_settings_back_only_at_the_prompt_bashs_hook_says_may_take_letters_as_keys() {
    let platform = setup();
    let work = std::env::temp_dir().join(format!("flow-locale-change-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    for (n, profile) in platform.shells().into_iter().filter(|p| p.id == "zsh" || p.id == "bash").enumerate() {
        let label = profile.program.display().to_string();
        let bash = profile.id == "bash";
        let session = Session::spawn(150 + n as u32, profile, GridSize { cols: 120, rows: 30 }, (9.0, 18.0), 2000, None, Waker::from_fn(|_| {}))
            .unwrap_or_else(|e| panic!("{label}: {e:#}"));
        wait_for(&format!("{label} first prompt"), Duration::from_secs(30), || {
            let st = session.state.lock();
            st.integrated && st.at_prompt
        });
        let version = run(&session, &label, "echo flow-v=$((BASH_VERSINFO[0] * 100 + BASH_VERSINFO[1]))").output;
        let resets = bash && version.lines().filter_map(|l| l.trim().strip_prefix("flow-v=")).any(|v| v.parse::<u32>().is_ok_and(|v| v >= 501));
        let at_prompt = |what: &str| {
            wait_for(&format!("{label}: {what}'s prompt"), Duration::from_secs(10), || session.state.lock().at_prompt);
            std::thread::sleep(Duration::from_millis(100));
            (session.types_letters(), session.letter_as_keys("echo 'Ētouch x #'"))
        };
        // readline's own word at the prompt, which reading it uses up.
        let meta = |what: &str| -> Option<bool> {
            if !bash {
                return None;
            }
            let said = run(&session, &label, "bind -v | grep convert-meta").output;
            let on = said.lines().any(|l| l.trim() == "set convert-meta on");
            assert!(on || said.lines().any(|l| l.trim() == "set convert-meta off"), "{label}: after {what}: {said:?}");
            Some(on)
        };
        assert_eq!(at_prompt("the first"), (true, None), "{label}");
        assert_eq!(meta("the first"), bash.then_some(false), "{label}");
        let lines = [
            ("export LANG=C.UTF-8; unset LC_ALL LC_CTYPE", false, Some(false)),
            ("export LC_ALL=C", true, Some(true)),
            ("true", false, Some(false)),
            ("export LC_ALL=C.UTF-8", false, Some(false)),
            ("export LC_ALL=C; /usr/bin/true; export LC_ALL=C.UTF-8", false, Some(false)),
            ("f() { local LC_ALL=C; /usr/bin/true; }; f", false, Some(false)),
            ("LC_ALL=POSIX", true, Some(true)),
            ("echo still", false, Some(false)),
            ("unset LC_ALL", false, Some(false)),
            ("LC_ALL=C", true, None),
            ("export LC_ALL", true, None),
            ("unset LC_ALL", false, Some(false)),
        ];
        for (line, says, back) in lines {
            run(&session, &label, line);
            let said = says && resets;
            assert_eq!(at_prompt(line), (!said, said.then_some('Ē')), "{label}: after {line}");
            if let Some(on) = meta(line) {
                assert!(!on || said, "{label}: readline put them back at the prompt after {line}, which Flow wasn't told of");
                assert!(back.is_none_or(|back| on == (back && resets)), "{label}: readline at the prompt after {line}: {on}");
            }
            if said {
                assert_eq!(at_prompt("the next"), (true, None), "{label}: after {line}, for that prompt alone");
                assert_eq!(meta("the next"), Some(false), "{label}: after {line}, the hook's settings again");
            }
        }
        if resets {
            run(&session, &label, "export LC_ALL=C");
            assert_eq!(at_prompt("export LC_ALL=C"), (false, Some('Ē')), "{label}");
            let marker = work.join(format!("keys-{n}"));
            let said = run(&session, &label, &format!("echo 'Ētouch {} #'; bind -v | grep convert-meta", marker.display())).output;
            std::thread::sleep(Duration::from_millis(300));
            let keys = marker.exists();
            assert!(keys || said.lines().any(|l| l.trim() == "set convert-meta on"), "{label}: not where readline put them back: {said:?}");
            eprintln!("{label}: where readline put a C locale's settings back, it took Ē's bytes as {}", if keys { "keys" } else { "letters" });
            assert_eq!(at_prompt("the next"), (true, None), "{label}");
        }
        eprintln!("{label}: a change of locale ok ({})", if resets { "said" } else { "nothing to say" });
    }
    let _ = std::fs::remove_dir_all(&work);
}

/// The Mac's test of the same name: a click on the line typed at the prompt
/// moves each shell's cursor there (`Session::click_to_move`), so what's
/// typed after it lands at the click, and a click on the line above does
/// nothing. zsh and bash read the user's startup files, so a prompt theme
/// of theirs is among what's tried (Ubuntu's zshrc has promptinit's
/// adam1); `tests/theme.rs` tries bash's array `PROMPT_COMMAND` and a zsh
/// theme set in `precmd`.
#[test]
fn a_click_on_the_prompts_line_moves_each_shells_cursor_there() {
    let platform = setup();
    for profile in shells_and_ash(platform) {
        let label = profile.label.clone();
        let session = Session::spawn(8, profile, GridSize { cols: 120, rows: 30 }, (9.0, 18.0), 2000, None, Waker::from_fn(|_| {}))
            .unwrap_or_else(|e| panic!("{label}: {e:#}"));
        wait_for(&format!("{label} first prompt"), Duration::from_secs(30), || {
            let st = session.state.lock();
            st.integrated && st.at_prompt
        });
        click_moves_the_cursor(&session, &label);
        eprintln!("{label}: ok");
    }
}

/// Types `echo flow-click-ab`, clicks on its `b` (and first on the line
/// above, which does nothing), types `X` and Enter, and waits for
/// `flow-click-aXb`.
fn click_moves_the_cursor(session: &Session, label: &str) {
    use alacritty_terminal::index::{Column, Point};
    for ch in "echo flow-click-ab".chars() {
        session.write(ch.to_string().as_bytes());
        std::thread::sleep(Duration::from_millis(15));
    }
    wait_for(&format!("{label}: the echo"), Duration::from_secs(10), || session.cursor_line().1.contains("flow-click-ab"));
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
}

/// The Mac's test of the same name: Up and Down over a command long enough
/// to wrap, with Devanagari's vowel sign ा in it, put it back whole in
/// each shell. On the Mac the shells measure that spacing mark as no
/// width, so Flow puts it on the letter before (`flow_core::combining`);
/// here glibc's `wcwidth`, as unicode-width does, gives it a cell, so the
/// emulator and the shells agree with nothing joined. zsh writes the line
/// as typed only because Flow's hook turns on COMBINING_CHARS (`ZSH_RC`),
/// which Ubuntu's /etc/zsh leaves off: without it, its line editor writes
/// each mark of no width as `<094d>`. BusyBox's ash is left out: Ubuntu's
/// builds show nothing past U+02FF but `?`, and break a full line onto the
/// next themselves, so there's nothing of the line to compare.
#[test]
fn a_long_command_with_spacing_marks_comes_back_whole_with_up_and_down() {
    use alacritty_terminal::term::TermMode;
    let platform = setup();
    let long = format!("echo \"नमस्ते, मेरा नाम साई है।\" {}", "b".repeat(40));
    for profile in platform.shells() {
        let label = profile.label.clone();
        let zsh = matches!(profile.kind, ShellKind::Zsh);
        let session = Session::spawn(8, profile, GridSize { cols: 60, rows: 20 }, (9.0, 18.0), 2000, None, Waker::from_fn(|_| {}))
            .unwrap_or_else(|e| panic!("{label}: {e:#}"));
        wait_for(&format!("{label} first prompt"), Duration::from_secs(30), || {
            let st = session.state.lock();
            st.integrated && st.at_prompt
        });
        let typed = |text: &str| {
            for ch in text.chars() {
                session.write(ch.to_string().as_bytes());
                std::thread::sleep(Duration::from_millis(10));
            }
        };
        // A history of zsh's own, kept in no file (`fc -p`): the user's
        // files may share zsh's history between shells (the .zshrc Ubuntu
        // recommends to a new user does), which would bring what the other
        // tests' shells run meanwhile into this one's, under its Up.
        if zsh {
            typed("fc -p\r");
            wait_for(&format!("{label}: a history of its own"), Duration::from_secs(10), || session.state.lock().at_prompt);
        }
        let key = |up: bool| {
            let app = session.emu.lock().term.mode().contains(TermMode::APP_CURSOR);
            session.write(match (app, up) {
                (true, true) => b"\x1bOA",
                (true, false) => b"\x1bOB",
                (false, true) => b"\x1b[A",
                (false, false) => b"\x1b[B",
            });
        };
        // The line being edited, and the one above it.
        let last_two = || {
            let text = session.emu.lock().recent_text(6);
            let lines: Vec<String> = text.lines().map(|l| l.trim_end().to_string()).filter(|l| !l.is_empty()).collect();
            let n = lines.len();
            (lines.get(n.wrapping_sub(2)).cloned().unwrap_or_default(), lines.last().cloned().unwrap_or_default())
        };
        typed(&long);
        wait_for(&format!("{label}: the long line typed whole"), Duration::from_secs(10), || last_two().1.ends_with(&long));
        typed("\r");
        wait_for(&format!("{label}: its output"), Duration::from_secs(10), || session.emu.lock().recent_text(8).contains("है। bbbb"));
        wait_for(&format!("{label} prompt"), Duration::from_secs(10), || session.state.lock().at_prompt);
        typed("echo short\r");
        wait_for(&format!("{label}: short"), Duration::from_secs(10), || last_two().0 == "short" && session.state.lock().at_prompt);
        for (up, want) in [(true, "echo short"), (true, long.as_str()), (false, "echo short"), (true, long.as_str())] {
            key(up);
            wait_for(&format!("{label}: {} to {want}", if up { "Up" } else { "Down" }), Duration::from_secs(10), || {
                let (above, line) = last_two();
                above == "short" && line.ends_with(want) && !line.contains(&format!("नम{want}"))
            });
        }
        eprintln!("{label}: ok");
    }
}
