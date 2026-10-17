//! subtitles: the cues of SubRip (.srt) and WebVTT (.vtt) files (spec §5.6). One pass over the
//! lines, as far as the last anchor; a cue is the block of a timing line.
use super::Processor;
use crate::model::{Input, Name, Segment, Trail};
use std::ops::Range;

pub struct Subtitles;

/// Characters of a cue identifier used in its name.
pub const ID_CHARS: usize = 40;

/// The end of a timestamp starting at `i`: 2 or 3 groups of digits joined by `:`, then `,` or
/// `.` and 1 to 3 digits (`00:01:02,500`, `01:02.500`).
fn timestamp(s: &[u8], mut i: usize) -> Option<usize> {
    let digits = |i: &mut usize, max: usize| {
        let s0 = *i;
        while *i < s.len() && s[*i].is_ascii_digit() && *i - s0 < max {
            *i += 1;
        }
        *i > s0
    };
    let mut groups = 0;
    loop {
        if !digits(&mut i, 9) {
            return None;
        }
        groups += 1;
        if i < s.len() && s[i] == b':' && groups < 3 {
            i += 1;
        } else {
            break;
        }
    }
    if groups < 2 || i >= s.len() || !matches!(s[i], b',' | b'.') {
        return None;
    }
    i += 1;
    digits(&mut i, 3).then_some(i)
}

/// A timing line `START --> END [settings]`: the ranges of START and END within `line`.
pub fn timing(line: &[u8]) -> Option<(Range<usize>, Range<usize>)> {
    let skip = |mut i: usize| {
        while i < line.len() && matches!(line[i], b' ' | b'\t') {
            i += 1;
        }
        i
    };
    let a = skip(0);
    let a_end = timestamp(line, a)?;
    let arrow = skip(a_end);
    if !line[arrow..].starts_with(b"-->") {
        return None;
    }
    let b = skip(arrow + 3);
    let b_end = timestamp(line, b)?;
    if b_end < line.len() && !matches!(line[b_end], b' ' | b'\t') {
        return None;
    }
    Some((a..a_end, b..b_end))
}

/// True when `line` starts with the word `w`, followed by white space or nothing.
fn block_word(line: &[u8], w: &[u8]) -> bool {
    line.starts_with(w) && line.get(w.len()).is_none_or(|b| b.is_ascii_whitespace())
}

/// One cue: its Section (`range`), its identifier line, its timestamps (Source ranges), its
/// 1-based position, and its lines.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Cue {
    range: Range<usize>,
    id: Option<Range<usize>>,
    start: Range<usize>,
    end: Range<usize>,
    ordinal: u64,
    lines: (u64, u64),
}

impl Cue {
    /// The identifier as the name shows it, trimmed and cut at `ID_CHARS`, with the pieces
    /// that map it to the Source (M3 review r1, Codex #6); the position when there is none.
    fn id(&self, src: &[u8]) -> (String, crate::text::Pieces) {
        let Some(r) = &self.id else {
            return (self.ordinal.to_string(), Vec::new());
        };
        let raw = &src[r.clone()];
        let lead = raw.len() - raw.trim_ascii_start().len();
        let trimmed = raw.trim_ascii();
        let s0 = r.start + lead;
        // Built only up to its cut: an identifier of megabytes costs no memory per invalid byte
        // (M3 code review r2, Codex #1).
        let mut n = crate::text::CutName::new(ID_CHARS);
        n.push_lossy(src, s0..s0 + trimmed.len());
        n.finish()
    }

    fn id_text(&self, src: &[u8]) -> String {
        self.id(src).0
    }

    fn segment(&self, src: &[u8]) -> Segment {
        let id = self.id_text(src);
        let start = String::from_utf8_lossy(&src[self.start.clone()]);
        let end = String::from_utf8_lossy(&src[self.end.clone()]);
        Segment {
            kind: "cue".into(),
            name: format!("cue {id} {start}"),
            line: self.lines.0,
            end_line: self.lines.1,
            level: None,
            signature: Some(format!("cue {id} {start} --> {end}")),
        }
    }
}

