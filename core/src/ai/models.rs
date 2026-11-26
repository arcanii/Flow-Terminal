//! The providers' own lists of models (`docs/design/live-models.md`): the
//! endpoint each provider has for the models a key may use, which `/model`
//! shows after Flow's own (`provider::model_list`), so a model released
//! since Flow's last release can be chosen. Fetched on a thread of its own
//! (`fetch`), every page of it, and kept in memory a day.

use std::time::{Duration, Instant};

use serde_json::Value;

use super::provider::{self, Provider};

/// How long a provider's list is kept before `/model` asks for it again.
pub const KEPT_FOR: Duration = Duration::from_secs(24 * 3600);
/// How long after a list that didn't come it's asked for again.
pub const AGAIN_AFTER: Duration = Duration::from_secs(10 * 60);
/// The most pages read, past which the rest is left out.
const MOST_PAGES: usize = 10;

/// A model a provider lists: its id, and the name it gives it (its id
/// where it gives none).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    pub id: String,
    pub name: String,
}

/// A provider's list as it last came, or, `failed`, the one before it (or
/// none) where it didn't.
#[derive(Debug, Clone)]
pub struct Live {
    pub provider: Provider,
    pub models: Vec<Listed>,
    pub at: Instant,
    pub failed: bool,
}

impl Live {
    /// Whether it's recent enough not to ask again at `now`.
    pub fn fresh(&self, now: Instant) -> bool {
        now.duration_since(self.at) < if self.failed { AGAIN_AFTER } else { KEPT_FOR }
    }
}

/// Asks `provider`, at `base_url` with `key`, for the models the key may
/// use, page after page: Claude's and GPT's `/v1/models`, Gemini's
/// `/v1beta/models`, as their key checks ask. Only those that can hold a
/// conversation, in the provider's order (OpenAI's newest first).
pub fn fetch(provider: Provider, key: &str, base_url: &str) -> Result<Vec<Listed>, String> {
    let agent = provider::http_agent();
    let base = base_url.trim_end_matches('/');
    let mut models = Vec::new();
    let mut after: Option<String> = None;
    for _ in 0..MOST_PAGES {
        let url = match (provider, &after) {
            (Provider::Anthropic, None) => format!("{base}/v1/models?limit=1000"),
            (Provider::Anthropic, Some(id)) => format!("{base}/v1/models?limit=1000&after_id={}", encoded(id)),
            (Provider::OpenAi, _) => format!("{base}/v1/models"),
            (Provider::Gemini, None) => format!("{base}/v1beta/models?pageSize=1000"),
            (Provider::Gemini, Some(token)) => format!("{base}/v1beta/models?pageSize=1000&pageToken={}", encoded(token)),
        };
        let request = agent.get(&url);
        let request = match provider {
            Provider::Anthropic => request.header("x-api-key", key).header("anthropic-version", super::claude::API_VERSION),
            Provider::OpenAi => request.header("authorization", format!("Bearer {key}")),
            Provider::Gemini => request.header("x-goog-api-key", key),
        };
        let response = request.call().map_err(|e| e.to_string())?;
        let status = response.status().as_u16();
        let body = response.into_body().read_to_string().map_err(|e| e.to_string())?;
        if status != 200 {
            return Err(format!("HTTP {status}"));
        }
        let json: Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
        let (page, next) = read_page(provider, &json);
        models.extend(page);
        match next {
            Some(next) => after = Some(next),
            None => break,
        }
    }
    Ok(models)
}

/// One page of `provider`'s list: its models that can hold a
/// conversation, and where the next page starts, if there's one.
pub fn read_page(provider: Provider, json: &Value) -> (Vec<Listed>, Option<String>) {
    let text = |v: &Value, key: &str| v.get(key).and_then(Value::as_str).map(str::to_string);
    match provider {
        Provider::Anthropic => {
            let models = json["data"].as_array().into_iter().flatten().filter_map(|m| {
                let id = text(m, "id")?;
                Some(Listed { name: text(m, "display_name").unwrap_or_else(|| id.clone()), id })
            });
            let next = json["has_more"].as_bool().filter(|more| *more).and_then(|_| text(json, "last_id"));
            (models.collect(), next)
        }
        Provider::OpenAi => {
            let mut models: Vec<(i64, Listed)> = json["data"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|m| {
                    let id = text(m, "id").filter(|id| chat_model(id))?;
                    Some((m["created"].as_i64().unwrap_or(0), Listed { name: id.clone(), id }))
                })
                .collect();
            models.sort_by_key(|m| std::cmp::Reverse(m.0));
            (models.into_iter().map(|(_, m)| m).collect(), None)
        }
        Provider::Gemini => {
            let models = json["models"].as_array().into_iter().flatten().filter_map(|m| {
                let chats = m["supportedGenerationMethods"].as_array().is_some_and(|ways| ways.iter().any(|w| w == "generateContent"));
                let id = text(m, "name")?;
                let id = id.strip_prefix("models/").unwrap_or(&id).to_string();
                chats.then(|| Listed { name: text(m, "displayName").unwrap_or_else(|| id.clone()), id })
            });
            let next = text(json, "nextPageToken").filter(|t| !t.is_empty());
            (models.collect(), next)
        }
    }
}

