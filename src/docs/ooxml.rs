//! Word, Excel and PowerPoint files (Office Open XML, spec §5.8): `2 Costs › table 1 › row 3 ›
//! Price` in a document, `Budget › B12` in a workbook, `slide 3 › notes` in a presentation.
//! Only the main parts are read: the document body, the sheets, the slides and their notes
//! (headers, footers, footnotes, comments and charts are left for later: M4 plan, D18).
use super::table::Table;
use super::xml::{Ev, Ns, Pull, attr, local, prefixed};
use super::zipped::{Zip, resolve};
use super::{Builder, DocError, column_letters};
use std::collections::HashMap;

/// Reads paragraphs and tables of WordprocessingML (`w:`) or DrawingML (`a:`), which share
/// their local names: `p`, `t`, `br`, `tab`, `tbl`, `tr`, `tc`. `heading` gives the level of a
/// paragraph from its `pStyle` value or its `outlineLvl` (Word only).
pub struct Flow<'s> {
    styles: &'s HashMap<String, u8>,
    /// The paragraphs being read, innermost last (a text box's paragraphs are inside one).
    paras: Vec<Para>,
    /// Depth inside `w:txbxContent`: a text box's paragraphs are lines of the box, never
    /// headings of the document (Fable M4 code r3 #2), as a `text:h` in an OpenDocument frame.
    boxes: usize,
    /// Inside `w:t` or `a:t`.
    in_text: bool,
    /// Depth inside elements whose text is not shown (`w:instrText`, `w:delText`,
    /// `mc:Fallback`, and the tab stops `w:tabs` of a paragraph's properties).
    skip: usize,
    tables: Vec<Table>,
    /// Top-level tables so far.
    count: usize,
    table_seg: Option<usize>,
}

/// A paragraph being read.
#[derive(Default)]
struct Para {
    text: String,
    level: Option<u8>,
    /// The lines of the text boxes it anchors, written after it, as in an OpenDocument text.
    after: Vec<String>,
    /// The tables open when it started: a paragraph inside it in a table opened after (one in
    /// a text box) belongs to that table's cell.
    tables: usize,
}

impl<'s> Flow<'s> {
    pub fn new(styles: &'s HashMap<String, u8>) -> Self {
        Flow {
            styles,
            paras: Vec::new(),
            boxes: 0,
            in_text: false,
            skip: 0,
            tables: Vec::new(),
            count: 0,
            table_seg: None,
        }
    }

