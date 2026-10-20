//! Rendering of Described files (spec §6).
pub mod flat;
pub mod fragment;
pub mod hyperlink;
pub mod json;
pub mod replace;
pub mod rg;
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
    /// How each row looks: ripgrep's output flags in the Trail layouts (M5a plan, D4).
    pub look: Look,
}

/// ripgrep's output flags as the tree and one-line layouts print them (M5a plan, D4). The
/// default is what M1–M4 print.
#[derive(Debug, Clone)]
pub struct Look {
    /// `--column`: the 1-based column of the row's first Match after its line number.
    pub column: bool,
    /// `-b`: the byte offset of the row's line (with `-o`, of its Match) after the column.
    pub byte_offset: bool,
    /// `-o`: one row per Match, its text the matched bytes alone.
    pub only_matching: bool,
    /// `--vimgrep`: one row per Match, its text the whole line.
    pub per_match: bool,
    /// `--trim`: the text without its leading ASCII white space.
    pub trim: bool,
    /// `--max-columns-preview`: with `-M`, the start of a long line before the omission.
    pub max_columns_preview: bool,
    /// `-r`: the Matches of each line replaced (`replace::Replacer`).
    pub replace: Option<std::sync::Arc<replace::Replacer>>,
    /// `--field-match-separator`: after the path and each number of a Match row (`:`).
    pub sep_match: Vec<u8>,
    /// `--field-context-separator`: the same for a context row (`-`).
    pub sep_context: Vec<u8>,
    /// `--context-separator` (`--`), None with `--no-context-separator`.
    pub sep_group: Option<Vec<u8>>,
    /// `-0`: the byte printed after each path in place of the field separator (one-line) or of
    /// the line break (tree's header).
    pub path_terminator: Option<u8>,
    /// `--path-separator`: the byte printed in place of each `/` of a path.
    pub path_separator: Option<u8>,
    /// `--colors`: the colours of path, line, column, match, Trail and markers.
    pub palette: style::Palette,
    /// `--hyperlink-format`: paths and line numbers as links on a terminal with colour.
    pub hyperlinks: Option<std::sync::Arc<hyperlink::Hyperlinks>>,
    /// `--crlf`: a row's `\r\n` is its terminator.
    pub crlf: bool,
    /// `--null-data`: an item is a NUL-terminated record, whose `\n` lines are rows that keep
    /// its number.
    pub null_data: bool,
    /// `-U`: a Match may span lines, each a row numbered as rg numbers it.
    pub multi_line: bool,
}

impl Look {
    /// Nothing but M1–M4's: no prelude field, separator or text change of M5's (the
    /// streaming printer's fast path).
    pub fn plain(&self) -> bool {
        !self.column
            && !self.byte_offset
            && !self.only_matching
            && !self.per_match
            && !self.trim
            && !self.max_columns_preview
            && self.replace.is_none()
            && self.sep_match == b":"
            && self.sep_context == b"-"
            && self.sep_group.as_deref() == Some(&b"--"[..])
            && self.path_terminator.is_none()
            && self.path_separator.is_none()
            && self.hyperlinks.is_none()
            && !self.crlf
            && !self.null_data
            && !self.multi_line
    }
}

impl Default for Look {
    fn default() -> Self {
        Look {
            column: false,
            byte_offset: false,
            only_matching: false,
            per_match: false,
            trim: false,
            max_columns_preview: false,
            replace: None,
            sep_match: b":".to_vec(),
            sep_context: b"-".to_vec(),
            sep_group: Some(b"--".to_vec()),
            path_terminator: None,
            path_separator: None,
            palette: style::Palette::default(),
            hyperlinks: None,
            crlf: false,
            null_data: false,
            multi_line: false,
        }
    }
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
/// Fable #1). Each part is computed when a row first needs it. The bytes are the line's, or
/// with `-r` the line with its Matches replaced (M5a plan, D4). Its UTF-8 is checked once too,
/// and `--trim`'s line made once (review r5 of the M5a plan).
pub(crate) struct LineText<'a> {
    /// The raw line, terminator included.
    bytes: Cow<'a, [u8]>,
    /// The terminator's bytes at the end of `bytes`, which are not printed.
    cut: usize,
    /// The text, invalid bytes as U+FFFD, and for a line that is not valid UTF-8 the map from
    /// its bytes to the text's (None for valid UTF-8, which is its own text).
    decoded: OnceCell<(Cow<'a, str>, Option<Vec<usize>>)>,
    chars: OnceCell<usize>,
    index: OnceCell<Vec<usize>>,
    /// The line without the white space `--trim` leaves out (`trimmed`), None when there is
    /// none.
    trim: OnceCell<Option<Box<LineText<'a>>>>,
}

impl<'a> LineText<'a> {
    /// A line whose terminator, if any, is one `\n`.
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        let cut = usize::from(bytes.ends_with(b"\n"));
        Self::with_cut(Cow::Borrowed(bytes), cut)
    }

    /// A line ending in `cut` bytes of terminator (`\r\n` with `--crlf`, NUL with
    /// `--null-data`), owned or borrowed.
    pub(crate) fn with_cut(bytes: Cow<'a, [u8]>, cut: usize) -> Self {
        LineText {
            cut: cut.min(bytes.len()),
            bytes,
            decoded: OnceCell::new(),
            chars: OnceCell::new(),
            index: OnceCell::new(),
            trim: OnceCell::new(),
        }
    }

    /// The line, terminator included.
    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    fn raw(&self) -> &[u8] {
        &self.bytes[..self.bytes.len() - self.cut]
    }

    fn decoded(&self) -> &(Cow<'a, str>, Option<Vec<usize>>) {
        self.decoded.get_or_init(|| {
            let end = self.bytes.len() - self.cut;
            let valid = match &self.bytes {
                Cow::Borrowed(b) => {
                    let b: &'a [u8] = b;
                    std::str::from_utf8(&b[..end]).ok().map(Cow::Borrowed)
                }
                Cow::Owned(b) => std::str::from_utf8(&b[..end])
                    .ok()
                    .map(|t| Cow::Owned(t.to_owned())),
            };
            match valid {
                Some(text) => (text, None),
                None => {
                    let (text, map) = crate::text::lossy_with_map(self.raw());
                    (Cow::Owned(text), Some(map))
                }
            }
        })
    }

    /// The text as printed, without its terminator, invalid bytes as U+FFFD.
    pub(crate) fn text(&self) -> &str {
        &self.decoded().0
    }

    /// The map from the line's bytes to the text's, for a line that is not valid UTF-8.
    fn map(&self) -> Option<&[usize]> {
        self.decoded().1.as_deref()
    }

