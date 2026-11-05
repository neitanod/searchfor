//! Jupyter notebooks (M6 spec §6.5): each element of `cells` is `cell N`, each of a cell's
//! `outputs` is `output N`; the keys `cells`, `source`, `outputs`, `data` and `text` and the
//! indexes of the source lines hide, and an output's MIME keys stay. The Markdown cells'
//! headings, decoded from their JSON strings, nest by level over the cells up to the next
//! heading of the same or a higher level.
use super::{Built, Ctx, NameBuf, Node, Probe, Role, find, text_of};
use crate::model::Segment;
use crate::processors::keypath::json::{self, Met, Step};
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
    /// The line, from its first byte to its end, in the JSON string.
    line: Range<usize>,
    /// Its title's bytes, and whether they have escapes.
    title: Range<usize>,
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
            scan(src, strings, out);
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
    for (i, h) in found.iter().enumerate() {
        let until = found[i + 1..]
            .iter()
            .find(|n| n.level <= h.level)
            .map_or(end, |n| n.line.start);
        let escaped = memchr::memchr(b'\\', &src[h.title.clone()]).is_some();
        let mut name = NameBuf::default();
        name.source(&json::decode(src, h.title.clone(), escaped), 0);
        let (name, pieces) = name.finish(name_chars);
        let line_escaped = memchr::memchr(b'\\', &src[h.line.clone()]).is_some();
        let mut sig = NameBuf::default();
        sig.source(&json::decode(src, h.line.clone(), line_escaped), 0);
        let mut s = OutlineSegment::new("heading", name, h.line.start..until, h.line.clone());
        s.segment.level = Some(h.level);
        s.segment.signature = Some(sig.finish(name_chars.saturating_mul(2)).0);
        s.name_anchor = pieces.first().map_or(h.title.start, |p| p.1.start);
        s.name = Some(pieces);
        segments.push(s);
    }
    Outline::build(segments, src)
}

/// The headings of a Markdown cell's source strings (their contents, raw): a line is a
/// decoded line of the source, the strings joined; a heading is `#` to `######` after at most
/// three spaces, then a space, a tab or the line's end, outside fenced code.
fn scan(src: &[u8], strings: &[Range<usize>], out: &mut Vec<Heading>) {
    let mut fence = false;
    // Whether the next string starts a line: the one before ended with a line break.
    let mut line_start = true;
    for s in strings {
        let (mut i, mut from) = (s.start, s.start);
        while i < s.end {
            // A decoded line break: `\n` or `\u000a`.
            let next = match (src[i], src.get(i + 1)) {
                (b'\\', Some(b'n')) => Some(i + 2),
                (b'\\', Some(b'u'))
                    if src
                        .get(i + 2..i + 6)
                        .is_some_and(|h| h.eq_ignore_ascii_case(b"000a")) =>
                {
                    Some(i + 6)
                }
                _ => None,
            };
            let Some(next) = next else {
                i += if src[i] == b'\\' { 2 } else { 1 };
                continue;
            };
            let mut end = i;
            // A `\r` before the line break is no text of the line.
            if end >= from + 2 && &src[end - 2..end] == b"\\r" {
                end -= 2;
            }
            if line_start {
                line(src, from..end, &mut fence, out);
            }
            line_start = true;
            (i, from) = (next, next);
        }
        // What follows the last line break: a line the next string may continue.
        if from < s.end {
            if line_start {
                line(src, from..s.end, &mut fence, out);
            }
            line_start = false;
        }
    }
}

/// One decoded line, raw at `r`: a heading, or a code fence that opens or closes.
fn line(src: &[u8], r: Range<usize>, fence: &mut bool, out: &mut Vec<Heading>) {
    let raw = &src[r.clone()];
    let indent = raw.iter().take_while(|&&b| b == b' ').count();
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
    let level = body.iter().take_while(|&&b| b == b'#').count();
    if !(1..=6).contains(&level) {
        return;
    }
    let after = &body[level..];
    let gap = if after.is_empty() {
        0
    } else if after[0] == b' ' {
        1
    } else if after.starts_with(b"\\t") {
        2
    } else {
        return;
    };
    let mut s = r.start + indent + level + gap;
    let mut e = r.end;
    let blank = |b: &[u8], at: usize| -> usize {
        if b.get(at) == Some(&b' ') {
            1
        } else if b.get(at..at + 2) == Some(b"\\t") {
            2
        } else {
            0
        }
    };
    loop {
        let n = blank(src, s);
        if n == 0 || s + n > e {
            break;
        }
        s += n;
    }
    // Trailing blanks, then a closing run of `#` after a blank.
    let trim_end = |e: &mut usize| {
        while *e > s {
            if src[*e - 1] == b' ' {
                *e -= 1;
            } else if *e >= s + 2 && &src[*e - 2..*e] == b"\\t" {
                *e -= 2;
            } else {
                break;
            }
        }
    };
    trim_end(&mut e);
    let mut k = e;
    while k > s && src[k - 1] == b'#' {
        k -= 1;
    }
    if k < e && (k == s || src[k - 1] == b' ') {
        e = k;
        trim_end(&mut e);
    }
    out.push(Heading {
        level,
        line: r,
        title: s..e,
    });
}
