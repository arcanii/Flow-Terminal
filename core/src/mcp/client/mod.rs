//! Flow as an MCP client: the assistant using the tools and resources of MCP
//! servers the user connected ([`servers`]), the other way round from
//! `flow-mcp`. Each server switched on in Settings has a thread of its own
//! ([`Link`]), speaking MCP over its program's standard input and output
//! ([`stdio`]) or over Streamable HTTP ([`http`], signed in to with
//! [`oauth`] where it asks). The thread connects (`initialize`), lists what
//! the server offers (again when it says that changed), hands each call,
//! resource read and prompt to a thread of its own, so one given up never
//! holds up the next, and answers what the server asks: a `ping`, and the
//! user's answer to a question it asks ([`elicit`]). Everything comes back
//! to Flow's window as a [`ServerEvent`].

pub mod elicit;
pub mod names;
pub mod oauth;
pub mod servers;

mod http;
mod sse;
mod stdio;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};

use servers::{How, Server};

/// The protocol versions Flow asks for, newest first.
const VERSIONS: [&str; 4] = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];
/// How long a server is given to start and answer `initialize`.
const START_WAIT: Duration = Duration::from_secs(30);
/// How long it's given to list what it offers, a page at a time.
const LIST_WAIT: Duration = Duration::from_secs(30);
/// At most so many of a server's resources, templates or prompts are kept.
const MOST_LISTED: usize = 1000;
/// How an error starts where the server went away, not just the request.
const GONE: &str = "The server went away";

/// A tool a server offers.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub schema: Value,
    /// It says it changes nothing (`annotations.readOnlyHint`).
    pub read_only: bool,
}

/// A resource a server offers to be read.
#[derive(Debug, Clone, PartialEq)]
pub struct ResourceInfo {
    pub uri: String,
    pub name: String,
    pub description: String,
    pub mime: Option<String>,
}

/// A resource template: resources whose URIs fill in its variables.
#[derive(Debug, Clone, PartialEq)]
pub struct TemplateInfo {
    pub uri_template: String,
    pub name: String,
    pub description: String,
    pub mime: Option<String>,
}

/// A prompt a server offers: a ready-made request the user starts.
#[derive(Debug, Clone, PartialEq)]
pub struct PromptInfo {
    pub name: String,
    pub description: String,
    pub arguments: Vec<PromptArg>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PromptArg {
    pub name: String,
    pub description: String,
    pub required: bool,
}

/// Everything a server offers, as last listed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Listing {
    pub tools: Vec<ToolInfo>,
    pub resources: Vec<ResourceInfo>,
    pub templates: Vec<TemplateInfo>,
    pub prompts: Vec<PromptInfo>,
}

/// Why a request to a server came to nothing.
#[derive(Debug, Clone, PartialEq)]
pub enum Fail {
    /// It failed, and why: the server's error, or no answer in time.
    Said(String),
    /// The server went away.
    Gone(String),
    /// It needs the user to sign in first (or again, for more).
    SignIn(oauth::Challenge),
    /// It needs the user to open these links first (MCP's -32042).
    Links(Vec<elicit::LinkAsk>),
    /// Flow gave it up.
    Stopped,
}

impl Fail {
    /// What the model is told.
    pub fn text(&self) -> String {
        match self {
            Fail::Said(why) | Fail::Gone(why) => why.clone(),
            Fail::SignIn(_) => "The MCP server needs the user to sign in to it first, in Flow's Settings (MCP servers).".into(),
            Fail::Links(_) => "The MCP server needs the user to open a link first.".into(),
            Fail::Stopped => "The call was stopped.".into(),
        }
    }
}

/// What a server's thread tells Flow's window.
#[derive(Debug, Clone, PartialEq)]
pub enum ServerEvent {
    /// Connected, and what it offers listed (again where that changed);
    /// `signed_in` where it's with a sign-in of Flow's.
    Connected { server: String, listing: Listing, signed_in: bool },
    /// It couldn't be reached, or it went away, and why.
    Failed { server: String, why: String },
    /// It needs the user to sign in, as its challenge says.
    NeedsSignIn { server: String, challenge: oauth::Challenge },
    /// A call's or a resource read's answer, for job `job`.
    Answer { server: String, job: String, said: Result<String, Fail> },
    /// A prompt's text, got for request `job`.
    Prompted { server: String, job: String, said: Result<String, String> },
    /// The server asks the user something; its answer goes back with
    /// [`Link::answer`] under `id`.
    Asks { server: String, id: Value, ask: elicit::Ask },
    /// The server gave up asking `id` (`notifications/cancelled`).
    AskWithdrawn { server: String, id: Value },
    /// A link the user was asked to open is done with, the server says.
    LinkDone { server: String, elicitation: String },
    /// Signing in: the address to open in the browser.
    OpenToSignIn { server: String, url: String },
    /// Signed in, with the access token got.
    SignedIn { server: String, access: oauth::Access },
    /// Signing in failed, and why, in the user's words.
    SignInFailed { server: String, why: String },
}

/// What the server sends without being asked, as a transport hands it on.
enum Incoming {
    Note { method: String, params: Value },
    Request { id: Value, method: String, params: Value },
    /// The transport lost the server, and why.
    Gone(String),
}

/// The conversation with a server, over either transport, which several
/// threads may use at once.
trait Transport: Send + Sync {
    /// Sends request `method` as `id` and waits up to `wait` for its answer
    /// (less once `cancel` is set): its result, or why there's none.
    fn request(&self, id: u64, method: &str, params: Value, wait: Duration, cancel: &AtomicBool) -> Result<Value, Fail>;
    /// Sends `message`: a notification, or an answer to the server.
    fn send(&self, message: &Value) -> Result<(), Fail>;
    /// Says the protocol version agreed on, for the headers that need it.
    fn agreed(&self, _version: &str) {}
}

