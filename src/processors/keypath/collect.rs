//! Builds an Outline from the open and close events of one walk over a file, in file order
//! (design note 6). For Trails it keeps only the Segments open at some anchor, so memory grows
//! with depth × anchors, not with the file; for `--in-headings` it keeps every Segment.
use crate::trail::{Outline, OutlineSegment};

pub struct Collector<'a> {
    /// Sorted ascending.
    anchors: &'a [usize],
    all: bool,
    /// First anchor not yet passed.
    next: usize,
    /// Open Segments, innermost last, with whether an anchor fell inside. A kept Segment has
    /// only kept Segments under it in the stack.
    stack: Vec<(OutlineSegment, bool)>,
    out: Vec<OutlineSegment>,
}

impl<'a> Collector<'a> {
    pub fn for_anchors(anchors: &'a [usize]) -> Self {
        debug_assert!(anchors.is_sorted());
        Self {
            anchors,
            all: false,
            next: 0,
            stack: Vec::new(),
            out: Vec::new(),
        }
    }

    pub fn for_names() -> Collector<'static> {
        Collector {
            anchors: &[],
            all: true,
            next: 0,
            stack: Vec::new(),
            out: Vec::new(),
        }
    }

    /// Every anchor before `at` lies inside every open Segment.
    fn pass(&mut self, at: usize) {
        let before = self.next;
        while self.next < self.anchors.len() && self.anchors[self.next] < at {
            self.next += 1;
        }
        if self.next > before {
            for (_, keep) in self.stack.iter_mut().rev() {
                if *keep {
                    break;
                }
                *keep = true;
            }
        }
    }

    /// A Segment starting at `seg.range.start`; `close` sets its end.
    pub fn open(&mut self, seg: OutlineSegment) {
        self.pass(seg.range.start);
        self.stack.push((seg, false));
    }

    /// Hides the innermost open Segment: a key or element whose value is an array (spec §5.2).
    pub fn hide_top(&mut self) {
        if let Some((s, _)) = self.stack.last_mut() {
            s.hidden = true;
        }
    }

    /// Shows every Segment of `kind`, closed or open: YAML documents once there are two.
    pub fn reveal(&mut self, kind: &str) {
        for s in self
            .out
            .iter_mut()
            .chain(self.stack.iter_mut().map(|(s, _)| s))
        {
            if s.segment.kind == kind {
                s.hidden = false;
            }
        }
    }

    /// Closes the innermost open Segment at `end` (exclusive).
    pub fn close(&mut self, end: usize) {
        self.pass(end);
        if let Some((mut s, keep)) = self.stack.pop() {
            s.range.end = end.max(s.range.start);
            if keep || self.all {
                self.out.push(s);
            }
        }
    }

    /// Closes what is still open at `end` (the end of the file, or a syntax error) and builds
    /// the Outline.
    pub fn finish(mut self, end: usize, src: &[u8]) -> Outline {
        while !self.stack.is_empty() {
            self.close(end);
        }
        Outline::build(self.out, src)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(kind: &str, name: &str, start: usize) -> OutlineSegment {
        OutlineSegment::new(kind, name, start..start, start..start)
    }

    /// a [0,20) { b [2,8) , c [10,18) { d [12,16) } }
    fn walk(c: &mut Collector) {
        c.open(seg("key", "a", 0));
        c.open(seg("key", "b", 2));
        c.close(8);
        c.open(seg("key", "c", 10));
        c.open(seg("key", "d", 12));
        c.close(16);
        c.close(18);
        c.close(20);
    }

    fn names(o: &Outline) -> Vec<&str> {
        o.segments.iter().map(|s| s.segment.name.as_str()).collect()
    }

    #[test]
    fn keeps_only_segments_that_hold_an_anchor() {
        let src = vec![b'x'; 30];
        let mut c = Collector::for_anchors(&[13]);
        walk(&mut c);
        assert_eq!(names(&c.finish(20, &src)), ["a", "c", "d"]);
        let mut c = Collector::for_anchors(&[3, 25]);
        walk(&mut c);
        assert_eq!(names(&c.finish(30, &src)), ["a", "b"]);
    }

    #[test]
    fn a_start_is_inside_and_an_end_is_outside() {
        let src = vec![b'x'; 30];
        let mut c = Collector::for_anchors(&[2, 8]);
        walk(&mut c);
        // 2 is inside b; 8 is past b's end, still inside a.
        assert_eq!(names(&c.finish(20, &src)), ["a", "b"]);
    }

    #[test]
    fn names_keep_everything_and_finish_closes_what_is_open() {
        let src = vec![b'x'; 30];
        let mut c = Collector::for_names();
        c.open(seg("key", "a", 0));
        c.open(seg("key", "b", 2));
        // A syntax error at 5: both are closed there.
        let o = c.finish(5, &src);
        assert_eq!(names(&o), ["a", "b"]);
        assert!(o.segments.iter().all(|s| s.range.end == 5));
    }

    #[test]
    fn hide_top_and_reveal() {
        let src = vec![b'x'; 30];
        let mut c = Collector::for_names();
        c.open(seg("key", "ports", 0));
        c.hide_top();
        c.open(seg("element", "ports[0]", 2));
        c.close(4);
        c.close(6);
        let mut d = seg("document", "document 1", 6);
        d.hidden = true;
        c.open(d);
        c.close(8);
        c.reveal("document");
        let o = c.finish(30, &src);
        let hidden: Vec<bool> = o.segments.iter().map(|s| s.hidden).collect();
        assert_eq!(hidden, [true, false, false]);
    }
}
