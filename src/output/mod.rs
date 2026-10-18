//! Rendering of Described files (spec §6).
pub mod flat;
pub mod fragment;
pub mod json;
pub mod stream;
pub mod style;
pub mod summary;
pub mod tree;

use crate::cli::BinaryChoice;
use crate::model::{Described, Item, Line, Segment, Submatch};
use std::borrow::Cow;
use std::cell::OnceCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;
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
/// (grep-printer 0.2.2 `standard.rs:1120` and `1600`).
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

/// A line's text as tree and flat print it, worked out once for all the rows of the line: a
/// line with Matches under many Trails prints one row per Trail, and decoding the line and
/// indexing its characters for each row made those rows cost rows × line (M3 code review r3,
/// Fable #1). Each part is computed when a row first needs it.
pub(crate) struct LineText<'a> {
    /// The raw line, terminator included.
    pub bytes: &'a [u8],
    /// The text, and for a line that is not valid UTF-8 the map from its bytes to the text's.
    decoded: OnceCell<(Cow<'a, str>, Option<Vec<usize>>)>,
    chars: OnceCell<usize>,
    index: OnceCell<Vec<usize>>,
}

impl<'a> LineText<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        LineText {
            bytes,
            decoded: OnceCell::new(),
            chars: OnceCell::new(),
            index: OnceCell::new(),
        }
    }

    fn raw(&self) -> &'a [u8] {
        crate::text::strip_terminator(self.bytes)
    }

    /// Valid UTF-8 is its own text, without the offset map, which costs a word per byte.
    fn decoded(&self) -> &(Cow<'a, str>, Option<Vec<usize>>) {
        self.decoded.get_or_init(|| {
            let raw = self.raw();
            match std::str::from_utf8(raw) {
                Ok(text) => (Cow::Borrowed(text), None),
                Err(_) => {
                    let (text, map) = crate::text::lossy_with_map(raw);
                    (Cow::Owned(text), Some(map))
                }
            }
        })
    }

    /// The text as printed, without its terminator, invalid bytes as U+FFFD.
    pub(crate) fn text(&self) -> &str {
        &self.decoded().0
    }

    /// The characters of `text`.
    pub(crate) fn width(&self) -> usize {
        *self.chars.get_or_init(|| match self.index.get() {
            Some(index) => *index.last().expect("never empty"),
            None => fragment::char_count(self.text()),
        })
    }

    fn index(&self) -> &[usize] {
        self.index.get_or_init(|| fragment::char_index(self.text()))
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
    write_line_text(w, prefix, &LineText::new(bytes), subs, is_match, avail, o)
}

