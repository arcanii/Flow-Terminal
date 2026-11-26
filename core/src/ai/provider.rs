//! What the assistant's model providers have in common: which one answers,
//! the errors they report, how a streamed reply comes back to the tool loop,
//! and the HTTP and SSE plumbing their clients share. Each provider's wire
//! format lives in its own module: claude.rs, openai.rs and gemini.rs.

use std::io::BufRead;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde_json::Value;

use super::usage::Usage;
use super::{AiSettings, PendingTool};

/// Who answers the assistant's requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Provider {
    #[default]
    Anthropic,
    OpenAi,
    Gemini,
}

/// The provider in use, as `set_in_use` last made it, by its place in
/// `Provider::ALL`.
#[cfg(not(test))]
static IN_USE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

#[cfg(test)]
thread_local! {
    static TEST_IN_USE: std::cell::Cell<Provider> = const { std::cell::Cell::new(Provider::Anthropic) };
}

/// Makes `provider` the one the words naming the assistant follow ("Ask
/// GPT" in the menus, the palette and the keys page), as `keys::set` does
/// the keys: process-wide, since those are worded apart from the window
/// (the Mac's menu as it opens), and per thread in tests.
pub fn set_in_use(provider: Provider) {
    #[cfg(test)]
    TEST_IN_USE.with(|p| p.set(provider));
    #[cfg(not(test))]
    IN_USE.store(Provider::ALL.iter().position(|p| *p == provider).unwrap_or(0), Ordering::Relaxed);
}

/// The provider config.toml chooses (`ai.provider`), for a program that
/// runs apart from Flow (`flow-help`): read as written, nothing written
/// back, and Anthropic where it says none.
pub fn from_config_file() -> Provider {
    let text = std::fs::read_to_string(crate::config::config_path()).unwrap_or_default();
    let table = toml::from_str::<toml::Table>(&text).unwrap_or_default();
    Provider::from_id(table.get("ai").and_then(|ai| ai.get("provider")).and_then(|p| p.as_str()).unwrap_or_default())
}

/// The provider in use: as `set_in_use` last made it, else Anthropic.
pub fn in_use() -> Provider {
    #[cfg(test)]
    return TEST_IN_USE.with(|p| p.get());
    #[cfg(not(test))]
    Provider::ALL.get(IN_USE.load(Ordering::Relaxed)).copied().unwrap_or_default()
}

impl Provider {
    pub const ALL: [Provider; 3] = [Provider::Anthropic, Provider::OpenAi, Provider::Gemini];

