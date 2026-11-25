//! A server at a URL, over MCP's Streamable HTTP: each message POSTed,
//! answered in JSON or as a stream of server-sent events read until the
//! answer to it comes, with the session the server gives at `initialize`
//! (`Mcp-Session-Id`) and the protocol version agreed
//! (`MCP-Protocol-Version`) on every request after. A stream the server
//! drops before it answers is picked up again with a GET from its last
//! event (2025-11-25). What the server asks or says meanwhile goes to the
//! link's thread. Headers of its own go with each request, and a bearer
//! token: one the user pasted, or one got by signing in ([`oauth`]), whose
//! refusal (a 401, or a 403 for scopes it lacks) asks the user to sign in.

use std::collections::BTreeMap;
use std::io::BufReader;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::Value;

use super::sse::Events;
use super::{oauth, request_message, result_of, route_unasked, Fail, Incoming, Transport, GONE};

/// How long to wait before picking a dropped stream up again, where the
/// server says nothing, and at most whatever it says.
const RESUME_AFTER: Duration = Duration::from_secs(1);
const RESUME_LONGEST: Duration = Duration::from_secs(30);
/// How many times in a row a stream is picked up again without a new
/// event before Flow gives up on it.
const RESUME_TRIES: u32 = 5;

pub struct Http {
    agent: ureq::Agent,
    url: String,
    headers: Vec<(String, String)>,
    auth: Mutex<oauth::Auth>,
    session: Mutex<Option<String>>,
    version: Mutex<Option<String>>,
    incoming: crossbeam_channel::Sender<Incoming>,
}

/// A response, read as far as its status and headers.
type Response = ureq::http::Response<ureq::Body>;

impl Http {
    pub fn new(url: &str, headers: &BTreeMap<String, String>, auth: oauth::Auth, incoming: crossbeam_channel::Sender<Incoming>) -> Http {
        let kept: Vec<(String, String)> = headers.iter().map(|(k, v)| (k.clone(), super::servers::expand(v))).collect();
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_connect(Some(Duration::from_secs(20)))
            .timeout_global(Some(Duration::from_secs(10 * 60)))
            .user_agent(concat!("flow-terminal/", env!("CARGO_PKG_VERSION")))
            .build()
            .into();
        Http { agent, url: url.to_string(), headers: kept, auth: Mutex::new(auth), session: Mutex::new(None), version: Mutex::new(None), incoming }
    }

