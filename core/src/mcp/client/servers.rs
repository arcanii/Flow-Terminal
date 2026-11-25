//! The MCP servers the assistant may use, as `mcp-servers.json` beside
//! config.toml lists them, in the `mcpServers` form Claude Desktop, Claude
//! Code and Cursor write, so a config can be pasted in: a command Flow
//! starts (stdio), or a URL (Streamable HTTP). What Flow does with each
//! (connected or not, each tool on or off, how its calls are approved)
//! is the user's in config.toml (`ai.mcp_*`), never in this file, so a
//! file written by anything else starts nothing.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

/// A server, by its name in the file.
#[derive(Debug, Clone, PartialEq)]
pub struct Server {
    pub name: String,
    pub how: How,
    /// How a remote one is signed in to, where the file says (Claude
    /// Code's `oauth` keys).
    pub oauth: OAuthFile,
}

/// A server's `oauth`: the client ID registered for Flow by hand, and the
/// port its sign-in comes back on.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OAuthFile {
    pub client_id: Option<String>,
    pub port: Option<u16>,
}

/// How Flow reaches a server.
#[derive(Debug, Clone, PartialEq)]
pub enum How {
    /// A program Flow starts, speaking MCP over its standard input and
    /// output, with its own environment over Flow's.
    Stdio { command: String, args: Vec<String>, env: BTreeMap<String, String> },
    /// A server at a URL, over Streamable HTTP, with headers of its own.
    Http { url: String, headers: BTreeMap<String, String> },
}

impl How {
    /// What Settings' Command or URL row shows: a URL, or the command line.
    pub fn line(&self) -> String {
        self.line_for(cfg!(windows))
    }

    /// The command line as `windows` or a Unix shell writes it.
    fn line_for(&self, windows: bool) -> String {
        match self {
            How::Http { url, .. } => url.clone(),
            How::Stdio { command, args, .. } => std::iter::once(command.as_str()).chain(args.iter().map(String::as_str)).map(|w| quoted(w, windows)).collect::<Vec<_>>().join(" "),
        }
    }

    /// A server from what's typed in Settings' Command or URL row: a URL,
    /// the whole path of a program that's there (spaces and all, as one is
    /// pasted), or a command line split as a shell would (quotes kept
    /// together; on Windows, where paths are written with them, `\` is no
    /// escape); none where nothing's typed.
    pub fn from_line(line: &str) -> Option<How> {
        Self::from_line_for(line, cfg!(windows))
    }

    fn from_line_for(line: &str, windows: bool) -> Option<How> {
        let line = line.trim();
        if line.starts_with("http://") || line.starts_with("https://") {
            return Some(How::Http { url: line.to_string(), headers: BTreeMap::new() });
        }
        if line.contains(' ') && Path::new(line).is_absolute() && Path::new(line).is_file() {
            return Some(How::Stdio { command: line.to_string(), args: Vec::new(), env: BTreeMap::new() });
        }
        let mut words = split_line(line, windows).into_iter();
        let command = words.next()?;
        Some(How::Stdio { command, args: words.collect(), env: BTreeMap::new() })
    }
}

/// `word` as `split_line` reads it back.
fn quoted(word: &str, windows: bool) -> String {
    if windows {
        // No escapes there: a word with a double quote goes in single ones.
        return if !word.is_empty() && !word.contains([' ', '\t', '"']) {
            word.to_string()
        } else if word.contains('"') {
            format!("'{word}'")
        } else {
            format!("\"{word}\"")
        };
    }
    if !word.is_empty() && !word.contains([' ', '"', '\'', '\\']) {
        word.to_string()
    } else {
        format!("\"{}\"", word.replace('\\', "\\\\").replace('"', "\\\""))
    }
}

/// `line` in words, as a shell splits them: spaces apart, quotes together,
/// `\` escaping the next character but on `windows`.
fn split_line(line: &str, windows: bool) -> Vec<String> {
    let (mut words, mut word, mut quote, mut any) = (Vec::new(), String::new(), None::<char>, false);
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (None, ' ' | '\t') => {
                if any {
                    words.push(std::mem::take(&mut word));
                    any = false;
                }
            }
            (None, '"' | '\'') => (quote, any) = (Some(c), true),
            (Some(q), c) if c == q => quote = None,
            (Some('"') | None, '\\') if !windows => {
                if let Some(next) = chars.next() {
                    word.push(next);
                    any = true;
                }
            }
            (_, c) => {
                word.push(c);
                any = true;
            }
        }
    }
    if any {
        words.push(word);
    }
    words
}

