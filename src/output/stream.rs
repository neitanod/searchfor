//! Files printed row by row as the searcher reports them (code review r1, Fable #1): Plain
//! files, and files whose Processor gives a native cursor (`processors::Prepared`). No model of
//! the file is kept: each Trail is asked of the cursor as its row is printed, and only the
//! context lines that open a group wait for the Match after them. Prints exactly what the
//! printers print from a Described file.
//!
//! The structural flags keep state, never the file (code review r1, Codex #1; M5b spec §6.1):
//! --within filters each Match as it comes, holding only the context lines before it that a
//! kept Match may still claim, and counts `-m` after the filter; --expand holds the Matches of
//! the Sections still open and prints each Section once it is closed; --in-headings feeds the
//! Matches of the Segment names in Source order (`heading_match`).
use super::json::{JsonFile, JsonOptions, RunStats};
use super::{Layout, LineText, RenderOptions, Row, flat, tree};
use crate::matcher::PatternMatcher;
use crate::model::{
    Described, FileMatches, FileStats, Item, Line, Segment, SharedTrail, Submatch, Trail,
};
use crate::processors::Cursor;
use crate::search::{MaxCount, context_submatches_in, match_submatches_in};
use grep_searcher::{Searcher, Sink, SinkContext, SinkFinish, SinkMatch};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;
use std::rc::Rc;
use std::time::Instant;
use termcolor::WriteColor;

/// A Section as 1-based inclusive lines (--expand), or None.
pub type Section = Option<(u64, u64)>;

/// --within over a streamed file: the names it wants, and the context and `-m` it applies
/// after the filter, as `filter::retain` and `filter::limit` apply them to a model.
pub struct WithinOptions {
    pub wanted: Vec<String>,
    pub max_count: Option<u64>,
    pub before: u64,
    pub after: u64,
}

/// What a file is printed from, besides the searcher's lines.
pub struct StreamFile<'a> {
    pub path: &'a Path,
    pub explicit: bool,
    /// The Format's name (JSON's `format`), None for Plain.
    pub format: Option<&'a str>,
    /// The bytes the search runs over: needed with a cursor (the context lines that wait for
    /// a Match are kept as ranges of it), and with the structural flags.
    pub src: Option<&'a [u8]>,
    /// The Trails of the file's Matches, in file order; None for Plain.
    pub cursor: Option<Box<dyn Cursor + 'a>>,
    /// `-m`, counted here (see `MaxCount`).
    pub max_count: Option<u64>,
    /// The flat layout's separator from the file before, written before the first row (see
    /// `flat::render`).
    pub lead: Option<&'a [u8]>,
    /// --within.
    pub within: Option<WithinOptions>,
    /// Every Match is counted (`--stats` outside JSON), which the one-line layout alone does
    /// not need.
    pub count: bool,
    /// The Matches come from Segment names (--in-headings, `heading_match`), each with its
    /// Trail and Section, and not from the searcher.
    pub named: bool,
    /// -q or a summary (`-c`, `--count-matches`, `-l`, `--files-without-match`) under the
    /// structural flags: the Matches the filters keep are counted (`StreamSink::kept`), and
    /// nothing is printed (code review r2, Codex #1).
    pub tally: Option<Tally>,
}

/// How a tallied file is searched (`StreamFile::tally`).
#[derive(Clone, Copy)]
pub struct Tally {
    /// The search stops at the first Match the filters keep: -q and -l, which need no more,
    /// when no statistics are counted.
    pub stop: bool,
}

/// A line `open_from` searched: where it starts and ends, and where its submatches start.
struct LookedLine {
    start: usize,
    end: usize,
    starts: Vec<usize>,
}

/// How many searched lines `open_from` keeps: the line of a Segment's start and the line it
/// reaches.
const LOOKED_LINES: usize = 4;

/// A row of a Match line: its Trail and the submatches under it.
type RowGroup = (Rc<[Segment]>, Vec<(usize, usize)>);

/// A line waiting for the Trail of the next Match of its group.
struct Pending {
    line: Line,
    gap_before: bool,
}

/// --within's state: the context lines a later kept Match may claim, and the window of the
/// last one kept.
struct Within {
    o: WithinOptions,
    /// Context lines, and Matches it dropped, that are near no kept Match yet: at most the
    /// `before` lines before the next line.
    held: VecDeque<Line>,
    /// The last line of the after context of the last kept Match.
    after_until: Option<u64>,
    /// Matches kept so far.
    kept: u64,
    /// With `-m`: the end of the window of the `-m`th kept Match, past which nothing is
    /// printed (`filter::limit`).
    window_end: Option<u64>,
}

/// --expand in the tree and flat layouts (`output::Pieces`): whether the file prints Sections
/// at all, and the Sections and rows waiting for the Sections before them to close.
struct Expand {
    mode: ExpandMode,
    /// The Matches the waiting Sections may hold, and those of the Sections still open, in
    /// file order; the rows of a Section are made from them (`section_rows`).
    window: Described,
    /// Matches dropped from the front of `window`.
    base: usize,
    /// Matches added to `window` so far.
    next: usize,
    queue: VecDeque<Queued>,
    /// The first line each entry of `queue` holds (a Section's start, a Match's line), as the
    /// ascending minima of the entries from the front, each with its number among the entries
    /// queued: the front is the least line the queue holds, found without a walk over it.
    least: VecDeque<(u64, u64)>,
    /// Entries queued, and entries printed, so far.
    queued: u64,
    printed: u64,
    /// The line the Sections' bytes were last found from, and where it starts
    /// (`LineSeek::position`).
    seek: (u64, usize),
    /// The Sections printed or waiting that a later Match may still name.
    seen: HashSet<(u64, u64)>,
}

impl Expand {
    /// Queues `q`, whose first line is `line`.
    fn push(&mut self, q: Queued, line: u64) {
        while self.least.back().is_some_and(|&(_, l)| l >= line) {
            self.least.pop_back();
        }
        self.least.push_back((self.queued, line));
        self.queued += 1;
        self.queue.push_back(q);
    }

    /// The entry at the front of the queue, printed.
    fn pop(&mut self) -> Option<Queued> {
        let q = self.queue.pop_front()?;
        if self.least.front().is_some_and(|&(k, _)| k == self.printed) {
            self.least.pop_front();
        }
        self.printed += 1;
        Some(q)
    }

    /// The least line the queue holds.
    fn bound(&self) -> u64 {
        self.least.front().map_or(u64::MAX, |&(_, l)| l)
    }
}

