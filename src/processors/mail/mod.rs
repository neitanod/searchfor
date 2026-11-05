//! mail: one RFC 5322 message (`.eml`) or an mbox of them (M6 spec §10.1). Header fields are
//! `header` Segments over their folded lines; an mbox message is a `message` Segment named by
//! its decoded `Subject`; a body is a `part`. The messages of an mbox are its top-level units:
//! the cursor finds their boundaries as the Matches reach them and builds the headers and parts
//! of a message only when a Match falls in it (plan D8, spec §11).
mod names;

use super::{Capability, Cursor, Prepared, Processor};
use crate::model::{Input, Name, SharedTrail, Trail};
use crate::trail::{Outline, OutlineSegment};
use names::{HeaderBlock, NameBuf, decoded_value, header_block, line_at};
use std::ops::Range;
use std::rc::Rc;

pub struct Mail;

#[derive(Debug, Clone, Copy)]
struct Params {
    mbox: bool,
    name_chars: usize,
}

impl Params {
    fn of(t: &toml::Table) -> anyhow::Result<Self> {
        let mbox = match t.get("syntax").and_then(|v| v.as_str()) {
            Some("eml") => false,
            Some("mbox") => true,
            _ => anyhow::bail!("mail needs `syntax` = \"eml\" or \"mbox\" in [params]"),
        };
        let name_chars = t
            .get("name_chars")
            .and_then(|v| v.as_integer())
            .map_or(40, |n| n.max(0) as usize);
        Ok(Self { mbox, name_chars })
    }
}

/// A top-level unit: a message of an mbox, the text before its first one, or a whole `.eml`.
/// A few dozen bytes each (spec §11).
#[derive(Debug, Clone)]
struct Unit {
    range: Range<usize>,
    /// The line its first byte is on.
    line: u64,
    ordinal: usize,
    message: bool,
}

/// Where the next mbox `From ` line after `from` starts: a line that begins with `From ` and
/// follows a blank line (mboxo and mboxrd alike; a `>From ` line is text).
fn next_from(src: &[u8], from: usize) -> Option<usize> {
    let finder = memchr::memmem::Finder::new(b"\nFrom ");
    let mut at = from;
    while let Some(k) = finder.find(&src[at..]) {
        let nl = at + k;
        let blank = nl == 0
            || src[nl - 1] == b'\n'
            || (src[nl - 1] == b'\r' && (nl == 1 || src[nl - 2] == b'\n'));
        if blank && nl + 1 > from {
            return Some(nl + 1);
        }
        at = nl + 1;
    }
    None
}

/// The units of a file in order, found as they are asked for.
struct Units<'s> {
    src: &'s [u8],
    mbox: bool,
    pos: usize,
    line: u64,
    ordinal: usize,
}

impl<'s> Units<'s> {
    fn new(src: &'s [u8], mbox: bool) -> Self {
        Units {
            src,
            mbox,
            pos: 0,
            line: 1,
            ordinal: 0,
        }
    }
}

impl Iterator for Units<'_> {
    type Item = Unit;

    fn next(&mut self) -> Option<Unit> {
        let src = self.src;
        if self.pos >= src.len() {
            return None;
        }
        let start = self.pos;
        let (end, message) = if self.mbox {
            (
                next_from(src, start).unwrap_or(src.len()),
                src[start..].starts_with(b"From "),
            )
        } else {
            (src.len(), true)
        };
        let unit = Unit {
            range: start..end,
            line: self.line,
            ordinal: self.ordinal,
            message,
        };
        self.line += memchr::memchr_iter(b'\n', &src[start..end]).count() as u64;
        self.ordinal += 1;
        self.pos = end;
        Some(unit)
    }
}

/// A header field's Segment: named by the field's name as written, labelled by it.
fn header_segment(src: &[u8], h: &names::Header, max: usize) -> OutlineSegment {
    let mut n = NameBuf::new(max);
    n.push_source(src, h.name.clone());
    let (name, pieces) = n.finish();
    let mut s = OutlineSegment::new("header", name, h.range.clone(), h.name.clone());
    s.name = Some(pieces);
    s.name_anchor = h.name.start;
    s
}