    pub fn event(&mut self, ev: &Ev, b: &mut Builder) {
        match ev {
            Ev::Start {
                name,
                ns,
                attrs,
                empty,
            } => {
                let l = local(name);
                // By namespace, never by prefix (Codex M4 code r2 #1).
                if self.skip > 0
                    || matches!(l, "instrText" | "delText")
                    || matches!((ns, l), (Ns::Mc, "Fallback") | (Ns::Word, "tabs"))
                {
                    if !empty {
                        self.skip += 1;
                    }
                    return;
                }
                match l {
                    "txbxContent" if *ns == Ns::Word && !empty => self.boxes += 1,
                    "p" if !empty => self.paras.push(Para {
                        tables: self.tables.len(),
                        ..Para::default()
                    }),
                    "t" if !empty => self.in_text = true,
                    "br" | "cr" => self.push("\n"),
                    // A tab, or a positional tab (Codex M4 code r3 #3): a space, as a break.
                    "tab" | "ptab" if *ns == Ns::Word && self.paras.last().is_some() => {
                        self.push("\t")
                    }
                    "pStyle" | "outlineLvl" if self.boxes > 0 => {}
                    "pStyle" => {
                        if let (Some(v), Some(p)) = (attr(attrs, "val"), self.paras.last_mut()) {
                            p.level = self.styles.get(v.as_ref()).copied().or(p.level);
                        }
                    }
                    "outlineLvl" => {
                        if let (Some(v), Some(p)) = (attr(attrs, "val"), self.paras.last_mut())
                            && let Ok(n) = v.parse::<u8>()
                            && n < 9
                        {
                            p.level = Some(n + 1);
                        }
                    }
                    "tbl" if !empty => {
                        if self.tables.is_empty() {
                            self.count += 1;
                            self.table_seg =
                                Some(b.open("table", &format!("table {}", self.count)));
                        }
                        self.tables.push(Table::default());
                    }
                    "tc" if !empty => {
                        if let Some(t) = self.tables.last_mut() {
                            t.push_cell();
                        }
                    }
                    _ => {}
                }
            }
            Ev::End { name, ns } => {
                let l = local(name);
                if self.skip > 0 {
                    self.skip -= 1;
                    return;
                }
                match l {
                    "t" => self.in_text = false,
                    "txbxContent" if *ns == Ns::Word => self.boxes = self.boxes.saturating_sub(1),
                    "p" => {
                        let Some(para) = self.paras.pop() else {
                            return;
                        };
                        let tables = self.tables.len();
                        if let Some(outer) = self.paras.last_mut()
                            && outer.tables == tables
                        {
                            // A text box's paragraph comes after the one that anchors it.
                            outer.after.push(para.text);
                            outer.after.extend(para.after);
                            return;
                        }
                        self.end_paragraph(para.text, para.level, b);
                        for line in para.after {
                            self.end_paragraph(line, None, b);
                        }
                    }
                    "tr" => {
                        if self.tables.len() == 1 {
                            self.tables[0].end_row(b);
                        } else if let Some(t) = self.tables.last_mut() {
                            // A nested table's row: its text joins the cell holding the table.
                            let row = t.take_row();
                            self.add_to_outer_cell(&row);
                        }
                    }
                    "tbl" => {
                        self.tables.pop();
                        if self.tables.is_empty()
                            && let Some(s) = self.table_seg.take()
                        {
                            b.close(s);
                        }
                    }
                    _ => {}
                }
            }
            Ev::Text(t) => {
                if self.in_text && self.skip == 0 {
                    self.push(t);
                }
            }
        }
    }

    fn push(&mut self, s: &str) {
        if let Some(p) = self.paras.last_mut() {
            p.text.push_str(s);
        }
    }

    /// Appends to the innermost table cell being read.
    fn add_to_outer_cell(&mut self, s: &str) {
        let n = self.tables.len();
        // The cell that holds the innermost table is in the table before it.
        if n >= 2 {
            self.tables[n - 2].add_text(s);
        }
    }

    fn end_paragraph(&mut self, text: String, level: Option<u8>, b: &mut Builder) {
        if let Some(t) = self.tables.last_mut() {
            t.add_text(&text);
            return;
        }
        // A paragraph is one line: its manual line breaks become spaces (spec §5.8).
        match level {
            Some(l) => b.heading(l, &text),
            None => b.line(&text),
        }
    }
}

fn walk(xml: &str, flow: &mut Flow, b: &mut Builder) {
    for ev in Pull::new(xml) {
        flow.event(&ev, b);
        if b.full() {
            break;
        }
    }
}

/// Paragraph styles that are headings, by id: their name is `heading N` (Word writes built-in
/// style names in English whatever the language of its interface), or they carry an outline
/// level.
fn heading_styles(xml: &str) -> HashMap<String, u8> {
    let mut out = HashMap::new();
    let mut id: Option<String> = None;
    for ev in Pull::new(xml) {
        match ev {
            Ev::Start { name, attrs, .. } => match local(name) {
                "style" => id = attr(attrs, "styleId").map(|s| s.into_owned()),
                "name" => {
                    if let (Some(i), Some(v)) = (&id, attr(attrs, "val")) {
                        let v = v.to_ascii_lowercase();
                        if let Some(n) = v
                            .strip_prefix("heading ")
                            .and_then(|n| n.parse::<u8>().ok())
                            && (1..=9).contains(&n)
                        {
                            out.insert(i.clone(), n);
                        }
                    }
                }
                "outlineLvl" => {
                    if let (Some(i), Some(v)) = (&id, attr(attrs, "val"))
                        && let Ok(n) = v.parse::<u8>()
                        && n < 9
                    {
                        out.entry(i.clone()).or_insert(n + 1);
                    }
                }
                _ => {}
            },
            Ev::End { name, .. } if local(name) == "style" => id = None,
            _ => {}
        }
    }
    out
}

