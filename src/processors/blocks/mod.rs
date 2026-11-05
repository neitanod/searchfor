//! blocks: the block scopes of nginx, Apache, HCL, GraphQL and Protobuf (M6 spec §7.6–§7.8,
//! §8.6–§8.7). A scanner per `syntax` reads the file once into a `BlockIndex`, a compact table
//! of Source ranges, label ranges, parents and name pieces in file order; the cursor builds a
//! Segment only when a Trail needs it (M6 plan, Chunk 3, D7).
mod apache;
mod hcl;
mod nginx;

use super::{Capability, Cursor, Prepared, Processor};
use crate::model::{Input, Name, Segment, SharedTrail, Trail};
use std::ops::Range;
use std::rc::Rc;

pub struct Blocks;

/// Past this many open Segments no more open (M6 spec §3, point 8).
pub(crate) const MAX_DEPTH: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Syntax {
    Nginx,
    Apache,
    Hcl,
}

#[derive(Debug, Clone, Copy)]
struct Params {
    syntax: Syntax,
    name_chars: usize,
}

impl Params {
    fn of(t: &toml::Table) -> anyhow::Result<Self> {
        let syntax = match t.get("syntax").and_then(|v| v.as_str()) {
            Some("nginx") => Syntax::Nginx,
            Some("apache") => Syntax::Apache,
            Some("hcl") => Syntax::Hcl,
            _ => anyhow::bail!(
                "blocks needs `syntax` = \"nginx\", \"apache\" or \"hcl\" in [params]"
            ),
        };
        let name_chars = t
            .get("name_chars")
            .and_then(|v| v.as_integer())
            .map_or(40, |n| n.max(0) as usize);
        Ok(Self { syntax, name_chars })
    }

    fn scan(&self, src: &[u8]) -> anyhow::Result<BlockIndex> {
        match self.syntax {
            Syntax::Nginx => nginx::scan(src, self.name_chars),
            Syntax::Apache => apache::scan(src, self.name_chars),
            Syntax::Hcl => hcl::scan(src, self.name_chars),
        }
    }
}

/// The kind of an indexed Segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Block,
    Key,
    Element,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Kind::Block => "block",
            Kind::Key => "key",
            Kind::Element => "element",
        }
    }
}

/// One piece of a Segment's name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Part {
    /// `len` bytes of the Source from `start`, as written, after a space unless first.
    Src { start: usize, len: u32 },
    /// `[N]`, 0-based, right after the holder's name: a list element's (M6 spec §7.8). It
    /// maps to no Source byte, so `--in-headings` never finds it.
    Index(u32),
}

impl Part {
    pub(crate) fn src(r: Range<usize>) -> Self {
        Part::Src {
            start: r.start,
            len: (r.end - r.start).min(u32::MAX as usize) as u32,
        }
    }
}

/// One Segment of the index: 40 bytes, with no allocation of its own.
#[derive(Debug, Clone)]
pub(crate) struct Entry {
    /// The Section's bytes, `start..end`; `end` is `usize::MAX` while the Segment is open.
    start: usize,
    end: usize,
    /// The bytes that name it, as (offset from `start`, length): a Match there is outside it.
    label: (u32, u32),
    /// Its name's pieces: `nparts` of `BlockIndex::parts` from `parts`.
    parts: u32,
    nparts: u16,
    parent: u32,
    kind: Kind,
    /// Takes part in containment, never shows in a Trail (a key that holds a list).
    hidden: bool,
}

const NONE: u32 = u32::MAX;

impl Entry {
    #[cfg(test)]
    fn range(&self) -> Range<usize> {
        self.start..self.end
    }

    fn label(&self) -> Range<usize> {
        let s = self.start + self.label.0 as usize;
        s..s + self.label.1 as usize
    }

    fn parent(&self) -> Option<usize> {
        (self.parent != NONE).then_some(self.parent as usize)
    }
}

