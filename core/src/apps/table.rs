//! What `flowapp` prints, laid out. In a terminal its lists are tables:
//! ruled with box drawing (which Flow draws itself, so the lines meet in any
//! font), a bold header, colors that say something (a newer version green,
//! the games that come with Flow dimmed), each app's name an OSC 8 link to
//! its release page, columns lined up by what shows (`unicode-width`, so
//! Japanese keeps its columns) and long text cut with "…" to the terminal's
//! width. Piped, or with `NO_COLOR`, the same columns are plain text lined
//! up with spaces, no escapes, so scripts can read them, and piped nothing
//! is cut. A download in a terminal is one line redrawn as it comes ([`fetching`]).
//! Nothing here asks the terminal anything, so tests lay out what `flowapp`
//! prints without one. With `--json` the same apps are JSON ([`json`]).

use std::time::Duration;

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::first::{Ahead, How, Row};
use super::signing::{self, CodeGroup, CodeSigned};
use super::{App, Download, Failed, Kept, Standing, STALE};
use crate::i18n::{t, tf};

/// How what's printed is laid out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Look {
    /// In a terminal that takes escapes: rules, styles and links. Else plain.
    pub styled: bool,
    /// The terminal's columns, which long text is cut to. None when piped,
    /// where nothing is cut.
    pub width: Option<usize>,
}

/// A piece of text's style, as SGR in a styled look.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ink {
    Plain,
    Bold,
    Dim,
    /// A version newer than the one installed, or what went well.
    Newer,
    /// What went wrong.
    Bad,
}

impl Ink {
    fn sgr(self) -> &'static str {
        match self {
            Ink::Plain => "",
            Ink::Bold => "1",
            Ink::Dim => "2",
            Ink::Newer => "32",
            Ink::Bad => "31",
        }
    }
}

/// Text in pieces of their own styles.
type Spans = Vec<(String, Ink)>;

/// One cell: its text, and the page its words link to. Empty, it shows a
/// dash, so a script reading columns by the spaces between them keeps
/// count.
#[derive(Debug, Clone, PartialEq)]
struct Cell {
    spans: Spans,
    link: Option<String>,
}

impl Cell {
    fn new(text: impl Into<String>, ink: Ink) -> Self {
        Cell { spans: vec![(text.into(), ink)], link: None }
    }

    fn none() -> Self {
        Cell { spans: Vec::new(), link: None }
    }

    fn linked(self, link: Option<String>) -> Self {
        Cell { link, ..self }
    }

    fn and(mut self, text: impl Into<String>, ink: Ink) -> Self {
        self.spans.push((text.into(), ink));
        self
    }

    /// What it shows: an empty cell's dash, a long dash in a terminal.
    fn shown(&self, styled: bool) -> Spans {
        match self.spans.is_empty() {
            true => vec![((if styled { "—" } else { "-" }).to_string(), Ink::Dim)],
            false => self.spans.clone(),
        }
    }
}

/// A column: its header, whether its text may be cut to fit the terminal
/// (and whether only once the others that may are as short as they go),
/// and whether it's set to the right (sizes).
struct Column {
    head: String,
    cut: bool,
    cut_last: bool,
    right: bool,
}

impl Column {
    fn new(head: &str) -> Self {
        Column { head: head.to_string(), cut: false, cut_last: false, right: false }
    }

    fn cut(self) -> Self {
        Column { cut: true, ..self }
    }

    /// Cut only once the columns cut first are as short as they go.
    fn cut_last(self) -> Self {
        Column { cut: true, cut_last: true, ..self }
    }

    fn right(self) -> Self {
        Column { right: true, ..self }
    }
}

/// A column that may be cut keeps this many columns at least.
const LEAST_CUT: usize = 6;

/// Rows of cells under their columns, with a header row unless it's a list
/// of names and values (`info`'s).
struct Table {
    columns: Vec<Column>,
    rows: Vec<Vec<Cell>>,
    header: bool,
}

impl Table {
    fn new(columns: Vec<Column>) -> Self {
        Table { columns, rows: Vec::new(), header: true }
    }

    /// The table, a line each row (and the header, and the rules when
    /// styled), every line ended.
    fn render(&self, look: Look) -> String {
        let styled = look.styled;
        let n = self.columns.len();
        let mut widths: Vec<usize> = self.columns.iter().map(|c| if self.header { c.head.width() } else { 0 }).collect();
        for row in &self.rows {
            for (i, cell) in row.iter().enumerate().take(n) {
                widths[i] = widths[i].max(width(&cell.shown(styled)));
            }
        }
        // Cut what may be cut, the widest first, to the terminal's width.
        if let Some(most) = look.width {
            let room = if styled { 3 * n + 1 } else { 2 * n.saturating_sub(1) };
            while widths.iter().sum::<usize>() + room > most {
                let may = |last: bool| (0..n).filter(|&i| self.columns[i].cut && self.columns[i].cut_last == last && widths[i] > LEAST_CUT).max_by_key(|&i| (widths[i], i));
                let widest = may(false).or_else(|| may(true));
                match widest {
                    Some(i) => widths[i] -= 1,
                    None => break,
                }
            }
        }
        let mut out = String::new();
        let rule = |out: &mut String, (left, middle, right): (&str, &str, &str)| {
            let line: Vec<String> = widths.iter().map(|w| "─".repeat(w + 2)).collect();
            out.push_str(&paint(&[(format!("{left}{}{right}", line.join(middle)), Ink::Dim)], true));
            out.push('\n');
        };
        if styled {
            rule(&mut out, ("╭", "┬", "╮"));
        }
        if self.header {
            let heads: Vec<Cell> = self.columns.iter().map(|c| Cell::new(c.head.clone(), Ink::Bold)).collect();
            self.line(&heads, &widths, look, &mut out);
            if styled {
                rule(&mut out, ("├", "┼", "┤"));
            }
        }
        for row in &self.rows {
            self.line(row, &widths, look, &mut out);
        }
        if styled {
            rule(&mut out, ("╰", "┴", "╯"));
        }
        out
    }

    /// One row of `cells`, each cut and padded to its column's width.
    fn line(&self, cells: &[Cell], widths: &[usize], look: Look, out: &mut String) {
        let styled = look.styled;
        let bar = paint(&[("│".to_string(), Ink::Dim)], styled);
        let mut line = String::new();
        if styled {
            line.push_str(&bar);
            line.push(' ');
        }
        for (i, column) in self.columns.iter().enumerate() {
            let none = Cell::none();
            let cell = cells.get(i).unwrap_or(&none);
            let spans = cut(&cell.shown(styled), widths[i]);
            let pad = " ".repeat(widths[i] - width(&spans));
            let text = link(&paint(&spans, styled), cell.link.as_deref(), styled);
            let last = i + 1 == self.columns.len();
            match (column.right, last && !styled) {
                (true, _) => line.push_str(&format!("{pad}{text}")),
                (false, true) => line.push_str(&text),
                (false, false) => line.push_str(&format!("{text}{pad}")),
            }
            if styled {
                line.push(' ');
                line.push_str(&bar);
                if !last {
                    line.push(' ');
                }
            } else if !last {
                line.push_str("  ");
            }
        }
        out.push_str(if styled { &line } else { line.trim_end() });
        out.push('\n');
    }
}

/// How many columns `spans` take.
fn width(spans: &[(String, Ink)]) -> usize {
    spans.iter().map(|(text, _)| text.width()).sum()
}

/// `spans` cut to `most` columns, ending in "…" where something was cut.
fn cut(spans: &[(String, Ink)], most: usize) -> Spans {
    if width(spans) <= most {
        return spans.to_vec();
    }
    let room = most.saturating_sub(1);
    let mut used = 0;
    let mut out: Spans = Vec::new();
    'spans: for (text, ink) in spans {
        let mut kept = String::new();
        for c in text.chars() {
            let w = c.width().unwrap_or(0);
            if used + w > room {
                out.push((kept, *ink));
                break 'spans;
            }
            used += w;
            kept.push(c);
        }
        out.push((kept, *ink));
    }
    if most > 0 {
        let ink = out.last().map_or(Ink::Plain, |(_, ink)| *ink);
        out.push(("…".to_string(), ink));
    }
    out.retain(|(text, _)| !text.is_empty());
    out
}

/// `spans` with their styles as SGR when `styled`.
/// `text` in `ink` where the look is styled, for a line flowapp prints
/// outside a table.
pub fn painted(text: &str, ink: Ink, styled: bool) -> String {
    paint(&[(text.to_string(), ink)], styled)
}

fn paint(spans: &[(String, Ink)], styled: bool) -> String {
    let mut out = String::new();
    for (text, ink) in spans {
        match ink.sgr() {
            sgr if styled && !sgr.is_empty() => out.push_str(&format!("\x1b[{sgr}m{text}\x1b[0m")),
            _ => out.push_str(text),
        }
    }
    out
}

