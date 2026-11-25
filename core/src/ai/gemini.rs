//! Google's Gemini API (generateContent, streamed over SSE) for Gemini
//! models. The conversation stays on this side: every request carries all of
//! it, with the thought signatures Gemini needs to go on after a tool call,
//! and asks Google not to log it (`store: false`).

use std::io::BufReader;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use serde_json::{json, Value};

use super::provider::{self, ApiError, Reply, Request, Stop, StreamDelta};
use super::usage::Usage;
use super::PendingTool;

pub struct Client {
    agent: ureq::Agent,
    api_key: String,
    base_url: String,
}

impl Client {
    pub fn new(api_key: String, base_url: &str) -> Self {
        Self { agent: provider::http_agent(), api_key, base_url: base_url.trim_end_matches('/').to_string() }
    }

    /// Cheap credential check that spends no tokens.
    pub fn validate_key(&self) -> Result<(), ApiError> {
        let resp = self
            .agent
            .get(format!("{}/v1beta/models?pageSize=1", self.base_url))
            .header("x-goog-api-key", &self.api_key)
            .call()
            .map_err(ApiError::connection)?;
        if resp.status().as_u16() == 200 {
            return Ok(());
        }
        Err(provider::error_response(resp, parse_error))
    }

    /// Streams one generateContent call (retrying transient failures that
    /// come before any of the reply), adding the tokens it used to `spent`.
    pub fn respond(
        &self,
        req: &Request,
        cancel: &AtomicBool,
        on_delta: &mut dyn FnMut(StreamDelta<'_>),
        spent: &mut Usage,
    ) -> Result<Reply, ApiError> {
        let payload = body(req).to_string();
        let url = format!("{}/v1beta/models/{}:streamGenerateContent?alt=sse", self.base_url, req.settings.model);
        provider::with_retries("Gemini", cancel, spent, |received_any, used| {
            self.stream_once(&url, &payload, cancel, on_delta, received_any, used)
        })
    }

    fn stream_once(
        &self,
        url: &str,
        payload: &str,
        cancel: &AtomicBool,
        on_delta: &mut dyn FnMut(StreamDelta<'_>),
        received_any: &mut bool,
        used: &mut Usage,
    ) -> Result<Reply, ApiError> {
        let resp = self
            .agent
            .post(url)
            .header("x-goog-api-key", &self.api_key)
            .header("content-type", "application/json")
            .header("accept", "text/event-stream")
            .send(payload)
            .map_err(ApiError::connection)?;
        if resp.status().as_u16() != 200 {
            return Err(provider::error_response(resp, parse_error));
        }
        let reader = BufReader::new(resp.into_body().into_reader());
        let mut acc = Accumulator::default();
        // Each event is a slice of the response; the stream simply ends after
        // the one with the finish reason.
        provider::read_sse(reader, cancel, |_, chunk| {
            if chunk["error"].is_object() {
                return Err(parse_error(chunk["error"]["code"].as_u64().unwrap_or(0) as u16, &chunk.to_string()));
            }
            acc.apply(&chunk, on_delta);
            *received_any |= acc.received;
            *used = acc.usage;
            Ok(false)
        })?;
        if acc.finish.is_some() || acc.blocked.is_some() {
            Ok(acc.finish())
        } else {
            Err(ApiError::new(None, "connection_error", "stream ended early"))
        }
    }
}

/// The generateContent request.
fn body(req: &Request) -> Value {
    // Gemini reads only part of JSON Schema, which an MCP server's tools may
    // pass: each schema is cleaned for it.
    let declarations: Vec<Value> = req
        .settings
        .offered()
        .into_iter()
        .map(|t| json!({"name": t["name"], "description": t["description"], "parameters": crate::mcp::client::names::for_gemini(&t["input_schema"])}))
        .collect();
    json!({
        "systemInstruction": {"parts": [{"text": req.system}]},
        "contents": contents(req.messages),
        "tools": [{"functionDeclarations": declarations}],
        "generationConfig": {
            "thinkingConfig": {"thinkingLevel": thinking_level(&req.settings.effort), "includeThoughts": true},
        },
        "store": false,
    })
}

/// Gemini thinks at three levels; Flow's two above high think like high.
fn thinking_level(effort: &str) -> &'static str {
    match effort {
        "low" => "low",
        "medium" => "medium",
        _ => "high",
    }
}

/// The conversation as Gemini contents: the user's words and tool results
/// (as `functionResponse` parts) from the user, the model's parts as they
/// came, signatures and all.
fn contents(messages: &[Value]) -> Vec<Value> {
    let mut out = Vec::new();
    for (i, m) in messages.iter().enumerate() {
        let blocks = m["content"].as_array().map(Vec::as_slice).unwrap_or_default();
        let mut parts = Vec::new();
        let role = if m["role"] == "assistant" { "model" } else { "user" };
        for b in blocks {
            match (role, b["type"].as_str()) {
                ("model", Some("gemini_part")) => parts.push(b["part"].clone()),
                (_, Some("text")) => parts.push(json!({"text": b["text"]})),
                // A file the user gave or read_file read (`files.rs`).
                ("user", Some("image" | "document")) => parts.push(json!({"inlineData": {"mimeType": b["source"]["media_type"], "data": b["source"]["data"]}})),
                ("user", Some("tool_result")) => {
                    let call = answered_call(&messages[..i], b["tool_use_id"].as_str().unwrap_or(""));
                    let content = &b["content"];
                    let response = if b["is_error"] == true { json!({"error": content}) } else { json!({"output": content}) };
                    let mut answer = json!({"name": call.and_then(|c| c["name"].as_str()).unwrap_or("tool"), "response": response});
                    if let Some(id) = call.and_then(|c| c["id"].as_str()) {
                        answer["id"] = json!(id);
                    }
                    parts.push(json!({"functionResponse": answer}));
                }
                _ => {}
            }
        }
        if !parts.is_empty() {
            out.push(json!({"role": role, "parts": parts}));
        }
    }
    out
}

/// The `functionCall` that a tool result answers, found by the id Flow gave
/// it (Gemini's own, when it sent one).
fn answered_call<'a>(earlier: &'a [Value], id: &str) -> Option<&'a Value> {
    earlier
        .iter()
        .rev()
        .filter(|m| m["role"] == "assistant")
        .flat_map(|m| m["content"].as_array().map(Vec::as_slice).unwrap_or_default())
        .find(|b| b["type"] == "gemini_part" && b["call_id"] == id)
        .map(|b| &b["part"]["functionCall"])
}

