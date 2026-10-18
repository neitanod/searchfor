//! OpenDocument text, spreadsheets and presentations (spec §5.8), read from `content.xml`:
//! `2 Costs › table 1 › row 3 › Price`, `Budget › B12`, `slide 3 › notes`.
use super::table::Table;
use super::xml::{Ev, Pull, attr, local};
use super::zipped::Zip;
use super::{Builder, DocError, column_letters};
use std::collections::HashMap;

/// Repeated rows and cells with text are written at most this many times each (a repeat of
/// empty cells is only counted).
const MAX_REPEAT: usize = 1000;

/// `content.xml`, and the paragraph styles of it and of `styles.xml` that are headings.
fn content(bytes: &[u8]) -> Result<(String, HashMap<String, u8>), DocError> {
    let mut z = Zip::open(bytes)?;
    let content = z
        .text("content.xml")?
        .ok_or_else(|| DocError::broken("no content.xml"))?;
    let styles = z.text("styles.xml")?.unwrap_or_default();
    let mut parents = HashMap::new();
    let mut levels = HashMap::new();
    for xml in [&styles, &content] {
        for ev in Pull::new(xml) {
            if let Ev::Start {
                name: "style:style",
                attrs,
                ..
            } = ev
                && let Some(n) = attr(attrs, "name")
            {
                if let Some(p) = attr(attrs, "parent-style-name") {
                    parents.insert(n.to_string(), p.into_owned());
                }
                // The outline level, or the level of a built-in `Heading N` style (whose
                // name is `Heading_20_N` whatever the interface language).
                let level = attr(attrs, "default-outline-level")
                    .and_then(|l| l.parse::<u8>().ok())
                    .or_else(|| n.strip_prefix("Heading_20_").and_then(|d| d.parse().ok()));
                if let Some(l) = level.filter(|l| (1..=10).contains(l)) {
                    levels.insert(n.into_owned(), l);
                }
            }
        }
    }
    // A style is a heading when it or one of its parents has an outline level.
    let mut headings = HashMap::new();
    for name in parents.keys().chain(levels.keys()) {
        let mut cur = name.as_str();
        for _ in 0..16 {
            if let Some(l) = levels.get(cur) {
                headings.insert(name.clone(), *l);
                break;
            }
            match parents.get(cur) {
                Some(p) => cur = p,
                None => break,
            }
        }
    }
    Ok((content, headings))
}

/// Text paragraphs and headings of `office:text`, a slide or a frame: `text:h` with its
/// `outline-level`, `text:p`, spans, `text:s` (spaces), `text:tab`, `text:line-break`; notes
/// (footnotes) inside a paragraph as lines after it; tables as in Word.
struct Flow {
    /// Paragraph styles that make a `text:p` a heading, by name, with their level.
    headings: HashMap<String, u8>,
    paras: Vec<(String, Option<u8>)>,
    tables: Vec<Table>,
    count: usize,
    table_seg: Option<usize>,
    /// Depth inside elements whose text is not shown (a note's citation, an annotation,
    /// tracked deletions).
    skip: usize,
}

impl Flow {
    fn new(headings: HashMap<String, u8>) -> Self {
        Flow {
            headings,
            paras: Vec::new(),
            tables: Vec::new(),
            count: 0,
            table_seg: None,
            skip: 0,
        }
    }

