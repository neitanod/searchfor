//! Rendering of Described files (spec §6).
pub mod flat;
pub mod fragment;
pub mod json;
pub mod style;
pub mod summary;
pub mod tree;

use crate::cli::BinaryChoice;
use crate::model::{Described, Item, Segment, Submatch};
use std::collections::HashMap;
use termcolor::WriteColor;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    Tree,
    Flat,
    Json,
}

#[derive(Debug, Clone)]
pub struct RenderOptions {
    pub layout: Layout,
    /// Show line numbers (tree and flat: on unless -N; JSON: `line_number` null with -N).
    pub line_numbers: bool,
    /// Show file names in the flat layout (-H / -I / ripgrep's default).
    pub with_filename: bool,
    /// Total characters available per output line: the terminal width on a TTY,
    /// `--fragment-width` otherwise. Printers subtract their own prefix (design note on C6).
    pub width: usize,
    /// -M: ripgrep's omission messages instead of Fragments.
    pub max_columns: Option<u64>,
    /// Context is on (-A/-B/-C > 0): print `--` between non-adjacent groups, as ripgrep.
    pub context: bool,
    /// --expand
    pub expand: bool,
    /// --binary / --text / ripgrep's default: picks the binary message (design note 3).
    pub binary: BinaryChoice,
}

/// True when -M omits this line: ripgrep compares the whole line, terminator included
/// (grep-printer 0.3.1 `standard.rs:1054` and `1562`).
pub fn omitted(bytes: &[u8], o: &RenderOptions) -> bool {
    o.max_columns.is_some_and(|m| bytes.len() as u64 > m)
}

/// ripgrep's -M message (grep-printer `standard.rs:1329`): without colour it computes no
/// submatches, so the count only appears when colour is on and the line has submatches.
pub(crate) fn omission_message(subs: usize, is_match: bool, color: bool) -> String {
    if subs == 0 || !color {
        if is_match {
            "[Omitted long matching line]".into()
        } else {
            "[Omitted long context line]".into()
        }
    } else {
        format!("[Omitted long line with {subs} matches]")
    }
}

/// Writes the text of one line for tree and flat: whole, as Fragments (one per output line,
/// each preceded by `prefix`), or ripgrep's omission message with -M.
/// `bytes` is the raw line; `subs` are byte ranges in it; `avail` is the width left after the
/// caller's prefix.
pub fn write_line(
    w: &mut dyn WriteColor,
    prefix: &dyn Fn(&mut dyn WriteColor) -> std::io::Result<()>,
    bytes: &[u8],
    subs: &[(usize, usize)],
    is_match: bool,
    avail: usize,
    o: &RenderOptions,
) -> std::io::Result<()> {
    if omitted(bytes, o) {
        prefix(w)?;
        let msg = omission_message(subs.len(), is_match, w.supports_color());
        w.write_all(msg.as_bytes())?;
        return w.write_all(b"\n");
    }
    let raw = crate::text::strip_terminator(bytes);
    let (text, map) = crate::text::lossy_with_map(raw);
    let subs: Vec<(usize, usize)> = subs
        .iter()
        .map(|&(s, e)| (map[s.min(raw.len())], map[e.min(raw.len())]))
        .collect();
    match fragment::fragments(&text, &subs, avail) {
        None => {
            prefix(w)?;
            style::write_highlighted(w, &text, &subs)?;
            w.write_all(b"\n")
        }
        Some(frags) => {
            for f in frags {
                prefix(w)?;
                if f.omitted_before > 0 {
                    style::write_styled(w, "dim", &format!("…[+{}] ", f.omitted_before))?;
                }
                style::write_highlighted(w, &f.text, &f.submatches)?;
                if f.omitted_after > 0 {
                    style::write_styled(w, "dim", &format!(" […+{}]", f.omitted_after))?;
                }
                w.write_all(b"\n")?;
            }
            Ok(())
        }
    }
}

/// One line for tree or flat: a Match line under one of its Trails, or a context line.
pub(crate) struct Row<'a> {
    pub trail: &'a [Segment],
    pub line_number: u64,
    pub bytes: &'a [u8],
    pub subs: Vec<(usize, usize)>,
    pub is_match: bool,
    /// Context is on and this line does not follow the previous one: print `--` first.
    pub gap_before: bool,
}

/// What tree and flat print in a file's place: its rows, or with --expand its Sections.
pub(crate) enum Piece<'a> {
    Row(Row<'a>),
    Section {
        trail: &'a [Segment],
        start: u64,
        end: u64,
    },
}

