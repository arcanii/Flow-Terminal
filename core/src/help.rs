//! Help for the screen in front: a page for each of Flow's own screens, in
//! the language in use (`help.*` in `core/locales/`), with the keys as this
//! OS has them. F1 opens the page of the screen in front, `⌘?` (the Mac's
//! Help menu) or `Ctrl+Shift+?` opens it anywhere, Flow's own over the
//! terminal; `ui::draw_help` lays it out with `text.rs`'s markdown over the
//! screen it's about. Pages link to each other (`[words](help:picker)`),
//! and Flow's own ends with a link to every other page, so every OS can
//! reach them all from help itself, not only the Mac from its Help menu.
//! The same pages print in a tab with `flow-help` ([`for_terminal`]).

use std::cell::Cell;
use std::fmt::Display;

use crate::i18n::{t, tf};
use crate::platform::keys;
use crate::text::{self, Line, Style};

/// A page of help.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    /// Flow's keys and screens, over the terminal.
    Flow,
    /// Every one of Flow's keys as it's bound, made from `keys::TABLE`
    /// (About's Keys link; no screen of its own).
    Keys,
    History,
    /// The find bar over the terminal.
    Find,
    /// The command palette.
    Palette,
    Assistant,
    Settings,
    /// A color row's picker, over Settings.
    Picker,
    /// The theme gallery, over Settings.
    Gallery,
    /// The dialog for the assistant's API key.
    Key,
    /// The question before a program's model file is read.
    Model,
    /// The About card, and updates.
    About,
    /// The question whether to check for updates once a day (Linux).
    Updates,
    /// The new-tab picker the + after the tabs opens.
    Shells,
    /// Flow's apps, as `flowapp` keeps them (no screen of its own: reached
    /// from Flow's page, the Help menu and `flow-help apps`).
    Apps,
    /// The `flow` command, which opens a tab from a tab (no screen of its
    /// own: reached from Flow's page, the Help menu and `flow-help
    /// command`, `flow` being Flow's own page's name).
    FlowCommand,
    /// Outside apps using Flow through `flow-mcp`, and the card asking what
    /// one may do.
    Outside,
    /// The card an MCP server the assistant uses asks the user something
    /// on, or a prompt its arguments.
    ServerAsk,
    /// Whose the logos Flow shows are, and why it shows them: About's
    /// Legal button.
    Legal,
}

/// Which of Flow's screens are open, for the page that's about the one in
/// front.
#[derive(Debug, Clone, Copy, Default)]
pub struct Front {
    pub model_ask: bool,
    /// An outside app's card.
    pub outside: bool,
    /// An MCP server's question, over everything else of Flow's.
    pub server_ask: bool,
    /// Whether to check for updates automatically, or to restart into the
    /// Flow an update put in place, is asked: both are the updates' page.
    pub update_ask: bool,
    pub key_dialog: bool,
    pub about: bool,
    pub settings: bool,
    pub picker: bool,
    /// The theme gallery, over Settings.
    pub gallery: bool,
    pub history: bool,
    pub shells: bool,
    /// The find bar over the terminal has the keys.
    pub find: bool,
    /// The command palette.
    pub palette: bool,
    /// The assistant's panel has the keys.
    pub assistant: bool,
    /// A tab's name is being typed in the title bar: Flow's own page says how.
    pub renaming: bool,
}

impl Page {
    /// Every page, in the order the Mac's Help menu lists them.
    pub const ALL: [Page; 19] = [
        Page::Flow,
        Page::Keys,
        Page::History,
        Page::Find,
        Page::Palette,
        Page::Assistant,
        Page::Settings,
        Page::Picker,
        Page::Gallery,
        Page::Key,
        Page::Model,
        Page::Shells,
        Page::Apps,
        Page::FlowCommand,
        Page::Outside,
        Page::ServerAsk,
        Page::About,
        Page::Updates,
        Page::Legal,
    ];