    fn event(&mut self, ev: &Ev, b: &mut Builder) {
        match ev {
            Ev::Start { name, attrs, empty } => {
                let l = local(name);
                if self.skip > 0 || matches!(l, "note-citation" | "annotation" | "tracked-changes")
                {
                    if !empty {
                        self.skip += 1;
                    }
                    return;
                }
                match l {
                    "p" | "h" if !empty => {
                        let level = if l == "h" {
                            Some(
                                attr(attrs, "outline-level")
                                    .and_then(|v| v.parse().ok())
                                    .unwrap_or(1),
                            )
                        } else {
                            attr(attrs, "style-name")
                                .and_then(|s| self.headings.get(s.as_ref()).copied())
                        };
                        self.paras.push((String::new(), level));
                    }
                    "s" => {
                        let n = attr(attrs, "c")
                            .and_then(|v| v.parse::<usize>().ok())
                            .unwrap_or(1);
                        self.push(&" ".repeat(n.min(100)));
                    }
                    "tab" => self.push("\t"),
                    "line-break" => self.push("\n"),
                    "table" if !empty => {
                        if self.tables.is_empty() {
                            self.count += 1;
                            self.table_seg =
                                Some(b.open("table", &format!("table {}", self.count)));
                        }
                        self.tables.push(Table::default());
                    }
                    "table-cell" => {
                        if let Some(t) = self.tables.last_mut() {
                            let n = attr(attrs, "number-columns-repeated")
                                .and_then(|v| v.parse::<usize>().ok())
                                .unwrap_or(1);
                            // Repeated cells are empty but the last, which `p` fills.
                            for _ in 0..n.min(MAX_REPEAT) {
                                t.push_cell();
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ev::End { name } => {
                if self.skip > 0 {
                    self.skip -= 1;
                    return;
                }
                match local(name) {
                    "p" | "h" => {
                        let Some((text, level)) = self.paras.pop() else {
                            return;
                        };
                        if let Some(t) = self.tables.last_mut() {
                            t.add_text(&text);
                        } else if let Some(l) = level {
                            b.heading(l, &text);
                        } else {
                            for line in text.split('\n') {
                                b.line(line);
                            }
                        }
                    }
                    "table-row" => {
                        if self.tables.len() == 1 {
                            self.tables[0].end_row(b);
                        } else if let Some(t) = self.tables.last_mut() {
                            let row = t.take_row();
                            let n = self.tables.len();
                            self.tables[n - 2].add_text(&row);
                        }
                    }
                    "table" => {
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
                if self.skip == 0
                    && let Some(p) = self.paras.last_mut()
                {
                    // White space in ODF text is collapsed; `text:s` writes the spaces it means.
                    let mut last_space = p.0.ends_with(' ');
                    for c in t.chars() {
                        if c.is_whitespace() && c != '\u{a0}' {
                            if !last_space {
                                p.0.push(' ');
                            }
                            last_space = true;
                        } else {
                            p.0.push(c);
                            last_space = false;
                        }
                    }
                }
            }
        }
    }

    fn push(&mut self, s: &str) {
        if let Some(p) = self.paras.last_mut() {
            p.0.push_str(s);
        }
    }
}

pub fn odt(bytes: &[u8], b: &mut Builder) -> Result<(), DocError> {
    let (xml, headings) = content(bytes)?;
    let mut flow = Flow::new(headings);
    let mut body = false;
    for ev in Pull::new(&xml) {
        match &ev {
            Ev::Start { name, .. } if *name == "office:text" => body = true,
            Ev::End { name } if *name == "office:text" => body = false,
            _ if body => flow.event(&ev, b),
            _ => {}
        }
        if b.full() {
            break;
        }
    }
    Ok(())
}

pub fn odp(bytes: &[u8], b: &mut Builder) -> Result<(), DocError> {
    let (xml, _) = content(bytes)?;
    let mut flow = Flow::new(HashMap::new());
    let (mut n, mut slide, mut notes) = (0, None, None);
    for ev in Pull::new(&xml) {
        match &ev {
            Ev::Start {
                name, empty: false, ..
            } if *name == "draw:page" => {
                n += 1;
                slide = Some(b.open("slide", &format!("slide {n}")));
            }
            Ev::End { name } if *name == "draw:page" => {
                if let Some(s) = slide.take() {
                    b.close(s);
                }
            }
            Ev::Start {
                name, empty: false, ..
            } if *name == "presentation:notes" && slide.is_some() => {
                notes = Some(b.open("notes", "notes"));
            }
            Ev::End { name } if *name == "presentation:notes" => {
                if let Some(s) = notes.take() {
                    b.close(s);
                }
            }
            _ if slide.is_some() => flow.event(&ev, b),
            _ => {}
        }
        if b.full() {
            break;
        }
    }
    Ok(())
}

/// The value a spreadsheet cell stores, by its `office:value-type` (Codex r1 #6): a number,
/// percentage or currency as its `office:value`, a date or time as its ISO value, a boolean as
/// TRUE or FALSE, a string as its text (or the `office:string-value` of a formula). A formula's
/// value is its cached one. None: the cell's text, as for a cell without a type.
fn stored(attrs: &str) -> Option<String> {
    let v = |a: &str| attr(attrs, a).map(|s| s.into_owned());
    match attr(attrs, "value-type").as_deref()? {
        "float" | "percentage" | "currency" => v("value"),
        "date" => v("date-value"),
        "time" => v("time-value"),
        "boolean" => v("boolean-value").map(|b| {
            if b == "true" {
                "TRUE".into()
            } else {
                "FALSE".into()
            }
        }),
        _ => v("string-value"),
    }
}

/// `a + b`, or the error of a document whose repeat counts take a row or column number past
/// what a machine word holds (Codex r1 #10).
fn add(a: usize, b: usize) -> Result<usize, DocError> {
    a.checked_add(b)
        .ok_or_else(|| DocError::broken("a repeated row or column goes past the largest number"))
}

/// A repeat count: 1 when absent or not a number, an error when it is a number past the
/// largest one (M4 plan r2, Codex #6).
fn repeat(attrs: &str, name: &str) -> Result<usize, DocError> {
    let Some(v) = attr(attrs, name) else {
        return Ok(1);
    };
    match v.trim().parse::<usize>() {
        Ok(n) => Ok(n.max(1)),
        // By the error's kind, so `+18446744073709551616` is one too (M4 plan r3, Codex #7).
        Err(e) if *e.kind() == std::num::IntErrorKind::PosOverflow => Err(DocError::broken(
            "a repeated row or column goes past the largest number",
        )),
        Err(_) => Ok(1),
    }
}

pub fn ods(bytes: &[u8], b: &mut Builder) -> Result<(), DocError> {
    let (xml, _) = content(bytes)?;
    let mut sheet = None;
    let mut row = 0usize;
    let mut col = 0usize;
    // The row being read: its repeat count and its cells (column, repeat, text).
    let mut cells: Vec<(usize, usize, String)> = Vec::new();
    let mut repeat_row = 1usize;
    // The cell being read: its column, repeat count, text, stored value.
    let mut cell: Option<(usize, usize, String, Option<String>)> = None;
    let mut para_depth = 0usize;
    for ev in Pull::new(&xml) {
        match ev {
            Ev::Start { name, attrs, empty } => match name {
                "table:table" if !empty => {
                    let n = attr(attrs, "name")
                        .map(|s| s.into_owned())
                        .unwrap_or_default();
                    sheet = Some(b.open("sheet", &n));
                    row = 0;
                }
                "table:table-row" => {
                    repeat_row = repeat(attrs, "number-rows-repeated")?;
                    col = 0;
                    cells.clear();
                    if empty {
                        row = add(row, repeat_row)?;
                    }
                }
                "table:table-cell" | "table:covered-table-cell" => {
                    let rep = repeat(attrs, "number-columns-repeated")?;
                    let value = stored(attrs);
                    if empty {
                        if let Some(v) = value.filter(|v| !v.is_empty()) {
                            cells.push((col, rep, v));
                        }
                        col = add(col, rep)?;
                    } else {
                        cell = Some((col, rep, String::new(), value));
                    }
                }
                "text:p" if !empty => {
                    if let Some(c) = cell.as_mut()
                        && !c.2.is_empty()
                    {
                        c.2.push(' ');
                    }
                    para_depth += 1;
                }
                "text:s" => {
                    if let Some(c) = cell.as_mut() {
                        c.2.push(' ');
                    }
                }
                _ => {}
            },
            Ev::End { name } => match name {
                "text:p" => para_depth = para_depth.saturating_sub(1),
                "table:table-cell" | "table:covered-table-cell" => {
                    if let Some((c, rep, text, value)) = cell.take() {
                        let shown = value.unwrap_or(text);
                        if !shown.trim().is_empty() {
                            cells.push((c, rep, shown));
                        }
                        col = add(c, rep)?;
                    }
                }
                "table:table-row" => {
                    let writes = if cells.is_empty() {
                        0
                    } else {
                        repeat_row.min(MAX_REPEAT)
                    };
                    for k in 0..writes {
                        let r = add(row, k + 1)?;
                        b.start_row(None);
                        for (c, rep, text) in &cells {
                            for j in 0..(*rep).min(MAX_REPEAT) {
                                let at = add(*c, j)?;
                                b.cell("cell", &format!("{}{r}", column_letters(at)), text);
                            }
                        }
                        b.end_row();
                    }
                    row = add(row, repeat_row)?;
                }
                "table:table" => {
                    if let Some(s) = sheet.take() {
                        b.close(s);
                    }
                }
                _ => {}
            },
            Ev::Text(t) if para_depth > 0 => {
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
    Ok(())
}

#[cfg(test)]
mod tests {
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
    fn odt_headings_and_tables() {
        let d = fixture::odt(&[
            h(1, "1 Intro"),
            p("first needle"),
            h(2, "1.1 Scope"),
            table(&[&["Item", "Price"], &["Pens", "1.50 needle"]]),
        ]);
        assert_eq!(trail(Kind::Odt, &d, "first"), ["1 Intro"]);
        assert_eq!(
            trail(Kind::Odt, &d, "1.50"),
            ["1 Intro", "1.1 Scope", "table 1", "row 2", "Price"]
        );
    }

    #[test]
    fn ods_counts_repeated_rows_and_cells_without_writing_them() {
        let d = fixture::ods(&[(
            "Budget",
            &[&["Item", "Price"], &["Pens", "1.5"], &["", "", "needle"]],
        )]);
        let e = extract(Kind::Ods, &d).unwrap();
        // 1 048 570 repeated empty rows and 16 380 empty cells per row write nothing.
        assert!(e.text.len() < 100, "{}", e.text.len());
        assert_eq!(trail(Kind::Ods, &d, "needle"), ["Budget", "C3"]);
        assert_eq!(trail(Kind::Ods, &d, "1.5"), ["Budget", "B2"]);
    }

    fn ods_of(rows: &str) -> Vec<u8> {
        let body = format!(
            r#"<office:spreadsheet><table:table table:name="S">{rows}</table:table></office:spreadsheet>"#
        );
        fixture::zip(
            &[
                ("mimetype", b"application/vnd.oasis.opendocument.spreadsheet".to_vec()),
                ("content.xml", format!("<office:document-content><office:body>{body}</office:body></office:document-content>").into_bytes()),
            ],
            &["mimetype"],
        )
    }

    /// Codex r1 #6: a cell prints its stored value, by its `office:value-type`, not the text a
    /// format made of it; a formula's is its cached value.
    #[test]
    fn ods_cells_print_their_stored_values() {
        let d = ods_of(concat!(
            r#"<table:table-row>"#,
            r#"<table:table-cell office:value-type="currency" office:value="1.5"><text:p>$1.50</text:p></table:table-cell>"#,
            r#"<table:table-cell office:value-type="boolean" office:boolean-value="true"><text:p>VERDADERO</text:p></table:table-cell>"#,
            r#"<table:table-cell office:value-type="date" office:date-value="2026-10-18"><text:p>18/10/2026</text:p></table:table-cell>"#,
            r#"<table:table-cell office:value-type="time" office:time-value="PT01H00M00S"><text:p>01:00</text:p></table:table-cell>"#,
            r#"<table:table-cell table:formula="of:=1+1" office:value-type="float" office:value="2"><text:p>2,00</text:p></table:table-cell>"#,
            r#"<table:table-cell office:value-type="string"><text:p>needle</text:p></table:table-cell>"#,
            r#"</table:table-row>"#,
        ));
        let e = extract(Kind::Ods, &d).unwrap();
        assert_eq!(e.text, b"1.5\tTRUE\t2026-10-18\tPT01H00M00S\t2\tneedle\n");
        assert_eq!(trail(Kind::Ods, &d, "needle"), ["S", "F1"]);
    }

    /// Codex r1 #10: repeat counts near `usize::MAX` are an error of the document, never a
    /// panic or a wrapped row number.
    #[test]
    fn huge_repeats_are_an_error_not_an_overflow() {
        let max = usize::MAX;
        for rows in [
            format!(
                r#"<table:table-row table:number-rows-repeated="{max}"><table:table-cell/></table:table-row><table:table-row><table:table-cell office:value-type="string"><text:p>needle</text:p></table:table-cell></table:table-row>"#
            ),
            format!(
                r#"<table:table-row><table:table-cell table:number-columns-repeated="{max}"/><table:table-cell office:value-type="string"><text:p>needle</text:p></table:table-cell></table:table-row>"#
            ),
            format!(
                r#"<table:table-row table:number-rows-repeated="{max}"><table:table-cell office:value-type="string"><text:p>needle</text:p></table:table-cell></table:table-row><table:table-row><table:table-cell office:value-type="string"><text:p>x</text:p></table:table-cell></table:table-row>"#
            ),
            // Codex r2 #6: a count past the largest number is not one.
            r#"<table:table-row table:number-rows-repeated="18446744073709551616"><table:table-cell/></table:table-row>"#.to_string(),
            r#"<table:table-row><table:table-cell table:number-columns-repeated="18446744073709551616"/></table:table-row>"#.to_string(),
            // Codex r3 #7: with a sign too.
            r#"<table:table-row table:number-rows-repeated="+18446744073709551616"><table:table-cell/></table:table-row>"#.to_string(),
            r#"<table:table-row><table:table-cell table:number-columns-repeated="+18446744073709551616"/></table:table-row>"#.to_string(),
        ] {
            let got = std::panic::catch_unwind(|| extract(Kind::Ods, &ods_of(&rows)));
            assert!(
                matches!(got, Ok(Err(super::super::DocError::Broken(_)))),
                "{rows}"
            );
        }
    }

    #[test]
    fn odp_slides_and_notes() {
        let d = fixture::odp(&[
            (&["Title", "a needle margin"], Some("the needle joke")),
            (&["find the needle"], None),
        ]);
        assert_eq!(trail(Kind::Odp, &d, "margin"), ["slide 1"]);
        assert_eq!(trail(Kind::Odp, &d, "joke"), ["slide 1", "notes"]);
        assert_eq!(trail(Kind::Odp, &d, "find"), ["slide 2"]);
    }

    #[test]
    fn spaces_tabs_and_line_breaks_of_odf_text() {
        let body = r#"<office:text><text:p>a<text:s text:c="3"/>b<text:tab/>c<text:line-break/>d   e</text:p></office:text>"#;
        let d = fixture::zip(
            &[
                (
                    "mimetype",
                    b"application/vnd.oasis.opendocument.text".to_vec(),
                ),
                (
                    "content.xml",
                    format!("<office:document-content>{body}</office:document-content>")
                        .into_bytes(),
                ),
            ],
            &["mimetype"],
        );
        let e = extract(Kind::Odt, &d).unwrap();
        assert_eq!(e.text, b"a   b c\nd e\n");
    }
}
