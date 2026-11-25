//! Themes installed as packages: `flowapp install flow-core-themes`, Flow's
//! machines as one package, or one of the user's own. A package is a folder
//! (its download unpacked under `apps/`) holding `themes.json`, a theme for
//! each look it brings, with any font files and logo pictures beside it.
//!
//! Each theme is read into a [`Look`], as the built-in ones are written, so
//! it's worn as they are: Theme details' switches, its sounds, its CRT
//! preset and its screen for Fit window to all work alike. One whose id is a
//! built-in machine's takes that one's place (a newer green screen from
//! `flow-core-themes`), and the rest come after the built-in ones in
//! Settings' Theme row and the new-tab picker. A theme only ever types text
//! (its start screen and prompt with their `{…}` fields): nothing in a
//! package runs.
//!
//! The looks read are kept for the rest of Flow's run (leaked, so they're
//! `'static` as the built-in ones are): they're read again only when what's
//! installed changes, which is rare, and each is a few kilobytes.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use super::{Look, Sounds, LOOKS};
use crate::config::{Config, CrtConfig, FontConfig, SavedTheme};
use crate::render::palette;

/// The file a theme package holds its themes in.
pub const FILE: &str = "themes.json";

/// The package of Flow's own machines in `flowapp`'s catalog.
pub const CORE: &str = "flow-core-themes";

/// The form of `themes.json` this Flow reads.
pub const FORMAT: u32 = 1;

/// The most `themes.json` may be.
const MOST: u64 = 1024 * 1024;

/// A package's `themes.json`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeFile {
    pub format: u32,
    /// Its name in `flowapp`'s catalog, and its version.
    pub package: String,
    pub version: String,
    pub themes: Vec<ThemeEntry>,
}

/// One theme, as a [`Look`] holds it, with the names it's shown by, the
/// font files and logo it brings, and any more of the look's settings
/// (`THEME_KEYS`, as a saved theme holds them: the liquid, say) put on
/// after the rest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeEntry {
    /// Lowercase letters, digits and dashes; a built-in machine's to take
    /// its place, never Flow's own (`flow`) or your own (`own`).
    pub id: String,
    /// Its name in English, and in other languages by their code ("ja").
    pub name: String,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub names: BTreeMap<String, String>,
    /// Font families to try first, by name.
    pub fonts: Vec<String>,
    /// Font files in the package (`fonts/mine.ttf`), loaded with Flow's
    /// own, which `fonts` can then name.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub font_files: Vec<String>,
    /// `#RRGGBB`.
    pub text: String,
    pub cursor: String,
    pub background: String,
    /// Empty for none.
    pub border: String,
    /// Programs' sixteen colors; empty for Flow's own.
    pub ansi: Vec<String>,
    /// Empty for a screen in color.
    pub phosphor: String,
    /// "block", "underline" or "bar".
    pub cursor_shape: String,
    pub blink: bool,
    pub cursor_speed: f32,
    /// A built-in CRT preset's name; empty for none.
    pub crt: String,
    pub start: Vec<String>,
    pub baud: u32,
    pub prompt: Option<String>,
    /// As `[theme]`'s `sounds` names them: "tube", "teletype" and the like.
    pub sounds: String,
    /// Columns and rows, which Fit window to offers first.
    pub screen: Option<(usize, usize)>,
    /// A shell window's frame round each pane ([`Look::frame`]).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub frame: bool,
    /// A picture in the package (`logos/mine.png`) shown as the startup
    /// logo; empty for the one Settings picks.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub logo: String,
    #[serde(skip_serializing_if = "serde_json::Map::is_empty")]
    pub keys: serde_json::Map<String, serde_json::Value>,
}

impl Default for ThemeEntry {
    fn default() -> Self {
        ThemeEntry {
            id: String::new(),
            name: String::new(),
            names: BTreeMap::new(),
            fonts: Vec::new(),
            font_files: Vec::new(),
            text: palette::hex(palette::FOREGROUND),
            cursor: palette::hex(palette::CURSOR),
            background: palette::hex(palette::BACKGROUND),
            border: String::new(),
            ansi: Vec::new(),
            phosphor: String::new(),
            cursor_shape: "block".into(),
            blink: false,
            cursor_speed: 1.0,
            crt: String::new(),
            start: Vec::new(),
            baud: 0,
            prompt: None,
            sounds: "quiet".into(),
            screen: None,
            frame: false,
            logo: String::new(),
            keys: serde_json::Map::new(),
        }
    }
}

impl ThemeEntry {
    /// `look` as a package writes it, with `name` and `names` its names.
    pub fn of(look: &Look, name: &str, names: BTreeMap<String, String>) -> Self {
        ThemeEntry {
            id: look.id.into(),
            name: name.into(),
            names,
            fonts: look.fonts.iter().map(|f| f.to_string()).collect(),
            font_files: Vec::new(),
            text: palette::hex(look.text),
            cursor: palette::hex(look.cursor),
            background: palette::hex(look.background),
            border: look.border.map(palette::hex).unwrap_or_default(),
            ansi: look.ansi.map(|a| a.iter().map(|c| palette::hex(*c)).collect()).unwrap_or_default(),
            phosphor: look.phosphor.map(palette::hex).unwrap_or_default(),
            cursor_shape: look.cursor_shape.into(),
            blink: look.blink,
            cursor_speed: look.cursor_speed,
            crt: look.crt.unwrap_or_default().into(),
            start: look.start.iter().map(|l| l.to_string()).collect(),
            baud: look.baud,
            prompt: look.prompt.map(String::from),
            sounds: look.sounds.name().into(),
            screen: look.screen,
            frame: look.frame,
            logo: String::new(),
            keys: look.keys.cloned().unwrap_or_default(),
        }
    }