/// `text` as an OSC 8 link to `to` when `styled`.
fn link(text: &str, to: Option<&str>, styled: bool) -> String {
    match to {
        Some(to) if styled => format!("\x1b]8;;{to}\x1b\\{text}\x1b]8;;\x1b\\"),
        _ => text.to_string(),
    }
}

/// Bytes as a download's size: "512 KB", "2.7 MB", "48 MB".
pub fn size(n: u64) -> String {
    const K: f64 = 1024.0;
    let n = n as f64;
    if n < K * K {
        format!("{:.0} KB", (n / K).ceil())
    } else if n < 10.0 * K * K {
        format!("{:.1} MB", n / K / K)
    } else if n < K * K * K {
        format!("{:.0} MB", n / K / K)
    } else {
        format!("{:.1} GB", n / K / K / K)
    }
}

/// An app as the tables show it.
#[derive(Debug, Clone, PartialEq)]
pub struct Shown {
    pub name: String,
    pub standing: Standing,
    /// The version of its download for this system, where there's one.
    pub latest: Option<String>,
    /// The size of that download.
    pub size: Option<u64>,
    pub summary: String,
    pub programs: Vec<String>,
    /// The page of the release that download is on.
    pub page: Option<String>,
    /// It's a package of themes rather than an app.
    pub themes: bool,
    /// It's a package of skills for the assistant rather than an app.
    pub skills: bool,
    /// The oldest Flow that download works in, where the catalog names one.
    pub needs_flow: Option<String>,
    /// That's newer than this Flow, so the download waits for Flow to be
    /// updated: never installed, nor shown as one to upgrade to.
    pub held: bool,
    /// Who signed that download, as `search` shows it: the key flowapp
    /// checks it with, and the catalog's date.
    pub offered: Option<Signature>,
    /// Who signed what's installed, as `list` shows it ([`Shown::noting`]).
    pub signing: Option<Signing>,
}

/// Who signed a download, as the tables show it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Signature {
    /// Whose key it is, where it's one Flow knows ([`signing::signer_of`]).
    pub by: Option<String>,
    /// The key's fingerprint, in hex.
    pub key: String,
    /// When it was signed, as the catalog says, a UTC date; empty where it
    /// doesn't say.
    pub on: String,
    /// When flowapp checked it, installing it; empty where nothing noted
    /// that.
    pub checked: String,
    /// The signature (base64) and where the download is, for checking it
    /// yourself.
    pub signature: String,
    pub url: String,
    /// It's what the catalog says of the version installed, nothing having
    /// noted the download at the install: dim in `list`.
    pub unnoted: bool,
}

impl Signature {
    /// `download`'s, checked (or to be checked) with the key whose
    /// fingerprint is `key`.
    pub fn of(download: &Download, key: &str) -> Self {
        Signature { by: signing::signer_of(key).map(String::from), key: key.to_string(), on: download.signed.clone(), signature: download.signature.clone(), url: download.url.clone(), ..Signature::default() }
    }
}

/// Who signed what's installed.
#[derive(Debug, Clone, PartialEq)]
pub enum Signing {
    /// Its download, as the install noted it, or the catalog has it.
    Signed(Signature),
    /// A file of the user's own, which no one signed.
    OwnFile,
    /// A game an older Flow carried, which came with Flow.
    WithFlow,
}

/// The fingerprint of the key flowapp checks the catalog's downloads with.
fn flows_key() -> String {
    signing::fingerprint(crate::UPDATE_PUBLIC_KEY).unwrap_or_default()
}

impl Shown {
    /// `name` as `app` (what the catalog says, if it knows it) has it, where
    /// it stands here.
    pub fn new(name: &str, app: Option<&App>, standing: Standing) -> Self {
        let here = app.and_then(App::here);
        let with_flow = matches!(standing, Standing::Bundled(_));
        Shown {
            name: name.to_string(),
            standing,
            latest: here.map(|d| d.version.clone()),
            size: here.map(|d| d.size),
            summary: app.map(|a| a.summary.clone()).unwrap_or_default(),
            programs: app.map(|a| a.programs.clone()).unwrap_or_default(),
            page: here.and_then(|d| d.release_page()),
            themes: false,
            skills: false,
            needs_flow: here.map(|d| d.flow.trim().to_string()).filter(|f| !f.is_empty()),
            held: here.is_some_and(|d| d.needs_newer_flow(&super::itself::flow_version()).is_some()),
            offered: here.map(|d| Signature::of(d, &flows_key())),
            signing: with_flow.then_some(Signing::WithFlow),
        }
    }

    /// Notes who signed what's installed as `kept`: what the install
    /// noted, else the catalog's (in `app`) download of the version
    /// installed, else nothing; or that it's a file of the user's own.
    pub fn noting(mut self, kept: Option<&Kept>, app: Option<&App>) -> Self {
        let Some(kept) = kept else { return self };
        self.signing = if kept.own_file {
            Some(Signing::OwnFile)
        } else if let Some(download) = &kept.download {
            let key = if kept.key.is_empty() { flows_key() } else { kept.key.clone() };
            Some(Signing::Signed(Signature { checked: kept.checked.clone(), ..Signature::of(download, &key) }))
        } else {
            app.and_then(App::here).filter(|d| d.version == kept.version).map(|d| Signing::Signed(Signature { unnoted: true, ..Signature::of(d, &flows_key()) }))
        };
        self
    }

    /// The download whose signature `info` and `--json` give: what's
    /// installed where that's known, else the catalog's for this system.
    pub fn signature(&self) -> Option<&Signature> {
        match &self.signing {
            Some(Signing::Signed(signature)) => Some(signature),
            _ => self.offered.as_ref(),
        }
    }

    /// Who signed the catalog's download, for `search`.
    fn offered_cell(&self) -> Cell {
        self.offered.as_ref().map_or_else(Cell::none, |s| signature_cell(s, Ink::Plain))
    }

    /// Who signed what's installed, for `list`: dim where it's only the
    /// catalog's word for the version installed, or for a game that came
    /// with Flow.
    fn signing_cell(&self) -> Cell {
        match &self.signing {
            None => Cell::none(),
            Some(Signing::Signed(s)) => signature_cell(s, if s.unnoted { Ink::Dim } else { Ink::Plain }),
            Some(Signing::OwnFile) => Cell::new(t("flowapp.not_signed"), Ink::Dim),
            Some(Signing::WithFlow) => Cell::new(t("flowapp.from_flow"), Ink::Dim),
        }
    }

    /// Whether this system's download is newer than what's installed:
    /// never where that's a package of the user's own of the same name,
    /// which isn't the catalog's to upgrade ([`super::in_the_way`]).
    pub fn newer(&self) -> bool {
        if self.signing == Some(Signing::OwnFile) {
            return false;
        }
        match (&self.latest, self.standing.version()) {
            (Some(latest), Some(have)) => super::newer(latest, have),
            _ => false,
        }
    }

    /// Its name, linked to its release page.
    fn name_cell(&self) -> Cell {
        Cell::new(self.name.clone(), Ink::Plain).linked(self.page.clone())
    }

    /// The version installed: dim for a game that comes with Flow.
    fn installed_cell(&self) -> Cell {
        match &self.standing {
            Standing::Installed(v) => Cell::new(v.clone(), Ink::Plain),
            Standing::Bundled(v) => Cell::new(v.clone(), Ink::Dim),
            Standing::Absent => Cell::none(),
        }
    }

    /// The version there is for this system: green where it's newer than
    /// what's installed (but plain where it waits for a newer Flow), dim
    /// where it's what's installed.
    fn latest_cell(&self) -> Cell {
        let Some(latest) = &self.latest else { return Cell::none() };
        let ink = match self.standing {
            _ if self.newer() && self.held => Ink::Plain,
            _ if self.newer() => Ink::Newer,
            Standing::Absent => Ink::Plain,
            _ => Ink::Dim,
        };
        Cell::new(latest.clone(), ink)
    }
}

/// Whose key signed it, the key's first digits, and when: `Flow f186 6b49
/// · 2026-11-18`.
fn signed_words(signature: &Signature) -> String {
    let who = signature.by.clone().unwrap_or_else(|| t("flowapp.signer_unknown").to_string());
    let key = signing::short(&signature.key);
    match signature.on.is_empty() {
        true => tf("flowapp.signed_undated", &[("who", &who), ("key", &key)]),
        false => tf("flowapp.signed_cell", &[("who", &who), ("key", &key), ("date", &signature.on)]),
    }
}

fn signature_cell(signature: &Signature, ink: Ink) -> Cell {
    Cell::new(signed_words(signature), ink)
}

