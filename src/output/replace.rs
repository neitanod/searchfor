//! `-r`/`--replace` in the Trail layouts (M5a plan, D4): each Match of a line replaced by the
//! replacement, its `$1`, `$name` and `${name}` read from the Match's capture groups, as
//! grep-printer 0.2.2's `Replacer::replace_all` replaces them (util.rs): over the Source up to
//! the line's end, its line terminator left out (on a `-U` block, 128 bytes past it, for a
//! look-ahead), from the line's start. A row's highlight then covers its replacements.
use crate::matcher::{Captures, PatternMatcher};
use grep_matcher::{Captures as _, LineTerminator, Match, Matcher as _};
use std::ops::Range;

/// The replacement of a run and the matcher whose groups it reads.
#[derive(Debug)]
pub struct Replacer {
    matcher: PatternMatcher,
    template: Vec<u8>,
    term: LineTerminator,
    multi_line: bool,
}

/// A line with its Matches replaced: the new bytes, and for each Match, in order, where it was
/// in the line and where its replacement is in `bytes`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Replaced {
    pub bytes: Vec<u8>,
    pub spans: Vec<((usize, usize), (usize, usize))>,
}

impl Replaced {
    /// The ranges of `bytes` that the Matches at `subs` (ranges of the original line) became;
    /// a range that is no Match's is dropped. `subs` and the spans are both in the line's
    /// order, so one walk over the two maps them: looking each submatch up from the first span
    /// made a line of N Matches cost N² (review r5 of the M5a plan, Codex #1). A submatch
    /// before the one looked up last starts the walk again.
    pub fn map(&self, subs: &[(usize, usize)]) -> Vec<(usize, usize)> {
        let mut at = 0;
        let mut prev = None;
        subs.iter()
            .filter_map(|s| {
                if prev.is_some_and(|p| s < p) {
                    at = 0;
                }
                prev = Some(s);
                while self.spans.get(at).is_some_and(|(o, _)| o < s) {
                    at += 1;
                }
                self.spans.get(at).filter(|(o, _)| o == s).map(|(_, n)| *n)
            })
            .collect()
    }
}

impl Replacer {
    /// `term` and `multi_line` are the search's (`search::line_terminator`, `-U`).
    pub fn new(
        matcher: PatternMatcher,
        template: Vec<u8>,
        term: LineTerminator,
        multi_line: bool,
    ) -> Self {
        Replacer {
            matcher,
            template,
            term,
            multi_line,
        }
    }

    /// The line at `range` of `haystack` with its Matches replaced; the line's terminator, when
    /// it has one, stays at the end.
    pub fn replace(&self, haystack: &[u8], range: Range<usize>) -> Replaced {
        let mut bytes = Vec::with_capacity(range.len());
        let mut spans = Vec::new();
        let mut last = range.start;
        self.pass(haystack, range.clone(), |m, caps, hay| {
            bytes.extend_from_slice(&hay[last..m.start()]);
            let at = bytes.len();
            self.interpolate(caps, hay, &mut bytes);
            spans.push((
                (m.start() - range.start, m.end() - range.start),
                (at, bytes.len()),
            ));
            last = m.end();
        });
        if last < range.end {
            bytes.extend_from_slice(&haystack[last..range.end]);
        }
        Replaced { bytes, spans }
    }

    /// The replacement of each Match of the line (with `-U`, the block) at `range` of
    /// `haystack`, with the Match's range in `haystack`, in file order: the one pass over the
    /// line that `replace` makes, so each Match has the groups the search found. An object
    /// file's search keeps them for its strings (`Described::replacements`): a line of an
    /// object file holds many strings, and each string looked for again from its own start or
    /// from its line's start either cost its line once per string or found a `\K` Match's
    /// range with another alternative's groups (review r4 of the M5a plan, Codex #1 and #2).
    pub fn replacements(
        &self,
        haystack: &[u8],
        range: Range<usize>,
    ) -> Vec<((usize, usize), Vec<u8>)> {
        let mut out = Vec::new();
        self.pass(haystack, range, |m, caps, hay| {
            let mut with = Vec::new();
            self.interpolate(caps, hay, &mut with);
            out.push(((m.start(), m.end()), with));
        });
        out
    }