/// The items tree and flat print: those before the binary cut (design note 3).
pub(crate) fn printed_items(d: &Described) -> &[Item] {
    let items = &d.file.items;
    match (d.file.stats.binary_offset, d.file.stats.binary_cut) {
        (Some(_), Some(cut)) => &items[..cut.min(items.len())],
        _ => items,
    }
}

/// Trail of submatch `k` of Match `i`; empty when the model has none.
pub(crate) fn trail_of(d: &Described, i: usize, k: usize) -> &[Segment] {
    d.trails
        .get(i)
        .and_then(|t| t.get(k))
        .map_or(&[], |t| t.as_slice())
}

fn subs_of(subs: &[Submatch]) -> Vec<(usize, usize)> {
    subs.iter().map(|s| (s.start, s.end)).collect()
}

/// The rows of a file, in file order. A Match line whose submatches have different Trails
/// gives one row per distinct Trail, each with only its own submatches, except when -M omits
/// the line: then one row under its first Trail with every submatch (design note 5). A
/// context line takes the Trail of the row before it in its group, or else of the row after.
pub(crate) fn rows<'a>(d: &'a Described, o: &RenderOptions) -> Vec<Row<'a>> {
    let mut out: Vec<(Option<&'a [Segment]>, usize, Row<'a>)> = Vec::new();
    let mut prev: Option<u64> = None;
    let (mut block, mut i) = (0, 0);
    for item in printed_items(d) {
        let line_number = match item {
            Item::Match(m) => m.line_number,
            Item::Context(c) => c.line_number,
        };
        let gap = prev.is_some_and(|p| p + 1 != line_number);
        if gap {
            block += 1;
        }
        prev = Some(line_number);
        let gap_before = gap && o.context;
        match item {
            Item::Context(c) => out.push((
                None,
                block,
                Row {
                    trail: &[],
                    line_number,
                    bytes: &c.bytes,
                    subs: subs_of(&c.submatches),
                    is_match: false,
                    gap_before,
                },
            )),
            Item::Match(m) => {
                let mut groups: Vec<(&'a [Segment], Vec<(usize, usize)>)> = Vec::new();
                if m.submatches.is_empty() || omitted(&m.bytes, o) {
                    groups.push((trail_of(d, i, 0), subs_of(&m.submatches)));
                } else {
                    for (k, s) in m.submatches.iter().enumerate() {
                        let t = trail_of(d, i, k);
                        match groups.iter_mut().find(|g| g.0 == t) {
                            Some(g) => g.1.push((s.start, s.end)),
                            None => groups.push((t, vec![(s.start, s.end)])),
                        }
                    }
                }
                for (n, (trail, subs)) in groups.into_iter().enumerate() {
                    out.push((
                        Some(trail),
                        block,
                        Row {
                            trail,
                            line_number,
                            bytes: &m.bytes,
                            subs,
                            is_match: true,
                            gap_before: gap_before && n == 0,
                        },
                    ));
                }
                i += 1;
            }
        }
    }
    for j in 1..out.len() {
        if out[j].0.is_none() && out[j - 1].1 == out[j].1 {
            out[j].0 = out[j - 1].0;
        }
    }
    for j in (0..out.len().saturating_sub(1)).rev() {
        if out[j].0.is_none() && out[j + 1].1 == out[j].1 {
            out[j].0 = out[j + 1].0;
        }
    }
    out.into_iter()
        .map(|(t, _, mut r)| {
            r.trail = t.unwrap_or(&[]);
            r
        })
        .collect()
}

/// With --expand: every distinct Section once, under the Trail of the first submatch that has
/// it, in file order. A Match without any Section keeps its rows.
pub(crate) fn pieces<'a>(d: &'a Described, o: &RenderOptions) -> Vec<Piece<'a>> {
    if !o.expand {
        return rows(d, o).into_iter().map(Piece::Row).collect();
    }
    let mut seen: Vec<(u64, u64)> = Vec::new();
    let mut out = Vec::new();
    let all = rows(d, o);
    let mut i = 0;
    for item in printed_items(d) {
        let Item::Match(m) = item else { continue };
        let secs: Vec<(usize, (u64, u64))> = (0..m.submatches.len().max(1))
            .filter_map(|k| {
                let s = d.sections.get(i).and_then(|v| v.get(k)).copied().flatten();
                s.map(|s| (k, s))
            })
            .collect();
        if secs.is_empty() {
            let line = m.line_number;
            out.extend(
                all.iter()
                    .filter(|r| r.is_match && r.line_number == line)
                    .map(|r| {
                        Piece::Row(Row {
                            subs: r.subs.clone(),
                            gap_before: false,
                            ..*r
                        })
                    }),
            );
        }
        for (k, (start, end)) in secs {
            if !seen.contains(&(start, end)) {
                seen.push((start, end));
                out.push(Piece::Section {
                    trail: trail_of(d, i, k),
                    start,
                    end,
                });
            }
        }
        i += 1;
    }
    out
}

