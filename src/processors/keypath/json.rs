//! The JSON walker of `keypath` (spec §5.2): one iterative pass over the bytes with an explicit
//! stack, so nesting never touches the call stack. Comments (`//`, `/* */`) and trailing commas
//! are accepted (JSONC); everything else must be JSON: numbers, `true`, `false`, `null`, and
//! strings with valid escapes and no control characters. Several top-level values (JSON Lines)
//! are read in turn. Past `MAX_DEPTH` nested containers no more Segments open, so a deeper
//! Match gets the Trail of the deepest one (M2 plan, D34).
use super::collect::Collector;
use crate::model::{Name, Segment, SharedTrail};
use crate::processors::{Cursor, Prepared};
use crate::trail::{OutlineSegment, names_of};
use std::collections::VecDeque;
use std::ops::Range;

/// `Name::pieces`: (range in the name's text, range in the Source).
type Pieces = Vec<(Range<usize>, Range<usize>)>;

pub const MAX_DEPTH: usize = 256;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Want {
    KeyOrEnd,
    Colon,
    Value,
    ValueOrEnd,
    CommaOrEnd,
}

/// One open object or array.
struct Frame {
    object: bool,
    want: Want,
    /// The next element's index (arrays).
    n: usize,
    /// The name an array's elements extend: `ports` → `ports[0]`. Empty past MAX_DEPTH.
    holder: String,
    /// The current member (key or element), when it is a Segment.
    member: Option<Member>,
    /// The key read, waiting for its value: its range (quotes included), and whether it had
    /// escapes.
    key: Option<(Range<usize>, bool)>,
}

/// A member's Segment, built when it closes and only if the Collector keeps it: most members
/// of a big file hold no anchor, and cost no allocation (M2 plan, Task 11.4).
struct Member {
    start: usize,
    name: MemberName,
    /// Its value is an array (spec §5.2).
    hidden: bool,
    /// Its place among the members the walk made, in file order: a Segment's identity for the
    /// cursor (`JsonCursor`).
    id: u64,
    /// Its place among the members whose value is an object or an array, in file order: the
    /// key of its end, which the cursor learns ahead (`JsonCursor::ahead`).
    container: Option<u64>,
    /// The cursor's, filled when first asked: the first line, and the end with its last line.
    line: u64,
    end: Option<(usize, u64)>,
    text: Option<String>,
}

enum MemberName {
    /// The key's contents, without quotes, and whether they have escapes.
    Key(Range<usize>, bool),
    /// The element's index in its array.
    Element(usize),
}

impl Member {
    /// The member's name: the decoded key, or `holder[n]`.
    fn name(&self, src: &[u8], holder: &str) -> String {
        match &self.name {
            MemberName::Key(inner, escaped) => decode(src, inner.clone(), *escaped).0,
            MemberName::Element(n) => format!("{holder}[{n}]"),
        }
    }

    /// Closes the member at `end` (exclusive): its Segment, if the Collector keeps it.
    fn close(self, c: &mut Collector, src: &[u8], holder: &str, end: usize) {
        if !c.keeps(self.start, end) {
            return;
        }
        let mut seg = match &self.name {
            MemberName::Key(inner, escaped) => {
                let (name, pieces) = decode(src, inner.clone(), *escaped);
                let label = inner.start - 1..inner.end + 1;
                let mut seg = OutlineSegment::new("key", name, self.start..self.start, label);
                seg.name = Some(pieces);
                seg.name_anchor = inner.start;
                seg
            }
            MemberName::Element(n) => OutlineSegment::new(
                "element",
                format!("{holder}[{n}]"),
                self.start..self.start,
                self.start..self.start,
            ),
        };
        seg.hidden = self.hidden;
        c.closed(seg, end);
    }
}

#[derive(Clone, Copy)]
struct Scan<'s> {
    s: &'s [u8],
    i: usize,
}

