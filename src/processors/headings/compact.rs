//! The compact index of headings and buildfile (M6 spec §11, criteria 50 and 53): a scanner's
//! Segments as a table of 40 bytes each, Source offsets with no text, kept sorted as the Outline
//! keeps its Segments. The cursor builds a Segment, name, lines and signature included, only
//! when a Trail holds it; the model path (`Index::outline`) builds the whole Outline from the
//! same table, so both give what `Outline::build` gave over the scanner's items.
//!
//! An Outline of a 50 MiB file of one heading per 17 bytes held 1.89 GB (C11, 06/11/2026), about
//! 600 bytes a Segment in its Strings and Vecs; the table holds 40, and what a Segment cannot
//! say in them (a title over several lines, a signature that starts elsewhere, a column's lines)
//! goes to a side table that few Segments use.
use crate::model::Segment;
use crate::text::{CutName, Pieces};
use crate::trail::{Outline, OutlineSegment};
use std::ops::Range;

/// Characters of a signature: an opening line, as treesitter's.
const SIGNATURE_CHARS: usize = 200;

/// The text of a signature: the opening line, invalid UTF-8 as U+FFFD, cut at
/// `SIGNATURE_CHARS` characters. Only that many bytes and a few more are decoded.
pub(crate) fn signature(src: &[u8], r: Range<usize>) -> String {
    let r = r.start..r.end.min(r.start.saturating_add(4 * SIGNATURE_CHARS + 4));
    let text = String::from_utf8_lossy(&src[r]);
    text.trim_end().chars().take(SIGNATURE_CHARS).collect()
}

