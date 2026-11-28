//! User configuration, stored as TOML in the platform config directory.

use alacritty_terminal::vte::ansi::{CursorShape, CursorStyle};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::ai::Provider;
use crate::render::palette::{self, Ink};
use crate::APP_ID;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub ui: UiConfig,
    pub font: FontConfig,
    pub colors: ColorConfig,
    pub cursor: CursorConfig,
    pub shell: ShellConfig,
    pub ai: AiConfig,
    pub fx: FxConfig,
    pub crt: CrtConfig,
    pub window: WindowConfig,
    pub history: HistoryConfig,
    pub sound: SoundConfig,
    pub theme: ThemeConfig,
    /// flowapp's apps, as Flow checks them (Settings' Apps).
    pub apps: AppsConfig,
    /// How the terminal is recorded (Settings' Recording, `record.rs`).
    pub record: RecordConfig,
    /// Outside apps using Flow through `flow-mcp` (Settings' Outside apps,
    /// `mcp`).
    pub mcp: McpConfig,
    /// Themes saved under a name; none, and the section isn't written.
    #[serde(skip_serializing_if = "ThemesConfig::is_empty")]
    pub themes: ThemesConfig,
    /// Flow's keys where they differ from its own (`keys.rs`): a key's name
    /// and its key, "cmd+n", "none", a few in a list, or a family's
    /// modifiers ("ctrl"). None changed, and the section isn't written.
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub keys: std::collections::BTreeMap<String, crate::keys::KeySetting>,
}

/// How Flow looks after flowapp's apps (Settings' Apps): it never
/// upgrades them by itself, but reads the catalog to say when there's a
/// newer one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppsConfig {
    /// Read the catalog afresh each time Flow starts, as `flowapp update`
    /// does.
    pub check_at_start: bool,
    /// How often Flow reads the catalog by itself: "off", "daily" or
    /// "weekly" (`apps::shelf::check_every`).
    pub check: String,
}

impl Default for AppsConfig {
    fn default() -> Self {
        Self { check_at_start: false, check: "off".into() }
    }
}

/// Outside apps using Flow as an MCP server (`crate::mcp`). What each may
/// do is kept apart, in `mcp.json`, as the user answers its card.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct McpConfig {
    /// Flow listens for `flow-mcp` only while this is on: off at first.
    pub enabled: bool,
}

/// How a recording of the terminal is encoded (Settings' Recording).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RecordConfig {
    /// "smaller", "balanced", "sharp" or "lossless" (`record::QUALITIES`).
    pub quality: String,
    /// "points", the window's size in points, or "pixels", every pixel a
    /// Retina screen has (`record::SIZES`).
    pub size: String,
    /// Seconds counted down in a toast before a recording starts, so the
    /// toast is gone from the file; 0 starts at once.
    pub countdown: u32,
}

impl Default for RecordConfig {
    fn default() -> Self {
        Self { quality: "balanced".into(), size: "points".into(), countdown: 3 }
    }
}

/// Themes the user saved under a name (Settings' Save as theme…), which
/// come after the built-in ones wherever a theme is chosen.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemesConfig {
    pub saved: Vec<SavedTheme>,
}

impl ThemesConfig {
    pub fn is_empty(&self) -> bool {
        self.saved.is_empty()
    }

    /// The one saved under `name`.
    pub fn named(&self, name: &str) -> Option<&SavedTheme> {
        self.saved.iter().find(|t| t.name == name)
    }

    /// Keeps `theme`, in place of one saved under its name before (which
    /// it says), or after the others.
    pub fn save(&mut self, theme: SavedTheme) -> bool {
        match self.saved.iter_mut().find(|t| t.name == theme.name) {
            Some(old) => {
                *old = theme;
                true
            }
            None => {
                self.saved.push(theme);
                false
            }
        }
    }

    /// Removes the one saved under `name`, if there is one.
    pub fn delete(&mut self, name: &str) -> bool {
        let before = self.saved.len();
        self.saved.retain(|t| t.name != name);
        self.saved.len() != before
    }
}

/// A theme saved under a name: the look's sections as a preset file holds
/// them (`colors`, `cursor`, `font`, `fx`, `crt` and `theme`, by the same
/// names as here), each with the settings a theme sets
/// (`looks::THEME_KEYS`), written as `[[themes.saved]]` with a table of
/// each. Like a preset file edited by hand, one may hold just a few, and
/// wearing it changes only those.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SavedTheme {
    pub name: String,
    /// Its start screen, prompt and sounds, which it brings while it's worn
    /// as your own brings `[theme]`'s, never put in your own's place.
    pub theme: ThemeConfig,
    /// The rest of the look's sections.
    #[serde(flatten)]
    pub look: serde_json::Map<String, serde_json::Value>,
}

/// The user's own theme (`ui.look = "own"`): what it brings to new tabs on
/// top of whatever colors, fonts and CRT are set. Choosing it in Settings
/// begins it from the old machine chosen last; a preset file keeps it with
/// the rest of the look. A theme saved under a name has one of its own
/// (`SavedTheme::theme`), and wearing that leaves this one as it is.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeConfig {
    /// Its start screen, a line each: one starting with `^` is centered,
    /// and one starting (after any `^`) with `{tall}` or `{wide}` is double
    /// height or double width. `{version}`, `{os}`, `{chip}`, `{memory}`
    /// ("16 GB"), `{mb}`, `{kb}`, `{bytes}` and `{shell}` stand for what they
    /// say, and teletext's colors (`{red}`, `{on-blue}`, `{plain}`, …) color
    /// what follows (`looks::start_screen`).
    pub start: Vec<String>,
    /// The bits a second it's typed at, ten to a letter; 0 shows it at once.
    pub baud: u32,
    /// The prompt it gives the shells: `\n` a new line, `{cwd}` the folder,
    /// `{n}` the command's number in the history; empty keeps the shell's own.
    pub prompt: String,
    /// What its machine sounded like, as `looks::Sounds::named` reads it:
    /// "quiet", "tube", "tones", "teletype" and the like.
    pub sounds: String,
    /// A shell window's frame round each pane wearing it, the Flow
    /// notanAmiga CLI's (`looks::framed`): begun from the machine it began
    /// from, and switched under Theme.
    pub frame: bool,
}

/// The tabs open when Flow quit, in their order, which the next start
/// opens again where `shell.reopen_tabs` says, each in its own theme and
/// folder. Kept in `tabs.json` beside config.toml, as `updates.json` is:
/// Flow writes it by itself as it quits.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OpenTabs {
    pub tabs: Vec<OpenTab>,
    /// The groups the tabs are in (`tab_groups`), in the order their tabs
    /// come; none written where there are none.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<OpenGroup>,
    /// The one in front.
    pub active: usize,
    /// Written by a Flow that hands over to a new one (Restart after an
    /// update), which reopens them whatever `shell.reopen_tabs` says, as
    /// Restart does.
    pub restart: bool,
}