pub fn docx(bytes: &[u8], b: &mut Builder) -> Result<(), DocError> {
    let mut z = Zip::open(bytes)?;
    let main = main_part(&mut z, "word/document.xml")?;
    let doc = z
        .text(&main)?
        .ok_or_else(|| DocError::broken("no document part (word/document.xml)"))?;
    let styles = z
        .text("word/styles.xml")?
        .map(|s| heading_styles(&s))
        .unwrap_or_default();
    let mut flow = Flow::new(&styles);
    walk(&doc, &mut flow, b);
    Ok(())
}

/// The main part named by `_rels/.rels` (`officeDocument`), else `default`.
fn main_part(z: &mut Zip, default: &str) -> Result<String, DocError> {
    let Some(rels) = z.text("_rels/.rels")? else {
        return Ok(default.into());
    };
    Ok(relationships(&rels, "")
        .into_iter()
        .find(|r| r.1.ends_with("/officeDocument"))
        .map_or(default.into(), |r| r.2))
}

/// (Id, Type, target part) of a relationships part; targets resolve against `base`, the
/// part the relationships are of.
fn relationships(xml: &str, base: &str) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    for ev in Pull::new(xml) {
        if let Ev::Start { name, attrs, .. } = ev
            && local(name) == "Relationship"
            && attr(attrs, "TargetMode").as_deref() != Some("External")
            && let (Some(id), Some(ty), Some(target)) = (
                attr(attrs, "Id"),
                attr(attrs, "Type"),
                attr(attrs, "Target"),
            )
        {
            out.push((id.into_owned(), ty.into_owned(), resolve(base, &target)));
        }
    }
    out
}

/// `xl/workbook.xml` → `xl/_rels/workbook.xml.rels`.
fn rels_of(part: &str) -> String {
    match part.rsplit_once('/') {
        Some((dir, file)) => format!("{dir}/_rels/{file}.rels"),
        None => format!("_rels/{part}.rels"),
    }
}

pub fn xlsx(bytes: &[u8], b: &mut Builder) -> Result<(), DocError> {
    let mut z = Zip::open(bytes)?;
    let book_part = main_part(&mut z, "xl/workbook.xml")?;
    let book = z
        .text(&book_part)?
        .ok_or_else(|| DocError::broken("no workbook part (xl/workbook.xml)"))?;
    let rels = z.text(&rels_of(&book_part))?.unwrap_or_default();
    let rels = relationships(&rels, &book_part);
    // By id: a workbook may have thousands of sheets, and a lookup per sheet must not scan them.
    let targets: HashMap<&str, String> = rels.iter().map(|r| (r.0.as_str(), r.2.clone())).collect();
    let shared = match rels.iter().find(|r| r.1.ends_with("/sharedStrings")) {
        Some(r) => z
            .text(&r.2)?
            .map(|s| shared_strings(&s))
            .unwrap_or_default(),
        None => Vec::new(),
    };
    for ev in Pull::new(&book) {
        let Ev::Start { name, attrs, .. } = ev else {
            continue;
        };
        if local(name) != "sheet" {
            continue;
        }
        let (Some(sheet), Some(id)) = (attr(attrs, "name"), prefixed(attrs, "id")) else {
            continue;
        };
        let Some(target) = targets.get(id.as_ref()).cloned() else {
            continue;
        };
        let Some(xml) = z.text(&target)? else {
            continue;
        };
        let s = b.open("sheet", &sheet);
        cells(&xml, &shared, b);
        b.close(s);
        if b.full() {
            break;
        }
    }
    Ok(())
}

fn shared_strings(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let (mut in_t, mut phonetic) = (false, false);
    for ev in Pull::new(xml) {
        match ev {
            Ev::Start { name, empty, .. } => match local(name) {
                "si" => cur.clear(),
                "t" if !empty => in_t = true,
                "rPh" => phonetic = !empty,
                _ => {}
            },
            Ev::End { name, .. } => match local(name) {
                "si" => out.push(std::mem::take(&mut cur)),
                "t" => in_t = false,
                "rPh" => phonetic = false,
                _ => {}
            },
            Ev::Text(t) if in_t && !phonetic => cur.push_str(&t),
            _ => {}
        }
    }
    out
}