/// The servers' file beside config.toml.
pub fn file() -> PathBuf {
    crate::config::config_dir().join("mcp-servers.json")
}

/// The servers in the file at `path`, each by name, and why any were
/// passed over; none where there's no file.
pub fn load(path: &Path) -> (Vec<Server>, Vec<String>) {
    let Ok(text) = std::fs::read_to_string(path) else { return (Vec::new(), Vec::new()) };
    read(&text)
}

/// The servers in `text`, a file's JSON, and why any were passed over.
pub fn read(text: &str) -> (Vec<Server>, Vec<String>) {
    let mut problems = Vec::new();
    let value: Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(e) => return (Vec::new(), vec![format!("mcp-servers.json: {e}")]),
    };
    let Some(listed) = value.get("mcpServers").and_then(Value::as_object) else {
        return (Vec::new(), vec!["mcp-servers.json has no mcpServers".into()]);
    };
    let mut servers = Vec::new();
    for (name, entry) in listed {
        match server(name, entry) {
            Ok(server) => servers.push(server),
            Err(why) => problems.push(format!("{name}: {why}")),
        }
    }
    (servers, problems)
}

fn strings(value: Option<&Value>) -> BTreeMap<String, String> {
    value.and_then(Value::as_object).map(|o| o.iter().filter_map(|(k, v)| v.as_str().map(|v| (k.clone(), v.to_string()))).collect()).unwrap_or_default()
}

fn server(name: &str, entry: &Value) -> Result<Server, String> {
    let oauth = OAuthFile {
        client_id: entry.pointer("/oauth/clientId").and_then(Value::as_str).map(str::trim).filter(|id| !id.is_empty()).map(String::from),
        port: entry.pointer("/oauth/callbackPort").and_then(Value::as_u64).and_then(|p| u16::try_from(p).ok()),
    };
    let how = how(name, entry)?;
    Ok(Server { name: name.into(), how, oauth })
}

fn how(name: &str, entry: &Value) -> Result<How, String> {
    if !valid_name(name) {
        return Err("a server's name is letters, digits, hyphens and underscores".into());
    }
    let kind = entry.get("type").and_then(Value::as_str).unwrap_or_default();
    if kind == "sse" {
        return Err("the old SSE transport isn't taken; its server likely has a Streamable HTTP URL too".into());
    }
    if let Some(url) = entry.get("url").and_then(Value::as_str) {
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return Err(format!("{url} isn't an http or https URL"));
        }
        return Ok(How::Http { url: url.into(), headers: strings(entry.get("headers")) });
    }
    let command = entry.get("command").and_then(Value::as_str).filter(|c| !c.trim().is_empty()).ok_or("it has neither a command nor a url")?;
    let args = entry.get("args").and_then(Value::as_array).map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default();
    Ok(How::Stdio { command: command.into(), args, env: strings(entry.get("env")) })
}