/// `write_line` for a line whose text the rows before may have worked out already.
pub(crate) fn write_line_text(
    w: &mut dyn WriteColor,
    prefix: &dyn Fn(&mut dyn WriteColor) -> std::io::Result<()>,
    line: &LineText,
    subs: &[(usize, usize)],
    is_match: bool,
    avail: usize,
    o: &RenderOptions,
) -> std::io::Result<()> {
    if omitted(line.bytes, o) {
        prefix(w)?;
        let msg = omission_message(subs.len(), is_match, w.supports_color());
        w.write_all(msg.as_bytes())?;
        return w.write_all(b"\n");
    }
    let raw = line.raw();
    let (text, map) = line.decoded();
    let subs: Cow<[(usize, usize)]> = match map {
        // The common case: it fits (its bytes are at least as many as its characters, and
        // Fragments start beyond `avail` characters).
        None if raw.len() <= avail => {
            prefix(w)?;
            style::write_highlighted(w, text, subs)?;
            return w.write_all(b"\n");
        }
        None => Cow::Borrowed(subs),
        Some(map) => Cow::Owned(
            subs.iter()
                .map(|&(s, e)| (map[s.min(raw.len())], map[e.min(raw.len())]))
                .collect(),
        ),
    };
    let frags = if line.width() <= avail {
        None
    } else {
        fragment::fragments_indexed(text, line.index(), &subs, avail)
    };
    match frags {
        None => {
            prefix(w)?;
            style::write_highlighted(w, text, &subs)?;
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

/// Byte ranges of submatches in a line.
type Subs = Vec<(usize, usize)>;

/// One line for tree or flat: a Match line under one of its Trails, or a context line.
pub(crate) struct Row<'a> {
    pub trail: &'a [Segment],
    pub line_number: u64,
    /// The line, shared by the rows of a Match line.
    pub line: Rc<LineText<'a>>,
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

/// A hash of a Trail's Segments, consistent with their equality.
fn trail_hash(trail: &[Segment]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for s in trail {
        s.kind.hash(&mut h);
        s.name.hash(&mut h);
        s.line.hash(&mut h);
        s.end_line.hash(&mut h);
        s.level.hash(&mut h);
        s.signature.hash(&mut h);
    }
    h.finish()
}

/// The submatches of a line grouped by Trail, the groups in the order their Trails first come.
/// A submatch under the Trail of the one before joins its group at once; any other finds its
/// group by the Trail's hash, so a line with a Trail per submatch costs one lookup per
/// submatch and not one comparison per group (M3 code review r3, Fable #1).
pub(crate) fn group_by_trail<T>(subs: impl Iterator<Item = (T, (usize, usize))>) -> Vec<(T, Subs)>
where
    T: std::ops::Deref<Target = [Segment]>,
{
    let mut groups: Vec<(T, Subs)> = Vec::new();
    let mut by_hash: HashMap<u64, Vec<usize>> = HashMap::new();
    let mut last = 0;
    for (t, sub) in subs {
        let Some(g) = groups.get_mut(last) else {
            groups.push((t, vec![sub]));
            continue;
        };
        if (g.0.as_ptr() == t.as_ptr() && g.0.len() == t.len()) || *g.0 == *t {
            g.1.push(sub);
            continue;
        }
        // The index is built once a second Trail comes.
        if groups.len() == 1 {
            by_hash.insert(trail_hash(&groups[0].0), vec![0]);
        }
        let ids = by_hash.entry(trail_hash(&t)).or_default();
        match ids.iter().find(|&&g| *groups[g].0 == *t) {
            Some(&g) => {
                groups[g].1.push(sub);
                last = g;
            }
            None => {
                last = groups.len();
                ids.push(last);
                groups.push((t, vec![sub]));
            }
        }
    }
    groups
}

/// The rows of Match `i`: one per distinct Trail of its submatches, each with only its own
/// submatches, except when -M omits the line: then one row under its first Trail with every
/// submatch (design note 5).
fn match_rows<'a>(
    d: &'a Described,
    i: usize,
    m: &'a Line,
    src: &'a [u8],
    o: &RenderOptions,
) -> Vec<Row<'a>> {
    let bytes = m.bytes(src);
    let groups: Vec<(&'a [Segment], Subs)> = if m.submatches.is_empty() || omitted(bytes, o) {
        vec![(trail_of(d, i, 0), subs_of(&m.submatches))]
    } else {
        group_by_trail(
            m.submatches
                .iter()
                .enumerate()
                .map(|(k, s)| (trail_of(d, i, k), (s.start, s.end))),
        )
    };
    let line = Rc::new(LineText::new(bytes));
    groups
        .into_iter()
        .map(|(trail, subs)| Row {
            trail,
            line_number: m.line_number,
            line: line.clone(),
            subs,
            is_match: true,
            gap_before: false,
        })
        .collect()
}

/// The rows of a file, in file order, produced as they are printed (only the context lines
/// that open a group wait for the Match after them). A context line takes the Trail of the row
/// before it in its group, or else of the row after.
pub(crate) struct Rows<'a> {
    d: &'a Described,
    src: &'a [u8],
    o: &'a RenderOptions,
    items: std::slice::Iter<'a, Item>,
    /// Index of the next Match.
    i: usize,
    prev: Option<u64>,
    /// Trail of the last row of the current group, once it has a Match.
    last: Option<&'a [Segment]>,
    /// Context rows that open the current group, before its first Match.
    pending: Vec<Row<'a>>,
    ready: VecDeque<Row<'a>>,
}