/// The rows of a worksheet: each row a line, each cell a `cell` Segment named by its
/// reference (`B12`); a shared string by its text, a boolean as TRUE or FALSE, anything else as
/// its value as stored (a date is its serial number: M4 plan, D18).
fn cells(xml: &str, shared: &[String], b: &mut Builder) {
    let mut row_n = 0u64;
    let mut col = 0usize;
    // The cell being read: its reference, type, and value text.
    let mut cell: Option<(String, String, String)> = None;
    let mut in_v = false;
    for ev in Pull::new(xml) {
        match ev {
            Ev::Start {
                name, attrs, empty, ..
            } => match local(name) {
                "row" => {
                    row_n = attr(attrs, "r")
                        .and_then(|r| r.parse().ok())
                        .unwrap_or(row_n.saturating_add(1));
                    col = 0;
                    b.start_row(None);
                    if empty {
                        b.end_row();
                    }
                }
                "c" => {
                    let r = attr(attrs, "r").map(|r| r.into_owned());
                    if let Some(c) = r.as_deref().and_then(column_of) {
                        col = c;
                    }
                    let r = r.unwrap_or_else(|| format!("{}{row_n}", column_letters(col)));
                    col += 1;
                    let ty = attr(attrs, "t").map(|t| t.into_owned()).unwrap_or_default();
                    cell = (!empty).then(|| (r, ty, String::new()));
                }
                "v" | "t" if !empty => in_v = true,
                _ => {}
            },
            Ev::End { name, .. } => match local(name) {
                "v" | "t" => in_v = false,
                "c" => {
                    if let Some((r, ty, v)) = cell.take() {
                        let shown = match ty.as_str() {
                            "s" => v
                                .trim()
                                .parse::<usize>()
                                .ok()
                                .and_then(|i| shared.get(i))
                                .cloned()
                                .unwrap_or_default(),
                            "b" => {
                                if v.trim() == "1" {
                                    "TRUE".into()
                                } else {
                                    "FALSE".into()
                                }
                            }
                            _ => v,
                        };
                        b.cell("cell", &r, &shown);
                    }
                }
                "row" => b.end_row(),
                _ => {}
            },
            Ev::Text(t) if in_v => {
                if let Some(c) = cell.as_mut() {
                    c.2.push_str(&t);
                }
            }
            _ => {}
        }
        if b.full() {
            break;
        }
    }
}

/// The 0-based column of a cell reference: `B12` → 1.
fn column_of(r: &str) -> Option<usize> {
    let letters: Vec<u8> = r.bytes().take_while(u8::is_ascii_alphabetic).collect();
    if letters.is_empty() || letters.len() > 3 {
        return None;
    }
    let n = letters.iter().fold(0usize, |a, c| {
        a * 26 + (c.to_ascii_uppercase() - b'A' + 1) as usize
    });
    Some(n - 1)
}

pub fn pptx(bytes: &[u8], b: &mut Builder) -> Result<(), DocError> {
    let mut z = Zip::open(bytes)?;
    let pres_part = main_part(&mut z, "ppt/presentation.xml")?;
    let pres = z
        .text(&pres_part)?
        .ok_or_else(|| DocError::broken("no presentation part (ppt/presentation.xml)"))?;
    let rels = z.text(&rels_of(&pres_part))?.unwrap_or_default();
    let rels = relationships(&rels, &pres_part);
    // By id: 20 000 slides searched one by one in the list took 1.7 s (M4 probe).
    let targets: HashMap<&str, String> = rels.iter().map(|r| (r.0.as_str(), r.2.clone())).collect();
    let none = HashMap::new();
    let mut n = 0;
    for ev in Pull::new(&pres) {
        let Ev::Start { name, attrs, .. } = ev else {
            continue;
        };
        if local(name) != "sldId" {
            continue;
        }
        // `<p:sldId id="256" r:id="rId2"/>`: the relationship is the prefixed `id`.
        let Some(rid) = prefixed(attrs, "id") else {
            continue;
        };
        let Some(target) = targets.get(rid.as_ref()).cloned() else {
            continue;
        };
        let Some(xml) = z.text(&target)? else {
            continue;
        };
        n += 1;
        let s = b.open("slide", &format!("slide {n}"));
        let mut flow = Flow::new(&none);
        walk(&xml, &mut flow, b);
        let srels = z.text(&rels_of(&target))?.unwrap_or_default();
        if let Some(notes) = relationships(&srels, &target)
            .into_iter()
            .find(|r| r.1.ends_with("/notesSlide"))
            && let Some(xml) = z.text(&notes.2)?
        {
            let ns = b.open("notes", "notes");
            notes_body(&xml, b);
            b.close(ns);
        }
        b.close(s);
        if b.full() {
            break;
        }
    }
    Ok(())
}

