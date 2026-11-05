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
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Kind::Key => "key",
        }
    }
}

/// Where a Segment's name comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NameSrc {
    /// The Source bytes as written.
    Verbatim(Range<usize>),
}

/// One Segment of a line syntax, as the scan found it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Entry {
    /// The Section's bytes.
    pub range: Range<usize>,
    /// The bytes that name it: a Match there is outside it (M6 spec §3 rule 5).
    pub label: Range<usize>,
    pub line: u64,
    pub end_line: u64,
    pub kind: Kind,
    pub hidden: bool,
    pub name: NameSrc,
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
    entries: Vec<Entry>,
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
        let i = self.entries.len();
        self.entries.push(Entry {
            range: start..start,
            label,
            line: 0,
            end_line: 0,
            kind,
            hidden: false,
            name,
        });
        self.open.push(i);
        Some(i)
    }

    /// Closes the open Segments past the first `depth` at `end`.
    pub fn close_to(&mut self, depth: usize, end: usize) {
        while self.open.len() > depth {
            let i = self.open.pop().expect("open");
            let e = &mut self.entries[i];
            e.range.end = end.max(e.range.start);
        }
    }

    /// The Segments, the open ones closed at the end of `src`, each within its parent, with
    /// their lines.
    pub fn finish(mut self, src: &[u8]) -> Vec<Entry> {
        self.close_to(0, src.len());
        let mut entries = self.entries;
        // Opened in file order: a parent precedes its children. A child never ends after it.
        let mut stack: Vec<usize> = Vec::new();
        for i in 0..entries.len() {
            let start = entries[i].range.start;
            while stack.last().is_some_and(|&t| entries[t].range.end <= start) {
                stack.pop();
            }
            if let Some(&t) = stack.last() {
                let end = entries[t].range.end;
                let e = &mut entries[i];
                e.range.end = e.range.end.min(end).max(e.range.start);
            }
            stack.push(i);
        }
        fill_lines(&mut entries, src);
        entries.shrink_to_fit();
        entries
    }
}

/// The first and last line of each Entry's range, counting line breaks once per pass.
fn fill_lines(entries: &mut [Entry], src: &[u8]) {
    let count = |at: &mut usize, line: &mut u64, byte: usize| {
        let to = byte.min(src.len());
        if to > *at {
            *line += memchr::memchr_iter(b'\n', &src[*at..to]).count() as u64;
            *at = to;
        }
        *line
    };
    let (mut at, mut line) = (0usize, 1u64);
    let mut ends: Vec<(usize, usize)> = Vec::with_capacity(entries.len());
    for (i, e) in entries.iter_mut().enumerate() {
        e.line = count(&mut at, &mut line, e.range.start);
        ends.push((e.range.end.saturating_sub(1).max(e.range.start), i));
    }
    ends.sort_unstable();
    let (mut at, mut line) = (0usize, 1u64);
    for (byte, i) in ends {
        entries[i].end_line = count(&mut at, &mut line, byte);
    }
}

/// A file of a line syntax prepared once: its Entries, which both cursors and names read.
pub(crate) struct LinePrepared<'a> {
    src: &'a [u8],
    entries: Vec<Entry>,
    name_chars: usize,
}

impl<'a> LinePrepared<'a> {
    pub fn new(src: &'a [u8], params: &::toml::Table, scan: fn(&[u8], &mut Builder)) -> Self {
        let mut b = Builder::default();
        scan(src, &mut b);
        Self {
            src,
            entries: b.finish(src),
            name_chars: name_chars(params),
        }
    }

    /// The name of `e`, cut at `name_chars`, with its pieces.
    fn name(&self, e: &Entry) -> (String, Pieces) {
        let mut n = CutName::new(self.name_chars);
        match &e.name {
            NameSrc::Verbatim(r) => n.push_lossy(self.src, r.clone()),
        }
        n.finish()
    }