/// A file prints its Sections when one of its Matches has one; without any, its rows, as
/// without --expand (`output::pieces`), which only its end tells.
enum ExpandMode {
    /// No Match with a Section yet: the lines so far.
    Undecided(Vec<Buffered>),
    Rows,
    Sections,
}

enum Buffered {
    Match(Line, Vec<SharedTrail>),
    Context(Line),
}

/// What --expand prints next, in file order.
enum Queued {
    /// The rows of the `i`th Match, which has no Section.
    Rows(usize),
    /// A Section under its Trail, once the search is past its end.
    Section(Trail, u64, u64),
}

pub struct StreamSink<'a, W: WriteColor> {
    w: &'a mut W,
    matcher: &'a PatternMatcher,
    invert: bool,
    o: &'a RenderOptions,
    file: StreamFile<'a>,
    trails: Option<Box<dyn Cursor + 'a>>,
    head: flat::FileHead,
    json: Option<JsonFile<'a>>,
    tree: tree::TreeState,
    max_count: MaxCount,
    /// Line number of the last row, for the `--` between groups.
    prev: Option<u64>,
    /// The tree's path line is written.
    header: bool,
    /// Binary data was found: rows after it are not printed (design note 3).
    binary: bool,
    /// Of the first line the search reached past the binary data: whether a gap parts it from
    /// the last row printed, which puts the group separator before the binary message
    /// (`super::binary_break`).
    binary_gap: Option<bool>,
    /// No colour, no -M and no cursor: a short valid line can be written without the
    /// printers' machinery.
    fast: bool,
    /// The path and its separator for the flat layout (empty without file names), and the
    /// width of that path in characters.
    path_prefix: (Vec<u8>, usize),
    scratch: Vec<u8>,
    /// The writer takes neither colour nor links: a flat row is made in `line_buf` first.
    plain_out: bool,
    line_buf: termcolor::NoColor<Vec<u8>>,
    /// Trail of the last row of the current group, once it has a Match.
    group: Option<Rc<[Segment]>>,
    pending: Vec<Pending>,
    /// The empty Trail of a Plain file's rows.
    empty: SharedTrail,
    started: Instant,
    /// The lines go through --within or --expand (`structured_match`).
    structured: bool,
    within: Option<Within>,
    expand: Option<Expand>,
    /// The lines of the file, for --expand's Sections.
    sections: super::SectionLines<'a>,
    /// Where the outermost Segment holding the last Match starts (`Cursor::open_from`): with
    /// --expand, the Matches before it are let go of. None: the cursor cannot tell.
    open: Option<usize>,
    /// `open_from`'s look ahead: the last lines it searched, each with the first byte of its
    /// submatches, so a line holding many Segments is searched once (code review r3, Codex #2).
    looked: Vec<LookedLine>,
    pub stats: FileStats,
    /// With `StreamFile::tally`: the Match lines the filters kept, and their submatches, an
    /// inverted Match line counted as one (`summary::count`).
    pub kept: (u64, u64),
    /// A tally that stops found its Match.
    stopped: bool,
    /// A row was printed or is due: a Match the filters kept.
    pub has_match: bool,
    /// The search found a Match, kept or not.
    pub found: bool,
}

impl<'a, W: WriteColor> StreamSink<'a, W> {
    pub fn new(
        w: &'a mut W,
        matcher: &'a PatternMatcher,
        invert: bool,
        o: &'a RenderOptions,
        jo: &'a JsonOptions,
        mut file: StreamFile<'a>,
    ) -> Self {
        let fast = !w.supports_color()
            && o.max_columns.is_none()
            && file.cursor.is_none()
            && o.look.plain();
        let plain_out = !w.supports_color() && !w.supports_hyperlinks();
        let within = file.within.take().map(|o| Within {
            o,
            held: VecDeque::new(),
            after_until: None,
            kept: 0,
            window_end: None,
        });
        let expand = (o.expand && o.layout != Layout::Json).then(|| Expand {
            // A Plain file has no Section.
            mode: if file.cursor.is_some() || file.named {
                ExpandMode::Undecided(Vec::new())
            } else {
                ExpandMode::Rows
            },
            window: Described {
                file: FileMatches {
                    path: file.path.to_path_buf(),
                    explicit: file.explicit,
                    items: Vec::new(),
                    stats: FileStats::default(),
                },
                format: file.format.map(Into::into),
                trails: Vec::new(),
                sections: Vec::new(),
                offsets: false,
                replacements: Vec::new(),
            },
            base: 0,
            next: 0,
            queue: VecDeque::new(),
            least: VecDeque::new(),
            queued: 0,
            printed: 0,
            seek: (1, 0),
            seen: HashSet::new(),
        });
        Self {
            w,
            matcher,
            invert,
            o,
            trails: file.cursor.take(),
            head: flat::FileHead::new(file.path, o),
            json: (o.layout == Layout::Json).then(|| JsonFile::new(file.path, file.format, jo)),
            tree: tree::TreeState::default(),
            max_count: MaxCount::new(file.max_count),
            prev: None,
            header: false,
            binary: false,
            binary_gap: None,
            fast,
            path_prefix: if o.with_filename && o.layout == Layout::Flat {
                let p = super::path_bytes(file.path);
                let width = String::from_utf8_lossy(&p).chars().count();
                (p, width)
            } else {
                (Vec::new(), 0)
            },
            scratch: Vec::new(),
            plain_out,
            line_buf: termcolor::NoColor::new(Vec::new()),
            group: None,
            pending: Vec::new(),
            empty: Rc::from(Vec::new()),
            started: Instant::now(),
            structured: within.is_some() || o.expand,
            within,
            expand,
            sections: super::SectionLines::local(file.src.unwrap_or_default()),
            open: None,
            looked: Vec::new(),
            stats: FileStats::default(),
            kept: (0, 0),
            stopped: false,
            has_match: false,
            found: false,
            file,
        }
    }

    /// The Trail of a Match at byte `at`, shared with the rows before it when it is the same
    /// (the cursor shares it). A cursor that fails fails the file: the built-in ones never do
    /// once prepared.
    fn trail_at(&mut self, at: usize) -> std::io::Result<SharedTrail> {
        let Some(c) = self.trails.as_mut() else {
            return Ok(self.empty.clone());
        };
        c.trail_at(at).map_err(std::io::Error::other)
    }

