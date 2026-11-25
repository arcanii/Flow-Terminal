//! Find in the scrollback: the text a tab's terminal holds, its screen and
//! everything scrolled off it, searched for what's typed in Flow's find bar
//! (⌘F, Ctrl+Shift+F), every match lit and one of them current, which ↑ and
//! ↓ move between and the view scrolls to.
//!
//! alacritty_terminal's own search does the finding, over the grid as its
//! rows wrap, so a match running on into the next row of a wrapped line is
//! found whole. What's typed is looked for as written, or, with the bar's
//! pattern switch on (`.*`, ⌥⌘R or Alt+R), as a regular expression, and
//! either way without regard to case unless it holds a capital letter, as
//! alacritty's search does. A pattern matching nothing but empty text finds
//! nothing, since alacritty's search passes over empty matches.

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Direction, Point};
use alacritty_terminal::term::search::{Match, RegexIter, RegexSearch};
use alacritty_terminal::term::Term;

/// The most matches kept; past it the bar says there are more.
pub const MOST: usize = 10_000;

/// `text` as a pattern matching it as written.
pub fn literal(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 2);
    for c in text.chars() {
        if "\\.+*?()|[]{}^$#&-~".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Every match of `query` in `term`, top to bottom, at most [`MOST`], and
/// whether there were more: `query` as written, or as a regular expression
/// where `pattern`. None for a pattern that isn't one.
pub fn matches<T>(term: &Term<T>, query: &str, pattern: bool) -> Option<(Vec<Match>, bool)> {
    if query.is_empty() {
        return Some((Vec::new(), false));
    }
    let search = if pattern { RegexSearch::new(query) } else { RegexSearch::new(&literal(query)) };
    let mut regex = match search {
        Ok(regex) => regex,
        Err(_) if pattern => return None,
        Err(_) => return Some((Vec::new(), false)),
    };
    let start = Point::new(term.topmost_line(), Column(0));
    let end = Point::new(term.bottommost_line(), term.last_column());
    let mut found: Vec<Match> = RegexIter::new(start, end, Direction::Right, term, &mut regex).take(MOST + 1).collect();
    let more = found.len() > MOST;
    found.truncate(MOST);
    Some((found, more))
}

/// Where a point is counted from the top of the scrollback, which stays
/// put as new lines push the screen up (until the scrollback is full and
/// its oldest go), unlike the grid's own lines.
fn absolute<T>(term: &Term<T>, point: Point) -> (i64, usize) {
    (term.history_size() as i64 + i64::from(point.line.0), point.column.0)
}

/// What the find bar holds for a tab.
#[derive(Debug, Default)]
pub struct Find {
    pub query: String,
    pub matches: Vec<Match>,
    /// The match chosen, which ↑ and ↓ move between.
    pub current: Option<usize>,
    /// More matched than [`MOST`].
    pub more: bool,
    /// Where the current match starts, counted from the top of the
    /// scrollback, so it stays chosen as output comes.
    current_at: Option<(i64, usize)>,
    /// The bar has the keys; a click in the terminal gives them back to it,
    /// the bar staying open with its matches lit.
    pub focus: bool,
    /// What's typed is a regular expression, not text as written.
    pub pattern: bool,
    /// The pattern typed isn't one, so nothing is looked for.
    pub bad: bool,
}

impl Find {
    /// A find bar looking for `query`, with the keys.
    pub fn new(query: String) -> Self {
        Find { query, focus: true, ..Find::default() }
    }

    /// Searches `term` for the query. `fresh` (a new query, another tab)
    /// chooses the newest match, the nearest the prompt; otherwise the
    /// current one stays chosen where it still matches, else the one
    /// nearest where it was.
    pub fn search<T>(&mut self, term: &Term<T>, fresh: bool) {
        let found = matches(term, &self.query, self.pattern);
        self.bad = found.is_none();
        (self.matches, self.more) = found.unwrap_or_default();
        let at = self.current_at.filter(|_| !fresh);
        self.current = match at {
            _ if self.matches.is_empty() => None,
            None => Some(self.matches.len() - 1),
            Some(at) => {
                let starts: Vec<(i64, usize)> = self.matches.iter().map(|m| absolute(term, *m.start())).collect();
                Some(starts.iter().position(|s| *s >= at).unwrap_or(starts.len() - 1))
            }
        };
        self.current_at = self.current.map(|i| absolute(term, *self.matches[i].start()));
    }

    /// Moves to the match before (`older`, up) or after, round to the
    /// other end past the first or the last.
    pub fn step<T>(&mut self, term: &Term<T>, older: bool) {
        let n = self.matches.len();
        if n == 0 {
            return;
        }
        let i = self.current.unwrap_or(n - 1);
        let next = if older { (i + n - 1) % n } else { (i + 1) % n };
        self.current = Some(next);
        self.current_at = Some(absolute(term, *self.matches[next].start()));
    }

    /// The match chosen.
    pub fn current_match(&self) -> Option<&Match> {
        self.matches.get(self.current?)
    }

    /// The matches on lines `top..=bottom` of the grid (the screen as it's
    /// scrolled), each with whether it's the one chosen.
    pub fn shown(&self, top: i32, bottom: i32) -> Vec<(Match, bool)> {
        self.matches
            .iter()
            .enumerate()
            .filter(|(_, m)| m.end().line.0 >= top && m.start().line.0 <= bottom)
            .map(|(i, m)| (m.clone(), Some(i) == self.current))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::event::VoidListener;
    use alacritty_terminal::index::Line;
    use alacritty_terminal::term::Config;
    use alacritty_terminal::vte::ansi::{Processor, StdSyncHandler};

    struct Size(usize, usize);

    impl Dimensions for Size {
        fn total_lines(&self) -> usize {
            self.1
        }
        fn screen_lines(&self) -> usize {
            self.1
        }
        fn columns(&self) -> usize {
            self.0
        }
    }

    /// A terminal `cols` wide and `rows` tall, with scrollback, given `text`.
    fn term(cols: usize, rows: usize, text: &str) -> Term<VoidListener> {
        let mut term = Term::new(Config { scrolling_history: 1000, ..Config::default() }, &Size(cols, rows), VoidListener);
        let mut parser: Processor<StdSyncHandler> = Processor::new();
        parser.advance(&mut term, text.replace('\n', "\r\n").as_bytes());
        term
    }

    #[test]
    fn what_is_typed_is_looked_for_as_written() {
        assert_eq!(literal("a.b*c"), "a\\.b\\*c");
        assert_eq!(literal("[x] (y) $z ^ |"), "\\[x\\] \\(y\\) \\$z \\^ \\|");
        let t = term(30, 4, "cost: $5.00 (gross)\ncost: 5x00\n");
        assert_eq!(matches(&t, "$5.00", false).unwrap().0.len(), 1, "the dot is a dot, not any letter");
        assert_eq!(matches(&t, "(gross)", false).unwrap().0.len(), 1);
    }

    #[test]
    fn matches_are_found_in_the_scrollback_and_on_the_screen() {
        let lines: String = (0..30).map(|i| format!("line {i} Error\n")).collect();
        let t = term(20, 5, &lines);
        let (found, more) = matches(&t, "error", false).unwrap();
        assert_eq!((found.len(), more), (30, false), "every one, whatever its case");
        assert!(found[0].start().line < Line(0), "the first is in the scrollback");
        assert_eq!(matches(&t, "Error", false).unwrap().0.len(), 30);
        assert!(matches(&t, "ERROR", false).unwrap().0.is_empty(), "a capital asks for its case");
        assert!(matches(&t, "", false).unwrap().0.is_empty());
    }

    #[test]
    fn a_match_across_a_wrapped_row_is_found_whole() {
        // Ten columns: "0123456789" fills the row and "abc" wraps under it.
        let t = term(10, 4, "0123456789abc\n");
        let found = matches(&t, "89ab", false).unwrap().0;
        assert_eq!(found.len(), 1);
        assert_eq!((found[0].start().line, found[0].end().line), (Line(0), Line(1)));
    }

    #[test]
    fn with_the_pattern_switch_on_what_is_typed_is_a_regular_expression() {
        let t = term(40, 6, "build 12 took 3.5 s\nbuild 7 failed\ncost: $5x00\n");
        let found = |q: &str| matches(&t, q, true).map(|(m, _)| m.len());
        assert_eq!(found(r"build \d+"), Some(2));
        assert_eq!(found(r"took|failed"), Some(2));
        assert_eq!(found("$5.00"), Some(0), "a $ is the line's end, not a dollar");
        assert_eq!(found(r"\$5.00"), Some(1), "and the dot any letter");
        assert_eq!(found("BUILD"), Some(0), "a capital still asks for its case");
        assert_eq!(found("(unclosed"), None, "not a pattern");
        assert_eq!(found("x*"), Some(1), "empty matches are passed over");
        assert_eq!(matches(&t, "(unclosed", false).map(|(m, _)| m.len()), Some(0), "as written, it's only text");
        let mut f = Find { query: "(unclosed".into(), pattern: true, ..Find::default() };
        f.search(&t, true);
        assert!(f.bad && f.matches.is_empty() && f.current.is_none());
        f.query = "fail(ed)?".into();
        f.search(&t, true);
        assert!(!f.bad && f.matches.len() == 1);
    }

    #[test]
    fn the_newest_match_is_chosen_and_the_arrows_go_round() {
        let t = term(20, 4, "one x\ntwo x\nthree x\n");
        let mut f = Find { query: "x".into(), ..Find::default() };
        f.search(&t, true);
        assert_eq!((f.matches.len(), f.current), (3, Some(2)), "the newest, nearest the prompt");
        f.step(&t, true);
        f.step(&t, true);
        assert_eq!(f.current, Some(0));
        f.step(&t, true);
        assert_eq!(f.current, Some(2), "round to the newest past the oldest");
        f.step(&t, false);
        assert_eq!(f.current, Some(0), "and back round");
    }

    #[test]
    fn the_match_chosen_stays_chosen_as_output_comes() {
        let mut t = term(20, 4, "a x\nb x\nc x\n");
        let mut f = Find { query: "x".into(), ..Find::default() };
        f.search(&t, true);
        f.step(&t, true);
        let chosen = f.current;
        // More lines, scrolling the screen up and adding matches below.
        let mut parser: Processor<StdSyncHandler> = Processor::new();
        parser.advance(&mut t, b"d x\r\ne x\r\nf x\r\n");
        f.search(&t, false);
        assert_eq!((f.matches.len(), f.current), (6, chosen), "still b's");
        f.search(&t, true);
        assert_eq!(f.current, Some(5), "a new search chooses the newest");
    }

    #[test]
    fn only_the_matches_in_view_are_drawn() {
        let lines: String = (0..10).map(|i| format!("{i} x\n")).collect();
        let t = term(20, 4, &lines);
        let mut f = Find { query: "x".into(), ..Find::default() };
        f.search(&t, true);
        let shown = f.shown(0, 3);
        assert!(shown.iter().all(|(m, _)| m.start().line.0 >= 0 && m.start().line.0 <= 3));
        assert_eq!(shown.iter().filter(|(_, current)| *current).count(), 1, "the newest is on the screen, and chosen");
    }

    #[test]
    fn past_the_most_the_bar_says_there_are_more() {
        let few = term(80, 4, &"x".repeat(80 * 3));
        assert!(!matches(&few, "x", false).unwrap().1);
        let many = term(200, 60, &"x".repeat(MOST + 50));
        let (found, more) = matches(&many, "x", false).unwrap();
        assert_eq!((found.len(), more), (MOST, true));
    }
}