/// One of [`OpenTabs`]: its shell profile's id and label (the label tells
/// apart profiles that share an id), its theme (`looks::ids`; none,
/// Settings' theme) and the folder its shell was in.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OpenTab {
    pub shell: String,
    pub label: String,
    pub look: Option<String>,
    pub folder: Option<PathBuf>,
    /// Its whole look as Settings made it its own (`looks::tab_part`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub own: Option<serde_json::Value>,
    /// A split tab's other panes, after its first (whose shell and folder
    /// are the tab's), how they share it, and the one in front.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub more: Vec<OpenPane>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub split: Option<crate::panes::Split>,
    #[serde(skip_serializing_if = "is_zero")]
    pub focus: usize,
    /// The name the user gave it; none, its shell's.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Its group, by its place in [`OpenTabs`]' `groups`; none, loose.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<usize>,
    /// Its number among its shell's tabs (`Tab::number`); 0, none kept.
    #[serde(skip_serializing_if = "is_zero")]
    pub number: usize,
}

/// One of [`OpenTabs`]' groups: its name, its color, by the color's name
/// (`tab_groups::COLORS`), and whether it's folded.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OpenGroup {
    pub name: String,
    pub color: String,
    /// Folded shut in the sidebar.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub folded: bool,
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

/// One of a split tab's panes in [`OpenTab`]: its shell, its folder and its
/// theme.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OpenPane {
    pub shell: String,
    pub label: String,
    pub folder: Option<PathBuf>,
    /// Its own theme, as a tab's `look` and `own` say one; none from a Flow
    /// before panes had one, whose panes wore their tab's.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<PaneTheme>,
}

/// A pane's theme in `tabs.json`: as [`OpenTab`]'s `look` and `own`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PaneTheme {
    pub look: Option<String>,
    pub own: Option<serde_json::Value>,
}

impl OpenTabs {
    fn path() -> PathBuf {
        config_dir().join("tabs.json")
    }

    pub fn load() -> Self {
        let mut tabs: Self = std::fs::read_to_string(Self::path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        tabs.fill_in();
        tabs
    }

    /// The tabs' and panes' own looks at Flow's glass from before 0.1.36
    /// take it opaque (`looks::bring_own_glass_up`), at 0.1.36's first
    /// start.
    pub fn bring_glass_up(&mut self) {
        for tab in &mut self.tabs {
            let panes = tab.more.iter_mut().filter_map(|p| p.theme.as_mut()?.own.as_mut());
            for own in tab.own.iter_mut().chain(panes) {
                crate::looks::bring_own_glass_up(own);
            }
        }
    }

    /// Brings the tabs' and panes' own looks up to date, as a config's is
    /// (`looks::fill_in_own_opacity`).
    fn fill_in(&mut self) {
        for tab in &mut self.tabs {
            let panes = tab.more.iter_mut().filter_map(|p| p.theme.as_mut()?.own.as_mut());
            for own in tab.own.iter_mut().chain(panes) {
                crate::looks::fill_in_own_opacity(own);
            }
        }
    }

    pub fn save(&self) {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(err) = serde_json::to_string(self).map_err(|e| e.to_string()).and_then(|t| std::fs::write(&path, t).map_err(|e| e.to_string())) {
            log::warn!("could not write {}: {err}", path.display());
        }
    }

    /// Flow quit with no tab left (the last one closed): the next start
    /// opens one, as it always did.
    pub fn forget() {
        let path = Self::path();
        if path.exists() {
            if let Err(err) = std::fs::remove_file(&path) {
                log::warn!("could not remove {}: {err}", path.display());
            }
        }
    }
}

/// Flow's own checks for a newer release, where it makes them (Linux; the
/// macOS and Windows updaters keep their own settings). Kept in
/// `updates.json` beside config.toml, not in it: Flow writes this by itself
/// once a day, and must never write over the user's config unasked.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdateChecks {
    /// Look once a day without being asked; not set until the user says.
    pub automatic: Option<bool>,
    /// When Flow last looked, in seconds since 1970.
    pub last_check: u64,
    /// The version of Flow that last started here, on every system: the
    /// first start of a newer one reminds the user to check their apps.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub ran: String,
}

impl UpdateChecks {
    fn path() -> PathBuf {
        config_dir().join("updates.json")
    }

    pub fn load() -> Self {
        std::fs::read_to_string(Self::path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    pub fn save(&self) {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(err) = serde_json::to_string(self).map_err(|e| e.to_string()).and_then(|t| std::fs::write(&path, t).map_err(|e| e.to_string())) {
            log::warn!("could not write {}: {err}", path.display());
        }
    }
}

/// Flow's sounds (`sound.rs`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SoundConfig {
    /// Flow's own sounds at all; off until turned on. The bell follows `bell`.
    pub enabled: bool,
    /// 0..1.
    pub volume: f32,
    /// What a program's bell (BEL) does: "system" (the system's alert
    /// sound), "flow" (Flow's own, at `volume`) or "off".
    pub bell: String,
    /// Flow's cues: a long command done while you're elsewhere, a failed
    /// one, the assistant waiting for an answer or done with a task.
    pub cues: bool,
    /// The liquid's effects that programs ask for sound as they look.
    pub effects: bool,
    /// Programs' notes (DECPS) and the sounds they ask for by name.
    pub programs: bool,
    /// The liquid's own: rain on the glass, the clouds' lightning, bubbles
    /// below the water, each quiet.
    pub liquid: bool,
    /// An old machine's own while its look is on: a teletype's clatter, a
    /// tube's hum, its keys' clicks.
    pub machines: bool,
    /// The glass flashes when a program's bell rings, heard or not.
    pub bell_flash: bool,
    /// The old machines' sounds turned off on their own in Theme details,
    /// each as its look and its part: `pc.fan`, `teletype.punch`.
    pub muted: Vec<String>,
}

impl Default for SoundConfig {
    fn default() -> Self {
        Self { enabled: false, volume: 0.6, bell: "system".into(), cues: true, effects: true, programs: true, liquid: true, machines: true, bell_flash: false, muted: Vec::new() }
    }
}

/// The command history (`history.rs`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HistoryConfig {
    /// Keep the commands run in Flow's tabs, to search them.
    pub enabled: bool,
}

