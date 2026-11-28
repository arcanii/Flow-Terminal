//! The command palette (⇧⌘P, Ctrl+Shift+P): everything Flow can do, found
//! by typing a few letters of its name. Its items are Flow's commands (in
//! the words of the Mac's menus, with their keys), every setting (which
//! opens Settings at its row), every theme (worn as Settings' Theme row
//! wears it), every page of help, a new tab in each shell, each tab to
//! go to, and each group the tab in front may move into (`tab_groups`).
//!
//! What's typed matches an item by its letters in order, any case: a
//! letter at the start of a word, or straight after the one before, counts
//! for more, and a gap for less, so "nt" finds New Tab before Font size. A
//! setting is found by any words about it too, as Settings' own search
//! finds it ([`settings_search::Index`](crate::settings_search::Index)):
//! its hint, its choices' names ("amber" finds Phosphor) and its key in
//! config.toml, in the language in use and in English, after what's found
//! by name as well but ahead of letters scattered through one.
//! What was chosen last comes first where the matches are as good, and
//! first of all before anything's typed. The last few chosen are kept
//! between runs in `palette.json` beside config.toml ([`Recent`]), by keys
//! that don't change with the language ([`Item::key`]).

use std::cell::Cell;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::event::Command;
use crate::i18n::{t, tf};
use crate::settings::{Entry, SettingId};
use crate::settings_search::Index;

/// What choosing an item does.
#[derive(Debug, Clone, PartialEq)]
pub enum Act {
    Command(Command),
    /// Opens Settings at the row.
    Setting(SettingId),
    /// Wears the theme, as Settings' Theme row does.
    Theme(String),
    /// Opens the page of help, by its place in `help::Page::ALL`.
    Help(usize),
    /// Opens a tab in the shell, by its place among the profiles.
    Shell(usize),
    /// Brings the tab forward, by its id.
    Tab(u64),
    /// Moves the tab in front into the group, by its id.
    Group(u64),
    /// Takes the tab in front out of its group.
    Ungroup,
}

/// What kind of thing an item is, said beside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Command,
    Setting,
    Theme,
    Help,
    NewTab,
    Tab,
}

impl Kind {
    pub fn words(self) -> &'static str {
        t(match self {
            Kind::Command => "palette.kind_command",
            Kind::Setting => "palette.kind_setting",
            Kind::Theme => "palette.kind_theme",
            Kind::Help => "palette.kind_help",
            Kind::NewTab => "palette.kind_new_tab",
            Kind::Tab => "palette.kind_tab",
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub label: String,
    /// Where it is: a setting's section.
    pub detail: String,
    pub kind: Kind,
    /// Its key, as it's bound, where it has one.
    pub keys: String,
    pub act: Act,
    /// What it's remembered by among the recently chosen, the same in any
    /// language: `command:NewTab`, `setting:FontSize`, `theme:green`,
    /// `help:find`, `shell:zsh`. A tab's and a group's is empty: they may
    /// be gone by the next run, so they aren't remembered.
    pub key: String,
}

/// The tabs and groups as the palette opens: each tab's id, name and its
/// group's name (empty, loose), the groups the tab in front may move into
/// (by id and name, its own left out), and whether it's in one.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Tabs {
    pub tabs: Vec<(u64, String, String)>,
    pub groups: Vec<(u64, String)>,
    pub grouped: bool,
}

/// How many of the latest chosen are remembered.
pub const RECENT: usize = 8;

/// The items chosen lately, the latest first, by their keys, kept between
/// runs beside config.toml (snapshots and recordings neither read nor
/// write it).
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recent {
    #[serde(default)]
    pub keys: Vec<String>,
}

impl Recent {
    fn path() -> PathBuf {
        crate::config::config_dir().join("palette.json")
    }

    pub fn load() -> Self {
        Self::read(&Self::path())
    }

    fn read(path: &std::path::Path) -> Self {
        let mut recent: Recent = std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        recent.keys.truncate(RECENT);
        recent
    }

    pub fn save(&self) {
        self.write(&Self::path());
    }

    fn write(&self, path: &std::path::Path) {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(err) = serde_json::to_string(self).map_err(|e| e.to_string()).and_then(|t| std::fs::write(path, t).map_err(|e| e.to_string())) {
            log::warn!("could not write {}: {err}", path.display());
        }
    }

