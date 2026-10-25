//! The YAML walker of `keypath` (spec §5.2), over saphyr-parser's events. A scalar's span covers
//! it (a literal block scalar through its last line); a block collection's end event sits at the
//! next token, so a container closes at the end of its last scalar or alias. `Marker` counts
//! characters, so bytes come from its character index. On a syntax error the walk stops there,
//! and the Segments still open close at the error (D3).
use super::collect::Collector;
use crate::trail::OutlineSegment;
use saphyr_parser::{Event, Marker, Parser, ScalarStyle, StrInput};
use std::ops::Range;

/// A complex key (`? [a, b]`) is named by its first 40 characters (spec §5.2).
const COMPLEX_KEY_CHARS: usize = 40;

/// `Name::pieces`: (range in the name's text, range in the Source).
type Pieces = Vec<(Range<usize>, Range<usize>)>;

enum Frame {
    /// A mapping; `key` is true between a key and the end of its value.
    Map {
        key: bool,
        key_name: String,
    },
    /// A sequence and the name its elements extend (`ports` → `ports[0]`).
    Seq {
        holder: String,
        n: usize,
    },
    /// A complex key (`? [a, b]`) being skipped: where it started and its nesting.
    Complex {
        start: usize,
        depth: usize,
    },
    Doc,
}

/// Byte offsets of saphyr's markers. `Marker::index` counts characters, and its line breaks are
/// CR, LF and CRLF where rg's are LF only, so bytes come from the character index alone (review
/// m2-code-r1, Codex #1). Markers mostly move forward, so a cursor walks from the last one.
struct Bytes<'t> {
    text: &'t str,
    /// Every character is one byte: the index is the byte.
    ascii: bool,
    /// (character, byte) of the last marker converted.
    at: (usize, usize),
}

impl Bytes<'_> {
    fn of(&mut self, m: &Marker) -> usize {
        let len = self.text.len();
        let target = m.index();
        if self.ascii {
            return target.min(len);
        }
        let b = self.text.as_bytes();
        let (mut ch, mut byte) = self.at;
        while ch < target && byte < len {
            byte += utf8_len(b[byte]);
            ch += 1;
        }
        while ch > target {
            byte -= 1;
            while !self.text.is_char_boundary(byte) {
                byte -= 1;
            }
            ch -= 1;
        }
        let byte = byte.min(len);
        self.at = (ch, byte);
        byte
    }
}

/// The length of the UTF-8 sequence that `lead` starts.
fn utf8_len(lead: u8) -> usize {
    match lead {
        0xF0.. => 4,
        0xE0.. => 3,
        0xC0.. => 2,
        _ => 1,
    }
}

/// `text[r]`, or "" when `r` does not fall on character boundaries inside `text`.
fn slice(text: &str, r: Range<usize>) -> &str {
    text.get(r).unwrap_or("")
}

/// What a walk reports: the events of `Collector`, which receives them for `trails` and
/// `names`; the preparation of a cursor (`Census`) and its walk (`Follow`) receive them too.
trait Sink {
    fn open(&mut self, seg: OutlineSegment);
    fn close(&mut self, end: usize);
    fn hide_top(&mut self);
    fn reveal(&mut self, kind: &str);
}

impl Sink for Collector<'_> {
    fn open(&mut self, seg: OutlineSegment) {
        Collector::open(self, seg)
    }

    fn close(&mut self, end: usize) {
        Collector::close(self, end)
    }

    fn hide_top(&mut self) {
        Collector::hide_top(self)
    }

    fn reveal(&mut self, kind: &str) {
        Collector::reveal(self, kind)
    }
}

/// Walks `src`, reporting its Segments to `c`; returns where the walk ended (the end of the
/// file, or a syntax error), where `Collector::finish` closes what is still open.
pub fn walk(src: &[u8], c: &mut Collector) -> usize {
    let mut w = Walker::new(src);
    loop {
        match w.next() {
            Next::Event(ev, s, e) => w.apply(ev, s, e, c),
            Next::Done(end) => return end,
        }
    }
}

/// An event of the walk with its Source range, or where the walk ended.
enum Next<'t> {
    Event(Event<'t>, usize, usize),
    Done(usize),
}

/// The walk over saphyr-parser's events, one at a time, so a cursor can stop it between two
/// Matches and go on later.
struct Walker<'t> {
    text: &'t str,
    bytes: Bytes<'t>,
    frames: Vec<Frame>,
    docs: usize,
    last_end: usize,
    /// None when `src` is not UTF-8: the walk ends at once, at 0.
    parser: Option<Parser<'t, StrInput<'t>>>,
    done: Option<usize>,
}

impl<'t> Walker<'t> {
    fn new(src: &'t [u8]) -> Self {
        let text = std::str::from_utf8(src).ok();
        Walker {
            text: text.unwrap_or(""),
            bytes: Bytes {
                text: text.unwrap_or(""),
                ascii: text.is_some_and(str::is_ascii),
                at: (0, 0),
            },
            frames: Vec::new(),
            docs: 0,
            last_end: 0,
            parser: text.map(Parser::new_from_str),
            done: text.is_none().then_some(0),
        }
    }

