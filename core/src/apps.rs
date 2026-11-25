//! Flow's apps, as `flowapp` keeps them: programs built on Flow's SDK that
//! run in its tabs (Flow Rogue, Flow Mines, Flow Invaders, the SDK itself
//! with its demos), listed in a catalog and kept up to date as Homebrew
//! keeps its formulae. `flowapp update` reads the catalog, `upgrade`
//! installs what's newer than what's installed, `install` and `remove` take
//! one on or off.
//!
//! The catalog is `apps.json` in Flow-releases, beside the appcasts
//! ([`CATALOG_URL`]; `FLOWAPP_CATALOG` names another, a URL or a file): each
//! app's version, a line about it, the programs it puts on the tabs' PATH,
//! and for each system and chip a download (a `.tar.gz`) with its size,
//! SHA-256 and Ed25519 signature by Flow's update key
//! ([`crate::UPDATE_PUBLIC_KEY`]), so a catalog from anywhere can name only
//! what Flow signed. A download is checked against all three before
//! anything is unpacked. One that needs a newer Flow than this one says so
//! (its `flow`, from 0.1.34): it's held back, never installed, until Flow
//! is updated ([`Download::needs_newer_flow`]).
//!
//! What the catalog last read said is kept in `apps/catalog.json`, which
//! `list`, `outdated`, `info` and `search` go by, so they're quick and work
//! offline. `install` and `upgrade` read it again first ([`catalog_afresh`]):
//! a copy saved before a release can name downloads that are gone, since a
//! release takes the place of the one before it.
//!
//! Installed apps live beside Flow's config, in `apps/` ([`dir`]): each
//! version unpacked in `apps/<name>/<version>/` (its `bin/`, and for the SDK
//! `lib/`, `include/` and `python/`), its programs copied into `apps/bin/`
//! ([`bin_dir`]), which Flow puts on every tab's PATH ahead of the folder
//! of its own programs (`shell.rs`), and what's installed in
//! `installed.json`. A program is put in place by renaming, so a tab
//! running the old one keeps it, and one that's open runs the new one next
//! time. Flow's releases carried Flow Rogue and Flow Mines until 0.1.33
//! (the catalog's `bundled`, a program whose `--version` says it): a copy
//! an older Flow left counts as installed at its own version, and the first
//! start after an update from one installs them here ([`keep_carried`]),
//! since from 0.1.34 Flow carries none and an update mustn't take a game
//! away.
//!
//! How `flowapp` prints what it finds, tables in a terminal and plain lines
//! for a script, is [`table`]'s; what Settings' Apps section shows of them
//! is [`shelf`]'s; and what `flowapp first` puts ahead of the system's
//! commands in Flow's tabs, and how each tab is told, [`first`]'s. Who
//! signed a download, with which key, is [`signing`]'s, and `flowapp
//! verify`'s look at what's installed against its download [`verify`]'s.
//! flowapp is an app of the catalog too, its own copy running in Flow's
//! tabs ahead of the one Flow carries: finding Flow's own folder and the
//! Flow it runs in from there, and which of the two copies runs, is
//! [`itself`]'s.

pub mod first;
pub mod itself;
pub mod shelf;
pub mod signing;
pub mod table;
pub mod verify;

use std::collections::BTreeMap;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Where the catalog is read from, unless `FLOWAPP_CATALOG` says otherwise.
pub const CATALOG_URL: &str = "https://raw.githubusercontent.com/arcanii/Flow-releases/main/apps.json";

/// The catalog's format: a catalog of another is refused rather than misread.
pub const FORMAT: u32 = 1;

/// The most a download may be, which a catalog's `size` can't raise.
const MOST: u64 = 512 * 1024 * 1024;

/// The most the catalog may be.
const CATALOG_MOST: u64 = 4 * 1024 * 1024;

/// How old the saved catalog may be before `list`, `outdated` and
/// `search` say so in a terminal, as Homebrew reads its own again after a
/// day.
pub const STALE: Duration = Duration::from_secs(24 * 60 * 60);

/// The apps there are, as the catalog lists them, and the theme packages
/// (from 0.1.34; a `flowapp` before reads past them).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Catalog {
    pub format: u32,
    pub apps: BTreeMap<String, App>,
    /// Packages of themes (`looks::packaged`), each an app with no
    /// programs whose download, the same for every system, is [`ANY`]'s.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub themes: BTreeMap<String, App>,
    /// Packages of skills for the assistant (`ai::skills`), each an app
    /// with no programs whose download is [`ANY`]'s, as a theme package's
    /// is (from 0.1.42; a `flowapp` before reads past them).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub skills: BTreeMap<String, App>,
}

/// The download a theme package has for every system and chip.
pub const ANY: &str = "any";

/// Names no app or package may have: flowapp's own folders beside theirs
/// (`bin/`, `first/` with each pane's under it, the gallery's `previews/`),
/// which removing it would empty. Its own files (`installed.json`,
/// `catalog.json`, `carried.json`, `first.json`) all have a dot, which no
/// name may.
const RESERVED: [&str; 3] = ["bin", "first", "previews"];

/// Names Windows keeps for its devices, whatever follows them after a dot,
/// in any case: a folder can't be called one there. A name or version is
/// refused on every system, so a package of one's own moves between them.
const DEVICES: [&str; 24] = ["con", "prn", "aux", "nul", "com0", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8", "com9", "lpt0", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9"];

/// Whether `name` is one an app or package can have, as the folder it's
/// unpacked in: lowercase ASCII letters, digits, `-` and `_`, from a letter
/// or a digit, at most 64, never one of flowapp's own folders nor a
/// device's. So it can't climb out of the apps' folder, name a drive (`C:`
/// on Windows, where `Path::join` would put it in place of the folder),
/// lose a trailing dot or space (Windows drops them), print an escape, or
/// read as an option; and lowercase, two names a Mac's or Windows' file
/// system takes for one folder (`Bin` and `bin`, `Flow-Rogue` and
/// `flow-rogue`) can't both be. The catalog's names are held to it too
/// (`examples/app_catalog.rs` checks them before it writes one).
pub fn fit_name(name: &str) -> bool {
    let first = name.chars().next().is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
    first && name.len() <= 64 && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_') && !RESERVED.contains(&name) && !DEVICES.contains(&name)
}

/// Whether `version` can be a version's folder: ASCII letters, digits,
/// `.`, `-`, `_` and `+` (`0.1.45`, `1.0-RC1+2`), at most 64, as Flow's
/// SDK makes them, but neither starting nor ending with a dot (so never
/// `..`, and never one Windows would take for the version without its
/// dot), nor a device's name before its first dot: never a path or a
/// drive. Two that differ only in case are one folder on a Mac or
/// Windows, which installing heeds (`install_kept`).
pub fn fit_version(version: &str) -> bool {
    let device = version.split('.').next().is_some_and(|stem| DEVICES.contains(&stem.to_ascii_lowercase().as_str()));
    let fine = !version.is_empty() && version.len() <= 64 && version.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+'));
    fine && !version.starts_with('.') && !version.ends_with('.') && !device
}

/// Whether `program` can be the name of a program an app puts in `bin/`:
/// a plain file's name there, never a path to one elsewhere (`../x`,
/// `C:x`), a hidden one (flowapp's own `.x.new` and `.x.old` are), a
/// device's on Windows (`con.exe` is the console there), nor one that
/// prints an escape.
pub fn fit_program(program: &str) -> bool {
    let device = program.split('.').next().is_some_and(|stem| DEVICES.contains(&stem.to_ascii_lowercase().as_str()));
    !program.is_empty() && !program.starts_with('.') && !program.contains(['/', '\\', ':']) && !program.chars().any(char::is_control) && !device
}

/// `word`, a name or version that was refused, as it can be printed: its
/// control characters spelled out, so it can't send the terminal an escape.
fn printable(word: &str) -> String {
    word.escape_debug().to_string()
}

/// `text`, a message that may hold what a file of anyone's says (a theme's
/// fields in an error), as a terminal can be given it: every control
/// character but a new line and a tab spelled out as `\u{1b}`, so none can
/// send it an escape (OSC 7732 would change Flow's settings), the rest as
/// it is.
pub fn plain(text: &str) -> String {
    text.chars().map(|c| if c.is_control() && !matches!(c, '\n' | '\t') { c.escape_debug().to_string() } else { c.to_string() }).collect()
}

/// Why `name` can't be installed over what's `installed` now, where it
/// can't. `own_file`: it's a package of the user's own from a file, else
/// the catalog's; `catalog`, the one flowapp last read, tells a package of
/// the user's own from before they were marked ([`Kept::own`]). The
/// catalog's never takes the place of a package of the user's own, which
/// would delete it without their word: they remove theirs first. A package
/// of the user's own never takes the place of one installed from the
/// catalog, whose name is Flow's. Nor does either share a name but for its
/// case with something installed already (from before names were
/// checked), whose folder it would be on a Mac or Windows, where nothing
/// is installed by its very name.
pub fn in_the_way(installed: &Installed, name: &str, own_file: bool, catalog: Option<&Catalog>) -> Option<String> {
    let Some(kept) = installed.apps.get(name) else {
        let (there, _) = installed.apps.iter().find(|(there, _)| there.eq_ignore_ascii_case(name))?;
        return Some(crate::i18n::tf("flowapp.name_taken", &[("name", &printable(name)), ("other", &printable(there))]));
    };
    match (own_file, kept.own(name, catalog)) {
        (false, true) => Some(crate::i18n::tf("flowapp.own_in_the_way", &[("name", &name)])),
        (true, false) => Some(crate::i18n::tf("flowapp.catalogs_in_the_way", &[("name", &name)])),
        _ => None,
    }
}

/// Why a package of the user's own can't be called `name`, where it
/// can't: it's no name an app can have ([`fit_name`]); or it's Flow's,
/// flowapp's own or one the `catalog` lists, any case, as an app, a
/// package of themes or one of skills, so its install would take that
/// one's place and its upgrades and `verify` would go by the wrong
/// download. `catalog` is the one flowapp last read, none where it never
/// has, when only flowapp's own name and what's installed can be known.
fn own_name_refused(name: &str, catalog: Option<&Catalog>) -> Option<String> {
    if !fit_name(name) {
        return Some(crate::i18n::tf("flowapp.bad_name", &[("name", &printable(name))]));
    }
    let flows = name == itself::NAME || catalog.is_some_and(|c| c.apps.keys().chain(c.themes.keys()).chain(c.skills.keys()).any(|n| n.eq_ignore_ascii_case(name)));
    flows.then(|| crate::i18n::tf("flowapp.name_is_flows", &[("name", &name)]))
}

/// Why a package of the user's own called `name`, its themes as `file`
/// says them, can't be installed under `root`, where it can't: a theme of
/// it has the id of one of Flow's own looks, or of one another package
/// installed brings, whose place it would take (shown by that one's name,
/// on the tabs already wearing it), so anyone's file could put its words
/// in a theme the user chose. `packaged::installed` leaves such a theme
/// out too, should one be there from before.
fn own_themes_refused(root: &Path, name: &str, file: &crate::looks::packaged::ThemeFile) -> Option<String> {
    let flows = file.themes.iter().find(|t| crate::looks::LOOKS.iter().any(|l| l.id == t.id));
    if let Some(theme) = flows {
        return Some(crate::i18n::tf("flowapp.theme_is_flows", &[("id", &theme.id)]));
    }
    let installed = Installed::load(root);
    for (other, kept) in installed.apps.iter().filter(|(other, kept)| kept.themes() && *other != name) {
        let Ok(theirs) = crate::looks::packaged::peek(&app_dir(root, other, &kept.version)) else { continue };
        if let Some(theme) = file.themes.iter().find(|t| theirs.themes.iter().any(|o| o.id == t.id)) {
            return Some(crate::i18n::tf("flowapp.theme_taken", &[("id", &theme.id), ("other", &printable(other))]));
        }
    }
    None
}

/// What `installed.json` calls a theme package (`Kept::kind`).
pub const THEMES: &str = "themes";
/// What it calls a package of nothing but skills.
pub const SKILLS: &str = "skills";

/// One app: its newest version anywhere, a line about it, the programs it
/// puts on the tabs' PATH, the one of them Flow's releases carry where they
/// carry it (whose `--version` is the app's), and a download for each
/// system and chip (`macos-aarch64`, [`platform`]), each of its own version,
/// since each system's are made on its own computer.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct App {
    pub version: String,
    pub summary: String,
    pub programs: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bundled: Option<String>,
    pub downloads: BTreeMap<String, Download>,
}

/// A `.tar.gz` of an app for one system and chip, and what it must be.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Download {
    pub version: String,
    pub url: String,
    pub size: u64,
    /// Hex.
    pub sha256: String,
    /// Base64, Ed25519 by Flow's update key over the whole file.
    pub signature: String,
    /// The oldest Flow it works in, as About shows Flow's version
    /// (`0.1.34`, or a patch's `0.1.31.3`), where it needs something a
    /// Flow before it lacks; none where any Flow with `flowapp` will do. A
    /// flowapp before 0.1.34 reads past it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub flow: String,
    /// When it was signed, a UTC date (`2026-11-18`), as `app_catalog`
    /// wrote it adding the download: the catalog's word, which nothing
    /// signs. None for a download added before it did. A flowapp before
    /// reads past it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub signed: String,
}

/// What `flowapp` has installed, kept in `installed.json`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Installed {
    pub apps: BTreeMap<String, Kept>,
}

/// An installed app: its version, and the programs it put in `bin/`; or a
/// theme package (`kind` [`THEMES`]) or a package of skills ([`SKILLS`]),
/// which put none there. With the download it came from and who signed
/// it, which a `flowapp` before kept none of, and drops if it writes the
/// file again.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Kept {
    pub version: String,
    pub programs: Vec<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub kind: String,
    /// The catalog's download it was installed from, which `flowapp
    /// verify` fetches again.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub download: Option<Download>,
    /// The fingerprint of the key that checked it ([`signing::fingerprint`]).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub key: String,
    /// When it was checked, a UTC date.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub checked: String,
    /// Installed from a file of the user's own ([`install_file`]), so
    /// signed by no one.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub own_file: bool,
}

impl Kept {
    /// Whether it's a package of themes.
    pub fn themes(&self) -> bool {
        self.kind == THEMES
    }

    /// Whether it's a package of nothing but skills.
    pub fn skills(&self) -> bool {
        self.kind == SKILLS
    }

    /// Whether it's a package of the user's own, installed from a file:
    /// marked so ([`Kept::own_file`]) from flowapp 0.2.0 (Flow 0.1.45);
    /// before, when nothing marked it, a package of themes or skills that
    /// came with no programs nor a download kept, which the `catalog`
    /// flowapp last read doesn't call `name`. Where it never read one, an
    /// unmarked one is taken for the catalog's, as it can't be told apart.
    pub fn own(&self, name: &str, catalog: Option<&Catalog>) -> bool {
        let as_a_file_was = self.programs.is_empty() && self.download.is_none() && self.key.is_empty() && (self.themes() || self.skills());
        self.own_file || (as_a_file_was && catalog.is_some_and(|c| c.get(name).is_none()))
    }
}

/// This system and chip, as the catalog's downloads are named:
/// `macos-aarch64`, `windows-x86_64`, `linux-x86_64`.
pub fn platform() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

/// The folder installed apps live in, beside Flow's config.
pub fn dir() -> PathBuf {
    crate::config::config_dir().join("apps")
}

/// The folder of installed apps' programs, which every tab's PATH has.
pub fn bin_dir() -> PathBuf {
    dir().join("bin")
}

/// The man pages each app installed under `root` brings, as `share/man`
/// in its version's folder (Flow-Utils' do), for the tabs' `MANPATH`
/// (`shell::build_command`); in the apps' order, those without none.
pub fn man_dirs(root: &Path) -> Vec<PathBuf> {
    Installed::load(root).apps.iter().filter(|(_, kept)| !kept.themes()).map(|(name, kept)| root.join(name).join(&kept.version).join("share").join("man")).filter(|dir| dir.is_dir()).collect()
}

/// The skills for the assistant each package installed under `root`
/// brings (`ai::skills`): its name and its version's `skills/` folder, by
/// name, those without none. An app may carry them beside its programs,
/// and a package of skills holds nothing else.
pub fn skill_dirs(root: &Path) -> Vec<(String, PathBuf)> {
    Installed::load(root)
        .apps
        .iter()
        .map(|(name, kept)| (name.clone(), root.join(name).join(&kept.version).join(crate::ai::skills::PACKAGE_DIR)))
        .filter(|(_, dir)| dir.is_dir())
        .collect()
}

/// `program` as a file name on this system (`flow-rogue.exe` on Windows).
pub fn exe(program: &str) -> String {
    format!("{program}{}", std::env::consts::EXE_SUFFIX)
}

/// Whether version `a` comes after `b`, number by number (0.1.31.3 after
/// 0.1.31, 0.1.10 after 0.1.9); what isn't a number counts as 0.
pub fn newer(a: &str, b: &str) -> bool {
    let parts = |v: &str| -> Vec<u64> { v.trim().trim_start_matches('v').split('.').map(|p| p.parse().unwrap_or(0)).collect() };
    let (a, b) = (parts(a), parts(b));
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0));
        if x != y {
            return x > y;
        }
    }
    false
}

/// The version a program says it is, run as `program --version`
/// ([`version_in`]); none where it doesn't run or says nothing.
pub fn version_of(program: &Path) -> Option<String> {
    let out = Command::new(program).arg("--version").output().ok()?;
    version_in(&String::from_utf8_lossy(&out.stdout))
}

/// The version in what a program's `--version` says: the first word of its
/// first line that starts with a digit, without a bracket or comma after
/// it, so `flow-rogue 0.1.45`, `flowapp 0.2.0 (build 750)` and `Flow
/// 0.1.45 (build 737)` alike. (Before flowapp 0.2.0 it took the last word,
/// which read flowapp's own `737)`.)
pub fn version_in(said: &str) -> Option<String> {
    let word = said.lines().next()?.split_whitespace().find(|w| w.starts_with(|c: char| c.is_ascii_digit()))?;
    let word = word.trim_end_matches([')', ',', ';']);
    (!word.is_empty()).then(|| word.to_string())
}

impl App {
    /// Its download for this system and chip, where there's one, else the
    /// one for every system (a theme package's).
    pub fn here(&self) -> Option<&Download> {
        self.downloads.get(&platform()).or_else(|| self.downloads.get(ANY))
    }
}

impl Download {
    /// The Flow it needs, where that's newer than `flow` (this one's
    /// version, [`crate::version::VERSION`]): so it waits until Flow is
    /// updated.
    pub fn needs_newer_flow(&self, flow: &str) -> Option<&str> {
        let needs = self.flow.trim();
        (!needs.is_empty() && newer(needs, flow)).then_some(needs)
    }

    /// The page of the release it's on, where it's on a GitHub release
    /// (`…/releases/download/<tag>/<file>`, as `app-release.sh` puts them):
    /// `…/releases/tag/<tag>`. None for a download anywhere else.
    pub fn release_page(&self) -> Option<String> {
        let (repo, rest) = self.url.split_once("/releases/download/")?;
        let (tag, file) = rest.split_once('/')?;
        let fine = |part: &str| !part.is_empty() && !part.contains(['?', '#']);
        (repo.starts_with("https://github.com/") && fine(tag) && fine(file) && !file.contains('/')).then(|| format!("{repo}/releases/tag/{tag}"))
    }
}

