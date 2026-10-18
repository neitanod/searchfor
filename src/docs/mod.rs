//! The Extractors of `--docs` (spec §5.8): each turns a document ripgrep cannot read into text
//! and an Outline of that text, which the pipeline then searches and describes like a file
//! (M4 plan, design note 4). No external programs: lopdf for PDF, zip and an XML reader of our
//! own (`xml`, D12) for the Office, OpenDocument and EPUB formats, a reader of our own for SQLite.
pub mod epub;
#[doc(hidden)]
pub mod fixture;
pub mod isolated;
pub mod memory;
pub mod odf;
pub mod ooxml;
pub mod pdf;
pub mod sqlite;
pub mod table;
pub mod xml;
pub mod zipped;

use crate::trail::{Outline, OutlineSegment};

/// The most text one document may extract to (256 MiB): past it the text is cut, with a
/// warning. Its reader cuts it within its 1 GiB (D30): 256 MiB of PDF text took 689 MB; 512 MiB
/// took 1.75 GB, past the reader (M4 plan r4, Fable #5).
pub const MAX_TEXT: usize = 256 << 20;

/// The document Formats, named by `extractor` in their Definition's `[params]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Pdf,
    Docx,
    Xlsx,
    Pptx,
    Odt,
    Ods,
    Odp,
    Epub,
    Sqlite,
}

impl Kind {
    /// The name `from_name` reads.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Pdf => "pdf",
            Kind::Docx => "docx",
            Kind::Xlsx => "xlsx",
            Kind::Pptx => "pptx",
            Kind::Odt => "odt",
            Kind::Ods => "ods",
            Kind::Odp => "odp",
            Kind::Epub => "epub",
            Kind::Sqlite => "sqlite",
        }
    }

    pub fn from_name(name: &str) -> Option<Kind> {
        Some(match name {
            "pdf" => Kind::Pdf,
            "docx" => Kind::Docx,
            "xlsx" => Kind::Xlsx,
            "pptx" => Kind::Pptx,
            "odt" => Kind::Odt,
            "ods" => Kind::Ods,
            "odp" => Kind::Odp,
            "epub" => Kind::Epub,
            "sqlite" => Kind::Sqlite,
            _ => return None,
        })
    }

    /// Whether `head`, the first bytes of a file, can be a document of this kind: `%PDF-`, a
    /// zip's local header, SQLite's header string, each where the magic bytes of its Format
    /// look for it, at the start, so a document is one whatever its name (M4 plan r1, Fable
    /// #12). A file whose name
    /// says it is a document but whose bytes do not is not one, and is searched as ripgrep
    /// searches it, without a warning (M4 plan, D10).
    pub fn looks_like(self, head: &[u8]) -> bool {
        match self {
            Kind::Pdf => head.starts_with(b"%PDF-"),
            Kind::Sqlite => head.starts_with(b"SQLite format 3\0"),
            _ => head.starts_with(b"PK\x03\x04"),
        }
    }
}

/// Why a document gave no text. Each is one warning, `s4: <path>: <format>: <reason>`, and
/// the file is then searched as ripgrep searches it (M4 plan, D10).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DocError {
    #[error("encrypted, and the empty password does not open it")]
    Encrypted,
    #[error("{0}")]
    Broken(String),
    #[error("{0} is over the {1} MiB limit")]
    TooLarge(String, usize),
}

impl DocError {
    pub fn broken(e: impl std::fmt::Display) -> Self {
        DocError::Broken(e.to_string())
    }
}

/// What an Extractor returns: the text s4 searches, its Outline, and a warning when the text
/// was cut at `MAX_TEXT`.
#[derive(Debug, Clone, Default)]
pub struct Extracted {
    pub text: Vec<u8>,
    pub outline: Outline,
    pub cut: bool,
}