impl Default for HistoryConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    /// The interface's language: "auto" (the system's, if Flow has it), or a
    /// code from core/locales ("en", "ja").
    pub language: String,
    /// The logo the first tab shows when Flow starts: "flow", "system" (the
    /// Apple or Windows logo) or "off".
    pub logo: String,
    /// The look last chosen (`looks.rs`): "flow", an old machine's, "own"
    /// (your own theme), or "saved:" and the name of a theme saved under
    /// one (`themes.saved`).
    pub look: String,
    /// An old machine's look also starts each tab with its start screen,
    /// and gives the shells in new tabs its prompt.
    pub look_start: bool,
    pub look_prompt: bool,
    /// And its font, its CRT, and its colors (programs' sixteen among
    /// them), each a switch of Settings' Theme details; off, Flow's.
    pub look_font: bool,
    pub look_crt: bool,
    pub look_colors: bool,
    /// A shell window's frame round each pane, where the look has one (the
    /// Flow notanAmiga CLI's), a switch of Theme details too.
    pub look_frame: bool,
    /// A workstation's start screen shows the computer's own Ethernet
    /// address and serial in place of made-up ones, which identify it, so
    /// it's off unless asked for.
    pub look_real_numbers: bool,
    /// Settings' sections shown closed (`settings::Section::id`), their
    /// settings hidden until opened.
    pub closed_sections: Vec<String>,
    /// Efficient mode, the title bar's switch for real work: the liquid,
    /// the CRT, effects and the theme's sounds paused, and the cursor and
    /// start screens shown at once, over the settings, which stay as set.
    pub efficient: bool,
    /// Efficient mode also comes on by itself while the computer runs on
    /// battery or saves power (the Mac's Low Power Mode, Windows' energy
    /// saver), and goes off when that ends, never changing `efficient`.
    pub efficient_on_battery: bool,
    /// A command that ran at least this many seconds and finished while
    /// Flow wasn't in front posts the system's notification
    /// (`notify::wanted`); 0 never does.
    pub notify_after: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct FontConfig {
    /// Preferred font families, first match wins.
    pub families: Vec<String>,
    pub size: f32,
    pub line_height: f32,
}

/// The terminal's own colors, as "#RRGGBB". Programs that color their output
/// still pick their own; a value that doesn't parse falls back to the default.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ColorConfig {
    /// Text that programs leave uncolored.
    pub text: String,
    pub cursor: String,
    /// The glass's tint, under any liquid.
    pub background: String,
    /// The window's margin round the text, as an 8-bit home computer's
    /// border; empty for none.
    pub border: String,
    /// Programs' sixteen colors (black, red, green, yellow, blue, magenta,
    /// cyan, white, then the bright ones), as an old machine's theme sets
    /// them; empty for Flow's own, as anything but sixteen colors is. Left
    /// out, as in a config from before themes had them, a machine worn
    /// takes its own (`looks::fill_in_colors`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ansi: Option<Vec<String>>,
    /// The one color everything is shown in, as on a monochrome tube (the
    /// green and amber screens'), "#RRGGBB"; empty for full color. Left
    /// out, as in a config from before 0.1.34, a machine worn takes its own
    /// (`looks::fill_in_colors`).
    pub phosphor: Option<String>,
}

