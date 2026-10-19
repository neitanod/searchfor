//! Types shared by every stage of the pipeline.
use serde::{Deserialize, Serialize};
use std::ops::Range;
use std::path::PathBuf;
use std::time::Duration;

/// One step of a Trail (see CONTEXT.md).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Segment {
    /// "document", "heading", "code", "item", "column"; later milestones add kinds.
    pub kind: String,
    pub name: String,
    /// 1-based first line of the Segment's Section.
    pub line: u64,
    /// 1-based last line of the Section (inclusive).
    pub end_line: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

pub type Trail = Vec<Segment>;

/// Byte range of a submatch, relative to the start of its line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Submatch {
    pub start: usize,
    pub end: usize,
}

/// One line the searcher reported: a Match line or a context line. Its bytes are not copied:
/// they are `len` bytes of the Source at `absolute_offset`, except when a reader search
/// converted binary data (a NUL becomes a line terminator, design note 3), where `owned` keeps
/// the bytes the searcher saw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub line_number: u64,
    /// Absolute byte offset of the start of the line in the Source.
    pub absolute_offset: u64,
    /// Length of the line, including its terminator if it had one.
    pub len: usize,
    /// The line's bytes when they differ from the Source's; None otherwise.
    pub owned: Option<Box<[u8]>>,
    /// A Match line: empty for an inverted search (`-v`). A context line: non-empty only with
    /// `-v`, where context lines are the lines that do match; ripgrep's JSON reports their
    /// submatches.
    pub submatches: Vec<Submatch>,
}

pub type LineMatch = Line;
pub type ContextLine = Line;

impl Line {
    /// A line that is `bytes`, copied (tests and hand-built models).
    pub fn owned(line_number: u64, absolute_offset: u64, bytes: &[u8]) -> Self {
        Self {
            line_number,
            absolute_offset,
            len: bytes.len(),
            owned: Some(bytes.into()),
            submatches: Vec::new(),
        }
    }

    /// The line, terminator included: `owned`, or its range of `src` (clamped, so a Source
    /// that changed under us never panics).
    pub fn bytes<'a>(&'a self, src: &'a [u8]) -> &'a [u8] {
        if let Some(b) = &self.owned {
            return b;
        }
        let start = (self.absolute_offset as usize).min(src.len());
        let end = start.saturating_add(self.len).min(src.len());
        &src[start..end]
    }

    /// One anchor per submatch (absolute byte of its start); the line start when there is none.
    pub fn anchors(&self) -> Vec<usize> {
        let base = self.absolute_offset as usize;
        if self.submatches.is_empty() {
            vec![base]
        } else {
            self.submatches.iter().map(|s| base + s.start).collect()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    Match(LineMatch),
    Context(ContextLine),
}

/// What ripgrep reports per file in its JSON `end` message.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileStats {
    pub bytes_searched: u64,
    pub matched_lines: u64,
    pub matches: u64,
    pub binary_offset: Option<u64>,
    /// Items recorded before the binary data was detected (design note 3).
    pub binary_cut: Option<usize>,
    pub elapsed: Duration,
}

/// Everything the searcher found in one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileMatches {
    /// The path as it must be printed (relative as given, or `<stdin>`).
    pub path: PathBuf,
    /// True when the path was named on the command line (binary policy, design note 3).
    pub explicit: bool,
    pub items: Vec<Item>,
    pub stats: FileStats,
}

impl FileMatches {
    pub fn matches(&self) -> impl Iterator<Item = &LineMatch> {
        self.items.iter().filter_map(|i| match i {
            Item::Match(m) => Some(m),
            Item::Context(_) => None,
        })
    }
}

/// A file after its Processor and the filters ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Described {
    pub file: FileMatches,
    /// Name of the Format, or None for Plain.
    pub format: Option<String>,
    /// `trails[i][k]`: Trail of submatch `k` of the `i`-th `Item::Match` (design note 5).
    pub trails: Vec<Vec<Trail>>,
    /// With `--expand`, `sections[i][k]`: the Section of submatch `k` of Match `i`
    /// (1-based inclusive lines), aligned with `trails`.
    pub sections: Vec<Vec<Option<(u64, u64)>>>,
    /// The rows are an object file's printable strings (spec §5.7): tree and one-line output
    /// print each at the hex offset of its first Match, where they print a line number.
    pub offsets: bool,
    /// An object file's `-r` (M5a plan, D4): each Match of the search, by its range in the
    /// Source, with its replacement, made in one pass over the Match's line as grep-printer's
    /// `Replacer` makes it, so with the groups the search found (review r4 of the plan, Codex
    /// #1 and #2). Empty otherwise.
    pub replacements: Vec<((usize, usize), Vec<u8>)>,
}

/// What a Processor receives about one file (design note 6). External Processors (M5) need
/// the path and the Format name; built-in ones mostly use the bytes.
#[derive(Debug, Clone, Copy)]
pub struct Input<'a> {
    pub bytes: &'a [u8],
    pub path: &'a std::path::Path,
    pub format: &'a str,
    /// The file on disk whose bytes are exactly `bytes`, for Processors that read the file
    /// themselves (Helper Processors, spec §4.4). None for stdin, for a Source whose BOM was
    /// removed or whose UTF-16 was transcoded, and for extracted text.
    pub disk: Option<&'a std::path::Path>,
}