    /// The [`Look`] it describes, its strings kept for good, and the font
    /// files and logo it names, found in `dir`; what's wrong, if anything.
    fn look(&self, dir: &Path) -> Result<(Look, Vec<PathBuf>, Option<PathBuf>), String> {
        let id = self.id.as_str();
        let fine = !id.is_empty() && id.len() <= 40 && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        if !fine || matches!(id, "flow" | super::OWN) {
            return Err(format!("“{id}” isn't an id a theme may have"));
        }
        let color = |what: &str, hex: &str| palette::parse_hex(hex).ok_or_else(|| format!("{id}: its {what} “{hex}” isn't #RRGGBB"));
        let maybe = |what: &str, hex: &str| if hex.is_empty() { Ok(None) } else { color(what, hex).map(Some) };
        let ansi = match self.ansi.len() {
            0 => None,
            16 => Some(self.ansi.iter().map(|c| color("color", c)).collect::<Result<Vec<_>, _>>()?.try_into().expect("sixteen")),
            n => return Err(format!("{id}: {n} colors for programs, not 16")),
        };
        if !matches!(self.cursor_shape.as_str(), "block" | "underline" | "bar") {
            return Err(format!("{id}: no cursor shape “{}”", self.cursor_shape));
        }
        let crt = match self.crt.as_str() {
            "" => None,
            name if crate::settings::builtin_crt_presets().iter().any(|p| p.name == name) => Some(leak(name)),
            name => return Err(format!("{id}: no CRT preset “{name}”")),
        };
        if self.start.len() > 200 || self.start.iter().any(|l| l.len() > 400) || self.baud > 1_000_000 {
            return Err(format!("{id}: its start screen is too long, or typed too fast"));
        }
        if self.screen.is_some_and(|(c, r)| !(10..=500).contains(&c) || !(4..=300).contains(&r)) {
            return Err(format!("{id}: no screen is {:?}", self.screen.unwrap_or_default()));
        }
        let font_files = self.font_files.iter().map(|f| inside(dir, f)).collect::<Result<Vec<_>, _>>()?;
        let logo = (!self.logo.is_empty()).then(|| inside(dir, &self.logo)).transpose()?;
        let keys = (!self.keys.is_empty()).then(|| &*Box::leak(Box::new(self.keys.clone())));
        let look = Look {
            id: leak(id),
            fonts: leak_all(&self.fonts),
            text: color("text", &self.text)?,
            cursor: color("cursor", &self.cursor)?,
            background: color("background", &self.background)?,
            border: maybe("border", &self.border)?,
            ansi,
            phosphor: maybe("phosphor", &self.phosphor)?,
            cursor_shape: leak(&self.cursor_shape),
            blink: self.blink,
            cursor_speed: self.cursor_speed.clamp(0.5, 4.0),
            crt,
            start: leak_all(&self.start),
            baud: self.baud,
            prompt: self.prompt.as_deref().map(leak),
            sounds: Sounds::named(&self.sounds),
            screen: self.screen,
            // One taking a built-in's place keeps its frame, from a package
            // written before themes had one.
            frame: self.frame || super::builtin_frame(&self.id),
            keys,
        };
        Ok((look, font_files, logo))
    }
}

/// Flow's machines as the [`CORE`] package holds them, at `version`: every
/// built-in look but Flow's own, named in each of Flow's languages. Built
/// in already, they're there for a newer one of each, or a new machine, to
/// come without a release of Flow.
pub fn core_themes(version: &str) -> ThemeFile {
    let themes = LOOKS
        .iter()
        .filter(|look| look.machine())
        .map(|look| {
            let key = format!("settings.theme.{}", look.id);
            let names = crate::i18n::codes().filter(|code| *code != "en").filter_map(|code| Some((code.to_string(), crate::i18n::in_language(code, &key)?.to_string()))).collect();
            ThemeEntry::of(look, crate::i18n::english(&key), names)
        })
        .collect();
    ThemeFile { format: FORMAT, package: CORE.into(), version: version.into(), themes }
}

/// The version of a package exported from a saved theme: exported again,
/// it installs over the one before.
pub const EXPORTED: &str = "1.0";

