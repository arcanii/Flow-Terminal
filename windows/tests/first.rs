//! What `flowapp first` puts first comes ahead of the system's commands in
//! the shells of Windows as it was chosen, through ConPTY as in a tab:
//! PowerShell's own aliases and functions taken away (an alias of the name to
//! the program put first comes ahead of them) and given back, kept with
//! `--take none`, and an alias at the prompt alone; cmd, whose PATH has its
//! pane's folder first and whose own commands nothing puts aside; the bundled
//! BusyBox, whose applets come ahead of the PATH but for
//! BB_OVERRIDE_APPLETS, read as the tab opens and after flowapp runs in it;
//! and Git Bash, whose own `ls` alias stays unless it's taken away. The
//! stand-ins are copies of System32's `hostname.exe`, which says the
//! computer's name when given nothing. A process of its own, as the apps'
//! folder the panes go by is process-wide (`apps::first::set_root`).
#![cfg(windows)]

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use flow_core::apps::first::{self, Choice, How, Record, Shells, Take};
use flow_core::session::{CommandResult, GridSize, Session, Waker};
use flow_core::shell::{BashFlavor, ShellKind, ShellProfile};
use flow_core::Platform;

fn wait_for(what: &str, timeout: Duration, mut f: impl FnMut() -> bool) {
    let start = Instant::now();
    while !f() {
        assert!(start.elapsed() < timeout, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

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

fn open(id: u32, profile: ShellProfile) -> Session {
    let label = profile.label.clone();
    let session = Session::spawn(id, profile, GridSize { cols: 160, rows: 30 }, (9.0, 18.0), 2000, None, Waker::from_fn(|_| {}))
        .unwrap_or_else(|e| panic!("{label}: {e:#}"));
    wait_for(&format!("{label} first prompt"), Duration::from_secs(30), || {
        let st = session.state.lock();
        st.integrated && st.at_prompt
    });
    session
}

/// The apps' folder of the test's own, never the user's.
fn root() -> PathBuf {
    std::env::temp_dir().join(format!("flow-first-test-{}", std::process::id()))
}

fn system32() -> PathBuf {
    PathBuf::from(std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into())).join("System32")
}

/// What a stand-in says when it runs: the computer's name, as System32's
/// own `hostname` says it.
fn host() -> String {
    let out = std::process::Command::new(system32().join("hostname.exe")).output().unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_lowercase()
}

/// An app, `flow-test`, of stand-ins named `names`, as flowapp installs one.
fn install(root: &Path, names: &[&str]) {
    std::fs::create_dir_all(root.join("bin")).unwrap();
    for name in names {
        std::fs::copy(system32().join("hostname.exe"), root.join("bin").join(format!("{name}.exe"))).unwrap();
    }
    let programs: Vec<String> = names.iter().map(|n| format!("\"{n}\"")).collect();
    std::fs::write(root.join("installed.json"), format!(r#"{{"apps":{{"flow-test":{{"version":"0.1.0","programs":[{}]}}}}}}"#, programs.join(","))).unwrap();
}

/// Makes `change` to the record and writes every pane's folder afresh, as
/// `flowapp first` and `last` do.
fn choose(root: &Path, change: impl FnOnce(&mut Record)) {
    let mut record = Record::load(root);
    change(&mut record);
    record.save(root).unwrap();
    first::rewrite_panes(root);
}

/// Only `name`, in every shell, first in the way `how`, taking `take`.
fn only(record: &mut Record, name: &str, how: How, take: Option<Vec<Take>>) {
    record.put_last(name, &Shells::All);
    record.put(Choice { how, take, ..Choice::new(name) });
}

#[test]
fn what_flowapp_puts_first_comes_ahead_in_each_windows_shell_as_chosen() {
    let root = root();
    let _ = std::fs::remove_dir_all(&root);
    first::set_root(root.clone());
    install(&root, &["ls", "mkdir", "tree", "dir", "ipcalc", "pwd"]);
    let host = host();
    let ran = |output: &str| output.to_lowercase().contains(&host);
    // A program of the user's own apps (`apps\bin`, on every tab's PATH
    // after the system's folders) may have a stand-in's name, so a path to
    // what the test puts first is told by the test's folder.
    let ours = root.file_name().unwrap().to_string_lossy().to_lowercase();
    let from_ours = |output: &str| output.to_lowercase().contains(&ours);
    let shells = flow_windows::PLATFORM.shells();

    // PowerShell: its own alias of `ls` goes by default, and comes back.
    let ps = shells.iter().find(|p| p.kind == ShellKind::PowerShell).cloned().expect("PowerShell");
    let session = open(1, ps);
    let label = "PowerShell";
    let definition = |name: &str| run(&session, label, &format!("(Get-Command {name}).Definition")).output;
    // A profile's own PSConsoleHostReadLine may rewrite a name as it's
    // typed (coreutils for Windows' turns `ls` and `mkdir` into its own
    // programs, by their paths), so those go through Invoke-Expression,
    // which looks the name up as typing it would.
    let typed = |name: &str| run(&session, label, &format!("Invoke-Expression {name}")).output;
    assert!(definition("ls").contains("Get-ChildItem"), "{label}: its own alias, while nothing's first");
    choose(&root, |r| only(r, "ls", How::Path, None));
    run(&session, label, "$null");
    let output = typed("ls");
    assert!(ran(&output), "{label}: the ls put first, its alias taken away: {output:?}");
    assert!(definition("ls").contains("ls.exe"), "{label}: an alias of it to the program put first");
    // --take none keeps its alias ahead.
    choose(&root, |r| only(r, "ls", How::Path, Some(vec![])));
    run(&session, label, "$null");
    assert!(definition("ls").contains("Get-ChildItem"), "{label}: kept with --take none");
    // Its function `mkdir` stays unless --take function.
    choose(&root, |r| only(r, "mkdir", How::Path, None));
    run(&session, label, "$null");
    let kind = run(&session, label, "(Get-Command mkdir).CommandType").output;
    assert!(kind.contains("Function"), "{label}: its own function stays by default: {kind:?}");
    choose(&root, |r| only(r, "mkdir", How::Path, Some(vec![Take::Function])));
    run(&session, label, "$null");
    let output = typed("mkdir");
    assert!(ran(&output), "{label}: the mkdir put first, its function taken away: {output:?}");
    choose(&root, |r| {
        r.put_last("mkdir", &Shells::All);
    });
    run(&session, label, "$null");
    let kind = run(&session, label, "(Get-Command mkdir).CommandType").output;
    assert!(kind.contains("Function"), "{label}: its function given back: {kind:?}");
    // At the prompt alone: an alias here, while a child shell keeps the
    // system's tree.com.
    choose(&root, |r| only(r, "tree", How::Prompt, None));
    run(&session, label, "$null");
    let output = run(&session, label, "tree").output;
    assert!(ran(&output), "{label}: the tree put first at the prompt: {output:?}");
    let output = run(&session, label, "powershell -NoProfile -Command \"(Get-Command tree).Source\"").output;
    assert!(output.to_lowercase().contains("tree.com"), "{label}: a script keeps the system's: {output:?}");
    choose(&root, |r| {
        r.put_last("tree", &Shells::All);
        r.put_last("ls", &Shells::All);
    });
    run(&session, label, "$null");
    assert!(definition("ls").contains("Get-ChildItem"), "{label}: its own alias given back");
    assert!(definition("tree").to_lowercase().contains("tree.com"), "{label}: the system's tree again");
    drop(session);

    // cmd: its pane's folder first on its PATH, which it searches each time;
    // its own `dir` nothing puts aside; nothing first at its prompt alone.
    let cmd = shells.iter().find(|p| p.kind == ShellKind::Cmd).cloned().expect("cmd");
    let session = open(2, cmd);
    let label = "cmd";
    let output = run(&session, label, "where tree").output;
    let found = output.lines().map(|l| l.trim().to_lowercase()).find(|l| l.ends_with("tree.com") || l.ends_with("tree.exe"));
    assert!(found.is_some_and(|l| l.ends_with("tree.com")) && !from_ours(&output), "{label}: the system's, while nothing's first: {output:?}");
    choose(&root, |r| {
        only(r, "tree", How::Path, None);
        only(r, "dir", How::Path, None);
    });
    let output = run(&session, label, "tree").output;
    assert!(ran(&output), "{label}: the tree put first: {output:?}");
    let output = run(&session, label, r"dir /b %SystemRoot%\System32\hostname.exe").output;
    assert!(output.to_lowercase().contains("hostname.exe"), "{label}: its own dir, not the one put first: {output:?}");
    choose(&root, |r| only(r, "tree", How::Prompt, None));
    let output = run(&session, label, "where tree").output;
    assert!(!from_ours(&output), "{label}: no hook at its prompt, so not first there: {output:?}");
    choose(&root, |r| {
        r.put_last("tree", &Shells::All);
        r.put_last("dir", &Shells::All);
    });
    drop(session);

    // BusyBox reads what's first as a tab opens: a stand-in there then is run
    // in place of its applet (ipcalc, which no Windows program is named
    // after, so taken away it's found nowhere on the PATH).
    let busybox = flow_windows::bundled_busybox().expect("windows/third_party/busybox/busybox64u.exe");
    choose(&root, |r| only(r, "ipcalc", How::Path, None));
    let session = open(3, busybox);
    let label = "BusyBox";
    let output = run(&session, label, "ipcalc").output;
    assert!(ran(&output), "{label}: the one put first, its applet taken away: {output:?}");
    // --take none keeps the applet, once flowapp runs in the tab (not found
    // here, but the function around it looks afresh all the same).
    choose(&root, |r| only(r, "ipcalc", How::Path, Some(vec![])));
    run(&session, label, "flowapp");
    let output = run(&session, label, "ipcalc -m 10.0.0.1").output;
    assert!(output.contains("NETMASK=255.0.0.0"), "{label}: its own applet, kept: {output:?}");
    // At the prompt alone: an alias here, the applet in a script.
    choose(&root, |r| only(r, "ipcalc", How::Prompt, None));
    run(&session, label, "flowapp");
    let output = run(&session, label, "ipcalc").output;
    assert!(ran(&output), "{label}: the one put first at the prompt: {output:?}");
    let output = run(&session, label, "sh -c 'ipcalc -m 10.0.0.1'").output;
    assert!(output.contains("NETMASK=255.0.0.0"), "{label}: a script keeps the applet: {output:?}");
    // Flow's own alias of ls (ls --color=auto) stays unless taken away.
    choose(&root, |r| {
        r.put_last("ipcalc", &Shells::All);
        only(r, "ls", How::Path, Some(vec![Take::Alias, Take::Applet]));
    });
    run(&session, label, "flowapp");
    let output = run(&session, label, "ls").output;
    assert!(ran(&output), "{label}: the ls put first, its alias and applet taken away: {output:?}");
    choose(&root, |r| {
        r.put_last("ls", &Shells::All);
    });
    run(&session, label, "flowapp");
    let output = run(&session, label, "alias ls").output;
    assert!(output.contains("--color=auto"), "{label}: its alias given back: {output:?}");
    drop(session);

    // Git Bash, where Git's bash.bashrc makes ls an alias (ls -F
    // --color=auto): it stays ahead unless taken away, and comes back.
    match shells.iter().find(|p| p.flavor == BashFlavor::GitBash).cloned() {
        Some(git) => {
            let session = open(4, git);
            let label = "Git Bash";
            choose(&root, |r| only(r, "ls", How::Path, None));
            run(&session, label, "true");
            let output = run(&session, label, "type -t ls; type -P ls").output;
            // MSYS's bash names a program without its .exe.
            assert!(output.contains("alias") && from_ours(&output), "{label}: its alias, ahead of the ls put first: {output:?}");
            choose(&root, |r| only(r, "ls", How::Path, Some(vec![Take::Alias])));
            run(&session, label, "true");
            let output = run(&session, label, "ls").output;
            assert!(ran(&output), "{label}: the ls put first, its alias taken away: {output:?}");
            // A builtin put aside.
            choose(&root, |r| only(r, "pwd", How::Path, Some(vec![Take::Builtin])));
            run(&session, label, "true");
            let output = run(&session, label, "pwd").output;
            assert!(ran(&output), "{label}: the pwd put first, the builtin put aside: {output:?}");
            choose(&root, |r| {
                r.put_last("ls", &Shells::All);
                r.put_last("pwd", &Shells::All);
            });
            run(&session, label, "true");
            let output = run(&session, label, "type -t ls; type -t pwd").output;
            assert!(output.contains("alias") && output.contains("builtin"), "{label}: given back: {output:?}");
        }
        None => eprintln!("no Git Bash here: PowerShell, cmd and BusyBox tried"),
    }
    let _ = std::fs::remove_dir_all(&root);
}