/// The terminal's cursor as it starts and as programs reset it. Programs can
/// still change both while they run, as Vim does in insert mode.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CursorConfig {
    /// "block", "underline" or "bar".
    pub shape: String,
    /// It blinks while Flow is in front.
    pub blink: bool,
    /// How fast it glides to its cell: 1 as it always has, 0.5 heavier, up
    /// to 4 quick.
    pub speed: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ShellConfig {
    /// Shell profile id to start with ("cmd", "powershell", "bash", "zsh", ...).
    /// Empty means the platform default.
    pub default: String,
    pub scrollback: usize,
    /// macOS: the Option key sends Esc+key (Meta) instead of typing
    /// characters like ƒ and ©.
    pub option_as_meta: bool,
    /// Programs that ask for the mouse (games, editors, `htop`) get its
    /// clicks, drags and wheel over the terminal; Shift still selects.
    pub mouse_reporting: bool,
    /// A click on the line being typed at the shell's prompt moves the
    /// cursor there, sending the shell ← or → (`Session::click_to_move`).
    pub click_moves_cursor: bool,
    /// A start opens again the tabs open when Flow last quit, each with its
    /// shell, theme and folder ([`OpenTabs`]); Restart always does.
    pub reopen_tabs: bool,
    /// When each pane's scrollbar shows (`scrollbar::MODES`): "scrolling"
    /// (scrolled back, the wheel turning, the pointer at its edge),
    /// "always" where there's a scrollback, or "never".
    pub scrollbar: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AiConfig {
    /// Who answers: anthropic | openai | gemini
    pub provider: String,
    /// Claude's model.
    pub model: String,
    /// GPT's model, for provider "openai".
    pub openai_model: String,
    /// Gemini's model, for provider "gemini".
    pub gemini_model: String,
    /// low | medium | high | xhigh | max
    pub effort: String,
    /// Start in automate mode instead of guide mode.
    pub auto_mode: bool,
    /// Let the assistant search the web for install steps, docs, etc.
    pub web_search: bool,
    /// Claude's server-side refusal fallback (routes a declined request to another model).
    pub fallbacks: bool,
    /// Seconds to wait for a command before handing partial output back to the assistant.
    pub command_timeout_secs: u64,
    /// Claude's API endpoint; `ANTHROPIC_BASE_URL` in the environment overrides it.
    pub base_url: String,
    /// GPT's endpoint, or any server with OpenAI's Responses API; `OPENAI_BASE_URL` overrides it.
    pub openai_base_url: String,
    /// Gemini's endpoint; `GEMINI_BASE_URL` overrides it.
    pub gemini_base_url: String,
    /// Stop asking the assistant once this many tokens are used (0 = no cap).
    pub token_cap: u64,
    /// Keep the assistant's memory: facts it's told to remember and a
    /// journal of its tasks, beside this file, sent with a new conversation.
    pub memory: bool,
    /// Where the assistant's commands run: `yours`, the pane in front while
    /// it's free, else a pane of its own; or `own`, always a pane of its own.
    pub pane: String,
    /// Let the assistant hand parts of a task to helpers.
    pub helpers: bool,
    /// The model its helpers use, of the provider in use; empty for the
    /// assistant's own.
    pub helper_model: String,
    /// Let the assistant ask to work through a task on its own.
    pub own: bool,
    /// The most minutes a task worked on its own may take.
    pub own_minutes: u32,
    /// The most tokens a task worked on its own may use.
    pub own_tokens: u64,
    /// The skills of packages `flowapp` installed that the user turned on
    /// in Settings, each as `<package>:<name>` (`ai::skills`): none at
    /// first, so installing an app never changes what the assistant reads.
    pub skills_on: Vec<String>,
    /// The user's own skills they switched off in Settings, by name: none
    /// at first, so their own are on.
    pub skills_off: Vec<String>,
    /// The MCP servers of `mcp-servers.json` the assistant connects to
    /// (`crate::mcp::client`), by name: none at first, so a server written
    /// into that file by anything else starts nothing until the user
    /// switches it on in Settings.
    pub mcp_servers_on: Vec<String>,
    /// Each of their tools the assistant is offered, as `<server>:<tool>`:
    /// none at first.
    pub mcp_tools_on: Vec<String>,
    /// How each server's calls are approved, by its name (`ask`,
    /// `read_only` or `allowed`; `ask` where it's absent).
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub mcp_approval: std::collections::BTreeMap<String, String>,
    /// The servers whose resources the assistant may read, none at first.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub mcp_resources_on: Vec<String>,
}

impl AiConfig {
    pub fn provider(&self) -> Provider {
        Provider::from_id(&self.provider)
    }

    /// The model of the provider in use.
    pub fn active_model(&self) -> &str {
        match self.provider() {
            Provider::Anthropic => &self.model,
            Provider::OpenAi => &self.openai_model,
            Provider::Gemini => &self.gemini_model,
        }
    }

    /// Picks the model for the provider in use.
    pub fn set_model(&mut self, id: &str) {
        let field = match self.provider() {
            Provider::Anthropic => &mut self.model,
            Provider::OpenAi => &mut self.openai_model,
            Provider::Gemini => &mut self.gemini_model,
        };
        *field = id.to_string();
    }

    /// The configured endpoint of the provider in use (before the
    /// environment's say).
    pub fn base_url(&self) -> &str {
        match self.provider() {
            Provider::Anthropic => &self.base_url,
            Provider::OpenAi => &self.openai_base_url,
            Provider::Gemini => &self.gemini_base_url,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct FxConfig {
    /// Master switch for the liquid simulation.
    pub liquid: bool,
    /// The glass answers what the terminal does: rings and splashes when a
    /// command ends, the progress tank, the liquid leaning toward the
    /// assistant, and tabs that glow when a long command finishes behind them.
    pub reactions: bool,
    /// Programs may set off effects on the glass where they draw (OSC 7731):
    /// lightning, splashes, ripples and pushes of the liquid.
    pub program_effects: bool,
    /// What the liquid is: "clouds", "water", "oil", "rain" or "none".
    pub style: String,
    /// For water: "above" (looking down into it) or "below" (underwater).
    pub water_view: String,
    /// Sunlight on the water, 0..1.
    pub sun: f32,
    /// Lightning in the clouds, or in the rain's storm outside (with heavy
    /// rain), 0..1 (0 is off).
    pub electricity: f32,
    /// How fast the liquid moves, 0.1..2 (1 is normal, lower is calmer).
    pub speed: f32,
    /// 0..1 strength of ambient flow when idle.
    pub ambient: f32,
    /// How hard it rains in the rain style, 0..1: a drizzle up to 0.3,
    /// then rain, heavy rain and from 0.8 a downpour (`fx::rain::rate`).
    pub rain: f32,
    /// How much the liquid surface bends the text (pixels).
    pub text_ripple: f32,
    /// Strength of the flowing light ribbons on the glass, 0..2.
    pub caustics: f32,
    /// Background opacity of the glass, 0..1.
    pub opacity: f32,
    /// How strongly the cursor, tabs, buttons and dialogs glow, 0..2 (1 is
    /// normal, 0 is none).
    pub glow: f32,
    /// The cursor glides along as you type, too. Off, it takes each step of a
    /// few cells along its row at once and glides only further; without a
    /// GPU it always does.
    pub cursor_glide: bool,
    /// An old tube monitor's look over everything, 0..1 (0 is off); its
    /// parts are in [crt].
    pub crt: f32,
    /// Backdrop material: "acrylic", "mica", "blur" or "none" (macOS has
    /// only "blur" and "none").
    pub backdrop: String,
    /// Palette for the liquid dye: "aurora", "ocean", "ember", "mono".
    pub palette: String,
}

/// The parts of the CRT look, each scaled by fx.crt.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CrtConfig {
    /// How dark every other line gets, 0..1 (0.5 is half brightness).
    pub scanlines: f32,
    /// The lit and dim lines swap each frame.
    pub interlace: bool,
    /// The light bright text spreads over the lines, 0..2.
    pub glow: f32,
    /// Seconds the phosphor takes to fade after the picture changes, 0..0.3.
    pub decay: f32,
    /// The stripes of red, green and blue phosphor, 0..1.
    pub grille: f32,
    /// Darker corners, 0..1.
    pub vignette: f32,
    /// How much the picture bulges like a tube's glass, 0..1.
    pub curvature: f32,
    /// Looks the user saved under a name, beside the ones Flow comes with.
    pub presets: Vec<CrtPreset>,
}

/// A whole CRT look under a name: its strength (fx.crt) and its parts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CrtPreset {
    pub name: String,
    pub strength: f32,
    pub scanlines: f32,
    pub interlace: bool,
    pub glow: f32,
    pub decay: f32,
    pub grille: f32,
    pub vignette: f32,
    pub curvature: f32,
}

impl Default for CrtPreset {
    fn default() -> Self {
        CrtConfig::default().preset("", 0.8)
    }
}

impl CrtConfig {
    /// The look as it's set, with `strength`, under `name`.
    pub fn preset(&self, name: &str, strength: f32) -> CrtPreset {
        CrtPreset {
            name: name.to_string(),
            strength,
            scanlines: self.scanlines,
            interlace: self.interlace,
            glow: self.glow,
            decay: self.decay,
            grille: self.grille,
            vignette: self.vignette,
            curvature: self.curvature,
        }
    }

    /// Takes on `p`'s parts; its strength goes to fx.crt.
    pub fn apply(&mut self, p: &CrtPreset) {
        self.scanlines = p.scanlines;
        self.interlace = p.interlace;
        self.glow = p.glow;
        self.decay = p.decay;
        self.grille = p.grille;
        self.vignette = p.vignette;
        self.curvature = p.curvature;
    }
}

impl CrtPreset {
    /// The look set now is this one (whatever its name).
    pub fn matches(&self, crt: &CrtConfig, strength: f32) -> bool {
        let near = |a: f32, b: f32| (a - b).abs() < 1e-3;
        let other = crt.preset(&self.name, strength);
        other.interlace == self.interlace
            && [
                (other.strength, self.strength),
                (other.scanlines, self.scanlines),
                (other.glow, self.glow),
                (other.decay, self.decay),
                (other.grille, self.grille),
                (other.vignette, self.vignette),
                (other.curvature, self.curvature),
            ]
            .iter()
            .all(|&(a, b)| near(a, b))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowConfig {
    pub width: u32,
    pub height: u32,
    /// The most frames a second Flow draws (0 keeps to the display's own
    /// rate, which vsync sets).
    pub refresh_rate: u32,
    /// Screens of the user's own for Fit window to and Fit text to, as
    /// typed in Settings' Your sizes: "132x43, 40x24"
    /// (`settings::parse_sizes`).
    pub sizes: String,
    /// Frames wait for the display's refresh. Without, each is shown as soon
    /// as it's drawn, as often as the GPU manages.
    pub vsync: bool,
    /// A key taken system-wide that brings Flow's window forward, or hides
    /// it while it's in front (`hotkey::Hotkey::parse`: "ctrl+`",
    /// "alt+space", "f12"); empty for none.
    pub hotkey: String,
    /// The hotkey drops the window down over the top of the screen, in
    /// front of every other app's and on every Space, rather than showing
    /// it where it was.
    pub drop_down: bool,
    /// How much of the screen's height the window dropped down takes
    /// (`hotkey::HEIGHTS`: "third", "half", "two_thirds", "all").
    pub drop_down_height: String,
    /// The window slides down from above its screen, and back up as it's
    /// put away, rather than showing and going at once.
    pub drop_down_slide: bool,
    /// The window dropped down is put away once Flow loses the keys to
    /// another app, as the hotkey would.
    pub drop_down_hides: bool,
    /// On the Mac, the window drops down over a full-screen app's Space
    /// too: Flow runs as an accessory app while it's down, without its
    /// Dock icon and menu bar.
    pub drop_down_over_full_screen: bool,
    /// The window stays on top of every other window (not on Wayland,
    /// which has no way for a program to ask it).
    pub on_top: bool,
    /// The tab sidebar pops out over the terminal ("pops_out"), or stays
    /// out beside it, the terminal narrowed for it ("pinned").
    pub tab_sidebar: String,
    /// How wide the tab sidebar is, in points (at most a third of the
    /// window), which dragging its edge changes.
    pub tab_sidebar_width: f32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            ui: UiConfig::default(),
            font: FontConfig::default(),
            colors: ColorConfig::default(),
            cursor: CursorConfig::default(),
            shell: ShellConfig::default(),
            ai: AiConfig::default(),
            fx: FxConfig::default(),
            crt: CrtConfig::default(),
            window: WindowConfig::default(),
            history: HistoryConfig::default(),
            sound: SoundConfig::default(),
            theme: ThemeConfig::default(),
            apps: AppsConfig::default(),
            record: RecordConfig::default(),
            mcp: McpConfig::default(),
            themes: ThemesConfig::default(),
            keys: Default::default(),
        }
    }
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            language: crate::i18n::AUTO.into(),
            logo: "flow".into(),
            look: "flow".into(),
            look_start: true,
            look_prompt: true,
            look_font: true,
            look_crt: true,
            look_colors: true,
            look_frame: true,
            look_real_numbers: false,
            // The CRT's parts and the sounds' are many, and seldom changed.
            closed_sections: vec!["theme_details".into(), "crt".into(), "keys".into(), "apps".into(), "sound".into()],
            efficient: false,
            efficient_on_battery: false,
            notify_after: 30,
        }
    }
}

