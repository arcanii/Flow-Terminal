//! Word wrapping and a small markdown subset for the assistant panel, and
//! for help's pages, whose links (`[words](target)`) it reads too.

use unicode_width::UnicodeWidthChar;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Normal,
    Bold,
    Code,
    Dim,
    /// A link's words: the index of its target in `Linked::links`. Only
    /// `layout_linked` makes them.
    Link(usize),
}

pub type Span = (String, Style);
pub type Line = Vec<Span>;

/// How a line goes on from the one above it where the wrap carried it
/// down, not the text: so copying what's selected joins it back on as it
/// was written, without the `indent` columns the wrap put before it and
/// with the `spaces` it took from the break (none where it broke a word, or
/// between Chinese or Japanese characters).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Wrapped {
    pub indent: usize,
    pub spaces: String,
}

/// Lines as they're laid out, each with how it goes on from the one above:
/// `None` for a line of the text's own.
#[derive(Default)]
struct Out {
    lines: Vec<Line>,
    wraps: Vec<Option<Wrapped>>,
}

impl Out {
    fn push(&mut self, line: Line, wrap: Option<Wrapped>) {
        self.lines.push(line);
        self.wraps.push(wrap);
    }

    fn len(&self) -> usize {
        self.lines.len()
    }
}

/// Splits `**bold**` and `` `code` `` runs out of one line of text, and
/// with `links`, `[words](target)` too: the words become one span of
/// `Style::Link`, the target goes on the list, and a bracket that isn't a
/// whole link stays as written.
fn inline_spans(text: &str, base: Style, mut links: Option<&mut Vec<String>>) -> Vec<Span> {
    let mut out: Vec<Span> = Vec::new();
    let mut cur = String::new();
    let mut style = base;
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '`' && style != Style::Bold {
            if !cur.is_empty() {
                out.push((std::mem::take(&mut cur), style));
            }
            style = if style == Style::Code { base } else { Style::Code };
            i += 1;
            continue;
        }
        if c == '*' && chars.get(i + 1) == Some(&'*') && style != Style::Code {
            if !cur.is_empty() {
                out.push((std::mem::take(&mut cur), style));
            }
            style = if style == Style::Bold { base } else { Style::Bold };
            i += 2;
            continue;
        }
        if c == '[' && style != Style::Code {
            if let (Some(list), Some((words, target, end))) = (links.as_deref_mut(), link_at(&chars, i)) {
                if !cur.is_empty() {
                    out.push((std::mem::take(&mut cur), style));
                }
                out.push((words, Style::Link(list.len())));
                list.push(target);
                i = end;
                continue;
            }
        }
        cur.push(c);
        i += 1;
    }
    if !cur.is_empty() {
        out.push((cur, style));
    }
    out
}

/// The link whose `[` is at `start`: its words, its target and where the
/// text after it starts. The words hold no brackets and the target no
/// spaces or parentheses, so brackets in prose stay as they are.
fn link_at(chars: &[char], start: usize) -> Option<(String, String, usize)> {
    let close = start + 1 + chars[start + 1..].iter().position(|&c| c == ']' || c == '[')?;
    if chars[close] != ']' || close == start + 1 || chars.get(close + 1) != Some(&'(') {
        return None;
    }
    let from = close + 2;
    let end = from + chars[from..].iter().position(|&c| c == ')' || c == '(' || c.is_whitespace())?;
    if chars[end] != ')' || end == from {
        return None;
    }
    Some((chars[start + 1..close].iter().collect(), chars[from..end].iter().collect(), end + 1))
}

/// Greedy word wrap of styled spans to `cols` columns. Continuation lines get
/// `indent` leading spaces.
/// Chinese, Japanese and Korean characters: two cells in the terminal, but
/// drawn 1em wide in the interface's own text, as their fonts make them.
/// Text in them breaks between any two, as their languages do.
pub fn is_cjk(ch: char) -> bool {
    matches!(ch as u32,
        0x1100..=0x115F | 0x2E80..=0x303E | 0x3041..=0x33FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xA960..=0xA97F
        | 0xAC00..=0xD7A3 | 0xF900..=0xFAFF | 0xFE30..=0xFE4F | 0xFF01..=0xFF60 | 0xFFE0..=0xFFE6 | 0x20000..=0x3FFFD)
}

