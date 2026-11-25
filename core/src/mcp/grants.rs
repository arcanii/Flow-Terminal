//! What the user lets each outside app do, kept in `mcp.json` beside
//! config.toml as they answer its card and change it in Settings. An app
//! is known by the name its MCP client gives and the path of the program
//! that started `flow-mcp`, so another program giving the same name asks
//! again. This keeps apps from doing what the user didn't mean them to;
//! a program already running as the user could do anything anyway.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::tools::Kind;

/// Whether an app may run commands.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Commands {
    /// Never.
    #[default]
    Off,
    /// Each one waits for the user's yes.
    Ask,
    /// Without asking, but for high-risk ones, which always ask.
    Allowed,
}

impl Commands {
    pub const ALL: [Commands; 3] = [Commands::Off, Commands::Ask, Commands::Allowed];

    pub fn key(self) -> &'static str {
        match self {
            Commands::Off => "off",
            Commands::Ask => "ask",
            Commands::Allowed => "allowed",
        }
    }
}

/// One app and what it may do.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Grant {
    /// Its MCP client's name.
    pub name: String,
    /// The program that started `flow-mcp`; empty where the system didn't
    /// say.
    pub path: String,
    pub read: bool,
    pub tabs: bool,
    pub settings: bool,
    pub commands: Commands,
}

/// Whether a call may go ahead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum May {
    Yes,
    /// It waits for the user's yes in a card.
    Ask,
    No,
}

impl Grant {
    /// Whether its calls of `kind` may go ahead; a command's, as
    /// [`Grant::command`] says.
    pub fn may(&self, kind: Kind) -> bool {
        match kind {
            Kind::Read => self.read,
            Kind::Tabs => self.tabs,
            Kind::Settings => self.settings,
            Kind::Commands => self.commands != Commands::Off,
        }
    }

    /// Whether it may run `command`, read by the `shells` of the pane it
    /// goes to: never with commands off; with Ask, once the user says yes;
    /// allowed, at once, but for one of high risk or one Flow always asks
    /// about, which waits for the user's yes whatever the app may do.
    pub fn command(&self, command: &str, shells: crate::ai::tools::Shells) -> May {
        use crate::ai::tools;
        match self.commands {
            Commands::Off => May::No,
            Commands::Ask => May::Ask,
            Commands::Allowed if tools::command_risk_in(command, shells) == tools::Risk::High || tools::always_asks(command) => May::Ask,
            Commands::Allowed => May::Yes,
        }
    }

    /// What it's allowed, set kind by kind.
    pub fn set(&mut self, kind: Kind, on: bool) {
        match kind {
            Kind::Read => self.read = on,
            Kind::Tabs => self.tabs = on,
            Kind::Settings => self.settings = on,
            Kind::Commands => self.commands = if on { Commands::Ask } else { Commands::Off },
        }
    }
}

/// Every app the user has answered for.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Grants {
    pub apps: Vec<Grant>,
}

impl Grants {
    /// `mcp.json` beside config.toml.
    pub fn file() -> PathBuf {
        crate::config::config_dir().join("mcp.json")
    }

    /// What's kept at `path`; none where nothing is, or it doesn't read.
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    /// Keeps them at `path`, whole then put in place.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder).map_err(|e| e.to_string())?;
        }
        let new = path.with_extension("json.new");
        std::fs::write(&new, text).map_err(|e| e.to_string())?;
        std::fs::rename(&new, path).map_err(|e| e.to_string())
    }

    /// The app of that name started by that program, where it's been
    /// answered for.
    pub fn find(&self, name: &str, path: &str) -> Option<usize> {
        self.apps.iter().position(|g| g.name == name && g.path == path)
    }

    /// Keeps `grant`, in place of one for the same app.
    pub fn put(&mut self, grant: Grant) {
        match self.find(&grant.name, &grant.path) {
            Some(i) => self.apps[i] = grant,
            None => self.apps.push(grant),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_app_is_kept_by_its_name_and_program_and_read_back() {
        let dir = std::env::temp_dir().join(format!("flow-grants-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("mcp.json");
        assert_eq!(Grants::load(&path), Grants::default(), "none at first");
        let mut grants = Grants::default();
        grants.put(Grant { name: "claude-code".into(), path: "/usr/local/bin/claude".into(), read: true, commands: Commands::Ask, ..Grant::default() });
        grants.put(Grant { name: "claude-code".into(), path: "/tmp/elsewhere".into(), ..Grant::default() });
        // The same app answered again takes its place.
        grants.put(Grant { name: "claude-code".into(), path: "/usr/local/bin/claude".into(), read: true, tabs: true, commands: Commands::Allowed, ..Grant::default() });
        assert_eq!(grants.apps.len(), 2);
        grants.save(&path).unwrap();
        let back = Grants::load(&path);
        assert_eq!(back, grants);
        let i = back.find("claude-code", "/usr/local/bin/claude").unwrap();
        assert!(back.apps[i].tabs && back.apps[i].commands == Commands::Allowed);
        assert!(back.find("claude-code", "/opt/other").is_none(), "another program of that name asks again");
        assert!(std::fs::read_to_string(&path).unwrap().contains("\"commands\": \"allowed\""));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn commands_ask_as_the_user_said_and_risky_ones_always_ask() {
        use crate::ai::tools::Shells;
        let posix = Shells::Posix;
        let mut grant = Grant { read: true, ..Grant::default() };
        assert!(grant.may(Kind::Read) && !grant.may(Kind::Tabs) && !grant.may(Kind::Settings) && !grant.may(Kind::Commands));
        assert_eq!(grant.command("ls", posix), May::No);
        grant.commands = Commands::Ask;
        assert!(grant.may(Kind::Commands));
        assert_eq!(grant.command("ls", posix), May::Ask);
        grant.commands = Commands::Allowed;
        assert_eq!(grant.command("ls -la", posix), May::Yes);
        assert_eq!(grant.command("sudo rm -rf /", posix), May::Ask, "high risk asks whatever the app may do");
        assert_eq!(grant.command("diskutil eraseDisk APFS Empty disk2", posix), May::Ask, "one Flow always asks about");
        // A statement that begins with a variable is PowerShell's value in
        // its pane, and a program in any other shell's.
        assert_eq!(grant.command("$files | Measure-Object", Shells::Any), May::Yes);
        assert_eq!(grant.command("$files | Measure-Object", posix), May::Ask);
        grant.set(Kind::Commands, false);
        assert_eq!(grant.commands, Commands::Off);
        grant.set(Kind::Tabs, true);
        assert!(grant.may(Kind::Tabs));
    }
}