impl Scan<'_> {
    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    /// Whitespace and comments.
    fn ws(&mut self) {
        loop {
            while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
                self.i += 1;
            }
            let rest = &self.s[self.i.min(self.s.len())..];
            if rest.starts_with(b"//") {
                self.i += memchr::memchr(b'\n', rest).unwrap_or(rest.len());
            } else if rest.starts_with(b"/*") {
                self.i += memchr::memmem::find(&rest[2..], b"*/").map_or(rest.len(), |k| k + 4);
            } else {
                return;
            }
        }
    }

    /// A string at `i` (on its `"`): its range, quotes included, and whether it has escapes.
    fn string(&mut self) -> Result<(Range<usize>, bool), usize> {
        let start = self.i;
        self.i += 1;
        let mut escaped = false;
        loop {
            match self.peek() {
                None => return Err(self.s.len()),
                Some(b'"') => {
                    self.i += 1;
                    return Ok((start..self.i, escaped));
                }
                Some(b'\\') => {
                    escaped = true;
                    match self.s.get(self.i + 1) {
                        Some(b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't') => self.i += 2,
                        Some(b'u')
                            if self
                                .s
                                .get(self.i + 2..self.i + 6)
                                .is_some_and(|h| h.iter().all(u8::is_ascii_hexdigit)) =>
                        {
                            self.i += 6
                        }
                        _ => return Err(self.i),
                    }
                }
                Some(b) if b < 0x20 => return Err(self.i),
                Some(_) => self.i += 1,
            }
        }
    }

    fn digits(&mut self) -> bool {
        let start = self.i;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.i += 1;
        }
        self.i > start
    }

    /// A scalar ends at a delimiter: `truex` and `01` are errors.
    fn delimited(&self) -> Result<(), usize> {
        match self.peek() {
            None | Some(b',' | b'}' | b']' | b' ' | b'\t' | b'\n' | b'\r' | b'/') => Ok(()),
            _ => Err(self.i),
        }
    }

    /// A number (RFC 8259 grammar), `true`, `false` or `null`.
    fn scalar(&mut self) -> Result<(), usize> {
        for lit in [&b"true"[..], b"false", b"null"] {
            if self.s[self.i..].starts_with(lit) {
                self.i += lit.len();
                return self.delimited();
            }
        }
        let start = self.i;
        if self.peek() == Some(b'-') {
            self.i += 1;
        }
        match self.peek() {
            Some(b'0') => self.i += 1,
            Some(b'1'..=b'9') => {
                self.digits();
            }
            _ => return Err(start),
        }
        if self.peek() == Some(b'.') {
            self.i += 1;
            if !self.digits() {
                return Err(self.i);
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.i += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.i += 1;
            }
            if !self.digits() {
                return Err(self.i);
            }
        }
        self.delimited()
    }
}

/// The text of a JSON string's contents at `inner` in `src`, escapes decoded (lossy on invalid
/// UTF-8), and its pieces (`Name::pieces`): one piece over the contents, verbatim or decoded (an
/// escape always decodes to fewer bytes than it takes). With invalid UTF-8, whose U+FFFD takes
/// more bytes, the pieces split at every escape and invalid run, so that the decoded text never
/// sums to the contents' length in one piece that looks verbatim (fuzz, review m2-code-r1).
/// `string` already checked the escapes.
pub(super) fn decode(src: &[u8], inner: Range<usize>, escaped: bool) -> (String, Pieces) {
    if !escaped && let Ok(text) = std::str::from_utf8(&src[inner.clone()]) {
        return (text.to_string(), vec![(0..text.len(), inner)]);
    }
    let raw = &src[inner.clone()];
    let valid = std::str::from_utf8(raw).is_ok();
    let mut name = Decoded {
        base: inner.start,
        text: String::new(),
        pieces: Vec::new(),
    };
    let mut run = 0;
    let mut i = 0;
    while i < raw.len() {
        if raw[i] != b'\\' || i + 1 >= raw.len() {
            i += 1;
            continue;
        }
        name.raw(raw, run, i);
        let esc = i;
        let c = raw[i + 1];
        i += 2;
        let decoded = match c {
            b'n' => '\n',
            b't' => '\t',
            b'r' => '\r',
            b'b' => '\u{8}',
            b'f' => '\u{c}',
            b'u' => {
                let hex = |at: usize| {
                    raw.get(at..at + 4)
                        .and_then(|h| std::str::from_utf8(h).ok())
                        .and_then(|h| u32::from_str_radix(h, 16).ok())
                };
                match hex(i) {
                    None => '\u{fffd}',
                    Some(hi) => {
                        i += 4;
                        let cp = if (0xD800..0xDC00).contains(&hi)
                            && raw.get(i..i + 2) == Some(b"\\u")
                        {
                            match hex(i + 2) {
                                Some(lo) if (0xDC00..0xE000).contains(&lo) => {
                                    i += 6;
                                    0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00)
                                }
                                _ => hi,
                            }
                        } else {
                            hi
                        };
                        char::from_u32(cp).unwrap_or('\u{fffd}')
                    }
                }
            }
            other => other as char,
        };
        name.piece(decoded.encode_utf8(&mut [0; 4]), esc, i);
        run = i;
    }
    name.raw(raw, run, raw.len());
    if valid {
        let len = name.text.len();
        return (name.text, vec![(0..len, inner)]);
    }
    (name.text, name.pieces)
}