    /// The page about the screen in front, in the order they cover each
    /// other (as `App::on_key` hands them keys); None over the terminal.
    pub fn in_front(f: Front) -> Option<Page> {
        if f.key_dialog {
            Some(Page::Key)
        } else if f.model_ask {
            Some(Page::Model)
        } else if f.outside {
            Some(Page::Outside)
        } else if f.server_ask {
            Some(Page::ServerAsk)
        } else if f.update_ask {
            Some(Page::Updates)
        } else if f.about {
            Some(Page::About)
        } else if f.palette {
            Some(Page::Palette)
        } else if f.history {
            Some(Page::History)
        } else if f.gallery {
            Some(Page::Gallery)
        } else if f.picker {
            Some(Page::Picker)
        } else if f.settings {
            Some(Page::Settings)
        } else if f.shells {
            Some(Page::Shells)
        } else if f.find {
            Some(Page::Find)
        } else if f.assistant {
            Some(Page::Assistant)
        } else if f.renaming {
            Some(Page::Flow)
        } else {
            None
        }
    }

    /// The page's name in links to it (`help:<name>`), which stays as it
    /// is whatever the language: the same as its words' keys.
    pub fn name(self) -> &'static str {
        match self {
            Page::Flow => "flow",
            Page::Keys => "keys",
            Page::History => "history",
            Page::Find => "find",
            Page::Palette => "palette",
            Page::Assistant => "assistant",
            Page::Settings => "settings",
            Page::Picker => "picker",
            Page::Gallery => "gallery",
            Page::Key => "key",
            Page::Model => "model",
            Page::About => "about",
            Page::Updates => "updates",
            Page::Shells => "shells",
            Page::Apps => "apps",
            Page::FlowCommand => "command",
            Page::Outside => "outside",
            Page::ServerAsk => "server_ask",
            Page::Legal => "legal",
        }
    }

    /// The page a link names (`help:<name>`), if it's one.
    pub fn linked(target: &str) -> Option<Page> {
        Page::named(target.strip_prefix("help:")?)
    }

    /// The page called `name` (`settings`), as links and `flow-help` name it.
    pub fn named(name: &str) -> Option<Page> {
        Page::ALL.into_iter().find(|page| page.name() == name)
    }

    /// The page laid out `cols` wide.
    pub fn laid_out(self, cols: usize) -> Laid {
        Laid::new(text::layout_linked(&self.body(), cols))
    }

    /// The pages the page's links go to, in the order they come, which
    /// doesn't depend on the width it's laid out at.
    pub fn links(self) -> Vec<Page> {
        self.laid_out(80).links
    }

    pub fn title(self) -> &'static str {
        t(match self {
            Page::Flow => "help.flow.title",
            Page::Keys => "help.keys.title",
            Page::History => "help.history.title",
            Page::Find => "help.find.title",
            Page::Palette => "help.palette.title",
            Page::Assistant => "help.assistant.title",
            Page::Settings => "help.settings.title",
            Page::Picker => "help.picker.title",
            Page::Gallery => "help.gallery.title",
            Page::Key => "help.key.title",
            Page::Model => "help.model.title",
            Page::About => "help.about.title",
            Page::Updates => "help.updates.title",
            Page::Shells => "help.shells.title",
            Page::Apps => "help.apps.title",
            Page::FlowCommand => "help.command.title",
            Page::Outside => "help.outside.title",
            Page::ServerAsk => "help.server_ask.title",
            Page::Legal => "help.legal.title",
        })
    }

    /// The page's text, in markdown, with Flow's keys in it as they're
    /// bound.
    pub fn body(self) -> String {
        let key = match self {
            Page::Keys => return keys_page(),
            Page::Flow => "help.flow.body",
            Page::History => "help.history.body",
            Page::Find => "help.find.body",
            Page::Palette => "help.palette.body",
            Page::Assistant => "help.assistant.body",
            Page::Settings => "help.settings.body",
            Page::Picker => "help.picker.body",
            Page::Gallery => "help.gallery.body",
            Page::Key => "help.key.body",
            Page::Model => "help.model.body",
            Page::About => "help.about.body",
            Page::Updates => "help.updates.body",
            Page::Shells => "help.shells.body",
            Page::Apps => "help.apps.body",
            Page::FlowCommand => "help.command.body",
            Page::Outside => "help.outside.body",
            Page::ServerAsk => "help.server_ask.body",
            Page::Legal => "help.legal.body",
        };
        with_keys(key)
    }
}

