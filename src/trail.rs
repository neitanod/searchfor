//! Outline of a Source and the computation of Trails from it (design notes 7 and 8).
use crate::model::{Name, Segment, Trail};
use std::ops::Range;

/// 1-based line lookup by byte offset.
pub struct LineIndex {
    starts: Vec<usize>,
}

impl LineIndex {
    pub fn new(content: &[u8]) -> Self {
        let mut starts = vec![0];
        starts.extend(memchr::memchr_iter(b'\n', content).map(|i| i + 1));
        Self { starts }
    }

    /// Line containing `byte`. A byte past the end belongs to the last line.
    pub fn line_of(&self, byte: usize) -> u64 {
        match self.starts.binary_search(&byte) {
            Ok(i) => i as u64 + 1,
            Err(i) => i as u64,
        }
    }

    pub fn line_start(&self, line: u64) -> Option<usize> {
        self.starts.get((line as usize).checked_sub(1)?).copied()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineSegment {
    /// `line`/`end_line` left at 0 are filled by `Outline::build` from `range`.
    pub segment: Segment,
    /// Bytes covered: the Section.
    pub range: Range<usize>,
    /// Bytes that name the Segment (a heading line, an item's first line). May be empty.
    pub label: Range<usize>,
    /// Pieces of the name's text in the Source, for `--in-headings`, in the format of
    /// `Name::pieces`. None: the Segment is not searchable by name.
    pub name: Option<Vec<(Range<usize>, Range<usize>)>>,
    /// `Name::anchor` for this Segment (meaningful only when `name` is Some).
    pub name_anchor: usize,
    pub parent: Option<usize>,
    /// Hidden Segments take part in containment but never appear in a Trail.
    pub hidden: bool,
}

impl OutlineSegment {
    pub fn new(
        kind: &str,
        name: impl Into<String>,
        range: Range<usize>,
        label: Range<usize>,
    ) -> Self {
        Self {
            segment: Segment {
                kind: kind.into(),
                name: name.into(),
                line: 0,
                end_line: 0,
                level: None,
                signature: None,
            },
            range,
            label,
            name: None,
            name_anchor: 0,
            parent: None,
            hidden: false,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Outline {
    /// Sorted by `range.start` ascending, then `range.end` descending; parents precede children.
    pub segments: Vec<OutlineSegment>,
}

impl Outline {
    /// Sorts the Segments, assigns parents by containment and fills missing line numbers.
    /// A Segment that starts inside another and ends after it is clamped to its parent's end,
    /// so a Processor bug can never break the tree.
    pub fn build(mut segments: Vec<OutlineSegment>, content: &[u8]) -> Self {
        // An Outline lives for the whole search: without the room its Vec grew into (code
        // review r1, Fable #3).
        segments.shrink_to_fit();
        segments.sort_by(|a, b| {
            a.range
                .start
                .cmp(&b.range.start)
                .then(b.range.end.cmp(&a.range.end))
        });
        let mut stack: Vec<usize> = Vec::new();
        for i in 0..segments.len() {
            let start = segments[i].range.start;
            while let Some(&top) = stack.last() {
                if start < segments[top].range.end {
                    break;
                }
                stack.pop();
            }
            if let Some(&top) = stack.last() {
                let end = segments[top].range.end;
                if segments[i].range.end > end {
                    segments[i].range.end = end;
                }
            }
            segments[i].parent = stack.last().copied();
            stack.push(i);
        }
        fill_lines(&mut segments, content);
        Self { segments }
    }

    /// Index of the innermost Segment whose range contains `byte`.
    fn innermost(&self, byte: usize) -> Option<usize> {
        self.segments
            .iter()
            .rposition(|s| s.range.start <= byte && byte < s.range.end)
    }

    fn chain(&self, i: usize) -> Vec<usize> {
        let mut chain = Vec::new();
        let mut cur = Some(i);
        while let Some(j) = cur {
            chain.push(j);
            cur = self.segments[j].parent;
        }
        chain.reverse();
        chain
    }
}

/// Fills the `line` and `end_line` left at 0 from the ranges, counting line breaks only up to
/// the last byte asked: memory grows with the Segments, never with the lines of the file (M3
/// review r1, Codex #2).
fn fill_lines(segments: &mut [OutlineSegment], content: &[u8]) {
    // The Segments are sorted by start: their first lines in one pass. Their ends are not:
    // (byte, Segment index) of each, sorted, 16 bytes a Segment (code review r1, Fable #3).
    let count = |at: &mut usize, line: &mut u64, byte: usize| {
        let to = byte.min(content.len());
        if to > *at {
            *line += memchr::memchr_iter(b'\n', &content[*at..to]).count() as u64;
            *at = to;
        }
        *line
    };
    let mut ends: Vec<(usize, usize)> = Vec::new();
    let (mut at, mut line) = (0usize, 1u64);
    for (i, s) in segments.iter_mut().enumerate() {
        if s.segment.line == 0 {
            let r = &s.range;
            ends.push((r.end.saturating_sub(1).max(r.start), i));
            s.segment.line = count(&mut at, &mut line, r.start);
        }
    }
    ends.sort_unstable();
    let (mut at, mut line) = (0usize, 1u64);
    for (byte, i) in ends {
        segments[i].segment.end_line = count(&mut at, &mut line, byte);
    }
}

/// Trails of byte offsets asked in non-decreasing order, from a stack of the Segments open at
/// the last offset instead of a scan of the whole Outline per offset. An offset smaller than the
/// last one starts over.
pub struct TrailCursor<'o> {
    outline: &'o Outline,
    /// Index of the next Segment to open.
    next: usize,
    /// The Segments containing the last offset, outermost first (each the parent of the next).
    stack: Vec<usize>,
    last: usize,
    /// The bytes whose Trails end with each Segment (`Owned`), made the first time
    /// `open_from` asks.
    owned: Option<Owned>,
    /// Of the Segments open at the last offset asked: (Segment, offset, first byte at or past
    /// the offset whose Trail ends with it, if any), as `open_from_with` found it.
    found: Vec<(usize, usize, Option<usize>)>,
}

/// The bytes whose Trail ends with each Segment (code review r2, Fable #2). A Trail ends with
/// the innermost shown Segment that holds the byte outside its label (`TrailCursor::indices`):
/// a Segment's bytes are its own but for its label and those of its shown children past their
/// labels; a hidden child's bytes are its parent's. The labels of grandchildren count as their
/// parents' bytes, which can only make a Segment own more bytes than it does: a stream lets go
/// of fewer Matches, never of one it needs.
struct Owned {
    /// By Segment, the first of its children's bytes in `covered`; one more, the end.
    first: Vec<usize>,
    /// The bytes of each Segment's shown children, merged, in order.
    covered: Vec<Range<usize>>,
    /// By Segment, one past its last own byte, 0 when it owns none.
    ends: Vec<usize>,
}

impl Owned {
    fn new(outline: &Outline) -> Self {
        let segs = &outline.segments;
        // The nearest ancestor that is not hidden: the parents precede their children.
        let mut shown: Vec<Option<usize>> = Vec::with_capacity(segs.len());
        for s in segs {
            let up = s
                .parent
                .and_then(|p| if segs[p].hidden { shown[p] } else { Some(p) });
            shown.push(up);
        }
        let mut children: Vec<(usize, usize, usize)> = segs
            .iter()
            .enumerate()
            .filter(|(_, c)| !c.hidden)
            .filter_map(|(i, c)| {
                let from = if c.label.is_empty() {
                    c.range.start
                } else {
                    c.label.end.max(c.range.start)
                };
                shown[i].map(|p| (p, from, c.range.end))
            })
            .filter(|&(_, a, b)| a < b)
            .collect();
        drop(shown);
        children.sort_unstable_by_key(|&(p, a, _)| (p, a));
        let mut first = Vec::with_capacity(segs.len() + 1);
        let mut covered: Vec<Range<usize>> = Vec::new();
        let mut at = 0;
        for p in 0..segs.len() {
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
        owned.ends = (0..segs.len())
            .map(|p| owned.last_own(outline, p).map_or(0, |b| b + 1))
            .collect();
        owned
    }

    /// The bytes of Segment `p`'s children.
    fn children(&self, p: usize) -> &[Range<usize>] {
        &self.covered[self.first[p]..self.first[p + 1]]
    }

    /// The last byte Segment `p` owns.
    fn last_own(&self, outline: &Outline, p: usize) -> Option<usize> {
        let s = &outline.segments[p];
        let covered = self.children(p);
        let mut end = s.range.end;
        let mut k = covered.len();
        while end > s.range.start {
            let b = end - 1;
            if s.label.contains(&b) {
                end = s.label.start;
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

    /// The ranges of bytes at or past `from` that Segment `p` owns, in order, until `f` gives a
    /// byte: what it gave.
    fn find(
        &self,
        outline: &Outline,
        p: usize,
        from: usize,
        f: &mut dyn FnMut(Range<usize>) -> Option<usize>,
    ) -> Option<usize> {
        let s = &outline.segments[p];
        let label = s.label.clone();
        // A range without the label's bytes.
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
        let mut at = from.max(s.range.start);
        let mut k = covered.partition_point(|c| c.end <= at);
        while at < s.range.end {
            match covered.get(k) {
                Some(c) if c.start <= at => {
                    at = c.end;
                    k += 1;
                }
                Some(c) => {
                    let to = c.start.min(s.range.end);
                    if let Some(b) = each(at..to) {
                        return Some(b);
                    }
                    at = to;
                }
                None => return each(at..s.range.end),
            }
        }
        None
    }
}

impl<'o> TrailCursor<'o> {
    pub fn new(outline: &'o Outline) -> Self {
        Self {
            outline,
            next: 0,
            stack: Vec::new(),
            last: 0,
            owned: None,
            found: Vec::new(),
        }
    }

    /// The indices into `outline.segments` of the Trail of `byte`, as `trail_for` gives it.
    pub fn indices(&mut self, byte: usize, out: &mut Vec<usize>) {
        let segs = &self.outline.segments;
        if byte < self.last {
            self.next = 0;
            self.stack.clear();
        }
        self.last = byte;
        while self.next < segs.len() && segs[self.next].range.start <= byte {
            let start = segs[self.next].range.start;
            while self
                .stack
                .last()
                .is_some_and(|&t| segs[t].range.end <= start)
            {
                self.stack.pop();
            }
            self.stack.push(self.next);
            self.next += 1;
        }
        while self
            .stack
            .last()
            .is_some_and(|&t| segs[t].range.end <= byte)
        {
            self.stack.pop();
        }
        out.clear();
        out.extend(
            self.stack
                .iter()
                .copied()
                .filter(|&i| !segs[i].hidden && !segs[i].label.contains(&byte)),
        );
    }

    /// Where the outermost Segment open at the last offset starts that the Trail of that
    /// offset or of a later one may still end with, or that offset when none may
    /// (`Cursor::open_from`): a hidden Segment never ends a Trail, and one whose bytes past the
    /// offset all have a Trail that ends with a Segment inside it never will (code review r2,
    /// Fable #2).
    pub fn open_from(&mut self) -> usize {
        let outline = self.outline;
        let owned = self.owned.get_or_insert_with(|| Owned::new(outline));
        let last = self.last;
        self.stack
            .iter()
            .find(|&&i| !outline.segments[i].hidden && owned.ends[i] > last)
            .map_or(last, |&i| outline.segments[i].range.start)
    }

    /// `open_from`, of the Segments that own a byte at or past the last offset where a later
    /// Trail is asked: `first` gives the first such byte of a range of bytes, if any (a Match
    /// the search will find there). What it found for each open Segment is kept until the
    /// offset passes it, so each byte is asked about once or twice.
    pub fn open_from_with(
        &mut self,
        first: &mut dyn FnMut(Range<usize>) -> Option<usize>,
    ) -> usize {
        let outline = self.outline;
        let owned = self.owned.get_or_insert_with(|| Owned::new(outline));
        let last = self.last;
        let stack = &self.stack;
        self.found.retain(|f| stack.contains(&f.0));
        // The Trail of the last offset ends with the innermost shown Segment that owns it.
        let at_last = stack.iter().rev().copied().find(|&i| {
            let s = &outline.segments[i];
            !s.hidden && !s.label.contains(&last)
        });
        for &i in stack {
            if outline.segments[i].hidden || owned.ends[i] <= last {
                continue;
            }
            if Some(i) == at_last {
                return outline.segments[i].range.start;
            }
            let known = self
                .found
                .iter()
                .find(|f| f.0 == i && f.1 <= last && f.2.is_none_or(|b| last <= b))
                .map(|f| f.2);
            let next = match known {
                Some(next) => next,
                None => {
                    let next = owned.find(outline, i, last, first);
                    self.found.retain(|f| f.0 != i);
                    self.found.push((i, last, next));
                    next
                }
            };
            if next.is_some() {
                return outline.segments[i].range.start;
            }
        }
        last
    }

    /// The Trail of `byte`.
    pub fn trail(&mut self, byte: usize) -> Trail {
        let mut ix = Vec::new();
        self.indices(byte, &mut ix);
        self.segments(&ix)
    }

    /// The Segments at `indices`.
    pub fn segments(&self, indices: &[usize]) -> Trail {
        indices
            .iter()
            .map(|&i| self.outline.segments[i].segment.clone())
            .collect()
    }
}

/// The Trail of the Match whose first byte is `byte` (design note 7).
pub fn trail_for(outline: &Outline, byte: usize) -> Trail {
    let Some(inner) = outline.innermost(byte) else {
        return Vec::new();
    };
    outline
        .chain(inner)
        .into_iter()
        .map(|i| &outline.segments[i])
        .filter(|s| !s.hidden && !s.label.contains(&byte))
        .map(|s| s.segment.clone())
        .collect()
}

/// Every Segment that has a name range, hidden ones included (a key that holds an array, D35),
/// with its own Trail (it included, last); hidden ancestors are left out of its Trail.
pub fn names_of(outline: &Outline) -> Vec<Name> {
    outline
        .segments
        .iter()
        .enumerate()
        .filter(|(_, s)| s.name.is_some())
        .map(|(i, s)| Name {
            text: s.segment.name.clone(),
            pieces: s.name.clone().unwrap(),
            anchor: s.name_anchor,
            trail: outline
                .chain(i)
                .into_iter()
                .filter(|&j| !outline.segments[j].hidden || j == i)
                .map(|j| outline.segments[j].segment.clone())
                .collect(),
            labels: outline
                .chain(i)
                .into_iter()
                .filter(|&j| !outline.segments[j].hidden || j == i)
                .map(|j| outline.segments[j].label.clone())
                .collect(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "# A\nalpha\n## B\nbeta\n# C\ngamma\n"; // 30 bytes
    // line starts:      0    4      10    15    20   24

    fn heading(name: &str, range: Range<usize>, label: Range<usize>) -> OutlineSegment {
        let mut s = OutlineSegment::new("heading", name, range, label.clone());
        s.name = Some(vec![(0..1, label.end - 2..label.end - 1)]); // the one-letter name before "\n"
        s
    }

    fn outline() -> Outline {
        Outline::build(
            vec![
                heading("C", 20..30, 20..24),
                heading("A", 0..20, 0..4),
                heading("B", 10..20, 10..15),
            ],
            DOC.as_bytes(),
        )
    }

    fn names(t: &Trail) -> Vec<&str> {
        t.iter().map(|s| s.name.as_str()).collect()
    }

    #[test]
    fn build_sorts_assigns_parents_and_lines() {
        let o = outline();
        let n: Vec<_> = o.segments.iter().map(|s| s.segment.name.as_str()).collect();
        assert_eq!(n, ["A", "B", "C"]);
        assert_eq!(o.segments[1].parent, Some(0));
        assert_eq!(o.segments[2].parent, None);
        assert_eq!(
            (o.segments[1].segment.line, o.segments[1].segment.end_line),
            (3, 4)
        );
    }

    #[test]
    fn trail_walks_parents() {
        assert_eq!(names(&trail_for(&outline(), 16)), ["A", "B"]); // "beta"
    }

    #[test]
    fn a_match_on_a_label_leaves_that_segment_out() {
        assert_eq!(names(&trail_for(&outline(), 13)), ["A"]); // inside "## B"
    }

    #[test]
    fn outside_every_segment_is_an_empty_trail() {
        let o = Outline::build(
            vec![OutlineSegment::new("heading", "A", 5..10, 5..6)],
            b"xxxxxyyyyyzzzzz",
        );
        assert!(trail_for(&o, 2).is_empty());
    }

    #[test]
    fn hidden_segments_contain_but_do_not_show() {
        let doc = b"| Plan | Price |\n|---|---|\n| Pro | 10 |\n";
        let mut table = OutlineSegment::new("table", "table", 0..doc.len(), 0..0);
        table.hidden = true;
        let mut cell = OutlineSegment::new("column", "Price", 34..38, 0..0);
        cell.segment.line = 1;
        cell.segment.end_line = 3;
        let o = Outline::build(vec![cell, table], doc);
        let t = trail_for(&o, 35);
        assert_eq!(t.len(), 1);
        assert_eq!(
            (
                t[0].kind.as_str(),
                t[0].name.as_str(),
                t[0].line,
                t[0].end_line
            ),
            ("column", "Price", 1, 3)
        );
    }

    #[test]
    fn the_cursor_gives_the_trails_of_trail_for() {
        let o = outline();
        let mut c = TrailCursor::new(&o);
        for byte in (0..=DOC.len()).chain([16, 2, 25, 13]) {
            assert_eq!(c.trail(byte), trail_for(&o, byte), "byte {byte}");
        }
        // Nested and empty Segments, and a hidden one.
        let mut segs = vec![
            OutlineSegment::new("item", "i", 0..10, 0..2),
            OutlineSegment::new("heading", "h", 2..8, 2..4),
            OutlineSegment::new("code", "e", 5..5, 5..5),
            OutlineSegment::new("heading", "k", 12..20, 12..13),
        ];
        segs[3].hidden = true;
        let o = Outline::build(segs, &[b'x'; 20]);
        let mut c = TrailCursor::new(&o);
        for byte in 0..=20 {
            assert_eq!(c.trail(byte), trail_for(&o, byte), "byte {byte}");
        }
    }

    #[test]
    fn crossing_ranges_are_clamped_to_the_parent() {
        let o = Outline::build(
            vec![
                OutlineSegment::new("item", "i", 0..10, 0..2),
                OutlineSegment::new("heading", "h", 2..20, 2..4),
            ],
            &[b'x'; 20],
        );
        assert_eq!(o.segments[1].range, 2..10);
        assert_eq!(o.segments[1].parent, Some(0));
    }

    #[test]
    fn names_carry_their_own_trail() {
        let n = names_of(&outline());
        let b = n.iter().find(|x| x.text == "B").unwrap();
        assert_eq!(&DOC[b.pieces[0].1.clone()], "B");
        assert_eq!(b.source_ranges(0..1), vec![13..14]);
        assert_eq!(names(&b.trail), ["A", "B"]);
    }

    #[test]
    fn hidden_segments_are_names_only_when_named() {
        // A Markdown table: hidden and without a name, so no Name (M1 unchanged).
        let doc = b"| Plan | Price |\n|---|---|\n| Pro | 10 |\n";
        let mut table = OutlineSegment::new("table", "table", 0..doc.len(), 0..0);
        table.hidden = true;
        let o = Outline::build(vec![table], doc);
        assert!(names_of(&o).is_empty());
        // A key that holds an array: hidden but named, so a Name; a named Segment under it
        // leaves it out of its Trail (D35).
        let mut ports = heading("P", 0..20, 0..4);
        ports.hidden = true;
        let o = Outline::build(vec![ports, heading("B", 10..20, 10..15)], DOC.as_bytes());
        let n = names_of(&o);
        let p = n
            .iter()
            .find(|x| x.text == "P")
            .expect("a named hidden Segment is a Name");
        assert_eq!(names(&p.trail), ["P"]);
        assert_eq!((p.labels.len(), &p.labels[0]), (1, &(0..4)));
        let b = n.iter().find(|x| x.text == "B").unwrap();
        assert_eq!(names(&b.trail), ["B"]);
        assert_eq!((b.labels.len(), &b.labels[0]), (1, &(10..15)));
    }

    #[test]
    fn line_index() {
        let li = LineIndex::new(b"ab\ncd\n");
        assert_eq!(
            (li.line_of(0), li.line_of(2), li.line_of(3), li.line_of(6)),
            (1, 1, 2, 3)
        );
        assert_eq!(li.line_start(2), Some(3));
    }

    /// Code review r2, Fable #2: `open_from` starts at the outermost open Segment that a Trail
    /// at or past the offset may still end with: past A's own bytes (its label, `alpha` and
    /// B's label), every byte of A is B's.
    #[test]
    fn open_from_skips_the_segments_no_later_trail_ends_with() {
        let o = outline();
        let mut c = TrailCursor::new(&o);
        let mut at = |byte: usize| {
            c.trail(byte);
            c.open_from()
        };
        assert_eq!(at(5), 0);
        assert_eq!(at(11), 0, "B's label is A's");
        assert_eq!(at(15), 10);
        assert_eq!(at(19), 10);
        assert_eq!(at(25), 20);
        // A hidden B ends no Trail: its bytes end A's.
        let mut segments = o.segments.clone();
        segments[1].hidden = true;
        let hidden = Outline { segments };
        let mut c = TrailCursor::new(&hidden);
        c.trail(16);
        assert_eq!(c.open_from(), 0);
        assert_eq!(names(&c.trail(16)), ["A"]);
    }

    /// Code review r2, Fable #2: a Segment that owns bytes ahead only in its children's labels
    /// is left out when no Match is found there, and kept when one is.
    #[test]
    fn open_from_with_asks_where_the_matches_ahead_are() {
        let doc = "# A\n## B\nx\n## C\ny\n"; // A 0..20, B 4..11, C 11..20
        let o = Outline::build(
            vec![
                heading("A", 0..20, 0..4),
                heading("B", 4..11, 4..9),
                heading("C", 11..20, 11..16),
            ],
            doc.as_bytes(),
        );
        let mut c = TrailCursor::new(&o);
        c.trail(9);
        assert_eq!(c.open_from(), 0, "C's label is A's");
        let mut asked = Vec::new();
        assert_eq!(
            c.open_from_with(&mut |r| {
                asked.push((r.start, r.end));
                None
            }),
            4
        );
        assert_eq!(asked, vec![(11, 16)]);
        // Known until the offset passes it: asked again, nothing is searched.
        assert_eq!(c.open_from_with(&mut |_| unreachable!()), 4);
        // A Match in C's label: A may still be a Section.
        let mut c = TrailCursor::new(&o);
        c.trail(9);
        assert_eq!(c.open_from_with(&mut |r| Some(r.start + 1)), 0);
        c.trail(10);
        assert_eq!(c.open_from_with(&mut |_| unreachable!()), 0);
    }
}
