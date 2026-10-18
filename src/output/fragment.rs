//! Fragments of long lines.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fragment {
    pub text: String,
    /// Byte offset of `text` in the line.
    pub start: usize,
    /// Characters omitted before and after `text`.
    pub omitted_before: usize,
    pub omitted_after: usize,
    /// Byte ranges of this Fragment's own Matches, relative to `text`.
    pub submatches: Vec<(usize, usize)>,
}

impl Fragment {
    /// `…[+N] text […+M]`, without colour.
    pub fn render_plain(&self) -> String {
        let mut s = String::new();
        if self.omitted_before > 0 {
            s.push_str(&format!("…[+{}] ", self.omitted_before));
        }
        s.push_str(&self.text);
        if self.omitted_after > 0 {
            s.push_str(&format!(" […+{}]", self.omitted_after));
        }
        s
    }
}

pub const MIN_TEXT: usize = 10;

/// Bytes per entry of `Chars`' index.
const BLOCK: usize = 4096;

/// Character and byte offsets of a line, converted through the number of characters before
/// every `BLOCK`-th byte: a line of 50 MB costs an index of 100 KB, where one offset per
/// character cost 400 MB (M2 plan, Task 11.4).
struct Chars<'a> {
    bytes: &'a [u8],
    /// `before[k]`: the characters that start before byte `k * BLOCK`; the last entry is the
    /// line's character count.
    before: std::borrow::Cow<'a, [usize]>,
}

/// The characters that start in `bytes`: the bytes that are not UTF-8 continuation bytes
/// (`10xxxxxx`), counted eight at a time.
fn starts(bytes: &[u8]) -> usize {
    let words = bytes.chunks_exact(8);
    let rest = words.remainder();
    let continuations: u32 = words
        .map(|w| {
            let w = u64::from_ne_bytes(w.try_into().expect("eight bytes"));
            // Bit 7 of every byte set, and bit 6 clear (`w << 1` brings it to bit 7).
            (w & !(w << 1) & 0x8080_8080_8080_8080).count_ones()
        })
        .sum();
    let in_words = bytes.len() - rest.len() - continuations as usize;
    in_words + rest.iter().filter(|&&b| (b as i8) >= -0x40).count()
}

/// The characters of `line`.
pub(crate) fn char_count(line: &str) -> usize {
    starts(line.as_bytes())
}

/// The index `Chars` converts offsets through, built once per line and shared by the rows of
/// the line (M3 code review r3, Fable #1).
pub(crate) fn char_index(line: &str) -> Vec<usize> {
    let bytes = line.as_bytes();
    let mut before = Vec::with_capacity(bytes.len() / BLOCK + 2);
    let mut n = 0;
    for block in bytes.chunks(BLOCK) {
        before.push(n);
        n += starts(block);
    }
    before.push(n);
    before
}

impl<'a> Chars<'a> {
    #[cfg(test)]
    fn new(line: &'a str) -> Self {
        Self {
            bytes: line.as_bytes(),
            before: char_index(line).into(),
        }
    }

    /// `line` with the index `char_index` built for it.
    fn indexed(line: &'a str, index: &'a [usize]) -> Self {
        Self {
            bytes: line.as_bytes(),
            before: index.into(),
        }
    }

    fn total(&self) -> usize {
        *self.before.last().expect("never empty")
    }

    /// The characters that start before byte `b` (`b` at most the line's length).
    fn to_char(&self, b: usize) -> usize {
        let k = b / BLOCK;
        self.before[k] + starts(&self.bytes[k * BLOCK..b])
    }

    /// The byte where character `c` starts; the line's length for `c` at its end.
    fn to_byte(&self, c: usize) -> usize {
        if c >= self.total() {
            return self.bytes.len();
        }
        // The block where character `c` starts: the last one with fewer characters before it.
        let k = self.before.partition_point(|&n| n <= c) - 1;
        let mut left = c - self.before[k];
        for (i, &b) in self.bytes[k * BLOCK..].iter().enumerate() {
            if (b as i8) >= -0x40 {
                if left == 0 {
                    return k * BLOCK + i;
                }
                left -= 1;
            }
        }
        unreachable!("character {c} starts before the end of the line")
    }
}