    /// The next event, with the bytes of its span.
    fn next(&mut self) -> Next<'t> {
        if let Some(end) = self.done {
            return Next::Done(end);
        }
        let parser = self.parser.as_mut().expect("a UTF-8 file");
        let end = match parser.next() {
            Some(Ok((ev, span))) => {
                let (s, e) = (self.bytes.of(&span.start), self.bytes.of(&span.end));
                return Next::Event(ev, s, e);
            }
            Some(Err(e)) => self.bytes.of(e.marker()).min(self.text.len()),
            None => self.text.len(),
        };
        self.done = Some(end);
        Next::Done(end)
    }

    /// Reports what event `ev`, over `s..e`, opens and closes.
    fn apply<S: Sink + ?Sized>(&mut self, ev: Event<'t>, s: usize, e: usize, c: &mut S) {
        let text = self.text;
        let frames = &mut self.frames;
        let last_end = &mut self.last_end;
        if let Some(Frame::Complex { start, depth }) = frames.last_mut() {
            match ev {
                Event::MappingStart(..) | Event::SequenceStart(..) => *depth += 1,
                Event::MappingEnd | Event::SequenceEnd => {
                    *depth -= 1;
                    // A flow collection's end event spans its `]` or `}`; a block one is empty.
                    if e > s {
                        *last_end = (*last_end).max(e);
                    }
                }
                Event::Scalar(..) | Event::Alias(_) => *last_end = e,
                _ => {}
            }
            if *depth == 0 {
                let start = *start;
                frames.pop();
                let raw = slice(text, start..(*last_end).max(start));
                let first = raw.lines().next().unwrap_or("");
                let name: String = first.chars().take(COMPLEX_KEY_CHARS).collect();
                let mut k =
                    OutlineSegment::new("key", name.clone(), start..start, start..*last_end);
                // The name is a prefix of the key's first line: verbatim.
                k.name = Some(vec![(0..name.len(), start..start + name.len())]);
                k.name_anchor = start;
                c.open(k);
                if let Some(Frame::Map { key, key_name }) = frames.last_mut() {
                    *key = true;
                    *key_name = name;
                }
            }
            return;
        }
        match ev {
            Event::DocumentStart(_) => {
                self.docs += 1;
                let docs = self.docs;
                if docs == 2 {
                    c.reveal("document");
                }
                let mut d = OutlineSegment::new("document", format!("document {docs}"), s..s, s..s);
                d.hidden = docs == 1;
                c.open(d);
                frames.push(Frame::Doc);
            }
            Event::DocumentEnd => {
                frames.pop();
                c.close((*last_end).max(s));
            }
            Event::MappingStart(..) | Event::SequenceStart(..) if expecting_key(frames) => {
                frames.push(Frame::Complex { start: s, depth: 1 });
            }
            Event::MappingStart(..) => {
                begin_value(frames, c, s);
                frames.push(Frame::Map {
                    key: false,
                    key_name: String::new(),
                });
            }
            Event::SequenceStart(..) => {
                let at_top = matches!(frames.last(), None | Some(Frame::Doc));
                let holder = begin_value(frames, c, s);
                if !at_top {
                    // The key or element that holds the sequence is hidden (spec §5.2).
                    c.hide_top();
                }
                frames.push(Frame::Seq { holder, n: 0 });
            }
            Event::MappingEnd | Event::SequenceEnd => {
                // A flow collection's end event spans its `]` or `}` (`a: {}` ends at byte 5); a
                // block collection's is empty, at the next token, so it ends at its last scalar.
                if e > s {
                    *last_end = (*last_end).max(e);
                }
                frames.pop();
                end_value(frames, c, *last_end);
            }
            Event::Scalar(value, style, ..) if expecting_key(frames) => {
                *last_end = e;
                let (name, pieces) = match style {
                    ScalarStyle::Literal | ScalarStyle::Folded => block_name(text, s..e, &value),
                    _ => (value.to_string(), name_pieces(text, s..e, &value, style)),
                };
                let mut k = OutlineSegment::new("key", name.clone(), s..s, s..e);
                k.name_anchor = pieces.first().map_or(s, |(_, r)| r.start);
                k.name = Some(pieces);
                c.open(k);
                if let Some(Frame::Map { key, key_name }) = frames.last_mut() {
                    *key = true;
                    *key_name = name;
                }
            }
            Event::Alias(_) if expecting_key(frames) => {
                // An alias used as a key (`*a : x`) is named by its source text.
                *last_end = e;
                let name = slice(text, s..e).to_string();
                let mut k = OutlineSegment::new("key", name.clone(), s..s, s..e);
                k.name = Some(vec![(0..name.len(), s..e)]);
                k.name_anchor = s;
                c.open(k);
                if let Some(Frame::Map { key, key_name }) = frames.last_mut() {
                    *key = true;
                    *key_name = name;
                }
            }
            Event::Scalar(..) | Event::Alias(_) => {
                *last_end = e;
                begin_value(frames, c, s);
                end_value(frames, c, e);
            }
            _ => {}
        }
    }
}

