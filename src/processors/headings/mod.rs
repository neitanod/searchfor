//! headings: the titles, headings, code blocks and other blocks of AsciiDoc, reStructuredText,
//! Org-mode and LaTeX (M6 spec §6.1–§6.4), with Markdown's Segment kinds and one scanner per
//! `syntax`. A scanner gives a compact index of Source ranges, with no text (`compact`), whose
//! cursor builds a Segment only when a Trail holds it and streams the Trails (M5b spec §6.1); the
//! model path builds the Outline from the same index. A file has no syntax errors here: a block
//! left open runs to the end of the file.
pub mod asciidoc;
pub(crate) mod compact;
pub mod latex;
pub mod org;
pub mod rst;

use super::{Capability, Cursor, Prepared, Processor};
use crate::model::{Input, Name, SharedTrail, Trail};
use crate::trail::{Outline, TrailCursor, names_of};
use compact::{Index, IndexCursor, Spec, SpecName};
use std::ops::Range;

pub struct Headings;

/// Characters of a name before it is cut with `…`, when the Definition sets no `name_chars`.
const NAME_CHARS: usize = 40;
/// Past this many open Segments no more Segments open (M6 spec §3, point 8).
pub(crate) const MAX_DEPTH: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Syntax {
    Asciidoc,
    Rst,
    Org,
    Latex,
}

fn syntax(params: &toml::Table) -> anyhow::Result<Syntax> {
    match params.get("syntax").and_then(|v| v.as_str()) {
        Some("asciidoc") => Ok(Syntax::Asciidoc),
        Some("rst") => Ok(Syntax::Rst),
        Some("org") => Ok(Syntax::Org),
        Some("latex") => Ok(Syntax::Latex),
        _ => anyhow::bail!(
            "headings needs `syntax` = \"asciidoc\", \"rst\", \"org\" or \"latex\" in [params]"
        ),
    }
}

fn name_chars(params: &toml::Table) -> usize {
    params
        .get("name_chars")
        .and_then(|v| v.as_integer())
        .map_or(NAME_CHARS, |n| n.max(0) as usize)
}

/// What names an indexed Segment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Named {
    /// Source pieces, joined by one space each (a title over several lines): `--in-headings`
    /// searches them.
    Source(Vec<Range<usize>>),
    /// A word the scanner adds, `literal`: it has no Source piece, so no search finds it.
    Word(&'static str),
    /// A word and a number the scanner adds, `column 3`: no search finds it either.
    Ordinal(&'static str, usize),
}

impl Named {
    /// Source pieces, or `word` when they hold no byte: an empty name shows the Segment's kind
    /// (code review M6 r1, Fable #56).
    pub fn source_or(pieces: Vec<Range<usize>>, word: &'static str) -> Self {
        if pieces.iter().all(|r| r.is_empty()) {
            Named::Word(word)
        } else {
            Named::Source(pieces)
        }
    }
}

/// One Segment as a scanner finds it: Source ranges only, kept as `Items` keeps it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Item {
    pub kind: &'static str,
    /// The Section.
    pub range: Range<usize>,
    /// The bytes that name it: a Match there leaves it out of its own Trail.
    pub label: Range<usize>,
    pub name: Named,
    /// Where an empty hit in an empty name lands: the end of the heading's markers.
    pub anchor: usize,
    /// The opening line the tree shows, for headings and code.
    pub signature: Option<Range<usize>>,
    /// A heading's semantic depth.
    pub level: Option<usize>,
    /// Whether `--in-headings` searches its name: a column's is its header's, as Markdown's.
    pub search: bool,
    /// The Section's lines when they are not the range's: a column's are its table's.
    pub lines: Option<(u64, u64)>,
    /// A hidden environment: it holds Segments without showing, and has no searchable name.
    pub hidden: bool,
}