/// Punctuation and small kana that Japanese and Chinese never start a line
/// with (kinsoku), and the opening brackets they never end one with.
fn no_line_start(ch: char) -> bool {
    "、。，．・：；？！ー～）」』】〕〉》’”ぁぃぅぇぉっゃゅょゎァィゥェォッャュョヮヵヶ々ゝゞヽヾ）］｝".contains(ch)
}

fn no_line_end(ch: char) -> bool {
    "（「『【〔〈《‘“［｛".contains(ch)
}

fn wrap_spans(spans: Vec<Span>, cols: usize, indent: usize, out: &mut Out) {
    let cols = cols.max(8);
    let mut line: Line = Vec::new();
    let mut width = 0usize;
    // How the line being laid out goes on from the one before it.
    let mut wrap = None;
    // Words keep the spaces before them, and run on where the style changes
    // without a space (a link or bold words and the stop after them), so
    // the two stay together and kinsoku holds across them.
    let mut word: Vec<(char, Style)> = Vec::new();
    for (ch, style) in spans.iter().flat_map(|(text, style)| text.chars().map(move |c| (c, *style))) {
        if ch == ' ' && word.last().is_some_and(|&(c, _)| c != ' ') {
            flush_word(&mut word, &mut line, &mut width, &mut wrap, out, (cols, indent));
        }
        word.push((ch, style));
    }
    flush_word(&mut word, &mut line, &mut width, &mut wrap, out, (cols, indent));
    out.push(line, wrap);
}

/// Adds a character to the line, in its last span when that's in the same style.
fn push_char(line: &mut Line, ch: char, style: Style) {
    match line.last_mut() {
        Some(last) if last.1 == style => last.0.push(ch),
        _ => line.push((ch.to_string(), style)),
    }
}

/// Puts one word on the line, or on a new one when it doesn't fit, which
/// `wrap` then says goes on from the line before.
fn flush_word(word: &mut Vec<(char, Style)>, line: &mut Line, width: &mut usize, wrap: &mut Option<Wrapped>, out: &mut Out, (cols, indent): (usize, usize)) {
    if word.is_empty() {
        return;
    }
    let w: usize = word.iter().map(|&(c, _)| c.width().unwrap_or(0)).sum();
    // A word in Chinese or Japanese runs on from where the line is;
    // it breaks between characters below.
    if *width + w > cols && *width > indent && !word.iter().any(|&(c, _)| is_cjk(c)) {
        out.push(std::mem::take(line), wrap.take());
        *width = 0;
        if indent > 0 {
            line.push((" ".repeat(indent), Style::Normal));
            *width = indent;
        }
        let spaces = word.iter().take_while(|&&(c, _)| c.is_whitespace()).count();
        *wrap = Some(Wrapped { indent, spaces: word.drain(..spaces).map(|(c, _)| c).collect() });
    }
    // Hard-break words longer than a line.
    let mut piece: Vec<(char, Style)> = Vec::new();
    let mut pw = 0;
    for &(ch, style) in word.iter() {
        let cw = ch.width().unwrap_or(0);
        if *width + pw + cw > cols && *width + pw > 0 {
            // Keep kinsoku: the character before one that can't
            // start a line, or an opening bracket, comes down too, and
            // the one before that when it can't start one either (a
            // closing bracket before a comma).
            let mut carry: Vec<(char, Style)> = Vec::new();
            let mut first = ch;
            while piece.len() > 1 && (no_line_start(first) || piece.last().is_some_and(|&(c, _)| no_line_end(c))) {
                let c = piece.pop().unwrap();
                first = c.0;
                carry.insert(0, c);
            }
            for &(c, s) in &piece {
                push_char(line, c, s);
            }
            out.push(std::mem::take(line), wrap.take());
            *wrap = Some(Wrapped { indent, spaces: String::new() });
            *width = 0;
            if indent > 0 {
                line.push((" ".repeat(indent), Style::Normal));
                *width = indent;
            }
            pw = carry.iter().map(|&(c, _)| c.width().unwrap_or(0)).sum();
            piece = carry;
        }
        piece.push((ch, style));
        pw += cw;
    }
    for &(c, s) in &piece {
        push_char(line, c, s);
    }
    *width += pw;
    word.clear();
}

