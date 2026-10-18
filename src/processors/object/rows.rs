//! The rows of an object file (spec §5.7): each printable string that holds Matches, as
//! `strings` reads it, located by the hex offset of its first Match (M4 plan, D6).
use crate::model::{Line, Submatch};
use crate::trail::LineIndex;
use std::ops::Range;

/// The most bytes a row shows on each side of its Matches; longer strings are cut there (the
/// Fragments of spec §6.3 then fit the row to the width).
pub const SIDE: usize = 4096;

/// The end of the printable character that starts at `i`: printable ASCII, a tab, or a valid
/// UTF-8 sequence of a character that is not a control.
fn char_after(b: &[u8], i: usize) -> Option<usize> {
    let c = *b.get(i)?;
    if c == b'\t' || (0x20..0x7f).contains(&c) {
        return Some(i + 1);
    }
    let w = match c {
        0xc2..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf4 => 4,
        _ => return None,
    };
    let s = std::str::from_utf8(b.get(i..i + w)?).ok()?;
    (!s.chars().next()?.is_control()).then_some(i + w)
}

/// The start of the printable character that ends at `i` (exclusive), as `char_after` reads it.
fn char_before(b: &[u8], i: usize) -> Option<usize> {
    (1..=4.min(i)).find_map(|w| (char_after(b, i - w) == Some(i)).then_some(i - w))
}

/// The rows of `hits` (absolute byte ranges of Matches in `bytes`, in file order): one `Line`
/// per printable string, its `owned` bytes the string with each control byte of a Match shown
/// as `.`, its submatches relative to the string, and `line_number` the line `rg -a` would
/// give it. At most `limit` rows (`-m`).
pub fn rows(
    bytes: &[u8],
    hits: &[Range<usize>],
    limit: Option<u64>,
    lines: &LineIndex,
) -> Vec<Line> {
    let mut out: Vec<(Range<usize>, Vec<Range<usize>>)> = Vec::new();
    for h in hits {
        let h = h.start.min(bytes.len())..h.end.min(bytes.len());
        // A Match joins the row before it when it starts inside the row's window, or when the
        // bytes from the row's last Match to it are printable: one string is one row, however
        // far apart its Matches are (M4 plan r2, Codex #5).
        if let Some((w, subs)) = out.last_mut()
            && h.start >= w.start
            && (h.start < w.end.max(w.start + 1)
                || printable(bytes, subs.last().map_or(w.start, |s| s.end), h.start))
        {
            if h.end > w.end {
                w.end = extend_right(bytes, h.end);
            }
            subs.push(h);
            continue;
        }
        if limit.is_some_and(|m| out.len() as u64 >= m) {
            break;
        }
        let mut start = h.start;
        while h.start - start < SIDE
            && let Some(s) = char_before(bytes, start)
        {
            start = s;
        }
        // A row never starts inside the previous one.
        if let Some((w, _)) = out.last() {
            start = start.max(w.end);
        }
        out.push((start..extend_right(bytes, h.end), vec![h]));
    }
    out.into_iter()
        .map(|(w, subs)| {
            let mut owned = bytes[w.clone()].to_vec();
            for s in &subs {
                for b in &mut owned[s.start - w.start..s.end - w.start] {
                    if *b < 0x20 && *b != b'\t' || *b == 0x7f {
                        *b = b'.';
                    }
                }
            }
            Line {
                line_number: lines.line_of(subs[0].start),
                absolute_offset: w.start as u64,
                len: w.len(),
                owned: Some(owned.into()),
                submatches: subs
                    .iter()
                    .map(|s| Submatch {
                        start: s.start - w.start,
                        end: s.end - w.start,
                    })
                    .collect(),
            }
        })
        .collect()
}

/// Whether `bytes[from..to]` is printable characters only.
fn printable(bytes: &[u8], from: usize, to: usize) -> bool {
    let mut i = from;
    while i < to {
        match char_after(bytes, i) {
            Some(e) if e <= to => i = e,
            _ => return false,
        }
    }
    true
}

