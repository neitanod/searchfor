//! Text for display: invalid UTF-8 becomes U+FFFD, with a map of offsets.

/// Returns the text and `map`, where `map[i]` is the offset in the text of original byte `i`,
/// for every `i` in `0..=bytes.len()`. Each maximal invalid sequence becomes one U+FFFD, as
/// `String::from_utf8_lossy` does.
pub fn lossy_with_map(bytes: &[u8]) -> (String, Vec<usize>) {
    let mut out = String::with_capacity(bytes.len());
    let mut map = Vec::with_capacity(bytes.len() + 1);
    for chunk in bytes.utf8_chunks() {
        let valid = chunk.valid();
        let start = out.len();
        out.push_str(valid);
        map.extend((0..valid.len()).map(|i| start + i));
        let invalid = chunk.invalid();
        if !invalid.is_empty() {
            let at = out.len();
            out.push('\u{FFFD}');
            map.extend(std::iter::repeat_n(at, invalid.len()));
        }
    }
    map.push(out.len());
    (out, map)
}

/// The line without its `\n` terminator. A `\r` before it stays: ripgrep's default line
/// terminator is `\n` alone (CRLF mode, `--crlf`, is not in M1), and it keeps the `\r` in
/// both its output and its submatch search.
pub fn strip_terminator(bytes: &[u8]) -> &[u8] {
    bytes.strip_suffix(b"\n").unwrap_or(bytes)
}

/// A name's pieces, in the format of `model::Name::pieces`: (range in the text, range in the
/// Source).
pub type Pieces = Vec<(std::ops::Range<usize>, std::ops::Range<usize>)>;

/// Appends the text of `src[r]`, invalid UTF-8 as U+FFFD, with one piece per valid run
/// (verbatim) and one per invalid sequence (its U+FFFD over its bytes): a cut then keeps every
/// visible run, a Latin-1 `é` before 40 `A`s included (M3 review r2, Codex #2).
pub fn push_lossy(text: &mut String, pieces: &mut Pieces, src: &[u8], r: std::ops::Range<usize>) {
    let mut at = r.start;
    for chunk in src[r].utf8_chunks() {
        for (part, replaced) in [(chunk.valid().as_bytes(), false), (chunk.invalid(), true)] {
            if part.is_empty() {
                continue;
            }
            let t = text.len();
            if replaced {
                text.push('\u{FFFD}');
            } else {
                text.push_str(chunk.valid());
            }
            pieces.push((t..text.len(), at..at + part.len()));
            at += part.len();
        }
    }
}

/// Cuts `text` to `max` characters, the last one `…`, when it is longer (M3 plan, D21). The
/// pieces keep mapping what stays visible (M3 review r1, Codex #6): a piece past the cut goes;
/// a verbatim piece across it keeps its bytes before the cut; a decoded piece across it maps
/// its visible part to its whole Source range, unless that would make the two lengths equal,
/// which would read as verbatim. Only the `…` maps to nothing.
pub fn cut_name(text: &mut String, pieces: &mut Pieces, max: usize) {
    if max == 0 || text.chars().count() <= max {
        return;
    }
    let cut = text
        .char_indices()
        .nth(max - 1)
        .map_or(text.len(), |(i, _)| i);
    text.truncate(cut);
    text.push('…');
    pieces.retain_mut(|(t, s)| {
        if t.end <= cut {
            return true;
        }
        if t.start >= cut {
            return false;
        }
        let verbatim = t.len() == s.len();
        t.end = cut;
        if verbatim {
            s.end = s.start + t.len();
            true
        } else {
            t.len() != s.len()
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cut_names_keep_the_pieces_of_what_stays_visible() {
        // A verbatim piece across the cut keeps its prefix, multi-byte included.
        let mut t = "ab".to_string() + &"ñ".repeat(10);
        let mut p: Pieces = vec![(0..2, 100..102), (2..22, 200..220)];
        cut_name(&mut t, &mut p, 5);
        assert_eq!(t, "abññ…");
        assert_eq!(p, vec![(0..2, 100..102), (2..6, 200..204)]);
        // Latin-1: `push_lossy` gives the replaced byte its own piece, so the run after it is
        // verbatim and keeps its visible prefix (M3 review r2, Codex #2).
        let src = [b"\xe9".as_slice(), &[b'A'; 40]].concat();
        let (mut t, mut p) = (String::new(), Pieces::new());
        push_lossy(&mut t, &mut p, &src, 0..src.len());
        assert_eq!(p, vec![(0..3, 0..1), (3..43, 1..41)]);
        cut_name(&mut t, &mut p, 10);
        assert_eq!(t, format!("\u{FFFD}{}…", "A".repeat(8)));
        assert_eq!(p, vec![(0..3, 0..1), (3..11, 1..9)]);
        // A decoded piece of several characters across the cut maps its visible part to its
        // whole Source range, unless the lengths would match, which would read as verbatim.
        let mut t = "\u{FFFD}\u{FFFD}ab".to_string();
        let mut p: Pieces = vec![(0..8, 0..4)];
        cut_name(&mut t, &mut p, 2);
        assert_eq!((t.as_str(), p), ("\u{FFFD}…", vec![(0..3, 0..4)]));
        // Short names are untouched.
        let mut t = "short".to_string();
        let mut p: Pieces = vec![(0..5, 0..5)];
        cut_name(&mut t, &mut p, 40);
        assert_eq!((t.as_str(), p), ("short", vec![(0..5, 0..5)]));
    }

    #[test]
    fn valid_text_maps_one_to_one() {
        let (t, m) = lossy_with_map("añb".as_bytes());
        assert_eq!(t, "añb");
        assert_eq!(m, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn invalid_bytes_become_one_replacement() {
        let (t, m) = lossy_with_map(b"caf\xe9 x");
        assert_eq!(t, "caf\u{FFFD} x");
        assert_eq!(m, vec![0, 1, 2, 3, 6, 7, 8]);
    }

    #[test]
    fn terminators() {
        assert_eq!(strip_terminator(b"a\r\n"), b"a\r");
        assert_eq!(strip_terminator(b"a\n"), b"a");
        assert_eq!(strip_terminator(b"a"), b"a");
    }

    #[test]
    fn replacements_agree_with_from_utf8_lossy() {
        let bytes: &[u8] = b"\xff\xfe a \xe2\x82 b \xf0\x9f\x98";
        let (t, m) = lossy_with_map(bytes);
        assert_eq!(t, String::from_utf8_lossy(bytes));
        assert_eq!(m.len(), bytes.len() + 1);
        assert_eq!(*m.last().unwrap(), t.len());
    }
}