/// The top frame is a mapping waiting for a key.
fn expecting_key(frames: &[Frame]) -> bool {
    matches!(frames.last(), Some(Frame::Map { key: false, .. }))
}

/// A value starts at `s`. In a sequence it opens the element Segment `holder[n]` and returns
/// that name; in a mapping the open key already holds it, and its name is returned; at the top
/// of a document the name is empty, so a top-level sequence's elements are `[0]`, `[1]`…
fn begin_value<S: Sink + ?Sized>(frames: &mut [Frame], c: &mut S, s: usize) -> String {
    match frames.last_mut() {
        Some(Frame::Seq { holder, n }) => {
            let name = format!("{holder}[{n}]");
            *n += 1;
            c.open(OutlineSegment::new("element", name.clone(), s..s, s..s));
            name
        }
        Some(Frame::Map {
            key: true,
            key_name,
        }) => key_name.clone(),
        _ => String::new(),
    }
}

/// The value that began last ends at `end`: it closes the key or the element that holds it.
fn end_value<S: Sink + ?Sized>(frames: &mut [Frame], c: &mut S, end: usize) {
    match frames.last_mut() {
        Some(Frame::Map { key, .. }) if *key => {
            c.close(end);
            *key = false;
        }
        Some(Frame::Seq { .. }) => c.close(end),
        _ => {}
    }
}

/// The name pieces of a key scalar over `span` (`Name::pieces`): one verbatim piece when the
/// source is the value, plain or inside its quotes; otherwise the scalar's source decoded line
/// by line, one piece per line, so a Match maps to the line it is on; and when that decoding
/// does not give saphyr's `value`, one decoded piece over the whole span.
fn name_pieces(text: &str, span: Range<usize>, value: &str, style: ScalarStyle) -> Pieces {
    let (s, e) = (span.start, span.end);
    let raw = slice(text, s..e);
    if raw == value {
        return vec![(0..value.len(), s..e)];
    }
    // The quotes are checked before they are cut off: a span that does not start and end with
    // its quote is decoded as one piece.
    let quote = match style {
        ScalarStyle::SingleQuoted => Some((b'\'', Quote::Single)),
        ScalarStyle::DoubleQuoted => Some((b'"', Quote::Double)),
        _ => None,
    };
    let inner = quote.and_then(|(q, kind)| {
        let r = raw.as_bytes();
        (r.len() >= 2 && r[0] == q && r[r.len() - 1] == q).then_some((s + 1..e - 1, kind))
    });
    if let Some((r, _)) = &inner
        && slice(text, r.clone()) == value
    {
        return vec![(0..value.len(), r.clone())];
    }
    let decoded = match (style, inner) {
        (ScalarStyle::Plain, _) if !raw.is_empty() => Some(by_lines(text, s..e, Quote::None)),
        (_, Some((r, kind))) => Some(by_lines(text, r, kind)),
        _ => None,
    };
    match decoded {
        Some((joined, pieces)) if joined == value => pieces,
        _ => vec![(0..value.len(), s..e)],
    }
}