/// A name piece read later in the file than its Segment's start (nginx's `server_name`,
/// Apache's `ServerName`): appended to the name and, as the label, outside the Segment.
#[derive(Debug, Clone)]
struct Late {
    entry: u32,
    start: usize,
    len: u32,
}

/// A prepared file: its Segments in file order (by start, parents before their children).
#[derive(Debug, Default)]
pub(crate) struct BlockIndex {
    entries: Vec<Entry>,
    parts: Vec<Part>,
    /// Sorted by entry.
    late: Vec<Late>,
}

impl BlockIndex {
    fn late_of(&self, id: usize) -> &[Late] {
        let id = id as u32;
        let a = self.late.partition_point(|l| l.entry < id);
        let b = self.late.partition_point(|l| l.entry <= id);
        &self.late[a..b]
    }

    /// Whether a Match at `byte` names Segment `id` (its label or a late name piece).
    fn names_at(&self, id: usize, byte: usize) -> bool {
        self.entries[id].label().contains(&byte)
            || self
                .late_of(id)
                .iter()
                .any(|l| l.start <= byte && byte < l.start + l.len as usize)
    }

    /// The name of Segment `id`, cut at `name_chars`, with its pieces.
    fn name(&self, src: &[u8], id: usize, name_chars: usize) -> (String, crate::text::Pieces) {
        let e = &self.entries[id];
        let parts = &self.parts[e.parts as usize..e.parts as usize + e.nparts as usize];
        let late = self.late_of(id).iter().map(|l| Part::Src {
            start: l.start,
            len: l.len,
        });
        let mut n = crate::text::CutName::new(name_chars);
        let mut first = true;
        for p in parts.iter().copied().chain(late) {
            if n.full() {
                break;
            }
            match p {
                Part::Src { start, len } => {
                    if !first {
                        n.push_str(" ");
                    }
                    let end = (start + len as usize).min(src.len());
                    n.push_lossy(src, start.min(end)..end);
                }
                Part::Index(k) => n.push_str(&format!("[{k}]")),
            }
            first = false;
        }
        n.finish()
    }

    /// The first Source byte of Segment `id`'s name, where an empty hit in it lands.
    fn name_anchor(&self, id: usize) -> usize {
        let e = &self.entries[id];
        match self.parts.get(e.parts as usize) {
            Some(Part::Src { start, .. }) if e.nparts > 0 => *start,
            _ => e.start,
        }
    }
}

/// What the scanners write the index with: the open Segments' stack gives each new Segment
/// its parent and stops opening past `MAX_DEPTH`.
pub(crate) struct Builder {
    ix: BlockIndex,
    open: Vec<u32>,
    /// Name pieces kept per Segment: each shows at least one character, so a name of more
    /// pieces than its cut can show keeps no more of them.
    max_parts: usize,
}

impl Builder {
    pub(crate) fn new(name_chars: usize) -> Self {
        Self {
            ix: BlockIndex::default(),
            open: Vec::new(),
            max_parts: if name_chars == 0 {
                u16::MAX as usize
            } else {
                (name_chars + 1).min(u16::MAX as usize)
            },
        }
    }

    /// Opens a Segment that starts at `start`: None past `MAX_DEPTH`, then the scanner keeps
    /// its structure without a Segment.
    pub(crate) fn open(
        &mut self,
        kind: Kind,
        start: usize,
        label: Range<usize>,
        parts: &[Part],
        hidden: bool,
    ) -> Option<u32> {
        if self.open.len() >= MAX_DEPTH || self.ix.entries.len() >= NONE as usize {
            return None;
        }
        debug_assert!(
            self.ix.entries.last().is_none_or(|e| e.start <= start),
            "Segments come in file order"
        );
        let id = self.ix.entries.len() as u32;
        let first = self.ix.parts.len() as u32;
        let take = parts.len().min(self.max_parts);
        self.ix.parts.extend_from_slice(&parts[..take]);
        let label = if label.is_empty() {
            (0, 0)
        } else {
            let from = label.start.max(start);
            (
                (from - start).min(u32::MAX as usize) as u32,
                (label.end.saturating_sub(from)).min(u32::MAX as usize) as u32,
            )
        };
        self.ix.entries.push(Entry {
            start,
            end: usize::MAX,
            label,
            parts: first,
            nparts: take as u16,
            parent: self.open.last().copied().unwrap_or(NONE),
            kind,
            hidden,
        });
        self.open.push(id);
        Some(id)
    }