    /// One Trail per submatch of `line`, or for the line start of an inverted Match.
    fn trails_of(&mut self, line: &Line) -> std::io::Result<Vec<SharedTrail>> {
        let base = line.absolute_offset as usize;
        if self.trails.is_none() {
            return Ok(vec![self.empty.clone(); line.submatches.len().max(1)]);
        }
        let mut trails = Vec::with_capacity(line.submatches.len().max(1));
        if line.submatches.is_empty() {
            trails.push(self.trail_at(base)?);
        }
        for s in &line.submatches {
            trails.push(self.trail_at(base + s.start)?);
        }
        Ok(trails)
    }

    /// --expand: where the Matches that a Section may still hold start (`Cursor::open_from`),
    /// told where the search finds the Matches ahead: a Match's Trail is asked at each of its
    /// submatches, found in its line as `match_submatches_in` finds them. An inverted search
    /// asks at every line that does not match, which the cursor is not told. The lines
    /// searched last are kept, with their submatches (`looked`).
    fn open_from(&mut self, s: &Searcher) -> Option<usize> {
        let c = self.trails.as_mut()?;
        let src = match self.file.src {
            Some(src) if !self.invert => src,
            _ => return c.open_from(),
        };
        let matcher = self.matcher;
        let looked = &mut self.looked;
        let mut first = |r: std::ops::Range<usize>| -> Option<usize> {
            // A line kept holds its start: no scan back over a long line for each range
            // (code review r4, Codex #1).
            let mut line = match looked
                .iter()
                .find(|l| l.start <= r.start && r.start < l.end)
            {
                Some(l) => l.start,
                None => memchr::memrchr(b'\n', &src[..r.start]).map_or(0, |i| i + 1),
            };
            while line < r.end {
                let at = match looked.iter().position(|l| l.start == line) {
                    Some(at) => at,
                    None => {
                        let end =
                            memchr::memchr(b'\n', &src[line..]).map_or(src.len(), |i| line + i + 1);
                        let starts = crate::search::submatches_in(s, matcher, src, line..end)
                            .into_iter()
                            .map(|m| line + m.start)
                            .collect();
                        if looked.len() == LOOKED_LINES {
                            looked.remove(0);
                        }
                        looked.push(LookedLine {
                            start: line,
                            end,
                            starts,
                        });
                        looked.len() - 1
                    }
                };
                let l = &looked[at];
                // The submatches start in order: the first at or past the range's start.
                let found = l
                    .starts
                    .get(l.starts.partition_point(|&b| b < r.start))
                    .copied()
                    .filter(|b| r.contains(b));
                if found.is_some() {
                    return found;
                }
                line = l.end;
            }
            None
        };
        c.open_from_with(&mut first)
    }

    /// A line the search reached past the binary data: the first tells whether a gap parts it
    /// from the last row printed.
    fn past_binary(&mut self, line_number: u64) {
        if self.binary_gap.is_none() {
            self.binary_gap = Some(self.prev.is_some_and(|p| p + 1 != line_number));
        }
    }

    /// Notes the line's place: true when it opens a new group after a gap, which ends the
    /// group before (its waiting context lines keep an empty Trail).
    fn step(&mut self, line_number: u64) -> std::io::Result<bool> {
        let gap = self.prev.is_some_and(|p| p + 1 != line_number);
        self.prev = Some(line_number);
        if gap {
            self.flush(None)?;
            self.group = None;
        }
        Ok(gap)
    }

    /// Prints the waiting context lines under `trail` (empty when None).
    fn flush(&mut self, trail: Option<&Rc<[Segment]>>) -> std::io::Result<()> {
        let empty: Rc<[Segment]> = Rc::from(Vec::new());
        let trail = trail.unwrap_or(&empty).clone();
        for p in std::mem::take(&mut self.pending) {
            let src = self.file.src.unwrap_or_default();
            let subs: Vec<(usize, usize)> =
                p.line.submatches.iter().map(|s| (s.start, s.end)).collect();
            let row = Row {
                offset: None,
                trail: &trail,
                line_number: p.line.line_number,
                line: Rc::new(LineText::new(p.line.bytes(src))),
                subs,
                is_match: false,
                gap_before: p.gap_before,
                column: None,
                byte_offset: p.line.absolute_offset,
            };
            self.row(&row)?;
        }
        Ok(())
    }

    /// The file's `lead`, before its first row.
    fn lead(&mut self) -> std::io::Result<()> {
        if let Some(l) = self.file.lead.take() {
            self.w.write_all(l)?;
            self.w.write_all(b"\n")?;
        }
        Ok(())
    }

    /// One row in the tree or flat layout.
    fn row(&mut self, r: &Row) -> std::io::Result<()> {
        self.row_of(r, None)
    }

    /// `row`, the row's Trail `shared` when it has its `Rc`. Without colour or links a flat
    /// row is made in `line_buf` and written at once: the writer under it (`Spill`, a spool)
    /// takes one write per row, not one per piece (code review r1, Fable #2).
    fn row_of(&mut self, r: &Row, shared: Option<&SharedTrail>) -> std::io::Result<()> {
        if self.o.layout == Layout::Flat {
            self.lead()?;
            if !self.plain_out {
                return flat::write_row_shared(self.w, &self.head, r, shared, self.o);
            }
            let buf = &mut self.line_buf;
            buf.get_mut().clear();
            flat::write_row_shared(buf, &self.head, r, shared, self.o)?;
            return self.w.write_all(buf.get_ref());
        }
        if !self.header {
            self.header = true;
            self.tree.header(self.w, self.file.path, self.o)?;
        }
        self.tree.row(self.w, r, self.o)
    }

    fn matched_line(
        &mut self,
        line: Line,
        bytes: &[u8],
        subs: &dyn Fn() -> Vec<Submatch>,
    ) -> std::io::Result<()> {
        let gap = if self.json.is_none() && !self.binary {
            self.step(line.line_number)?
        } else {
            false
        };
        if self.json.is_none()
            && !self.binary
            && self.fast
            && self.write_fast(line.line_number, bytes, true, gap)?
        {
            return Ok(());
        }
        let line = Line {
            submatches: subs(),
            ..line
        };
        let trails = self.trails_of(&line)?;
        if let Some(j) = self.json.as_mut() {
            self.stats.matches += line.submatches.len() as u64;
            let refs: Vec<&[Segment]> = trails.iter().map(|t| &t[..]).collect();
            return j.matched(self.w, &line, bytes, &refs, &[]);
        }
        if self.binary {
            self.past_binary(line.line_number);
            return Ok(());
        }
        self.match_rows(&line, bytes, trails, gap)
    }