/// How a server's thread finds its credentials, where it has any: read on
/// that thread, so a credential store that asks the user first never holds
/// up Flow's window.
pub type Credentials = Box<dyn FnOnce() -> oauth::Auth + Send>;

/// What Flow's window hands a server's thread.
enum Ask {
    Call { job: String, tool: String, args: Value, wait: Duration },
    Read { job: String, uri: String, wait: Duration },
    Prompt { job: String, name: String, args: serde_json::Map<String, Value>, wait: Duration },
    Cancel { job: String, reason: String },
    /// An answer to a request of the server's.
    Reply(Value),
}

/// A server's thread, which stops when this is dropped.
pub struct Link {
    pub server: String,
    to: crossbeam_channel::Sender<Ask>,
}

impl Link {
    /// Starts talking to `server` on a thread of its own: `credentials`,
    /// how a remote one's are found; `path`, folders put first on a
    /// program's PATH; `tell`, how its events reach Flow's window.
    pub fn start(server: Server, credentials: Credentials, path: Vec<std::path::PathBuf>, tell: Arc<dyn Fn(ServerEvent) + Send + Sync>) -> Link {
        let (to, from) = crossbeam_channel::unbounded();
        let name = server.name.clone();
        let _ = std::thread::Builder::new().name("flow-mcp-client".into()).spawn(move || run(server, credentials, path, from, tell));
        Link { server: name, to }
    }

    /// Calls `tool` with `args`, its answer coming back as
    /// [`ServerEvent::Answer`] for `job`, an error past `wait`. Flow's
    /// window gives a call far less, by its own clock, and gives it up
    /// with [`Link::cancel`], so a server's question can hold it.
    pub fn call(&self, job: &str, tool: &str, args: Value, wait: Duration) {
        let _ = self.to.send(Ask::Call { job: job.into(), tool: tool.into(), args, wait });
    }

    /// Reads resource `uri`, its text coming back as [`ServerEvent::Answer`].
    pub fn read(&self, job: &str, uri: &str, wait: Duration) {
        let _ = self.to.send(Ask::Read { job: job.into(), uri: uri.into(), wait });
    }

    /// Gets prompt `name` filled in with `args`, as [`ServerEvent::Prompted`].
    pub fn prompt(&self, job: &str, name: &str, args: serde_json::Map<String, Value>, wait: Duration) {
        let _ = self.to.send(Ask::Prompt { job: job.into(), name: name.into(), args, wait });
    }

    /// Gives up job `job`, telling the server (`notifications/cancelled`).
    pub fn cancel(&self, job: &str, reason: &str) {
        let _ = self.to.send(Ask::Cancel { job: job.into(), reason: reason.into() });
    }

    /// Answers the server's request `id` with `result`.
    pub fn answer(&self, id: &Value, result: Value) {
        let _ = self.to.send(Ask::Reply(json!({ "jsonrpc": "2.0", "id": id, "result": result })));
    }

    /// Refuses the server's request `id`.
    pub fn refuse(&self, id: &Value, code: i64, message: &str) {
        let _ = self.to.send(Ask::Reply(refusal(id, code, message)));
    }
}

fn refusal(id: &Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// A server's thread's side shared with the threads its requests run on.
struct Live {
    name: String,
    transport: Box<dyn Transport>,
    next: AtomicU64,
    /// Each request underway by its job: its id and its flag for giving up.
    underway: Mutex<HashMap<String, (u64, Arc<AtomicBool>)>>,
    tell: Arc<dyn Fn(ServerEvent) + Send + Sync>,
    /// Where a request's thread says the server went away.
    lost: crossbeam_channel::Sender<Incoming>,
}

impl Live {
    fn request(&self, method: &str, params: Value, wait: Duration) -> Result<Value, Fail> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        self.transport.request(id, method, params, wait, &AtomicBool::new(false))
    }

    /// Request `method` for job `job`, which [`Link::cancel`] may give up.
    fn request_for(&self, job: &str, method: &str, params: Value, wait: Duration) -> Result<Value, Fail> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let cancel = Arc::new(AtomicBool::new(false));
        self.underway.lock().unwrap_or_else(|e| e.into_inner()).insert(job.to_string(), (id, cancel.clone()));
        let said = self.transport.request(id, method, params, wait, &cancel);
        self.underway.lock().unwrap_or_else(|e| e.into_inner()).remove(job);
        // An answer that comes after all to a request given up is let go.
        let said = if cancel.load(Ordering::Relaxed) { Err(Fail::Stopped) } else { said };
        match &said {
            Err(Fail::Gone(why)) => {
                let _ = self.lost.send(Incoming::Gone(why.clone()));
            }
            Err(Fail::SignIn(challenge)) => (self.tell)(ServerEvent::NeedsSignIn { server: self.name.clone(), challenge: challenge.clone() }),
            _ => {}
        }
        said
    }
}