    /// Whether the file's own headers authorize it, so Flow sends no token.
    fn authorized_by_headers(&self) -> bool {
        self.headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("authorization"))
    }

    /// `request` with the server's headers, its token, the session and the
    /// version agreed.
    fn dress<B>(&self, mut request: ureq::RequestBuilder<B>) -> ureq::RequestBuilder<B> {
        for (key, value) in &self.headers {
            request = request.header(key.as_str(), value.as_str());
        }
        if !self.authorized_by_headers() {
            if let Some(token) = self.auth.lock().unwrap_or_else(|e| e.into_inner()).bearer() {
                request = request.header("authorization", format!("Bearer {token}"));
            }
        }
        if let Some(session) = self.session.lock().unwrap_or_else(|e| e.into_inner()).as_deref() {
            request = request.header("mcp-session-id", session);
        }
        if let Some(version) = self.version.lock().unwrap_or_else(|e| e.into_inner()).as_deref() {
            request = request.header("mcp-protocol-version", version);
        }
        request
    }

    fn post_once(&self, message: &Value) -> Result<Response, Fail> {
        let request = self.agent.post(&self.url).header("content-type", "application/json").header("accept", "application/json, text/event-stream");
        self.dress(request).send(message.to_string()).map_err(|e| Fail::Gone(format!("{GONE}: {e}")))
    }

    /// POSTs `message`; a 401 is tried again once a refresh gives a new
    /// token, and one the user can fix by signing in says so.
    fn post(&self, message: &Value) -> Result<Response, Fail> {
        let mut response = self.post_once(message)?;
        if response.status().as_u16() == 401 && self.auth.lock().unwrap_or_else(|e| e.into_inner()).refresh() {
            response = self.post_once(message)?;
        }
        self.refused(response)
    }

    /// `response`, or the sign-in it asks for: a 401, or a 403 for scopes
    /// the token lacks, where the user didn't give the token themselves.
    fn refused(&self, response: Response) -> Result<Response, Fail> {
        let status = response.status().as_u16();
        if !matches!(status, 401 | 403) {
            return Ok(response);
        }
        let challenge = response.headers().get_all("www-authenticate").iter().filter_map(|v| v.to_str().ok()).collect::<Vec<_>>().join(", ");
        let challenge = oauth::Challenge::parse(&challenge);
        let theirs = self.authorized_by_headers() || self.auth.lock().unwrap_or_else(|e| e.into_inner()).pasted();
        let sign_in = !theirs && (status == 401 || challenge.error.as_deref() == Some("insufficient_scope"));
        if sign_in {
            return Err(Fail::SignIn(challenge));
        }
        Err(status_error(status, &response.into_body().read_to_string().unwrap_or_default()))
    }

    fn keep_session(&self, response: &Response) {
        if let Some(session) = response.headers().get("mcp-session-id").and_then(|v| v.to_str().ok()) {
            *self.session.lock().unwrap_or_else(|e| e.into_inner()) = Some(session.to_string());
        }
    }

    /// Reads a stream of events for the answer to `id`, handing on what the
    /// server sends meanwhile, and picks it up again from its last event
    /// where it ends first, until `until`.
    fn read_stream(&self, mut response: Response, id: u64, until: Instant, cancel: &AtomicBool) -> Result<Value, Fail> {
        let mut last: Option<String> = None;
        let mut idle = 0;
        loop {
            let mut events = Events::new(BufReader::new(response.into_body().into_reader()));
            events.last_id = last.clone();
            while let Some(data) = events.next_data(cancel) {
                let data = data.map_err(|e| Fail::Gone(format!("{GONE}: {e}")))?;
                let Ok(message) = serde_json::from_str::<Value>(&data) else { continue };
                if route_unasked(&message, &self.incoming) {
                    continue;
                }
                if message.get("id").and_then(Value::as_u64) == Some(id) {
                    return result_of(&message);
                }
            }
            if cancel.load(Ordering::Relaxed) {
                return Err(Fail::Stopped);
            }
            let Some(from) = events.last_id.clone() else { return Err(Fail::Said("Its stream ended before it answered.".into())) };
            idle = if last.as_ref() == Some(&from) { idle + 1 } else { 0 };
            last = Some(from.clone());
            if idle >= RESUME_TRIES {
                return Err(Fail::Said("Its stream ended before it answered.".into()));
            }
            let delay = events.retry.unwrap_or(RESUME_AFTER).min(RESUME_LONGEST);
            let wake = Instant::now() + delay;
            while Instant::now() < wake {
                if cancel.load(Ordering::Relaxed) {
                    return Err(Fail::Stopped);
                }
                std::thread::sleep(Duration::from_millis(50).min(delay));
            }
            if Instant::now() >= until {
                return Err(Fail::Said("It didn't answer in time.".into()));
            }
            let request = self.agent.get(&self.url).header("accept", "text/event-stream").header("last-event-id", from.as_str());
            response = self.refused(self.dress(request).call().map_err(|e| Fail::Gone(format!("{GONE}: {e}")))?)?;
            let status = response.status().as_u16();
            if !(200..300).contains(&status) || !streamed(&response) {
                return Err(Fail::Said(format!("Its stream ended before it answered, and it couldn't be picked up again ({status}).")));
            }
        }
    }
}

fn streamed(response: &Response) -> bool {
    response.headers().get("content-type").and_then(|v| v.to_str().ok()).is_some_and(|t| t.starts_with("text/event-stream"))
}

/// Why a status other than success came back, in a word or two.
fn status_error(status: u16, body: &str) -> Fail {
    let said: String = body.trim().chars().take(300).collect();
    match status {
        401 | 403 => Fail::Said(format!("It refused Flow ({status}): its token may be missing or wrong. {said}")),
        404 => Fail::Gone(format!("{GONE}: it doesn't know the session, or the URL ({status}). {said}")),
        _ => Fail::Said(format!("It answered {status}. {said}")),
    }
}

