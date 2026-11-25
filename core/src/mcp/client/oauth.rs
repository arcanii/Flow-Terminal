//! Signing in to a remote MCP server, as the spec's Authorization page
//! (2025-11-25) has it: the server's 401 names where its protected
//! resource metadata is (or Flow looks at the well-known places), that
//! names its authorization server, whose metadata says where to sign in.
//! Flow registers itself there (the client ID the user typed, one it
//! registered before, or dynamic registration), opens the sign-in in the
//! user's browser with PKCE, a `state` and the server as the `resource`,
//! hears the code come back on a loopback port, and trades it for tokens.
//! What it keeps between runs (the client, the endpoints and the refresh
//! token, [`Kept`]) goes in the credential store; the access token stays
//! in memory ([`Session`]) and is got again by refresh.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::i18n::{t, tf};

/// The loopback port Flow hears a sign-in come back on, where it's free: a
/// client ID registered by hand registers `http://127.0.0.1:47811/callback`.
pub const CALLBACK_PORT: u16 = 47811;
/// How long a sign-in in the browser is waited for.
const WAIT: Duration = Duration::from_secs(5 * 60);
/// How long before it runs out an access token is refreshed.
const EARLY: Duration = Duration::from_secs(60);

/// What a server's `WWW-Authenticate` says of signing in.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Challenge {
    /// Where its protected resource metadata is.
    pub resource_metadata: Option<String>,
    /// The scopes it needs.
    pub scope: Option<String>,
    /// Why it refused (`invalid_token`, `insufficient_scope`).
    pub error: Option<String>,
}

impl Challenge {
    /// The Bearer challenge in `header` (one or several schemes' challenges,
    /// as a 401 or 403 has them); empty where there's none.
    pub fn parse(header: &str) -> Challenge {
        let params = bearer_params(header).unwrap_or_default();
        let get = |k: &str| params.get(k).filter(|v| !v.is_empty()).cloned();
        Challenge { resource_metadata: get("resource_metadata"), scope: get("scope"), error: get("error") }
    }
}

/// The parameters of the Bearer challenge in a `WWW-Authenticate` value,
/// by their names in lowercase.
fn bearer_params(header: &str) -> Option<BTreeMap<String, String>> {
    let chars: Vec<char> = header.chars().collect();
    let mut i = 0;
    let skip = |i: &mut usize| {
        while *i < chars.len() && (chars[*i].is_whitespace() || chars[*i] == ',') {
            *i += 1;
        }
    };
    let token = |i: &mut usize| {
        let start = *i;
        while *i < chars.len() && !chars[*i].is_whitespace() && chars[*i] != ',' && chars[*i] != '=' {
            *i += 1;
        }
        chars[start..*i].iter().collect::<String>()
    };
    let value = |i: &mut usize| {
        let mut out = String::new();
        if chars.get(*i) == Some(&'"') {
            *i += 1;
            while *i < chars.len() && chars[*i] != '"' {
                if chars[*i] == '\\' && *i + 1 < chars.len() {
                    *i += 1;
                }
                out.push(chars[*i]);
                *i += 1;
            }
            *i += 1;
        } else {
            while *i < chars.len() && chars[*i] != ',' && !chars[*i].is_whitespace() {
                out.push(chars[*i]);
                *i += 1;
            }
        }
        out
    };
    let mut bearer: Option<BTreeMap<String, String>> = None;
    loop {
        skip(&mut i);
        if i >= chars.len() {
            return bearer;
        }
        let at = i;
        let word = token(&mut i);
        while i < chars.len() && chars[i] == ' ' {
            i += 1;
        }
        if chars.get(i) == Some(&'=') {
            // A parameter, of the scheme last named.
            i += 1;
            while i < chars.len() && chars[i] == ' ' {
                i += 1;
            }
            let v = value(&mut i);
            if let Some(params) = bearer.as_mut() {
                params.insert(word.to_ascii_lowercase(), v);
            }
            continue;
        }
        if word.is_empty() {
            i = at + 1;
            continue;
        }
        // A scheme: Bearer's parameters are kept until another begins.
        if bearer.is_some() {
            return bearer;
        }
        if word.eq_ignore_ascii_case("bearer") {
            bearer = Some(BTreeMap::new());
        }
    }
}

/// `url` in its parts: its scheme and authority in lowercase, and its path
/// without its query or fragment.
fn parts(url: &str) -> Option<(String, String, String)> {
    let (scheme, rest) = url.split_once("://")?;
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = rest[..end].to_ascii_lowercase();
    let after = &rest[end..];
    let path = after.split(['?', '#']).next().unwrap_or_default().to_string();
    (!authority.is_empty()).then(|| (scheme.to_ascii_lowercase(), authority, path))
}