/// An mbox message's Segment: named by its decoded `Subject` (`(no subject)` without one),
/// labelled by the subject's bytes, so a Match in them is outside the message and inside its
/// `Subject` header (rule 5); signature `From · Date · Subject`.
fn message_segment(src: &[u8], unit: &Unit, block: &HeaderBlock, max: usize) -> OutlineSegment {
    let subject = block
        .get(src, "Subject")
        .map(|h| (decoded_value(src, h, max), h.value_span()))
        .filter(|((t, _), _)| !t.is_empty());
    let at = unit.range.start;
    let (name, pieces, label) = match subject {
        Some(((t, p), span)) => (t, Some(p), span),
        None => ("(no subject)".to_string(), None, at..at),
    };
    let mut sig: Vec<String> = ["From", "Date"]
        .iter()
        .filter_map(|f| block.get(src, f))
        .map(|h| decoded_value(src, h, max).0)
        .filter(|t| !t.is_empty())
        .collect();
    sig.push(name.clone());
    let mut s = OutlineSegment::new("message", name, unit.range.clone(), label.clone());
    s.segment.signature = Some(sig.join(" · "));
    s.name_anchor = label.start;
    s.name = pieces;
    s
}

/// The Segments of a body: a non-multipart one is the part `body`.
fn body_segments(
    src: &[u8],
    _block: &HeaderBlock,
    body: Range<usize>,
    _p: Params,
) -> Vec<OutlineSegment> {
    let _ = src;
    if body.is_empty() {
        return Vec::new();
    }
    vec![OutlineSegment::new(
        "part",
        "body",
        body.clone(),
        body.start..body.start,
    )]
}

/// Fills every Segment's lines, counting line breaks from `base`, which is on line `line`:
/// a message's Segments cost the bytes of that message only.
fn fill_lines(segs: &mut [OutlineSegment], src: &[u8], base: usize, line: u64) {
    let mut marks: Vec<(usize, usize, bool)> = Vec::with_capacity(segs.len() * 2);
    for (i, s) in segs.iter().enumerate() {
        let r = &s.range;
        marks.push((r.start, i, false));
        marks.push((r.end.saturating_sub(1).max(r.start), i, true));
    }
    marks.sort_unstable();
    let (mut at, mut n) = (base, line);
    for (byte, i, end) in marks {
        let to = byte.min(src.len()).max(at);
        n += memchr::memchr_iter(b'\n', &src[at..to]).count() as u64;
        at = to;
        if end {
            segs[i].segment.end_line = n;
        } else {
            segs[i].segment.line = n;
        }
    }
}

/// The Outline of one unit: its headers, its message (mbox) and its parts.
fn unit_outline(src: &[u8], unit: &Unit, p: Params) -> Outline {
    if !unit.message {
        return Outline::default();
    }
    let end = unit.range.end;
    let mut pos = unit.range.start;
    if p.mbox {
        pos = line_at(src, pos, end).1;
    } else if src[pos..end].starts_with(b"\xEF\xBB\xBF") {
        pos += 3;
    }
    let block = header_block(src, pos, end);
    let mut segs: Vec<OutlineSegment> = block
        .headers
        .iter()
        .map(|h| header_segment(src, h, p.name_chars))
        .collect();
    if p.mbox {
        segs.push(message_segment(src, unit, &block, p.name_chars));
    }
    segs.extend(body_segments(src, &block, block.body..end, p));
    fill_lines(&mut segs, src, unit.range.start, unit.line);
    Outline::build(segs, src)
}

/// Ordered Trails over one unit's Outline, owned (`trail::TrailCursor` borrows its Outline,
/// which here lives only as long as the cursor is in its unit).
struct Walk {
    outline: Outline,
    next: usize,
    stack: Vec<usize>,
    last: usize,
}

impl Walk {
    fn new(outline: Outline) -> Self {
        Walk {
            outline,
            next: 0,
            stack: Vec::new(),
            last: 0,
        }
    }

