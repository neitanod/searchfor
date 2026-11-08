//! Org-mode (M6 spec §6.3): `*` to any depth followed by a space is a heading of that many
//! stars, its name the text after them with its TODO keyword, priority and tags as written;
//! `#+TITLE:` is the document. `#+BEGIN_SRC LANG` … `#+END_SRC`, without case, is a `code`
//! Segment named `LANG` (`src` when absent) and `#+BEGIN_EXAMPLE` is `example`; other blocks
//! hold no Segment, and nothing in a block is a heading or a table. A table whose first row is
//! followed by a rule names its columns by that row's cells: a Match in a later row's cell gets
//! a `column` Segment, whose Section is the table.
use super::{Item, Items, Line, MAX_DEPTH, Named, Open, line_at, trim};
use std::ops::Range;

/// The text of a block marker, `#+BEGIN_` or `#+END_`, compared without case: what follows it.
fn after<'a>(text: &'a [u8], marker: &[u8]) -> Option<&'a [u8]> {
    (text.len() >= marker.len() && text[..marker.len()].eq_ignore_ascii_case(marker))
        .then(|| &text[marker.len()..])
}

/// The cells of a table row: the bytes between its pipes, the last one up to the end of the
/// row when no pipe closes it.
fn cells(src: &[u8], row: Range<usize>) -> Vec<Range<usize>> {
    let base = row.start + 1;
    let mut out = Vec::new();
    let mut from = base;
    for k in memchr::memchr_iter(b'|', &src[base..row.end]) {
        out.push(from..base + k);
        from = base + k + 1;
    }
    if from < row.end {
        out.push(from..row.end);
    }
    out
}

/// Whether a table row is a rule, `|---+---|`.
fn rule(src: &[u8], row: &Range<usize>) -> bool {
    src[row.clone()].starts_with(b"|-")
}

/// A table whose first row is followed by a rule, as the cursor's scan keeps it: its columns
/// are named when a Match falls in a cell, so preparing a table costs one record whatever its
/// cells (code review M6 r1, Fable #3). The model's scan gives the same columns as Items.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Table {
    /// The header row, without the white space around it.
    header: Range<usize>,
    /// From the first row after the rule to the end of the last row.
    pub body: Range<usize>,
    /// The table's first and last lines: every column's Section.
    lines: (u64, u64),
}

/// A row of a table's body as the cursor keeps it from one Match to the next, so the Matches of
/// one row find their cells without reading the row again (code review M6 r2, Codex #1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Row {
    /// From the row's first byte to the next row's.
    pub line: Range<usize>,
    /// Its cells, in order; none in a rule.
    cells: Vec<Range<usize>>,
}

impl Row {
    /// The cell that holds `byte`: (its index, its start). None on a pipe and around the row.
    pub fn cell_at(&self, byte: usize) -> Option<(usize, usize)> {
        let k = self.cells.partition_point(|c| c.end <= byte);
        let c = self.cells.get(k)?;
        (c.start <= byte).then_some((k, c.start))
    }
}

impl Table {
    /// The row of the body that holds `byte`, a byte of the body.
    pub fn row_at(&self, src: &[u8], byte: usize) -> Row {
        let start = memchr::memrchr(b'\n', &src[self.body.start..byte])
            .map_or(self.body.start, |k| self.body.start + k + 1);
        let Some(l) = line_at(src, start) else {
            return Row {
                line: start..src.len().max(start + 1),
                cells: Vec::new(),
            };
        };
        let row = trim(src, l.start..l.end);
        let cells = if src.get(row.start) != Some(&b'|') || rule(src, &row) {
            Vec::new()
        } else {
            cells(src, row)
        };
        Row {
            line: l.start..l.next.max(l.start + 1),
            cells,
        }
    }

    /// The cells of the header row.
    pub fn header_cells(&self, src: &[u8]) -> Vec<Range<usize>> {
        cells(src, self.header.clone())
    }

    /// The `column` Segment of the `k`th cell, `header` being this table's `header_cells`.
    pub fn column(
        &self,
        src: &[u8],
        header: &[Range<usize>],
        k: usize,
        name_chars: usize,
    ) -> crate::model::Segment {
        let name = match header.get(k).map(|h| trim(src, h.clone())) {
            Some(h) if !h.is_empty() => {
                let mut n = crate::text::CutName::new(name_chars);
                n.push_lossy(src, h);
                n.finish().0
            }
            _ => format!("column {}", k + 1),
        };
        crate::model::Segment {
            kind: "column".into(),
            name,
            line: self.lines.0,
            end_line: self.lines.1,
            level: None,
            signature: None,
        }
    }
}

/// The model's scan: every cell of a table with a header is a `column` Item.
pub(crate) fn scan(src: &[u8]) -> Items {
    scan_with(src, true).0
}

/// The cursor's scan: a table with a header is one `Table`, and its cells hold no Item.
pub(crate) fn scan_tables(src: &[u8]) -> (Items, Vec<Table>) {
    scan_with(src, false)
}

