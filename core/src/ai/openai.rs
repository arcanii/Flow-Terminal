//! OpenAI's Responses API over raw HTTP with SSE streaming, for GPT models
//! (and servers that speak the same API). Nothing is kept on OpenAI's side
//! (`store: false`): every request carries the whole conversation, with the
//! model's reasoning sent back as the encrypted items it came in, so it can
//! pick up where it left off after a tool call.

use std::io::BufReader;
use std::sync::atomic::AtomicBool;

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
            .get(format!("{}/v1/models", self.base_url))
            .header("authorization", format!("Bearer {}", self.api_key))
            .call()
            .map_err(ApiError::connection)?;
        if resp.status().as_u16() == 200 {
            return Ok(());
        }
        Err(provider::error_response(resp, parse_error))
    }

    /// Streams one Responses API call (retrying transient failures that come
    /// before any of the reply), adding the tokens it used to `spent`.
    pub fn respond(
        &self,
        req: &Request,
        cancel: &AtomicBool,
        on_delta: &mut dyn FnMut(StreamDelta<'_>),
        spent: &mut Usage,
    ) -> Result<Reply, ApiError> {
        let payload = body(req).to_string();
        provider::with_retries("GPT", cancel, spent, |received_any, used| {
            self.stream_once(&payload, cancel, on_delta, received_any, used)
        })
    }

    fn stream_once(
        &self,
        payload: &str,
        cancel: &AtomicBool,
        on_delta: &mut dyn FnMut(StreamDelta<'_>),
        received_any: &mut bool,
        used: &mut Usage,
    ) -> Result<Reply, ApiError> {
        let resp = self
            .agent
            .post(format!("{}/v1/responses", self.base_url))
            .header("authorization", format!("Bearer {}", self.api_key))
            .header("content-type", "application/json")
            .header("accept", "text/event-stream")
            .send(payload)
            .map_err(ApiError::connection)?;
        if resp.status().as_u16() != 200 {
            return Err(provider::error_response(resp, parse_error));
        }
        let reader = BufReader::new(resp.into_body().into_reader());
        let mut acc = Accumulator::default();
        let done = provider::read_sse(reader, cancel, |kind, value| {
            let done = acc.apply(kind, &value, on_delta)?;
            *received_any |= acc.received;
            *used = acc.usage;
            Ok(done)
        })?;
        if done {
            Ok(acc.finish())
        } else {
            Err(ApiError::new(None, "connection_error", "stream ended early"))
        }
    }
}

/// The Responses API request.
fn body(req: &Request) -> Value {
    let s = req.settings;
    let mut tools: Vec<Value> = s.offered()
        .into_iter()
        .map(|t| {
            json!({
                "type": "function",
                "name": t["name"],
                "description": t["description"],
                "parameters": t["input_schema"],
                "strict": false,
            })
        })
        .collect();
    if s.web_search {
        tools.push(json!({"type": "web_search"}));
    }
    json!({
        "model": s.model,
        "instructions": req.system,
        "input": input(req.messages),
        "tools": tools,
        "reasoning": {"effort": s.effort},
        "include": ["reasoning.encrypted_content"],
        "store": false,
        "stream": true,
    })
}

/// The conversation as Responses API input items: the user's words, tool
/// results as `function_call_output`, and the model's own items as they came.
fn input(messages: &[Value]) -> Vec<Value> {
    let mut items = Vec::new();
    for m in messages {
        let blocks = m["content"].as_array().map(Vec::as_slice).unwrap_or_default();
        if m["role"] == "assistant" {
            for b in blocks {
                match b["type"].as_str() {
                    Some("openai_item") => items.push(b["item"].clone()),
                    Some("text") => items.push(json!({"role": "assistant", "content": b["text"]})),
                    _ => {}
                }
            }
            continue;
        }
        // Results first: they answer the calls just before them.
        let mut texts = Vec::new();
        for b in blocks {
            match b["type"].as_str() {
                Some("tool_result") => items.push(json!({
                    "type": "function_call_output",
                    "call_id": b["tool_use_id"],
                    "output": provider::result_text(b),
                })),
                Some("text") => texts.push(json!({"type": "input_text", "text": b["text"]})),
                // A file the user gave or read_file read (`files.rs`).
                Some("image") => texts.push(json!({
                    "type": "input_image",
                    "image_url": format!("data:{};base64,{}", b["source"]["media_type"].as_str().unwrap_or_default(), b["source"]["data"].as_str().unwrap_or_default()),
                })),
                Some("document") => texts.push(json!({
                    "type": "input_file",
                    "filename": b["title"],
                    "file_data": format!("data:{};base64,{}", b["source"]["media_type"].as_str().unwrap_or_default(), b["source"]["data"].as_str().unwrap_or_default()),
                })),
                _ => {}
            }
        }
        if !texts.is_empty() {
            items.push(json!({"role": "user", "content": texts}));
        }
    }
    items
}

fn parse_error(status: u16, body: &str) -> ApiError {
    let parsed: Option<Value> = serde_json::from_str(body).ok();
    let err = parsed.as_ref().map(|v| &v["error"]);
    let kind = err
        .and_then(|e| e["code"].as_str().filter(|c| *c == "invalid_api_key").or(e["type"].as_str()))
        .unwrap_or("api_error");
    let message = err
        .and_then(|e| e["message"].as_str())
        .map(str::to_string)
        .unwrap_or_else(|| body.chars().take(300).collect());
    ApiError::new(Some(status), kind, message)
}

fn usage(u: &Value) -> Usage {
    let input = u["input_tokens"].as_u64().unwrap_or(0);
    let cached = u["input_tokens_details"]["cached_tokens"].as_u64().unwrap_or(0);
    Usage {
        input: input.saturating_sub(cached),
        output: u["output_tokens"].as_u64().unwrap_or(0),
        cache_read: cached,
        ..Usage::default()
    }
}

/// Collects a streamed response.
#[derive(Default)]
struct Accumulator {
    /// Finished output items, by their place in the response.
    items: Vec<(u64, Value)>,
    model: Option<String>,
    usage: Usage,
    /// Some of the reply has arrived (so a failure isn't retried).
    received: bool,
    /// Set when the response ended early.
    stop: Option<Stop>,
}

impl Accumulator {
    /// Returns true when the response is complete.
    fn apply(&mut self, kind: &str, v: &Value, on_delta: &mut dyn FnMut(StreamDelta<'_>)) -> Result<bool, ApiError> {
        match kind {
            "response.created" | "response.in_progress" => {
                self.model = v["response"]["model"].as_str().map(str::to_string);
            }
            "response.output_item.added" => {
                let item = &v["item"];
                match item["type"].as_str() {
                    Some("function_call") => {
                        self.received = true;
                        on_delta(StreamDelta::ToolStart { name: item["name"].as_str().unwrap_or("tool"), server: false });
                    }
                    Some("web_search_call") => {
                        self.received = true;
                        on_delta(StreamDelta::ToolStart { name: "web_search", server: true });
                    }
                    _ => {}
                }
            }
            "response.output_text.delta" | "response.refusal.delta" => {
                self.received = true;
                on_delta(StreamDelta::Text(v["delta"].as_str().unwrap_or("")));
            }
            "response.reasoning_summary_text.delta" => {
                on_delta(StreamDelta::Thinking(v["delta"].as_str().unwrap_or("")));
            }
            "response.output_item.done" => {
                let index = v["output_index"].as_u64().unwrap_or(self.items.len() as u64);
                self.items.push((index, v["item"].clone()));
            }
            "response.completed" | "response.incomplete" => {
                let r = &v["response"];
                self.usage = usage(&r["usage"]);
                if let Some(model) = r["model"].as_str() {
                    self.model = Some(model.to_string());
                }
                // The full output, when given, is the authority.
                if let Some(output) = r["output"].as_array().filter(|o| !o.is_empty()) {
                    self.items = output.iter().cloned().enumerate().map(|(i, item)| (i as u64, item)).collect();
                }
                if kind == "response.incomplete" {
                    self.stop = Some(match r["incomplete_details"]["reason"].as_str() {
                        Some("content_filter") => Stop::Refusal(Some("content filter".into())),
                        _ => Stop::MaxTokens,
                    });
                }
                return Ok(true);
            }
            "response.failed" => {
                let e = &v["response"]["error"];
                return Err(ApiError::new(
                    None,
                    e["code"].as_str().unwrap_or("api_error"),
                    e["message"].as_str().unwrap_or("the response failed"),
                ));
            }
            "error" => {
                let e = if v["error"].is_object() { &v["error"] } else { v };
                return Err(ApiError::new(
                    None,
                    e["code"].as_str().or(e["type"].as_str()).unwrap_or("api_error"),
                    e["message"].as_str().unwrap_or("stream error"),
                ));
            }
            _ => {}
        }
        Ok(false)
    }

    fn finish(mut self) -> Reply {
        self.items.sort_by_key(|(i, _)| *i);
        let mut calls = Vec::new();
        let mut refused = false;
        let mut content = Vec::new();
        for (_, item) in self.items {
            match item["type"].as_str() {
                Some("function_call") => {
                    let args = item["arguments"].as_str().unwrap_or("");
                    let (input, invalid_input) = match serde_json::from_str::<Value>(args) {
                        Ok(v) if v.is_object() => (v, None),
                        _ if args.trim().is_empty() => (json!({}), None),
                        _ => (json!({}), Some(args.to_string())),
                    };
                    calls.push(PendingTool {
                        id: item["call_id"].as_str().unwrap_or("").to_string(),
                        name: item["name"].as_str().unwrap_or("").to_string(),
                        input,
                        invalid_input,
                    });
                }
                Some("message") => {
                    let parts = item["content"].as_array().map(Vec::as_slice).unwrap_or_default();
                    refused |= parts.iter().any(|p| p["type"] == "refusal");
                }
                _ => {}
            }
            content.push(json!({"type": "openai_item", "item": item}));
        }
        let stop = self.stop.unwrap_or(if refused {
            Stop::Refusal(None)
        } else if calls.is_empty() {
            Stop::Done
        } else {
            Stop::ToolUse
        });
        Reply { content, calls, stop, model: self.model }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::provider::Provider;
    use crate::ai::AiSettings;

    fn run(events: &[(&str, Value)]) -> (Reply, String) {
        let mut acc = Accumulator::default();
        let mut text = String::new();
        let mut sink = |d: StreamDelta<'_>| {
            if let StreamDelta::Text(t) = d {
                text.push_str(t);
            }
        };
        let mut done = false;
        for (k, v) in events {
            done = acc.apply(k, v, &mut sink).unwrap();
        }
        assert!(done, "the response never completed");
        (acc.finish(), text)
    }

    #[test]
    fn a_streamed_response_gives_its_text_calls_and_items_back() {
        let reasoning = json!({"type": "reasoning", "id": "rs_1", "summary": [], "encrypted_content": "enc"});
        let call = json!({"type": "function_call", "id": "fc_1", "call_id": "call_1", "name": "read_terminal", "arguments": "{\"lines\": 5}", "status": "completed"});
        let (reply, text) = run(&[
            ("response.created", json!({"response": {"model": "gpt-6.1-sol"}})),
            ("response.output_item.added", json!({"output_index": 0, "item": {"type": "reasoning"}})),
            ("response.output_item.done", json!({"output_index": 0, "item": reasoning})),
            ("response.output_item.added", json!({"output_index": 1, "item": {"type": "message"}})),
            ("response.output_text.delta", json!({"output_index": 1, "delta": "Let me "})),
            ("response.output_text.delta", json!({"output_index": 1, "delta": "look."})),
            ("response.output_item.added", json!({"output_index": 2, "item": {"type": "function_call", "name": "read_terminal"}})),
            ("response.output_item.done", json!({"output_index": 2, "item": call})),
            (
                "response.completed",
                json!({"response": {"model": "gpt-6.1-sol-2026-09-11", "usage": {"input_tokens": 120, "input_tokens_details": {"cached_tokens": 20}, "output_tokens": 30}}}),
            ),
        ]);
        assert_eq!(text, "Let me look.");
        assert_eq!(reply.stop, Stop::ToolUse);
        assert_eq!(reply.model.as_deref(), Some("gpt-6.1-sol-2026-09-11"));
        assert_eq!(reply.calls[0].id, "call_1");
        assert_eq!(reply.calls[0].input, json!({"lines": 5}));
        // The items go back exactly as they came, in their order.
        assert_eq!(reply.content[0]["item"]["encrypted_content"], "enc");
        assert_eq!(reply.content[1]["item"]["call_id"], "call_1");
        let used = usage(&json!({"input_tokens": 120, "input_tokens_details": {"cached_tokens": 20}, "output_tokens": 30}));
        assert_eq!((used.input, used.cache_read, used.output), (100, 20, 30));
    }

    #[test]
    fn cut_off_refused_and_failed_responses_say_so() {
        let (reply, _) = run(&[(
            "response.incomplete",
            json!({"response": {"incomplete_details": {"reason": "max_output_tokens"}, "output": [], "usage": {}}}),
        )]);
        assert_eq!(reply.stop, Stop::MaxTokens);
        let (reply, text) = run(&[
            ("response.refusal.delta", json!({"delta": "I can't help with that."})),
            (
                "response.completed",
                json!({"response": {"output": [{"type": "message", "content": [{"type": "refusal", "refusal": "I can't help with that."}]}], "usage": {}}}),
            ),
        ]);
        assert_eq!(reply.stop, Stop::Refusal(None));
        assert_eq!(text, "I can't help with that.");
        let mut acc = Accumulator::default();
        let err = acc
            .apply("response.failed", &json!({"response": {"error": {"code": "server_error", "message": "boom"}}}), &mut |_| {})
            .unwrap_err();
        assert_eq!((err.kind.as_str(), err.message.as_str()), ("server_error", "boom"));
        let invalid = parse_error(401, r#"{"error": {"message": "Incorrect API key", "type": "invalid_request_error", "code": "invalid_api_key"}}"#);
        assert!(invalid.is_auth() && invalid.kind == "invalid_api_key");
    }

    #[test]
    fn a_picture_and_a_document_become_an_input_image_and_an_input_file() {
        let messages = vec![json!({"role": "user", "content": [
            {"type": "text", "text": "look"},
            {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "AAAA"}},
            {"type": "document", "source": {"type": "base64", "media_type": "application/pdf", "data": "BBBB"}, "title": "r.pdf"},
        ]})];
        let items = input(&messages);
        assert_eq!(items, [json!({"role": "user", "content": [
            {"type": "input_text", "text": "look"},
            {"type": "input_image", "image_url": "data:image/png;base64,AAAA"},
            {"type": "input_file", "filename": "r.pdf", "file_data": "data:application/pdf;base64,BBBB"},
        ]})]);
    }

    #[test]
    fn the_conversation_becomes_input_items() {
        let messages = vec![
            json!({"role": "user", "content": [{"type": "text", "text": "<ctx>"}, {"type": "text", "text": "hi"}]}),
            json!({"role": "assistant", "content": [
                {"type": "openai_item", "item": {"type": "reasoning", "id": "rs_1", "encrypted_content": "enc"}},
                {"type": "openai_item", "item": {"type": "function_call", "call_id": "call_1", "name": "read_terminal", "arguments": "{}"}},
            ]}),
            json!({"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "call_1", "content": "boom", "is_error": true},
                {"type": "text", "text": "and now?"},
            ]}),
            json!({"role": "assistant", "content": [{"type": "text", "text": "(stopped by the user)"}]}),
        ];
        let items = input(&messages);
        assert_eq!(items[0], json!({"role": "user", "content": [{"type": "input_text", "text": "<ctx>"}, {"type": "input_text", "text": "hi"}]}));
        assert_eq!(items[1]["type"], "reasoning");
        assert_eq!(items[2]["type"], "function_call");
        assert_eq!(items[3], json!({"type": "function_call_output", "call_id": "call_1", "output": "Error: boom"}));
        assert_eq!(items[4]["role"], "user");
        assert_eq!(items[5], json!({"role": "assistant", "content": "(stopped by the user)"}));

        let mut settings = AiSettings::for_tests(Provider::OpenAi, "gpt-6-luna", "");
        settings.effort = "max".into();
        settings.web_search = true;
        let req = Request { settings: &settings, system: "be brief", messages: &messages };
        let b = body(&req);
        assert_eq!(b["instructions"], "be brief");
        // Flow's effort levels are OpenAI's.
        assert_eq!(b["reasoning"]["effort"], "max");
        assert_eq!(b["store"], false);
        assert_eq!(b["tools"][0]["type"], "function");
        assert_eq!(b["tools"][0]["parameters"]["required"][0], "command");
        assert!(b["tools"].as_array().unwrap().iter().any(|t| t["type"] == "web_search"));

        // An MCP server's tool turned on goes as a function too.
        settings.server_tools = vec![crate::ai::ServerTool::for_tests()];
        let b = body(&Request { settings: &settings, system: "be brief", messages: &messages });
        let tool = b["tools"].as_array().unwrap().iter().find(|t| t["name"] == "mcp__notes__find_note").expect("the server's tool");
        assert_eq!(tool["type"], "function");
        assert_eq!(tool["parameters"]["properties"]["words"]["type"], "string");
    }

    /// Hits the real API with a bogus key: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn bogus_key_is_rejected_as_auth_error() {
        let client = Client::new("sk-proj-not-a-real-key-0000000000000000".into(), Provider::OpenAi.default_base_url());
        let err = client.validate_key().expect_err("bogus key must fail");
        assert!(err.is_auth() && !err.is_retryable(), "{err}");
    }
}
