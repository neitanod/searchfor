//! MIME (RFC 2045–2046, 2231): the media type and parameters of `Content-Type` and
//! `Content-Disposition`, file names decoded, and the parts of a multipart body, nested as its
//! boundaries say (M6 spec §10.1). Bodies are never decoded (8390).
use super::names::{Header, HeaderBlock, NameBuf, Unfolded, header_block, line_at};
use std::ops::Range;

/// Bytes of a `Content-Type` or `Content-Disposition` value read for its parameters.
const PARAMS_CAP: usize = 64 * 1024;

/// Open multiparts at most: a deeper one is read as a leaf part, so nesting of any depth never
/// opens more than 256 Segments (rule 8: a part and its headers under each).
pub(crate) const MAX_DEPTH: usize = 100;

/// One parameter: its name lowercased, its value's bytes unquoted, and the range of those
/// bytes in `Unfolded::bytes` (quotes left out).
#[derive(Debug, Clone)]
pub(crate) struct Param {
    pub name: Vec<u8>,
    pub value: Vec<u8>,
    pub at: Range<usize>,
}

/// A `type/subtype; name=value; …` value: the type's range in `u.bytes` and the parameters.
pub(crate) struct Typed {
    pub u: Unfolded,
    pub kind: Range<usize>,
    pub params: Vec<Param>,
}

impl Typed {
    pub fn of(src: &[u8], h: &Header) -> Self {
        let u = Unfolded::new(src, &h.value, PARAMS_CAP);
        let s = &u.bytes;
        let mut i = skip_ws(s, 0);
        let k0 = i;
        while i < s.len() && s[i] != b';' && !matches!(s[i], b' ' | b'\t' | b'(') {
            i += 1;
        }
        let kind = k0..i;
        let mut params = Vec::new();
        while let Some(semi) = s[i..].iter().position(|&b| b == b';') {
            i = skip_ws(s, i + semi + 1);
            let n0 = i;
            while i < s.len() && !matches!(s[i], b'=' | b';' | b' ' | b'\t') {
                i += 1;
            }
            let name = s[n0..i].to_ascii_lowercase();
            i = skip_ws(s, i);
            if s.get(i) != Some(&b'=') {
                continue;
            }
            i = skip_ws(s, i + 1);
            let (value, at) = if s.get(i) == Some(&b'"') {
                let v0 = i + 1;
                let mut v = Vec::new();
                i += 1;
                while i < s.len() && s[i] != b'"' {
                    if s[i] == b'\\' && i + 1 < s.len() {
                        i += 1;
                    }
                    v.push(s[i]);
                    i += 1;
                }
                let at = v0..i;
                i = (i + 1).min(s.len());
                (v, at)
            } else {
                let v0 = i;
                while i < s.len() && !matches!(s[i], b';' | b' ' | b'\t') {
                    i += 1;
                }
                (s[v0..i].to_vec(), v0..i)
            };
            if !name.is_empty() {
                params.push(Param { name, value, at });
            }
        }
        Typed { u, kind, params }
    }

    pub fn kind(&self) -> &[u8] {
        &self.u.bytes[self.kind.clone()]
    }

    pub fn is_multipart(&self) -> bool {
        let k = self.kind();
        k.len() > 10 && k[..10].eq_ignore_ascii_case(b"multipart/")
    }

    pub fn param(&self, name: &str) -> Option<&Param> {
        self.params.iter().find(|p| p.name == name.as_bytes())
    }
}

fn skip_ws(s: &[u8], mut i: usize) -> usize {
    while i < s.len() && matches!(s[i], b' ' | b'\t') {
        i += 1;
    }
    i
}

/// The boundary of a multipart entity whose headers are `block`.
pub(crate) fn boundary(src: &[u8], block: &HeaderBlock) -> Option<Vec<u8>> {
    let t = Typed::of(src, block.get(src, "Content-Type")?);
    if !t.is_multipart() {
        return None;
    }
    t.param("boundary")
        .map(|p| p.value.clone())
        .filter(|b| !b.is_empty())
}

/// Whether `line` is a delimiter of `boundary` (`--boundary`), and whether it closes it
/// (`--boundary--`); transport padding after it allowed.
fn delimiter(line: &[u8], boundary: &[u8]) -> Option<bool> {
    let rest = line.strip_prefix(b"--")?.strip_prefix(boundary)?;
    let rest = rest.trim_ascii_end();
    match rest {
        b"" => Some(false),
        b"--" => Some(true),
        _ => None,
    }
}

/// One part of a multipart body: from the line after its delimiter to the line of the next
/// delimiter that closes it, its headers, and its IMAP number (`1.2`).
#[derive(Debug, Clone)]
pub(crate) struct Part {
    pub range: Range<usize>,
    pub number: String,
    pub block: HeaderBlock,
}

struct Multi {
    boundary: Vec<u8>,
    number: String,
    count: usize,
    open: Option<usize>,
}