/// Calls `f` with every cue in file order until it returns false.
fn cues(src: &[u8], mut f: impl FnMut(&Cue) -> bool) {
    // The cue being read; its range ends after its last non-blank line.
    let mut open: Option<Cue> = None;
    // The previous line: (range without terminator, blank, its line number, a timing line);
    // and whether the line before it was blank or absent.
    let mut prev: Option<(Range<usize>, bool, u64, bool)> = None;
    let mut before_prev_blank = true;
    let mut ordinal = 0;
    let mut pos = 0;
    let mut line_no = 0u64;
    // WebVTT (a file that starts with `WEBVTT`): its header and its NOTE, STYLE and REGION
    // blocks hold no cue, even when a line in them looks like a timing line (M3 review r1,
    // Codex #3). `excluded` is true inside one, up to the blank line that ends it.
    let vtt = block_word(src, b"WEBVTT");
    let mut excluded = false;
    while pos < src.len() {
        line_no += 1;
        let nl = memchr::memchr(b'\n', &src[pos..]).map_or(src.len(), |k| pos + k);
        let next = (nl + 1).min(src.len());
        let content = pos..if nl > pos && src[nl - 1] == b'\r' {
            nl - 1
        } else {
            nl
        };
        let line = &src[content.clone()];
        let blank = line.iter().all(|b| matches!(b, b' ' | b'\t'));
        let block_start = prev.as_ref().is_none_or(|(_, b, _, _)| *b);
        if vtt
            && !blank
            && block_start
            && (line_no == 1
                || [&b"NOTE"[..], b"STYLE", b"REGION"]
                    .iter()
                    .any(|w| block_word(line, w)))
        {
            excluded = true;
        }
        let tm = if excluded { None } else { timing(line) };
        if excluded {
            excluded = !blank;
        } else if let Some((a, b)) = tm.clone() {
            // The line before is the identifier when it starts a block and is not a timing
            // line itself (M3 review r1, Fable #4).
            let id = prev
                .as_ref()
                .filter(|(_, pb, _, pt)| !pb && !pt && before_prev_blank)
                .map(|(r, _, n, _)| (r.clone(), *n));
            // A timing line right after a cue's text starts the next cue (a missing blank line).
            if let Some(c) = open.take()
                && !f(&c)
            {
                return;
            }
            ordinal += 1;
            let (start, start_line) = match &id {
                Some((r, n)) => (r.start, *n),
                None => (pos, line_no),
            };
            open = Some(Cue {
                range: start..next,
                id: id.map(|(r, _)| r),
                start: content.start + a.start..content.start + a.end,
                end: content.start + b.start..content.start + b.end,
                ordinal,
                lines: (start_line, line_no),
            });
        } else if blank {
            if let Some(c) = open.take()
                && !f(&c)
            {
                return;
            }
        } else if let Some(c) = open.as_mut() {
            c.range.end = next;
            c.lines.1 = line_no;
        }
        before_prev_blank = prev.as_ref().is_none_or(|(_, b, _, _)| *b);
        prev = Some((content, blank, line_no, tm.is_some()));
        pos = next;
    }
    if let Some(c) = open {
        f(&c);
    }
}

impl Processor for Subtitles {
    fn trails(
        &self,
        input: &Input,
        _: &toml::Table,
        anchors: &[usize],
    ) -> anyhow::Result<Vec<Trail>> {
        let src = input.bytes;
        let mut out = vec![Vec::new(); anchors.len()];
        let mut order: Vec<usize> = (0..anchors.len()).collect();
        order.sort_by_key(|&i| anchors[i]);
        let mut next = 0;
        cues(src, |c| {
            while next < order.len() && anchors[order[next]] < c.range.end {
                let a = anchors[order[next]];
                if a >= c.range.start {
                    out[order[next]] = vec![c.segment(src)];
                }
                next += 1;
            }
            next < order.len()
        });
        Ok(out)
    }