/// An app as `--json` gives it, its keys English whatever the language.
#[derive(serde::Serialize)]
struct Entry<'a> {
    name: &'a str,
    /// The version installed, if any.
    installed: Option<&'a str>,
    /// Who keeps it: `flowapp`, or `flow` for a game that comes with Flow.
    from: Option<&'a str>,
    /// The version of this system's download, if there's one.
    latest: Option<&'a str>,
    /// Whether that's newer than the one installed.
    newer: bool,
    /// The download's size in bytes.
    size: Option<u64>,
    summary: &'a str,
    programs: &'a [String],
    /// The page of the release the download is on.
    page: Option<&'a str>,
    /// `app`, `themes` for a package of themes, or `skills` for one of
    /// skills.
    kind: &'a str,
    /// The oldest Flow the download works in, if the catalog names one.
    needs_flow: Option<&'a str>,
    /// Whether that's newer than this Flow, so it waits for an update.
    held: bool,
    /// Whose key signed the download installed, or else this system's in
    /// the catalog (`Flow`), the key's fingerprint, the catalog's date,
    /// when flowapp checked it as it installed it, and the signature
    /// (base64); each null where there's none.
    signed_by: Option<&'a str>,
    key: Option<&'a str>,
    signed: Option<&'a str>,
    checked: Option<&'a str>,
    signature: Option<&'a str>,
    /// Installed from a file of the user's own, signed by no one.
    own_file: bool,
}

impl<'a> Entry<'a> {
    fn of(app: &'a Shown) -> Self {
        let signature = app.signature();
        let from = match app.standing {
            Standing::Installed(_) => Some("flowapp"),
            Standing::Bundled(_) => Some("flow"),
            Standing::Absent => None,
        };
        Entry {
            name: &app.name,
            installed: app.standing.version(),
            from,
            latest: app.latest.as_deref(),
            newer: app.newer(),
            size: app.size,
            summary: &app.summary,
            programs: &app.programs,
            page: app.page.as_deref(),
            kind: if app.themes {
                "themes"
            } else if app.skills {
                "skills"
            } else {
                "app"
            },
            needs_flow: app.needs_flow.as_deref(),
            held: app.held,
            signed_by: signature.and_then(|s| s.by.as_deref()),
            key: signature.map(|s| s.key.as_str()).filter(|k| !k.is_empty()),
            signed: signature.map(|s| s.on.as_str()).filter(|d| !d.is_empty()),
            checked: signature.map(|s| s.checked.as_str()).filter(|d| !d.is_empty()),
            signature: signature.map(|s| s.signature.as_str()).filter(|s| !s.is_empty()),
            own_file: app.signing == Some(Signing::OwnFile),
        }
    }
}

/// A program's code signature as `info --json` gives it.
#[derive(serde::Serialize)]
struct CodeEntry<'a> {
    /// `signed`, `adhoc` or `unsigned`.
    kind: &'a str,
    authority: Option<&'a str>,
    team: Option<&'a str>,
    timestamp: Option<&'a str>,
    files: usize,
}

/// One app as `info --json` gives it: [`json`]'s object, and its programs'
/// code signatures.
#[derive(serde::Serialize)]
struct InfoEntry<'a> {
    #[serde(flatten)]
    entry: Entry<'a>,
    code_signatures: Vec<CodeEntry<'a>>,
}

/// `apps` as `--json` prints a list of them: an array of objects, each
/// app's `name`, `installed`, `from`, `latest`, `newer`, `size`, `summary`,
/// `programs`, `page`, `kind` (`app`, `themes` or `skills`), `needs_flow`,
/// `held`, `signed_by`, `key`, `signed`, `checked`, `signature` and
/// `own_file`, `null` where it has none; `[]` for none.
pub fn json(apps: &[Shown]) -> String {
    serde_json::to_string_pretty(&apps.iter().map(Entry::of).collect::<Vec<_>>()).unwrap_or_default() + "\n"
}

/// One app as `flowapp info --json` prints it: the object [`json`] lists,
/// and `code_signatures`, what `code` says signed its programs.
pub fn json_one(app: &Shown, code: &[CodeGroup]) -> String {
    let code_signatures = code
        .iter()
        .map(|group| match &group.signed {
            CodeSigned::Signed { authority, team, timestamp } => CodeEntry { kind: "signed", authority: Some(authority), team: Some(team).filter(|t| !t.is_empty()).map(|t| t.as_str()), timestamp: Some(timestamp).filter(|t| !t.is_empty()).map(|t| t.as_str()), files: group.files },
            CodeSigned::AdHoc => CodeEntry { kind: "adhoc", authority: None, team: None, timestamp: None, files: group.files },
            CodeSigned::Unsigned => CodeEntry { kind: "unsigned", authority: None, team: None, timestamp: None, files: group.files },
        })
        .collect();
    serde_json::to_string_pretty(&InfoEntry { entry: Entry::of(app), code_signatures }).unwrap_or_default() + "\n"
}

/// `flowapp list`: what's installed, its version, the newest there is, and
/// who signed it (or that it came with Flow, or is a file of the user's
/// own).
pub fn list(apps: &[Shown], look: Look) -> String {
    // Who signed it says too where it's from: a game that came with Flow
    // says so there.
    let mut table = Table::new(vec![Column::new(t("flowapp.head_app")), Column::new(t("flowapp.head_installed")), Column::new(t("flowapp.head_latest")), Column::new(t("flowapp.head_signed")).cut()]);
    for app in apps {
        table.rows.push(vec![app.name_cell(), app.installed_cell(), app.latest_cell(), app.signing_cell()]);
    }
    table.render(look)
}

/// `flowapp outdated` and `update`: what has a newer version, and how much
/// there is to download.
pub fn outdated(apps: &[Shown], look: Look) -> String {
    let mut table = Table::new(vec![Column::new(t("flowapp.head_app")), Column::new(t("flowapp.head_installed")), Column::new(t("flowapp.head_latest")), Column::new(t("flowapp.head_size")).right()]);
    for app in apps {
        let size = app.size.map_or_else(Cell::none, |n| Cell::new(size(n), Ink::Plain));
        table.rows.push(vec![app.name_cell(), app.installed_cell(), app.latest_cell(), size]);
    }
    table.render(look)
}

/// `flowapp search`: the apps there are, what's installed of them, and
/// what each is, cut to the terminal.
pub fn search(apps: &[Shown], look: Look) -> String {
    let mut table = Table::new(vec![Column::new(t("flowapp.head_app")), Column::new(t("flowapp.head_installed")), Column::new(t("flowapp.head_latest")), Column::new(t("flowapp.head_signed")).cut_last(), Column::new(t("flowapp.head_about")).cut()]);
    for app in apps {
        table.rows.push(vec![app.name_cell(), app.installed_cell(), app.latest_cell(), app.offered_cell(), Cell::new(app.summary.clone(), Ink::Plain)]);
    }
    table.render(look)
}

/// A shell as `flowapp first` names it to people: as it calls itself.
pub fn shell_word(shell: &str) -> &str {
    match shell {
        "powershell" => "PowerShell",
        "busybox" => "BusyBox",
        other => other,
    }
}

/// What still comes ahead of a name in a shell, in a few words.
fn ahead_words(ahead: Ahead) -> (String, Ink) {
    match ahead {
        Ahead::Builtin => (t("flowapp.ahead_cell_builtin").to_string(), Ink::Bad),
        Ahead::Which => (t("flowapp.ahead_cell_which").to_string(), Ink::Dim),
        Ahead::Alias(of) => (tf("flowapp.ahead_cell_alias", &[("of", &of)]), Ink::Bad),
        Ahead::Function => (t("flowapp.ahead_cell_function").to_string(), Ink::Bad),
        Ahead::Applet => (t("flowapp.ahead_cell_applet").to_string(), Ink::Bad),
        Ahead::Own => (t("flowapp.ahead_cell_own").to_string(), Ink::Bad),
    }
}

/// `flowapp first` alone: each name first in each shell (and in the tab
/// it runs in, where that tab's own choices change it), the app whose
/// program it runs, how it's first, what it took away there, and what
/// still comes ahead of it there.
pub fn firsts(rows: &[Row], look: Look) -> String {
    let mut table = Table::new(vec![Column::new(t("flowapp.head_shell")), Column::new(t("flowapp.head_command")), Column::new(t("flowapp.head_app")).cut(), Column::new(t("flowapp.head_how")).cut(), Column::new(t("flowapp.head_took")).cut(), Column::new(t("flowapp.head_ahead")).cut()]);
    for row in rows {
        let shell = match row.here {
            true => tf("flowapp.shell_here", &[("shell", &shell_word(&row.shell))]),
            false => shell_word(&row.shell).to_string(),
        };
        let how = match row.how {
            How::Path => t("flowapp.how_path"),
            How::Prompt => t("flowapp.how_prompt"),
        };
        let app = row.app.as_ref().map_or_else(Cell::none, |app| Cell::new(app.clone(), Ink::Dim));
        let took = match row.took.is_empty() {
            true => Cell::none(),
            false => Cell::new(row.took.iter().map(|t| t.word()).collect::<Vec<_>>().join(", "), Ink::Plain),
        };
        let mut ahead = Cell::none();
        for (i, (words, ink)) in row.ahead.iter().map(|a| ahead_words(*a)).enumerate() {
            if i > 0 {
                ahead = ahead.and(", ", Ink::Plain);
            }
            ahead = ahead.and(words, ink);
        }
        table.rows.push(vec![Cell::new(shell, Ink::Plain), Cell::new(row.name.clone(), Ink::Bold), app, Cell::new(how, Ink::Plain), took, ahead]);
    }
    table.render(look)
}

