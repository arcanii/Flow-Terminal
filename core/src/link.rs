//! Links programs write with OSC 8 (`ESC ] 8 ; ; <uri> ST`, words,
//! `ESC ] 8 ; ; ST`), which alacritty_terminal keeps on each cell under
//! them: `flowapp`'s app names, a compiler's notes, a build's report, the
//! names `ls` and `tree` list. With the pointer over one Flow underlines it
//! and shows where it goes, and ⌘-click (Ctrl+click on Windows and Linux)
//! follows it (`App::link_under`, `App::open_link`), even where a program
//! takes the mouse. An address no program marked, in the text itself (a
//! web, mail or `file:` one, [`plain`]), is a link the same way.
//!
//! Web and mail links open. A program can put any words over a link, so a
//! `file:` link dressed as a web page could start a program if it were
//! opened: a file on this computer is only shown in its folder (the
//! Finder, Explorer, the file manager) with nothing run ([`Target::Reveal`]).
//! A path on another computer isn't one ([`on_this_computer`]): on Windows,
//! merely looking at `\\host\share` hands that host the user's sign-in.
//! Other schemes stay plain text.

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Boundary, Column, Direction, Point};
use alacritty_terminal::term::cell::{Flags, Hyperlink};
use alacritty_terminal::term::search::{Match, RegexIter, RegexSearch};
use alacritty_terminal::term::Term;

/// The longest link Flow opens, about as long as browsers take them.
const LONGEST: usize = 2048;

/// Whether Flow opens `uri`: a web address ([`web_address`]) or a mail
/// one (`mail_address`), no longer than `LONGEST`.
pub fn openable(uri: &str) -> bool {
    (web_address(uri) || mail_address(uri)) && uri.len() <= LONGEST
}

/// Whether `uri` is a mailto address written in what an address may hold
/// (`address_letters`), whose fields are only those a mail's own are
/// (`MAIL_FIELDS`): a mail app may attach the file an `attach=` names, which
/// on Windows a path to another computer reaches, so a link could hand it
/// the user's sign-in.
fn mail_address(uri: &str) -> bool {
    let Some((scheme, rest)) = uri.split_once(':') else { return false };
    let fields = rest.split_once('?').map_or("", |(_, fields)| fields);
    let known = |field: &str| {
        let name = field.split_once('=').map_or(field, |(name, _)| name);
        percent_decoded(name).is_some_and(|name| MAIL_FIELDS.iter().any(|known| name.eq_ignore_ascii_case(known)))
    };
    scheme.eq_ignore_ascii_case("mailto") && !rest.is_empty() && address_letters(uri) && fields.split('&').filter(|f| !f.is_empty()).all(known)
}

/// The fields a mailto link may fill in (RFC 6068's, less what a mail app
/// might read as more than words).
const MAIL_FIELDS: &[&str] = &["to", "cc", "bcc", "subject", "body", "in-reply-to"];