impl Catalog {
    /// A catalog from its JSON: one of another format is refused.
    pub fn parse(text: &str) -> Result<Self, String> {
        let catalog: Catalog = serde_json::from_str(text).map_err(|e| e.to_string())?;
        if catalog.format != FORMAT {
            return Err(format!("format {} (this flowapp reads {FORMAT})", catalog.format));
        }
        Ok(catalog)
    }

    /// The app, theme package or package of skills called `name`.
    pub fn get(&self, name: &str) -> Option<&App> {
        self.apps.get(name).or_else(|| self.themes.get(name)).or_else(|| self.skills.get(name))
    }

    /// Whether `name` is a theme package.
    pub fn is_themes(&self, name: &str) -> bool {
        !self.apps.contains_key(name) && self.themes.contains_key(name)
    }

    /// Whether `name` is a package of skills.
    pub fn is_skills(&self, name: &str) -> bool {
        !self.apps.contains_key(name) && !self.themes.contains_key(name) && self.skills.contains_key(name)
    }

    /// Every app's name, then every theme package's, then every package of
    /// skills'.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        let themes = self.themes.keys().filter(|n| !self.apps.contains_key(*n));
        let skills = self.skills.keys().filter(|n| !self.apps.contains_key(*n) && !self.themes.contains_key(*n));
        self.apps.keys().chain(themes).chain(skills).map(String::as_str)
    }

    /// Its apps and packages whose words or names hold every one of
    /// `words`, any case.
    pub fn search(&self, words: &[String]) -> Vec<&str> {
        let words: Vec<String> = words.iter().map(|w| w.to_lowercase()).collect();
        self.apps
            .iter()
            .chain(self.themes.iter())
            .chain(self.skills.iter())
            .filter(|(name, app)| {
                let text = format!("{name} {}", app.summary).to_lowercase();
                words.iter().all(|w| text.contains(w.as_str()))
            })
            .map(|(name, _)| name.as_str())
            .collect()
    }
}

impl Installed {
    fn path(root: &Path) -> PathBuf {
        root.join("installed.json")
    }

    /// What's installed under `root`; none where nothing is.
    pub fn load(root: &Path) -> Self {
        std::fs::read_to_string(Self::path(root)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    fn save(&self, root: &Path) -> Result<(), String> {
        std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        // Written whole, then put in place, so a crash leaves the old one.
        let new = root.join("installed.json.new");
        std::fs::write(&new, text).map_err(|e| e.to_string())?;
        std::fs::rename(&new, Self::path(root)).map_err(|e| e.to_string())
    }
}

/// Where an app stands: installed by `flowapp`, carried in Flow's release
/// (`bundled`, at the version its program says), or neither.
#[derive(Debug, Clone, PartialEq)]
pub enum Standing {
    Installed(String),
    Bundled(String),
    Absent,
}

impl Standing {
    pub fn version(&self) -> Option<&str> {
        match self {
            Standing::Installed(v) | Standing::Bundled(v) => Some(v),
            Standing::Absent => None,
        }
    }
}

/// Where `name` stands, with `bundled_dir` the folder of the games Flow's
/// release carries (where `flowapp` itself is), if any.
pub fn standing(installed: &Installed, name: &str, app: Option<&App>, bundled_dir: Option<&Path>) -> Standing {
    if let Some(kept) = installed.apps.get(name) {
        return Standing::Installed(kept.version.clone());
    }
    let program = app.and_then(|a| a.bundled.as_deref());
    match program.zip(bundled_dir).map(|(p, d)| d.join(exe(p))).filter(|p| p.is_file()).and_then(|p| version_of(&p)) {
        Some(version) => Standing::Bundled(version),
        None => Standing::Absent,
    }
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(20)))
        .timeout_global(Some(Duration::from_secs(10 * 60)))
        .user_agent(concat!("flowapp/", env!("FLOWAPP_VERSION")))
        .build()
        .into()
}

/// What's at `place`, a URL or a file, at most `most` bytes.
pub fn fetch(place: &str, most: u64) -> Result<Vec<u8>, Failed> {
    fetch_with(place, most, &mut |_| {})
}

/// [`fetch`], telling `got` how many bytes have come each time more do (a
/// file's all at once), so a download can show how far it's got.
pub fn fetch_with(place: &str, most: u64, got: &mut dyn FnMut(u64)) -> Result<Vec<u8>, Failed> {
    if !(place.starts_with("https://") || place.starts_with("http://")) {
        let bytes = std::fs::read(place.strip_prefix("file://").unwrap_or(place)).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => Failed::Missing(format!("{place}: {e}")),
            _ => Failed::Other(format!("{place}: {e}")),
        })?;
        if bytes.len() as u64 > most {
            return Err(format!("{place}: more than {most} bytes").into());
        }
        got(bytes.len() as u64);
        return Ok(bytes);
    }
    fetch_routed(place, most, got, GITHUB, GITHUB_API)
}

/// GitHub's web front, where a release's files are downloaded from, and
/// its API, which reaches the same files by another way.
const GITHUB: &str = "https://github.com/";
const GITHUB_API: &str = "https://api.github.com/repos/";

/// The waits before each try again after a gateway's trouble (502, 503,
/// 504), which comes and goes: GitHub's edge has answered every page of
/// github.com with 504 for minutes at a time, release downloads too.
#[cfg(not(test))]
const AGAIN_AFTER: [Duration; 2] = [Duration::from_secs(2), Duration::from_secs(5)];
#[cfg(test)]
const AGAIN_AFTER: [Duration; 2] = [Duration::from_millis(10), Duration::from_millis(10)];

/// Why a request came to nothing.
enum Trouble {
    /// A gateway's, worth trying again: its words.
    Passing(String),
    /// Anything else.
    Failed(Failed),
}

/// [`fetch_with`] over HTTP: tried again after a gateway's trouble, then,
/// for a file of a release at `web` (GitHub's), fetched through `api`
/// (its API) where the web front still fails. Whichever way it comes, the
/// caller checks its size, SHA-256 and signature against the catalog.
fn fetch_routed(place: &str, most: u64, got: &mut dyn FnMut(u64), web: &str, api: &str) -> Result<Vec<u8>, Failed> {
    let mut why = match get(place, None, most, got) {
        Ok(bytes) => return Ok(bytes),
        Err(Trouble::Failed(failed)) => return Err(failed),
        Err(Trouble::Passing(why)) => why,
    };
    for wait in AGAIN_AFTER {
        std::thread::sleep(wait);
        match get(place, None, most, got) {
            Ok(bytes) => return Ok(bytes),
            Err(Trouble::Failed(failed)) => return Err(failed),
            Err(Trouble::Passing(again)) => why = again,
        }
    }
    if let Some((release, name)) = api_route(place, web, api) {
        if let Ok(bytes) = from_api(&release, &name, most, got) {
            return Ok(bytes);
        }
    }
    Err(Failed::Other(why))
}

/// For a release's file on GitHub's web front (`web` + `owner/repo/
/// releases/download/<tag>/<name>`), its release in the API (`api` +
/// `owner/repo/releases/tags/<tag>`) and the file's name.
fn api_route(place: &str, web: &str, api: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = place.strip_prefix(web)?.split('/').collect();
    match parts[..] {
        [owner, repo, "releases", "download", tag, name] if [owner, repo, tag, name].iter().all(|p| !p.is_empty()) => Some((format!("{api}{owner}/{repo}/releases/tags/{tag}"), name.to_string())),
        _ => None,
    }
}

/// A release's file `name` through GitHub's API: the release at `release`
/// lists its files, each with its own address there, which hands the
/// bytes over when asked for them as such.
fn from_api(release: &str, name: &str, most: u64, got: &mut dyn FnMut(u64)) -> Result<Vec<u8>, Trouble> {
    let listed = get(release, Some("application/vnd.github+json"), 1 << 20, &mut |_| {})?;
    let listed: serde_json::Value = serde_json::from_slice(&listed).map_err(|e| Trouble::Failed(format!("{release}: {e}").into()))?;
    let asset = listed["assets"].as_array().into_iter().flatten().find(|a| a["name"] == name);
    let Some(url) = asset.and_then(|a| a["url"].as_str()) else { return Err(Trouble::Failed(Failed::Missing(format!("{release}: no {name}")))) };
    get(url, Some("application/octet-stream"), most, got)
}

/// One request for `place` (asking for `accept`), at most `most` bytes.
fn get(place: &str, accept: Option<&str>, most: u64, got: &mut dyn FnMut(u64)) -> Result<Vec<u8>, Trouble> {
    let mut request = agent().get(place);
    if let Some(accept) = accept {
        request = request.header("accept", accept);
    }
    let mut resp = request.call().map_err(|e| match e {
        ureq::Error::StatusCode(404 | 410) => Trouble::Failed(Failed::Missing(format!("{place}: {e}"))),
        ureq::Error::StatusCode(502..=504) => Trouble::Passing(format!("{place}: {e}")),
        e => Trouble::Failed(Failed::Other(format!("{place}: {e}"))),
    })?;
    read_body(place, &mut resp, most, got).map_err(Trouble::Failed)
}

/// A response's body, at most `most` bytes, telling `got` as it comes.
fn read_body(place: &str, resp: &mut ureq::http::Response<ureq::Body>, most: u64, got: &mut dyn FnMut(u64)) -> Result<Vec<u8>, Failed> {
    let mut reader = resp.body_mut().with_config().limit(most + 1).reader();
    let mut bytes = Vec::new();
    let mut chunk = vec![0u8; 64 * 1024];
    loop {
        match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                bytes.extend_from_slice(&chunk[..n]);
                if bytes.len() as u64 > most {
                    return Err(format!("{place}: more than {most} bytes").into());
                }
                got(bytes.len() as u64);
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(format!("{place}: {e}").into()),
        }
    }
    Ok(bytes)
}

/// The catalog: from `FLOWAPP_CATALOG` where it's set, else Flow's.
pub fn catalog_place() -> String {
    std::env::var("FLOWAPP_CATALOG").ok().filter(|v| !v.is_empty()).unwrap_or_else(|| CATALOG_URL.to_string())
}

/// The themes in a package of themes `name`'s download `bytes` (already
/// checked against the catalog), read without installing it: unpacked into
/// a folder of its own under `root`'s `previews/`, read as an install
/// would (`packaged::peek_shown`, with the fonts and logos it brings), and
/// the folder taken away again. For Settings' theme gallery.
pub fn peek_themes(root: &Path, name: &str, bytes: &[u8]) -> Result<crate::looks::packaged::Peeked, String> {
    if !fit_name(name) {
        return Err(crate::i18n::tf("flowapp.bad_name", &[("name", &printable(name))]));
    }
    let dir = root.join("previews").join(format!("{name}-{}", std::process::id()));
    take_away(&dir)?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let read = untar(bytes, &dir).and_then(|()| {
        if dir.join("bin").exists() {
            return Err(format!("{name} holds programs, so it isn't a package of themes"));
        }
        crate::looks::packaged::peek_shown(&dir)
    });
    let _ = take_away(&dir);
    read
}

/// A catalog as a command goes by it.
#[derive(Debug, Clone, PartialEq)]
pub struct Read {
    pub catalog: Catalog,
    /// How long ago it was read: none where it was just now.
    pub age: Duration,
    /// Why it wasn't read again, where that was tried and this is the copy
    /// the last read saved.
    pub failed: Option<String>,
}

fn saved_path(root: &Path) -> PathBuf {
    root.join("catalog.json")
}

/// The catalog the last read saved under `root`, and how long ago that was
/// (by the file's time).
pub fn saved_catalog(root: &Path) -> Option<Read> {
    let path = saved_path(root);
    let catalog = Catalog::parse(&std::fs::read_to_string(&path).ok()?).ok()?;
    let at = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
    let age = at.and_then(|at| SystemTime::now().duration_since(at).ok()).unwrap_or_default();
    Some(Read { catalog, age, failed: None })
}

/// The catalog at `place` read now, and saved under `root` for the
/// commands after.
pub fn read_catalog(root: &Path, place: &str) -> Result<Read, String> {
    let bytes = fetch(place, CATALOG_MOST)?;
    let unreadable = |why: Option<String>| crate::i18n::tf("flowapp.unreadable", &[("place", &why.map_or(place.to_string(), |why| format!("{place} ({why})")))]);
    let text = String::from_utf8(bytes).map_err(|_| unreadable(None))?;
    let catalog = Catalog::parse(&text).map_err(|e| unreadable(Some(e)))?;
    std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
    std::fs::write(saved_path(root), &text).map_err(|e| e.to_string())?;
    Ok(Read { catalog, age: Duration::ZERO, failed: None })
}

/// The catalog the last read saved, or read now where none was: what
/// `list`, `outdated`, `info` and `search` go by.
pub fn catalog_as_saved(root: &Path, place: &str) -> Result<Read, String> {
    saved_catalog(root).map_or_else(|| read_catalog(root, place), Ok)
}

/// The catalog read now, as `install` and `upgrade` read it before they
/// download: it's a few kilobytes beside the megabytes they fetch, and a
/// saved copy may name a release that's gone. Where it won't read, the copy
/// the last read saved, with why.
pub fn catalog_afresh(root: &Path, place: &str) -> Result<Read, String> {
    read_catalog(root, place).or_else(|error| saved_catalog(root).map(|read| Read { failed: Some(error.clone()), ..read }).ok_or(error))
}

/// Why a fetch failed, in words: nothing was at its place (a URL that
/// answers 404 or 410, a file that isn't there), which for a download means
/// the catalog names a release that's gone, or anything else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failed {
    Missing(String),
    Other(String),
}

impl std::fmt::Display for Failed {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let (Failed::Missing(words) | Failed::Other(words)) = self;
        f.write_str(words)
    }
}

impl From<String> for Failed {
    fn from(words: String) -> Self {
        Failed::Other(words)
    }
}

impl From<Failed> for String {
    fn from(failed: Failed) -> Self {
        failed.to_string()
    }
}

/// Whether `bytes` are what `download` says: its size, its SHA-256, and
/// signed by `key` (base64, Ed25519), as Sparkle's `sign_update` signs.
pub fn check(bytes: &[u8], download: &Download, key: &str) -> Result<(), String> {
    if bytes.len() as u64 != download.size {
        return Err(format!("{} bytes, not the {} the catalog says", bytes.len(), download.size));
    }
    if !sha256(bytes).eq_ignore_ascii_case(download.sha256.trim()) {
        return Err("its SHA-256 isn't the catalog's".into());
    }
    if !signed(bytes, &download.signature, key) {
        return Err("it isn't signed with Flow's key".into());
    }
    Ok(())
}

/// `bytes`' SHA-256, in hex, as the catalog writes it.
pub fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// Whether `signature` (base64) is `key`'s (base64) Ed25519 signature of
/// `bytes`.
pub fn signed(bytes: &[u8], signature: &str, key: &str) -> bool {
    use ed25519_dalek::{Signature, VerifyingKey};
    let key: Option<[u8; 32]> = STANDARD.decode(key.trim()).ok().and_then(|k| k.try_into().ok());
    let sig: Option<[u8; 64]> = STANDARD.decode(signature.trim()).ok().and_then(|s| s.try_into().ok());
    let (Some(key), Some(sig)) = (key, sig) else { return false };
    VerifyingKey::from_bytes(&key).is_ok_and(|k| k.verify_strict(bytes, &Signature::from_bytes(&sig)).is_ok())
}

/// How far an install has got, as [`install_with`] tells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// This many bytes of the download have come, of its `size`.
    Fetched(u64),
    /// The download is its size, its SHA-256 and signed with the key.
    Checked,
}

/// Downloads `name` as the catalog has it for this system, checks it with
/// `key` and installs it under `root`: unpacked in `<name>/<version>/`, its
/// programs put in `bin/`, the version before it let go. Says what's
/// installed.
pub fn install(root: &Path, name: &str, app: &App, key: &str) -> Result<Kept, Failed> {
    install_with(root, name, app, key, &mut |_| {})
}

/// [`install`], telling `step` how the download comes and when it's been
/// checked.
pub fn install_with(root: &Path, name: &str, app: &App, key: &str, step: &mut dyn FnMut(Step)) -> Result<Kept, Failed> {
    let download = app.here().ok_or_else(|| format!("no download of {name} for {} yet", platform()))?;
    // Refused before it's fetched, rather than after.
    if let Some(why) = in_the_way(&Installed::load(root), name, false, None) {
        return Err(why.into());
    }
    if let Some(needs) = download.needs_newer_flow(&itself::flow_version()) {
        return Err(needs_flow(name, &download.version, needs).into());
    }
    if download.size > MOST {
        return Err(format!("{name}'s download is more than {MOST} bytes").into());
    }
    let bytes = fetch_with(&download.url, download.size, &mut |got| step(Step::Fetched(got)))?;
    check(&bytes, download, key).map_err(|e| format!("{name}: {e}"))?;
    step(Step::Checked);
    let signed = Signed { download: download.clone(), key: signing::fingerprint(key).unwrap_or_default(), checked: today() };
    Ok(install_kept(root, name, &download.version, &app.programs, &bytes, Some(signed), false)?)
}

/// What an install checked its download against, for `installed.json`.
pub struct Signed {
    pub download: Download,
    /// The fingerprint of the key that checked it.
    pub key: String,
    /// When, a UTC date.
    pub checked: String,
}

/// Today, a UTC date, as the catalog and `installed.json` write it.
pub fn today() -> String {
    jiff::Timestamp::now().to_zoned(jiff::tz::TimeZone::UTC).date().to_string()
}

/// `text` as a date the catalog writes (`2026-11-18`), or none where it
/// isn't one.
pub fn date(text: &str) -> Option<String> {
    text.trim().parse::<jiff::civil::Date>().ok().map(|date| date.to_string())
}

/// Why version `version` of `name` won't install here: it `needs` a newer
/// Flow than this one.
pub fn needs_flow(name: &str, version: &str, needs: &str) -> String {
    crate::i18n::tf("flowapp.needs_flow", &[("name", &name), ("version", &version), ("needs", &needs), ("flow", &itself::flow_version())])
}

/// Installs `bytes`, version `version` of an app's checked download, with
/// its `programs`, under `root`, noting nothing of who signed it.
pub fn install_bytes(root: &Path, name: &str, version: &str, programs: &[String], bytes: &[u8]) -> Result<Kept, String> {
    install_kept(root, name, version, programs, bytes, None, false)
}

