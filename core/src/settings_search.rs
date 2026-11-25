//! Searching Settings: a field at the top of the card that finds a setting
//! by any words about it, not only its name. Each row is kept with its
//! words ([`Index`]): its label, its section's header, its hint, its
//! options' names (so "amber" finds Phosphor) and its key in config.toml
//! (`fx.crt`), in the language in use and in English, so an English word
//! finds a row in Japanese too.
//!
//! What's typed matches as the command history's search does: every word,
//! in any order, each the start of a word in one of those; Japanese, which
//! leaves no spaces between words, by any run of its letters. A row found
//! by its label comes before one found by its hint, and the palette's
//! [`score`](crate::command_palette::score) of the label breaks ties. There
//! are a couple of hundred rows, so it's all kept in memory, made afresh
//! as the field opens.

use std::ops::Range;

use crate::settings::{Control, Entry, Row, Section, SettingId};

/// How much a word found in each of a row's words counts.
pub const LABEL: i32 = 8;
const KEY: i32 = 5;
const OPTIONS: i32 = 3;
const SECTION: i32 = 2;
pub const HINT: i32 = 1;

/// A row and its words, lowercase, each with what a word found there
/// counts.
#[derive(Debug, Clone)]
struct Indexed {
    id: SettingId,
    /// Its section's header, in the language in use, said beside it.
    section: &'static str,
    /// Its label as shown, and lowercase, to light what matched in it.
    label: String,
    lower: String,
    words: Vec<(i32, String)>,
}

/// Every row's words, for [`Index::find`].
#[derive(Debug, Clone, Default)]
pub struct Index {
    rows: Vec<Indexed>,
}

/// A row found: which, its section's name, where in its label the words
/// typed are (byte ranges), to light them, and how much the words they
/// were found in count, each word's best, added up (a label's [`LABEL`]
/// each, a hint's [`HINT`]), which the command palette ranks it by.
#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    pub id: SettingId,
    pub section: &'static str,
    pub lit: Vec<Range<usize>>,
    pub weight: i32,
}

/// A row's options as words: a choice's and a color row's names.
fn options(row: &Row) -> String {
    match &row.control {
        Control::Choice { options, .. } => options.join(" "),
        Control::Colors { options, .. } => options.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>().join(" "),
        Control::Actions(labels) => labels.join(" "),
        Control::Item { text, .. } => text.clone(),
        _ => String::new(),
    }
}

/// The settings in `entries`, each with its section, every section open.
fn settings_of(entries: &[Entry]) -> Vec<(&Row, Option<Section>)> {
    let mut section = None;
    let mut rows = Vec::new();
    for e in entries {
        match e {
            Entry::Header(Row { id: SettingId::Section(s), .. }) => section = Some(*s),
            Entry::Header(_) => {}
            Entry::Setting(row) => rows.push((row, section)),
        }
    }
    rows
}

impl Index {
    /// The index of `here`, Settings' rows in the language in use with every
    /// section open, and `english`, the same rows in English.
    pub fn new(here: &[Entry], english: &[Entry]) -> Index {
        let english = settings_of(english);
        let rows = settings_of(here)
            .into_iter()
            .enumerate()
            .map(|(i, (row, section))| {
                let title = section.map_or("", Section::title);
                let mut words = vec![(LABEL, row.label.to_lowercase()), (SECTION, title.to_lowercase()), (HINT, row.hint.to_lowercase()), (OPTIONS, options(row).to_lowercase())];
                if let Some(key) = crate::settings::config_key(row.id) {
                    words.push((KEY, key));
                }
                // The same row in English: at its place, which the rows
                // built in both languages share, else found by its id.
                let en = english.get(i).filter(|(r, _)| r.id == row.id).or_else(|| english.iter().find(|(r, _)| r.id == row.id));
                if let Some((en, en_section)) = en {
                    words.push((LABEL, en.label.to_lowercase()));
                    words.push((HINT, en.hint.to_lowercase()));
                    words.push((OPTIONS, options(en).to_lowercase()));
                    if let Some(s) = en_section {
                        words.push((SECTION, crate::i18n::english(&format!("settings.header.{}", s.id())).to_lowercase()));
                    }
                }
                words.retain(|(_, w)| !w.is_empty());
                Indexed { id: row.id, section: title, label: row.label.to_string(), lower: row.label.to_lowercase(), words }
            })
            .collect();
        Index { rows }
    }