    /// The line without the leading white space `--trim` leaves out (`trimmed`), its
    /// terminator kept; None when it starts with none.
    fn trimmed_line(&self, o: &RenderOptions) -> Option<&LineText<'a>> {
        self.trim
            .get_or_init(|| {
                let skip = trimmed(self.raw(), o);
                (skip > 0).then(|| {
                    let rest = match &self.bytes {
                        Cow::Borrowed(b) => {
                            let b: &'a [u8] = b;
                            Cow::Borrowed(&b[skip..])
                        }
                        Cow::Owned(b) => Cow::Owned(b[skip..].to_vec()),
                    };
                    Box::new(LineText::with_cut(rest, self.cut))
                })
            })
            .as_deref()
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
/// each preceded by `prefix`), or ripgrep's omission message with -M (with
/// `--max-columns-preview`, the line's first columns before it). `--trim` leaves out the
/// line's leading ASCII white space first. `bytes` is the raw line; `subs` are byte ranges in
/// it; `avail` is the width left after the caller's prefix.
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

/// The number of leading white-space bytes `--trim` leaves out of `raw`: grep-printer's
/// `trim_ascii_prefix`, whose white space is `\t`, `\n`, `\x0B`, `\x0C`, `\r` and ` `, less
/// the bytes of the line terminator (`\n`; `\r\n` with `--crlf`; NUL with `--null-data`).
pub(crate) fn trimmed(raw: &[u8], o: &RenderOptions) -> usize {
    if !o.look.trim {
        return 0;
    }
    let term: &[u8] = if o.look.null_data {
        b"\0"
    } else if o.look.crlf {
        b"\r\n"
    } else {
        b"\n"
    };
    raw.iter()
        .take_while(|&&b| {
            matches!(b, b'\t' | b'\n' | 0x0B | 0x0C | b'\r' | b' ') && !term.contains(&b)
        })
        .count()
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
    let p = &o.look.palette;
    if o.look.trim
        && let Some(text) = line.trimmed_line(o)
    {
        // The trimmed line, its submatches moved, as its own text, made once for all the rows
        // of the line.
        let skip = line.bytes().len() - text.bytes().len();
        let subs: Vec<(usize, usize)> = subs
            .iter()
            .filter(|&&(_, e)| e > skip)
            .map(|&(s, e)| (s.saturating_sub(skip), e - skip))
            .collect();
        let o = RenderOptions {
            look: Look {
                trim: false,
                ..o.look.clone()
            },
            ..o.clone()
        };
        return write_line_text(w, prefix, text, &subs, is_match, avail, &o);
    }
    if omitted(line.bytes(), o) {
        prefix(w)?;
        if o.look.max_columns_preview {
            return write_preview(w, line, subs, o);
        }
        // `-o` rows say nothing of their count (grep-printer's `write_exceeded_line`).
        let n = if o.look.only_matching { 0 } else { subs.len() };
        let msg = omission_message(n, is_match, w.supports_color());
        w.write_all(msg.as_bytes())?;
        return w.write_all(b"\n");
    }
    let raw = line.raw();
    let text = line.text();
    // Ranges past the text (a Match of the line's terminator) end where the text ends.
    let clamped: Vec<(usize, usize)>;
    let subs = if subs.iter().any(|&(_, e)| e > raw.len()) {
        clamped = subs
            .iter()
            .map(|&(s, e)| (s.min(raw.len()), e.min(raw.len())))
            .collect();
        &clamped[..]
    } else {
        subs
    };
    let subs: Cow<[(usize, usize)]> = match line.map() {
        // The common case: it fits (its bytes are at least as many as its characters, and
        // Fragments start beyond `avail` characters).
        None if raw.len() <= avail => {
            prefix(w)?;
            style::write_highlighted_in(w, p, text, subs)?;
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
            style::write_highlighted_in(w, p, text, &subs)?;
            w.write_all(b"\n")
        }
        Some(frags) => {
            for f in frags {
                prefix(w)?;
                if f.omitted_before > 0 {
                    let mark = format!("…[+{}] ", f.omitted_before);
                    style::write_in(w, p, "dim", mark.as_bytes())?;
                }
                style::write_highlighted_in(w, p, &f.text, &f.submatches)?;
                if f.omitted_after > 0 {
                    let mark = format!(" […+{}]", f.omitted_after);
                    style::write_in(w, p, "dim", mark.as_bytes())?;
                }
                w.write_all(b"\n")?;
            }
            Ok(())
        }
    }
}

/// `--max-columns-preview` (grep-printer's `write_exceeded_line`): the first `-M` graphemes of
/// the line, its Matches in them highlighted, then ` [... omitted end of long line]`, or with
/// colour and Matches ` [... N more matches]`, N those that start past the preview.
fn write_preview(
    w: &mut dyn WriteColor,
    line: &LineText,
    subs: &[(usize, usize)],
    o: &RenderOptions,
) -> std::io::Result<()> {
    use bstr::ByteSlice;
    let raw = line.raw();
    let max = o.max_columns.unwrap_or(0) as usize;
    let end = raw
        .grapheme_indices()
        .map(|(_, end, _)| end)
        .take(max)
        .last()
        .unwrap_or(0);
    let shown = LineText::new(&raw[..end]);
    let clipped: Vec<(usize, usize)> = subs
        .iter()
        .filter(|&&(s, _)| s < end)
        .map(|&(s, e)| (s, e.min(end)))
        .collect();
    let map = shown.map();
    let clipped: Vec<(usize, usize)> = match map {
        None => clipped,
        Some(m) => clipped.iter().map(|&(s, e)| (m[s], m[e])).collect(),
    };
    style::write_highlighted_in(w, &o.look.palette, shown.text(), &clipped)?;
    if subs.is_empty() || !w.supports_color() {
        w.write_all(b" [... omitted end of long line]")?;
    } else {
        let more = subs.iter().filter(|&&(s, _)| s >= end).count();
        let tense = if more == 1 { "match" } else { "matches" };
        write!(w, " [... {more} more {tense}]")?;
    }
    w.write_all(b"\n")
}

/// Byte ranges of submatches in a line.
type Subs = Vec<(usize, usize)>;

/// One line for tree or flat: a Match line under one of its Trails, or a context line.
pub(crate) struct Row<'a> {
    pub trail: &'a [Segment],
    pub line_number: u64,
    /// An object file's row (`Described::offsets`): the hex offset printed in place of the
    /// line number.
    pub offset: Option<u64>,
    /// The line, shared by the rows of a Match line.
    pub line: Rc<LineText<'a>>,
    pub subs: Vec<(usize, usize)>,
    pub is_match: bool,
    /// Context is on and this line does not follow the previous one: print `--` first.
    pub gap_before: bool,
    /// `--column`: the 1-based column of the row's first Match, as rg counts it; None on a
    /// context row.
    pub column: Option<u64>,
    /// `-b`: the byte offset of the row in the file, as rg gives it.
    pub byte_offset: u64,
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

/// The lines of an item's bytes (M5a plan, D6): a `-U` Match spans several, and a
/// `--null-data` record holds `\n` lines; each line is a row of its own. Each is `(start, end)`
/// in the bytes, its `\n` included, and the item's own terminator (a record's NUL) left out.
fn split_lines(bytes: &[u8], o: &RenderOptions) -> Vec<(usize, usize)> {
    let end = if o.look.null_data && bytes.last() == Some(&0) {
        bytes.len() - 1
    } else {
        bytes.len()
    };
    let mut lines = Vec::new();
    let mut start = 0;
    for nl in memchr::memchr_iter(b'\n', &bytes[..end]) {
        lines.push((start, nl + 1));
        start = nl + 1;
    }
    if start < end || lines.is_empty() {
        lines.push((start, end));
    }
    lines
}

/// The terminator bytes at the end of a row's text that are not printed: `\r\n` with
/// `--crlf`, else a `\n`.
fn cut_of(bytes: &[u8], o: &RenderOptions) -> usize {
    if o.look.crlf && bytes.ends_with(b"\r\n") {
        2
    } else {
        usize::from(bytes.ends_with(b"\n"))
    }
}

/// The number of line `k` of an item at `line_number`: rg numbers each line of a `-U` block,
/// and gives a `--null-data` record's lines its own number.
fn number_of(line_number: u64, k: usize, o: &RenderOptions) -> u64 {
    if o.look.null_data {
        line_number
    } else {
        line_number + k as u64
    }
}

/// An object file's replacements, each Match's range in the Source with its replacement
/// (`Described::replacements`).
type Replacements = [((usize, usize), Vec<u8>)];

/// The text of a line with its Matches replaced (`-r`), or as it is; and its submatches in it.
/// `object`: the line is an object file's string (spec §5.7), with the replacements its search
/// made (`Described::replacements`).
fn sunk<'a>(
    m: &'a Line,
    src: &'a [u8],
    subs: Vec<(usize, usize)>,
    o: &RenderOptions,
    object: Option<&Replacements>,
) -> (Cow<'a, [u8]>, Vec<(usize, usize)>) {
    let bytes = m.bytes(src);
    let Some(r) = o.look.replace.as_ref().filter(|_| !subs.is_empty()) else {
        return (Cow::Borrowed(bytes), subs);
    };
    // An object file's string is printable text made from the file's bytes: each Match is
    // replaced as the search's pass over its line replaced it in those bytes, its anchors and
    // groups read there (review r2 of the M5a plan, Codex #2; review r4, Codex #1 and #2), and
    // the string made printable after. A Match the search did not replace keeps its bytes.
    if let Some(replacements) = object {
        let start = (m.absolute_offset as usize).min(src.len());
        let string = &src[start..(start + m.len).min(src.len())];
        let mut text = Vec::with_capacity(string.len());
        let mut spans = Vec::with_capacity(subs.len());
        let mut last = 0;
        for &(s, e) in &subs {
            let (s, e) = (s.min(string.len()), e.min(string.len()));
            let with = replacements
                .binary_search_by_key(&(start + s, start + e), |(at, _)| *at)
                .map_or(&string[s..e], |i| &replacements[i].1[..]);
            text.extend_from_slice(&string[last.min(s)..s]);
            let at = text.len();
            text.extend(
                with.iter()
                    .copied()
                    .map(crate::processors::object::rows::shown),
            );
            spans.push((at, text.len()));
            last = last.max(e);
        }
        text.extend_from_slice(&string[last..]);
        return (Cow::Owned(text), spans);
    }
    // Over the Source, as rg's printer replaces over its buffer; a line converted from binary
    // data is its own haystack.
    let replaced = match &m.owned {
        None => {
            let start = (m.absolute_offset as usize).min(src.len());
            r.replace(src, start..start + bytes.len())
        }
        Some(own) => r.replace(own, 0..own.len()),
    };
    let mapped = replaced.map(&subs);
    (Cow::Owned(replaced.bytes), mapped)
}

