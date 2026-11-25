//! `flow-mcp` run for real on Windows, against Flow's own pipe for outside
//! apps (`flow_windows::mcp`), which `tests/mcp.rs` can't reach: `flow-mcp`
//! answers only a pipe its own user owns and a `flow.exe` of theirs serves,
//! so a copy of this test program named `flow.exe` serves it as the
//! stand-in Flow (through `flow_windows::mcp::listen`, as Flow does), and
//! says who the system says called. The same pipe served by anything else
//! is never answered, and the model hears why.
//! Flow's own client starts `flow-mcp` from a folder whose name has spaces,
//! as it starts any server that's a program.
#![cfg(windows)]

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use flow_core::mcp::bridge::{Event, Handler, NotUsersFlow};
use serde_json::{json, Value};

/// Set for the copy that's to serve as the stand-in Flow.
const STAND_IN: &str = "FLOW_MCP_STAND_IN";
/// What the stand-in prints once it listens.
const LISTENING: &str = "stand-in listening";

/// The stand-in Flow: serves `FLOW_MCP_SOCKET` until it's ended, answering
/// `list_tabs` with who called and refusing the rest. Run only by the tests
/// below, as `flow.exe`.
#[test]
#[ignore = "the stand-in Flow, which the tests below run as flow.exe"]
fn stand_in_flow() {
    if std::env::var_os(STAND_IN).is_none() {
        return;
    }
    let handler: Handler = Arc::new(|event| {
        if let Event::Call(request) = event {
            let host = request.peer.host.as_ref().map(|h| h.path.display().to_string()).unwrap_or_default();
            let said = match request.tool.as_str() {
                "list_tabs" => Ok(format!("tabs for {}|{}|{host}", request.client.name, request.peer.pid.unwrap_or(0))),
                _ => Err(format!("{} isn't allowed", request.tool)),
            };
            let _ = request.reply.send(said);
        }
    });
    let _listening = flow_windows::mcp::listen(handler).unwrap();
    println!("{LISTENING}");
    let _ = std::io::stdout().flush();
    std::thread::sleep(Duration::from_secs(120));
}

/// The stand-in Flow, ended when dropped.
struct StandIn(Child);

impl Drop for StandIn {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// A copy of this program named `flow.exe` in `dir`, serving `pipe` once
/// it says so.
fn stand_in(dir: &Path, pipe: &str) -> StandIn {
    let flow = dir.join("flow.exe");
    std::fs::copy(std::env::current_exe().unwrap(), &flow).unwrap();
    let mut child = Command::new(&flow)
        .args(["stand_in_flow", "--exact", "--ignored", "--nocapture", "--test-threads=1"])
        .env(STAND_IN, "1")
        .env("FLOW_MCP_SOCKET", pipe)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let out = BufReader::new(child.stdout.take().unwrap());
    let child = StandIn(child);
    for line in out.lines() {
        if line.unwrap().contains(LISTENING) {
            return child;
        }
    }
    panic!("the stand-in Flow ended without listening");
}

/// A scratch folder whose name has spaces, as `C:\Program Files` has.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("flow mcp {name} {}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A pipe's name of this test's own.
fn pipe(name: &str) -> String {
    format!(r"\\.\pipe\flow-mcp-test-{name}-{}", std::process::id())
}

/// A `flow-mcp` talking to whatever answers at `pipe`.
struct Mcp {
    child: Child,
    to: std::process::ChildStdin,
    from: BufReader<std::process::ChildStdout>,
}

impl Mcp {
    fn start(pipe: &str) -> Mcp {
        let mut child = Command::new(env!("CARGO_BIN_EXE_flow-mcp"))
            .env("FLOW_MCP_SOCKET", pipe)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let (to, from) = (child.stdin.take().unwrap(), BufReader::new(child.stdout.take().unwrap()));
        let mut mcp = Mcp { child, to, from };
        let init = mcp.ask(json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "claude-code", "version": "2.1.0" } } }));
        assert_eq!(init["result"]["serverInfo"]["name"], "flow", "{init}");
        writeln!(mcp.to, "{}", json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).unwrap();
        mcp
    }

    fn ask(&mut self, message: Value) -> Value {
        writeln!(self.to, "{message}").unwrap();
        let mut line = String::new();
        self.from.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap_or_else(|e| panic!("{e}: {line:?}"))
    }

    fn call(&mut self, id: u64, tool: &str) -> Value {
        self.ask(json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": tool, "arguments": {} } }))
    }

    fn end(mut self) {
        drop(self.to);
        assert!(self.child.wait().unwrap().success());
    }
}

/// The same file, however each path names it.
fn same_file(a: &Path, b: &Path) -> bool {
    a.canonicalize().ok().zip(b.canonicalize().ok()).is_some_and(|(a, b)| a == b)
}

