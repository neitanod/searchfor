//! The compact preparation of keypath's line syntaxes (M6 plan, Task 2.1): INI, `.properties`,
//! `.env`, SSH, iCalendar/vCard and BibTeX scan the file once into `Entry`s, a Segment's
//! byte ranges and the Source ranges its name is made of, with no owned String per Segment.
//! The cursor builds a Segment, name included, only when a Trail holds it, and the Outline of
//! the model path (`outline`) is built from the same scan, with its own selection.
use crate::model::{Input, Name, Segment, SharedTrail};
use crate::processors::{Cursor, Prepared};
use crate::text::{CutName, Pieces};
use crate::trail::{Outline, OutlineSegment, names_of};
use std::ops::Range;

/// Past this many open Segments no more open (M6 spec §3 rule 8).
pub(crate) const MAX_DEPTH: usize = 256;

/// The `name_chars` of a Definition: 40 when absent (M6 spec §3 rule 4); 0 cuts nothing.
pub(crate) fn name_chars(params: &::toml::Table) -> usize {
    params
        .get("name_chars")
        .and_then(|v| v.as_integer())
        .map_or(40, |n| n.max(0) as usize)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Key,
    /// An iCalendar or vCard component (M6 spec §10.2).
    Component,
    /// A BibTeX entry (M6 spec §10.3).
    Entry,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Kind::Key => "key",
            Kind::Component => "component",
            Kind::Entry => "entry",
        }
    }
}

/// Where a Segment's name comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NameSrc {
    /// The Source bytes as written.
    Verbatim(Range<usize>),
    /// A `.properties` key, decoded (`properties::push_key`).
    Properties(Range<usize>),
    /// An iCalendar or vCard component (`ical::push_component`): the `X` of its `BEGIN:X`,
    /// then the value of its SUMMARY, FN or TZID, unfolded, or else its ordinal among its
    /// siblings of that `X`.
    Component {
        x: Range<usize>,
        value: Option<Range<usize>>,
        ordinal: u32,
    },
    /// A BibTeX entry: its name as written, and its signature (`@article{knuth1984,`).
    Entry {
        name: Range<usize>,
        signature: Range<usize>,
    },
}

/// What `Builder::entry_mut` gives a scanner of a Segment it opened: its name, and whether it
/// is hidden.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Entry {
    pub kind: Kind,
    pub hidden: bool,
    pub name: NameSrc,
    label: Range<usize>,
}

/// One Segment of the table: 32 bytes, with no allocation of its own (criterion 50: C11
/// measured 0.76 GB for a 50 MiB INI file of one key per 9 bytes, 130 bytes a Segment).
#[derive(Debug, Clone)]
struct Packed {
    start: usize,
    end: usize,
    /// The label, as (offset from `start`, length).
    label: u32,
    label_len: u32,
    /// A key's name as (offset from `start`, length, its `TAG_*` in the top two bits of the
    /// length); with `TAG_RICH`, the index of its `Entry` in `Table::rich`.
    name: u32,
    name_len: u32,
}

const TAG_SHIFT: u32 = 30;
const TAG_VERBATIM: u32 = 0;
const TAG_PROPERTIES: u32 = 1;
const TAG_RICH: u32 = 2;
const LEN_MASK: u32 = (1 << TAG_SHIFT) - 1;

/// The Segments of a scan, in file order: the 32-byte table, and the Segments it cannot hold
/// (another kind, a component's or an entry's name, a hidden one, offsets past u32).
#[derive(Debug, Default)]
struct Table {
    packed: Vec<Packed>,
    rich: Vec<Entry>,
}

/// A name, read from the table.
enum NameAt<'t> {
    Verbatim(Range<usize>),
    Properties(Range<usize>),
    Rich(&'t NameSrc),
}

