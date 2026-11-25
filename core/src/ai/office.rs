//! The text of an Office file (docx, xlsx, pptx) for the assistant
//! (`docs/design/assistant-files.md`): each is a zip of XML, which Flow
//! reads itself, the zip's central directory by hand, inflate through
//! `miniz_oxide` and the XML through `roxmltree`. Claude and Gemini take
//! no Office file, so they get this text; OpenAI takes the file itself.
//! Only the words come out: a document's paragraphs, a workbook's sheets a
//! row a line, a deck's slides each under its number.

use roxmltree::{Document, Node};

/// The most any one part of the zip may unpack to, against a zip made to
/// unpack forever.
const MOST_UNPACKED: usize = 64 * 1024 * 1024;

/// The kinds of Office file, by what's in their zip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Office {
    Word,
    Excel,
    PowerPoint,
}

impl Office {
    /// The kind a file named `name` is, by its extension.
    pub fn of_name(name: &str) -> Option<Office> {
        let ext = name.rsplit_once('.')?.1.to_ascii_lowercase();
        match ext.as_str() {
            "docx" | "docm" => Some(Office::Word),
            "xlsx" | "xlsm" => Some(Office::Excel),
            "pptx" | "pptm" => Some(Office::PowerPoint),
            _ => None,
        }
    }

    /// Its media type, as OpenAI takes the file.
    pub fn media_type(self) -> &'static str {
        match self {
            Office::Word => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            Office::Excel => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            Office::PowerPoint => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        }
    }
}

/// The text of `bytes`, an Office file of kind `kind`, or why not.
pub fn text(bytes: &[u8], kind: Office) -> Result<String, String> {
    let zip = Zip::read(bytes)?;
    match kind {
        Office::Word => word(&zip),
        Office::Excel => excel(&zip),
        Office::PowerPoint => powerpoint(&zip),
    }
}

/// A zip's entries, read from its central directory.
struct Zip<'a> {
    bytes: &'a [u8],
    entries: Vec<Entry>,
}

struct Entry {
    name: String,
    method: u16,
    packed: usize,
    size: usize,
    local: usize,
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

impl<'a> Zip<'a> {
    fn read(bytes: &'a [u8]) -> Result<Self, String> {
        let bad = || "it isn't a zip Flow can read".to_string();
        // The end of the central directory, its comment at most 64 KB.
        let from = bytes.len().saturating_sub(22 + 65_535);
        let end = (from..bytes.len().saturating_sub(21)).rev().find(|&i| u32_at(bytes, i) == Some(0x0605_4b50)).ok_or_else(bad)?;
        let count = u16_at(bytes, end + 10).ok_or_else(bad)? as usize;
        let mut at = u32_at(bytes, end + 16).ok_or_else(bad)? as usize;
        let mut entries = Vec::with_capacity(count);
        for _ in 0..count {
            if u32_at(bytes, at) != Some(0x0201_4b50) {
                return Err(bad());
            }
            let method = u16_at(bytes, at + 10).ok_or_else(bad)?;
            let packed = u32_at(bytes, at + 20).ok_or_else(bad)? as usize;
            let size = u32_at(bytes, at + 24).ok_or_else(bad)? as usize;
            let name_len = u16_at(bytes, at + 28).ok_or_else(bad)? as usize;
            let extra = u16_at(bytes, at + 30).ok_or_else(bad)? as usize;
            let comment = u16_at(bytes, at + 32).ok_or_else(bad)? as usize;
            let local = u32_at(bytes, at + 42).ok_or_else(bad)? as usize;
            let name = bytes.get(at + 46..at + 46 + name_len).ok_or_else(bad)?;
            entries.push(Entry { name: String::from_utf8_lossy(name).into_owned(), method, packed, size, local });
            at += 46 + name_len + extra + comment;
        }
        Ok(Zip { bytes, entries })
    }

    fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|e| e.name.as_str())
    }

    /// The part called `name`, unpacked as text; none where it isn't there.
    fn part(&self, name: &str) -> Result<Option<String>, String> {
        let Some(e) = self.entries.iter().find(|e| e.name == name) else { return Ok(None) };
        let bad = || format!("its {name} won't unpack");
        if e.size > MOST_UNPACKED {
            return Err(bad());
        }
        let b = self.bytes;
        if u32_at(b, e.local) != Some(0x0403_4b50) {
            return Err(bad());
        }
        let skip = 30 + u16_at(b, e.local + 26).ok_or_else(bad)? as usize + u16_at(b, e.local + 28).ok_or_else(bad)? as usize;
        let data = b.get(e.local + skip..e.local + skip + e.packed).ok_or_else(bad)?;
        let unpacked = match e.method {
            0 => data.to_vec(),
            8 => miniz_oxide::inflate::decompress_to_vec_with_limit(data, MOST_UNPACKED).map_err(|_| bad())?,
            _ => return Err(bad()),
        };
        Ok(Some(String::from_utf8_lossy(&unpacked).into_owned()))
    }

    /// The part called `name`, read as XML.
    fn xml(&self, name: &str) -> Result<Option<String>, String> {
        self.part(name)
    }
}

/// Reads `text` as XML, saying which part didn't where it doesn't.
fn parse<'t>(text: &'t str, part: &str) -> Result<Document<'t>, String> {
    Document::parse(text).map_err(|e| format!("its {part} isn't XML Flow can read: {e}"))
}

/// Whether `node` is an element called `name`, whatever its namespace.
fn is(node: Node, name: &str) -> bool {
    node.is_element() && node.tag_name().name() == name
}

/// A Word document's paragraphs, a line each, tabs and breaks kept.
fn word(zip: &Zip) -> Result<String, String> {
    let text = zip.xml("word/document.xml")?.ok_or("it has no word/document.xml")?;
    let doc = parse(&text, "word/document.xml")?;
    let mut out = String::new();
    for p in doc.descendants().filter(|n| is(*n, "p")) {
        for n in p.descendants() {
            if is(n, "t") {
                out.push_str(n.text().unwrap_or_default());
            } else if is(n, "tab") {
                out.push('\t');
            } else if is(n, "br") || is(n, "cr") {
                out.push('\n');
            }
        }
        out.push('\n');
    }
    Ok(out)
}

/// A PowerPoint deck's slides in order, each's paragraphs under its
/// number.
fn powerpoint(zip: &Zip) -> Result<String, String> {
    let mut slides: Vec<(u32, String)> = zip
        .names()
        .filter_map(|n| Some((n.strip_prefix("ppt/slides/slide")?.strip_suffix(".xml")?.parse().ok()?, n.to_string())))
        .collect();
    slides.sort();
    let mut out = String::new();
    for (number, name) in slides {
        let Some(text) = zip.xml(&name)? else { continue };
        let doc = parse(&text, &name)?;
        out.push_str(&format!("# Slide {number}\n"));
        for p in doc.descendants().filter(|n| is(*n, "p")) {
            let line: String = p.descendants().filter(|n| is(*n, "t")).filter_map(|n| n.text()).collect();
            if !line.trim().is_empty() {
                out.push_str(&line);
                out.push('\n');
            }
        }
        out.push('\n');
    }
    Ok(out)
}