/// Splits `line` (valid UTF-8, without terminator) into Fragments around `subs` (byte ranges
/// on char boundaries). Returns None when the line fits in `width` characters.
pub fn fragments(line: &str, subs: &[(usize, usize)], width: usize) -> Option<Vec<Fragment>> {
    fragments_indexed(line, &char_index(line), subs, width)
}

/// `fragments` with the line's `char_index` already built.
pub(crate) fn fragments_indexed(
    line: &str,
    index: &[usize],
    subs: &[(usize, usize)],
    width: usize,
) -> Option<Vec<Fragment>> {
    let chars = Chars::indexed(line, index);
    let total = chars.total();
    if total <= width {
        return None;
    }
    let to_char = |b: usize| chars.to_char(b);
    // Both markers are reserved, so every Fragment of a line has the same text width.
    let marker = 5 + total.to_string().len();
    let inner = width.saturating_sub(2 * marker).max(MIN_TEXT);

    // (char start, char end, index into subs); an inverted search has no subs: anchor at 0.
    let mut spans: Vec<(usize, usize, Option<usize>)> = subs
        .iter()
        .enumerate()
        .map(|(k, &(s, e))| {
            let cs = to_char(s);
            (cs, to_char(e).max(cs + 1), Some(k))
        })
        .collect();
    if spans.is_empty() {
        spans.push((0, 1, None));
    }
    spans.sort();

    // A Match joins the current cluster when the cluster, extended to its end, still fits.
    let mut clusters: Vec<(usize, usize, Vec<usize>)> = Vec::new();
    for (s, e, k) in spans {
        match clusters.last_mut() {
            Some(c) if e.max(c.1) - c.0 <= inner => {
                c.1 = c.1.max(e);
                c.2.extend(k);
            }
            _ => clusters.push((s, e, k.into_iter().collect())),
        }
    }

    Some(
        clusters
            .into_iter()
            .map(|(s, e, own)| {
                // Window in chars: centred on the cluster, clamped to the line.
                let (ws, we) = if e - s >= inner {
                    (s, (s + inner).min(total))
                } else {
                    let pad = (inner - (e - s)) / 2;
                    let we = (s.saturating_sub(pad) + inner).min(total);
                    (we.saturating_sub(inner), we)
                };
                let (bs, be) = (chars.to_byte(ws), chars.to_byte(we));
                Fragment {
                    text: line[bs..be].to_string(),
                    start: bs,
                    omitted_before: ws,
                    omitted_after: total - we,
                    // Only this cluster's Matches, clipped to the window.
                    submatches: own
                        .iter()
                        .map(|&k| subs[k])
                        .filter(|&(a, b)| a < be && b > bs)
                        .map(|(a, b)| (a.max(bs) - bs, b.min(be) - bs))
                        .collect(),
                }
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line_with(len: usize, at: &[(usize, &str)]) -> String {
        let mut s = "x".repeat(len);
        for (i, t) in at {
            s.replace_range(*i..*i + t.len(), t);
        }
        s
    }

    /// `Chars` agrees with an offset per character, across block boundaries and characters
    /// cut by them.
    #[test]
    fn chars_convert_as_one_offset_per_character() {
        let line: String = (0..3 * BLOCK)
            .map(|i| ['a', 'ñ', '€', '😀'][i % 7 % 4])
            .collect();
        let offs: Vec<usize> = line
            .char_indices()
            .map(|(i, _)| i)
            .chain(std::iter::once(line.len()))
            .collect();
        let c = Chars::new(&line);
        assert_eq!(c.total(), offs.len() - 1);
        // Every byte near a block boundary, and some in between.
        let near = |b: usize| b % BLOCK < 8 || b % BLOCK > BLOCK - 8 || b.is_multiple_of(61);
        for b in (0..=line.len()).filter(|&b| near(b) || b == line.len()) {
            assert_eq!(c.to_char(b), offs.partition_point(|&o| o < b), "byte {b}");
        }
        for (i, &o) in offs
            .iter()
            .enumerate()
            .filter(|&(_, &o)| near(o) || o == line.len())
        {
            assert_eq!(c.to_byte(i), o, "char {i}");
        }
        assert_eq!(Chars::new("").total(), 0);
        let b = line.as_bytes();
        for (i, j) in [(0, 1), (1, 9), (3, 20), (5, 4100), (0, b.len())] {
            let plain = b[i..j].iter().filter(|&&x| (x as i8) >= -0x40).count();
            assert_eq!(starts(&b[i..j]), plain, "{i}..{j}");
        }
    }

    #[test]
    fn short_lines_are_untouched() {
        assert!(fragments("short line", &[(0, 5)], 80).is_none());
    }

    #[test]
    fn single_match_is_centred_with_counts() {
        let line = line_with(2006, &[(1000, "NEEDLE")]);
        let f = fragments(&line, &[(1000, 1006)], 80).unwrap();
        let inner = 80 - 2 * (5 + 4);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].text.chars().count(), inner);
        assert_eq!(f[0].omitted_before + inner + f[0].omitted_after, 2006);
        let (a, b) = f[0].submatches[0];
        assert_eq!(&f[0].text[a..b], "NEEDLE");
        assert!(f[0].render_plain().chars().count() <= 80);
    }

    #[test]
    fn matches_that_fit_together_share_a_fragment() {
        // width 100, total 3000 → marker 9, inner 82; 45 characters apart fit.
        let line = line_with(3000, &[(1000, "A"), (1045, "B")]);
        let f = fragments(&line, &[(1000, 1001), (1045, 1046)], 100).unwrap();
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].submatches.len(), 2);
    }

    #[test]
    fn every_match_is_shown_exactly_once() {
        // width 60, total 3000 → inner 42; matches 30 apart form two clusters of two.
        let at = [(1000, "A"), (1030, "B"), (1060, "C"), (1090, "D")];
        let line = line_with(3000, &at);
        let subs: Vec<_> = at.iter().map(|(i, _)| (*i, i + 1)).collect();
        let f = fragments(&line, &subs, 60).unwrap();
        assert_eq!(f.len(), 2);
        let shown: Vec<String> = f
            .iter()
            .flat_map(|fr| {
                fr.submatches
                    .iter()
                    .map(|&(a, b)| fr.text[a..b].to_string())
            })
            .collect();
        assert_eq!(shown, ["A", "B", "C", "D"]);
    }

    #[test]
    fn far_matches_split() {
        let line = line_with(3000, &[(1000, "A"), (2500, "C")]);
        let f = fragments(&line, &[(1000, 1001), (2500, 2501)], 100).unwrap();
        assert_eq!(f.len(), 2);
    }

    #[test]
    fn tiny_widths_keep_ten_characters_of_text() {
        let line = line_with(500, &[(250, "N")]);
        let f = fragments(&line, &[(250, 251)], 5).unwrap();
        assert_eq!(f[0].text.chars().count(), MIN_TEXT);
        assert!(f[0].text.contains('N'));
    }

    #[test]
    fn edges_and_multibyte() {
        let line = format!("ñ{}", "é".repeat(500));
        let f = fragments(&line, &[(0, 2)], 60).unwrap();
        assert_eq!(f[0].omitted_before, 0);
        assert!(f[0].text.starts_with('ñ'));
        let end = line.len();
        let g = fragments(&line, &[(end - 2, end)], 60).unwrap();
        assert_eq!(g[0].omitted_after, 0);
    }

    #[test]
    fn inverted_search_without_submatches_shows_the_start() {
        let f = fragments(&"z".repeat(500), &[], 60).unwrap();
        assert_eq!(
            (f.len(), f[0].omitted_before, f[0].submatches.len()),
            (1, 0, 0)
        );
    }
}