impl Table {
    fn push(&mut self, start: usize, label: Range<usize>, kind: Kind, name: NameSrc) {
        let rel = |r: &Range<usize>| -> Option<(u32, u32)> {
            (r.start >= start && r.start <= r.end).then(|| {
                Some((
                    u32::try_from(r.start - start).ok()?,
                    u32::try_from(r.end - r.start).ok()?,
                ))
            })?
        };
        let packed = match (&name, kind, rel(&label)) {
            (NameSrc::Verbatim(r) | NameSrc::Properties(r), Kind::Key, Some(l)) => {
                let tag = match name {
                    NameSrc::Verbatim(_) => TAG_VERBATIM,
                    _ => TAG_PROPERTIES,
                };
                rel(r)
                    .filter(|n| n.1 <= LEN_MASK)
                    .map(|n| (l, n.0, n.1 | tag << TAG_SHIFT))
            }
            _ => None,
        };
        let (label, name, name_len) = match packed {
            Some(p) => p,
            None => {
                let at = u32::try_from(self.rich.len()).expect("fewer rich Segments than u32");
                self.rich.push(Entry {
                    kind,
                    hidden: false,
                    name,
                    label: label.clone(),
                });
                ((0, 0), at, TAG_RICH << TAG_SHIFT)
            }
        };
        self.packed.push(Packed {
            start,
            end: start,
            label: label.0,
            label_len: label.1,
            name,
            name_len,
        });
    }

    fn len(&self) -> usize {
        self.packed.len()
    }

    fn tag(&self, i: usize) -> u32 {
        self.packed[i].name_len >> TAG_SHIFT
    }

    fn rich(&self, i: usize) -> Option<&Entry> {
        (self.tag(i) == TAG_RICH).then(|| &self.rich[self.packed[i].name as usize])
    }

    /// The `Entry` of Segment `i`, moved to `rich` if the table held it.
    fn rich_mut(&mut self, i: usize) -> &mut Entry {
        if self.tag(i) != TAG_RICH {
            let name = match self.name(i) {
                NameAt::Verbatim(r) => NameSrc::Verbatim(r),
                NameAt::Properties(r) => NameSrc::Properties(r),
                NameAt::Rich(_) => unreachable!("not rich"),
            };
            let label = self.label(i);
            let at = u32::try_from(self.rich.len()).expect("fewer rich Segments than u32");
            self.rich.push(Entry {
                kind: Kind::Key,
                hidden: false,
                name,
                label,
            });
            let p = &mut self.packed[i];
            p.name = at;
            p.name_len = TAG_RICH << TAG_SHIFT;
        }
        let at = self.packed[i].name as usize;
        &mut self.rich[at]
    }

    fn range(&self, i: usize) -> Range<usize> {
        self.packed[i].start..self.packed[i].end
    }

    fn label(&self, i: usize) -> Range<usize> {
        match self.rich(i) {
            Some(e) => e.label.clone(),
            None => {
                let p = &self.packed[i];
                let a = p.start + p.label as usize;
                a..a + p.label_len as usize
            }
        }
    }

    fn kind(&self, i: usize) -> Kind {
        self.rich(i).map_or(Kind::Key, |e| e.kind)
    }

    fn hidden(&self, i: usize) -> bool {
        self.rich(i).is_some_and(|e| e.hidden)
    }

    fn name(&self, i: usize) -> NameAt<'_> {
        let p = &self.packed[i];
        let r = || {
            let a = p.start + p.name as usize;
            a..a + (p.name_len & LEN_MASK) as usize
        };
        match self.tag(i) {
            TAG_VERBATIM => NameAt::Verbatim(r()),
            TAG_PROPERTIES => NameAt::Properties(r()),
            _ => NameAt::Rich(&self.rich[p.name as usize].name),
        }
    }
}

/// The physical lines of a Source: (start, end of the content without its LF and the CR
/// before it, start of the next line). A lone CR is content (M6 spec §3 rule 7).
pub(crate) struct Lines<'a> {
    src: &'a [u8],
    at: usize,
}

impl<'a> Lines<'a> {
    pub fn new(src: &'a [u8]) -> Self {
        Self { src, at: 0 }
    }

    /// The lines from the one that starts at `at`.
    pub fn starting(src: &'a [u8], at: usize) -> Self {
        Self { src, at }
    }
}