/// Whether `url` is an http or https address with a host, written in what
/// an address may hold (`address_letters`), however long: a server's
/// page to sign in on is one too, and may be longer than a link a program
/// prints.
pub fn web_address(url: &str) -> bool {
    let Some((scheme, rest)) = url.split_once(':') else { return false };
    let Some(after) = rest.strip_prefix("//") else { return false };
    let host = &after[..after.find(['/', '?', '#']).unwrap_or(after.len())];
    (scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https")) && !host.is_empty() && address_letters(url)
}

/// Whether `url` may go to the system's opener (`Platform::open_url`): a
/// web address, a mail one (`mail_address`), or a `file:` URL with no host
/// (or `localhost`), each written in what an address may hold. Anything
/// else, a bare path above all, Windows' opener would run as a program,
/// and `file://host/…` names another computer.
///
/// The `file:` URLs are trusted: only Flow makes them, for its own files
/// (`ai::memory::file_url`: the memory file, the instructions, the skills'
/// folder, the servers file), never from a link, and they pass whatever
/// path they hold, `file:////server/share/…` too, since a config folder
/// redirected to a server is the user's own. A link a program wrote goes
/// through [`target`], whose [`openable`] takes no `file:` URL.
pub fn for_the_opener(url: &str) -> bool {
    let Some((scheme, rest)) = url.split_once(':') else { return false };
    let own = scheme.eq_ignore_ascii_case("file")
        && rest.strip_prefix("//").and_then(|r| r.split_once('/')).is_some_and(|(host, _)| host.is_empty() || host.eq_ignore_ascii_case("localhost"));
    web_address(url) || mail_address(url) || (own && address_letters(url))
}

/// Whether `uri` holds none of what no address has (RFC 3986): spaces,
/// control characters, quotes, angle brackets, braces, `|`, `^`, `` ` ``
/// or a backslash. The system's opener, or the browser it hands an
/// address to on a command line of its own, could take one of those for
/// the address's end and what follows for more (Windows' Explorer, which
/// Flow opened addresses with before 0.1.46, could read a quote and the
/// commas after it as its own switches), and a browser reads a backslash
/// in a host as a slash, so the host shown wouldn't be the one opened.
/// Letters past ASCII stay, as Japanese text writes an address.
fn address_letters(uri: &str) -> bool {
    !uri.chars().any(|c| c.is_whitespace() || c.is_control() || matches!(c, '"' | '<' | '>' | '\\' | '^' | '`' | '{' | '|' | '}'))
}

/// `uri` with what an address may not hold as it is, but a link may well
/// (`"`, `<`, `>`, `^`, `` ` ``, `{`, `|`, `}`: `…?family=Roboto|Open+Sans`),
/// written `%XX`, as a browser sends them: so such a link still opens, and
/// none of them reaches the opener. A space, a control character or a
/// backslash stays, which `address_letters` refuses.
fn escaped(uri: &str) -> String {
    let mut out = String::with_capacity(uri.len());
    for c in uri.chars() {
        if matches!(c, '"' | '<' | '>' | '^' | '`' | '{' | '|' | '}') {
            out.push_str(&format!("%{:02X}", c as u32));
        } else {
            out.push(c);
        }
    }
    out
}

/// What following a link does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// Opens a web or mail address ([`openable`]).
    Open(String),
    /// Shows a file on this computer in its folder, selected, running
    /// nothing.
    Reveal(std::path::PathBuf),
}

/// What following `uri` does, if Flow follows it: a web or mail address
/// opens, what it may not hold as it is written `%XX` (`escaped`); a
/// `file:` URL on this computer (no host, or `localhost`) is revealed, its
/// path's `%XX` read back as bytes (and on Windows its drive's leading
/// slash dropped); anything else, a path with control characters, or one
/// [`on_this_computer`] says is another computer's (`file:////host/share`,
/// `file://localhost//host/share`), isn't followed.
pub fn target(uri: &str) -> Option<Target> {
    let address = escaped(uri);
    if openable(&address) {
        return Some(Target::Open(address));
    }
    file_path(uri, cfg!(windows)).map(|path| Target::Reveal(path.into()))
}

/// The path a `file:` URL names on this computer, if it names one there
/// (as [`target`] says), as Windows reads it or as the other systems do,
/// so both are tested on any of them.
fn file_path(uri: &str, windows: bool) -> Option<String> {
    if uri.len() > LONGEST {
        return None;
    }
    let (scheme, rest) = uri.split_once(':')?;
    if !scheme.eq_ignore_ascii_case("file") {
        return None;
    }
    let rest = rest.strip_prefix("//")?;
    let slash = rest.find('/')?;
    let (host, path) = rest.split_at(slash);
    if !(host.is_empty() || host.eq_ignore_ascii_case("localhost")) {
        return None;
    }
    let path = percent_decoded(path)?;
    if path.chars().any(char::is_control) {
        return None;
    }
    // file:///C:/Users/… on Windows is C:/Users/….
    let path = match path.as_bytes() {
        [b'/', drive, b':', ..] if windows && drive.is_ascii_alphabetic() => path[1..].to_string(),
        _ => path,
    };
    local(&path, windows).then_some(path)
}