fn extend_right(bytes: &[u8], from: usize) -> usize {
    let mut end = from;
    while end - from < SIDE
        && let Some(e) = char_after(bytes, end)
    {
        end = e;
    }
    end
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each row's text, submatches and offset.
    type Shown = Vec<(String, Vec<(usize, usize)>, u64)>;

    fn shown(bytes: &[u8], hits: &[Range<usize>]) -> Shown {
        rows(bytes, hits, None, &LineIndex::new(bytes))
            .into_iter()
            .map(|l| {
                (
                    String::from_utf8_lossy(l.owned.as_deref().unwrap()).into_owned(),
                    l.submatches.iter().map(|s| (s.start, s.end)).collect(),
                    l.absolute_offset,
                )
            })
            .collect()
    }

    fn find(hay: &[u8], needle: &[u8]) -> Vec<Range<usize>> {
        memchr::memmem::find_iter(hay, needle)
            .map(|i| i..i + needle.len())
            .collect()
    }

    #[test]
    fn a_row_is_the_printable_string_around_its_matches() {
        let b = b"\x01\x02version=1.2.3-needle\0\xffneedle and needle\0";
        let r = shown(b, &find(b, b"needle"));
        assert_eq!(
            r,
            [
                ("version=1.2.3-needle".into(), vec![(14, 20)], 2),
                ("needle and needle".into(), vec![(0, 6), (11, 17)], 24),
            ]
        );
    }

    /// Codex r2 #5: Matches more than `SIDE` apart in one string are one row, even with -m1.
    #[test]
    fn a_long_string_is_one_row_whatever_its_matches_distance() {
        let mut b = b"\0needle".to_vec();
        b.extend(vec![b'a'; 5000]);
        b.extend(b"needle\0");
        let hits = find(&b, b"needle");
        let r = rows(&b, &hits, Some(1), &LineIndex::new(&b));
        assert_eq!(r.len(), 1);
        let subs: Vec<(usize, usize)> = r[0].submatches.iter().map(|s| (s.start, s.end)).collect();
        assert_eq!(
            (r[0].absolute_offset, subs),
            (1, vec![(0, 6), (5006, 5012)])
        );
    }

    #[test]
    fn control_bytes_inside_a_match_print_as_dots() {
        let b = b"ab\0cd";
        let r = shown(b, std::slice::from_ref(&(1..4)));
        assert_eq!(r, [("ab.cd".into(), vec![(1, 4)], 0)]);
    }

    #[test]
    fn utf8_text_is_printable_and_stray_high_bytes_are_not() {
        let b = "\u{1}año needle\u{0}".as_bytes();
        let r = shown(b, &find(b, b"needle"));
        assert_eq!(r[0].0, "año needle");
        let b = b"\x80\xfeneedle\x9f";
        assert_eq!(shown(b, &find(b, b"needle"))[0].0, "needle");
    }

    #[test]
    fn long_strings_are_cut_at_side_bytes_each_way() {
        let mut b = vec![b'x'; SIDE * 3];
        b.extend(b"needle");
        b.extend(vec![b'y'; SIDE * 3]);
        let r = rows(&b, &find(&b, b"needle"), None, &LineIndex::new(&b));
        assert_eq!(r[0].len, SIDE * 2 + 6);
        assert_eq!(r[0].submatches[0].start, SIDE);
    }

    #[test]
    fn rows_count_toward_the_limit_and_number_lines_as_rg_a() {
        let b = b"one needle\0two\nneedle\0three needle\0";
        let li = LineIndex::new(b);
        let r = rows(b, &find(b, b"needle"), Some(2), &li);
        assert_eq!(r.len(), 2);
        assert_eq!((r[0].line_number, r[1].line_number), (1, 2));
    }

    #[test]
    fn an_empty_match_makes_a_row_of_its_string() {
        let b = b"\0abc\0";
        let r = shown(b, std::slice::from_ref(&(2..2)));
        assert_eq!(r, [("abc".into(), vec![(1, 1)], 1)]);
    }
}