/// One name first in a shell as `flowapp first --json` gives it, its keys
/// and words English whatever the language.
#[derive(serde::Serialize)]
struct FirstEntry<'a> {
    shell: &'a str,
    /// In the tab `flowapp` runs in, as its own choices make it.
    here: bool,
    name: &'a str,
    app: Option<&'a str>,
    /// `path` (scripts find it too) or `prompt` (at the prompt alone).
    how: &'a str,
    took: Vec<&'static str>,
    /// What still comes ahead of it: `builtin`, `which` (zsh's, which hands
    /// it plain uses), `alias` (with what it's an alias of), `function`,
    /// `applet`, `own` (the shell's own, which nothing puts aside).
    ahead: Vec<serde_json::Value>,
}

/// `flowapp first --json`: an array of what [`firsts`] lists.
pub fn firsts_json(rows: &[Row]) -> String {
    let entries: Vec<FirstEntry> = rows
        .iter()
        .map(|row| FirstEntry {
            shell: &row.shell,
            here: row.here,
            name: &row.name,
            app: row.app.as_deref(),
            how: match row.how {
                How::Path => "path",
                How::Prompt => "prompt",
            },
            took: row.took.iter().map(|t| t.word()).collect(),
            ahead: row
                .ahead
                .iter()
                .map(|a| match a {
                    Ahead::Builtin => serde_json::json!({ "what": "builtin" }),
                    Ahead::Which => serde_json::json!({ "what": "which" }),
                    Ahead::Alias(of) => serde_json::json!({ "what": "alias", "of": of }),
                    Ahead::Function => serde_json::json!({ "what": "function" }),
                    Ahead::Applet => serde_json::json!({ "what": "applet" }),
                    Ahead::Own => serde_json::json!({ "what": "own" }),
                })
                .collect(),
        })
        .collect();
    serde_json::to_string_pretty(&entries).unwrap_or_default() + "\n"
}

/// `flowapp info`: the app's name and what it is, wrapped to the
/// terminal, over a table of where it stands, its download for `platform`,
/// who signed it and when, what `code` says signed its programs, its
/// programs, its release page and how long ago the catalog it goes by was
/// `read`; then how to check its signature yourself.
pub fn info(app: &Shown, platform: &str, read: Duration, code: &[CodeGroup], look: Look) -> String {
    let mut out = paint(&[(app.name.clone(), Ink::Bold)], look.styled);
    out = link(&out, app.page.as_deref(), look.styled) + "\n";
    if !app.summary.is_empty() {
        let lines = match look.width {
            Some(cols) => crate::text::wrap_plain(&app.summary, cols, crate::text::Style::Normal).into_iter().map(|line| line.into_iter().map(|(text, _)| text).collect::<String>()).collect(),
            None => vec![app.summary.clone()],
        };
        for line in lines {
            out.push_str(line.trim_end());
            out.push('\n');
        }
    }
    out.push('\n');
    let mut table = Table::new(vec![Column::new(""), Column::new("").cut()]);
    table.header = false;
    // A row under the one before it, of the same kind, has no word of its own.
    let mut row = |key: &str, value: Cell| table.rows.push(vec![Cell::new(if key.is_empty() { "" } else { t(key) }, Ink::Bold), value]);
    let installed = match &app.standing {
        Standing::Installed(v) => Cell::new(v.clone(), Ink::Plain),
        Standing::Bundled(v) => Cell::new(v.clone(), Ink::Plain).and(t("flowapp.with_flow"), Ink::Dim),
        Standing::Absent => Cell::new(t("flowapp.not_installed_yet"), Ink::Dim),
    };
    row("flowapp.head_installed", installed);
    row("flowapp.head_latest", app.latest_cell());
    let download = match app.size {
        Some(n) => Cell::new(tf("flowapp.download_for", &[("size", &size(n)), ("platform", &platform)]), Ink::Plain),
        None => Cell::new(tf("flowapp.none_for", &[("platform", &platform)]), Ink::Dim),
    };
    row("flowapp.head_download", download);
    match (&app.signing, app.signature()) {
        (Some(Signing::OwnFile), _) => row("flowapp.head_signed_by", Cell::new(t("flowapp.not_signed"), Ink::Dim)),
        (Some(Signing::WithFlow), None) => row("flowapp.head_signed_by", Cell::new(t("flowapp.from_flow"), Ink::Dim)),
        (_, Some(signature)) => {
            let who = signature.by.clone().unwrap_or_else(|| t("flowapp.signer_unknown").to_string());
            let ink = if signature.unnoted { Ink::Dim } else { Ink::Plain };
            row("flowapp.head_signed_by", Cell::new(tf("flowapp.signed_by_key", &[("who", &who), ("key", &signing::short(&signature.key))]), ink));
            if !signature.on.is_empty() {
                row("flowapp.head_signed_on", Cell::new(signature.on.clone(), ink));
            }
            if !signature.checked.is_empty() {
                row("flowapp.head_checked", Cell::new(tf("flowapp.checked_on", &[("date", &signature.checked)]), Ink::Plain));
            }
        }
        _ => {}
    }
    // Who signed each group of programs, then on a row of its own how
    // many and when, so a terminal's width cuts neither.
    for (i, group) in code.iter().enumerate() {
        let label = if i == 0 { "flowapp.head_code_signed" } else { "" };
        let files = match group.files {
            1 => t("flowapp.code_files_one").to_string(),
            n => tf("flowapp.code_files", &[("count", &n)]),
        };
        match &group.signed {
            CodeSigned::Signed { authority, timestamp, .. } => {
                row(label, Cell::new(authority.clone(), Ink::Plain));
                let when = if timestamp.is_empty() { files } else { tf("flowapp.code_files_at", &[("files", &files), ("at", timestamp)]) };
                row("", Cell::new(when, Ink::Dim));
            }
            CodeSigned::AdHoc => row(label, Cell::new(tf("flowapp.code_adhoc", &[("files", &files)]), Ink::Dim)),
            CodeSigned::Unsigned => row(label, Cell::new(tf("flowapp.code_unsigned", &[("files", &files)]), Ink::Dim)),
        }
    }
    if let Some(needs) = &app.needs_flow {
        let cell = match app.held {
            true => Cell::new(tf("flowapp.needs_newer", &[("needs", needs), ("flow", &super::itself::flow_version())]), Ink::Bad),
            false => Cell::new(tf("flowapp.needs", &[("needs", needs)]), Ink::Plain),
        };
        row("flowapp.head_needs", cell);
    }
    if !app.programs.is_empty() {
        row("flowapp.head_programs", Cell::new(app.programs.join(", "), Ink::Plain));
    }
    if let Some(page) = &app.page {
        row("flowapp.head_release", Cell::new(page.clone(), Ink::Plain).linked(Some(page.clone())));
    }
    row("flowapp.head_catalog", Cell::new(tf("flowapp.read_ago", &[("ago", &ago(read))]), Ink::Plain));
    out += &table.render(look);
    if let Some(signature) = app.signature().filter(|s| s.by.is_some() && !s.signature.is_empty()) {
        out.push('\n');
        out += &check_yourself(signature, look);
    }
    out
}

/// The lines that check `signature` with OpenSSL alone, trusting nothing
/// of flowapp's: the key fetched from Flow-releases and its fingerprint
/// worked out, the download fetched, its signature written as a file, and
/// the signature checked (`docs/design/flowapp-signatures.md`).
pub fn check_yourself(signature: &Signature, look: Look) -> String {
    let file = signature.url.rsplit(['/', '\\']).next().unwrap_or(&signature.url);
    let mut out = paint(&[(t("flowapp.check_yourself").to_string(), Ink::Bold)], look.styled) + "\n";
    out += &tf("flowapp.check_fingerprint", &[("key", &signature.key)]);
    out.push('\n');
    for line in [
        format!("curl -LO {}", signing::KEY_URL),
        "openssl pkey -pubin -in flow-update.pem -outform DER | openssl dgst -sha256".to_string(),
        format!("curl -LO {}", signature.url),
        format!("printf '%s\\n' '{}' | openssl base64 -d -A -out {file}.sig", signature.signature),
        format!("openssl pkeyutl -verify -pubin -inkey flow-update.pem -rawin -in {file} -sigfile {file}.sig"),
    ] {
        out += "  ";
        out += &line;
        out.push('\n');
    }
    out += t("flowapp.check_says");
    out.push('\n');
    out
}