/// Whether OpenAI's model `id` holds a conversation: one named `gpt-…`,
/// `chatgpt-…` or `o<digit>…`, but not for embeddings, speech,
/// transcription, images, moderation, realtime or search.
pub fn chat_model(id: &str) -> bool {
    const NOT_CHAT: [&str; 9] = ["embedding", "tts", "whisper", "transcribe", "audio", "realtime", "image", "moderation", "search"];
    let o_series = id.strip_prefix('o').is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_digit()));
    let named = id.starts_with("gpt-") || id.starts_with("chatgpt-") || o_series;
    named && !NOT_CHAT.iter().any(|word| id.contains(word))
}

/// Whether `id` is a dated copy of model `of`: `of` and `-20261001`,
/// `-2026-10-01` or `-001`, as the providers list their snapshots beside
/// their names.
pub fn snapshot_of(id: &str, of: &str) -> bool {
    let Some(rest) = id.strip_prefix(of).and_then(|rest| rest.strip_prefix('-')) else { return false };
    let digits = |s: &str, n: usize| s.len() == n && s.bytes().all(|b| b.is_ascii_digit());
    let dashed = rest.split('-').collect::<Vec<_>>();
    digits(rest, 8) || digits(rest, 3) || digits(rest, 4) || (dashed.len() == 3 && digits(dashed[0], 4) && digits(dashed[1], 2) && digits(dashed[2], 2))
}

/// The models of `provider`'s list `live` that Flow doesn't list itself:
/// none of its own, nor a dated copy of one of them or of another listed.
pub fn extras(provider: Provider, live: &[Listed]) -> Vec<Listed> {
    let ours: Vec<&str> = provider.models().iter().map(|(id, _)| *id).collect();
    let mut out: Vec<Listed> = Vec::new();
    for m in live {
        let copy = ours.iter().copied().chain(live.iter().map(|m| m.id.as_str())).any(|of| snapshot_of(&m.id, of));
        if !ours.contains(&m.id.as_str()) && !copy && !out.iter().any(|o| o.id == m.id) {
            out.push(m.clone());
        }
    }
    out
}