/// The parts of the multipart `body` whose boundary is `top`, nested parts included, in file order. One
/// pass over the body's lines with a stack of the open multiparts: a delimiter closes the part
/// open in its multipart and everything inside it; a boundary never closed ends its part at the
/// end of `body` (spec §10.1, Failure).
pub(crate) fn scan_parts(src: &[u8], body: Range<usize>, top: Vec<u8>) -> Vec<Part> {
    let mut parts: Vec<Part> = Vec::new();
    let mut stack = vec![Multi {
        boundary: top,
        number: String::new(),
        count: 0,
        open: None,
    }];
    let mut pos = body.start;
    while pos < body.end && !stack.is_empty() {
        let (content, next) = line_at(src, pos, body.end);
        let line = &src[content];
        let found = if line.starts_with(b"--") {
            stack
                .iter()
                .rposition(|m| delimiter(line, &m.boundary).is_some())
                .map(|k| (k, delimiter(line, &stack[k].boundary) == Some(true)))
        } else {
            None
        };
        let Some((k, close)) = found else {
            pos = next;
            continue;
        };
        while stack.len() > k + 1 {
            if let Some(i) = stack.pop().and_then(|m| m.open) {
                parts[i].range.end = pos;
            }
        }
        if let Some(i) = stack[k].open.take() {
            parts[i].range.end = pos;
        }
        if close {
            stack.pop();
            pos = next;
            continue;
        }
        let m = &mut stack[k];
        m.count += 1;
        let number = if m.number.is_empty() {
            m.count.to_string()
        } else {
            format!("{}.{}", m.number, m.count)
        };
        let block = header_block(src, next, body.end);
        let inner = boundary(src, &block).filter(|_| stack.len() < MAX_DEPTH);
        stack[k].open = Some(parts.len());
        pos = block.body;
        parts.push(Part {
            range: next..body.end,
            number: number.clone(),
            block,
        });
        if let Some(b) = inner {
            stack.push(Multi {
                boundary: b,
                number,
                count: 0,
                open: None,
            });
        }
    }
    parts
}

/// A part's file name, decoded: RFC 2231's `filename*` (with its ordered `filename*N` and
/// `filename*N*` continuations), else `filename`, of `Content-Disposition`; else `name` of
/// `Content-Type`. RFC 2047 encoded words in a plain one are decoded too. Gives the name after
/// `prefix`, its pieces and its Source span.
pub(crate) struct FileName {
    pub text: String,
    pub pieces: crate::text::Pieces,
    pub span: Range<usize>,
}

pub(crate) fn file_name(
    src: &[u8],
    block: &HeaderBlock,
    prefix: &str,
    max: usize,
) -> Option<FileName> {
    for (field, param) in [
        ("Content-Disposition", "filename"),
        ("Content-Type", "name"),
    ] {
        let Some(h) = block.get(src, field) else {
            continue;
        };
        let t = Typed::of(src, h);
        let mut out = NameBuf::new(max);
        out.push_generated(prefix);
        let before = out.len();
        let mut span: Option<Range<usize>> = None;
        let mut grow = |r: Range<usize>| {
            span = Some(match span.take() {
                Some(s) => s.start.min(r.start)..s.end.max(r.end),
                None => r,
            });
        };
        if let Some(pieces) = extended(&t, param) {
            let mut cs: Option<Vec<u8>> = None;
            for (i, (p, encoded)) in pieces.iter().enumerate() {
                let mut at = p.at.clone();
                if *encoded && i == 0 {
                    // charset'language'value
                    let v = &t.u.bytes[at.clone()];
                    let mut q = v.iter().enumerate().filter(|(_, b)| **b == b'\'');
                    if let (Some((a, _)), Some((b, _))) = (q.next(), q.next()) {
                        cs = Some(v[..a].to_vec());
                        at = at.start + b + 1..at.end;
                    }
                }
                grow(t.u.source(p.at.clone()));
                if *encoded {
                    percent(
                        src,
                        &t.u,
                        at,
                        cs.as_deref().unwrap_or(b"us-ascii"),
                        &mut out,
                    );
                } else {
                    t.u.push_verbatim(src, at, &mut out);
                }
            }
        } else if let Some(p) = t.param(param) {
            grow(t.u.source(p.at.clone()));
            let sub = Unfolded {
                bytes: t.u.bytes[p.at.clone()].to_vec(),
                offsets: t.u.offsets[p.at.clone()].to_vec(),
            };
            super::names::push_decoded_text(src, &sub, &mut out);
        }
        if let Some(span) = span
            && out.len() > before
        {
            let (text, pieces) = out.finish();
            return Some(FileName { text, pieces, span });
        }
    }
    None
}