impl Default for FontConfig {
    fn default() -> Self {
        let families = if cfg!(target_os = "macos") {
            vec!["SF Mono", "Menlo", "Monaco"]
        } else if cfg!(windows) {
            vec!["Cascadia Mono", "Cascadia Code", "Consolas", "Courier New"]
        } else {
            vec!["JetBrains Mono", "DejaVu Sans Mono", "Liberation Mono"]
        };
        Self {
            families: families.into_iter().map(String::from).collect(),
            size: 15.0,
            line_height: 1.25,
        }
    }
}

impl FontConfig {
    /// Puts `family` first, keeping the others behind it as fallbacks. Names
    /// match without regard to case, as they do when the font is picked.
    pub fn prefer(&mut self, family: &str) {
        let want = family.to_lowercase();
        self.families.retain(|f| f.to_lowercase() != want);
        self.families.insert(0, family.to_string());
    }
}

impl Default for ColorConfig {
    fn default() -> Self {
        Self {
            text: palette::hex(palette::FOREGROUND),
            cursor: palette::hex(palette::CURSOR),
            background: palette::hex(palette::BACKGROUND),
            border: String::new(),
            ansi: None,
            phosphor: None,
        }
    }
}

impl ColorConfig {
    /// The colors to draw with.
    pub fn ink(&self) -> Ink {
        Ink {
            text: palette::parse_hex(&self.text).unwrap_or(palette::FOREGROUND),
            cursor: palette::parse_hex(&self.cursor).unwrap_or(palette::CURSOR),
            background: palette::parse_hex(&self.background).unwrap_or(palette::BACKGROUND),
            border: palette::parse_hex(&self.border),
            ansi: self.ansi.iter().flatten().map(|c| palette::parse_hex(c)).collect::<Option<Vec<_>>>().and_then(|colors| colors.try_into().ok()),
            phosphor: self.phosphor.as_deref().and_then(palette::parse_hex),
        }
    }
}

impl Default for CursorConfig {
    fn default() -> Self {
        Self { shape: "block".into(), blink: false, speed: 1.0 }
    }
}

impl CursorConfig {
    /// The style the terminal starts with and programs reset to.
    pub fn style(&self) -> CursorStyle {
        let shape = match self.shape.as_str() {
            "underline" => CursorShape::Underline,
            "bar" => CursorShape::Beam,
            _ => CursorShape::Block,
        };
        CursorStyle { shape, blinking: self.blink }
    }
}

impl Default for ShellConfig {
    fn default() -> Self {
        Self { default: String::new(), scrollback: 10_000, option_as_meta: false, mouse_reporting: true, click_moves_cursor: true, reopen_tabs: true, scrollbar: "scrolling".into() }
    }
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            provider: Provider::Anthropic.id().into(),
            model: Provider::Anthropic.default_model().into(),
            openai_model: Provider::OpenAi.default_model().into(),
            gemini_model: Provider::Gemini.default_model().into(),
            effort: "medium".into(),
            auto_mode: false,
            web_search: true,
            fallbacks: true,
            command_timeout_secs: 120,
            base_url: Provider::Anthropic.default_base_url().into(),
            openai_base_url: Provider::OpenAi.default_base_url().into(),
            gemini_base_url: Provider::Gemini.default_base_url().into(),
            token_cap: 0,
            memory: true,
            pane: "yours".into(),
            helpers: true,
            helper_model: String::new(),
            own: true,
            own_minutes: 60,
            own_tokens: 500_000,
            skills_on: Vec::new(),
            skills_off: Vec::new(),
            mcp_servers_on: Vec::new(),
            mcp_tools_on: Vec::new(),
            mcp_approval: std::collections::BTreeMap::new(),
            mcp_resources_on: Vec::new(),
        }
    }
}