    /// Puts `key` first, once, keeping the latest [`RECENT`].
    pub fn chose(&mut self, key: &str) {
        self.keys.retain(|k| k != key);
        self.keys.insert(0, key.to_string());
        self.keys.truncate(RECENT);
    }
}

/// Flow's commands, in the menus' words; their keys are as they're bound
/// (`keys.rs`).
const COMMANDS: [(Command, &str); 42] = [
    (Command::NewTab, "menu.new_tab"),
    (Command::CloseTab, "menu.close_tab"),
    (Command::NextTab, "menu.next_tab"),
    (Command::PreviousTab, "menu.previous_tab"),
    (Command::RestartTab, "menu.restart_tab"),
    (Command::RenameTab, "menu.rename_tab"),
    (Command::NewGroup, "menu.new_group"),
    (Command::TabSidebar, "menu.tab_sidebar"),
    (Command::SplitRight, "menu.split_right"),
    (Command::SplitDown, "menu.split_down"),
    (Command::Find, "menu.find"),
    (Command::FindNext, "menu.find_next"),
    (Command::FindPrevious, "menu.find_previous"),
    (Command::SearchHistory, "menu.search_history"),
    (Command::Copy, "menu.copy"),
    (Command::Paste, "menu.paste"),
    (Command::SelectAll, "menu.select_all"),
    (Command::ClearBuffer, "menu.clear_buffer"),
    (Command::ScrollToBottom, "menu.scroll_to_bottom"),
    (Command::ScrollToTop, "menu.scroll_to_top"),
    (Command::PreviousCommand, "menu.previous_command"),
    (Command::NextCommand, "menu.next_command"),
    (Command::BiggerText, "menu.bigger"),
    (Command::SmallerText, "menu.smaller"),
    (Command::ActualSize, "menu.actual_size"),
    (Command::ToggleEfficient, "menu.efficient"),
    (Command::ToggleOnTop, "menu.on_top"),
    (Command::Record, "menu.record"),
    (Command::SaveTheme, "menu.save_theme"),
    (Command::ToggleAssistant, "menu.ask"),
    (Command::NewConversation, "menu.new_conversation"),
    (Command::AskWhy, "menu.ask_why"),
    (Command::GuideMode, "menu.guide_mode"),
    (Command::AutoMode, "menu.auto_mode"),
    (Command::ChangeKey, "menu.change_key"),
    (Command::ForgetKey, "menu.remove_key"),
    (Command::Settings, "menu.settings"),
    (Command::Help, "menu.help_item"),
    (Command::About, "menu.about"),
    (Command::CheckForUpdates, "menu.check_updates"),
    (Command::Restart, "menu.restart"),
    (Command::Quit, "menu.quit"),
];

/// Every item, in the order they come before anything's typed: Flow's
/// commands (Check for Updates only where there's an updater), every
/// setting in `entries` (Settings' rows, every section open), the themes
/// (their ids and names), the pages of help (their names and titles), a
/// new tab in each of `shells` (their ids and labels), then `tabs`' moves
/// for the tab in front and each tab to go to.
pub fn items(entries: &[Entry], themes: &[(String, String)], help: &[(&str, &str)], shells: &[(&str, String)], tabs: &Tabs, can_update: bool) -> Vec<Item> {
    let ai = crate::ai::provider::in_use().name();
    let app = [("app", &crate::APP_NAME as &dyn std::fmt::Display), ("ai", &ai)];
    let bound = crate::keys::bound();
    let key_of = |cmd: Command| bound.chords(cmd).first().map(|c| c.label(bound.mac())).unwrap_or_default();
    let mut items: Vec<Item> = COMMANDS
        .iter()
        .filter(|(cmd, ..)| can_update || *cmd != Command::CheckForUpdates)
        .map(|&(cmd, words)| Item { label: tf(words, &app), detail: String::new(), kind: Kind::Command, keys: key_of(cmd), act: Act::Command(cmd), key: format!("command:{cmd:?}") })
        .collect();
    let mut section = "";
    for entry in entries {
        match entry {
            Entry::Header(row) => section = row.label,
            // A key's row would only say again what its command does.
            Entry::Setting(row) if matches!(row.id, SettingId::Key(_)) => {}
            Entry::Setting(row) => items.push(Item { label: row.label.to_string(), detail: section.to_string(), kind: Kind::Setting, keys: String::new(), act: Act::Setting(row.id), key: format!("setting:{:?}", row.id) }),
        }
    }
    items.extend(themes.iter().map(|(id, name)| Item { label: name.clone(), detail: String::new(), kind: Kind::Theme, keys: String::new(), act: Act::Theme(id.clone()), key: format!("theme:{id}") }));
    items.extend(help.iter().enumerate().map(|(i, (name, title))| Item { label: title.to_string(), detail: String::new(), kind: Kind::Help, keys: String::new(), act: Act::Help(i), key: format!("help:{name}") }));
    items.extend(shells.iter().enumerate().map(|(i, (id, label))| Item { label: label.clone(), detail: String::new(), kind: Kind::NewTab, keys: String::new(), act: Act::Shell(i), key: format!("shell:{id}") }));
    let command = |label: String, act: Act| Item { label, detail: String::new(), kind: Kind::Command, keys: String::new(), act, key: String::new() };
    items.extend(tabs.groups.iter().map(|(id, name)| command(tf("palette.move_to_group", &[("group", name)]), Act::Group(*id))));
    if tabs.grouped {
        items.push(command(t("palette.ungroup_tab").to_string(), Act::Ungroup));
    }
    items.extend(tabs.tabs.iter().map(|(id, name, group)| Item { label: tf("palette.go_to_tab", &[("tab", name)]), detail: group.clone(), kind: Kind::Tab, keys: String::new(), act: Act::Tab(*id), key: String::new() }));
    items
}

/// How well `query` matches `text`: its letters in order, any case, spaces
/// in it skipped. A letter at a word's start, or straight after the one
/// matched before, scores more, a gap less, and a shorter text a little
/// more; of every way the letters can fall, the best counts. None where
/// they aren't all there.
pub fn score(query: &str, text: &str) -> Option<i32> {
    let wanted: Vec<char> = query.to_lowercase().chars().filter(|c| !c.is_whitespace()).collect();
    if wanted.is_empty() {
        return Some(0);
    }
    let letters: Vec<char> = text.to_lowercase().chars().collect();
    let starts_word = |i: usize| i == 0 || !letters[i - 1].is_alphanumeric();
    // best[i]: the best score with the letters so far matched, the last at i.
    let mut best: Vec<Option<i32>> = letters.iter().enumerate().map(|(i, &c)| (c == wanted[0]).then(|| 1 + if starts_word(i) { 8 } else { 0 } - i.min(5) as i32)).collect();
    for &w in &wanted[1..] {
        let mut next = vec![None; letters.len()];
        for i in (0..letters.len()).filter(|&i| letters[i] == w) {
            let here = 1 + if starts_word(i) { 8 } else { 0 };
            next[i] = (0..i).filter_map(|k| Some(best[k]? + here + if k + 1 == i { 5 } else { -((i - k - 1).min(5) as i32) })).max();
        }
        best = next;
    }
    // Found as typed, at a word's start, counts far more than letters
    // scattered over words that happen to start with them; found inside a
    // word, from three letters on (two, "nt", are inside too many).
    let typed = query.trim().to_lowercase();
    let lower: String = letters.iter().collect();
    let at_start = |at: usize| at == 0 || !lower[..at].chars().next_back().is_some_and(char::is_alphanumeric);
    let whole = lower.match_indices(&typed).map(|(at, _)| if at_start(at) { 60 } else if typed.chars().count() >= 3 { 40 } else { 0 }).max().unwrap_or(0);
    best.into_iter().flatten().max().map(|score| (score + whole) * 4 - (letters.len() as i32 / 8))
}

/// The palette while it's open.
pub struct Palette {
    pub query: String,
    pub items: Vec<Item>,
    /// The items that match, best first, by their place in `items`.
    pub found: Vec<usize>,
    pub selected: usize,
    /// The first row shown, so the list moves only as far as the choice
    /// takes it.
    pub top: Cell<usize>,
    /// Every setting's words, as Settings' search keeps them, so a setting
    /// is found by its hint, its choices and its key too.
    pub words: Index,
}

/// What a setting found by its words (`weight`, as the index counts them)
/// scores: below a name matched well, as every name found as typed is,
/// above letters scattered through one.
fn by_words(weight: i32) -> i32 {
    100 + weight * 8
}

impl Palette {
    pub fn new(items: Vec<Item>, words: Index, recent: &[String]) -> Self {
        let mut palette = Palette { query: String::new(), items, found: Vec::new(), selected: 0, top: Cell::new(0), words };
        palette.refilter(recent);
        palette
    }