    /// As written in config.toml (`ai.provider`).
    pub fn id(self) -> &'static str {
        match self {
            Provider::Anthropic => "anthropic",
            Provider::OpenAi => "openai",
            Provider::Gemini => "gemini",
        }
    }

    /// Unknown names mean Anthropic, the default.
    pub fn from_id(id: &str) -> Self {
        Self::ALL.into_iter().find(|p| p.id().eq_ignore_ascii_case(id.trim())).unwrap_or_default()
    }

    /// What the UI calls the assistant: the model family.
    pub fn name(self) -> &'static str {
        match self {
            Provider::Anthropic => "Claude",
            Provider::OpenAi => "GPT",
            Provider::Gemini => "Gemini",
        }
    }

    /// Who issues the API keys.
    pub fn company(self) -> &'static str {
        match self {
            Provider::Anthropic => "Anthropic",
            Provider::OpenAi => "OpenAI",
            Provider::Gemini => "Google",
        }
    }

    /// Where to get a key.
    pub fn console(self) -> &'static str {
        match self {
            Provider::Anthropic => "console.anthropic.com",
            Provider::OpenAi => "platform.openai.com",
            Provider::Gemini => "aistudio.google.com",
        }
    }

    /// The environment variable whose key wins over the stored one.
    pub fn key_env(self) -> &'static str {
        match self {
            Provider::Anthropic => "ANTHROPIC_API_KEY",
            Provider::OpenAi => "OPENAI_API_KEY",
            Provider::Gemini => "GEMINI_API_KEY",
        }
    }

    /// The environment variable whose endpoint wins over the configured one.
    pub fn base_url_env(self) -> &'static str {
        match self {
            Provider::Anthropic => "ANTHROPIC_BASE_URL",
            Provider::OpenAi => "OPENAI_BASE_URL",
            Provider::Gemini => "GEMINI_BASE_URL",
        }
    }

    pub fn default_base_url(self) -> &'static str {
        match self {
            Provider::Anthropic => "https://api.anthropic.com",
            Provider::OpenAi => "https://api.openai.com",
            Provider::Gemini => "https://generativelanguage.googleapis.com",
        }
    }

    /// Model ids with the short names Settings shows, most capable first.
    pub fn models(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Provider::Anthropic => {
                &[("claude-opus-5-5", "Opus 5.5"), ("claude-sonnet-5-5", "Sonnet 5.5"), ("claude-haiku-5-5", "Haiku 5.5")]
            }
            Provider::OpenAi => &[("gpt-6-astra", "6 Astra"), ("gpt-6.1-sol", "6.1 Sol"), ("gpt-6-luna", "6 Luna")],
            Provider::Gemini => {
                &[("gemini-4-argon", "4 Argon"), ("gemini-3.8-flash", "3.8 Flash"), ("gemini-3.5-flash-lite", "3.5 Flash-Lite")]
            }
        }
    }

    /// The model a new config starts with: each provider's recommended one.
    pub fn default_model(self) -> &'static str {
        match self {
            Provider::Anthropic => "claude-opus-5-5",
            Provider::OpenAi => "gpt-6.1-sol",
            Provider::Gemini => "gemini-4-argon",
        }
    }

    /// How its keys start, for the key dialog's placeholder and first check.
    pub fn key_prefix(self) -> &'static str {
        match self {
            Provider::Anthropic => "sk-ant-",
            Provider::OpenAi => "sk-",
            Provider::Gemini => "AIza",
        }
    }

    /// Catches obvious paste mistakes before a network call. Google's keys
    /// have changed shape before, so for Gemini only the length counts.
    pub fn looks_like_key(self, key: &str) -> bool {
        let key = key.trim();
        let prefix = if self == Provider::Gemini { "" } else { self.key_prefix() };
        key.starts_with(prefix) && key.len() > 20 && !key.contains(char::is_whitespace)
    }

    /// Whether Flow offers it a web search tool.
    pub fn has_web_search(self) -> bool {
        self != Provider::Gemini
    }
}

#[derive(Debug, Clone)]
pub struct ApiError {
    pub status: Option<u16>,
    pub kind: String,
    pub message: String,
    pub retry_after: Option<Duration>,
}

impl ApiError {
    pub fn new(status: Option<u16>, kind: impl Into<String>, message: impl Into<String>) -> Self {
        Self { status, kind: kind.into(), message: message.into(), retry_after: None }
    }

    pub fn is_auth(&self) -> bool {
        self.status == Some(401) || matches!(self.kind.as_str(), "authentication_error" | "invalid_api_key")
    }

    /// Rate limits, overload, server errors and dropped connections are worth retrying.
    pub fn is_retryable(&self) -> bool {
        match self.status {
            Some(408 | 409 | 429 | 500 | 502 | 503 | 504 | 529) => true,
            Some(_) => self.kind == "overloaded_error",
            None => self.kind != "cancelled",
        }
    }

    pub fn cancelled() -> Self {
        Self::new(None, "cancelled", "cancelled")
    }

    pub fn connection(err: impl std::fmt::Display) -> Self {
        Self::new(None, "connection_error", err.to_string())
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.status {
            Some(s) => write!(f, "{} ({s}): {}", self.kind, self.message),
            None => write!(f, "{}: {}", self.kind, self.message),
        }
    }
}

