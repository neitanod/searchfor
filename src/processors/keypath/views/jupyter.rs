//! Jupyter notebooks (M6 spec §6.5): each element of `cells` is `cell N`, each of a cell's
//! `outputs` is `output N`; the keys `cells`, `source`, `outputs`, `data` and `text` and the
//! indexes of the source lines hide, and an output's MIME keys stay. The Markdown cells'
//! headings, decoded from their JSON strings, nest by level over the cells up to the next
//! heading of the same or a higher level.
use super::{Built, Ctx, NameBuf, Node, Probe, Role, find, text_of};
use crate::model::Segment;
use crate::processors::keypath::json::{self, Met, Step};
use crate::text::Pieces;
use crate::trail::{Outline, OutlineSegment};
use std::ops::Range;

pub(super) fn project(ctx: Ctx, p: &Probe) -> (Role, Ctx) {
    match ctx {
        Ctx::Notebook if p.is("cells") && p.array() => (Role::Hidden, Ctx::Cells),
        Ctx::Cells if p.element() && p.object() => (Role::Node(Node::Cell(p.index)), Ctx::Cell),
        Ctx::Cell if p.is("source") => (Role::Hidden, Ctx::Lines),
        Ctx::Cell if p.is("outputs") && p.array() => (Role::Hidden, Ctx::Outputs),
        Ctx::Outputs if p.element() && p.object() => {
            (Role::Node(Node::Output(p.index)), Ctx::Output)
        }
        Ctx::Output if p.is("data") && p.object() => (Role::Hidden, Ctx::Data),
        Ctx::Output if p.is("text") => (Role::Hidden, Ctx::Lines),
        // A MIME key stays, its lines hide.
        Ctx::Data if p.array() => (Role::Shown, Ctx::Lines),
        _ => (Role::Plain, Ctx::Plain),
    }
}

/// `cell N` or `output N`, with its type in the signature: `cell 2 (code)`,
/// `output 1 (execute_result)`. Every word of the name is s4's.
pub(super) fn node(
    src: &[u8],
    node: Node,
    value: usize,
    base: Segment,
    name_chars: usize,
) -> Option<Built> {
    let (kind, n, key) = match node {
        Node::Cell(i) => ("cell", i + 1, "cell_type"),
        Node::Output(i) => ("output", i + 1, "output_type"),
        _ => return None,
    };
    let name = format!("{kind} {n}");
    let ty = find(src, value, &[&[key]])[0]
        .as_ref()
        .and_then(|v| text_of(src, v));
    let signature = ty.map(|t| {
        let mut b = NameBuf::default();
        b.source(&t, 0);
        format!("{name} ({})", b.finish(name_chars).0)
    });
    Some(Built {
        seg: Segment {
            kind: kind.into(),
            name,
            signature,
            ..base
        },
        label: value..value,
        pieces: None,
    })
}

/// A heading line of a Markdown cell.
struct Heading {
    level: usize,
    /// The line, from its first byte to its end, in the Source.
    line: Range<usize>,
    /// Its title, decoded and cut, with its pieces.
    name: (String, Pieces),
    /// Where its title starts in the Source: an empty title's anchor.
    title_at: usize,
    /// The whole line, decoded and cut at twice `name_chars`.
    signature: String,
}

/// The Outline of the headings of the Markdown cells of `cells`, the array at `at` (M6 plan,
/// Task 6.1): read once, as the file is prepared.
pub(super) fn headings(src: &[u8], at: usize, name_chars: usize) -> Outline {
    let mut found: Vec<Heading> = Vec::new();
    // The cell being read: whether it is Markdown, and its source strings.
    let mut markdown = false;
    let mut strings: Vec<Range<usize>> = Vec::new();
    let flush = |markdown: &mut bool, strings: &mut Vec<Range<usize>>, out: &mut Vec<Heading>| {
        if *markdown {
            scan(src, strings, name_chars, out);
        }
        *markdown = false;
        strings.clear();
    };
    let end = json::visit(src, at, &mut |m: &Met| {
        let string = src.get(m.value.start) == Some(&b'"');
        match m.path {
            [Step::Index(_)] => flush(&mut markdown, &mut strings, &mut found),
            [Step::Index(_), k] if k.is(src, "cell_type") => {
                markdown = string && &src[m.value.clone()] == b"\"markdown\"";
            }
            [Step::Index(_), k] if string && k.is(src, "source") => {
                strings.push(m.value.start + 1..m.value.end - 1);
            }
            [Step::Index(_), k, Step::Index(_)] if string && k.is(src, "source") => {
                strings.push(m.value.start + 1..m.value.end - 1);
            }
            _ => {}
        }
        true
    })
    .unwrap_or(src.len());
    flush(&mut markdown, &mut strings, &mut found);
    let mut segments = Vec::with_capacity(found.len());
    let starts: Vec<(usize, usize)> = found.iter().map(|h| (h.level, h.line.start)).collect();
    for (i, h) in found.into_iter().enumerate() {
        let until = starts[i + 1..]
            .iter()
            .find(|n| n.0 <= h.level)
            .map_or(end, |n| n.1);
        let (name, pieces) = h.name;
        let mut s = OutlineSegment::new("heading", name, h.line.start..until, h.line.clone());
        s.segment.level = Some(h.level);
        s.segment.signature = Some(h.signature);
        s.name_anchor = pieces.first().map_or(h.title_at, |p| p.1.start);
        s.name = Some(pieces);
        segments.push(s);
    }
    Outline::build(segments, src)
}