/// Whether `name` may name a server: letters, digits, `-` and `_`, at
/// most 40.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 40 && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Writes `servers` to the file at `path`, keeping whatever else it holds
/// (a server's keys Flow doesn't read among them).
pub fn save(path: &Path, servers: &[Server]) -> Result<(), String> {
    let mut whole: Value = std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_else(|| json!({}));
    if !whole.is_object() {
        whole = json!({});
    }
    let before = whole.get("mcpServers").and_then(Value::as_object).cloned().unwrap_or_default();
    let mut listed = Map::new();
    for server in servers {
        let mut entry = before.get(&server.name).and_then(Value::as_object).cloned().unwrap_or_default();
        for key in ["command", "args", "env", "url", "headers", "type"] {
            entry.remove(key);
        }
        match &server.how {
            How::Stdio { command, args, env } => {
                entry.insert("command".into(), json!(command));
                entry.insert("args".into(), json!(args));
                if !env.is_empty() {
                    entry.insert("env".into(), json!(env));
                }
            }
            How::Http { url, headers } => {
                entry.insert("type".into(), json!("http"));
                entry.insert("url".into(), json!(url));
                if !headers.is_empty() {
                    entry.insert("headers".into(), json!(headers));
                }
            }
        }
        // Its sign-in's keys, Flow's own put over any others there.
        let mut oauth = entry.get("oauth").and_then(Value::as_object).cloned().unwrap_or_default();
        match &server.oauth.client_id {
            Some(id) => oauth.insert("clientId".into(), json!(id)),
            None => oauth.remove("clientId"),
        };
        match server.oauth.port {
            Some(port) => oauth.insert("callbackPort".into(), json!(port)),
            None => oauth.remove("callbackPort"),
        };
        if oauth.is_empty() {
            entry.remove("oauth");
        } else {
            entry.insert("oauth".into(), Value::Object(oauth));
        }
        listed.insert(server.name.clone(), Value::Object(entry));
    }
    whole["mcpServers"] = Value::Object(listed);
    let text = serde_json::to_string_pretty(&whole).map_err(|e| e.to_string())? + "\n";
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder).map_err(|e| e.to_string())?;
    }
    let new = path.with_extension("json.new");
    std::fs::write(&new, text).map_err(|e| e.to_string())?;
    std::fs::rename(&new, path).map_err(|e| e.to_string())
}