    /// Closes Segment `id`, and every Segment opened inside it still open, at `end`.
    pub(crate) fn close(&mut self, id: u32, end: usize) {
        if let Some(k) = self.open.iter().rposition(|&o| o == id) {
            for o in self.open.drain(k..) {
                let e = &mut self.ix.entries[o as usize];
                e.end = end.max(e.start);
            }
        }
    }

    /// A Segment with nothing inside it.
    pub(crate) fn leaf(
        &mut self,
        kind: Kind,
        range: Range<usize>,
        label: Range<usize>,
        parts: &[Part],
    ) -> Option<u32> {
        let id = self.open(kind, range.start, label, parts, false)?;
        self.close(id, range.end);
        Some(id)
    }

    /// Appends `r`, read after Segment `id` opened, to its name, outside the Segment.
    pub(crate) fn late(&mut self, id: u32, r: Range<usize>) {
        self.ix.late.push(Late {
            entry: id,
            start: r.start,
            len: (r.end - r.start).min(u32::MAX as usize) as u32,
        });
    }

    /// The index, the Segments still open ending at `end` (the end of the file), each kept
    /// inside its parent.
    pub(crate) fn finish(mut self, end: usize) -> BlockIndex {
        for o in std::mem::take(&mut self.open) {
            let e = &mut self.ix.entries[o as usize];
            e.end = end.max(e.start);
        }
        for i in 0..self.ix.entries.len() {
            if let Some(p) = self.ix.entries[i].parent() {
                let pe = self.ix.entries[p].end;
                let e = &mut self.ix.entries[i];
                e.end = e.end.min(pe).max(e.start);
            }
        }
        self.ix.late.sort_by_key(|l| l.entry);
        self.ix.entries.shrink_to_fit();
        self.ix.parts.shrink_to_fit();
        self.ix
    }
}

/// A file prepared once: its index, which the cursors borrow.
pub(crate) struct BlockPrepared<'a> {
    src: &'a [u8],
    name_chars: usize,
    ix: BlockIndex,
}

impl Prepared for BlockPrepared<'_> {
    fn cursor(&self) -> Box<dyn Cursor + '_> {
        Box::new(BlockCursor::new(self.src, &self.ix, self.name_chars))
    }

    fn names(&self) -> anyhow::Result<Vec<Name>> {
        Ok(names(self.src, &self.ix, self.name_chars))
    }

    fn streams(&self) -> bool {
        true
    }
}

/// Every named Segment, hidden ones included (a key that holds a list), with its Trail: it
/// last, hidden ancestors left out. A Match in a name leaves out the named Segment alone
/// (`Name::trail_at` without labels): a name piece never lies in an ancestor's label.
fn names(src: &[u8], ix: &BlockIndex, name_chars: usize) -> Vec<Name> {
    let lines = crate::trail::LineIndex::new(src);
    let seg = |i: usize| segment(src, ix, i, name_chars, &lines);
    let mut out = Vec::new();
    for (i, e) in ix.entries.iter().enumerate() {
        if e.nparts == 0 && ix.late_of(i).is_empty() {
            continue;
        }
        let (text, pieces) = ix.name(src, i, name_chars);
        let mut chain = vec![i];
        let mut up = e.parent();
        while let Some(p) = up {
            if !ix.entries[p].hidden {
                chain.push(p);
            }
            up = ix.entries[p].parent();
        }
        chain.reverse();
        out.push(Name {
            text,
            pieces,
            trail: chain.into_iter().map(seg).collect(),
            labels: Vec::new(),
            anchor: ix.name_anchor(i),
        });
    }
    out
}

