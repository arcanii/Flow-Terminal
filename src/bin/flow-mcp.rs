//! `flow-mcp`: Flow as an MCP server for outside apps (Claude Desktop,
//! Claude Code, Cursor), which an app's MCP config runs. It speaks MCP's
//! JSON-RPC over its standard input and output (`flow_core::mcp::protocol`)
//! and hands each tool call to the running Flow over a socket only the user
//! can open, a named pipe on Windows (`flow_core::mcp::bridge`). Flow asks
//! the user, the first time an app calls, what it may do. Where Flow isn't
//! running, `flow-mcp` starts the Flow it came with and waits for it.
//!
//! Nothing but MCP's messages goes to standard output: what it has to say
//! otherwise goes to standard error. Its words to the app's model stay
//! English; `--help` is in the user's language.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::{Duration, Instant};

use flow_core::i18n::{self, t};
use flow_core::mcp::bridge::{self, FromFlow, ToFlow};
use flow_core::mcp::protocol::{self, Outcome};
use flow_core::mcp::ClientInfo;

/// How long a Flow just started is given to listen.
const START_WAIT: Duration = Duration::from_secs(20);
/// How long a Flow running without listening is given before it's taken to
/// have Outside apps off.
const UP_WAIT: Duration = Duration::from_secs(4);

fn main() -> ExitCode {
    let preferred: Vec<String> = ["FLOW_LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"].iter().filter_map(|v| std::env::var(v).ok()).filter(|v| !v.is_empty()).collect();
    i18n::set_language(i18n::AUTO, &preferred);
    match std::env::args().nth(1).as_deref() {
        Some("--help" | "-h" | "help") => {
            println!("{}", t("flow_mcp.usage"));
            if let Ok(me) = std::env::current_exe() {
                println!("\n  {}", me.display());
            }
            return ExitCode::SUCCESS;
        }
        Some("--version") => {
            println!("flow-mcp {}", flow_core::version::display());
            return ExitCode::SUCCESS;
        }
        Some(other) => {
            eprintln!("flow-mcp: {other}?\n\n{}", t("flow_mcp.usage"));
            return ExitCode::from(2);
        }
        None => {}
    }
    let mut server = protocol::Server::default();
    let mut flow: Option<Flow> = None;
    let mut out = io::stdout().lock();
    for line in io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let reply = match server.handle(&line) {
            Outcome::Reply(reply) => reply,
            Outcome::Nothing => continue,
            Outcome::Call { id, tool, args } => {
                let client = server.client.clone().unwrap_or_default();
                protocol::answer(id, call(&mut flow, &client, &tool, args))
            }
        };
        if writeln!(out, "{reply}").and_then(|_| out.flush()).is_err() {
            break;
        }
    }
    ExitCode::SUCCESS
}

/// The running Flow, connected.
struct Flow {
    to: Box<dyn Write>,
    from: BufReader<Box<dyn Read>>,
    next: u64,
}

/// Calls `tool` in Flow, connecting first where it isn't yet (starting it
/// where it isn't running), and once more where the connection was lost (a
/// Flow restarted meanwhile).
fn call(flow: &mut Option<Flow>, client: &ClientInfo, tool: &str, args: serde_json::Value) -> Result<String, String> {
    for attempt in 0..2 {
        if flow.is_none() {
            *flow = Some(connect(client)?);
        }
        let Some(f) = flow.as_mut() else { continue };
        let id = f.next;
        f.next += 1;
        let sent = bridge::write_frame(&mut f.to, &ToFlow::Call { id, tool: tool.to_string(), args: args.clone() });
        match sent.and_then(|_| bridge::read_frame::<FromFlow>(&mut f.from)) {
            Ok(Some(answer)) if answer.id == id => return if answer.ok { Ok(answer.text) } else { Err(answer.text) },
            Ok(Some(_)) => return Err("Flow answered another call.".into()),
            _ if attempt == 0 => *flow = None,
            _ => break,
        }
    }
    *flow = None;
    Err("Flow went away before it answered.".into())
}

