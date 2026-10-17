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
    push_lossy_upto(text, pieces, src, r, usize::MAX);
}

/// `push_lossy` of at most `max` characters of `src[r]`: the pieces are the same up to there.
/// Returns how many characters it pushed.
fn push_lossy_upto(
    text: &mut String,
    pieces: &mut Pieces,
    src: &[u8],
    r: std::ops::Range<usize>,
    max: usize,
) -> usize {
    let mut at = r.start;
    let mut n = 0;
    for chunk in src[r].utf8_chunks() {
        let valid = chunk.valid();
        if n == max {
            return n;
        }
        if !valid.is_empty() {
            let take = valid
                .char_indices()
                .nth(max - n)
                .map_or(valid.len(), |(k, _)| k);
            let t = text.len();
            text.push_str(&valid[..take]);
            n += valid[..take].chars().count();
            pieces.push((t..text.len(), at..at + take));
            if take < valid.len() {
                return n;
            }
            at += take;
        }
        let invalid = chunk.invalid();
        if !invalid.is_empty() && n < max {
            let t = text.len();
            text.push('\u{FFFD}');
            pieces.push((t..text.len(), at..at + invalid.len()));
            at += invalid.len();
            n += 1;
        }
    }
    n
}

/// A name cut at `max` characters (`cut_name`), built only up to one character past its cut,
/// which tells that there is one: what lies beyond adds no text and no pieces, so a `class` of
/// millions of words, a cue identifier of megabytes of Latin-1 or a header cell of `""`s costs
/// the characters it shows (M3 code review r1 and r2, Codex #1). `finish` gives what
/// `push_lossy` and `cut_name` give for the whole name.
pub struct CutName {
    text: String,
    pieces: Pieces,
    /// Characters pushed so far, at most `budget`.
    chars: usize,
    budget: usize,
    max: usize,
}

impl CutName {
    /// `max` 0 cuts nothing, as `cut_name`.
    pub fn new(max: usize) -> Self {
        Self {
            text: String::new(),
            pieces: Pieces::new(),
            chars: 0,
            budget: if max == 0 { usize::MAX } else { max + 1 },
            max,
        }
    }

    /// Whether the cut is decided: nothing pushed from now on is kept.
    pub fn full(&self) -> bool {
        self.chars >= self.budget
    }

    /// `push_lossy` of `src[r]`, up to the budget.
    pub fn push_lossy(&mut self, src: &[u8], r: std::ops::Range<usize>) {
        let room = self.budget - self.chars;
        self.chars += push_lossy_upto(&mut self.text, &mut self.pieces, src, r, room);
    }

    /// Text that maps to nothing in the Source, as the `#` and `.` of an HTML name.
    pub fn push_str(&mut self, s: &str) {
        for c in s.chars() {
            if self.full() {
                return;
            }
            self.text.push(c);
            self.chars += 1;
        }
    }

    /// One character decoded from the Source bytes `r`, as CSV's `""`.
    pub fn push_decoded(&mut self, c: char, r: std::ops::Range<usize>) {
        if self.full() {
            return;
        }
        let at = self.text.len();
        self.text.push(c);
        self.pieces.push((at..self.text.len(), r));
        self.chars += 1;
    }

    /// The name, cut, and its pieces.
    pub fn finish(mut self) -> (String, Pieces) {
        cut_name(&mut self.text, &mut self.pieces, self.max);
        (self.text, self.pieces)
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

/// True for the characters that would break a printed name or drive the terminal: C0 and C1
/// controls, DEL, and the Unicode line and paragraph separators.
fn breaks_display(c: char) -> bool {
    c.is_control() || c == '\u{2028}' || c == '\u{2029}'
}

/// A Segment name or signature as tree and one-line output print it (spec §6.5, M3 plan D14):
/// each character of `breaks_display` as its escape (`\n`, `\r`, `\t`, `\0`, else
/// `\u{1b}`), so a decoded `"a\nb"` key stays on its row and cannot write escape sequences to
/// the terminal. Everything else, a backslash included, is unchanged; `--json` keeps the name
/// as decoded.
pub fn visible(name: &str) -> std::borrow::Cow<'_, str> {
    if !name.chars().any(breaks_display) {
        return std::borrow::Cow::Borrowed(name);
    }
    let mut out = String::with_capacity(name.len() + 8);
    for c in name.chars() {
        if breaks_display(c) {
            out.extend(c.escape_debug());
        } else {
            out.push(c);
        }
    }
    std::borrow::Cow::Owned(out)
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
    fn a_cut_name_keeps_nothing_past_its_cut() {
        // M3 code review r2, Codex #1: past one character after the cut, pushes keep nothing,
        // and the result is what `push_lossy` and `cut_name` give for the whole name.
        let src = [
            &b"div"[..],
            &b" a".repeat(1000),
            b"\xe9".repeat(100).as_slice(),
        ]
        .concat();
        let mut n = CutName::new(10);
        n.push_lossy(&src, 0..3);
        for k in 0..1000 {
            n.push_str(".");
            n.push_lossy(&src, 4 + 2 * k..5 + 2 * k);
        }
        assert!(n.full());
        assert!(n.pieces.len() <= 11, "{}", n.pieces.len());
        let at = 2003;
        n.push_lossy(&src, at..at + 100);
        let (t, p) = n.finish();
        let (mut whole, mut wp) = (String::new(), Pieces::new());
        push_lossy(&mut whole, &mut wp, &src, 0..3);
        for k in 0..1000 {
            whole.push('.');
            push_lossy(&mut whole, &mut wp, &src, 4 + 2 * k..5 + 2 * k);
        }
        cut_name(&mut whole, &mut wp, 10);
        assert_eq!((t.as_str(), &p), (whole.as_str(), &wp));
        assert_eq!(t, "div.a.a.a…");
        // Latin-1: one character per invalid byte, before the cut or past it.
        let mut n = CutName::new(4);
        n.push_lossy(&src, at..at + 100);
        assert_eq!(
            n.finish(),
            (
                "\u{FFFD}\u{FFFD}\u{FFFD}…".to_string(),
                vec![
                    (0..3, at..at + 1),
                    (3..6, at + 1..at + 2),
                    (6..9, at + 2..at + 3)
                ]
            )
        );
        // A decoded character keeps its Source range; a short name is untouched.
        let mut n = CutName::new(40);
        n.push_decoded('"', 7..9);
        n.push_lossy(b"ab", 0..2);
        assert!(!n.full());
        assert_eq!(
            n.finish(),
            ("\"ab".to_string(), vec![(0..1, 7..9), (1..3, 0..2)])
        );
    }

    #[test]
    fn visible_escapes_controls_and_line_separators_only() {
        assert_eq!(visible("plain ñ"), "plain ñ");
        assert!(matches!(visible("plain"), std::borrow::Cow::Borrowed(_)));
        assert_eq!(visible("a\nb"), "a\\nb");
        assert_eq!(visible("a\r\tb\0"), "a\\r\\tb\\0");
        assert_eq!(visible("\u{1b}[31mred"), "\\u{1b}[31mred");
        assert_eq!(
            visible("x\u{85}y\u{2028}z\u{7f}"),
            "x\\u{85}y\\u{2028}z\\u{7f}"
        );
        // A backslash and quotes are text, not escapes.
        assert_eq!(visible(r#"a\nb "q" 's'"#), r#"a\nb "q" 's'"#);
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