fn segment(
    src: &[u8],
    ix: &BlockIndex,
    i: usize,
    name_chars: usize,
    lines: &crate::trail::LineIndex,
) -> Segment {
    let e = &ix.entries[i];
    Segment {
        kind: e.kind.as_str().into(),
        name: ix.name(src, i, name_chars).0,
        line: lines.line_of(e.start),
        end_line: lines.line_of(e.end.saturating_sub(1).max(e.start)),
        level: None,
        signature: None,
    }
}

/// A Segment open at the cursor's offset, and its Segment once a Trail needed it.
struct Open {
    id: usize,
    seg: Option<Segment>,
}

/// The Trails of a prepared file in file order: the stack of the Segments open at the last
/// offset, as `trail::TrailCursor` keeps it over an Outline, over the index.
struct BlockCursor<'p> {
    src: &'p [u8],
    ix: &'p BlockIndex,
    name_chars: usize,
    /// The next Segment to open.
    next: usize,
    stack: Vec<Open>,
    last: usize,
    /// The line of byte `.0`, counted on from the last Segment start asked.
    lines: (usize, u64),
    ids: Vec<usize>,
    shared: Option<(Vec<usize>, SharedTrail)>,
    /// (Segment, offset, the first byte at or past it whose Trail ends with the Segment), as
    /// `open_from_with` found it.
    found: Vec<(usize, usize, Option<usize>)>,
}

impl<'p> BlockCursor<'p> {
    fn new(src: &'p [u8], ix: &'p BlockIndex, name_chars: usize) -> Self {
        Self {
            src,
            ix,
            name_chars,
            next: 0,
            stack: Vec::new(),
            last: 0,
            lines: (0, 1),
            ids: Vec::new(),
            shared: None,
            found: Vec::new(),
        }
    }