/// The headings of a Markdown cell's source strings (their contents, raw), read over the
/// strings decoded and joined (code round 1, Codex #7): a line ends at a decoded line break,
/// a `\r` before it dropped; a heading is `#` to `######` after at most three spaces, then a
/// blank or the line's end, outside fenced code.
fn scan(src: &[u8], strings: &[Range<usize>], name_chars: usize, out: &mut Vec<Heading>) {
    let mut text = String::new();
    let mut pieces: Pieces = Vec::new();
    for s in strings {
        let (t, p) = json::decode_split(src, s.clone());
        let base = text.len();
        text.push_str(&t);
        pieces.extend(
            p.into_iter()
                .map(|(t, s)| (t.start + base..t.end + base, s)),
        );
    }
    let cell = Decoded {
        text: &text,
        pieces: &pieces,
        at: strings.first().map_or(0, |s| s.start),
        name_chars,
    };
    let bytes = text.as_bytes();
    let mut fence = false;
    let mut from = 0;
    while from < bytes.len() {
        let nl = memchr::memchr(b'\n', &bytes[from..]).map(|i| from + i);
        let mut end = nl.unwrap_or(bytes.len());
        if end > from && bytes[end - 1] == b'\r' {
            end -= 1;
        }
        cell.line(from..end, &mut fence, out);
        from = nl.map_or(bytes.len(), |i| i + 1);
    }
}

/// A cell's source decoded: its text, and where each piece of it is in the Source.
struct Decoded<'t> {
    text: &'t str,
    pieces: &'t Pieces,
    /// The Source offset of a text with no piece.
    at: usize,
    name_chars: usize,
}

impl Decoded<'_> {
    /// The Source offset of text byte `p`: where the piece that holds it starts, or where the
    /// one before it ends. The pieces are in text order, so each lookup is a binary search.
    fn source_at(&self, p: usize) -> usize {
        let i = self.pieces.partition_point(|(t, _)| t.end <= p);
        match self.pieces.get(i) {
            Some((t, s)) if t.start <= p && t.len() == s.len() => s.start + (p - t.start),
            Some((t, s)) if t.start <= p => s.start,
            _ => i.checked_sub(1).map_or(self.at, |j| self.pieces[j].1.end),
        }
    }

    /// The Source offset just past text byte `p - 1`.
    fn source_end(&self, p: usize) -> usize {
        let i = self.pieces.partition_point(|(t, _)| t.end < p);
        match self.pieces.get(i) {
            Some((t, s)) if t.start < p && t.len() == s.len() => s.start + (p - t.start),
            Some((t, s)) if t.start < p => s.end,
            _ => self.source_at(p),
        }
    }

    /// The text `r` and its pieces, cut at `max` characters. `r` ends at characters'
    /// boundaries, so a decoded piece is either in it or out.
    fn slice(&self, r: Range<usize>, max: usize) -> (String, Pieces) {
        let first = self.pieces.partition_point(|(t, _)| t.end <= r.start);
        let mut pieces = Vec::new();
        for (t, s) in &self.pieces[first..] {
            if t.start >= r.end {
                break;
            }
            let (from, to) = (t.start.max(r.start), t.end.min(r.end));
            let s = if t.len() == s.len() {
                s.start + (from - t.start)..s.start + (to - t.start)
            } else {
                s.clone()
            };
            pieces.push((from - r.start..to - r.start, s));
        }
        let mut name = NameBuf::default();
        name.source(&(self.text[r].to_string(), pieces), 0);
        name.finish(max)
    }

    /// One decoded line `r` of the text: a heading, or a code fence that opens or closes.
    fn line(&self, r: Range<usize>, fence: &mut bool, out: &mut Vec<Heading>) {
        let b = self.text.as_bytes();
        let raw = &b[r.clone()];
        let indent = raw.iter().take_while(|&&c| c == b' ').count();
        if indent > 3 {
            return;
        }
        let body = &raw[indent..];
        if body.starts_with(b"```") || body.starts_with(b"~~~") {
            *fence = !*fence;
            return;
        }
        if *fence {
            return;
        }
        let level = body.iter().take_while(|&&c| c == b'#').count();
        if !(1..=6).contains(&level) {
            return;
        }
        let blank = |c: u8| c == b' ' || c == b'\t';
        if body.get(level).is_some_and(|&c| !blank(c)) {
            return;
        }
        let mut s = r.start + indent + level;
        let mut e = r.end;
        while s < e && blank(b[s]) {
            s += 1;
        }
        while e > s && blank(b[e - 1]) {
            e -= 1;
        }
        // A closing run of `#` after a blank, or the whole title.
        let mut k = e;
        while k > s && b[k - 1] == b'#' {
            k -= 1;
        }
        if k < e && (k == s || blank(b[k - 1])) {
            e = k;
            while e > s && blank(b[e - 1]) {
                e -= 1;
            }
        }
        out.push(Heading {
            level,
            line: self.source_at(r.start)..self.source_end(r.end),
            name: self.slice(s..e, self.name_chars),
            title_at: self.source_at(s),
            signature: self.slice(r, self.name_chars.saturating_mul(2)).0,
        });
    }
}