/// A JSON name being decoded, piece by piece; `base` is where its contents start.
struct Decoded {
    base: usize,
    text: String,
    pieces: Pieces,
}

impl Decoded {
    /// `text` decodes `raw[from..to]`.
    fn piece(&mut self, text: &str, from: usize, to: usize) {
        if from < to || !text.is_empty() {
            let t = self.text.len();
            self.pieces
                .push((t..t + text.len(), self.base + from..self.base + to));
            self.text.push_str(text);
        }
    }

    /// The raw bytes `raw[from..to]`: each valid run verbatim, each invalid one a U+FFFD.
    fn raw(&mut self, raw: &[u8], from: usize, to: usize) {
        let mut at = from;
        for chunk in raw[from..to].utf8_chunks() {
            let v = chunk.valid();
            if !v.is_empty() {
                self.piece(v, at, at + v.len());
            }
            at += v.len();
            let bad = chunk.invalid();
            if !bad.is_empty() {
                self.piece("\u{fffd}", at, at + bad.len());
                at += bad.len();
            }
        }
    }
}

/// Starts the value at `sc.i`: pushes a Frame for a container, or reads a scalar whole.
fn begin(
    sc: &mut Scan,
    stack: &mut Vec<Frame>,
    holder: String,
    last_end: &mut usize,
) -> Result<(), usize> {
    let frame = |object, want, holder| Frame {
        object,
        want,
        n: 0,
        holder,
        member: None,
        key: None,
    };
    match sc.peek() {
        Some(b'{') => {
            sc.i += 1;
            stack.push(frame(true, Want::KeyOrEnd, String::new()));
        }
        Some(b'[') => {
            sc.i += 1;
            stack.push(frame(false, Want::ValueOrEnd, holder));
        }
        Some(b'"') => {
            sc.string()?;
            *last_end = sc.i;
        }
        Some(_) => {
            sc.scalar()?;
            *last_end = sc.i;
        }
        None => return Err(sc.i),
    }
    Ok(())
}

/// Walks `src`; returns where it stopped: `src.len()`, or the byte of the first syntax error,
/// where the members still open close.
pub fn walk(src: &[u8], c: &mut Collector) -> usize {
    let mut w = Walker::new(src);
    let mut close = |m: Member, holder: &str, end: usize| m.close(c, src, holder, end);
    let end = loop {
        if let Some(end) = w.step(&mut close) {
            break end;
        }
    };
    w.close_open(end, &mut close);
    end
}

/// The walk, one token at a time (plan Task 9.4): `walk` runs it to its end, and the cursor
/// (`JsonCursor`) only as far as each Match.
struct Walker<'s> {
    sc: Scan<'s>,
    stack: Vec<Frame>,
    /// Members that are Segments, open: at most MAX_DEPTH.
    open: usize,
    /// End of the last complete value: where its member's Segment closes.
    last_end: usize,
    /// Members made so far, and those of them whose value is an object or an array.
    members: u64,
    containers: u64,
    /// Where the walk stopped, once it did (`walk`).
    done: Option<usize>,
}

