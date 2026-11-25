//! The interface's words in each language. Each language is a JSON file in
//! `core/locales`, built into Flow: nested objects whose leaves are strings,
//! looked up by their dotted path ("settings.title"). `{name}` in a string is
//! a value [`tf`] fills in. A key a language lacks falls back to English, and
//! one English lacks shows as the key itself (the tests make sure neither
//! happens).

use std::collections::HashMap;
use std::fmt::Display;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

/// The languages Flow speaks: code and file. English comes first and is what
/// the others fall back to.
const FILES: [(&str, &str); 2] = [("en", include_str!("../locales/en.json")), ("ja", include_str!("../locales/ja.json"))];

/// What config.toml's `ui.language` holds to follow the system's languages.
pub const AUTO: &str = "auto";

static CURRENT: AtomicUsize = AtomicUsize::new(0);

// Tests run side by side on their own threads, so each keeps its own
// language rather than switching everyone's.
#[cfg(test)]
thread_local! {
    static TEST_LANGUAGE: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}

// While a thread builds words in English alone (`in_english`), its `t` is
// English whatever the language in use.
thread_local! {
    static ENGLISH_HERE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

type Table = HashMap<String, String>;

fn tables() -> &'static [Table] {
    static TABLES: OnceLock<Vec<Table>> = OnceLock::new();
    TABLES.get_or_init(|| FILES.iter().map(|(code, json)| parse(code, json)).collect())
}

fn parse(code: &str, json: &str) -> Table {
    fn flatten(prefix: &str, value: &serde_json::Value, out: &mut Table) {
        match value {
            serde_json::Value::Object(map) => {
                for (k, v) in map {
                    let key = if prefix.is_empty() { k.clone() } else { format!("{prefix}.{k}") };
                    flatten(&key, v, out);
                }
            }
            serde_json::Value::String(s) => {
                out.insert(prefix.to_string(), s.clone());
            }
            other => log::warn!("{prefix} in a locale file is {other}, not a string"),
        }
    }
    let mut table = Table::new();
    match serde_json::from_str::<serde_json::Value>(json) {
        Ok(value) => flatten("", &value, &mut table),
        Err(err) => log::warn!("locales/{code}.json doesn't parse: {err}"),
    }
    table
}

fn current() -> usize {
    if ENGLISH_HERE.get() {
        return 0;
    }
    #[cfg(test)]
    if let Some(i) = TEST_LANGUAGE.get() {
        return i;
    }
    CURRENT.load(Ordering::Relaxed)
}

/// The language codes Flow has, English first.
pub fn codes() -> impl Iterator<Item = &'static str> {
    FILES.iter().map(|(code, _)| *code)
}

/// A language's own name for itself ("日本語"), for the Language setting.
pub fn native_name(code: &str) -> &'static str {
    FILES.iter().position(|(c, _)| *c == code).and_then(|i| tables()[i].get("language.name")).map_or("?", String::as_str)
}

/// The language in use.
pub fn language() -> &'static str {
    FILES[current()].0
}

/// Picks the language: `setting` is a code, or [`AUTO`] for the first of
/// `preferred` (the system's languages, most wanted first, such as "ja-JP")
/// that Flow has, else English.
pub fn set_language(setting: &str, preferred: &[String]) {
    let index_of = |code: &str| {
        let base = code.split(['-', '_']).next().unwrap_or("").to_ascii_lowercase();
        FILES.iter().position(|(c, _)| *c == base)
    };
    let i = if setting == AUTO { preferred.iter().find_map(|p| index_of(p)) } else { index_of(setting) }.unwrap_or(0);
    #[cfg(test)]
    TEST_LANGUAGE.set(Some(i));
    #[cfg(not(test))]
    CURRENT.store(i, Ordering::Relaxed);
}

/// What `make` makes with the words in English, on this thread alone,
/// whatever the language in use: Settings' search keeps every row's words
/// in English too, so an English word finds them in any language.
pub fn in_english<T>(make: impl FnOnce() -> T) -> T {
    let was = ENGLISH_HERE.replace(true);
    let made = make();
    ENGLISH_HERE.set(was);
    made
}

/// The words for `key` in the language in use.
pub fn t(key: &str) -> &'static str {
    let tables = tables();
    match tables[current()].get(key).or_else(|| tables[0].get(key)) {
        Some(s) => s,
        None => missing(key),
    }
}

/// The words for `key` in the language `code`, where it has its own: for
/// what's written for every language at once (a theme package's names).
pub fn in_language(code: &str, key: &str) -> Option<&'static str> {
    FILES.iter().position(|(c, _)| *c == code).and_then(|i| tables()[i].get(key)).map(String::as_str)
}

/// The English words for `key`, whatever the language in use: for what
/// goes to the assistant's model rather than the user.
pub fn english(key: &str) -> &'static str {
    match tables()[0].get(key) {
        Some(s) => s,
        None => missing(key),
    }
}

