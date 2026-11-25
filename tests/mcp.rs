//! `flow-mcp` run for real, as an app's MCP config runs it: MCP over its
//! standard input and output, each tool call handed to a stand-in Flow
//! listening on a scratch socket (`FLOW_MCP_SOCKET`), as Flow's own
//! listener would hand it to Flow's window.
#![cfg(unix)]

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;

use flow_core::mcp::bridge::{self, Event, Handler, Peer};
use serde_json::{json, Value};

/// A `flow-mcp` talking to whatever answers at `socket`.
struct Mcp {
    child: std::process::Child,
    to: std::process::ChildStdin,
    from: BufReader<std::process::ChildStdout>,
}

impl Mcp {
    fn start(socket: &std::path::Path) -> Mcp {
        let mut child = Command::new(env!("CARGO_BIN_EXE_flow-mcp"))
            .env("FLOW_MCP_SOCKET", socket)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let (to, from) = (child.stdin.take().unwrap(), BufReader::new(child.stdout.take().unwrap()));
        Mcp { child, to, from }
    }

    fn send(&mut self, message: Value) {
        writeln!(self.to, "{message}").unwrap();
    }

    fn ask(&mut self, message: Value) -> Value {
        self.send(message);
        let mut line = String::new();
        self.from.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap_or_else(|e| panic!("{e}: {line:?}"))
    }