/// A Segment name, for `--in-headings` (design note 6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Name {
    /// The name as shown: text pieces joined, a soft line break becoming one space.
    pub text: String,
    /// Where each piece of `text` comes from: `(range in text, range in the Source)`. The two
    /// ranges have the same length when the text is verbatim, and differ when the Processor
    /// decoded it (an entity like `&lt;` is 1 byte of text over 4 of Source). A setext heading
    /// spanning two lines has one piece per line.
    pub pieces: Vec<(Range<usize>, Range<usize>)>,
    /// Trail of the named Segment, the Segment itself last.
    pub trail: Trail,
    /// The label of each Segment of `trail` (the bytes that name it, design note 7): a Match
    /// in the name leaves out the Segments whose label holds it, as `trail_for` does. Empty
    /// when unknown: then only the named Segment is left out.
    pub labels: Vec<Range<usize>>,
    /// Where an empty hit lands when no piece contains it (a name with no text, like `###`):
    /// the end of the heading's markers.
    pub anchor: usize,
}

impl Name {
    /// The Trail of a Match in the name that starts at byte `at` of the Source: the Segments
    /// of `trail` whose label does not hold it (code review r1, Fable #4).
    pub fn trail_at(&self, at: usize) -> Trail {
        if self.labels.len() != self.trail.len() {
            return self.trail[..self.trail.len().saturating_sub(1)].to_vec();
        }
        self.trail
            .iter()
            .zip(&self.labels)
            .filter(|(_, l)| !l.contains(&at))
            .map(|(s, _)| s.clone())
            .collect()
    }

    /// Source ranges covered by the text range `hit`, one per piece it touches. A verbatim
    /// piece maps byte for byte; a decoded piece maps to its whole Source range. An empty hit
    /// (`^`, `$`) at text position `p` maps to an empty range: inside a verbatim piece at the
    /// corresponding byte; at the start of a decoded piece to its Source start, at its end to its
    /// Source end; with no piece (an empty name) at `anchor`.
    // The single-element vectors below hold one (possibly empty) range, not a range of indices.
    #[allow(clippy::single_range_in_vec_init)]
    pub fn source_ranges(&self, hit: Range<usize>) -> Vec<Range<usize>> {
        if hit.is_empty() {
            let p = hit.start;
            let at = self
                .pieces
                .iter()
                .find(|(t, _)| t.start <= p && p <= t.end)
                .map(|(t, src)| {
                    if t.len() == src.len() {
                        src.start + (p - t.start)
                    } else if p == t.end && !t.is_empty() {
                        src.end
                    } else {
                        src.start
                    }
                })
                .unwrap_or(self.anchor);
            return vec![at..at];
        }
        self.pieces
            .iter()
            .filter_map(|(t, src)| {
                let (s, e) = (hit.start.max(t.start), hit.end.min(t.end));
                (s < e).then(|| {
                    if t.len() == src.len() {
                        src.start + (s - t.start)..src.start + (e - t.start)
                    } else {
                        src.clone()
                    }
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchors_per_submatch_or_line_start() {
        let m = LineMatch {
            submatches: vec![
                Submatch { start: 2, end: 8 },
                Submatch { start: 11, end: 17 },
            ],
            ..Line::owned(3, 100, b"a needle b needle\n")
        };
        assert_eq!(m.anchors(), vec![102, 111]);
        let inverted = LineMatch {
            submatches: vec![],
            ..m
        };
        assert_eq!(inverted.anchors(), vec![100]);
    }

    #[test]
    fn bytes_come_from_the_source_or_the_copy() {
        let src = b"one\ntwo\n";
        let ranged = Line {
            line_number: 2,
            absolute_offset: 4,
            len: 4,
            owned: None,
            submatches: Vec::new(),
        };
        assert_eq!(ranged.bytes(src), b"two\n");
        // A Source shorter than the line (it changed under us) is clamped.
        assert_eq!(ranged.bytes(b"one\nt"), b"t");
        assert_eq!(ranged.bytes(b"on"), b"");
        let copied = Line::owned(2, 4, b"t\nwo\n");
        assert_eq!(copied.bytes(src), b"t\nwo\n");
    }

    #[test]
    fn segment_json_skips_empty_options() {
        let s = Segment {
            kind: "heading".into(),
            name: "Install".into(),
            line: 1,
            end_line: 9,
            level: Some(1),
            signature: None,
        };
        assert_eq!(
            serde_json::to_string(&s).unwrap(),
            r#"{"kind":"heading","name":"Install","line":1,"end_line":9,"level":1}"#
        );
    }

    #[test]
    fn source_ranges_map_verbatim_and_decoded_pieces() {
        // "a<b" decoded from "a&lt;b", which starts at byte 10 of the Source.
        let name = Name {
            text: "a<b".into(),
            pieces: vec![(0..1, 10..11), (1..2, 11..15), (2..3, 15..16)],
            trail: Vec::new(),
            labels: Vec::new(),
            anchor: 7,
        };
        assert_eq!(name.source_ranges(1..2), vec![11..15]);
        assert_eq!(name.source_ranges(0..3), vec![10..11, 11..15, 15..16]);
        assert_eq!(name.source_ranges(2..3), vec![15..16]);
        // Empty hits: inside a verbatim piece, at the end of a decoded one.
        assert_eq!(name.source_ranges(1..1), vec![11..11]);
        assert_eq!(name.source_ranges(2..2), vec![15..15]);
        let empty = Name {
            text: String::new(),
            pieces: Vec::new(),
            trail: Vec::new(),
            labels: Vec::new(),
            anchor: 7,
        };
        assert_eq!(empty.source_ranges(0..0), vec![7..7]);
    }
}