pub(crate) fn rows<'a>(d: &'a Described, src: &'a [u8], o: &'a RenderOptions) -> Rows<'a> {
    Rows {
        d,
        src,
        o,
        items: printed_items(d).iter(),
        i: 0,
        prev: None,
        last: None,
        pending: Vec::new(),
        ready: VecDeque::new(),
    }
}

impl<'a> Rows<'a> {
    fn push(&mut self, item: &'a Item) {
        let line = match item {
            Item::Match(m) | Item::Context(m) => m,
        };
        let gap = self.prev.is_some_and(|p| p + 1 != line.line_number);
        if gap {
            // A group without a Match keeps an empty Trail.
            self.ready.extend(self.pending.drain(..));
            self.last = None;
        }
        self.prev = Some(line.line_number);
        let gap_before = gap && self.o.context;
        match item {
            Item::Context(c) => {
                let row = Row {
                    trail: self.last.unwrap_or(&[]),
                    line_number: c.line_number,
                    line: Rc::new(LineText::new(c.bytes(self.src))),
                    subs: subs_of(&c.submatches),
                    is_match: false,
                    gap_before,
                };
                if self.last.is_some() {
                    self.ready.push_back(row);
                } else {
                    self.pending.push(row);
                }
            }
            Item::Match(m) => {
                let rows = match_rows(self.d, self.i, m, self.src, self.o);
                self.i += 1;
                let first = rows[0].trail;
                for mut r in self.pending.drain(..) {
                    r.trail = first;
                    self.ready.push_back(r);
                }
                for (n, mut r) in rows.into_iter().enumerate() {
                    r.gap_before = gap_before && n == 0;
                    self.last = Some(r.trail);
                    self.ready.push_back(r);
                }
            }
        }
    }
}

impl<'a> Iterator for Rows<'a> {
    type Item = Row<'a>;

    fn next(&mut self) -> Option<Row<'a>> {
        loop {
            if let Some(r) = self.ready.pop_front() {
                return Some(r);
            }
            match self.items.next() {
                Some(item) => self.push(item),
                None if self.pending.is_empty() => return None,
                None => self.ready.extend(self.pending.drain(..)),
            }
        }
    }
}

/// What tree and flat print, produced as it is printed: the rows, or with --expand every
/// distinct Section once, under the Trail of the first submatch that has it, in file order.
/// With --expand, a Match without any Section keeps its rows.
pub(crate) enum Pieces<'a> {
    Rows(Rows<'a>),
    Expand {
        d: &'a Described,
        src: &'a [u8],
        o: &'a RenderOptions,
        items: std::slice::Iter<'a, Item>,
        i: usize,
        seen: HashSet<(u64, u64)>,
        ready: VecDeque<Piece<'a>>,
    },
}

pub(crate) fn pieces<'a>(d: &'a Described, src: &'a [u8], o: &'a RenderOptions) -> Pieces<'a> {
    if !o.expand {
        return Pieces::Rows(rows(d, src, o));
    }
    Pieces::Expand {
        d,
        src,
        o,
        items: printed_items(d).iter(),
        i: 0,
        seen: HashSet::new(),
        ready: VecDeque::new(),
    }
}

impl<'a> Iterator for Pieces<'a> {
    type Item = Piece<'a>;