/// `saved`, a theme the user saved under a name (Settings' Export…), as a
/// package of its own for `flowapp install` on another computer, worn
/// there as it's worn here. Its colors, cursor, fonts and tube (where it's
/// a built-in preset's) are the theme's own, so Theme details' switches
/// there turn them off as they do a built-in machine's; the rest of the
/// look (the liquid, the glass, a tube of no preset's) goes in `keys`; and
/// it brings its start screen, prompt and sounds. Its fonts are named
/// only, since their files are this computer's, not Flow's to give away.
pub fn export(saved: &SavedTheme) -> Result<ThemeFile, String> {
    let name = &saved.name;
    let mut worn = Config::default();
    super::put_keys(&saved.look, &mut worn).map_err(|e| format!("“{name}”: {e}"))?;
    let color = |what: &str, hex: &str| palette::parse_hex(hex).map(palette::hex).ok_or_else(|| format!("“{name}”: its {what} “{hex}” isn't #RRGGBB"));
    let maybe = |what: &str, hex: &str| if hex.is_empty() { Ok(String::new()) } else { color(what, hex) };
    let colors = &worn.colors;
    // Wearing a theme puts Flow's fonts after its own, as they are here.
    let mut fonts = worn.font.families.clone();
    let flows = FontConfig::default().families;
    if fonts.ends_with(&flows) {
        fonts.truncate(fonts.len() - flows.len());
    }
    let (fx, crt) = (worn.fx.crt, &worn.crt);
    let parts = |c: &CrtConfig| (c.scanlines, c.interlace, c.glow, c.decay, c.grille, c.vignette, c.curvature);
    let preset = crate::settings::builtin_crt_presets().into_iter().find(|p| (p.strength, (p.scanlines, p.interlace, p.glow, p.decay, p.grille, p.vignette, p.curvature)) == (fx, parts(crt)));
    let none = fx == 0.0 && parts(crt) == parts(&CrtConfig::default());
    let mut keys = serde_json::Map::new();
    let mut liquid = saved.look.get("fx").and_then(|f| f.as_object()).cloned().unwrap_or_default();
    if preset.is_some() || none {
        liquid.remove("crt");
    } else if let Some(tube) = saved.look.get("crt") {
        keys.insert("crt".into(), tube.clone());
    }
    if !liquid.is_empty() {
        keys.insert("fx".into(), serde_json::Value::Object(liquid));
    }
    let id = exported_id(name);
    let theme = ThemeEntry {
        id: id.clone(),
        name: name.clone(),
        names: BTreeMap::new(),
        fonts,
        font_files: Vec::new(),
        text: color("text", &colors.text)?,
        cursor: color("cursor", &colors.cursor)?,
        background: color("background", &colors.background)?,
        border: maybe("border", &colors.border)?,
        ansi: colors.ansi.iter().flatten().map(|c| color("color", c)).collect::<Result<_, _>>()?,
        phosphor: maybe("phosphor", colors.phosphor.as_deref().unwrap_or_default())?,
        cursor_shape: worn.cursor.shape.clone(),
        blink: worn.cursor.blink,
        cursor_speed: worn.cursor.speed,
        crt: preset.map(|p| p.name).unwrap_or_default(),
        start: saved.theme.start.clone(),
        baud: saved.theme.baud,
        prompt: Some(saved.theme.prompt.clone()).filter(|p| !p.is_empty()),
        sounds: Sounds::named(&saved.theme.sounds).name().into(),
        screen: None,
        frame: false,
        logo: String::new(),
        keys,
    };
    let package = if id.ends_with("-theme") { id } else { format!("{id}-theme") };
    Ok(ThemeFile { format: FORMAT, package, version: EXPORTED.into(), themes: vec![theme] })
}

/// The id a theme saved under `name` has in its package: its letters and
/// digits in lowercase, dashes between, at most 32 of them; never a
/// built-in look's (whose place it would take on the computer it's
/// installed on), Flow's own or your own, which get `-theme` after.
fn exported_id(name: &str) -> String {
    let mut id = String::new();
    for c in name.chars().flat_map(char::to_lowercase) {
        if id.len() == 32 {
            break;
        }
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            id.push(c);
        } else if !id.is_empty() && !id.ends_with('-') {
            id.push('-');
        }
    }
    let id = id.trim_end_matches('-');
    match id {
        "" => "my-theme".into(),
        id if id == super::OWN || LOOKS.iter().any(|l| l.id == id) => format!("{id}-theme"),
        id => id.into(),
    }
}

/// Packs `file` as the `.tar.gz` at `tarball` (a package of its
/// `themes.json` alone, which an exported theme is), as `flowapp install`
/// takes one: written into a folder of its own and read back as Flow
/// reads a package first, so one that wouldn't wear isn't made, then
/// packed by the system's `tar` owned by no one, as Flow's own packages
/// are, so a package shared names nobody's account.
pub fn pack(file: &ThemeFile, tarball: &Path) -> Result<(), String> {
    static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let stage = std::env::temp_dir().join(format!("flow-theme-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&stage);
    let packed = (|| {
        std::fs::create_dir_all(&stage).map_err(|e| format!("{}: {e}", stage.display()))?;
        let json = serde_json::to_string_pretty(file).map_err(|e| e.to_string())? + "\n";
        std::fs::write(stage.join(FILE), json).map_err(|e| format!("{FILE}: {e}"))?;
        read(&stage)?;
        let mut tar = Command::new(if cfg!(target_os = "macos") { "/usr/bin/tar" } else { "tar" });
        if cfg!(target_os = "linux") {
            // GNU tar.
            tar.args(["--owner=0", "--group=0", "--numeric-owner"]);
        } else {
            // bsdtar: the Mac's, and Windows' own tar.exe.
            if cfg!(target_os = "macos") {
                tar.env("COPYFILE_DISABLE", "1").arg("--no-mac-metadata");
            }
            tar.args(["--uid", "0", "--gid", "0", "--uname", "", "--gname", ""]);
        }
        let out = tar.arg("-czf").arg(tarball).arg("-C").arg(&stage).arg(".").output().map_err(|e| format!("tar: {e}"))?;
        if !out.status.success() {
            let _ = std::fs::remove_file(tarball);
            return Err(format!("tar: {}", String::from_utf8_lossy(&out.stderr).trim()));
        }
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&stage);
    packed
}

/// `file`, a path in a package, as a file in `dir`: never one outside it.
fn inside(dir: &Path, file: &str) -> Result<PathBuf, String> {
    let path = Path::new(file);
    if file.is_empty() || !path.components().all(|c| matches!(c, Component::Normal(_))) {
        return Err(format!("“{file}” isn't a file in the package"));
    }
    let at = dir.join(path);
    if !at.is_file() {
        return Err(format!("the package has no {file}"));
    }
    Ok(at)
}

fn leak(s: &str) -> &'static str {
    Box::leak(s.to_owned().into_boxed_str())
}

fn leak_all(lines: &[String]) -> &'static [&'static str] {
    Box::leak(lines.iter().map(|l| leak(l)).collect::<Vec<_>>().into_boxed_slice())
}

/// A theme from a package, as Flow wears it.
#[derive(Debug)]
pub struct Packaged {
    pub look: &'static Look,
    /// The package it came in.
    pub package: String,
    name: String,
    names: BTreeMap<String, String>,
    /// Its font files, for the atlas to load.
    pub font_files: Vec<PathBuf>,
    /// Its own startup logo, a picture.
    pub logo: Option<PathBuf>,
}

impl Packaged {
    /// Its name in the language Flow speaks, else in English, else its id.
    pub fn name(&self) -> String {
        let language = crate::i18n::language();
        let by_language = self.names.iter().find(|(code, _)| language.split(['-', '_']).next() == Some(code.as_str())).map(|(_, n)| n);
        by_language.or(Some(&self.name).filter(|n| !n.is_empty())).cloned().unwrap_or_else(|| self.look.id.to_string())
    }

    /// Whether it takes a built-in look's place.
    pub fn replaces(&self) -> bool {
        LOOKS.iter().any(|l| l.id == self.look.id)
    }
}

/// The package unpacked in `dir`'s `themes.json`, read but its themes not
/// yet checked.
fn file(dir: &Path) -> Result<ThemeFile, String> {
    let path = dir.join(FILE);
    if std::fs::metadata(&path).map_err(|e| format!("{FILE}: {e}"))?.len() > MOST {
        return Err(format!("{FILE} is more than {MOST} bytes"));
    }
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{FILE}: {e}"))?;
    let file: ThemeFile = serde_json::from_str(&text).map_err(|e| format!("{FILE}: {e}"))?;
    if file.format != FORMAT {
        return Err(format!("{FILE} is format {} (this Flow reads {FORMAT})", file.format));
    }
    Ok(file)
}

/// The package unpacked in `dir`, read and checked as [`read`] reads it,
/// but its themes neither kept nor worn: a look at a package before it's
/// installed (Settings' theme gallery), or a check before it is.
pub fn peek(dir: &Path) -> Result<ThemeFile, String> {
    let file = file(dir)?;
    let mut ids: Vec<&str> = Vec::new();
    for entry in &file.themes {
        if ids.contains(&entry.id.as_str()) {
            return Err(format!("two themes are “{}”", entry.id));
        }
        ids.push(&entry.id);
        entry.look(dir)?;
    }
    Ok(file)
}

/// A package of themes read to be shown, not worn (the theme gallery's
/// preview): its themes, and the font files and each theme's logo it
/// brings, as bytes read before the folder it was unpacked in goes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Peeked {
    pub file: ThemeFile,
    pub fonts: Vec<Vec<u8>>,
    /// Each theme's logo, by its id, a PNG.
    pub logos: BTreeMap<String, Vec<u8>>,
}

