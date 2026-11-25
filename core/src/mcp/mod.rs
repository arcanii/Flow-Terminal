//! Flow as an MCP server, so outside apps (Claude Desktop, Claude Code,
//! Cursor) can use it, each as far as the user allows.
//!
//! An app's MCP config runs `flow-mcp` (`src/bin/flow-mcp.rs`), which speaks
//! MCP's JSON-RPC over its stdin and stdout ([`protocol`]) and hands each
//! tool call to the running Flow over a socket only the user can open, a
//! named pipe on Windows ([`bridge`]). Flow listens only while Settings'
//! Outside apps is on (`mcp.enabled`). The first time an app connects, a
//! card asks the user what it may do, by kind ([`tools::Kind`]), which
//! [`grants`] keeps in `mcp.json`; commands still ask each time unless the
//! app is allowed to run them, and high-risk ones always ask.

pub mod bridge;
pub mod client;
pub mod grants;
pub mod protocol;
pub mod tools;

/// What the MCP client says it is in `initialize` (`clientInfo`): its own
/// word, which the app that started `flow-mcp` backs up ([`bridge::Peer`]).
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ClientInfo {
    pub name: String,
    #[serde(default)]
    pub version: String,
}

impl ClientInfo {
    /// The name the card and Settings show: its title where it gave one,
    /// else its name, else "An app".
    pub fn shown(&self) -> &str {
        if self.name.trim().is_empty() {
            "An app"
        } else {
            self.name.trim()
        }
    }
}