/// Whether `path`, one a program wrote (a `file:` link's, the folder its
/// shell says it's in, a 3D model's file, one the assistant was led to),
/// is on this computer, so Flow may look at it before the user has said
/// yes to anything. Never one that starts with two separators, `/` or `\`
/// either way round (`//host/share`, `\\host\share`, `\\?\UNC\…`,
/// `\\.\pipe\…`): on Windows that's a device's path, or another
/// computer's, reached over SMB with the user's own sign-in, so merely
/// looking hands its server the user's NTLM hash; elsewhere no real path
/// needs two. On Windows only a drive's own path (`C:\…`) is, since
/// `\??\UNC\…` reaches another computer too, and a path from no drive
/// goes by Flow's own folder; or one written verbatim as
/// `std::fs::canonicalize` gives it, `\\?\C:\…` exactly, in backslashes,
/// with no `.` or `..` in it. Windows hands on as written only a path
/// that starts `\\?\` so: any other `?` or `.` start (`//?/`, `\\?/`,
/// `/\?\`) it tidies up as a device's path, whose `..` climbs from the
/// drive, so `//?/C:/../UNC/host/share` is `\\host\share`.
pub fn on_this_computer(path: &std::path::Path) -> bool {
    local(&path.to_string_lossy(), cfg!(windows))
}

/// [`on_this_computer`] for a path as text, as Windows reads one or as
/// the other systems do, so both are tested on any of them.
fn local(path: &str, windows: bool) -> bool {
    let separator = |b: &u8| matches!(b, b'/' | b'\\');
    let drive = |path: &[u8]| matches!(path, [drive, b':', after, ..] if drive.is_ascii_alphabetic() && separator(after));
    match path.as_bytes() {
        [b'\\', b'\\', b'?', b'\\', verbatim @ ..] if windows => {
            matches!(verbatim, [drive, b':', b'\\', ..] if drive.is_ascii_alphabetic())
                && !verbatim.split(separator).any(|part| part == b"." || part == b"..")
        }
        path if windows => drive(path),
        [first, second, ..] => !(separator(first) && separator(second)),
        _ => true,
    }
}

