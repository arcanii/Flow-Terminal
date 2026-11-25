//! The tools Flow offers outside apps, each of a [`Kind`] the user allows
//! or not, with the JSON schema of its arguments. Their words go to the
//! app's model, so they stay English.

use serde_json::{json, Value};

/// What a tool does, as the user allows it: each app may be allowed some
/// kinds and not others ([`super::grants::Grant`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    /// List the tabs and read what a pane shows, and Flow's help.
    Read,
    /// Open, split, close and bring forward tabs and panes.
    Tabs,
    /// Change what `flow-settings` may change.
    Settings,
    /// Run commands in the app's own panes.
    Commands,
}

impl Kind {
    pub const ALL: [Kind; 4] = [Kind::Read, Kind::Tabs, Kind::Settings, Kind::Commands];

    /// Its name in `mcp.json` and the words' keys.
    pub fn key(self) -> &'static str {
        match self {
            Kind::Read => "read",
            Kind::Tabs => "tabs",
            Kind::Settings => "settings",
            Kind::Commands => "commands",
        }
    }
}

/// A tool: its name, what it is, its kind and its arguments' schema.
pub struct Tool {
    pub name: &'static str,
    pub title: &'static str,
    pub kind: Kind,
    pub description: &'static str,
    /// Whether it only reads.
    pub read_only: bool,
    pub schema: fn() -> Value,
}

/// Every tool, in the order `tools/list` gives them.
pub const TOOLS: [Tool; 9] = [
    Tool {
        name: "list_tabs",
        title: "List tabs",
        kind: Kind::Read,
        description: "List Flow's tabs and each one's panes: the pane's id, its label and shell, its folder, whether it's at its \
prompt, which is in front, and which are yours (opened for you). Use the ids with the other tools.",
        read_only: true,
        schema: || json!({ "type": "object", "properties": {} }),
    },
    Tool {
        name: "read_pane",
        title: "Read a pane",
        kind: Kind::Read,
        description: "Read the text a pane shows, its screen and the scrollback above it: the last `lines` lines (200 at first, \
at most 2000). With no pane, the pane in front.",
        read_only: true,
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "pane": { "type": "integer", "description": "The pane's id, from list_tabs; the pane in front where it's left out." },
                    "lines": { "type": "integer", "minimum": 1, "maximum": 2000, "description": "How many of the last lines to read." }
                }
            })
        },
    },
    Tool {
        name: "run_command",
        title: "Run a command",
        kind: Kind::Commands,
        description: "Run a shell command in a pane of your own in Flow (one is opened beside the user's the first time, labeled \
with your name), or in a pane you opened, and get back its output, exit code and folder. Never in the user's own panes. The \
user may be asked first, and always is for a risky command. A command still running when the time is up is left running, and \
its output so far comes back.",
        read_only: false,
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "The command line to run, in that pane's shell." },
                    "pane": { "type": "integer", "description": "One of your own panes, from list_tabs; your pane beside the user's where it's left out." },
                    "timeout_secs": { "type": "integer", "minimum": 1, "maximum": 600, "description": "How long to wait for it; the user's setting where it's left out." }
                },
                "required": ["command"]
            })
        },
    },
    Tool {
        name: "open_tab",
        title: "Open a tab",
        kind: Kind::Tabs,
        description: "Open a tab of your own in Flow, in a shell, a theme and a folder where given. Says its pane's id, which \
run_command can run in.",
        read_only: false,
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "shell": { "type": "string", "description": "The shell, by its name in Flow's new-tab picker (zsh, bash, PowerShell…); the default where left out." },
                    "theme": { "type": "string", "description": "A theme's id (flow, green, amber, teletype…); Flow's own where left out." },
                    "folder": { "type": "string", "description": "The folder to start in." }
                }
            })
        },
    },
    Tool {
        name: "split_pane",
        title: "Split a pane",
        kind: Kind::Tabs,
        description: "Split one of your own panes, side by side or one above the other, and open a shell of the same kind in the \
new half, which is yours too. Says its id.",
        read_only: false,
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "pane": { "type": "integer", "description": "One of your own panes." },
                    "across": { "type": "boolean", "description": "True for one above the other; side by side where false or left out." }
                },
                "required": ["pane"]
            })
        },
    },
    Tool {
        name: "close_pane",
        title: "Close a pane",
        kind: Kind::Tabs,
        description: "Close one of your own panes, or its tab where it's the tab's last. The user's own panes and tabs can't be \
closed.",
        read_only: false,
        schema: || json!({ "type": "object", "properties": { "pane": { "type": "integer" } }, "required": ["pane"] }),
    },
    Tool {
        name: "focus_pane",
        title: "Bring a pane forward",
        kind: Kind::Tabs,
        description: "Bring a pane, and its tab, to the front of Flow, so the user sees it.",
        read_only: false,
        schema: || json!({ "type": "object", "properties": { "pane": { "type": "integer" } }, "required": ["pane"] }),
    },
    Tool {
        name: "flow_settings",
        title: "Flow's settings",
        kind: Kind::Settings,
        description: "Read or change Flow's own settings, as the flow-settings program does: list (every setting a program may \
change, with its value), describe (what each takes, or one setting's given its key), get, set or reset one by its dotted key (font.size, ui.look, fx.style). \
The look's settings change the tab in front, the rest every tab. The assistant's settings and the user's own themes are closed.",
        read_only: false,
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "do": { "type": "string", "enum": ["list", "describe", "get", "set", "reset"] },
                    "key": { "type": "string", "description": "The setting's dotted key, for get, set and reset, and for describe to say what one setting takes." },
                    "value": { "description": "The new value, for set, of the kind the setting has." }
                },
                "required": ["do"]
            })
        },
    },
    Tool {
        name: "flow_help",
        title: "Flow's help",
        kind: Kind::Read,
        description: "Read one of Flow's own help pages (its keys, Settings, themes, apps and more), or with no page, the list of \
them. Use it for questions about Flow itself.",
        read_only: true,
        schema: || json!({ "type": "object", "properties": { "page": { "type": "string", "description": "The page's name, such as settings or keys." } } }),
    },
];

/// The tool called `name`.
pub fn find(name: &str) -> Option<&'static Tool> {
    TOOLS.iter().find(|t| t.name == name)
}

/// `tools/list`'s answer: each tool's name, title, description, schema and
/// hints.
pub fn list() -> Value {
    let tools: Vec<Value> = TOOLS
        .iter()
        .map(|t| {
            json!({
                "name": t.name,
                "title": t.title,
                "description": t.description,
                "inputSchema": (t.schema)(),
                "annotations": { "title": t.title, "readOnlyHint": t.read_only, "destructiveHint": !t.read_only, "openWorldHint": false }
            })
        })
        .collect();
    json!({ "tools": tools })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_has_an_object_schema_and_a_kind() {
        let listed = list();
        let tools = listed["tools"].as_array().unwrap();
        assert_eq!(tools.len(), TOOLS.len());
        for (tool, entry) in TOOLS.iter().zip(tools) {
            assert_eq!(entry["name"], tool.name);
            assert_eq!(entry["inputSchema"]["type"], "object", "{}", tool.name);
            for required in entry["inputSchema"]["required"].as_array().into_iter().flatten() {
                assert!(entry["inputSchema"]["properties"].get(required.as_str().unwrap()).is_some(), "{}", tool.name);
            }
            assert_eq!(find(tool.name).map(|t| t.kind), Some(tool.kind));
        }
        assert!(find("rm_rf").is_none());
        // Every kind has a tool.
        assert!(Kind::ALL.iter().all(|k| TOOLS.iter().any(|t| t.kind == *k)));
    }
}