/// The server's canonical URI, as the `resource` it's signed in for:
/// lowercase scheme and host, no query or fragment, no trailing slash.
pub fn canonical(url: &str) -> String {
    match parts(url) {
        Some((scheme, authority, path)) => format!("{scheme}://{authority}{}", path.trim_end_matches('/')),
        None => url.trim_end_matches('/').to_string(),
    }
}

fn origin(url: &str) -> Option<String> {
    parts(url).map(|(scheme, authority, _)| format!("{scheme}://{authority}"))
}

/// The host in an authority, without its user or port.
fn host(authority: &str) -> &str {
    let authority = authority.rsplit('@').next().unwrap_or(authority);
    if let Some(rest) = authority.strip_prefix('[') {
        return rest.split(']').next().unwrap_or(rest);
    }
    authority.split(':').next().unwrap_or(authority)
}

/// Whether `url` is a web address on this computer, where plain HTTP is
/// allowed (local servers, and the tests). Only http and https: the page
/// to sign in on goes to the system's opener, which would start a program
/// a `file://localhost/…` named.
fn loopback(url: &str) -> bool {
    parts(url).is_some_and(|(scheme, authority, _)| matches!(scheme.as_str(), "http" | "https") && matches!(host(&authority), "127.0.0.1" | "::1" | "localhost"))
}

/// Whether an endpoint may be used: HTTPS, or on this computer, written
/// as a web address (`link::web_address`: nothing in it a browser's command
/// line could take for more than the address).
fn secure(url: &str) -> bool {
    (parts(url).is_some_and(|(scheme, _, _)| scheme == "https") || loopback(url)) && crate::link::web_address(url)
}

/// Where a server's protected resource metadata may be, in the spec's
/// order: under its path, then at its root.
pub fn resource_metadata_urls(server: &str) -> Vec<String> {
    let Some((scheme, authority, path)) = parts(server) else { return Vec::new() };
    let path = path.trim_end_matches('/');
    let mut urls = Vec::new();
    if !path.is_empty() {
        urls.push(format!("{scheme}://{authority}/.well-known/oauth-protected-resource{path}"));
    }
    urls.push(format!("{scheme}://{authority}/.well-known/oauth-protected-resource"));
    urls
}

/// Where an authorization server's metadata may be, in the spec's order:
/// RFC 8414's and OpenID Connect's, the path put in, then OpenID Connect's
/// with it appended.
pub fn server_metadata_urls(issuer: &str) -> Vec<String> {
    let Some((scheme, authority, path)) = parts(issuer) else { return Vec::new() };
    let base = format!("{scheme}://{authority}");
    let path = path.trim_end_matches('/');
    if path.is_empty() {
        return vec![format!("{base}/.well-known/oauth-authorization-server"), format!("{base}/.well-known/openid-configuration")];
    }
    vec![
        format!("{base}/.well-known/oauth-authorization-server{path}"),
        format!("{base}/.well-known/openid-configuration{path}"),
        format!("{base}{path}/.well-known/openid-configuration"),
    ]
}

/// Whether metadata naming `resource` covers the server at `server`: the
/// same, or a path above it.
fn covers(resource: &str, server: &str) -> bool {
    let (resource, server) = (canonical(resource), canonical(server));
    server == resource || server.strip_prefix(&resource).is_some_and(|rest| rest.starts_with('/'))
}

/// PKCE's S256 challenge for `verifier` (RFC 7636).
pub fn pkce_challenge(verifier: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// `bytes` random bytes from the system, in base64url.
fn random(bytes: usize) -> Result<String, String> {
    let mut buf = vec![0u8; bytes];
    getrandom::fill(&mut buf).map_err(|e| e.to_string())?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buf))
}

/// `text` percent-encoded for a query or a form, all but the unreserved.
pub fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// A query's or a form's fields, decoded.
pub fn query_params(text: &str) -> BTreeMap<String, String> {
    let decode = |s: &str| {
        let bytes = s.as_bytes();
        let hex = |b: u8| (b as char).to_digit(16);
        let mut out = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'%' && i + 2 < bytes.len() {
                if let (Some(high), Some(low)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                    out.push((high * 16 + low) as u8);
                    i += 3;
                    continue;
                }
            }
            out.push(if bytes[i] == b'+' { b' ' } else { bytes[i] });
            i += 1;
        }
        String::from_utf8_lossy(&out).into_owned()
    };
    text.split('&').filter(|p| !p.is_empty()).map(|p| p.split_once('=').unwrap_or((p, ""))).map(|(k, v)| (decode(k), decode(v))).collect()
}