/// The words at `key` with the keys the pages name put in.
pub fn with_keys(key: &str) -> String {
    let shown = shown_keys();
    let args: Vec<(&str, &dyn Display)> = shown.iter().map(|(name, key)| (*name, key as &dyn Display)).collect();
    tf(key, &args)
}

/// Each `{name}` the pages hold and the key it is: Flow's keys as they're
/// bound, then the keys inside its screens, which aren't.
fn shown_keys() -> Vec<(&'static str, String)> {
    let bound = crate::keys::bound();
    let first = |name: &str| {
        let i = crate::keys::TABLE.iter().position(|e| e.name == name).expect("a key help names is in the table");
        match bound.all_of(i).first() {
            Some(_) if crate::keys::TABLE[i].command().is_none() => bound.label_at(i),
            Some(chord) => chord.label(bound.mac()),
            None => t("keys.none").to_string(),
        }
    };
    let mut shown: Vec<(&str, String)> = BOUND.iter().map(|&(name, entry)| (name, first(entry))).collect();
    shown.push(("pane_move", bound.held_label(crate::keys::Does::Panes)));
    shown.extend(FIXED.iter().map(|&(name, mac, other)| (name, keys(mac, other).to_string())));
    shown
}

/// Help's page of every key (`Page::Keys`), made from the table as the
/// keys are bound: each group's keys, a changed one with its default, then
/// the keys inside Flow's screens, which aren't bound.
fn keys_page() -> String {
    use crate::keys::{Bindings, Group, TABLE};
    let bound = crate::keys::bound();
    let mac = bound.mac();
    let mut page = with_keys("help.keys.intro");
    for group in Group::ALL {
        if !page.ends_with('\n') {
            page.push('\n');
        }
        page.push_str(&format!("\n# {}\n", group.words()));
        for (i, entry) in TABLE.iter().enumerate().filter(|(_, e)| e.group == group) {
            let keys = if bound.get(i).is_empty() { t("keys.none").to_string() } else { format!("`{}`", bound.label_at(i)) };
            let default = if bound.is_default(i) {
                String::new()
            } else {
                let was = Bindings::defaults(mac).label_at(i);
                format!(" ({})", tf("help.keys.default", &[("key", &format!("`{was}`"))]))
            };
            page.push_str(&format!("- **{}** {keys}{default}\n", entry.label()));
        }
    }
    page.push_str(&format!("\n# {}\n", t("help.keys.fixed_title")));
    page.push_str(&with_keys("help.keys.fixed"));
    page
}

/// The keys the pages name (`{history}`) that are Flow's keys, and their
/// names in `keys::TABLE`: shown as they're bound.
const BOUND: [(&str, &str); 27] = [
    ("help", "help"),
    ("history", "search_history"),
    ("find", "find"),
    ("find_next", "find_next"),
    ("find_previous", "find_previous"),
    ("scroll_bottom", "scroll_to_bottom"),
    ("scroll_top", "scroll_to_top"),
    ("previous_command", "previous_command"),
    ("next_command", "next_command"),
    ("palette", "palette"),
    ("record", "record"),
    ("split_right", "split_right"),
    ("split_down", "split_down"),
    ("settings", "settings"),
    ("assistant", "assistant"),
    ("new_tab", "new_tab"),
    ("close_tab", "close_tab"),
    ("pick_tab", "go_to_tab"),
    ("copy", "copy"),
    ("paste", "paste"),
    ("bigger", "bigger_text"),
    ("smaller", "smaller_text"),
    ("reset", "actual_size"),
    ("key", "change_key"),
    ("efficient", "efficient"),
    ("restart", "restart_tab"),
    ("next_tab", "next_tab"),
];

