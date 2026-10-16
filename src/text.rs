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

#[cfg(test)]
mod tests {
    use super::*;

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
