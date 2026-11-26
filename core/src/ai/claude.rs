//! Minimal Claude Messages API client over raw HTTP with SSE streaming.
//! (Rust has no official Anthropic SDK, so this speaks the wire format.)

use std::io::BufReader;
use std::sync::atomic::AtomicBool;

use serde_json::{json, Value};

use super::provider::{self, ApiError, Reply, Request, Stop, StreamDelta};
use super::usage::Usage;
use super::PendingTool;

pub(crate) const API_VERSION: &str = "2023-06-01";

#[derive(Debug, Default)]
struct StreamedMessage {
    content: Vec<Value>,
    stop_reason: Option<String>,
    stop_details: Option<Value>,
    model: Option<String>,
}

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
            .get(format!("{}/v1/models", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", API_VERSION)
            .call()
            .map_err(ApiError::connection)?;
        if resp.status().as_u16() == 200 {
            return Ok(());
        }
        Err(provider::error_response(resp, parse_error))
    }

    /// Streams one Messages API call (retrying transient failures that come
    /// before any content), adding the tokens it used to `spent`.
    pub fn respond(
        &self,
        req: &Request,
        cancel: &AtomicBool,
        on_delta: &mut dyn FnMut(StreamDelta<'_>),
        spent: &mut Usage,
    ) -> Result<Reply, ApiError> {
        let (body, betas) = body(req);
        let payload = body.to_string();
        provider::with_retries("Claude", cancel, spent, |received_any, used| {
            self.stream_once(&payload, &betas, cancel, on_delta, received_any, used).map(to_reply)
        })
    }

    fn stream_once(
        &self,
        payload: &str,
        betas: &[&str],
        cancel: &AtomicBool,
        on_delta: &mut dyn FnMut(StreamDelta<'_>),
        received_any: &mut bool,
        used: &mut Usage,
    ) -> Result<StreamedMessage, ApiError> {
        let mut req = self
            .agent
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", API_VERSION)
            .header("content-type", "application/json")
            .header("accept", "text/event-stream");
        if !betas.is_empty() {
            req = req.header("anthropic-beta", betas.join(","));
        }
        let resp = req.send(payload).map_err(ApiError::connection)?;
        if resp.status().as_u16() != 200 {
            return Err(provider::error_response(resp, parse_error));
        }

        let reader = BufReader::new(resp.into_body().into_reader());
        let mut acc = Accumulator::default();
        let done = provider::read_sse(reader, cancel, |kind, value| {
            if kind == "error" {
                let err = &value["error"];
                return Err(ApiError::new(
                    None,
                    err["type"].as_str().unwrap_or("api_error"),
                    err["message"].as_str().unwrap_or("stream error"),
                ));
            }
            *received_any |= kind.starts_with("content_block");
            let done = acc.apply(kind, value, on_delta);
            *used = acc.usage;
            Ok(done)
        })?;
        if done || acc.stop_reason.is_some() {
            Ok(acc.finish())
        } else {
            Err(ApiError::new(None, "connection_error", "stream ended early"))
        }
    }
}

/// The Messages API request, and the beta headers it needs.
fn body(req: &Request) -> (Value, Vec<&'static str>) {
    let s = req.settings;
    let model = s.model.as_str();
    let mut tools: Vec<Value> = s.offered()
        .into_iter()
        .map(|mut t| {
            t["eager_input_streaming"] = json!(true);
            t
        })
        .collect();
    if s.web_search {
        tools.push(json!({"type": web_search_type(model), "name": "web_search", "max_uses": 5}));
    }
    let mut body = json!({
        "model": model,
        "max_tokens": 64000,
        "stream": true,
        "system": [{"type": "text", "text": req.system}],
        "tools": tools,
        "messages": req.messages,
        "cache_control": {"type": "ephemeral"},
    });
    let legacy = model.starts_with("claude-3") || model.contains("-4-5") || model.contains("-4-1") || model.contains("-4-0");
    if !legacy {
        body["thinking"] = json!({"type": "adaptive", "display": "summarized"});
        body["output_config"] = json!({"effort": s.effort});
    }
    let mut betas = Vec::new();
    let fallback_models = ["claude-fable-5-1", "claude-opus-5-5", "claude-opus-5", "claude-sonnet-5-5"];
    if s.fallbacks && fallback_models.contains(&model) {
        body["fallbacks"] = json!("default");
        betas.push("server-side-fallback-2026-07-01");
    }
    (body, betas)
}

/// The dynamic-filtering web search needs a recent Opus or Sonnet; other
/// models get the basic version.
fn web_search_type(model: &str) -> &'static str {
    const DYNAMIC: &[&str] = &[
        "claude-opus-5-5",
        "claude-opus-5",
        "claude-opus-4-8",
        "claude-opus-4-7",
        "claude-opus-4-6",
        "claude-sonnet-5-5",
        "claude-sonnet-5",
        "claude-sonnet-4-6",
    ];
    if DYNAMIC.contains(&model) {
        "web_search_20260209"
    } else {
        "web_search_20250305"
    }
}

/// The streamed message as the tool loop wants it: its blocks ready to send
/// back, its tool calls, and why it stopped.
fn to_reply(msg: StreamedMessage) -> Reply {
    let mut content = echo_content(msg.content);
    let calls = content
        .iter()
        .filter(|b| b["type"] == "tool_use")
        .map(|b| PendingTool {
            id: b["id"].as_str().unwrap_or("").to_string(),
            name: b["name"].as_str().unwrap_or("").to_string(),
            input: b["input"].clone(),
            invalid_input: b["__invalid_input"].as_str().map(str::to_string),
        })
        .collect();
    for block in content.iter_mut() {
        if let Some(obj) = block.as_object_mut() {
            obj.remove("__invalid_input");
        }
    }
    let stop = match msg.stop_reason.as_deref() {
        Some("tool_use") => Stop::ToolUse,
        Some("max_tokens") => Stop::MaxTokens,
        Some("pause_turn") => Stop::Pause,
        Some("refusal") => {
            Stop::Refusal(msg.stop_details.as_ref().and_then(|d| d["category"].as_str()).map(str::to_string))
        }
        _ => Stop::Done,
    };
    Reply { content, calls, stop, model: msg.model }
}

/// Prepares streamed content for the history. After a mid-stream fallback,
/// blocks before the boundary other than text must not be echoed.
fn echo_content(content: Vec<Value>) -> Vec<Value> {
    let last_fallback = content.iter().rposition(|b| b["type"] == "fallback");
    content
        .into_iter()
        .enumerate()
        .filter(|(i, b)| match last_fallback {
            Some(f) if *i < f => b["type"] == "text",
            Some(f) if *i == f => false,
            _ => true,
        })
        .map(|(_, mut b)| {
            if let Some(obj) = b.as_object_mut() {
                if obj.get("citations").is_some_and(Value::is_null) {
                    obj.remove("citations");
                }
            }
            b
        })
        // Empty text blocks are rejected when sent back.
        .filter(|b| !(b["type"] == "text" && b["text"].as_str().is_none_or(str::is_empty)))
        .collect()
}

fn parse_error(status: u16, body: &str) -> ApiError {
    let parsed: Option<Value> = serde_json::from_str(body).ok();
    let err = parsed.as_ref().map(|v| &v["error"]);
    let kind = err.and_then(|e| e["type"].as_str()).unwrap_or("api_error");
    let message = err
        .and_then(|e| e["message"].as_str())
        .map(str::to_string)
        .unwrap_or_else(|| body.chars().take(300).collect());
    ApiError::new(Some(status), kind, message)
}

/// Rebuilds content blocks from stream events.
#[derive(Default)]
struct Accumulator {
    blocks: Vec<Value>,
    json_bufs: Vec<Option<String>>,
    stop_reason: Option<String>,
    stop_details: Option<Value>,
    model: Option<String>,
    usage: Usage,
}

impl Accumulator {
    /// Returns true when the message is complete.
    fn apply(&mut self, kind: &str, v: Value, on_delta: &mut dyn FnMut(StreamDelta<'_>)) -> bool {
        match kind {
            "message_start" => {
                self.model = v["message"]["model"].as_str().map(str::to_string);
                self.usage.update_from_api(&v["message"]["usage"]);
            }
            "content_block_start" => {
                let index = v["index"].as_u64().unwrap_or(0) as usize;
                let block = v["content_block"].clone();
                let btype = block["type"].as_str().unwrap_or("").to_string();
                while self.blocks.len() <= index {
                    self.blocks.push(Value::Null);
                    self.json_bufs.push(None);
                }
                match btype.as_str() {
                    "tool_use" | "server_tool_use" | "mcp_tool_use" => {
                        self.json_bufs[index] = Some(String::new());
                        on_delta(StreamDelta::ToolStart {
                            name: block["name"].as_str().unwrap_or("tool"),
                            server: btype != "tool_use",
                        });
                    }
                    "fallback" => on_delta(StreamDelta::Fallback {
                        to: block["to"]["model"].as_str().unwrap_or("another model"),
                    }),
                    _ => {}
                }
                self.blocks[index] = block;
            }
            "content_block_delta" => {
                let index = v["index"].as_u64().unwrap_or(0) as usize;
                let Some(block) = self.blocks.get_mut(index) else { return false };
                let delta = &v["delta"];
                match delta["type"].as_str().unwrap_or("") {
                    "text_delta" => {
                        let t = delta["text"].as_str().unwrap_or("");
                        append_str(block, "text", t);
                        on_delta(StreamDelta::Text(t));
                    }
                    "thinking_delta" => {
                        let t = delta["thinking"].as_str().unwrap_or("");
                        append_str(block, "thinking", t);
                        on_delta(StreamDelta::Thinking(t));
                    }
                    "signature_delta" => {
                        block["signature"] = delta["signature"].clone();
                    }
                    "input_json_delta" => {
                        if let Some(Some(buf)) = self.json_bufs.get_mut(index) {
                            buf.push_str(delta["partial_json"].as_str().unwrap_or(""));
                        }
                    }
                    "citations_delta" => {
                        if !block["citations"].is_array() {
                            block["citations"] = json!([]);
                        }
                        if let Some(arr) = block["citations"].as_array_mut() {
                            arr.push(delta["citation"].clone());
                        }
                    }
                    _ => {}
                }
            }
            "content_block_stop" => {
                let index = v["index"].as_u64().unwrap_or(0) as usize;
                if let (Some(block), Some(slot)) = (self.blocks.get_mut(index), self.json_bufs.get_mut(index)) {
                    if let Some(buf) = slot.take() {
                        if !buf.trim().is_empty() {
                            match serde_json::from_str::<Value>(&buf) {
                                Ok(input) => block["input"] = input,
                                // Eager input streaming hands us raw JSON; keep it so the
                                // caller can report it back as an error result.
                                Err(_) => block["__invalid_input"] = Value::String(buf),
                            }
                        } else if block.get("input").is_none() {
                            block["input"] = json!({});
                        }
                    }
                }
            }
            "message_delta" => {
                self.usage.update_from_api(&v["usage"]);
                if let Some(reason) = v["delta"]["stop_reason"].as_str() {
                    self.stop_reason = Some(reason.to_string());
                }
                if !v["delta"]["stop_details"].is_null() {
                    self.stop_details = Some(v["delta"]["stop_details"].clone());
                }
            }
            "message_stop" => return true,
            _ => {}
        }
        false
    }

    fn finish(self) -> StreamedMessage {
        StreamedMessage {
            content: self.blocks.into_iter().filter(|b| !b.is_null()).collect(),
            stop_reason: self.stop_reason,
            stop_details: self.stop_details,
            model: self.model,
        }
    }
}

fn append_str(block: &mut Value, key: &str, text: &str) {
    let mut s = block[key].as_str().unwrap_or("").to_string();
    s.push_str(text);
    block[key] = Value::String(s);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::provider::Provider;
    use crate::ai::AiSettings;

    #[test]
    fn accumulates_text_tool_and_thinking() {
        let mut acc = Accumulator::default();
        let mut sink = |_d: StreamDelta<'_>| {};
        let events = [
            ("message_start", json!({"message": {"model": "claude-opus-5-5"}})),
            ("content_block_start", json!({"index": 0, "content_block": {"type": "thinking", "thinking": ""}})),
            ("content_block_delta", json!({"index": 0, "delta": {"type": "thinking_delta", "thinking": "hmm"}})),
            ("content_block_delta", json!({"index": 0, "delta": {"type": "signature_delta", "signature": "sig"}})),
            ("content_block_stop", json!({"index": 0})),
            ("content_block_start", json!({"index": 1, "content_block": {"type": "text", "text": ""}})),
            ("content_block_delta", json!({"index": 1, "delta": {"type": "text_delta", "text": "Hi"}})),
            ("content_block_stop", json!({"index": 1})),
            ("content_block_start", json!({"index": 2, "content_block": {"type": "tool_use", "id": "t1", "name": "run_command", "input": {}}})),
            ("content_block_delta", json!({"index": 2, "delta": {"type": "input_json_delta", "partial_json": "{\"command\": \"di"}})),
            ("content_block_delta", json!({"index": 2, "delta": {"type": "input_json_delta", "partial_json": "r\"}"}})),
            ("content_block_stop", json!({"index": 2})),
            ("message_delta", json!({"delta": {"stop_reason": "tool_use"}})),
        ];
        for (k, v) in events {
            assert!(!acc.apply(k, v, &mut sink));
        }
        assert!(acc.apply("message_stop", json!({}), &mut sink));
        let msg = acc.finish();
        assert_eq!(msg.stop_reason.as_deref(), Some("tool_use"));
        assert_eq!(msg.content[0]["thinking"], "hmm");
        assert_eq!(msg.content[0]["signature"], "sig");
        assert_eq!(msg.content[1]["text"], "Hi");
        assert_eq!(msg.content[2]["input"]["command"], "dir");
        let reply = to_reply(msg);
        assert_eq!(reply.stop, Stop::ToolUse);
        assert_eq!(reply.calls[0].input["command"], "dir");
    }

    /// Hits the real API with a bogus key: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn bogus_key_is_rejected_as_auth_error() {
        let client = Client::new("sk-ant-api03-not-a-real-key-000000000000".into(), Provider::Anthropic.default_base_url());
        let err = client.validate_key().expect_err("bogus key must fail");
        assert!(err.is_auth(), "{err}");
        let settings = AiSettings::for_tests(Provider::Anthropic, "claude-opus-5-5", "");
        let messages = [json!({"role": "user", "content": "hi"})];
        let req = Request { settings: &settings, system: "test", messages: &messages };
        let cancel = AtomicBool::new(false);
        let mut spent = Usage::default();
        let err = client.respond(&req, &cancel, &mut |_| {}, &mut spent).expect_err("bogus key must fail");
        assert_eq!(spent.total(), 0);
        assert!(err.is_auth() && !err.is_retryable(), "{err}");
    }

    #[test]
    fn invalid_tool_json_is_kept_raw() {
        let mut acc = Accumulator::default();
        let mut sink = |_d: StreamDelta<'_>| {};
        acc.apply("content_block_start", json!({"index": 0, "content_block": {"type": "tool_use", "id": "t", "name": "x", "input": {}}}), &mut sink);
        acc.apply("content_block_delta", json!({"index": 0, "delta": {"type": "input_json_delta", "partial_json": "{\"a\": "}}), &mut sink);
        acc.apply("content_block_stop", json!({"index": 0}), &mut sink);
        let msg = acc.finish();
        assert_eq!(msg.content[0]["__invalid_input"], "{\"a\": ");
        let reply = to_reply(msg);
        assert_eq!(reply.calls[0].invalid_input.as_deref(), Some("{\"a\": "));
        assert!(reply.content[0].get("__invalid_input").is_none());
    }

    #[test]
    fn fallback_boundary_drops_pre_boundary_internals() {
        let content = vec![
            json!({"type": "thinking", "thinking": "a", "signature": "s"}),
            json!({"type": "text", "text": "partial"}),
            json!({"type": "tool_use", "id": "x", "name": "run_command", "input": {}}),
            json!({"type": "fallback", "from": {"model": "a"}, "to": {"model": "b"}}),
            json!({"type": "thinking", "thinking": "b", "signature": "t"}),
            json!({"type": "text", "text": "rest"}),
        ];
        let out = echo_content(content);
        let types: Vec<&str> = out.iter().map(|b| b["type"].as_str().unwrap()).collect();
        assert_eq!(types, vec!["text", "thinking", "text"]);
    }

    #[test]
    fn drops_empty_text_and_null_citations() {
        let out = echo_content(vec![
            json!({"type": "text", "text": ""}),
            json!({"type": "text", "text": "hi", "citations": null}),
        ]);
        assert_eq!(out, vec![json!({"type": "text", "text": "hi"})]);
    }
}