impl<'s> Walker<'s> {
    fn new(src: &'s [u8]) -> Self {
        Walker {
            sc: Scan { s: src, i: 0 },
            stack: Vec::new(),
            open: 0,
            last_end: 0,
            members: 0,
            containers: 0,
            done: None,
        }
    }

    /// Reads the next token, calling `close` for each member it closes. Once the walk is over,
    /// where it stopped: `src.len()`, or the byte of the first syntax error. The members still
    /// open then are closed by `close_open`.
    fn step(&mut self, close: &mut impl FnMut(Member, &str, usize)) -> Option<usize> {
        if self.done.is_none() {
            self.done = self.token(close);
        }
        self.done
    }

    /// Closes what is still open at `end`, innermost first, as the walk ends.
    fn close_open(&mut self, end: usize, close: &mut impl FnMut(Member, &str, usize)) {
        for f in self.stack.iter_mut().rev() {
            if let Some(m) = f.member.take() {
                close(m, &f.holder, end);
            }
        }
    }

    fn token(&mut self, close: &mut impl FnMut(Member, &str, usize)) -> Option<usize> {
        let src = self.sc.s;
        let sc = &mut self.sc;
        let stack = &mut self.stack;
        sc.ws();
        let Some(k) = stack.len().checked_sub(1) else {
            if sc.i >= src.len() {
                return Some(src.len());
            }
            if let Err(at) = begin(sc, stack, String::new(), &mut self.last_end) {
                return Some(at.min(src.len()));
            }
            return None;
        };
        let (object, want) = (stack[k].object, stack[k].want);
        let Some(b) = sc.peek() else {
            return Some(src.len());
        };
        match (want, b) {
            (Want::KeyOrEnd, b'}') | (Want::ValueOrEnd, b']') => {
                sc.i += 1;
                self.last_end = sc.i;
                stack.pop();
            }
            (Want::CommaOrEnd, b',') => {
                sc.i += 1;
                let f = &mut stack[k];
                if let Some(m) = f.member.take() {
                    close(m, &f.holder, self.last_end);
                    self.open -= 1;
                }
                f.want = if object {
                    Want::KeyOrEnd
                } else {
                    Want::ValueOrEnd
                };
            }
            (Want::CommaOrEnd, b'}') if object => {
                sc.i += 1;
                let f = stack.pop().expect("k is the top");
                if let Some(m) = f.member {
                    close(m, &f.holder, self.last_end);
                    self.open -= 1;
                }
                self.last_end = sc.i;
            }
            (Want::CommaOrEnd, b']') if !object => {
                sc.i += 1;
                let f = stack.pop().expect("k is the top");
                if let Some(m) = f.member {
                    close(m, &f.holder, self.last_end);
                    self.open -= 1;
                }
                self.last_end = sc.i;
            }
            (Want::KeyOrEnd, b'"') => {
                let (r, escaped) = match sc.string() {
                    Ok(x) => x,
                    Err(at) => return Some(at.min(src.len())),
                };
                stack[k].key = Some((r, escaped));
                stack[k].want = Want::Colon;
            }
            (Want::Colon, b':') => {
                sc.i += 1;
                stack[k].want = Want::Value;
            }
            (Want::Value | Want::ValueOrEnd, _) => {
                let f = &mut stack[k];
                let (start, name) = if object {
                    let (r, escaped) = f.key.take().expect("Want::Value follows a key");
                    (r.start, MemberName::Key(r.start + 1..r.end - 1, escaped))
                } else {
                    f.n += 1;
                    (sc.i, MemberName::Element(f.n - 1))
                };
                // Only an array's elements extend the member's name.
                let array = b == b'[';
                let mut holder = String::new();
                if self.open < MAX_DEPTH {
                    let container = matches!(b, b'{' | b'[').then(|| {
                        self.containers += 1;
                        self.containers - 1
                    });
                    let member = Member {
                        start,
                        name,
                        hidden: array,
                        id: self.members,
                        container,
                        line: 0,
                        end: None,
                        text: None,
                    };
                    self.members += 1;
                    if array {
                        holder = member.name(src, &f.holder);
                    }
                    f.member = Some(member);
                    self.open += 1;
                }
                f.want = Want::CommaOrEnd;
                if let Err(at) = begin(sc, stack, holder, &mut self.last_end) {
                    return Some(at.min(src.len()));
                }
            }
            _ => return Some(sc.i),
        }
        None
    }
}