/// A server's thread: connected, what it offers listed, then each of
/// Flow's asks and the server's in turn, until Flow lets it go (the
/// channel closed) or it goes away.
fn run(server: Server, credentials: Credentials, path: Vec<std::path::PathBuf>, from: crossbeam_channel::Receiver<Ask>, tell: Arc<dyn Fn(ServerEvent) + Send + Sync>) {
    let name = server.name.clone();
    let failed = |why: String| tell(ServerEvent::Failed { server: name.clone(), why });
    let (incoming_to, incoming) = crossbeam_channel::unbounded();
    let mut signed_in = false;
    let transport: Box<dyn Transport> = match &server.how {
        How::Stdio { command, args, env } => match stdio::Stdio::start(command, args, env, &path, incoming_to.clone()) {
            Ok(t) => Box::new(t),
            Err(why) => return failed(why),
        },
        How::Http { url, headers } => {
            let auth = credentials();
            signed_in = matches!(auth, oauth::Auth::Session(_));
            Box::new(http::Http::new(url, headers, auth, incoming_to.clone()))
        }
    };
    let live = Arc::new(Live { name: name.clone(), transport, next: AtomicU64::new(1), underway: Mutex::new(HashMap::new()), tell: tell.clone(), lost: incoming_to });
    let offers = match initialize(&live) {
        Ok(offers) => offers,
        Err(Fail::SignIn(challenge)) => return tell(ServerEvent::NeedsSignIn { server: name.clone(), challenge }),
        Err(fail) => return failed(fail.text()),
    };
    let mut listing = Listing::default();
    for kind in Kind::ALL {
        if !offers.has(kind) {
            continue;
        }
        match list(&live, kind, &mut listing) {
            Ok(()) => {}
            Err(Fail::SignIn(challenge)) => return tell(ServerEvent::NeedsSignIn { server: name.clone(), challenge }),
            // What it offers besides its tools is a bonus: a failure there
            // is noted and passed over.
            Err(fail) if kind != Kind::Tools => log::warn!("MCP server {name}: listing its {kind:?}: {}", fail.text()),
            Err(fail) => return failed(fail.text()),
        }
    }
    tell(ServerEvent::Connected { server: name.clone(), listing: listing.clone(), signed_in });
    loop {
        crossbeam_channel::select! {
            recv(from) -> ask => match ask {
                Ok(ask) => flow_asks(&live, ask),
                Err(_) => return,
            },
            recv(incoming) -> message => match message {
                Ok(Incoming::Gone(why)) => return failed(why),
                Ok(Incoming::Request { id, method, params }) => server_asks(&live, id, &method, &params),
                Ok(Incoming::Note { method, params }) => {
                    let kinds: &[Kind] = match method.as_str() {
                        "notifications/tools/list_changed" => &[Kind::Tools],
                        "notifications/resources/list_changed" => &[Kind::Resources, Kind::Templates],
                        "notifications/prompts/list_changed" => &[Kind::Prompts],
                        "notifications/elicitation/complete" => {
                            if let Some(id) = params.get("elicitationId").and_then(Value::as_str) {
                                tell(ServerEvent::LinkDone { server: name.clone(), elicitation: id.to_string() });
                            }
                            &[]
                        }
                        "notifications/cancelled" => {
                            if let Some(id) = params.get("requestId") {
                                tell(ServerEvent::AskWithdrawn { server: name.clone(), id: id.clone() });
                            }
                            &[]
                        }
                        _ => &[],
                    };
                    let mut changed = false;
                    for kind in kinds.iter().copied().filter(|k| offers.has(*k)) {
                        match list(&live, kind, &mut listing) {
                            Ok(()) => changed = true,
                            Err(Fail::Gone(why)) => return failed(why),
                            Err(fail) => log::warn!("MCP server {name}: listing its {kind:?} again: {}", fail.text()),
                        }
                    }
                    if changed {
                        tell(ServerEvent::Connected { server: name.clone(), listing: listing.clone(), signed_in });
                    }
                }
                Err(_) => return,
            },
        }
    }
}

/// One of Flow's asks: a request handed to a thread of its own, a request
/// given up, or an answer to the server.
fn flow_asks(live: &Arc<Live>, ask: Ask) {
    let spawn = |work: Box<dyn FnOnce(&Live) + Send>| {
        let live = live.clone();
        let _ = std::thread::Builder::new().name("flow-mcp-call".into()).spawn(move || work(&live));
    };
    match ask {
        Ask::Call { job, tool, args, wait } => spawn(
            Box::new(move |live| {
                let said = live.request_for(&job, "tools/call", json!({ "name": tool, "arguments": args }), wait).and_then(|result| answer(&result).map_err(Fail::Said));
                (live.tell)(ServerEvent::Answer { server: live.name.clone(), job, said });
            }),
        ),
        Ask::Read { job, uri, wait } => spawn(
            Box::new(move |live| {
                let said = live.request_for(&job, "resources/read", json!({ "uri": uri }), wait).map(|result| resource_text(&result));
                (live.tell)(ServerEvent::Answer { server: live.name.clone(), job, said });
            }),
        ),
        Ask::Prompt { job, name, args, wait } => spawn(
            Box::new(move |live| {
                let said = live.request_for(&job, "prompts/get", json!({ "name": name, "arguments": args }), wait).map(|result| prompt_text(&result)).map_err(|f| f.text());
                (live.tell)(ServerEvent::Prompted { server: live.name.clone(), job, said });
            }),
        ),
        Ask::Cancel { job, reason } => {
            let underway = live.underway.lock().unwrap_or_else(|e| e.into_inner()).get(&job).cloned();
            if let Some((id, cancel)) = underway {
                cancel.store(true, Ordering::Relaxed);
                let _ = live.transport.send(&json!({ "jsonrpc": "2.0", "method": "notifications/cancelled", "params": { "requestId": id, "reason": reason } }));
            }
        }
        Ask::Reply(message) => {
            let _ = live.transport.send(&message);
        }
    }
}

/// A request of the server's: a ping answered, a question for the user
/// handed to Flow's window, anything else refused.
fn server_asks(live: &Live, id: Value, method: &str, params: &Value) {
    match method {
        "ping" => {
            let _ = live.transport.send(&json!({ "jsonrpc": "2.0", "id": id, "result": {} }));
        }
        "elicitation/create" => match elicit::Ask::read(params) {
            Ok(ask) => (live.tell)(ServerEvent::Asks { server: live.name.clone(), id, ask }),
            Err(why) => {
                let _ = live.transport.send(&refusal(&id, -32602, &why));
            }
        },
        _ => {
            let _ = live.transport.send(&refusal(&id, -32601, "Flow doesn't do that"));
        }
    }
}

/// What a server offers besides being called, as `initialize` says.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Kind {
    Tools,
    Resources,
    Templates,
    Prompts,
}

impl Kind {
    const ALL: [Kind; 4] = [Kind::Tools, Kind::Resources, Kind::Templates, Kind::Prompts];
}

/// The capabilities a server said it has at `initialize`.
struct Offers(Value);

