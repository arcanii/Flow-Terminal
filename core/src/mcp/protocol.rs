//! MCP's JSON-RPC 2.0 as `flow-mcp` speaks it over its stdin and stdout, a
//! message a line, written by hand as the AI clients are: `initialize`,
//! `notifications/initialized`, `ping`, `tools/list` and `tools/call`. A
//! tool call isn't answered here but handed on ([`Outcome::Call`]) for
//! Flow to answer ([`answer`]); everything else is answered at once.

use serde_json::{json, Value};

use super::ClientInfo;

/// The protocol versions Flow speaks, newest first: a client asking for
/// one of them gets it, any other the newest.
pub const VERSIONS: [&str; 4] = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

/// JSON-RPC's error codes.
const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;
/// A request before `initialize`.
const NOT_INITIALIZED: i64 = -32002;

/// What came of a line from the client.
#[derive(Debug, PartialEq)]
pub enum Outcome {
    /// A message to write back.
    Reply(Value),
    /// Nothing to write (a notification).
    Nothing,
    /// A tool to call in Flow, its answer to go back by [`answer`].
    Call { id: Value, tool: String, args: Value },
}

/// One client's side of the conversation.
#[derive(Debug, Default)]
pub struct Server {
    /// What the client said it is, once it has.
    pub client: Option<ClientInfo>,
}

impl Server {
    /// What `line`, one message from the client, comes to.
    pub fn handle(&mut self, line: &str) -> Outcome {
        let Ok(message) = serde_json::from_str::<Value>(line) else {
            return Outcome::Reply(error(Value::Null, PARSE_ERROR, "That isn't JSON."));
        };
        let Some(method) = message.get("method").and_then(Value::as_str) else {
            // A client's answer to something Flow never asks, or not a request.
            return match message.get("id") {
                Some(id) if message.get("result").is_none() && message.get("error").is_none() => Outcome::Reply(error(id.clone(), INVALID_REQUEST, "A request needs a method.")),
                _ => Outcome::Nothing,
            };
        };
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        // A notification has no id and gets no answer.
        let Some(id) = message.get("id").cloned() else { return Outcome::Nothing };
        match method {
            "initialize" => {
                let asked = params.get("protocolVersion").and_then(Value::as_str).unwrap_or_default();
                let version = VERSIONS.iter().find(|v| **v == asked).copied().unwrap_or(VERSIONS[0]);
                self.client = Some(params.get("clientInfo").and_then(|c| serde_json::from_value(c.clone()).ok()).unwrap_or_default());
                Outcome::Reply(result(
                    id,
                    json!({
                        "protocolVersion": version,
                        "capabilities": { "tools": { "listChanged": false } },
                        "serverInfo": { "name": "flow", "title": "Flow", "version": crate::version::VERSION, "description": "A terminal with an assistant built in" },
                        "instructions": "Flow is a terminal. These tools read its tabs and panes, run commands in panes of \
your own, open and close tabs, and change Flow's settings, as far as the user allows: the first call asks them in Flow what you \
may do, and a refused call says where they can allow it."
                    }),
                ))
            }
            "ping" => Outcome::Reply(result(id, json!({}))),
            _ if self.client.is_none() => Outcome::Reply(error(id, NOT_INITIALIZED, "Say initialize first.")),
            "tools/list" => Outcome::Reply(result(id, super::tools::list())),
            "tools/call" => {
                let Some(name) = params.get("name").and_then(Value::as_str) else {
                    return Outcome::Reply(error(id, INVALID_PARAMS, "tools/call needs a tool's name."));
                };
                if super::tools::find(name).is_none() {
                    return Outcome::Reply(error(id, INVALID_PARAMS, &format!("Flow has no tool {name}.")));
                }
                let args = match params.get("arguments") {
                    None | Some(Value::Null) => json!({}),
                    Some(args) if args.is_object() => args.clone(),
                    Some(_) => return Outcome::Reply(error(id, INVALID_PARAMS, "A tool's arguments are an object.")),
                };
                Outcome::Call { id, tool: name.to_string(), args }
            }
            other => Outcome::Reply(error(id, METHOD_NOT_FOUND, &format!("Flow doesn't do {other}."))),
        }
    }
}