/// The keys the pages name that are inside Flow's own screens, which
/// aren't bound: on the Mac, then elsewhere.
const FIXED: [(&str, &str, &str); 18] = [
    ("find_pattern", "⌥⌘R", "Alt+R"),
    // The assistant's input (`panel_input`).
    ("input_rows", "⌘← ⌘→", "Home and End"),
    ("input_ends", "⌘↑ ⌘↓", "Ctrl+Home and Ctrl+End"),
    ("input_select_all", "⌘A", "Ctrl+A"),
    ("input_copy", "⌘C", "Ctrl+C"),
    ("input_cut", "⌘X", "Ctrl+X"),
    ("input_undo", "⌘Z", "Ctrl+Z"),
    ("input_redo", "⇧⌘Z", "Ctrl+Y"),
    ("new_conversation", "⌃L", "Ctrl+L"),
    ("dialog_paste", "⌘V", "Ctrl+V"),
    ("forget", "⌘⌫", "Shift+Delete"),
    ("forget_all", "⌘⇧⌫", "Ctrl+Shift+Delete"),
    ("jump", "⌘", "Ctrl"),
    ("word", "⌥", "Ctrl"),
    ("word_delete", "⌥⌫", "Ctrl+Backspace"),
    ("done", "⌘Enter", "Ctrl+Enter"),
    // Settings' Hotkey choices (`hotkey::CHOICES`), not F12 on Windows.
    ("hotkeys", "⌃`, ⌥Space, F12, ⌃Space", if cfg!(windows) { "Ctrl+`, Alt+Space, Ctrl+Space" } else { "Ctrl+`, Alt+Space, F12, Ctrl+Space" }),
    ("quit", "⌘Q", "Alt+F4"),
];

/// A page laid out to a width, its links to other pages read: each one's
/// words are spans of `Style::Link(i)`, `i` its page's place in `links`.
#[derive(Debug)]
pub struct Laid {
    pub lines: Vec<Line>,
    pub links: Vec<Page>,
}

impl Laid {
    /// Keeps the links that name a page. One that names none (a typo, a page
    /// gone) reads as its words alone, so it's never a link to nowhere.
    fn new(linked: text::Linked) -> Self {
        let mut links = Vec::new();
        let kept: Vec<Option<usize>> = linked
            .links
            .iter()
            .map(|target| {
                Page::linked(target).map(|page| {
                    links.push(page);
                    links.len() - 1
                })
            })
            .collect();
        let mut lines = linked.lines;
        for (_, style) in lines.iter_mut().flatten() {
            if let Style::Link(i) = *style {
                *style = kept[i].map_or(Style::Normal, Style::Link);
            }
        }
        Laid { lines, links }
    }
}

/// What `flow-help` prints in a tab, for its arguments: every page with
/// none, the pages named (`settings history`), their names and titles with
/// `--list`, or how it's used with `--help`; a page there's none of is an
/// error that names them all. Pages are laid out `cols` wide, as help lays
/// them out over the screen, and `styled` puts in SGR for bold, code, dim
/// words and links, which can't be followed in a terminal and are
/// underlined. A link alone on its line (Flow's own list of pages) says the
/// command that prints its page.
pub fn for_terminal(args: &[String], cols: usize, styled: bool) -> Result<String, String> {
    let mut out = String::new();
    let names = || Page::ALL.map(Page::name).join(", ");
    if args.iter().any(|a| a == "--help" || a == "-h") {
        let usage = [("flow-help", "flow_help.all"), ("flow-help <page>…", "flow_help.one"), ("flow-help --list", "flow_help.list")];
        for (command, words) in usage {
            push_line(&mut out, &[(format!("{command:<20}"), Style::Code), (t(words).to_string(), Style::Normal)], styled);
        }
        return Ok(out);
    }
    if args.iter().any(|a| a == "--list" || a == "-l") {
        for page in Page::ALL {
            push_line(&mut out, &[(format!("{:<12}", page.name()), Style::Code), (page.title().to_string(), Style::Normal)], styled);
        }
        return Ok(out);
    }
    let pages = if args.is_empty() {
        Page::ALL.to_vec()
    } else {
        args.iter().map(|a| Page::named(a).ok_or_else(|| tf("flow_help.unknown", &[("name", a), ("pages", &names())]))).collect::<Result<Vec<_>, _>>()?
    };
    for (i, page) in pages.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        print_page(*page, cols, styled, &mut out);
    }
    if args.is_empty() {
        out.push('\n');
        for line in text::wrap_plain(t("flow_help.more"), cols, Style::Dim) {
            push_line(&mut out, &line, styled);
        }
    }
    Ok(out)
}