/// Lays out assistant text: bullets, headings, code fences and inline
/// styles, and finds its fenced blocks of code, for their Copy and Run, and
/// its lines of nothing but a span of code, for their Copy, which the panel
/// draws at the end of the line: such a line wraps `room` columns short of
/// `cols`, so it has room there. Unless the text is `whole`, more of it may
/// come, so its line after its last newline, which may yet go on past a
/// span, is laid out as any other line.
/// Links stay as written, since the assistant's panel can't follow them and
/// their targets would be lost.
pub fn layout_markdown(text: &str, cols: usize, room: usize, whole: bool) -> (Vec<Line>, Vec<Block>) {
    let (lines, blocks, _) = layout_markdown_wraps(text, cols, room, whole);
    (lines, blocks)
}

/// [`layout_markdown`], with how each line goes on from the one above it
/// ([`Wrapped`]), for copying what's selected.
pub fn layout_markdown_wraps(text: &str, cols: usize, room: usize, whole: bool) -> (Vec<Line>, Vec<Block>, Vec<Option<Wrapped>>) {
    let mut blocks = Vec::new();
    let out = layout_blocks(text, cols, None, Some((&mut blocks, room, whole)));
    (out.lines, blocks, out.wraps)
}

/// Markdown laid out with its links read: each link's words are spans of
/// `Style::Link(i)` (on two lines or more where they wrap), `i` its target's
/// place in `links`. Which links there are doesn't depend on the width.
#[derive(Debug)]
pub struct Linked {
    pub lines: Vec<Line>,
    /// Each link's target, in the order the links come.
    pub links: Vec<String>,
}

/// Lays out help's markdown as `layout_markdown` does, and reads its links.
pub fn layout_linked(text: &str, cols: usize) -> Linked {
    let mut links = Vec::new();
    let lines = layout(text, cols, Some(&mut links));
    Linked { lines, links }
}

/// The first and last lines that link `link`'s words are on, for scrolling
/// it into view; None when no line has it.
pub fn lines_of(lines: &[Line], link: usize) -> Option<(usize, usize)> {
    let mut on = lines.iter().enumerate().filter(|(_, line)| line.iter().any(|&(_, style)| style == Style::Link(link))).map(|(i, _)| i);
    let first = on.next()?;
    Some((first, on.next_back().unwrap_or(first)))
}

/// A block of code in markdown, as laid out: the lines it took (`lines`,
/// of the layout's), the word after its opening fence (`bash`, or empty),
/// and its text as written. A line that's nothing but one span of
/// `` `code` `` (a list item's after its bullet or number) is one too
/// (`inline`, its text the span's, trimmed), which is copied as written
/// and never run: a model puts names of files, paths, settings and keys in
/// spans as often as commands. A span among other words isn't one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub lines: std::ops::Range<usize>,
    pub lang: String,
    pub code: String,
    pub inline: bool,
}

impl Block {
    /// Whether it reads as commands for a shell: a fenced block with no
    /// language named, or a shell's, rather than a program's source or a
    /// file's. A span names no language, and isn't.
    pub fn is_shell(&self) -> bool {
        if self.inline {
            return false;
        }
        let shells = ["", "sh", "bash", "zsh", "fish", "shell", "console", "terminal", "powershell", "pwsh", "ps1", "ps", "cmd", "bat", "batch"];
        shells.contains(&self.lang.to_ascii_lowercase().as_str())
    }

    /// The commands to run or copy: its text as written, or for a shell
    /// session (lines starting with a prompt: `$ `, `% `, `PS> `, and `> `
    /// for a line that goes on) the lines with a prompt, without it, and
    /// not the output between them, which would run as commands.
    pub fn commands(&self) -> String {
        fn prompted(line: &str) -> Option<&str> {
            ["$ ", "% ", "PS> ", "> "].into_iter().find_map(|prompt| line.trim_start().strip_prefix(prompt))
        }
        let lines: Vec<&str> = self.code.lines().filter_map(prompted).collect();
        if lines.is_empty() {
            self.code.clone()
        } else {
            lines.join("\n")
        }
    }
}