/// A JSON file prepared for its cursor (plan Task 9.4): the walk runs once, as far as each
/// Match, and what it needs ahead (where an object or an array that holds a Match ends) is
/// learned by a second walk that runs only as far as that end.
pub(crate) struct JsonPrepared<'a> {
    src: &'a [u8],
}

pub(crate) fn prepare(src: &[u8]) -> JsonPrepared<'_> {
    JsonPrepared { src }
}

impl Prepared for JsonPrepared<'_> {
    fn cursor(&self) -> Box<dyn Cursor + '_> {
        Box::new(JsonCursor::new(self.src))
    }

    fn names(&self) -> anyhow::Result<Vec<Name>> {
        let mut c = Collector::for_names();
        let end = walk(self.src, &mut c);
        Ok(names_of(&c.finish(end, self.src)))
    }

    fn streams(&self) -> bool {
        true
    }
}

/// The Trails of a JSON file in file order (plan Task 9.4). `main` walks to each Match; the
/// Segments of its Trail are the members open there. A member whose value is still open ends
/// where `ahead`, a second walk, finds it closing: `ends` keeps those ends from the open
/// member of `main` that is first in file order, so a JSON Lines file keeps one value's, and
/// only one object or array that holds the whole file keeps an end per object or array in it
/// (8 bytes each). Each Segment's name and lines are made once.
struct JsonCursor<'s> {
    src: &'s [u8],
    main: Walker<'s>,
    ahead: Walker<'s>,
    /// The end of each container member `ahead` closed, by its place from `base` on;
    /// `usize::MAX` while unknown.
    ends: VecDeque<usize>,
    base: u64,
    /// The line of byte `.0`, the last member start counted.
    lines: (usize, u64),
    last_at: usize,
    /// The last Trail given, by the ids of its members.
    last: Option<(Vec<u64>, SharedTrail)>,
    ids: Vec<u64>,
}

impl<'s> JsonCursor<'s> {
    fn new(src: &'s [u8]) -> Self {
        JsonCursor {
            src,
            main: Walker::new(src),
            ahead: Walker::new(src),
            ends: VecDeque::new(),
            base: 0,
            lines: (0, 1),
            last_at: 0,
            last: None,
            ids: Vec::new(),
        }
    }

    /// `main` up to `at`: every token that starts at or before it, and a key that starts at or
    /// before it, with its value's first token, so that its member is made.
    fn reach(&mut self, at: usize) {
        let w = &mut self.main;
        while w.done.is_none() {
            w.sc.ws();
            if w.sc.i > at {
                break;
            }
            w.step(&mut |_, _, _| {});
        }
        let Some(k) = w.stack.len().checked_sub(1) else {
            return;
        };
        if !matches!(w.stack[k].want, Want::Colon | Want::Value)
            || w.stack[k].key.as_ref().is_none_or(|(r, _)| r.start > at)
        {
            return;
        }
        // The colon and the value's first token close nothing.
        while w.done.is_none() && matches!(w.stack[k].want, Want::Colon | Want::Value) {
            w.step(&mut |_, _, _| {});
        }
    }

    /// Where the member of frame `k` of `main`, open at `main`'s place, ends.
    fn end_of(&mut self, k: usize) -> usize {
        if let Some(e) = self.main.done {
            return e;
        }
        let top = k + 1 == self.main.stack.len();
        let m = self.main.stack[k].member.as_ref().expect("a member");
        match m.container {
            // Its value is open: its end is ahead.
            Some(cid) if !top => self.ahead_end(cid),
            // Its value is complete: it ends with it, before a comma or the closing bracket;
            // else the walk stops at the next token (a syntax error) or at the end of the file,
            // where it closes.
            _ => {
                let mut sc = self.main.sc;
                sc.ws();
                let object = self.main.stack[k].object;
                match sc.peek() {
                    Some(b',') => self.main.last_end,
                    Some(b'}') if object => self.main.last_end,
                    Some(b']') if !object => self.main.last_end,
                    Some(_) => sc.i,
                    None => self.src.len(),
                }
            }
        }
    }