impl Item {
    pub fn new(kind: &'static str, range: Range<usize>, label: Range<usize>, name: Named) -> Self {
        let anchor = range.start;
        Self {
            kind,
            range,
            label,
            name,
            anchor,
            signature: None,
            level: None,
            search: true,
            lines: None,
            hidden: false,
        }
    }
}

/// The Segments a scanner found, in any order, in the compact index (`compact::Index`): a
/// Segment's position stays the one `push` gave until the scanner is done.
#[derive(Debug, Default)]
pub(crate) struct Items(Index);

impl Items {
    pub fn push(&mut self, item: Item) {
        let name = match &item.name {
            Named::Source(p) => SpecName::Source(p),
            Named::Word(w) => SpecName::Word(w),
            Named::Ordinal(w, n) => SpecName::Ordinal(w, *n),
        };
        self.0.push(Spec {
            kind: item.kind,
            range: item.range.clone(),
            label: item.label.clone(),
            name,
            anchor: item.anchor,
            signature: item.signature.clone(),
            level: item.level,
            search: item.search,
            lines: item.lines,
            hidden: item.hidden,
        });
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Closes Segment `i` at `end`.
    pub fn set_end(&mut self, i: usize, end: usize) {
        self.0.set_end(i, end);
    }

    /// The index, sorted and each Segment within its parent.
    pub fn finish(self) -> Index {
        self.0.finish()
    }
}

/// One physical line: `start..end` without its `\n` or `\r\n`, and `next`, where the following
/// one starts. A lone `\r` breaks no line (M6 spec §3, point 7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Line {
    pub start: usize,
    pub end: usize,
    pub next: usize,
}

/// The line that starts at `pos`, or None at the end of `src`.
pub(crate) fn line_at(src: &[u8], pos: usize) -> Option<Line> {
    if pos >= src.len() {
        return None;
    }
    let (end, next) = match memchr::memchr(b'\n', &src[pos..]) {
        Some(k) => (pos + k, pos + k + 1),
        None => (src.len(), src.len()),
    };
    let end = if end > pos && src[end - 1] == b'\r' && next > end {
        end - 1
    } else {
        end
    };
    Some(Line {
        start: pos,
        end,
        next,
    })
}

/// Every line of `src`, in order.
pub(crate) fn lines(src: &[u8]) -> impl Iterator<Item = Line> + '_ {
    let mut pos = 0;
    std::iter::from_fn(move || {
        let l = line_at(src, pos)?;
        pos = l.next;
        Some(l)
    })
}

/// `src[r]` without the spaces and tabs around it, as a range of `src`.
pub(crate) fn trim(src: &[u8], r: Range<usize>) -> Range<usize> {
    let mut a = r.start;
    let mut b = r.end;
    while a < b && matches!(src[a], b' ' | b'\t') {
        a += 1;
    }
    while b > a && matches!(src[b - 1], b' ' | b'\t') {
        b -= 1;
    }
    a..b
}

/// Whether the line holds only spaces and tabs.
pub(crate) fn blank(src: &[u8], l: Line) -> bool {
    src[l.start..l.end]
        .iter()
        .all(|b| matches!(b, b' ' | b'\t'))
}

/// The columns of the line's leading white space, a tab reaching the next multiple of 8.
pub(crate) fn indent(src: &[u8], l: Line) -> usize {
    let mut col = 0;
    for &b in &src[l.start..l.end] {
        match b {
            b' ' => col += 1,
            b'\t' => col = (col / 8 + 1) * 8,
            _ => break,
        }
    }
    col
}

/// The open headings of a scanner, outermost first: (item, level). A heading's Section runs
/// to the next heading of the same or a higher level (M6 spec §6), or the end of the file.
#[derive(Default)]
pub(crate) struct Open {
    stack: Vec<(usize, usize)>,
}

impl Open {
    /// Closes, at `at`, the headings of level `level` or deeper.
    pub fn close_to(&mut self, items: &mut Items, level: usize, at: usize) {
        while let Some(&(i, l)) = self.stack.last() {
            if l < level {
                break;
            }
            items.set_end(i, at);
            self.stack.pop();
        }
    }