/// A line that's nothing but one span of `` `code` `` (a paragraph's, or a
/// list item's after its bullet or number): the code in it, trimmed. Its
/// two backticks are its only ones, so a line of two spans has none.
fn lone_code(text: &str) -> Option<&str> {
    let code = text.trim().strip_prefix('`')?.strip_suffix('`')?.trim();
    (!code.is_empty() && !code.contains('`')).then_some(code)
}

/// Notes a line of nothing but code as a block, for its Copy.
fn note_lone_code(blocks: Option<&mut Vec<Block>>, code: Option<&str>, lines: std::ops::Range<usize>) {
    if let (Some(blocks), Some(code)) = (blocks, code) {
        blocks.push(Block { lines, lang: String::new(), code: code.to_string(), inline: true });
    }
}

fn layout(text: &str, cols: usize, links: Option<&mut Vec<String>>) -> Vec<Line> {
    layout_blocks(text, cols, links, None).lines
}

/// Lays out markdown, finding its blocks of code where `blocks` is given,
/// with the room its lines of nothing but a span of code leave at their end
/// and whether the text is whole.
fn layout_blocks(text: &str, cols: usize, mut links: Option<&mut Vec<String>>, blocks: Option<(&mut Vec<Block>, usize, bool)>) -> Out {
    let (mut blocks, room, whole) = match blocks {
        Some((blocks, room, whole)) => (Some(blocks), room, whole),
        None => (None, 0, true),
    };
    let pieces = text.split('\n').count();
    let mut out = Out::default();
    // The block open: where its lines began, its language and its text.
    let mut fence: Option<(usize, String, Vec<&str>)> = None;
    for (n, raw) in text.split('\n').enumerate() {
        // A line of nothing but a span of code (after any bullet or number)
        // is wrapped short of the width by the room for its Copy; the line
        // after the last newline of a text still coming in isn't one yet.
        let open = !whole && n + 1 == pieces;
        let lone = |text| lone_code(text).filter(|_| !open);
        let width = |text| if room > 0 && lone(text).is_some() { cols.saturating_sub(room).max(cols / 2) } else { cols };
        let line = raw.trim_end_matches('\r');
        if let Some(rest) = line.trim_start().strip_prefix("```") {
            match fence.take() {
                Some((first, lang, code)) => {
                    if let Some(blocks) = blocks.as_deref_mut() {
                        blocks.push(Block { lines: first..out.len(), lang, code: code.join("\n"), inline: false });
                    }
                }
                None => fence = Some((out.len(), rest.trim().to_string(), Vec::new())),
            }
            continue;
        }
        if let Some((_, _, code)) = fence.as_mut() {
            code.push(line);
            let mut s = String::from("  ");
            s.push_str(line);
            wrap_spans(vec![(s, Style::Code)], cols, 2, &mut out);
            continue;
        }
        let trimmed = line.trim_start();
        let lead = line.len() - trimmed.len();
        if trimmed.is_empty() {
            out.push(Vec::new(), None);
            continue;
        }
        if let Some(h) = trimmed.strip_prefix('#') {
            let h = h.trim_start_matches('#').trim();
            wrap_spans(inline_spans(h, Style::Bold, links.as_deref_mut()), cols, 0, &mut out);
            continue;
        }
        let bullet = trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("* "));
        if let Some(rest) = bullet {
            let pad = " ".repeat(lead.min(6));
            let mut spans = vec![(format!("{pad}• "), Style::Dim)];
            spans.extend(inline_spans(rest, Style::Normal, links.as_deref_mut()));
            let first = out.len();
            wrap_spans(spans, width(rest), pad.len() + 2, &mut out);
            note_lone_code(blocks.as_deref_mut(), lone(rest), first..out.len());
            continue;
        }
        let numbered = trimmed.find(". ").filter(|&i| i <= 3 && trimmed[..i].chars().all(|c| c.is_ascii_digit()));
        if let Some(i) = numbered {
            let mut spans = vec![(trimmed[..i + 2].to_string(), Style::Dim)];
            spans.extend(inline_spans(&trimmed[i + 2..], Style::Normal, links.as_deref_mut()));
            let first = out.len();
            wrap_spans(spans, width(&trimmed[i + 2..]), i + 2, &mut out);
            note_lone_code(blocks.as_deref_mut(), lone(&trimmed[i + 2..]), first..out.len());
            continue;
        }
        let first = out.len();
        wrap_spans(inline_spans(line, Style::Normal, links.as_deref_mut()), width(line), 0, &mut out);
        note_lone_code(blocks.as_deref_mut(), lone(line), first..out.len());
    }
    while out.lines.last().is_some_and(|l: &Line| l.is_empty()) {
        out.lines.pop();
        out.wraps.pop();
    }
    out
}