/// What names a Segment, as a scanner gives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SpecName<'a> {
    /// Source pieces, joined by one space each (a title over several lines): `--in-headings`
    /// searches them.
    Source(&'a [Range<usize>]),
    /// A word the scanner adds: no search finds it.
    Word(&'static str),
    /// A word and a number the scanner adds, `column 3`: no search finds it either.
    Ordinal(&'static str, usize),
}

/// One Segment as a scanner gives it.
#[derive(Debug, Clone)]
pub(crate) struct Spec<'a> {
    pub kind: &'static str,
    pub range: Range<usize>,
    pub label: Range<usize>,
    pub name: SpecName<'a>,
    /// Where an empty hit in an empty name lands.
    pub anchor: usize,
    pub signature: Option<Range<usize>>,
    pub level: Option<usize>,
    /// Whether `--in-headings` searches its name.
    pub search: bool,
    /// The Section's lines when they are not the range's.
    pub lines: Option<(u64, u64)>,
    pub hidden: bool,
}

const HIDDEN: u8 = 1;
const SEARCH: u8 = 1 << 1;
/// The label, name, anchor, signature, level and lines are `Index::extra[seg.name]`'s.
const EXTRA: u8 = 1 << 2;
/// The name is `Index::words[seg.name]`.
const WORD: u8 = 1 << 3;
/// The name is `Index::words[seg.name]` and the number `seg.name_len`.
const ORDINAL: u8 = 1 << 4;
/// The signature is `start..start + sig_len`, or the name with `SIG_IS_NAME`.
const SIG: u8 = 1 << 5;
const SIG_IS_NAME: u8 = 1 << 6;
/// The level is `seg.level`.
const LEVEL: u8 = 1 << 7;

/// One Segment of the table: 40 bytes.
#[derive(Debug, Clone)]
struct Seg {
    start: usize,
    end: usize,
    /// The label, as (offset from `start`, length).
    label: u32,
    label_len: u32,
    /// A Source name as (offset from `start`, length); a word's index and an ordinal's number;
    /// with `EXTRA`, the index of its side record.
    name: u32,
    name_len: u32,
    sig_len: u32,
    kind: u8,
    flags: u8,
    level: u16,
}

/// The name of a side record.
#[derive(Debug, Clone)]
enum ExtraName {
    Source(Box<[Range<usize>]>),
    Word(&'static str),
    Ordinal(&'static str, usize),
}

/// What a Segment cannot say in its 40 bytes.
#[derive(Debug, Clone)]
struct Extra {
    label: Range<usize>,
    name: ExtraName,
    anchor: usize,
    signature: Option<Range<usize>>,
    level: Option<usize>,
    lines: Option<(u64, u64)>,
}

/// A name, read from the table.
enum NameRef<'i> {
    Source(&'i [Range<usize>]),
    One(Range<usize>),
    Word(&'static str),
    Ordinal(&'static str, usize),
}

/// The Segments of a file, sorted by start, then by end from the longest, parents before their
/// children, each within its parent, as `Outline::build` sorts and clamps them.
#[derive(Debug, Default)]
pub(crate) struct Index {
    segs: Vec<Seg>,
    kinds: Vec<&'static str>,
    words: Vec<&'static str>,
    extra: Vec<Extra>,
}

fn small(n: usize) -> Option<u32> {
    u32::try_from(n).ok()
}

impl Index {
    /// Adds `s`, in any order, and gives its position until `finish`.
    pub fn push(&mut self, s: Spec) -> usize {
        let kind = match self.kinds.iter().position(|k| *k == s.kind) {
            Some(k) => k,
            None => {
                self.kinds.push(s.kind);
                self.kinds.len() - 1
            }
        };
        let kind = u8::try_from(kind).expect("a few kinds");
        let start = s.range.start;
        let mut flags = 0;
        if s.hidden {
            flags |= HIDDEN;
        }
        if s.search {
            flags |= SEARCH;
        }
        let rel = |r: &Range<usize>| -> Option<(u32, u32)> {
            (r.start >= start && r.start <= r.end)
                .then(|| Some((small(r.start - start)?, small(r.end - r.start)?)))?
        };
        let label = rel(&s.label);
        let name = match s.name {
            SpecName::Source([one]) if s.anchor == one.start => rel(one).map(|n| (n, 0)),
            SpecName::Source(_) => None,
            SpecName::Word(w) => Some(((self.word(w), 0), WORD)),
            SpecName::Ordinal(w, n) => small(n).map(|n| ((self.word(w), n), ORDINAL)),
        };
        let one = match s.name {
            SpecName::Source([one]) => Some(one),
            _ => None,
        };
        let sig = match &s.signature {
            None => Some((0, 0)),
            Some(r) if Some(r) == one && name.is_some() => Some((0, SIG | SIG_IS_NAME)),
            Some(r) if r.start == start && r.start <= r.end => small(r.len()).map(|n| (n, SIG)),
            Some(_) => None,
        };
        let level = match s.level {
            None => Some((0, 0)),
            Some(l) => u16::try_from(l).ok().map(|l| (l, LEVEL)),
        };
        let seg = match (label, name, sig, level, s.lines) {
            (Some(label), Some((name, nf)), Some((sig_len, sf)), Some((level, lf)), None) => Seg {
                start,
                end: s.range.end,
                label: label.0,
                label_len: label.1,
                name: name.0,
                name_len: name.1,
                sig_len,
                kind,
                flags: flags | nf | sf | lf,
                level,
            },
            _ => {
                let at = small(self.extra.len()).expect("fewer side records than u32::MAX");
                self.extra.push(Extra {
                    label: s.label.clone(),
                    name: match s.name {
                        SpecName::Source(p) => ExtraName::Source(p.into()),
                        SpecName::Word(w) => ExtraName::Word(w),
                        SpecName::Ordinal(w, n) => ExtraName::Ordinal(w, n),
                    },
                    anchor: s.anchor,
                    signature: s.signature.clone(),
                    level: s.level,
                    lines: s.lines,
                });
                Seg {
                    start,
                    end: s.range.end,
                    label: 0,
                    label_len: 0,
                    name: at,
                    name_len: 0,
                    sig_len: 0,
                    kind,
                    flags: flags | EXTRA,
                    level: 0,
                }
            }
        };
        self.segs.push(seg);
        self.segs.len() - 1
    }

    fn word(&mut self, w: &'static str) -> u32 {
        match self.words.iter().position(|k| *k == w) {
            Some(k) => k as u32,
            None => {
                self.words.push(w);
                (self.words.len() - 1) as u32
            }
        }
    }

    pub fn len(&self) -> usize {
        self.segs.len()
    }

    /// Sets the end of Segment `i` (a position `push` gave, before `finish`).
    pub fn set_end(&mut self, i: usize, end: usize) {
        self.segs[i].end = end;
    }

    /// The Section of Segment `i`.
    pub fn range(&self, i: usize) -> Range<usize> {
        self.segs[i].start..self.segs[i].end
    }

    pub fn kind(&self, i: usize) -> &'static str {
        self.kinds[self.segs[i].kind as usize]
    }

    /// Sorts the Segments as `Outline::build` does (a stable sort, needed only when a scanner
    /// gave them out of order) and clamps each to its parent's end.
    pub fn finish(mut self) -> Self {
        let order = |a: &Seg, b: &Seg| a.start.cmp(&b.start).then(b.end.cmp(&a.end));
        if !self.segs.is_sorted_by(|a, b| order(a, b).is_le()) {
            self.segs.sort_by(order);
        }
        let mut stack: Vec<usize> = Vec::new();
        for i in 0..self.segs.len() {
            let start = self.segs[i].start;
            while let Some(&top) = stack.last() {
                if start < self.segs[top].end {
                    break;
                }
                stack.pop();
            }
            if let Some(&top) = stack.last() {
                let end = self.segs[top].end;
                if self.segs[i].end > end {
                    self.segs[i].end = end;
                }
            }
            stack.push(i);
        }
        self.segs.shrink_to_fit();
        self.extra.shrink_to_fit();
        self
    }

    fn extra_of(&self, i: usize) -> Option<&Extra> {
        let s = &self.segs[i];
        (s.flags & EXTRA != 0).then(|| &self.extra[s.name as usize])
    }

    fn start(&self, i: usize) -> usize {
        self.segs[i].start
    }

    fn end(&self, i: usize) -> usize {
        self.segs[i].end
    }

    fn hidden(&self, i: usize) -> bool {
        self.segs[i].flags & HIDDEN != 0
    }

    fn label(&self, i: usize) -> Range<usize> {
        match self.extra_of(i) {
            Some(e) => e.label.clone(),
            None => {
                let s = &self.segs[i];
                let a = s.start + s.label as usize;
                a..a + s.label_len as usize
            }
        }
    }

    fn name_of(&self, i: usize) -> NameRef<'_> {
        if let Some(e) = self.extra_of(i) {
            return match &e.name {
                ExtraName::Source(p) => NameRef::Source(p),
                ExtraName::Word(w) => NameRef::Word(w),
                ExtraName::Ordinal(w, n) => NameRef::Ordinal(w, *n),
            };
        }
        let s = &self.segs[i];
        if s.flags & WORD != 0 {
            NameRef::Word(self.words[s.name as usize])
        } else if s.flags & ORDINAL != 0 {
            NameRef::Ordinal(self.words[s.name as usize], s.name_len as usize)
        } else {
            let a = s.start + s.name as usize;
            NameRef::One(a..a + s.name_len as usize)
        }
    }

    fn anchor(&self, i: usize) -> usize {
        match (self.extra_of(i), self.name_of(i)) {
            (Some(e), _) => e.anchor,
            (None, NameRef::One(r)) => r.start,
            (None, _) => self.segs[i].start,
        }
    }

    fn signature_range(&self, i: usize) -> Option<Range<usize>> {
        if let Some(e) = self.extra_of(i) {
            return e.signature.clone();
        }
        let s = &self.segs[i];
        if s.flags & SIG == 0 {
            None
        } else if s.flags & SIG_IS_NAME != 0 {
            let a = s.start + s.name as usize;
            Some(a..a + s.name_len as usize)
        } else {
            Some(s.start..s.start + s.sig_len as usize)
        }
    }

    fn level(&self, i: usize) -> Option<usize> {
        match self.extra_of(i) {
            Some(e) => e.level,
            None => {
                let s = &self.segs[i];
                (s.flags & LEVEL != 0).then_some(s.level as usize)
            }
        }
    }

    fn lines(&self, i: usize) -> Option<(u64, u64)> {
        self.extra_of(i).and_then(|e| e.lines)
    }

    /// The name's text cut at `name_chars`, and its pieces when it has Source pieces.
    fn name(&self, src: &[u8], i: usize, name_chars: usize) -> (String, Option<Pieces>) {
        let source = |parts: &mut dyn Iterator<Item = Range<usize>>| {
            let mut n = CutName::new(name_chars);
            let mut last: Option<usize> = None;
            for r in parts {
                if let Some(end) = last {
                    // The line break between two lines of a title, as one space.
                    n.push_decoded(' ', end..r.start);
                }
                last = Some(r.end);
                n.push_lossy(src, r);
            }
            let (t, p) = n.finish();
            (t, Some(p))
        };
        match self.name_of(i) {
            NameRef::Source(p) => source(&mut p.iter().cloned()),
            NameRef::One(r) => source(&mut std::iter::once(r)),
            NameRef::Word(w) => (w.to_string(), None),
            NameRef::Ordinal(w, n) => (format!("{w} {n}"), None),
        }
    }

    /// The Segment of `i` with the lines `lines` gives, unless it has its own.
    fn segment(
        &self,
        src: &[u8],
        i: usize,
        name_chars: usize,
        lines: impl FnOnce() -> (u64, u64),
    ) -> Segment {
        let (line, end_line) = self.lines(i).unwrap_or_else(lines);
        Segment {
            kind: self.kind(i).into(),
            name: self.name(src, i, name_chars).0,
            line,
            end_line,
            level: self.level(i),
            signature: self.signature_range(i).map(|r| signature(src, r)),
        }
    }

    /// The Outline of the table: what `Outline::build` gives over the scanner's items, built
    /// for the model path and `--in-headings`' names.
    pub fn outline(&self, src: &[u8], name_chars: usize) -> Outline {
        let segments = (0..self.segs.len())
            .map(|i| {
                let (text, pieces) = self.name(src, i, name_chars);
                let mut s = OutlineSegment::new(self.kind(i), text, self.range(i), self.label(i));
                s.hidden = self.hidden(i);
                if let Some(pieces) =
                    pieces.filter(|_| self.segs[i].flags & SEARCH != 0 && !s.hidden)
                {
                    s.name = Some(pieces);
                    s.name_anchor = self.anchor(i);
                }
                if let Some((first, last)) = self.lines(i) {
                    s.segment.line = first;
                    s.segment.end_line = last;
                }
                s.segment.level = self.level(i);
                s.segment.signature = self.signature_range(i).map(|r| signature(src, r));
                s
            })
            .collect();
        Outline::build(segments, src)
    }
}

/// The bytes whose Trail ends with each Segment, as `trail::Owned` gives them over an Outline
/// (code review r2, Fable #2), made the first time `open_from` asks (`--expand`).
struct Owned {
    first: Vec<usize>,
    covered: Vec<Range<usize>>,
    ends: Vec<usize>,
}

impl Owned {
    fn new(ix: &Index) -> Self {
        let n = ix.segs.len();
        // The parents, by containment, as `Outline::build` gives them.
        let mut parent: Vec<Option<usize>> = Vec::with_capacity(n);
        let mut stack: Vec<usize> = Vec::new();
        for i in 0..n {
            let start = ix.start(i);
            while stack.last().is_some_and(|&t| start >= ix.end(t)) {
                stack.pop();
            }
            parent.push(stack.last().copied());
            stack.push(i);
        }
        drop(stack);
        // The nearest ancestor that is not hidden: the parents precede their children.
        let mut shown: Vec<Option<usize>> = Vec::with_capacity(n);
        for p in &parent {
            let up = p.and_then(|p| if ix.hidden(p) { shown[p] } else { Some(p) });
            shown.push(up);
        }
        drop(parent);
        let mut children: Vec<(usize, usize, usize)> = (0..n)
            .filter(|&i| !ix.hidden(i))
            .filter_map(|i| {
                let label = ix.label(i);
                let from = if label.is_empty() {
                    ix.start(i)
                } else {
                    label.end.max(ix.start(i))
                };
                shown[i].map(|p| (p, from, ix.end(i)))
            })
            .filter(|&(_, a, b)| a < b)
            .collect();
        drop(shown);
        children.sort_unstable_by_key(|&(p, a, _)| (p, a));
        let mut first = Vec::with_capacity(n + 1);
        let mut covered: Vec<Range<usize>> = Vec::new();
        let mut at = 0;
        for p in 0..n {
            first.push(covered.len());
            let from = covered.len();
            while at < children.len() && children[at].0 == p {
                let (_, a, b) = children[at];
                let mine = covered.len() > from;
                match covered.last_mut() {
                    Some(m) if mine && a <= m.end => m.end = m.end.max(b),
                    _ => covered.push(a..b),
                }
                at += 1;
            }
        }
        first.push(covered.len());
        drop(children);
        covered.shrink_to_fit();
        let mut owned = Owned {
            first,
            covered,
            ends: Vec::new(),
        };
        owned.ends = (0..n)
            .map(|p| owned.last_own(ix, p).map_or(0, |b| b + 1))
            .collect();
        owned
    }

    fn children(&self, p: usize) -> &[Range<usize>] {
        &self.covered[self.first[p]..self.first[p + 1]]
    }

    fn last_own(&self, ix: &Index, p: usize) -> Option<usize> {
        let (start, label) = (ix.start(p), ix.label(p));
        let covered = self.children(p);
        let mut end = ix.end(p);
        let mut k = covered.len();
        while end > start {
            let b = end - 1;
            if label.contains(&b) {
                end = label.start;
                continue;
            }
            while k > 0 && covered[k - 1].start > b {
                k -= 1;
            }
            match k.checked_sub(1).map(|j| &covered[j]) {
                Some(m) if m.end > b => end = m.start,
                _ => return Some(b),
            }
        }
        None
    }

    fn find(
        &self,
        ix: &Index,
        p: usize,
        from: usize,
        f: &mut dyn FnMut(Range<usize>) -> Option<usize>,
    ) -> Option<usize> {
        let label = ix.label(p);
        let (start, end) = (ix.start(p), ix.end(p));
        let mut each = |r: Range<usize>| -> Option<usize> {
            for piece in [
                r.start..r.end.min(label.start),
                r.start.max(label.end)..r.end,
            ] {
                if !piece.is_empty()
                    && let Some(b) = f(piece)
                {
                    return Some(b);
                }
            }
            None
        };
        let covered = self.children(p);
        let mut at = from.max(start);
        let mut k = covered.partition_point(|c| c.end <= at);
        while at < end {
            match covered.get(k) {
                Some(c) if c.start <= at => {
                    at = c.end;
                    k += 1;
                }
                Some(c) => {
                    let to = c.start.min(end);
                    if let Some(b) = each(at..to) {
                        return Some(b);
                    }
                    at = to;
                }
                None => return each(at..end),
            }
        }
        None
    }
}

/// A Segment open at the cursor's offset, and its Segment once a Trail needed it.
struct Open {
    id: usize,
    seg: Option<Segment>,
}

/// The Trails of an index in file order, as `trail::TrailCursor` gives them over its Outline:
/// a stack of the Segments open at the last offset, each built the first time a Trail holds it.
pub(crate) struct IndexCursor<'p> {
    src: &'p [u8],
    ix: &'p Index,
    name_chars: usize,
    next: usize,
    stack: Vec<Open>,
    last: usize,
    /// The line of byte `.0`, counted on from the last Segment start built.
    lines: (usize, u64),
    owned: Option<Owned>,
    found: Vec<(usize, usize, Option<usize>)>,
    indices: Vec<usize>,
    shared: Option<(Vec<usize>, crate::model::SharedTrail)>,
}

impl<'p> IndexCursor<'p> {
    pub fn new(src: &'p [u8], ix: &'p Index, name_chars: usize) -> Self {
        Self {
            src,
            ix,
            name_chars,
            next: 0,
            stack: Vec::new(),
            last: 0,
            lines: (0, 1),
            owned: None,
            found: Vec::new(),
            indices: Vec::new(),
            shared: None,
        }
    }

    /// Moves the stack to `byte`, and gives the positions in it of the Trail's Segments.
    fn reach(&mut self, byte: usize) {
        let ix = self.ix;
        if byte < self.last {
            self.next = 0;
            self.stack.clear();
        }
        self.last = byte;
        while self.next < ix.segs.len() && ix.start(self.next) <= byte {
            let start = ix.start(self.next);
            while self.stack.last().is_some_and(|t| ix.end(t.id) <= start) {
                self.stack.pop();
            }
            self.stack.push(Open {
                id: self.next,
                seg: None,
            });
            self.next += 1;
        }
        while self.stack.last().is_some_and(|t| ix.end(t.id) <= byte) {
            self.stack.pop();
        }
        self.indices.clear();
        for o in &self.stack {
            if !ix.hidden(o.id) && !ix.label(o.id).contains(&byte) {
                self.indices.push(o.id);
            }
        }
    }

    /// The line of `byte`, counted from the last one asked, either way.
    fn line_of(&mut self, byte: usize) -> u64 {
        let (at, line) = self.lines;
        let byte = byte.min(self.src.len());
        let line = if byte >= at {
            line + memchr::memchr_iter(b'\n', &self.src[at..byte]).count() as u64
        } else {
            line - memchr::memchr_iter(b'\n', &self.src[byte..at]).count() as u64
        };
        self.lines = (byte, line);
        line
    }

    fn segment_at(&mut self, k: usize) -> Segment {
        if let Some(s) = &self.stack[k].seg {
            return s.clone();
        }
        let (ix, src, id) = (self.ix, self.src, self.stack[k].id);
        let s = if ix.lines(id).is_some() {
            ix.segment(src, id, self.name_chars, || (0, 0))
        } else {
            let (start, end) = (ix.start(id), ix.end(id));
            let line = self.line_of(start);
            // `Outline::build`'s last line: the one of the Section's last byte.
            let last = end.saturating_sub(1).max(start).min(src.len());
            let from = start.min(last);
            let end_line = line + memchr::memchr_iter(b'\n', &src[from..last]).count() as u64;
            ix.segment(src, id, self.name_chars, || (line, end_line))
        };
        self.stack[k].seg = Some(s.clone());
        s
    }
}

impl super::super::Cursor for IndexCursor<'_> {
    fn trail_at(&mut self, byte: usize) -> anyhow::Result<crate::model::SharedTrail> {
        self.reach(byte);
        if let Some((ix, t)) = &self.shared
            && *ix == self.indices
        {
            return Ok(t.clone());
        }
        let mut trail = Vec::with_capacity(self.indices.len());
        let mut k = 0;
        for at in 0..self.indices.len() {
            let id = self.indices[at];
            while self.stack[k].id != id {
                k += 1;
            }
            trail.push(self.segment_at(k));
        }
        let t: crate::model::SharedTrail = std::rc::Rc::from(trail);
        self.shared = Some((self.indices.clone(), t.clone()));
        Ok(t)
    }

    fn open_from(&mut self) -> Option<usize> {
        let ix = self.ix;
        let owned = self.owned.get_or_insert_with(|| Owned::new(ix));
        let last = self.last;
        Some(
            self.stack
                .iter()
                .find(|o| !ix.hidden(o.id) && owned.ends[o.id] > last)
                .map_or(last, |o| ix.start(o.id)),
        )
    }

    fn open_from_with(
        &mut self,
        first: &mut dyn FnMut(Range<usize>) -> Option<usize>,
    ) -> Option<usize> {
        let ix = self.ix;
        let owned = self.owned.get_or_insert_with(|| Owned::new(ix));
        let last = self.last;
        let stack = &self.stack;
        self.found.retain(|f| stack.iter().any(|o| o.id == f.0));
        let at_last = stack
            .iter()
            .rev()
            .map(|o| o.id)
            .find(|&i| !ix.hidden(i) && !ix.label(i).contains(&last));
        for i in stack.iter().map(|o| o.id) {
            if ix.hidden(i) || owned.ends[i] <= last {
                continue;
            }
            if Some(i) == at_last {
                return Some(ix.start(i));
            }
            let known = self
                .found
                .iter()
                .find(|f| f.0 == i && f.1 <= last && f.2.is_none_or(|b| last <= b))
                .map(|f| f.2);
            let next = match known {
                Some(next) => next,
                None => {
                    let next = owned.find(ix, i, last, first);
                    self.found.retain(|f| f.0 != i);
                    self.found.push((i, last, next));
                    next
                }
            };
            if next.is_some() {
                return Some(ix.start(i));
            }
        }
        Some(last)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_segment_is_40_bytes() {
        assert_eq!(std::mem::size_of::<Seg>(), 40);
    }

    fn spec<'a>(kind: &'static str, range: Range<usize>, name: SpecName<'a>) -> Spec<'a> {
        Spec {
            kind,
            label: range.start..range.start,
            range,
            name,
            anchor: 0,
            signature: None,
            level: None,
            search: true,
            lines: None,
            hidden: false,
        }
    }

    /// Every field a Spec gives comes back, from the table or from its side record.
    #[test]
    fn the_table_and_the_side_records_give_back_every_field() {
        let src = b"== A\nbody\n=== B\n|x|\n";
        let mut ix = Index::default();
        let mut a = spec(
            "heading",
            0..src.len(),
            SpecName::Source(std::slice::from_ref(&(3..4))),
        );
        a.anchor = 3;
        a.label = 0..5;
        a.level = Some(1);
        a.signature = Some(0..4);
        ix.push(a);
        let mut b = spec(
            "heading",
            10..src.len(),
            SpecName::Source(std::slice::from_ref(&(14..15))),
        );
        b.anchor = 14;
        b.label = 10..16;
        b.signature = Some(14..15);
        b.level = Some(70_000);
        ix.push(b);
        let mut c = spec("column", 16..19, SpecName::Ordinal("column", 2));
        c.search = false;
        c.lines = Some((4, 4));
        ix.push(c);
        let ix = ix.finish();
        assert_eq!(
            ix.extra.len(),
            2,
            "the level past u16 and the column's lines"
        );
        let o = ix.outline(src, 40);
        let s: Vec<_> = o.segments.iter().map(|s| &s.segment).collect();
        assert_eq!(
            (s[0].name.as_str(), s[0].level, s[0].signature.as_deref()),
            ("A", Some(1), Some("== A"))
        );
        assert_eq!(o.segments[0].label, 0..5);
        assert_eq!(o.segments[0].name_anchor, 3);
        assert_eq!(
            (s[1].name.as_str(), s[1].level, s[1].signature.as_deref()),
            ("B", Some(70_000), Some("B"))
        );
        assert_eq!(
            (s[2].name.as_str(), s[2].line, s[2].end_line),
            ("column 2", 4, 4)
        );
        assert!(o.segments[2].name.is_none());
    }
}