    fn initialize(&mut self) {
        let init = self.ask(json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "claude-code", "version": "2.1.0" } } }));
        assert_eq!(init["result"]["serverInfo"]["name"], "flow", "{init}");
        self.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));
    }
}

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(format!("/tmp/flow-mcp-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn flow_mcp_speaks_mcp_and_hands_each_call_to_flow() {
    let dir = scratch("e2e");
    let socket = dir.join("flow.sock");
    let handler: Handler = Arc::new(|event| {
        if let Event::Call(request) = event {
            let said = match request.tool.as_str() {
                "list_tabs" => Ok(format!("tabs, for {}", request.client.name)),
                _ => Err(format!("{} isn't allowed", request.tool)),
            };
            let _ = request.reply.send(said);
        }
    });
    let listening = bridge::listen_unix(&socket, |_| Peer::default(), handler).unwrap();
    let mut mcp = Mcp::start(&socket);
    mcp.initialize();
    let listed = mcp.ask(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }));
    let names: Vec<&str> = listed["result"]["tools"].as_array().unwrap().iter().filter_map(|t| t["name"].as_str()).collect();
    assert!(names.contains(&"list_tabs") && names.contains(&"run_command"), "{names:?}");
    // A call goes to Flow, which knows the app by what it said it is.
    let tabs = mcp.ask(json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": "list_tabs", "arguments": {} } }));
    assert_eq!(tabs["result"]["content"][0]["text"], "tabs, for claude-code", "{tabs}");
    assert_eq!(tabs["result"]["isError"], false);
    // A refusal comes back as the tool's error, for the model to read.
    let refused = mcp.ask(json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": { "name": "run_command", "arguments": { "command": "ls" } } }));
    assert_eq!((refused["id"].clone(), refused["result"]["isError"].clone()), (json!(4), json!(true)));
    assert_eq!(refused["result"]["content"][0]["text"], "run_command isn't allowed");
    // Its input closed, it's done.
    drop(mcp.to);
    assert!(mcp.child.wait().unwrap().success());
    drop(listening);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn with_nothing_answering_a_call_says_so() {
    let dir = scratch("none");
    let mut mcp = Mcp::start(&dir.join("flow.sock"));
    mcp.initialize();
    let call = mcp.ask(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": { "name": "list_tabs" } }));
    assert_eq!(call["result"]["isError"], true);
    assert!(call["result"]["content"][0]["text"].as_str().unwrap().starts_with("Nothing answers"), "{call}");
    drop(mcp.to);
    assert!(mcp.child.wait().unwrap().success());
}

/// A socket the system won't let `flow-mcp` into (another user's, say, or
/// one an app's sandbox keeps it from): the model hears so, not that
/// nothing answers nor that it's surely another user's (which
/// `bridge::connect_unix`'s own check says, as core's tests show), and
/// nothing reaches whatever listens.
#[test]
fn flow_mcp_says_so_when_the_system_wont_let_it_reach_flow() {
    use std::os::unix::fs::PermissionsExt;
    let dir = scratch("theirs");
    let socket = dir.join("flow.sock");
    let (seen, heard) = crossbeam_channel::unbounded();
    let handler: Handler = Arc::new(move |event| {
        let _ = seen.send(format!("{event:?}"));
    });
    let listening = bridge::listen_unix(&socket, |_| Peer::default(), handler).unwrap();
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::os::unix::net::UnixStream::connect(&socket).is_ok() {
        // Root is let in whatever the socket says: nothing to show.
        eprintln!("skipped: the socket's permissions don't hold for this user");
        drop(listening);
        let _ = std::fs::remove_dir_all(&dir);
        return;
    }
    let mut mcp = Mcp::start(&socket);
    mcp.initialize();
    let call = mcp.ask(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": { "name": "list_tabs" } }));
    assert_eq!(call["result"]["isError"], true, "{call}");
    assert!(call["result"]["content"][0]["text"].as_str().unwrap().starts_with("The system won't let flow-mcp open"), "{call}");
    drop(mcp.to);
    assert!(mcp.child.wait().unwrap().success());
    assert!(heard.try_recv().is_err(), "nothing reached it");
    drop(listening);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Flow's own client, connecting to the built `flow-mcp` (itself talking to
/// a stand-in Flow) as it would to any server it starts as a program.
#[test]
fn flows_client_lists_a_servers_tools_and_calls_one() {
    use flow_core::mcp::client::servers::{How, Server};
    use flow_core::mcp::client::{Link, ServerEvent};
    use std::collections::BTreeMap;
    let dir = scratch("client");
    let socket = dir.join("flow.sock");
    let handler: Handler = Arc::new(|event| {
        if let Event::Call(request) = event {
            let _ = request.reply.send(Ok(format!("{} asked for {}", request.client.name, request.tool)));
        }
    });
    let listening = bridge::listen_unix(&socket, |_| Peer::default(), handler).unwrap();
    let (told, events) = crossbeam_channel::unbounded();
    let tell: Arc<dyn Fn(ServerEvent) + Send + Sync> = Arc::new(move |event| {
        let _ = told.send(event);
    });
    let env = BTreeMap::from([("FLOW_MCP_SOCKET".to_string(), socket.display().to_string())]);
    let server = Server { name: "flow".into(), how: How::Stdio { command: env!("CARGO_BIN_EXE_flow-mcp").into(), args: vec![], env }, oauth: Default::default() };
    let link = Link::start(server, Box::new(|| flow_core::mcp::client::oauth::Auth::None), Vec::new(), tell.clone());
    let wait = std::time::Duration::from_secs(20);
    let ServerEvent::Connected { server, listing, .. } = events.recv_timeout(wait).unwrap() else { panic!("not connected") };
    let tools = listing.tools;
    assert_eq!(server, "flow");
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    assert!(names.contains(&"list_tabs") && names.contains(&"run_command"), "{names:?}");
    assert!(tools.iter().find(|t| t.name == "read_pane").unwrap().read_only, "its hints read");
    link.call("job-1", "list_tabs", json!({}), wait);
    assert_eq!(events.recv_timeout(wait).unwrap(), ServerEvent::Answer { server: "flow".into(), job: "job-1".into(), said: Ok("flow asked for list_tabs".into()) });
    drop(link);
    // A program that fails as it starts says why, from its standard error.
    let broken = Server { name: "broken".into(), how: How::Stdio { command: "/bin/sh".into(), args: vec!["-c".into(), "echo no config here >&2; exit 3".into()], env: BTreeMap::new() }, oauth: Default::default() };
    let _gone = Link::start(broken, Box::new(|| flow_core::mcp::client::oauth::Auth::None), Vec::new(), tell);
    let ServerEvent::Failed { server, why } = events.recv_timeout(wait).unwrap() else { panic!("not failed") };
    assert_eq!(server, "broken");
    assert!(why.contains("no config here"), "{why}");
    drop(listening);
    let _ = std::fs::remove_dir_all(&dir);
}