impl Default for FxConfig {
    fn default() -> Self {
        Self {
            liquid: true,
            reactions: true,
            program_effects: true,
            style: "clouds".into(),
            water_view: "above".into(),
            sun: 0.8,
            electricity: 0.0,
            speed: 1.0,
            ambient: 0.6,
            rain: 0.15,
            text_ripple: 2.5,
            caustics: 1.0,
            // Opaque: 55% let the desktop through on Windows and Linux
            // until the text was hard to read. Glass opacity dials it back.
            opacity: 1.0,
            glow: 1.0,
            cursor_glide: true,
            crt: 0.0,
            backdrop: if cfg!(windows) { "acrylic" } else { "blur" }.into(),
            palette: "aurora".into(),
        }
    }
}

impl Default for CrtConfig {
    fn default() -> Self {
        Self {
            scanlines: 0.5,
            interlace: true,
            glow: 1.2,
            decay: 0.06,
            grille: 0.4,
            vignette: 0.6,
            curvature: 0.0,
            presets: Vec::new(),
        }
    }
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            width: 1100,
            height: 700,
            refresh_rate: 0,
            sizes: String::new(),
            vsync: true,
            hotkey: String::new(),
            drop_down: true,
            drop_down_height: "half".into(),
            drop_down_slide: true,
            drop_down_hides: false,
            drop_down_over_full_screen: false,
            on_top: false,
            tab_sidebar: crate::tab_sidebar::POPS_OUT.into(),
            tab_sidebar_width: crate::tab_sidebar::WIDTH,
        }
    }
}

/// The config's sections a preset file holds: how Flow looks, not what it
/// does (the assistant, the shells, the language and the window stay). The
/// themes saved under a name aren't among them: a preset file holds one
/// look, which may be one of them.
pub const PRESET_SECTIONS: [&str; 6] = ["fx", "crt", "colors", "cursor", "font", "theme"];

/// Why a preset file couldn't be loaded.
#[derive(Debug, PartialEq)]
pub enum PresetError {
    /// It isn't JSON, or a setting in it has the wrong kind of value.
    Invalid(String),
    /// It's JSON, but sets nothing a preset holds.
    Empty,
}

impl Config {
    /// The start screen, prompt and sounds the theme chosen brings where it
    /// has its own: a theme saved under a name while one kept is chosen
    /// (`ui.look`), else your own (`[theme]`).
    pub fn worn_theme(&self) -> &ThemeConfig {
        match crate::looks::saved_name(&self.ui.look).and_then(|name| self.themes.named(name)) {
            Some(saved) => &saved.theme,
            None => &self.theme,
        }
    }

    /// [`worn_theme`](Self::worn_theme), to change: Settings' rows under
    /// Theme type into the saved theme chosen, leaving your own as it is.
    pub fn worn_theme_mut(&mut self) -> &mut ThemeConfig {
        let at = crate::looks::saved_name(&self.ui.look).and_then(|name| self.themes.saved.iter().position(|t| t.name == name));
        match at {
            Some(i) => &mut self.themes.saved[i].theme,
            None => &mut self.theme,
        }
    }

    /// The look as a preset file: the [`PRESET_SECTIONS`] as JSON, by the
    /// same names as in config.toml, without the CRT looks saved under a
    /// name. Its `theme` is the saved theme's while one is chosen, so a
    /// preset file moves that one whole to another computer.
    pub fn preset_json(&self) -> String {
        let mut all = serde_json::to_value(self).unwrap_or_default();
        if let (Some(all), Ok(theme)) = (all.as_object_mut(), serde_json::to_value(self.worn_theme())) {
            all.insert("theme".into(), theme);
        }
        let mut preset = serde_json::Map::new();
        for key in PRESET_SECTIONS {
            if let Some(section) = all.get(key) {
                preset.insert(key.to_string(), tidy(section.clone()));
            }
        }
        if let Some(crt) = preset.get_mut("crt").and_then(|c| c.as_object_mut()) {
            crt.remove("presets");
        }
        let mut json = serde_json::to_string_pretty(&preset).unwrap_or_default();
        json.push('\n');
        json
    }

    /// Takes on the settings a preset file names and keeps the rest, so a
    /// file edited by hand can hold just a few. Nothing changes if one of
    /// them doesn't fit.
    pub fn apply_preset_json(&mut self, json: &str) -> Result<(), PresetError> {
        let invalid = |e: serde_json::Error| PresetError::Invalid(e.to_string());
        let preset: serde_json::Value = serde_json::from_str(json).map_err(invalid)?;
        let mut all = serde_json::to_value(&*self).map_err(invalid)?;
        let mut found = false;
        for key in PRESET_SECTIONS {
            let (Some(serde_json::Value::Object(part)), Some(serde_json::Value::Object(section))) = (preset.get(key), all.get_mut(key))
            else {
                continue;
            };
            for (name, value) in part.iter().filter(|(name, _)| !(key == "crt" && *name == "presets")) {
                section.insert(name.clone(), value.clone());
                found = true;
            }
        }
        if !found {
            return Err(PresetError::Empty);
        }
        *self = serde_json::from_value(all).map_err(invalid)?;
        // A preset with a start screen or a prompt of its own is a whole
        // theme: new tabs take it on.
        let theme = preset.get("theme").and_then(|t| t.as_object());
        if theme.is_some_and(|t| t.get("start").and_then(|s| s.as_array()).is_some_and(|s| !s.is_empty()) || t.get("prompt").and_then(|p| p.as_str()).is_some_and(|p| !p.is_empty())) {
            self.ui.look = crate::looks::OWN.into();
        }
        Ok(())
    }
}

/// Writes each fraction as its settings hold it (`0.55`, not the
/// `0.550000011920929` a 32-bit float becomes in JSON), for editing by hand.
pub(crate) fn tidy(value: serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match value {
        Value::Number(n) if n.is_f64() => {
            let short = n.as_f64().map(|f| (f as f32).to_string().parse::<f64>().unwrap_or(f));
            short.and_then(serde_json::Number::from_f64).map_or(Value::Number(n), Value::Number)
        }
        Value::Array(items) => Value::Array(items.into_iter().map(tidy).collect()),
        Value::Object(map) => Value::Object(map.into_iter().map(|(k, v)| (k, tidy(v))).collect()),
        other => other,
    }
}

