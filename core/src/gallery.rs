//! Settings' theme gallery: the packages of themes flowapp's catalog offers
//! (and those installed), each read to show its themes before it's
//! installed, a theme's look drawn from what its package says (its colors,
//! its sixteen, a few lines of its start screen, its prompt, its font's
//! name, its tube), and Install, Upgrade or Wear for the one chosen.
//!
//! A package is fetched, checked against the catalog and Flow's key, and
//! read as an install would (`apps::peek_themes`, `packaged::peek`) on a
//! thread of its own the first time it's chosen; nothing is installed or
//! worn until asked. This is what the gallery holds and how the keys move
//! in it; `ui::draw_gallery` draws it and `App` does what it asks.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::apps::{self, Catalog, Installed};
use crate::graphics::Image;
use crate::looks::packaged::{Peeked, ThemeEntry, ThemeFile};

/// A package read for the gallery: its themes, the fonts it brings, as
/// bytes for the atlas to load so the preview is drawn in them, and each
/// theme's logo, decoded.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Preview {
    pub file: ThemeFile,
    pub fonts: Vec<Vec<u8>>,
    pub logos: BTreeMap<String, Logo>,
}

impl Preview {
    /// What `peeked` says, its logos decoded (one that won't, left out).
    pub fn of(peeked: Peeked) -> Self {
        let logos = peeked.logos.iter().filter_map(|(id, png)| logo_of(png).map(|image| (id.clone(), Logo(Arc::new(image))))).collect();
        Preview { file: peeked.file, fonts: peeked.fonts, logos }
    }
}

/// A theme's logo as the gallery shows it.
#[derive(Clone)]
pub struct Logo(pub Arc<Image>);

impl std::fmt::Debug for Logo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Logo({} × {})", self.0.width, self.0.height)
    }
}

impl PartialEq for Logo {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

/// A theme's logo, a PNG of at most 8 MB, premultiplied as pictures are
/// drawn; none where it won't decode.
pub fn logo_of(png: &[u8]) -> Option<Image> {
    if png.len() > 8 << 20 {
        return None;
    }
    let (w, h, mut rgba) = crate::graphics::decode_png(png).map_err(|e| log::warn!("a theme's logo: {e}")).ok()?;
    for px in rgba.as_chunks_mut::<4>().0 {
        let a = px[3] as u16;
        for c in &mut px[..3] {
            *c = (*c as u16 * a / 255) as u8;
        }
    }
    Some(Image::new(w, h, rgba))
}

/// What's known of a package's themes.
#[derive(Debug, Clone, PartialEq)]
pub enum Contents {
    Reading,
    Read(Preview),
    Failed(String),
}

/// A package of themes, as the gallery lists it.
#[derive(Debug, Clone, PartialEq)]
pub struct Package {
    pub name: String,
    pub summary: String,
    /// The version the catalog offers, where it does.
    pub latest: Option<String>,
    /// The version installed, where one is.
    pub installed: Option<String>,
    /// What's installed is a package of the user's own, from a file, which
    /// the catalog's of its name doesn't upgrade (`apps::in_the_way`).
    pub own: bool,
    /// The Flow that version needs, where it's newer than this one.
    pub needs_flow: Option<String>,
    /// Its themes, once asked for: none until it's chosen.
    pub contents: Option<Contents>,
}

impl Package {
    /// A newer version than the one installed can be had here.
    pub fn upgradable(&self) -> bool {
        !self.own && self.needs_flow.is_none() && matches!((&self.installed, &self.latest), (Some(have), Some(latest)) if apps::newer(latest, have))
    }

    /// Its themes, once read.
    pub fn themes(&self) -> &[ThemeEntry] {
        match &self.contents {
            Some(Contents::Read(read)) => &read.file.themes,
            _ => &[],
        }
    }
}

/// What a button in the gallery does.
#[derive(Debug, Clone, PartialEq)]
pub enum Act {
    Install,
    Upgrade,
    /// Wears the theme, by its id, as Settings' Theme row does.
    Wear(String),
}

impl Act {
    pub fn label(&self) -> &'static str {
        crate::i18n::t(match self {
            Act::Install => "gallery.install",
            Act::Upgrade => "gallery.upgrade",
            Act::Wear(_) => "gallery.wear",
        })
    }
}