    /// Moves the stack to `byte`.
    fn reach(&mut self, byte: usize) {
        let es = &self.ix.entries;
        if byte < self.last {
            self.next = 0;
            self.stack.clear();
            self.found.clear();
        }
        self.last = byte;
        while self.next < es.len() && es[self.next].start <= byte {
            let start = es[self.next].start;
            while self.stack.last().is_some_and(|t| es[t.id].end <= start) {
                self.stack.pop();
            }
            self.stack.push(Open {
                id: self.next,
                seg: None,
            });
            self.next += 1;
        }
        while self.stack.last().is_some_and(|t| es[t.id].end <= byte) {
            self.stack.pop();
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
        let id = self.stack[k].id;
        let e = &self.ix.entries[id];
        let (start, end) = (e.start, e.end.min(self.src.len()));
        let line = self.line_of(start);
        let last = end.saturating_sub(1).max(start).min(self.src.len());
        let end_line =
            line + memchr::memchr_iter(b'\n', &self.src[start.min(last)..last]).count() as u64;
        let s = Segment {
            kind: e.kind.as_str().into(),
            name: self.ix.name(self.src, id, self.name_chars).0,
            line,
            end_line,
            level: None,
            signature: None,
        };
        self.stack[k].seg = Some(s.clone());
        s
    }

    /// The innermost shown Segment that holds `byte` outside its name: the one its Trail
    /// ends with.
    fn ends_with(&self, byte: usize) -> Option<usize> {
        self.stack
            .iter()
            .rev()
            .map(|o| o.id)
            .find(|&i| !self.ix.entries[i].hidden && !self.ix.names_at(i, byte))
    }

    /// The direct child of Segment `p` that holds or follows `from`, if any.
    fn child_from(&self, p: usize, from: usize) -> Option<usize> {
        let es = &self.ix.entries;
        let end = es[p].end;
        // The last Segment that starts at or before `from`, up to a child of `p`.
        let k = p + 1 + es[p + 1..].partition_point(|e| e.start <= from);
        let mut c = if k > p + 1 {
            let mut c = k - 1;
            while es[c].parent().is_some_and(|q| q != p) {
                c = es[c].parent().expect("checked");
            }
            (es[c].parent() == Some(p)).then_some(c)
        } else {
            None
        };
        if c.is_some_and(|c| es[c].end <= from) {
            c = self.sibling(p, c.expect("checked"));
        }
        match c {
            Some(c) => Some(c),
            None => (k < es.len() && es[k].start < end && es[k].parent() == Some(p)).then_some(k),
        }
    }

    /// The next direct child of `p` after its child `c`.
    fn sibling(&self, p: usize, c: usize) -> Option<usize> {
        let es = &self.ix.entries;
        let k = c + 1 + es[c + 1..].partition_point(|e| e.start < es[c].end);
        (k < es.len() && es[k].start < es[p].end && es[k].parent() == Some(p)).then_some(k)
    }

    /// The first byte at or past `from` whose Trail may end with Segment `p`, of those `first`
    /// finds a Match at: the bytes of `p` that no shown child holds outside its label. A hidden
    /// child's bytes count as `p`'s, which can only keep more Matches.
    fn find(
        &self,
        p: usize,
        from: usize,
        first: &mut dyn FnMut(Range<usize>) -> Option<usize>,
    ) -> Option<usize> {
        let es = &self.ix.entries;
        let end = es[p].end.min(self.src.len());
        let mut at = from.max(es[p].start);
        let mut child = self.child_from(p, at);
        let mut gap = |a: usize, b: usize| -> Option<usize> {
            let b = b.min(end);
            if a < b { first(a..b) } else { None }
        };
        while at < end {
            let Some(c) = child else {
                return gap(at, end);
            };
            let e = &es[c];
            if !e.hidden {
                let label = e.label();
                let covered = if label.is_empty() {
                    [e.start..e.end, 0..0]
                } else {
                    [e.start..label.start, label.end..e.end]
                };
                for q in covered {
                    if q.is_empty() || q.end <= at {
                        continue;
                    }
                    if q.start > at
                        && let Some(b) = gap(at, q.start)
                    {
                        return Some(b);
                    }
                    at = at.max(q.end);
                }
            }
            child = self.sibling(p, c);
        }
        None
    }
}

impl Cursor for BlockCursor<'_> {
    fn trail_at(&mut self, byte: usize) -> anyhow::Result<SharedTrail> {
        self.reach(byte);
        let mut ids = std::mem::take(&mut self.ids);
        ids.clear();
        let ix = self.ix;
        ids.extend(
            self.stack
                .iter()
                .enumerate()
                .filter(|(_, o)| !ix.entries[o.id].hidden && !ix.names_at(o.id, byte))
                .map(|(k, _)| k),
        );
        let same = self.shared.as_ref().is_some_and(|(prev, _)| {
            prev.len() == ids.len()
                && prev
                    .iter()
                    .zip(&ids)
                    .all(|(&a, &k)| self.stack.get(k).is_some_and(|o| o.id == a))
        });
        if same {
            self.ids = ids;
            return Ok(self.shared.as_ref().expect("checked").1.clone());
        }
        let trail: Vec<Segment> = ids.iter().map(|&k| self.segment_at(k)).collect();
        let key = ids.iter().map(|&k| self.stack[k].id).collect();
        let t: SharedTrail = Rc::from(trail);
        self.shared = Some((key, t.clone()));
        self.ids = ids;
        Ok(t)
    }

    fn open_from(&mut self) -> Option<usize> {
        let es = &self.ix.entries;
        Some(
            self.stack
                .iter()
                .find(|o| !es[o.id].hidden)
                .map_or(self.last, |o| es[o.id].start),
        )
    }