/// Flow's folder of its own: its config, its tabs, its history, its apps.
/// On Windows that's in `APPDATA` where it's set, which names the same
/// folder as the system's (`dirs::config_dir`, the user's known folder)
/// unless it was set to another, as the Mac's and Linux's follow `HOME`
/// and `XDG_CONFIG_HOME`: so a Flow started with a scratch `APPDATA`, a
/// test copy, keeps everything of its own there, and so do the programs
/// in its tabs, `flowapp` and `flow-settings` among them.
pub fn config_dir() -> PathBuf {
    let set = std::env::var_os("APPDATA").map(PathBuf::from).filter(|dir| cfg!(windows) && dir.is_absolute());
    set.or_else(dirs::config_dir).unwrap_or_else(std::env::temp_dir).join(APP_ID)
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.toml")
}

impl Config {
    /// Loads the config file, writing a commented default on first run.
    pub fn load() -> Self {
        let path = config_path();
        match std::fs::read_to_string(&path) {
            Ok(text) => match toml::from_str(&text) {
                Ok(mut cfg) => {
                    crate::looks::fill_in_colors(&mut cfg);
                    crate::looks::fill_in_fonts(&mut cfg);
                    crate::looks::fill_in_opacity(&mut cfg);
                    cfg
                }
                Err(err) => {
                    log::warn!("ignoring invalid config {}: {err}", path.display());
                    Self::default()
                }
            },
            Err(_) => {
                let cfg = Self::default();
                cfg.save();
                cfg
            }
        }
    }

    /// Reads the config file again, as a restart does. A file that's gone
    /// or doesn't read is an error, and the settings in use stay.
    pub fn reread() -> Result<Self, String> {
        let text = std::fs::read_to_string(config_path()).map_err(|e| e.to_string())?;
        let mut cfg: Self = toml::from_str(&text).map_err(|e| e.to_string())?;
        crate::looks::fill_in_colors(&mut cfg);
        crate::looks::fill_in_fonts(&mut cfg);
        crate::looks::fill_in_opacity(&mut cfg);
        Ok(cfg)
    }