/// An Excel workbook's sheets in order, each under its name, a row a line
/// and its cells parted by tabs, each in its own column.
fn excel(zip: &Zip) -> Result<String, String> {
    // Text cells keep their words in the shared strings, by number.
    let shared: Vec<String> = match zip.xml("xl/sharedStrings.xml")? {
        Some(text) => {
            let doc = parse(&text, "xl/sharedStrings.xml")?;
            let root = doc.root_element();
            root.children().filter(|n| is(*n, "si")).map(|si| si.descendants().filter(|n| is(*n, "t")).filter_map(|n| n.text()).collect()).collect()
        }
        None => Vec::new(),
    };
    // The sheets' names in the workbook's order, and where each is.
    let rels: Vec<(String, String)> = match zip.xml("xl/_rels/workbook.xml.rels")? {
        Some(text) => {
            let doc = parse(&text, "xl/_rels/workbook.xml.rels")?;
            doc.descendants().filter(|n| is(*n, "Relationship")).filter_map(|n| Some((n.attribute("Id")?.to_string(), n.attribute("Target")?.to_string()))).collect()
        }
        None => Vec::new(),
    };
    let mut sheets: Vec<(String, String)> = Vec::new();
    if let Some(text) = zip.xml("xl/workbook.xml")? {
        let doc = parse(&text, "xl/workbook.xml")?;
        for s in doc.descendants().filter(|n| is(*n, "sheet")) {
            let name = s.attribute("name").unwrap_or_default().to_string();
            let id = s.attributes().find(|a| a.name() == "id").map(|a| a.value().to_string()).unwrap_or_default();
            if let Some((_, target)) = rels.iter().find(|(r, _)| *r == id) {
                let target = target.trim_start_matches('/');
                let path = if target.starts_with("xl/") { target.to_string() } else { format!("xl/{target}") };
                sheets.push((name, path));
            }
        }
    }
    if sheets.is_empty() {
        let mut found: Vec<(u32, String)> = zip
            .names()
            .filter_map(|n| Some((n.strip_prefix("xl/worksheets/sheet")?.strip_suffix(".xml")?.parse().ok()?, n.to_string())))
            .collect();
        found.sort();
        sheets = found.into_iter().map(|(i, path)| (format!("Sheet{i}"), path)).collect();
    }
    let mut out = String::new();
    for (name, path) in sheets {
        let Some(text) = zip.xml(&path)? else { continue };
        let doc = parse(&text, &path)?;
        out.push_str(&format!("# Sheet: {name}\n"));
        for row in doc.descendants().filter(|n| is(*n, "row")) {
            let mut cells: Vec<String> = Vec::new();
            for (i, c) in row.children().filter(|n| is(*n, "c")).enumerate() {
                let column = c.attribute("r").and_then(column_of).unwrap_or(i);
                let value = cell_text(c, &shared);
                if cells.len() <= column {
                    cells.resize(column + 1, String::new());
                }
                cells[column] = value;
            }
            while cells.last().is_some_and(String::is_empty) {
                cells.pop();
            }
            if !cells.is_empty() {
                out.push_str(&cells.join("\t"));
                out.push('\n');
            }
        }
        out.push('\n');
    }
    Ok(out)
}

/// A cell's words: a shared string by its number, an inline string, or its
/// value as written (a number, a formula's last result, TRUE as 1).
fn cell_text(c: Node, shared: &[String]) -> String {
    let value = || c.children().find(|n| is(*n, "v")).and_then(|v| v.text()).unwrap_or_default().to_string();
    match c.attribute("t") {
        Some("s") => value().trim().parse::<usize>().ok().and_then(|i| shared.get(i).cloned()).unwrap_or_default(),
        Some("inlineStr") => c.descendants().filter(|n| is(*n, "t")).filter_map(|n| n.text()).collect(),
        _ => value(),
    }
}