    fn open_from_with(
        &mut self,
        first: &mut dyn FnMut(Range<usize>) -> Option<usize>,
    ) -> Option<usize> {
        let last = self.last;
        let stack: Vec<usize> = self.stack.iter().map(|o| o.id).collect();
        self.found.retain(|f| stack.contains(&f.0));
        let at_last = self.ends_with(last);
        for &i in &stack {
            let e = &self.ix.entries[i];
            if e.hidden {
                continue;
            }
            if Some(i) == at_last {
                return Some(e.start);
            }
            let known = self
                .found
                .iter()
                .find(|f| f.0 == i && f.1 <= last && f.2.is_none_or(|b| last <= b))
                .map(|f| f.2);
            let next = match known {
                Some(next) => next,
                None => {
                    let next = self.find(i, last, first);
                    self.found.retain(|f| f.0 != i);
                    self.found.push((i, last, next));
                    next
                }
            };
            if next.is_some() {
                return Some(e.start);
            }
        }
        Some(last)
    }
}

impl Processor for Blocks {
    fn trails(
        &self,
        input: &Input,
        params: &toml::Table,
        anchors: &[usize],
    ) -> anyhow::Result<Vec<Trail>> {
        let p = Params::of(params)?;
        let ix = p.scan(input.bytes)?;
        let mut c = BlockCursor::new(input.bytes, &ix, p.name_chars);
        let mut order: Vec<usize> = (0..anchors.len()).collect();
        order.sort_by_key(|&i| anchors[i]);
        let mut out = vec![Vec::new(); anchors.len()];
        for i in order {
            out[i] = c.trail_at(anchors[i])?.to_vec();
        }
        Ok(out)
    }

    fn names(&self, input: &Input, params: &toml::Table) -> anyhow::Result<Vec<Name>> {
        let p = Params::of(params)?;
        let ix = p.scan(input.bytes)?;
        Ok(names(input.bytes, &ix, p.name_chars))
    }

    fn prepare<'a>(
        &'a self,
        input: Input<'a>,
        params: &'a toml::Table,
    ) -> anyhow::Result<Box<dyn Prepared + 'a>> {
        let p = Params::of(params)?;
        let ix = p.scan(input.bytes)?;
        Ok(Box::new(BlockPrepared {
            src: input.bytes,
            name_chars: p.name_chars,
            ix,
        }))
    }

    fn capability(&self, params: &toml::Table) -> Capability {
        if Params::of(params).is_ok() {
            Capability::Native
        } else {
            Capability::Model
        }
    }
}