    pub fn save(&self) {
        let path = config_path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        match toml::to_string_pretty(self) {
            Ok(text) => {
                if let Err(err) = std::fs::write(&path, text) {
                    log::warn!("could not write config {}: {err}", path.display());
                }
            }
            Err(err) => log::warn!("could not serialize config: {err}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_tabs_and_panes_wearing_a_machine_take_its_glass_opaque_as_theyre_read() {
        let mut worn = Config::default();
        crate::looks::find("teletype").apply(&mut worn);
        worn.fx.opacity = 0.96;
        let own = crate::looks::tab_part(&worn);
        let pane = OpenPane { theme: Some(PaneTheme { look: Some("teletype".into()), own: Some(own.clone()) }), ..Default::default() };
        let mut tabs = OpenTabs { tabs: vec![OpenTab { own: Some(own), more: vec![pane], ..Default::default() }], ..Default::default() };
        tabs.fill_in();
        let opacity = |own: &Option<serde_json::Value>| own.as_ref().and_then(|o| o["fx"]["opacity"].as_f64());
        assert_eq!(opacity(&tabs.tabs[0].own), Some(1.0));
        assert_eq!(opacity(&tabs.tabs[0].more[0].theme.as_ref().unwrap().own), Some(1.0));
    }

    fn fonts(families: &[&str]) -> FontConfig {
        FontConfig { families: families.iter().map(|f| f.to_string()).collect(), ..FontConfig::default() }
    }

    #[test]
    fn a_new_config_notifies_after_half_a_minute_and_takes_no_hotkey() {
        let config = Config::default();
        assert_eq!(config.ui.notify_after, 30);
        assert_eq!(config.window.hotkey, "");
        assert!(config.window.drop_down, "a hotkey once set drops the window down");
        // Over half the screen, sliding, staying when Flow loses the keys,
        // and on the Mac as a Regular app.
        let w = &config.window;
        assert_eq!((w.drop_down_height.as_str(), w.drop_down_slide, w.drop_down_hides, w.drop_down_over_full_screen), ("half", true, false, false));
        // A config from before them gets the same.
        let old: Config = toml::from_str("[ui]\nlanguage = \"en\"\n[window]\nwidth = 900\n").unwrap();
        assert_eq!((old.ui.notify_after, old.window.hotkey.as_str(), old.window.drop_down), (30, "", true));
        let w = &old.window;
        assert_eq!((w.drop_down_height.as_str(), w.drop_down_slide, w.drop_down_hides, w.drop_down_over_full_screen), ("half", true, false, false));
    }

    #[test]
    fn a_picked_font_goes_first_and_the_rest_stay_as_fallbacks() {
        let mut f = fonts(&["SF Mono", "Menlo", "Monaco"]);
        f.prefer("PT Mono");
        assert_eq!(f.families, ["PT Mono", "SF Mono", "Menlo", "Monaco"]);
    }

    #[test]
    fn picking_a_listed_font_moves_it_without_a_duplicate() {
        let mut f = fonts(&["SF Mono", "Menlo", "Monaco"]);
        f.prefer("menlo");
        assert_eq!(f.families, ["menlo", "SF Mono", "Monaco"]);
        f.prefer("Menlo");
        assert_eq!(f.families, ["Menlo", "SF Mono", "Monaco"]);
    }

    #[test]
    fn the_cursor_settings_become_the_terminals_own_style() {
        let style = |shape: &str, blink| CursorConfig { shape: shape.into(), blink, ..CursorConfig::default() }.style();
        assert_eq!(style("block", false), CursorStyle { shape: CursorShape::Block, blinking: false });
        assert_eq!(style("underline", true), CursorStyle { shape: CursorShape::Underline, blinking: true });
        assert_eq!(style("bar", false).shape, CursorShape::Beam);
        // A shape Flow doesn't know is a block, as it was before there was a choice.
        assert_eq!(style("triangle", false).shape, CursorShape::Block);
    }

    #[test]
    fn a_preset_holds_the_look_and_nothing_else() {
        let mut config = Config::default();
        config.crt.presets.push(CrtPreset { name: "Mine".into(), ..CrtPreset::default() });
        config.fx.opacity = 0.55;
        let json = config.preset_json();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        let mut keys: Vec<&str> = value.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort();
        assert_eq!(keys, ["colors", "crt", "cursor", "font", "fx", "theme"]);
        assert!(value["crt"].get("presets").is_none());
        // Fractions read as they're set, for editing by hand.
        assert!(json.contains("\"opacity\": 0.55"), "{json}");
        let mut back = Config::default();
        back.apply_preset_json(&json).unwrap();
        assert_eq!(back.preset_json(), json);
    }

    #[test]
    fn a_preset_with_a_start_screen_or_prompt_is_a_whole_theme_new_tabs_take_on() {
        let mut config = Config::default();
        config.apply_preset_json(r##"{ "colors": { "text": "#FFFFFF" } }"##).unwrap();
        assert_eq!(config.ui.look, "flow", "only colors: the look chosen stays");
        config.apply_preset_json(r#"{ "theme": { "start": ["MY MACHINE", ""], "prompt": "> ", "sounds": "tube" } }"#).unwrap();
        assert_eq!(config.ui.look, crate::looks::OWN);
        assert_eq!((config.theme.start.len(), config.theme.prompt.as_str(), config.theme.sounds.as_str()), (2, "> ", "tube"));
        // Saved again, it comes back whole.
        let mut back = Config::default();
        back.apply_preset_json(&config.preset_json()).unwrap();
        assert_eq!(back.theme, config.theme);
    }

    #[test]
    fn a_preset_saved_while_a_saved_theme_is_worn_holds_that_ones_start_screen_and_prompt() {
        let green = crate::looks::find("green").as_theme();
        let mut config = Config::default();
        config.theme.prompt = "mine> ".into();
        config.themes.save(crate::looks::save_as(&crate::looks::wear(&Config::default(), "green"), "Night"));
        let preset = |c: &Config| serde_json::from_str::<serde_json::Value>(&c.preset_json()).unwrap()["theme"].clone();
        assert_eq!(preset(&config)["prompt"], serde_json::json!("mine> "), "your own, under any other");
        let worn = crate::looks::wear(&config, "saved:Night");
        assert_eq!(preset(&worn), serde_json::to_value(&green).unwrap());
        // Loaded on another computer, it's your own theme there, whole.
        let mut there = Config::default();
        there.apply_preset_json(&worn.preset_json()).unwrap();
        assert_eq!((there.ui.look.as_str(), &there.theme), (crate::looks::OWN, &green));
    }

    #[test]
    fn a_preset_changes_only_what_it_names() {
        let mut config = Config::default();
        config.ai.model = "mine".into();
        config.crt.presets.push(CrtPreset { name: "Mine".into(), ..CrtPreset::default() });
        config.apply_preset_json(r#"{ "fx": { "style": "rain" }, "cursor": { "shape": "bar" }, "crt": { "presets": [] } }"#).unwrap();
        assert_eq!((config.fx.style.as_str(), config.cursor.shape.as_str()), ("rain", "bar"));
        assert_eq!(config.fx.opacity, FxConfig::default().opacity);
        assert_eq!((config.ai.model.as_str(), config.crt.presets.len()), ("mine", 1));
    }

    #[test]
    fn a_preset_that_doesnt_fit_changes_nothing() {
        let mut config = Config::default();
        let err = config.apply_preset_json(r#"{ "fx": { "style": "rain", "opacity": "lots" } }"#);
        assert!(matches!(err, Err(PresetError::Invalid(_))), "{err:?}");
        assert_eq!(config.fx.style, FxConfig::default().style);
        assert_eq!(config.apply_preset_json("{ not json"), Err(PresetError::Invalid("key must be a string at line 1 column 3".into())));
        assert_eq!(config.apply_preset_json(r#"{ "ai": { "model": "x" }, "fx": 3 }"#), Err(PresetError::Empty));
    }

    #[test]
    fn saved_themes_are_written_as_tables_of_the_looks_sections_and_read_back() {
        // None: no section at all, and a config without one reads none.
        let config = Config::default();
        let text = toml::to_string_pretty(&config).unwrap();
        assert!(!text.contains("themes"), "{text}");
        assert!(toml::from_str::<Config>(&text).unwrap().themes.is_empty());
        // Two, each a name and its sections, as a preset file has them.
        let mut config = Config::default();
        config.fx.style = "oil".into();
        config.theme.prompt = "> ".into();
        config.ui.look = crate::looks::OWN.into();
        config.themes.save(crate::looks::save_as(&config, "Oil slick"));
        config.themes.save(crate::looks::save_as(&Config::default(), "Plain"));
        let text = toml::to_string_pretty(&config).unwrap();
        assert!(text.contains("[[themes.saved]]\nname = \"Oil slick\"") && text.contains("[themes.saved.fx]") && text.contains("[themes.saved.theme]"), "{text}");
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(back.themes, config.themes);
        assert_eq!(back.themes.named("Oil slick").unwrap().look["fx"]["style"], serde_json::json!("oil"));
        assert_eq!(back.themes.named("Oil slick").unwrap().theme.prompt, "> ");
        assert_eq!(back.themes.named("Oil slick").unwrap().look["fx"]["opacity"], serde_json::json!(1.0));
        // One written by hand with a single setting reads as that.
        let back: Config = toml::from_str("[[themes.saved]]\nname = \"Few\"\n[themes.saved.colors]\ntext = \"#FFFFFF\"\n").unwrap();
        assert_eq!(back.themes.saved.len(), 1);
        assert_eq!(back.themes.saved[0].look, serde_json::from_str::<serde_json::Map<_, _>>(r##"{ "colors": { "text": "#FFFFFF" } }"##).unwrap());
    }

    #[test]
    fn a_theme_saved_under_a_name_taken_saves_over_it_and_deleting_removes_it() {
        let mut themes = ThemesConfig::default();
        let theme = |name: &str, style: &str| {
            let mut c = Config::default();
            c.fx.style = style.into();
            crate::looks::save_as(&c, name)
        };
        assert!(!themes.save(theme("Mine", "oil")));
        assert!(!themes.save(theme("Yours", "rain")));
        assert!(themes.save(theme("Mine", "water")), "a name taken is saved over");
        assert_eq!(themes.saved.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["Mine", "Yours"], "in its place");
        assert_eq!(themes.named("Mine").unwrap().look["fx"]["style"], serde_json::json!("water"));
        assert!(themes.delete("Mine") && !themes.delete("Mine"));
        assert_eq!(themes.saved.len(), 1);
    }

    #[test]
    fn picking_into_an_empty_list_leaves_just_that_font() {
        let mut f = fonts(&[]);
        f.prefer("Courier New");
        assert_eq!(f.families, ["Courier New"]);
    }
}