/// Unpacks the `.tar.gz` in `bytes` into `into`, which must be empty, with
/// the system's `tar`. On a Mac, without the locks its `tar` (bsdtar,
/// named by its path, since a GNU tar first on the PATH knows no such
/// flags) would otherwise put on them: as a user and not only as root,
/// whatever ACL the tarball's AppleDouble entries (`._bin`) hold, and as
/// root its other ACLs and file flags (`schg`) too, any of which can keep
/// what anyone's tarball made from ever being taken away. Flow's own
/// downloads are packed without AppleDouble entries (`--no-mac-metadata`),
/// so they lose nothing.
pub fn untar(bytes: &[u8], into: &Path) -> Result<(), String> {
    let tarball = into.join("download.tar.gz");
    std::fs::write(&tarball, bytes).map_err(|e| e.to_string())?;
    let mut tar = Command::new(if cfg!(target_os = "macos") { "/usr/bin/tar" } else { "tar" });
    if cfg!(target_os = "macos") {
        tar.args(["--no-mac-metadata", "--no-acls", "--no-fflags"]);
    }
    let out = tar.arg("-xzf").arg(&tarball).arg("-C").arg(into).output().map_err(|e| format!("tar: {e}"));
    let _ = std::fs::remove_file(&tarball);
    let out = out?;
    if !out.status.success() {
        return Err(format!("tar: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(())
}

/// Takes `path` away, and all in it, where it's there; why not, where it's
/// still there after. What anyone's tarball unpacked may hold what its
/// owner can't take away as it is: folders `tar` gave the mode the tarball
/// says (`0555`), and on a Mac what an older flowapp's `tar` gave from the
/// tarball's AppleDouble entries, an ACL denying delete ([`untar`] gives
/// none now). So where `remove_dir_all` alone fails, all in it is made its
/// owner's to change ([`let_owner_change_all`]) and it's tried again. A
/// link in it is taken away as itself and never followed, as
/// `remove_dir_all` takes it, so nothing outside is changed.
fn take_away(path: &Path) -> Result<(), String> {
    let Ok(meta) = std::fs::symlink_metadata(path) else { return Ok(()) };
    let remove = || if meta.is_dir() { std::fs::remove_dir_all(path) } else { std::fs::remove_file(path) };
    let removed = remove().or_else(|_| {
        let_owner_change_all(path);
        remove()
    });
    match std::fs::symlink_metadata(path) {
        Err(_) => Ok(()),
        Ok(_) => {
            // The system's words, as for any other failure; something put
            // back there meanwhile is one that exists.
            let error = removed.err().unwrap_or_else(|| std::io::ErrorKind::AlreadyExists.into()).to_string();
            Err(crate::i18n::tf("flowapp.cant_take_away", &[("path", &path.display()), ("error", &error)]))
        }
    }
}

/// Makes all at `path` its owner's to change, for [`take_away`]: each
/// folder's mode, each file not read-only on Windows, and on a Mac each
/// one's ACL taken off ([`take_acls_off`]). Each folder before what's in
/// it, which it must let be read and gone into; never through a link.
fn let_owner_change_all(path: &Path) {
    let Ok(meta) = std::fs::symlink_metadata(path) else { return };
    take_acls_off(&[path.to_path_buf()], meta.file_type().is_symlink());
    if !meta.is_dir() {
        let_owner_change(path);
        return;
    }
    let mut folders = vec![path.to_path_buf()];
    while let Some(folder) = folders.pop() {
        let_owner_change(&folder);
        let (mut plain, mut links) = (Vec::new(), Vec::new());
        for entry in std::fs::read_dir(&folder).into_iter().flatten().flatten() {
            let Ok(kind) = entry.file_type() else { continue };
            if kind.is_symlink() {
                links.push(entry.path());
                continue;
            }
            if kind.is_dir() {
                folders.push(entry.path());
            } else if cfg!(windows) {
                let_owner_change(&entry.path());
            }
            plain.push(entry.path());
        }
        // A folder's own before it's popped and read.
        take_acls_off(&plain, false);
        take_acls_off(&links, true);
    }
}

/// Takes off the ACL of each of `paths`, with the Mac's own `chmod -N`:
/// of a link itself where `links` (`-h`), never of what it leads to, and
/// otherwise of what wasn't a link as its folder was read. Its owner may,
/// whatever the ACL denies; and without `-h` it goes by the path, so a
/// folder whose mode or ACL won't let it be read loses its ACL too. Each
/// path goes as one `chmod` can't read as an option (`-w…`).
#[cfg(target_os = "macos")]
fn take_acls_off(paths: &[PathBuf], links: bool) {
    // A few hundred at a time, well within what a command line holds.
    for some in paths.chunks(256) {
        let mut chmod = Command::new("/bin/chmod");
        if links {
            chmod.arg("-h");
        }
        chmod.arg("-N").args(some.iter().map(|path| if path.is_absolute() { path.clone() } else { Path::new(".").join(path) }));
        let _ = chmod.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status();
    }
}

#[cfg(not(target_os = "macos"))]
fn take_acls_off(_: &[PathBuf], _: bool) {}

/// Lets the owner change the folder at `path` (read, write and go into it)
/// where the mode it was given doesn't, never through a link.
#[cfg(unix)]
fn let_owner_change(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let Ok(meta) = std::fs::symlink_metadata(path) else { return };
    let mode = meta.permissions().mode();
    if meta.is_dir() && mode & 0o700 != 0o700 {
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode | 0o700));
    }
}

/// Takes the read-only attribute off what's at `path`, never through a
/// link: Windows deletes nothing read-only where it can't ignore that.
#[cfg(windows)]
#[allow(clippy::permissions_set_readonly_false)] // Windows' attribute alone, which is what's meant
fn let_owner_change(path: &Path) {
    let Ok(meta) = std::fs::symlink_metadata(path) else { return };
    let mut permissions = meta.permissions();
    if !meta.file_type().is_symlink() && permissions.readonly() {
        permissions.set_readonly(false);
        let _ = std::fs::set_permissions(path, permissions);
    }
}

#[cfg(not(any(unix, windows)))]
fn let_owner_change(_: &Path) {}

/// [`install_bytes`], keeping in `installed.json` what `signed` says
/// checked the download, or that it's a file of the user's own.
fn install_kept(root: &Path, name: &str, version: &str, programs: &[String], bytes: &[u8], signed: Option<Signed>, own_file: bool) -> Result<Kept, String> {
    let fail = |e: &dyn std::fmt::Display| format!("{name}: {e}");
    // Its name and version become folders here, the catalog's (which
    // nothing signs) as well as a package of one's own.
    if !fit_name(name) {
        return Err(crate::i18n::tf("flowapp.bad_name", &[("name", &printable(name))]));
    }
    if !fit_version(version) {
        return Err(crate::i18n::tf("flowapp.bad_version", &[("name", &name), ("version", &printable(version))]));
    }
    // A package of the user's own is told from the catalog's by the
    // catalog flowapp last read, where it isn't marked.
    let catalog = own_file.then(|| saved_catalog(root)).flatten();
    if let Some(why) = in_the_way(&Installed::load(root), name, own_file, catalog.as_ref().map(|read| &read.catalog)) {
        return Err(why);
    }
    // A program is a file in bin/, never a path to one elsewhere.
    if let Some(program) = programs.iter().find(|p| !fit_program(p)) {
        return Err(crate::i18n::tf("flowapp.bad_program", &[("name", &name), ("program", &printable(program))]));
    }
    let home = root.join(name);
    let at = home.join(version);
    // Unpacked beside where it goes, then put there whole.
    let unpacking = home.join(format!(".{version}.unpacking"));
    // A refusal leaves nothing behind: not what was unpacked, a folder
    // it made read-only or locked too ([`take_away`]), nor the app's
    // folder where this was to be its first version (`remove_dir` takes
    // only an empty one, so a version installed before stays).
    let give_up = || {
        let _ = take_away(&unpacking);
        let _ = std::fs::remove_dir(&home);
    };
    // What an install before left (an older flowapp's refusal) is never
    // unpacked over, which would make it part of this one.
    take_away(&unpacking).map_err(|e| fail(&e))?;
    std::fs::create_dir_all(&unpacking).map_err(|e| fail(&e))?;
    if let Err(error) = untar(bytes, &unpacking) {
        give_up();
        return Err(fail(&error));
    }
    for program in programs {
        if !unpacking.join("bin").join(exe(program)).is_file() {
            give_up();
            return Err(fail(&format!("its download has no bin/{}", exe(program))));
        }
    }
    // A package of the user's own holds no program, even one it doesn't
    // name: so what was looked at in its file is all there is.
    if own_file && unpacking.join("bin").exists() {
        give_up();
        return Err(fail(&crate::i18n::t("flowapp.own_programs")));
    }
    // A package of themes, whose themes must all read, and one of the
    // user's own take no other's place; or one of skills, which must hold
    // one that reads.
    let themes = unpacking.join(crate::looks::packaged::FILE).is_file();
    let skills = !themes && unpacking.join(crate::ai::skills::FILE).is_file();
    let checked = if themes {
        crate::looks::packaged::peek(&unpacking).and_then(|file| match own_file.then(|| own_themes_refused(root, name, &file)).flatten() {
            Some(why) => Err(why),
            None => Ok(()),
        })
    } else if skills {
        crate::ai::skills::manifest(&unpacking).map(|_| ())
    } else {
        Ok(())
    };
    // What introduces it must be one of its own programs, there.
    let checked = checked.and_then(|()| read_hello(&unpacking, programs).map(|_| ()));
    if let Err(error) = checked {
        give_up();
        return Err(fail(&error));
    }
    if let Err(error) = take_away(&at) {
        give_up();
        return Err(fail(&error));
    }
    std::fs::rename(&unpacking, &at).map_err(|e| fail(&e))?;
    // Each program into bin/, by renaming over the old (aside first, as
    // Windows won't rename over a program that's running).
    let bin = root.join("bin");
    if !programs.is_empty() {
        std::fs::create_dir_all(&bin).map_err(|e| fail(&e))?;
    }
    for program in programs {
        let file = exe(program);
        let new = bin.join(format!(".{file}.new"));
        std::fs::copy(at.join("bin").join(&file), &new).map_err(|e| fail(&e))?;
        let to = bin.join(&file);
        if to.exists() {
            let aside = bin.join(format!(".{file}.old"));
            let _ = std::fs::remove_file(&aside);
            let _ = std::fs::rename(&to, &aside);
        }
        std::fs::rename(&new, &to).map_err(|e| fail(&e))?;
    }
    let mut installed = Installed::load(root);
    let before = installed.apps.get(name).cloned();
    let kind = if themes {
        THEMES
    } else if skills {
        SKILLS
    } else {
        ""
    };
    let (download, key, checked) = signed.map(|s| (Some(s.download), s.key, s.checked)).unwrap_or_default();
    let kept = Kept { version: version.to_string(), programs: programs.to_vec(), kind: kind.into(), download, key, checked, own_file };
    installed.apps.insert(name.to_string(), kept.clone());
    installed.save(root)?;
    // Programs it had that this version hasn't go from bin/, and from
    // first where no other app has them.
    if let Some(before) = &before {
        for program in before.programs.iter().filter(|p| !programs.contains(p)) {
            let _ = std::fs::remove_file(bin.join(exe(program)));
        }
    }
    // The panes' folders link the new programs (a Windows link is the old
    // file's own, not a name), before the version before them goes.
    follow_firsts(root, &installed);
    // Not where it differs from this one only in case, which is this
    // one's folder on a Mac or Windows; nor a version from before versions
    // were checked ([`fit_version`]), which may be this one's folder there
    // too (`1.0.` is `1.0` on Windows), or no folder under the app's at all
    // (`C:`).
    if let Some(before) = before.filter(|before| !before.version.eq_ignore_ascii_case(version) && fit_version(&before.version)) {
        let _ = take_away(&home.join(&before.version));
    }
    sweep_aside(&bin);
    Ok(kept)
}

/// Keeps what's first in step with what's `installed`: the choices of
/// programs gone forgotten, and every pane's folder written afresh, so
/// open tabs run what's installed now.
fn follow_firsts(root: &Path, installed: &Installed) {
    let mut record = first::Record::load(root);
    if record.forget_gone(installed) {
        let _ = record.save(root);
    }
    first::rewrite_panes(root);
}

/// Installs the package of themes or skills in the `.tar.gz` at `file`,
/// one of the user's own rather than the catalog's, so signed by no one: a
/// theme is only data (colors, fonts, words to type), and a skill words
/// the assistant reads only once the user turns its package on in
/// Settings, never a program; a file that holds neither, or any program,
/// is refused. So is one whose name is Flow's ([`own_name_refused`],
/// [`in_the_way`]): flowapp's, one in the catalog saved under `root`, or
/// one installed from it; and one bringing a theme in place of another
/// ([`own_themes_refused`]), since anyone can make such a file and send
/// it. Says its name and what's installed.
pub fn install_file(root: &Path, file: &Path) -> Result<(String, Kept), String> {
    let bytes = std::fs::read(file).map_err(|e| format!("{}: {e}", file.display()))?;
    if bytes.len() as u64 > MOST {
        return Err(format!("{}: more than {MOST} bytes", file.display()));
    }
    // Unpacked once to read its name and version, then installed as a
    // download is: the bytes read, never the file again, which could have
    // changed meanwhile.
    let looking = root.join(".file.unpacking");
    take_away(&looking)?;
    std::fs::create_dir_all(&looking).map_err(|e| e.to_string())?;
    let read = (|| {
        untar(&bytes, &looking)?;
        if looking.join("bin").exists() {
            return Err(format!("{}: {}", file.display(), crate::i18n::t("flowapp.own_programs")));
        }
        if !looking.join(crate::looks::packaged::FILE).is_file() && looking.join(crate::ai::skills::FILE).is_file() {
            let skills = crate::ai::skills::manifest(&looking).map_err(|e| format!("{}: {e}", file.display()))?;
            return Ok((skills.package, skills.version));
        }
        let text = std::fs::read_to_string(looking.join(crate::looks::packaged::FILE))
            .map_err(|_| format!("{}: holds no {} or {}", file.display(), crate::looks::packaged::FILE, crate::ai::skills::FILE))?;
        let themes = serde_json::from_str::<crate::looks::packaged::ThemeFile>(&text).map_err(|e| format!("{}: {e}", crate::looks::packaged::FILE))?;
        Ok((themes.package, themes.version))
    })();
    let _ = take_away(&looking);
    let (package, version) = read?;
    // Never the name of one of Flow's, which it would take the place of,
    // as far as the catalog flowapp last read says (`flowapp install`
    // reads it afresh first, so a name it has added since counts too).
    if let Some(why) = own_name_refused(&package, saved_catalog(root).as_ref().map(|read| &read.catalog)) {
        return Err(format!("{}: {why}", file.display()));
    }
    let kept = install_kept(root, &package, &version, &[], &bytes, None, true)?;
    Ok((package, kept))
}

/// Takes `name` off: its programs out of `bin/`, its versions deleted.
/// False where `flowapp` hadn't installed it.
pub fn remove(root: &Path, name: &str) -> Result<bool, String> {
    let mut installed = Installed::load(root);
    let Some(kept) = installed.apps.remove(name) else { return Ok(false) };
    let bin = root.join("bin");
    for program in &kept.programs {
        let to = bin.join(exe(program));
        if std::fs::remove_file(&to).is_err() && to.exists() {
            // Running, on Windows: aside, for the next sweep.
            let _ = std::fs::rename(&to, bin.join(format!(".{}.old", exe(program))));
        }
        // Another app's of the same name takes its place in bin/.
        if let Some((other, theirs)) = installed.apps.iter().find(|(_, k)| k.programs.contains(program)) {
            let _ = std::fs::copy(root.join(other).join(&theirs.version).join("bin").join(exe(program)), &to);
        }
    }
    installed.save(root)?;
    follow_firsts(root, &installed);
    // One installed before names were checked leaves its folder where
    // that may be another's ([`own_folder`]).
    if own_folder(name, &installed) {
        let _ = take_away(&root.join(name));
    }
    sweep_aside(&bin);
    Ok(true)
}

/// Whether the folder `name` is under the apps' folder is that app's
/// alone, to delete as it's removed, with what's still `installed`: not
/// where another installed app's name is the same but for its case, whose
/// folder it is on a Mac or Windows; else always for a name [`fit_name`]
/// takes, and for one installed before names were checked (`MyThemes`,
/// `my themes`) unless it may be another's on some system: a path or a
/// drive (`C:`), one Windows drops a trailing dot or space of, or one of
/// flowapp's own folders or a device's in any case (`Bin` is `bin/` on a
/// Mac).
fn own_folder(name: &str, installed: &Installed) -> bool {
    let lower = name.to_lowercase();
    let reserved = RESERVED.contains(&lower.as_str()) || DEVICES.contains(&lower.as_str());
    let anothers = installed.apps.keys().any(|other| other.to_lowercase() == lower);
    let plain = !name.is_empty() && !matches!(name, "." | "..") && !name.contains(['/', '\\', ':']) && !name.ends_with(['.', ' ']) && !name.chars().any(char::is_control);
    !anothers && (fit_name(name) || (plain && !reserved))
}

/// Deletes the programs put aside by an upgrade or a removal, where they
/// aren't still running (Windows won't delete one that is).
fn sweep_aside(bin: &Path) {
    let Ok(entries) = std::fs::read_dir(bin) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') && name.ends_with(".old") {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// The programs `name` stands for, of what's `installed`: an app's (none
/// for a package of themes), or one of them by its own name.
pub fn programs_of(installed: &Installed, name: &str) -> Option<Vec<String>> {
    if let Some(kept) = installed.apps.get(name) {
        return (!kept.programs.is_empty()).then(|| kept.programs.clone());
    }
    installed.apps.values().any(|kept| kept.programs.iter().any(|p| p == name)).then(|| vec![name.to_string()])
}

/// The command of `program`'s name the system has, where it has one: the
/// first on `path` but in Flow's own folders (`skip`), which `flowapp
/// first` puts a program ahead of.
pub fn system_command(program: &str, path: Option<&std::ffi::OsStr>, skip: &[PathBuf]) -> Option<PathBuf> {
    let file = exe(program);
    std::env::split_paths(path?).filter(|dir| !dir.as_os_str().is_empty() && !skip.iter().any(|s| dir.starts_with(s))).map(|dir| dir.join(&file)).find(|p| p.is_file())
}

/// The folder of an installed app's version, where its `lib/`, `include/`
/// and `python/` are (the SDK's).
pub fn app_dir(root: &Path, name: &str, version: &str) -> PathBuf {
    root.join(name).join(version)
}

/// The file at a download's root that tells flowapp what of the app it
/// runs: its `hello`, the command that introduces it once it's installed.
/// It's in the signed download, so nothing but Flow's signature says what
/// runs, never the catalog, which nothing signs.
pub const APP_FILE: &str = "flowapp.json";

/// What introduces an app once it's installed (`flowapp.json`'s
/// `hello`): one of its own programs, by its path in the version's
/// `bin/`, never through the PATH (on the Mac the system's `say` comes
/// ahead of flow-say's), and the words it's given.
#[derive(Debug, Clone, PartialEq)]
pub struct Hello {
    pub program: PathBuf,
    pub args: Vec<String>,
}

/// The hello `at` holds (a version's folder, or one being unpacked) for
/// an app of `programs`: none without a `flowapp.json` or a `hello` in it;
/// refused where the file doesn't read, or the hello's program isn't one
/// of the app's or its `bin/` hasn't it.
fn read_hello(at: &Path, programs: &[String]) -> Result<Option<Hello>, String> {
    #[derive(Default, Deserialize)]
    #[serde(default)]
    struct File {
        hello: Vec<String>,
    }
    let text = match std::fs::read_to_string(at.join(APP_FILE)) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("{APP_FILE}: {e}")),
    };
    let file: File = serde_json::from_str(&text).map_err(|e| format!("{APP_FILE}: {e}"))?;
    let Some((program, args)) = file.hello.split_first() else { return Ok(None) };
    if !programs.contains(program) {
        return Err(format!("{APP_FILE}: its hello runs {program}, which isn't one of its programs"));
    }
    let path = at.join("bin").join(exe(program));
    if !path.is_file() {
        return Err(format!("{APP_FILE}: its download has no bin/{}", exe(program)));
    }
    Ok(Some(Hello { program: path, args: args.to_vec() }))
}

