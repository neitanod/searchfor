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
        segments.sort_by(|a, b| {
            a.range
                .start
                .cmp(&b.range.start)
                .then(b.range.end.cmp(&a.range.end))
        });
        let lines = LineIndex::new(content);
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
            let r = segments[i].range.clone();
            let seg = &mut segments[i].segment;
            if seg.line == 0 {
                seg.line = lines.line_of(r.start);
                seg.end_line = lines.line_of(r.end.saturating_sub(1).max(r.start));
            }
            stack.push(i);
        }
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

/// Every visible Segment that has a name range, with its own Trail (it included, last).
pub fn names_of(outline: &Outline) -> Vec<Name> {
    outline
        .segments
        .iter()
        .enumerate()
        .filter(|(_, s)| !s.hidden && s.name.is_some())
        .map(|(i, s)| Name {
            text: s.segment.name.clone(),
            pieces: s.name.clone().unwrap(),
            anchor: s.name_anchor,
            trail: outline
                .chain(i)
                .into_iter()
                .map(|j| &outline.segments[j])
                .filter(|x| !x.hidden)
                .map(|x| x.segment.clone())
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
    fn line_index() {
        let li = LineIndex::new(b"ab\ncd\n");
        assert_eq!(
            (li.line_of(0), li.line_of(2), li.line_of(3), li.line_of(6)),
            (1, 1, 2, 3)
        );
        assert_eq!(li.line_start(2), Some(3));
    }
}