/// The submatches of the printed Match lines, by line number (for --expand).
pub(crate) fn match_lines(d: &Described) -> HashMap<u64, Vec<(usize, usize)>> {
    let mut map: HashMap<u64, Vec<(usize, usize)>> = HashMap::new();
    for item in printed_items(d) {
        if let Item::Match(m) = item {
            map.entry(m.line_number)
                .or_default()
                .extend(subs_of(&m.submatches));
        }
    }
    map
}

/// Lines `start..=end` (1-based) of `src`, terminators included, with their numbers.
pub(crate) fn section_lines(src: &[u8], start: u64, end: u64) -> Vec<(u64, &[u8])> {
    src.split_inclusive(|&b| b == b'\n')
        .zip(1u64..)
        .skip_while(|(_, n)| *n < start)
        .take_while(|(_, n)| *n <= end)
        .map(|(l, n)| (n, l))
        .collect()
}

/// ripgrep's message for a binary file, when it printed something (design note 3).
pub(crate) fn binary_message(d: &Described, o: &RenderOptions) -> Option<String> {
    let offset = d.file.stats.binary_offset?;
    d.file.matches().next()?;
    let convert = match o.binary {
        BinaryChoice::Text => return None,
        BinaryChoice::Binary => true,
        BinaryChoice::Auto => d.file.explicit,
    };
    Some(if convert {
        format!("binary file matches (found \"\\0\" byte around offset {offset})")
    } else {
        format!(
            "WARNING: stopped searching binary file after match (found \"\\0\" byte around offset {offset})"
        )
    })
}

/// The path as printed: its bytes on Unix, as ripgrep.
pub(crate) fn path_bytes(p: &std::path::Path) -> Vec<u8> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        p.as_os_str().as_bytes().to_vec()
    }
    #[cfg(not(unix))]
    {
        p.to_string_lossy().into_owned().into_bytes()
    }
}

pub(crate) fn write_path(w: &mut dyn WriteColor, p: &std::path::Path) -> std::io::Result<()> {
    w.set_color(&style::spec("path"))?;
    w.write_all(&path_bytes(p))?;
    w.reset()
}

/// Hand-built Described values for the printers' tests.
#[cfg(test)]
pub(crate) mod fixture {
    use crate::model::*;

    /// The Source: `lines` joined, each with a `\n`.
    pub fn src(lines: &[&str]) -> Vec<u8> {
        lines
            .iter()
            .flat_map(|l| format!("{l}\n").into_bytes())
            .collect()
    }

    pub fn seg(kind: &str, name: &str, line: u64, end_line: u64) -> Segment {
        Segment {
            kind: kind.into(),
            name: name.into(),
            line,
            end_line,
            level: None,
            signature: None,
        }
    }

    pub fn h(level: u8, name: &str, line: u64, end_line: u64) -> Segment {
        Segment {
            level: Some(level),
            ..seg("heading", name, line, end_line)
        }
    }

    /// Offset and bytes (terminator included) of 1-based line `n`.
    fn line_of(src: &[u8], n: u64) -> (u64, Vec<u8>) {
        let mut off = 0;
        for (i, l) in src.split_inclusive(|&b| b == b'\n').enumerate() {
            if i as u64 + 1 == n {
                return (off as u64, l.to_vec());
            }
            off += l.len();
        }
        panic!("no line {n}")
    }

    /// Line `n` as a Match, with one submatch per occurrence of `needle`.
    pub fn mat(src: &[u8], n: u64) -> Item {
        let (absolute_offset, bytes) = line_of(src, n);
        let submatches = bytes
            .windows(6)
            .enumerate()
            .filter(|(_, w)| *w == b"needle")
            .map(|(i, _)| Submatch {
                start: i,
                end: i + 6,
            })
            .collect();
        Item::Match(LineMatch {
            line_number: n,
            absolute_offset,
            bytes,
            submatches,
        })
    }

    /// Line `n` as an inverted Match (`-v`).
    pub fn inverted(src: &[u8], n: u64) -> Item {
        let (absolute_offset, bytes) = line_of(src, n);
        Item::Match(LineMatch {
            line_number: n,
            absolute_offset,
            bytes,
            submatches: Vec::new(),
        })
    }

    pub fn ctx(src: &[u8], n: u64) -> Item {
        let (absolute_offset, bytes) = line_of(src, n);
        Item::Context(ContextLine {
            line_number: n,
            absolute_offset,
            bytes,
            submatches: Vec::new(),
        })
    }

