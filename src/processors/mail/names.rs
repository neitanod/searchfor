//! Header fields and decoded names of a mail (M6 spec §10.1): RFC 5322 header blocks over their
//! folded lines, and RFC 2047 encoded words decoded through `encoding_rs`, each decoded word one
//! piece of the name over its Source bytes (rule 4).
use std::ops::Range;

/// One header field: its physical lines (`range`, line terminators included), the field name
/// as written and its value's line contents, unfolded: the first after the colon and its white
/// space, each continuation whole (its leading white space is part of the value, RFC 5322
/// §2.2.3), none with its line terminator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Header {
    pub range: Range<usize>,
    pub name: Range<usize>,
    pub value: Vec<Range<usize>>,
}

impl Header {
    /// Whether the field is `name`, as fields are compared: ASCII case-insensitively.
    pub fn is(&self, src: &[u8], name: &str) -> bool {
        src[self.name.clone()].eq_ignore_ascii_case(name.as_bytes())
    }

    /// The Source bytes the value spans, its first byte to its last (empty at the value's
    /// start when it has none).
    pub fn value_span(&self) -> Range<usize> {
        match (self.value.first(), self.value.last()) {
            (Some(a), Some(b)) => a.start..b.end.max(a.start),
            _ => self.name.end..self.name.end,
        }
    }
}

/// A header block: its fields in order and where the body starts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct HeaderBlock {
    pub headers: Vec<Header>,
    pub body: usize,
}

impl HeaderBlock {
    /// The first field named `name`.
    pub fn get(&self, src: &[u8], name: &str) -> Option<&Header> {
        self.headers.iter().find(|h| h.is(src, name))
    }
}

/// The line at `pos` (ending no later than `end`): its content without `\n` or `\r\n`, and
/// where the next line starts. A lone CR is no line break (rule 7).
pub(crate) fn line_at(src: &[u8], pos: usize, end: usize) -> (Range<usize>, usize) {
    let nl = memchr::memchr(b'\n', &src[pos..end]).map_or(end, |k| pos + k);
    let next = (nl + 1).min(end);
    let content_end = if nl < end && nl > pos && src[nl - 1] == b'\r' {
        nl - 1
    } else {
        nl
    };
    (pos..content_end, next)
}

/// The end of the field name of a header line (`Name:` or the obsolete `Name :`), and where
/// the value starts; None when the line is no field.
fn field(line: &[u8]) -> Option<(usize, usize)> {
    let n = line
        .iter()
        .position(|&b| !(33..=126).contains(&b) || b == b':')
        .unwrap_or(line.len());
    if n == 0 {
        return None;
    }
    let mut i = n;
    while i < line.len() && matches!(line[i], b' ' | b'\t') {
        i += 1;
    }
    if line.get(i) != Some(&b':') {
        return None;
    }
    i += 1;
    while i < line.len() && matches!(line[i], b' ' | b'\t') {
        i += 1;
    }
    Some((n, i))
}

/// The header block at `start`, up to `end`: fields and their continuation lines until a blank
/// line, whose next line starts the body. A line that is neither a field nor a continuation
/// starts the body itself, even without the blank line (spec §10.1, Failure).
pub(crate) fn header_block(src: &[u8], start: usize, end: usize) -> HeaderBlock {
    let mut block = HeaderBlock {
        headers: Vec::new(),
        body: end,
    };
    let mut pos = start;
    while pos < end {
        let (content, next) = line_at(src, pos, end);
        let line = &src[content.clone()];
        if line.is_empty() {
            block.body = next;
            return block;
        }
        if matches!(line[0], b' ' | b'\t') {
            if let Some(h) = block.headers.last_mut() {
                h.range.end = next;
                h.value.push(content);
                pos = next;
                continue;
            }
        } else if let Some((n, v)) = field(line) {
            block.headers.push(Header {
                range: pos..next,
                name: pos..pos + n,
                // One line of the value, not a range of indices.
                #[allow(clippy::single_range_in_vec_init)]
                value: vec![pos + v..content.end],
            });
            pos = next;
            continue;
        }
        block.body = pos;
        return block;
    }
    block.body = end;
    block
}