impl Iterator for Lines<'_> {
    type Item = (usize, usize, usize);
    fn next(&mut self) -> Option<Self::Item> {
        let src = self.src;
        let start = self.at;
        if start >= src.len() {
            return None;
        }
        let (mut end, next) = match memchr::memchr(b'\n', &src[start..]) {
            Some(i) => (start + i, start + i + 1),
            None => (src.len(), src.len()),
        };
        if next > end && end > start && src[end - 1] == b'\r' {
            end -= 1;
        }
        self.at = next;
        Some((start, end, next))
    }
}

/// `r` without the ASCII spaces and tabs at both ends.
pub(crate) fn trim(src: &[u8], r: Range<usize>) -> Range<usize> {
    let (mut s, mut e) = (r.start, r.end);
    while s < e && matches!(src[s], b' ' | b'\t') {
        s += 1;
    }
    while e > s && matches!(src[e - 1], b' ' | b'\t') {
        e -= 1;
    }
    s..e
}

/// The Segments of a scan, opened in file order, each closed by its scanner.
#[derive(Default)]
pub(crate) struct Builder {
    table: Table,
    /// Indices of the open Segments, outermost first.
    open: Vec<usize>,
}

impl Builder {
    /// Opens a Segment at `start`, inside the open ones. None past `MAX_DEPTH`.
    pub fn open(
        &mut self,
        start: usize,
        label: Range<usize>,
        kind: Kind,
        name: NameSrc,
    ) -> Option<usize> {
        if self.open.len() >= MAX_DEPTH {
            return None;
        }
        let i = self.table.len();
        self.table.push(start, label, kind, name);
        self.open.push(i);
        Some(i)
    }

    /// How many Segments are open.
    pub fn depth(&self) -> usize {
        self.open.len()
    }

    pub fn entry_mut(&mut self, i: usize) -> &mut Entry {
        self.table.rich_mut(i)
    }

    /// Closes the open Segments past the first `depth` at `end`.
    pub fn close_to(&mut self, depth: usize, end: usize) {
        while self.open.len() > depth {
            let i = self.open.pop().expect("open");
            let e = &mut self.table.packed[i];
            e.end = end.max(e.start);
        }
    }

    /// The Segments, the open ones closed at the end of `src`, each within its parent.
    fn finish(mut self, src: &[u8]) -> Table {
        self.close_to(0, src.len());
        let mut table = self.table;
        let entries = &mut table.packed;
        // Opened in file order: a parent precedes its children. A child never ends after it.
        let mut stack: Vec<usize> = Vec::new();
        for i in 0..entries.len() {
            let start = entries[i].start;
            while stack.last().is_some_and(|&t| entries[t].end <= start) {
                stack.pop();
            }
            if let Some(&t) = stack.last() {
                let end = entries[t].end;
                let e = &mut entries[i];
                e.end = e.end.min(end).max(e.start);
            }
            stack.push(i);
        }
        table.packed.shrink_to_fit();
        table.rich.shrink_to_fit();
        table
    }
}

/// A file of a line syntax prepared once: its table, which both cursors and names read.
pub(crate) struct LinePrepared<'a> {
    src: &'a [u8],
    table: Table,
    name_chars: usize,
}

impl<'a> LinePrepared<'a> {
    pub fn new(src: &'a [u8], params: &::toml::Table, scan: fn(&[u8], &mut Builder)) -> Self {
        let mut b = Builder::default();
        scan(src, &mut b);
        Self {
            src,
            table: b.finish(src),
            name_chars: name_chars(params),
        }
    }

    /// The name of Segment `i`, cut at `name_chars`, with its pieces.
    fn name(&self, i: usize) -> (String, Pieces) {
        let mut n = CutName::new(self.name_chars);
        match self.table.name(i) {
            NameAt::Verbatim(r) => n.push_lossy(self.src, r),
            NameAt::Properties(r) => super::properties::push_key(self.src, r, &mut n),
            NameAt::Rich(NameSrc::Verbatim(r)) => n.push_lossy(self.src, r.clone()),
            NameAt::Rich(NameSrc::Properties(r)) => {
                super::properties::push_key(self.src, r.clone(), &mut n)
            }
            NameAt::Rich(NameSrc::Component { x, value, ordinal }) => {
                super::ical::push_component(self.src, x, value.as_ref(), *ordinal, &mut n)
            }
            NameAt::Rich(NameSrc::Entry { name, .. }) => n.push_lossy(self.src, name.clone()),
        }
        n.finish()
    }