    /// The rows `query` finds, the best first: none for nothing typed.
    pub fn find(&self, query: &str) -> Vec<Found> {
        let wanted: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        if wanted.is_empty() {
            return Vec::new();
        }
        let mut found: Vec<(i32, i32, usize, Found)> = Vec::new();
        for (place, row) in self.rows.iter().enumerate() {
            let mut total = 0;
            let mut every = true;
            for w in &wanted {
                let best = row.words.iter().filter(|(_, text)| has(text, w)).map(|(weight, _)| *weight).max();
                match best {
                    Some(weight) => total += weight,
                    None => {
                        every = false;
                        break;
                    }
                }
            }
            if !every {
                continue;
            }
            let tie = crate::command_palette::score(query, &row.label).unwrap_or(i32::MIN / 2);
            let lit = if row.lower.len() == row.label.len() { wanted.iter().flat_map(|w| places(&row.lower, w)).collect() } else { Vec::new() };
            found.push((total, tie, place, Found { id: row.id, section: row.section, lit: merged(lit), weight: total }));
        }
        found.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)).then(a.2.cmp(&b.2)));
        found.into_iter().map(|(.., f)| f).collect()
    }
}

/// Whether `word` is in `text`: at the start of a word in it, or, with
/// letters that leave no spaces between words (Japanese), anywhere.
fn has(text: &str, word: &str) -> bool {
    !places(text, word).is_empty()
}

/// Where `word` is in `text`, by the rule of [`has`].
fn places(text: &str, word: &str) -> Vec<Range<usize>> {
    let anywhere = !word.is_ascii();
    text.match_indices(word)
        .filter(|(at, _)| anywhere || text[..*at].chars().next_back().is_none_or(|c| !c.is_alphanumeric()))
        .map(|(at, w)| at..at + w.len())
        .collect()
}

/// `ranges` in order, those that touch joined.
fn merged(mut ranges: Vec<Range<usize>>) -> Vec<Range<usize>> {
    ranges.sort_by_key(|r| r.start);
    let mut out: Vec<Range<usize>> = Vec::new();
    for r in ranges {
        match out.last_mut() {
            Some(last) if r.start <= last.end => last.end = last.end.max(r.end),
            _ => out.push(r),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::settings::{entries, FontChoices, Tally};

    /// Settings' rows in `code`, every section open and the tube on, which
    /// shows its parts' rows.
    fn rows_in(code: &str) -> Vec<Entry> {
        crate::i18n::set_language(code, &[]);
        let mut cfg = Config::default();
        cfg.ui.closed_sections.clear();
        cfg.fx.crt = 0.6;
        entries(&cfg, &FontChoices::default(), None, &[], false, true, Tally::default())
    }

    fn index_in(code: &str) -> Index {
        let english = rows_in("en");
        let here = rows_in(code);
        Index::new(&here, &english)
    }

    fn ids(found: &[Found]) -> Vec<SettingId> {
        found.iter().map(|f| f.id).collect()
    }

    #[test]
    fn a_setting_is_found_by_its_name_first_then_by_what_its_hint_says() {
        let index = index_in("en");
        let found = index.find("glow");
        assert_eq!(found.first().map(|f| f.id), Some(SettingId::Glow), "{:?}", ids(&found));
        // The tube's glow too, by its label; others by their hints after.
        assert!(ids(&found).contains(&SettingId::PhosphorGlow));
        // Words in any order, each the start of a word.
        assert_eq!(index.find("size font").first().map(|f| f.id), Some(SettingId::FontSize));
        assert!(index.find("ont").is_empty() || !ids(&index.find("ont")).contains(&SettingId::FontSize), "not inside a word");
        // Every word has to be there.
        assert!(index.find("glow nothinglikethis").is_empty());
        assert!(index.find("  ").is_empty());
    }

    #[test]
    fn a_setting_is_found_by_its_options_and_its_key_in_the_config() {
        let index = index_in("en");
        assert!(ids(&index.find("amber")).contains(&SettingId::Phosphor), "{:?}", ids(&index.find("amber")));
        assert_eq!(index.find("fx.crt").first().map(|f| f.id), Some(SettingId::Crt));
        assert_eq!(index.find("shell.scrollbar").first().map(|f| f.id), Some(SettingId::Scrollbar));
        // Closed sections are searched too, and each found row says its section.
        let found = index.find("scanlines");
        assert_eq!(found.first().map(|f| (f.id, f.section)), Some((SettingId::Scanlines, crate::i18n::t("settings.header.crt"))));
    }

    #[test]
    fn japanese_is_found_by_any_run_of_its_letters_and_english_words_find_it_too() {
        let index = index_in("ja");
        let font = crate::i18n::t("settings.font_size.label");
        // Two letters from the middle of the label.
        let run: String = font.chars().skip(1).take(2).collect();
        assert!(ids(&index.find(&run)).contains(&SettingId::FontSize), "{run} in {font}");
        // An English word finds the row whose words are Japanese.
        assert!(ids(&index.find("scrollbar")).contains(&SettingId::Scrollbar));
        crate::i18n::set_language("en", &[]);
    }

    #[test]
    fn the_words_typed_are_lit_where_they_are_in_the_label() {
        let index = index_in("en");
        let found = index.find("font size");
        let row = found.iter().find(|f| f.id == SettingId::FontSize).unwrap();
        assert_eq!(row.lit, [0..4, 5..9], "Font size");
    }
}