impl Offers {
    fn has(&self, kind: Kind) -> bool {
        match kind {
            // Listed whatever it says, as Flow always has: a server that
            // says nothing of its tools may still have some.
            Kind::Tools => true,
            Kind::Resources | Kind::Templates => self.0.get("resources").is_some(),
            Kind::Prompts => self.0.get("prompts").is_some(),
        }
    }
}

/// `initialize`, then `notifications/initialized`: what the server offers.
fn initialize(live: &Live) -> Result<Offers, Fail> {
    let params = json!({
        "protocolVersion": VERSIONS[0],
        "capabilities": { "elicitation": { "form": {}, "url": {} } },
        "clientInfo": { "name": "flow", "title": "Flow", "version": crate::version::VERSION, "description": "A terminal with an assistant built in" }
    });
    let result = live.request("initialize", params, START_WAIT)?;
    let version = result.get("protocolVersion").and_then(Value::as_str).unwrap_or(VERSIONS[0]).to_string();
    if !VERSIONS.contains(&version.as_str()) {
        return Err(Fail::Said(format!("it speaks MCP {version}, which Flow doesn't")));
    }
    live.transport.agreed(&version);
    live.transport.send(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized", "params": {} }))?;
    Ok(Offers(result.get("capabilities").cloned().unwrap_or_else(|| json!({}))))
}

/// Lists everything of `kind` the server has, a page at a time, into
/// `listing`.
fn list(live: &Live, kind: Kind, listing: &mut Listing) -> Result<(), Fail> {
    let (method, field) = match kind {
        Kind::Tools => ("tools/list", "tools"),
        Kind::Resources => ("resources/list", "resources"),
        Kind::Templates => ("resources/templates/list", "resourceTemplates"),
        Kind::Prompts => ("prompts/list", "prompts"),
    };
    let mut items = Vec::new();
    let mut cursor: Option<String> = None;
    for _ in 0..50 {
        let params = cursor.as_ref().map_or_else(|| json!({}), |c| json!({ "cursor": c }));
        let page = live.request(method, params, LIST_WAIT)?;
        items.extend(page.get(field).and_then(Value::as_array).into_iter().flatten().cloned());
        cursor = page.get("nextCursor").and_then(Value::as_str).map(String::from);
        if cursor.is_none() || items.len() >= MOST_LISTED {
            break;
        }
    }
    let text = |v: &Value, key: &str| v.get(key).and_then(Value::as_str).unwrap_or_default().to_string();
    let mime = |v: &Value| v.get("mimeType").and_then(Value::as_str).map(String::from);
    let named = items.iter().filter(|v| v.get("name").and_then(Value::as_str).is_some());
    match kind {
        Kind::Tools => {
            listing.tools = named
                .map(|tool| ToolInfo {
                    name: text(tool, "name"),
                    description: text(tool, "description"),
                    schema: tool.get("inputSchema").cloned().unwrap_or_else(|| json!({ "type": "object", "properties": {} })),
                    read_only: tool.pointer("/annotations/readOnlyHint").and_then(Value::as_bool).unwrap_or(false),
                })
                .collect()
        }
        Kind::Resources => {
            listing.resources = named
                .filter(|r| r.get("uri").and_then(Value::as_str).is_some())
                .take(MOST_LISTED)
                .map(|r| ResourceInfo { uri: text(r, "uri"), name: text(r, "name"), description: text(r, "description"), mime: mime(r) })
                .collect()
        }
        Kind::Templates => {
            listing.templates = named
                .filter(|r| r.get("uriTemplate").and_then(Value::as_str).is_some())
                .take(MOST_LISTED)
                .map(|r| TemplateInfo { uri_template: text(r, "uriTemplate"), name: text(r, "name"), description: text(r, "description"), mime: mime(r) })
                .collect()
        }
        Kind::Prompts => {
            listing.prompts = named
                .take(MOST_LISTED)
                .map(|p| PromptInfo {
                    name: text(p, "name"),
                    description: text(p, "description"),
                    arguments: p
                        .get("arguments")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter(|a| a.get("name").and_then(Value::as_str).is_some())
                        .map(|a| PromptArg { name: text(a, "name"), description: text(a, "description"), required: a.get("required").and_then(Value::as_bool).unwrap_or(false) })
                        .collect(),
                })
                .collect()
        }
    }
    Ok(())
}

/// One part of a call's content, a resource's or a prompt's message, as
/// the model reads it.
fn part_text(part: &Value) -> String {
    match part.get("type").and_then(Value::as_str).unwrap_or_default() {
        "text" => part.get("text").and_then(Value::as_str).unwrap_or_default().to_string(),
        "image" | "audio" => format!("[{} ({}), which Flow doesn't pass on]", part["type"].as_str().unwrap_or("media"), part.get("mimeType").and_then(Value::as_str).unwrap_or("?")),
        "resource" => match part.pointer("/resource/text").and_then(Value::as_str) {
            Some(text) => text.to_string(),
            None => format!("[a resource: {}]", part.pointer("/resource/uri").and_then(Value::as_str).unwrap_or("?")),
        },
        "resource_link" => format!("[a link: {}]", part.get("uri").and_then(Value::as_str).unwrap_or("?")),
        other => format!("[a part of kind {other}]"),
    }
}

/// A `tools/call` result as the model reads it: its text parts, an image or
/// a resource said in a line (a resource's text where it has one), and its
/// error where it is one.
fn answer(result: &Value) -> Result<String, String> {
    let mut said: Vec<String> = result.get("content").and_then(Value::as_array).into_iter().flatten().map(part_text).collect();
    if said.is_empty() {
        if let Some(structured) = result.get("structuredContent") {
            said.push(structured.to_string());
        }
    }
    let text = said.join("\n");
    if result.get("isError").and_then(Value::as_bool).unwrap_or(false) {
        Err(if text.is_empty() { "The tool failed, and said nothing.".into() } else { text })
    } else {
        Ok(text)
    }
}

/// A `resources/read` result as the model reads it: each part's text, a
/// binary one said in a line.
fn resource_text(result: &Value) -> String {
    let parts: Vec<String> = result
        .get("contents")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|part| match part.get("text").and_then(Value::as_str) {
            Some(text) => text.to_string(),
            None => {
                let size = part.get("blob").and_then(Value::as_str).map_or(0, |b| b.len() * 3 / 4);
                format!("[{} ({}, about {} KB), which Flow doesn't pass on]", part.get("uri").and_then(Value::as_str).unwrap_or("?"), part.get("mimeType").and_then(Value::as_str).unwrap_or("binary"), size.div_ceil(1024))
            }
        })
        .collect();
    if parts.is_empty() {
        "The resource is empty.".into()
    } else {
        parts.join("\n")
    }
}