/// The hello of `name` as it's installed under `root`, where it has one.
pub fn hello(root: &Path, name: &str) -> Option<Hello> {
    let kept = Installed::load(root).apps.get(name)?.clone();
    read_hello(&app_dir(root, name, &kept.version), &kept.programs).ok().flatten()
}

/// Whether `flowapp install` runs an app's hello as it installs it: after
/// a fresh install alone (none of it was installed), and only where
/// someone's there to answer it, standard input and output terminals, so
/// never from a script, a pipe or the assistant.
pub fn hello_runs(fresh: bool, terminal: bool) -> bool {
    fresh && terminal
}

/// What Flow wrote once it had kept the games an older Flow carried
/// (`carried.json` under the apps' folder): the apps it installed for them,
/// none for a Flow that began with 0.1.34 or later, which carried nothing.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Carried {
    pub kept: Vec<String>,
}

fn carried_path(root: &Path) -> PathBuf {
    root.join("carried.json")
}

/// Whether what an older Flow carried has been kept already, or there was
/// nothing to keep: so it's done once.
pub fn carried_settled(root: &Path) -> bool {
    carried_path(root).is_file()
}

/// Notes that what an older Flow carried is kept, with what was installed
/// for it.
pub fn settle_carried(root: &Path, kept: &[String]) -> Result<(), String> {
    std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(&Carried { kept: kept.to_vec() }).map_err(|e| e.to_string())?;
    std::fs::write(carried_path(root), text).map_err(|e| e.to_string())
}

/// The apps an older Flow's release carried (the catalog's `bundled`: Flow
/// Rogue and Flow Mines, until 0.1.33) that aren't installed under `root`
/// and have a download for this system: what an update to a Flow that
/// carries none installs, so it takes no game away. Not one something
/// installed is in the way of ([`in_the_way`]: a package from before names
/// were checked, `Flow-Rogue`, which only the user can take off), which
/// would be refused at every start and keep the others from being settled.
pub fn carried_to_keep(catalog: &Catalog, installed: &Installed) -> Vec<String> {
    carried_here(catalog, installed).filter(|name| in_the_way(installed, name, false, Some(catalog)).is_none()).map(str::to_string).collect()
}

/// What [`carried_to_keep`] passes over, each with the app installed in
/// its way, whose name is its own but for its case.
pub fn carried_in_the_way(catalog: &Catalog, installed: &Installed) -> Vec<(String, String)> {
    let there = |name: &str| installed.apps.keys().find(|there| there.eq_ignore_ascii_case(name)).map(|there| printable(there));
    carried_here(catalog, installed).filter_map(|name| Some((name.to_string(), there(name)?))).collect()
}

/// The apps an older Flow carried with a download here that aren't
/// installed under their own names.
fn carried_here<'a>(catalog: &'a Catalog, installed: &'a Installed) -> impl Iterator<Item = &'a str> {
    catalog.apps.iter().filter(|(name, app)| app.bundled.is_some() && app.here().is_some() && !installed.apps.contains_key(*name)).map(|(name, _)| name.as_str())
}

/// What [`keep_carried`] did, for the toast that says so.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CarriedKept {
    /// The apps it installed.
    pub kept: Vec<String>,
    /// Those it passed over ([`carried_in_the_way`]), each with the app in
    /// its way, which only the user can take off.
    pub passed_over: Vec<(String, String)>,
}

impl CarriedKept {
    /// What the toast says, where there's anything to say: what was kept,
    /// and what was left out and how to have it.
    pub fn words(&self) -> Option<String> {
        let kept = self.kept.join(", ");
        let left = self.passed_over.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>().join(", ");
        let others = self.passed_over.iter().map(|(_, other)| other.as_str()).collect::<Vec<_>>().join(", ");
        match (kept.is_empty(), left.is_empty()) {
            (true, true) => None,
            (false, true) => Some(crate::i18n::tf("toast.apps_kept", &[("apps", &kept)])),
            (true, false) => Some(crate::i18n::tf("toast.apps_passed_over", &[("apps", &left), ("others", &others)])),
            (false, false) => Some(crate::i18n::tf("toast.apps_kept_passed_over", &[("apps", &kept), ("left", &left), ("others", &others)])),
        }
    }
}