    /// The end of container member `cid`, from `ahead`, which walks on until it closes.
    fn ahead_end(&mut self, cid: u64) -> usize {
        loop {
            if let Some(&e) = self.ends.get((cid - self.base) as usize)
                && e != usize::MAX
            {
                return e;
            }
            if let Some(e) = self.ahead.done {
                // Closed by `close_open` when it stopped: never unknown past it.
                return e;
            }
            let (ends, base) = (&mut self.ends, self.base);
            let mut record = |m: Member, _: &str, end: usize| {
                if let Some(c) = m.container
                    && c >= base
                {
                    let i = (c - base) as usize;
                    if ends.len() <= i {
                        ends.resize(i + 1, usize::MAX);
                    }
                    ends[i] = end;
                }
            };
            if let Some(e) = self.ahead.step(&mut record) {
                self.ahead.close_open(e, &mut record);
            }
        }
    }

    /// The line of `byte`, a member's start, counted on from the last one asked.
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

    /// Drops the ends no open member of `main` can ask for again.
    fn release(&mut self) {
        let n = self.main.stack.len();
        let first_open = self.main.stack[..n.saturating_sub(1)]
            .iter()
            .filter_map(|f| f.member.as_ref()?.container)
            .min()
            .unwrap_or(self.main.containers);
        while self.base < first_open {
            self.ends.pop_front();
            self.base += 1;
        }
    }
}

impl Cursor for JsonCursor<'_> {
    fn trail_at(&mut self, at: usize) -> anyhow::Result<SharedTrail> {
        if at < self.last_at {
            *self = JsonCursor::new(self.src);
        }
        self.last_at = at;
        self.reach(at);
        // The members holding `at` (start ≤ at < end), outermost first, and of them those of
        // the Trail: not hidden, and `at` not in their label (the key's own text).
        self.ids.clear();
        let mut chain = Vec::new();
        for k in 0..self.main.stack.len() {
            let Some(m) = self.main.stack[k].member.as_ref() else {
                continue;
            };
            let (start, hidden, id) = (m.start, m.hidden, m.id);
            let label = match &m.name {
                MemberName::Key(inner, _) => inner.start - 1..inner.end + 1,
                MemberName::Element(_) => start..start,
            };
            let end = match m.end {
                Some((e, _)) => e,
                None => self.end_of(k),
            };
            if start <= at && at < end && !hidden && !label.contains(&at) {
                self.ids.push(id);
                chain.push((k, end));
            }
        }
        self.release();
        if let Some((ids, t)) = &self.last
            && *ids == self.ids
        {
            return Ok(t.clone());
        }
        let mut trail = Vec::with_capacity(chain.len());
        for (k, end) in chain {
            let src = self.src;
            let start = self.main.stack[k].member.as_ref().expect("a member").start;
            let line = match self.main.stack[k].member.as_ref().expect("a member").line {
                0 => self.line_of(start),
                l => l,
            };
            let f = &mut self.main.stack[k];
            let m = f.member.as_mut().expect("a member");
            m.line = line;
            let (end, end_line) = *m.end.get_or_insert_with(|| {
                let last = end.saturating_sub(1).max(start).min(src.len());
                let lines = memchr::memchr_iter(b'\n', &src[start.min(last)..last]).count();
                (end, line + lines as u64)
            });
            let _ = end;
            if m.text.is_none() {
                m.text = Some(m.name(src, &f.holder));
            }
            let name = m.text.clone().expect("named above");
            let kind = match m.name {
                MemberName::Key(..) => "key",
                MemberName::Element(_) => "element",
            };
            trail.push(Segment {
                kind: kind.into(),
                name,
                line,
                end_line,
                level: None,
                signature: None,
            });
        }
        let t: SharedTrail = trail.into();
        self.last = Some((self.ids.clone(), t.clone()));
        Ok(t)
    }
}