/// The name of a block scalar key (`? |` or `? >`) over `span`, and its pieces: the value
/// without its leading and final line breaks, its lines joined by a space, so that a Segment name
/// stays on one line (review m2-code-r2, Fable #1). One verbatim piece per non-empty source line,
/// without the block's indentation; when a line is not found in the name, one decoded piece over
/// the whole span.
fn block_name(text: &str, span: Range<usize>, value: &str) -> (String, Pieces) {
    let name = value.trim_matches('\n').replace('\n', " ");
    let (s, e) = (span.start, span.end);
    // saphyr's span starts at the first content character, after the block's indentation.
    let line_start = text
        .get(..s)
        .and_then(|t| t.rfind(['\n', '\r']))
        .map_or(0, |i| i + 1);
    let indent = s - line_start;
    let mut pieces = Vec::new();
    let mut at = 0;
    for (i, (from, line)) in break_lines(slice(text, s..e), s).into_iter().enumerate() {
        let lead = if i == 0 {
            0
        } else {
            indent.min(line.len() - line.trim_start_matches(' ').len())
        };
        let part = &line[lead..];
        if part.trim_matches([' ', '\t']).is_empty() {
            continue;
        }
        let Some(k) = name.get(at..).and_then(|rest| rest.find(part)) else {
            return (name.clone(), vec![(0..name.len(), s..e)]);
        };
        let from = from + lead;
        pieces.push((at + k..at + k + part.len(), from..from + part.len()));
        at += k + part.len();
    }
    if pieces.is_empty() {
        pieces.push((0..name.len(), s..e));
    }
    (name, pieces)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Quote {
    None,
    Single,
    Double,
}

/// Decodes a flow scalar's source `inner` (inside its quotes) line by line: each line without
/// its indentation and, before a line break, without its trailing blanks; escapes decoded;
/// lines joined by one space, an empty line by a line break, and an escaped line break (a
/// double-quoted line ending in `\`) by nothing. Returns the text and one piece per line.
fn by_lines(text: &str, inner: Range<usize>, quote: Quote) -> (String, Pieces) {
    let body = slice(text, inner.clone());
    let mut out = String::new();
    let mut pieces = Vec::new();
    let parts = break_lines(body, inner.start);
    let last = parts.len() - 1;
    let mut pending = 0;
    let mut glue = false;
    for (i, &(line_start, line)) in parts.iter().enumerate() {
        let lead = if i == 0 {
            0
        } else {
            line.len() - line.trim_start_matches([' ', '\t']).len()
        };
        let mut part = &line[lead..];
        let mut escaped_break = false;
        if i < last {
            if quote == Quote::Double && ends_in_escape(part) {
                part = &part[..part.len() - 1];
                escaped_break = true;
            } else {
                part = part.trim_end_matches([' ', '\t']);
            }
        }
        if i > 0 && i < last && part.is_empty() && !glue {
            pending += 1;
            continue;
        }
        if i > 0 {
            if glue {
                // An escaped line break joins with nothing.
            } else if pending > 0 {
                out.push_str(&"\n".repeat(pending));
            } else {
                out.push(' ');
            }
        }
        pending = 0;
        glue = escaped_break;
        let from = line_start + lead;
        let src = from..from + part.len();
        let decoded = match quote {
            Quote::None => part.to_string(),
            Quote::Single => part.replace("''", "'"),
            Quote::Double => unescape(part),
        };
        if !decoded.is_empty() {
            pieces.push((out.len()..out.len() + decoded.len(), src));
        }
        out.push_str(&decoded);
    }
    (out, pieces)
}

/// `body`'s lines without their breaks, each with its offset (`body` starts at `at`). A line
/// break is CR, LF or CRLF, as saphyr reads them.
fn break_lines(body: &str, at: usize) -> Vec<(usize, &str)> {
    let b = body.as_bytes();
    let mut out = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'\n' => {
                out.push((at + start, &body[start..i]));
                start = i + 1;
            }
            b'\r' => {
                out.push((at + start, &body[start..i]));
                if b.get(i + 1) == Some(&b'\n') {
                    i += 1;
                }
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    out.push((at + start, &body[start..]));
    out
}

/// A double-quoted line ends in an unescaped `\`: an odd run of backslashes.
fn ends_in_escape(part: &str) -> bool {
    part.bytes().rev().take_while(|&b| b == b'\\').count() % 2 == 1
}

/// The escapes of a YAML 1.2 double-quoted scalar, decoded. An unknown escape is kept as is:
/// the result then differs from saphyr's value and the name falls back to one decoded piece.
fn unescape(part: &str) -> String {
    let mut out = String::with_capacity(part.len());
    let mut chars = part.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        let Some(esc) = chars.next() else {
            out.push('\\');
            break;
        };
        let hex = |chars: &mut std::str::Chars, n: usize| -> Option<char> {
            let h: String = chars.by_ref().take(n).collect();
            u32::from_str_radix(&h, 16).ok().and_then(char::from_u32)
        };
        let decoded = match esc {
            '0' => Some('\0'),
            'a' => Some('\u{7}'),
            'b' => Some('\u{8}'),
            't' | '\t' => Some('\t'),
            'n' => Some('\n'),
            'v' => Some('\u{b}'),
            'f' => Some('\u{c}'),
            'r' => Some('\r'),
            'e' => Some('\u{1b}'),
            ' ' => Some(' '),
            '"' => Some('"'),
            '/' => Some('/'),
            '\\' => Some('\\'),
            'N' => Some('\u{85}'),
            '_' => Some('\u{a0}'),
            'L' => Some('\u{2028}'),
            'P' => Some('\u{2029}'),
            'x' => hex(&mut chars, 2),
            'u' => hex(&mut chars, 4),
            'U' => hex(&mut chars, 8),
            _ => None,
        };
        match decoded {
            Some(d) => out.push(d),
            None => {
                out.push('\\');
                out.push(esc);
            }
        }
    }
    out
}

/// Whether `prepare` gives a native cursor: yes (plan Task 10.2).
pub(crate) const NATIVE: bool = true;

/// The prepared YAML file (plan Task 10.2): one walk over the whole file records where each
/// Segment ends and whether it is hidden (a key that holds a sequence; the document while it is
/// the only one), which a Trail needs before the walk that follows the Matches reaches them.
struct YamlPrepared<'a> {
    input: crate::model::Input<'a>,
    params: &'a ::toml::Table,
    census: Vec<u64>,
}

/// Per Segment, in the order the walk opens them: where it ends, with `HIDDEN` set when it is
/// hidden. 8 bytes per Segment, the preparation of a YAML file (M5b spec §6.1).
const HIDDEN: u64 = 1 << 63;

/// The `Sink` of the preparation: `Census`'s ends and hidden marks, with the Segments open and
/// the documents seen.
#[derive(Default)]
struct Census {
    ends: Vec<u64>,
    /// Open Segments: their place in `ends` and their start.
    open: Vec<(u32, usize)>,
    docs: Vec<u32>,
}

