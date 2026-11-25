//! A theme's prompt is text, never shell code (`looks::prompt_for` and the
//! hooks): zsh and bash, Homebrew's and the Mac's own 3.2, in a real PTY
//! show every letter of one full of what shells expand as itself, and run
//! none of it, whatever the user's files set: zsh's prompt_subst,
//! prompt_bang, glob_assign and glob_subst, or prompt_subst turned on only
//! once the prompt is set, bash's posix mode or promptvars off, a locale
//! whose letters may end in a backslash's byte (Shift JIS, GB18030), or a
//! PS1 exported (with bash's set -a, zsh's all_export) to the shells
//! started in the tab, which would expand it in their own way. In a test
//! binary of its own, since it points HOME at those files and the prompt is
//! set for every shell started.
#![cfg(target_os = "macos")]

use std::time::{Duration, Instant};

use flow_core::session::{GridSize, Session, Waker};
use flow_core::shell::ShellProfile;
use flow_core::Platform;

fn wait_for(what: &str, mut f: impl FnMut() -> bool) {
    let start = Instant::now();
    while !f() {
        assert!(start.elapsed() < Duration::from_secs(20), "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Waits for the cursor's line, at the prompt, to hold `want`, and says
/// what it held if not.
fn wait_for_line(session: &Session, label: &str, want: &str) {
    let start = Instant::now();
    while !(session.state.lock().at_prompt && session.cursor_line().1.contains(want)) {
        assert!(start.elapsed() < Duration::from_secs(20), "{label}: no {want:?} in {:?}", screen(session));
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Everything on the screen and above it.
fn screen(session: &Session) -> String {
    let emu = session.emu.lock();
    emu.text_between(0, emu.cursor_abs_line() + 1)
}

/// Whether `screen` holds a complaint naming one of Flow's hooks, as zsh
/// (`__flow_precmd:11: …`) and bash (`bash: __flow_prompt: …`) write one.
/// bash with promptvars off shows PS0's `$(__flow_line)` as it is, which
/// isn't one.
fn complains(screen: &str) -> bool {
    screen.match_indices("__flow_").any(|(i, _)| {
        let name = &screen[i..];
        name.trim_start_matches(|c: char| c.is_ascii_alphanumeric() || c == '_').starts_with(':')
    })
}

/// Each would make a file in the tab's folder (its home) if a shell ran
/// it, or show something else if it expanded it. ぁ's last byte, read as
/// Shift JIS or GB18030, begins a letter whose second byte may be a
/// backslash, so it would take one meant to keep what follows from running.
/// The last pair of backticks runs its file even where what's between
/// them is bash's escapes around them, as dash would read bash's PS1.
const PROMPT: &str = r#"$(touch ran-1) `touch ran-2` ${HOME:+home} $((6*7)) !! ! %% \ \$ " 'q' ~ ぁ$(touch ran-3) ぁ`touch ran-4` ぁ\ `x;touch ran-7;` {cwd}>"#;

/// A glob qualifier runs its code for each file its pattern matches, so a
/// prompt that zsh globbed would make a file.
const GLOB: &str = "*(e:touch ran-5:)";

/// Typed as a command after the first prompt.
const TYPED: &[u8] = b"true\r";

/// Typed where zsh globs what's assigned: a comment, so no shell runs it,
/// unless a hook globbed the line it was given, which as a pattern matches
/// the file made for it (`true # x`).
const COMMENTED: &[u8] = b"true # *(e:touch ran-6:)\r";

/// Typed where the user's files export PS1: shells that take a prompt from
/// their environment and expand it, which would run the theme's prompt if
/// Flow's PS1 reached them.
const CHILD_SHELLS: &[u8] = b"/bin/dash -i </dev/null; /bin/bash --norc -i </dev/null\r";

/// A .zshrc that turns prompt_subst on once the first prompt is set (in a
/// widget run as the line editor starts, as a loader that defers a
/// framework's setup does it), and draws that prompt again.
const LATE_SUBST: &str = "__late() { setopt prompt_subst; zle reset-prompt; zle -D zle-line-init; }\nzle -N zle-line-init __late\n";

#[test]
fn a_themes_prompt_shows_as_written_and_runs_nothing_whatever_the_shells_options() {
    let home = std::env::temp_dir().join(format!("flow-prompt-text-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    // As the shells see the folder (/private/var, not /var), so they write it as ~.
    let home = home.canonicalize().unwrap();
    // Files for a glob's * to match, and for COMMENTED as a pattern.
    std::fs::write(home.join("somefile"), "").unwrap();
    std::fs::write(home.join("true # x"), "").unwrap();
    std::env::set_var("HOME", &home);
    // None of what the terminal running the tests may have set.
    for name in ["ZDOTDIR", "XDG_CONFIG_HOME", "FLOW_PROMPT", "FLOW_SKIP_PROFILE"] {
        std::env::remove_var(name);
    }
    let platform = &flow_macos::PLATFORM;
    flow_core::shell::set_shell_env(platform.shell_env());
    flow_core::apps::first::set_root(home.join("apps"));

    let mut shells = platform.shells();
    if let Some(bash) = shells.iter().find(|p| p.id == "bash" && p.program != std::path::Path::new("/bin/bash")).cloned() {
        shells.push(ShellProfile { program: "/bin/bash".into(), label: "/bin/bash".into(), ..bash });
    }
    let passes = [
        ("zsh", "", "as it starts", PROMPT, TYPED),
        ("zsh", "setopt prompt_subst prompt_bang warn_nested_var warn_create_global\n", "with prompt_subst, prompt_bang, warn_nested_var and warn_create_global", PROMPT, TYPED),
        ("zsh", "setopt glob_assign glob_subst warn_nested_var interactive_comments\n", "with glob_assign and glob_subst", PROMPT, COMMENTED),
        ("zsh", "setopt glob_assign glob_subst interactive_comments\n", "globbing what's assigned", GLOB, COMMENTED),
        ("zsh", "setopt glob_assign glob_subst prompt_subst prompt_bang extended_glob interactive_comments\n", "globbing with prompt_subst", GLOB, COMMENTED),
        ("zsh", "export LC_ALL=ja_JP.SJIS\nsetopt prompt_subst\n", "in Shift JIS with prompt_subst", PROMPT, TYPED),
        ("zsh", LATE_SUBST, "with prompt_subst turned on after the prompt is set", PROMPT, TYPED),
        ("zsh", "export PS1='%~ %# '\nsetopt all_export\n", "with PS1 exported", PROMPT, CHILD_SHELLS),
        ("bash", "", "as it starts", PROMPT, TYPED),
        ("bash", "set -o posix\n", "in posix mode", PROMPT, TYPED),
        ("bash", "shopt -u promptvars\n", "with promptvars off", PROMPT, TYPED),
        ("bash", "export LC_ALL=ja_JP.SJIS\n", "in Shift JIS", PROMPT, TYPED),
        ("bash", "export LC_ALL=zh_CN.GB18030\nset -o posix\n", "in GB18030 and posix mode", PROMPT, COMMENTED),
        ("bash", "set -a\nexport PS1='\\w \\$ '\n", "with PS1 exported and set -a", PROMPT, CHILD_SHELLS),
    ];
    let mut n = 0;
    for (id, rc, what, prompt, typed) in passes {
        flow_core::shell::set_prompt(Some(prompt));
        // The tab opens in its home, which both shells show as ~.
        let shown = prompt.replace("{cwd}", "~");
        std::fs::write(home.join(".zshrc"), if id == "zsh" { rc } else { "" }).unwrap();
        std::fs::write(home.join(".bashrc"), if id == "bash" { rc } else { "" }).unwrap();
        for profile in shells.iter().filter(|p| p.id == id).cloned() {
            n += 1;
            let label = format!("{} {what}", profile.label);
            let session = Session::spawn(20 + n, profile, GridSize { cols: 200, rows: 30 }, (9.0, 18.0), 2000, Some(home.clone()), Waker::from_fn(|_| {}))
                .unwrap_or_else(|e| panic!("{label}: {e:#}"));
            wait_for(&format!("{label} first prompt"), || {
                let st = session.state.lock();
                st.integrated && st.at_prompt
            });
            wait_for_line(&session, &label, &shown);
            // Drawn again after a command, as prompt_subst expands it each time.
            session.state.lock().history.clear();
            session.write(typed);
            wait_for(&format!("{label}: the command done"), || {
                let st = session.state.lock();
                st.at_prompt && !st.history.is_empty()
            });
            wait_for_line(&session, &format!("{label}, again"), &shown);
            for ran in ["ran-1", "ran-2", "ran-3", "ran-4", "ran-5", "ran-6", "ran-7"] {
                assert!(!home.join(ran).exists(), "{label}: the prompt ran {ran}");
            }
            // zsh's hook puts the four letters its prompt brings from psvar
            // last in it before each prompt, and takes away the four from
            // the one before.
            if id == "zsh" {
                session.write(b"print -r -- psvar:${#psvar}\r");
                wait_for(&format!("{label}: psvar's length"), || screen(&session).contains("\npsvar:4\n"));
            }
            // Nor did Flow's hooks complain (zsh's warn_nested_var, say).
            let all = screen(&session);
            assert!(!complains(&all), "{label}: {all:?}");
            eprintln!("{label}: {:?}", session.cursor_line().1.trim_end());
        }
    }
    flow_core::shell::set_prompt(None);
    let _ = std::fs::remove_dir_all(&home);
}
