//! OpenDocument text, spreadsheets and presentations (spec §5.8), read from `content.xml`:
//! `2 Costs › table 1 › row 3 › Price`, `Budget › B12`, `slide 3 › notes`.
use super::table::Table;
use super::xml::{Ev, Ns, Pull, attr, local};
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
                name,
                ns: Ns::Style,
                attrs,
                ..
            } = ev
                && local(name) == "style"
                && let Some(n) = attr(attrs, "name")
            {
                if let Some(p) = attr(attrs, "parent-style-name") {
                    parents.insert(n.to_string(), p.into_owned());
                }
                // The outline level, or the level of a built-in `Heading N` style (whose
                // name is `Heading_20_N` whatever the interface language).
                let level = attr(attrs, "default-outline-level")
                    .and_then(|l| l.parse::<u8>().ok())
                    .filter(|l| (1..=10).contains(l))
                    .or_else(|| built_in(&n));
                if let Some(l) = level {
                    levels.insert(n.into_owned(), l);
                }
            }
        }
    }
    // A style is a heading when it or one of its parents has an outline level or is a
    // built-in `Heading N`, declared or not (Fable r1 #6).
    let mut headings = HashMap::new();
    for name in parents.keys().chain(levels.keys()) {
        let mut cur = name.as_str();
        for _ in 0..16 {
            if let Some(l) = levels.get(cur).copied().or_else(|| built_in(cur)) {
                headings.insert(name.clone(), l);
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

/// The level of the built-in `Heading N` style, named `Heading_20_N` whatever the interface
/// language.
fn built_in(style: &str) -> Option<u8> {
    style
        .strip_prefix("Heading_20_")
        .and_then(|d| d.parse().ok())
        .filter(|l| (1..=10).contains(l))
}

/// A paragraph being read: its text, its heading level, and the paragraphs inside it (a
/// footnote's body), written after it.
#[derive(Default)]
struct Para {
    text: String,
    level: Option<u8>,
    after: Vec<(String, Option<u8>)>,
}

/// Text paragraphs and headings of `office:text`, a slide or a frame: `text:h` with its
/// `outline-level`, `text:p`, spans, `text:s` (spaces), `text:tab`, `text:line-break`; notes
/// (footnotes) inside a paragraph as lines after it; tables as in Word.
struct Flow {
    /// Paragraph styles that make a `text:p` a heading, by name, with their level.
    headings: HashMap<String, u8>,
    paras: Vec<Para>,
    tables: Vec<Table>,
    count: usize,
    table_seg: Option<usize>,
    /// Depth inside elements whose text is not shown (a note's citation, an annotation,
    /// tracked deletions).
    skip: usize,
    /// Depth inside `draw:frame`s: a text box's headings are lines of the box, never headings
    /// of the document (Fable M4 code r2 #6).
    frames: usize,
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
            frames: 0,
        }
    }

    fn event(&mut self, ev: &Ev, b: &mut Builder) {
        match ev {
            Ev::Start {
                name,
                ns,
                attrs,
                empty,
            } => {
                let l = local(name);
                if self.skip > 0
                    || matches!(
                        (ns, l),
                        (Ns::Text, "note-citation" | "tracked-changes")
                            | (Ns::Office, "annotation")
                    )
                {
                    if !empty {
                        self.skip += 1;
                    }
                    return;
                }
                match (ns, l) {
                    (Ns::Draw, "frame") if !empty => self.frames += 1,
                    (Ns::Text, "p" | "h") if !empty => {
                        let level = if self.frames > 0 {
                            None
                        } else if l == "h" {
                            // Outside 1–10, the nearest level (Fable M4 code r2 #6).
                            Some(
                                attr(attrs, "outline-level")
                                    .and_then(|v| v.trim().parse::<u64>().ok())
                                    .map_or(1, |v| v.clamp(1, 10) as u8),
                            )
                        } else {
                            attr(attrs, "style-name").and_then(|s| {
                                self.headings
                                    .get(s.as_ref())
                                    .copied()
                                    .or_else(|| built_in(&s))
                            })
                        };
                        self.paras.push(Para {
                            level,
                            ..Para::default()
                        });
                    }
                    (Ns::Text, "s" | "tab" | "line-break") => self.push(&control(l, attrs)),
                    (Ns::Table, "table") if !empty => {
                        if self.tables.is_empty() {
                            self.count += 1;
                            self.table_seg =
                                Some(b.open("table", &format!("table {}", self.count)));
                        }
                        self.tables.push(Table::default());
                    }
                    (Ns::Table, "table-cell") => {
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
            Ev::End { name, ns } => {
                if self.skip > 0 {
                    self.skip -= 1;
                    return;
                }
                match (ns, local(name)) {
                    (Ns::Draw, "frame") => self.frames = self.frames.saturating_sub(1),
                    (Ns::Text, "p" | "h") => {
                        let Some(para) = self.paras.pop() else {
                            return;
                        };
                        if let Some(outer) = self.paras.last_mut() {
                            // A footnote's paragraph comes after the one holding the note.
                            outer.after.push((para.text, para.level));
                            outer.after.extend(para.after);
                            return;
                        }
                        let lines = std::iter::once((para.text, para.level)).chain(para.after);
                        for (text, level) in lines {
                            if let Some(t) = self.tables.last_mut() {
                                t.add_text(&text);
                            } else if let Some(l) = level {
                                b.heading(l, &text);
                            } else {
                                // One line: its `text:line-break`s become spaces (spec §5.8).
                                b.line(&text);
                            }
                        }
                    }
                    (Ns::Table, "table-row") => {
                        if self.tables.len() == 1 {
                            self.tables[0].end_row(b);
                        } else if let Some(t) = self.tables.last_mut() {
                            let row = t.take_row();
                            let n = self.tables.len();
                            self.tables[n - 2].add_text(&row);
                        }
                    }
                    (Ns::Table, "table") => {
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
                    collapse(&mut p.text, t);
                }
            }
        }
    }

    fn push(&mut self, s: &str) {
        if let Some(p) = self.paras.last_mut() {
            p.text.push_str(s);
        }
    }
}

/// The text of `text:s` (its `text:c` spaces, at most 100), `text:tab` and `text:line-break`:
/// a tab and a line break are written as themselves, and the Builder makes each a space, so a
/// paragraph or a cell is one line (spec §5.8; Codex M4 code r3 #3).
fn control(local: &str, attrs: &str) -> String {
    match local {
        "s" => {
            let n = attr(attrs, "c")
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(1);
            " ".repeat(n.min(100))
        }
        "tab" => "\t".into(),
        _ => "\n".into(),
    }
}

/// Appends `t` to `out` with its white space collapsed, as ODF text is read; `text:s` writes the
/// spaces it means.
fn collapse(out: &mut String, t: &str) {
    let mut last_space = out.ends_with(' ');
    for c in t.chars() {
        if c.is_whitespace() && c != '\u{a0}' {
            if !last_space {
                out.push(' ');
            }
            last_space = true;
        } else {
            out.push(c);
            last_space = false;
        }
    }
}

pub fn odt(bytes: &[u8], b: &mut Builder) -> Result<(), DocError> {
    let (xml, headings) = content(bytes)?;
    let mut flow = Flow::new(headings);
    let mut body = false;
    for ev in Pull::new(&xml) {
        match &ev {
            Ev::Start {
                name,
                ns: Ns::Office,
                ..
            } if local(name) == "text" => body = true,
            Ev::End {
                name,
                ns: Ns::Office,
            } if local(name) == "text" => body = false,
            _ if body => flow.event(&ev, b),
            _ => {}
        }
        if b.full() {
            break;
        }
    }
    Ok(())
}

/// The `presentation:class` of the placeholders a slide's notes leave out (spec §5.8): the
/// slide image, its number, the date, the header and the footer (Codex M4 code r2 #4).
fn left_out_of_notes(attrs: &str) -> bool {
    matches!(
        attr(attrs, "class").as_deref(),
        Some("page" | "page-number" | "date-time" | "header" | "footer")
    )
}

pub fn odp(bytes: &[u8], b: &mut Builder) -> Result<(), DocError> {
    let (xml, _) = content(bytes)?;
    let mut flow = Flow::new(HashMap::new());
    let (mut n, mut slide, mut notes) = (0, None, None);
    // Depth inside a placeholder of the notes that is left out.
    let mut hidden = 0usize;
    for ev in Pull::new(&xml) {
        if hidden > 0 {
            match &ev {
                Ev::Start { empty: false, .. } => hidden += 1,
                Ev::End { .. } => hidden -= 1,
                _ => {}
            }
            continue;
        }
        match &ev {
            Ev::Start {
                name,
                ns: Ns::Draw,
                empty: false,
                ..
            } if local(name) == "page" => {
                n += 1;
                slide = Some(b.open("slide", &format!("slide {n}")));
            }
            Ev::End { name, ns: Ns::Draw } if local(name) == "page" => {
                if let Some(s) = slide.take() {
                    b.close(s);
                }
            }
            Ev::Start {
                name,
                ns: Ns::Presentation,
                empty: false,
                ..
            } if local(name) == "notes" && slide.is_some() => {
                notes = Some(b.open("notes", "notes"));
            }
            Ev::End {
                name,
                ns: Ns::Presentation,
            } if local(name) == "notes" => {
                if let Some(s) = notes.take() {
                    b.close(s);
                }
            }
            Ev::Start { attrs, empty, .. } if notes.is_some() && left_out_of_notes(attrs) => {
                if !empty {
                    hidden = 1;
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
    // Depth inside a cell's comment, `office:annotation`, which is not the cell's text, as in
    // Excel and in an OpenDocument text (Fable M4 code r3 #1).
    let mut skip = 0usize;
    for ev in Pull::new(&xml) {
        if skip > 0 {
            match ev {
                Ev::Start { empty: false, .. } => skip += 1,
                Ev::End { .. } => skip -= 1,
                _ => {}
            }
            continue;
        }
        match ev {
            Ev::Start {
                name,
                ns,
                attrs,
                empty,
            } => match (ns, local(name)) {
                (Ns::Office, "annotation") => {
                    if !empty {
                        skip = 1;
                    }
                }
                (Ns::Table, "table") if !empty => {
                    let n = attr(attrs, "name")
                        .map(|s| s.into_owned())
                        .unwrap_or_default();
                    sheet = Some(b.open("sheet", &n));
                    row = 0;
                }
                (Ns::Table, "table-row") => {
                    repeat_row = repeat(attrs, "number-rows-repeated")?;
                    col = 0;
                    cells.clear();
                    if empty {
                        row = add(row, repeat_row)?;
                    }
                }
                (Ns::Table, "table-cell" | "covered-table-cell") => {
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
                (Ns::Text, "p") if !empty => {
                    if let Some(c) = cell.as_mut()
                        && !c.2.is_empty()
                    {
                        c.2.push(' ');
                    }
                    para_depth += 1;
                }
                (Ns::Text, l @ ("s" | "tab" | "line-break")) => {
                    if let Some(c) = cell.as_mut() {
                        c.2.push_str(&control(l, attrs));
                    }
                }
                _ => {}
            },
            Ev::End { name, ns } => match (ns, local(name)) {
                (Ns::Text, "p") => para_depth = para_depth.saturating_sub(1),
                (Ns::Table, "table-cell" | "covered-table-cell") => {
                    if let Some((c, rep, text, value)) = cell.take() {
                        let shown = value.unwrap_or(text);
                        if !shown.trim().is_empty() {
                            cells.push((c, rep, shown));
                        }
                        col = add(c, rep)?;
                    }
                }
                (Ns::Table, "table-row") => {
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
                (Ns::Table, "table") => {
                    if let Some(s) = sheet.take() {
                        b.close(s);
                    }
                }
                _ => {}
            },
            Ev::Text(t) if para_depth > 0 => {
                if let Some(c) = cell.as_mut() {
                    collapse(&mut c.2, &t);
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

    fn odt_of(body: &str, styles: &str) -> Vec<u8> {
        fixture::zip(
            &[
                (
                    "mimetype",
                    b"application/vnd.oasis.opendocument.text".to_vec(),
                ),
                (
                    "content.xml",
                    format!("<office:document-content><office:body><office:text>{body}</office:text></office:body></office:document-content>")
                        .into_bytes(),
                ),
                (
                    "styles.xml",
                    format!("<office:document-styles><office:styles>{styles}</office:styles></office:document-styles>")
                        .into_bytes(),
                ),
            ],
            &["mimetype"],
        )
    }

    /// Fable r1 #5: a footnote's body is a line after the paragraph that holds it.
    #[test]
    fn a_footnote_is_a_line_after_its_paragraph() {
        let d = odt_of(
            concat!(
                r#"<text:h text:outline-level="1">Intro</text:h>"#,
                r#"<text:p>Intro <text:note text:note-class="footnote"><text:note-citation>1</text:note-citation>"#,
                r#"<text:note-body><text:p>a footnote needle</text:p></text:note-body></text:note>needle.</text:p>"#,
                r#"<text:p>next</text:p>"#,
            ),
            "",
        );
        let e = extract(Kind::Odt, &d).unwrap();
        assert_eq!(e.text, b"Intro\nIntro needle.\na footnote needle\nnext\n");
        assert_eq!(trail(Kind::Odt, &d, "a footnote"), ["Intro"]);
    }

    /// Fable r1 #6: a style whose parent is the built-in `Heading N` is a heading also when
    /// the document does not declare `Heading_20_N`, and so is a paragraph of that style.
    #[test]
    fn the_built_in_heading_styles_need_no_declaration() {
        let d = odt_of(
            concat!(
                r#"<text:p text:style-name="Heading_20_1">Declared nowhere</text:p>"#,
                r#"<text:p text:style-name="Mine">Child heading</text:p>"#,
                r#"<text:p>needle one</text:p>"#,
            ),
            r#"<style:style style:name="Mine" style:family="paragraph" style:parent-style-name="Heading_20_2"/>"#,
        );
        assert_eq!(
            trail(Kind::Odt, &d, "needle one"),
            ["Declared nowhere", "Child heading"]
        );
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

    const OFFICE: &str = "urn:oasis:names:tc:opendocument:xmlns:office:1.0";
    const TEXT: &str = "urn:oasis:names:tc:opendocument:xmlns:text:1.0";
    const TABLE: &str = "urn:oasis:names:tc:opendocument:xmlns:table:1.0";
    const STYLE: &str = "urn:oasis:names:tc:opendocument:xmlns:style:1.0";
    const DRAW: &str = "urn:oasis:names:tc:opendocument:xmlns:drawing:1.0";
    const PRES: &str = "urn:oasis:names:tc:opendocument:xmlns:presentation:1.0";

    fn odf_with(mime: &str, content: String) -> Vec<u8> {
        fixture::zip(
            &[
                ("mimetype", mime.as_bytes().to_vec()),
                ("content.xml", content.into_bytes()),
            ],
            &["mimetype"],
        )
    }

    /// Codex M4 code r2 #1: the elements are known by their namespace and local name, so a
    /// document that binds the OpenDocument namespaces to other prefixes, or to none, reads as
    /// the same document with the usual ones.
    #[test]
    fn odt_elements_are_known_by_namespace_not_by_prefix() {
        let d = odf_with(
            "application/vnd.oasis.opendocument.text",
            format!(
                r#"<o:document-content xmlns:o="{OFFICE}" xmlns:t="{TEXT}" xmlns:tb="{TABLE}" xmlns:s="{STYLE}"><o:automatic-styles><s:style s:name="P1" s:family="paragraph" s:parent-style-name="Heading_20_2"/></o:automatic-styles><o:body><o:text><t:h t:outline-level="1">Intro</t:h><t:p>first needle</t:p><t:p t:style-name="P1">Scope</t:p><tb:table tb:name="T"><tb:table-row><tb:table-cell><t:p>Item</t:p></tb:table-cell><tb:table-cell><t:p>Price</t:p></tb:table-cell></tb:table-row><tb:table-row><tb:table-cell><t:p>Pens</t:p></tb:table-cell><tb:table-cell><t:p>1.50 needle</t:p></tb:table-cell></tb:table-row></tb:table></o:text></o:body></o:document-content>"#
            ),
        );
        assert_eq!(trail(Kind::Odt, &d, "first"), ["Intro"]);
        assert_eq!(
            trail(Kind::Odt, &d, "1.50"),
            ["Intro", "Scope", "table 1", "row 2", "Price"]
        );
        // The office namespace as the default one: `text` without a prefix is `office:text`.
        let d = odf_with(
            "application/vnd.oasis.opendocument.text",
            format!(
                r#"<document-content xmlns="{OFFICE}" xmlns:text="{TEXT}"><body><text><text:h text:outline-level="1">Intro</text:h><text:p>a needle</text:p></text></body></document-content>"#
            ),
        );
        assert_eq!(trail(Kind::Odt, &d, "a needle"), ["Intro"]);
        // The usual prefix bound to another namespace is not OpenDocument's.
        let d = odf_with(
            "application/vnd.oasis.opendocument.text",
            format!(
                r#"<office:document-content xmlns:office="{OFFICE}" xmlns:text="{TEXT}"><office:body><office:text><text:p>kept needle</text:p><text:p xmlns:text="urn:example:other">foreign needle</text:p><text:p>last needle</text:p></office:text></office:body></office:document-content>"#
            ),
        );
        assert_eq!(
            extract(Kind::Odt, &d).unwrap().text,
            b"kept needle\nlast needle\n"
        );
    }

    #[test]
    fn ods_elements_are_known_by_namespace_not_by_prefix() {
        let d = odf_with(
            "application/vnd.oasis.opendocument.spreadsheet",
            format!(
                r#"<o:document-content xmlns:o="{OFFICE}" xmlns:x="{TABLE}" xmlns:y="{TEXT}"><o:body><o:spreadsheet><x:table x:name="Budget"><x:table-row><x:table-cell o:value-type="string"><y:p>Item</y:p></x:table-cell><x:covered-table-cell/><x:table-cell o:value-type="float" o:value="1.5"><y:p>1,50</y:p></x:table-cell></x:table-row><x:table-row><x:table-cell x:number-columns-repeated="2"/><x:table-cell><y:p>a<y:s/>needle</y:p></x:table-cell></x:table-row></x:table></o:spreadsheet></o:body></o:document-content>"#
            ),
        );
        let e = extract(Kind::Ods, &d).unwrap();
        assert_eq!(e.text, b"Item\t1.5\na needle\n");
        assert_eq!(trail(Kind::Ods, &d, "needle"), ["Budget", "C2"]);
        assert_eq!(trail(Kind::Ods, &d, "1.5"), ["Budget", "C1"]);
    }

    #[test]
    fn odp_elements_are_known_by_namespace_not_by_prefix() {
        let d = odf_with(
            "application/vnd.oasis.opendocument.presentation",
            format!(
                r#"<o:document-content xmlns:o="{OFFICE}" xmlns:d="{DRAW}" xmlns:pr="{PRES}" xmlns:t="{TEXT}"><o:body><o:presentation><d:page d:name="p1"><d:frame><d:text-box><t:p>a needle margin</t:p></d:text-box></d:frame><pr:notes><d:frame pr:class="notes"><d:text-box><t:p>the needle joke</t:p></d:text-box></d:frame></pr:notes></d:page><d:page d:name="p2"><d:frame><d:text-box><t:p>find the needle</t:p></d:text-box></d:frame></d:page></o:presentation></o:body></o:document-content>"#
            ),
        );
        assert_eq!(trail(Kind::Odp, &d, "margin"), ["slide 1"]);
        assert_eq!(trail(Kind::Odp, &d, "joke"), ["slide 1", "notes"]);
        assert_eq!(trail(Kind::Odp, &d, "find"), ["slide 2"]);
    }

    /// Codex M4 code r2 #4: the notes of a slide leave out the placeholders of the slide
    /// image, its number, the date, the header and the footer (spec §5.8), as PowerPoint's do.
    #[test]
    fn odp_notes_leave_out_their_placeholders() {
        let d = odf_with(
            "application/vnd.oasis.opendocument.presentation",
            format!(
                r#"<office:document-content xmlns:office="{OFFICE}" xmlns:draw="{DRAW}" xmlns:presentation="{PRES}" xmlns:text="{TEXT}"><office:body><office:presentation><draw:page draw:name="p1"><draw:frame><draw:text-box><text:p>slide text</text:p></draw:text-box></draw:frame><presentation:notes><draw:page-thumbnail presentation:class="page" draw:page-number="1"/><draw:frame presentation:class="page"><draw:text-box><text:p>image needle</text:p></draw:text-box></draw:frame><draw:frame presentation:class="header"><draw:text-box><text:p>header needle</text:p></draw:text-box></draw:frame><draw:frame presentation:class="notes"><draw:text-box><text:p>the real needle</text:p></draw:text-box></draw:frame><draw:frame presentation:class="date-time"><draw:text-box><text:p>2026-10-18 needle</text:p></draw:text-box></draw:frame><draw:frame presentation:class="footer"><draw:text-box><text:p>needle footer</text:p></draw:text-box></draw:frame><draw:frame presentation:class="page-number"><draw:text-box><text:p><text:page-number>42</text:page-number></text:p></draw:text-box></draw:frame><draw:frame><draw:text-box><text:p>plain needle</text:p></draw:text-box></draw:frame></presentation:notes></draw:page></office:presentation></office:body></office:document-content>"#
            ),
        );
        let e = extract(Kind::Odp, &d).unwrap();
        assert_eq!(e.text, b"slide text\nthe real needle\nplain needle\n");
        assert_eq!(trail(Kind::Odp, &d, "real"), ["slide 1", "notes"]);
        assert_eq!(trail(Kind::Odp, &d, "plain"), ["slide 1", "notes"]);
    }

    /// Fable M4 code r2 #6: a `text:h` level past 1–10 is taken to the nearest one, as for the
    /// styles, and a heading inside a frame (a text box anchored in a paragraph) is a line of
    /// that box, not a heading of the document.
    #[test]
    fn odt_heading_levels_stay_in_range_and_frames_hold_no_headings() {
        let d = odt_of(
            concat!(
                r#"<text:h text:outline-level="0">Zero</text:h>"#,
                r#"<text:h text:outline-level="1">One</text:h><text:p>one needle</text:p>"#,
                r#"<text:h text:outline-level="10">Ten</text:h>"#,
                r#"<text:h text:outline-level="11">Eleven</text:h><text:p>eleven needle</text:p>"#,
            ),
            "",
        );
        // Level 0 is 1, so `One` follows `Zero` instead of falling under it; 11 is 10.
        assert_eq!(trail(Kind::Odt, &d, "one needle"), ["One"]);
        assert_eq!(trail(Kind::Odt, &d, "eleven needle"), ["One", "Eleven"]);
        let d = odt_of(
            concat!(
                r#"<text:h text:outline-level="1">H1</text:h>"#,
                r#"<text:p>anchor<draw:frame><draw:text-box><text:h text:outline-level="2">Box heading</text:h><text:p>box needle</text:p></draw:text-box></draw:frame></text:p>"#,
                r#"<text:p>after needle</text:p>"#,
            ),
            "",
        );
        let e = extract(Kind::Odt, &d).unwrap();
        assert_eq!(
            e.text,
            b"H1\nanchor\nBox heading\nbox needle\nafter needle\n"
        );
        assert_eq!(trail(Kind::Odt, &d, "after needle"), ["H1"]);
        assert_eq!(trail(Kind::Odt, &d, "box needle"), ["H1"]);
    }

    #[test]
    fn spaces_tabs_and_line_breaks_of_odf_text_stay_on_its_line() {
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
        // Codex r1 #9: a line break inside a paragraph is a space too: a paragraph is a line.
        assert_eq!(e.text, b"a   b c d e\n");
    }

    /// Codex M4 code r3 #3: the same rule in every OpenDocument reader: `text:s` is its
    /// spaces, `text:tab` and `text:line-break` one space each, and white space is collapsed,
    /// in a spreadsheet's cell, a text's table cell and a slide as in a text's paragraph.
    #[test]
    fn spaces_tabs_and_line_breaks_are_the_same_in_every_odf_reader() {
        const P: &str =
            r#"<text:p>a<text:s text:c="3"/>b<text:tab/>c<text:line-break/>d   e</text:p>"#;
        let d = ods_of(&format!(
            r#"<table:table-row><table:table-cell office:value-type="string">{P}</table:table-cell><table:table-cell office:value-type="string"><text:p>one<text:line-break/>two</text:p></table:table-cell></table:table-row>"#
        ));
        let e = extract(Kind::Ods, &d).unwrap();
        assert_eq!(e.text, b"a   b c d e\tone two\n");
        assert_eq!(trail(Kind::Ods, &d, "one two"), ["S", "B1"]);
        let d = odt_of(
            &format!(
                r#"<table:table><table:table-row><table:table-cell><text:p>Item</text:p></table:table-cell></table:table-row><table:table-row><table:table-cell>{P}</table:table-cell></table:table-row></table:table>"#
            ),
            "",
        );
        assert_eq!(extract(Kind::Odt, &d).unwrap().text, b"Item\na   b c d e\n");
        let d = odf_with(
            "application/vnd.oasis.opendocument.presentation",
            format!(
                r#"<office:document-content xmlns:office="{OFFICE}" xmlns:draw="{DRAW}" xmlns:text="{TEXT}"><office:body><office:presentation><draw:page draw:name="p1"><draw:frame><draw:text-box>{P}</draw:text-box></draw:frame></draw:page></office:presentation></office:body></office:document-content>"#
            ),
        );
        assert_eq!(extract(Kind::Odp, &d).unwrap().text, b"a   b c d e\n");
    }

    /// Fable M4 code r3 #1: a cell's comment (`office:annotation`) is not the cell's text, as
    /// in Excel and in an OpenDocument text.
    #[test]
    fn a_cells_comment_is_not_its_text() {
        let d = ods_of(concat!(
            r#"<table:table-row><table:table-cell office:value-type="string"><office:annotation><dc:creator>Ana</dc:creator><dc:date>2026-10-18T00:00:00</dc:date><text:p>check needle</text:p><text:p>more</text:p></office:annotation><text:p>Pens</text:p></table:table-cell>"#,
            r#"<table:table-cell office:value-type="float" office:value="1.5"><text:p>1.5</text:p></table:table-cell></table:table-row>"#,
            r#"<table:table-row><table:table-cell><office:annotation><text:p>only a comment</text:p></office:annotation></table:table-cell>"#,
            r#"<table:table-cell office:value-type="string"><text:p>Ink needle</text:p></table:table-cell></table:table-row>"#,
        ));
        let e = extract(Kind::Ods, &d).unwrap();
        assert_eq!(e.text, b"Pens\t1.5\nInk needle\n");
        assert_eq!(trail(Kind::Ods, &d, "needle"), ["S", "B2"]);
    }
}