/// What `outdated`, `update` and `upgrade` say of an app whose newer
/// version waits for a newer Flow than this one.
pub fn held_back(app: &Shown) -> String {
    let needs = app.needs_flow.as_deref().unwrap_or_default();
    tf("flowapp.held_back", &[("name", &app.name), ("version", &app.latest.as_deref().unwrap_or_default()), ("needs", &needs), ("flow", &super::itself::flow_version())])
}

/// How long ago something was, as the command history says it.
fn ago(age: Duration) -> String {
    crate::ui::ago(age.as_millis().min(i64::MAX as u128) as i64, 0)
}

/// What `install` and `upgrade` say where `name`'s download `failed`: where
/// nothing was there, that the catalog may be out of date, as it can be for
/// a few minutes after a release, and how to read it again.
pub fn failure(name: &str, failed: &Failed) -> String {
    match failed {
        Failed::Missing(words) => tf("flowapp.download_gone", &[("name", &name), ("error", words)]),
        Failed::Other(words) => words.clone(),
    }
}

/// What `install` and `upgrade` say where the catalog didn't read (`error`)
/// and they go by the copy read `age` ago.
pub fn offline(error: &str, age: Duration) -> String {
    tf("flowapp.offline", &[("error", &error), ("ago", &ago(age))])
}

/// The line under what `list`, `outdated` and `search` print in a terminal
/// where the catalog they go by was read more than [`STALE`] ago, dimmed;
/// none piped, where a script reads the lines.
pub fn stale(age: Duration, look: Look) -> Option<String> {
    (look.width.is_some() && age >= STALE).then(|| paint(&[(tf("flowapp.stale", &[("ago", &ago(age))]), Ink::Dim)], look.styled))
}

/// Why a download is fetched, which its line says: to install it, or
/// again for `verify`, which installs nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fetch {
    Install,
    Again,
}

impl Fetch {
    /// The words while it comes, and once it's done.
    fn words(self) -> (&'static str, &'static str) {
        match self {
            Fetch::Install => ("flowapp.installing", "flowapp.installed"),
            Fetch::Again => ("flowapp.fetching_again", "flowapp.fetched_again"),
        }
    }
}

/// The line a download in a terminal is, redrawn as it comes: what's being
/// installed (or fetched again, `fetch`), a bar, how much has come of how
/// much, and how fast, all within `cols` (a line that wrapped couldn't be
/// drawn again in place).
pub fn fetching(name: &str, version: &str, got: u64, total: u64, per_second: Option<f64>, fetch: Fetch, cols: usize) -> String {
    let what = tf(fetch.words().0, &[("name", &name), ("version", &version)]);
    let sizes = tf("flowapp.of_size", &[("got", &size(got)), ("size", &size(total))]);
    let speed = per_second.map(|r| tf("flowapp.per_second", &[("size", &size(r as u64))])).unwrap_or_default();
    let rest = format!("  {sizes}  {speed}");
    let most = cols.saturating_sub(1);
    let bar_room = most.saturating_sub(what.width() + rest.width() + 2).min(BAR);
    let mut spans: Spans = vec![(what, Ink::Plain)];
    if bar_room >= 6 {
        spans.push(("  ".to_string(), Ink::Plain));
        spans.extend(bar(if total == 0 { 1.0 } else { got as f64 / total as f64 }, bar_room));
    }
    spans.push((rest.trim_end().to_string(), Ink::Dim));
    paint(&cut(&spans, most), true)
}

/// The most columns a download's bar takes.
const BAR: usize = 24;

/// A bar `cells` wide, `done` of it (0 to 1) filled in green half a cell at
/// a time, the rest a dim track, in heavy box-drawing lines.
fn bar(done: f64, cells: usize) -> Spans {
    let halves = (done.clamp(0.0, 1.0) * cells as f64 * 2.0).round() as usize;
    let (full, half) = (halves / 2, halves % 2 == 1);
    let mut filled = "━".repeat(full);
    if half {
        filled.push('╸');
    }
    let track = "━".repeat(cells - full - usize::from(half));
    vec![(filled, Ink::Newer), (track, Ink::Dim)].into_iter().filter(|(text, _)| !text.is_empty()).collect()
}

/// A download's line once it's been checked against the catalog and
/// Flow's key (and once it's in place or `verify` has compared it, `done`):
/// a green mark, then what was installed (or fetched again, `fetch`) and
/// how much it was, within `cols`.
pub fn checked(name: &str, version: &str, total: u64, done: bool, fetch: Fetch, cols: usize) -> String {
    let (going, gone) = fetch.words();
    let what = tf(if done { gone } else { going }, &[("name", &name), ("version", &version)]);
    let how = tf("flowapp.checked", &[("size", &size(total))]);
    let spans: Spans = vec![("✓ ".to_string(), Ink::Newer), (what, Ink::Plain), (format!("  {how}"), Ink::Dim)];
    paint(&cut(&spans, cols.saturating_sub(1)), true)
}