/// Connects to Flow and says who's asking. Where nothing answers, a Flow
/// that's running has Outside apps off; one that isn't is started, and
/// waited for. With `FLOW_MCP_SOCKET` set, only that one is tried. Where
/// what answers isn't the user's own Flow, or the system won't let
/// `flow-mcp` in, nothing is sent and no Flow started, which couldn't take
/// the address back nor let it in.
fn connect(client: &ClientInfo) -> Result<Flow, String> {
    let address = bridge::address();
    let open = || open(&address);
    let refused = |e: &io::Error| e.kind() == io::ErrorKind::PermissionDenied;
    let mut flow = match open() {
        Ok(flow) => flow,
        Err(e) if refused(&e) => return Err(kept_out(&e, &address)),
        Err(_) if std::env::var_os("FLOW_MCP_SOCKET").is_some() => return Err(format!("Nothing answers at {}.", address.display())),
        Err(_) if flow_running() => return Err(off()),
        Err(_) => {
            start_flow()?;
            let started = Instant::now();
            loop {
                std::thread::sleep(Duration::from_millis(250));
                match open() {
                    Ok(flow) => break flow,
                    Err(e) if refused(&e) => return Err(kept_out(&e, &address)),
                    Err(_) => {}
                }
                let waited = started.elapsed();
                if waited > START_WAIT || (waited > UP_WAIT && flow_running()) {
                    return Err(off());
                }
            }
        }
    };
    bridge::write_frame(&mut flow.to, &ToFlow::Hello { client: client.clone() }).map_err(|e| e.to_string())?;
    Ok(flow)
}

fn off() -> String {
    "Flow is running, but doesn't let outside apps connect. Ask the user to turn on Settings → Outside apps → Let outside apps \
connect in Flow, then try again."
        .into()
}

/// What the model hears where `flow-mcp` is kept from what's at Flow's
/// address: by its own check of the other end ([`bridge::NotUsersFlow`]),
/// or by the system.
fn kept_out(e: &io::Error, address: &Path) -> String {
    if bridge::NotUsersFlow::is(e) {
        not_flows(address)
    } else {
        not_let_in(address)
    }
}

/// Where something other than the user's own Flow answers at Flow's
/// address (another user's program, or on Windows one of the user's that
/// isn't Flow), which would hear every call and could answer anything.
fn not_flows(address: &Path) -> String {
    format!(
        "What answers at {} isn't the user's own Flow, so flow-mcp sends it nothing: it's another user's, or a program that took \
Flow's address first. Tell the user, who can quit it and start Flow.",
        address.display()
    )
}

/// Where the system won't let `flow-mcp` open Flow's address: Flow's own
/// lets its user in, so it's another user's, or the app runs `flow-mcp`
/// somewhere (a sandbox) that may not reach it.
fn not_let_in(address: &Path) -> String {
    format!(
        "The system won't let flow-mcp open {}: another user may have taken Flow's address first, or the app runs flow-mcp \
somewhere, such as a sandbox, that may not reach Flow. Tell the user.",
        address.display()
    )
}

/// Opens Flow's socket; one another user listens at is refused
/// ([`bridge::NotUsersFlow`]), before anything is sent.
#[cfg(unix)]
fn open(address: &Path) -> io::Result<Flow> {
    let stream = bridge::connect_unix(address)?;
    let reader: Box<dyn Read> = Box::new(stream.try_clone()?);
    Ok(Flow { to: Box::new(stream), from: BufReader::new(reader), next: 1 })
}

/// Opens Flow's pipe (`flow_windows::mcp::open`), letting what serves it
/// know who opened it but never act as them, since whatever serves it
/// isn't known to be Flow until it's open; one this user doesn't own, or
/// served by anything but a `flow.exe` of this user, took its name first,
/// and is refused ([`bridge::NotUsersFlow`]) before anything is sent. One
/// the system won't open is its `PermissionDenied`.
#[cfg(windows)]
fn open(address: &Path) -> io::Result<Flow> {
    let pipe = flow_windows::mcp::open(address)?;
    let reader: Box<dyn Read> = Box::new(pipe.try_clone()?);
    Ok(Flow { to: Box::new(pipe), from: BufReader::new(reader), next: 1 })
}

/// Whether a Flow is running for this user.
fn flow_running() -> bool {
    if cfg!(windows) {
        Command::new("tasklist")
            .args(["/FI", "IMAGENAME eq flow.exe", "/NH"])
            .output()
            .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).to_ascii_lowercase().contains("flow.exe"))
    } else {
        let user = std::env::var("USER").unwrap_or_default();
        let mut pgrep = Command::new("pgrep");
        if !user.is_empty() {
            pgrep.args(["-u", &user]);
        }
        pgrep.args(["-x", "flow"]).stdout(Stdio::null()).status().is_ok_and(|s| s.success())
    }
}