/// ASCII white space, a lone CR included (M6 spec §3, point 7: no line break).
pub(crate) fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n' | b'\x0b' | b'\x0c')
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::path::Path;

    pub(crate) fn params(syntax: &str) -> toml::Table {
        toml::from_str(&format!("syntax = '{syntax}'\nname_chars = 40")).unwrap()
    }

    fn input(src: &[u8]) -> Input<'_> {
        Input {
            bytes: src,
            path: Path::new("x"),
            format: "x",
            disk: None,
        }
    }

    /// The Trail of `byte` read from the index alone: every shown Segment that holds it
    /// outside its name.
    fn model(src: &[u8], ix: &BlockIndex, byte: usize) -> Vec<String> {
        ix.entries
            .iter()
            .enumerate()
            .filter(|(i, e)| e.range().contains(&byte) && !e.hidden && !ix.names_at(*i, byte))
            .map(|(i, _)| ix.name(src, i, 40).0)
            .collect()
    }

    /// The cursor's Trails equal the model's at every byte, forward and back, and its
    /// `open_from` never lets go of a byte a later Trail ends with (M6 plan, Task 3.1).
    pub(crate) fn cursor_agrees(syntax: &str, src: &[u8]) {
        let params = params(syntax);
        let prepared = Blocks.prepare(input(src), &params).unwrap();
        assert!(prepared.streams());
        assert_eq!(Blocks.capability(&params), Capability::Native);
        let ix = Params::of(&params).unwrap().scan(src).unwrap();
        let mut ends: Vec<Option<usize>> = Vec::new();
        for b in 0..src.len() {
            let mut c = BlockCursor::new(src, &ix, 40);
            c.reach(b);
            ends.push(c.ends_with(b).map(|i| ix.entries[i].start));
        }
        let mut c = prepared.cursor();
        for b in 0..src.len() {
            let t = c.trail_at(b).unwrap();
            let names: Vec<String> = t.iter().map(|s| s.name.clone()).collect();
            assert_eq!(names, model(src, &ix, b), "{syntax} byte {b}");
            let open = c.open_from().unwrap();
            let with = c.open_from_with(&mut |r| Some(r.start)).unwrap();
            for &start in ends[b..].iter().flatten() {
                assert!(
                    open <= start,
                    "{syntax} byte {b}: open_from {open} > {start}"
                );
                assert!(
                    with <= start,
                    "{syntax} byte {b}: open_from_with {with} > {start}"
                );
            }
        }
        for b in (0..src.len()).rev().step_by(7) {
            let names: Vec<String> = c
                .trail_at(b)
                .unwrap()
                .iter()
                .map(|s| s.name.clone())
                .collect();
            assert_eq!(names, model(src, &ix, b), "{syntax} back to byte {b}");
        }
        let anchors: Vec<usize> = (0..src.len()).rev().collect();
        let trails = Blocks.trails(&input(src), &params, &anchors).unwrap();
        for (t, &a) in trails.iter().zip(&anchors) {
            let names: Vec<String> = t.iter().map(|s| s.name.clone()).collect();
            assert_eq!(names, model(src, &ix, a), "{syntax} model byte {a}");
        }
    }

    #[test]
    fn a_syntax_is_required() {
        let t = toml::Table::new();
        assert!(Params::of(&t).is_err());
        assert_eq!(Blocks.capability(&t), Capability::Model);
        assert!(Blocks.prepare(input(b"x"), &t).is_err());
    }

    #[test]
    fn server_cursors_agree_with_the_model() {
        cursor_agrees(
            "nginx",
            b"http {\n server {\n  location /api {\n   proxy_pass http://needle:8080;\n  }\n  server_name example.com;\n }\n}\n",
        );
        cursor_agrees(
            "apache",
            b"<VirtualHost *:443>\n <Directory /var/www>\n  Require ip 10.0.0.0/8 # needle\n </Directory>\n ServerName example.com\n</VirtualHost>\n",
        );
    }

    #[test]
    fn hcl_cursors_agree_with_the_model() {
        cursor_agrees(
            "hcl",
            b"resource \"aws_instance\" \"web\" {\n tags = {\n  Name = \"needle-web\"\n }\n ingress = [\n  { port = 80 },\n  { name = \"needle\" }\n ]\n script = <<-EOT\n  echo needle ${not_a_block}\n  EOT\n}\nl = [[1, 2], [3]]\n",
        );
    }

    #[test]
    fn depth_stops_opening_past_256() {
        let src = format!("{}k needle;{}", "x {".repeat(300), "}".repeat(300));
        let ix = Params::of(&params("nginx"))
            .unwrap()
            .scan(src.as_bytes())
            .unwrap();
        assert_eq!(ix.entries.len(), MAX_DEPTH);
        let mut c = BlockCursor::new(src.as_bytes(), &ix, 40);
        let at = src.find("needle").unwrap();
        assert_eq!(c.trail_at(at).unwrap().len(), MAX_DEPTH);
    }

    #[test]
    fn late_names_are_outside_their_segment() {
        let src = b"server {\n server_name a.example b;\n}\n";
        let ix = Params::of(&params("nginx")).unwrap().scan(src).unwrap();
        assert_eq!(ix.name(src, 0, 40).0, "server a.example");
        let at = src.windows(9).position(|w| w == b"a.example").unwrap();
        let mut c = BlockCursor::new(src, &ix, 40);
        let names: Vec<String> = c
            .trail_at(at)
            .unwrap()
            .iter()
            .map(|s| s.name.clone())
            .collect();
        assert_eq!(names, ["server_name"]);
        let n = names_for(src);
        let server = n.iter().find(|n| n.text == "server a.example").unwrap();
        assert_eq!(server.pieces, vec![(0..6, 0..6), (7..16, at..at + 9)]);
    }

    fn names_for(src: &[u8]) -> Vec<Name> {
        Blocks.names(&input(src), &params("nginx")).unwrap()
    }
}