    pub fn described(src: &[u8], items: Vec<Item>, trails: Vec<Vec<Trail>>) -> Described {
        let matches: Vec<&LineMatch> = items
            .iter()
            .filter_map(|i| match i {
                Item::Match(m) => Some(m),
                Item::Context(_) => None,
            })
            .collect();
        let stats = FileStats {
            bytes_searched: src.len() as u64,
            matched_lines: matches.len() as u64,
            matches: matches.iter().map(|m| m.submatches.len() as u64).sum(),
            elapsed: std::time::Duration::from_nanos(28_123),
            ..FileStats::default()
        };
        Described {
            file: FileMatches {
                path: "docs/a.md".into(),
                explicit: true,
                items,
                stats,
            },
            format: Some("markdown".into()),
            trails,
            sections: Vec::new(),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use termcolor::{Ansi, NoColor};

    pub(crate) fn opts() -> RenderOptions {
        RenderOptions {
            layout: Layout::Tree,
            line_numbers: true,
            with_filename: true,
            width: 200,
            max_columns: None,
            context: false,
            expand: false,
            binary: BinaryChoice::Auto,
        }
    }

    fn pre(w: &mut dyn WriteColor) -> std::io::Result<()> {
        w.write_all(b"P:")
    }

    fn line(
        color: bool,
        bytes: &[u8],
        subs: &[(usize, usize)],
        is_match: bool,
        avail: usize,
        o: &RenderOptions,
    ) -> String {
        let out = if color {
            let mut w = Ansi::new(Vec::new());
            write_line(&mut w, &pre, bytes, subs, is_match, avail, o).unwrap();
            w.into_inner()
        } else {
            let mut w = NoColor::new(Vec::new());
            write_line(&mut w, &pre, bytes, subs, is_match, avail, o).unwrap();
            w.into_inner()
        };
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn omitted_counts_the_terminator() {
        let o = RenderOptions {
            max_columns: Some(8),
            ..opts()
        };
        assert!(omitted(b"a needle\n", &o));
        assert!(!omitted(b"a needl\n", &o));
        assert!(!omitted(b"a needle\n", &opts()));
    }

    #[test]
    fn omission_messages_as_ripgrep() {
        let o = RenderOptions {
            max_columns: Some(5),
            ..opts()
        };
        let b = b"c needle needle\n";
        let subs = [(2, 8), (9, 15)];
        // Without colour ripgrep computes no submatches.
        assert_eq!(
            line(false, b, &subs, true, 80, &o),
            "P:[Omitted long matching line]\n"
        );
        assert_eq!(
            line(false, b, &[], false, 80, &o),
            "P:[Omitted long context line]\n"
        );
        // With colour it counts them; an inverted Match or a context line has none.
        assert_eq!(
            line(true, b, &subs, true, 80, &o),
            "P:[Omitted long line with 2 matches]\n"
        );
        assert_eq!(
            line(true, b, &[], true, 80, &o),
            "P:[Omitted long matching line]\n"
        );
        assert_eq!(
            line(true, b, &[], false, 80, &o),
            "P:[Omitted long context line]\n"
        );
        // -v: context lines are the matching ones and carry submatches.
        assert_eq!(
            line(true, b, &subs, false, 80, &o),
            "P:[Omitted long line with 2 matches]\n"
        );
    }

    #[test]
    fn short_line_is_written_whole_without_terminator_doubling() {
        assert_eq!(
            line(false, b"a needle\n", &[(2, 8)], true, 80, &opts()),
            "P:a needle\n"
        );
        assert_eq!(
            line(false, b"no eol", &[], false, 80, &opts()),
            "P:no eol\n"
        );
    }

    #[test]
    fn invalid_utf8_does_not_shift_the_highlight() {
        let out = line(true, b"caf\xe9 NEEDLE x\n", &[(5, 11)], true, 80, &opts());
        assert_eq!(out, "P:caf\u{FFFD} \x1b[0m\x1b[1m\x1b[31mNEEDLE\x1b[0m x\n");
    }

    #[test]
    fn long_line_becomes_fragments_each_with_the_prefix() {
        let mut s = "x".repeat(3000);
        s.replace_range(1000..1001, "A");
        s.replace_range(2500..2501, "C");
        s.push('\n');
        let out = line(
            false,
            s.as_bytes(),
            &[(1000, 1001), (2500, 2501)],
            true,
            60,
            &opts(),
        );
        let rows: Vec<&str> = out.lines().collect();
        assert_eq!(rows.len(), 2, "{out}");
        for r in &rows {
            assert!(r.starts_with("P:…[+"), "{r}");
            assert!(r.ends_with(']'), "{r}");
            assert!(r.chars().count() - 2 <= 60, "{r}");
        }
        assert!(rows[0].contains('A') && rows[1].contains('C'));
    }
}