/// Fields as a form's body.
fn form(fields: &[(&str, &str)]) -> String {
    fields.iter().map(|(k, v)| format!("{}={}", encode(k), encode(v))).collect::<Vec<_>>().join("&")
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_connect(Some(Duration::from_secs(20)))
        .timeout_global(Some(Duration::from_secs(60)))
        .user_agent(concat!("flow-terminal/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

/// The JSON at `url`, none where it isn't there.
fn get_json(agent: &ureq::Agent, url: &str) -> Result<Option<Value>, String> {
    let response = agent.get(url).header("accept", "application/json").header("mcp-protocol-version", super::VERSIONS[0]).call().map_err(|e| e.to_string())?;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return if matches!(status, 400 | 401 | 403 | 404 | 405 | 410) { Ok(None) } else { Err(format!("{url} answered {status}")) };
    }
    let body = response.into_body().read_to_string().map_err(|e| e.to_string())?;
    Ok(serde_json::from_str(&body).ok())
}

/// A server's protected resource metadata, from what its challenge named
/// or the well-known places: the resource it names, its authorization
/// servers and its scopes.
struct ResourceMeta {
    resource: String,
    servers: Vec<String>,
    scopes: Vec<String>,
}

fn strings(value: &Value, key: &str) -> Vec<String> {
    value.get(key).and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(String::from).collect()
}

fn find_resource(agent: &ureq::Agent, server: &str, named: Option<&str>) -> Result<Option<ResourceMeta>, String> {
    let urls: Vec<String> = match named {
        Some(url) => vec![url.to_string()],
        None => resource_metadata_urls(server),
    };
    for url in urls {
        if !secure(&url) {
            return Err(tf("oauth.not_https", &[("url", &url)]));
        }
        if let Some(meta) = get_json(agent, &url).map_err(|e| tf("oauth.unreachable", &[("why", &e)]))? {
            let servers = strings(&meta, "authorization_servers");
            if servers.is_empty() {
                continue;
            }
            let resource = meta.get("resource").and_then(Value::as_str).unwrap_or(server).to_string();
            return Ok(Some(ResourceMeta { resource, servers, scopes: strings(&meta, "scopes_supported") }));
        }
    }
    Ok(None)
}

/// An authorization server's metadata, what Flow needs of it.
#[derive(Debug)]
struct ServerMeta {
    issuer: String,
    authorize: String,
    token: String,
    register: Option<String>,
}

fn find_server(agent: &ureq::Agent, issuer: &str) -> Result<ServerMeta, String> {
    for url in server_metadata_urls(issuer) {
        let Some(meta) = get_json(agent, &url).map_err(|e| tf("oauth.unreachable", &[("why", &e)]))? else { continue };
        return check_server(&meta, issuer);
    }
    Err(tf("oauth.no_metadata", &[("url", &issuer)]))
}

/// What Flow needs of metadata `meta` for the authorization server at
/// `issuer`, refused where it can't be signed in to safely.
fn check_server(meta: &Value, issuer: &str) -> Result<ServerMeta, String> {
    let get = |k: &str| meta.get(k).and_then(Value::as_str).map(String::from);
    let named = get("issuer").unwrap_or_default();
    if named.trim_end_matches('/') != issuer.trim_end_matches('/') {
        return Err(tf("oauth.wrong_issuer", &[("issuer", &named), ("expected", &issuer)]));
    }
    if !strings(meta, "code_challenge_methods_supported").iter().any(|m| m == "S256") {
        return Err(t("oauth.no_pkce").to_string());
    }
    let (Some(authorize), Some(token)) = (get("authorization_endpoint"), get("token_endpoint")) else {
        return Err(tf("oauth.no_metadata", &[("url", &issuer)]));
    };
    let register = get("registration_endpoint");
    for url in [Some(&authorize), Some(&token), register.as_ref()].into_iter().flatten() {
        if !secure(url) {
            return Err(tf("oauth.not_https", &[("url", url)]));
        }
    }
    Ok(ServerMeta { issuer: named, authorize, token, register })
}

/// Flow as an authorization server knows it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Client {
    pub id: String,
    #[serde(default)]
    pub secret: Option<String>,
    /// How it proves itself at the token endpoint: `none`,
    /// `client_secret_post` or `client_secret_basic`.
    pub method: String,
    pub redirect: String,
    pub issuer: String,
    /// Registered by Flow (dynamic registration), not typed by the user.
    pub registered: bool,
}

/// What's kept between runs for a server signed in to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Kept {
    pub client: Client,
    pub token_endpoint: String,
    pub resource: String,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub refresh: Option<String>,
}

/// An access token, and when it runs out where it was said.
#[derive(Debug, Clone, PartialEq)]
pub struct Access {
    pub token: String,
    pub until: Option<Instant>,
}