    /// Each Match of the line at `range` of `haystack` with its groups, as grep-printer's
    /// `Replacer::replace_all` finds them: from the line's start, over the Source up to the
    /// line's end, its terminator left out (with `-U`, up to 128 bytes past the block, for a
    /// look-ahead), up to the first Match that starts past the line.
    fn pass(
        &self,
        haystack: &[u8],
        range: Range<usize>,
        mut found: impl FnMut(Match, &Captures, &[u8]),
    ) {
        let end = if self.multi_line {
            haystack.len().min(range.end + 128)
        } else {
            range.start + content_len(&haystack[range.clone()], self.term)
        };
        let hay = &haystack[..end];
        let Ok(mut caps) = self.matcher.new_captures() else {
            return;
        };
        let _ = self
            .matcher
            .captures_iter_at(hay, range.start, &mut caps, |caps| {
                let Some(m) = caps.get(0) else {
                    return false;
                };
                if m.start() >= range.end {
                    return false;
                }
                found(m, caps, hay);
                true
            });
    }

    /// The replacement of the Match of `caps`, appended to `dst`.
    fn interpolate(&self, caps: &Captures, hay: &[u8], dst: &mut Vec<u8>) {
        caps.interpolate(
            |name| self.matcher.capture_index(name),
            hay,
            &self.template,
            dst,
        );
    }
}

/// The length of `line` without its terminator (a CRLF's `\r` too).
fn content_len(line: &[u8], term: LineTerminator) -> usize {
    let mut end = line.len();
    if end > 0 && term.is_suffix(line) {
        end -= 1;
        if term.is_crlf() && end > 0 && line[end - 1] == b'\r' {
            end -= 1;
        }
    }
    end
}

#[cfg(test)]
mod tests {
    use super::*;

    fn replacer(argv: &[&str], pat: &str, with: &str) -> Replacer {
        let mut v = vec!["s4"];
        v.extend_from_slice(argv);
        v.push(pat);
        let a = crate::cli::parse(v.iter().map(Into::into).collect()).unwrap();
        let m = crate::matcher::build(&a, &[pat.to_string()]).unwrap();
        let term = if a.crlf {
            LineTerminator::crlf()
        } else {
            LineTerminator::byte(b'\n')
        };
        Replacer::new(m, with.as_bytes().to_vec(), term, a.multiline)
    }

    #[test]
    fn groups_and_names_as_ripgrep() {
        let r = replacer(&[], r"(?P<w>\w+)@(\w+)", "$2 at ${w}");
        let hay = b"x a@b y c@d\n";
        let got = r.replace(hay, 0..hay.len());
        assert_eq!(got.bytes, b"x b at a y d at c\n");
        assert_eq!(got.spans, [((2, 5), (2, 8)), ((8, 11), (11, 17))]);
        assert_eq!(got.map(&[(8, 11)]), [(11, 17)]);
        assert_eq!(got.map(&[(0, 1)]), Vec::<(usize, usize)>::new());
        // Review r5 of the M5a plan, Codex #1: the walk over both lists keeps every submatch
        // that is a Match's, skips one that is not, and finds one given out of order.
        assert_eq!(
            got.map(&[(0, 1), (2, 5), (3, 4), (8, 11)]),
            [(2, 8), (11, 17)]
        );
        assert_eq!(
            got.map(&[(8, 11), (2, 5), (2, 5)]),
            [(11, 17), (2, 8), (2, 8)]
        );
    }

    #[test]
    fn the_line_terminator_is_kept_and_not_matched() {
        let r = replacer(&["--crlf"], r"\s$", "!");
        let hay = b"a \r\nb\r\n";
        assert_eq!(r.replace(hay, 0..4).bytes, b"a!\r\n");
        // Only the line's Matches: the next line is not searched.
        let r = replacer(&[], "b", "B");
        assert_eq!(r.replace(hay, 0..4).bytes, b"a \r\n");
    }

    #[test]
    fn a_multi_line_match_is_replaced_whole() {
        let r = replacer(&["-U"], r"a\nb", "X");
        let hay = b"a\nb\nc\n";
        assert_eq!(r.replace(hay, 0..4).bytes, b"X\n");
    }

    /// Review r4 of the M5a plan, Codex #1 and #2: the replacements of a line are made in one
    /// pass from its start, each Match with its range in the haystack and its groups; the
    /// pass reads no byte of the next line, and stops at the line's end.
    #[test]
    fn the_replacements_of_a_line_in_one_pass() {
        let hay = b"\x7fELF\0abcd target\0x\nabcd\0";
        let r = replacer(&[], "(c)d|target|^a", "<$1>");
        assert_eq!(
            r.replacements(hay, 0..19),
            [((7, 9), b"<c>".to_vec()), ((10, 16), b"<>".to_vec())]
        );
        assert_eq!(
            r.replacements(hay, 19..24),
            [((19, 20), b"<>".to_vec()), ((21, 23), b"<c>".to_vec())]
        );
        let r = replacer(&[], "x$", "X");
        assert_eq!(r.replacements(hay, 0..19), [((17, 18), b"X".to_vec())]);
        assert_eq!(r.replacements(hay, 19..24), []);
    }
}