/// The most of one font file or logo a preview reads.
const SHOWN_MOST: u64 = 16 << 20;

/// [`peek`], with the fonts and logos the package brings read for a
/// preview: each file at most [`SHOWN_MOST`], one too big or unreadable
/// left out (the preview then draws in Flow's letters, or without it).
pub fn peek_shown(dir: &Path) -> Result<Peeked, String> {
    let file = peek(dir)?;
    let small = |p: &Path| std::fs::metadata(p).ok().filter(|m| m.len() <= SHOWN_MOST).and_then(|_| std::fs::read(p).ok());
    let (mut fonts, mut seen, mut logos) = (Vec::new(), Vec::new(), BTreeMap::new());
    for entry in &file.themes {
        let (_, font_files, logo) = entry.look(dir)?;
        for font in font_files {
            if !seen.contains(&font) {
                fonts.extend(small(&font));
                seen.push(font);
            }
        }
        if let Some(bytes) = logo.as_deref().and_then(small) {
            logos.insert(entry.id.clone(), bytes);
        }
    }
    Ok(Peeked { file, fonts, logos })
}

/// The themes in the package unpacked in `dir`: none at all if any of them
/// is wrong, saying what.
pub fn read(dir: &Path) -> Result<Vec<Packaged>, String> {
    let file = file(dir)?;
    let mut themes = Vec::new();
    for entry in &file.themes {
        if themes.iter().any(|t: &Packaged| t.look.id == entry.id) {
            return Err(format!("two themes are “{}”", entry.id));
        }
        let (look, font_files, logo) = entry.look(dir)?;
        themes.push(Packaged { look: Box::leak(Box::new(look)), package: file.package.clone(), name: entry.name.clone(), names: entry.names.clone(), font_files, logo });
    }
    Ok(themes)
}

/// Every theme the packages `flowapp` installed under `root` bring: the
/// catalog's first, then the user's own ([`crate::apps::Kept::own`]), each
/// by package name; where two give the same id, the first's. A theme of
/// the user's own never takes the place of one of Flow's own looks, as the
/// catalog's `flow-core-themes` may: anyone's file could then put its
/// words in a theme the user already wears, under that theme's name.
/// flowapp refuses one, and one from before is left out, as is a package
/// that won't read, saying why in the log.
pub fn installed(root: &Path) -> Vec<Packaged> {
    let catalog = crate::apps::saved_catalog(root).map(|read| read.catalog);
    let installed = crate::apps::Installed::load(root);
    let (own, flows): (Vec<_>, Vec<_>) = installed.apps.iter().filter(|(_, k)| k.themes()).partition(|(name, k)| k.own(name, catalog.as_ref()));
    let mut all: Vec<Packaged> = Vec::new();
    for ((name, kept), own) in flows.into_iter().map(|p| (p, false)).chain(own.into_iter().map(|p| (p, true))) {
        match read(&crate::apps::app_dir(root, name, &kept.version)) {
            Ok(themes) => {
                for theme in themes {
                    if own && LOOKS.iter().any(|l| l.id == theme.look.id) {
                        log::warn!("the theme “{}” in {name}, a package of your own, would take the place of Flow's own", theme.look.id);
                    } else if all.iter().any(|a| a.look.id == theme.look.id) {
                        log::warn!("the theme “{}” in {name} is another package's already", theme.look.id);
                    } else {
                        all.push(theme);
                    }
                }
            }
            Err(error) => log::warn!("the themes in {name} {}: {error}", kept.version),
        }
    }
    all
}