/// A download's line once it's failed: a red mark, and what it was.
pub fn failed(name: &str, version: &str, fetch: Fetch, cols: usize) -> String {
    let what = tf(fetch.words().0, &[("name", &name), ("version", &version)]);
    paint(&cut(&[("✗ ".to_string(), Ink::Bad), (what, Ink::Plain)], cols.saturating_sub(1)), true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apps::Download;

    fn english() {
        crate::i18n::set_language("en", &[]);
    }

    /// The text of a styled line, its escapes taken out.
    fn bare(text: &str) -> String {
        let mut out = String::new();
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            if c != '\x1b' {
                out.push(c);
                continue;
            }
            match chars.next() {
                // SGR: to its final letter.
                Some('[') => while chars.next().is_some_and(|c| !c.is_ascii_alphabetic()) {},
                // OSC 8: to its string terminator, ESC \.
                Some(']') => while let Some(c) = chars.next() {
                    if c == '\x1b' {
                        chars.next();
                        break;
                    }
                },
                _ => {}
            }
        }
        out
    }

    /// A signature of no download's, which only the lines that check it
    /// yourself print.
    const SIGNATURE: &str = "G4/OI3sqyZi/nlfl66N+uo9dRyHDBWM6XzqFfBYXrwJyGtbgII78Zbk28xTzotaOzQwYqZ7zc9rMHJ79OpF+Ag==";

    /// Flow's key's fingerprint, as OpenSSL works it out.
    const FLOWS: &str = "f1866b49b0b42871463772bf1c9d77fae2091c32dc01681ae5d752f13008b4d0";

    /// An app as `flowapp` would show it, its download signed with Flow's
    /// key on 2026-11-18, and one installed checked as it was installed.
    fn app(name: &str, standing: Standing, latest: &str, summary: &str) -> Shown {
        let url = format!("https://github.com/arcanii/Flow-releases/releases/download/{name}-v{latest}/{name}-{latest}-macos-aarch64.tar.gz");
        let download = Download { version: latest.into(), url, size: 2_879_986, signature: SIGNATURE.into(), signed: "2026-11-18".into(), ..Download::default() };
        let offered = Signature::of(&download, FLOWS);
        let signing = match standing {
            Standing::Installed(_) => Some(Signing::Signed(Signature { checked: "2026-11-19".into(), ..offered.clone() })),
            Standing::Bundled(_) => Some(Signing::WithFlow),
            Standing::Absent => None,
        };
        Shown {
            name: name.into(),
            standing,
            latest: Some(latest.into()),
            size: Some(download.size),
            summary: summary.into(),
            programs: vec![name.into()],
            page: download.release_page(),
            themes: false,
            skills: false,
            needs_flow: None,
            held: false,
            offered: Some(offered),
            signing,
        }
    }

    fn some() -> Vec<Shown> {
        vec![
            app("flow-mines", Standing::Bundled("0.1.6".into()), "0.1.6", "Flow Mines: the mine-finding puzzle, drawn in text over Flow's liquid"),
            app("flow-rogue", Standing::Installed("0.1.44".into()), "0.1.45", "Flow Rogue: a roguelike in Flow's tabs, with a town, the wilds, lairs and a dungeon"),
            app("flow-sdk", Standing::Absent, "0.1.0", "Flow's SDK for C and Python, with its demos"),
        ]
    }

    const STYLED: Look = Look { styled: true, width: Some(100) };

    #[test]
    fn a_ruled_tables_lines_are_all_as_wide_and_its_rules_meet() {
        english();
        let text = search(&some(), STYLED);
        let lines: Vec<String> = text.lines().map(bare).collect();
        let widths: Vec<usize> = lines.iter().map(|l| l.width()).collect();
        assert!(widths.iter().all(|w| *w == widths[0]), "{widths:?}\n{}", lines.join("\n"));
        assert!(lines[0].starts_with('╭') && lines[0].ends_with('╮'));
        assert!(lines[1].starts_with("│ App") && lines[2].starts_with('├'));
        assert!(lines.last().unwrap().starts_with('╰'));
        // Each crossing sits where the bars of the rows meet it.
        let bars = |line: &str| line.chars().enumerate().filter(|(_, c)| "│┬┼┴╭╮├┤╰╯".contains(*c)).map(|(i, _)| i).collect::<Vec<_>>();
        assert!(lines.iter().all(|l| bars(l) == bars(&lines[0])), "{}", lines.join("\n"));
    }

    #[test]
    fn its_header_is_bold_a_newer_version_green_and_a_game_that_comes_with_flow_dim() {
        english();
        let text = list(&some(), STYLED);
        assert!(text.contains("\x1b[1mApp\x1b[0m"), "{text}");
        let rogue = text.lines().find(|l| l.contains("flow-rogue")).unwrap();
        assert!(rogue.contains("0.1.44 ") && rogue.contains("\x1b[32m0.1.45\x1b[0m"), "{rogue:?}");
        let mines = text.lines().find(|l| l.contains("flow-mines")).unwrap();
        assert!(mines.contains("\x1b[2m0.1.6\x1b[0m") && mines.contains("\x1b[2mcomes with Flow\x1b[0m"), "{mines:?}");
        assert!(bare(rogue).contains("Flow f186 6b49 · 2026-11-18"), "{rogue:?}");
    }

    #[test]
    fn each_apps_name_links_to_its_release_page() {
        english();
        let text = outdated(&some()[1..2], STYLED);
        assert!(text.contains("\x1b]8;;https://github.com/arcanii/Flow-releases/releases/tag/flow-rogue-v0.1.45\x1b\\flow-rogue\x1b]8;;\x1b\\"), "{text:?}");
        assert!(bare(&text).contains("2.7 MB"), "{text}");
    }

    #[test]
    fn piped_a_table_is_plain_lined_up_text_and_nothing_is_cut() {
        english();
        let text = search(&some(), Look { styled: false, width: None });
        assert!(!text.contains('\x1b') && !text.contains('│'), "{text}");
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "App         Installed  Latest  Signed                       What it is");
        assert_eq!(lines[1], "flow-mines  0.1.6      0.1.6   Flow f186 6b49 · 2026-11-18  Flow Mines: the mine-finding puzzle, drawn in text over Flow's liquid");
        assert_eq!(lines[3], "flow-sdk    -          0.1.0   Flow f186 6b49 · 2026-11-18  Flow's SDK for C and Python, with its demos");
        // Every column starts where its header does.
        assert!(lines[1..].iter().all(|l| l.find("0.1.").is_some_and(|i| i == 12 || i == 23) && l.find("Flow f186") == Some(31)), "{text}");
    }

    #[test]
    fn a_long_summary_is_cut_to_the_terminal_with_an_ellipsis() {
        english();
        for styled in [true, false] {
            let text = search(&some(), Look { styled, width: Some(60) });
            for line in text.lines().map(bare) {
                assert!(line.width() <= 60, "{} columns: {line}", line.width());
            }
            let rogue = text.lines().map(bare).find(|l| l.contains("flow-rogue")).unwrap();
            assert!(rogue.trim_end_matches([' ', '│']).ends_with('…'), "{rogue}");
        }
        // The names, never cut, run past a terminal too narrow for them.
        let narrow = search(&some(), Look { styled: false, width: Some(20) });
        assert!(narrow.lines().nth(2).unwrap().starts_with("flow-rogue  0.1.44"), "{narrow}");
    }

    #[test]
    fn japanese_keeps_its_columns_by_what_shows() {
        english();
        let mut apps = some();
        apps[0].summary = "フローの地雷さがし、液体の上に文字で描かれます".into();
        apps[0].name = "地雷".into();
        for width in [Some(100), Some(40)] {
            let text = search(&apps, Look { styled: true, width });
            let widths: Vec<usize> = text.lines().map(|l| bare(l).width()).collect();
            assert!(widths.iter().all(|w| *w == widths[0]), "{widths:?}\n{text}");
        }
        assert!(cut(&[("地雷さがし".to_string(), Ink::Plain)], 6).iter().map(|(t, _)| t.as_str()).collect::<String>() == "地雷…");
    }

    #[test]
    fn info_says_where_an_app_stands_under_what_it_is() {
        english();
        let rogue = &some()[1];
        let text = info(rogue, "macos-aarch64", Duration::ZERO, &[], Look { styled: false, width: Some(50) });
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "flow-rogue");
        // What it is, wrapped to the terminal, then a line between.
        assert!(lines[1].starts_with("Flow Rogue: a roguelike") && lines[1].width() <= 50 && lines[2].ends_with("dungeon") && lines[3].is_empty(), "{text}");
        assert!(lines.contains(&"Installed  0.1.44") && lines.contains(&"Latest     0.1.45"), "{text}");
        assert!(lines.contains(&"Download   2.7 MB for macos-aarch64"), "{text}");
        assert!(lines.iter().any(|l| l.starts_with("Release    https://github.com/") && l.ends_with('…')), "{text}");
        let mines = info(&some()[0], "macos-aarch64", Duration::ZERO, &[], Look { styled: false, width: None });
        assert!(mines.contains("Installed  0.1.6, comes with Flow"), "{mines}");
        let sdk = Shown { latest: None, size: None, page: None, ..some()[2].clone() };
        let sdk = info(&sdk, "linux-x86_64", Duration::ZERO, &[], Look { styled: false, width: None });
        assert!(sdk.contains("Installed  not installed") && sdk.contains("Latest     -") && sdk.contains("Download   none for linux-x86_64 yet"), "{sdk}");
    }

    #[test]
    fn a_download_thats_gone_says_the_catalog_may_be_out_of_date() {
        english();
        let url = "https://github.com/arcanii/Flow-releases/releases/download/flow-invaders-v0.1.0/flow-invaders-0.1.0-macos-aarch64.tar.gz";
        let gone = failure("flow-invaders", &Failed::Missing(format!("{url}: http status: 404")));
        assert!(gone.starts_with(&format!("The download of flow-invaders isn't where the catalog says ({url}: http status: 404).")), "{gone}");
        assert!(gone.ends_with("a few minutes after a release: flowapp update reads it again."), "{gone}");
        assert_eq!(failure("flow-invaders", &Failed::Other("flow-invaders: its SHA-256 isn't the catalog's".into())), "flow-invaders: its SHA-256 isn't the catalog's");
    }

    #[test]
    fn a_catalog_read_a_while_ago_says_how_long() {
        english();
        let hours = Duration::from_secs(9 * 3600);
        assert_eq!(offline("nowhere.json: no such file", hours), "The catalog didn't read (nowhere.json: no such file), so this goes by the one read 9 h ago.");
        // info says it in its table, read just now or long ago.
        let text = info(&some()[1], "macos-aarch64", hours, &[], Look { styled: false, width: None });
        assert!(text.lines().any(|l| l == "Catalog    read 9 h ago"), "{text}");
        assert!(info(&some()[1], "macos-aarch64", Duration::ZERO, &[], Look { styled: false, width: None }).contains("Catalog    read just now\n"));
        // Under the other tables, only once it's a day old, dimmed where
        // styled, and never piped, where a script reads the lines.
        let days = Duration::from_secs(3 * 86400);
        assert_eq!(stale(hours, STYLED), None);
        assert_eq!(stale(days, Look { styled: false, width: Some(80) }).as_deref(), Some("This is from the catalog read 3 d ago: flowapp update reads it again."));
        assert_eq!(stale(days, STYLED).as_deref(), Some("\x1b[2mThis is from the catalog read 3 d ago: flowapp update reads it again.\x1b[0m"));
        assert_eq!(stale(days, Look { styled: false, width: None }), None);
    }

    #[test]
    fn a_download_line_fits_the_terminal_with_its_bar_its_size_and_its_speed() {
        english();
        let line = bare(&fetching("flow-rogue", "0.1.45", 1_400_000, 2_879_986, Some(3_565_158.0), Fetch::Install, 100));
        assert!(line.starts_with("Installing flow-rogue 0.1.45…  ━"), "{line}");
        assert!(line.ends_with("1.3 MB of 2.7 MB  3.4 MB/s"), "{line}");
        assert_eq!(line.chars().filter(|c| "━╸".contains(*c)).count(), BAR, "{line}");
        for cols in [20, 40, 60] {
            let line = bare(&fetching("flow-rogue", "0.1.45", 0, 2_879_986, None, Fetch::Install, cols));
            assert!(line.width() < cols, "{cols}: {line}");
        }
        assert_eq!(bar(0.5, 10), [("━━━━━".to_string(), Ink::Newer), ("━━━━━".to_string(), Ink::Dim)]);
        assert_eq!(bar(0.25, 10)[0].0, "━━╸");
        assert_eq!(bar(1.0, 4), [("━━━━".to_string(), Ink::Newer)]);
        let done = checked("flow-rogue", "0.1.45", 2_879_986, true, Fetch::Install, 80);
        assert!(done.starts_with("\x1b[32m✓ \x1b[0m") && bare(&done) == "✓ Installed flow-rogue 0.1.45.  2.7 MB, its checksum and signature checked", "{done:?}");
    }

    #[test]
    fn verifys_download_line_says_it_fetches_again_and_never_that_it_installs() {
        english();
        let line = bare(&fetching("flowapp", "0.2.0", 1_000_000, 2_060_840, None, Fetch::Again, 100));
        assert!(line.starts_with("Downloading flowapp 0.2.0 again…  ━"), "{line}");
        assert_eq!(bare(&checked("flowapp", "0.2.0", 2_060_840, false, Fetch::Again, 80)), "✓ Downloading flowapp 0.2.0 again…  2.0 MB, its checksum and signature checked");
        assert_eq!(bare(&checked("flowapp", "0.2.0", 2_060_840, true, Fetch::Again, 80)), "✓ Downloaded flowapp 0.2.0 again.  2.0 MB, its checksum and signature checked");
        assert_eq!(bare(&failed("flowapp", "0.2.0", Fetch::Again, 80)), "✗ Downloading flowapp 0.2.0 again…");
        assert_eq!(bare(&failed("flowapp", "0.2.0", Fetch::Install, 80)), "✗ Installing flowapp 0.2.0…");
    }

    #[test]
    fn json_gives_each_app_in_keys_a_script_reads_whatever_the_language() {
        crate::i18n::set_language("ja", &[]);
        let text = json(&some());
        let apps: serde_json::Value = serde_json::from_str(&text).unwrap();
        let mines = &apps[0];
        assert_eq!((mines["name"].as_str(), mines["installed"].as_str(), mines["from"].as_str()), (Some("flow-mines"), Some("0.1.6"), Some("flow")));
        assert_eq!((mines["latest"].as_str(), mines["newer"].as_bool(), mines["size"].as_u64()), (Some("0.1.6"), Some(false), Some(2_879_986)));
        let rogue = &apps[1];
        assert_eq!((rogue["from"].as_str(), rogue["newer"].as_bool()), (Some("flowapp"), Some(true)));
        assert_eq!(rogue["page"].as_str(), Some("https://github.com/arcanii/Flow-releases/releases/tag/flow-rogue-v0.1.45"));
        assert_eq!(rogue["programs"], serde_json::json!(["flow-rogue"]));
        let sdk = &apps[2];
        assert!(sdk["installed"].is_null() && sdk["from"].is_null() && sdk["newer"] == false, "{sdk}");
        assert_eq!(json(&[]), "[]\n");
        let mut one: serde_json::Value = serde_json::from_str(&json_one(&some()[1], &[])).unwrap();
        assert_eq!(one.as_object_mut().unwrap().remove("code_signatures"), Some(serde_json::json!([])));
        assert_eq!(one, apps[1]);
        assert!(!text.contains('\x1b'));
        // Each says what kind it is: an app, or a package of themes or skills.
        let mut kinds = some();
        kinds[1].themes = true;
        kinds[2].skills = true;
        let kinds: serde_json::Value = serde_json::from_str(&json(&kinds)).unwrap();
        assert_eq!([&kinds[0]["kind"], &kinds[1]["kind"], &kinds[2]["kind"]], ["app", "themes", "skills"]);
    }

    #[test]
    fn an_app_waiting_for_a_newer_flow_isnt_green_and_says_which_flow() {
        english();
        let mut rogue = some()[1].clone();
        rogue.needs_flow = Some("999.0".into());
        rogue.held = true;
        assert!(rogue.newer(), "it's newer all the same");
        let text = outdated(std::slice::from_ref(&rogue), STYLED);
        let line = text.lines().find(|l| l.contains("flow-rogue")).unwrap();
        assert!(!line.contains("\x1b[32m") && bare(line).contains("0.1.45"), "{line:?}");
        assert_eq!(held_back(&rogue), format!("flow-rogue 0.1.45 needs Flow 999.0 or later, and this is Flow {}: it'll upgrade once Flow is updated.", crate::version::VERSION));
        let plain = Look { styled: false, width: None };
        let held = info(&rogue, "macos-aarch64", Duration::ZERO, &[], plain);
        assert!(held.contains(&format!("Needs      Flow 999.0 or later (this is {})", crate::version::VERSION)), "{held}");
        let styled = info(&rogue, "macos-aarch64", Duration::ZERO, &[], STYLED);
        assert!(styled.contains("\x1b[31mFlow 999.0 or later"), "red where this Flow is older: {styled:?}");
        // One this Flow is new enough for says so plainly; one naming no
        // Flow says nothing.
        let fine = Shown { needs_flow: Some("0.1.0".into()), held: false, ..rogue.clone() };
        assert!(info(&fine, "macos-aarch64", Duration::ZERO, &[], plain).contains("Needs      Flow 0.1.0 or later\n"));
        assert!(!info(&some()[1], "macos-aarch64", Duration::ZERO, &[], plain).contains("Needs"));
        let entry: serde_json::Value = serde_json::from_str(&json_one(&rogue, &[])).unwrap();
        assert_eq!((entry["needs_flow"].as_str(), entry["held"].as_bool(), entry["newer"].as_bool()), (Some("999.0"), Some(true), Some(true)));
        let entry: serde_json::Value = serde_json::from_str(&json_one(&some()[1], &[])).unwrap();
        assert!(entry["needs_flow"].is_null() && entry["held"] == false, "{entry}");
    }

    #[test]
    fn the_signed_column_says_whose_key_and_when_dim_where_only_the_catalog_says_it() {
        english();
        let mut apps = some();
        // Installed before flowapp noted its download: the catalog's word.
        let catalogs = Signature { unnoted: true, ..apps[2].offered.clone().unwrap() };
        apps[2] = Shown { standing: Standing::Installed("0.1.0".into()), signing: Some(Signing::Signed(catalogs)), ..apps[2].clone() };
        let mine = Shown { name: "my-themes".into(), standing: Standing::Installed("1.0".into()), signing: Some(Signing::OwnFile), offered: None, ..apps[2].clone() };
        apps.push(mine);
        let text = list(&apps, STYLED);
        let line = |name: &str| text.lines().find(|l| l.contains(name)).unwrap().to_string();
        assert!(line("flow-rogue").contains("Flow f186 6b49 · 2026-11-18") && !line("flow-rogue").contains("\x1b[2mFlow f186"), "{text}");
        assert!(line("flow-sdk").contains("\x1b[2mFlow f186 6b49 · 2026-11-18\x1b[0m"), "dim: {text}");
        assert!(line("flow-mines").contains("\x1b[2mcomes with Flow\x1b[0m"), "{text}");
        assert!(line("my-themes").contains("\x1b[2mnot signed\x1b[0m"), "{text}");
        assert!(text.lines().nth(1).is_some_and(|head| bare(head).contains("│ Signed ")), "{text}");
        // A download added before the catalog kept a date shows the key alone.
        let mut undated = some();
        if let Some(Signing::Signed(s)) = &mut undated[1].signing {
            s.on.clear();
        }
        let undated = list(&undated, Look { styled: false, width: None });
        assert!(undated.lines().any(|l| l.starts_with("flow-rogue") && l.ends_with("Flow f186 6b49")), "{undated}");
    }

    #[test]
    fn search_shows_the_catalogs_download_and_fits_eighty_columns() {
        english();
        let mut apps = some();
        // Installed at an older version, nothing noted: list has nothing,
        // search still says who signed the catalog's.
        apps[1].signing = None;
        let found = search(&apps, Look { styled: true, width: Some(80) });
        for line in found.lines().map(bare) {
            assert!(line.width() <= 80, "{} columns: {line}", line.width());
        }
        let rogue = found.lines().map(bare).find(|l| l.contains("flow-rogue")).unwrap();
        assert!(rogue.contains("Flow f186 6b49 · 2026-11-18") && rogue.trim_end_matches([' ', '│']).ends_with('…'), "the last column is cut: {rogue}");
        let listed = list(&apps, Look { styled: false, width: None });
        assert!(listed.lines().any(|l| l.starts_with("flow-rogue") && l.ends_with("  -")), "{listed}");
    }

    #[test]
    fn info_says_who_signed_it_and_how_to_check_that_yourself() {
        english();
        let code = [
            CodeGroup { signed: CodeSigned::Signed { authority: "Developer ID Application: Someone (ABCDE12345)".into(), team: "ABCDE12345".into(), timestamp: "18 Nov 2026 at 08:15:35".into() }, files: 3 },
            CodeGroup { signed: CodeSigned::AdHoc, files: 1 },
        ];
        let text = info(&some()[1], "macos-aarch64", Duration::ZERO, &code, Look { styled: false, width: None });
        let lines: Vec<&str> = text.lines().collect();
        assert!(lines.contains(&"Signed by    Flow (key f186 6b49)"), "{text}");
        assert!(lines.contains(&"Signed on    2026-11-18"), "{text}");
        assert!(lines.contains(&"Checked      with Flow's key as it was installed, 2026-11-19"), "{text}");
        assert!(lines.contains(&"Code signed  Developer ID Application: Someone (ABCDE12345)"), "{text}");
        assert!(lines.contains(&"             3 files · signed 18 Nov 2026 at 08:15:35"), "{text}");
        assert!(lines.contains(&"             ad hoc, by no one · 1 file"), "{text}");
        // Then the lines, each whole, never cut.
        let file = "flow-rogue-0.1.45-macos-aarch64.tar.gz";
        assert!(lines.contains(&format!("Flow's key's fingerprint is {FLOWS}, which the second line must print.").as_str()), "{text}");
        assert!(lines.contains(&"  curl -LO https://raw.githubusercontent.com/arcanii/Flow-releases/main/flow-update.pem"), "{text}");
        assert!(lines.contains(&"  openssl pkey -pubin -in flow-update.pem -outform DER | openssl dgst -sha256"), "{text}");
        assert!(lines.contains(&format!("  printf '%s\\n' '{SIGNATURE}' | openssl base64 -d -A -out {file}.sig").as_str()), "{text}");
        assert!(lines.contains(&format!("  openssl pkeyutl -verify -pubin -inkey flow-update.pem -rawin -in {file} -sigfile {file}.sig").as_str()), "{text}");
        assert!(text.ends_with("The last line says “Signature Verified Successfully” for a download Flow signed.\n"), "{text}");
        // A file of your own: not signed, and nothing to check.
        let mine = Shown { signing: Some(Signing::OwnFile), offered: None, ..some()[1].clone() };
        let mine = info(&mine, "macos-aarch64", Duration::ZERO, &[], Look { styled: false, width: None });
        assert!(mine.contains("Signed by  not signed") && !mine.contains("openssl"), "{mine}");
    }

    #[test]
    fn json_says_who_signed_it_with_the_whole_key_and_its_code_signatures() {
        english();
        let apps: serde_json::Value = serde_json::from_str(&json(&some())).unwrap();
        let rogue = &apps[1];
        assert_eq!((rogue["signed_by"].as_str(), rogue["key"].as_str(), rogue["signed"].as_str(), rogue["checked"].as_str()), (Some("Flow"), Some(FLOWS), Some("2026-11-18"), Some("2026-11-19")));
        assert_eq!((rogue["signature"].as_str(), rogue["own_file"].as_bool()), (Some(SIGNATURE), Some(false)));
        // Not installed: the catalog's, not yet checked.
        assert!(apps[2]["checked"].is_null() && apps[2]["signed_by"] == "Flow", "{}", apps[2]);
        let code = [CodeGroup { signed: CodeSigned::Signed { authority: "Developer ID Application: Someone (ABCDE12345)".into(), team: "ABCDE12345".into(), timestamp: String::new() }, files: 2 }, CodeGroup { signed: CodeSigned::Unsigned, files: 1 }];
        let one: serde_json::Value = serde_json::from_str(&json_one(&some()[1], &code)).unwrap();
        assert_eq!(
            one["code_signatures"],
            serde_json::json!([
                {"kind": "signed", "authority": "Developer ID Application: Someone (ABCDE12345)", "team": "ABCDE12345", "timestamp": null, "files": 2},
                {"kind": "unsigned", "authority": null, "team": null, "timestamp": null, "files": 1}
            ])
        );
    }

    #[test]
    fn whats_installed_is_noted_from_its_install_else_the_catalogs_download_of_its_version() {
        english();
        let mut app = App { version: "0.2.0".into(), ..App::default() };
        let download = Download { version: "0.2.0".into(), url: "https://example.com/x-0.2.0.tar.gz".into(), signature: SIGNATURE.into(), signed: "2026-11-18".into(), ..Download::default() };
        app.downloads.insert(crate::apps::platform(), download.clone());
        let shown = || Shown::new("x", Some(&app), Standing::Installed("0.2.0".into()));
        let noted = Kept { version: "0.2.0".into(), download: Some(download.clone()), key: FLOWS.into(), checked: "2026-11-19".into(), ..Kept::default() };
        let Some(Signing::Signed(signature)) = shown().noting(Some(&noted), Some(&app)).signing else { panic!("not signed") };
        assert_eq!((signature.by.as_deref(), signature.checked.as_str(), signature.unnoted), (Some("Flow"), "2026-11-19", false));
        let before = Kept { version: "0.2.0".into(), ..Kept::default() };
        let Some(Signing::Signed(signature)) = shown().noting(Some(&before), Some(&app)).signing else { panic!("not signed") };
        assert!(signature.unnoted && signature.checked.is_empty() && signature.on == "2026-11-18");
        let older = Kept { version: "0.1.0".into(), ..Kept::default() };
        assert_eq!(shown().noting(Some(&older), Some(&app)).signing, None);
        let mine = Kept { version: "1.0".into(), own_file: true, ..Kept::default() };
        assert_eq!(shown().noting(Some(&mine), None).signing, Some(Signing::OwnFile));
        // Search's is the catalog's, whatever's installed.
        assert_eq!(shown().offered.map(|s| s.key), Some(FLOWS.to_string()));
    }

    #[test]
    fn a_package_of_your_own_of_a_catalog_apps_name_is_never_newer_in_the_catalog() {
        let mut app = App { version: "0.2.0".into(), ..App::default() };
        app.downloads.insert(crate::apps::platform(), Download { version: "0.2.0".into(), ..Download::default() });
        let shown = || Shown::new("x", Some(&app), Standing::Installed("0.1.0".into()));
        assert!(shown().newer() && shown().noting(Some(&Kept { version: "0.1.0".into(), ..Kept::default() }), Some(&app)).newer());
        let mine = Kept { version: "0.1.0".into(), own_file: true, ..Kept::default() };
        assert!(!shown().noting(Some(&mine), Some(&app)).newer(), "the catalog's doesn't upgrade it");
    }

    #[test]
    fn an_apps_needs_come_from_its_download_here_against_this_flow() {
        let mut app = App { version: "0.2.0".into(), ..App::default() };
        app.downloads.insert(crate::apps::platform(), Download { version: "0.2.0".into(), flow: "999.0".into(), ..Download::default() });
        let shown = Shown::new("flow-utils", Some(&app), Standing::Installed("0.1.0".into()));
        assert_eq!((shown.needs_flow.as_deref(), shown.held, shown.newer()), (Some("999.0"), true, true));
        app.downloads.get_mut(&crate::apps::platform()).unwrap().flow = crate::version::VERSION.into();
        let shown = Shown::new("flow-utils", Some(&app), Standing::Installed("0.1.0".into()));
        assert_eq!((shown.needs_flow.as_deref(), shown.held), (Some(crate::version::VERSION), false), "this Flow is the oldest it works in");
    }

    #[test]
    fn what_is_first_is_a_row_for_each_shell_and_name() {
        use crate::apps::first::{Ahead, How, Row, Take};
        english();
        let rows = vec![
            Row { shell: "zsh".into(), here: false, name: "echo".into(), app: Some("flow-utils".into()), how: How::Path, took: vec![], ahead: vec![Ahead::Builtin] },
            Row { shell: "powershell".into(), here: false, name: "ls".into(), app: None, how: How::Path, took: vec![Take::Alias], ahead: vec![] },
            Row { shell: "bash".into(), here: true, name: "tree".into(), app: Some("flow-utils".into()), how: How::Prompt, took: vec![], ahead: vec![] },
        ];
        let plain = firsts(&rows, Look { styled: false, width: None });
        let lines: Vec<&str> = plain.lines().collect();
        assert_eq!(lines[0], "Shell           Command  App         How                Took away  Still ahead");
        assert_eq!(lines[1], "zsh             echo     flow-utils  PATH, scripts too  -          builtin");
        assert_eq!(lines[2], "PowerShell      ls       -           PATH, scripts too  alias      -");
        assert_eq!(lines[3], "bash, this tab  tree     flow-utils  prompt alone       -          -");
        let styled = firsts(&rows, Look { styled: true, width: Some(72) });
        assert!(styled.lines().all(|l| bare(l).width() == 72), "fitted to the terminal: {styled}");
        let json: serde_json::Value = serde_json::from_str(&firsts_json(&rows)).unwrap();
        assert_eq!(json[0], serde_json::json!({ "shell": "zsh", "here": false, "name": "echo", "app": "flow-utils", "how": "path", "took": [], "ahead": [{ "what": "builtin" }] }));
        assert_eq!(json[1]["took"], serde_json::json!(["alias"]));
        assert_eq!((json[2]["here"].as_bool(), json[2]["how"].as_str()), (Some(true), Some("prompt")));
        let alias = firsts_json(&[Row { ahead: vec![Ahead::Alias("Get-ChildItem")], ..rows[1].clone() }]);
        assert!(alias.contains("\"of\": \"Get-ChildItem\""), "{alias}");
    }

    #[test]
    fn sizes_read_as_a_download_would_say_them() {
        assert_eq!(size(368_163), "360 KB");
        assert_eq!(size(2_879_986), "2.7 MB");
        assert_eq!(size(48 << 20), "48 MB");
        assert_eq!(size(0), "0 KB");
    }
}