/// `subs` clipped to the text of the line `bytes[start..end]`, its active terminator left out
/// (a Match of a `\n` inside a `--null-data` record ends where its row's text ends; a `\r`
/// without `--crlf` is text), and made relative to `start`.
fn clip_line(
    bytes: &[u8],
    subs: &[(usize, usize)],
    start: usize,
    end: usize,
    o: &RenderOptions,
) -> Vec<(usize, usize)> {
    clip(subs, start, end - cut_of(&bytes[start..end], o))
}

/// `clip_line` of submatches in the order a search finds them, each starting and ending where
/// the one before does or after (`in_order`): only those that reach the line are looked at, so
/// each line of a block or of a record costs its own submatches and not every submatch of the
/// item (review r5 of the M5a plan). Submatches out of that order are clipped one by one.
fn clip_line_of(
    bytes: &[u8],
    subs: &[(usize, usize)],
    in_order: bool,
    start: usize,
    end: usize,
    o: &RenderOptions,
) -> Vec<(usize, usize)> {
    if !in_order {
        return clip_line(bytes, subs, start, end, o);
    }
    let end = end - cut_of(&bytes[start..end], o);
    // Those that end before the line (an empty one at its start reaches it), then those that
    // start before its text ends: `clip`'s two conditions, each true of a prefix.
    let from = subs.partition_point(|&(s, e)| e < start || (e == start && s < e));
    let to = from + subs[from..].partition_point(|&(s, _)| s < end);
    clip(&subs[from..to], start, end)
}

/// Whether `subs` are in the order a search finds them: each starts and ends where the one
/// before does or after.
fn in_order(subs: &[(usize, usize)]) -> bool {
    subs.windows(2)
        .all(|w| w[0].0 <= w[1].0 && w[0].1 <= w[1].1)
}

/// The first of `lines` (ranges in file order, each starting where the one before ends) that
/// ends after `at`: the line that holds byte `at`, or `lines.len()` past the last.
fn line_at(lines: &[(usize, usize)], at: usize) -> usize {
    lines.partition_point(|&(_, le)| le <= at)
}

/// `subs` clipped to `start..end` and made relative to `start`.
fn clip(subs: &[(usize, usize)], start: usize, end: usize) -> Vec<(usize, usize)> {
    subs.iter()
        .filter(|&&(s, e)| s < end && (e > start || (s == e && s >= start)))
        .map(|&(s, e)| (s.max(start) - start, e.min(end) - start))
        .collect()
}