fn register(agent: &ureq::Agent, endpoint: &str, redirect: &str, issuer: &str) -> Result<Client, String> {
    let body = json!({
        "client_name": "Flow",
        "redirect_uris": [redirect],
        "grant_types": ["authorization_code", "refresh_token"],
        "response_types": ["code"],
        "token_endpoint_auth_method": "none"
    });
    let failed = |why: String| tf("oauth.registration_failed", &[("why", &why)]);
    let response = agent.post(endpoint).header("content-type", "application/json").header("accept", "application/json").send(body.to_string()).map_err(|e| failed(e.to_string()))?;
    let status = response.status().as_u16();
    let text = response.into_body().read_to_string().unwrap_or_default();
    let said: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    if !(200..300).contains(&status) {
        return Err(failed(said_error(&said).unwrap_or_else(|| format!("{status}"))));
    }
    let id = said.get("client_id").and_then(Value::as_str).ok_or_else(|| failed("no client_id".into()))?;
    let secret = said.get("client_secret").and_then(Value::as_str).map(String::from);
    let method = said.get("token_endpoint_auth_method").and_then(Value::as_str).map(String::from).unwrap_or_else(|| if secret.is_some() { "client_secret_basic".into() } else { "none".into() });
    Ok(Client { id: id.into(), secret, method, redirect: redirect.into(), issuer: issuer.into(), registered: true })
}

/// An OAuth error's words: its description, else its code.
fn said_error(said: &Value) -> Option<String> {
    said.get("error_description").or_else(|| said.get("error")).and_then(Value::as_str).map(String::from)
}

/// What a token request gave.
struct Got {
    access: Access,
    refresh: Option<String>,
    scope: Option<String>,
}

/// Why a token request gave nothing: `refused` where the server turned it
/// down (a refresh token no longer good), rather than wasn't reached.
struct NoToken {
    refused: bool,
    why: String,
}

fn tokens(agent: &ureq::Agent, endpoint: &str, client: &Client, fields: &[(&str, &str)]) -> Result<Got, NoToken> {
    let mut fields = fields.to_vec();
    let mut request = agent.post(endpoint).header("content-type", "application/x-www-form-urlencoded").header("accept", "application/json");
    match (client.method.as_str(), client.secret.as_deref()) {
        ("client_secret_basic", Some(secret)) => {
            let pair = format!("{}:{}", encode(&client.id), encode(secret));
            let basic = format!("Basic {}", base64::engine::general_purpose::STANDARD.encode(pair));
            request = request.header("authorization", basic.as_str());
        }
        ("client_secret_post", Some(secret)) => {
            fields.push(("client_id", &client.id));
            fields.push(("client_secret", secret));
        }
        _ => fields.push(("client_id", &client.id)),
    }
    let response = request.send(form(&fields)).map_err(|e| NoToken { refused: false, why: e.to_string() })?;
    let status = response.status().as_u16();
    let text = response.into_body().read_to_string().unwrap_or_default();
    let said: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    if !(200..300).contains(&status) {
        return Err(NoToken { refused: matches!(status, 400 | 401), why: said_error(&said).unwrap_or_else(|| format!("{status}")) });
    }
    let Some(token) = said.get("access_token").and_then(Value::as_str) else {
        return Err(NoToken { refused: false, why: "no access_token".into() });
    };
    let kind = said.get("token_type").and_then(Value::as_str).unwrap_or("Bearer");
    if !kind.eq_ignore_ascii_case("bearer") {
        return Err(NoToken { refused: false, why: format!("a {kind} token, which Flow doesn't send") });
    }
    let until = said.get("expires_in").and_then(Value::as_u64).map(|s| Instant::now() + Duration::from_secs(s));
    Ok(Got {
        access: Access { token: token.into(), until },
        refresh: said.get("refresh_token").and_then(Value::as_str).map(String::from),
        scope: said.get("scope").and_then(Value::as_str).map(String::from),
    })
}

/// A sign-in to make.
pub struct SignIn {
    /// The server's name, for the page the browser shows after.
    pub server: String,
    pub url: String,
    pub challenge: Challenge,
    /// A client ID the user typed.
    pub client_id: Option<String>,
    /// The port to hear the sign-in come back on first.
    pub port: u16,
    /// What was kept from the last sign-in, whose registration may serve.
    pub before: Option<Kept>,
}