impl Sink for Census {
    fn open(&mut self, seg: OutlineSegment) {
        let k = self.ends.len() as u32;
        if seg.segment.kind == "document" {
            self.docs.push(k);
        }
        self.ends.push(if seg.hidden { HIDDEN } else { 0 });
        self.open.push((k, seg.range.start));
    }

    fn close(&mut self, end: usize) {
        if let Some((k, start)) = self.open.pop() {
            let e = &mut self.ends[k as usize];
            *e = (*e & HIDDEN) | end.max(start) as u64;
        }
    }

    fn hide_top(&mut self) {
        if let Some(&(k, _)) = self.open.last() {
            self.ends[k as usize] |= HIDDEN;
        }
    }

    fn reveal(&mut self, kind: &str) {
        // The walk reveals documents only.
        debug_assert_eq!(kind, "document");
        for &k in &self.docs {
            self.ends[k as usize] &= !HIDDEN;
        }
    }
}

/// The prepared YAML file: the census, then a cursor that follows the Matches.
pub(crate) fn prepare<'a>(
    input: crate::model::Input<'a>,
    params: &'a ::toml::Table,
) -> anyhow::Result<Box<dyn crate::processors::Prepared + 'a>> {
    let mut c = Census::default();
    let mut w = Walker::new(input.bytes);
    let end = loop {
        match w.next() {
            Next::Event(ev, s, e) => w.apply(ev, s, e, &mut c),
            Next::Done(end) => break end,
        }
    };
    while !c.open.is_empty() {
        c.close(end);
    }
    Ok(Box::new(YamlPrepared {
        input,
        params,
        census: c.ends,
    }))
}

impl crate::processors::Prepared for YamlPrepared<'_> {
    fn cursor(&self) -> Box<dyn crate::processors::Cursor + '_> {
        Box::new(YamlCursor {
            src: self.input.bytes,
            census: &self.census,
            walk: Walker::new(self.input.bytes),
            follow: Follow::default(),
            pending: None,
            at: 0,
            lines: (0, 1),
            last: None,
            open: 0,
        })
    }

    fn names(&self) -> anyhow::Result<Vec<crate::model::Name>> {
        use crate::processors::Processor;
        super::Keypath.names(&self.input, self.params)
    }

    fn streams(&self) -> bool {
        true
    }
}

/// The `Sink` of the cursor's walk: the Segments open where the walk is, and those closed at an
/// end past the offset asked (a scalar that holds the offset closes its key at its own end), in
/// the order they opened, each with its place in the census.
#[derive(Default)]
struct Follow {
    segs: Vec<(u32, OutlineSegment, bool)>,
    next: u32,
    /// The offset asked: a Segment that ends at or before it is dropped as it closes.
    target: usize,
}

impl Sink for Follow {
    fn open(&mut self, seg: OutlineSegment) {
        self.segs.push((self.next, seg, false));
        self.next += 1;
    }

    fn close(&mut self, end: usize) {
        let Some(i) = self.segs.iter().rposition(|(_, _, closed)| !closed) else {
            return;
        };
        if end.max(self.segs[i].1.range.start) <= self.target {
            self.segs.remove(i);
        } else {
            self.segs[i].2 = true;
        }
    }

    // Hidden marks come from the census, which knows how the file ends.
    fn hide_top(&mut self) {}

    fn reveal(&mut self, _: &str) {}
}

/// The Trails of a YAML file in file order.
struct YamlCursor<'s> {
    src: &'s [u8],
    census: &'s [u64],
    walk: Walker<'s>,
    follow: Follow,
    /// The event read and not applied yet: it starts past the last offset asked.
    pending: Option<(Event<'s>, usize, usize)>,
    at: usize,
    /// The line of byte `.0`, the last Segment start counted.
    lines: (usize, u64),
    /// The last Trail given, by the census places of its Segments.
    last: Option<(Vec<u32>, crate::model::SharedTrail)>,
    /// Where the outermost Segment holding the last offset starts (`Cursor::open_from`).
    open: usize,
}

impl YamlCursor<'_> {
    fn end_of(&self, k: u32) -> usize {
        (self.census[k as usize] & !HIDDEN) as usize
    }

    /// The walk up to `byte`: every event that starts at or before it.
    fn reach(&mut self, byte: usize) {
        self.follow.target = byte;
        // What closed at an end now passed is dropped.
        let census = self.census;
        self.follow
            .segs
            .retain(|(k, _, closed)| !closed || (census[*k as usize] & !HIDDEN) as usize > byte);
        loop {
            let (ev, s, e) = match self.pending.take() {
                Some(p) => p,
                None => match self.walk.next() {
                    Next::Event(ev, s, e) => (ev, s, e),
                    // Where the walk ends, the census closed what is open.
                    Next::Done(_) => return,
                },
            };
            if s > byte {
                self.pending = Some((ev, s, e));
                return;
            }
            self.walk.apply(ev, s, e, &mut self.follow);
        }
    }

    /// The line of `byte`, counted on from the last one asked.
    fn line_of(&mut self, byte: usize) -> u64 {
        let (at, line) = self.lines;
        let line = if byte >= at {
            line + memchr::memchr_iter(b'\n', &self.src[at..byte]).count() as u64
        } else {
            line - memchr::memchr_iter(b'\n', &self.src[byte..at]).count() as u64
        };
        self.lines = (byte, line);
        line
    }
}