/// Plain wrap of `text` in one style.
pub fn wrap_plain(text: &str, cols: usize, style: Style) -> Vec<Line> {
    wrap_plain_wraps(text, cols, style).0
}

/// [`wrap_plain`], with how each line goes on from the one above it
/// ([`Wrapped`]), for copying what's selected.
pub fn wrap_plain_wraps(text: &str, cols: usize, style: Style) -> (Vec<Line>, Vec<Option<Wrapped>>) {
    let mut out = Out::default();
    for line in text.split('\n') {
        wrap_spans(vec![(line.to_string(), style)], cols, 0, &mut out);
    }
    (out.lines, out.wraps)
}

pub fn line_text(line: &Line) -> String {
    line.iter().map(|(s, _)| s.as_str()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use unicode_width::UnicodeWidthStr;

    #[test]
    fn a_line_the_wrap_carries_down_says_what_the_break_took() {
        let texts = |lines: &[Line]| lines.iter().map(line_text).collect::<Vec<_>>();
        let (lines, wraps) = wrap_plain_wraps("one two  three\nfour", 9, Style::Normal);
        assert_eq!(texts(&lines), ["one two", "three", "four"]);
        assert_eq!(wraps, [None, Some(Wrapped { indent: 0, spaces: "  ".into() }), None], "the spaces as they were; a line of its own has none");
        // A word longer than the line breaks with nothing taken.
        let (lines, wraps) = wrap_plain_wraps("abcdefghijkl", 8, Style::Normal);
        assert_eq!(texts(&lines), ["abcdefgh", "ijkl"]);
        assert_eq!(wraps, [None, Some(Wrapped::default())]);
        // A block of code's line goes on past the two columns it's set in.
        let (lines, _, wraps) = layout_markdown_wraps("Then:\n```\nls -la /etc/ssh\n```", 10, 0, true);
        assert_eq!(texts(&lines), ["Then:", "  ls -la", "  /etc/ssh"]);
        assert_eq!(wraps, [None, None, Some(Wrapped { indent: 2, spaces: " ".into() })]);
    }

    #[test]
    fn inline_styles() {
        let spans = inline_spans("run `ls -la` **now**", Style::Normal, None);
        assert_eq!(
            spans,
            vec![
                ("run ".to_string(), Style::Normal),
                ("ls -la".to_string(), Style::Code),
                (" ".to_string(), Style::Normal),
                ("now".to_string(), Style::Bold),
            ]
        );
    }

    #[test]
    fn japanese_breaks_between_characters_but_keeps_kinsoku() {
        let lines = |text: &str| wrap_plain(text, 8, Style::Normal).iter().map(line_text).collect::<Vec<_>>();
        // Four characters fill a line of eight cells; ー can't start the next.
        assert_eq!(lines("あいうえーお"), ["あいう", "えーお"]);
        // Nor can a line end on an opening bracket.
        assert_eq!(lines("あいう「えお」"), ["あいう", "「えお」"]);
        // After a space, Japanese runs on from where the line is.
        assert_eq!(lines("ab あいうえおかきく"), ["ab あい", "うえおか", "きく"]);
    }

    #[test]
    fn wraps_at_width() {
        let lines = layout_markdown("the quick brown fox jumps over the lazy dog", 12, 0, true).0;
        for l in &lines {
            assert!(line_text(l).width() <= 12, "{:?}", line_text(l));
        }
        assert!(lines.len() >= 4);
    }

    #[test]
    fn bullets_indent_continuations() {
        let lines = layout_markdown("- alpha beta gamma delta epsilon", 14, 0, true).0;
        assert!(line_text(&lines[0]).starts_with("• "));
        assert!(line_text(&lines[1]).starts_with("  "));
    }

    #[test]
    fn long_word_breaks() {
        let lines = wrap_plain("abcdefghijklmnopqrstuvwxyz", 10, Style::Normal);
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn links_are_read_only_where_asked_and_keep_their_words_and_targets() {
        let text = "See [the color picker](help:picker) and **[Settings](help:settings)**.";
        // The assistant's markdown shows a link as written.
        assert_eq!(line_text(&layout_markdown(text, 80, 0, true).0[0]), text.replace("**", ""));
        let laid = layout_linked(text, 80);
        assert_eq!(laid.links, ["help:picker", "help:settings"]);
        assert_eq!(
            laid.lines[0],
            vec![
                ("See ".to_string(), Style::Normal),
                ("the color picker".to_string(), Style::Link(0)),
                (" and ".to_string(), Style::Normal),
                ("Settings".to_string(), Style::Link(1)),
                (".".to_string(), Style::Normal),
            ]
        );
    }

    #[test]
    fn brackets_that_are_not_whole_links_stay_as_written() {
        for text in ["[x] done", "a [b] (c)", "[](help:x)", "[a](  )", "[a](b c)", "`[a](b)`", "[a](b"] {
            let laid = layout_linked(text, 80);
            assert!(laid.links.is_empty(), "{text}");
            assert_eq!(line_text(&laid.lines[0]), text.replace('`', ""), "{text}");
        }
        // A heading's and a bullet's links are read too.
        assert_eq!(layout_linked("# [A](a)\n- [B](b)\n1. [C](c)", 80).links, ["a", "b", "c"]);
    }

    #[test]
    fn a_link_wraps_like_other_words_and_says_which_lines_it_is_on() {
        let laid = layout_linked("one two [three four five](x) six", 14);
        let lines: Vec<_> = laid.lines.iter().map(line_text).collect();
        assert_eq!(lines, ["one two three", "four five six"]);
        assert_eq!(laid.lines[0].last(), Some(&("three".to_string(), Style::Link(0))));
        assert_eq!(laid.lines[1][0], ("four five".to_string(), Style::Link(0)));
        assert_eq!(lines_of(&laid.lines, 0), Some((0, 1)));
        assert_eq!(lines_of(&laid.lines, 1), None);
    }

    #[test]
    fn a_stop_after_a_link_stays_with_it_and_so_does_kinsoku() {
        // The stop isn't left to start a line of its own.
        let laid = layout_linked("aaaa bbbb [cccc](x).", 14);
        assert_eq!(laid.lines.iter().map(line_text).collect::<Vec<_>>(), ["aaaa bbbb", "cccc."]);
        // Nor does 、 start one after a link in Japanese: the link's last
        // character comes down with it.
        let laid = layout_linked("あい[うえ](x)、か", 8);
        assert_eq!(laid.lines.iter().map(line_text).collect::<Vec<_>>(), ["あいう", "え、か"]);
        assert_eq!(laid.lines[0][1], ("う".to_string(), Style::Link(0)));
        assert_eq!(laid.lines[1][0], ("え".to_string(), Style::Link(0)));
    }

    #[test]
    fn a_closing_bracket_before_a_comma_comes_down_with_the_character_before_it() {
        let lines = |text: &str| wrap_plain(text, 8, Style::Normal).iter().map(line_text).collect::<Vec<_>>();
        // Neither ） nor 、 may start the line, so ロ comes down with them.
        assert_eq!(lines("あいロ）、か"), ["あい", "ロ）、か"]);
        // A run of nothing but such characters still breaks somewhere,
        // within the width, losing nothing.
        let run = lines("あ）））、、）））、、");
        assert_eq!(run.concat(), "あ）））、、）））、、");
        assert!(run.iter().all(|line| line.width() <= 8 && !line.is_empty()), "{run:?}");
    }

    #[test]
    fn a_replys_blocks_of_code_are_found_with_the_lines_they_take() {
        let reply = "To list them:\n```bash\nls -la\ndu -sh *\n```\nThen in Python:\n```python\nprint('hi')\n```\nAnd ```unclosed";
        let (lines, blocks) = layout_markdown(reply, 40, 0, true);
        assert_eq!(lines, layout_linked(reply, 40).lines, "laid out as help's pages are");
        assert_eq!(blocks.len(), 2);
        assert_eq!((blocks[0].lang.as_str(), blocks[0].code.as_str()), ("bash", "ls -la\ndu -sh *"));
        assert_eq!(blocks[0].lines, 1..3);
        assert_eq!(line_text(&lines[1]), "  ls -la");
        assert!(blocks[0].is_shell() && !blocks[1].is_shell());
        assert_eq!(blocks[1].lines, 4..5);
        // A long line wraps, and the block takes every line it wraps onto.
        let (lines, blocks) = layout_markdown("```\necho aaaa bbbb cccc dddd eeee\n```", 16, 0, true);
        assert_eq!(blocks[0].lines, 0..lines.len());
        assert!(lines.len() > 1);
    }

    #[test]
    fn a_line_of_nothing_but_code_is_a_block_and_code_among_words_is_not() {
        let reply = "Install it with:\n`brew install ripgrep`\n- `cargo test --workspace`\n2. `config.toml`\n\
            Then `rg foo` searches.\n`a` and `b`\n`unclosed\n# `heading`\n``\n```\n`inside a fence`\n```\n  ` $ npm test `  ";
        let (lines, blocks) = layout_markdown(reply, 40, 0, true);
        assert_eq!(lines, layout_linked(reply, 40).lines, "laid out as before");
        let found: Vec<_> = blocks.iter().map(|b| (b.code.as_str(), b.lines.clone(), b.inline)).collect();
        assert_eq!(
            found,
            [
                ("brew install ripgrep", 1..2, true),
                ("cargo test --workspace", 2..3, true),
                ("config.toml", 3..4, true),
                ("`inside a fence`", 9..10, false),
                ("$ npm test", 10..11, true),
            ]
        );
        assert_eq!(line_text(&lines[1]), "brew install ripgrep");
        assert_eq!(line_text(&lines[2]), "• cargo test --workspace");
        // A span is never run, and is copied as written, a prompt and all; a
        // fenced block goes by its language.
        assert_eq!(blocks.iter().map(Block::is_shell).collect::<Vec<_>>(), [false, false, false, true, false]);
        assert_eq!(blocks[4].code, "$ npm test");
        // It leaves room at its line's end for its Copy, the block taking
        // every line it wraps onto; another line has the whole width.
        let (lines, blocks) = layout_markdown("`echo aaaa bbbb cccc`\necho aaaa bbbb cccc", 24, 8, true);
        assert_eq!((blocks.len(), blocks[0].lines.clone()), (1, 0..2));
        assert_eq!(lines.iter().map(line_text).collect::<Vec<_>>(), ["echo aaaa bbbb", "cccc", "echo aaaa bbbb cccc"]);
    }

    #[test]
    fn a_span_ending_a_text_still_coming_in_is_laid_out_as_words_until_its_line_ends() {
        // Two columns short of the width: whole, it's a block wrapped short
        // by the room for its Copy; open, it may yet go on past the span, so
        // it's a line like any other and takes the width.
        let text = "`echo aaaa bbbb cccc dd`";
        let (lines, blocks) = layout_markdown(text, 24, 8, false);
        assert_eq!((lines.len(), blocks.len()), (1, 0));
        let (lines, blocks) = layout_markdown(text, 24, 8, true);
        assert_eq!((lines.len(), blocks.len()), (2, 1));
        // A newline ends it, and the line after it is the open one.
        let (lines, blocks) = layout_markdown(&format!("{text}\n"), 24, 8, false);
        assert_eq!((lines.len(), blocks.len()), (2, 1));
        assert_eq!(layout_markdown(&format!("{text}\n{text}"), 24, 8, false).1.len(), 1);
    }

    #[test]
    fn a_shell_sessions_prompts_are_left_out_of_its_commands() {
        let block = |code: &str| Block { lines: 0..1, lang: "console".into(), code: code.into(), inline: false };
        assert_eq!(block("$ cd src\n$ cargo build").commands(), "cd src\ncargo build");
        assert_eq!(block("PS> Get-ChildItem").commands(), "Get-ChildItem");
        // Output among them stays out of what runs.
        assert_eq!(block("$ which python3\n/usr/bin/python3\n$ python3 -V").commands(), "which python3\npython3 -V");
        // A command going on over lines, its second prompt left out too.
        assert_eq!(block("$ for f in *.md\n> do wc -l $f\n> done").commands(), "for f in *.md\ndo wc -l $f\ndone");
        assert_eq!(block("cargo test").commands(), "cargo test");
    }
}