    fn names(&self, input: &Input, _: &toml::Table) -> anyhow::Result<Vec<Name>> {
        let src = input.bytes;
        let mut names = Vec::new();
        cues(src, |c| {
            let seg = c.segment(src);
            // "cue ", the identifier, " ", the start time: the identifier (when it is an
            // identifier line) and the time come from the Source; the rest is text of the name.
            let (id, id_pieces) = c.id(src);
            let mut pieces: crate::text::Pieces = id_pieces
                .into_iter()
                .map(|(t, s)| (4 + t.start..4 + t.end, s))
                .collect();
            let at = 4 + id.len() + 1;
            pieces.push((at..at + c.start.len(), c.start.clone()));
            names.push(Name {
                text: seg.name.clone(),
                pieces,
                trail: vec![seg],
                // A Match in a cue's name is inside the cue, as in its timing line: an empty
                // label holds nothing.
                #[allow(clippy::single_range_in_vec_init)]
                labels: vec![0..0],
                anchor: c.start.start,
            });
            true
        });
        Ok(names)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn needles(src: &str) -> Vec<Vec<(String, u64, u64)>> {
        let anchors: Vec<usize> = src.match_indices("needle").map(|(i, _)| i).collect();
        let input = Input {
            bytes: src.as_bytes(),
            path: Path::new("x.srt"),
            format: "srt",
            disk: None,
        };
        Subtitles
            .trails(&input, &toml::Table::new(), &anchors)
            .unwrap()
            .iter()
            .map(|t| {
                t.iter()
                    .map(|s| (s.name.clone(), s.line, s.end_line))
                    .collect()
            })
            .collect()
    }

    #[test]
    fn timing_lines() {
        assert_eq!(
            timing(b"00:01:02,500 --> 00:01:04,000"),
            Some((0..12, 17..29))
        );
        assert_eq!(
            timing(b"01:02.500 --> 01:04.000 align:start"),
            Some((0..9, 14..23))
        );
        assert_eq!(timing(b"  0:00:01,5-->0:00:02,25"), Some((2..11, 14..24)));
        assert_eq!(timing(b"00:01 --> 00:02"), None);
        assert_eq!(timing(b"needle --> 00:00:01,000"), None);
        assert_eq!(timing(b"00:00:01,000 --> 00:00:02,000x"), None);
    }

    #[test]
    fn srt_cues_with_crlf_and_a_missing_blank_line() {
        let src = "1\r\n00:00:01,000 --> 00:00:02,000\r\nneedle one\r\n\r\n2\r\n00:00:03,000 --> 00:00:04,000\r\nsecond\r\nneedle two\r\n00:00:05,000 --> 00:00:06,000\r\nneedle three\r\n";
        assert_eq!(
            needles(src),
            [
                vec![("cue 1 00:00:01,000".into(), 1, 3)],
                vec![("cue 2 00:00:03,000".into(), 5, 8)],
                vec![("cue 3 00:00:05,000".into(), 9, 10)],
            ]
        );
    }

    #[test]
    fn vtt_header_notes_and_identifiers() {
        let src = "WEBVTT - needle\n\nNOTE needle here\n\nintro\n00:01.000 --> 00:02.000\nneedle\n\n00:03.000 --> 00:04.000 line:0\n<v Ana>needle\n";
        assert_eq!(
            needles(src),
            [
                vec![],
                vec![],
                vec![("cue intro 00:01.000".into(), 5, 7)],
                vec![("cue 2 00:03.000".into(), 9, 10)]
            ]
        );
    }

    #[test]
    fn a_match_in_the_timing_line_is_inside_the_cue_and_names_are_searchable() {
        let src = "7\n00:00:01,000 --> 00:00:02,000\ntext\n";
        let input = Input {
            bytes: src.as_bytes(),
            path: Path::new("x.srt"),
            format: "srt",
            disk: None,
        };
        let t = Subtitles
            .trails(&input, &toml::Table::new(), &[2, 0])
            .unwrap();
        assert_eq!(t[0][0].name, "cue 7 00:00:01,000");
        assert_eq!(
            t[0][0].signature.as_deref(),
            Some("cue 7 00:00:01,000 --> 00:00:02,000")
        );
        assert_eq!(t[1][0].name, "cue 7 00:00:01,000");
        let n = Subtitles.names(&input, &toml::Table::new()).unwrap();
        assert_eq!(n[0].text, "cue 7 00:00:01,000");
        assert_eq!(n[0].pieces, vec![(4..5, 0..1), (6..18, 2..14)]);
        assert_eq!(n[0].trail_at(2).len(), 1);
    }

    #[test]
    fn vtt_blocks_hold_no_cue_even_with_a_timing_line() {
        let src = "WEBVTT\n00:00.000 --> 00:00.500\nneedle\n\nNOTE\n00:01.000 --> 00:02.000\nneedle\n\nSTYLE\n00:01.000 --> 00:02.000\n::cue { needle }\n\n00:03.000 --> 00:04.000\nneedle\n";
        assert_eq!(
            needles(src),
            [
                vec![],
                vec![],
                vec![],
                vec![("cue 1 00:03.000".into(), 13, 14)]
            ]
        );
        // Without the WEBVTT header (SubRip) a line `NOTE` is an identifier like any other.
        assert_eq!(
            needles("NOTE\n00:01,000 --> 00:02,000\nneedle\n"),
            [vec![("cue NOTE 00:01,000".into(), 1, 3)]]
        );
    }

    #[test]
    fn a_timing_line_is_never_an_identifier() {
        // M3 review r1, Fable #4: two timing lines in a row are two cues.
        assert_eq!(
            needles("00:00:01,000 --> 00:00:02,000\n00:00:03,000 --> 00:00:04,000\nneedle\n"),
            [vec![("cue 2 00:00:03,000".into(), 2, 3)]]
        );
    }

    #[test]
    fn text_outside_cues_has_no_trail() {
        assert_eq!(
            needles("needle\n\n00:00:01,000 --> 00:00:02,000\nx\n\nneedle\n"),
            [vec![], vec![]]
        );
    }
}