/// A name built up to one character past its cut, as `text::CutName`, whose pieces may also be
/// decoded strings over the Source bytes they come from (an encoded word).
pub(crate) struct NameBuf {
    text: String,
    pieces: crate::text::Pieces,
    chars: usize,
    budget: usize,
    max: usize,
}

impl NameBuf {
    /// `max` 0 cuts nothing.
    pub fn new(max: usize) -> Self {
        Self {
            text: String::new(),
            pieces: Vec::new(),
            chars: 0,
            budget: if max == 0 { usize::MAX } else { max + 1 },
            max,
        }
    }

    pub fn full(&self) -> bool {
        self.chars >= self.budget
    }

    /// `src[r]` as written, invalid UTF-8 as U+FFFD. Only the bytes that can hold the
    /// characters left before the cut are read: a character takes at most four.
    pub fn push_source(&mut self, src: &[u8], r: Range<usize>) {
        if self.full() || r.is_empty() {
            return;
        }
        let room = (self.budget - self.chars).saturating_mul(4);
        let r = r.start..r.end.min(r.start.saturating_add(room));
        let at = self.text.len();
        crate::text::push_lossy(&mut self.text, &mut self.pieces, src, r);
        self.chars += self.text[at..].chars().count();
    }

    /// Text decoded from the Source bytes `r`: one piece.
    pub fn push_decoded(&mut self, s: &str, r: Range<usize>) {
        if self.full() || s.is_empty() {
            return;
        }
        let at = self.text.len();
        for c in s.chars() {
            if self.full() {
                break;
            }
            self.text.push(c);
            self.chars += 1;
        }
        self.pieces.push((at..self.text.len(), r));
    }

    pub fn finish(mut self) -> (String, crate::text::Pieces) {
        crate::text::cut_name(&mut self.text, &mut self.pieces, self.max);
        (self.text, self.pieces)
    }
}

/// Bytes of a folded value read as one: the value's bytes in order, each with its Source
/// offset, at most `cap` of them.
pub(crate) struct Unfolded {
    pub bytes: Vec<u8>,
    pub offsets: Vec<usize>,
}

impl Unfolded {
    pub fn new(src: &[u8], value: &[Range<usize>], cap: usize) -> Self {
        let mut u = Unfolded {
            bytes: Vec::new(),
            offsets: Vec::new(),
        };
        for r in value {
            let room = cap.saturating_sub(u.bytes.len());
            let r = r.start..r.end.min(r.start.saturating_add(room));
            u.bytes.extend_from_slice(&src[r.clone()]);
            u.offsets.extend(r);
            if u.bytes.len() >= cap {
                break;
            }
        }
        u
    }

    /// The Source bytes of `r`, a range of `bytes`: its first byte's to its last's.
    pub fn source(&self, r: Range<usize>) -> Range<usize> {
        if r.is_empty() {
            let at = self
                .offsets
                .get(r.start)
                .copied()
                .unwrap_or_else(|| self.offsets.last().map_or(0, |o| o + 1));
            return at..at;
        }
        self.offsets[r.start]..self.offsets[r.end - 1] + 1
    }

    /// Pushes `bytes[r]` as written, one call per run of contiguous Source bytes.
    pub fn push_verbatim(&self, src: &[u8], r: Range<usize>, out: &mut NameBuf) {
        let mut i = r.start;
        while i < r.end && !out.full() {
            let mut j = i + 1;
            while j < r.end && self.offsets[j] == self.offsets[j - 1] + 1 {
                j += 1;
            }
            out.push_source(src, self.offsets[i]..self.offsets[j - 1] + 1);
            i = j;
        }
    }
}