    /// The rows of a Match line, as `output::match_rows`: one per distinct Trail, or one under
    /// the first Trail when -M omits the line. `gap`: the line opens a group (`step`).
    fn match_rows(
        &mut self,
        line: &Line,
        bytes: &[u8],
        trails: Vec<SharedTrail>,
        gap: bool,
    ) -> std::io::Result<()> {
        let groups: Vec<RowGroup> = if line.submatches.is_empty() || super::omitted(bytes, self.o) {
            let subs = line.submatches.iter().map(|s| (s.start, s.end)).collect();
            vec![(trails[0].clone(), subs)]
        } else {
            super::group_by_trail(
                trails
                    .into_iter()
                    .zip(line.submatches.iter().map(|s| (s.start, s.end))),
            )
        };
        self.flush(Some(&groups[0].0))?;
        let gap_before = gap && self.o.context;
        // One text for every row of the line, which outlives none of the Trails it is under.
        let text = Rc::new(LineText::new(bytes));
        let mut groups = groups;
        for (n, (trail, subs)) in groups.iter_mut().enumerate() {
            let subs = std::mem::take(subs);
            let row = Row {
                offset: None,
                trail,
                line_number: line.line_number,
                line: text.clone(),
                column: subs.first().map(|s| s.0 as u64 + 1),
                subs,
                is_match: true,
                gap_before: gap_before && n == 0,
                byte_offset: line.absolute_offset,
            };
            self.row_of(&row, Some(trail))?;
        }
        self.group = groups.pop().map(|g| g.0);
        Ok(())
    }

    fn context_line(
        &mut self,
        line: Line,
        bytes: &[u8],
        subs: &dyn Fn() -> Vec<Submatch>,
    ) -> std::io::Result<()> {
        if let Some(j) = self.json.as_mut() {
            let line = Line {
                submatches: subs(),
                ..line
            };
            return j.context(self.w, &line, bytes);
        }
        if self.binary {
            self.past_binary(line.line_number);
            return Ok(());
        }
        let gap = self.step(line.line_number)?;
        if self.fast && self.write_fast(line.line_number, bytes, false, gap)? {
            return Ok(());
        }
        let line = Line {
            submatches: subs(),
            ..line
        };
        self.context_row(line, bytes, gap)
    }

    /// The row of a context line, or its wait for the Trail of its group's first Match.
    fn context_row(&mut self, line: Line, bytes: &[u8], gap: bool) -> std::io::Result<()> {
        let gap_before = gap && self.o.context;
        match self.group.clone() {
            // The first lines of a group wait for the Trail of its first Match.
            None if self.trails.is_some() || self.file.named => {
                self.pending.push(Pending { line, gap_before });
                Ok(())
            }
            group => {
                let trail = group.unwrap_or_else(|| self.empty.clone());
                let subs = line.submatches.iter().map(|s| (s.start, s.end)).collect();
                let row = Row {
                    offset: None,
                    trail: &trail,
                    line_number: line.line_number,
                    line: Rc::new(LineText::new(bytes)),
                    subs,
                    is_match: false,
                    gap_before,
                    column: None,
                    byte_offset: line.absolute_offset,
                };
                self.row(&row)
            }
        }
    }

    /// --in-headings: a line with Matches in Segment names, with the Trail and the Section of
    /// each (`Ctx::heading_file`), in file order. It goes through --within as a Match of the
    /// search would.
    pub fn heading_match(
        &mut self,
        line: Line,
        trails: Vec<SharedTrail>,
        sections: Vec<Section>,
    ) -> std::io::Result<()> {
        self.found = true;
        self.filtered_match(line, trails, Some(sections))
    }

    /// --in-headings: a context line of `heading_match`'s lines.
    pub fn heading_context(&mut self, line: Line) -> std::io::Result<()> {
        self.filtered_context(line)
    }

    /// A Match line through --within (`filter::retain`, `filter::limit`): a Match with some
    /// submatch under a wanted Trail keeps those, and claims the held context lines before it;
    /// one with none is a context line. With `-m`, nothing past the window of the `-m`th
    /// Match kept is printed.
    fn filtered_match(
        &mut self,
        line: Line,
        trails: Vec<SharedTrail>,
        sections: Option<Vec<Section>>,
    ) -> std::io::Result<()> {
        let Some(w) = self.within.as_mut() else {
            return self.structured_match(line, trails, sections);
        };
        let n = line.line_number;
        if w.window_end.is_some_and(|e| n > e) {
            return Ok(());
        }
        let keep: Vec<bool> = trails
            .iter()
            .map(|t| crate::filter::within(t, &w.o.wanted))
            .collect();
        if !keep.iter().any(|&k| k) {
            return self.filtered_context(Line {
                submatches: Vec::new(),
                ..line
            });
        }
        let (line, trails, sections) = if line.submatches.is_empty() {
            (line, trails, sections)
        } else {
            let kept = |k: &usize| keep[*k];
            let submatches = (0..line.submatches.len())
                .filter(kept)
                .map(|k| line.submatches[k])
                .collect();
            let trails = (0..trails.len())
                .filter(kept)
                .map(|k| trails[k].clone())
                .collect();
            let sections = sections.map(|s| (0..s.len()).filter(kept).map(|k| s[k]).collect());
            (Line { submatches, ..line }, trails, sections)
        };
        let from = n.saturating_sub(w.o.before);
        let held: Vec<Line> = w.held.drain(..).filter(|h| h.line_number >= from).collect();
        w.after_until = Some(n.saturating_add(w.o.after));
        w.kept += 1;
        if w.o.max_count.is_some_and(|m| w.kept == m) {
            w.window_end = Some(n.saturating_add(w.o.after));
        }
        for h in held {
            self.structured_context(h)?;
        }
        self.structured_match(line, trails, sections)
    }

    /// A context line through --within: printed in the after context of a kept Match, held
    /// while a later one may claim it, dropped otherwise.
    fn filtered_context(&mut self, line: Line) -> std::io::Result<()> {
        let Some(w) = self.within.as_mut() else {
            return self.structured_context(line);
        };
        let n = line.line_number;
        if w.window_end.is_some_and(|e| n > e) {
            return Ok(());
        }
        if w.after_until.is_some_and(|a| n <= a) {
            return self.structured_context(line);
        }
        // A later Match, past `n`, claims the lines from `n + 1 - before`.
        while w
            .held
            .front()
            .is_some_and(|h| h.line_number + w.o.before <= n)
        {
            w.held.pop_front();
        }
        if w.o.before > 0 {
            w.held.push_back(line);
        }
        Ok(())
    }