/// The gallery, open over Settings.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Gallery {
    pub packages: Vec<Package>,
    /// The package chosen, and the theme chosen in it.
    pub chosen: usize,
    pub theme: usize,
    /// The keys move through the themes, not the packages.
    pub on_themes: bool,
}

impl Gallery {
    /// The packages of themes in `catalog` and `installed`, by name.
    pub fn new(installed: &Installed, catalog: Option<&Catalog>) -> Self {
        let mut names: Vec<String> = installed.apps.iter().filter(|(_, kept)| kept.themes()).map(|(name, _)| name.clone()).collect();
        if let Some(catalog) = catalog {
            names.extend(catalog.themes.keys().filter(|n| !names.contains(n)).cloned().collect::<Vec<_>>());
        }
        names.sort();
        let packages = names
            .into_iter()
            .map(|name| {
                let app = catalog.and_then(|c| c.themes.get(&name));
                let here = app.and_then(apps::App::here);
                Package {
                    summary: app.map(|a| a.summary.clone()).unwrap_or_default(),
                    latest: here.map(|d| d.version.clone()),
                    installed: installed.apps.get(&name).map(|k| k.version.clone()),
                    own: installed.apps.get(&name).is_some_and(|k| k.own_file),
                    needs_flow: here.and_then(|d| d.needs_newer_flow(crate::version::VERSION)).map(str::to_string),
                    contents: None,
                    name,
                }
            })
            .collect();
        Gallery { packages, ..Gallery::default() }
    }

    pub fn package(&self) -> Option<&Package> {
        self.packages.get(self.chosen)
    }

    /// The theme chosen, once its package is read.
    pub fn theme(&self) -> Option<&ThemeEntry> {
        self.package()?.themes().get(self.theme)
    }

    /// The chosen theme's logo, where its package brings one.
    pub fn logo(&self) -> Option<&Arc<Image>> {
        let theme = self.theme()?;
        match &self.package()?.contents {
            Some(Contents::Read(read)) => read.logos.get(&theme.id).map(|l| &l.0),
            _ => None,
        }
    }

    /// The package chosen, where its themes haven't been asked for yet:
    /// the one to read now.
    pub fn to_read(&self) -> Option<&Package> {
        self.package().filter(|p| p.contents.is_none())
    }

    /// Chooses package `i`, its first theme.
    pub fn choose(&mut self, i: usize) {
        if i < self.packages.len() {
            self.chosen = i;
            self.theme = 0;
            self.on_themes = false;
        }
    }

    /// Chooses theme `i` of the package chosen.
    pub fn choose_theme(&mut self, i: usize) {
        if i < self.package().map_or(0, |p| p.themes().len()) {
            self.theme = i;
            self.on_themes = true;
        }
    }

    /// ↑ or ↓ in the list that has the keys.
    pub fn step(&mut self, down: bool) {
        let move_in = |at: usize, n: usize| if down { (at + 1).min(n.saturating_sub(1)) } else { at.saturating_sub(1) };
        if self.on_themes {
            self.theme = move_in(self.theme, self.package().map_or(0, |p| p.themes().len()));
        } else {
            let at = move_in(self.chosen, self.packages.len());
            if at != self.chosen {
                self.choose(at);
            }
        }
    }

    /// ← or →: the packages' list or the themes', where its package has
    /// some.
    pub fn switch(&mut self, to_themes: bool) {
        self.on_themes = to_themes && self.package().is_some_and(|p| !p.themes().is_empty());
    }

    /// What reading `name`'s themes came to.
    pub fn read(&mut self, name: &str, contents: Contents) {
        if let Some(p) = self.packages.iter_mut().find(|p| p.name == name) {
            p.contents = Some(contents);
        }
    }

    /// The versions installed, afresh (after an install).
    pub fn installed_now(&mut self, installed: &Installed) {
        for p in &mut self.packages {
            p.installed = installed.apps.get(&p.name).map(|k| k.version.clone());
            p.own = installed.apps.get(&p.name).is_some_and(|k| k.own_file);
        }
    }