    /// The indices of the Trail of `byte`, as `trail::TrailCursor::indices` gives them.
    fn indices(&mut self, byte: usize, out: &mut Vec<usize>) {
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

    fn segments(&self, ix: &[usize]) -> Trail {
        ix.iter()
            .map(|&i| self.outline.segments[i].segment.clone())
            .collect()
    }

    /// The start of the outermost shown Segment open at the last offset: no later Trail ends
    /// with a Segment that starts before it.
    fn open_from(&self) -> usize {
        let segs = &self.outline.segments;
        self.stack
            .iter()
            .find(|&&i| !segs[i].hidden)
            .map_or(self.last, |&i| segs[i].range.start)
    }
}

/// The searchable names of one unit's Outline. A message's or a part's name comes from a
/// header inside it: its Trail ends with that header, so a Match in the name has the header
/// and not the Segment it names (rule 5), as a Match on the same bytes of the Source has.
fn unit_names(o: &Outline, out: &mut Vec<Name>) {
    let segs = &o.segments;
    let chain = |i: usize| {
        let mut c = Vec::new();
        let mut cur = Some(i);
        while let Some(j) = cur {
            if !segs[j].hidden || j == i {
                c.push(j);
            }
            cur = segs[j].parent;
        }
        c.reverse();
        c
    };
    for (i, s) in segs.iter().enumerate() {
        let Some(pieces) = &s.name else {
            continue;
        };
        let mut c = chain(i);
        if s.segment.kind != "header" && !s.label.is_empty() {
            let at = s.label.start;
            if let Some(j) = (i + 1..segs.len())
                .take_while(|&j| segs[j].range.start < s.range.end)
                .find(|&j| {
                    segs[j].parent == Some(i)
                        && segs[j].segment.kind == "header"
                        && segs[j].range.contains(&at)
                })
            {
                c.push(j);
            }
        }
        out.push(Name {
            text: s.segment.name.clone(),
            pieces: pieces.clone(),
            trail: c.iter().map(|&j| segs[j].segment.clone()).collect(),
            labels: c.iter().map(|&j| segs[j].label.clone()).collect(),
            anchor: s.name_anchor,
        });
    }
}

/// A mail file prepared for its cursor: nothing is read ahead of time.
struct MailPrepared<'a> {
    src: &'a [u8],
    p: Params,
}

impl Prepared for MailPrepared<'_> {
    fn cursor(&self) -> Box<dyn Cursor + '_> {
        Box::new(MailCursor {
            src: self.src,
            p: self.p,
            units: Units::new(self.src, self.p.mbox),
            unit: None,
            walk: None,
            at: 0,
            ix: Vec::new(),
            last: None,
        })
    }

    /// Every unit's names: `--in-headings` reads the whole file, the one path that does.
    fn names(&self) -> anyhow::Result<Vec<Name>> {
        let mut out = Vec::new();
        for u in Units::new(self.src, self.p.mbox) {
            unit_names(&unit_outline(self.src, &u, self.p), &mut out);
        }
        Ok(out)
    }

    fn streams(&self) -> bool {
        true
    }
}

/// The Trails of a mail file in file order: the unit that holds or follows the last offset,
/// its Outline once a Match falls in it, and the Trail last given.
struct MailCursor<'s> {
    src: &'s [u8],
    p: Params,
    units: Units<'s>,
    unit: Option<Unit>,
    walk: Option<Walk>,
    at: usize,
    ix: Vec<usize>,
    /// (unit ordinal, Segment indices, Trail); `usize::MAX` for the empty Trail.
    last: Option<(usize, Vec<usize>, SharedTrail)>,
}

impl MailCursor<'_> {
    fn shared(&mut self, ordinal: usize, build: impl FnOnce(&[usize]) -> Trail) -> SharedTrail {
        if let Some((o, ix, t)) = &self.last
            && *o == ordinal
            && *ix == self.ix
        {
            return t.clone();
        }
        let t: SharedTrail = Rc::from(build(&self.ix));
        self.last = Some((ordinal, self.ix.clone(), t.clone()));
        t
    }
}