    fn segment(&self, e: &Entry) -> Segment {
        Segment {
            kind: e.kind.as_str().into(),
            name: self.name(e).0,
            line: e.line,
            end_line: e.end_line,
            level: None,
            signature: None,
        }
    }

    /// The model's Outline (M6 plan, Task 2.1): every Segment (`None`), or those that hold
    /// one of the `anchors`, sorted.
    pub fn outline(&self, anchors: Option<&[usize]>) -> Outline {
        let keep = |e: &Entry| match anchors {
            None => true,
            Some(a) => {
                let k = a.partition_point(|&x| x < e.range.start);
                a.get(k).is_some_and(|&x| x < e.range.end)
            }
        };
        let segments = self
            .entries
            .iter()
            .filter(|e| keep(e))
            .map(|e| {
                let (name, pieces) = self.name(e);
                let anchor = pieces.first().map_or(e.label.start, |p| p.1.start);
                OutlineSegment {
                    segment: Segment {
                        kind: e.kind.as_str().into(),
                        name,
                        line: e.line,
                        end_line: e.end_line,
                        level: None,
                        signature: None,
                    },
                    range: e.range.clone(),
                    label: e.label.clone(),
                    name: Some(pieces),
                    name_anchor: anchor,
                    parent: None,
                    hidden: e.hidden,
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
            indices: Vec::new(),
            shown: None,
            cache: Vec::new(),
        })
    }

    fn names(&self) -> anyhow::Result<Vec<Name>> {
        Ok(names_of(&self.outline(None)))
    }

    fn streams(&self) -> bool {
        true
    }
}

/// The Trails of a prepared file in file order: a stack of the Entries open at the last
/// offset, and the Segments of the last Trail, built once each.
struct LineCursor<'p> {
    p: &'p LinePrepared<'p>,
    /// The next Entry to open.
    next: usize,
    /// The Entries that hold the last offset, outermost first.
    stack: Vec<usize>,
    last: usize,
    indices: Vec<usize>,
    /// The last Trail given and its Entries.
    shown: Option<(Vec<usize>, SharedTrail)>,
    /// The Segments of the last Trail built, by Entry.
    cache: Vec<(usize, Segment)>,
}

impl LineCursor<'_> {
    fn locate(&mut self, byte: usize) {
        let entries = &self.p.entries;
        if byte < self.last {
            self.next = 0;
            self.stack.clear();
        }
        self.last = byte;
        while self.next < entries.len() && entries[self.next].range.start <= byte {
            let start = entries[self.next].range.start;
            while self
                .stack
                .last()
                .is_some_and(|&t| entries[t].range.end <= start)
            {
                self.stack.pop();
            }
            self.stack.push(self.next);
            self.next += 1;
        }
        while self
            .stack
            .last()
            .is_some_and(|&t| entries[t].range.end <= byte)
        {
            self.stack.pop();
        }
        self.indices.clear();
        self.indices.extend(
            self.stack
                .iter()
                .copied()
                .filter(|&i| !entries[i].hidden && !entries[i].label.contains(&byte)),
        );
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
        let mut cache = Vec::with_capacity(self.indices.len());
        for &i in &self.indices {
            let seg = match self.cache.iter().find(|(j, _)| *j == i) {
                Some((_, s)) => s.clone(),
                None => self.p.segment(&self.p.entries[i]),
            };
            cache.push((i, seg));
        }
        self.cache = cache;
        let t: SharedTrail = self.cache.iter().map(|(_, s)| s.clone()).collect();
        self.shown = Some((self.indices.clone(), t.clone()));
        Ok(t)
    }

    /// The start of the outermost shown Entry that holds the last offset: every later Trail
    /// ends with it, one inside it or one that starts later.
    fn open_from(&mut self) -> Option<usize> {
        let entries = &self.p.entries;
        Some(
            self.stack
                .iter()
                .find(|&&i| !entries[i].hidden)
                .map_or(self.last, |&i| entries[i].range.start),
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