    /// The buttons for what's chosen: Install where its package isn't
    /// installed (and needs no newer Flow); Wear the theme where it is, and
    /// Upgrade where a newer one's out. Enter does the first.
    pub fn acts(&self) -> Vec<Act> {
        let Some(p) = self.package() else { return Vec::new() };
        match &p.installed {
            None if p.latest.is_some() && p.needs_flow.is_none() => vec![Act::Install],
            None => Vec::new(),
            Some(_) => {
                let mut acts: Vec<Act> = self.theme().map(|t| Act::Wear(t.id.clone())).into_iter().collect();
                if p.upgradable() {
                    acts.push(Act::Upgrade);
                }
                acts
            }
        }
    }
}

/// A theme's name in the language Flow speaks, else its own.
pub fn name_of(theme: &ThemeEntry) -> &str {
    theme.names.get(crate::i18n::language()).map(String::as_str).unwrap_or(&theme.name)
}

/// A start screen's or prompt's line without its markup (`{red}`,
/// `{tall}`, `{logo}`, a centering `^`), as plain words to show in a
/// preview.
pub fn plain(line: &str) -> String {
    let mut out = String::new();
    let mut rest = line.strip_prefix('^').unwrap_or(line);
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        match rest[open..].find('}') {
            Some(close) => rest = &rest[open + close + 1..],
            None => {
                rest = &rest[open..];
                break;
            }
        }
    }
    out.push_str(rest);
    out.trim_end().to_string()
}