impl Cursor for MailCursor<'_> {
    fn trail_at(&mut self, byte: usize) -> anyhow::Result<SharedTrail> {
        if byte < self.at {
            // An offset before the last one reads the units again.
            self.units = Units::new(self.src, self.p.mbox);
            self.unit = None;
            self.walk = None;
        }
        self.at = byte;
        while self.unit.as_ref().is_none_or(|u| u.range.end <= byte) {
            self.walk = None;
            self.unit = self.units.next();
            if self.unit.is_none() {
                break;
            }
        }
        let Some(u) = self.unit.as_ref().filter(|u| u.range.start <= byte) else {
            self.ix.clear();
            return Ok(self.shared(usize::MAX, |_| Vec::new()));
        };
        let ordinal = u.ordinal;
        let walk = self
            .walk
            .get_or_insert_with(|| Walk::new(unit_outline(self.src, u, self.p)));
        walk.indices(byte, &mut self.ix);
        let walk = self.walk.take().expect("built above");
        let t = self.shared(ordinal, |ix| walk.segments(ix));
        self.walk = Some(walk);
        Ok(t)
    }

    fn open_from(&mut self) -> Option<usize> {
        match (&self.unit, &self.walk) {
            (Some(u), Some(w)) if u.range.contains(&self.at) => Some(w.open_from()),
            _ => Some(self.at),
        }
    }
}

impl Processor for Mail {
    fn trails(
        &self,
        input: &Input,
        params: &toml::Table,
        anchors: &[usize],
    ) -> anyhow::Result<Vec<Trail>> {
        let prepared = self.prepare(*input, params)?;
        let mut cursor = prepared.cursor();
        let mut order: Vec<usize> = (0..anchors.len()).collect();
        order.sort_by_key(|&i| anchors[i]);
        let mut out = vec![Vec::new(); anchors.len()];
        for i in order {
            out[i] = cursor.trail_at(anchors[i])?.to_vec();
        }
        Ok(out)
    }

    fn names(&self, input: &Input, params: &toml::Table) -> anyhow::Result<Vec<Name>> {
        self.prepare(*input, params)?.names()
    }

    fn prepare<'a>(
        &'a self,
        input: Input<'a>,
        params: &'a toml::Table,
    ) -> anyhow::Result<Box<dyn Prepared + 'a>> {
        Ok(Box::new(MailPrepared {
            src: input.bytes,
            p: Params::of(params)?,
        }))
    }

    fn capability(&self, _: &toml::Table) -> Capability {
        Capability::Native
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn params(syntax: &str) -> toml::Table {
        toml::from_str(&format!("syntax='{syntax}'\nname_chars=40")).unwrap()
    }

    fn needles(syntax: &str, src: &str) -> Vec<Vec<(String, u64, u64)>> {
        let anchors: Vec<usize> = src.match_indices("needle").map(|(i, _)| i).collect();
        let input = Input {
            bytes: src.as_bytes(),
            path: Path::new("x"),
            format: syntax,
            disk: None,
        };
        Mail.trails(&input, &params(syntax), &anchors)
            .unwrap()
            .iter()
            .map(|t| {
                t.iter()
                    .map(|s| (s.name.clone(), s.line, s.end_line))
                    .collect()
            })
            .collect()
    }

    #[test]
    fn mbox_messages_start_after_a_blank_line_only() {
        let src = "junk needle\n\nFrom a x\nSubject: A\n\nneedle\nFrom b\n>From needle\n\nFrom c\n\nneedle\n";
        let t = needles("mbox", src);
        assert_eq!(t[0], []);
        assert_eq!(t[1], [("A".into(), 3, 9), ("body".into(), 6, 9)]);
        assert_eq!(t[2], t[1]);
        assert_eq!(
            t[3],
            [("(no subject)".into(), 10, 12), ("body".into(), 12, 12)]
        );
    }

    #[test]
    fn the_cursor_reads_only_up_to_the_unit_of_the_last_offset() {
        let src = b"From a\nSubject: A\n\nneedle\n\nFrom b\n\nx\n";
        let p = params("mbox");
        let prepared = Mail
            .prepare(
                Input {
                    bytes: src,
                    path: Path::new("x"),
                    format: "mbox",
                    disk: None,
                },
                &p,
            )
            .unwrap();
        let mut c = prepared.cursor();
        let t = c.trail_at(19).unwrap();
        assert_eq!(t.len(), 2);
        assert_eq!(c.open_from(), Some(0));
        // A repeated offset is the same Trail.
        assert!(Rc::ptr_eq(&t, &c.trail_at(19).unwrap()));
        // An earlier one is answered too.
        assert_eq!(c.trail_at(8).unwrap().len(), 1);
    }
}