/// A `prompts/get` result as the user's message: its messages' text, any
/// of the assistant's labelled as the prompt's example of a reply.
fn prompt_text(result: &Value) -> String {
    let messages: Vec<(bool, String)> = result
        .get("messages")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|m| (m.get("role").and_then(Value::as_str) == Some("assistant"), m.get("content").map(part_text).unwrap_or_default()))
        .collect();
    if messages.iter().all(|(theirs, _)| !theirs) {
        return messages.into_iter().map(|(_, text)| text).collect::<Vec<_>>().join("\n\n");
    }
    messages.into_iter().map(|(theirs, text)| if theirs { format!("(The prompt's example of a reply:)\n{text}") } else { text }).collect::<Vec<_>>().join("\n\n")
}

/// A JSON-RPC request's message.
fn request_message(id: u64, method: &str, params: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
}

/// The result in JSON-RPC answer `message`, or its error.
fn result_of(message: &Value) -> Result<Value, Fail> {
    if let Some(error) = message.get("error") {
        if error.get("code").and_then(Value::as_i64) == Some(elicit::LINKS_NEEDED) {
            let links: Vec<elicit::LinkAsk> = error.pointer("/data/elicitations").and_then(Value::as_array).into_iter().flatten().filter_map(|e| elicit::LinkAsk::read(e).ok()).collect();
            if !links.is_empty() {
                return Err(Fail::Links(links));
            }
        }
        let text = error.get("message").and_then(Value::as_str).unwrap_or("an error");
        return Err(Fail::Said(format!("The server said: {text}")));
    }
    Ok(message.get("result").cloned().unwrap_or(Value::Null))
}

/// Hands a message the server sent unasked to the link's thread: true
/// where it was one (a request of its own or a notification).
fn route_unasked(message: &Value, incoming: &crossbeam_channel::Sender<Incoming>) -> bool {
    let Some(method) = message.get("method").and_then(Value::as_str) else { return false };
    let params = message.get("params").cloned().unwrap_or(Value::Null);
    let routed = match message.get("id") {
        Some(id) => Incoming::Request { id: id.clone(), method: method.to_string(), params },
        None => Incoming::Note { method: method.to_string(), params },
    };
    let _ = incoming.send(routed);
    true
}

/// A stand-in HTTP server for the tests: each request handed to `handle`
/// with the server's own address, its reply sent and the connection
/// closed, every request kept.
#[cfg(test)]
pub(crate) mod stand_in {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    #[derive(Debug, Clone)]
    pub struct Request {
        pub method: String,
        pub target: String,
        pub headers: Vec<(String, String)>,
        pub body: String,
    }

    impl Request {
        pub fn header(&self, name: &str) -> Option<&str> {
            self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
        }

        /// Its body's JSON, or null.
        pub fn json(&self) -> serde_json::Value {
            serde_json::from_str(&self.body).unwrap_or(serde_json::Value::Null)
        }

        /// Its body's form fields, or its target's query.
        pub fn form(&self) -> std::collections::BTreeMap<String, String> {
            let text = if self.method == "GET" { self.target.split_once('?').map(|(_, q)| q).unwrap_or_default() } else { self.body.as_str() };
            super::oauth::query_params(text)
        }

        pub fn path(&self) -> &str {
            self.target.split('?').next().unwrap_or_default()
        }
    }

    pub struct Reply {
        pub status: u16,
        pub headers: Vec<(String, String)>,
        pub body: String,
    }

    impl Reply {
        pub fn json(status: u16, value: serde_json::Value) -> Reply {
            Reply { status, headers: vec![("content-type".into(), "application/json".into())], body: value.to_string() }
        }

        pub fn events(body: String) -> Reply {
            Reply { status: 200, headers: vec![("content-type".into(), "text/event-stream".into())], body }
        }

        pub fn with(mut self, name: &str, value: &str) -> Reply {
            self.headers.push((name.into(), value.into()));
            self
        }
    }

    pub type Seen = Arc<Mutex<Vec<Request>>>;

