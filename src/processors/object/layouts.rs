//! The binary layouts of M6 (M6 spec §9): JPEG, PNG, WebP, TIFF, ID3, MP4, WebAssembly and
//! Java classes, each a walk of our own over length-prefixed units that never reads past a
//! unit's end nor the file's.
//!
//! A `BinaryMap` is a set of nodes, each with an explicit parent, and the byte intervals each
//! node owns (M6 plan, D12). The owner of a byte is the last interval added that holds it, so a
//! reader adds a container before what it holds, and an owner that lies elsewhere in the file
//! (a TIFF value past its IFD, a Java string loaded by a method) is said explicitly instead of
//! inferred from where its bytes are.
//!
//! What a file costs is bounded by its size: every unit a reader walks is at least a few bytes
//! of the file, read once, and a count it declares is checked against the bytes left before
//! anything is reserved for it.
use crate::model::{Segment, Trail};
use crate::trail::LineIndex;
use std::borrow::Cow;
use std::cell::OnceCell;
use std::ops::Range;

/// Characters of a name before it is cut with `…`, when the Definition sets no `name_chars`.
pub const NAME_CHARS: usize = 40;

/// Past this many Segments in a Trail, no more open (M6 spec §3 rule 8).
const MAX_DEPTH: usize = 256;

/// Below this many owner intervals a Trail is found by looking at each; above it, through an
/// index of the intervals, built once.
const SCAN: usize = 32;

/// One node: a Segment of the Trails of the bytes it owns and of its descendants'.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinaryNode<'a> {
    pub parent: Option<usize>,
    pub kind: &'static str,
    pub name: Cow<'a, str>,
    /// The bytes its Section covers: its unit in the file.
    pub span: Range<usize>,
    depth: usize,
}

/// What the `object` Processor knows of a file of an M6 layout: its nodes and the bytes each
/// owns.
#[derive(Debug)]
pub struct BinaryMap<'a> {
    nodes: Vec<BinaryNode<'a>>,
    /// (interval, node), in the order they were added: the last that holds a byte owns it.
    owners: Vec<(Range<usize>, usize)>,
    /// With more than `SCAN` owners: the start of each run of bytes with one owner, and that
    /// owner (`usize::MAX` for none), sorted by start.
    index: OnceCell<Vec<(usize, usize)>>,
}

/// The nodes and owners a reader adds, over the file's `bytes`.
pub(super) struct Builder<'a> {
    pub bytes: &'a [u8],
    pub name_chars: usize,
    nodes: Vec<BinaryNode<'a>>,
    owners: Vec<(Range<usize>, usize)>,
}

impl<'a> Builder<'a> {
    fn new(bytes: &'a [u8], name_chars: usize) -> Self {
        Self {
            bytes,
            name_chars,
            nodes: Vec::new(),
            owners: Vec::new(),
        }
    }

    /// A node under `parent` that owns `span` (clamped to the file), or `parent` itself when
    /// the Trail is already `MAX_DEPTH` deep: deeper bytes get the deepest Trail. Without a
    /// parent, a top-level node.
    pub fn node(
        &mut self,
        parent: Option<usize>,
        kind: &'static str,
        name: Cow<'a, str>,
        span: Range<usize>,
    ) -> usize {
        let i = self.open(parent, kind, name, span.clone());
        if Some(i) != parent {
            self.own(i, span);
        }
        i
    }

    /// `node` without owning its span yet: a reader that learns a unit's end after its
    /// children's starts (an XML element) adds what each owns afterwards, parents first.
    pub fn open(
        &mut self,
        parent: Option<usize>,
        kind: &'static str,
        name: Cow<'a, str>,
        span: Range<usize>,
    ) -> usize {
        let depth = parent.map_or(1, |p| self.nodes[p].depth + 1);
        if depth > MAX_DEPTH
            && let Some(p) = parent
        {
            return p;
        }
        let span = self.clamp(span);
        self.nodes.push(BinaryNode {
            parent,
            kind,
            name: cut(name, self.name_chars),
            span,
            depth,
        });
        self.nodes.len() - 1
    }

    /// The Section of `node` covers `span` (clamped to the file).
    pub fn set_span(&mut self, node: usize, span: Range<usize>) {
        let span = self.clamp(span);
        self.nodes[node].span = span;
    }

    /// `node` owns `range` too (clamped to the file), above what owned it before.
    pub fn own(&mut self, node: usize, range: Range<usize>) {
        let range = self.clamp(range);
        if range.start < range.end {
            self.owners.push((range, node));
        }
    }

    fn clamp(&self, r: Range<usize>) -> Range<usize> {
        let end = r.end.min(self.bytes.len());
        r.start.min(end)..end
    }