    fn next(&mut self) -> Option<Piece<'a>> {
        let Pieces::Expand {
            d,
            src,
            o,
            items,
            i,
            seen,
            ready,
        } = self
        else {
            let Pieces::Rows(r) = self else {
                unreachable!()
            };
            return r.next().map(Piece::Row);
        };
        loop {
            if let Some(p) = ready.pop_front() {
                return Some(p);
            }
            let item = items.next()?;
            let Item::Match(m) = item else { continue };
            let secs: Vec<(usize, (u64, u64))> = (0..m.submatches.len().max(1))
                .filter_map(|k| {
                    let s = d.sections.get(*i).and_then(|v| v.get(k)).copied().flatten();
                    s.map(|s| (k, s))
                })
                .collect();
            if secs.is_empty() {
                ready.extend(match_rows(d, *i, m, src, o).into_iter().map(Piece::Row));
            }
            for (k, (start, end)) in secs {
                if seen.insert((start, end)) {
                    ready.push_back(Piece::Section {
                        trail: trail_of(d, *i, k),
                        start,
                        end,
                    });
                }
            }
            *i += 1;
        }
    }
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
pub(crate) fn section_lines(
    src: &[u8],
    start: u64,
    end: u64,
) -> impl Iterator<Item = (u64, &[u8])> {
    src.split_inclusive(|&b| b == b'\n')
        .zip(1u64..)
        .skip_while(move |(_, n)| *n < start)
        .take_while(move |(_, n)| *n <= end)
        .map(|(l, n)| (n, l))
}

/// ripgrep's message for a binary file that had a Match (design note 3): `offset` is where the
/// binary data was found, `explicit` whether the file was named on the command line.
pub(crate) fn binary_message(
    offset: Option<u64>,
    has_match: bool,
    explicit: bool,
    o: &RenderOptions,
) -> Option<String> {
    let offset = offset?;
    if !has_match {
        return None;
    }
    let convert = match o.binary {
        BinaryChoice::Text => return None,
        BinaryChoice::Binary => true,
        BinaryChoice::Auto => explicit,
    };
    Some(if convert {
        format!("binary file matches (found \"\\0\" byte around offset {offset})")
    } else {
        format!(
            "WARNING: stopped searching binary file after match (found \"\\0\" byte around offset {offset})"
        )
    })
}

/// `binary_message` for a Described file.
pub(crate) fn binary_message_of(d: &Described, o: &RenderOptions) -> Option<String> {
    binary_message(
        d.file.stats.binary_offset,
        d.file.matches().next().is_some(),
        d.file.explicit,
        o,
    )
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

    /// Line `n` of `src` as a range of it, without submatches.
    fn ranged(src: &[u8], n: u64) -> (Line, Vec<u8>) {
        let (absolute_offset, bytes) = line_of(src, n);
        let line = Line {
            line_number: n,
            absolute_offset,
            len: bytes.len(),
            owned: None,
            submatches: Vec::new(),
        };
        (line, bytes)
    }

    /// Line `n` as a Match, with one submatch per occurrence of `needle`.
    pub fn mat(src: &[u8], n: u64) -> Item {
        let (line, bytes) = ranged(src, n);
        let submatches = bytes
            .windows(6)
            .enumerate()
            .filter(|(_, w)| *w == b"needle")
            .map(|(i, _)| Submatch {
                start: i,
                end: i + 6,
            })
            .collect();
        Item::Match(Line { submatches, ..line })
    }

    /// Line `n` as an inverted Match (`-v`).
    pub fn inverted(src: &[u8], n: u64) -> Item {
        Item::Match(ranged(src, n).0)
    }

    pub fn ctx(src: &[u8], n: u64) -> Item {
        Item::Context(ranged(src, n).0)
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
    fn submatches_group_by_trail_in_the_order_their_trails_come() {
        use super::fixture::seg;
        let t = |n: &str| vec![seg("row", "row 1", 1, 2), seg("column", n, 1, 2)];
        let (a, b, c) = (t("a"), t("b"), t("c"));
        let subs = [
            (&a, 0),
            (&b, 1),
            (&a.clone(), 2),
            (&c, 3),
            (&b, 4),
            (&b.clone(), 5),
        ];
        let groups = group_by_trail(subs.iter().map(|(t, k)| (t.as_slice(), (*k, k + 1))));
        let got: Vec<(&str, Vec<usize>)> = groups
            .iter()
            .map(|(t, s)| (t[1].name.as_str(), s.iter().map(|x| x.0).collect()))
            .collect();
        assert_eq!(
            got,
            [("a", vec![0, 2]), ("b", vec![1, 4, 5]), ("c", vec![3])]
        );
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