/// Signs in as `ask` says, opening the sign-in with `open`; given up when
/// `cancel` is set. What to keep, and the access token got, or why not, in
/// the user's words.
pub fn sign_in(ask: &SignIn, open: &dyn Fn(&str), cancel: &AtomicBool) -> Result<(Kept, Access), String> {
    let agent = agent();
    // Where to sign in. A server of the spec's earlier versions has no
    // protected resource metadata, and its own origin signs in.
    let found = find_resource(&agent, &ask.url, ask.challenge.resource_metadata.as_deref())?;
    let (issuer, resource, scopes) = match found {
        Some(meta) => {
            if !covers(&meta.resource, &ask.url) {
                return Err(tf("oauth.not_covered", &[("resource", &meta.resource)]));
            }
            (meta.servers[0].clone(), canonical(&meta.resource), meta.scopes)
        }
        None => (origin(&ask.url).ok_or_else(|| tf("oauth.no_metadata", &[("url", &ask.url)]))?, canonical(&ask.url), Vec::new()),
    };
    if !secure(&issuer) {
        return Err(tf("oauth.not_https", &[("url", &issuer)]));
    }
    let meta = find_server(&agent, &issuer)?;
    // Where the browser comes back to.
    let typed = ask.client_id.as_deref().map(str::trim).filter(|id| !id.is_empty());
    let listener = match TcpListener::bind(("127.0.0.1", ask.port)) {
        Ok(listener) => listener,
        Err(_) if typed.is_some() && ask.port != 0 => return Err(tf("oauth.port_taken", &[("port", &ask.port)])),
        Err(_) => TcpListener::bind(("127.0.0.1", 0)).map_err(|e| tf("oauth.unreachable", &[("why", &e.to_string())]))?,
    };
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let redirect = format!("http://127.0.0.1:{port}/callback");
    // Who Flow is there.
    let client = match (typed, &ask.before) {
        (Some(id), _) => Client { id: id.into(), secret: None, method: "none".into(), redirect: redirect.clone(), issuer: meta.issuer.clone(), registered: false },
        (None, Some(before)) if before.client.registered && before.client.issuer == meta.issuer && before.client.redirect == redirect => before.client.clone(),
        (None, _) => match &meta.register {
            Some(endpoint) => register(&agent, endpoint, &redirect, &meta.issuer)?,
            None => return Err(tf("oauth.needs_client_id", &[("redirect", &format!("http://127.0.0.1:{CALLBACK_PORT}/callback"))])),
        },
    };
    let verifier = random(32)?;
    let state = random(16)?;
    let scope = ask.challenge.scope.clone().or_else(|| (!scopes.is_empty()).then(|| scopes.join(" ")));
    let mut url = format!(
        "{}{}response_type=code&client_id={}&redirect_uri={}&code_challenge={}&code_challenge_method=S256&state={}&resource={}",
        meta.authorize,
        if meta.authorize.contains('?') { '&' } else { '?' },
        encode(&client.id),
        encode(&redirect),
        pkce_challenge(&verifier),
        encode(&state),
        encode(&resource)
    );
    if let Some(scope) = &scope {
        url.push_str(&format!("&scope={}", encode(scope)));
    }
    open(&url);
    let code = wait_for_code(&listener, &state, &ask.server, cancel)?;
    let got = tokens(&agent, &meta.token, &client, &[("grant_type", "authorization_code"), ("code", &code), ("redirect_uri", &redirect), ("code_verifier", &verifier), ("resource", &resource)])
        .map_err(|e| tf("oauth.token_failed", &[("why", &e.why)]))?;
    let kept = Kept { client, token_endpoint: meta.token, resource, scope: got.scope.or(scope), refresh: got.refresh };
    Ok((kept, got.access))
}

/// Waits for the browser to come back with the code, answering it with a
/// page in the user's words.
fn wait_for_code(listener: &TcpListener, state: &str, server: &str, cancel: &AtomicBool) -> Result<String, String> {
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let until = Instant::now() + WAIT;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(t("oauth.stopped").to_string());
        }
        if Instant::now() > until {
            return Err(t("oauth.timed_out").to_string());
        }
        match listener.accept() {
            Ok((stream, _)) => {
                if let Some(outcome) = answer_callback(stream, state, server) {
                    return outcome;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => return Err(e.to_string()),
        }
    }
}

/// One request to the loopback port: the code where it's the sign-in
/// coming back with the right `state`, its refusal, or none (anything
/// else, which leaves Flow waiting).
fn answer_callback(stream: TcpStream, state: &str, server: &str) -> Option<Result<String, String>> {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let mut first = String::new();
    reader.read_line(&mut first).ok()?;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 || line.trim().is_empty() {
            break;
        }
    }
    let target = first.split_whitespace().nth(1).unwrap_or_default();
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let mut out = stream;
    if path != "/callback" {
        let _ = write!(out, "HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n");
        return None;
    }
    let params = query_params(query);
    let page = |out: &mut TcpStream, words: String| {
        let escaped = words.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
        let body = format!("<!doctype html><meta charset=\"utf-8\"><title>Flow</title><body style=\"font: 16px system-ui, sans-serif; margin: 3em auto; max-width: 32em; padding: 0 1em\"><p>{escaped}</p></body>");
        let _ = write!(out, "HTTP/1.1 200 OK\r\ncontent-type: text/html; charset=utf-8\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len());
    };
    if params.get("state").map(String::as_str) != Some(state) {
        page(&mut out, tf("oauth.page_failed", &[("server", &server), ("why", &t("oauth.wrong_state"))]));
        return None;
    }
    if let Some(error) = params.get("error") {
        let why = params.get("error_description").unwrap_or(error).clone();
        page(&mut out, tf("oauth.page_failed", &[("server", &server), ("why", &why)]));
        return Some(Err(tf("oauth.denied", &[("why", &why)])));
    }
    let code = params.get("code").filter(|c| !c.is_empty())?.clone();
    page(&mut out, tf("oauth.page_done", &[("server", &server)]));
    Some(Ok(code))
}