/// Bytes of an unfolded value read for its name: enough for the characters a cut name shows,
/// encoded as long as RFC 2047 allows several times over.
pub(crate) fn name_cap(max: usize) -> usize {
    if max == 0 {
        usize::MAX
    } else {
        max.saturating_add(1)
            .saturating_mul(16)
            .saturating_add(1024)
    }
}

/// Decodes the base64 of `s` (RFC 2045's alphabet, `=` padding optional); None when it is not
/// base64.
pub(crate) fn base64(s: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() / 4 * 3 + 3);
    let (mut acc, mut bits) = (0u32, 0u32);
    let mut pad = false;
    for &c in s {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => {
                pad = true;
                continue;
            }
            _ => return None,
        };
        if pad {
            return None;
        }
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    // Six bits left over hold no byte: no base64 ends there.
    (bits < 6).then_some(out)
}

fn hex(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

/// Decodes RFC 2047's Q encoding: `_` is a space, `=XX` a byte. None when an `=` is not
/// followed by two hexadecimal digits.
fn q(s: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        match s[i] {
            b'_' => out.push(b' '),
            b'=' => {
                let h = hex(*s.get(i + 1)?)?;
                let l = hex(*s.get(i + 2)?)?;
                out.push(h << 4 | l);
                i += 2;
            }
            c => out.push(c),
        }
        i += 1;
    }
    Some(out)
}

/// Decodes `bytes` in the charset named `label` (an RFC 2231 `*language` suffix ignored);
/// None for a charset `encoding_rs` does not know.
pub(crate) fn charset(label: &[u8], bytes: &[u8]) -> Option<String> {
    let label = label.split(|&b| b == b'*').next().unwrap_or(label);
    let enc = encoding_rs::Encoding::for_label(label)?;
    Some(enc.decode_without_bom_handling(bytes).0.into_owned())
}

/// The encoded word `=?charset?B|Q?text?=` at `i` of `s`: its end and decoded text. None when
/// it is not one or does not decode: then it stays literal (plan Task 9.1).
fn encoded_word(s: &[u8], i: usize) -> Option<(usize, String)> {
    let rest = s.get(i..)?;
    if !rest.starts_with(b"=?") {
        return None;
    }
    let token = |b: u8| (33..=126).contains(&b) && b != b'?';
    let cs = rest[2..].iter().position(|&b| !token(b))? + 2;
    if cs == 2 || rest.get(cs) != Some(&b'?') {
        return None;
    }
    let enc = *rest.get(cs + 1)?;
    if rest.get(cs + 2) != Some(&b'?') {
        return None;
    }
    let text = cs + 3;
    let len = rest[text..]
        .windows(2)
        .position(|w| w == b"?=")
        .filter(|&k| !rest[text..text + k].iter().any(|&b| b <= b' ' || b == 127))?;
    let raw = &rest[text..text + len];
    let bytes = match enc {
        b'B' | b'b' => base64(raw)?,
        b'Q' | b'q' => q(raw)?,
        _ => return None,
    };
    let decoded = charset(&rest[2..cs], &bytes)?;
    Some((i + text + len + 2, decoded))
}

/// Pushes the unfolded text `u`, its leading and trailing white space left out, with its
/// encoded words decoded, each one piece over its Source bytes; the white space between two
/// adjacent encoded words is dropped, as RFC 2047 §6.2 requires.
pub(crate) fn push_decoded_text(src: &[u8], u: &Unfolded, out: &mut NameBuf) {
    let s = &u.bytes;
    let mut i = 0;
    let mut end = s.len();
    while i < end && matches!(s[i], b' ' | b'\t') {
        i += 1;
    }
    while end > i && matches!(s[end - 1], b' ' | b'\t') {
        end -= 1;
    }
    let mut literal = i;
    let mut after_word = false;
    while i < end && !out.full() {
        if s[i] == b'='
            && s.get(i + 1) == Some(&b'?')
            && let Some((e, text)) = encoded_word(&s[..end], i)
        {
            let between = &s[literal..i];
            let blank = between.iter().all(|b| matches!(b, b' ' | b'\t'));
            if !(after_word && blank) {
                u.push_verbatim(src, literal..i, out);
            }
            out.push_decoded(&text, u.source(i..e));
            i = e;
            literal = e;
            after_word = true;
            continue;
        }
        if !matches!(s[i], b' ' | b'\t') {
            after_word = false;
        }
        i += 1;
    }
    if literal < end {
        u.push_verbatim(src, literal..end, out);
    }
}