    /// Finds the items matching what's typed again, best first, the
    /// recently chosen (`recent`, their keys, the latest first) ahead of
    /// others as good, and chooses the first.
    pub fn refilter(&mut self, recent: &[String]) {
        let rank = |item: &Item| recent.iter().position(|r| *r == item.key).unwrap_or(usize::MAX);
        let by_word = self.words.find(&self.query);
        let mut found: Vec<(i32, usize, usize)> = self
            .items
            .iter()
            .enumerate()
            .filter_map(|(i, item)| {
                let label = score(&self.query, &item.label);
                // A setting found by its section too ("glass opacity").
                let with_section = score(&self.query, &format!("{} {}", item.detail, item.label)).map(|s| s - 4);
                // Or by its words: its hint, its choices, its key.
                let words = match item.act {
                    Act::Setting(id) => by_word.iter().find(|f| f.id == id).map(|f| by_words(f.weight)),
                    _ => None,
                };
                let best = label.max(with_section).max(words)?;
                Some((best, rank(item), i))
            })
            .collect();
        found.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
        self.found = found.into_iter().map(|(_, _, i)| i).collect();
        self.selected = 0;
        self.top.set(0);
    }

    pub fn chosen(&self) -> Option<&Item> {
        self.items.get(*self.found.get(self.selected)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(palette: &Palette) -> Vec<&str> {
        palette.found.iter().map(|&i| palette.items[i].label.as_str()).collect()
    }

    #[test]
    fn letters_in_order_match_and_word_starts_count_most() {
        assert!(score("nt", "New Tab").is_some());
        assert!(score("tn", "New Tab").is_none(), "in order");
        assert!(score("NEW tab", "New Tab").is_some(), "any case, spaces skipped");
        assert!(score("nt", "New Tab") > score("nt", "Font size"), "at words' starts");
        // The t of Tab, at a word's start, counts rather than next's.
        assert!(score("nt", "Next Tab") > score("nt", "Font size"));
        assert!(score("find", "Find…") > score("find", "Find Next"), "the shorter, as well matched");
        assert!(score("fnd", "Find…") > score("fnd", "Font and fade"), "together beats far apart");
        assert!(score("tab", "Show Previous Tab") > score("tab", "Text and cursor Cursor blink"), "as typed beats scattered word starts");
        assert!(score("tab", "New Tab") > score("tab", "Stable"), "at a word's start beats inside one");
        assert_eq!(score("", "anything"), Some(0));
    }

    #[test]
    fn every_command_setting_theme_page_and_shell_is_an_item() {
        let rows = crate::settings::entries(&crate::config::Config::default(), &Default::default(), None, &[], false, true, Default::default());
        let themes = vec![("green".to_string(), "Green screen".to_string())];
        let help = [("flow", "Flow"), ("find", "Find in the scrollback")];
        let shells = vec![("zsh", "zsh".to_string()), ("bash", "bash".to_string())];
        let items = items(&rows, &themes, &help, &shells, &Tabs::default(), false);
        for (cmd, ..) in COMMANDS.iter().filter(|(c, ..)| *c != Command::CheckForUpdates) {
            assert!(items.iter().any(|i| i.act == Act::Command(*cmd)), "{cmd:?}");
        }
        assert!(!items.iter().any(|i| i.act == Act::Command(Command::CheckForUpdates)), "no updater, no check");
        for entry in &rows {
            if let Entry::Setting(row) = entry.clone() {
                if matches!(row.id, SettingId::Key(_)) {
                    assert!(!items.iter().any(|i| i.act == Act::Setting(row.id)), "{:?} isn't the palette's", row.id);
                    continue;
                }
                assert!(items.iter().any(|i| i.act == Act::Setting(row.id)), "{:?}", row.id);
            }
        }
        assert!(items.contains(&Item { label: "Green screen".into(), detail: String::new(), kind: Kind::Theme, keys: String::new(), act: Act::Theme("green".into()), key: "theme:green".into() }));
        assert_eq!(items.iter().filter(|i| i.kind == Kind::Help).count(), 2);
        assert_eq!(items.iter().filter(|i| i.kind == Kind::NewTab).map(|i| i.act.clone()).collect::<Vec<_>>(), [Act::Shell(0), Act::Shell(1)]);
        // A setting says its section.
        assert!(items.iter().filter(|i| i.kind == Kind::Setting).all(|i| !i.detail.is_empty()));
    }

    #[test]
    fn each_tab_and_group_is_an_item_that_isnt_remembered() {
        crate::i18n::set_language("en", &[]);
        let tabs = Tabs { tabs: vec![(7, "zsh 1".into(), "Web".into()), (9, "build".into(), String::new())], groups: vec![(3, "Docs".into())], grouped: true };
        let all = items(&[], &[], &[], &[], &tabs, false);
        let ours: Vec<&Item> = all.iter().filter(|i| matches!(i.act, Act::Tab(_) | Act::Group(_) | Act::Ungroup)).collect();
        let shown: Vec<(&str, &str, Kind)> = ours.iter().map(|i| (i.label.as_str(), i.detail.as_str(), i.kind)).collect();
        assert_eq!(shown, [("Move Tab to Group: Docs", "", Kind::Command), ("Remove Tab from Group", "", Kind::Command), ("Go to Tab: zsh 1", "Web", Kind::Tab), ("Go to Tab: build", "", Kind::Tab)]);
        assert_eq!(ours.iter().map(|i| i.act.clone()).collect::<Vec<_>>(), [Act::Group(3), Act::Ungroup, Act::Tab(7), Act::Tab(9)]);
        assert!(ours.iter().all(|i| i.key.is_empty()), "a tab or a group may be gone by the next run");
        // Loose, the tab in front has nothing to leave.
        let loose = items(&[], &[], &[], &[], &Tabs { grouped: false, ..tabs }, false);
        assert!(!loose.iter().any(|i| i.act == Act::Ungroup));
        assert!(loose.iter().any(|i| i.act == Act::Command(Command::NewGroup)), "New Group is a command");
    }

    #[test]
    fn every_item_is_remembered_by_a_key_of_its_own_whatever_the_language() {
        let keys = |code: &str| {
            crate::i18n::set_language(code, &[]);
            let rows = crate::settings::entries(&crate::config::Config::default(), &Default::default(), None, &[], false, true, Default::default());
            let help: Vec<(&str, &str)> = crate::help::Page::ALL.iter().map(|p| (p.name(), p.title())).collect();
            let items = items(&rows, &[("green".into(), "Green screen".into())], &help, &[("zsh", "zsh".into())], &Tabs::default(), true);
            items.into_iter().map(|i| (i.key, i.label)).collect::<Vec<_>>()
        };
        let (en, ja) = (keys("en"), keys("ja"));
        let unique: std::collections::BTreeSet<&String> = en.iter().map(|(k, _)| k).collect();
        assert_eq!(unique.len(), en.len(), "no two items share a key");
        assert_eq!(en.iter().map(|(k, _)| k).collect::<Vec<_>>(), ja.iter().map(|(k, _)| k).collect::<Vec<_>>(), "the keys don't change with the language");
        assert_ne!(en.iter().map(|(_, l)| l).collect::<Vec<_>>(), ja.iter().map(|(_, l)| l).collect::<Vec<_>>(), "though the words do");
        for key in ["command:NewTab", "setting:FontSize", "theme:green", "help:find", "shell:zsh"] {
            assert!(unique.contains(&key.to_string()), "{key}");
        }
        crate::i18n::set_language("en", &[]);
    }

    #[test]
    fn the_recently_chosen_are_kept_between_runs_the_latest_first() {
        let dir = std::env::temp_dir().join(format!("flow-palette-{}", std::process::id()));
        let path = dir.join("palette.json");
        assert_eq!(Recent::read(&path), Recent::default(), "none kept yet");
        let mut recent = Recent::default();
        for key in ["command:NewTab", "theme:green", "command:NewTab"] {
            recent.chose(key);
        }
        assert_eq!(recent.keys, ["command:NewTab", "theme:green"], "once each, the latest first");
        for i in 0..20 {
            recent.chose(&format!("setting:{i}"));
        }
        assert_eq!(recent.keys.len(), RECENT);
        recent.write(&path);
        assert_eq!(Recent::read(&path), recent);
        // A key no item has any more (a theme removed) just ranks nothing.
        let item = Item { label: "New Tab".into(), detail: String::new(), kind: Kind::Command, keys: String::new(), act: Act::Command(Command::NewTab), key: "command:NewTab".into() };
        let p = Palette::new(vec![item], Index::default(), &["theme:gone".to_string()]);
        assert_eq!(p.found, [0]);
        std::fs::write(&path, "not json").unwrap();
        assert_eq!(Recent::read(&path), Recent::default(), "a file that won't read is none");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_best_match_comes_first_and_the_recent_ahead_of_others_as_good() {
        let item = |label: &str| Item { label: label.into(), detail: String::new(), kind: Kind::Command, keys: String::new(), act: Act::Help(0), key: format!("test:{label}") };
        let mut p = Palette::new(vec![item("Font size"), item("New Tab"), item("Clear Buffer"), item("Close Tab")], Index::default(), &[]);
        assert_eq!(labels(&p).len(), 4, "everything, before anything's typed");
        p.query = "nt".into();
        p.refilter(&[]);
        assert_eq!(labels(&p)[0], "New Tab", "at words' starts, before Font size's");
        p.query = "cl".into();
        p.refilter(&[]);
        assert_eq!(labels(&p), ["Clear Buffer", "Close Tab"], "as good: in their order");
        p.refilter(&["test:Close Tab".to_string()]);
        assert_eq!(labels(&p)[0], "Close Tab", "chosen last, and as good");
        p.query = String::new();
        p.refilter(&["test:Close Tab".to_string()]);
        assert_eq!((labels(&p)[0], p.chosen().map(|i| i.label.as_str())), ("Close Tab", Some("Close Tab")));
        p.query = "zzz".into();
        p.refilter(&[]);
        assert!(p.found.is_empty() && p.chosen().is_none());
    }

    #[test]
    fn a_setting_is_found_by_its_choices_and_its_key_after_those_found_by_name() {
        crate::i18n::set_language("en", &[]);
        let mut cfg = crate::config::Config::default();
        cfg.ui.closed_sections.clear();
        cfg.fx.crt = 0.6;
        let entries = crate::settings::entries(&cfg, &crate::settings::FontChoices::default(), None, &[], false, true, crate::settings::Tally::default());
        let mut p = Palette::new(items(&entries, &[], &[], &[], &Tabs::default(), false), Index::new(&entries, &entries), &[]);
        let mut find = |q: &str| {
            p.query = q.into();
            p.refilter(&[]);
            p.found.iter().map(|&i| p.items[i].act.clone()).collect::<Vec<_>>()
        };
        // "amber" names no setting: it's one of Phosphor's choices.
        assert!(find("amber").contains(&Act::Setting(SettingId::Phosphor)));
        // By its key in config.toml.
        assert_eq!(find("fx.crt").first(), Some(&Act::Setting(SettingId::Crt)));
        // A name found as typed comes before rows whose hints say it.
        let glow = find("glow");
        assert_eq!(glow.first(), Some(&Act::Setting(SettingId::Glow)), "{glow:?}");
        assert!(glow.len() > 2, "rows whose hints say glow, after: {glow:?}");
    }

    #[test]
    fn a_setting_is_found_by_its_section_too() {
        let opacity = Item { label: "Opacity".into(), detail: "Glass".into(), kind: Kind::Setting, keys: String::new(), act: Act::Help(0), key: "test:opacity".into() };
        let mut p = Palette::new(vec![opacity], Index::default(), &[]);
        p.query = "glass opa".into();
        p.refilter(&[]);
        assert_eq!(p.found, [0]);
    }
}