/// The themes installed packages bring, as [`set`] was last given them.
#[cfg(not(test))]
static ALL: parking_lot::RwLock<&'static [Packaged]> = parking_lot::const_rwlock(&[]);

// Each test thread has its own, so tests can set them side by side.
#[cfg(test)]
thread_local! {
    static ALL: std::cell::Cell<&'static [Packaged]> = const { std::cell::Cell::new(&[]) };
}

/// Makes `themes` the packaged themes Flow wears from now on.
pub fn set(themes: Vec<Packaged>) {
    let themes: &'static [Packaged] = Box::leak(themes.into_boxed_slice());
    #[cfg(not(test))]
    {
        *ALL.write() = themes;
    }
    #[cfg(test)]
    ALL.with(|all| all.set(themes));
}

/// The packaged themes Flow wears.
pub fn all() -> &'static [Packaged] {
    #[cfg(not(test))]
    {
        *ALL.read()
    }
    #[cfg(test)]
    {
        ALL.with(|all| all.get())
    }
}

/// The packaged theme with `id`, if one is installed.
pub fn get(id: &str) -> Option<&'static Packaged> {
    all().iter().find(|p| p.look.id == id)
}

/// Every font file the packaged themes bring.
pub fn font_files() -> Vec<PathBuf> {
    all().iter().flat_map(|p| p.font_files.iter().cloned()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, ThemeConfig};
    use crate::looks::{find, ids, name, save_as, wear, OWN};

    fn tempdir() -> PathBuf {
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("flow-themes-test-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// `file` written as a package in a folder of its own, and read.
    fn package(file: &ThemeFile) -> Result<Vec<Packaged>, String> {
        let dir = tempdir();
        std::fs::write(dir.join(FILE), serde_json::to_string(file).unwrap()).unwrap();
        read(&dir)
    }

    fn with(themes: Vec<ThemeEntry>) -> ThemeFile {
        ThemeFile { format: FORMAT, package: "mine".into(), version: "1.0".into(), themes }
    }

    #[test]
    fn flows_own_machines_read_back_from_their_package_as_they_are_built_in() {
        let read = package(&core_themes("0.1.0")).unwrap();
        assert_eq!(read.len(), LOOKS.len() - 1, "every machine, not Flow's own look");
        for theme in &read {
            let built_in = LOOKS.iter().find(|l| l.id == theme.look.id).unwrap();
            assert_eq!(ThemeEntry::of(theme.look, "", BTreeMap::new()), ThemeEntry::of(built_in, "", BTreeMap::new()), "{}", theme.look.id);
            assert!(theme.replaces());
        }
        assert_eq!(read.iter().find(|t| t.look.id == "green").unwrap().names["ja"], crate::i18n::in_language("ja", "settings.theme.green").unwrap());
    }

    #[test]
    fn a_packaged_theme_takes_a_built_in_ones_place_and_a_new_one_comes_after_them() {
        let mut green = ThemeEntry::of(find("green"), "Green screen", BTreeMap::new());
        green.text = "#00FF00".into();
        let vt = ThemeEntry { id: "vt52".into(), name: "VT52".into(), names: [("ja".to_string(), "VT52 端末".to_string())].into(), screen: Some((80, 24)), ..ThemeEntry::default() };
        set(package(&with(vec![green, vt])).unwrap());
        assert_eq!(find("green").text, [0x00, 0xFF, 0x00]);
        assert_eq!(name("green"), crate::i18n::t("settings.theme.green"), "a built-in one's place keeps its name");
        let base = Config::default();
        let all = ids(&base);
        assert_eq!(all[LOOKS.len()..], ["vt52", OWN]);
        assert_eq!(name("vt52"), "VT52");
        crate::i18n::set_language("ja", &[]);
        assert_eq!(name("vt52"), "VT52 端末");
        crate::i18n::set_language("en", &[]);
        let worn = wear(&base, "vt52");
        assert_eq!((worn.ui.look.as_str(), crate::settings::screens(&worn)[0]), ("vt52", (80, 24)));
        set(Vec::new());
        assert_eq!(find("green").text, [0x41, 0xFF, 0x6E], "the built-in one again");
        assert_eq!(find("vt52").id, "flow", "an id no longer installed is Flow's look");
    }

    #[test]
    fn the_notanamigas_frame_goes_with_its_package_and_one_written_before_keeps_it() {
        let mut amiga = ThemeEntry::of(find("amiga"), "notanAmiga", BTreeMap::new());
        assert!(amiga.frame && serde_json::to_string(&amiga).unwrap().contains("\"frame\":true"));
        assert!(!serde_json::to_string(&ThemeEntry::of(find("green"), "Green", BTreeMap::new())).unwrap().contains("frame"), "none unless it has one");
        // A package from before themes had a frame, in the notanAmiga's place.
        amiga.frame = false;
        set(package(&with(vec![amiga])).unwrap());
        assert!(find("amiga").frame);
        let mut worn = Config::default();
        find("amiga").apply(&mut worn);
        assert!(crate::looks::framed(&worn));
        worn.ui.look_frame = false;
        assert!(!crate::looks::framed(&worn), "Theme details' switch off");
        find("green").apply(&mut worn);
        worn.ui.look_frame = true;
        assert!(!crate::looks::framed(&worn), "a machine whose desktop drew none");
        set(Vec::new());
    }

    #[test]
    fn a_theme_may_bring_more_of_the_looks_settings() {
        let mut liquid = ThemeEntry { id: "aquarium".into(), ..ThemeEntry::default() };
        liquid.keys = serde_json::json!({ "fx": { "style": "water", "water_view": "below" } }).as_object().unwrap().clone();
        set(package(&with(vec![liquid])).unwrap());
        let worn = wear(&Config::default(), "aquarium");
        assert_eq!((worn.fx.style.as_str(), worn.fx.water_view.as_str()), ("water", "below"));
        set(Vec::new());
    }

    #[test]
    fn a_package_with_anything_wrong_is_refused_whole() {
        let bad: Vec<(ThemeEntry, &str)> = vec![
            (ThemeEntry { id: "flow".into(), ..ThemeEntry::default() }, "Flow's own look"),
            (ThemeEntry { id: "own".into(), ..ThemeEntry::default() }, "your own"),
            (ThemeEntry { id: "Big Name".into(), ..ThemeEntry::default() }, "an id of other letters"),
            (ThemeEntry { id: "a".into(), text: "green".into(), ..ThemeEntry::default() }, "a color by name"),
            (ThemeEntry { id: "a".into(), ansi: vec!["#000000".into(); 15], ..ThemeEntry::default() }, "fifteen colors"),
            (ThemeEntry { id: "a".into(), cursor_shape: "heart".into(), ..ThemeEntry::default() }, "a cursor shape"),
            (ThemeEntry { id: "a".into(), crt: "mine".into(), ..ThemeEntry::default() }, "a CRT preset Flow hasn't"),
            (ThemeEntry { id: "a".into(), font_files: vec!["../../etc/passwd".into()], ..ThemeEntry::default() }, "a file outside it"),
            (ThemeEntry { id: "a".into(), logo: "/etc/hosts".into(), ..ThemeEntry::default() }, "a file anywhere"),
            (ThemeEntry { id: "a".into(), screen: Some((5000, 24)), ..ThemeEntry::default() }, "a screen too wide"),
        ];
        for (theme, what) in bad {
            assert!(package(&with(vec![ThemeEntry { id: "fine".into(), ..ThemeEntry::default() }, theme])).is_err(), "{what}");
        }
        let twice = ThemeEntry { id: "same".into(), ..ThemeEntry::default() };
        assert!(package(&with(vec![twice.clone(), twice])).is_err(), "two of one id");
        assert!(package(&ThemeFile { format: 2, ..with(Vec::new()) }).is_err(), "another format");
        assert_eq!(package(&with(vec![ThemeEntry { id: "fine-1".into(), ..ThemeEntry::default() }])).unwrap().len(), 1);
    }

    #[test]
    fn a_saved_theme_exported_is_worn_on_another_computer_as_it_was_saved() {
        let base = Config::default();
        let green = save_as(&wear(&base, "green"), "Green");
        let mut night = base.clone();
        (night.fx.style, night.fx.sun) = ("rain".into(), 0.3);
        let night = save_as(&night, "Night shift");
        let mut tube = wear(&base, OWN);
        tube.theme = ThemeConfig { start: vec!["^HELLO".into(), String::new()], baud: 300, prompt: "{cwd}> ".into(), sounds: "tones".into() };
        (tube.fx.crt, tube.crt.curvature, tube.colors.border) = (0.8, 0.42, "#102030".into());
        tube.font.families.insert(0, "Monaco".into());
        let tube = save_as(&tube, "夜");
        let green_crt = find("green").crt.unwrap();
        for (saved, id, named, crt, tube_in_keys) in [(&green, "green-theme", "green-theme", green_crt, false), (&night, "night-shift", "night-shift-theme", "", false), (&tube, "my-theme", "my-theme", "", true)] {
            let file = export(saved).unwrap();
            let theme = &file.themes[0];
            assert_eq!((theme.id.as_str(), file.package.as_str(), file.version.as_str(), theme.crt.as_str()), (id, named, EXPORTED, crt), "{}", saved.name);
            assert_eq!(theme.keys.contains_key("crt"), tube_in_keys, "a tube of no preset's goes in keys");
            set(package(&file).unwrap());
            assert_eq!(save_as(&wear(&base, id), &saved.name), *saved, "{id} worn is {} as it was saved", saved.name);
            set(Vec::new());
        }
        // Flow's fonts follow a theme's own, so only its own are named.
        assert_eq!(export(&tube).unwrap().themes[0].fonts, ["Monaco"]);
        assert_eq!(export(&night).unwrap().themes[0].fonts, Vec::<String>::new());
    }

    #[test]
    fn an_exported_themes_id_is_its_name_in_letters_a_package_takes() {
        let long = "a".repeat(50);
        for (name, id) in [("Night shift", "night-shift"), ("  Amber -- 2!", "amber-2"), ("Green", "green-theme"), ("Own", "own-theme"), ("flow", "flow-theme"), ("夜", "my-theme"), (long.as_str(), &long[..32])] {
            assert_eq!(exported_id(name), id, "{name}");
        }
    }

    #[test]
    fn an_exported_theme_packs_as_flowapp_installs_it_naming_nobody() {
        let mut night = Config::default();
        night.fx.style = "rain".into();
        let file = export(&save_as(&night, "Night shift")).unwrap();
        let dir = tempdir();
        let tarball = dir.join(format!("{}-{}.tar.gz", file.package, file.version));
        pack(&file, &tarball).unwrap();
        let entries = tar_entries(&std::fs::read(&tarball).unwrap());
        assert!(entries.iter().any(|e| e.0 == "./themes.json"), "{entries:?}");
        assert!(entries.iter().all(|(_, uid, gid, user, group)| (*uid, *gid, user.as_str(), group.as_str()) == (0, 0, "", "")), "{entries:?}");
        let root = dir.join("apps");
        let (name, kept) = crate::apps::install_file(&root, &tarball).unwrap();
        assert_eq!((name.as_str(), kept.version.as_str(), kept.themes()), ("night-shift-theme", EXPORTED, true));
        assert_eq!(installed(&root).iter().map(|t| t.look.id).collect::<Vec<_>>(), ["night-shift"]);
        // What Flow wouldn't read isn't packed.
        let mut wrong = file.clone();
        wrong.themes[0].cursor_shape = "heart".into();
        let not = dir.join("not.tar.gz");
        assert!(pack(&wrong, &not).is_err() && !not.exists());
    }

    #[test]
    fn a_package_of_your_own_never_wears_in_place_of_flows_themes_or_the_catalogs() {
        crate::i18n::set_language("en", &[]);
        let dir = tempdir();
        let root = dir.join("apps");
        let packed = |file: &ThemeFile| {
            let tarball = dir.join(format!("{}.tar.gz", file.package));
            pack(file, &tarball).unwrap();
            std::fs::read(&tarball).unwrap()
        };
        let theme = |id: &str, name: &str| ThemeEntry { id: id.into(), name: name.into(), prompt: Some(format!("{name} $ ")), ..ThemeEntry::default() };
        // The catalog's, named to come last, with Flow's green and a night
        // of its own; and a stranger's, named to come first, with the same
        // and a theme of its own, as a flowapp before it was refused could
        // have installed it (its themes' ids weren't checked then).
        let catalogs = ThemeFile { format: FORMAT, package: "zzz-themes".into(), version: "1.0".into(), themes: vec![theme("green", "Theirs"), theme("night", "Theirs")] };
        crate::apps::install_bytes(&root, "zzz-themes", "1.0", &[], &packed(&catalogs)).unwrap();
        let strangers = ThemeFile { format: FORMAT, package: "aaa-themes".into(), version: "1.0".into(), themes: vec![theme("green", "Stranger"), theme("amber", "Stranger"), theme("night", "Stranger"), theme("dusk", "Stranger")] };
        crate::apps::install_bytes(&root, "aaa-themes", "1.0", &[], &packed(&strangers)).unwrap();
        let mut kept = crate::apps::Installed::load(&root);
        kept.apps.get_mut("aaa-themes").unwrap().own_file = true;
        std::fs::write(root.join("installed.json"), serde_json::to_string(&kept).unwrap()).unwrap();
        set(installed(&root));
        // The catalog's take their places; the stranger's only its own.
        assert_eq!((find("green").prompt, find("night").prompt), (Some("Theirs $ "), Some("Theirs $ ")));
        assert_eq!(find("amber").prompt, LOOKS.iter().find(|l| l.id == "amber").unwrap().prompt, "Flow's own amber");
        assert_eq!(find("dusk").prompt, Some("Stranger $ "));
        assert_eq!(all().iter().map(|t| (t.look.id, t.package.as_str())).collect::<Vec<_>>(), [("green", "zzz-themes"), ("night", "zzz-themes"), ("dusk", "aaa-themes")]);
        // And flowapp no longer installs it so.
        let tarball = dir.join("aaa-themes.tar.gz");
        assert!(crate::apps::install_file(&root, &tarball).unwrap_err().contains("“green”"));
        set(Vec::new());
    }

    /// The files in the `.tar.gz` `bytes` (no pax headers): each one's
    /// name, its owner and group by number, and by name.
    fn tar_entries(bytes: &[u8]) -> Vec<(String, u64, u64, String, String)> {
        // gzip's ten bytes, then what its flags say follows them.
        let flags = bytes[3];
        let mut at = 10;
        if flags & 4 != 0 {
            at += 2 + u16::from_le_bytes([bytes[at], bytes[at + 1]]) as usize;
        }
        for bit in [8, 16] {
            if flags & bit != 0 {
                at += bytes[at..].iter().position(|b| *b == 0).unwrap() + 1;
            }
        }
        if flags & 2 != 0 {
            at += 2;
        }
        let tar = miniz_oxide::inflate::decompress_to_vec(&bytes[at..bytes.len() - 8]).unwrap();
        let text = |b: &[u8]| String::from_utf8_lossy(b.split(|c| *c == 0).next().unwrap()).trim().to_string();
        let octal = |b: &[u8]| u64::from_str_radix(&text(b), 8).unwrap();
        let (mut entries, mut block) = (Vec::new(), 0);
        while block + 512 <= tar.len() && tar[block] != 0 {
            let header = &tar[block..block + 512];
            if !matches!(header[156], b'x' | b'g') {
                entries.push((text(&header[..100]), octal(&header[108..116]), octal(&header[116..124]), text(&header[265..297]), text(&header[297..329])));
            }
            block += 512 + octal(&header[124..136]).div_ceil(512) as usize * 512;
        }
        entries
    }

    // Programs write packages with the SDK's `flow_sdk::theme`, which keeps
    // its own copy of these types and of what this refuses. These tests, at
    // the tag Flow takes, fail where the two have drifted apart.
    use flow_sdk::theme as sdk;

    #[test]
    fn a_package_read_to_be_shown_brings_its_fonts_and_logos_as_they_are() {
        let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets");
        let font = assets.join("fonts/flow-teletype.ttf");
        let logo = assets.join("logos/kickstart.png");
        let dir = tempdir();
        std::fs::create_dir_all(dir.join("fonts")).unwrap();
        std::fs::create_dir_all(dir.join("logos")).unwrap();
        std::fs::copy(&font, dir.join("fonts/tt.ttf")).unwrap();
        std::fs::copy(&logo, dir.join("logos/disk.png")).unwrap();
        // Two themes sharing the font, one with a logo.
        let tt = |id: &str| ThemeEntry { id: id.into(), name: id.into(), font_files: vec!["fonts/tt.ttf".into()], fonts: vec!["Flow Teletype".into()], ..ThemeEntry::default() };
        let file = with(vec![ThemeEntry { logo: "logos/disk.png".into(), ..tt("a") }, tt("b")]);
        std::fs::write(dir.join(FILE), serde_json::to_string(&file).unwrap()).unwrap();
        let peeked = peek_shown(&dir).unwrap();
        assert_eq!(peeked.file, file);
        assert_eq!(peeked.fonts, [std::fs::read(&font).unwrap()], "the font once");
        assert_eq!(peeked.logos.keys().collect::<Vec<_>>(), ["a"]);
        assert_eq!(peeked.logos["a"], std::fs::read(&logo).unwrap());
        // A package with neither reads as before.
        let plain = tempdir();
        std::fs::write(plain.join(FILE), serde_json::to_string(&with(vec![ThemeEntry { id: "p".into(), ..ThemeEntry::default() }])).unwrap()).unwrap();
        let peeked = peek_shown(&plain).unwrap();
        assert!(peeked.fonts.is_empty() && peeked.logos.is_empty());
    }

    #[test]
    fn a_package_the_sdk_writes_reads_here_with_nothing_lost() {
        let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets");
        let vt52 = sdk::Theme::new("vt52")
            .name("VT52")
            .name_in("ja", "VT52 端末")
            .font_file(assets.join("fonts/flow-teletype.ttf"))
            .fonts(["Flow Teletype", "VT323"])
            .text(0xE8F0F0)
            .cursor(0xC0FFC0)
            .background(0x0A0F0A)
            .border(0x203020)
            .ansi(std::array::from_fn(|i| i as u32 * 0x0F0F0F))
            .phosphor(0xE8F0F0)
            .cursor_shape(sdk::CursorShape::Underline)
            .blink(true)
            .cursor_speed(1.5)
            .crt(sdk::Crt::Classic)
            .start(["", "^{tall}VT52 DECscope", "^{tall}VT52 DECscope", "{wait}", "^READY", ""])
            .baud(9600)
            .prompt("$ ")
            .sounds(sdk::Sounds::Terminal)
            .screen(80, 24)
            .logo(assets.join("logos/kickstart.png"))
            .key("fx", "style", "water")
            .key("fx", "sun", 0.8);
        let dir = tempdir();
        sdk::Package::new("vt-themes", "1.0").theme(vt52).theme(sdk::Theme::new("plain")).write(&dir).unwrap();
        let file: ThemeFile = serde_json::from_str(&std::fs::read_to_string(dir.join(FILE)).unwrap()).unwrap();
        let read = read(&dir).unwrap();
        assert_eq!(read.len(), 2);
        for (theme, entry) in read.iter().zip(&file.themes) {
            let back = ThemeEntry { font_files: entry.font_files.clone(), logo: entry.logo.clone(), ..ThemeEntry::of(theme.look, &theme.name, theme.names.clone()) };
            assert_eq!(&back, entry, "{} as worn is {} as written", entry.id, entry.id);
        }
        let vt = &read[0];
        assert_eq!((vt.package.as_str(), vt.names["ja"].as_str()), ("vt-themes", "VT52 端末"));
        assert_eq!((vt.look.sounds, vt.look.crt, vt.look.screen), (Sounds::Terminal, Some("classic"), Some((80, 24))));
        assert_eq!((vt.font_files.as_slice(), vt.logo.as_deref()), ([dir.join("fonts/flow-teletype.ttf")].as_slice(), Some(dir.join("logos/kickstart.png").as_path())));
        set(read);
        let worn = wear(&Config::default(), "vt52");
        assert_eq!((worn.fx.style.as_str(), worn.fx.sun), ("water", 0.8));
        set(Vec::new());
    }

    #[test]
    fn the_sdk_writes_flows_own_package_as_flow_does_and_takes_the_names_it_does() {
        let mut flows = core_themes("0.1.0");
        let json = serde_json::to_string_pretty(&flows).unwrap() + "\n";
        let theirs: sdk::ThemeFile = serde_json::from_str(&json).unwrap();
        // From Flow-SDK 0.1.4, the SDK refuses a package that takes one of
        // Flow's own looks' ids, as flowapp refuses one of the user's own.
        // Flow's own package is the one that may, so the SDK's check turns
        // it away, and every id in it is one the SDK knows as Flow's.
        assert!(theirs.check().is_err(), "Flow's own ids are refused for a package of the user's own");
        for entry in &flows.themes {
            assert!(sdk::BUILT_IN.contains(&entry.id.as_str()), "{} is one of Flow's own to the SDK", entry.id);
        }
        assert!(sdk::BUILT_IN.contains(&crate::looks::OWN));
        // Flow-SDK 0.1.3's entry has no frame yet: the notanAmiga's alone
        // has one, and a package taking its place keeps the built-in's
        // (`builtin_frame`), so nothing is lost meanwhile.
        for entry in &mut flows.themes {
            entry.frame = false;
        }
        let json = serde_json::to_string_pretty(&flows).unwrap() + "\n";
        assert_eq!(theirs.to_json(), json, "every field of every machine, as examples/theme_package.rs writes it");
        assert_eq!(serde_json::to_value(sdk::ThemeEntry::default()).unwrap(), serde_json::to_value(ThemeEntry::default()).unwrap(), "the same defaults");
        assert_eq!(sdk::KEYS[..], crate::looks::THEME_KEYS[..]);
        let mut presets: Vec<String> = crate::settings::builtin_crt_presets().into_iter().map(|p| p.name).collect();
        let mut named: Vec<&str> = sdk::Crt::ALL.iter().map(|c| c.name()).collect();
        presets.sort();
        named.sort();
        assert_eq!(named, presets);
        for sounds in sdk::Sounds::ALL {
            assert_eq!(Sounds::named(sounds.name()).name(), sounds.name());
        }
        // And a saved theme Flow exports, the SDK reads and takes.
        let dir = tempdir();
        std::fs::write(dir.join(FILE), serde_json::to_string_pretty(&export(&save_as(&wear(&Config::default(), "pc"), "Mine")).unwrap()).unwrap()).unwrap();
        sdk::Package::read(&dir).unwrap().check().unwrap();
    }
}