/// `s` as a URL's query value: all but the unreserved characters
/// percent-encoded.
fn encoded(s: &str) -> String {
    s.bytes()
        .map(|b| if b.is_ascii_alphanumeric() || b"-._~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    fn ids(models: &[Listed]) -> Vec<&str> {
        models.iter().map(|m| m.id.as_str()).collect()
    }

    #[test]
    fn claudes_list_is_read_with_its_names_and_where_the_next_page_starts() {
        let page = json!({"data": [
            {"type": "model", "id": "claude-opus-6-20270101", "display_name": "Claude Opus 6", "created_at": "2027-01-01T00:00:00Z"},
            {"type": "model", "id": "claude-opus-5-5-20261001", "display_name": "Claude Opus 5.5"}
        ], "has_more": true, "last_id": "claude-opus-5-5-20261001"});
        let (models, next) = read_page(Provider::Anthropic, &page);
        assert_eq!(models[0], Listed { id: "claude-opus-6-20270101".into(), name: "Claude Opus 6".into() });
        assert_eq!(next.as_deref(), Some("claude-opus-5-5-20261001"));
        assert_eq!(read_page(Provider::Anthropic, &json!({"data": [], "has_more": false, "last_id": "x"})).1, None);
    }

    #[test]
    fn openais_list_keeps_its_chat_models_newest_first() {
        let page = json!({"object": "list", "data": [
            {"id": "gpt-6-luna", "created": 10},
            {"id": "text-embedding-3-large", "created": 50},
            {"id": "gpt-7", "created": 40},
            {"id": "gpt-6-luna-tts", "created": 30},
            {"id": "o5-mini", "created": 20},
            {"id": "dall-e-3", "created": 60},
            {"id": "omni-moderation-latest", "created": 70}
        ]});
        let (models, next) = read_page(Provider::OpenAi, &page);
        assert_eq!(ids(&models), ["gpt-7", "o5-mini", "gpt-6-luna"]);
        assert_eq!(models[0].name, "gpt-7");
        assert_eq!(next, None);
    }

    #[test]
    fn geminis_list_keeps_those_that_generate_content() {
        let page = json!({"models": [
            {"name": "models/gemini-5-pro", "displayName": "Gemini 5 Pro", "supportedGenerationMethods": ["generateContent", "countTokens"]},
            {"name": "models/text-embedding-004", "displayName": "Text Embedding 004", "supportedGenerationMethods": ["embedContent"]}
        ], "nextPageToken": "abc="});
        let (models, next) = read_page(Provider::Gemini, &page);
        assert_eq!(models, [Listed { id: "gemini-5-pro".into(), name: "Gemini 5 Pro".into() }]);
        assert_eq!(next.as_deref(), Some("abc="));
    }

    #[test]
    fn dated_copies_are_known_by_their_dates_alone() {
        assert!(snapshot_of("claude-opus-5-5-20261001", "claude-opus-5-5"));
        assert!(snapshot_of("gpt-6-astra-2026-08-01", "gpt-6-astra"));
        assert!(snapshot_of("gemini-3.8-flash-001", "gemini-3.8-flash"));
        // Another model, not a copy: Opus 6.1 beside Opus 6.
        assert!(!snapshot_of("claude-opus-6-1", "claude-opus-6"));
        assert!(!snapshot_of("gpt-6-astra-mini", "gpt-6-astra"));
        assert!(!snapshot_of("gpt-6-astra", "gpt-6-astra"));
    }

    #[test]
    fn the_extras_are_what_flow_doesnt_list_without_their_dated_copies() {
        let listed = |id: &str| Listed { id: id.into(), name: id.into() };
        let live = [listed("claude-opus-6"), listed("claude-opus-6-20270101"), listed("claude-opus-5-5"), listed("claude-opus-5-5-20261001"), listed("claude-haiku-6-20270301")];
        assert_eq!(ids(&extras(Provider::Anthropic, &live)), ["claude-opus-6", "claude-haiku-6-20270301"]);
    }

    #[test]
    fn a_list_that_didnt_come_is_asked_for_again_sooner() {
        let now = Instant::now();
        let at = now - Duration::from_secs(3600);
        let came = Live { provider: Provider::Anthropic, models: Vec::new(), at, failed: false };
        assert!(came.fresh(now));
        assert!(!Live { failed: true, ..came }.fresh(now));
    }

    /// A server answering each request with the next of `pages`, which
    /// notes each request's line and headers.
    fn serve(pages: Vec<(u16, Value)>) -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let noted = seen.clone();
        std::thread::spawn(move || {
            for (status, page) in pages {
                let (stream, _) = listener.accept().unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut request = String::new();
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" || line.is_empty() {
                        break;
                    }
                    request.push_str(&line);
                }
                noted.lock().unwrap().push(request);
                let body = page.to_string();
                let mut stream = stream;
                write!(stream, "HTTP/1.1 {status} OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
        });
        (format!("http://{addr}"), seen)
    }

    #[test]
    fn fetch_reads_every_page_with_the_providers_own_key_header() {
        let (url, seen) = serve(vec![
            (200, json!({"data": [{"id": "claude-opus-6", "display_name": "Claude Opus 6"}], "has_more": true, "last_id": "claude-opus-6"})),
            (200, json!({"data": [{"id": "claude-haiku-6", "display_name": "Claude Haiku 6"}], "has_more": false})),
        ]);
        let models = fetch(Provider::Anthropic, "made-up-key", &url).unwrap();
        assert_eq!(ids(&models), ["claude-opus-6", "claude-haiku-6"]);
        let seen = seen.lock().unwrap();
        assert!(seen[0].starts_with("GET /v1/models?limit=1000 "), "{}", seen[0]);
        assert!(seen[1].starts_with("GET /v1/models?limit=1000&after_id=claude-opus-6 "), "{}", seen[1]);
        assert!(seen[0].to_ascii_lowercase().contains("x-api-key: made-up-key"));
        assert!(seen[0].to_ascii_lowercase().contains("anthropic-version: "));
    }

    #[test]
    fn fetch_says_why_a_list_didnt_come() {
        let (url, _) = serve(vec![(401, json!({"error": {"message": "invalid x-api-key"}}))]);
        assert_eq!(fetch(Provider::OpenAi, "made-up-key", &url), Err("HTTP 401".into()));
    }
}