/// The column a cell's reference names (`B3` is 1), counted from 0.
fn column_of(reference: &str) -> Option<usize> {
    let letters: String = reference.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    if letters.is_empty() {
        return None;
    }
    let n = letters.chars().fold(0usize, |n, c| n * 26 + (c.to_ascii_uppercase() as usize - 'A' as usize + 1));
    Some(n - 1)
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// A zip of `parts` (name, text), each packed with deflate, as Office
    /// writes them: for this module's tests and `files.rs`'.
    pub fn zip_of(parts: &[(&str, &str)]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        for (name, text) in parts {
            let packed = miniz_oxide::deflate::compress_to_vec(text.as_bytes(), 6);
            let local = out.len() as u32;
            out.extend(0x0403_4b50u32.to_le_bytes());
            out.extend([20, 0, 0, 0, 8, 0, 0, 0, 0, 0]);
            out.extend(0u32.to_le_bytes()); // crc, which Flow doesn't check
            out.extend((packed.len() as u32).to_le_bytes());
            out.extend((text.len() as u32).to_le_bytes());
            out.extend((name.len() as u16).to_le_bytes());
            out.extend(0u16.to_le_bytes());
            out.extend(name.as_bytes());
            out.extend(&packed);
            central.extend(0x0201_4b50u32.to_le_bytes());
            central.extend([20, 0, 20, 0, 0, 0, 8, 0, 0, 0, 0, 0]);
            central.extend(0u32.to_le_bytes());
            central.extend((packed.len() as u32).to_le_bytes());
            central.extend((text.len() as u32).to_le_bytes());
            central.extend((name.len() as u16).to_le_bytes());
            central.extend([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            central.extend(local.to_le_bytes());
            central.extend(name.as_bytes());
        }
        let start = out.len() as u32;
        out.extend(&central);
        out.extend(0x0605_4b50u32.to_le_bytes());
        out.extend([0, 0, 0, 0]);
        out.extend((parts.len() as u16).to_le_bytes());
        out.extend((parts.len() as u16).to_le_bytes());
        out.extend((central.len() as u32).to_le_bytes());
        out.extend(start.to_le_bytes());
        out.extend(0u16.to_le_bytes());
        out
    }

    pub const DOCX: &str = r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Quarterly</w:t></w:r><w:r><w:t xml:space="preserve"> report</w:t></w:r></w:p><w:p><w:r><w:t>a</w:t><w:tab/><w:t>b &amp; c</w:t></w:r></w:p></w:body></w:document>"#;

    #[test]
    fn a_word_documents_paragraphs_come_out_a_line_each() {
        let zip = zip_of(&[("[Content_Types].xml", "<Types/>"), ("word/document.xml", DOCX)]);
        assert_eq!(text(&zip, Office::Word).unwrap(), "Quarterly report\na\tb & c\n");
    }

    #[test]
    fn a_workbooks_sheets_come_out_a_row_a_line_each_cell_in_its_column() {
        let workbook = r#"<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Sales" sheetId="1" r:id="rId1"/></sheets></workbook>"#;
        let rels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#;
        let shared = r#"<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><si><t>Item</t></si><si><t>Price</t></si><si><r><t>Te</t></r><r><t>a</t></r></si></sst>"#;
        let sheet = r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1" t="s"><v>1</v></c></row><row r="2"><c r="A2" t="s"><v>2</v></c><c r="C2"><v>3.5</v></c></row><row r="3"><c r="B3" t="inlineStr"><is><t>note</t></is></c></row></sheetData></worksheet>"#;
        let zip = zip_of(&[("xl/workbook.xml", workbook), ("xl/_rels/workbook.xml.rels", rels), ("xl/sharedStrings.xml", shared), ("xl/worksheets/sheet1.xml", sheet)]);
        assert_eq!(text(&zip, Office::Excel).unwrap(), "# Sheet: Sales\nItem\tPrice\nTea\t\t3.5\n\tnote\n\n");
        assert_eq!((column_of("A1"), column_of("AA10"), column_of("7")), (Some(0), Some(26), None));
    }

    #[test]
    fn a_decks_slides_come_out_in_order_under_their_numbers() {
        let slide = |words: &str| format!(r#"<p:sld xmlns:p="p" xmlns:a="a"><p:cSld><p:spTree><p:sp><p:txBody><a:p><a:r><a:t>{words}</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>"#);
        let (one, two, ten) = (slide("Title"), slide("Second"), slide("Last"));
        let zip = zip_of(&[("ppt/slides/slide10.xml", &ten), ("ppt/slides/slide2.xml", &two), ("ppt/slides/slide1.xml", &one)]);
        assert_eq!(text(&zip, Office::PowerPoint).unwrap(), "# Slide 1\nTitle\n\n# Slide 2\nSecond\n\n# Slide 10\nLast\n\n");
    }

    #[test]
    fn what_isnt_a_zip_or_has_no_document_is_refused() {
        assert!(text(b"not a zip at all", Office::Word).is_err());
        assert!(text(&zip_of(&[("other.xml", "<a/>")]), Office::Word).is_err());
        assert_eq!(Office::of_name("Report.DOCX"), Some(Office::Word));
        assert_eq!(Office::of_name("old.doc"), None);
    }
}