    /// A Match line the filters kept: printed, or with --expand given to its Section.
    /// `sections`: --in-headings' own, else those of the Trails.
    fn structured_match(
        &mut self,
        line: Line,
        trails: Vec<SharedTrail>,
        sections: Option<Vec<Section>>,
    ) -> std::io::Result<()> {
        self.has_match = true;
        if let Some(t) = self.file.tally {
            self.kept.0 += 1;
            self.kept.1 += line.submatches.len().max(1) as u64;
            self.stopped = t.stop;
            return Ok(());
        }
        let src = self.file.src.unwrap_or_default();
        let sections = || {
            sections.unwrap_or_else(|| {
                trails
                    .iter()
                    .map(|t| t.last().map(|s| (s.line, s.end_line)))
                    .collect()
            })
        };
        if let Some(j) = self.json.as_mut() {
            let bytes = line.bytes(src);
            let refs: Vec<&[Segment]> = trails.iter().map(|t| &t[..]).collect();
            if !self.o.expand {
                return j.matched(self.w, &line, bytes, &refs, &[]);
            }
            // Each distinct Section of the line made once (`json::render_messages`).
            let sections = sections();
            let mut made: Vec<serde_json::Value> = Vec::new();
            let mut at: HashMap<Section, usize> = HashMap::new();
            let at_line = (line.line_number, line.absolute_offset as usize);
            let index: Vec<usize> = sections
                .iter()
                .map(|&s| {
                    *at.entry(s).or_insert_with(|| {
                        made.push(section_value(src, at_line, s));
                        made.len() - 1
                    })
                })
                .collect();
            let values: Vec<&serde_json::Value> = index.iter().map(|&k| &made[k]).collect();
            return j.matched(self.w, &line, bytes, &refs, &values);
        }
        match self.expand.as_mut().map(|e| &mut e.mode) {
            None | Some(ExpandMode::Rows) => {
                let gap = self.step(line.line_number)?;
                self.match_rows(&line, line.bytes(src), trails, gap)
            }
            Some(ExpandMode::Undecided(lines)) => {
                let sections = sections();
                if sections.iter().all(Option::is_none) {
                    lines.push(Buffered::Match(line, trails));
                    return Ok(());
                }
                // The file prints its Sections: the Matches before, none with a Section, print
                // their rows; the context lines are the Sections' own.
                let before = std::mem::take(lines);
                self.expand.as_mut().expect("matched above").mode = ExpandMode::Sections;
                for b in before {
                    if let Buffered::Match(l, ts) = b {
                        let none = vec![None; ts.len()];
                        self.expand_match(l, ts, none)?;
                    }
                }
                self.expand_match(line, trails, sections)
            }
            Some(ExpandMode::Sections) => {
                let sections = sections();
                self.expand_match(line, trails, sections)
            }
        }
    }

    /// A context line the filters kept.
    fn structured_context(&mut self, line: Line) -> std::io::Result<()> {
        if self.file.tally.is_some() {
            return Ok(());
        }
        let src = self.file.src.unwrap_or_default();
        if let Some(j) = self.json.as_mut() {
            return j.context(self.w, &line, line.bytes(src));
        }
        match self.expand.as_mut().map(|e| &mut e.mode) {
            None | Some(ExpandMode::Rows) => {
                let gap = self.step(line.line_number)?;
                let bytes = in_src(src, &line);
                self.context_row(line, bytes, gap)
            }
            Some(ExpandMode::Undecided(lines)) => {
                lines.push(Buffered::Context(line));
                Ok(())
            }
            // A Section prints its own context.
            Some(ExpandMode::Sections) => Ok(()),
        }
    }

    /// --expand: a Match line given to its Sections (`output::Pieces`): each Section not seen
    /// yet waits for the search to pass its end, and a Match without one prints its rows in
    /// turn. The Matches no waiting or open Section can hold are let go.
    fn expand_match(
        &mut self,
        line: Line,
        trails: Vec<SharedTrail>,
        sections: Vec<Section>,
    ) -> std::io::Result<()> {
        let n = line.line_number;
        self.print_ready(Some(n))?;
        let open = self.open;
        let e = self.expand.as_mut().expect("--expand");
        // A Section holds its Matches (a cursor gives the Segments that hold an offset): one
        // that ended before this line is named by no later Match.
        if open.is_some() {
            e.seen.retain(|&(_, end)| end >= n);
        }
        let index = e.next;
        e.next += 1;
        let mut any = false;
        for (k, s) in sections.iter().enumerate() {
            let Some((start, end)) = *s else { continue };
            any = true;
            if e.seen.insert((start, end)) {
                let trail = trails.get(k).map_or_else(Vec::new, |t| t.to_vec());
                e.push(Queued::Section(trail, start, end), start);
            }
        }
        if !any {
            e.push(Queued::Rows(index), n);
        }
        e.window.file.items.push(Item::Match(line));
        e.window
            .trails
            .push(trails.iter().map(|t| t.to_vec()).collect());
        e.window.sections.push(sections);
        self.print_ready(Some(n))?;
        // What a waiting Section or row holds stays, and so does what a later Section may
        // hold: no later Segment starts before `open`.
        let Some(open) = open else {
            return Ok(());
        };
        let e = self.expand.as_mut().expect("--expand");
        let bound = e.bound();
        let gone = e
            .window
            .file
            .items
            .iter()
            .take_while(|i| {
                let l = item_line(i);
                l.absolute_offset as usize + l.len <= open && l.line_number < bound
            })
            .count();
        if gone > 0 {
            e.window.file.items.drain(..gone);
            e.window.trails.drain(..gone);
            e.window.sections.drain(..gone);
            e.base += gone;
        }
        Ok(())
    }