/// The answer to a tool call `id`: Flow's words, or why it couldn't, as
/// the call's result (a refusal, or arguments Flow can't take, is the
/// tool's error, which the model reads and can correct, as 2025-11-25
/// asks; only an unknown tool or a malformed request is a JSON-RPC one).
pub fn answer(id: Value, said: Result<String, String>) -> Value {
    let (text, is_error) = match said {
        Ok(text) => (text, false),
        Err(text) => (text, true),
    };
    result(id, json!({ "content": [{ "type": "text", "text": text }], "isError": is_error }))
}

fn result(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(server: &mut Server, line: &str) -> Value {
        match server.handle(line) {
            Outcome::Reply(v) => v,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_client_says_initialize_first_and_gets_a_version_flow_speaks() {
        let mut server = Server::default();
        let early = reply(&mut server, r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#);
        assert_eq!(early["error"]["code"], NOT_INITIALIZED);
        // Ping is answered before it too.
        assert_eq!(reply(&mut server, r#"{"jsonrpc":"2.0","id":"p","method":"ping"}"#)["result"], json!({}));
        let init = reply(
            &mut server,
            r#"{"jsonrpc":"2.0","id":2,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"claude-code","version":"2.1.0"}}}"#,
        );
        assert_eq!(init["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(init["result"]["serverInfo"]["name"], "flow");
        assert!(init["result"]["capabilities"]["tools"].is_object());
        assert_eq!(server.client, Some(ClientInfo { name: "claude-code".into(), version: "2.1.0".into() }));
        // One Flow doesn't know is given the newest.
        let mut other = Server::default();
        let newer = reply(&mut other, r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2099-01-01","clientInfo":{"name":"x"}}}"#);
        assert_eq!(newer["result"]["protocolVersion"], VERSIONS[0]);
        // The notification that follows gets no answer.
        assert_eq!(server.handle(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#), Outcome::Nothing);
    }

    #[test]
    fn tools_are_listed_and_a_call_is_handed_on_with_its_arguments() {
        let mut server = Server::default();
        reply(&mut server, r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","clientInfo":{"name":"c"}}}"#);
        let listed = reply(&mut server, r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#);
        let names: Vec<&str> = listed["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"run_command") && names.contains(&"read_pane"), "{names:?}");
        let call = server.handle(r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"read_pane","arguments":{"lines":5}}}"#);
        assert_eq!(call, Outcome::Call { id: json!(3), tool: "read_pane".into(), args: json!({ "lines": 5 }) });
        let none = server.handle(r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"list_tabs"}}"#);
        assert_eq!(none, Outcome::Call { id: json!(4), tool: "list_tabs".into(), args: json!({}) });
        // The answer goes back as the call's result, a refusal as its error.
        let said = answer(json!(3), Err("Not allowed.".into()));
        assert_eq!((said["id"].clone(), said["result"]["isError"].clone(), said["result"]["content"][0]["text"].clone()), (json!(3), json!(true), json!("Not allowed.")));
    }

    #[test]
    fn what_isnt_a_request_flow_knows_gets_json_rpcs_errors() {
        let mut server = Server::default();
        assert_eq!(reply(&mut server, "not json")["error"]["code"], PARSE_ERROR);
        reply(&mut server, r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"clientInfo":{"name":"c"}}}"#);
        assert_eq!(reply(&mut server, r#"{"jsonrpc":"2.0","id":2,"method":"resources/list"}"#)["error"]["code"], METHOD_NOT_FOUND);
        assert_eq!(reply(&mut server, r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"rm"}}"#)["error"]["code"], INVALID_PARAMS);
        assert_eq!(reply(&mut server, r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{}}"#)["error"]["code"], INVALID_PARAMS);
        assert_eq!(reply(&mut server, r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"read_pane","arguments":[1]}}"#)["error"]["code"], INVALID_PARAMS);
        assert_eq!(reply(&mut server, r#"{"jsonrpc":"2.0","id":6}"#)["error"]["code"], INVALID_REQUEST);
        // A notification of anything is passed over.
        assert_eq!(server.handle(r#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":3}}"#), Outcome::Nothing);
    }
}