/// For snapshots: Flow's own machines as a package, read, as the catalog
/// would offer it, not installed, the notanAmiga's chosen with its
/// picture as a logo.
pub fn made_up() -> Gallery {
    let file = crate::looks::packaged::core_themes("0.1.0");
    let mut gallery = Gallery {
        packages: vec![Package {
            name: crate::looks::packaged::CORE.to_string(),
            summary: "Flow's ten machines as themes".into(),
            latest: Some("0.1.0".into()),
            installed: None,
            own: false,
            needs_flow: None,
            // The notanAmiga's own picture as its logo, as a package may
            // bring one, so the preview shows a logo and a theme's letters.
            contents: Some(Contents::Read(Preview::of(Peeked { file, fonts: Vec::new(), logos: [("amiga".to_string(), crate::logos::KICKSTART.to_vec())].into() }))),
        }],
        ..Gallery::default()
    };
    let at = gallery.package().map_or(0, |p| p.themes().iter().position(|t| t.id == "amiga").unwrap_or(0));
    gallery.choose_theme(at);
    gallery
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apps::Kept;

    fn catalog() -> Catalog {
        Catalog::parse(
            r#"{"format":1,"apps":{"flow-rogue":{"version":"0.1.46","summary":"Not themes","programs":["flow-rogue"],"downloads":{}}},
            "themes":{
              "flow-core-themes":{"version":"0.1.1","summary":"Flow's machines","programs":[],"downloads":{"any":{"version":"0.1.1","url":"https://example.com/a.tar.gz","size":1,"sha256":"00","signature":"00"}}},
              "far-future":{"version":"2.0.0","summary":"Later","programs":[],"downloads":{"any":{"version":"2.0.0","url":"https://example.com/b.tar.gz","size":1,"sha256":"00","signature":"00","flow":"9.9.9"}}}
            }}"#,
        )
        .unwrap()
    }

    fn read_core(gallery: &mut Gallery) {
        gallery.read(crate::looks::packaged::CORE, Contents::Read(Preview { file: crate::looks::packaged::core_themes("0.1.0"), ..Preview::default() }));
    }

    #[test]
    fn a_previews_logo_is_decoded_and_shown_for_its_theme_alone() {
        let g = made_up();
        assert_eq!(g.theme().map(|t| t.id.as_str()), Some("amiga"));
        let logo = g.logo().expect("the notanAmiga's picture");
        assert_eq!((logo.width, logo.height), (130, 131));
        let mut other = g.clone();
        other.choose_theme(0);
        assert!(other.theme().is_some_and(|t| t.id != "amiga") && other.logo().is_none());
        // A logo that won't decode is left out, the theme read all the same.
        let file = crate::looks::packaged::core_themes("0.1.0");
        let preview = Preview::of(Peeked { file, fonts: Vec::new(), logos: [("amiga".to_string(), b"not a png".to_vec())].into() });
        assert!(preview.logos.is_empty() && !preview.file.themes.is_empty());
    }

    #[test]
    fn the_gallery_lists_the_catalogs_packages_of_themes_and_those_installed() {
        let mut installed = Installed::default();
        installed.apps.insert("mine".into(), Kept { version: "1.0.0".into(), programs: Vec::new(), kind: "themes".into(), ..Kept::default() });
        installed.apps.insert("flow-mines".into(), Kept { version: "0.1.9".into(), programs: vec!["flow-mines".into()], kind: String::new(), ..Kept::default() });
        let g = Gallery::new(&installed, Some(&catalog()));
        let names: Vec<&str> = g.packages.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["far-future", "flow-core-themes", "mine"], "themes only, apps left out");
        assert_eq!(g.packages[0].needs_flow.as_deref(), Some("9.9.9"));
        assert_eq!(g.packages[2].installed.as_deref(), Some("1.0.0"));
        assert!(g.packages.iter().all(|p| p.contents.is_none()), "nothing read until chosen");
        assert!(Gallery::new(&Installed::default(), None).packages.is_empty());
    }

    #[test]
    fn what_can_be_done_follows_where_the_package_stands() {
        let mut g = Gallery::new(&Installed::default(), Some(&catalog()));
        g.choose(1);
        assert_eq!(g.to_read().map(|p| p.name.as_str()), Some("flow-core-themes"));
        assert_eq!(g.acts(), [Act::Install]);
        read_core(&mut g);
        assert!(g.to_read().is_none() && g.package().unwrap().themes().len() >= 10);
        g.choose(0);
        assert!(g.acts().is_empty(), "it needs a newer Flow");
        // Installed, its themes can be worn, and an older one upgraded.
        let mut installed = Installed::default();
        installed.apps.insert("flow-core-themes".into(), Kept { version: "0.1.0".into(), programs: Vec::new(), kind: "themes".into(), ..Kept::default() });
        g.installed_now(&installed);
        g.choose(1);
        g.choose_theme(2);
        let id = g.theme().unwrap().id.clone();
        assert_eq!(g.acts(), [Act::Wear(id.clone()), Act::Upgrade]);
        // A package of your own of its name is worn, but not upgraded to
        // the catalog's, which won't install over it.
        installed.apps.get_mut("flow-core-themes").unwrap().own_file = true;
        g.installed_now(&installed);
        assert!(g.package().unwrap().own);
        assert_eq!(g.acts(), [Act::Wear(id)]);
        assert!(Gallery::new(&installed, Some(&catalog())).packages.iter().any(|p| p.name == "flow-core-themes" && p.own && !p.upgradable()));
    }

    #[test]
    fn the_keys_move_in_the_list_that_has_them() {
        let mut g = Gallery::new(&Installed::default(), Some(&catalog()));
        g.step(true);
        assert_eq!((g.chosen, g.on_themes), (1, false));
        g.switch(true);
        assert!(!g.on_themes, "no themes read yet to move into");
        read_core(&mut g);
        g.switch(true);
        g.step(true);
        g.step(true);
        assert_eq!((g.chosen, g.theme, g.on_themes), (1, 2, true));
        g.switch(false);
        g.step(false);
        assert_eq!((g.chosen, g.theme), (0, 0), "a package chosen starts at its first theme");
        g.step(false);
        assert_eq!(g.chosen, 0, "no further than the first");
        g.read("flow-core-themes", Contents::Failed("no".into()));
        assert!(matches!(g.packages[1].contents, Some(Contents::Failed(_))));
    }

    #[test]
    fn a_start_screens_markup_is_left_out_of_its_preview() {
        assert_eq!(plain("^{tall}{red}FLOW INSTRUMENTS{plain}"), "FLOW INSTRUMENTS");
        assert_eq!(plain("{on-#d5524d}  {#ffffff}READY."), "  READY.");
        assert_eq!(plain("{logo}"), "");
        assert_eq!(plain("a {brace"), "a {brace", "an unclosed brace stays");
    }

    #[test]
    fn the_snapshots_gallery_shows_flows_machines_read() {
        let g = made_up();
        assert_eq!(g.theme().map(|t| t.id.as_str()), Some("amiga"));
        assert_eq!(g.acts(), [Act::Install]);
    }
}