    /// Opens heading `item` of `level`, unless `MAX_DEPTH` Segments are open with `extra`
    /// Segments outside this stack: then the item is dropped, and Matches below it keep the
    /// Trail of the deepest open one.
    pub fn push(&mut self, items: &mut Items, item: Item, level: usize, extra: usize) {
        if self.stack.len() + extra >= MAX_DEPTH {
            return;
        }
        items.push(item);
        self.stack.push((items.len() - 1, level));
    }

    pub fn len(&self) -> usize {
        self.stack.len()
    }

    /// Closes every heading at `at`, the end of the file.
    pub fn finish(&mut self, items: &mut Items, at: usize) {
        self.close_to(items, 0, at);
    }
}

/// LaTeX's `hidden_environments`, or its defaults.
fn hidden_environments(params: &toml::Table) -> Vec<String> {
    match params.get("hidden_environments").and_then(|v| v.as_array()) {
        Some(a) => a
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect(),
        None => latex::HIDDEN.iter().map(|s| s.to_string()).collect(),
    }
}

fn scan(src: &[u8], syntax: Syntax, params: &toml::Table) -> Index {
    let items = match syntax {
        Syntax::Asciidoc => asciidoc::scan(src),
        Syntax::Rst => rst::scan(src),
        Syntax::Org => org::scan(src),
        Syntax::Latex => {
            let hidden = hidden_environments(params);
            let hidden: Vec<&[u8]> = hidden.iter().map(|s| s.as_bytes()).collect();
            latex::scan(src, &hidden)
        }
    };
    items.finish()
}

/// The cursor's index, and Org's tables with a header, whose cells it leaves out: the cursor
/// names their columns (code review M6 r1, Fable #3).
fn index_of(src: &[u8], params: &toml::Table) -> anyhow::Result<(Index, Vec<org::Table>)> {
    let syntax = syntax(params)?;
    if syntax == Syntax::Org {
        let (items, tables) = org::scan_tables(src);
        return Ok((items.finish(), tables));
    }
    Ok((scan(src, syntax, params), Vec::new()))
}

/// The model's Outline: every Segment as an Item, Org's columns included.
fn outline_of(src: &[u8], params: &toml::Table) -> anyhow::Result<Outline> {
    let syntax = syntax(params)?;
    Ok(scan(src, syntax, params).outline(src, name_chars(params)))
}

impl Processor for Headings {
    fn trails(
        &self,
        input: &Input,
        params: &toml::Table,
        anchors: &[usize],
    ) -> anyhow::Result<Vec<Trail>> {
        let o = outline_of(input.bytes, params)?;
        let mut c = TrailCursor::new(&o);
        // An anchor before the last one starts the cursor's walk over: any order is answered.
        Ok(anchors.iter().map(|&a| c.trail(a)).collect())
    }

    fn names(&self, input: &Input, params: &toml::Table) -> anyhow::Result<Vec<Name>> {
        Ok(names_of(&outline_of(input.bytes, params)?))
    }

    fn outline_of(&self, input: &Input, params: &toml::Table) -> Option<anyhow::Result<Outline>> {
        Some(outline_of(input.bytes, params))
    }

    fn prepare<'a>(
        &'a self,
        input: Input<'a>,
        params: &'a toml::Table,
    ) -> anyhow::Result<Box<dyn Prepared + 'a>> {
        let (index, tables) = index_of(input.bytes, params)?;
        Ok(Box::new(HeadingPrepared {
            src: input.bytes,
            index,
            tables,
            name_chars: name_chars(params),
        }))
    }

    fn capability(&self, params: &toml::Table) -> Capability {
        match syntax(params) {
            Ok(_) => Capability::Native,
            Err(_) => Capability::Model,
        }
    }
}

