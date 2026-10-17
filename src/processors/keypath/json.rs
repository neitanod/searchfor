//! The JSON walker of `keypath` (spec §5.2): one iterative pass over the bytes with an explicit
//! stack, so nesting never touches the call stack. Comments (`//`, `/* */`) and trailing commas
//! are accepted (JSONC); everything else must be JSON: numbers, `true`, `false`, `null`, and
//! strings with valid escapes and no control characters. Several top-level values (JSON Lines)
//! are read in turn. Past `MAX_DEPTH` nested containers no more Segments open, so a deeper
//! Match gets the Trail of the deepest one (M2 plan, D34).
use super::collect::Collector;
use crate::trail::OutlineSegment;
use std::ops::Range;

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
    /// The current member (key or element) opened a Segment.
    member: bool,
    /// The key read, waiting for its value: its decoded name, its range (quotes included),
    /// and whether it had escapes.
    key: Option<(String, Range<usize>, bool)>,
}

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

/// The text of a JSON string's contents, escapes decoded (lossy on invalid UTF-8). `string`
/// already checked the escapes.
pub(super) fn decode(raw: &[u8], escaped: bool) -> String {
    if !escaped {
        return String::from_utf8_lossy(raw).into_owned();
    }
    let mut out = String::new();
    let mut bytes = Vec::new();
    let mut i = 0;
    let flush = |bytes: &mut Vec<u8>, out: &mut String| {
        out.push_str(&String::from_utf8_lossy(bytes));
        bytes.clear();
    };
    while i < raw.len() {
        if raw[i] != b'\\' || i + 1 >= raw.len() {
            bytes.push(raw[i]);
            i += 1;
            continue;
        }
        flush(&mut bytes, &mut out);
        let c = raw[i + 1];
        i += 2;
        match c {
            b'n' => out.push('\n'),
            b't' => out.push('\t'),
            b'r' => out.push('\r'),
            b'b' => out.push('\u{8}'),
            b'f' => out.push('\u{c}'),
            b'u' => {
                let hex = |at: usize| {
                    raw.get(at..at + 4)
                        .and_then(|h| std::str::from_utf8(h).ok())
                        .and_then(|h| u32::from_str_radix(h, 16).ok())
                };
                let Some(hi) = hex(i) else {
                    out.push('\u{fffd}');
                    continue;
                };
                i += 4;
                let cp = if (0xD800..0xDC00).contains(&hi) && raw.get(i..i + 2) == Some(b"\\u") {
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
                out.push(char::from_u32(cp).unwrap_or('\u{fffd}'));
            }
            other => out.push(other as char),
        }
    }
    flush(&mut bytes, &mut out);
    out
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
        member: false,
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

/// Walks `src`; returns where it stopped: `src.len()`, or the byte of the first syntax error.
pub fn walk(src: &[u8], c: &mut Collector) -> usize {
    let mut sc = Scan { s: src, i: 0 };
    let mut stack: Vec<Frame> = Vec::new();
    // Segments open in the collector: at most MAX_DEPTH.
    let mut open = 0usize;
    // End of the last complete value: where its member's Segment closes.
    let mut last_end = 0usize;
    loop {
        sc.ws();
        let Some(k) = stack.len().checked_sub(1) else {
            if sc.i >= src.len() {
                return src.len();
            }
            if let Err(at) = begin(&mut sc, &mut stack, String::new(), &mut last_end) {
                return at.min(src.len());
            }
            continue;
        };
        let (object, want) = (stack[k].object, stack[k].want);
        let Some(b) = sc.peek() else {
            return src.len();
        };
        match (want, b) {
            (Want::KeyOrEnd, b'}') | (Want::ValueOrEnd, b']') => {
                sc.i += 1;
                last_end = sc.i;
                stack.pop();
            }
            (Want::CommaOrEnd, b',') => {
                sc.i += 1;
                let f = &mut stack[k];
                if std::mem::take(&mut f.member) {
                    c.close(last_end);
                    open -= 1;
                }
                f.want = if object {
                    Want::KeyOrEnd
                } else {
                    Want::ValueOrEnd
                };
            }
            (Want::CommaOrEnd, b'}') if object => {
                sc.i += 1;
                if stack[k].member {
                    c.close(last_end);
                    open -= 1;
                }
                stack.pop();
                last_end = sc.i;
            }
            (Want::CommaOrEnd, b']') if !object => {
                sc.i += 1;
                if stack[k].member {
                    c.close(last_end);
                    open -= 1;
                }
                stack.pop();
                last_end = sc.i;
            }
            (Want::KeyOrEnd, b'"') => {
                let (r, escaped) = match sc.string() {
                    Ok(x) => x,
                    Err(at) => return at.min(src.len()),
                };
                let name = if open < MAX_DEPTH {
                    decode(&src[r.start + 1..r.end - 1], escaped)
                } else {
                    String::new()
                };
                stack[k].key = Some((name, r, escaped));
                stack[k].want = Want::Colon;
            }
            (Want::Colon, b':') => {
                sc.i += 1;
                stack[k].want = Want::Value;
            }
            (Want::Value | Want::ValueOrEnd, _) => {
                let f = &mut stack[k];
                let holder = if object {
                    let (name, r, _) = f.key.take().expect("Want::Value follows a key");
                    if open < MAX_DEPTH {
                        let inner = r.start + 1..r.end - 1;
                        let mut seg =
                            OutlineSegment::new("key", name.clone(), r.start..r.start, r.clone());
                        seg.name = Some(vec![(0..name.len(), inner.clone())]);
                        seg.name_anchor = inner.start;
                        c.open(seg);
                        f.member = true;
                        open += 1;
                    }
                    name
                } else {
                    let name = if open < MAX_DEPTH {
                        format!("{}[{}]", f.holder, f.n)
                    } else {
                        String::new()
                    };
                    f.n += 1;
                    if open < MAX_DEPTH {
                        c.open(OutlineSegment::new(
                            "element",
                            name.clone(),
                            sc.i..sc.i,
                            sc.i..sc.i,
                        ));
                        f.member = true;
                        open += 1;
                    }
                    name
                };
                if f.member && b == b'[' {
                    c.hide_top();
                }
                f.want = Want::CommaOrEnd;
                if let Err(at) = begin(&mut sc, &mut stack, holder, &mut last_end) {
                    return at.min(src.len());
                }
            }
            _ => return sc.i,
        }
    }
}
