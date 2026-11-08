//! Java `.properties` (M6 spec §7.3): each logical line is a key, one `key` Segment named by
//! its decoded key. A key ends at the first unescaped `=`, `:` or white space; an odd number
//! of `\` at a line's end continues the value; `#` and `!` start comments. The scanner is
//! permissive: an escape it cannot decode stays as written, and reading goes on.
use super::lines::{self, Builder, Kind, Lines, NameSrc};
use crate::text::CutName;
use std::ops::Range;

pub(crate) fn prepare<'a>(
    input: crate::model::Input<'a>,
    params: &'a ::toml::Table,
) -> anyhow::Result<Box<dyn crate::processors::Prepared + 'a>> {
    lines::prepare(input, params, scan)
}

pub(crate) fn outline(
    src: &[u8],
    params: &::toml::Table,
    anchors: Option<&[usize]>,
) -> anyhow::Result<crate::trail::Outline> {
    lines::outline(src, params, anchors, scan)
}

fn blank(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\x0c')
}

/// Whether the line `src[r]` ends with an odd number of `\`.
fn continues(src: &[u8], r: Range<usize>) -> bool {
    src[r].iter().rev().take_while(|&&c| c == b'\\').count() % 2 == 1
}

fn scan(src: &[u8], b: &mut Builder) {
    let mut lines = Lines::new(src);
    while let Some((start, end, mut next)) = lines.next() {
        let mut k = start;
        while k < end && blank(src[k]) {
            k += 1;
        }
        if k == end || matches!(src[k], b'#' | b'!') {
            continue;
        }
        let mut i = k;
        while i < end {
            match src[i] {
                b'\\' => i += 2,
                b'=' | b':' => break,
                c if blank(c) => break,
                _ => i += 1,
            }
        }
        let key = k..i.min(end);
        let mut last = start..end;
        while continues(src, last.clone()) {
            match lines.next() {
                Some((s, e, n)) => {
                    last = s..e;
                    next = n;
                }
                None => break,
            }
        }
        // An empty key (`=value`) names nothing: its lines have an empty Trail, as INI's.
        if !key.is_empty() {
            b.open(start, key.clone(), Kind::Key, NameSrc::Properties(key));
            b.close_to(0, next);
        }
    }
}

/// Four hex digits at `src[at..]`.
fn hex4(src: &[u8], at: usize, end: usize) -> Option<u32> {
    let d = src.get(at..at + 4).filter(|_| at + 4 <= end)?;
    let s = std::str::from_utf8(d).ok()?;
    if !s.bytes().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(s, 16).ok()
}

/// The key `src[r]` decoded into `n`: each escape is one decoded piece over its Source
/// bytes, a `\uXXXX` pair of surrogates one piece, a lone surrogate U+FFFD. An escape that
/// cannot be decoded (`\uQQQQ`) stays as written.
pub(super) fn push_key(src: &[u8], r: Range<usize>, n: &mut CutName) {
    let end = r.end;
    let mut i = r.start;
    let mut run = i;
    while i < end && !n.full() {
        if src[i] != b'\\' {
            i += 1;
            continue;
        }
        n.push_lossy(src, run..i);
        let Some(&c) = src.get(i + 1).filter(|_| i + 1 < end) else {
            // A `\` that ends the key escapes nothing.
            run = end;
            break;
        };
        let simple = match c {
            b't' => Some('\t'),
            b'n' => Some('\n'),
            b'r' => Some('\r'),
            b'f' => Some('\x0c'),
            b'u' => None,
            c if c.is_ascii() => Some(c as char),
            _ => {
                // `\` before a character outside ASCII: the character as written.
                i += 1;
                run = i;
                continue;
            }
        };
        if let Some(d) = simple {
            n.push_decoded(d, i..i + 2);
            i += 2;
            run = i;
            continue;
        }
        let Some(u) = hex4(src, i + 2, end) else {
            n.push_lossy(src, i..i + 2);
            i += 2;
            run = i;
            continue;
        };
        let mut to = i + 6;
        let ch = if (0xD800..0xDC00).contains(&u) {
            let low = (src.get(to) == Some(&b'\\') && src.get(to + 1) == Some(&b'u'))
                .then(|| hex4(src, to + 2, end))
                .flatten()
                .filter(|l| (0xDC00..0xE000).contains(l));
            match low {
                Some(l) => {
                    to += 6;
                    char::from_u32(0x10000 + ((u - 0xD800) << 10) + (l - 0xDC00))
                }
                None => None,
            }
        } else {
            char::from_u32(u)
        };
        n.push_decoded(ch.unwrap_or('\u{FFFD}'), i..to);
        i = to;
        run = i;
    }
    if run < end {
        n.push_lossy(src, run..end);
    }
}

#[cfg(test)]
mod tests {
    use super::super::lines::tests::cursor_is_model;
    use super::super::tests::needles;
    use crate::text::CutName;

    fn key(src: &[u8]) -> (String, crate::text::Pieces) {
        let mut n = CutName::new(40);
        super::push_key(src, 0..src.len(), &mut n);
        n.finish()
    }

    #[test]
    fn keys_decode_their_escapes() {
        assert_eq!(
            key(b"caf\\u00e9\\:key"),
            (
                "café:key".to_string(),
                vec![(0..3, 0..3), (3..5, 3..9), (5..6, 9..11), (6..9, 11..14)]
            )
        );
        assert_eq!(key(b"bad\\uQQQQ").0, "bad\\uQQQQ");
        assert_eq!(key(b"\\uD83D\\uDE00\\uD83D!").0, "😀\u{FFFD}!");
        assert_eq!(key(b"a\\tb\\").0, "a\tb");
    }

    #[test]
    fn a_line_without_a_key_opens_no_segment() {
        // `=value` has an empty key: no Segment named "", as INI (code round 1, Fable #41);
        // its continuation lines are still its value.
        let src = b"=needle\nk=needle\n=a\\\n x=needle\n";
        assert_eq!(needles(src, "properties"), vec![vec![], vec!["k"], vec![]]);
        cursor_is_model("properties", src);
    }

    #[test]
    fn logical_lines_and_comments() {
        let src = b"# needle\n! needle\na\\ b = needle \\\\\nc:needle\\\n  needle\n\nd needle\n";
        assert_eq!(
            needles(src, "properties"),
            vec![vec![], vec![], vec!["a b"], vec!["c"], vec!["c"], vec!["d"]]
        );
    }

    #[test]
    fn the_cursor_gives_the_model_trails_at_every_byte() {
        for src in [
            &b"# comment\nspring.datasource.url=jdbc:needle://db\ncaf\\u00e9\\:key : needle\\\n continued\n"[..],
            b"k=needle\r\nbad\\uQQQQ=needle\r\nlast=needle",
            b"\\",
            b"a\\\n",
            b"",
        ] {
            cursor_is_model("properties", src);
        }
    }
}