/// A text document prepared once: its compact index, which the cursors borrow; the Segments of
/// a Trail are built when it needs them.
pub struct HeadingPrepared<'a> {
    src: &'a [u8],
    index: Index,
    /// Org's tables with a header, in file order; their cells are not in `index`.
    tables: Vec<org::Table>,
    name_chars: usize,
}

impl Prepared for HeadingPrepared<'_> {
    fn cursor(&self) -> Box<dyn Cursor + '_> {
        let inner = IndexCursor::new(self.src, &self.index, self.name_chars);
        if self.tables.is_empty() {
            return Box::new(inner);
        }
        Box::new(TableCursor {
            inner,
            src: self.src,
            tables: &self.tables,
            name_chars: self.name_chars,
            row: None,
            header: None,
            last: None,
            cell: None,
        })
    }

    fn names(&self) -> anyhow::Result<Vec<Name>> {
        Ok(names_of(&self.index.outline(self.src, self.name_chars)))
    }

    fn streams(&self) -> bool {
        true
    }
}

/// The index's cursor, and the `column` of an Org table's cell at the end of a Trail whose
/// offset falls in one: the column the model's Items give, built when a Match asks for it.
/// The row of the last offset and its table's header cells are kept, so the Matches of one
/// row cost one read of it (code review M6 r2, Codex #1).
struct TableCursor<'p> {
    inner: IndexCursor<'p>,
    src: &'p [u8],
    tables: &'p [org::Table],
    name_chars: usize,
    /// The row that holds the last offset in a table's body: (table, row).
    row: Option<(usize, org::Row)>,
    /// The header cells of the last table a column was named in: (table, cells).
    header: Option<(usize, Vec<Range<usize>>)>,
    /// The last Trail given with a column: the index's Trail it extends, its table and
    /// column, and the Trail.
    last: Option<(SharedTrail, usize, usize, SharedTrail)>,
    /// The start of the cell that holds the last offset, if one does.
    cell: Option<usize>,
}