/// Things that happen while a reply streams in.
pub enum StreamDelta<'a> {
    Text(&'a str),
    Thinking(&'a str),
    /// A tool call began (client or server tool).
    ToolStart { name: &'a str, server: bool },
    /// The server switched models after a refusal.
    Fallback { to: &'a str },
}

/// Why a reply ended.
#[derive(Debug, Clone, PartialEq)]
pub enum Stop {
    /// It answered.
    Done,
    /// It called tools, for the UI to run.
    ToolUse,
    /// It was cut off at the output limit.
    MaxTokens,
    /// It declined; why, when the API says.
    Refusal(Option<String>),
    /// The server paused a long turn; sending the history again goes on.
    Pause,
    /// The reply went wrong some other way; the message says how.
    Failed(String),
}

/// One streamed reply, ready for the conversation.
#[derive(Debug)]
pub struct Reply {
    /// Blocks for the history: `text`, plus the provider's own kinds that
    /// only its client reads back (Claude's blocks as they are,
    /// `openai_item` and `gemini_part`).
    pub content: Vec<Value>,
    /// The tools it called, in order.
    pub calls: Vec<PendingTool>,
    pub stop: Stop,
    /// The model that answered, when the API says.
    pub model: Option<String>,
}

impl Reply {
    /// Drops its tool calls, which must never run from a declined or cut-off
    /// reply (and would need results the next request can't give). OpenAI's
    /// reasoning goes with them, so the messages kept lose their ids, which
    /// would tie them to it.
    pub fn drop_calls(&mut self) {
        self.calls.clear();
        self.content.retain(|b| {
            let openai = b["type"] == "openai_item";
            !(b["type"] == "tool_use"
                || openai && matches!(b["item"]["type"].as_str(), Some("function_call" | "reasoning"))
                || b["type"] == "gemini_part" && b["part"].get("functionCall").is_some())
        });
        for b in &mut self.content {
            if b["type"] == "openai_item" {
                if let Some(item) = b["item"].as_object_mut() {
                    item.remove("id");
                }
            }
        }
    }
}

/// One request: the settings, the system prompt and the conversation so far.
pub struct Request<'a> {
    pub settings: &'a AiSettings,
    pub system: &'a str,
    pub messages: &'a [Value],
}

/// The HTTP client every provider uses.
pub fn http_agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_connect(Some(Duration::from_secs(20)))
        // Long agentic turns can stream for many minutes.
        .timeout_global(Some(Duration::from_secs(30 * 60)))
        .user_agent(concat!("flow-terminal/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

/// The error in a non-200 response, read by the provider's `parse`, with its
/// Retry-After.
pub fn error_response(resp: ureq::http::Response<ureq::Body>, parse: fn(u16, &str) -> ApiError) -> ApiError {
    let status = resp.status().as_u16();
    let retry_after = resp
        .headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<f64>().ok())
        .map(Duration::from_secs_f64);
    let text = resp.into_body().read_to_string().unwrap_or_default();
    let mut err = parse(status, &text);
    err.retry_after = retry_after;
    err
}

/// Runs one request, again on failures that came before any of the reply
/// did (up to four tries, backing off). Each `attempt` sets whether anything
/// arrived and the tokens it used; those from every try, including ones
/// that failed or were cancelled part way, are added to `spent`.
pub fn with_retries(
    name: &str,
    cancel: &AtomicBool,
    spent: &mut Usage,
    mut attempt: impl FnMut(&mut bool, &mut Usage) -> Result<Reply, ApiError>,
) -> Result<Reply, ApiError> {
    let mut tries = 0;
    loop {
        tries += 1;
        let mut received_any = false;
        let mut used = Usage::default();
        let result = attempt(&mut received_any, &mut used);
        if used.total() > 0 {
            used.requests = 1;
            spent.add(&used);
        }
        match result {
            Ok(reply) => return Ok(reply),
            Err(err) if err.is_retryable() && !received_any && tries < 4 => {
                let wait = err.retry_after.unwrap_or_else(|| Duration::from_millis(800 * 2u64.pow(tries - 1)));
                log::warn!("{name} request failed ({err}); retrying in {wait:?}");
                let deadline = std::time::Instant::now() + wait.min(Duration::from_secs(30));
                while std::time::Instant::now() < deadline {
                    if cancel.load(Ordering::Relaxed) {
                        return Err(ApiError::cancelled());
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
            Err(err) => return Err(err),
        }
    }
}

/// Reads server-sent events, handing each event's name (or, without one, its
/// payload's `type`) and JSON payload to `on_event` until that says the reply
/// is complete. Returns false when the stream ended first.
pub fn read_sse(
    reader: impl BufRead,
    cancel: &AtomicBool,
    mut on_event: impl FnMut(&str, Value) -> Result<bool, ApiError>,
) -> Result<bool, ApiError> {
    let mut event_name = String::new();
    let mut data = String::new();
    for line in reader.lines() {
        if cancel.load(Ordering::Relaxed) {
            return Err(ApiError::cancelled());
        }
        let line = line.map_err(ApiError::connection)?;
        if line.is_empty() {
            if !data.is_empty() && data != "[DONE]" {
                let value: Value = serde_json::from_str(&data)
                    .map_err(|e| ApiError::new(None, "parse_error", format!("bad SSE payload: {e}")))?;
                let kind = if event_name.is_empty() {
                    value.get("type").and_then(Value::as_str).unwrap_or("").to_string()
                } else {
                    std::mem::take(&mut event_name)
                };
                if on_event(&kind, value)? {
                    return Ok(true);
                }
            }
            data.clear();
            event_name.clear();
            continue;
        }
        if let Some(rest) = line.strip_prefix("event:") {
            event_name = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix("data:") {
            if !data.is_empty() {
                data.push('\n');
            }
            data.push_str(rest.strip_prefix(' ').unwrap_or(rest));
        }
    }
    Ok(false)
}

/// A tool result's text for providers without an error flag on results.
pub fn result_text(block: &Value) -> String {
    let content = block["content"].as_str().unwrap_or("");
    if block["is_error"] == true {
        format!("Error: {content}")
    } else {
        content.to_string()
    }
}

/// A model `/model` offers: its provider, id and short name, and whether
/// it's from the provider's own list rather than Flow's
/// (`docs/design/live-models.md`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Model {
    pub provider: Provider,
    pub id: String,
    pub name: String,
    pub listed: bool,
}

impl Model {
    /// Its name with its provider's, "Claude Opus 5.5"; one its provider
    /// names by its id alone, the id.
    pub fn full_name(&self) -> String {
        if self.name == self.id {
            self.id.clone()
        } else {
            format!("{} {}", self.provider.name(), self.name)
        }
    }
}

/// Every model Flow offers, the provider in use's first, then the models
/// of its own list `live` that Flow doesn't list (`models::extras`), then
/// the other providers' in their order: the list `/model` shows, which its
/// numbers count by.
pub fn model_list(in_use: Provider, live: &[super::models::Listed]) -> Vec<Model> {
    let ours = |p: Provider| p.models().iter().map(move |&(id, name)| Model { provider: p, id: id.into(), name: name.into(), listed: false });
    let mut list: Vec<Model> = ours(in_use).collect();
    list.extend(super::models::extras(in_use, live).into_iter().map(|m| {
        // The provider's name for it, less the provider's own name before it.
        let name = m.name.strip_prefix(&format!("{} ", in_use.name())).map_or(m.name.clone(), str::to_string);
        Model { provider: in_use, id: m.id, name, listed: true }
    }));
    for p in Provider::ALL.into_iter().filter(|p| *p != in_use) {
        list.extend(ours(p));
    }
    list
}

/// What `/model`'s words name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelFound {
    One(Provider, String),
    /// Several, by their names with their providers'.
    Several(Vec<String>),
    None,
}

/// The model `words` name, the provider in use being `in_use` and `live`
/// its own list: its number in [`model_list`], its id or name ("Sonnet
/// 5.5", "Claude Sonnet 5.5"), in any case, or words each found in those
/// ("sonnet", "gpt luna"). Of several found, the provider in use's count
/// first.
pub fn find_model(words: &str, in_use: Provider, live: &[super::models::Listed]) -> ModelFound {
    let list = model_list(in_use, live);
    let words = words.trim().to_lowercase();
    let one = |m: &Model| ModelFound::One(m.provider, m.id.clone());
    if let Ok(n) = words.parse::<usize>() {
        return n.checked_sub(1).and_then(|i| list.get(i)).map_or(ModelFound::None, one);
    }
    if let Some(m) = list.iter().find(|m| m.id.to_lowercase() == words || m.name.to_lowercase() == words || m.full_name().to_lowercase() == words) {
        return one(m);
    }
    let found: Vec<&Model> = list
        .iter()
        .filter(|m| {
            let named = format!("{} {}", m.id, m.full_name()).to_lowercase();
            !words.is_empty() && words.split_whitespace().all(|w| named.contains(w))
        })
        .collect();
    let ours: Vec<&Model> = found.iter().copied().filter(|m| m.provider == in_use).collect();
    match (found.as_slice(), ours.as_slice()) {
        ([], _) => ModelFound::None,
        ([m], _) | (_, [m]) => one(m),
        (_, []) => ModelFound::Several(found.iter().map(|m| m.full_name()).collect()),
        (_, ours) => ModelFound::Several(ours.iter().map(|m| m.full_name()).collect()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn model_finds_a_model_by_its_number_id_name_or_words() {
        use Provider::*;
        let one = |words: &str, in_use| find_model(words, in_use, &[]);
        let found = |p, id: &str| ModelFound::One(p, id.to_string());
        assert_eq!(model_list(OpenAi, &[])[0].provider, OpenAi);
        assert_eq!(one("2", Anthropic), found(Anthropic, "claude-sonnet-5-5"));
        assert_eq!(one("4", Anthropic), found(OpenAi, "gpt-6-astra"));
        assert_eq!(one("0", Anthropic), ModelFound::None);
        assert_eq!(one("99", Anthropic), ModelFound::None);
        assert_eq!(one("claude-haiku-5-5", OpenAi), found(Anthropic, "claude-haiku-5-5"));
        assert_eq!(one("Sonnet 5.5", Gemini), found(Anthropic, "claude-sonnet-5-5"));
        assert_eq!(one("GPT 6 Luna", Anthropic), found(OpenAi, "gpt-6-luna"));
        assert_eq!(one("opus", Gemini), found(Anthropic, "claude-opus-5-5"));
        assert_eq!(one("gpt luna", Anthropic), found(OpenAi, "gpt-6-luna"));
        // Several: the provider in use's, else all of them.
        assert_eq!(one("flash", Gemini), ModelFound::Several(vec!["Gemini 3.8 Flash".into(), "Gemini 3.5 Flash-Lite".into()]));
        assert_eq!(one("5.5", Anthropic), ModelFound::Several(vec!["Claude Opus 5.5".into(), "Claude Sonnet 5.5".into(), "Claude Haiku 5.5".into()]));
        // A number is a place in the list, never a word of a name.
        assert_eq!(one("6", Anthropic), found(OpenAi, "gpt-6-luna"));
        assert_eq!(one("nothing like it", Anthropic), ModelFound::None);
    }

    #[test]
    fn the_providers_own_models_come_after_flows_named_as_it_names_them() {
        use super::super::models::Listed;
        use Provider::*;
        let live = [
            Listed { id: "claude-opus-6-20270101".into(), name: "Claude Opus 6".into() },
            Listed { id: "claude-opus-5-5-20261001".into(), name: "Claude Opus 5.5".into() },
        ];
        let list = model_list(Anthropic, &live);
        let names: Vec<String> = list.iter().map(Model::full_name).collect();
        assert_eq!(names[..5], ["Claude Opus 5.5", "Claude Sonnet 5.5", "Claude Haiku 5.5", "Claude Opus 6", "GPT 6 Astra"]);
        assert!(list[3].listed && !list[2].listed);
        assert_eq!(find_model("opus 6", Anthropic, &live), ModelFound::One(Anthropic, "claude-opus-6-20270101".into()));
        assert_eq!(find_model("claude-opus-6-20270101", Anthropic, &live), ModelFound::One(Anthropic, "claude-opus-6-20270101".into()));
        // GPT's are named by their ids.
        let gpt = [Listed { id: "gpt-7".into(), name: "gpt-7".into() }];
        assert_eq!(model_list(OpenAi, &gpt)[3].full_name(), "gpt-7");
        // Another provider's list isn't this one's.
        assert_eq!(model_list(Gemini, &[]).len(), 9);
    }

    #[test]
    fn providers_read_back_from_config_and_unknown_ones_mean_anthropic() {
        for p in Provider::ALL {
            assert_eq!(Provider::from_id(p.id()), p);
            assert!(p.models().iter().any(|(id, _)| *id == p.default_model()));
        }
        assert_eq!(Provider::from_id(" OpenAI "), Provider::OpenAi);
        assert_eq!(Provider::from_id("mistral"), Provider::Anthropic);
    }

    #[test]
    fn keys_are_checked_for_their_provider_s_shape() {
        assert!(Provider::Anthropic.looks_like_key("sk-ant-api03-abcdefghijklmnop"));
        assert!(!Provider::Anthropic.looks_like_key("sk-proj-abcdefghijklmnopqrstu"));
        assert!(Provider::OpenAi.looks_like_key("sk-proj-abcdefghijklmnopqrstu"));
        assert!(!Provider::OpenAi.looks_like_key("AIzaSyabcdefghijklmnopqrstuvw"));
        assert!(Provider::Gemini.looks_like_key("AIzaSyabcdefghijklmnopqrstuvw"));
        assert!(!Provider::Gemini.looks_like_key("too short"));
        assert!(!Provider::Gemini.looks_like_key("AIzaSy abcdefghijklmnopqrstuvw"));
    }

    #[test]
    fn dropping_calls_leaves_only_what_can_be_sent_back() {
        let mut reply = Reply {
            content: vec![
                json!({"type": "text", "text": "hi"}),
                json!({"type": "tool_use", "id": "t", "name": "x", "input": {}}),
                json!({"type": "openai_item", "item": {"type": "reasoning", "id": "rs_1"}}),
                json!({"type": "openai_item", "item": {"type": "function_call", "call_id": "c"}}),
                json!({"type": "openai_item", "item": {"type": "message", "id": "msg_1", "content": []}}),
                json!({"type": "gemini_part", "part": {"functionCall": {"name": "x"}}}),
                json!({"type": "gemini_part", "part": {"text": "ok"}}),
            ],
            calls: vec![PendingTool { id: "t".into(), name: "x".into(), input: json!({}), invalid_input: None }],
            stop: Stop::MaxTokens,
            model: None,
        };
        reply.drop_calls();
        assert!(reply.calls.is_empty());
        let kinds: Vec<String> = reply.content.iter().map(|b| b["type"].as_str().unwrap().to_string()).collect();
        assert_eq!(kinds, ["text", "openai_item", "gemini_part"]);
        assert!(reply.content[1]["item"].get("id").is_none());
    }

    #[test]
    fn sse_events_are_named_by_their_event_line_or_their_type() {
        let stream = "event: first\ndata: {\"a\": 1}\n\ndata: {\"type\": \"second\"}\n\ndata: [DONE]\n\n";
        let mut seen = Vec::new();
        let done = read_sse(stream.as_bytes(), &AtomicBool::new(false), |kind, v| {
            seen.push((kind.to_string(), v));
            Ok(false)
        })
        .unwrap();
        assert!(!done);
        assert_eq!(seen[0], ("first".to_string(), json!({"a": 1})));
        assert_eq!(seen[1].0, "second");
        assert_eq!(seen.len(), 2);
    }
}