/// `page` for a terminal: its title over a rule, then its text.
fn print_page(page: Page, cols: usize, styled: bool, out: &mut String) {
    use unicode_width::UnicodeWidthStr;
    let title = page.title();
    push_line(out, &[(title.to_string(), Style::Bold)], styled);
    push_line(out, &[("─".repeat(title.width().max(1)), Style::Dim)], styled);
    let laid = page.laid_out(cols);
    // The link a line holds alone, beside its bullet.
    let alone = |line: &Line| match line.iter().filter(|(_, style)| *style != Style::Dim).map(|(_, style)| *style).collect::<Vec<_>>()[..] {
        [Style::Link(i)] => Some(i),
        _ => None,
    };
    let width = |line: &Line| line.iter().map(|(words, _)| words.width()).sum::<usize>();
    // Their commands in a column.
    let widest = laid.lines.iter().filter(|line| alone(line).is_some()).map(width).max().unwrap_or(0);
    for line in &laid.lines {
        match alone(line) {
            Some(i) => {
                let mut line = line.clone();
                line.push((" ".repeat(widest - width(&line) + 2), Style::Normal));
                line.push((format!("flow-help {}", laid.links[i].name()), Style::Dim));
                push_line(out, &line, styled);
            }
            None => push_line(out, line, styled),
        }
    }
}

/// One line of spans, with their styles as SGR when `styled`.
fn push_line(out: &mut String, line: &[(String, Style)], styled: bool) {
    let mut text = String::new();
    for (words, style) in line {
        let sgr = match style {
            Style::Normal => "",
            Style::Bold => "\x1b[1m",
            Style::Code => "\x1b[36m",
            Style::Dim => "\x1b[2m",
            Style::Link(_) => "\x1b[4m",
        };
        if styled && !sgr.is_empty() {
            text.push_str(&format!("{sgr}{words}\x1b[0m"));
        } else {
            text.push_str(words);
        }
    }
    out.push_str(text.trim_end());
    out.push('\n');
}

/// A page of help open over the screen it's about, with the pages its
/// links were followed from.
#[derive(Debug)]
pub struct Help {
    pub page: Page,
    /// The first line shown; the drawing keeps it within the page.
    pub top: Cell<usize>,
    /// The link Tab chose, which Enter follows. The drawing lets it go once
    /// it's scrolled out of sight, so Enter never follows a link not shown.
    pub chosen: Cell<Option<usize>>,
    /// Set when Tab chooses a link, for the drawing to scroll it into view
    /// once (only the drawing knows the lines); the wheel may then scroll
    /// it away.
    pub reveal: Cell<bool>,
    /// The first and last links the drawing showed, where Tab starts.
    pub shown: Cell<Option<(usize, usize)>>,
    /// The pages followed from, the latest last, each where it was
    /// scrolled to and its chosen link: Esc goes back through them.
    back: Vec<(Page, usize, Option<usize>)>,
}

impl Help {
    pub fn new(page: Page) -> Self {
        Help { page, top: Cell::new(0), chosen: Cell::new(None), reveal: Cell::new(false), shown: Cell::new(None), back: Vec::new() }
    }