/// The RFC 2231 pieces of parameter `name`, in order: `name*` alone, or `name*0`, `name*1`…
/// (each `*` after its number marks it percent-encoded). None when it has none.
fn extended<'t>(t: &'t Typed, name: &str) -> Option<Vec<(&'t Param, bool)>> {
    let base = name.as_bytes();
    if let Some(p) = t
        .params
        .iter()
        .find(|p| p.name.strip_prefix(base) == Some(b"*"))
    {
        return Some(vec![(p, true)]);
    }
    let mut numbered: Vec<(usize, &Param, bool)> = t
        .params
        .iter()
        .filter_map(|p| {
            let rest = p.name.strip_prefix(base)?.strip_prefix(b"*")?;
            let (digits, encoded) = match rest.strip_suffix(b"*") {
                Some(d) => (d, true),
                None => (rest, false),
            };
            let n: usize = std::str::from_utf8(digits).ok()?.parse().ok()?;
            Some((n, p, encoded))
        })
        .collect();
    if numbered.is_empty() {
        return None;
    }
    numbered.sort_by_key(|&(n, _, _)| n);
    numbered.dedup_by_key(|x| x.0);
    // Continuations count from 0 with no gap; the ones after a gap are left out.
    let ok = numbered
        .iter()
        .enumerate()
        .take_while(|(i, (n, _, _))| i == n)
        .count();
    if ok == 0 {
        return None;
    }
    Some(numbered[..ok].iter().map(|&(_, p, e)| (p, e)).collect())
}

/// Pushes the percent-encoded `u.bytes[at]` decoded in `charset`: each run of `%XX` one decoded
/// piece over its Source bytes, the bytes between as written.
fn percent(src: &[u8], u: &Unfolded, at: Range<usize>, charset: &[u8], out: &mut NameBuf) {
    let s = &u.bytes;
    let mut i = at.start;
    let mut literal = i;
    while i < at.end && !out.full() {
        if s[i] != b'%' {
            i += 1;
            continue;
        }
        let run0 = i;
        let mut bytes = Vec::new();
        while i + 2 < at.end && s[i] == b'%' {
            let (Some(h), Some(l)) = (hexv(s[i + 1]), hexv(s[i + 2])) else {
                break;
            };
            bytes.push(h << 4 | l);
            i += 3;
        }
        if bytes.is_empty() {
            i += 1;
            continue;
        }
        u.push_verbatim(src, literal..run0, out);
        match super::names::charset(charset, &bytes) {
            Some(text) => out.push_decoded(&text, u.source(run0..i)),
            None => u.push_verbatim(src, run0..i, out),
        }
        literal = i;
    }
    if literal < at.end {
        u.push_verbatim(src, literal..at.end, out);
    }
}

fn hexv(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts(src: &[u8]) -> Vec<(String, String)> {
        let top = header_block(src, 0, src.len());
        let b = boundary(src, &top).unwrap();
        scan_parts(src, top.body..src.len(), b)
            .into_iter()
            .map(|p| {
                (
                    p.number,
                    String::from_utf8_lossy(&src[p.range]).into_owned(),
                )
            })
            .collect()
    }

    #[test]
    fn nested_parts_and_their_closures() {
        let src = b"Content-Type: multipart/mixed; boundary=\"O U\"\n\npre\n--O U\nContent-Type: multipart/alternative; boundary=IN\n\n--IN\n\na\n--IN--\nepi\n--O U\n\nb\n--O U--\nafter\n";
        let p = parts(src);
        let n: Vec<&str> = p.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(n, ["1", "1.1", "2"]);
        assert_eq!(p[1].1, "\na\n");
        assert_eq!(p[2].1, "\nb\n");
        assert!(p[0].1.ends_with("--IN--\nepi\n"));
    }

    #[test]
    fn an_outer_delimiter_closes_inner_parts_and_unclosed_ones_end_with_the_body() {
        let src = b"Content-Type: multipart/mixed; boundary=X\n\n--X\nContent-Type: multipart/mixed; boundary=Y\n\n--Y\n\ninner\n--X\n\nlast\n";
        let p = parts(src);
        let n: Vec<&str> = p.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(n, ["1", "1.1", "2"]);
        assert_eq!(p[1].1, "\ninner\n");
        assert_eq!(p[2].1, "\nlast\n");
    }

    #[test]
    fn file_names_decode_rfc_2231_and_2047() {
        let name = |h: &str| {
            let src = format!("{h}\n\n");
            let b = header_block(src.as_bytes(), 0, src.len());
            file_name(src.as_bytes(), &b, "", 40).map(|f| f.text)
        };
        assert_eq!(
            name("Content-Disposition: attachment; filename*=UTF-8''caf%C3%A9.txt").as_deref(),
            Some("café.txt")
        );
        assert_eq!(
            name("Content-Disposition: attachment;\n filename*0*=UTF-8''caf%C3%A9;\n filename*1=\".t xt\"")
                .as_deref(),
            Some("café.t xt")
        );
        assert_eq!(
            name("Content-Type: text/plain; name=\"=?UTF-8?Q?caf=C3=A9?=.txt\"").as_deref(),
            Some("café.txt")
        );
        assert_eq!(
            name("Content-Disposition: inline; filename=a.txt").as_deref(),
            Some("a.txt")
        );
        assert_eq!(name("Content-Type: text/plain"), None);
    }
}