impl Transport for Http {
    /// Waits as long as the agent does (ten minutes at most) for a JSON
    /// answer, and picks a stream up again until `wait` is out; Flow's
    /// window gives up sooner, as the user's command timeout says.
    fn request(&self, id: u64, method: &str, params: Value, wait: Duration, cancel: &AtomicBool) -> Result<Value, Fail> {
        let until = Instant::now() + wait;
        let response = self.post(&request_message(id, method, params))?;
        let status = response.status().as_u16();
        self.keep_session(&response);
        if !(200..300).contains(&status) {
            return Err(status_error(status, &response.into_body().read_to_string().unwrap_or_default()));
        }
        if streamed(&response) {
            return self.read_stream(response, id, until, cancel);
        }
        let body = response.into_body().read_to_string().map_err(|e| Fail::Gone(format!("{GONE}: {e}")))?;
        let message: Value = serde_json::from_str(&body).map_err(|e| Fail::Said(format!("Its answer isn't JSON: {e}")))?;
        // A batch of messages, or one.
        let messages = match message {
            Value::Array(all) => all,
            one => vec![one],
        };
        let mut answer = None;
        for message in messages {
            if !route_unasked(&message, &self.incoming) && message.get("id").and_then(Value::as_u64) == Some(id) {
                answer = Some(result_of(&message));
            }
        }
        answer.unwrap_or_else(|| Err(Fail::Said("Its answer didn't answer Flow's request.".into())))
    }