    /// The signature of Segment `i`: a BibTeX entry's header, cut at twice `name_chars`.
    fn signature(&self, i: usize) -> Option<String> {
        let NameAt::Rich(NameSrc::Entry { signature, .. }) = self.table.name(i) else {
            return None;
        };
        let mut n = CutName::new(self.name_chars.saturating_mul(2));
        n.push_lossy(self.src, signature.clone());
        Some(n.finish().0)
    }

    fn segment(&self, i: usize, name: String, line: u64, end_line: u64) -> Segment {
        Segment {
            kind: self.table.kind(i).as_str().into(),
            name,
            line,
            end_line,
            level: None,
            signature: self.signature(i),
        }
    }

    /// The model's Outline (M6 plan, Task 2.1): every Segment (`None`), or those that hold
    /// one of the `anchors`, sorted. `Outline::build` counts their lines.
    pub fn outline(&self, anchors: Option<&[usize]>) -> Outline {
        let t = &self.table;
        let keep = |i: usize| match anchors {
            None => true,
            Some(a) => {
                let r = t.range(i);
                let k = a.partition_point(|&x| x < r.start);
                a.get(k).is_some_and(|&x| x < r.end)
            }
        };
        let segments = (0..t.len())
            .filter(|&i| keep(i))
            .map(|i| {
                let (name, pieces) = self.name(i);
                let label = t.label(i);
                let anchor = pieces.first().map_or(label.start, |p| p.1.start);
                OutlineSegment {
                    segment: self.segment(i, name, 0, 0),
                    range: t.range(i),
                    label,
                    name: Some(pieces),
                    name_anchor: anchor,
                    parent: None,
                    hidden: t.hidden(i),
                }
            })
            .collect();
        Outline::build(segments, self.src)
    }
}

impl Prepared for LinePrepared<'_> {
    fn cursor(&self) -> Box<dyn Cursor + '_> {
        Box::new(LineCursor {
            p: self,
            next: 0,
            stack: Vec::new(),
            last: 0,
            lines: (0, 1),
            indices: Vec::new(),
            shown: None,
        })
    }

    fn names(&self) -> anyhow::Result<Vec<Name>> {
        Ok(names_of(&self.outline(None)))
    }

    fn streams(&self) -> bool {
        true
    }
}

/// The Trails of a prepared file in file order: a stack of the Segments open at the last
/// offset, each built the first time a Trail holds it.
struct LineCursor<'p> {
    p: &'p LinePrepared<'p>,
    /// The next Segment to open.
    next: usize,
    /// The Segments that hold the last offset, outermost first, with their Segment once a
    /// Trail needed it.
    stack: Vec<(usize, Option<Segment>)>,
    last: usize,
    /// The line of byte `.0`, counted on from the last Segment start built.
    lines: (usize, u64),
    indices: Vec<usize>,
    /// The last Trail given and its Segments.
    shown: Option<(Vec<usize>, SharedTrail)>,
}

impl LineCursor<'_> {
    fn locate(&mut self, byte: usize) {
        let t = &self.p.table;
        if byte < self.last {
            self.next = 0;
            self.stack.clear();
        }
        self.last = byte;
        while self.next < t.len() && t.packed[self.next].start <= byte {
            let start = t.packed[self.next].start;
            while self
                .stack
                .last()
                .is_some_and(|&(s, _)| t.packed[s].end <= start)
            {
                self.stack.pop();
            }
            self.stack.push((self.next, None));
            self.next += 1;
        }
        while self
            .stack
            .last()
            .is_some_and(|&(s, _)| t.packed[s].end <= byte)
        {
            self.stack.pop();
        }
        self.indices.clear();
        for &(i, _) in &self.stack {
            if !t.hidden(i) && !t.label(i).contains(&byte) {
                self.indices.push(i);
            }
        }
    }

    /// The line of `byte`, counted from the last one asked, either way.
    fn line_of(&mut self, byte: usize) -> u64 {
        let src = self.p.src;
        let (at, line) = self.lines;
        let byte = byte.min(src.len());
        let line = if byte >= at {
            line + memchr::memchr_iter(b'\n', &src[at..byte]).count() as u64
        } else {
            line - memchr::memchr_iter(b'\n', &src[byte..at]).count() as u64
        };
        self.lines = (byte, line);
        line
    }

    /// The Segment at `k` in the stack, built once.
    fn segment_at(&mut self, k: usize) -> Segment {
        if let Some(s) = &self.stack[k].1 {
            return s.clone();
        }
        let i = self.stack[k].0;
        let r = self.p.table.range(i);
        let src = self.p.src;
        let line = self.line_of(r.start);
        // `Outline::build`'s last line: the one of the Section's last byte.
        let last = r.end.saturating_sub(1).max(r.start).min(src.len());
        let from = r.start.min(last);
        let end_line = line + memchr::memchr_iter(b'\n', &src[from..last]).count() as u64;
        let s = self.p.segment(i, self.p.name(i).0, line, end_line);
        self.stack[k].1 = Some(s.clone());
        s
    }
}