#[test]
fn flow_mcp_reaches_flow_through_its_pipe_and_flow_knows_who_called() {
    let dir = scratch("e2e");
    let pipe = pipe("e2e");
    let _flow = stand_in(&dir, &pipe);
    let mut mcp = Mcp::start(&pipe);
    let tabs = mcp.call(2, "list_tabs");
    assert_eq!(tabs["result"]["isError"], false, "{tabs}");
    let text = tabs["result"]["content"][0]["text"].as_str().unwrap().to_string();
    let said: Vec<&str> = text.split('|').collect();
    // The app by what it said it is; flow-mcp's process and the program that
    // started it (this test) by what the pipe and the system say.
    assert_eq!(said[0], "tabs for claude-code", "{text}");
    assert_eq!(said[1], mcp.child.id().to_string(), "{text}");
    assert!(same_file(Path::new(said[2]), &std::env::current_exe().unwrap()), "{text}");
    let refused = mcp.call(3, "run_command");
    assert_eq!((refused["result"]["isError"].clone(), refused["result"]["content"][0]["text"].clone()), (json!(true), json!("run_command isn't allowed")));
    mcp.end();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_pipe_that_isnt_flows_is_never_answered_and_a_second_flow_gets_none() {
    let pipe = pipe("squat");
    let (seen, events) = crossbeam_channel::unbounded();
    let handler: Handler = Arc::new(move |event| {
        if let Event::Call(request) = event {
            let _ = request.reply.send(Ok("answered".into()));
            let _ = seen.send(request.tool);
        }
    });
    // This test program serves Flow's pipe, as anything that took its name
    // first would; only this test reads the variable in this process.
    std::env::set_var("FLOW_MCP_SOCKET", &pipe);
    let listening = flow_windows::mcp::listen(handler.clone()).unwrap();
    assert!(flow_windows::mcp::listen(handler).is_err(), "the first instance has the name");
    // Opened as flow-mcp opens it, and turned away.
    let refused = flow_windows::mcp::open(Path::new(&pipe)).err();
    assert!(refused.as_ref().is_some_and(NotUsersFlow::is), "served by a test, not flow.exe: {refused:?}");
    let mut mcp = Mcp::start(&pipe);
    let call = mcp.call(2, "list_tabs");
    assert_eq!(call["result"]["isError"], true, "{call}");
    assert!(call["result"]["content"][0]["text"].as_str().unwrap().contains("isn't the user's own Flow"), "{call}");
    mcp.end();
    assert!(events.try_recv().is_err(), "no call reached it");
    drop(listening);
}

/// Flow's own client, starting the built `flow-mcp` from a folder whose
/// name has spaces, as it starts any server that's a program.
#[test]
fn flows_client_starts_a_server_whose_path_has_spaces() {
    use flow_core::mcp::client::servers::{How, Server};
    use flow_core::mcp::client::{Link, ServerEvent};
    let dir = scratch("client");
    let pipe = pipe("client");
    let _flow = stand_in(&dir, &pipe);
    let program = dir.join("flow-mcp.exe");
    std::fs::copy(env!("CARGO_BIN_EXE_flow-mcp"), &program).unwrap();
    let (told, events) = crossbeam_channel::unbounded();
    let tell: Arc<dyn Fn(ServerEvent) + Send + Sync> = Arc::new(move |event| {
        let _ = told.send(event);
    });
    let env = BTreeMap::from([("FLOW_MCP_SOCKET".to_string(), pipe.clone())]);
    let command = program.display().to_string();
    // Written with a variable, as `%USERPROFILE%\…` is, which Flow expands
    // as cmd did; only this test reads it in this process.
    std::env::set_var("FLOW_MCP_TEST_FOLDER", &dir);
    let server = Server { name: "flow".into(), how: How::Stdio { command: r"%FLOW_MCP_TEST_FOLDER%\flow-mcp.exe".into(), args: vec![], env: env.clone() }, oauth: Default::default() };
    let link = Link::start(server, Box::new(|| flow_core::mcp::client::oauth::Auth::None), Vec::new(), tell.clone());
    let wait = Duration::from_secs(20);
    let ServerEvent::Connected { server, listing, .. } = events.recv_timeout(wait).unwrap() else { panic!("not connected") };
    let tools = listing.tools;
    assert_eq!(server, "flow");
    assert!(tools.iter().any(|t| t.name == "list_tabs"), "{tools:?}");
    link.call("job-1", "list_tabs", json!({}), wait);
    let ServerEvent::Answer { job, said: Ok(text), .. } = events.recv_timeout(wait).unwrap() else { panic!("no answer") };
    assert!(job == "job-1" && text.starts_with("tabs for flow|"), "{text}");
    drop(link);
    // A bare name found on the path as a batch file, as npx is npx.cmd.
    std::fs::write(dir.join("flow shim.cmd"), "@\"%~dp0flow-mcp.exe\" %*\r\n").unwrap();
    let shim = Server { name: "shim".into(), how: How::Stdio { command: "flow shim".into(), args: vec![], env: env.clone() }, oauth: Default::default() };
    let link = Link::start(shim, Box::new(|| flow_core::mcp::client::oauth::Auth::None), vec![dir.clone()], tell.clone());
    let ServerEvent::Connected { server, .. } = events.recv_timeout(wait).unwrap() else { panic!("the batch file's server didn't connect") };
    assert_eq!(server, "shim");
    link.call("job-2", "list_tabs", json!({}), wait);
    let ServerEvent::Answer { said: Ok(text), .. } = events.recv_timeout(wait).unwrap() else { panic!("no answer through the batch file") };
    assert!(text.starts_with("tabs for flow|"), "{text}");
    drop(link);
    // Its arguments, spaces and all, reach it as they were given: flow-mcp
    // turns away one it doesn't know, saying it on its standard error.
    let asked = Server { name: "asked".into(), how: How::Stdio { command, args: vec!["no such thing".into()], env }, oauth: Default::default() };
    let _asked = Link::start(asked, Box::new(|| flow_core::mcp::client::oauth::Auth::None), Vec::new(), tell);
    let ServerEvent::Failed { why, .. } = events.recv_timeout(wait).unwrap() else { panic!("not failed") };
    assert!(why.contains("flow-mcp: no such thing?"), "{why}");
    let _ = std::fs::remove_dir_all(&dir);
}