    /// --expand: prints what waits at the front of the queue, in turn: the rows of a Match
    /// without a Section, and a Section the search is past (`upto`, the line the search
    /// reached; None at the end of the file).
    fn print_ready(&mut self, upto: Option<u64>) -> std::io::Result<()> {
        let Some(e) = self.expand.as_mut() else {
            return Ok(());
        };
        let src = self.file.src.unwrap_or_default();
        let o = self.o;
        loop {
            match e.queue.front() {
                None => break,
                Some(Queued::Section(_, _, end)) if upto.is_some_and(|n| *end >= n) => break,
                Some(_) => {}
            }
            let q = e.pop().expect("a front");
            if let Some(l) = self.file.lead.take() {
                self.w.write_all(l)?;
                self.w.write_all(b"\n")?;
            }
            if o.layout == Layout::Tree && !self.header {
                self.header = true;
                self.tree.header(self.w, self.file.path, o)?;
            }
            match q {
                Queued::Rows(i) => {
                    let k = i - e.base;
                    let Item::Match(m) = &e.window.file.items[k] else {
                        unreachable!("the window holds Matches")
                    };
                    for r in super::match_rows(&e.window, k, m, src, o) {
                        match o.layout {
                            Layout::Flat => flat::write_row(self.w, &self.head, &r, o)?,
                            _ => self.tree.row(self.w, &r, o)?,
                        }
                    }
                }
                Queued::Section(trail, start, end) => {
                    // The blocks of the Matches of the Section's bytes, as `SectionLines::rows`
                    // finds those bytes: a Match before them that ends in them included.
                    let mut seek = LineSeek::from(src, e.seek);
                    let first = seek.start(start.max(1));
                    let last = seek.start(end + 1).max(first);
                    e.seek = seek.position();
                    let items = &e.window.file.items;
                    let lo = items.partition_point(|i| {
                        let l = item_line(i);
                        l.absolute_offset as usize + l.len <= first
                    });
                    let hi = lo
                        + items[lo..]
                            .partition_point(|i| (item_line(i).absolute_offset as usize) < last);
                    let kept = &super::section_rows_of(&e.window, lo..hi, src, o);
                    let section = (&trail[..], start, end);
                    match o.layout {
                        Layout::Flat => flat::write_section(
                            self.w,
                            &self.head,
                            section,
                            &self.sections,
                            kept,
                            o,
                        )?,
                        _ => self
                            .tree
                            .section(self.w, section, &self.sections, kept, o)?,
                    }
                }
            }
        }
        Ok(())
    }

    /// What `flat::write_row` and `tree::write_row` print for a row without a Trail, when there
    /// is no colour and no -M and the line is valid UTF-8 that fits: the prefix and the line.
    /// Returns false, writing nothing, when the line needs the printers (Fragments, U+FFFD).
    fn write_fast(
        &mut self,
        line_number: u64,
        bytes: &[u8],
        is_match: bool,
        gap: bool,
    ) -> std::io::Result<bool> {
        let raw = crate::text::strip_terminator(bytes);
        let sep = if is_match { b':' } else { b'-' };
        let p = &mut self.scratch;
        p.clear();
        let tree = self.o.layout == Layout::Tree;
        if tree {
            p.extend_from_slice(b"  ");
        } else if !self.path_prefix.0.is_empty() {
            p.extend_from_slice(&self.path_prefix.0);
            p.push(sep);
        }
        if self.o.line_numbers {
            let mut digits = [0u8; 20];
            let mut n = line_number;
            let mut at = digits.len();
            loop {
                at -= 1;
                digits[at] = b'0' + (n % 10) as u8;
                n /= 10;
                if n == 0 {
                    break;
                }
            }
            p.extend_from_slice(&digits[at..]);
            p.push(sep);
        }
        // Flat: a Match row puts a space between its head and its text.
        if !tree && is_match && !p.is_empty() {
            p.push(b' ');
        }
        // Characters before the text: the path counts in characters, the rest is ASCII.
        let used = p.len() - self.path_prefix.0.len() + self.path_prefix.1;
        let avail = self.o.width.saturating_sub(used);
        if raw.len() > avail || (!raw.is_ascii() && std::str::from_utf8(raw).is_err()) {
            return Ok(false);
        }
        if !self.header && tree {
            self.header = true;
            self.tree.header(self.w, self.file.path, self.o)?;
        }
        self.lead()?;
        if gap && self.o.context {
            self.w.write_all(if tree { b"  --\n" } else { b"--\n" })?;
        }
        self.w.write_all(&self.scratch)?;
        self.w.write_all(raw)?;
        self.w.write_all(b"\n")?;
        Ok(true)
    }

    /// After the search: the waiting context lines, the tree's path line if no row wrote it,
    /// ripgrep's binary message, or JSON's `end`. Returns the file's JSON stats, for the
    /// summary. Writes nothing for a file without a Match the filters kept; with --json, the
    /// stats of a search whose Matches they dropped still count (spec §6.4).
    pub fn close(mut self) -> std::io::Result<Option<RunStats>> {
        if self.file.tally.is_some() {
            return Ok(None);
        }
        if !self.has_match {
            let counted = self.json.is_some() && self.found;
            return Ok(counted.then(|| super::json::file_stats(&self.stats, 0)));
        }
        if let Some(j) = self.json.as_mut() {
            return j.end(self.w, &self.stats).map(Some);
        }
        let mode = self.expand.as_mut().map(|e| &mut e.mode);
        if let Some(ExpandMode::Undecided(lines)) = mode {
            // No Match had a Section: the rows, as without --expand.
            let lines = std::mem::take(lines);
            self.expand.as_mut().expect("matched above").mode = ExpandMode::Rows;
            let src = self.file.src.unwrap_or_default();
            for l in lines {
                match l {
                    Buffered::Match(line, trails) => {
                        let gap = self.step(line.line_number)?;
                        self.match_rows(&line, line.bytes(src), trails, gap)?;
                    }
                    Buffered::Context(line) => self.structured_context(line)?,
                }
            }
        }
        self.print_ready(None)?;
        self.flush(None)?;
        if self.o.layout == Layout::Tree && !self.header {
            self.tree.header(self.w, self.file.path, self.o)?;
        }
        let msg = super::binary_message(self.stats.binary_offset, true, self.file.explicit, self.o);
        if let Some(msg) = msg {
            if self.binary_gap == Some(true) && self.o.context && self.expand.is_none() {
                match self.o.layout {
                    Layout::Tree => tree::write_binary_break(self.w, self.o)?,
                    _ => flat::write_group_separator(self.w, self.o)?,
                }
            }
            match self.o.layout {
                Layout::Tree => tree::write_binary_message(self.w, &msg)?,
                _ => flat::write_binary_message(self.w, self.file.path, &msg, self.o)?,
            }
        }
        Ok(None)
    }
}

/// The bytes of `line`, a range of `src`.
fn in_src<'s>(src: &'s [u8], line: &Line) -> &'s [u8] {
    let start = (line.absolute_offset as usize).min(src.len());
    &src[start..start.saturating_add(line.len).min(src.len())]
}