/// [`t`] with each `{name}` filled in from `args`.
pub fn tf(key: &str, args: &[(&str, &dyn Display)]) -> String {
    let mut s = t(key).to_string();
    for (name, value) in args {
        s = s.replace(&format!("{{{name}}}"), &value.to_string());
    }
    s
}

/// A key no language has: shown as itself, kept once so it can be returned
/// for good, and noted for the tests.
fn missing(key: &str) -> &'static str {
    #[cfg(test)]
    MISSED.with(|m| m.borrow_mut().push(key.to_string()));
    static MISSING: OnceLock<parking_lot::Mutex<HashMap<String, &'static str>>> = OnceLock::new();
    let mut seen = MISSING.get_or_init(Default::default).lock();
    if let Some(s) = seen.get(key) {
        return s;
    }
    log::warn!("no words for {key}");
    let s: &'static str = Box::leak(key.to_string().into_boxed_str());
    seen.insert(key.to_string(), s);
    s
}

#[cfg(test)]
thread_local! {
    static MISSED: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// The keys this thread looked up that no language has, for tests.
#[cfg(test)]
pub fn missed() -> Vec<String> {
    MISSED.with(|m| m.borrow().clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn placeholders(s: &str) -> BTreeSet<&str> {
        s.split('{').skip(1).filter_map(|rest| rest.split_once('}').map(|(name, _)| name)).collect()
    }

    #[test]
    fn every_language_has_every_english_key_with_the_same_placeholders() {
        let tables = tables();
        let english = &tables[0];
        assert!(english.len() > 100, "{}", english.len());
        for (i, (code, _)) in FILES.iter().enumerate().skip(1) {
            let table = &tables[i];
            let lacking: Vec<_> = english.keys().filter(|k| !table.contains_key(*k)).collect();
            assert!(lacking.is_empty(), "{code} lacks {lacking:?}");
            let extra: Vec<_> = table.keys().filter(|k| !english.contains_key(*k)).collect();
            assert!(extra.is_empty(), "{code} has keys English doesn't: {extra:?}");
            for (key, words) in table {
                assert_eq!(placeholders(words), placeholders(&english[key]), "{code} {key}");
            }
        }
    }

    /// Every "section.key" string in the sources, for a section the language
    /// files have, is a key English has. Keys put together at run time
    /// (`settings.options.style.rain`) are checked by building the UI.
    #[test]
    fn the_keys_in_the_sources_are_in_english() {
        let sections: BTreeSet<&str> = tables()[0].keys().filter_map(|k| k.split('.').next()).collect();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut files = Vec::new();
        let mut dirs = vec![root.join("src"), root.join("../macos/src"), root.join("../windows/src")];
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    dirs.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    files.push(path);
                }
            }
        }
        let mut checked = 0;
        for file in &files {
            let source = std::fs::read_to_string(file).unwrap();
            for literal in source.split('"').skip(1).step_by(2) {
                let mut parts = literal.split('.');
                let section = parts.next().unwrap_or("");
                // One ending in a dot is a key's start (`"settings."`), never a key.
                let keyish = literal.contains('.') && !literal.ends_with('.') && literal.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '.');
                // File names, like settings.png, aren't keys.
                if matches!(literal.rsplit('.').next(), Some("png" | "webp" | "json" | "toml" | "log" | "txt" | "xml" | "rs" | "sqlite3")) {
                    continue;
                }
                if keyish && sections.contains(section) {
                    assert!(tables()[0].contains_key(literal), "{} uses {literal}, which en.json lacks", file.display());
                    checked += 1;
                }
            }
        }
        assert!(checked > 100, "only {checked} keys found in the sources");
    }

    #[test]
    fn automatic_follows_the_first_system_language_flow_has() {
        let langs = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        set_language(AUTO, &langs(&["ja-JP", "en-US"]));
        assert_eq!(language(), "ja");
        set_language(AUTO, &langs(&["fr-FR", "ja_JP.UTF-8"]));
        assert_eq!(language(), "ja");
        set_language(AUTO, &langs(&["fr-FR"]));
        assert_eq!(language(), "en");
        set_language("en", &langs(&["ja-JP"]));
        assert_eq!(language(), "en");
        set_language("ja", &[]);
        assert_eq!((language(), native_name("ja"), native_name("en")), ("ja", "日本語", "English"));
    }

    #[test]
    fn words_come_in_the_language_in_use_with_their_values_filled_in() {
        set_language("ja", &[]);
        assert_eq!(t("common.cancel"), "キャンセル");
        assert_eq!(tf("toast.exited", &[("shell", &"zsh")]), "zshが終了しました");
        assert_eq!(english("common.cancel"), "Cancel");
        set_language("en", &[]);
        assert_eq!(tf("panel.tokens_of_cap", &[("used", &"1.2k"), ("cap", &"100k")]), "· 1.2k / 100k tokens");
        assert!(missed().is_empty());
        // An unknown key shows as itself, and is noted.
        assert_eq!(t("nowhere.at_all"), "nowhere.at_all");
        assert_eq!(missed(), ["nowhere.at_all"]);
    }
}