/// `value` with each `${env:NAME}` in it Flow's environment's `NAME`
/// (empty where it isn't set).
pub fn expand(value: &str) -> String {
    let mut out = String::new();
    let mut rest = value;
    while let Some(at) = rest.find("${env:") {
        out.push_str(&rest[..at]);
        let after = &rest[at + 6..];
        match after.find('}') {
            Some(end) => {
                out.push_str(&std::env::var(&after[..end]).unwrap_or_default());
                rest = &after[end + 1..];
            }
            None => {
                out.push_str(&rest[at..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// How a server's calls are approved, as the user chose for it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Approval {
    /// Every call waits for the user's yes.
    #[default]
    Ask,
    /// Tools the server marks read-only run; the rest ask.
    ReadOnly,
    /// Without asking.
    Allowed,
}

impl Approval {
    pub const ALL: [Approval; 3] = [Approval::Ask, Approval::ReadOnly, Approval::Allowed];

    pub fn key(self) -> &'static str {
        match self {
            Approval::Ask => "ask",
            Approval::ReadOnly => "read_only",
            Approval::Allowed => "allowed",
        }
    }

    pub fn of(key: &str) -> Approval {
        Approval::ALL.into_iter().find(|a| a.key() == key).unwrap_or_default()
    }

    /// Whether a call of a tool, read-only as `read_only` says, waits for
    /// the user's yes.
    pub fn asks(self, read_only: bool) -> bool {
        match self {
            Approval::Ask => true,
            Approval::ReadOnly => !read_only,
            Approval::Allowed => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_file_reads_as_each_client_writes_it() {
        let (servers, problems) = read(
            r#"{"mcpServers": {
                "files": {"command": "npx", "args": ["-y", "@modelcontextprotocol/server-filesystem", "/Users/me/Documents"], "env": {"DEBUG": "1"}},
                "linear": {"type": "http", "url": "https://mcp.linear.app/mcp", "headers": {"Authorization": "Bearer ${env:LINEAR}"}},
                "plain": {"url": "http://localhost:8080/mcp"},
                "old": {"type": "sse", "url": "https://example.com/sse"},
                "bad name": {"command": "x"},
                "nothing": {}
            }, "other": true}"#,
        );
        let names: Vec<&str> = servers.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["files", "linear", "plain"]);
        assert!(matches!(&servers[0].how, How::Stdio { command, args, env } if command == "npx" && args.len() == 3 && env["DEBUG"] == "1"));
        assert!(matches!(&servers[1].how, How::Http { url, headers } if url.ends_with("/mcp") && headers.contains_key("Authorization")));
        assert!(matches!(&servers[2].how, How::Http { .. }), "a url alone is remote");
        assert_eq!(problems.len(), 3, "{problems:?}");
        assert!(problems.iter().any(|p| p.starts_with("old: the old SSE")));
        assert!(read("nope").1[0].starts_with("mcp-servers.json"));
    }

    #[test]
    fn settings_line_is_a_url_or_a_command_split_as_a_shell_would() {
        assert_eq!(How::from_line("  https://mcp.example.com/mcp "), Some(How::Http { url: "https://mcp.example.com/mcp".into(), headers: BTreeMap::new() }));
        let how = How::from_line_for(r#"npx -y server "/Users/me/My Documents" it\'s"#, false).unwrap();
        assert_eq!(how, How::Stdio { command: "npx".into(), args: vec!["-y".into(), "server".into(), "/Users/me/My Documents".into(), "it's".into()], env: BTreeMap::new() });
        assert_eq!(How::from_line_for(&how.line_for(false), false), Some(how.clone()), "said back as typed");
        assert_eq!(How::from_line("   "), None);
        // Windows' paths keep their backslashes, and a quoted one its spaces.
        let how = How::from_line_for(r#""C:\Program Files\Flow\games\flow-mcp.exe" --dir C:\Users\me\work '{"a": 1}'"#, true).unwrap();
        let words = ["--dir", r"C:\Users\me\work", r#"{"a": 1}"#].map(String::from).to_vec();
        assert_eq!(how, How::Stdio { command: r"C:\Program Files\Flow\games\flow-mcp.exe".into(), args: words, env: BTreeMap::new() });
        assert_eq!(How::from_line_for(&how.line_for(true), true), Some(how.clone()), "said back as typed");
        // A program's whole path, pasted unquoted, is the program.
        let dir = std::env::temp_dir().join(format!("flow mcp line {}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let program = dir.join("my server");
        std::fs::write(&program, b"").unwrap();
        let line = program.display().to_string();
        assert_eq!(How::from_line(&line), Some(How::Stdio { command: line.clone(), args: vec![], env: BTreeMap::new() }));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn saving_keeps_what_flow_doesnt_read_and_expands_nothing() {
        let dir = std::env::temp_dir().join(format!("flow-mcp-servers-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("mcp-servers.json");
        std::fs::write(&path, r#"{"mcpServers": {"files": {"command": "old", "disabled": true}}, "theirs": 1}"#).unwrap();
        let servers = vec![
            Server { name: "files".into(), how: How::from_line("npx -y files").unwrap(), oauth: OAuthFile::default() },
            Server {
                name: "web".into(),
                how: How::Http { url: "https://x/mcp".into(), headers: BTreeMap::from([("Authorization".into(), "Bearer ${env:T}".into())]) },
                oauth: OAuthFile { client_id: Some("flow-123".into()), port: Some(8765) },
            },
        ];
        save(&path, &servers).unwrap();
        let back: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(back["theirs"], 1);
        assert_eq!(back["mcpServers"]["files"]["disabled"], true, "a key Flow doesn't read stays");
        assert_eq!(back["mcpServers"]["web"]["headers"]["Authorization"], "Bearer ${env:T}", "kept as written");
        assert_eq!((back["mcpServers"]["web"]["oauth"]["clientId"].as_str(), back["mcpServers"]["web"]["oauth"]["callbackPort"].as_u64()), (Some("flow-123"), Some(8765)));
        assert!(back["mcpServers"]["files"].get("oauth").is_none());
        assert_eq!(load(&path).0, servers);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn env_is_taken_from_flows_environment_and_approval_asks_as_chosen() {
        std::env::set_var("FLOW_TEST_MCP_TOKEN", "t0k");
        assert_eq!(expand("Bearer ${env:FLOW_TEST_MCP_TOKEN}!"), "Bearer t0k!");
        assert_eq!(expand("${env:FLOW_TEST_MCP_NOT_SET} ${env:unclosed"), " ${env:unclosed");
        assert!(Approval::Ask.asks(true) && Approval::ReadOnly.asks(false) && !Approval::ReadOnly.asks(true) && !Approval::Allowed.asks(false));
        assert_eq!(Approval::of("read_only"), Approval::ReadOnly);
        assert_eq!(Approval::of("?"), Approval::Ask);
    }
}