fn scan_with(src: &[u8], per_cell: bool) -> (Items, Vec<Table>) {
    let mut items = Items::default();
    let mut tables = Vec::new();
    let mut open = Open::default();
    let mut document = false;
    let mut pos = 0;
    let mut line_no: u64 = 0;
    while let Some(l) = line_at(src, pos) {
        pos = l.next;
        line_no += 1;
        let t = trim(src, l.start..l.end);
        let text = &src[t.clone()];
        let extra = usize::from(document);
        if let Some(rest) = after(text, b"#+begin_") {
            let word = rest
                .iter()
                .position(|b| matches!(b, b' ' | b'\t'))
                .unwrap_or(rest.len());
            let kind = &rest[..word];
            let mut end = src.len();
            while let Some(m) = line_at(src, pos) {
                pos = m.next;
                line_no += 1;
                let close = after(&src[trim(src, m.start..m.end)], b"#+end_");
                if close.is_some_and(|c| c.eq_ignore_ascii_case(kind)) {
                    end = m.next;
                    break;
                }
            }
            let name = if kind.eq_ignore_ascii_case(b"src") {
                let from = t.start + 8 + word;
                let lang = trim(src, from..t.end);
                let lang = lang.start
                    ..lang.start
                        + src[lang.clone()]
                            .iter()
                            .position(|b| matches!(b, b' ' | b'\t'))
                            .unwrap_or(lang.len());
                Some(if lang.is_empty() {
                    Named::Word("src")
                } else {
                    Named::Source(vec![lang])
                })
            } else if kind.eq_ignore_ascii_case(b"example") {
                Some(Named::Word("example"))
            } else {
                None
            };
            if let Some(name) = name
                && open.len() + extra < MAX_DEPTH
            {
                let mut it = Item::new("code", l.start..end, l.start..l.next, name);
                if let Named::Source(p) = &it.name {
                    it.anchor = p[0].start;
                }
                it.signature = Some(t);
                items.push(it);
            }
            continue;
        }
        if !document && let Some(rest) = after(text, b"#+title:") {
            let name = trim(src, t.end - rest.len()..t.end);
            if !name.is_empty() {
                document = true;
                let mut it = Item::new(
                    "document",
                    0..src.len(),
                    l.start..l.next,
                    Named::Source(vec![name.clone()]),
                );
                it.anchor = name.start;
                items.push(it);
            }
            continue;
        }
        if let Some((level, name)) = heading(src, l) {
            open.close_to(&mut items, level, l.start);
            let mut it = Item::new(
                "heading",
                l.start..src.len(),
                l.start..l.next,
                Named::source_or(vec![name.clone()], "heading"),
            );
            it.anchor = name.start;
            it.level = Some(level);
            it.signature = Some(trim(src, l.start..l.end));
            open.push(&mut items, it, level, extra);
            continue;
        }
        if text.first() == Some(&b'|') {
            // The table's rows, this one first.
            let mut rows = vec![t];
            let first = line_no;
            while let Some(m) = line_at(src, pos) {
                let r = trim(src, m.start..m.end);
                if src.get(r.start) != Some(&b'|') {
                    break;
                }
                rows.push(r);
                pos = m.next;
                line_no += 1;
            }
            if rows.len() < 3 || rule(src, &rows[0]) || !rule(src, &rows[1]) {
                continue;
            }
            if open.len() + extra >= MAX_DEPTH {
                continue;
            }
            if !per_cell {
                let last = rows.last().unwrap();
                tables.push(Table {
                    header: rows[0].clone(),
                    body: rows[2].start..last.end,
                    lines: (first, line_no),
                });
                continue;
            }
            let header = cells(src, rows[0].clone());
            for row in &rows[2..] {
                if rule(src, row) {
                    continue;
                }
                for (k, cell) in cells(src, row.clone()).into_iter().enumerate() {
                    let name = match header.get(k).map(|h| trim(src, h.clone())) {
                        Some(h) if !h.is_empty() => Named::Source(vec![h]),
                        _ => Named::Ordinal("column", k + 1),
                    };
                    let mut it = Item::new("column", cell.clone(), cell.start..cell.start, name);
                    it.search = false;
                    it.lines = Some((first, line_no));
                    items.push(it);
                }
            }
        }
    }
    open.finish(&mut items, src.len());
    (items, tables)
}

/// A heading line: (stars, name range).
fn heading(src: &[u8], l: Line) -> Option<(usize, Range<usize>)> {
    let n = src[l.start..l.end]
        .iter()
        .take_while(|&&b| b == b'*')
        .count();
    if n == 0 || !matches!(src.get(l.start + n), Some(b' ' | b'\t')) || l.start + n >= l.end {
        return None;
    }
    Some((n, trim(src, l.start + n..l.end)))
}

#[cfg(test)]
mod tests {
    use super::super::tests::{cursor_equals_model, params, trail};