/// The rows of Match `i` (M5a plan, D4 and D6): one per distinct Trail of its submatches,
/// each with only its own submatches, except when -M omits the line: then one row under its
/// first Trail with every submatch (design note 5). With `-o`, one row per Match, its text
/// the matched bytes; with `--vimgrep`, one row per Match, its text the line. A `-U` block or
/// a `--null-data` record prints a row per line. An object file's string is one line whatever
/// it holds, its rows at hex offsets.
fn match_rows<'a>(
    d: &'a Described,
    i: usize,
    m: &'a Line,
    src: &'a [u8],
    o: &RenderOptions,
) -> Vec<Row<'a>> {
    let original = m.bytes(src);
    let off = m.absolute_offset;
    let found = subs_of(&m.submatches);
    let object = d.offsets.then_some(&d.replacements[..]);
    let (bytes, subs) = sunk(m, src, found.clone(), o, object);
    let trails: Vec<&'a [Segment]> = (0..subs.len().max(1)).map(|k| trail_of(d, i, k)).collect();
    // An object file's row is one string at the hex offset of a Match (spec §5.7): one row at
    // its first Match, with that Match's Trail (code review of the M4 plan r1, Codex #9); with
    // `-o` and `--vimgrep`, a row per Match at that Match's offset (review r1 of the M5a plan,
    // Codex #4). Match `k`'s offset is where it starts in the file, before `-r` replaces it.
    let offset = |k: usize| {
        d.offsets
            .then(|| off + found.get(k).or(found.first()).map_or(0, |s| s.0) as u64)
    };
    let row = |k: usize, trail, number, line: Rc<LineText<'a>>, subs, column, at| Row {
        offset: offset(k),
        trail,
        line_number: number,
        line,
        subs,
        is_match: true,
        gap_before: false,
        column,
        byte_offset: at,
    };
    // A string of an object file is one row whatever bytes it holds.
    let lines = if d.offsets {
        vec![(0, bytes.len())]
    } else {
        split_lines(&bytes, o)
    };
    let piece = |s: usize, e: usize| -> Cow<'a, [u8]> {
        match &bytes {
            Cow::Borrowed(b) => Cow::Borrowed(&b[s..e]),
            Cow::Owned(b) => Cow::Owned(b[s..e].to_vec()),
        }
    };
    let text = |s: usize, e: usize| -> Rc<LineText<'a>> {
        let text = piece(s, e);
        let cut = cut_of(&text, o);
        Rc::new(LineText::with_cut(text, cut))
    };
    if !subs.is_empty() && o.look.only_matching {
        let mut rows = Vec::new();
        for (k, &(s, e)) in subs.iter().enumerate() {
            let before = rows.len();
            // The lines the Match touches, from the one that holds its start: looking at every
            // line of a block for each Match made it cost lines × Matches (review r5 of the
            // M5a plan).
            let first = line_at(&lines, s);
            for (n, &(ls, le)) in lines.iter().enumerate().skip(first) {
                if ls >= e {
                    break;
                }
                let (ps, pe) = (s.max(ls), e.min(le));
                let pe = pe.min(le - cut_of(&bytes[ls..le], o));
                if ps >= pe {
                    continue;
                }
                let len = pe - ps;
                rows.push(row(
                    k,
                    trails[k],
                    number_of(m.line_number, n, o),
                    text(ps, pe),
                    vec![(0, len)],
                    Some(s as u64 + 1),
                    off + s as u64,
                ));
            }
            // Nothing of the Match to print: it is empty, its replacement is, or it is a line
            // terminator. rg's printer of one line prints it as an empty line, its printer of a
            // `-U` search nothing (review r2 of the M5a plan, Codex #4).
            if rows.len() == before && !o.look.multi_line {
                let n = first.min(lines.len() - 1);
                rows.push(row(
                    k,
                    trails[k],
                    number_of(m.line_number, n, o),
                    text(s, s),
                    Vec::new(),
                    Some(s as u64 + 1),
                    off + s as u64,
                ));
            }
        }
        return rows;
    }
    if !subs.is_empty() && o.look.per_match {
        let mut rows = Vec::new();
        // The line of the row before, shared by the rows of the Matches on it: its text made
        // for each Match made a line of N Matches cost N × the line (review r5 of the M5a plan).
        let mut shared: Option<(usize, Rc<LineText<'a>>)> = None;
        for (k, &(s, e)) in subs.iter().enumerate() {
            // The first line the Match touches; rg's printer of several lines prints none for
            // an empty Match at a line's start (review r3 of the M5a plan, Codex #4). Past the
            // line that holds its start, a line touches it only if it starts before its end.
            let Some((n, &(ls, le))) = lines
                .iter()
                .enumerate()
                .skip(line_at(&lines, s))
                .take_while(|&(_, &(ls, _))| ls <= s || ls < e)
                .find(|&(_, &(ls, le))| s < le && (e > ls || (s >= ls && !o.look.multi_line)))
            else {
                continue;
            };
            let (column, at) = if lines.len() == 1 {
                (s as u64 + 1, off + s as u64)
            } else {
                ((s - ls) as u64 + 1, off + ls as u64)
            };
            let line = match &shared {
                Some((at, line)) if *at == n => line.clone(),
                _ => text(ls, le),
            };
            shared = Some((n, line.clone()));
            rows.push(row(
                k,
                trails[k],
                number_of(m.line_number, n, o),
                line,
                clip_line(&bytes, &[(s, e)], ls, le, o),
                Some(column),
                at,
            ));
        }
        return rows;
    }
    let one = subs.is_empty() || omitted(original, o) || d.offsets;
    let groups: Vec<(&'a [Segment], Subs)> = if one {
        vec![(trails[0], subs.clone())]
    } else {
        group_by_trail(trails.iter().copied().zip(subs.iter().copied()))
    };
    // One line: its text without a `--null-data` record's NUL (review r4 of the M5a plan), made
    // once for the rows of all its Trails (review r5).
    if let [(ls, le)] = lines[..] {
        let line = text(ls, le);
        return groups
            .into_iter()
            .map(|(trail, subs)| {
                let column = subs.first().map(|s| s.0 as u64 + 1);
                row(0, trail, m.line_number, line.clone(), subs, column, off)
            })
            .collect();
    }
    // A block of lines: each line under the Trail of the first group with a Match in it, or of
    // the line before it. Each submatch marks the lines it reaches, from the one that holds its
    // start, and each line clips the submatches of its group that reach it: comparing every
    // group's submatches with every line made a block cost lines × Matches (review r5 of the
    // M5a plan).
    let mut first = vec![usize::MAX; lines.len()];
    for (g, (_, subs)) in groups.iter().enumerate() {
        for &(s, e) in subs {
            let at = line_at(&lines, s);
            for (n, &(ls, le)) in lines.iter().enumerate().skip(at) {
                if n > at && ls >= e {
                    break;
                }
                if !clip_line(&bytes, &[(s, e)], ls, le, o).is_empty() {
                    first[n] = first[n].min(g);
                }
            }
        }
    }
    let ordered: Vec<bool> = groups.iter().map(|(_, subs)| in_order(subs)).collect();
    let mut rows = Vec::new();
    let mut last = 0;
    for (n, &(ls, le)) in lines.iter().enumerate() {
        let g = if first[n] == usize::MAX {
            last
        } else {
            first[n]
        };
        last = g;
        let (trail, subs) = &groups[g];
        rows.push(row(
            0,
            trail,
            number_of(m.line_number, n, o),
            text(ls, le),
            clip_line_of(&bytes, subs, ordered[g], ls, le, o),
            subs.first().map(|s| s.0 as u64 + 1),
            off + ls as u64,
        ));
    }
    rows
}

/// The length of a line without its terminator: `\n`, `\r\n` or a record's NUL.
fn content_len(line: &[u8]) -> usize {
    let mut n = line.len();
    if line.ends_with(b"\n") {
        n -= 1;
        if line[..n].ends_with(b"\r") {
            n -= 1;
        }
    } else if line.ends_with(b"\0") {
        n -= 1;
    }
    n
}

/// The rows of a context line: one, or with `--null-data` one per line of its record, each
/// without the record's NUL (D6; review r5 of the M5a plan, Codex #3). With `-v` its
/// submatches are the pattern's, which `-r` replaces.
fn context_rows<'a>(c: &'a Line, src: &'a [u8], o: &RenderOptions) -> Vec<Row<'a>> {
    let (bytes, subs) = sunk(c, src, subs_of(&c.submatches), o, None);
    let lines = split_lines(&bytes, o);
    let ordered = in_order(&subs);
    lines
        .iter()
        .enumerate()
        .map(|(n, &(ls, le))| {
            let text: Cow<'a, [u8]> = match &bytes {
                Cow::Borrowed(b) => Cow::Borrowed(&b[ls..le]),
                Cow::Owned(b) => Cow::Owned(b[ls..le].to_vec()),
            };
            let cut = cut_of(&text, o);
            Row {
                trail: &[],
                line_number: number_of(c.line_number, n, o),
                offset: None,
                line: Rc::new(LineText::with_cut(text, cut)),
                subs: if lines.len() == 1 {
                    subs.clone()
                } else {
                    clip_line_of(&bytes, &subs, ordered, ls, le, o)
                },
                is_match: false,
                gap_before: false,
                column: None,
                byte_offset: c.absolute_offset + ls as u64,
            }
        })
        .collect()
}