/// A server signed in to: its access token, refreshed as it runs out, and
/// what's kept, saved again where the refresh token turns over.
pub struct Session {
    pub kept: Kept,
    access: Option<Access>,
    save: Box<dyn Fn(&Kept) + Send>,
}

impl Session {
    pub fn new(kept: Kept, access: Option<Access>, save: Box<dyn Fn(&Kept) + Send>) -> Session {
        Session { kept, access, save }
    }

    /// The access token to send, refreshed first where it's about to run
    /// out; none where there's no good one.
    pub fn bearer(&mut self) -> Option<String> {
        let fresh = self.access.as_ref().is_some_and(|a| a.until.is_none_or(|until| until > Instant::now() + EARLY));
        if !fresh {
            self.refresh();
        }
        self.access.as_ref().map(|a| a.token.clone())
    }

    /// Gets a new access token with the refresh token: false where there's
    /// none, or it's no longer good (then forgotten).
    pub fn refresh(&mut self) -> bool {
        let Some(refresh) = self.kept.refresh.clone() else {
            self.access = None;
            return false;
        };
        let fields = [("grant_type", "refresh_token"), ("refresh_token", refresh.as_str()), ("resource", self.kept.resource.as_str())];
        match tokens(&agent(), &self.kept.token_endpoint, &self.kept.client, &fields) {
            Ok(got) => {
                self.access = Some(got.access);
                if let Some(new) = got.refresh.filter(|r| *r != refresh) {
                    self.kept.refresh = Some(new);
                    (self.save)(&self.kept);
                }
                true
            }
            Err(no) => {
                log::warn!("refreshing an MCP server's token: {}", no.why);
                self.access = None;
                if no.refused {
                    self.kept.refresh = None;
                    (self.save)(&self.kept);
                }
                false
            }
        }
    }
}

/// How a remote server's requests are authorized.
#[derive(Default)]
pub enum Auth {
    #[default]
    None,
    /// A token the user pasted (Settings' Token…).
    Token(String),
    /// Signed in.
    Session(Box<Session>),
}

impl Auth {
    pub fn bearer(&mut self) -> Option<String> {
        match self {
            Auth::None => None,
            Auth::Token(token) => Some(token.clone()),
            Auth::Session(session) => session.bearer(),
        }
    }

    /// After a 401: whether a refresh gave a new token to try again with.
    pub fn refresh(&mut self) -> bool {
        match self {
            Auth::Session(session) => session.refresh(),
            _ => false,
        }
    }

    /// Whether the user gave the token, so a refusal is theirs to fix
    /// rather than a sign-in.
    pub fn pasted(&self) -> bool {
        matches!(self, Auth::Token(_))
    }
}

