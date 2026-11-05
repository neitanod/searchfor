//! headings: the titles, headings, code blocks and other blocks of AsciiDoc, reStructuredText,
//! Org-mode and LaTeX (M6 spec §6.1–§6.4), with Markdown's Segment kinds and one scanner per
//! `syntax`. A scanner gives a compact index of Source ranges, with no text; the Outline is built
//! from it once per file, and its cursor streams the Trails (M5b spec §6.1). A file has no
//! syntax errors here: a block left open runs to the end of the file.
pub mod asciidoc;
pub mod org;
pub mod rst;

use super::{Capability, Cursor, Prepared, Processor};
use crate::model::{Input, Name, SharedTrail, Trail};
use crate::trail::{Outline, OutlineSegment, TrailCursor, names_of};
use std::ops::Range;
use std::rc::Rc;

pub struct Headings;

/// Characters of a name before it is cut with `…`, when the Definition sets no `name_chars`.
const NAME_CHARS: usize = 40;
/// Characters of a signature: an opening line, as treesitter's.
const SIGNATURE_CHARS: usize = 200;
/// Past this many open Segments no more Segments open (M6 spec §3, point 8).
pub(crate) const MAX_DEPTH: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Syntax {
    Asciidoc,
    Rst,
    Org,
}

fn syntax(params: &toml::Table) -> anyhow::Result<Syntax> {
    match params.get("syntax").and_then(|v| v.as_str()) {
        Some("asciidoc") => Ok(Syntax::Asciidoc),
        Some("rst") => Ok(Syntax::Rst),
        Some("org") => Ok(Syntax::Org),
        _ => {
            anyhow::bail!("headings needs `syntax` = \"asciidoc\", \"rst\" or \"org\" in [params]")
        }
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

/// One Segment of the index: Source ranges only.
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
        }
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
    pub fn close_to(&mut self, items: &mut [Item], level: usize, at: usize) {
        while let Some(&(i, l)) = self.stack.last() {
            if l < level {
                break;
            }
            items[i].range.end = at;
            self.stack.pop();
        }
    }

    /// Opens heading `item` of `level`, unless `MAX_DEPTH` Segments are open with `extra`
    /// Segments outside this stack: then the item is dropped, and Matches below it keep the
    /// Trail of the deepest open one.
    pub fn push(&mut self, items: &mut Vec<Item>, item: Item, level: usize, extra: usize) {
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
    pub fn finish(&mut self, items: &mut [Item], at: usize) {
        self.close_to(items, 0, at);
    }
}

fn scan(src: &[u8], syntax: Syntax) -> Vec<Item> {
    match syntax {
        Syntax::Asciidoc => asciidoc::scan(src),
        Syntax::Rst => rst::scan(src),
        Syntax::Org => org::scan(src),
    }
}

/// The text of a signature: the opening line, invalid UTF-8 as U+FFFD, cut at
/// `SIGNATURE_CHARS` characters. Only that many bytes and a few more are decoded.
fn signature(src: &[u8], r: Range<usize>) -> String {
    let r = r.start..r.end.min(r.start.saturating_add(4 * SIGNATURE_CHARS + 4));
    let text = String::from_utf8_lossy(&src[r]);
    text.trim_end().chars().take(SIGNATURE_CHARS).collect()
}

/// The Outline of an index: the names and signatures decoded, cut at `name_chars`.
fn outline(src: &[u8], index: Vec<Item>, name_chars: usize) -> Outline {
    let segments = index
        .into_iter()
        .map(|item| {
            let (text, pieces) = match &item.name {
                Named::Source(parts) => {
                    let mut n = crate::text::CutName::new(name_chars);
                    let mut last: Option<usize> = None;
                    for r in parts {
                        if let Some(end) = last {
                            // The line break between two lines of a title, as one space.
                            n.push_decoded(' ', end..r.start);
                        }
                        n.push_lossy(src, r.clone());
                        last = Some(r.end);
                    }
                    let (t, p) = n.finish();
                    (t, Some(p))
                }
                Named::Word(word) => ((*word).to_string(), None),
                Named::Ordinal(word, n) => (format!("{word} {n}"), None),
            };
            let mut s = OutlineSegment::new(item.kind, text, item.range, item.label);
            if let Some(pieces) = pieces.filter(|_| item.search) {
                s.name = Some(pieces);
                s.name_anchor = item.anchor;
            }
            if let Some((first, last)) = item.lines {
                s.segment.line = first;
                s.segment.end_line = last;
            }
            s.segment.level = item.level;
            s.segment.signature = item.signature.map(|r| signature(src, r));
            s
        })
        .collect();
    Outline::build(segments, src)
}

fn outline_of(src: &[u8], params: &toml::Table) -> anyhow::Result<Outline> {
    let syntax = syntax(params)?;
    Ok(outline(src, scan(src, syntax), name_chars(params)))
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
        Ok(Box::new(HeadingPrepared {
            outline: outline_of(input.bytes, params)?,
        }))
    }

    fn capability(&self, params: &toml::Table) -> Capability {
        match syntax(params) {
            Ok(_) => Capability::Native,
            Err(_) => Capability::Model,
        }
    }
}

/// A text document prepared once: the Outline of its index.
pub struct HeadingPrepared {
    outline: Outline,
}

impl Prepared for HeadingPrepared {
    fn cursor(&self) -> Box<dyn Cursor + '_> {
        Box::new(HeadingCursor {
            trails: TrailCursor::new(&self.outline),
            indices: Vec::new(),
            last: None,
        })
    }

    fn names(&self) -> anyhow::Result<Vec<Name>> {
        Ok(names_of(&self.outline))
    }

    fn streams(&self) -> bool {
        true
    }
}

/// The Trails of a prepared document in file order: a Trail is cloned out of the Outline only
/// when the indices of its Segments change.
pub struct HeadingCursor<'o> {
    trails: TrailCursor<'o>,
    indices: Vec<usize>,
    last: Option<(Vec<usize>, SharedTrail)>,
}

impl Cursor for HeadingCursor<'_> {
    fn trail_at(&mut self, byte: usize) -> anyhow::Result<SharedTrail> {
        self.trails.indices(byte, &mut self.indices);
        match &self.last {
            Some((ix, t)) if *ix == self.indices => Ok(t.clone()),
            _ => {
                let t: SharedTrail = Rc::from(self.trails.segments(&self.indices));
                self.last = Some((self.indices.clone(), t.clone()));
                Ok(t)
            }
        }
    }

    fn open_from(&mut self) -> Option<usize> {
        Some(self.trails.open_from())
    }

    fn open_from_with(
        &mut self,
        first: &mut dyn FnMut(Range<usize>) -> Option<usize>,
    ) -> Option<usize> {
        Some(self.trails.open_from_with(first))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn input<'a>(bytes: &'a [u8], format: &'a str) -> Input<'a> {
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
        let o = outline(
            src,
            vec![Item::new(
                "heading",
                0..src.len(),
                0..src.len(),
                Named::Source(vec![0..2, 5..7]),
            )],
            40,
        );
        let s = &o.segments[0];
        assert_eq!(s.segment.name, "ab cd");
        assert_eq!(s.name.as_ref().unwrap()[0], (0..2, 0..2));
        assert_eq!(s.name.as_ref().unwrap().last().unwrap(), &(3..5, 5..7));
    }
}