impl Cursor for TableCursor<'_> {
    fn trail_at(&mut self, byte: usize) -> anyhow::Result<SharedTrail> {
        let t = self.inner.trail_at(byte)?;
        self.cell = None;
        let Some(ti) = self
            .tables
            .partition_point(|tb| tb.body.start <= byte)
            .checked_sub(1)
        else {
            return Ok(t);
        };
        let table = &self.tables[ti];
        if byte >= table.body.end {
            return Ok(t);
        }
        if !matches!(&self.row, Some((i, r)) if *i == ti && r.line.contains(&byte)) {
            self.row = Some((ti, table.row_at(self.src, byte)));
        }
        let Some((k, cell)) = self.row.as_ref().and_then(|(_, r)| r.cell_at(byte)) else {
            return Ok(t);
        };
        self.cell = Some(cell);
        if let Some((inner, lt, lk, out)) = &self.last
            && std::rc::Rc::ptr_eq(inner, &t)
            && (*lt, *lk) == (ti, k)
        {
            return Ok(out.clone());
        }
        if !matches!(&self.header, Some((i, _)) if *i == ti) {
            self.header = Some((ti, table.header_cells(self.src)));
        }
        let header = self.header.as_ref().map_or(&[][..], |(_, h)| h);
        let column = table.column(self.src, header, k, self.name_chars);
        if let Some((inner, _, _, out)) = &self.last
            && std::rc::Rc::ptr_eq(inner, &t)
            && out.last() == Some(&column)
        {
            let out = out.clone();
            self.last = Some((t, ti, k, out.clone()));
            return Ok(out);
        }
        let mut trail = Vec::with_capacity(t.len() + 1);
        trail.extend(t.iter().cloned());
        trail.push(column);
        let out: SharedTrail = std::rc::Rc::from(trail);
        self.last = Some((t, ti, k, out.clone()));
        Ok(out)
    }

    /// The index's answer, or the cell's start when the column is the outermost Segment that
    /// holds the offset.
    fn open_from(&mut self) -> Option<usize> {
        let o = self.inner.open_from()?;
        Some(self.cell.map_or(o, |c| o.min(c)))
    }

    fn open_from_with(
        &mut self,
        first: &mut dyn FnMut(Range<usize>) -> Option<usize>,
    ) -> Option<usize> {
        let o = self.inner.open_from_with(first)?;
        Some(self.cell.map_or(o, |c| o.min(c)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::rc::Rc;

    pub(crate) fn input<'a>(bytes: &'a [u8], format: &'a str) -> Input<'a> {
        Input {
            bytes,
            path: Path::new("x"),
            format,
            disk: None,
        }
    }

    pub(crate) fn params(syntax: &str) -> toml::Table {
        let mut p = toml::Table::new();
        p.insert("syntax".into(), toml::Value::String(syntax.into()));
        p
    }

    /// The native cursor gives what the model gives, at every byte, and shares equal Trails.
    pub(crate) fn cursor_equals_model(src: &[u8], p: &toml::Table) {
        let anchors: Vec<usize> = (0..src.len()).collect();
        let model = Headings.trails(&input(src, "x"), p, &anchors).unwrap();
        let prepared = Headings.prepare(input(src, "x"), p).unwrap();
        assert!(prepared.streams());
        let mut c = prepared.cursor();
        let mut last: Option<SharedTrail> = None;
        for (&a, m) in anchors.iter().zip(&model) {
            let t = c.trail_at(a).unwrap();
            assert_eq!(t.as_ref(), m.as_slice(), "byte {a}");
            if let Some(l) = &last
                && l.as_ref() == t.as_ref()
            {
                assert!(Rc::ptr_eq(l, &t), "byte {a}");
            }
            assert!(c.open_from().is_some_and(|o| o <= a));
            last = Some(t);
        }
    }

    /// The Trail names at the first byte of `needle` in `src`.
    pub(crate) fn trail(src: &[u8], p: &toml::Table, needle: &str) -> Vec<String> {
        let at = src
            .windows(needle.len())
            .position(|w| w == needle.as_bytes())
            .unwrap();
        Headings.trails(&input(src, "x"), p, &[at]).unwrap()[0]
            .iter()
            .map(|s| s.name.clone())
            .collect()
    }

    #[test]
    fn the_syntax_is_required_and_decides_the_capability() {
        assert_eq!(Headings.capability(&params("asciidoc")), Capability::Native);
        assert_eq!(Headings.capability(&params("cobol")), Capability::Model);
        let e = Headings
            .names(&input(b"= T\n", "x"), &toml::Table::new())
            .unwrap_err();
        assert!(e.to_string().contains("syntax"), "{e}");
    }

    #[test]
    fn lines_end_at_lf_or_crlf_and_a_lone_cr_breaks_none() {
        let src = b"a\r\nb\rc\nd";
        let l: Vec<_> = lines(src).map(|l| (l.start, l.end, l.next)).collect();
        assert_eq!(l, [(0, 1, 3), (3, 6, 7), (7, 8, 8)]);
        assert_eq!(indent(b"\t x", line_at(b"\t x", 0).unwrap()), 9);
    }

    #[test]
    fn a_title_over_lines_is_joined_by_one_space_with_a_piece_per_line() {
        let src = b"ab\n  cd";
        let mut items = Items::default();
        items.push(Item::new(
            "heading",
            0..src.len(),
            0..src.len(),
            Named::Source(vec![0..2, 5..7]),
        ));
        let o = items.finish().outline(src, 40);
        let s = &o.segments[0];
        assert_eq!(s.segment.name, "ab cd");
        assert_eq!(s.name.as_ref().unwrap()[0], (0..2, 0..2));
        assert_eq!(s.name.as_ref().unwrap().last().unwrap(), &(3..5, 5..7));
    }
}