fn item_line(i: &Item) -> &Line {
    match i {
        Item::Match(l) | Item::Context(l) => l,
    }
}

/// The starts of the lines of `src`, as `LineIndex::line_start` gives them (the end of `src`
/// past its last line), found by the line terminators around the last one found: a walk over
/// nearby lines costs those lines, and no index of the whole file is kept.
pub(crate) struct LineSeek<'s> {
    src: &'s [u8],
    /// A line, 1-based, and where it starts.
    line: u64,
    at: usize,
}

impl<'s> LineSeek<'s> {
    pub(crate) fn new(src: &'s [u8]) -> Self {
        LineSeek {
            src,
            line: 1,
            at: 0,
        }
    }

    /// From line `known.0`, which starts at `known.1`.
    pub(crate) fn from(src: &'s [u8], known: (u64, usize)) -> Self {
        LineSeek {
            src,
            line: known.0,
            at: known.1,
        }
    }

    /// The line it stands at, and where that line starts.
    pub(crate) fn position(&self) -> (u64, usize) {
        (self.line, self.at)
    }

    /// The start of 1-based line `n`.
    pub(crate) fn start(&mut self, n: u64) -> usize {
        while self.line < n {
            match memchr::memchr(b'\n', &self.src[self.at.min(self.src.len())..]) {
                Some(i) => {
                    self.at += i + 1;
                    self.line += 1;
                }
                None => return self.src.len(),
            }
        }
        while self.line > n && self.at > 0 {
            self.at = memchr::memrchr(b'\n', &self.src[..self.at - 1]).map_or(0, |i| i + 1);
            self.line -= 1;
        }
        self.at
    }
}

/// JSON's `section` of a Match on the line `at` (its number and start), as
/// `json::render_messages` makes it: lines `a..=b` and their text.
fn section_value(src: &[u8], at: (u64, usize), s: Section) -> serde_json::Value {
    let Some((a, b)) = s else {
        return serde_json::Value::Null;
    };
    let mut lines = LineSeek::from(src, at);
    let start = lines.start(a.max(1));
    let end = lines.start(b + 1);
    serde_json::json!({
        "start_line": a,
        "end_line": b,
        "text": String::from_utf8_lossy(&src[start..end.max(start)]).into_owned(),
    })
}