impl crate::processors::Cursor for YamlCursor<'_> {
    fn open_from(&mut self) -> Option<usize> {
        Some(self.open)
    }

    fn trail_at(&mut self, byte: usize) -> anyhow::Result<crate::model::SharedTrail> {
        if byte < self.at {
            // An offset before the last one walks again from the start.
            self.walk = Walker::new(self.src);
            self.follow = Follow::default();
            self.pending = None;
        }
        self.at = byte;
        self.reach(byte);
        let census = self.census;
        self.open = self
            .follow
            .segs
            .iter()
            // A hidden Segment, the document of a file of one, ends no Trail (code review r2,
            // Fable #2).
            .filter(|(k, seg, _)| {
                let c = census[*k as usize];
                c & HIDDEN == 0 && seg.range.start <= byte && (c & !HIDDEN) as usize > byte
            })
            .map(|(_, seg, _)| seg.range.start)
            .fold(byte, usize::min);
        // The Segments that hold `byte`, in the order of `Outline::build`: by start, the longer
        // first, and of two equal ranges the inner one, which closed first, first.
        let mut held: Vec<(u32, usize, usize)> = self
            .follow
            .segs
            .iter()
            .filter(|(k, seg, _)| {
                let c = self.census[*k as usize];
                c & HIDDEN == 0
                    && seg.range.start <= byte
                    && (c & !HIDDEN) as usize > byte
                    && !seg.label.contains(&byte)
            })
            .map(|(k, seg, _)| (*k, seg.range.start, self.end_of(*k)))
            .collect();
        held.sort_by(|a, b| a.1.cmp(&b.1).then(b.2.cmp(&a.2)).then(b.0.cmp(&a.0)));
        let keys: Vec<u32> = held.iter().map(|h| h.0).collect();
        if let Some((k, t)) = &self.last
            && *k == keys
        {
            return Ok(t.clone());
        }
        let mut trail = Vec::with_capacity(held.len());
        for &(k, start, end) in &held {
            let known = self
                .last
                .as_ref()
                .and_then(|(ks, t)| ks.iter().position(|&q| q == k).map(|j| t[j].clone()));
            let seg = match known {
                Some(seg) => seg,
                None => {
                    let mut seg = self
                        .follow
                        .segs
                        .iter()
                        .find(|(q, _, _)| *q == k)
                        .expect("a held Segment")
                        .1
                        .segment
                        .clone();
                    seg.line = self.line_of(start);
                    let last = end.saturating_sub(1).max(start).min(self.src.len());
                    seg.end_line = seg.line
                        + memchr::memchr_iter(b'\n', &self.src[start..last]).count() as u64;
                    seg
                }
            };
            trail.push(seg);
        }
        let t: crate::model::SharedTrail = std::rc::Rc::from(trail);
        self.last = Some((keys, t.clone()));
        Ok(t)
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::needles;
    use crate::processors::Processor;
    use std::path::Path;

    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/data")
                .join(name),
        )
        .unwrap()
    }

    #[test]
    fn yaml_keys_sequences_block_scalars_and_merge_keys() {
        assert_eq!(
            needles(&fixture("config.yaml"), "yaml"),
            vec![
                vec!["server", "ports[2]", "host"], // spec §11 #4
                vec!["server", "motd"],
                vec!["worker", "tag"],
            ]
        );
    }

    #[test]
    fn character_offsets_become_bytes() {
        // "ñandú" is 5 characters and 7 bytes: a Trail after it must still be right.
        let src = "a: ñandú\nb:\n  c: needle\n".as_bytes();
        assert_eq!(needles(src, "yaml"), vec![vec!["b", "c"]]);
    }

    #[test]
    fn a_lone_carriage_return_is_a_line_break() {
        // saphyr breaks lines at CR, LF and CRLF; rg's lines are LF only (review m2-code-r1,
        // Codex #1). The key after the CR must still be found, at its own bytes.
        assert_eq!(
            needles(b"a:\r  x: needle\nb: value\n", "yaml"),
            vec![vec!["a", "x"]]
        );
        assert_eq!(
            needles("a:\r  \"x\": needle\néééééééé: value\n".as_bytes(), "yaml"),
            vec![vec!["a", "x"]]
        );
        assert_eq!(
            needles("ñ: 1\r\nb:\r  c: needle\rd: 2\n".as_bytes(), "yaml"),
            vec![vec!["b", "c"]]
        );
    }

    #[test]
    fn names_point_at_their_own_bytes_across_line_breaks() {
        let src = "é: 1\r'ü': 2\r\n\"ñ x\": needle\r? multi\r  line\r: x\n".as_bytes();
        let input = crate::model::Input {
            bytes: src,
            path: Path::new("x"),
            format: "yaml",
            disk: None,
        };
        let names = super::super::Keypath
            .names(&input, &super::super::tests::params("yaml"))
            .unwrap();
        for n in &names {
            let got: String = n
                .pieces
                .iter()
                .map(|(_, r)| std::str::from_utf8(&src[r.clone()]).unwrap())
                .collect();
            assert_eq!(
                got.replace("multiline", "multi line"),
                n.text,
                "{:?}",
                n.pieces
            );
        }
        let multi = names.iter().find(|n| n.text == "multi line").unwrap();
        assert_eq!(
            multi.pieces.len(),
            2,
            "one piece per line: {:?}",
            multi.pieces
        );
        assert_eq!(names.len(), 4);
    }

    #[test]
    fn documents_appear_only_when_there_are_two() {
        assert_eq!(
            needles(&fixture("multi.yaml"), "yaml"),
            vec![vec!["document 1", "first"], vec!["document 2", "second[0]"]]
        );
        assert_eq!(needles(b"only: needle\n", "yaml"), vec![vec!["only"]]);
    }

    #[test]
    fn a_syntax_error_keeps_the_trails_before_it() {
        // A block sequence entry where a key is expected: saphyr-parser 0.2.2 stops at line 3
        // ("did not find expected key"), verified in the probe.
        let src = b"a:\n  b: needle\n- x\nd: needle\n";
        assert_eq!(needles(src, "yaml"), vec![vec!["a", "b"], vec![]]);
    }

    #[test]
    fn past_the_tree_sitter_yaml_row_limit() {
        // tree-sitter-yaml fails over 32 768 lines (spec §4.3); the native parser does not.
        let mut s = String::new();
        for i in 0..40_000 {
            s.push_str(&format!("k{i}: v\n"));
        }
        s.push_str("last:\n  deep: needle\n");
        assert_eq!(needles(s.as_bytes(), "yaml"), vec![vec!["last", "deep"]]);
    }

    #[test]
    fn flow_collections_end_at_their_bracket() {
        let src = b"a: {}\nb: needle\n";
        let o = super::super::outline_for(src, super::super::Syntax::Yaml, &[3]);
        let a = o.segments.iter().find(|s| s.segment.name == "a").unwrap();
        assert_eq!(a.range, 0..5);
    }

    #[test]
    fn a_multi_line_key_has_one_piece_per_line() {
        let src = b"? multi\n  line key\n: needle\n";
        let input = crate::model::Input {
            bytes: src,
            path: Path::new("x"),
            format: "yaml",
            disk: None,
        };
        let names = super::super::Keypath
            .names(&input, &super::super::tests::params("yaml"))
            .unwrap();
        let n = names
            .iter()
            .find(|n| n.text == "multi line key")
            .expect("the key");
        let lines: Vec<&str> = n
            .pieces
            .iter()
            .map(|(_, r)| std::str::from_utf8(&src[r.clone()]).unwrap())
            .collect();
        assert_eq!(lines, ["multi", "line key"]);
    }

    #[test]
    fn a_block_scalar_key_is_named_on_one_line() {
        // Review m2-code-r2, Fable #1: a `? |` or `? >` key kept its line breaks in the name,
        // which split the one-line output of its Matches. The name is the value without its
        // final break, its lines joined by a space, with one verbatim piece per line.
        for (src, name, lines) in [
            (
                "a: 1\n? |\n  ñ multi\n  line\n: needle\n",
                "ñ multi line",
                &["ñ multi", "line"][..],
            ),
            (
                "? >\n  folded key\n: needle\n",
                "folded key",
                &["folded key"],
            ),
            (
                "? >\n  folded\n  key\n\n  more\n    ind\n: needle\n",
                "folded key more   ind",
                &["folded", "key", "more", "  ind"],
            ),
            ("? |+ # kept\n  keep\n\n\n: needle\n", "keep", &["keep"]),
            ("? |-\n  strip\n: needle\n", "strip", &["strip"]),
            ("? |\r\n  a\r\n  b\r\n: needle\n", "a b", &["a", "b"]),
            ("? |\n\n  a\n\n  b\n: needle\n", "a  b", &["a", "b"]),
            (
                "? |2\n    two\n   three\n: needle\n",
                "  two  three",
                &["  two", " three"],
            ),
            (
                "x:\n  ? |\n    deep\n    key\n  : needle\n",
                "deep key",
                &["deep", "key"],
            ),
        ] {
            let b = src.as_bytes();
            let input = crate::model::Input {
                bytes: b,
                path: Path::new("x"),
                format: "yaml",
                disk: None,
            };
            let names = super::super::Keypath
                .names(&input, &super::super::tests::params("yaml"))
                .unwrap();
            let n = names
                .iter()
                .find(|n| n.text != "a" && n.text != "x")
                .unwrap_or_else(|| panic!("{src:?}: {names:?}"));
            assert_eq!(n.text, name, "{src:?}");
            let got: Vec<&str> = n
                .pieces
                .iter()
                .map(|(t, r)| {
                    let s = std::str::from_utf8(&b[r.clone()]).unwrap();
                    assert_eq!(&n.text[t.clone()], s, "{src:?}: {:?}", n.pieces);
                    s
                })
                .collect();
            assert_eq!(got, lines, "{src:?}");
            let trail = needles(b, "yaml");
            assert_eq!(trail.last().unwrap().last().unwrap(), name, "{src:?}");
        }
    }

    #[test]
    fn an_escaped_line_break_keeps_each_part_on_its_line() {
        // saphyr gives "foobar": no space marks the join, so the pieces come from the source.
        let src = b"? \"foo\\\n  bar\"\n: needle\n";
        let input = crate::model::Input {
            bytes: src,
            path: Path::new("x"),
            format: "yaml",
            disk: None,
        };
        let names = super::super::Keypath
            .names(&input, &super::super::tests::params("yaml"))
            .unwrap();
        let n = names.iter().find(|n| n.text == "foobar").expect("the key");
        let line_of =
            |r: &std::ops::Range<usize>| src[..r.start].iter().filter(|&&b| b == b'\n').count() + 1;
        let bar = n
            .pieces
            .iter()
            .find(|(t, _)| t.end == "foobar".len())
            .unwrap();
        assert_eq!(line_of(&bar.1), 2, "{:?}", n.pieces);
        assert_eq!(n.pieces.len(), 2);
    }

    #[test]
    fn deep_flow_nesting_is_a_syntax_error_not_a_crash() {
        // saphyr-parser stops at its recursion limit with an error (probe: "recursion limit
        // exceeded at byte 258"); the Trails before it stay.
        let mut s = String::from("a: needle\nb: ");
        s.push_str(&"[".repeat(100_000));
        s.push_str(&"]".repeat(100_000));
        s.push('\n');
        assert_eq!(needles(s.as_bytes(), "yaml"), vec![vec!["a"]]);
    }

    #[test]
    fn complex_keys_are_named_by_their_text() {
        let src = b"? [a, b]\n: needle\n";
        assert_eq!(needles(src, "yaml"), vec![vec!["[a, b]"]]);
    }

    #[test]
    fn empty_values_alias_keys_and_quoted_names() {
        let src = b"a:\nb: needle\nbase: &x k\n? *x\n: needle\n'it''s': needle\n";
        assert_eq!(
            needles(src, "yaml"),
            vec![vec!["b"], vec!["*x"], vec!["it's"]]
        );
    }

    #[test]
    fn yaml_second_document_is_known_before_the_first_trail_prints() {
        use crate::processors::{Processor, keypath::Keypath};
        let src = b"a: needle\n---\nb: needle\n";
        let params: ::toml::Table = ::toml::from_str("syntax = 'yaml'").unwrap();
        let input = crate::model::Input {
            bytes: src,
            path: std::path::Path::new("x.yaml"),
            format: "yaml",
            disk: None,
        };
        let anchors: Vec<_> = memchr::memmem::find_iter(src, b"needle").collect();
        let expected = Keypath.trails(&input, &params, &anchors).unwrap();
        let p = super::prepare(input, &params).unwrap();
        assert!(p.streams());
        let mut c = p.cursor();
        for (at, trail) in anchors.into_iter().zip(expected) {
            assert_eq!(c.trail_at(at).unwrap().as_ref(), trail.as_slice());
        }
    }

    /// The cursor against `Keypath::trails` at every byte of `src`, then once more from the
    /// start.
    fn every_byte(src: &[u8], syntax: &str) {
        use crate::processors::{Processor, keypath::Keypath};
        let params: ::toml::Table = ::toml::from_str(&format!("syntax = '{syntax}'")).unwrap();
        let input = crate::model::Input {
            bytes: src,
            path: std::path::Path::new("x"),
            format: syntax,
            disk: None,
        };
        let anchors: Vec<usize> = (0..src.len()).collect();
        let expected = Keypath.trails(&input, &params, &anchors).unwrap();
        let p = super::prepare(input, &params).unwrap();
        let mut c = p.cursor();
        for (&at, t) in anchors.iter().zip(&expected) {
            assert_eq!(c.trail_at(at).unwrap().as_ref(), t.as_slice(), "byte {at}");
        }
        if let Some(t) = expected.first() {
            assert_eq!(c.trail_at(0).unwrap().as_ref(), t.as_slice());
        }
    }

    #[test]
    fn the_yaml_cursor_gives_the_trails_of_every_byte() {
        every_byte(&fixture("config.yaml"), "yaml");
        every_byte(&fixture("multi.yaml"), "yaml");
        // A mapping in a sequence element starts with its first key; flow collections; a
        // complex key; block scalars; an alias key; documents ended by `...`; not ASCII.
        every_byte(
            "- a: 1\n  b: [x, {y: z}]\n- - c\n  - d: |\n      text\n      more\n? [k, l]\n: v\n"
                .as_bytes(),
            "yaml",
        );
        every_byte(
            "--- \nk: &x é\n*x : w\n...\n---\nñ: >\n  folded\n  lines\n\n# end\n".as_bytes(),
            "yaml",
        );
        // A syntax error: what is open ends there.
        every_byte(b"a:\n  b: 1\n  c: [2\nd: 3\n", "yaml");
        every_byte(b"a: 1\nb: needle\n", "yaml");
    }
}