/// Extracts `bytes`, a whole document of `kind`.
pub fn extract(kind: Kind, bytes: &[u8]) -> Result<Extracted, DocError> {
    extract_with(kind, bytes, MAX_TEXT)
}

/// `extract` with another text limit (tests).
pub fn extract_with(kind: Kind, bytes: &[u8], max_text: usize) -> Result<Extracted, DocError> {
    let mut b = Builder::new(max_text);
    match kind {
        Kind::Pdf => pdf::extract(bytes, &mut b)?,
        Kind::Docx => ooxml::docx(bytes, &mut b)?,
        Kind::Xlsx => ooxml::xlsx(bytes, &mut b)?,
        Kind::Pptx => ooxml::pptx(bytes, &mut b)?,
        Kind::Odt => odf::odt(bytes, &mut b)?,
        Kind::Ods => odf::ods(bytes, &mut b)?,
        Kind::Odp => odf::odp(bytes, &mut b)?,
        Kind::Epub => epub::extract(bytes, &mut b)?,
        Kind::Sqlite => sqlite::extract(bytes, &mut b)?,
    }
    Ok(b.finish())
}

/// The ranks of `Builder::chapter` and `Builder::heading`: a chapter holds headings.
const CHAPTER: u8 = 0;
const HEADING: u8 = 1;

/// Builds the text and Outline of a document as an Extractor walks it. Text arrives a line at
/// a time; Segments open at the end of the text so far and close where it is when they close.
pub struct Builder {
    text: Vec<u8>,
    segments: Vec<OutlineSegment>,
    /// Open Segments: index into `segments`, and for chapters and headings their rank
    /// (`CHAPTER` outside `HEADING`) and level.
    open: Vec<(usize, Option<(u8, u8)>)>,
    max: usize,
    cut: bool,
    /// The row being built.
    row: Option<Row>,
    /// Line breaks in `text[..counted]`, for the line of each row.
    lines: u64,
    counted: usize,
}