    fn send(&self, message: &Value) -> Result<(), Fail> {
        let response = self.post(message)?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            return Err(status_error(status, &response.into_body().read_to_string().unwrap_or_default()));
        }
        // What a server streams back to a notification or an answer is
        // read for what it asks or says.
        if streamed(&response) {
            let mut events = Events::new(BufReader::new(response.into_body().into_reader()));
            while let Some(Ok(data)) = events.next_data(&AtomicBool::new(false)) {
                if let Ok(message) = serde_json::from_str::<Value>(&data) {
                    route_unasked(&message, &self.incoming);
                }
            }
        }
        Ok(())
    }

    fn agreed(&self, version: &str) {
        *self.version.lock().unwrap_or_else(|e| e.into_inner()) = Some(version.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::super::stand_in::{serve, Reply};
    use super::*;
    use serde_json::json;

    /// An answer's JSON, for the stand-in servers below.
    fn answer_to(request: &Value, result: Value) -> String {
        json!({ "jsonrpc": "2.0", "id": request["id"], "result": result }).to_string()
    }

    fn http(url: &str, auth: oauth::Auth) -> (Http, crossbeam_channel::Receiver<Incoming>) {
        let (to, from) = crossbeam_channel::unbounded();
        (Http::new(url, &BTreeMap::from([("X-Team".into(), "flow".into())]), auth, to), from)
    }

    #[test]
    fn answers_come_in_json_or_as_events_and_the_session_goes_with_each_request() {
        let (base, seen) = serve(|_, request| {
            let asked = request.json();
            match asked["method"].as_str() {
                Some("initialize") => Reply::json(200, json!({ "jsonrpc": "2.0", "id": asked["id"], "result": { "protocolVersion": "2025-11-25" } })).with("mcp-session-id", "s-42"),
                Some("tools/list") => {
                    let note = json!({ "jsonrpc": "2.0", "method": "notifications/tools/list_changed" });
                    Reply::events(format!("event: message\ndata: {note}\n\nevent: message\ndata: {}\n\n", answer_to(&asked, json!({ "tools": [{ "name": "echo" }] }))))
                }
                Some(_) if asked.get("id").is_some() => Reply::json(200, json!({ "jsonrpc": "2.0", "id": asked["id"], "error": { "code": -1, "message": "no" } })),
                _ => Reply { status: 202, headers: vec![], body: String::new() },
            }
        });
        let (http, incoming) = http(&format!("{base}/mcp"), oauth::Auth::Token("t0k".into()));
        let no = AtomicBool::new(false);
        let init = http.request(1, "initialize", json!({}), Duration::from_secs(5), &no).unwrap();
        assert_eq!(init["protocolVersion"], "2025-11-25");
        http.agreed("2025-11-25");
        http.send(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).unwrap();
        let listed = http.request(2, "tools/list", json!({}), Duration::from_secs(5), &no).unwrap();
        assert_eq!(listed["tools"][0]["name"], "echo");
        assert!(matches!(incoming.try_recv(), Ok(Incoming::Note { method, .. }) if method == "notifications/tools/list_changed"));
        assert_eq!(http.request(3, "tools/call", json!({}), Duration::from_secs(5), &no), Err(Fail::Said("The server said: no".into())));
        let seen = seen.lock().unwrap();
        assert_eq!(seen[0].header("authorization"), Some("Bearer t0k"));
        assert_eq!((seen[0].header("x-team"), seen[0].header("accept")), (Some("flow"), Some("application/json, text/event-stream")));
        assert_eq!(seen[0].header("mcp-session-id"), None, "none before initialize gives one");
        assert_eq!((seen[2].header("mcp-session-id"), seen[2].header("mcp-protocol-version")), (Some("s-42"), Some("2025-11-25")));
    }

    #[test]
    fn a_refusal_asks_for_a_sign_in_unless_the_user_gave_the_token() {
        let (base, _) = serve(|base, _| {
            let challenge = format!(r#"Bearer resource_metadata="{base}/.well-known/oauth-protected-resource", scope="notes""#);
            Reply::json(401, json!({ "error": "unauthorized" })).with("www-authenticate", &challenge)
        });
        let no = AtomicBool::new(false);
        let (pasted, _) = http(&format!("{base}/mcp"), oauth::Auth::Token("wrong".into()));
        match pasted.request(1, "initialize", json!({}), Duration::from_secs(5), &no) {
            Err(Fail::Said(why)) => assert!(why.contains("token"), "{why}"),
            other => panic!("{other:?}"),
        }
        let (none, _) = http(&format!("{base}/mcp"), oauth::Auth::None);
        match none.request(1, "initialize", json!({}), Duration::from_secs(5), &no) {
            Err(Fail::SignIn(challenge)) => {
                assert_eq!(challenge.scope.as_deref(), Some("notes"));
                assert!(challenge.resource_metadata.unwrap().ends_with("/.well-known/oauth-protected-resource"));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_stream_dropped_before_its_answer_is_picked_up_from_its_last_event() {
        let (base, seen) = serve(|_, request| match request.method.as_str() {
            "POST" => {
                let asked = request.json();
                // A priming event, a question for the user, then the server
                // drops the stream.
                let ask = json!({ "jsonrpc": "2.0", "id": "q1", "method": "elicitation/create", "params": { "message": "Which?", "requestedSchema": { "type": "object", "properties": {} } } });
                Reply::events(format!("id: 7-0\nretry: 10\ndata:\n\nid: 7-1\ndata: {ask}\n\n")).with("x-asked", asked["id"].to_string().as_str())
            }
            _ => {
                assert_eq!(request.header("last-event-id"), Some("7-1"));
                Reply::events(format!("id: 7-2\ndata: {}\n\n", answer_to(&json!({ "id": 9 }), json!({ "content": [] }))))
            }
        });
        let (http, incoming) = http(&format!("{base}/mcp"), oauth::Auth::None);
        let answered = http.request(9, "tools/call", json!({}), Duration::from_secs(5), &AtomicBool::new(false)).unwrap();
        assert_eq!(answered, json!({ "content": [] }));
        assert!(matches!(incoming.try_recv(), Ok(Incoming::Request { id, method, .. }) if id == "q1" && method == "elicitation/create"));
        let seen = seen.lock().unwrap();
        assert_eq!((seen.len(), seen[1].method.as_str(), seen[1].header("accept")), (2, "GET", Some("text/event-stream")));
    }
}