/// `text` with each `%XX` as its byte, if that's UTF-8.
fn percent_decoded(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// A link under the pointer.
#[derive(Debug, Clone, PartialEq)]
pub enum Link {
    /// One a program marked (OSC 8), on every cell under it.
    Marked(Hyperlink),
    /// An address found in the text ([`plain`]), on the cells it was found
    /// in.
    Plain { uri: String, cells: Match },
}

impl Link {
    pub fn uri(&self) -> &str {
        match self {
            Link::Marked(link) => link.uri(),
            Link::Plain { uri, .. } => uri,
        }
    }

    /// Whether the cell at `point`, linked to `cell` where a program linked
    /// it, is part of it: one a program marked with it, or one of the cells
    /// its address was found in.
    pub fn covers(&self, cell: Option<&Hyperlink>, point: Point) -> bool {
        match self {
            Link::Marked(link) => cell == Some(link),
            Link::Plain { cells, .. } => cells.contains(&point),
        }
    }
}

/// What an address in plain text looks like: a web, mail or `file:` one,
/// up to a space, a control character, a picture's cell, or one of the
/// characters that set one off in text (quotes, angle brackets, braces).
/// All lowercase, since alacritty's search heeds case once a pattern holds
/// a capital, and an address's scheme may be written in either.
const ADDRESS: &str = r#"(https?://|mailto:|file://)[^\x{0000}-\x{001f}\x{007f}-\x{009f}\x{10eeee}\s<>"'`{}\^|\\]+"#;

/// The search [`plain`] goes by, made once.
pub fn addresses() -> RegexSearch {
    RegexSearch::new(ADDRESS).expect("the address pattern is one")
}

/// The address in plain text under `point`, found with `regex` (made by
/// [`addresses`]) on the line it's on, its wrapped rows and all: a cell a
/// program linked is the program's ([`at`]), and an address ending in
/// punctuation (a sentence's full stop, a bracket it was written in) ends
/// before it. Only one Flow would follow ([`target`]).
pub fn plain<T>(term: &Term<T>, point: Point, regex: &mut RegexSearch) -> Option<Link> {
    let grid = term.grid();
    let lines = -(grid.history_size() as i32)..grid.screen_lines() as i32;
    if !lines.contains(&point.line.0) || point.column.0 >= grid.columns() || grid[point].hyperlink().is_some() {
        return None;
    }
    let (start, end) = (term.line_search_left(point), term.line_search_right(point));
    let found = RegexIter::new(start, end, Direction::Right, term, regex).find(|m| m.contains(&point))?;
    let text = term.bounds_to_string(*found.start(), *found.end());
    let text = text.trim_end_matches('\n');
    let uri = trimmed(text);
    let cut = text.chars().count() - uri.chars().count();
    let last = found.end().sub(term, Boundary::Grid, cut);
    let cells = *found.start()..=last;
    (cells.contains(&point) && target(uri).is_some()).then(|| Link::Plain { uri: uri.to_string(), cells })
}

/// An address found in text without what ends the sentence around it: a
/// full stop, comma, colon, semicolon, ! or ?, and a closing bracket it
/// doesn't open itself, as `(see https://example.com)` writes one.
fn trimmed(text: &str) -> &str {
    let mut out = text;
    loop {
        let before = out;
        out = out.trim_end_matches(['.', ',', ':', ';', '!', '?']);
        for (open, close) in [('(', ')'), ('[', ']')] {
            if out.ends_with(close) && out.matches(open).count() < out.matches(close).count() {
                out = &out[..out.len() - 1];
            }
        }
        if out == before {
            return out;
        }
    }
}

/// The link Flow would follow at `point` of `term`'s grid (a wide
/// letter's second half is its letter's), if one's there.
pub fn at<T>(term: &Term<T>, point: Point) -> Option<Hyperlink> {
    let grid = term.grid();
    let lines = -(grid.history_size() as i32)..grid.screen_lines() as i32;
    if !lines.contains(&point.line.0) || point.column.0 >= grid.columns() {
        return None;
    }
    let mut cell = &grid[point];
    if cell.flags.contains(Flags::WIDE_CHAR_SPACER) && point.column.0 > 0 {
        cell = &grid[Point::new(point.line, Column(point.column.0 - 1))];
    }
    cell.hyperlink().filter(|link| target(link.uri()).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::GridSize;
    use alacritty_terminal::event::VoidListener;
    use alacritty_terminal::index::Line;
    use alacritty_terminal::term::Config;
    use alacritty_terminal::vte::ansi::Processor;

    fn term(bytes: &[u8]) -> Term<VoidListener> {
        let mut term = Term::new(Config::default(), &GridSize { cols: 30, rows: 4 }, VoidListener);
        let mut processor: Processor = Processor::new();
        processor.advance(&mut term, bytes);
        term
    }

    #[test]
    fn web_and_mail_links_open_and_nothing_else_does() {
        for uri in ["https://github.com/arcanii/Flow-releases/releases/tag/flow-rogue-v0.1.45", "http://localhost:8080/", "HTTPS://example.com", "mailto:someone@example.com"] {
            assert!(openable(uri), "{uri}");
        }
        for uri in ["file:///Applications/Calculator.app", "javascript:alert(1)", "ssh://host", "https://", "https:///etc/passwd", "https:example.com", "https://example.com/a b", "https://example.com/\n", "mailto:", "flow-line-size", ""] {
            assert!(!openable(uri), "{uri}");
        }
        assert!(!openable(&format!("https://example.com/{}", "a".repeat(LONGEST))));
    }

    #[test]
    fn what_no_address_holds_never_reaches_the_opener() {
        // What an address may hold, commas and letters past ASCII among it.
        for uri in ["https://maps.example.com/@35.6,139.7,15z", "https://example.com/a;b=c'd!$&*+()[]~%20#top", "https://ja.wikipedia.org/wiki/地図", "https://[::1]:8080/", "mailto:me@example.com?subject=hi"] {
            assert!(openable(uri), "{uri}");
        }
        // Quotes, a backslash, brackets an address never has, and a host
        // that's none: Explorer, or a browser's command line, could read
        // them as more than an address.
        for uri in [
            "https://example.com/a\",/root,C:\\Windows\\System32\\calc.exe",
            "https://example.com/\"-profile",
            "https://evil.example\\@good.example/",
            "https://example.com/<b>",
            "https://example.com/a|b",
            "https://example.com/{x}",
            "https://example.com/`x`",
            "https://example.com/a^b",
            "https://?q",
            "https://#top",
            "mailto:me@example.com\"",
            "https://example.com/a\u{3000}b",
        ] {
            assert!(!openable(uri), "{uri}");
            assert!(!for_the_opener(uri), "{uri}");
        }
        // A web address may be as long as a server's page to sign in on is.
        let long = format!("https://auth.example.com/authorize?state={}", "a".repeat(LONGEST));
        assert!(web_address(&long) && for_the_opener(&long) && !openable(&long));
    }

    #[test]
    fn a_link_holding_what_an_address_may_not_opens_with_it_escaped() {
        // As a browser sends them, so the opener never sees one.
        for (uri, opened) in [
            ("https://fonts.googleapis.com/css?family=Roboto|Open+Sans", "https://fonts.googleapis.com/css?family=Roboto%7COpen+Sans"),
            ("https://example.com/\"-profile", "https://example.com/%22-profile"),
            ("https://example.com/{x}/<b>/a^b/`x`", "https://example.com/%7Bx%7D/%3Cb%3E/a%5Eb/%60x%60"),
            ("mailto:me@example.com?subject=\"hi\"", "mailto:me@example.com?subject=%22hi%22"),
        ] {
            assert_eq!(target(uri), Some(Target::Open(opened.into())), "{uri}");
            assert!(for_the_opener(opened), "{opened}");
        }
        // A space, a control character or a backslash isn't one, escaped
        // or not: a browser reads a backslash in a host as a slash.
        for uri in ["https://example.com/a\",/root,C:\\Windows\\System32\\calc.exe", "https://evil.example\\@good.example/", "https://example.com/a b", "https://example.com/a\u{3000}b", "https://example.com/a\u{7}"] {
            assert_eq!(target(uri), None, "{uri}");
        }
    }

    #[test]
    fn a_mail_link_fills_in_a_mail_and_attaches_nothing() {
        for uri in ["mailto:me@example.com", "mailto:me@example.com?subject=hi&body=see%20you&CC=you@example.com", "mailto:?to=me@example.com&bcc=a@example.com&in-reply-to=%3Cid@example.com%3E", "mailto:me@example.com?"] {
            assert!(openable(uri) && for_the_opener(uri), "{uri}");
        }
        // A mail app may attach the file a field names, which on Windows
        // could be another computer's.
        for uri in [
            "mailto:a@b.example?attach=%5C%5Cevil.example%5Cshare%5Cx",
            "mailto:a@b.example?subject=hi&Attachment=//evil.example/share/x",
            "mailto:a@b.example?%61ttach=/etc/passwd",
            "mailto:a@b.example?subject=hi&x-attach=y",
            "mailto:a@b.example?%zz=y",
            "mailto:",
        ] {
            assert!(!openable(uri) && !for_the_opener(uri), "{uri}");
            assert_eq!(target(uri), None, "{uri}");
        }
    }

    #[test]
    fn the_opener_is_handed_addresses_and_flows_own_files_never_a_path() {
        // Flow's own files, a config folder redirected to a server's
        // included: only Flow makes a `file:` URL for the opener.
        for url in ["https://example.com", "mailto:me@example.com", "file:///C:/Users/me/AppData/Roaming/flow/assistant-memory.txt", "file:////server/users/me/AppData/Roaming/flow/instructions.md"] {
            assert!(for_the_opener(url), "{url}");
        }
        let own = crate::ai::memory::file_url(std::path::Path::new("/Users/me/Library/Application Support/flow/地図.txt"));
        assert!(for_the_opener(&own), "{own}");
        // Windows' opener runs a path as a program; xdg-open takes a word
        // starting with a dash for an option.
        for url in ["C:\\Windows\\System32\\calc.exe", "calc.exe", "\\\\evil.example\\share\\x.exe", "/usr/bin/xterm", "-e", "--manual", "javascript:alert(1)", "ms-settings:", "file:", "file:///a b", "file://evil.example/share/x.exe", "file:relative", ""] {
            assert!(!for_the_opener(url), "{url}");
        }
    }

    #[test]
    fn a_file_link_to_another_computer_is_never_followed() {
        // On Windows each is \\evil.example\share\x, or a device, which
        // merely looking at hands the server the user's NTLM hash.
        for uri in [
            "file:////evil.example/share/x",
            "file://localhost//evil.example/share/x",
            "file:///%2Fevil.example/share/x",
            "file:///%5C%5Cevil.example%5Cshare%5Cx",
            "file:///%5Cevil.example/share/x",
            "file:////%3F/UNC/evil.example/share/x",
            "file:////./pipe/x",
            "file://///evil.example/share/x",
            "file:////?/C:/../UNC/evil.example/share/x",
            "file:////%3F/C:/../UNC/evil.example/share/x",
            "file:///%5C%3F%5CC:%5C..%5CUNC%5Cevil.example%5Cshare%5Cx",
            "file:///%5C%5C?%5CC:%5C..%5CUNC%5Cevil.example%5Cshare%5Cx",
        ] {
            assert_eq!(target(uri), None, "{uri}");
            // As Windows reads them, whatever system the tests run on.
            assert_eq!(file_path(uri, true), None, "{uri}");
        }
        assert_eq!(file_path("file:///C:/Users/me/notes.txt", true).as_deref(), Some("C:/Users/me/notes.txt"));
        // Not as a link a program marked, nor as an address in the text.
        let t = term(b"\x1b]8;;file:////evil.example/s/x\x1b\\notes.txt\x1b]8;;\x1b\\");
        assert_eq!(at(&t, Point::new(Line(0), Column(2))), None);
        let t = term(b"file:////evil.example/s/x");
        assert_eq!(plain_at(&t, 0, 9), None);
    }

    #[test]
    fn a_path_is_on_this_computer_only_where_no_server_is_named() {
        // As Windows reads paths: only a drive's own.
        for path in ["C:\\Users\\me", "c:/Users/me", "D:\\"] {
            assert!(local(path, true), "{path}");
        }
        for path in [
            "\\\\evil.example\\share",
            "//evil.example/share",
            "\\/evil.example/share",
            "/\\evil.example\\share",
            "\\\\?\\UNC\\evil.example\\share",
            "//?/UNC/evil.example/share",
            "\\\\?\\GLOBALROOT\\Device\\Mup\\evil.example\\share",
            "\\\\?\\C:",
            "\\\\.\\C:\\x",
            "\\\\.\\pipe\\x",
            "\\??\\UNC\\evil.example\\share",
            "\\Users\\me",
            "/tmp",
            "C:",
            "C:x",
            "x",
            "",
        ] {
            assert!(!local(path, true), "{path}");
        }
        // A drive's path written verbatim, as `canonicalize` gives it.
        assert!(local("\\\\?\\C:\\Users\\me\\m.glb", true));
        // Any other way of writing it Windows tidies up as a device's
        // path, whose `..` climbs from the drive: each of these is
        // \\evil.example\share. Verbatim, it's never tidied, but no path
        // `canonicalize` gives holds a `.` or `..`, so none is taken.
        for path in [
            "//?/c:/Users/me",
            "//?/C:/../UNC/evil.example/share",
            "\\\\?/C:\\..\\UNC\\evil.example\\share",
            "/\\?\\C:\\..\\UNC\\evil.example\\share",
            "\\/?\\C:\\..\\UNC\\evil.example\\share",
            "\\\\.\\C:\\..\\UNC\\evil.example\\share",
            "\\\\?\\C:\\..\\UNC\\evil.example\\share",
            "\\\\?\\C:\\Users\\.\\me",
            "\\\\?\\C:/Users/me",
        ] {
            assert!(!local(path, true), "{path}");
        }
        // As the other systems read them: anything but two separators.
        for path in ["/tmp/a b", "/", "x", "", "/a//b"] {
            assert!(local(path, false), "{path}");
        }
        for path in ["//evil.example/share", "/\\evil.example", "\\\\evil.example"] {
            assert!(!local(path, false), "{path}");
        }
    }

    #[test]
    fn a_link_covers_the_cells_of_its_words_and_no_others() {
        let t = term("see \x1b]8;;https://example.com/page\x1b\\the page\x1b]8;;\x1b\\ now, 地図 \x1b]8;id=a;https://example.com/map\x1b\\地図\x1b]8;;\x1b\\".as_bytes());
        let link = |col: usize| at(&t, Point::new(Line(0), Column(col))).map(|l| l.uri().to_string());
        assert_eq!(link(3), None);
        assert_eq!(link(4).as_deref(), Some("https://example.com/page"));
        assert_eq!(link(11).as_deref(), Some("https://example.com/page"));
        assert_eq!(link(12), None);
        // A wide letter's second half is part of its link too.
        let map = (23..27).map(link).collect::<Vec<_>>();
        assert!(map.iter().all(|l| l.as_deref() == Some("https://example.com/map")), "{map:?}");
        // The cells of one link are one link, which the underline goes by.
        assert_eq!(at(&t, Point::new(Line(0), Column(4))), at(&t, Point::new(Line(0), Column(9))));
        assert_ne!(at(&t, Point::new(Line(0), Column(4))), at(&t, Point::new(Line(0), Column(23))));
        assert_eq!(link(18), None);
        assert_eq!(at(&t, Point::new(Line(7), Column(0))), None);
    }

    /// An address found and its first and last cells, as rows and columns.
    type Found = (String, (i32, usize), (i32, usize));

    /// The address [`plain`] finds under column `col` of row `row`, and
    /// its cells as columns of rows.
    fn plain_at(t: &Term<VoidListener>, row: i32, col: usize) -> Option<Found> {
        match plain(t, Point::new(Line(row), Column(col)), &mut addresses())? {
            Link::Plain { uri, cells } => Some((uri, (cells.start().line.0, cells.start().column.0), (cells.end().line.0, cells.end().column.0))),
            Link::Marked(_) => None,
        }
    }

    #[test]
    fn an_address_in_plain_text_is_a_link_without_the_sentence_round_it() {
        let t = term(b"see https://ex.com/a. now");
        assert_eq!(plain_at(&t, 0, 10), Some(("https://ex.com/a".into(), (0, 4), (0, 19))));
        assert_eq!(plain_at(&t, 0, 20), None, "the full stop isn't part of it");
        assert_eq!(plain_at(&t, 0, 2), None);
        // In brackets, keeping a pair of its own.
        let t = term(b"(see https://ex.com/a_(b))");
        assert_eq!(plain_at(&t, 0, 8).map(|l| l.0).as_deref(), Some("https://ex.com/a_(b)"));
        // Mail and a file on this computer (a drive's on Windows); capitals
        // in the scheme too.
        let file = if cfg!(windows) { "file:///C:/tmp/a.txt" } else { "file:///tmp/a.txt" };
        let t = term(format!("mailto:me@ex.com\r\n{file}\r\nHTTPS://EX.COM").as_bytes());
        assert_eq!(plain_at(&t, 0, 3).map(|l| l.0).as_deref(), Some("mailto:me@ex.com"));
        assert_eq!(plain_at(&t, 1, 9).map(|l| l.0).as_deref(), Some(file));
        assert_eq!(plain_at(&t, 2, 9).map(|l| l.0).as_deref(), Some("HTTPS://EX.COM"));
        // What Flow wouldn't follow isn't one.
        let t = term(b"ssh://host https://");
        assert_eq!(plain_at(&t, 0, 3), None);
        assert_eq!(plain_at(&t, 0, 15), None);
    }

    #[test]
    fn an_address_wrapped_across_rows_is_found_whole_from_either() {
        // 30 columns: the address runs on into the second row.
        let t = term(b"go to https://example.com/a/long/path/x here");
        let whole = Some(("https://example.com/a/long/path/x".into(), (0, 6), (1, 8)));
        assert_eq!(plain_at(&t, 0, 20), whole);
        assert_eq!(plain_at(&t, 1, 3), whole);
        assert_eq!(plain_at(&t, 1, 12), None, "the words after it");
    }

    #[test]
    fn a_marked_link_and_a_pictures_cells_end_an_address_in_the_text() {
        // A cell a program linked is the program's, even to somewhere Flow
        // won't go.
        let t = term(b"\x1b]8;;ssh://host/x\x1b\\https://example.com\x1b]8;;\x1b\\");
        assert_eq!(plain_at(&t, 0, 3), None);
        // A picture's placeholder cell right after an address isn't in it.
        let t = term("https://ex.com\u{10EEEE}\u{10EEEE}".as_bytes());
        assert_eq!(plain_at(&t, 0, 3).map(|l| l.0).as_deref(), Some("https://ex.com"));
        // A link's cells, as the underline goes by them.
        let link = plain(&t, Point::new(Line(0), Column(3)), &mut addresses()).unwrap();
        assert!(link.covers(None, Point::new(Line(0), Column(13))) && !link.covers(None, Point::new(Line(0), Column(14))));
    }

    #[test]
    fn a_link_flow_wont_follow_is_left_as_plain_text() {
        let t = term(b"\x1b]8;;ssh://host/x\x1b\\https://example.com\x1b]8;;\x1b\\");
        assert_eq!(at(&t, Point::new(Line(0), Column(3))), None);
    }

    #[test]
    fn a_file_on_this_computer_is_revealed_never_opened() {
        // Windows' files are a drive's; a path from no drive is none.
        let (app, shown, hosts) = if cfg!(windows) {
            ("file:///C:/Windows/System32/calc.exe", "C:/Windows/System32/calc.exe", ("file://localhost/C:/Windows/System32/drivers/etc/hosts", "C:/Windows/System32/drivers/etc/hosts"))
        } else {
            ("file:///Applications/Calculator.app", "/Applications/Calculator.app", ("file://localhost/etc/hosts", "/etc/hosts"))
        };
        let t = term(format!("\x1b]8;;{app}\x1b\\https://example.com\x1b]8;;\x1b\\").as_bytes());
        let link = at(&t, Point::new(Line(0), Column(3))).expect("a link to reveal");
        assert_eq!(target(link.uri()), Some(Target::Reveal(shown.into())), "dressed as a web page, it still only shows the file");
        let (ls, listed) = if cfg!(windows) { ("file:///C:/tmp/a%20b/%C3%BC.txt", "C:/tmp/a b/ü.txt") } else { ("file:///tmp/a%20b/%C3%BC.txt", "/tmp/a b/ü.txt") };
        assert_eq!(target(ls), Some(Target::Reveal(listed.into())), "as Flow-Utils' ls writes them");
        assert_eq!(target(hosts.0), Some(Target::Reveal(hosts.1.into())));
        assert_eq!(target("https://example.com"), Some(Target::Open("https://example.com".into())));
        for uri in ["file://otherhost/etc/hosts", "file:relative", "file:///a%0Ab", "file:///bad%zz", "file:///bad%ff", "javascript:alert(1)"] {
            assert_eq!(target(uri), None, "{uri}");
        }
        if cfg!(windows) {
            assert_eq!(target("file:///tmp/a.txt"), None, "a path from no drive goes by Flow's own folder");
        }
    }
}