/// The text of a notes slide's shapes, except the placeholders of the slide image, its number,
/// the date, the header and the footer.
fn notes_body(xml: &str, b: &mut Builder) {
    let none = HashMap::new();
    // The events of the shape being read, and whether it is kept.
    let mut shape: Option<(Vec<Ev>, bool)> = None;
    for ev in Pull::new(xml) {
        match &ev {
            Ev::Start { name, .. } if local(name) == "sp" => shape = Some((Vec::new(), true)),
            Ev::Start { name, attrs, .. } if local(name) == "ph" => {
                if let Some(s) = shape.as_mut() {
                    let ty = attr(attrs, "type");
                    s.1 = !matches!(
                        ty.as_deref(),
                        Some("sldImg" | "sldNum" | "dt" | "hdr" | "ftr")
                    );
                }
            }
            Ev::End { name, .. } if local(name) == "sp" => {
                if let Some((evs, true)) = shape.take() {
                    let mut flow = Flow::new(&none);
                    for e in &evs {
                        flow.event(e, b);
                    }
                }
                continue;
            }
            _ => {}
        }
        if let Some(s) = shape.as_mut() {
            s.0.push(ev);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heading_styles_come_from_names_and_outline_levels() {
        let x = r#"<w:styles><w:style w:type="paragraph" w:styleId="Ttulo1"><w:name w:val="heading 1"/></w:style><w:style w:styleId="Mine"><w:name w:val="Mine"/><w:pPr><w:outlineLvl w:val="2"/></w:pPr></w:style><w:style w:styleId="Normal"><w:name w:val="Normal"/></w:style></w:styles>"#;
        let s = heading_styles(x);
        assert_eq!(s.get("Ttulo1"), Some(&1));
        assert_eq!(s.get("Mine"), Some(&3));
        assert_eq!(s.get("Normal"), None);
    }

    #[test]
    fn column_references() {
        assert_eq!(column_of("B12"), Some(1));
        assert_eq!(column_of("AA3"), Some(26));
        assert_eq!(column_of("12"), None);
    }

    #[test]
    fn relationship_targets_resolve_against_their_part() {
        let r = r#"<Relationships><Relationship Id="rId1" Type="http://x/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId9" Type="http://x/hyperlink" Target="http://e.com" TargetMode="External"/></Relationships>"#;
        assert_eq!(
            relationships(r, "xl/workbook.xml"),
            [(
                "rId1".into(),
                "http://x/worksheet".into(),
                "xl/worksheets/sheet1.xml".into()
            )]
        );
        assert_eq!(rels_of("xl/workbook.xml"), "xl/_rels/workbook.xml.rels");
    }
}

#[cfg(test)]
mod extract_tests {
    use super::super::fixture::{self, h, p, table};
    use super::super::{Kind, extract};
    use crate::trail::trail_for;

    fn trail(kind: Kind, doc: &[u8], needle: &str) -> Vec<String> {
        let e = extract(kind, doc).unwrap();
        let at = memchr::memmem::find(&e.text, needle.as_bytes())
            .unwrap_or_else(|| panic!("{needle} not in {:?}", String::from_utf8_lossy(&e.text)));
        trail_for(&e.outline, at)
            .into_iter()
            .map(|s| s.name)
            .collect()
    }

    #[test]
    fn docx_headings_and_tables() {
        let d = fixture::docx(&[
            h(1, "1 Intro"),
            p("first needle"),
            h(2, "1.1 Scope"),
            p("second needle"),
            h(1, "2 Costs"),
            table(&[&["Item", "Price"], &["Pens", "1.50 needle"]]),
            p("after the table needle"),
        ]);
        assert_eq!(trail(Kind::Docx, &d, "first"), ["1 Intro"]);
        assert_eq!(trail(Kind::Docx, &d, "second"), ["1 Intro", "1.1 Scope"]);
        assert_eq!(
            trail(Kind::Docx, &d, "1.50"),
            ["2 Costs", "table 1", "row 2", "Price"]
        );
        // The first row names the columns: a Match in it is in no column.
        assert_eq!(
            trail(Kind::Docx, &d, "Item"),
            ["2 Costs", "table 1", "row 1"]
        );
        assert_eq!(trail(Kind::Docx, &d, "after"), ["2 Costs"]);
        let e = extract(Kind::Docx, &d).unwrap();
        assert_eq!(
            String::from_utf8(e.text).unwrap(),
            "1 Intro\nfirst needle\n1.1 Scope\nsecond needle\n2 Costs\nItem\tPrice\nPens\t1.50 needle\nafter the table needle\n"
        );
    }

    /// Codex r8 #2: an empty row counts (spec §5.8 counts from the table's first row), the
    /// first physical row names the columns even when it is empty, and an empty row still
    /// writes no line.
    #[test]
    fn docx_empty_rows_count_and_the_first_row_names_the_columns() {
        let d = fixture::docx(&[table(&[
            &["Name", "Value"],
            &["", ""],
            &["needle", "value"],
        ])]);
        assert_eq!(
            trail(Kind::Docx, &d, "needle"),
            ["table 1", "row 3", "Name"]
        );
        let e = extract(Kind::Docx, &d).unwrap();
        assert_eq!(e.text, b"Name\tValue\nneedle\tvalue\n");
        let d = fixture::docx(&[table(&[
            &["", ""],
            &["Name", "Value"],
            &["needle", "value"],
        ])]);
        assert_eq!(
            trail(Kind::Docx, &d, "Name"),
            ["table 1", "row 2", "column 1"]
        );
        assert_eq!(
            trail(Kind::Docx, &d, "value"),
            ["table 1", "row 3", "column 2"]
        );
        let e = extract(Kind::Docx, &d).unwrap();
        assert_eq!(e.text, b"Name\tValue\nneedle\tvalue\n");
    }

    #[test]
    fn xlsx_cells_are_named_by_their_reference() {
        let d = fixture::xlsx(&[
            (
                "Budget",
                &[&["Item", "Price"], &["Pens", "1.5"], &["needle", "", "x"]],
            ),
            ("Other", &[&["", "the needle here"]]),
        ]);
        assert_eq!(trail(Kind::Xlsx, &d, "needle"), ["Budget", "A3"]);
        assert_eq!(trail(Kind::Xlsx, &d, "1.5"), ["Budget", "B2"]);
        assert_eq!(trail(Kind::Xlsx, &d, "x"), ["Budget", "C3"]);
        assert_eq!(trail(Kind::Xlsx, &d, "the needle"), ["Other", "B1"]);
    }

    #[test]
    fn pptx_slides_and_their_notes() {
        let d = fixture::pptx(&[
            (
                &["Quarterly results", "a needle margin"],
                Some("say the needle joke"),
            ),
            (&["Next steps", "find the needle"], None),
        ]);
        assert_eq!(trail(Kind::Pptx, &d, "margin"), ["slide 1"]);
        assert_eq!(trail(Kind::Pptx, &d, "joke"), ["slide 1", "notes"]);
        assert_eq!(trail(Kind::Pptx, &d, "find"), ["slide 2"]);
        // The slide-image placeholder of the notes page is left out.
        let e = extract(Kind::Pptx, &d).unwrap();
        assert!(memchr::memmem::find(&e.text, b"slide image").is_none());
    }

    /// Codex r1 #9: a manual line break inside a paragraph (`w:br`, `a:br`) is a space, so
    /// a paragraph is one line (spec §5.8) and the lines after it keep their numbers.
    #[test]
    fn a_line_break_inside_a_paragraph_keeps_it_one_line() {
        let d = fixture::docx(&[p("one BREAK two"), p("after needle")]);
        let d = fixture::rezip(&d, |name, data| {
            if name != "word/document.xml" {
                return data;
            }
            String::from_utf8(data)
                .unwrap()
                .replace(" BREAK ", "</w:t><w:br/><w:t>")
                .into_bytes()
        });
        let e = extract(Kind::Docx, &d).unwrap();
        assert_eq!(e.text, b"one two\nafter needle\n");
        let d = fixture::pptx(&[(&["one BREAK two"], None)]);
        let d = fixture::rezip(&d, |name, data| {
            if name != "ppt/slides/slide1.xml" {
                return data;
            }
            String::from_utf8(data)
                .unwrap()
                .replace(" BREAK ", "</a:t></a:r><a:br/><a:r><a:t>")
                .into_bytes()
        });
        let e = extract(Kind::Pptx, &d).unwrap();
        assert_eq!(e.text, b"one two\n");
    }

    /// Codex M4 code r2 #1, in Word: the two names the reader knows by their prefix, the tab
    /// `w:tab` and the `mc:Fallback` it skips, are known by their namespace, so a document
    /// that binds them to other prefixes reads the same. A tab stop of the paragraph's
    /// properties is not a tab of its text.
    #[test]
    fn word_names_are_known_by_namespace_not_by_prefix() {
        let usual = fixture::docx(&[p("one TAB two"), p("ALT"), p("STOP after needle")]);
        let usual = fixture::rezip(&usual, |name, data| {
            if name != "word/document.xml" {
                return data;
            }
            String::from_utf8(data)
                .unwrap()
                .replace(" TAB ", "</w:t><w:tab/><w:t>")
                .replace(
                    "<w:r><w:t xml:space=\"preserve\">ALT</w:t></w:r>",
                    concat!(
                        r#"<mc:AlternateContent xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006">"#,
                        r#"<mc:Choice Requires="wps"><w:r><w:t>choice</w:t></w:r></mc:Choice>"#,
                        r#"<mc:Fallback><w:r><w:t>fallback</w:t></w:r></mc:Fallback></mc:AlternateContent>"#
                    ),
                )
                .replace(
                    "<w:p><w:r><w:t xml:space=\"preserve\">STOP ",
                    r#"<w:p><w:pPr><w:tabs><w:tab w:val="left" w:pos="720"/></w:tabs></w:pPr><w:r><w:t xml:space="preserve">"#,
                )
                .into_bytes()
        });
        let e = extract(Kind::Docx, &usual).unwrap();
        // The builder writes a tab as a space, as in the rest of the text.
        assert_eq!(e.text, b"one two\nchoice\nafter needle\n");
        let other = fixture::rezip(&usual, |name, data| {
            if name != "word/document.xml" {
                return data;
            }
            String::from_utf8(data)
                .unwrap()
                .replace("w:", "x:")
                .replace("xmlns:w=", "xmlns:x=")
                .replace("mc:", "m:")
                .replace("xmlns:mc=", "xmlns:m=")
                .into_bytes()
        });
        assert_eq!(extract(Kind::Docx, &other).unwrap().text, e.text);
    }

    /// A Word document whose body is `body`, with the fixture's heading styles.
    fn docx_with(body: &str) -> Vec<u8> {
        let d = fixture::docx(&[p("BODY")]);
        fixture::rezip(&d, |name, data| {
            if name != "word/document.xml" {
                return data;
            }
            String::from_utf8(data)
                .unwrap()
                .replace(
                    r#"<w:p><w:r><w:t xml:space="preserve">BODY</w:t></w:r></w:p>"#,
                    body,
                )
                .into_bytes()
        })
    }

    /// Codex M4 code r3 #3, in Word: the same rule as in OpenDocument and EPUB: a tab
    /// (`w:tab`, `w:ptab`), a carriage return `w:cr` and a break `w:br` inside a paragraph are a
    /// space each, in the body and in a table cell.
    #[test]
    fn word_tabs_and_breaks_are_a_space_each() {
        const P: &str = r#"<w:p><w:r><w:t>a</w:t><w:tab/><w:t>b</w:t><w:ptab w:relativeTo="margin" w:alignment="right" w:leader="none"/><w:t>c</w:t><w:cr/><w:t>d</w:t><w:br w:type="page"/><w:t>e</w:t></w:r></w:p>"#;
        let d = docx_with(&format!(
            "{P}<w:tbl><w:tr><w:tc><w:p><w:r><w:t>Item</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc>{P}</w:tc></w:tr></w:tbl>"
        ));
        assert_eq!(
            extract(Kind::Docx, &d).unwrap().text,
            b"a b c d e\nItem\na b c d e\n"
        );
    }

    /// Fable M4 code r3 #2: a paragraph inside a text box (`w:txbxContent`, which LibreOffice
    /// and Word write inside a run of the paragraph that anchors it) is a line of the box, never
    /// a heading of the document, whatever its style, and the box's lines come after the
    /// anchoring paragraph, as in an OpenDocument text.
    #[test]
    fn a_text_boxs_heading_is_a_line_of_the_box() {
        let d = docx_with(concat!(
            r#"<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Intro</w:t></w:r></w:p>"#,
            r#"<w:p><w:pPr><w:pStyle w:val="Heading2"/></w:pPr><w:r><w:t>Scope</w:t></w:r></w:p>"#,
            r#"<w:p><w:r><w:t>anchor after</w:t></w:r><w:r><mc:AlternateContent xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"><mc:Choice Requires="wps"><w:drawing><wps:wsp><wps:txbx><w:txbxContent>"#,
            r#"<w:p><w:pPr><w:pStyle w:val="Heading3"/></w:pPr><w:r><w:t>Box head</w:t></w:r></w:p>"#,
            r#"<w:p><w:pPr><w:outlineLvl w:val="0"/></w:pPr><w:r><w:t>box needle</w:t></w:r></w:p>"#,
            r#"</w:txbxContent></wps:txbx></wps:wsp></w:drawing></mc:Choice>"#,
            r#"<mc:Fallback><w:pict><v:textbox><w:txbxContent><w:p><w:r><w:t>fallback</w:t></w:r></w:p></w:txbxContent></v:textbox></w:pict></mc:Fallback></mc:AlternateContent></w:r></w:p>"#,
            r#"<w:p><w:r><w:t>last needle</w:t></w:r></w:p>"#,
        ));
        let e = extract(Kind::Docx, &d).unwrap();
        assert_eq!(
            String::from_utf8_lossy(&e.text),
            "Intro\nScope\nanchor after\nBox head\nbox needle\nlast needle\n"
        );
        assert_eq!(trail(Kind::Docx, &d, "last needle"), ["Intro", "Scope"]);
        assert_eq!(trail(Kind::Docx, &d, "box needle"), ["Intro", "Scope"]);
        assert_eq!(trail(Kind::Docx, &d, "anchor"), ["Intro", "Scope"]);
        // A table inside a text box keeps its cells.
        let d = docx_with(concat!(
            r#"<w:p><w:r><w:t>anchor</w:t></w:r><w:r><w:drawing><wps:txbx><w:txbxContent><w:tbl>"#,
            r#"<w:tr><w:tc><w:p><w:r><w:t>Item</w:t></w:r></w:p></w:tc></w:tr>"#,
            r#"<w:tr><w:tc><w:p><w:r><w:t>box cell needle</w:t></w:r></w:p></w:tc></w:tr>"#,
            r#"</w:tbl></w:txbxContent></wps:txbx></w:drawing></w:r></w:p>"#,
        ));
        assert_eq!(
            trail(Kind::Docx, &d, "box cell"),
            ["table 1", "row 2", "Item"]
        );
    }

    #[test]
    fn a_zip_that_is_not_a_document_is_an_error() {
        let z = fixture::zip(&[("a.txt", b"needle".to_vec())], &[]);
        assert!(extract(Kind::Docx, &z).is_err());
        assert!(extract(Kind::Xlsx, &z).is_err());
        assert!(extract(Kind::Pptx, &z).is_err());
        assert!(extract(Kind::Docx, b"PK\x03\x04 truncated").is_err());
    }
}