impl Cursor for LineCursor<'_> {
    fn trail_at(&mut self, byte: usize) -> anyhow::Result<SharedTrail> {
        self.locate(byte);
        if let Some((ix, t)) = &self.shown
            && *ix == self.indices
        {
            return Ok(t.clone());
        }
        let mut trail = Vec::with_capacity(self.indices.len());
        let mut k = 0;
        for at in 0..self.indices.len() {
            let i = self.indices[at];
            while self.stack[k].0 != i {
                k += 1;
            }
            trail.push(self.segment_at(k));
        }
        let t: SharedTrail = trail.into();
        self.shown = Some((self.indices.clone(), t.clone()));
        Ok(t)
    }

    /// The start of the outermost shown Segment that holds the last offset: every later Trail
    /// ends with it, one inside it or one that starts later.
    fn open_from(&mut self) -> Option<usize> {
        let t = &self.p.table;
        Some(
            self.stack
                .iter()
                .find(|&&(i, _)| !t.hidden(i))
                .map_or(self.last, |&(i, _)| t.packed[i].start),
        )
    }
}

/// `prepare` of a line syntax.
pub(crate) fn prepare<'a>(
    input: Input<'a>,
    params: &'a ::toml::Table,
    scan: fn(&[u8], &mut Builder),
) -> anyhow::Result<Box<dyn Prepared + 'a>> {
    Ok(Box::new(LinePrepared::new(input.bytes, params, scan)))
}

/// `outline` of a line syntax.
pub(crate) fn outline(
    src: &[u8],
    params: &::toml::Table,
    anchors: Option<&[usize]>,
    scan: fn(&[u8], &mut Builder),
) -> anyhow::Result<Outline> {
    Ok(LinePrepared::new(src, params, scan).outline(anchors))
}

#[cfg(test)]
pub(super) mod tests {
    use crate::model::Input;
    use crate::processors::Processor;
    use crate::processors::keypath::Keypath;
    use std::path::Path;

    /// The cursor asked at every byte of `src`, in order and once more from the start, gives
    /// the model's Trails, and `open_from` never starts after a Segment the Trail ends with.
    pub(crate) fn cursor_is_model(syntax: &str, src: &[u8]) {
        let params = super::super::tests::params(syntax);
        let input = Input {
            bytes: src,
            path: Path::new("x"),
            format: syntax,
            disk: None,
        };
        let anchors: Vec<usize> = (0..=src.len()).collect();
        let expected = Keypath.trails(&input, &params, &anchors).unwrap();
        let prepared = Keypath.prepare(input, &params).unwrap();
        assert!(prepared.streams());
        let mut cursor = prepared.cursor();
        for (&at, trail) in anchors.iter().zip(&expected) {
            let got = cursor.trail_at(at).unwrap();
            assert_eq!(
                got.as_ref(),
                trail.as_slice(),
                "{} at {at}",
                String::from_utf8_lossy(src)
            );
            let from = cursor.open_from().unwrap();
            assert!(from <= at, "open_from {from} past {at}");
        }
        assert_eq!(cursor.trail_at(0).unwrap().as_ref(), expected[0].as_slice());
        let names = prepared.names().unwrap();
        assert_eq!(names, Keypath.names(&input, &params).unwrap());
    }
}