struct Row {
    /// The row's Segment, when the row has one.
    seg: Option<usize>,
    line_start: usize,
    /// The cells written so far, empty ones included: each but the first writes its tab first.
    written: usize,
    /// Every cell so far is empty or white space: the row writes nothing.
    blank: bool,
    /// Each cell with text: its Segment's kind and name, and where its text starts.
    cells: Vec<(&'static str, String, usize)>,
}

/// A name on one line: its words, control characters counted as spaces, joined by one space.
fn one_line(s: &str) -> String {
    s.split(|c: char| c.is_whitespace() || c.is_control())
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// One line of text: control characters (line breaks, tabs, NUL) become spaces, so a line is
/// one line and the text never looks binary to the searcher.
fn clean(s: &str, out: &mut Vec<u8>) {
    for c in s.chars() {
        if c.is_control() {
            out.push(b' ');
        } else {
            let mut buf = [0u8; 4];
            out.extend(c.encode_utf8(&mut buf).as_bytes());
        }
    }
}

impl Builder {
    pub fn new(max: usize) -> Self {
        Builder {
            text: Vec::new(),
            segments: Vec::new(),
            open: Vec::new(),
            max,
            cut: false,
            row: None,
            lines: 0,
            counted: 0,
        }
    }

    /// The text reached its limit: the Extractor may stop walking.
    pub fn full(&self) -> bool {
        self.cut
    }

    /// The bytes of text the document has room for still.
    pub fn remaining(&self) -> usize {
        self.max.saturating_sub(self.text.len())
    }

    /// The Extractor stopped before the end of the document because its text would not fit:
    /// the text is cut.
    pub fn mark_cut(&mut self) {
        self.cut = true;
    }

    fn room(&mut self, more: usize) -> bool {
        if self.text.len() + more > self.max {
            self.cut = true;
        }
        !self.cut
    }

    /// Appends one line (its control characters as spaces). Empty lines are skipped.
    pub fn line(&mut self, s: &str) {
        let s = s.trim_end();
        if s.trim_start().is_empty() || !self.room(s.len() + 1) {
            return;
        }
        clean(s, &mut self.text);
        self.text.push(b'\n');
    }

    /// Appends one line as it is, with the spaces at its ends (an EPUB `pre`, or a line that
    /// ends in a no-break space), its control characters as spaces. A line of ASCII white space
    /// alone is skipped.
    pub fn verbatim(&mut self, s: &str) {
        if s.trim_matches(|c: char| c.is_ascii_whitespace()).is_empty() || !self.room(s.len() + 1)
        {
            return;
        }
        clean(s, &mut self.text);
        self.text.push(b'\n');
    }

    /// Opens a Segment that is not in the text (a page, a sheet, a slide, a table): `--within`
    /// finds it by name, `--in-headings` does not (M4 plan, D11). Returns its id for `close`.
    pub fn open(&mut self, kind: &str, name: &str) -> usize {
        let at = self.text.len();
        let id = self.segments.len();
        self.segments
            .push(OutlineSegment::new(kind, name, at..at, at..at));
        self.open.push((id, None));
        id
    }

    /// Closes Segment `id` and every Segment opened after it.
    pub fn close(&mut self, id: usize) {
        self.end_row();
        while let Some((top, _)) = self.open.pop() {
            self.segments[top].range.end = self.text.len();
            if top == id {
                break;
            }
        }
    }

    /// Closes, from the top of the stack, what a new entry of `rank` and `level` ends: entries
    /// of a higher rank (headings, when a chapter opens) and of its rank at its level or deeper.
    /// A Segment from `open` (a page, a table) stops it.
    fn close_levels(&mut self, rank: u8, level: u8) {
        while let Some(&(top, Some((r, l)))) = self.open.last() {
            if r < rank || r == rank && l < level {
                break;
            }
            self.open.pop();
            self.segments[top].range.end = self.text.len();
        }
    }

    /// A chapter of `level` (1 = top) whose title is not in the text, such as a PDF bookmark or
    /// an EPUB table-of-contents entry: closes the open chapters of that level or deeper and
    /// the headings inside them.
    pub fn chapter(&mut self, level: u8, title: &str) {
        let name = one_line(title);
        if name.is_empty() {
            return;
        }
        self.end_row();
        self.close_levels(CHAPTER, level);
        let at = self.text.len();
        let id = self.segments.len();
        let mut s = OutlineSegment::new("chapter", name, at..at, at..at);
        s.segment.level = Some(level);
        self.segments.push(s);
        self.open.push((id, Some((CHAPTER, level))));
    }

    /// A heading of `level` (1 = top): closes the open headings of that level or deeper, and
    /// writes `text` as a line that is the new heading's label and its searchable name, as a
    /// Markdown heading's. A heading that repeats the title of the chapter it opens in (an
    /// EPUB's nav entry and its `h1`) holds its line only, so the Trail names the chapter once;
    /// what follows nests in the chapter (pending C8 of M4).
    pub fn heading(&mut self, level: u8, text: &str) {
        let name = one_line(text);
        if name.is_empty() {
            return;
        }
        self.end_row();
        self.close_levels(HEADING, level);
        let repeats = matches!(self.open.last(), Some(&(top, Some((CHAPTER, _))))
            if self.segments[top].segment.name == name);
        let start = self.text.len();
        if !self.room(name.len() + 1) {
            return;
        }
        clean(&name, &mut self.text);
        let end = self.text.len();
        self.text.push(b'\n');
        let id = self.segments.len();
        let mut s = OutlineSegment::new("heading", name.clone(), start..start, start..end + 1);
        s.segment.level = Some(level);
        s.name = Some(vec![(0..name.len(), start..end)]);
        s.name_anchor = start;
        if repeats {
            s.range.end = end + 1;
            self.segments.push(s);
            return;
        }
        self.segments.push(s);
        self.open.push((id, Some((HEADING, level))));
    }

    /// Starts a row of a table, whose cells come with `cell` and are written as one line,
    /// separated by tabs: a Segment `row` named `name`, or no row Segment with None (a
    /// spreadsheet's cells carry the row in their own name, `B12`).
    pub fn start_row(&mut self, name: Option<&str>) {
        self.end_row();
        let at = self.text.len();
        let seg = name.map(|n| {
            self.segments
                .push(OutlineSegment::new("row", n, at..at, at..at));
            self.segments.len() - 1
        });
        self.row = Some(Row {
            seg,
            line_start: at,
            written: 0,
            blank: true,
            cells: Vec::new(),
        });
    }

    /// One cell of the current row, a Segment of `kind` (`column` in a table, `cell` in a
    /// spreadsheet) named `name`, whose Section is the row (M3 plan, D10). A cell holds the tab
    /// after it, so a Match on the tab belongs to the cell before it, as in CSV. Every cell but
    /// the first writes a tab before it, so an empty cell is an empty field (Codex r1 #12); a
    /// value is written as it is, its spaces included; an empty cell has no Segment.
    pub fn cell(&mut self, kind: &'static str, name: &str, text: &str) {
        if self.row.is_none() || !self.room(text.len() + 1) {
            return;
        }
        let row = self.row.as_mut().unwrap();
        if row.written > 0 {
            self.text.push(b'\t');
        }
        row.written += 1;
        let start = self.text.len();
        clean(text, &mut self.text);
        if !text.trim().is_empty() {
            row.blank = false;
        }
        if self.text.len() > start {
            // A cell without a name (a table's first row, which names the columns) has no
            // Segment; its text still makes the row non-empty.
            row.cells.push((kind, name.to_string(), start));
        }
    }

    /// Ends the current row, if any: its line, and a Segment per non-empty cell.
    pub fn end_row(&mut self) {
        let Some(row) = self.row.take() else { return };
        let end = self.text.len();
        if row.blank {
            // A row without text writes nothing (its tabs are removed) and has no Segment.
            self.text.truncate(row.line_start);
            return;
        }
        self.text.push(b'\n');
        self.lines +=
            memchr::memchr_iter(b'\n', &self.text[self.counted..row.line_start]).count() as u64;
        self.counted = row.line_start;
        let line = self.lines + 1;
        if let Some(seg) = row.seg {
            let s = &mut self.segments[seg];
            s.range = row.line_start..end + 1;
            s.segment.line = line;
            s.segment.end_line = line;
        }
        for (i, (kind, name, start)) in row.cells.iter().enumerate() {
            if name.is_empty() {
                continue;
            }
            // The cell's bytes up to the next cell, so its tab too; the last one, its line break.
            let stop = row.cells.get(i + 1).map_or(end + 1, |c| c.2);
            let mut s = OutlineSegment::new(kind, name.clone(), *start..stop, *start..*start);
            s.segment.line = line;
            s.segment.end_line = line;
            self.segments.push(s);
        }
    }

    pub fn finish(mut self) -> Extracted {
        self.end_row();
        let end = self.text.len();
        for (i, _) in self.open.drain(..) {
            self.segments[i].range.end = end;
        }
        // Segments that hold no text (an empty page, a sheet without values) are dropped.
        self.segments.retain(|s| s.range.start < s.range.end);
        Extracted {
            outline: Outline::build(self.segments, &self.text),
            text: self.text,
            cut: self.cut,
        }
    }
}

/// `A`, `B`, …, `Z`, `AA`: the letters of 0-based column `i`, as a spreadsheet names it.
pub fn column_letters(mut i: usize) -> String {
    let mut s = Vec::new();
    loop {
        s.push(b'A' + (i % 26) as u8);
        if i < 26 {
            break;
        }
        i = i / 26 - 1;
    }
    s.reverse();
    String::from_utf8(s).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fable r1 #12: a document is told by the bytes its Format's magic looks at, by name or
    /// not.
    #[test]
    fn documents_look_like_their_magic() {
        let defs = crate::formats::builtin::BUILTIN;
        for (kind, name, head) in [
            (Kind::Pdf, "pdf", &b"%PDF-1.7\n"[..]),
            (Kind::Sqlite, "sqlite", b"SQLite format 3\0"),
            (Kind::Docx, "docx", b"PK\x03\x04"),
        ] {
            let file = format!("{name}.toml");
            let def = defs.iter().find(|d| d.0 == file).map(|d| d.1).unwrap();
            let def = crate::formats::definition::FormatDefinition::parse(def, |_| true).unwrap();
            assert!(kind.looks_like(head), "{name}");
            assert!(def.magic.is_empty() || def.magic_at(head), "{name}");
            let late = [b"junk ".as_slice(), head].concat();
            assert!(!kind.looks_like(&late), "{name}");
        }
    }
    use crate::trail::trail_for;

    fn trail(e: &Extracted, needle: &str) -> Vec<String> {
        let at = memchr::memmem::find(&e.text, needle.as_bytes()).expect(needle);
        trail_for(&e.outline, at)
            .into_iter()
            .map(|s| s.name)
            .collect()
    }

    #[test]
    fn headings_nest_by_level_and_pages_hold_them() {
        let mut b = Builder::new(MAX_TEXT);
        let p = b.open("page", "page 1");
        b.heading(1, "Intro");
        b.line("first needle");
        b.heading(2, "Detail");
        b.line("second needle");
        b.heading(1, "Costs");
        b.line("third needle");
        b.close(p);
        let e = b.finish();
        assert_eq!(
            String::from_utf8(e.text.clone()).unwrap(),
            "Intro\nfirst needle\nDetail\nsecond needle\nCosts\nthird needle\n"
        );
        assert_eq!(trail(&e, "first"), ["page 1", "Intro"]);
        assert_eq!(trail(&e, "second"), ["page 1", "Intro", "Detail"]);
        assert_eq!(trail(&e, "third"), ["page 1", "Costs"]);
        // On the heading line itself, the heading is left out (its label).
        assert_eq!(trail(&e, "Detail"), ["page 1", "Intro"]);
        let names = crate::trail::names_of(&e.outline);
        assert_eq!(
            names.iter().map(|n| n.text.as_str()).collect::<Vec<_>>(),
            ["Intro", "Detail", "Costs"]
        );
    }

    #[test]
    fn rows_are_lines_of_tab_separated_cells_with_column_segments() {
        let mut b = Builder::new(MAX_TEXT);
        let s = b.open("sheet", "Budget");
        b.start_row(None);
        b.cell("cell", "A1", "Item");
        b.cell("cell", "B1", "Price");
        b.start_row(None);
        b.cell("cell", "A2", "pens needle");
        b.cell("cell", "B2", "1.50");
        b.start_row(None);
        b.cell("cell", "A3", "");
        b.cell("cell", "B3", " ");
        b.end_row();
        b.close(s);
        let t = b.open("table", "table 1");
        b.start_row(Some("row 1"));
        b.cell("column", "Name", "Ana");
        b.cell("column", "Note", "");
        b.cell("column", "Plan", "pro needle");
        b.close(t);
        let e = b.finish();
        assert_eq!(
            String::from_utf8(e.text.clone()).unwrap(),
            "Item\tPrice\npens needle\t1.50\nAna\t\tpro needle\n"
        );
        assert_eq!(trail(&e, "needle"), ["Budget", "A2"]);
        assert_eq!(trail(&e, "1.50"), ["Budget", "B2"]);
        assert_eq!(trail(&e, "pro"), ["table 1", "row 1", "Plan"]);
        // The tabs after a cell belong to that cell, also an empty cell's.
        let ana = memchr::memmem::find(&e.text, b"Ana").unwrap();
        let t: Vec<_> = trail_for(&e.outline, ana + 4)
            .into_iter()
            .map(|s| s.name)
            .collect();
        assert_eq!(t, ["table 1", "row 1", "Name"]);
        // The Section of a cell is its row.
        let pens = memchr::memmem::find(&e.text, b"pens").unwrap();
        let col = trail_for(&e.outline, pens).pop().unwrap();
        assert_eq!((col.line, col.end_line), (2, 2));
    }

    #[test]
    fn chapters_hold_pages_and_headings_and_close_by_level() {
        let mut b = Builder::new(MAX_TEXT);
        b.chapter(1, "1 Intro");
        let p = b.open("page", "page 1");
        b.line("alpha needle");
        b.close(p);
        b.chapter(2, "1.1 Scope");
        let p = b.open("page", "page 2");
        b.line("beta needle");
        b.close(p);
        b.chapter(1, "2 Costs");
        b.heading(1, "Totals");
        b.line("gamma needle");
        b.heading(1, "Taxes");
        b.line("delta needle");
        b.chapter(1, "3 End");
        b.line("omega needle");
        let e = b.finish();
        assert_eq!(trail(&e, "alpha"), ["1 Intro", "page 1"]);
        assert_eq!(trail(&e, "beta"), ["1 Intro", "1.1 Scope", "page 2"]);
        assert_eq!(trail(&e, "gamma"), ["2 Costs", "Totals"]);
        assert_eq!(trail(&e, "delta"), ["2 Costs", "Taxes"]);
        assert_eq!(trail(&e, "omega"), ["3 End"]);
        // Chapter titles are not in the text, so `--in-headings` does not search them.
        let names = crate::trail::names_of(&e.outline);
        assert_eq!(
            names.iter().map(|n| n.text.as_str()).collect::<Vec<_>>(),
            ["Totals", "Taxes"]
        );
    }

    /// Codex r1 #12: every cell writes its tab, also an empty one before the first value, and
    /// a value keeps its spaces.
    #[test]
    fn empty_cells_keep_their_tabs_and_values_their_spaces() {
        let mut b = Builder::new(MAX_TEXT);
        b.start_row(Some("row 1"));
        b.cell("column", "a", "");
        b.cell("column", "b", "");
        b.cell("column", "c", "  needle ");
        b.cell("column", "d", "");
        b.start_row(Some("row 2"));
        b.cell("column", "a", " ");
        b.cell("column", "b", "");
        b.end_row();
        let e = b.finish();
        assert_eq!(e.text, b"\t\t  needle \t\n");
        assert_eq!(trail(&e, "needle"), ["row 1", "c"]);
        let c = e
            .outline
            .segments
            .iter()
            .find(|s| s.segment.name == "c")
            .unwrap();
        // Up to the line break: the tabs of the empty cell after it are its own.
        assert_eq!(c.range, 2..13);
    }

    #[test]
    fn control_characters_become_spaces_and_empty_lines_are_skipped() {
        let mut b = Builder::new(MAX_TEXT);
        b.line("a\tb\0c\r");
        b.line("   ");
        b.line("d\ne");
        let e = b.finish();
        assert_eq!(e.text, b"a b c\nd e\n");
    }

    #[test]
    fn the_text_stops_at_its_limit_and_says_so() {
        let mut b = Builder::new(11);
        b.line("12345");
        b.line("6789");
        b.line("more");
        assert!(b.full());
        let e = b.finish();
        assert_eq!(e.text, b"12345\n6789\n");
        assert!(e.cut);
    }

    #[test]
    fn column_letters_are_a_spreadsheets() {
        assert_eq!(column_letters(0), "A");
        assert_eq!(column_letters(25), "Z");
        assert_eq!(column_letters(26), "AA");
        assert_eq!(column_letters(701), "ZZ");
        assert_eq!(column_letters(702), "AAA");
    }
}