impl<W: WriteColor> Sink for StreamSink<'_, W> {
    type Error = std::io::Error;

    fn begin(&mut self, s: &Searcher) -> Result<bool, Self::Error> {
        self.max_count.begin(s);
        self.started = Instant::now();
        Ok(true)
    }

    fn matched(&mut self, s: &Searcher, m: &SinkMatch<'_>) -> Result<bool, Self::Error> {
        self.max_count.matched();
        self.stats.matched_lines += m.lines().count() as u64;
        self.found = true;
        let (matcher, invert) = (self.matcher, self.invert);
        let subs = || match_submatches_in(s, matcher, invert, m);
        let line = Line {
            line_number: m.line_number().unwrap_or(0),
            absolute_offset: m.absolute_byte_offset(),
            len: m.bytes().len(),
            owned: None,
            submatches: Vec::new(),
        };
        if self.structured {
            // The search's counts, before the filters (spec §6.4).
            let line = Line {
                submatches: subs(),
                ..line
            };
            self.stats.matches += line.submatches.len() as u64;
            let trails = self.trails_of(&line)?;
            if self.expand.is_some() && self.file.tally.is_none() {
                self.open = self.open_from(s);
            }
            self.filtered_match(line, trails, None)?;
        } else if self.file.count && self.json.is_none() {
            let found = subs();
            self.stats.matches += found.len() as u64;
            self.has_match = true;
            self.matched_line(line, m.bytes(), &|| found.clone())?;
        } else {
            self.has_match = true;
            self.matched_line(line, m.bytes(), &subs)?;
        }
        Ok(self.max_count.go_on() && !self.stopped)
    }

    fn context(&mut self, s: &Searcher, c: &SinkContext<'_>) -> Result<bool, Self::Error> {
        self.max_count.context(c);
        let (matcher, invert) = (self.matcher, self.invert);
        let subs = || context_submatches_in(s, matcher, invert, c.bytes());
        let line = Line {
            line_number: c.line_number().unwrap_or(0),
            absolute_offset: c.absolute_byte_offset(),
            len: c.bytes().len(),
            owned: None,
            submatches: Vec::new(),
        };
        if self.structured {
            self.filtered_context(Line {
                submatches: subs(),
                ..line
            })?;
        } else {
            self.context_line(line, c.bytes(), &subs)?;
        }
        Ok(self.max_count.go_on() && !self.stopped)
    }

    fn binary_data(&mut self, _s: &Searcher, _offset: u64) -> Result<bool, Self::Error> {
        self.binary = true;
        Ok(true)
    }

    fn finish(&mut self, _s: &Searcher, f: &SinkFinish) -> Result<(), Self::Error> {
        self.stats.bytes_searched = f.byte_count();
        self.stats.binary_offset = f.binary_byte_offset();
        self.stats.elapsed = self.started.elapsed();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::parse;
    use crate::model::{Described, FileMatches, Input};
    use crate::output::json::render_with_stats;
    use crate::processors::{Processor, markdown::Markdown};
    use crate::search::{matcher, search_source, searcher};
    use crate::source::Source;
    use termcolor::Buffer;

    /// The same file printed by `StreamSink` and by the printers from a Described model; with
    /// `markdown`, Trails come from the Markdown Processor. JSON elapsed times are masked.
    fn both(
        argv: &[&str],
        layout: Layout,
        color: bool,
        named: bool,
        content: &[u8],
        markdown: bool,
    ) -> (String, String) {
        let mut v = vec!["s4"];
        v.extend_from_slice(argv);
        let a = parse(v.iter().map(Into::into).collect()).unwrap();
        let explicit = !argv.contains(&"walked");
        let m = matcher(&a, &a.patterns().unwrap().patterns).unwrap();
        let ro = RenderOptions {
            layout,
            line_numbers: !a.no_line_number,
            with_filename: named,
            width: 40,
            max_columns: a.max_columns,
            context: a.before() > 0 || a.after() > 0,
            expand: false,
            binary: a.binary_choice,
            look: crate::output::Look::default(),
        };
        let jo = JsonOptions {
            line_numbers: ro.line_numbers,
            width: ro.width,
            expand: false,
        };
        let path = Path::new("docs/a.md");
        let src = Source::from_bytes(path.into(), content.to_vec(), explicit);
        let input = Input {
            bytes: src.bytes(),
            path,
            format: "markdown",
            disk: None,
        };
        let params = toml::Table::new();
        let prepared = markdown.then(|| Markdown.prepare(input, &params).unwrap());
        let format = markdown.then_some("markdown");
        let new_buf = || {
            if color {
                Buffer::ansi()
            } else {
                Buffer::no_color()
            }
        };
        // The separator from a file before, as with one thread: both write it before the first
        // row, and not before a binary message alone.
        let lead = (layout == Layout::Flat).then_some(&b"--"[..]);
        let mut streamed = new_buf();
        let mut s = searcher(&a, explicit, true);
        let file = StreamFile {
            path,
            explicit,
            format,
            src: Some(src.bytes()),
            cursor: prepared.as_ref().map(|p| p.cursor()),
            max_count: crate::search::max_count(&a),
            lead,
            within: None,
            count: false,
            named: false,
            tally: None,
        };
        let mut sink = StreamSink::new(&mut streamed, &m, a.invert_match, &ro, &jo, file);
        s.search_reader(&m, std::io::Cursor::new(src.bytes()), &mut sink)
            .unwrap();
        sink.close().unwrap();
        let mut modelled = new_buf();
        if let Some(fm) =
            search_source(&mut s, &m, &src, false, false, crate::search::max_count(&a)).unwrap()
        {
            let trails = if markdown {
                let anchors: Vec<Vec<usize>> = fm.matches().map(|m| m.anchors()).collect();
                let all: Vec<usize> = anchors.iter().flatten().copied().collect();
                let mut it = Markdown
                    .trails(&input, &toml::Table::new(), &all)
                    .unwrap()
                    .into_iter();
                anchors
                    .iter()
                    .map(|a| it.by_ref().take(a.len()).collect())
                    .collect()
            } else {
                fm.matches()
                    .map(|m| vec![Vec::new(); m.submatches.len().max(1)])
                    .collect()
            };
            let d = Described {
                file: FileMatches {
                    path: path.into(),
                    ..fm
                },
                format: format.map(Into::into),
                trails,
                sections: Vec::new(),
                offsets: false,
                replacements: Vec::new(),
            };
            match layout {
                Layout::Flat => {
                    super::super::flat::render(&d, src.bytes(), &ro, lead, &mut modelled).unwrap()
                }
                Layout::Tree => {
                    super::super::tree::render(&d, src.bytes(), &ro, &mut modelled).unwrap()
                }
                Layout::Json => {
                    let mut out = Vec::new();
                    render_with_stats(&d, src.bytes(), &jo, &mut out).unwrap();
                    std::io::Write::write_all(&mut modelled, &out).unwrap();
                }
            }
        }
        let text = |b: Buffer| {
            let s = String::from_utf8(b.into_inner()).unwrap();
            regex_lite::Regex::new(r#""elapsed":\{[^}]*\}"#)
                .unwrap()
                .replace_all(&s, "")
                .into_owned()
        };
        (text(streamed), text(modelled))
    }

    fn check(cases: &[(Vec<&str>, &[u8])], markdown: bool) {
        for (argv, content) in cases {
            let mut argv = argv.clone();
            if argv[0] == "walked" {
                argv.remove(0);
                argv.push("walked");
            }
            for layout in [Layout::Flat, Layout::Tree, Layout::Json] {
                for color in [false, true] {
                    for named in [true, false] {
                        let (s, m) = both(&argv, layout, color, named, content, markdown);
                        assert!(!m.is_empty(), "{argv:?} printed nothing");
                        assert_eq!(s, m, "{argv:?} {layout:?} color={color} named={named}");
                    }
                }
            }
        }
    }

    #[test]
    fn plain_rows_written_as_they_come_equal_the_printers() {
        let long = format!("{}needle{}", "x".repeat(60), "y".repeat(60));
        let mut content =
            format!("one needle\ntwo\nthree\nfour\nfive needle\n{long}\nsix\nneedle caf\u{e9}\n")
                .into_bytes();
        content.extend_from_slice(b"bad \xff needle\nseven\nlast needle");
        let mut binary = b"needle first\nmore\n".to_vec();
        binary.extend_from_slice(b"x\0binary needle\nneedle again\n");
        // A walked file stops at binary data: put it past the first buffer (64 KiB).
        let mut late = b"needle first\n".to_vec();
        late.extend(std::iter::repeat_n(&b"padding line\n"[..], 6000).flatten());
        late.extend_from_slice(b"x\0binary needle\n");
        let cases: Vec<(Vec<&str>, &[u8])> = vec![
            (vec!["needle"], &content),
            (vec!["-C1", "needle"], &content),
            (vec!["-A1", "-N", "needle"], &content),
            (vec!["-v", "needle"], &content),
            (vec!["-v", "-C1", "needle"], &content),
            (vec!["-M", "12", "-C1", "needle"], &content),
            (vec!["-m2", "-A1", "needle"], &content),
            (vec!["needle"], &binary),
            (vec!["walked", "needle"], &late),
            (vec!["walked", "-M", "5", "-C1", "needle"], &late),
            (vec!["-a", "needle"], &binary),
        ];
        check(&cases, false);
    }

    #[test]
    fn outlined_rows_written_as_they_come_equal_the_printers() {
        let guide = include_bytes!("../../tests/fixtures/md/guide.md");
        let long = format!("# Long\n\n{}needle{}\n", "x".repeat(60), "y".repeat(60));
        let nested = b"# A\n\n- ## B needle\n  more needle\n\n- # Head in item\n  needle\n";
        let cases: Vec<(Vec<&str>, &[u8])> = vec![
            (vec!["needle"], guide),
            (vec!["-C1", "needle"], guide),
            (vec!["-C2", "-e", "Linux", "-e", "needle-pkg"], guide),
            (vec!["-B3", "needle-a"], guide),
            (vec!["-v", "needle"], guide),
            (vec!["-v", "-C1", "needle"], guide),
            (vec!["-M", "15", "-C1", "needle"], guide),
            (vec!["-m3", "-A2", "needle"], guide),
            (vec!["-e", "\\| Pro", "-e", "needle-10"], guide),
            (vec!["$"], guide),
            (vec!["-C1", "needle"], long.as_bytes()),
            (vec!["-C1", "needle"], nested),
            (vec!["Head|needle"], nested),
        ];
        check(&cases, true);
    }
}