    pub fn serve(handle: impl Fn(&str, &Request) -> Reply + Send + Sync + 'static) -> (String, Seen) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let seen: Seen = Arc::new(Mutex::new(Vec::new()));
        let (kept, handle, at) = (seen.clone(), Arc::new(handle), base.clone());
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let (kept, handle, at) = (kept.clone(), handle.clone(), at.clone());
                std::thread::spawn(move || {
                    let mut reader = BufReader::new(stream.try_clone().unwrap());
                    let mut first = String::new();
                    if reader.read_line(&mut first).unwrap_or(0) == 0 {
                        return;
                    }
                    let mut words = first.split_whitespace();
                    let (method, target) = (words.next().unwrap_or_default().to_string(), words.next().unwrap_or_default().to_string());
                    let (mut headers, mut length) = (Vec::new(), 0);
                    loop {
                        let mut line = String::new();
                        if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                            break;
                        }
                        if let Some((k, v)) = line.trim_end().split_once(':') {
                            if k.eq_ignore_ascii_case("content-length") {
                                length = v.trim().parse().unwrap_or(0);
                            }
                            headers.push((k.trim().to_string(), v.trim().to_string()));
                        }
                    }
                    let mut body = vec![0; length];
                    let _ = reader.read_exact(&mut body);
                    let request = Request { method, target, headers, body: String::from_utf8_lossy(&body).into_owned() };
                    kept.lock().unwrap().push(request.clone());
                    let reply = handle(&at, &request);
                    let mut out = stream;
                    let mut head = format!("HTTP/1.1 {} X\r\ncontent-length: {}\r\nconnection: close\r\n", reply.status, reply.body.len());
                    for (k, v) in &reply.headers {
                        head.push_str(&format!("{k}: {v}\r\n"));
                    }
                    let _ = write!(out, "{head}\r\n{}", reply.body);
                });
            }
        });
        (base, seen)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_calls_content_is_text_for_the_model() {
        let result = json!({ "content": [
            { "type": "text", "text": "Two files." },
            { "type": "image", "data": "…", "mimeType": "image/png" },
            { "type": "resource", "resource": { "uri": "file:///a", "text": "a's text" } },
            { "type": "resource_link", "uri": "file:///b", "name": "b" }
        ] });
        assert_eq!(answer(&result).unwrap(), "Two files.\n[image (image/png), which Flow doesn't pass on]\na's text\n[a link: file:///b]");
        assert_eq!(answer(&json!({ "content": [{ "type": "text", "text": "No such file." }], "isError": true })), Err("No such file.".into()));
        assert_eq!(answer(&json!({ "content": [], "structuredContent": { "n": 2 } })).unwrap(), r#"{"n":2}"#);
        assert_eq!(result_of(&json!({ "jsonrpc": "2.0", "id": 1, "error": { "code": -32601, "message": "nope" } })), Err(Fail::Said("The server said: nope".into())));
    }

    #[test]
    fn resources_and_prompts_read_as_text() {
        let read = json!({ "contents": [
            { "uri": "notes://1", "mimeType": "text/plain", "text": "Kyoto trip" },
            { "uri": "notes://map.png", "mimeType": "image/png", "blob": "A".repeat(4096) }
        ] });
        assert_eq!(resource_text(&read), "Kyoto trip\n[notes://map.png (image/png, about 3 KB), which Flow doesn't pass on]");
        assert_eq!(resource_text(&json!({ "contents": [] })), "The resource is empty.");
        let one = json!({ "messages": [{ "role": "user", "content": { "type": "text", "text": "Review PR 12." } }] });
        assert_eq!(prompt_text(&one), "Review PR 12.");
        let two = json!({ "messages": [
            { "role": "user", "content": { "type": "text", "text": "Sum it up." } },
            { "role": "assistant", "content": { "type": "text", "text": "In short:" } }
        ] });
        assert_eq!(prompt_text(&two), "Sum it up.\n\n(The prompt's example of a reply:)\nIn short:");
    }

    #[test]
    fn a_links_needed_error_carries_its_links() {
        let error = json!({ "jsonrpc": "2.0", "id": 2, "error": { "code": -32042, "message": "more", "data": { "elicitations": [
            { "mode": "url", "elicitationId": "e1", "url": "https://mcp.example.com/connect?e=e1", "message": "Connect your files." }
        ] } } });
        match result_of(&error) {
            Err(Fail::Links(links)) => assert_eq!((links.len(), links[0].id.as_str(), links[0].url.as_str()), (1, "e1", "https://mcp.example.com/connect?e=e1")),
            other => panic!("{other:?}"),
        }
    }

    /// A server over HTTP, end to end: what it offers listed; a slow call
    /// never holding up the next, and given up with the server told; a
    /// question asked inside a call's stream, the stream dropped and picked
    /// up again once the user's answer is in; a resource read and a prompt
    /// got.
    #[test]
    fn a_link_lists_answers_asks_and_gives_up_as_its_server_and_flow_say() {
        use stand_in::{serve, Reply};
        use std::sync::atomic::AtomicU64;
        let released = Arc::new(AtomicBool::new(false));
        let answered = Arc::new(AtomicBool::new(false));
        let slow_id = Arc::new(AtomicU64::new(0));
        let cancelled = Arc::new(Mutex::new(None::<Value>));
        let ask_id = Arc::new(AtomicU64::new(0));
        let (r, a, sid, c, aid) = (released.clone(), answered.clone(), slow_id.clone(), cancelled.clone(), ask_id.clone());
        let (base, _) = serve(move |_, request| {
            let asked = request.json();
            let reply = |result: Value| Reply::json(200, json!({ "jsonrpc": "2.0", "id": asked["id"], "result": result }));
            if request.method == "GET" {
                // The dropped stream, picked up: the call's answer once the
                // user's answer came.
                for _ in 0..200 {
                    if a.load(Ordering::Relaxed) {
                        let answer = json!({ "jsonrpc": "2.0", "id": aid.load(Ordering::Relaxed), "result": { "content": [{ "type": "text", "text": "noted in Japan 2026" }] } });
                        return Reply::events(format!("id: s-2\ndata: {answer}\n\n"));
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
                return Reply::json(500, json!({}));
            }
            match (asked["method"].as_str(), asked["params"]["name"].as_str()) {
                (Some("initialize"), _) => reply(json!({ "protocolVersion": "2025-11-25", "capabilities": { "tools": {}, "resources": {}, "prompts": {} } })),
                (Some("tools/list"), _) => reply(json!({ "tools": [{ "name": "slow" }, { "name": "fast" }, { "name": "add_note" }] })),
                (Some("resources/list"), _) => reply(json!({ "resources": [{ "uri": "notes://1", "name": "Kyoto trip", "mimeType": "text/markdown" }] })),
                (Some("resources/templates/list"), _) => reply(json!({ "resourceTemplates": [{ "uriTemplate": "notes://{id}", "name": "A note" }] })),
                (Some("prompts/list"), _) => reply(json!({ "prompts": [{ "name": "summarize", "arguments": [{ "name": "note", "required": true }] }] })),
                (Some("tools/call"), Some("slow")) => {
                    sid.store(asked["id"].as_u64().unwrap(), Ordering::Relaxed);
                    while !r.load(Ordering::Relaxed) {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    reply(json!({ "content": [{ "type": "text", "text": "too late" }] }))
                }
                (Some("tools/call"), Some("fast")) => reply(json!({ "content": [{ "type": "text", "text": "quick" }] })),
                (Some("tools/call"), Some("add_note")) => {
                    aid.store(asked["id"].as_u64().unwrap(), Ordering::Relaxed);
                    let ask = json!({ "jsonrpc": "2.0", "id": "q1", "method": "elicitation/create", "params": { "message": "Which notebook?", "requestedSchema": { "type": "object", "properties": { "notebook": { "type": "string", "enum": ["trips", "japan"] } }, "required": ["notebook"] } } });
                    Reply::events(format!("id: s-0\nretry: 10\ndata:\n\nid: s-1\ndata: {ask}\n\n"))
                }
                (Some("resources/read"), _) => reply(json!({ "contents": [{ "uri": asked["params"]["uri"], "text": "Nozomi 7, 07:00" }] })),
                (Some("prompts/get"), _) => reply(json!({ "messages": [{ "role": "user", "content": { "type": "text", "text": format!("Sum up {}.", asked["params"]["arguments"]["note"].as_str().unwrap_or("?")) } }] })),
                (None, _) if asked["id"] == "q1" => {
                    assert_eq!(asked["result"], json!({ "action": "accept", "content": { "notebook": "japan" } }));
                    a.store(true, Ordering::Relaxed);
                    Reply { status: 202, headers: vec![], body: String::new() }
                }
                (Some("notifications/cancelled"), _) => {
                    *c.lock().unwrap() = Some(asked["params"].clone());
                    r.store(true, Ordering::Relaxed);
                    Reply { status: 202, headers: vec![], body: String::new() }
                }
                _ => Reply { status: 202, headers: vec![], body: String::new() },
            }
        });
        let (told, events) = crossbeam_channel::unbounded();
        let tell: Arc<dyn Fn(ServerEvent) + Send + Sync> = Arc::new(move |event| {
            let _ = told.send(event);
        });
        let server = Server { name: "notes".into(), how: How::Http { url: format!("{base}/mcp"), headers: Default::default() }, oauth: Default::default() };
        let link = Link::start(server, Box::new(|| oauth::Auth::None), Vec::new(), tell);
        let wait = Duration::from_secs(10);
        let Ok(ServerEvent::Connected { listing, signed_in: false, .. }) = events.recv_timeout(wait) else { panic!("not connected") };
        assert_eq!((listing.tools.len(), listing.resources[0].name.as_str(), listing.templates[0].uri_template.as_str()), (3, "Kyoto trip", "notes://{id}"));
        assert!(listing.prompts[0].arguments[0].required);
        // The slow call holds nothing up; given up, the server hears of it.
        link.call("job-slow", "slow", json!({}), wait);
        while slow_id.load(Ordering::Relaxed) == 0 {
            std::thread::sleep(Duration::from_millis(10));
        }
        link.call("job-fast", "fast", json!({}), wait);
        assert_eq!(events.recv_timeout(wait).unwrap(), ServerEvent::Answer { server: "notes".into(), job: "job-fast".into(), said: Ok("quick".into()) });
        link.cancel("job-slow", "The user stopped it.");
        assert!(matches!(events.recv_timeout(wait), Ok(ServerEvent::Answer { job, said: Err(Fail::Stopped), .. }) if job == "job-slow"));
        let told = cancelled.lock().unwrap().clone().unwrap();
        assert_eq!((told["requestId"].as_u64(), told["reason"].as_str()), (Some(slow_id.load(Ordering::Relaxed)), Some("The user stopped it.")));
        // A question inside a call, answered; then the call's answer.
        link.call("job-ask", "add_note", json!({}), wait);
        let Ok(ServerEvent::Asks { id, ask: elicit::Ask::Form(form), .. }) = events.recv_timeout(wait) else { panic!("no question") };
        assert_eq!((id.as_str(), form.message.as_str()), (Some("q1"), "Which notebook?"));
        link.answer(&id, json!({ "action": "accept", "content": { "notebook": "japan" } }));
        assert_eq!(events.recv_timeout(wait).unwrap(), ServerEvent::Answer { server: "notes".into(), job: "job-ask".into(), said: Ok("noted in Japan 2026".into()) });
        // A resource read, and a prompt got.
        link.read("job-read", "notes://1", wait);
        assert_eq!(events.recv_timeout(wait).unwrap(), ServerEvent::Answer { server: "notes".into(), job: "job-read".into(), said: Ok("Nozomi 7, 07:00".into()) });
        link.prompt("prompt-1", "summarize", serde_json::Map::from_iter([("note".to_string(), json!("Kyoto trip"))]), wait);
        assert_eq!(events.recv_timeout(wait).unwrap(), ServerEvent::Prompted { server: "notes".into(), job: "prompt-1".into(), said: Ok("Sum up Kyoto trip.".into()) });
    }

    /// Flow's client against the MCP project's reference server, where
    /// `FLOW_MCP_EVERYTHING` names its `dist/index.js` (npm's
    /// `@modelcontextprotocol/server-everything`, run by `node`): what it
    /// offers listed, a call, a resource read, a prompt got, a form asked
    /// for and answered as its schema says, a link asked to open (and
    /// -32042's), and a long call given up without holding up the next.
    #[test]
    #[ignore]
    fn the_reference_server_is_listed_called_read_asked_and_given_up() {
        let Ok(script) = std::env::var("FLOW_MCP_EVERYTHING") else { return eprintln!("FLOW_MCP_EVERYTHING names no server") };
        crate::i18n::set_language("en", &[]);
        let (told, events) = crossbeam_channel::unbounded();
        let tell: Arc<dyn Fn(ServerEvent) + Send + Sync> = Arc::new(move |event| {
            let _ = told.send(event);
        });
        let how = How::Stdio { command: "node".into(), args: vec![script, "stdio".into()], env: Default::default() };
        let link = Link::start(Server { name: "everything".into(), how, oauth: Default::default() }, Box::new(|| oauth::Auth::None), Vec::new(), tell);
        let wait = Duration::from_secs(20);
        let ServerEvent::Connected { listing, .. } = events.recv_timeout(wait).expect("not connected") else { panic!("not connected") };
        // What comes next, past its lists made again (it says they changed).
        let next = || loop {
            match events.recv_timeout(wait).expect("the server said nothing") {
                ServerEvent::Connected { .. } => continue,
                event => break event,
            }
        };
        let tools: Vec<&str> = listing.tools.iter().map(|t| t.name.as_str()).collect();
        for tool in ["echo", "trigger-elicitation-request", "trigger-url-elicitation", "trigger-long-running-operation"] {
            assert!(tools.contains(&tool), "{tool} among {tools:?}");
        }
        assert!(!listing.resources.is_empty() || !listing.templates.is_empty(), "{listing:?}");
        assert!(listing.prompts.iter().any(|p| p.name == "args-prompt" && p.arguments.iter().any(|a| a.name == "city" && a.required)));
        let answer = |job: &str| match next() {
            ServerEvent::Answer { job: j, said, .. } if j == job => said,
            other => panic!("{other:?}"),
        };
        link.call("echo", "echo", json!({ "message": "hello from Flow" }), wait);
        assert!(answer("echo").unwrap().contains("hello from Flow"));
        let uri = listing.resources.first().map(|r| r.uri.clone()).unwrap_or_else(|| "demo://resource/dynamic/text/1".into());
        link.read("read", &uri, wait);
        assert!(!answer("read").unwrap().is_empty(), "{uri} read as text");
        link.prompt("prompt", "args-prompt", serde_json::Map::from_iter([("city".to_string(), json!("Kyoto"))]), wait);
        let ServerEvent::Prompted { said, .. } = next() else { panic!("no prompt") };
        assert!(said.unwrap().contains("Kyoto"));
        // A form of every kind of field, answered as its schema says.
        link.call("form", "trigger-elicitation-request", json!({}), wait);
        let ServerEvent::Asks { id, ask: elicit::Ask::Form(form), .. } = next() else { panic!("no form") };
        let mut answers = form.start.clone();
        for (field, answer) in form.fields.iter().zip(answers.iter_mut()) {
            match (&field.kind, &mut *answer) {
                (elicit::Kind::Text { format, .. }, elicit::Answer::Text(text)) if text.is_empty() => {
                    *text = match format {
                        Some(elicit::Format::Email) => "ada@example.com",
                        Some(elicit::Format::Uri) => "https://example.com/",
                        Some(elicit::Format::Date) => "2026-11-16",
                        Some(elicit::Format::DateTime) => "2026-11-16T09:15:00Z",
                        None => "Flow",
                    }
                    .into()
                }
                (elicit::Kind::Number { min, .. }, elicit::Answer::Text(text)) if text.is_empty() => *text = min.unwrap_or(5.0).max(1.0).to_string(),
                (elicit::Kind::One(_), elicit::Answer::One(picked)) if picked.is_none() => *picked = Some(0),
                (elicit::Kind::Many { min, .. }, elicit::Answer::Many(on)) => {
                    for o in on.iter_mut().take(min.unwrap_or(1).max(1)) {
                        *o = true;
                    }
                }
                _ => {}
            }
        }
        let content = form.content(&answers).unwrap_or_else(|problems| panic!("{problems:?} in {:?}", form.fields));
        link.answer(&id, json!({ "action": "accept", "content": content }));
        let said = answer("form").unwrap();
        assert!(said.contains("accept"), "{said}");
        // A link to open, asked for, then as -32042.
        let args = json!({ "url": "https://example.com/connect", "message": "Connect it." });
        link.call("link", "trigger-url-elicitation", args.clone(), wait);
        let ServerEvent::Asks { id, ask: elicit::Ask::Link(asked), .. } = next() else { panic!("no link") };
        assert_eq!(asked.around_host().1, "example.com");
        link.answer(&id, json!({ "action": "accept" }));
        assert!(answer("link").is_ok());
        let mut error_path = args.clone();
        error_path["errorPath"] = json!(true);
        link.call("needs", "trigger-url-elicitation", error_path, wait);
        assert!(matches!(answer("needs"), Err(Fail::Links(links)) if !links.is_empty()));
        // A long call given up: the next isn't held up by it.
        link.call("long", "trigger-long-running-operation", json!({ "duration": 30, "steps": 30 }), Duration::from_secs(60));
        std::thread::sleep(Duration::from_secs(1));
        link.cancel("long", "The user stopped it.");
        assert_eq!(answer("long"), Err(Fail::Stopped));
        link.call("after", "echo", json!({ "message": "still here" }), wait);
        assert!(answer("after").unwrap().contains("still here"));
    }

    #[test]
    fn what_a_server_sends_unasked_goes_to_the_links_thread() {
        let (to, from) = crossbeam_channel::unbounded();
        assert!(route_unasked(&json!({ "jsonrpc": "2.0", "id": "a", "method": "ping" }), &to));
        assert!(route_unasked(&json!({ "jsonrpc": "2.0", "method": "notifications/tools/list_changed" }), &to));
        assert!(!route_unasked(&json!({ "jsonrpc": "2.0", "id": 3, "result": {} }), &to), "an answer is a request's own");
        assert!(matches!(from.try_recv(), Ok(Incoming::Request { id, method, .. }) if id == "a" && method == "ping"));
        assert!(matches!(from.try_recv(), Ok(Incoming::Note { method, .. }) if method == "notifications/tools/list_changed"));
    }
}