    /// A name read from `bytes[r]` as UTF-8, its invalid bytes as U+FFFD.
    pub fn utf8(&self, r: Range<usize>) -> Cow<'a, str> {
        String::from_utf8_lossy(self.bytes.get(r).unwrap_or_default())
    }

    fn finish(self) -> BinaryMap<'a> {
        BinaryMap {
            nodes: self.nodes,
            owners: self.owners,
            index: OnceCell::new(),
        }
    }
}

/// `name` cut to `max` characters, the last one `…` (M6 spec §3 rule 4); 0 cuts nothing.
fn cut(name: Cow<'_, str>, max: usize) -> Cow<'_, str> {
    if max == 0 || name.len() <= max || name.chars().count() <= max {
        return name;
    }
    let mut s: String = name.chars().take(max - 1).collect();
    s.push('…');
    Cow::Owned(s)
}

/// `b` as Latin-1, borrowed when it is ASCII.
pub(super) fn latin1(b: &[u8]) -> Cow<'_, str> {
    if b.is_ascii() {
        // ASCII is UTF-8.
        Cow::Borrowed(std::str::from_utf8(b).unwrap_or_default())
    } else {
        Cow::Owned(b.iter().map(|&c| c as char).collect())
    }
}

/// Fixed-width reads that are None past the end of `b`.
pub(super) fn u16_at(b: &[u8], at: usize, le: bool) -> Option<u16> {
    let v: [u8; 2] = b.get(at..at.checked_add(2)?)?.try_into().ok()?;
    Some(if le {
        u16::from_le_bytes(v)
    } else {
        u16::from_be_bytes(v)
    })
}

pub(super) fn u32_at(b: &[u8], at: usize, le: bool) -> Option<u32> {
    let v: [u8; 4] = b.get(at..at.checked_add(4)?)?.try_into().ok()?;
    Some(if le {
        u32::from_le_bytes(v)
    } else {
        u32::from_be_bytes(v)
    })
}

pub(super) fn u64_at(b: &[u8], at: usize, le: bool) -> Option<u64> {
    let v: [u8; 8] = b.get(at..at.checked_add(8)?)?.try_into().ok()?;
    Some(if le {
        u64::from_le_bytes(v)
    } else {
        u64::from_be_bytes(v)
    })
}

/// `start..start + len` when it lies inside `within`.
pub(super) fn sub(within: &Range<usize>, start: usize, len: usize) -> Option<Range<usize>> {
    let end = start.checked_add(len)?;
    (start >= within.start && end <= within.end).then_some(start..end)
}

impl<'a> BinaryMap<'a> {
    /// The map of `bytes` read with `layout`'s reader, or None when the reader does not read
    /// them (the file is then Plain, without a warning: M6 spec §9) or there is no such layout.
    pub fn parse(bytes: &'a [u8], layout: &str) -> Option<Self> {
        Self::parse_with(bytes, layout, NAME_CHARS)
    }

    /// `parse` with names cut at `name_chars`, as the Definition's parameter says.
    pub fn parse_with(bytes: &'a [u8], layout: &str, name_chars: usize) -> Option<Self> {
        super::quietly(|| Self::read(bytes, layout, name_chars))
    }

    /// `parse` without catching a panic, for the fuzz: a panic in a reader must fail it.
    #[doc(hidden)]
    pub fn parse_loudly(bytes: &'a [u8], layout: &str) -> Option<Self> {
        Self::read(bytes, layout, NAME_CHARS)
    }

    fn read(bytes: &'a [u8], layout: &str, name_chars: usize) -> Option<Self> {
        let mut b = Builder::new(bytes, name_chars);
        match layout {
            "tiff" => super::tiff::layout(&mut b)?,
            "jpeg" => super::jpeg::layout(&mut b)?,
            "png" => super::png::layout(&mut b)?,
            "webp" => super::webp::layout(&mut b)?,
            "id3" => super::id3::layout(&mut b)?,
            "mp4" => super::mp4::layout(&mut b)?,
            "wasm" => super::wasm::layout(&mut b)?,
            "class" => super::class::layout(&mut b)?,
            _ => return None,
        }
        Some(b.finish())
    }

    /// The nodes, for the tests.
    #[doc(hidden)]
    pub fn nodes(&self) -> &[BinaryNode<'a>] {
        &self.nodes
    }

    /// The node that owns the byte at `at`.
    fn owner(&self, at: usize) -> Option<usize> {
        if self.owners.len() <= SCAN {
            return self
                .owners
                .iter()
                .rev()
                .find(|(r, _)| r.contains(&at))
                .map(|&(_, n)| n);
        }
        let index = self.index.get_or_init(|| self.build_index());
        let i = index.partition_point(|&(s, _)| s <= at).checked_sub(1)?;
        let n = index[i].1;
        (n != usize::MAX).then_some(n)
    }