    #[test]
    fn headings_blocks_and_tables() {
        let p = params("org");
        let src = b"* A [#B] x :t:\n** B\n#+begin_quote\n* q-needle\n#+end_QUOTE\nb-needle\n| H1 |  |\n|----|\n|----|\n| a-needle | c-needle | d-needle\n* C\n| x | y |\n| e-needle |\n";
        assert_eq!(trail(src, &p, "q-needle"), ["A [#B] x :t:", "B"]);
        assert_eq!(trail(src, &p, "b-needle"), ["A [#B] x :t:", "B"]);
        assert_eq!(trail(src, &p, "a-needle"), ["A [#B] x :t:", "B", "H1"]);
        assert_eq!(
            trail(src, &p, "c-needle"),
            ["A [#B] x :t:", "B", "column 2"]
        );
        assert_eq!(
            trail(src, &p, "d-needle"),
            ["A [#B] x :t:", "B", "column 3"]
        );
        assert_eq!(trail(src, &p, "e-needle"), ["C"]);
        cursor_equals_model(src, &p);
    }

    /// Code review M6 r1, Fable #3: a table's cells hold no record in the prepared index, so
    /// preparing a large table costs what its headings cost, whatever the cells; the cursor
    /// finds a Match's column in its row.
    #[test]
    fn a_table_costs_the_index_no_record_per_cell() {
        let p = params("org");
        let mut src = b"* T\n| a | b | c |\n|---+---+---|\n".to_vec();
        for _ in 0..1000 {
            src.extend(b"| x | y | z |\n");
        }
        src.extend(b"|---+---+---|\n| q | needle |\n* U\n| needle |\n");
        let (ix, _) = super::super::index_of(&src, &p).unwrap();
        assert_eq!(ix.len(), 2, "the two headings alone");
        assert_eq!(trail(&src, &p, "needle"), ["T", "b"]);
        cursor_equals_model(&src, &p);
    }

    /// Code review M6 r2, Codex #1: the Matches of one row read the row and the header once,
    /// so a row of 16 000 cells with a Match in each, and 64 000 Matches in one cell, take
    /// what a few rows take. Read again per Match, the first took 2,7 s in release.
    #[test]
    fn the_matches_of_one_row_read_it_once() {
        use crate::processors::Processor;
        let p = params("org");
        let n = 16_000;
        let mut src = format!("* T\n|{}\n|---|\n|", " H |".repeat(n)).into_bytes();
        let body = src.len();
        src.extend(" needle |".repeat(n).as_bytes());
        src.extend(b"\n| ");
        let cell = src.len();
        src.extend("needle ".repeat(64_000).as_bytes());
        src.extend(b"|\n");
        let mut at: Vec<usize> = (0..n).map(|k| body + 1 + 9 * k).collect();
        at.extend((0..64_000).map(|k| cell + 7 * k));
        let prepared = super::super::Headings
            .prepare(super::super::tests::input(&src, "x"), &p)
            .unwrap();
        let mut c = prepared.cursor();
        let t0 = std::time::Instant::now();
        let mut last = Vec::new();
        for &a in &at {
            last = c
                .trail_at(a)
                .unwrap()
                .iter()
                .map(|s| s.name.clone())
                .collect();
        }
        let took = t0.elapsed();
        assert_eq!(last, ["T", "H"]);
        assert!(
            took < std::time::Duration::from_millis(500),
            "{} Matches took {took:?}",
            at.len()
        );
    }

    /// Code review M6 r2, Codex #3: `--in-headings` does not search an Org column's header,
    /// as it does not search Markdown's (M6 spec §6.3, «as Markdown's»): the prepared names
    /// are the model's, the header cells left out, as since the columns were added.
    #[test]
    fn the_names_leave_a_column_header_out_as_markdown_does() {
        use crate::processors::Processor;
        let p = params("org");
        let src = b"* T\n| needle-header | B |\n|---+---|\n| x | y |\n";
        let input = super::super::tests::input(src, "x");
        let model: Vec<String> = super::super::Headings
            .names(&input, &p)
            .unwrap()
            .into_iter()
            .map(|n| n.text)
            .collect();
        let prepared = super::super::Headings.prepare(input, &p).unwrap();
        let names: Vec<String> = prepared
            .names()
            .unwrap()
            .into_iter()
            .map(|n| n.text)
            .collect();
        assert_eq!(model, ["T"]);
        assert_eq!(names, model);
    }

    /// Code review M6 r1, Fable #56: a heading of stars and spaces alone is named `heading`,
    /// a word s4 adds, so its Trail shows what it is.
    #[test]
    fn an_empty_heading_is_named_heading() {
        let p = params("org");
        let src = b"* A\n*   \nneedle\n";
        assert_eq!(trail(src, &p, "needle"), ["heading"]);
        cursor_equals_model(src, &p);
    }

    #[test]
    fn deep_headings_stop_opening_past_the_depth() {
        let p = params("org");
        let mut src = String::new();
        for n in 1..=300 {
            src.push_str(&format!("{} h{n}\n", "*".repeat(n)));
        }
        src.push_str("needle\n");
        let t = trail(src.as_bytes(), &p, "needle");
        assert_eq!(t.len(), super::MAX_DEPTH);
        assert_eq!(t.last().unwrap(), "h256");
    }
}