#[cfg(test)]
mod tests {
    use super::super::stand_in::{serve, Reply};
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn a_challenge_is_read_from_among_others() {
        let c = Challenge::parse(r#"Bearer resource_metadata="https://mcp.example.com/.well-known/oauth-protected-resource", scope="files:read files:write""#);
        assert_eq!(c.resource_metadata.as_deref(), Some("https://mcp.example.com/.well-known/oauth-protected-resource"));
        assert_eq!(c.scope.as_deref(), Some("files:read files:write"));
        let c = Challenge::parse(r#"Basic realm="x", Bearer error="insufficient_scope", scope="a b", error_description="needs \"b\"", DPoP algs="ES256""#);
        assert_eq!((c.error.as_deref(), c.scope.as_deref()), (Some("insufficient_scope"), Some("a b")));
        assert_eq!(c.resource_metadata, None);
        assert_eq!(Challenge::parse("Basic realm=\"x\""), Challenge::default());
        assert_eq!(Challenge::parse("bearer realm=mcp,scope=one").scope.as_deref(), Some("one"));
    }

    #[test]
    fn metadata_is_looked_for_where_the_spec_says_in_its_order() {
        assert_eq!(resource_metadata_urls("https://example.com/public/mcp"), ["https://example.com/.well-known/oauth-protected-resource/public/mcp", "https://example.com/.well-known/oauth-protected-resource"]);
        assert_eq!(resource_metadata_urls("https://example.com/"), ["https://example.com/.well-known/oauth-protected-resource"]);
        assert_eq!(
            server_metadata_urls("https://auth.example.com/tenant1"),
            ["https://auth.example.com/.well-known/oauth-authorization-server/tenant1", "https://auth.example.com/.well-known/openid-configuration/tenant1", "https://auth.example.com/tenant1/.well-known/openid-configuration"]
        );
        assert_eq!(server_metadata_urls("https://auth.example.com"), ["https://auth.example.com/.well-known/oauth-authorization-server", "https://auth.example.com/.well-known/openid-configuration"]);
        assert_eq!(canonical("HTTPS://MCP.Example.com:8443/Server/mcp/?x=1#f"), "https://mcp.example.com:8443/Server/mcp");
        assert!(covers("https://mcp.example.com", "https://mcp.example.com/mcp") && covers("https://mcp.example.com/mcp/", "https://mcp.example.com/mcp"));
        assert!(!covers("https://mcp.example.com/mc", "https://mcp.example.com/mcp") && !covers("https://other.example.com", "https://mcp.example.com/mcp"));
    }

    #[test]
    fn pkce_and_forms_are_as_their_rfcs_have_them() {
        // RFC 7636's own example.
        assert_eq!(pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"), "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
        assert_eq!(random(32).unwrap().len(), 43);
        assert_eq!(encode("a b/c?d=é"), "a%20b%2Fc%3Fd%3D%C3%A9");
        let params = query_params("code=a%2Bb&state=x+y&empty=&odd=%zz%4");
        assert_eq!(params["code"], "a+b");
        assert_eq!(params["state"], "x y");
        assert_eq!(params["empty"], "");
        assert_eq!(params["odd"], "%zz%4");
    }

    #[test]
    fn metadata_that_cant_be_signed_in_to_safely_is_refused() {
        crate::i18n::set_language("en", &[]);
        let good = json!({ "issuer": "https://auth.example.com", "authorization_endpoint": "https://auth.example.com/a", "token_endpoint": "https://auth.example.com/t", "code_challenge_methods_supported": ["S256"] });
        assert!(check_server(&good, "https://auth.example.com/").is_ok());
        let mut no_pkce = good.clone();
        no_pkce["code_challenge_methods_supported"] = json!(["plain"]);
        assert!(check_server(&no_pkce, "https://auth.example.com").unwrap_err().contains("PKCE"));
        assert!(check_server(&good, "https://evil.example.com").unwrap_err().contains("evil.example.com"));
        let mut plain = good.clone();
        plain["token_endpoint"] = json!("http://auth.example.com/t");
        assert!(check_server(&plain, "https://auth.example.com").unwrap_err().contains("http://auth.example.com/t"));
        assert!(secure("http://127.0.0.1:9/x") && secure("http://localhost/x") && !secure("http://example.com"));
    }

    #[test]
    fn a_page_to_sign_in_on_that_would_start_a_program_is_refused() {
        crate::i18n::set_language("en", &[]);
        // The page goes to the system's opener: on this computer, as a
        // file, it would be run; with a quote in it, a browser's command
        // line could take what follows for more.
        for endpoint in ["file://localhost/C:/Windows/System32/calc.exe", "file://127.0.0.1/Applications/Calculator.app", "ms-settings://localhost/", "https://auth.example.com/a\"-new-instance", "https://auth.example.com\\@evil.example/a"] {
            assert!(!secure(endpoint), "{endpoint}");
            let mut meta = json!({ "issuer": "https://auth.example.com", "authorization_endpoint": "https://auth.example.com/a", "token_endpoint": "https://auth.example.com/t", "code_challenge_methods_supported": ["S256"] });
            meta["authorization_endpoint"] = json!(endpoint);
            assert!(check_server(&meta, "https://auth.example.com").is_err(), "{endpoint}");
        }
        assert!(secure("https://auth.example.com/authorize?prompt=consent") && secure("http://[::1]:8080/a"));
    }

    /// A stand-in authorization server beside a stand-in MCP server, on
    /// one port, keeping what the token endpoint was sent.
    fn sign_in_server(rotate: bool) -> (String, Arc<Mutex<Vec<std::collections::BTreeMap<String, String>>>>) {
        let tokens = Arc::new(Mutex::new(Vec::new()));
        let challenge = Arc::new(Mutex::new(String::new()));
        let kept = tokens.clone();
        let (base, _) = serve(move |base, request| match (request.method.as_str(), request.path()) {
            ("GET", "/.well-known/oauth-protected-resource/mcp") => Reply::json(200, json!({ "resource": format!("{base}/mcp"), "authorization_servers": [base], "scopes_supported": ["notes", "notes:write"] })),
            ("GET", "/.well-known/oauth-authorization-server") => Reply::json(
                200,
                json!({ "issuer": base, "authorization_endpoint": format!("{base}/authorize"), "token_endpoint": format!("{base}/token"),
                        "registration_endpoint": format!("{base}/register"), "code_challenge_methods_supported": ["S256"] }),
            ),
            ("POST", "/register") => {
                let asked = request.json();
                assert_eq!(asked["token_endpoint_auth_method"], "none");
                assert!(asked["redirect_uris"][0].as_str().unwrap().starts_with("http://127.0.0.1:"));
                Reply::json(201, json!({ "client_id": "c-1" }))
            }
            ("GET", "/authorize") => {
                let asked = request.form();
                assert_eq!((asked["client_id"].as_str(), asked["code_challenge_method"].as_str(), asked["scope"].as_str()), ("c-1", "S256", "notes notes:write"));
                assert_eq!(asked["resource"], format!("{base}/mcp"));
                *challenge.lock().unwrap() = asked["code_challenge"].clone();
                let back = format!("{}?code=the-code&state={}", asked["redirect_uri"], encode(&asked["state"]));
                Reply { status: 302, headers: vec![("location".into(), back)], body: String::new() }
            }
            ("POST", "/token") => {
                let sent = request.form();
                kept.lock().unwrap().push(sent.clone());
                match sent["grant_type"].as_str() {
                    "authorization_code" => {
                        assert_eq!(pkce_challenge(&sent["code_verifier"]), *challenge.lock().unwrap(), "the verifier answers the challenge");
                        assert_eq!(sent["code"], "the-code");
                        Reply::json(200, json!({ "access_token": "a-1", "token_type": "Bearer", "expires_in": 3600, "refresh_token": "r-1" }))
                    }
                    _ if sent["refresh_token"] == "r-1" => Reply::json(200, json!({ "access_token": "a-2", "token_type": "bearer", "refresh_token": if rotate { "r-2" } else { "r-1" } })),
                    _ => Reply::json(400, json!({ "error": "invalid_grant" })),
                }
            }
            _ => Reply::json(404, json!({})),
        });
        (base, tokens)
    }

    #[test]
    fn signing_in_registers_flow_opens_the_browser_and_trades_the_code_for_tokens() {
        crate::i18n::set_language("en", &[]);
        let (base, sent) = sign_in_server(true);
        let ask = SignIn { server: "notes".into(), url: format!("{base}/mcp"), challenge: Challenge::default(), client_id: None, port: 0, before: None };
        // The browser: it follows the sign-in back to Flow's port.
        let open = |url: &str| {
            let url = url.to_string();
            std::thread::spawn(move || {
                let page = ureq::get(&url).call().unwrap().into_body().read_to_string().unwrap();
                assert!(page.contains("signed in to notes"), "{page}");
            });
        };
        let (kept, access) = sign_in(&ask, &open, &AtomicBool::new(false)).unwrap();
        assert_eq!((kept.client.id.as_str(), kept.client.registered, kept.refresh.as_deref()), ("c-1", true, Some("r-1")));
        assert_eq!(access.token, "a-1");
        assert!(access.until.is_some());
        let first = sent.lock().unwrap()[0].clone();
        assert_eq!((first["client_id"].as_str(), first["resource"].as_str()), ("c-1", format!("{base}/mcp").as_str()));
        assert!(first["redirect_uri"].ends_with("/callback"));
        // Refreshed, its refresh token turned over and saved.
        let saved = Arc::new(Mutex::new(None));
        let into = saved.clone();
        let mut session = Session::new(kept, None, Box::new(move |k: &Kept| *into.lock().unwrap() = Some(k.clone())));
        assert_eq!(session.bearer().as_deref(), Some("a-2"));
        assert_eq!(saved.lock().unwrap().as_ref().and_then(|k| k.refresh.clone()).as_deref(), Some("r-2"));
        // r-2 is no good to this server: refused, it's forgotten.
        assert!(!session.refresh());
        assert_eq!(session.kept.refresh, None);
        assert_eq!(session.bearer(), None);
    }

    #[test]
    fn the_browser_coming_back_with_the_wrong_state_is_turned_away() {
        crate::i18n::set_language("en", &[]);
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let at = format!("http://{}", listener.local_addr().unwrap());
        let cancel = Arc::new(AtomicBool::new(false));
        let stop = cancel.clone();
        let browser = std::thread::spawn(move || {
            let wrong = ureq::get(&format!("{at}/callback?code=x&state=nope")).call().unwrap().into_body().read_to_string().unwrap();
            let elsewhere = ureq::get(&format!("{at}/favicon.ico")).config().http_status_as_error(false).build().call().unwrap().status().as_u16();
            let refused = ureq::get(&format!("{at}/callback?error=access_denied&error_description=No+thanks&state=s1")).call().unwrap().into_body().read_to_string().unwrap();
            stop.store(true, Ordering::Relaxed);
            (wrong, elsewhere, refused)
        });
        let outcome = wait_for_code(&listener, "s1", "notes", &cancel);
        let (wrong, elsewhere, refused) = browser.join().unwrap();
        assert!(wrong.contains("didn't work"), "{wrong}");
        assert_eq!(elsewhere, 404);
        assert!(refused.contains("No thanks"));
        assert_eq!(outcome, Err(tf("oauth.denied", &[("why", &"No thanks")])));
    }
}