    /// Scrolls by `lines` (up when negative); the drawing stops it at the
    /// page's end.
    pub fn scroll(&self, lines: isize) {
        self.top.set(self.top.get().saturating_add_signed(lines));
    }

    /// Opens `page` in place of this one, which Esc comes back to.
    pub fn follow(&mut self, page: Page) {
        self.back.push((self.page, self.top.get(), self.chosen.get()));
        *self = Help { back: std::mem::take(&mut self.back), ..Help::new(page) };
    }

    /// Whether a link was followed to this page, so there's one to go back to.
    pub fn can_go_back(&self) -> bool {
        !self.back.is_empty()
    }

    /// Goes back to the page a link was followed from, as it was left;
    /// false on the first page, which has none.
    pub fn back(&mut self) -> bool {
        let Some((page, top, chosen)) = self.back.pop() else { return false };
        *self = Help { top: Cell::new(top), chosen: Cell::new(chosen), back: std::mem::take(&mut self.back), ..Help::new(page) };
        true
    }

    /// Moves the choice to the next of the page's `count` links (the one
    /// before, with `forward` false), round from the last to the first.
    /// With none chosen, Tab takes the first link shown and Shift+Tab the
    /// last, so it starts where the reader is.
    pub fn choose(&self, forward: bool, count: usize) {
        if count == 0 {
            return;
        }
        let next = match (self.chosen.get(), self.shown.get()) {
            (Some(i), _) if forward => (i + 1) % count,
            (Some(i), _) => (i + count - 1) % count,
            (None, Some((first, last))) => if forward { first } else { last },
            (None, None) => if forward { 0 } else { count - 1 },
        };
        self.chosen.set(Some(next.min(count - 1)));
        self.reveal.set(true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f1_opens_the_page_of_the_screen_in_front_and_none_over_the_terminal() {
        let front = |f: fn(&mut Front)| {
            let mut front = Front::default();
            f(&mut front);
            Page::in_front(front)
        };
        assert_eq!(front(|_| {}), None, "F1 is the program's");
        assert_eq!(front(|f| f.assistant = true), Some(Page::Assistant));
        assert_eq!(front(|f| f.settings = true), Some(Page::Settings));
        assert_eq!(front(|f| (f.settings, f.picker) = (true, true)), Some(Page::Picker));
        assert_eq!(front(|f| (f.history, f.assistant) = (true, true)), Some(Page::History));
        assert_eq!(front(|f| (f.model_ask, f.settings) = (true, true)), Some(Page::Model));
        assert_eq!(front(|f| (f.key_dialog, f.model_ask) = (true, true)), Some(Page::Key), "the key dialog takes keys before the question");
        assert_eq!(front(|f| (f.about, f.history) = (true, true)), Some(Page::About));
        assert_eq!(front(|f| (f.update_ask, f.about) = (true, true)), Some(Page::Updates));
        assert_eq!(front(|f| (f.shells, f.assistant) = (true, true)), Some(Page::Shells));
        assert_eq!(front(|f| (f.find, f.assistant) = (true, true)), Some(Page::Find), "the find bar takes keys before the assistant");
        assert_eq!(front(|f| (f.settings, f.find) = (true, true)), Some(Page::Settings));
        assert_eq!(front(|f| (f.palette, f.history) = (true, true)), Some(Page::Palette), "the palette takes keys first of the panels");
    }

    #[test]
    fn every_page_has_its_words_with_the_keys_filled_in_in_every_language() {
        for code in ["en", "ja"] {
            crate::i18n::set_language(code, &[]);
            for page in Page::ALL {
                let (title, body) = (page.title(), page.body());
                // A key no language has comes back as itself.
                assert!(!title.ends_with(".title") && body.len() > 80, "{code} {page:?}");
                for (name, _) in shown_keys() {
                    assert!(!body.contains(&format!("{{{name}}}")), "{code} {page:?} leaves {{{name}}}");
                }
            }
        }
    }

    #[test]
    fn every_link_in_every_page_names_a_page_in_every_language() {
        for code in ["en", "ja"] {
            crate::i18n::set_language(code, &[]);
            for page in Page::ALL {
                let linked = text::layout_linked(&page.body(), 80);
                for target in &linked.links {
                    assert!(Page::linked(target).is_some(), "{code} {page:?} links to {target}");
                }
                assert_eq!(page.links().len(), linked.links.len(), "{code} {page:?}");
                assert!(!page.links().contains(&page), "{code} {page:?} links to itself");
            }
        }
    }

    #[test]
    fn flows_own_page_links_to_every_other_page_in_every_language() {
        for code in ["en", "ja"] {
            crate::i18n::set_language(code, &[]);
            let links = Page::Flow.links();
            for page in Page::ALL.into_iter().skip(1) {
                assert!(links.contains(&page), "{code}: Flow's page has no link to {page:?}");
            }
            // The list of pages names each by its title.
            let laid = Page::Flow.laid_out(200);
            let words: Vec<String> = laid.lines.iter().flatten().filter(|(_, style)| matches!(style, Style::Link(_))).map(|(words, _)| words.clone()).collect();
            for page in Page::ALL.into_iter().skip(1) {
                assert!(words.iter().any(|w| w == page.title()), "{code}: no link says {}", page.title());
            }
        }
    }

    #[test]
    fn a_link_to_no_page_reads_as_its_words() {
        let laid = Laid::new(text::layout_linked("[Settings](help:settings), [nowhere](help:nowhere), [web](https://example.com) and [About](help:about)", 200));
        assert_eq!(laid.links, [Page::Settings, Page::About]);
        let styles: Vec<_> = laid.lines[0].iter().map(|(words, style)| (words.as_str(), *style)).collect();
        assert_eq!(styles[0], ("Settings", Style::Link(0)));
        assert!(styles.iter().any(|&(words, style)| words.contains("nowhere") && style == Style::Normal));
        assert!(styles.iter().any(|&(words, style)| words.contains("web") && style == Style::Normal));
        assert_eq!(styles.last(), Some(&("About", Style::Link(1))));
        assert_eq!(Page::linked("settings"), None, "a link names a page as help:<name>");
    }

    fn flow_help(args: &[&str], cols: usize, styled: bool) -> Result<String, String> {
        for_terminal(&args.iter().map(|a| a.to_string()).collect::<Vec<_>>(), cols, styled)
    }

    #[test]
    fn flow_help_prints_every_page_within_the_width_and_without_markup() {
        use unicode_width::UnicodeWidthStr;
        for code in ["en", "ja"] {
            crate::i18n::set_language(code, &[]);
            let out = flow_help(&[], 60, false).unwrap();
            for page in Page::ALL {
                assert!(out.lines().any(|line| line == page.title()), "{code}: no title {}", page.title());
            }
            assert!(!out.contains("](help:") && !out.contains("**") && !out.contains('\x1b'), "{code}");
            for line in out.lines() {
                assert!(line.width() <= 60, "{code}: {line}");
            }
            assert!(out.trim_end().ends_with(t("flow_help.more").rsplit(' ').next().unwrap()), "{code}: ends saying how to print one page");
        }
    }

    #[test]
    fn flow_help_prints_the_pages_named_lists_them_and_refuses_one_there_isnt() {
        crate::i18n::set_language("en", &[]);
        let mut one = String::new();
        print_page(Page::Settings, 80, false, &mut one);
        assert_eq!(flow_help(&["settings"], 80, false).unwrap(), one);
        let two = flow_help(&["picker", "key"], 80, false).unwrap();
        assert!(two.starts_with(Page::Picker.title()) && two.contains(&format!("\n\n{}\n", Page::Key.title())));
        let list = flow_help(&["--list"], 80, false).unwrap();
        assert_eq!(list.lines().count(), Page::ALL.len());
        assert!(list.lines().zip(Page::ALL).all(|(line, page)| line.starts_with(page.name()) && line.ends_with(page.title())));
        let err = flow_help(&["settings", "nowhere"], 80, false).unwrap_err();
        assert!(err.contains("\"nowhere\"") && err.contains("flow, keys, history"), "{err}");
        assert_eq!(flow_help(&["--help"], 80, false).unwrap().lines().count(), 3);
        // Flow's own list of pages says the command that prints each.
        let flow = flow_help(&["flow"], 80, false).unwrap();
        for page in Page::ALL.into_iter().skip(1) {
            assert!(flow.lines().any(|line| line.contains(page.title()) && line.ends_with(&format!("flow-help {}", page.name()))), "{page:?}");
        }
    }

    #[test]
    fn flow_help_styles_its_words_only_when_asked() {
        crate::i18n::set_language("en", &[]);
        let history = flow_help(&["history"], 80, true).unwrap();
        assert!(history.starts_with(&format!("\x1b[1m{}\x1b[0m\n\x1b[2m─", Page::History.title())));
        assert!(history.contains("\x1b[36muil\x1b[0m"), "code in its color");
        assert!(flow_help(&["flow"], 80, true).unwrap().contains(&format!("\x1b[4m{}\x1b[0m", Page::Settings.title())), "links underlined");
    }

    #[test]
    fn following_a_link_and_going_back_returns_to_the_page_as_it_was() {
        let mut help = Help::new(Page::Flow);
        assert!(!help.back(), "the first page has none before it");
        help.top.set(7);
        help.chosen.set(Some(2));
        help.follow(Page::Settings);
        assert_eq!((help.page, help.top.get(), help.chosen.get()), (Page::Settings, 0, None));
        help.follow(Page::Picker);
        assert!(help.back());
        assert_eq!(help.page, Page::Settings);
        assert!(help.back());
        assert_eq!((help.page, help.top.get(), help.chosen.get()), (Page::Flow, 7, Some(2)));
        assert!(!help.back());
    }

    #[test]
    fn tab_goes_round_the_links_starting_from_those_shown() {
        let help = Help::new(Page::Flow);
        help.choose(true, 0);
        assert_eq!(help.chosen.get(), None, "a page without links has nothing to choose");
        help.shown.set(Some((3, 5)));
        help.choose(true, 8);
        assert_eq!(help.chosen.get(), Some(3));
        assert!(help.reveal.get());
        help.choose(false, 8);
        help.choose(false, 8);
        help.choose(false, 8);
        assert_eq!(help.chosen.get(), Some(0));
        help.choose(false, 8);
        assert_eq!(help.chosen.get(), Some(7), "Shift+Tab goes round to the last");
        help.choose(true, 8);
        assert_eq!(help.chosen.get(), Some(0));
        let help = Help::new(Page::Flow);
        help.shown.set(Some((3, 5)));
        help.choose(false, 8);
        assert_eq!(help.chosen.get(), Some(5), "Shift+Tab starts at the last link shown");
    }

    #[test]
    fn the_keys_page_lists_every_key_as_its_bound_with_a_changed_ones_default() {
        use crate::keys::{Bindings, Chord, Keys, TABLE};
        crate::i18n::set_language("en", &[]);
        let mut bound = Bindings::defaults(true);
        let new_tab = TABLE.iter().position(|e| e.name == "new_tab").unwrap();
        bound.put(new_tab, Keys::Chords(vec![Chord::parse("cmd+n").unwrap()]));
        crate::keys::set(bound);
        let page = Page::Keys.body();
        for entry in TABLE {
            assert!(page.contains(&format!("**{}**", entry.label())), "{} isn't listed", entry.name);
        }
        assert!(page.contains("**New Tab** `⌘N` (default `⌘T`)"), "{page}");
        assert!(page.contains("**Find…** `⌘F`\n") || page.contains("`⌘F`\n"), "{page}");
        // Help's other pages say the key as it's bound, too.
        assert!(Page::Flow.body().contains("⌘N"));
        crate::keys::set(Bindings::defaults(true));
    }
}