/// The Flow this `flow-mcp` came with: the Mac's bundle it's in, the
/// `flow.exe` above Windows' `games` folder, Linux's `bin/flow` of its
/// prefix, or beside it (a build folder). A release's own layout comes
/// first: there the `flow` beside this program is the `flow` command
/// (`Contents/Helpers/flow`, `games\flow.exe`), which, started with no
/// tab to ask, starts a guest Flow that listens for no outside app.
fn flow_program(me: &Path) -> Option<(PathBuf, bool)> {
    let dir = me.parent()?;
    if cfg!(target_os = "macos") {
        // Flow.app/Contents/Helpers/flow-mcp: the bundle, opened as an app.
        let bundle = dir.parent().and_then(Path::parent).filter(|b| b.extension().is_some_and(|e| e == "app"));
        if let Some(bundle) = bundle {
            return Some((bundle.to_path_buf(), true));
        }
    }
    if cfg!(windows) && dir.file_name().is_some_and(|n| n.eq_ignore_ascii_case("games")) {
        let flow = dir.parent()?.join("flow.exe");
        if flow.is_file() {
            return Some((flow, false));
        }
    }
    let beside = dir.join(format!("flow{}", std::env::consts::EXE_SUFFIX));
    if beside.is_file() {
        return Some((beside, false));
    }
    if cfg!(any(target_os = "macos", windows)) {
        return None;
    }
    // <prefix>/lib/flow-terminal/games/flow-mcp → <prefix>/bin/flow.
    let flow = dir.parent()?.parent()?.parent()?.join("bin").join("flow");
    flow.is_file().then_some((flow, false))
}

/// Starts Flow on its own, outliving this program.
fn start_flow() -> Result<(), String> {
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    let Some((flow, bundle)) = flow_program(&me) else { return Err("Flow isn't running, and flow-mcp couldn't find it to start it.".into()) };
    let mut command = if bundle {
        let mut open = Command::new("/usr/bin/open");
        open.arg("-g").arg(&flow);
        open
    } else {
        Command::new(&flow)
    };
    // An app running in one of Flow's tabs (under tmux, say, outliving
    // that Flow) hands this program the tab's variables: on Linux `flow`
    // with its key is the flow command, which would only ask a Flow that
    // isn't there for a tab.
    flow_core::flow_command::leave_the_tab(&mut command);
    command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // A group of its own, so the app ending flow-mcp's leaves Flow.
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        command.creation_flags(DETACHED_PROCESS);
        // Or Flow holds the app's pipes to this program open while it runs.
        flow_windows::keep_std_handles();
    }
    command.spawn().map(|_| ()).map_err(|e| format!("Flow isn't running, and starting it failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Files at `paths` under a scratch folder of this test's own.
    fn layout(name: &str, paths: &[&str]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("flow-mcp-layout-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for path in paths {
            let file = root.join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, b"").unwrap();
        }
        root
    }

    #[test]
    fn flow_mcp_starts_flow_itself_and_never_the_flow_command_beside_it() {
        let exe = std::env::consts::EXE_SUFFIX;
        // A build folder: Flow is the `flow` beside it.
        let build = layout("build", &[&format!("flow{exe}"), &format!("flow-mcp{exe}")]);
        assert_eq!(flow_program(&build.join(format!("flow-mcp{exe}"))), Some((build.join(format!("flow{exe}")), false)));
        if cfg!(windows) {
            // The setup's and the zip's: games\flow.exe is the flow command.
            let release = layout("windows", &["flow.exe", r"games\flow.exe", r"games\flow-mcp.exe"]);
            assert_eq!(flow_program(&release.join(r"games\flow-mcp.exe")), Some((release.join("flow.exe"), false)));
            let _ = std::fs::remove_dir_all(&release);
        }
        if cfg!(target_os = "macos") {
            // Flow.app/Contents/Helpers/flow is the flow command.
            let app = layout("macos", &["Flow.app/Contents/MacOS/flow", "Flow.app/Contents/Helpers/flow", "Flow.app/Contents/Helpers/flow-mcp"]);
            assert_eq!(flow_program(&app.join("Flow.app/Contents/Helpers/flow-mcp")), Some((app.join("Flow.app"), true)));
            let _ = std::fs::remove_dir_all(&app);
        }
        let _ = std::fs::remove_dir_all(&build);
    }
}