/// The line number of the last line of an item: its own, or with `-U` the last of its block.
fn last_number(l: &Line, src: &[u8], o: &RenderOptions) -> u64 {
    if o.look.null_data || !o.look.multi_line {
        return l.line_number;
    }
    let bytes = l.bytes(src);
    let inner = &bytes[..content_len(bytes)];
    l.line_number + memchr::memchr_iter(b'\n', inner).count() as u64
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
        // An object file's rows are strings, not lines: no `--` between them.
        let gap = !self.d.offsets && self.prev.is_some_and(|p| p + 1 != line.line_number);
        if gap {
            // A group without a Match keeps an empty Trail.
            self.ready.extend(self.pending.drain(..));
            self.last = None;
        }
        self.prev = Some(last_number(line, self.src, self.o));
        let gap_before = gap && self.o.context;
        match item {
            Item::Context(c) => {
                for (n, mut row) in context_rows(c, self.src, self.o).into_iter().enumerate() {
                    row.trail = self.last.unwrap_or(&[]);
                    row.gap_before = gap_before && n == 0;
                    if self.last.is_some() {
                        self.ready.push_back(row);
                    } else {
                        self.pending.push(row);
                    }
                }
            }
            Item::Match(m) => {
                let rows = match_rows(self.d, self.i, m, self.src, self.o);
                self.i += 1;
                let Some(first) = rows.first().map(|r| r.trail) else {
                    return;
                };
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

/// The lines of a file's Sections (--expand), found through the offsets of its lines, which
/// the first Section reads: reading the file from its start for each Section made a file of N
/// Sections cost N × the file (review r5 of the M5a plan, the audit).
pub(crate) struct SectionLines<'s> {
    src: &'s [u8],
    starts: OnceCell<Vec<usize>>,
}

impl<'s> SectionLines<'s> {
    pub(crate) fn new(src: &'s [u8]) -> Self {
        SectionLines {
            src,
            starts: OnceCell::new(),
        }
    }

    /// Lines `start..=end` (1-based), terminators included, with their numbers.
    pub(crate) fn lines(&self, start: u64, end: u64) -> impl Iterator<Item = (u64, &'s [u8])> {
        let starts = self.starts.get_or_init(|| line_starts(self.src));
        let at = |n: u64| starts[(n as usize).min(starts.len() - 1)];
        let (s, e) = (at(start.saturating_sub(1)), at(end));
        self.src[s..e.max(s)]
            .split_inclusive(|&b| b == b'\n')
            .zip(start.max(1)..)
            .map(|(l, n)| (n, l))
    }
}

/// The offset of the start of every line of `src`, and its end.
pub(crate) fn line_starts(src: &[u8]) -> Vec<usize> {
    let mut v = vec![0];
    v.extend(memchr::memchr_iter(b'\n', src).map(|i| i + 1));
    if *v.last().expect("non-empty") != src.len() {
        v.push(src.len());
    }
    v
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

/// The path as the Trail layouts print it (M5a plan, D4): `path_bytes`, each `/` (and on
/// Windows each `\\`) as `--path-separator`'s byte, as grep-printer's `PrinterPath`.
pub fn shown_path(p: &std::path::Path, look: &Look) -> Vec<u8> {
    let mut bytes = path_bytes(p);
    if let Some(sep) = look.path_separator {
        for b in bytes.iter_mut() {
            if *b == b'/' || (cfg!(windows) && *b == b'\\') {
                *b = sep;
            }
        }
    }
    bytes
}

/// Opens a link to `p` at `line` and `column` (M5a plan, D12) when `--hyperlink-format` is
/// set and `w` writes colour and links, as grep-printer's `Interpolator::begin`; returns
/// whether it did, for `link_end`.
pub fn link_start(
    w: &mut dyn WriteColor,
    look: &Look,
    p: &std::path::Path,
    line: Option<u64>,
    column: Option<u64>,
) -> std::io::Result<bool> {
    let Some(h) = &look.hyperlinks else {
        return Ok(false);
    };
    if !w.supports_hyperlinks() || !w.supports_color() {
        return Ok(false);
    }
    let Some(url) = h.url(p, line, column) else {
        return Ok(false);
    };
    w.set_hyperlink(&termcolor::HyperlinkSpec::open(&url))?;
    Ok(true)
}

/// Closes the link `link_start` opened.
pub fn link_end(w: &mut dyn WriteColor, started: bool) -> std::io::Result<()> {
    if started {
        w.set_hyperlink(&termcolor::HyperlinkSpec::close())?;
    }
    Ok(())
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

    /// An object file's rows (spec §5.7): two printable strings of a binary on its first line,
    /// with context on, as `s4 -C1` gives them.
    pub fn object_rows() -> (Vec<u8>, Described) {
        let src = b"\x7fELF\0\0version=needle\0\x01another needle\0".to_vec();
        let row = |at: usize, text: &[u8], start: usize| {
            let mut l = Line::owned(1, at as u64, text);
            l.submatches = vec![Submatch {
                start,
                end: start + 6,
            }];
            Item::Match(l)
        };
        let base = vec![
            seg("object", "ELF x86-64", 1, 1),
            seg("section", ".rodata", 1, 1),
        ];
        let mut sym = base.clone();
        sym.push(seg("symbol", "main.version+0x8", 1, 1));
        let mut d = described(
            &src,
            vec![row(6, b"version=needle", 8), row(22, b"another needle", 8)],
            vec![vec![sym], vec![base]],
        );
        d.offsets = true;
        d.format = Some("elf".into());
        (src, d)
    }

    /// An object file's row holding two Matches of one symbol, whose Trails differ in the
    /// distance into the symbol (code review of the M4 plan r1, Codex #9).
    pub fn object_row_twice() -> (Vec<u8>, Described) {
        let src = b"\x7fELF\0\0needle needle\0".to_vec();
        let mut l = Line::owned(1, 6, b"needle needle");
        l.submatches = vec![
            Submatch { start: 0, end: 6 },
            Submatch { start: 7, end: 13 },
        ];
        let at = |n: &str| {
            vec![
                seg("object", "ELF x86-64", 1, 1),
                seg("section", ".rodata", 1, 1),
                seg("symbol", n, 1, 1),
            ]
        };
        let mut d = described(&src, vec![Item::Match(l)], vec![vec![at("s"), at("s+0x7")]]);
        d.offsets = true;
        d.format = Some("elf".into());
        (src, d)
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
            offsets: false,
            replacements: Vec::new(),
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
            look: Look::default(),
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

    /// (line number, text, highlighted ranges, column, byte offset, Trail names) of each row.
    type Seen = (u64, String, Vec<(usize, usize)>, Option<u64>, u64, String);

    fn seen(d: &Described, src: &[u8], o: &RenderOptions) -> Vec<Seen> {
        rows(d, src, o)
            .map(|r| {
                let names: Vec<&str> = r.trail.iter().map(|s| s.name.as_str()).collect();
                (
                    r.line_number,
                    r.line.text().to_string(),
                    r.subs.clone(),
                    r.column,
                    r.byte_offset,
                    names.join(" › "),
                )
            })
            .collect()
    }

    fn with(look: Look) -> RenderOptions {
        RenderOptions { look, ..opts() }
    }

    /// A line with two needles under two Trails, and a context line.
    fn two_trails() -> (Vec<u8>, Described) {
        use super::fixture::*;
        let src = src(&["intro", "  a needle b needle"]);
        let d = described(
            &src,
            vec![ctx(&src, 1), mat(&src, 2)],
            vec![vec![
                vec![seg("key", "A", 2, 2)],
                vec![seg("key", "B", 2, 2)],
            ]],
        );
        (src, d)
    }

    #[test]
    fn rows_carry_column_and_byte_offset() {
        let (src, d) = two_trails();
        assert_eq!(
            seen(&d, &src, &opts()),
            [
                (1, "intro".into(), vec![], None, 0, "A".into()),
                (
                    2,
                    "  a needle b needle".into(),
                    vec![(4, 10)],
                    Some(5),
                    6,
                    "A".into()
                ),
                (
                    2,
                    "  a needle b needle".into(),
                    vec![(13, 19)],
                    Some(14),
                    6,
                    "B".into()
                ),
            ]
        );
    }

    /// M5a plan, D4: `-o` makes a row of each Match, its text the matched bytes.
    #[test]
    fn only_matching_rows() {
        let (src, d) = two_trails();
        let o = with(Look {
            only_matching: true,
            ..Look::default()
        });
        assert_eq!(
            seen(&d, &src, &o),
            [
                (1, "intro".into(), vec![], None, 0, "A".into()),
                (2, "needle".into(), vec![(0, 6)], Some(5), 10, "A".into()),
                (2, "needle".into(), vec![(0, 6)], Some(14), 19, "B".into()),
            ]
        );
    }

    /// `--vimgrep`: a row of each Match, its text the whole line.
    #[test]
    fn per_match_rows() {
        let (src, d) = two_trails();
        let o = with(Look {
            per_match: true,
            ..Look::default()
        });
        let got = seen(&d, &src, &o);
        assert_eq!(
            got[1],
            (
                2,
                "  a needle b needle".into(),
                vec![(4, 10)],
                Some(5),
                10,
                "A".into()
            )
        );
        assert_eq!(
            got[2],
            (
                2,
                "  a needle b needle".into(),
                vec![(13, 19)],
                Some(14),
                19,
                "B".into()
            )
        );
    }

    /// Review r3 of the M5a plan, Codex #4: `--vimgrep` in a search of several lines (`-U` with
    /// a pattern that can match a line break) prints a row for the first line a Match touches,
    /// so none for an empty Match at a line's start, as rg's printer of several lines; a search
    /// of lines prints every Match.
    #[test]
    fn per_match_rows_of_a_multi_line_search() {
        use super::fixture::*;
        let src = src(&["needle", "next"]);
        let columns = |n: u64, subs: &[(usize, usize)], multi_line: bool| -> Vec<(u64, u64)> {
            let mut item = mat(&src, n);
            if let Item::Match(l) = &mut item {
                l.submatches = subs
                    .iter()
                    .map(|&(start, end)| Submatch { start, end })
                    .collect();
            }
            let trails = vec![vec![seg("key", "A", 1, 2)]; subs.len()];
            let d = described(&src, vec![item], vec![trails]);
            let look = Look {
                per_match: true,
                multi_line,
                ..Look::default()
            };
            seen(&d, &src, &with(look))
                .into_iter()
                .map(|r| (r.0, r.3.unwrap()))
                .collect()
        };
        let subs = [(0, 0), (1, 2), (6, 6)];
        assert_eq!(columns(1, &subs, true), [(1, 2), (1, 7)]);
        assert_eq!(columns(1, &subs, false), [(1, 1), (1, 2), (1, 7)]);
        assert_eq!(columns(2, &[(0, 0)], true), []);
    }

    /// M5a plan, D6: a `-U` Match prints a row per line, numbered as rg numbers them, under
    /// its Trail; with `-o`, each line of the Match that is not empty.
    #[test]
    fn multi_line_blocks() {
        use super::fixture::*;
        let src = src(&["## Linux", "", "Run the needle"]);
        let mut m = Line::owned(1, 0, &src[..]);
        m.owned = None;
        m.submatches = vec![Submatch { start: 3, end: 13 }];
        let d = described(
            &src,
            vec![Item::Match(m)],
            vec![vec![vec![seg("heading", "Linux", 1, 3)]]],
        );
        let o = with(Look {
            multi_line: true,
            ..Look::default()
        });
        assert_eq!(
            seen(&d, &src, &o),
            [
                (
                    1,
                    "## Linux".into(),
                    vec![(3, 8)],
                    Some(4),
                    0,
                    "Linux".into()
                ),
                (2, "".into(), vec![(0, 0)], Some(4), 9, "Linux".into()),
                (
                    3,
                    "Run the needle".into(),
                    vec![(0, 3)],
                    Some(4),
                    10,
                    "Linux".into()
                ),
            ]
        );
        let o = with(Look {
            multi_line: true,
            only_matching: true,
            ..Look::default()
        });
        let got: Vec<(u64, String)> = seen(&d, &src, &o).into_iter().map(|r| (r.0, r.1)).collect();
        assert_eq!(got, [(1, "Linux".into()), (3, "Run".into())]);
    }

    /// A `--null-data` record's lines are rows with the record's number, its NUL not printed.
    #[test]
    fn null_data_records() {
        use super::fixture::*;
        let src = b"one needle\ntwo\0".to_vec();
        let mut m = Line::owned(1, 0, &src);
        m.owned = None;
        m.submatches = vec![Submatch { start: 4, end: 10 }];
        let d = described(&src, vec![Item::Match(m)], vec![vec![vec![]]]);
        let o = with(Look {
            null_data: true,
            ..Look::default()
        });
        let got: Vec<(u64, String)> = seen(&d, &src, &o).into_iter().map(|r| (r.0, r.1)).collect();
        assert_eq!(got, [(1, "one needle".into()), (1, "two".into())]);
        // A record of one line too (review r4 of the plan, found checking Fable #1).
        let src = b"a needle\0".to_vec();
        let mut m = Line::owned(1, 0, &src);
        m.owned = None;
        m.submatches = vec![Submatch { start: 2, end: 8 }];
        let d = described(&src, vec![Item::Match(m)], vec![vec![vec![]]]);
        let got: Vec<(u64, String)> = seen(&d, &src, &o).into_iter().map(|r| (r.0, r.1)).collect();
        assert_eq!(got, [(1, "a needle".into())]);
        // Review r5 of the plan, Codex #3: a context record of one line too, as `-C1` and
        // `--passthru` print it, and as `-v` prints it with the pattern's submatches.
        let src = b"before\0needle\0after\0".to_vec();
        let record = |n: u64, at: usize, len: usize, subs: Vec<Submatch>| {
            let mut l = Line::owned(n, at as u64, &src[at..at + len]);
            l.owned = None;
            l.submatches = subs;
            l
        };
        let needle = || vec![Submatch { start: 0, end: 6 }];
        let d = described(
            &src,
            vec![
                Item::Context(record(1, 0, 7, vec![])),
                Item::Match(record(2, 7, 7, needle())),
                Item::Context(record(3, 14, 6, vec![])),
            ],
            vec![vec![vec![]]],
        );
        let got: Vec<(u64, String)> = seen(&d, &src, &o).into_iter().map(|r| (r.0, r.1)).collect();
        assert_eq!(
            got,
            [
                (1, "before".into()),
                (2, "needle".into()),
                (3, "after".into())
            ]
        );
        let d = described(
            &src,
            vec![
                Item::Match(record(1, 0, 7, vec![])),
                Item::Context(record(2, 7, 7, needle())),
                Item::Match(record(3, 14, 6, vec![])),
            ],
            vec![vec![vec![]], vec![vec![]]],
        );
        let got: Vec<(String, Subs)> = seen(&d, &src, &o).into_iter().map(|r| (r.1, r.2)).collect();
        assert_eq!(
            got,
            [
                ("before".into(), vec![]),
                ("needle".into(), vec![(0, 6)]),
                ("after".into(), vec![])
            ]
        );
    }

    /// A Match of the line break inside a `--null-data` record ends where its row's text ends
    /// (found by tests/fuzz_m5.rs: its range past the text panicked in the Fragments).
    #[test]
    fn a_match_of_a_records_line_break_stays_in_its_row() {
        use super::fixture::*;
        let src = format!("# Long\n\n{} needle\n", "x".repeat(300)).into_bytes();
        let mut m = Line::owned(1, 0, &src);
        m.owned = None;
        m.submatches = memchr::memchr_iter(b'\n', &src)
            .map(|i| Submatch {
                start: i,
                end: i + 1,
            })
            .collect();
        let d = described(&src, vec![Item::Match(m)], vec![vec![vec![]; 3]]);
        let o = RenderOptions {
            width: 40,
            look: Look {
                null_data: true,
                ..Look::default()
            },
            ..opts()
        };
        let got = seen(&d, &src, &o);
        assert_eq!(got.len(), 3);
        assert!(
            got.iter().all(|r| r.2.iter().all(|&(_, e)| e <= r.1.len())),
            "{got:?}"
        );
        let mut w = NoColor::new(Vec::new());
        super::flat::render(
            &d,
            &src,
            &RenderOptions {
                layout: Layout::Flat,
                ..o
            },
            None,
            &mut w,
        )
        .unwrap();
    }

    /// `-r`: each row's text is the line with its Matches replaced, and its highlight covers
    /// its own replacements; the column is the replacement's, as rg's.
    #[test]
    fn replaced_rows() {
        let (src, d) = two_trails();
        let a = crate::cli::parse(["s4", "needle"].iter().map(Into::into).collect()).unwrap();
        let m = crate::matcher::build(&a, &["needle".into()]).unwrap();
        let r = replace::Replacer::new(
            m,
            b"N".to_vec(),
            grep_matcher::LineTerminator::byte(b'\n'),
            false,
        );
        let o = with(Look {
            replace: Some(std::sync::Arc::new(r)),
            ..Look::default()
        });
        let got = seen(&d, &src, &o);
        assert_eq!(
            got[1],
            (2, "  a N b N".into(), vec![(4, 5)], Some(5), 6, "A".into())
        );
        assert_eq!(
            got[2],
            (2, "  a N b N".into(), vec![(8, 9)], Some(9), 6, "B".into())
        );
    }

    /// A Replacer of `pattern` by `with`, as `-r` builds it.
    fn replacer(pattern: &str, with: &[u8]) -> Look {
        let a = crate::cli::parse(["s4", "x"].iter().map(Into::into).collect()).unwrap();
        let m = crate::matcher::build(&a, &[pattern.into()]).unwrap();
        let r = replace::Replacer::new(
            m,
            with.to_vec(),
            grep_matcher::LineTerminator::byte(b'\n'),
            false,
        );
        Look {
            replace: Some(std::sync::Arc::new(r)),
            ..Look::default()
        }
    }

    /// Review r2 of the M5a plan, Codex #2: an object file's `-r` replaces each Match where the
    /// search found it in the file's bytes, its anchors and groups read there, and only then
    /// makes the string printable. `^needle` does not match after a NUL, and a control byte in a
    /// Match is replaced with the Match.
    #[test]
    fn object_rows_replace_over_the_files_bytes() {
        use super::fixture::*;
        let src = b"\x7fELF\0\0needle target\0zz ab\x01cd yy\0".to_vec();
        // As processors::object::rows makes them: each string at its offset, the control bytes
        // of its Matches shown as `.`.
        let row = |at: u64, text: &[u8], sub: (usize, usize)| {
            let mut l = Line::owned(1, at, text);
            l.submatches = vec![Submatch {
                start: sub.0,
                end: sub.1,
            }];
            l
        };
        let base = || vec![vec![seg("object", "ELF x86-64", 1, 1)]];
        // The replacements as the search of the file makes them: one pass over each line.
        let texts = |l: Line, look: Look| -> Vec<(String, Vec<(usize, usize)>)> {
            let mut d = described(&src, vec![Item::Match(l)], vec![base()]);
            d.offsets = true;
            let r = look.replace.as_ref().unwrap();
            d.replacements = r.replacements(&src, 0..src.len());
            rows(&d, &src, &with(look))
                .map(|r| (r.line.text().to_string(), r.subs.clone()))
                .collect()
        };
        let target = || row(6, b"needle target", (7, 13));
        assert_eq!(
            texts(target(), replacer("^needle|target", b"X")),
            [("needle X".to_string(), vec![(7, 8)])]
        );
        let control = || row(20, b"zz ab.cd yy", (3, 8));
        assert_eq!(
            texts(control(), replacer("ab\\x01cd", b"X")),
            [("zz X yy".to_string(), vec![(3, 4)])]
        );
        // A group that holds the control byte: read from the file, then shown as `.`.
        assert_eq!(
            texts(control(), replacer("a(b\\x01c)d", b"<$1>")),
            [("zz <b.c> yy".to_string(), vec![(3, 8)])]
        );
        let only = Look {
            only_matching: true,
            ..replacer("a(b\\x01c)d", b"<$1>")
        };
        assert_eq!(
            texts(control(), only),
            [("<b.c>".to_string(), vec![(0, 5)])]
        );
        // Review r4 of the plan, Codex #1 and #2: each Match prints the replacement the search
        // made, with the groups it found (PCRE2's `(A)\x00\KB|(B)` gives `<A|>` where a search
        // from the string's start finds the second alternative), and is not looked for again;
        // a Match without one keeps its bytes.
        let mut d = described(&src, vec![Item::Match(target())], vec![base()]);
        d.offsets = true;
        d.replacements = vec![((13, 19), b"<A|>".to_vec())];
        let look = with(replacer("(t)arget|(target)", b"<$1|$2>"));
        let got: Vec<String> = rows(&d, &src, &look)
            .map(|r| r.line.text().to_string())
            .collect();
        assert_eq!(got, ["needle <A|>"]);
        d.replacements.clear();
        let got: Vec<String> = rows(&d, &src, &look)
            .map(|r| r.line.text().to_string())
            .collect();
        assert_eq!(got, ["needle target"]);
    }

    /// Review r2 of the M5a plan, Codex #4: `-o` prints a row for every Match, as rg's printer
    /// of one line does: an empty Match and an empty replacement are an empty row, and only the
    /// active line terminator is left out (a `\r` Match without `--crlf` is printed).
    #[test]
    fn only_matching_rows_as_ripgrep() {
        use super::fixture::*;
        let texts = |src: &[u8], subs: &[(usize, usize)], look: Look| -> Vec<Seen> {
            let mut item = mat(src, 1);
            if let Item::Match(l) = &mut item {
                l.submatches = subs
                    .iter()
                    .map(|&(start, end)| Submatch { start, end })
                    .collect();
            }
            let trails = vec![vec![seg("key", "A", 1, 1)]; subs.len()];
            let d = described(src, vec![item], vec![trails]);
            seen(
                &d,
                src,
                &with(Look {
                    only_matching: true,
                    ..look
                }),
            )
        };
        let row = |text: &str, subs: Vec<(usize, usize)>, column, at| {
            (1, text.to_string(), subs, Some(column), at, "A".to_string())
        };
        assert_eq!(
            texts(b"needle\n", &[(0, 6)], replacer("needle", b"")),
            [row("", vec![], 1, 0)]
        );
        assert_eq!(
            texts(b"needle\n", &[(0, 0), (3, 3)], Look::default()),
            [row("", vec![], 1, 0), row("", vec![], 4, 3)]
        );
        assert_eq!(
            texts(b"needle\r\n", &[(6, 7)], Look::default()),
            [row("\r", vec![(0, 1)], 7, 6)]
        );
    }

    /// An object file's rows (review r1 of the plan, Codex #4): `-o` and `--vimgrep` make a
    /// row of each Match, at that Match's offset and under its Trail; `-r` replaces in the
    /// string, which stays one row at its first Match.
    #[test]
    fn object_rows_take_o_vimgrep_and_r() {
        let (src, d) = super::fixture::object_row_twice();
        type ObjectRow = (Option<u64>, String, Vec<(usize, usize)>, String);
        let at = |d: &Described, o: &RenderOptions| -> Vec<ObjectRow> {
            rows(d, &src, o)
                .map(|r| {
                    let names: Vec<&str> = r.trail.iter().map(|s| s.name.as_str()).collect();
                    (
                        r.offset,
                        r.line.text().to_string(),
                        r.subs.clone(),
                        names.join(" › "),
                    )
                })
                .collect()
        };
        let only = with(Look {
            only_matching: true,
            ..Look::default()
        });
        assert_eq!(
            at(&d, &only),
            [
                (
                    Some(6),
                    "needle".into(),
                    vec![(0, 6)],
                    "ELF x86-64 › .rodata › s".into()
                ),
                (
                    Some(13),
                    "needle".into(),
                    vec![(0, 6)],
                    "ELF x86-64 › .rodata › s+0x7".into()
                ),
            ]
        );
        let vim = with(Look {
            per_match: true,
            ..Look::default()
        });
        assert_eq!(
            at(&d, &vim),
            [
                (
                    Some(6),
                    "needle needle".into(),
                    vec![(0, 6)],
                    "ELF x86-64 › .rodata › s".into()
                ),
                (
                    Some(13),
                    "needle needle".into(),
                    vec![(7, 13)],
                    "ELF x86-64 › .rodata › s+0x7".into()
                ),
            ]
        );
        let a = crate::cli::parse(["s4", "needle"].iter().map(Into::into).collect()).unwrap();
        let m = crate::matcher::build(&a, &["needle".into()]).unwrap();
        let r = replace::Replacer::new(
            m,
            b"N".to_vec(),
            grep_matcher::LineTerminator::byte(b'\n'),
            false,
        );
        // The replacements the search of the file makes, in one pass over its line.
        let mut d = d.clone();
        d.replacements = r.replacements(&src, 0..src.len());
        let replaced = with(Look {
            replace: Some(std::sync::Arc::new(r)),
            ..Look::default()
        });
        assert_eq!(
            at(&d, &replaced),
            [(
                Some(6),
                "N N".into(),
                vec![(0, 1), (2, 3)],
                "ELF x86-64 › .rodata › s".into()
            )]
        );
    }

    /// `--trim` leaves out grep-printer's white space (`\t`, `\n`, `\x0B`, `\x0C`, `\r`, ` `)
    /// but never a byte of the line terminator (review r1 of the plan, Codex #8).
    #[test]
    fn trim_leaves_out_greps_white_space_but_the_terminator() {
        let o = |crlf: bool, null_data: bool| {
            with(Look {
                trim: true,
                crlf,
                null_data,
                ..Look::default()
            })
        };
        assert_eq!(trimmed(b"\r needle\n", &o(false, false)), 2);
        assert_eq!(trimmed(b"\x0b needle\n", &o(false, false)), 2);
        assert_eq!(trimmed(b"\x0c\t needle\n", &o(false, false)), 3);
        assert_eq!(trimmed(b"\r needle\r\n", &o(true, false)), 0);
        assert_eq!(trimmed(b"\n needle\n", &o(false, false)), 0);
        assert_eq!(trimmed(b"\n\r needle\0", &o(false, true)), 3);
        assert_eq!(trimmed(b"\0 needle", &o(false, true)), 0);
    }

    fn written(line: &[u8], subs: &[(usize, usize)], color: bool, o: &RenderOptions) -> String {
        let pre = |w: &mut dyn WriteColor| w.write_all(b"P:");
        let out = if color {
            let mut w = Ansi::new(Vec::new());
            write_line(&mut w, &pre, line, subs, true, 80, o).unwrap();
            w.into_inner()
        } else {
            let mut w = NoColor::new(Vec::new());
            write_line(&mut w, &pre, line, subs, true, 80, o).unwrap();
            w.into_inner()
        };
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn trim_leaves_out_leading_ascii_space() {
        let o = with(Look {
            trim: true,
            ..Look::default()
        });
        assert_eq!(
            written(b" \t needle x\n", &[(3, 9)], false, &o),
            "P:needle x\n"
        );
        assert_eq!(
            written(b"  needle\n", &[(2, 8)], true, &o),
            "P:\x1b[0m\x1b[1m\x1b[31mneedle\x1b[0m\n"
        );
    }

    /// `--max-columns-preview`, as grep-printer's `write_exceeded_line`.
    #[test]
    fn previews_of_long_lines() {
        let o = RenderOptions {
            max_columns: Some(10),
            look: Look {
                max_columns_preview: true,
                ..Look::default()
            },
            ..opts()
        };
        let line = b"Some intro needle here.\n";
        assert_eq!(
            written(line, &[(11, 17)], false, &o),
            "P:Some intro [... omitted end of long line]\n"
        );
        assert_eq!(
            written(line, &[(11, 17)], true, &o),
            "P:Some intro [... 1 more match]\n"
        );
        assert_eq!(
            written(b"Run the needle command:\n", &[(8, 14)], true, &o),
            "P:Run the \x1b[0m\x1b[1m\x1b[31mne\x1b[0m [... 0 more matches]\n"
        );
    }

    #[test]
    fn the_palette_colours_matches_and_markers() {
        let o = with(Look {
            palette: style::Palette::new(&["match:fg:blue".into(), "match:style:nobold".into()]),
            ..Look::default()
        });
        assert_eq!(
            written(b"a needle\n", &[(2, 8)], true, &o),
            "P:a \x1b[0m\x1b[34mneedle\x1b[0m\n"
        );
    }

    #[test]
    fn paths_with_a_separator_and_links() {
        let look = Look {
            path_separator: Some(b'\\'),
            ..Look::default()
        };
        assert_eq!(
            shown_path(std::path::Path::new("a/b/c.md"), &look),
            b"a\\b\\c.md"
        );
        assert_eq!(
            shown_path(std::path::Path::new("a/b"), &Look::default()),
            b"a/b"
        );
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("f.md");
        std::fs::write(&p, "x").unwrap();
        let look = Look {
            hyperlinks: hyperlink::Hyperlinks::new("file://{path}:{line}", None)
                .map(std::sync::Arc::new),
            ..Look::default()
        };
        let mut w = Ansi::new(Vec::new());
        let on = link_start(&mut w, &look, &p, Some(3), None).unwrap();
        std::io::Write::write_all(&mut w, b"f.md").unwrap();
        link_end(&mut w, on).unwrap();
        let out = String::from_utf8(w.into_inner()).unwrap();
        let canon = p.canonicalize().unwrap();
        assert_eq!(
            out,
            format!(
                "\x1b]8;;file://{}:3\x1b\\f.md\x1b]8;;\x1b\\",
                canon.display()
            )
        );
        // No colour, no link.
        let mut w = NoColor::new(Vec::new());
        assert!(!link_start(&mut w, &look, &p, Some(3), None).unwrap());
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