fn parse_error(status: u16, body: &str) -> ApiError {
    let parsed: Option<Value> = serde_json::from_str(body).ok();
    // Streaming calls can wrap the error in an array.
    let root = parsed.as_ref().map(|v| if v.is_array() { &v[0] } else { v });
    let err = root.map(|v| if v["error"].is_object() { &v["error"] } else { v });
    let bad_key = err
        .and_then(|e| e["details"].as_array())
        .is_some_and(|d| d.iter().any(|x| x["reason"] == "API_KEY_INVALID"));
    let kind = if bad_key { "authentication_error" } else { err.and_then(|e| e["status"].as_str()).unwrap_or("api_error") };
    let message = err
        .and_then(|e| e["message"].as_str())
        .map(str::to_string)
        .unwrap_or_else(|| body.chars().take(300).collect());
    ApiError::new(Some(status), kind, message)
}

fn usage(u: &Value) -> Usage {
    let n = |key: &str| u[key].as_u64().unwrap_or(0);
    let cached = n("cachedContentTokenCount");
    Usage {
        input: (n("promptTokenCount") + n("toolUsePromptTokenCount")).saturating_sub(cached),
        output: n("candidatesTokenCount") + n("thoughtsTokenCount"),
        cache_read: cached,
        ..Usage::default()
    }
}

/// Numbers the calls that come without an id of their own.
static NEXT_CALL: AtomicU64 = AtomicU64::new(1);

/// Collects the slices of a streamed response.
#[derive(Default)]
struct Accumulator {
    parts: Vec<Value>,
    finish: Option<String>,
    /// Why the prompt itself was blocked, if it was.
    blocked: Option<String>,
    model: Option<String>,
    usage: Usage,
    /// Some of the reply has arrived (so a failure isn't retried).
    received: bool,
}