/// Keeps what an older Flow carried: reads the catalog at `place` again and
/// installs each of [`carried_to_keep`] under `root` with `key`, then notes
/// it's done. Says what it installed. Where one won't install (offline, or
/// a download that fails its checks) it's an error and nothing is noted,
/// so the next start tries again, with what did install kept. One whose
/// download needs a newer Flow than this one waits too: the rest install,
/// nothing is noted, and a start after Flow is updated installs it. One
/// something installed is in the way of is passed over, the rest
/// installed and noted, since no later start would install it either; it
/// says which, so that isn't the last word of it.
pub fn keep_carried(root: &Path, place: &str, key: &str) -> Result<CarriedKept, String> {
    if carried_settled(root) {
        return Ok(CarriedKept::default());
    }
    let read = catalog_afresh(root, place)?;
    let installed = Installed::load(root);
    let passed_over = carried_in_the_way(&read.catalog, &installed);
    let (held, kept): (Vec<String>, Vec<String>) = carried_to_keep(&read.catalog, &installed).into_iter().partition(|name| read.catalog.apps[name].here().is_some_and(|d| d.needs_newer_flow(crate::version::VERSION).is_some()));
    for name in &kept {
        install(root, name, &read.catalog.apps[name], key)?;
    }
    if held.is_empty() {
        settle_carried(root, &kept)?;
    }
    Ok(CarriedKept { kept, passed_over })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    pub(super) fn key() -> (SigningKey, String) {
        let signing = SigningKey::from_bytes(&[7; 32]);
        let public = STANDARD.encode(signing.verifying_key().to_bytes());
        (signing, public)
    }

    /// A download of `programs`, each a script saying `version`, as a
    /// `.tar.gz` (made with the system's tar) and what the catalog would say.
    pub(super) fn package(programs: &[&str], version: &str, signing: &SigningKey) -> (Vec<u8>, Download) {
        let dir = tempdir();
        std::fs::create_dir_all(dir.join("bin")).unwrap();
        for p in programs {
            let file = dir.join("bin").join(exe(p));
            std::fs::write(&file, format!("#!/bin/sh\necho {p} {version}\n")).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
            }
        }
        let tarball = dir.join("app.tar.gz");
        assert!(Command::new("tar").arg("-czf").arg(&tarball).arg("-C").arg(&dir).arg("bin").status().unwrap().success());
        let bytes = std::fs::read(&tarball).unwrap();
        let sha256 = Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect();
        let signature = STANDARD.encode(signing.sign(&bytes).to_bytes());
        let download = Download { version: version.into(), url: tarball.to_string_lossy().into(), size: bytes.len() as u64, sha256, signature, flow: String::new(), signed: String::new() };
        (bytes, download)
    }

    /// [`package`]'s download with `flowapp.json` holding `app_file`
    /// beside its `bin/`.
    fn package_with_app_file(programs: &[&str], version: &str, app_file: &str, signing: &SigningKey) -> (Vec<u8>, Download) {
        let (bytes, _) = package(programs, version, signing);
        let dir = tempdir();
        untar(&bytes, &dir).unwrap();
        std::fs::write(dir.join(APP_FILE), app_file).unwrap();
        let tarball = std::env::temp_dir().join(format!("{}.tar.gz", dir.file_name().unwrap().to_string_lossy()));
        assert!(Command::new("tar").arg("-czf").arg(&tarball).arg("-C").arg(&dir).arg(".").status().unwrap().success());
        let bytes = std::fs::read(&tarball).unwrap();
        let _ = (std::fs::remove_file(&tarball), std::fs::remove_dir_all(&dir));
        let sha256 = Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect();
        let signature = STANDARD.encode(signing.sign(&bytes).to_bytes());
        let download = Download { version: version.into(), size: bytes.len() as u64, sha256, signature, ..Download::default() };
        (bytes, download)
    }

    #[test]
    fn an_apps_hello_is_one_of_its_own_programs_named_by_its_download() {
        let (signing, _) = key();
        let root = tempdir();
        let programs = vec!["say".to_string(), "flow-say".to_string()];
        let (bytes, _) = package_with_app_file(&["say", "flow-say"], "0.1.2", r#"{"hello": ["flow-say", "--hello"], "later": 1}"#, &signing);
        install_bytes(&root, "flow-say", "0.1.2", &programs, &bytes).unwrap();
        let hello = hello(&root, "flow-say").expect("a hello");
        assert_eq!(hello.program, root.join("flow-say/0.1.2/bin").join(exe("flow-say")), "by its path in the version, never the PATH");
        assert_eq!(hello.args, ["--hello"]);
        // None without the file, or with none in it, or for an app not there.
        let (plain, _) = package(&["flow-mines"], "0.1.9", &signing);
        install_bytes(&root, "flow-mines", "0.1.9", &["flow-mines".into()], &plain).unwrap();
        assert_eq!(super::hello(&root, "flow-mines"), None);
        let (empty, _) = package_with_app_file(&["flow-rogue"], "0.1.46", "{}", &signing);
        install_bytes(&root, "flow-rogue", "0.1.46", &["flow-rogue".into()], &empty).unwrap();
        assert_eq!(super::hello(&root, "flow-rogue"), None);
        assert_eq!(super::hello(&root, "flow-gone"), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_download_whose_hello_isnt_its_own_is_refused_and_nothing_put_in_place() {
        let (signing, _) = key();
        let root = tempdir();
        let programs = vec!["flow-say".to_string()];
        for (app_file, why) in [(r#"{"hello": ["sh", "-c", "true"]}"#, "isn't one of its programs"), (r#"{"hello": "flow-say"}"#, APP_FILE), ("not json", APP_FILE)] {
            let (bytes, _) = package_with_app_file(&["flow-say"], "0.1.2", app_file, &signing);
            let error = install_bytes(&root, "flow-say", "0.1.2", &programs, &bytes).unwrap_err();
            assert!(error.contains(why), "{app_file}: {error}");
            assert!(!root.join("flow-say/0.1.2").exists() && !Installed::load(&root).apps.contains_key("flow-say"), "{app_file}");
        }
        // Its program named, but its download without it in bin/.
        let (bytes, _) = package_with_app_file(&["flow-say"], "0.1.2", r#"{"hello": ["say"]}"#, &signing);
        let error = install_bytes(&root, "flow-say", "0.1.2", &["flow-say".into(), "say".into()], &bytes).unwrap_err();
        assert!(error.contains("no bin/"), "{error}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_hello_runs_after_a_fresh_install_with_someone_there_alone() {
        assert!(hello_runs(true, true));
        assert!(!hello_runs(false, true), "an upgrade, or one installed again");
        assert!(!hello_runs(true, false), "a script, a pipe or the assistant");
    }

    pub(super) fn tempdir() -> PathBuf {
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("flowapp-test-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn an_apps_man_pages_are_found_in_its_version_and_a_themes_never() {
        let root = tempdir();
        let mut installed = Installed::default();
        let kept = |version: &str, kind: &str| Kept { version: version.into(), programs: vec![], kind: kind.into(), ..Kept::default() };
        installed.apps.insert("flow-utils".into(), kept("0.1.0", ""));
        installed.apps.insert("flow-rogue".into(), kept("0.1.46", ""));
        installed.apps.insert("old-looks".into(), kept("1.0.0", THEMES));
        installed.save(&root).unwrap();
        for dir in ["flow-utils/0.1.0/share/man/man1", "flow-utils/0.0.9/share/man/man1", "old-looks/1.0.0/share/man/man1"] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
        }
        assert_eq!(man_dirs(&root), vec![root.join("flow-utils/0.1.0/share/man")], "the installed version's, not an older one's; none for an app without pages or a theme");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn versions_go_number_by_number() {
        assert!(newer("0.1.10", "0.1.9"));
        assert!(newer("0.1.31.3", "0.1.31"));
        assert!(newer("v0.2", "0.1.99"));
        assert!(!newer("0.1.45", "0.1.45"));
        assert!(!newer("0.1.31", "0.1.31.0"));
        assert!(!newer("0.1.9", "0.1.10"));
    }

    #[test]
    fn a_catalog_reads_back_and_one_of_another_format_is_refused() {
        let mut catalog = Catalog { format: FORMAT, ..Catalog::default() };
        catalog.apps.insert("flow-rogue".into(), App { version: "0.1.45".into(), summary: "A roguelike in Flow's tabs".into(), programs: vec!["flow-rogue".into()], bundled: Some("flow-rogue".into()), ..App::default() });
        catalog.apps.insert("flow-sdk".into(), App { version: "0.1.0".into(), summary: "Flow's SDK for C and Python, and its demos".into(), ..App::default() });
        let text = serde_json::to_string(&catalog).unwrap();
        assert_eq!(Catalog::parse(&text).unwrap(), catalog);
        assert!(Catalog::parse(&text.replace("\"format\":1", "\"format\":2")).is_err());
        assert_eq!(catalog.search(&["ROGUE".into()]), ["flow-rogue"]);
        assert_eq!(catalog.search(&["flow".into(), "demos".into()]), ["flow-sdk"]);
        assert_eq!(catalog.search(&[]).len(), 2);
    }

    /// A package of themes holding `themes`'s `themes.json` (and `files`,
    /// each a path and its bytes), as a `.tar.gz`.
    #[cfg(unix)]
    fn theme_package(themes: &crate::looks::packaged::ThemeFile, files: &[(&str, &[u8])]) -> Vec<u8> {
        let dir = tempdir();
        std::fs::write(dir.join(crate::looks::packaged::FILE), serde_json::to_string(themes).unwrap()).unwrap();
        for (path, bytes) in files {
            std::fs::create_dir_all(dir.join(path).parent().unwrap()).unwrap();
            std::fs::write(dir.join(path), bytes).unwrap();
        }
        let tarball = std::env::temp_dir().join(format!("{}.tar.gz", dir.file_name().unwrap().to_string_lossy()));
        assert!(Command::new("tar").arg("-czf").arg(&tarball).arg("-C").arg(&dir).arg(".").status().unwrap().success());
        let bytes = std::fs::read(&tarball).unwrap();
        let _ = (std::fs::remove_file(&tarball), std::fs::remove_dir_all(&dir));
        bytes
    }

    #[cfg(unix)]
    fn a_theme(id: &str) -> crate::looks::packaged::ThemeEntry {
        crate::looks::packaged::ThemeEntry { id: id.into(), name: format!("The {id}"), ..Default::default() }
    }

    #[test]
    fn a_catalog_keeps_its_theme_packages_apart_and_finds_them_by_name() {
        let mut catalog = Catalog { format: FORMAT, ..Catalog::default() };
        catalog.apps.insert("flow-rogue".into(), App { summary: "A roguelike".into(), programs: vec!["flow-rogue".into()], ..App::default() });
        let mut themes = App { summary: "Flow's machines as themes".into(), ..App::default() };
        themes.downloads.insert(ANY.into(), Download { version: "0.1.0".into(), ..Download::default() });
        catalog.themes.insert("flow-core-themes".into(), themes);
        let text = serde_json::to_string(&catalog).unwrap();
        assert_eq!(Catalog::parse(&text).unwrap(), catalog);
        assert_eq!(catalog.names().collect::<Vec<_>>(), ["flow-rogue", "flow-core-themes"]);
        assert!(catalog.is_themes("flow-core-themes") && !catalog.is_themes("flow-rogue"));
        assert_eq!(catalog.get("flow-core-themes").and_then(App::here).map(|d| d.version.as_str()), Some("0.1.0"), "its download is for any system");
        assert_eq!(catalog.search(&["machines".into()]), ["flow-core-themes"]);
        // A flowapp from before reads past them, and a catalog without any
        // writes none.
        #[derive(serde::Deserialize)]
        #[allow(dead_code)]
        struct Before {
            format: u32,
            apps: BTreeMap<String, App>,
        }
        assert_eq!(serde_json::from_str::<Before>(&text).unwrap().apps.len(), 1);
        catalog.themes.clear();
        assert!(!serde_json::to_string(&catalog).unwrap().contains("themes"));
    }

    #[test]
    fn a_catalog_keeps_its_packages_of_skills_apart_too() {
        let mut catalog = Catalog { format: FORMAT, ..Catalog::default() };
        catalog.apps.insert("flow-utils".into(), App { summary: "Commands".into(), programs: vec!["tree".into()], ..App::default() });
        let mut skills = App { summary: "Flow's own skills for its assistant".into(), ..App::default() };
        skills.downloads.insert(ANY.into(), Download { version: "0.1.0".into(), ..Download::default() });
        catalog.skills.insert("flow-skills".into(), skills);
        let text = serde_json::to_string(&catalog).unwrap();
        assert_eq!(Catalog::parse(&text).unwrap(), catalog);
        assert_eq!(catalog.names().collect::<Vec<_>>(), ["flow-utils", "flow-skills"]);
        assert!(catalog.is_skills("flow-skills") && !catalog.is_skills("flow-utils") && !catalog.is_themes("flow-skills"));
        assert_eq!(catalog.get("flow-skills").and_then(App::here).map(|d| d.version.as_str()), Some("0.1.0"));
        assert_eq!(catalog.search(&["assistant".into()]), ["flow-skills"]);
        catalog.skills.clear();
        assert!(!serde_json::to_string(&catalog).unwrap().contains("skills"));
    }

    /// A package of skills named `package`, version `version`, holding
    /// `files` (each a path and its text) beside its `skills.json`, as a
    /// `.tar.gz`; with no package, `files` alone.
    #[cfg(unix)]
    fn skill_package(package: Option<(&str, &str)>, files: &[(&str, &str)]) -> Vec<u8> {
        let dir = tempdir();
        if let Some((package, version)) = package {
            let manifest = crate::ai::skills::Manifest { format: crate::ai::skills::FORMAT, package: package.into(), version: version.into() };
            std::fs::write(dir.join(crate::ai::skills::FILE), serde_json::to_string(&manifest).unwrap()).unwrap();
        }
        for (path, text) in files {
            std::fs::create_dir_all(dir.join(path).parent().unwrap()).unwrap();
            std::fs::write(dir.join(path), text).unwrap();
        }
        let tarball = std::env::temp_dir().join(format!("{}.tar.gz", dir.file_name().unwrap().to_string_lossy()));
        assert!(Command::new("tar").arg("-czf").arg(&tarball).arg("-C").arg(&dir).arg(".").status().unwrap().success());
        let bytes = std::fs::read(&tarball).unwrap();
        let _ = (std::fs::remove_file(&tarball), std::fs::remove_dir_all(&dir));
        bytes
    }

    #[cfg(unix)]
    const A_SKILL: &str = "---\nname: theme-package\ndescription: Making a package of themes.\n---\n1. Write themes.json.\n";

    #[test]
    #[cfg(unix)]
    fn a_package_of_skills_installs_as_skills_and_one_with_none_that_reads_is_refused() {
        let root = tempdir();
        let bytes = skill_package(Some(("flow-skills", "0.1.0")), &[("skills/theme-package/SKILL.md", A_SKILL)]);
        let kept = install_bytes(&root, "flow-skills", "0.1.0", &[], &bytes).unwrap();
        assert!(kept.skills() && !kept.themes() && kept.programs.is_empty());
        assert_eq!(skill_dirs(&root), [("flow-skills".to_string(), root.join("flow-skills/0.1.0/skills"))]);
        // None that reads, and it's refused, the one before staying.
        let none = skill_package(Some(("flow-skills", "0.1.1")), &[("skills/notes.md", "no front matter\n")]);
        assert!(install_bytes(&root, "flow-skills", "0.1.1", &[], &none).unwrap_err().contains("no skill that reads"));
        assert_eq!(Installed::load(&root).apps["flow-skills"].version, "0.1.0");
        assert_eq!(remove(&root, "flow-skills"), Ok(true));
        assert!(skill_dirs(&root).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    #[cfg(unix)]
    fn an_app_may_carry_skills_beside_its_programs() {
        let root = tempdir();
        let (bytes, _) = package(&["tree"], "0.3.0", &key().0);
        install_bytes(&root, "flow-utils", "0.3.0", &["tree".into()], &bytes).unwrap();
        assert!(skill_dirs(&root).is_empty(), "an app without skills/ brings none");
        // The same app with a skills/ folder (its download unpacked as it
        // would be) brings them, and is still an app.
        let with = root.join("flow-utils/0.3.0/skills/tree");
        std::fs::create_dir_all(&with).unwrap();
        std::fs::write(with.join("SKILL.md"), "---\nname: tree\ndescription: Showing a folder as a tree.\n---\ntree -L 2\n").unwrap();
        assert_eq!(skill_dirs(&root), [("flow-utils".to_string(), root.join("flow-utils/0.3.0/skills"))]);
        assert!(!Installed::load(&root).apps["flow-utils"].skills());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    #[cfg(unix)]
    fn a_package_of_skills_of_your_own_installs_from_its_file_but_never_a_program() {
        let root = tempdir();
        let tarball = root.join("mine.tar.gz");
        std::fs::write(&tarball, skill_package(Some(("my-skills", "1.2")), &[("skills/theme.md", A_SKILL)])).unwrap();
        let (name, kept) = install_file(&root, &tarball).unwrap();
        assert_eq!((name.as_str(), kept.version.as_str(), kept.skills()), ("my-skills", "1.2", true));
        let sneaky = skill_package(Some(("my-skills", "1.3")), &[("skills/theme.md", A_SKILL), ("bin/tree", "#!/bin/sh\n")]);
        std::fs::write(&tarball, sneaky).unwrap();
        assert!(install_file(&root, &tarball).unwrap_err().contains("programs"));
        let unnamed = skill_package(None, &[("skills/theme.md", A_SKILL)]);
        std::fs::write(&tarball, unnamed).unwrap();
        assert!(install_file(&root, &tarball).unwrap_err().contains("skills.json"));
        assert!(!root.join("bin").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    #[cfg(unix)]
    fn a_theme_package_installs_as_themes_and_one_that_wont_read_is_refused() {
        let root = tempdir();
        let mut file = crate::looks::packaged::ThemeFile { format: crate::looks::packaged::FORMAT, package: "my-themes".into(), version: "1.0".into(), themes: vec![a_theme("vt52")] };
        file.themes[0].logo = "logos/vt52.png".into();
        let bytes = theme_package(&file, &[("logos/vt52.png", b"not really a PNG")]);
        let kept = install_bytes(&root, "my-themes", "1.0", &[], &bytes).unwrap();
        assert!(kept.themes() && kept.programs.is_empty());
        assert!(Installed::load(&root).apps["my-themes"].themes());
        let themes = crate::looks::packaged::installed(&root);
        assert_eq!(themes.iter().map(|t| t.look.id).collect::<Vec<_>>(), ["vt52"]);
        assert_eq!(themes[0].logo, Some(root.join("my-themes/1.0/logos/vt52.png")));
        // One naming a file it hasn't is refused, and the one before stays.
        file.version = "1.1".into();
        file.themes[0].logo = "logos/missing.png".into();
        assert!(install_bytes(&root, "my-themes", "1.1", &[], &theme_package(&file, &[])).is_err());
        assert_eq!(Installed::load(&root).apps["my-themes"].version, "1.0");
        assert_eq!(remove(&root, "my-themes"), Ok(true));
        assert!(crate::looks::packaged::installed(&root).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    #[cfg(unix)]
    fn a_theme_package_of_your_own_installs_from_its_file_but_never_a_program() {
        let root = tempdir();
        let file = crate::looks::packaged::ThemeFile { format: crate::looks::packaged::FORMAT, package: "mine".into(), version: "0.2".into(), themes: vec![a_theme("paper")] };
        let tarball = root.join("mine.tar.gz");
        std::fs::write(&tarball, theme_package(&file, &[])).unwrap();
        let (name, kept) = install_file(&root, &tarball).unwrap();
        assert_eq!((name.as_str(), kept.version.as_str(), kept.themes()), ("mine", "0.2", true));
        // Signed by no one, which installed.json keeps.
        assert!(kept.own_file && kept.download.is_none() && kept.key.is_empty());
        assert!(Installed::load(&root).apps["mine"].own_file);
        // A program in it, or no themes at all, and it's refused.
        let sneaky = theme_package(&file, &[("bin/paper", b"#!/bin/sh\necho hi\n")]);
        std::fs::write(&tarball, sneaky).unwrap();
        assert!(install_file(&root, &tarball).is_err());
        let (none, _) = package(&["flow-mines"], "0.1.6", &key().0);
        std::fs::write(&tarball, none).unwrap();
        assert!(install_file(&root, &tarball).is_err());
        assert!(!root.join("bin").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A package of themes of the user's own called `package`, at
    /// `version`, as its `.tar.gz` file in `dir`.
    #[cfg(unix)]
    fn own_themes_file(dir: &Path, package: &str, version: &str) -> PathBuf {
        let file = crate::looks::packaged::ThemeFile { format: crate::looks::packaged::FORMAT, package: package.into(), version: version.into(), themes: vec![a_theme("paper")] };
        let tarball = dir.join(format!("{}.tar.gz", package.replace(['/', '\\', ':'], "_")));
        std::fs::write(&tarball, theme_package(&file, &[])).unwrap();
        tarball
    }

    #[test]
    #[cfg(unix)]
    fn a_package_of_your_own_cant_take_the_name_of_one_of_flows() {
        crate::i18n::set_language("en", &[]);
        let root = tempdir();
        let files = tempdir();
        // Flow Rogue and Flow Mines installed from the catalog, which is
        // read, though it no longer has Flow Mines.
        let (bytes, _) = package(&["flow-rogue"], "0.1.46", &key().0);
        install_bytes(&root, "flow-rogue", "0.1.46", &["flow-rogue".into()], &bytes).unwrap();
        let (bytes, _) = package(&["flow-mines"], "0.1.9", &key().0);
        install_bytes(&root, "flow-mines", "0.1.9", &["flow-mines".into()], &bytes).unwrap();
        let mut catalog = Catalog { format: FORMAT, ..Catalog::default() };
        for name in ["flow-rogue", "flow-sdk"] {
            catalog.apps.insert(name.into(), App::default());
        }
        catalog.themes.insert("flow-core-themes".into(), App::default());
        catalog.skills.insert("flow-skills".into(), App::default());
        std::fs::write(root.join("catalog.json"), serde_json::to_string(&catalog).unwrap()).unwrap();
        let skills = files.join("flow-skills.tar.gz");
        std::fs::write(&skills, skill_package(Some(("flow-skills", "9.0")), &[("skills/theme.md", A_SKILL)])).unwrap();
        // The catalog's app, its theme package, its skills, and flowapp
        // itself: each refused, saying why, nothing changed; and one
        // installed from the catalog, which it no longer has.
        for file in [own_themes_file(&files, "flow-rogue", "9.0"), own_themes_file(&files, "flow-sdk", "9.0"), own_themes_file(&files, "flow-core-themes", "9.0"), skills, own_themes_file(&files, "flowapp", "9.0")] {
            let error = install_file(&root, &file).unwrap_err();
            assert!(error.contains("one of Flow's own"), "{}: {error}", file.display());
        }
        let error = install_file(&root, &own_themes_file(&files, "flow-mines", "9.0")).unwrap_err();
        assert!(error.contains("installed from the catalog") && error.contains("flowapp remove flow-mines"), "{error}");
        let installed = Installed::load(&root);
        assert_eq!(installed.apps.keys().collect::<Vec<_>>(), ["flow-mines", "flow-rogue"]);
        assert!(!installed.apps["flow-rogue"].own_file && installed.apps["flow-rogue"].version == "0.1.46");
        assert_eq!(version_of(&root.join("bin/flow-rogue")).as_deref(), Some("0.1.46"), "its program still in bin/");
        assert_eq!(version_of(&root.join("bin/flow-mines")).as_deref(), Some("0.1.9"));
        for name in ["flow-sdk", "flow-core-themes", "flow-skills", "flowapp"] {
            assert!(!root.join(name).exists(), "{name}");
        }
        // Nor in another case, which a Mac's or Windows' folders don't tell
        // apart from it, nor with a name that isn't one.
        for name in ["Flow-Rogue", "FLOW-SDK", "Bin", "../evil", "C:", "bin "] {
            let error = install_file(&root, &own_themes_file(&files, name, "1.0")).unwrap_err();
            assert!(error.contains("isn't a name"), "{name}: {error}");
        }
        assert_eq!(Installed::load(&root).apps.len(), 2);
        // A name of its own installs, and again over itself.
        let mine = own_themes_file(&files, "my-themes", "1.0");
        assert_eq!(install_file(&root, &mine).map(|(name, kept)| (name, kept.own_file)), Ok(("my-themes".to_string(), true)));
        assert!(install_file(&root, &mine).is_ok());
        let _ = (std::fs::remove_dir_all(&root), std::fs::remove_dir_all(&files));
    }

    #[test]
    #[cfg(unix)]
    fn the_catalogs_package_never_installs_over_one_of_your_own_of_its_name() {
        crate::i18n::set_language("en", &[]);
        let root = tempdir();
        let files = tempdir();
        // With no catalog read yet, only flowapp's own name is known to be
        // Flow's: a package of one's own can be called flow-core-themes.
        let file = own_themes_file(&files, "flow-core-themes", "99.0");
        assert!(install_file(&root, &file).is_ok());
        assert!(install_file(&root, &own_themes_file(&files, "flowapp", "1.0")).is_err());
        // The catalog's then won't install over it, newer or not, nor is it
        // fetched to be refused.
        let (signing, public) = key();
        let theirs = crate::looks::packaged::ThemeFile { format: crate::looks::packaged::FORMAT, package: "flow-core-themes".into(), version: "0.1.0".into(), themes: vec![a_theme("amber")] };
        let bytes = theme_package(&theirs, &[]);
        let at = files.join("theirs.tar.gz");
        std::fs::write(&at, &bytes).unwrap();
        let download = Download { version: "0.1.0".into(), url: at.to_string_lossy().into(), size: bytes.len() as u64, sha256: sha256(&bytes), signature: STANDARD.encode(signing.sign(&bytes).to_bytes()), ..Download::default() };
        let mut themes = App { version: "0.1.0".into(), ..App::default() };
        themes.downloads.insert(ANY.into(), download);
        let mut steps = 0;
        let error = install_with(&root, "flow-core-themes", &themes, &public, &mut |_| steps += 1).unwrap_err().to_string();
        assert!(error.contains("a package of your own") && error.contains("flowapp remove flow-core-themes"), "{error}");
        assert_eq!(steps, 0, "refused before it's fetched");
        assert!(install_bytes(&root, "flow-core-themes", "0.1.0", &[], &bytes).is_err());
        let kept = &Installed::load(&root).apps["flow-core-themes"];
        assert!(kept.own_file && kept.version == "99.0", "yours stays as it was");
        // Nor is it offered as an upgrade: what's installed isn't the
        // catalog's.
        let mut catalog = Catalog { format: FORMAT, ..Catalog::default() };
        catalog.themes.insert("flow-core-themes".into(), App { version: "100.0".into(), downloads: [(ANY.to_string(), Download { version: "100.0".into(), ..Download::default() })].into(), ..App::default() });
        assert!(shelf::upgradable(&Installed::load(&root), &catalog, None).is_empty());
        // Removed, the catalog's installs, and the name is the catalog's.
        assert_eq!(remove(&root, "flow-core-themes"), Ok(true));
        let kept = install_with(&root, "flow-core-themes", &themes, &public, &mut |_| {}).unwrap();
        assert!(!kept.own_file && kept.themes() && kept.version == "0.1.0");
        assert!(install_file(&root, &file).unwrap_err().contains("installed from the catalog"));
        let _ = (std::fs::remove_dir_all(&root), std::fs::remove_dir_all(&files));
    }

    #[test]
    fn names_and_versions_that_arent_a_folders_own_are_refused_on_every_system() {
        for name in ["flow-rogue", "flow_sdk", "mine", "night-shift-theme", "2048", "a"] {
            assert!(fit_name(name), "{name}");
        }
        let escape = "\u{1b}]0;hi\u{7}";
        for name in ["", "Bin", "bin", "first", "previews", "Flow-Rogue", "../evil", "a/b", "a\\b", "C:", "c:", "a:b", ".hidden", "-rf", "_x", "a.b", "bin ", "my themes", "con", "nul", "com1", "lpt9", "é", escape] {
            assert!(!fit_name(name), "{name:?}");
        }
        // Versions as Flow's SDK makes them, a semver's capitals among them.
        for version in ["0.1.45", "1.0", "1.0-beta+2", "v2", "V2", "2026_11", "0.1.31.3", "1.0-rc1", "0.6.0-RC1", "-1", "+1"] {
            assert!(fit_version(version), "{version}");
        }
        // But never one Windows takes for another (its trailing dot gone),
        // nor a device's.
        let long = "1".repeat(65);
        for version in ["", ".", "..", ".1", "1.0.", "1..", "../0.1.0", "1/2", "1\\2", "C:", "1:2", "1.0 ", "con", "con.1", "CON.1", "nul.txt", "Lpt1", long.as_str(), escape] {
            assert!(!fit_version(version), "{version:?}");
        }
        assert!(fit_name(&"a".repeat(64)) && !fit_name(&"a".repeat(65)));
        for program in ["flow-rogue", "[", "tree", "Flow Say"] {
            assert!(fit_program(program), "{program}");
        }
        for program in ["", ".x", "../x", "a/b", "a\\b", "C:x", "con", "AUX.txt", escape] {
            assert!(!fit_program(program), "{program:?}");
        }
        // What's refused is said with its escapes spelled out.
        crate::i18n::set_language("en", &[]);
        let root = tempdir();
        let error = install_bytes(&root, escape, "1.0", &[], &[]).unwrap_err();
        assert!(!error.contains('\u{1b}') && error.contains("\\u{1b}"), "{error:?}");
        let error = install_bytes(&root, "mine", escape, &[], &[]).unwrap_err();
        assert!(!error.contains('\u{1b}') && error.contains("isn't a version"), "{error:?}");
        assert!(std::fs::read_dir(&root).unwrap().next().is_none(), "nothing made");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    #[cfg(unix)]
    fn a_program_is_a_file_in_bin_never_a_path() {
        let root = tempdir();
        let (bytes, _) = package(&["tree"], "0.1.0", &key().0);
        for program in ["../tree", "bin/tree", ".tree", "", "a:b", "con", "Nul.x"] {
            assert!(install_bytes(&root, "flow-utils", "0.1.0", &[program.into()], &bytes).is_err(), "{program:?}");
        }
        assert!(!root.join("flow-utils").exists() && !Installed::path(&root).exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_name_installed_before_names_were_checked_is_taken_and_removed_leaves_its_folder() {
        crate::i18n::set_language("en", &[]);
        let root = tempdir();
        // A package named Bin, from before: on a Mac its folder is bin/.
        std::fs::create_dir_all(root.join("bin")).unwrap();
        std::fs::write(root.join("bin").join(exe("flow-mines")), "").unwrap();
        let mut installed = Installed::default();
        installed.apps.insert("Bin".into(), Kept { version: "1.0".into(), kind: THEMES.into(), own_file: true, ..Kept::default() });
        installed.apps.insert("Flow-Rogue".into(), Kept { version: "1.0".into(), kind: THEMES.into(), own_file: true, ..Kept::default() });
        installed.save(&root).unwrap();
        let installed = Installed::load(&root);
        let taken = in_the_way(&installed, "flow-rogue", false, None).unwrap();
        assert!(taken.contains("Flow-Rogue is installed"), "{taken}");
        assert!(in_the_way(&installed, "flow-rogue", true, None).is_some());
        assert_eq!(in_the_way(&installed, "flow-mines", false, None), None);
        assert_eq!(remove(&root, "Bin"), Ok(true));
        assert!(root.join("bin").join(exe("flow-mines")).exists(), "flowapp's own folder stays");
        assert!(!Installed::load(&root).apps.contains_key("Bin"));
        // The one installed by its very name decides, where both are.
        let mut installed = Installed::load(&root);
        installed.apps.insert("flow-rogue".into(), Kept { version: "0.1.46".into(), programs: vec!["flow-rogue".into()], ..Kept::default() });
        assert_eq!(in_the_way(&installed, "flow-rogue", false, None), None);
        assert!(in_the_way(&installed, "flow-rogue", true, None).unwrap().contains("installed from the catalog"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_name_from_before_names_were_checked_leaves_its_folder_only_where_it_may_be_anothers() {
        let root = tempdir();
        let mut installed = Installed::default();
        for name in ["MyThemes", "my themes", "Bin", "CON", "C:", "night.", "Flow-Rogue", "flow-rogue", "mine"] {
            std::fs::create_dir_all(root.join(name.replace(':', "_")).join("1.0")).unwrap();
            installed.apps.insert(name.into(), Kept { version: "1.0".into(), kind: THEMES.into(), own_file: true, ..Kept::default() });
        }
        installed.save(&root).unwrap();
        // Taken off, each its own: one named as flowapp names them, and
        // ones from before that can't be anyone else's.
        for name in ["MyThemes", "my themes", "mine"] {
            assert_eq!(remove(&root, name), Ok(true));
            assert!(!root.join(name).exists(), "{name}");
        }
        // One that may be flowapp's own folder, a device, a drive, another
        // version's (Windows drops the dot) or another app's but for its
        // case, still installed: taken off, its folder left.
        for name in ["Bin", "CON", "C:", "night.", "flow-rogue"] {
            assert_eq!(remove(&root, name), Ok(true));
        }
        assert!(root.join("Bin").exists() && root.join("CON").exists() && root.join("night.").exists() && root.join("flow-rogue").exists());
        assert_eq!(Installed::load(&root).apps.keys().collect::<Vec<_>>(), ["Flow-Rogue"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    #[cfg(unix)]
    fn a_package_of_your_own_from_before_it_was_marked_installs_again_from_its_file() {
        crate::i18n::set_language("en", &[]);
        let root = tempdir();
        let files = tempdir();
        // As flowapps before 0.2.0 wrote one: no mark, no download.
        let mine = own_themes_file(&files, "night-shift-theme", "1.0");
        install_file(&root, &mine).unwrap();
        std::fs::write(Installed::path(&root), r#"{"apps":{"night-shift-theme":{"version":"1.0","programs":[],"kind":"themes"}}}"#).unwrap();
        // With no catalog read, it can't be told from the catalog's: so
        // it's refused, saying how to clear it.
        let error = install_file(&root, &mine).unwrap_err();
        assert!(error.contains("installed from the catalog") && error.contains("flowapp remove night-shift-theme"), "{error}");
        // With one that doesn't have it, it's the user's, and installs
        // again, marked now.
        let mut catalog = Catalog { format: FORMAT, ..Catalog::default() };
        catalog.themes.insert("flow-core-themes".into(), App::default());
        std::fs::write(root.join("catalog.json"), serde_json::to_string(&catalog).unwrap()).unwrap();
        let (_, kept) = install_file(&root, &own_themes_file(&files, "night-shift-theme", "1.1")).unwrap();
        assert!(kept.own_file && kept.version == "1.1");
        // And the catalog's of the same name is then refused over it.
        let saved = saved_catalog(&root).map(|read| read.catalog);
        assert!(in_the_way(&Installed::load(&root), "night-shift-theme", false, saved.as_ref()).is_some());
        let _ = (std::fs::remove_dir_all(&root), std::fs::remove_dir_all(&files));
    }

    #[test]
    fn only_a_package_of_themes_or_skills_without_programs_or_a_download_was_the_users_own_before_it_was_marked() {
        let mut catalog = Catalog { format: FORMAT, ..Catalog::default() };
        catalog.themes.insert("flow-core-themes".into(), App::default());
        let themes = Kept { version: "1.0".into(), kind: THEMES.into(), ..Kept::default() };
        let skills = Kept { kind: SKILLS.into(), ..themes.clone() };
        assert!(themes.own("night", Some(&catalog)) && skills.own("night", Some(&catalog)));
        // Not one the catalog has, nor without a catalog to tell.
        assert!(!themes.own("flow-core-themes", Some(&catalog)) && !themes.own("night", None));
        // Nor one with programs, a download or a key, which only the
        // catalog's came with.
        let app = Kept { version: "1.0".into(), programs: vec!["flow-mines".into()], ..Kept::default() };
        let downloaded = Kept { download: Some(Download::default()), ..themes.clone() };
        let keyed = Kept { key: "ab".into(), ..themes.clone() };
        assert!(!app.own("flow-mines", Some(&catalog)) && !downloaded.own("night", Some(&catalog)) && !keyed.own("night", Some(&catalog)));
        // One marked is the user's own whatever the catalog says.
        assert!(Kept { own_file: true, ..app }.own("flow-mines", None));
    }

    #[test]
    #[cfg(unix)]
    fn a_package_of_your_own_never_brings_a_theme_in_place_of_another() {
        crate::i18n::set_language("en", &[]);
        let root = tempdir();
        let files = tempdir();
        let file = |package: &str, ids: &[&str]| {
            let themes = crate::looks::packaged::ThemeFile { format: crate::looks::packaged::FORMAT, package: package.into(), version: "1.0".into(), themes: ids.iter().map(|id| a_theme(id)).collect() };
            let tarball = files.join(format!("{package}.tar.gz"));
            std::fs::write(&tarball, theme_package(&themes, &[])).unwrap();
            tarball
        };
        // One of Flow's own machines, in any package.
        let error = install_file(&root, &file("aaa-themes", &["paper", "green"])).unwrap_err();
        assert!(error.contains("“green”") && error.contains("one of Flow's own themes"), "{error}");
        // One another package brings, the catalog's or one's own.
        let theirs = crate::looks::packaged::ThemeFile { format: crate::looks::packaged::FORMAT, package: "zzz-themes".into(), version: "1.0".into(), themes: vec![a_theme("night")] };
        install_bytes(&root, "zzz-themes", "1.0", &[], &theme_package(&theirs, &[])).unwrap();
        let error = install_file(&root, &file("aaa-themes", &["night"])).unwrap_err();
        assert!(error.contains("“night”") && error.contains("zzz-themes"), "{error}");
        assert!(install_file(&root, &file("mine", &["paper"])).is_ok());
        assert!(install_file(&root, &file("aaa-themes", &["paper"])).unwrap_err().contains("mine"));
        // Its own again, at another version, takes its own place.
        assert!(install_file(&root, &file("mine", &["paper", "ink"])).is_ok());
        assert_eq!(Installed::load(&root).apps.keys().collect::<Vec<_>>(), ["mine", "zzz-themes"]);
        let _ = (std::fs::remove_dir_all(&root), std::fs::remove_dir_all(&files));
    }

    #[test]
    #[cfg(unix)]
    fn a_refused_install_leaves_no_folder_of_its_name_behind() {
        crate::i18n::set_language("en", &[]);
        let root = tempdir();
        let files = tempdir();
        let themes = |package: &str, version: &str, ids: &[&str]| crate::looks::packaged::ThemeFile { format: crate::looks::packaged::FORMAT, package: package.into(), version: version.into(), themes: ids.iter().map(|id| a_theme(id)).collect() };
        let file = |themes: &crate::looks::packaged::ThemeFile| {
            let tarball = files.join(format!("{}-{}.tar.gz", themes.package, themes.version));
            std::fs::write(&tarball, theme_package(themes, &[])).unwrap();
            tarball
        };
        // Refused where it was to be the first version: one of your own
        // bringing one of Flow's machines, a theme that won't read, and a
        // program its download hasn't.
        assert!(install_file(&root, &file(&themes("aaa-themes", "1.0", &["green"]))).unwrap_err().contains("“green”"));
        let mut unread = themes("my-themes", "1.0", &["vt52"]);
        unread.themes[0].logo = "logos/missing.png".into();
        assert!(install_bytes(&root, "my-themes", "1.0", &[], &theme_package(&unread, &[])).is_err());
        let (bytes, _) = package(&["tree"], "0.3.0", &key().0);
        assert!(install_bytes(&root, "flow-utils", "0.3.0", &["tree".into(), "du".into()], &bytes).unwrap_err().contains("bin/du"));
        for name in ["aaa-themes", "my-themes", "flow-utils"] {
            assert!(!root.join(name).exists(), "{name}");
        }
        assert!(!root.join("bin").exists());
        // Refused over a version installed, which stays, alone in its
        // app's folder.
        install_file(&root, &file(&themes("mine", "1.0", &["paper"]))).unwrap();
        assert!(install_file(&root, &file(&themes("mine", "1.1", &["amber"]))).is_err());
        assert!(app_dir(&root, "mine", "1.0").join(crate::looks::packaged::FILE).is_file());
        assert_eq!(std::fs::read_dir(root.join("mine")).unwrap().count(), 1, "nothing unpacked beside it");
        assert_eq!(Installed::load(&root).apps["mine"].version, "1.0");
        let _ = (std::fs::remove_dir_all(&root), std::fs::remove_dir_all(&files));
    }

    /// A package of themes of the user's own called `package` at
    /// `version`, its one theme `id`, holding each of `folders` with a file
    /// in it, made read-only as anyone's tarball may have them: its
    /// `.tar.gz` file in `dir`.
    #[cfg(unix)]
    fn read_only_themes_file(dir: &Path, package: &str, version: &str, id: &str, folders: &[&str]) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let from = tempdir();
        let themes = crate::looks::packaged::ThemeFile { format: crate::looks::packaged::FORMAT, package: package.into(), version: version.into(), themes: vec![a_theme(id)] };
        std::fs::write(from.join(crate::looks::packaged::FILE), serde_json::to_string(&themes).unwrap()).unwrap();
        for folder in folders {
            std::fs::create_dir_all(from.join(folder)).unwrap();
            std::fs::write(from.join(folder).join("SKILL.md"), "---\nname: left\ndescription: Left behind\n---\nHello.\n").unwrap();
        }
        for folder in folders {
            std::fs::set_permissions(from.join(folder), std::fs::Permissions::from_mode(0o555)).unwrap();
        }
        let tarball = dir.join(format!("{package}-{version}.tar.gz"));
        assert!(Command::new("tar").arg("-czf").arg(&tarball).arg("-C").arg(&from).arg(".").status().unwrap().success());
        for folder in folders {
            std::fs::set_permissions(from.join(folder), std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        std::fs::remove_dir_all(&from).unwrap();
        tarball
    }

    /// Makes `folder` under `root`, a file in it, and gives it `mode`.
    #[cfg(unix)]
    fn left_folder(root: &Path, folder: &str, mode: u32) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir_all(root.join(folder)).unwrap();
        std::fs::write(root.join(folder).join("SKILL.md"), "left").unwrap();
        std::fs::set_permissions(root.join(folder), std::fs::Permissions::from_mode(mode)).unwrap();
    }

    #[test]
    #[cfg(unix)]
    fn taking_a_folder_away_empties_ones_made_read_only_and_never_follows_a_link_out_of_it() {
        use std::os::unix::fs::PermissionsExt;
        crate::i18n::set_language("en", &[]);
        let mode = |path: &Path| std::fs::symlink_metadata(path).unwrap().permissions().mode() & 0o777;
        let gone = |path: &Path| std::fs::symlink_metadata(path).is_err();
        let dir = tempdir();
        let outside = tempdir();
        std::fs::write(outside.join("keep.txt"), "mine").unwrap();
        let tree = dir.join("tree");
        std::fs::create_dir_all(tree.join("a")).unwrap();
        std::os::unix::fs::symlink(&outside, tree.join("a/out")).unwrap();
        left_folder(&tree, "a/b", 0o000);
        std::fs::set_permissions(tree.join("a"), std::fs::Permissions::from_mode(0o555)).unwrap();
        std::fs::set_permissions(&tree, std::fs::Permissions::from_mode(0o500)).unwrap();
        std::fs::set_permissions(&outside, std::fs::Permissions::from_mode(0o555)).unwrap();
        take_away(&tree).unwrap();
        assert!(gone(&tree));
        assert!(outside.join("keep.txt").is_file() && mode(&outside) == 0o555, "what a link led to is as it was");
        // Nothing there is nothing to do; a file or a link goes as itself.
        take_away(&tree).unwrap();
        std::fs::write(dir.join("file"), "x").unwrap();
        take_away(&dir.join("file")).unwrap();
        std::os::unix::fs::symlink(&outside, dir.join("link")).unwrap();
        take_away(&dir.join("link")).unwrap();
        assert!(gone(&dir.join("file")) && gone(&dir.join("link")) && outside.join("keep.txt").is_file());
        // One its folder won't let go of is said, rather than taken as gone
        // (where permissions hold: not for root).
        left_folder(&dir, "held/stuck", 0o755);
        std::fs::set_permissions(dir.join("held"), std::fs::Permissions::from_mode(0o555)).unwrap();
        if std::fs::write(dir.join("held/probe"), "").is_err() {
            let error = take_away(&dir.join("held/stuck")).unwrap_err();
            assert!(error.contains("couldn't be taken away"), "{error}");
        }
        std::fs::set_permissions(dir.join("held"), std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::set_permissions(&outside, std::fs::Permissions::from_mode(0o755)).unwrap();
        take_away(&dir).unwrap();
        take_away(&outside).unwrap();
    }

    #[test]
    #[cfg(unix)]
    fn a_refused_file_holding_a_read_only_folder_leaves_nothing_and_the_next_install_is_clean() {
        use std::os::unix::fs::PermissionsExt;
        crate::i18n::set_language("en", &[]);
        let root = tempdir();
        let files = tempdir();
        // Refused for one of Flow's machines' ids, once it's unpacked where
        // it would go; then the same name and version, a good one, holds
        // nothing of it.
        let refused = read_only_themes_file(&files, "aaa-themes", "1.0", "green", &["skills", "skills/left"]);
        assert!(install_file(&root, &refused).unwrap_err().contains("“green”"));
        assert!(!root.join("aaa-themes").exists() && !root.join(".file.unpacking").exists());
        install_file(&root, &read_only_themes_file(&files, "aaa-themes", "1.0", "paper", &[])).unwrap();
        assert!(!app_dir(&root, "aaa-themes", "1.0").join("skills").exists());
        // Refused for a read-only bin/ as it's looked at, and the next file
        // of your own still installs, rather than every one after said to
        // hold programs.
        let programs = read_only_themes_file(&files, "zzz", "1.0", "zzz", &["bin"]);
        assert!(install_file(&root, &programs).unwrap_err().contains("bin/"));
        assert!(!root.join(".file.unpacking").exists() && !root.join("zzz").exists());
        install_file(&root, &read_only_themes_file(&files, "my-themes", "1.0", "sepia", &[])).unwrap();
        // What an older flowapp's refusals left is taken away first, never
        // unpacked over.
        left_folder(&root, ".file.unpacking/bin", 0o555);
        left_folder(&root, "mine/.1.0.unpacking/skills/left", 0o555);
        install_file(&root, &read_only_themes_file(&files, "mine", "1.0", "dusk", &[])).unwrap();
        assert!(!app_dir(&root, "mine", "1.0").join("skills").exists() && !root.join(".file.unpacking").exists());
        assert_eq!(std::fs::read_dir(root.join("mine")).unwrap().count(), 1);
        // One installed with a read-only folder goes whole as it's removed.
        install_file(&root, &read_only_themes_file(&files, "kept", "1.0", "night", &["extra"])).unwrap();
        assert!(app_dir(&root, "kept", "1.0").join("extra/SKILL.md").is_file());
        assert_eq!(remove(&root, "kept"), Ok(true));
        assert!(!root.join("kept").exists());
        // What can't be taken away is said, and nothing's unpacked over it
        // (where permissions hold: not for root).
        left_folder(&root, "held/.1.0.unpacking/skills/left", 0o555);
        std::fs::set_permissions(root.join("held"), std::fs::Permissions::from_mode(0o555)).unwrap();
        if std::fs::write(root.join("held/probe"), "").is_err() {
            let error = install_file(&root, &read_only_themes_file(&files, "held", "1.0", "slate", &[])).unwrap_err();
            assert!(error.contains("couldn't be taken away"), "{error}");
            assert!(!Installed::load(&root).apps.contains_key("held") && !app_dir(&root, "held", "1.0").exists());
        }
        std::fs::set_permissions(root.join("held"), std::fs::Permissions::from_mode(0o755)).unwrap();
        let installed = Installed::load(&root);
        assert_eq!(installed.apps.keys().collect::<Vec<_>>(), ["aaa-themes", "mine", "my-themes"]);
        take_away(&root).unwrap();
        take_away(&files).unwrap();
    }

    /// Gives what's at `path` (a link itself, where `link`) an ACL denying
    /// everyone `what`, as `chmod +a` does.
    #[cfg(target_os = "macos")]
    fn deny(path: &Path, what: &str, link: bool) {
        let mut chmod = Command::new("/bin/chmod");
        if link {
            chmod.arg("-h");
        }
        assert!(chmod.arg("+a").arg(format!("everyone deny {what}")).arg(path).status().unwrap().success(), "{}", path.display());
    }

    /// Whether what's at `path` has an ACL, as `ls -le` lists one.
    #[cfg(target_os = "macos")]
    fn has_acl(path: &Path) -> bool {
        let out = Command::new("/bin/ls").arg("-led").arg(path).output().unwrap();
        String::from_utf8_lossy(&out.stdout).lines().skip(1).any(|line| line.trim_start().starts_with("0:"))
    }

    /// A package of themes of the user's own called `package`, its one
    /// theme `id`, holding each of `folders` with a file in it, each
    /// denying delete as its ACL, and packed with them by the Mac's `tar`,
    /// AppleDouble entries and all (anyone can make one): its `.tar.gz`
    /// file in `dir`.
    #[cfg(target_os = "macos")]
    fn locked_themes_file(dir: &Path, package: &str, id: &str, folders: &[&str]) -> PathBuf {
        let from = tempdir();
        let themes = crate::looks::packaged::ThemeFile { format: crate::looks::packaged::FORMAT, package: package.into(), version: "1.0".into(), themes: vec![a_theme(id)] };
        std::fs::write(from.join(crate::looks::packaged::FILE), serde_json::to_string(&themes).unwrap()).unwrap();
        for folder in folders {
            std::fs::create_dir_all(from.join(folder)).unwrap();
            std::fs::write(from.join(folder).join("SKILL.md"), "---\nname: left\ndescription: Left behind\n---\nHello.\n").unwrap();
            deny(&from.join(folder).join("SKILL.md"), "delete", false);
            deny(&from.join(folder), "delete,delete_child", false);
        }
        let tarball = dir.join(format!("{package}-1.0.tar.gz"));
        assert!(Command::new("/usr/bin/tar").env_remove("COPYFILE_DISABLE").arg("--acls").arg("-czf").arg(&tarball).arg("-C").arg(&from).arg(".").status().unwrap().success());
        take_away(&from).unwrap();
        tarball
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn on_a_mac_a_refused_file_whose_folders_deny_delete_leaves_nothing_and_the_next_install_is_clean() {
        crate::i18n::set_language("en", &[]);
        let root = tempdir();
        let files = tempdir();
        // Refused for its bin/ as it's looked at, and for one of Flow's
        // machines' ids once it's unpacked where it would go: neither
        // leaves anything, nor keeps the next file of your own out.
        assert!(install_file(&root, &locked_themes_file(&files, "zzz", "zzz", &["bin"])).unwrap_err().contains("bin/"));
        assert!(!root.join(".file.unpacking").exists() && !root.join("zzz").exists());
        assert!(install_file(&root, &locked_themes_file(&files, "aaa-themes", "green", &["skills", "skills/x"])).unwrap_err().contains("“green”"));
        assert!(!root.join(".file.unpacking").exists() && !root.join("aaa-themes").exists());
        install_file(&root, &locked_themes_file(&files, "my-themes", "sepia", &[])).unwrap();
        // One that installs has its folders without the tarball's ACLs, so
        // it's removed whole.
        install_file(&root, &locked_themes_file(&files, "kept", "night", &["extra"])).unwrap();
        let extra = app_dir(&root, "kept", "1.0").join("extra");
        assert!(extra.join("SKILL.md").is_file() && !has_acl(&extra) && !has_acl(&extra.join("SKILL.md")));
        assert_eq!(remove(&root, "kept"), Ok(true));
        assert!(!root.join("kept").exists());
        // What an older flowapp's tar left with those ACLs (a folder that
        // won't even be read, and a link) is taken away first, never
        // unpacked over.
        let left = root.join(".file.unpacking/bin");
        std::fs::create_dir_all(&left).unwrap();
        std::fs::write(left.join("x"), "x").unwrap();
        std::os::unix::fs::symlink("/etc/hosts", left.join("link")).unwrap();
        deny(&left.join("x"), "read,delete", false);
        deny(&left.join("link"), "delete", true);
        deny(&left, "list,search,delete,delete_child", false);
        std::fs::set_permissions(&left, <std::fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o000)).unwrap();
        assert!(std::fs::remove_dir_all(root.join(".file.unpacking")).is_err(), "it won't go as it is");
        install_file(&root, &locked_themes_file(&files, "mine", "dusk", &[])).unwrap();
        assert!(!root.join(".file.unpacking").exists());
        assert_eq!(Installed::load(&root).apps.keys().collect::<Vec<_>>(), ["mine", "my-themes"]);
        take_away(&root).unwrap();
        take_away(&files).unwrap();
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn on_a_mac_taking_a_folder_away_takes_off_its_acls_and_never_those_a_link_leads_to() {
        let dir = tempdir();
        let outside = tempdir();
        std::fs::write(outside.join("keep.txt"), "mine").unwrap();
        deny(&outside.join("keep.txt"), "delete", false);
        let tree = dir.join("tree");
        std::fs::create_dir_all(tree.join("a/b")).unwrap();
        std::fs::write(tree.join("a/b/f"), "f").unwrap();
        std::os::unix::fs::symlink(outside.join("keep.txt"), tree.join("a/out")).unwrap();
        deny(&tree.join("a/b/f"), "delete", false);
        deny(&tree.join("a/out"), "delete", true);
        deny(&tree.join("a/b"), "list,search,delete,delete_child", false);
        deny(&tree.join("a"), "delete,delete_child", false);
        deny(&tree, "delete", false);
        take_away(&tree).unwrap();
        assert!(std::fs::symlink_metadata(&tree).is_err());
        assert!(outside.join("keep.txt").is_file() && has_acl(&outside.join("keep.txt")), "what a link led to is as it was");
        // A file, and a link, alone.
        std::fs::write(dir.join("file"), "x").unwrap();
        deny(&dir.join("file"), "delete", false);
        std::os::unix::fs::symlink(outside.join("keep.txt"), dir.join("link")).unwrap();
        deny(&dir.join("link"), "delete", true);
        take_away(&dir.join("file")).unwrap();
        take_away(&dir.join("link")).unwrap();
        assert!(std::fs::symlink_metadata(dir.join("file")).is_err() && std::fs::symlink_metadata(dir.join("link")).is_err());
        assert!(has_acl(&outside.join("keep.txt")));
        take_away(&dir).unwrap();
        take_away(&outside).unwrap();
        assert!(std::fs::symlink_metadata(&outside).is_err());
    }

    #[test]
    fn keeping_the_carried_games_says_what_it_kept_and_what_it_left_out() {
        crate::i18n::set_language("en", &[]);
        assert_eq!(CarriedKept::default().words(), None, "nothing to say");
        let kept = CarriedKept { kept: vec!["flow-mines".into(), "flow-rogue".into()], ..CarriedKept::default() };
        assert!(kept.words().unwrap().starts_with("Kept flow-mines, flow-rogue:"));
        let left = CarriedKept { passed_over: vec![("flow-rogue".into(), "Flow-Rogue".into())], ..CarriedKept::default() };
        let words = left.words().unwrap();
        assert!(words.starts_with("Left out flow-rogue") && words.contains("Flow-Rogue, installed here") && words.contains("flowapp install"), "{words}");
        crate::i18n::set_language("ja", &[]);
        assert!(left.words().unwrap().contains("Flow-Rogue") && left.words().unwrap() != words);
        crate::i18n::set_language("en", &[]);
    }

    #[test]
    #[cfg(unix)]
    fn a_version_that_differs_only_in_case_takes_its_place_and_keeps_its_folder() {
        crate::i18n::set_language("en", &[]);
        let root = tempdir();
        let files = tempdir();
        install_file(&root, &own_themes_file(&files, "mine", "1.0-RC1")).unwrap();
        // On a Mac or Windows the two are one folder, which the version
        // before mustn't take with it as it goes.
        let (_, kept) = install_file(&root, &own_themes_file(&files, "mine", "1.0-rc1")).unwrap();
        assert_eq!(kept.version, "1.0-rc1");
        assert!(app_dir(&root, "mine", "1.0-rc1").join(crate::looks::packaged::FILE).is_file());
        let _ = (std::fs::remove_dir_all(&root), std::fs::remove_dir_all(&files));
    }

    #[test]
    #[cfg(unix)]
    fn a_package_of_your_own_never_puts_a_program_in_place_however_it_came() {
        crate::i18n::set_language("en", &[]);
        let root = tempdir();
        let (bytes, _) = package(&["tree"], "0.1.0", &key().0);
        let error = install_kept(&root, "mine", "1.0", &[], &bytes, None, true).unwrap_err();
        assert!(error.contains("holds programs"), "{error}");
        assert!(!app_dir(&root, "mine", "1.0").exists() && !Installed::path(&root).exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_message_is_given_the_terminal_with_its_controls_spelled_out() {
        let said = plain("mine: “\u{1b}]7732;x\u{7}” isn't C:\\Users\nテーマ\tok\r");
        assert_eq!(said, "mine: “\\u{1b}]7732;x\\u{7}” isn't C:\\Users\nテーマ\tok\\r");
        assert!(!said.chars().any(|c| c.is_control() && !matches!(c, '\n' | '\t')));
    }

    #[test]
    fn a_downloads_date_is_kept_by_the_catalog_and_one_without_reads_as_before() {
        let with = r#"{"format":1,"apps":{"x":{"version":"0.2.0","summary":"","programs":["x"],"downloads":{"macos-aarch64":{"version":"0.2.0","url":"u","size":1,"sha256":"s","signature":"g","signed":"2026-11-18"}}}}}"#;
        let catalog = Catalog::parse(with).unwrap();
        assert_eq!(catalog.apps["x"].downloads["macos-aarch64"].signed, "2026-11-18");
        assert_eq!(Catalog::parse(&serde_json::to_string(&catalog).unwrap()).unwrap(), catalog);
        // One from before: no date, and none written back.
        let without = with.replace(r#","signed":"2026-11-18""#, "");
        let catalog = Catalog::parse(&without).unwrap();
        assert!(catalog.apps["x"].downloads["macos-aarch64"].signed.is_empty());
        assert!(!serde_json::to_string(&catalog).unwrap().contains("signed"));
        assert_eq!(date(" 2026-11-18 ").as_deref(), Some("2026-11-18"));
        assert_eq!(date("18-11-2026"), None);
        assert!(date(&today()).is_some());
    }

    #[test]
    fn an_installed_json_from_before_its_signatures_reads_and_writes_back_as_it_was() {
        let root = tempdir();
        let before = r#"{"apps":{"flow-mines":{"version":"0.1.9","programs":["flow-mines"]},"mine":{"version":"1.0","programs":[],"kind":"themes"}}}"#;
        std::fs::write(root.join("installed.json"), before).unwrap();
        let installed = Installed::load(&root);
        let mines = &installed.apps["flow-mines"];
        assert!(mines.download.is_none() && mines.key.is_empty() && mines.checked.is_empty() && !mines.own_file);
        installed.save(&root).unwrap();
        let again: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(root.join("installed.json")).unwrap()).unwrap();
        assert_eq!(again, serde_json::from_str::<serde_json::Value>(before).unwrap());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_install_keeps_its_download_the_keys_fingerprint_and_when_it_was_checked() {
        let root = tempdir();
        let (signing, public) = key();
        let (_, download) = package(&["flow-mines"], "0.1.9", &signing);
        let download = Download { signed: "2026-11-18".into(), ..download };
        let mut app = App { version: "0.1.9".into(), programs: vec!["flow-mines".into()], ..App::default() };
        app.downloads.insert(platform(), download.clone());
        let kept = install(&root, "flow-mines", &app, &public).unwrap();
        assert_eq!(kept.download, Some(download));
        assert_eq!(kept.key, signing::fingerprint(&public).unwrap());
        assert_eq!(kept.checked, today());
        assert!(!kept.own_file);
        assert_eq!(Installed::load(&root).apps["flow-mines"], kept);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    #[cfg(unix)]
    fn a_program_put_first_follows_its_upgrades_and_goes_with_its_app() {
        let (signing, _) = key();
        let root = tempdir();
        let names = |programs: &[&str]| -> Vec<String> { programs.iter().map(|p| p.to_string()).collect() };
        let (bytes, _) = package(&["ls", "tree"], "0.1.0", &signing);
        install_bytes(&root, "flow-utils", "0.1.0", &names(&["ls", "tree"]), &bytes).unwrap();
        let installed = Installed::load(&root);
        assert_eq!(programs_of(&installed, "flow-utils"), Some(names(&["ls", "tree"])), "an app: all its programs");
        assert_eq!(programs_of(&installed, "tree"), Some(names(&["tree"])), "a program by its own name");
        assert_eq!(programs_of(&installed, "cat"), None);
        let mut record = first::Record::default();
        record.put(first::Choice::new("ls"));
        record.put(first::Choice { app: Some("flow-utils".into()), ..first::Choice::new("tree") });
        record.save(&root).unwrap();
        let pane = first::open_pane_in(&root, std::process::id(), 900, "zsh").unwrap();
        assert_eq!(version_of(&pane.join("ls")).as_deref(), Some("0.1.0"));
        assert_eq!(version_of(&pane.join("tree")).as_deref(), Some("0.1.0"));
        // Upgraded, the one put first is the new one, from bin/ or from
        // its app's version.
        let (bytes, _) = package(&["ls", "tree"], "0.1.1", &signing);
        install_bytes(&root, "flow-utils", "0.1.1", &names(&["ls", "tree"]), &bytes).unwrap();
        assert_eq!(version_of(&pane.join("ls")).as_deref(), Some("0.1.1"));
        assert_eq!(version_of(&pane.join("tree")).as_deref(), Some("0.1.1"));
        // A version without one of its programs: that one leaves the front.
        let (bytes, _) = package(&["ls"], "0.1.2", &signing);
        install_bytes(&root, "flow-utils", "0.1.2", &names(&["ls"]), &bytes).unwrap();
        assert_eq!(first::Record::load(&root).names(), ["ls"]);
        assert!(std::fs::symlink_metadata(pane.join("tree")).is_err());
        // Another app's ls takes bin/'s place, and gives it back when it goes.
        let (bytes, _) = package(&["ls"], "9.0.0", &signing);
        install_bytes(&root, "other", "9.0.0", &names(&["ls"]), &bytes).unwrap();
        assert_eq!(version_of(&pane.join("ls")).as_deref(), Some("9.0.0"));
        remove(&root, "other").unwrap();
        assert_eq!(version_of(&root.join("bin/ls")).as_deref(), Some("0.1.2"), "flow-utils' again");
        assert_eq!(first::Record::load(&root).names(), ["ls"], "flow-utils still has one");
        // Removed, an app's programs leave the front too.
        remove(&root, "flow-utils").unwrap();
        assert!(first::Record::load(&root).firsts.is_empty() && std::fs::symlink_metadata(pane.join("ls")).is_err());
        first::close_pane(&pane);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    #[cfg(unix)]
    fn the_system_command_a_program_goes_ahead_of_is_the_first_not_flows() {
        let root = tempdir();
        let (ours, system) = (root.join("apps/first"), root.join("usr/bin"));
        for dir in [&ours, &system] {
            std::fs::create_dir_all(dir).unwrap();
            std::fs::write(dir.join("ls"), "").unwrap();
        }
        let path = std::env::join_paths([&ours, &root.join("empty"), &system]).unwrap();
        assert_eq!(system_command("ls", Some(&path), &[root.join("apps")]), Some(system.join("ls")));
        assert_eq!(system_command("tree", Some(&path), &[root.join("apps")]), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_download_must_be_its_size_its_sum_and_signed_with_the_key() {
        let (signing, public) = key();
        let (bytes, download) = package(&["flow-mines"], "0.1.6", &signing);
        assert_eq!(check(&bytes, &download, &public), Ok(()));
        let mut other = bytes.clone();
        other[10] ^= 1;
        assert!(check(&other, &download, &public).is_err(), "a byte changed");
        assert!(check(&bytes[1..], &download, &public).is_err(), "a byte short");
        let wrong = STANDARD.encode(SigningKey::from_bytes(&[9; 32]).verifying_key().to_bytes());
        assert!(check(&bytes, &download, &wrong).is_err(), "another key's");
        let unsigned = Download { signature: STANDARD.encode([0u8; 64]), ..download.clone() };
        assert!(check(&bytes, &unsigned, &public).is_err());
    }

    #[test]
    #[cfg(unix)]
    fn an_app_installs_upgrades_and_goes_its_programs_in_bin() {
        let (signing, _) = key();
        let root = tempdir();
        let names = |programs: &[&str]| -> Vec<String> { programs.iter().map(|p| p.to_string()).collect() };
        let (bytes, _) = package(&["flow-sdk-demo", "flow-sdk-tool"], "0.1.0", &signing);
        let kept = install_bytes(&root, "flow-sdk", "0.1.0", &names(&["flow-sdk-demo", "flow-sdk-tool"]), &bytes).unwrap();
        assert_eq!(kept.version, "0.1.0");
        assert_eq!(version_of(&root.join("bin/flow-sdk-demo")).as_deref(), Some("0.1.0"));
        assert!(root.join("flow-sdk/0.1.0/bin/flow-sdk-tool").is_file());
        // Upgraded: the new one runs, the old version and the program it
        // no longer has go.
        let (bytes, _) = package(&["flow-sdk-demo"], "0.1.1", &signing);
        install_bytes(&root, "flow-sdk", "0.1.1", &names(&["flow-sdk-demo"]), &bytes).unwrap();
        assert_eq!(version_of(&root.join("bin/flow-sdk-demo")).as_deref(), Some("0.1.1"));
        assert!(!root.join("bin/flow-sdk-tool").exists() && !root.join("flow-sdk/0.1.0").exists());
        assert_eq!(Installed::load(&root).apps["flow-sdk"].version, "0.1.1");
        assert_eq!(standing(&Installed::load(&root), "flow-sdk", None, None), Standing::Installed("0.1.1".into()));
        // A download without a program the catalog names installs nothing.
        assert!(install_bytes(&root, "flow-sdk", "0.1.2", &names(&["missing"]), &bytes).is_err());
        assert_eq!(Installed::load(&root).apps["flow-sdk"].version, "0.1.1");
        // Removed: its program and its folder go.
        assert_eq!(remove(&root, "flow-sdk"), Ok(true));
        assert!(!root.join("bin/flow-sdk-demo").exists() && !root.join("flow-sdk").exists());
        assert_eq!(remove(&root, "flow-sdk"), Ok(false));
        assert!(install_bytes(&root, "../evil", "0.1.0", &[], &bytes).is_err());
        assert!(install_bytes(&root, "bin", "0.1.0", &[], &bytes).is_err() && install_bytes(&root, "first", "0.1.0", &[], &bytes).is_err(), "flowapp's own folders");
        assert!(install_bytes(&root, "flow-sdk", "../0.1.0", &[], &bytes).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    #[cfg(unix)]
    fn an_install_says_how_its_download_came_and_that_it_was_checked() {
        let (signing, public) = key();
        let root = tempdir();
        let (bytes, download) = package(&["flow-mines"], "0.1.7", &signing);
        let mut mines = App { version: "0.1.7".into(), programs: vec!["flow-mines".into()], ..App::default() };
        mines.downloads.insert(platform(), download.clone());
        let mut steps = Vec::new();
        install_with(&root, "flow-mines", &mines, &public, &mut |s| steps.push(s)).unwrap();
        assert_eq!(steps, [Step::Fetched(bytes.len() as u64), Step::Checked]);
        // One that isn't signed with the key is never said to be checked.
        let wrong = STANDARD.encode(SigningKey::from_bytes(&[9; 32]).verifying_key().to_bytes());
        steps.clear();
        assert!(install_with(&root, "flow-mines", &mines, &wrong, &mut |s| steps.push(s)).is_err());
        assert!(!steps.contains(&Step::Checked));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_release_page_is_the_github_release_its_download_is_on() {
        let at = |url: &str| Download { url: url.into(), ..Download::default() }.release_page();
        let url = "https://github.com/arcanii/Flow-releases/releases/download/flow-rogue-v0.1.45/flow-rogue-0.1.45-macos-aarch64.tar.gz";
        assert_eq!(at(url).as_deref(), Some("https://github.com/arcanii/Flow-releases/releases/tag/flow-rogue-v0.1.45"));
        assert_eq!(at("/tmp/flow-rogue.tar.gz"), None);
        assert_eq!(at("https://example.com/releases/download/v1/a.tar.gz"), None);
        assert_eq!(at("https://github.com/a/b/releases/download/v1/"), None);
    }

    #[test]
    #[cfg(unix)]
    fn a_game_flows_release_carries_counts_as_installed_at_its_version() {
        let root = tempdir();
        let bundled = root.join("Helpers");
        std::fs::create_dir_all(&bundled).unwrap();
        let program = bundled.join("flow-mines");
        std::fs::write(&program, "#!/bin/sh\necho flow-mines 0.1.6\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mines = App { version: "0.1.7".into(), programs: vec!["flow-mines".into()], bundled: Some("flow-mines".into()), ..App::default() };
        let none = Installed::default();
        assert_eq!(standing(&none, "flow-mines", Some(&mines), Some(&bundled)), Standing::Bundled("0.1.6".into()));
        assert_eq!(standing(&none, "flow-mines", Some(&mines), None), Standing::Absent);
        let invaders = App { bundled: None, ..mines.clone() };
        assert_eq!(standing(&none, "flow-invaders", Some(&invaders), Some(&bundled)), Standing::Absent);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_programs_version_is_its_first_word_of_numbers_not_its_build() {
        assert_eq!(version_in("flow-rogue 0.1.45\n").as_deref(), Some("0.1.45"));
        assert_eq!(version_in("flowapp 0.2.0 (build 750)").as_deref(), Some("0.2.0"));
        assert_eq!(version_in("Flow 0.1.45 (build 737)").as_deref(), Some("0.1.45"));
        // What the flowapps of 0.1.45 and before say, read as the version,
        // not the `737)` they read it as themselves.
        assert_eq!(version_in("flowapp 0.1.45 (build 737)").as_deref(), Some("0.1.45"));
        assert_eq!(version_in("flowapp 0.1.44.2, the last\nsecond line 9"), Some("0.1.44.2".into()));
        assert_eq!(version_in("Flow's settings, seen and changed from the shell."), None);
        assert_eq!(version_in(""), None);
        // And what they read it as is older than any flowapp of its own.
        assert!(newer(crate::version::FLOWAPP, "737)"));
    }

    #[test]
    #[cfg(unix)]
    fn flowapp_as_flow_carries_it_stands_at_the_version_it_says_and_the_catalogs_is_newer() {
        let root = tempdir();
        let helpers = root.join("Helpers");
        std::fs::create_dir_all(&helpers).unwrap();
        let program = helpers.join("flowapp");
        std::fs::write(&program, "#!/bin/sh\necho 'flowapp 0.1.45 (build 737)'\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        let flowapp = App { version: "0.2.0".into(), programs: vec!["flowapp".into()], bundled: Some("flowapp".into()), ..App::default() };
        let standing = standing(&Installed::default(), "flowapp", Some(&flowapp), Some(&helpers));
        assert_eq!(standing, Standing::Bundled("0.1.45".into()));
        assert!(newer(&flowapp.version, standing.version().unwrap()), "upgrade offers it");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A catalog of flow-mines at `version`, its download for this system
    /// `download`.
    fn catalog_of(version: &str, download: Download) -> Catalog {
        let mut mines = App { version: version.into(), programs: vec!["flow-mines".into()], ..App::default() };
        mines.downloads.insert(platform(), download);
        let mut catalog = Catalog { format: FORMAT, ..Catalog::default() };
        catalog.apps.insert("flow-mines".into(), mines);
        catalog
    }

    /// Saves `catalog` under `root` as a read `ago` would have.
    fn saved_ago(root: &Path, catalog: &Catalog, ago: Duration) {
        let file = root.join("catalog.json");
        std::fs::write(&file, serde_json::to_string(catalog).unwrap()).unwrap();
        std::fs::File::options().write(true).open(&file).unwrap().set_modified(SystemTime::now() - ago).unwrap();
    }

    #[test]
    #[cfg(unix)]
    fn a_catalog_saved_before_a_release_is_read_again_before_an_install() {
        let (signing, public) = key();
        let root = tempdir();
        // Saved hours ago, naming a release that's gone since.
        let hours = Duration::from_secs(9 * 3600);
        let gone = Download { version: "0.1.5".into(), url: root.join("flow-mines-0.1.5.tar.gz").to_string_lossy().into(), ..Download::default() };
        saved_ago(&root, &catalog_of("0.1.5", gone), hours);
        let (_, download) = package(&["flow-mines"], "0.1.6", &signing);
        let place = root.join("apps.json");
        std::fs::write(&place, serde_json::to_string(&catalog_of("0.1.6", download)).unwrap()).unwrap();
        let place = place.to_string_lossy();
        // What list and info go by is the saved one, as old as it is.
        let saved = catalog_as_saved(&root, &place).unwrap();
        assert_eq!(saved.catalog.apps["flow-mines"].version, "0.1.5");
        assert!(saved.age >= hours && saved.age < hours + Duration::from_secs(60), "{:?}", saved.age);
        assert!(matches!(install(&root, "flow-mines", &saved.catalog.apps["flow-mines"], &public), Err(Failed::Missing(_))));
        // Before an install it's read again, and saved for the commands after.
        let read = catalog_afresh(&root, &place).unwrap();
        assert_eq!((read.catalog.apps["flow-mines"].version.as_str(), read.age, read.failed.as_deref()), ("0.1.6", Duration::ZERO, None));
        assert!(saved_catalog(&root).is_some_and(|saved| saved.catalog == read.catalog && saved.age < Duration::from_secs(60)));
        assert_eq!(install(&root, "flow-mines", &read.catalog.apps["flow-mines"], &public).map(|kept| kept.version), Ok("0.1.6".to_string()));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn offline_an_install_goes_by_the_catalog_read_last_and_says_why() {
        let root = tempdir();
        let nowhere = root.join("nowhere.json").to_string_lossy().to_string();
        // Never read, and it won't read now: there's nothing to go by.
        assert!(catalog_afresh(&root, &nowhere).is_err());
        let days = Duration::from_secs(3 * 86400);
        let saved = catalog_of("0.1.6", Download::default());
        saved_ago(&root, &saved, days);
        let read = catalog_afresh(&root, &nowhere).unwrap();
        assert!(read.catalog == saved && read.age >= days, "{read:?}");
        assert!(read.failed.as_deref().is_some_and(|why| why.contains("nowhere.json")), "{read:?}");
        // One that reads but isn't a catalog this flowapp reads is the same.
        let other = root.join("apps.json");
        std::fs::write(&other, r#"{"format": 2, "apps": {}}"#).unwrap();
        let read = catalog_afresh(&root, &other.to_string_lossy()).unwrap();
        assert!(read.catalog == saved && read.failed.is_some(), "{read:?}");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A server on this computer that answers each request it gets with the
    /// next of `statuses`, and nothing more.
    fn answering(statuses: &[u16]) -> String {
        use std::io::{BufRead, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let at = listener.local_addr().unwrap();
        let statuses = statuses.to_vec();
        std::thread::spawn(move || {
            for status in statuses {
                let (mut stream, _) = listener.accept().unwrap();
                let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                while reader.read_line(&mut line).unwrap() > 0 && line != "\r\n" {
                    line.clear();
                }
                write!(stream, "HTTP/1.1 {status} Status\r\ncontent-length: 0\r\nconnection: close\r\n\r\n").unwrap();
            }
        });
        format!("http://{at}")
    }

    #[test]
    fn a_download_thats_gone_is_told_from_one_that_failed() {
        let server = answering(&[404, 410, 500]);
        let url = format!("{server}/flow-invaders-0.1.0-macos-aarch64.tar.gz");
        assert!(matches!(fetch(&url, 1024), Err(Failed::Missing(words)) if words.starts_with(&url) && words.contains("404")));
        assert!(matches!(fetch(&url, 1024), Err(Failed::Missing(_))));
        assert!(matches!(fetch(&url, 1024), Err(Failed::Other(words)) if words.contains("500")));
        let root = tempdir();
        assert!(matches!(fetch(&root.join("gone.tar.gz").to_string_lossy(), 1024), Err(Failed::Missing(_))));
        assert!(matches!(fetch(&root.to_string_lossy(), 1024), Err(Failed::Other(_))), "a folder is there, and isn't a file");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_gateways_trouble_is_tried_again_twice() {
        let server = answering(&[504, 503, 200]);
        assert_eq!(fetch(&format!("{server}/apps.json"), 1024).ok(), Some(vec![]), "the third try comes");
        let server = answering(&[502, 503, 504]);
        assert!(matches!(fetch(&format!("{server}/apps.json"), 1024), Err(Failed::Other(words)) if words.contains("504")), "three tries, then the last one's words");
    }

    /// A server on this computer that answers every request with what
    /// `answer` makes of where it is and the request's head, lowercased.
    fn routing(answer: impl Fn(&str, &str) -> String + Send + 'static) -> String {
        use std::io::{BufRead, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let at = listener.local_addr().unwrap().to_string();
        let here = at.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
                let (mut head, mut line) = (String::new(), String::new());
                while reader.read_line(&mut line).unwrap_or(0) > 0 && line != "\r\n" {
                    head.push_str(&line.to_ascii_lowercase());
                    line.clear();
                }
                let _ = stream.write_all(answer(&here, &head).as_bytes());
            }
        });
        format!("http://{at}")
    }

    #[test]
    fn a_release_file_githubs_front_wont_hand_over_comes_through_its_api() {
        let server = routing(|at, head| {
            let answer = |status: &str, more: &str, body: &str| format!("HTTP/1.1 {status}\r\n{more}content-length: {}\r\nconnection: close\r\n\r\n{body}", body.len());
            let first = head.lines().next().unwrap_or_default();
            if first.starts_with("get /web/") {
                answer("504 Gateway Time-out", "", "")
            } else if first.starts_with("get /api/o/r/releases/tags/t ") && head.contains("accept: application/vnd.github+json") {
                let listed = format!(r#"{{"assets":[{{"name":"other.tar.gz","url":"http://{at}/api/o/r/releases/assets/1"}},{{"name":"app.tar.gz","url":"http://{at}/api/o/r/releases/assets/2"}}]}}"#);
                answer("200 OK", "content-type: application/json\r\n", &listed)
            } else if first.starts_with("get /api/o/r/releases/assets/2 ") && head.contains("accept: application/octet-stream") {
                answer("302 Found", &format!("location: http://{at}/stored/app.tar.gz\r\n"), "")
            } else if first.starts_with("get /stored/app.tar.gz ") {
                answer("200 OK", "", "the bytes")
            } else {
                answer("404 Not Found", "", "")
            }
        });
        let (web, api) = (format!("{server}/web/"), format!("{server}/api/"));
        let mut seen = 0;
        let bytes = fetch_routed(&format!("{web}o/r/releases/download/t/app.tar.gz"), 1024, &mut |n| seen = n, &web, &api).unwrap();
        assert_eq!((bytes.as_slice(), seen), (&b"the bytes"[..], 9));
        // A file the release doesn't list is still the front's failure.
        let gone = fetch_routed(&format!("{web}o/r/releases/download/t/gone.tar.gz"), 1024, &mut |_| {}, &web, &api);
        assert!(matches!(gone, Err(Failed::Other(words)) if words.contains("504")));
        // Only a release's file goes round by the API.
        assert_eq!(api_route(&format!("{web}o/r/archive/main.zip"), &web, &api), None);
        assert_eq!(api_route("https://raw.githubusercontent.com/arcanii/Flow-releases/main/apps.json", GITHUB, GITHUB_API), None);
        let invaders = "https://github.com/arcanii/Flow-releases/releases/download/flow-invaders-v0.1.21/flow-invaders-0.1.21-windows-x86_64.tar.gz";
        assert_eq!(api_route(invaders, GITHUB, GITHUB_API), Some(("https://api.github.com/repos/arcanii/Flow-releases/releases/tags/flow-invaders-v0.1.21".into(), "flow-invaders-0.1.21-windows-x86_64.tar.gz".into())));
    }

    #[test]
    #[ignore = "fetches a release's file from GitHub's API"]
    fn githubs_api_hands_over_a_release_file() {
        let place = "https://github.com/arcanii/Flow-releases/releases/download/flow-mines-v0.1.6/flow-mines-0.1.6-macos-aarch64.tar.gz";
        let (release, name) = api_route(place, GITHUB, GITHUB_API).unwrap();
        let mut seen = 0;
        let Ok(bytes) = from_api(&release, &name, 16 << 20, &mut |n| seen = n) else { panic!("{release} handed over no {name}") };
        assert_eq!((&bytes[..2], seen), (&[0x1f, 0x8b][..], bytes.len() as u64), "a gzipped tarball, all of it told");
    }

    #[test]
    fn this_system_is_named_as_the_catalog_names_it() {
        let p = platform();
        assert!(["macos-", "windows-", "linux-"].iter().any(|os| p.starts_with(os)), "{p}");
        assert!(p.ends_with("aarch64") || p.ends_with("x86_64"), "{p}");
    }

    /// A catalog of the two games an older Flow carried and one it never
    /// did, each with a download for this system, written under `root` as
    /// `apps.json`: where it is.
    #[cfg(unix)]
    fn carried_catalog(root: &Path, signing: &SigningKey) -> String {
        let mut catalog = Catalog { format: FORMAT, ..Catalog::default() };
        for (name, bundled) in [("flow-rogue", true), ("flow-mines", true), ("flow-invaders", false)] {
            let (_, download) = package(&[name], "0.2.0", signing);
            let mut app = App { version: "0.2.0".into(), programs: vec![name.into()], bundled: bundled.then(|| name.to_string()), ..App::default() };
            app.downloads.insert(platform(), download);
            catalog.apps.insert(name.into(), app);
        }
        let place = root.join("apps.json");
        std::fs::write(&place, serde_json::to_string(&catalog).unwrap()).unwrap();
        place.to_string_lossy().into()
    }

    #[test]
    #[cfg(unix)]
    fn an_update_keeps_the_games_an_older_flow_carried_once_and_only_those() {
        let (signing, public) = key();
        let root = tempdir();
        let place = carried_catalog(&root, &signing);
        assert!(!carried_settled(&root));
        assert_eq!(keep_carried(&root, &place, &public).map(|done| done.kept), Ok(vec!["flow-mines".to_string(), "flow-rogue".to_string()]));
        let installed = Installed::load(&root);
        assert_eq!(installed.apps.keys().collect::<Vec<_>>(), ["flow-mines", "flow-rogue"], "not Flow Invaders, which no Flow carried");
        assert_eq!(version_of(&root.join("bin/flow-rogue")).as_deref(), Some("0.2.0"));
        assert!(carried_settled(&root));
        // Kept once: removed afterwards, it stays removed.
        assert_eq!(remove(&root, "flow-mines"), Ok(true));
        assert_eq!(keep_carried(&root, &place, &public).map(|done| done.kept), Ok(Vec::new()));
        assert!(!Installed::load(&root).apps.contains_key("flow-mines"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    #[cfg(unix)]
    fn offline_the_carried_games_wait_for_the_next_start_and_one_installed_already_stays() {
        let (signing, public) = key();
        let root = tempdir();
        let nowhere = root.join("nowhere.json").to_string_lossy().to_string();
        assert!(keep_carried(&root, &nowhere, &public).is_err());
        assert!(!carried_settled(&root), "tried again next time");
        // Flow Rogue installed by hand meanwhile, at a version of its own.
        let (bytes, _) = package(&["flow-rogue"], "0.1.9", &signing);
        install_bytes(&root, "flow-rogue", "0.1.9", &["flow-rogue".to_string()], &bytes).unwrap();
        let place = carried_catalog(&root, &signing);
        assert_eq!(keep_carried(&root, &place, &public).map(|done| done.kept), Ok(vec!["flow-mines".to_string()]));
        assert_eq!(Installed::load(&root).apps["flow-rogue"].version, "0.1.9");
        // A download that fails its checks keeps nothing settled.
        let other = tempdir();
        let wrong = STANDARD.encode(SigningKey::from_bytes(&[9; 32]).verifying_key().to_bytes());
        let place = carried_catalog(&other, &signing);
        assert!(keep_carried(&other, &place, &wrong).is_err());
        assert!(!carried_settled(&other) && Installed::load(&other).apps.is_empty());
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&other);
    }

    #[test]
    fn a_download_that_needs_a_newer_flow_waits_for_one() {
        let needing = |flow: &str| Download { flow: flow.into(), ..Download::default() };
        assert_eq!(needing("").needs_newer_flow("0.1.34"), None, "any Flow will do");
        assert_eq!(needing("0.1.33").needs_newer_flow("0.1.34"), None);
        assert_eq!(needing("0.1.34").needs_newer_flow("0.1.34"), None, "this one is the oldest it works in");
        assert_eq!(needing(" 0.1.35 ").needs_newer_flow("0.1.34"), Some("0.1.35"));
        assert_eq!(needing("0.1.34.1").needs_newer_flow("0.1.34"), Some("0.1.34.1"), "a patch release's number counts");
        // A catalog from before says nothing of it, and one written without
        // it says nothing either.
        let old: Download = serde_json::from_str(r#"{"version":"0.1.0","url":"u","size":1,"sha256":"","signature":""}"#).unwrap();
        assert_eq!(old.flow, "");
        assert!(!serde_json::to_string(&old).unwrap().contains("flow"));
        assert!(serde_json::to_string(&needing("0.1.35")).unwrap().contains(r#""flow":"0.1.35""#));
    }

    #[test]
    fn an_app_needing_a_newer_flow_is_refused_before_its_download_comes() {
        crate::i18n::set_language("en", &[]);
        let root = tempdir();
        let nowhere = root.join("nowhere.tar.gz").to_string_lossy().to_string();
        let mut app = App { version: "0.2.0".into(), programs: vec!["flow-utils".into()], ..App::default() };
        app.downloads.insert(platform(), Download { version: "0.2.0".into(), url: nowhere, flow: "999.0".into(), ..Download::default() });
        let mut steps = 0;
        let refused = install_with(&root, "flow-utils", &app, "", &mut |_| steps += 1).unwrap_err().to_string();
        assert!(refused.contains("Flow 999.0") && refused.contains(crate::version::VERSION), "{refused}");
        assert_eq!(steps, 0, "nothing fetched");
        assert!(Installed::load(&root).apps.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    #[cfg(unix)]
    fn a_carried_game_needing_a_newer_flow_waits_for_a_start_after_flows_update() {
        let (signing, public) = key();
        let root = tempdir();
        let place = carried_catalog(&root, &signing);
        let mut catalog = Catalog::parse(&std::fs::read_to_string(&place).unwrap()).unwrap();
        let mines = catalog.apps.get_mut("flow-mines").unwrap().downloads.get_mut(&platform()).unwrap();
        mines.flow = "999.0".into();
        std::fs::write(&place, serde_json::to_string(&catalog).unwrap()).unwrap();
        assert_eq!(keep_carried(&root, &place, &public).map(|done| done.kept), Ok(vec!["flow-rogue".to_string()]));
        assert!(!carried_settled(&root), "Flow Mines is still to keep");
        // A Flow it works in (here, the catalog no longer asking more).
        catalog.apps.get_mut("flow-mines").unwrap().downloads.get_mut(&platform()).unwrap().flow.clear();
        std::fs::write(&place, serde_json::to_string(&catalog).unwrap()).unwrap();
        assert_eq!(keep_carried(&root, &place, &public).map(|done| done.kept), Ok(vec!["flow-mines".to_string()]));
        assert!(carried_settled(&root));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    #[cfg(unix)]
    fn a_carried_game_an_older_package_is_in_the_way_of_is_passed_over_saying_so_and_the_rest_are_kept() {
        crate::i18n::set_language("en", &[]);
        let (signing, public) = key();
        let root = tempdir();
        // A package of one's own called Flow-Rogue, from before names were
        // checked, which on a Mac or Windows has Flow Rogue's folder.
        let mut installed = Installed::default();
        installed.apps.insert("Flow-Rogue".into(), Kept { version: "1.0".into(), kind: THEMES.into(), own_file: true, ..Kept::default() });
        installed.save(&root).unwrap();
        let place = carried_catalog(&root, &signing);
        let catalog = Catalog::parse(&std::fs::read_to_string(&place).unwrap()).unwrap();
        assert_eq!(carried_to_keep(&catalog, &Installed::load(&root)), ["flow-mines"]);
        assert_eq!(carried_in_the_way(&catalog, &Installed::load(&root)), [("flow-rogue".to_string(), "Flow-Rogue".to_string())]);
        // Flow Mines is kept and that's noted, rather than Flow Rogue
        // refused at every start and Flow Mines never settled; and what was
        // left out, and why, is said, that once.
        let done = keep_carried(&root, &place, &public).unwrap();
        assert_eq!(done, CarriedKept { kept: vec!["flow-mines".into()], passed_over: vec![("flow-rogue".into(), "Flow-Rogue".into())] });
        let words = done.words().unwrap();
        assert!(words.starts_with("Kept flow-mines") && words.contains("left out flow-rogue: Flow-Rogue, installed here") && words.contains("install flow-rogue"), "{words}");
        assert!(carried_settled(&root));
        assert_eq!(keep_carried(&root, &place, &public), Ok(CarriedKept::default()));
        assert_eq!(Installed::load(&root).apps.keys().collect::<Vec<_>>(), ["Flow-Rogue", "flow-mines"]);
        assert_eq!(version_of(&root.join("bin/flow-mines")).as_deref(), Some("0.2.0"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_carried_game_without_a_download_here_isnt_kept() {
        let mut catalog = Catalog { format: FORMAT, ..Catalog::default() };
        catalog.apps.insert("flow-rogue".into(), App { bundled: Some("flow-rogue".into()), ..App::default() });
        assert!(carried_to_keep(&catalog, &Installed::default()).is_empty());
        let root = tempdir();
        settle_carried(&root, &[]).unwrap();
        assert!(carried_settled(&root));
        let _ = std::fs::remove_dir_all(&root);
    }
}