    /// The runs of bytes with one owner: a sweep over the intervals' ends, the owner of each
    /// run the last added of the intervals open over it.
    fn build_index(&self) -> Vec<(usize, usize)> {
        let mut events: Vec<(usize, bool, usize)> = Vec::with_capacity(self.owners.len() * 2);
        for (i, (r, _)) in self.owners.iter().enumerate() {
            events.push((r.start, true, i));
            events.push((r.end, false, i));
        }
        events.sort_unstable_by_key(|&(at, open, i)| (at, open, i));
        let mut open = std::collections::BTreeSet::new();
        let mut runs: Vec<(usize, usize)> = Vec::new();
        let mut k = 0;
        while k < events.len() {
            let at = events[k].0;
            while k < events.len() && events[k].0 == at {
                let (_, opens, i) = events[k];
                if opens {
                    open.insert(i);
                } else {
                    open.remove(&i);
                }
                k += 1;
            }
            let owner = open.last().map_or(usize::MAX, |&i| self.owners[i].1);
            if runs.last().is_none_or(|&(_, o)| o != owner) {
                runs.push((at, owner));
            }
        }
        runs
    }

    /// The Trail of the byte at file offset `at`: its owner and the owner's ancestors, outermost
    /// first; empty when no node owns it (M6 spec §3 rule 3).
    pub fn trail(&self, at: usize, lines: &LineIndex) -> Trail {
        let mut chain = Vec::new();
        let mut n = self.owner(at);
        while let Some(i) = n {
            chain.push(i);
            n = self.nodes[i].parent;
        }
        chain
            .into_iter()
            .rev()
            .map(|i| {
                let node = &self.nodes[i];
                let r = &node.span;
                Segment {
                    kind: node.kind.into(),
                    name: node.name.clone().into_owned(),
                    line: lines.line_of(r.start),
                    end_line: lines.line_of(r.end.saturating_sub(1).max(r.start)),
                    level: None,
                    signature: None,
                }
            })
            .collect()
    }

    /// The source byte ranges of the nodes and of what they own, for the fuzz: never past the
    /// file.
    #[doc(hidden)]
    pub fn ranges(&self) -> impl Iterator<Item = Range<usize>> + '_ {
        self.nodes
            .iter()
            .map(|n| n.span.clone())
            .chain(self.owners.iter().map(|(r, _)| r.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(m: &BinaryMap, at: usize) -> Vec<String> {
        let lines = LineIndex::new(b"");
        m.trail(at, &lines).into_iter().map(|s| s.name).collect()
    }

    /// The last interval added that holds a byte owns it, whether the map scans the intervals
    /// or indexes them.
    #[test]
    fn the_last_owner_added_wins_scanned_or_indexed() {
        let bytes = vec![0u8; 400];
        for extra in [0, 2 * SCAN] {
            let mut b = Builder::new(&bytes, 40);
            let top = b.node(None, "object", "top".into(), 0..100);
            let inner = b.node(Some(top), "section", "inner".into(), 10..20);
            b.own(inner, 50..60);
            let leaf = b.node(Some(inner), "key", "leaf".into(), 12..14);
            for i in 0..extra {
                b.node(None, "object", format!("o{i}").into(), 200 + i..201 + i);
            }
            let m = b.finish();
            assert_eq!(names(&m, 5), ["top"]);
            assert_eq!(names(&m, 10), ["top", "inner"]);
            assert_eq!(names(&m, 13), ["top", "inner", "leaf"]);
            assert_eq!(names(&m, 14), ["top", "inner"]);
            assert_eq!(names(&m, 55), ["top", "inner"]);
            assert_eq!(names(&m, 100), Vec::<String>::new());
            assert_eq!(names(&m, 399), Vec::<String>::new());
            let _ = leaf;
        }
    }

    #[test]
    fn past_the_depth_limit_no_node_opens() {
        let bytes = vec![0u8; 10];
        let mut b = Builder::new(&bytes, 40);
        let mut n = b.node(None, "object", "0".into(), 0..10);
        for i in 1..MAX_DEPTH + 10 {
            n = b.node(Some(n), "section", i.to_string().into(), 0..10);
        }
        let m = b.finish();
        assert_eq!(m.trail(3, &LineIndex::new(&bytes)).len(), MAX_DEPTH);
    }

    #[test]
    fn names_are_cut_and_intervals_clamped() {
        let bytes = vec![0u8; 10];
        let mut b = Builder::new(&bytes, 4);
        b.node(None, "object", "abcdef".into(), 5..99);
        let m = b.finish();
        assert_eq!(m.nodes()[0].name, "abc…");
        assert!(m.ranges().all(|r| r.end <= 10));
    }
}