impl Accumulator {
    fn apply(&mut self, chunk: &Value, on_delta: &mut dyn FnMut(StreamDelta<'_>)) {
        if let Some(model) = chunk["modelVersion"].as_str() {
            self.model = Some(model.to_string());
        }
        if chunk["usageMetadata"].is_object() {
            self.usage = usage(&chunk["usageMetadata"]);
        }
        if let Some(reason) = chunk["promptFeedback"]["blockReason"].as_str() {
            self.blocked = Some(reason.to_string());
        }
        let candidate = &chunk["candidates"][0];
        for part in candidate["content"]["parts"].as_array().map(Vec::as_slice).unwrap_or_default() {
            self.add(part, on_delta);
        }
        if let Some(reason) = candidate["finishReason"].as_str() {
            self.finish = Some(reason.to_string());
        }
    }

    fn add(&mut self, part: &Value, on_delta: &mut dyn FnMut(StreamDelta<'_>)) {
        if let Some(call) = part.get("functionCall") {
            self.received = true;
            on_delta(StreamDelta::ToolStart { name: call["name"].as_str().unwrap_or("tool"), server: false });
        } else if let Some(text) = part["text"].as_str() {
            let thought = part["thought"] == true;
            if !text.is_empty() {
                self.received = true;
                on_delta(if thought { StreamDelta::Thinking(text) } else { StreamDelta::Text(text) });
            }
            // Text streams in pieces that join into one part. A signed piece
            // (often an empty last one) stays as it came: Google says never
            // to merge a part with a signature into one without.
            let signed = part.get("thoughtSignature").is_some();
            if let Some(last) = self.parts.last_mut() {
                let open = last["text"].is_string() && last.get("thoughtSignature").is_none();
                if !signed && open && (last["thought"] == true) == thought {
                    let joined = format!("{}{text}", last["text"].as_str().unwrap_or(""));
                    last["text"] = json!(joined);
                    return;
                }
            }
        }
        self.parts.push(part.clone());
    }

    fn finish(self) -> Reply {
        let mut calls = Vec::new();
        let mut content = Vec::new();
        for part in self.parts {
            let mut block = json!({"type": "gemini_part", "part": part});
            if let Some(call) = block["part"].get("functionCall") {
                let id = match call["id"].as_str() {
                    Some(id) => id.to_string(),
                    None => format!("gemini-call-{}", NEXT_CALL.fetch_add(1, Ordering::Relaxed)),
                };
                let input = if call["args"].is_object() { call["args"].clone() } else { json!({}) };
                calls.push(PendingTool { id: id.clone(), name: call["name"].as_str().unwrap_or("").to_string(), input, invalid_input: None });
                block["call_id"] = json!(id);
            }
            content.push(block);
        }
        // Only a plain stop is a finished reply; anything else must not run
        // its tool calls.
        let stop = match (self.blocked, self.finish.as_deref()) {
            (Some(reason), _) => Stop::Refusal(Some(reason.to_lowercase())),
            (_, None | Some("STOP")) if calls.is_empty() => Stop::Done,
            (_, None | Some("STOP")) => Stop::ToolUse,
            (_, Some("MAX_TOKENS" | "CONTINUATION")) => Stop::MaxTokens,
            (_, Some(reason @ ("SAFETY" | "RECITATION" | "PROHIBITED_CONTENT" | "BLOCKLIST" | "SPII" | "IMAGE_SAFETY"))) => {
                Stop::Refusal(Some(reason.to_lowercase()))
            }
            (_, Some("MALFORMED_FUNCTION_CALL" | "UNEXPECTED_TOOL_CALL")) => {
                Stop::Failed(crate::i18n::t("error.gemini_tool_call").into())
            }
            (_, Some(reason)) => Stop::Failed(crate::i18n::tf("error.gemini_stopped", &[("reason", &reason.to_lowercase())])),
        };
        Reply { content, calls, stop, model: self.model }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::provider::Provider;
    use crate::ai::AiSettings;

    fn run(chunks: &[Value]) -> (Reply, String, String) {
        let mut acc = Accumulator::default();
        let (mut text, mut thinking) = (String::new(), String::new());
        let mut sink = |d: StreamDelta<'_>| match d {
            StreamDelta::Text(t) => text.push_str(t),
            StreamDelta::Thinking(t) => thinking.push_str(t),
            _ => {}
        };
        for chunk in chunks {
            acc.apply(chunk, &mut sink);
        }
        (acc.finish(), text, thinking)
    }

    fn parts(parts: Value) -> Value {
        json!({"candidates": [{"content": {"role": "model", "parts": parts}}]})
    }

    #[test]
    fn streamed_pieces_join_and_keep_their_signatures_and_calls() {
        let (reply, text, thinking) = run(&[
            parts(json!([{"text": "Look at ", "thought": true}])),
            parts(json!([{"text": "the screen", "thought": true}])),
            parts(json!([{"text": "", "thought": true, "thoughtSignature": "sig-t"}])),
            parts(json!([{"text": "Checking ", "thoughtSignature": "sig-a"}])),
            parts(json!([{"text": "now."}])),
            parts(json!([{"functionCall": {"name": "read_terminal", "args": {"lines": 5}}, "thoughtSignature": "sig-b"}])),
            json!({
                "candidates": [{"content": {"parts": [{"text": "", "thoughtSignature": "sig-c"}]}, "finishReason": "STOP"}],
                "usageMetadata": {"promptTokenCount": 120, "cachedContentTokenCount": 20, "candidatesTokenCount": 10, "thoughtsTokenCount": 5},
                "modelVersion": "gemini-4-argon",
            }),
        ]);
        assert_eq!(thinking, "Look at the screen");
        assert_eq!(text, "Checking now.");
        assert_eq!(reply.stop, Stop::ToolUse);
        assert_eq!(reply.model.as_deref(), Some("gemini-4-argon"));
        let kept: Vec<&Value> = reply.content.iter().map(|b| &b["part"]).collect();
        assert_eq!(kept[0], &json!({"text": "Look at the screen", "thought": true}));
        // Signed pieces stay as they came, and later text starts anew.
        assert_eq!(kept[1], &json!({"text": "", "thought": true, "thoughtSignature": "sig-t"}));
        assert_eq!(kept[2], &json!({"text": "Checking ", "thoughtSignature": "sig-a"}));
        assert_eq!(kept[3], &json!({"text": "now."}));
        assert_eq!(kept[4]["thoughtSignature"], "sig-b");
        assert_eq!(kept[5], &json!({"text": "", "thoughtSignature": "sig-c"}));
        assert_eq!(reply.calls[0].name, "read_terminal");
        assert_eq!(reply.calls[0].input, json!({"lines": 5}));
        assert_eq!(reply.content[4]["call_id"], json!(reply.calls[0].id));
        let used = usage(&json!({"promptTokenCount": 120, "cachedContentTokenCount": 20, "candidatesTokenCount": 10, "thoughtsTokenCount": 5}));
        assert_eq!((used.input, used.cache_read, used.output), (100, 20, 15));
    }

    #[test]
    fn blocked_cut_off_and_broken_replies_say_so() {
        let blocked = json!({"promptFeedback": {"blockReason": "SAFETY"}});
        assert_eq!(run(&[blocked]).0.stop, Stop::Refusal(Some("safety".into())));
        let cut = json!({"candidates": [{"content": {"parts": [{"text": "half"}]}, "finishReason": "MAX_TOKENS"}]});
        assert_eq!(run(&[cut]).0.stop, Stop::MaxTokens);
        let broken = json!({"candidates": [{"finishReason": "MALFORMED_FUNCTION_CALL"}]});
        assert!(matches!(run(&[broken]).0.stop, Stop::Failed(_)));
        // A reason Flow doesn't know is no success, even with a call in it.
        let odd = json!({"candidates": [{"content": {"parts": [{"functionCall": {"name": "read_terminal", "args": {}}}]}, "finishReason": "TOO_MANY_TOOL_CALLS"}]});
        assert!(matches!(run(&[odd]).0.stop, Stop::Failed(ref m) if m.contains("too_many_tool_calls")));
        let bad_key = parse_error(
            400,
            r#"[{"error": {"code": 400, "message": "API key not valid. Please pass a valid API key.", "status": "INVALID_ARGUMENT", "details": [{"reason": "API_KEY_INVALID"}]}}]"#,
        );
        assert!(bad_key.is_auth(), "{bad_key}");
        let busy = parse_error(503, r#"{"error": {"code": 503, "message": "The model is overloaded.", "status": "UNAVAILABLE"}}"#);
        assert!(busy.is_retryable() && !busy.is_auth());
    }

    #[test]
    fn a_picture_and_a_document_become_inline_data() {
        let messages = vec![json!({"role": "user", "content": [
            {"type": "text", "text": "look"},
            {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "AAAA"}},
            {"type": "document", "source": {"type": "base64", "media_type": "application/pdf", "data": "BBBB"}, "title": "r.pdf"},
        ]})];
        assert_eq!(contents(&messages), [json!({"role": "user", "parts": [
            {"text": "look"},
            {"inlineData": {"mimeType": "image/png", "data": "AAAA"}},
            {"inlineData": {"mimeType": "application/pdf", "data": "BBBB"}},
        ]})]);
    }

    #[test]
    fn the_conversation_becomes_contents_with_results_matched_to_calls() {
        let messages = vec![
            json!({"role": "user", "content": [{"type": "text", "text": "<ctx>"}, {"type": "text", "text": "hi"}]}),
            json!({"role": "assistant", "content": [
                {"type": "gemini_part", "part": {"text": "Looking", "thoughtSignature": "sig"}},
                {"type": "gemini_part", "part": {"functionCall": {"name": "read_terminal", "args": {}}}, "call_id": "gemini-call-7"},
                {"type": "gemini_part", "part": {"functionCall": {"id": "abc", "name": "read_file", "args": {"path": "x"}}}, "call_id": "abc"},
            ]}),
            json!({"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "gemini-call-7", "content": "prompt>", "is_error": false},
                {"type": "tool_result", "tool_use_id": "abc", "content": "no such file", "is_error": true},
            ]}),
            json!({"role": "assistant", "content": [{"type": "text", "text": "(stopped by the user)"}]}),
        ];
        let c = contents(&messages);
        assert_eq!(c[0], json!({"role": "user", "parts": [{"text": "<ctx>"}, {"text": "hi"}]}));
        assert_eq!(c[1]["role"], "model");
        assert_eq!(c[1]["parts"][0]["thoughtSignature"], "sig");
        assert!(c[1]["parts"][1].get("call_id").is_none(), "Flow's own ids stay out of the request");
        assert_eq!(c[2]["parts"][0], json!({"functionResponse": {"name": "read_terminal", "response": {"output": "prompt>"}}}));
        assert_eq!(c[2]["parts"][1], json!({"functionResponse": {"id": "abc", "name": "read_file", "response": {"error": "no such file"}}}));
        assert_eq!(c[3], json!({"role": "model", "parts": [{"text": "(stopped by the user)"}]}));

        let mut settings = AiSettings::for_tests(Provider::Gemini, "gemini-4-argon", "");
        settings.effort = "xhigh".into();
        let req = Request { settings: &settings, system: "be brief", messages: &messages };
        let b = body(&req);
        assert_eq!(b["systemInstruction"]["parts"][0]["text"], "be brief");
        assert_eq!(b["generationConfig"]["thinkingConfig"]["thinkingLevel"], "high");
        assert_eq!(b["store"], false);
        assert_eq!(b["tools"][0]["functionDeclarations"][0]["name"], "run_command");

        // An MCP server's tool turned on is declared too, its schema cleaned
        // of what Gemini doesn't read.
        settings.server_tools = vec![crate::ai::ServerTool::for_tests()];
        let b = body(&Request { settings: &settings, system: "be brief", messages: &messages });
        let declared = b["tools"][0]["functionDeclarations"].as_array().unwrap().iter().find(|d| d["name"] == "mcp__notes__find_note").expect("the server's tool");
        assert_eq!(declared["description"], "[notes] Finds a note by its words.");
        assert!(declared["parameters"].get("$schema").is_none() && declared["parameters"].get("additionalProperties").is_none(), "{declared}");
        assert_eq!(declared["parameters"]["required"][0], "words");
    }

    /// Hits the real API with a bogus key: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn bogus_key_is_rejected_as_auth_error() {
        let client = Client::new("AIzaSy-not-a-real-key-000000000000000".into(), Provider::Gemini.default_base_url());
        let err = client.validate_key().expect_err("bogus key must fail");
        assert!(err.is_auth() && !err.is_retryable(), "{err}");
    }
}