/// A header's value as a name: unfolded, its encoded words decoded, cut at `max` characters.
pub(crate) fn decoded_value(src: &[u8], h: &Header, max: usize) -> (String, crate::text::Pieces) {
    let mut out = NameBuf::new(max);
    let u = Unfolded::new(src, &h.value, name_cap(max));
    push_decoded_text(src, &u, &mut out);
    out.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(src: &str) -> (String, crate::text::Pieces) {
        let b = header_block(src.as_bytes(), 0, src.len());
        decoded_value(src.as_bytes(), &b.headers[0], 40)
    }

    #[test]
    fn folded_fields_and_the_body() {
        let src = b"A: 1\r\nB:  x\r\n\ty\r\n\r\nbody\r\n";
        let b = header_block(src, 0, src.len());
        assert_eq!(b.headers.len(), 2);
        assert_eq!(b.headers[1].range, 6..17);
        assert_eq!(&src[b.headers[1].name.clone()], b"B");
        assert_eq!(b.headers[1].value, vec![10..11, 13..15]);
        assert_eq!(&src[b.body..], b"body\r\n");
        // A line that is no field starts the body.
        let src = b"A: 1\nnot a field\n";
        assert_eq!(header_block(src, 0, src.len()).body, 5);
        // A continuation with no field before it is no header either.
        assert_eq!(header_block(b" x\n", 0, 3).body, 0);
    }

    #[test]
    fn encoded_words_decode_into_pieces() {
        let (t, p) = value("Subject: =?UTF-8?B?QnVkZ2V0?=\n");
        assert_eq!(t, "Budget");
        assert_eq!(p, vec![(0..6, 9..29)]);
        let (t, _) = value("Subject: =?UTF-8?Q?Budget_=C3=A9?=\n");
        assert_eq!(t, "Budget é");
        // Adjacent encoded words join; across a fold too.
        let (t, _) = value("Subject: =?ISO-8859-1?Q?a?= \n =?ISO-8859-1?Q?=E9?= b\n");
        assert_eq!(t, "aé b");
        // Invalid words stay literal.
        let (t, p) = value("Subject: =?nope?Q?x?= and =?UTF-8?B?!!?=\n");
        assert_eq!(t, "=?nope?Q?x?= and =?UTF-8?B?!!?=");
        assert_eq!(p, vec![(0..31, 9..40)]);
        let (t, p) = value("Subject: a\n b\n");
        assert_eq!(t, "a b");
        assert_eq!(p, vec![(0..1, 9..10), (1..3, 11..13)]);
    }

    #[test]
    fn long_values_are_cut_without_reading_them_whole() {
        let src = format!("Subject: {}\n", "a".repeat(1 << 20));
        let (t, p) = value(&src);
        assert_eq!(t.chars().count(), 40);
        assert!(t.ends_with('…'));
        assert!(p.iter().all(|(_, s)| s.end < 9 + 1024));
    }

    #[test]
    fn base64_alphabet() {
        assert_eq!(base64(b"bmVlZGxl").unwrap(), b"needle");
        assert_eq!(base64(b"YQ==").unwrap(), b"a");
        assert_eq!(base64(b"YQ").unwrap(), b"a");
        assert!(base64(b"Y").is_none());
        assert!(base64(b"Y*").is_none());
    }
}
