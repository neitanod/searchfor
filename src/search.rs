//! Walking and searching with ripgrep's crates (ADR 0001, design notes 3 and 4).
use crate::cli::{Args, BinaryMode, Case};
use crate::model::{FileMatches, FileStats, Item, Line, Submatch};
use crate::source::Source;
use anyhow::Result;
use grep_matcher::Matcher;
use grep_regex::{RegexMatcher, RegexMatcherBuilder};
use grep_searcher::{
    BinaryDetection, MmapChoice, Searcher, SearcherBuilder, Sink, SinkContext, SinkContextKind,
    SinkFinish, SinkMatch,
};
use ignore::{WalkBuilder, overrides::OverrideBuilder, types::TypesBuilder};
use std::ops::Range;
use std::path::{Path, PathBuf};

pub fn matcher(args: &Args, patterns: &[String]) -> Result<RegexMatcher> {
    let mut b = RegexMatcherBuilder::new();
    match args.case() {
        Case::Insensitive => {
            b.case_insensitive(true);
        }
        Case::Smart => {
            b.case_smart(true);
        }
        Case::Sensitive => {}
    }
    b.word(args.word_regexp)
        .whole_line(args.line_regexp)
        .fixed_strings(args.fixed_strings && !args.no_fixed_strings)
        .multi_line(true) // as ripgrep: ^ and $ match at line boundaries inside a buffer
        .line_terminator(Some(b'\n'));
    // An empty pattern set never reaches here: C9 exits 1 before searching.
    Ok(b.build_many(patterns)?)
}

/// ripgrep 14.1.1's memory-map choice, which is also its in-memory vs reader choice: the
/// search runs in memory only when every path given is an existing regular file (no
/// directory, no stdin) and there are at most 10 of them (`--mmap`/`--no-mmap` are not in
/// M1). C9 verifies this rule by behaviour against rg: `--json` on `misc/bin.dat` as one
/// explicit file (second Match on line 2, offset 13), walked inside `misc`, as 11 explicit
/// files, and through stdin (line 3, offset 18).
pub fn rg_in_memory(paths: &[PathBuf], stdin: bool) -> bool {
    !stdin && !paths.is_empty() && paths.len() <= 10 && paths.iter().all(|p| p.is_file())
}

/// `explicit`: the file was named on the command line, or is stdin (design note 3).
/// `decoded`: the input is a Source (BOM already handled: sniffing off). Passthrough and the
/// quiet fast path search raw files and stdin, so they pass `decoded = false` and keep
/// grep-searcher's BOM sniffing, as ripgrep.
pub fn searcher(args: &Args, explicit: bool, decoded: bool) -> Searcher {
    searcher_builder(args, explicit, decoded).build()
}

/// The searcher of the two paths that never build a Source: passthrough and `-q` without
/// structural flags. BOM sniffing is on, and memory maps are used when `mmap` is true, which
/// the caller sets to `rg_in_memory(..)`; then `search_path` runs in memory as ripgrep does,
/// and `search_reader` on stdin returns as soon as the sink stops, before EOF.
pub fn streaming_searcher(args: &Args, explicit: bool, mmap: bool) -> Searcher {
    printing_searcher(args, explicit, mmap, true)
}

/// `streaming_searcher` for passthrough, where grep-printer prints line numbers exactly when
/// the searcher counts them: `line_number` is the printer's setting (design note 9).
pub fn printing_searcher(args: &Args, explicit: bool, mmap: bool, line_number: bool) -> Searcher {
    let mut b = searcher_builder(args, explicit, false);
    b.line_number(line_number);
    if mmap {
        // SAFETY: read-only maps; a file truncated by another process while it is mapped can
        // crash the process, the risk ripgrep accepts too (see source::MMAP_THRESHOLD).
        b.memory_map(unsafe { MmapChoice::auto() });
    }
    b.build()
}

/// ripgrep's binary policy for one file (design note 3). A searcher built for one kind of
/// file is reused for the other with `Searcher::set_binary_detection`.
pub fn binary_detection(args: &Args, explicit: bool) -> BinaryDetection {
    match args.binary_mode(explicit) {
        BinaryMode::Text => BinaryDetection::none(),
        BinaryMode::Binary => BinaryDetection::convert(b'\0'),
        BinaryMode::Quit => BinaryDetection::quit(b'\0'),
    }
}

fn searcher_builder(args: &Args, explicit: bool, decoded: bool) -> SearcherBuilder {
    let binary = binary_detection(args, explicit);
    let mut b = SearcherBuilder::new();
    b.line_number(true) // Trails need line numbers even when they are not printed.
        .before_context(args.before())
        .after_context(args.after())
        .invert_match(args.invert_match)
        // -m is counted by the sinks, as ripgrep 14.1.1's printers count it (`MaxCount`).
        .binary_detection(binary)
        .bom_sniffing(!decoded);
    b
}

/// The `-m` the sinks count. With --within, -m counts the lines the filter keeps: C9 cuts after
/// it (`filter::limit`).
pub fn max_count(args: &Args) -> Option<u64> {
    args.max_count.filter(|_| args.within.is_none())
}

#[cfg(test)]
thread_local! {
    /// The `WalkBuilder`s `walker` built on this thread: one per run (code review r4, Fable #1).
    pub static WALKERS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// `cwd` anchors the globs, as ripgrep does (`-g 'docs/*.md' x docs` works).
pub fn walker(args: &Args, paths: &[PathBuf], cwd: &Path) -> Result<WalkBuilder> {
    #[cfg(test)]
    WALKERS.with(|n| n.set(n.get() + 1));
    let first = paths.first().cloned().unwrap_or_else(|| PathBuf::from("."));
    let mut w = WalkBuilder::new(&first);
    for p in paths.iter().skip(1) {
        w.add(p);
    }
    let no_ignore = args.no_ignore();
    w.hidden(!args.hidden())
        .ignore(!no_ignore)
        .git_ignore(!no_ignore)
        .git_global(!no_ignore)
        .git_exclude(!no_ignore)
        .parents(!no_ignore)
        .follow_links(args.follow && !args.no_follow)
        .max_depth(args.max_depth)
        .skip_stdout(true);
    if !no_ignore {
        w.add_custom_ignore_filename(".rgignore");
    }
    // The file types before the globs, as ripgrep 14.1.1 validates them (code review r3).
    let mut tb = rg_types()?;
    // In command-line order, as ripgrep: the last selection of a type wins.
    for (select, t) in &args.type_selections {
        if *select {
            tb.select(t);
        } else {
            tb.negate(t);
        }
    }
    w.types(tb.build()?);
    // As ripgrep 14.1.1: every -g first, then every --iglob (case-insensitive); the relative
    // order of the two kinds on the command line does not matter (measured in review r2).
    let mut ob = OverrideBuilder::new(cwd);
    for g in &args.globs {
        ob.add(g)?;
    }
    if !args.iglobs.is_empty() {
        ob.case_insensitive(true)?;
        for g in &args.iglobs {
            ob.add(g)?;
        }
    }
    w.overrides(ob.build()?);
    if args.sort.as_deref() == Some("path") {
        w.sort_by_file_path(|a, b| a.cmp(b));
    }
    w.threads(walker_threads(args));
    Ok(w)
}

/// Whether ripgrep 14.1.1 searches with one thread (hiargs.rs:167): under `--sort path`, with
/// one file, or when the walker's threads are one.
pub fn one_thread(args: &Args, one_file: bool) -> bool {
    args.sort.as_deref() == Some("path") || one_file || walker_threads(args) == 1
}

/// The walker's threads: `-j N`, or ripgrep 14.1.1's default when `-j` is absent or 0, the
/// available parallelism up to 12; ignore 0.4.23 alone would use 2.
pub fn walker_threads(args: &Args) -> usize {
    match args.threads {
        Some(j) if j > 0 => j,
        _ => std::thread::available_parallelism()
            .map_or(1, |n| n.get())
            .min(12),
    }
}

/// ripgrep 14.1.1's file types, as `rg --type-list` prints them: `name: glob, glob`.
pub const RG_TYPES: &str = include_str!("rg_types.txt");

/// A `TypesBuilder` with ripgrep 14.1.1's file types, which differ from the ignore crate's
/// defaults.
pub fn rg_types() -> anyhow::Result<TypesBuilder> {
    let mut tb = TypesBuilder::new();
    for line in RG_TYPES.lines() {
        let (name, globs) = line
            .split_once(": ")
            .ok_or_else(|| anyhow::anyhow!("bad file type line: {line}"))?;
        for g in globs.split(", ") {
            tb.add(name, g)?;
        }
    }
    Ok(tb)
}

/// ripgrep 14.1.1's `-m`, counted by its printers (grep-printer 0.2.2, standard.rs:785 and
/// json.rs:651) and so by our sinks: the searcher (grep-searcher 0.1.14, rg 14.1.1's) has no
/// limit of its own. After the limit the after context goes on; a line that matches inside it
/// is a Match that does not extend it, and the search stops where it ends. Summary printers
/// stop at the limit (summary.rs:552). A sink that stops its search makes the searcher stop
/// reading, and the searcher's `byte_count` is then rg's `bytes_searched`.
#[derive(Debug, Clone, Default)]
pub struct MaxCount {
    limit: Option<u64>,
    /// A summary printer's -m: no after context.
    summary: bool,
    after: u64,
    count: u64,
    after_left: u64,
    stopped: bool,
}

impl MaxCount {
    pub fn new(limit: Option<u64>) -> Self {
        Self {
            limit,
            ..Self::default()
        }
    }

    /// The -m of grep-printer 0.2.2's summary printer: it stops at the limit.
    pub fn summary(limit: Option<u64>) -> Self {
        Self {
            summary: true,
            ..Self::new(limit)
        }
    }

    /// For a sink's `begin`: a new search with the searcher's after context.
    pub fn begin(&mut self, s: &Searcher) {
        *self = Self {
            limit: self.limit,
            summary: self.summary,
            after: if self.summary {
                0
            } else {
                s.after_context() as u64
            },
            ..Self::default()
        };
    }

    /// A Match line.
    pub fn matched(&mut self) {
        self.count += 1;
        if self.limit.is_some_and(|l| self.count > l) {
            self.after_left = self.after_left.saturating_sub(1);
        } else {
            self.after_left = self.after;
        }
        self.check();
    }

    /// A context line.
    pub fn context(&mut self, c: &SinkContext<'_>) {
        if *c.kind() == SinkContextKind::After {
            self.after_left = self.after_left.saturating_sub(1);
        }
        self.check();
    }

    /// The sink stops the search for its own reason (-q's first Match, -l).
    pub fn stop_now(&mut self) {
        self.stopped = true;
    }

    /// The search was stopped, at the limit or by `stop_now`.
    pub fn is_stopped(&self) -> bool {
        self.stopped
    }

    /// What the sink answers the searcher.
    pub fn go_on(&self) -> bool {
        !self.stopped
    }

    fn check(&mut self) {
        if self.limit.is_some_and(|l| self.count >= l) && self.after_left == 0 {
            self.stopped = true;
        }
    }
}

/// Builds the model of one file (design note 4) and its stats. Lines are not copied (they are
/// ranges of the Source) except after binary data in a reader search, whose NUL conversion makes
/// them differ from the Source (`reader`).
pub struct CollectSink<'m> {
    matcher: &'m RegexMatcher,
    invert: bool,
    /// `-q` without structural flags: stop the file at its first Match (design note 10).
    quiet: bool,
    /// The search reads through a reader (not a slice in memory).
    reader: bool,
    max_count: MaxCount,
    pub items: Vec<Item>,
    pub stats: FileStats,
}

impl<'m> CollectSink<'m> {
    pub fn new(matcher: &'m RegexMatcher, invert: bool, quiet: bool) -> Self {
        Self {
            matcher,
            invert,
            quiet,
            reader: true,
            max_count: MaxCount::default(),
            items: Vec::new(),
            stats: FileStats::default(),
        }
    }

    /// `-m` (see `MaxCount`).
    pub fn limit(mut self, limit: Option<u64>) -> Self {
        self.max_count = MaxCount::new(limit);
        self.reader = true;
        self
    }

    /// The sink stopped its search (at `-m`, or at `-q`'s first Match).
    pub fn stopped(&self) -> bool {
        self.max_count.is_stopped()
    }

    /// The search runs in memory: lines always equal the Source's bytes.
    pub fn in_memory(mut self) -> Self {
        self.reader = false;
        self
    }

    fn copy(&self, bytes: &[u8]) -> Option<Box<[u8]>> {
        (self.reader && self.stats.binary_cut.is_some()).then(|| bytes.into())
    }
}

/// Numbers the lines of `items`, reported in file order by a search that did not count lines
/// (`printing_searcher` without line numbers, as ripgrep searches without `-n`), from
/// `bytes`, the Source the items' offsets point into: a line's number is one more than the
/// line terminators before it. Only for a search that converted no binary data, whose NUL
/// bytes the searcher would have counted as line terminators.
pub fn number_lines(items: &mut [Item], bytes: &[u8]) {
    let (mut counted, mut line) = (0usize, 1u64);
    for item in items {
        let (Item::Match(l) | Item::Context(l)) = item;
        let at = (l.absolute_offset as usize).min(bytes.len());
        if at < counted {
            (counted, line) = (0, 1);
        }
        line += memchr::memchr_iter(b'\n', &bytes[counted..at]).count() as u64;
        counted = at;
        l.line_number = line;
    }
}

/// grep-printer 0.2.2's JSON sink (`record_matches`, json.rs:611–644, and
/// `find_iter_at_in_context`, util.rs:467–522), copied because those helpers are
/// `pub(crate)`. Only the single-line branch: our searcher never runs in multi-line mode
/// (`-U` is not in M1). The line terminator is trimmed from the end of `range` (a `\r`
/// stays: CRLF mode is not in M1) and the matcher runs over the buffer up to there, from
/// `range.start`, so `\A` sees what ripgrep's printer sees (`bar\nfoo needle\n` with
/// `\Afoo|needle` reports only `needle`). Matches starting at or after `range.end` stop
/// the search; offsets are relative to `range.start`.
pub fn submatches(matcher: &RegexMatcher, buffer: &[u8], range: Range<usize>) -> Vec<Submatch> {
    let mut end = range.end;
    if buffer[..end].ends_with(b"\n") {
        end -= 1;
    }
    let haystack = &buffer[..end];
    let mut subs = Vec::new();
    // RegexMatcher's error type is NoError: the search cannot fail.
    let _ = matcher.find_iter_at(haystack, range.start, |m| {
        if m.start() >= range.end {
            return false;
        }
        subs.push(Submatch {
            start: m.start() - range.start,
            end: m.end() - range.start,
        });
        true
    });
    // As grep-printer: an empty match appearing at the end of the bytes is not reported.
    // (It compares the relative start against the whole buffer's length.)
    if subs
        .last()
        .is_some_and(|s| s.start == s.end && s.start >= buffer.len())
    {
        subs.pop();
    }
    subs
}

/// The submatches of a reported line as ripgrep's printers record them: none on an inverted
/// Match; on a context line, only with `-v`, searched in the line's own bytes.
pub fn match_submatches(matcher: &RegexMatcher, invert: bool, m: &SinkMatch<'_>) -> Vec<Submatch> {
    // grep-printer records matches on inverted lines too; none are found there.
    if invert {
        Vec::new()
    } else {
        submatches(matcher, m.buffer(), m.bytes_range_in_buffer())
    }
}

pub fn context_submatches(matcher: &RegexMatcher, invert: bool, bytes: &[u8]) -> Vec<Submatch> {
    // grep-printer's context path searches the line's own bytes (range 0..len).
    if invert {
        submatches(matcher, bytes, 0..bytes.len())
    } else {
        Vec::new()
    }
}

impl Sink for CollectSink<'_> {
    type Error = std::io::Error;

    fn begin(&mut self, s: &Searcher) -> Result<bool, Self::Error> {
        self.max_count.begin(s);
        Ok(true)
    }

    fn matched(&mut self, _s: &Searcher, m: &SinkMatch<'_>) -> Result<bool, Self::Error> {
        self.max_count.matched();
        let subs = match_submatches(self.matcher, self.invert, m);
        self.stats.matched_lines += m.lines().count() as u64;
        self.stats.matches += subs.len() as u64;
        self.items.push(Item::Match(Line {
            line_number: m.line_number().unwrap_or(0),
            absolute_offset: m.absolute_byte_offset(),
            len: m.bytes().len(),
            owned: self.copy(m.bytes()),
            submatches: subs,
        }));
        if self.quiet {
            self.max_count.stop_now();
        }
        Ok(self.max_count.go_on())
    }

    fn context(&mut self, _s: &Searcher, c: &SinkContext<'_>) -> Result<bool, Self::Error> {
        self.max_count.context(c);
        self.items.push(Item::Context(Line {
            line_number: c.line_number().unwrap_or(0),
            absolute_offset: c.absolute_byte_offset(),
            len: c.bytes().len(),
            owned: self.copy(c.bytes()),
            submatches: context_submatches(self.matcher, self.invert, c.bytes()),
        }));
        Ok(self.max_count.go_on())
    }

    fn binary_data(&mut self, _s: &Searcher, _offset: u64) -> Result<bool, Self::Error> {
        self.stats.binary_cut.get_or_insert(self.items.len());
        Ok(true)
    }

    fn finish(&mut self, _s: &Searcher, f: &SinkFinish) -> Result<(), Self::Error> {
        self.stats.bytes_searched = f.byte_count();
        self.stats.binary_offset = f.binary_byte_offset();
        Ok(())
    }
}

/// Where a reader search detects binary data, keeping no line (code review r7, Codex #4): the end
/// of the bytes it searched before the buffer that brings the NUL, and the searcher's
/// `binary_offset` and `bytes_searched`.
#[derive(Debug, Default)]
pub struct BinarySink {
    /// The end of the bytes searched before the buffer with the binary data: rg reads the names
    /// in them, also those of lines the pattern does not find (code review r9, Codex #1).
    pub cut: u64,
    /// The end of the last line reported before `binary_data`: the cut of an input whose reads
    /// `Reads` cannot follow (UTF-16).
    reported: u64,
    detected: bool,
    pub binary_offset: Option<u64>,
    pub bytes_searched: u64,
}

impl BinarySink {
    fn line(&mut self, offset: u64, len: usize) {
        if !self.detected {
            self.reported = offset + len as u64;
        }
    }
}

impl Sink for BinarySink {
    type Error = std::io::Error;

    fn matched(&mut self, _s: &Searcher, m: &SinkMatch<'_>) -> Result<bool, Self::Error> {
        self.line(m.absolute_byte_offset(), m.bytes().len());
        Ok(true)
    }

    fn context(&mut self, _s: &Searcher, c: &SinkContext<'_>) -> Result<bool, Self::Error> {
        self.line(c.absolute_byte_offset(), c.bytes().len());
        Ok(true)
    }

    fn binary_data(&mut self, _s: &Searcher, _offset: u64) -> Result<bool, Self::Error> {
        self.detected = true;
        Ok(true)
    }

    fn finish(&mut self, _s: &Searcher, f: &SinkFinish) -> Result<(), Self::Error> {
        self.bytes_searched = f.byte_count();
        self.binary_offset = f.binary_byte_offset();
        Ok(())
    }
}

/// The reads of a reader search under grep-searcher 0.1.14's decoder, which follow where its
/// search stands when it detects the first NUL. `LineBuffer::fill` reads until a read brings a
/// line terminator and the search takes the buffer up to the last one, so the bytes searched
/// before the buffer with the NUL end at the last line terminator read before the read that
/// brings it. With a context, `Core::roll` carries the last lines of a buffer into the next:
/// they were searched, and rg's `bytes_searched` stops before them. The decoder, sniffing a
/// BOM, gives the first three bytes in a read of their own, strips a UTF-8 BOM, and transcodes
/// UTF-16 in reads of its own.
struct Reads<R> {
    inner: R,
    /// The first three bytes, for the BOM.
    head: Vec<u8>,
    /// The bytes read so far.
    read: u64,
    /// The end of the last line terminator read before the read with the first NUL.
    line_end: u64,
    /// `line_end` when the read with the first NUL came.
    before_nul: Option<u64>,
}

impl<R: std::io::Read> std::io::Read for Reads<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        let bytes = &buf[..n];
        if self.before_nul.is_none() {
            if memchr::memchr(0, bytes).is_some() {
                // The bytes read for the BOM are the search's first read.
                self.before_nul = Some(if self.read < 3 { 0 } else { self.line_end });
            } else if let Some(i) = memchr::memrchr(b'\n', bytes) {
                self.line_end = self.read + i as u64 + 1;
            }
        }
        let head = 3usize.saturating_sub(self.head.len()).min(n);
        self.head.extend_from_slice(&bytes[..head]);
        self.read += n as u64;
        Ok(n)
    }
}

impl<R> Reads<R> {
    /// Where the bytes searched before the first NUL end, in the decoded bytes. None: UTF-16.
    fn cut(&self) -> Option<u64> {
        let at = self.before_nul?;
        match self.head.as_slice() {
            [0xEF, 0xBB, 0xBF] => Some(at.saturating_sub(3)),
            [0xFF, 0xFE, ..] | [0xFE, 0xFF, ..] => None,
            _ => Some(at),
        }
    }
}

/// Searches `reader` with `BinarySink`: a Source's bytes, as `search_source` would, or the
/// input itself when it cannot be read twice, so that its reads are rg's. `searcher` sniffs
/// BOMs, as rg's. `matcher` and the searcher's context are rg's: grep-searcher's `Core::roll`
/// keeps the context lines after the last line it reported for the next buffer, so the lines it
/// reports decide where each buffer ends (code review r8, Fable #1). The cut is where the bytes
/// it searched before the buffer with the NUL end, which the lines the pattern finds do not
/// decide (code review r9, Codex #1).
pub fn binary_probe(
    searcher: &mut Searcher,
    matcher: &RegexMatcher,
    reader: impl std::io::Read,
) -> Result<BinarySink> {
    let mut sink = BinarySink::default();
    let mut reads = Reads {
        inner: reader,
        head: Vec::with_capacity(3),
        read: 0,
        line_end: 0,
        before_nul: None,
    };
    searcher.search_reader(matcher, &mut reads, &mut sink)?;
    sink.cut = match reads.cut() {
        Some(cut) => cut,
        // UTF-16: where the search stopped at the binary data, past the last line reported, or
        // that line. The context lines carried into the buffer with the NUL are left out.
        None if searcher.binary_detection().quit_byte().is_some() => {
            sink.reported.max(sink.bytes_searched)
        }
        None => sink.reported,
    };
    Ok(sink)
}

/// Searches a Source. Returns None when nothing matched (stats are discarded: ripgrep's
/// summary counts only files with Matches, verified with rg 14.1.1).
///
/// The search path decides how `BinaryDetection::convert` behaves: the reader path converts NUL
/// bytes into line terminators, the in-memory path does not
/// (grep-searcher-0.1.14/src/searcher/mod.rs:83), and line numbers and offsets differ
/// accordingly. ripgrep 14.1.1 searches memory-mapped (in memory) only in the cases of
/// `rg_in_memory`, and with a reader otherwise; we do the same on the Source's bytes, so our
/// coordinates are ripgrep's. C9 passes `in_memory && !src.had_bom`, because grep-searcher
/// reads a file with a BOM through a reader even when memory maps are on; the reader reads
/// `Source::as_read`, so with a BOM `searcher` must sniff it (`streaming_searcher`). When the
/// result has a binary offset, C9 skips the Processor.
pub fn search_source(
    searcher: &mut Searcher,
    matcher: &RegexMatcher,
    src: &Source,
    in_memory: bool,
    quiet: bool,
    limit: Option<u64>,
) -> Result<Option<FileMatches>> {
    Ok(search_with(searcher, matcher, src, in_memory, quiet, limit)?.0)
}

/// `search_source` without `quiet`, with the file's stats also when nothing matched (for the
/// summary of `--json -q`).
pub fn search_source_stats(
    searcher: &mut Searcher,
    matcher: &RegexMatcher,
    src: &Source,
    in_memory: bool,
    limit: Option<u64>,
) -> Result<(Option<FileMatches>, FileStats)> {
    search_with(searcher, matcher, src, in_memory, false, limit)
}

fn search_with(
    searcher: &mut Searcher,
    matcher: &RegexMatcher,
    src: &Source,
    in_memory: bool,
    quiet: bool,
    limit: Option<u64>,
) -> Result<(Option<FileMatches>, FileStats)> {
    let started = std::time::Instant::now();
    let mut sink = CollectSink::new(matcher, searcher.invert_match(), quiet).limit(limit);
    if in_memory {
        sink = sink.in_memory();
        searcher.search_slice(matcher, src.bytes(), &mut sink)?;
    } else {
        searcher.search_reader(matcher, std::io::Cursor::new(src.as_read()), &mut sink)?;
    }
    sink.stats.elapsed = started.elapsed();
    let stats = sink.stats.clone();
    if !sink.items.iter().any(|i| matches!(i, Item::Match(_))) {
        return Ok((None, stats));
    }
    let fm = FileMatches {
        path: src.path.clone(),
        explicit: src.explicit,
        items: sink.items,
        stats: sink.stats,
    };
    Ok((Some(fm), stats))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::parse;
    use std::ffi::OsString;

    fn args(v: &[&str]) -> Args {
        parse(v.iter().map(OsString::from).collect()).unwrap()
    }

    #[test]
    fn file_types_are_ripgreps() {
        let tb = rg_types().unwrap();
        let defs = tb.definitions();
        assert_eq!(defs.len(), RG_TYPES.lines().count());
        assert_eq!(defs.len(), 206);
        // The ignore crate has `*.env` in `sh` and a `pkgbuild` type; rg 14.1.1 has neither.
        let sh = defs.iter().find(|d| d.name() == "sh").unwrap();
        assert!(!sh.globs().iter().any(|g| g == "*.env" || g == ".env"));
        assert!(defs.iter().all(|d| d.name() != "pkgbuild"));
        let printed: String = defs
            .iter()
            .map(|d| format!("{}: {}\n", d.name(), d.globs().join(", ")))
            .collect();
        assert_eq!(printed, RG_TYPES);
    }

    /// ignore 0.4.23 walks with 2 threads when given none; ripgrep 14.1.1 picks its own default,
    /// and `-j0` means that default.
    #[test]
    fn walker_threads_default_as_ripgrep() {
        let rg_default = std::thread::available_parallelism()
            .map_or(1, |n| n.get())
            .min(12);
        assert_eq!(walker_threads(&args(&["s4", "needle"])), rg_default);
        assert_eq!(walker_threads(&args(&["s4", "-j0", "needle"])), rg_default);
        assert_eq!(walker_threads(&args(&["s4", "-j3", "needle"])), 3);
    }

    /// Code review r5, Fable #2: ripgrep 14.1.1 searches with one thread under `--sort`, with
    /// one file (stdin, or a path that is not a directory) and with `-j1` (hiargs.rs:167), and
    /// then its printer writes the separator between files.
    #[test]
    fn one_thread_as_ripgrep() {
        let many = walker_threads(&args(&["s4", "needle"])) > 1;
        assert!(one_thread(
            &args(&["s4", "--sort", "path", "needle"]),
            false
        ));
        assert!(one_thread(&args(&["s4", "-j1", "needle"]), false));
        assert!(one_thread(&args(&["s4", "-j4", "needle"]), true));
        assert!(!one_thread(&args(&["s4", "-j4", "needle"]), false));
        assert_eq!(one_thread(&args(&["s4", "needle"]), false), !many);
        assert_eq!(
            one_thread(&args(&["s4", "--sort=none", "needle"]), false),
            !many
        );
    }
    fn fixture(p: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(p)
    }

    /// A reader search over `Source::as_read`, which sniffs a BOM as ripgrep's.
    fn run(a: &Args, pats: &[&str], src: &Source) -> Option<FileMatches> {
        let pats: Vec<String> = pats.iter().map(|s| s.to_string()).collect();
        search_source(
            &mut streaming_searcher(a, src.explicit, false),
            &matcher(a, &pats).unwrap(),
            src,
            false,
            false,
            max_count(a),
        )
        .unwrap()
    }

    fn run_in_memory(a: &Args, pats: &[&str], src: &Source) -> Option<FileMatches> {
        let pats: Vec<String> = pats.iter().map(|s| s.to_string()).collect();
        search_source(
            &mut searcher(a, src.explicit, true),
            &matcher(a, &pats).unwrap(),
            src,
            true,
            false,
            max_count(a),
        )
        .unwrap()
    }

    /// (line number, absolute offset, submatches) of every Match, as `rg --json` reports them.
    type Reported = (u64, u64, Vec<(usize, usize)>);

    fn summary(fm: &FileMatches) -> Vec<Reported> {
        fm.matches()
            .map(|m| {
                (
                    m.line_number,
                    m.absolute_offset,
                    m.submatches.iter().map(|s| (s.start, s.end)).collect(),
                )
            })
            .collect()
    }

    /// A search that does not count lines, numbered afterwards from the Source, reports the
    /// lines a counting search reports: context, `-v`, `-m`, a BOM, UTF-16 and CRLF.
    #[test]
    fn lines_numbered_after_the_search_are_the_counted_ones() {
        let utf16: Vec<u8> = [0xFF, 0xFE]
            .into_iter()
            .chain(
                "a\nb needle\nc\nneedle\n"
                    .encode_utf16()
                    .flat_map(u16::to_le_bytes),
            )
            .collect();
        let inputs: [&[u8]; 4] = [
            b"one\ntwo needle\nthree\n\nneedle\nlast",
            b"\xEF\xBB\xBFx\nneedle\ny\nneedle",
            &utf16,
            b"a\r\nneedle\r\nb\r\nneedle\r\n",
        ];
        for flags in [&["-C1"][..], &["-v"], &["-m1", "-A1"], &[]] {
            let mut v = vec!["s4"];
            v.extend_from_slice(flags);
            v.push("x");
            let a = args(&v);
            let m = matcher(&a, &["needle".to_string()]).unwrap();
            for bytes in inputs {
                let src = Source::from_bytes("a.txt".into(), bytes.to_vec(), false);
                let collect = |s: &mut Searcher| {
                    let mut sink = CollectSink::new(&m, a.invert_match, false).limit(max_count(&a));
                    s.search_reader(&m, src.as_read(), &mut sink).unwrap();
                    sink.items
                };
                let counted = collect(&mut streaming_searcher(&a, false, false));
                let mut numbered = collect(&mut printing_searcher(&a, false, false, false));
                number_lines(&mut numbered, src.bytes());
                assert!(!counted.is_empty(), "{flags:?} {bytes:?}");
                assert_eq!(numbered, counted, "{flags:?} {bytes:?}");
            }
        }
    }

    #[test]
    fn match_and_context_with_offsets() {
        let src = Source::from_bytes("a.md".into(), b"one\ntwo needle\nthree\n".to_vec(), false);
        let fm = run(&args(&["s4", "-C1", "x"]), &["needle"], &src).unwrap();
        assert_eq!(fm.items.len(), 3);
        let Item::Match(lm) = &fm.items[1] else {
            panic!()
        };
        assert_eq!(
            (lm.line_number, lm.absolute_offset, lm.submatches[0].start),
            (2, 4, 4)
        );
        assert_eq!(lm.anchors(), vec![8]);
        assert_eq!((fm.stats.matched_lines, fm.stats.bytes_searched), (1, 21));
        assert!(run(&args(&["s4", "x"]), &["absent"], &src).is_none());
    }

    #[test]
    fn stats_count_submatches_and_lines() {
        let src = Source::from_bytes("a".into(), b"needle needle\nx\nneedle\n".to_vec(), false);
        let fm = run(&args(&["s4", "x"]), &["needle"], &src).unwrap();
        assert_eq!((fm.stats.matched_lines, fm.stats.matches), (2, 3));
        assert_eq!(fm.stats.binary_offset, None);
        assert_eq!(fm.stats.binary_cut, None);
        assert_eq!(fm.path, PathBuf::from("a"));
    }

    #[test]
    fn offsets_are_after_the_bom_and_utf16_is_decoded() {
        for f in ["misc/bom.md", "misc/utf16.md"] {
            let src = Source::open(&fixture(f), true).unwrap();
            let fm = run(&args(&["s4", "x"]), &["needle"], &src).unwrap();
            let Item::Match(lm) = &fm.items[0] else {
                panic!()
            };
            assert_eq!((lm.line_number, lm.absolute_offset), (2, 4), "{f}");
        }
    }

    #[test]
    fn binary_policy_explicit_vs_walked() {
        let walked = Source::open(&fixture("misc/bin.dat"), false).unwrap();
        assert!(run(&args(&["s4", "x"]), &["binary"], &walked).is_none());
        let explicit = Source::open(&fixture("misc/bin.dat"), true).unwrap();
        let fm = run(&args(&["s4", "x"]), &["binary"], &explicit).unwrap();
        assert!(fm.stats.binary_offset.is_some());
    }

    #[test]
    fn binary_cut_counts_the_items_before_the_nul() {
        // Walked (quit): the early Match is recorded, then the search stops at the NUL.
        let src = Source::open(&fixture("misc/late-nul.txt"), false).unwrap();
        let fm = run(&args(&["s4", "x"]), &["needle"], &src).unwrap();
        assert_eq!(fm.matches().count(), 1);
        assert_eq!(fm.stats.binary_cut, Some(1));
        // rg 14.1.1 --json walking misc: `"binary_offset":70013` in late-nul.txt's end message.
        assert_eq!(fm.stats.binary_offset, Some(70013));
        // -a: no binary detection at all.
        let fm = run(&args(&["s4", "-a", "x"]), &["needle"], &src).unwrap();
        assert_eq!(fm.matches().count(), 2);
        assert_eq!((fm.stats.binary_cut, fm.stats.binary_offset), (None, None));
    }

    #[test]
    fn submatches_as_ripgrep_records_them() {
        let a = args(&["s4", "x"]);
        let crlf = Source::from_bytes("a".into(), b"a\r\n".to_vec(), false);
        let Item::Match(m) = &run(&a, &["\\r"], &crlf).unwrap().items[0] else {
            panic!()
        };
        assert_eq!(m.submatches, vec![Submatch { start: 1, end: 2 }]);
        let Item::Match(m) = &run(&a, &["$"], &crlf).unwrap().items[0] else {
            panic!()
        };
        assert_eq!(m.submatches, vec![Submatch { start: 2, end: 2 }]);
        let eof = Source::from_bytes("a".into(), b"a".to_vec(), false);
        let Item::Match(m) = &run(&a, &["$"], &eof).unwrap().items[0] else {
            panic!()
        };
        assert!(m.submatches.is_empty());
    }

    #[test]
    fn empty_match_at_the_end_of_an_unterminated_last_line() {
        // rg 14.1.1: `printf 'x\na' | rg --json '$'` (and the same as a file) reports 1..1 on
        // line 1 and no submatch on line 2.
        let src = Source::from_bytes("a".into(), b"x\na".to_vec(), false);
        let a = args(&["s4", "x"]);
        let want = vec![(1, 0, vec![(1, 1)]), (2, 2, vec![])];
        assert_eq!(summary(&run(&a, &["$"], &src).unwrap()), want);
        assert_eq!(summary(&run_in_memory(&a, &["$"], &src).unwrap()), want);
    }

    #[test]
    fn submatches_see_the_buffer_like_ripgrep() {
        let src = Source::from_bytes("a".into(), b"bar\nfoo needle\n".to_vec(), false);
        let fm = run(&args(&["s4", "x"]), &["\\Afoo|needle"], &src).unwrap();
        let Item::Match(m) = &fm.items[0] else {
            panic!()
        };
        assert_eq!(m.submatches, vec![Submatch { start: 4, end: 10 }]);
        let anchored = Source::from_bytes("a".into(), b"x\nneedle\n".to_vec(), false);
        let fm = run(&args(&["s4", "x"]), &["^needle"], &anchored).unwrap();
        let Item::Match(m) = &fm.items[0] else {
            panic!()
        };
        assert_eq!(m.submatches, vec![Submatch { start: 0, end: 6 }]);
    }

    #[test]
    fn anchor_fixture_matches_ripgrep_json() {
        // rg 14.1.1 --json on misc/anchor.txt, measured.
        let src = Source::open(&fixture("misc/anchor.txt"), true).unwrap();
        let a = args(&["s4", "x"]);
        for in_memory in [false, true] {
            let fm = search_source(
                &mut searcher(&a, true, true),
                &matcher(&a, &["\\Afoo|needle".into()]).unwrap(),
                &src,
                in_memory,
                false,
                None,
            )
            .unwrap()
            .unwrap();
            assert_eq!(
                summary(&fm),
                vec![(2, 4, vec![(4, 10)]), (3, 15, vec![(0, 6)])]
            );
        }
        assert_eq!(
            summary(&run(&a, &["^needle"], &src).unwrap()),
            vec![(3, 15, vec![(0, 6)])]
        );
    }

    #[test]
    fn explicit_file_in_memory_keeps_the_nul() {
        // rg 14.1.1 on one explicit file (memory-mapped): second match on line 2, offset 13.
        let src = Source::from_bytes(
            "bin.dat".into(),
            b"needle first\nmore\0binary needle\n".to_vec(),
            true,
        );
        let a = args(&["s4", "x"]);
        let fm = search_source(
            &mut searcher(&a, true, true),
            &matcher(&a, &["needle".into()]).unwrap(),
            &src,
            true,
            false,
            None,
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            fm.matches()
                .map(|m| (m.line_number, m.absolute_offset))
                .last(),
            Some((2, 13))
        );
        // rg 14.1.1 `--json needle misc/bin.dat`, end message: binary_offset 17,
        // bytes_searched 17, matched_lines 2, matches 2.
        let s = &fm.stats;
        assert_eq!(
            (
                s.binary_offset,
                s.bytes_searched,
                s.matched_lines,
                s.matches
            ),
            (Some(17), 17, 2, 2)
        );
        // Through a reader (rg on stdin): binary_offset 17, bytes_searched 32.
        let fm = run(&a, &["needle"], &src).unwrap();
        assert_eq!(
            (fm.stats.binary_offset, fm.stats.bytes_searched),
            (Some(17), 32)
        );
    }

    #[test]
    fn binary_stdin_coordinates_follow_nul_conversion() {
        // rg 14.1.1: `printf 'needle first\nmore\0binary needle\n' | rg --json needle` reports
        // the second match on line 3, absolute offset 18.
        let src = Source::from_bytes(
            "<stdin>".into(),
            b"needle first\nmore\0binary needle\n".to_vec(),
            true,
        );
        let fm = run(&args(&["s4", "x"]), &["needle"], &src).unwrap();
        let lines: Vec<(u64, u64)> = fm
            .matches()
            .map(|m| (m.line_number, m.absolute_offset))
            .collect();
        assert_eq!(lines.last(), Some(&(3, 18)));
        assert!(fm.stats.binary_offset.is_some());
    }

    #[test]
    fn quiet_stops_at_the_first_match() {
        let src = Source::from_bytes("a".into(), b"needle\nneedle\n".to_vec(), false);
        let a = args(&["s4", "x"]);
        let fm = search_source(
            &mut searcher(&a, false, true),
            &matcher(&a, &["needle".into()]).unwrap(),
            &src,
            false,
            true,
            None,
        )
        .unwrap()
        .unwrap();
        assert_eq!(fm.matches().count(), 1);
    }

    fn stream_path(mmap: bool, explicit: bool, path: &Path) -> FileMatches {
        let a = args(&["s4", "x"]);
        let m = matcher(&a, &["needle".into()]).unwrap();
        let mut sink = CollectSink::new(&m, false, false);
        streaming_searcher(&a, explicit, mmap)
            .search_path(&m, path, &mut sink)
            .unwrap();
        FileMatches {
            path: path.into(),
            explicit,
            items: sink.items,
            stats: sink.stats,
        }
    }

    #[test]
    fn printing_searcher_counts_lines_only_when_asked() {
        // grep-printer prints line numbers exactly when the searcher counts them.
        let a = args(&["s4", "x"]);
        assert!(!printing_searcher(&a, true, false, false).line_number());
        assert!(printing_searcher(&a, true, false, true).line_number());
        assert!(streaming_searcher(&a, true, false).line_number());
    }

    #[test]
    fn binary_detection_per_file() {
        let a = args(&["s4", "x"]);
        assert_eq!(binary_detection(&a, false).quit_byte(), Some(b'\0'));
        assert_eq!(binary_detection(&a, true).convert_byte(), Some(b'\0'));
        let text = args(&["s4", "-a", "x"]);
        assert_eq!(binary_detection(&text, true).convert_byte(), None);
        assert_eq!(binary_detection(&text, false).quit_byte(), None);
    }

    #[test]
    fn streaming_searcher_maps_only_when_asked() {
        // In memory (rg on one explicit file): line 2, offset 13; with a reader: line 3, 18.
        let p = fixture("misc/bin.dat");
        assert_eq!(
            summary(&stream_path(true, true, &p))
                .last()
                .map(|m| (m.0, m.1)),
            Some((2, 13))
        );
        assert_eq!(
            summary(&stream_path(false, true, &p))
                .last()
                .map(|m| (m.0, m.1)),
            Some((3, 18))
        );
    }

    #[test]
    fn streaming_searcher_sniffs_the_bom() {
        // Raw files keep BOM sniffing on: offsets after the BOM, UTF-16 decoded, as rg.
        for f in ["misc/bom.md", "misc/utf16.md"] {
            for mmap in [false, true] {
                assert_eq!(
                    summary(&stream_path(mmap, true, &fixture(f))),
                    vec![(2, 4, vec![(0, 6)])],
                    "{f} {mmap}"
                );
            }
        }
    }

    /// Yields its chunks (in pieces if `buf` is short) and then fails: a search that reads past them errors.
    struct Endless(Vec<&'static [u8]>);
    impl std::io::Read for Endless {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.0.is_empty() {
                return Err(std::io::Error::other("read past the first Match"));
            }
            let n = self.0[0].len().min(buf.len());
            buf[..n].copy_from_slice(&self.0[0][..n]);
            self.0[0] = &self.0[0][n..];
            if self.0[0].is_empty() {
                self.0.remove(0);
            }
            Ok(n)
        }
    }

    #[test]
    fn quiet_stream_returns_before_eof() {
        let a = args(&["s4", "x"]);
        let m = matcher(&a, &["needle".into()]).unwrap();
        let mut sink = CollectSink::new(&m, false, true);
        streaming_searcher(&a, true, false)
            .search_reader(&m, Endless(vec![b"x\nneedle\n"]), &mut sink)
            .unwrap();
        assert_eq!(sink.items.len(), 1);
        // Without -q the same input reads on and hits the error.
        let mut sink = CollectSink::new(&m, false, false);
        assert!(
            streaming_searcher(&a, true, false)
                .search_reader(&m, Endless(vec![b"x\nneedle\n"]), &mut sink)
                .is_err()
        );
    }

    #[test]
    fn inverted_context_lines_carry_submatches() {
        let src = Source::from_bytes("a".into(), b"keep\nneedle\n".to_vec(), false);
        let fm = run(&args(&["s4", "-v", "-C1", "x"]), &["needle"], &src).unwrap();
        let Item::Context(c) = &fm.items[1] else {
            panic!("{:?}", fm.items)
        };
        assert_eq!(c.submatches.len(), 1);
        let Item::Match(m) = &fm.items[0] else {
            panic!()
        };
        assert!(m.submatches.is_empty());
        assert_eq!(fm.stats.matches, 0);
    }

    #[test]
    fn matcher_flags() {
        let src = Source::from_bytes("a".into(), b"Needle\nneedles\na.b\n".to_vec(), false);
        let n = |argv: &[&str], p: &str| {
            run(&args(argv), &[p], &src).map_or(0, |fm| fm.matches().count())
        };
        assert_eq!(n(&["s4", "x"], "needle"), 1);
        assert_eq!(n(&["s4", "-i", "x"], "needle"), 2);
        assert_eq!(n(&["s4", "-S", "x"], "needle"), 2);
        assert_eq!(n(&["s4", "-S", "x"], "Needle"), 1);
        assert_eq!(n(&["s4", "-w", "-i", "x"], "needle"), 1);
        assert_eq!(n(&["s4", "-x", "x"], "needles"), 1);
        assert_eq!(n(&["s4", "-x", "x"], "needle"), 0);
        assert_eq!(n(&["s4", "-F", "x"], "a.b"), 1);
        assert_eq!(n(&["s4", "-F", "x"], "a.."), 0);
        assert_eq!(n(&["s4", "x"], "a.."), 1);
        assert_eq!(n(&["s4", "-m1", "x"], "eedle"), 1);
    }

    #[test]
    fn in_memory_rule() {
        let d = tempfile::tempdir().unwrap();
        let files: Vec<PathBuf> = (0..11)
            .map(|i| {
                let p = d.path().join(format!("{i}.txt"));
                std::fs::write(&p, "x").unwrap();
                p
            })
            .collect();
        assert!(rg_in_memory(&files[..1], false));
        assert!(rg_in_memory(&files[..10], false));
        assert!(!rg_in_memory(&files, false));
        assert!(!rg_in_memory(&files[..1], true));
        assert!(!rg_in_memory(&[], false));
        assert!(!rg_in_memory(&[d.path().into()], false));
        assert!(!rg_in_memory(&[files[0].clone(), d.path().into()], false));
        assert!(!rg_in_memory(&[d.path().join("missing")], false));
    }

    fn walk_names(a: &Args, paths: &[PathBuf], cwd: &Path) -> Vec<String> {
        let mut v: Vec<String> = walker(a, paths, cwd)
            .unwrap()
            .build()
            .flatten()
            .filter(|e| e.file_type().is_some_and(|t| t.is_file()))
            .map(|e| {
                e.path()
                    .strip_prefix(cwd)
                    .unwrap_or(e.path())
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        v.sort();
        v
    }

    #[test]
    fn walker_respects_gitignore_and_rgignore() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir(d.path().join(".git")).unwrap();
        std::fs::write(d.path().join(".gitignore"), "skip.md\n").unwrap();
        std::fs::write(d.path().join(".rgignore"), "also.md\n").unwrap();
        for n in ["keep.md", "skip.md", "also.md"] {
            std::fs::write(d.path().join(n), "x").unwrap();
        }
        assert_eq!(
            walk_names(&args(&["s4", "x"]), &[d.path().into()], d.path()),
            ["keep.md"]
        );
        assert_eq!(
            walk_names(
                &args(&["s4", "--no-ignore", "x"]),
                &[d.path().into()],
                d.path()
            ),
            ["also.md", "keep.md", "skip.md"]
        );
    }

    #[test]
    fn walker_hidden_files_and_types() {
        let d = tempfile::tempdir().unwrap();
        for n in ["a.md", ".h.md", "b.rs"] {
            std::fs::write(d.path().join(n), "x").unwrap();
        }
        let root = [d.path().to_path_buf()];
        assert_eq!(
            walk_names(&args(&["s4", "x"]), &root, d.path()),
            ["a.md", "b.rs"]
        );
        assert_eq!(
            walk_names(&args(&["s4", "--hidden", "x"]), &root, d.path()),
            [".h.md", "a.md", "b.rs"]
        );
        // A selected type whitelists hidden files too: `rg --files -t markdown` lists `.h.md`
        // (rg 14.1.1, measured).
        assert_eq!(
            walk_names(&args(&["s4", "-t", "markdown", "x"]), &root, d.path()),
            [".h.md", "a.md"]
        );
        assert_eq!(
            walk_names(&args(&["s4", "-T", "markdown", "x"]), &root, d.path()),
            ["b.rs"]
        );
        // The last selection of a type wins, as in rg 14.1.1 (code review r1, Codex #3).
        assert_eq!(
            walk_names(
                &args(&["s4", "-T", "markdown", "-t", "markdown", "x"]),
                &root,
                d.path()
            ),
            [".h.md", "a.md"]
        );
        // A selection whitelists, even when it is negated later: nothing is left.
        assert!(
            walk_names(
                &args(&["s4", "-t", "markdown", "-T", "markdown", "x"]),
                &root,
                d.path()
            )
            .is_empty()
        );
    }

    #[test]
    fn a_glob_whitelists_over_gitignore_as_in_ripgrep() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir(d.path().join(".git")).unwrap();
        std::fs::write(d.path().join(".gitignore"), "skip.md\n").unwrap();
        for n in ["keep.md", "skip.md", "other.txt"] {
            std::fs::write(d.path().join(n), "x").unwrap();
        }
        assert_eq!(
            walk_names(
                &args(&["s4", "-g", "*.md", "x"]),
                &[d.path().into()],
                d.path()
            ),
            ["keep.md", "skip.md"]
        );
    }

    #[test]
    fn iglobs_go_after_globs_whatever_the_order() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir(d.path().join(".git")).unwrap();
        std::fs::write(d.path().join(".gitignore"), "skip.md\n").unwrap();
        for n in ["keep.md", "skip.md"] {
            std::fs::write(d.path().join(n), "x").unwrap();
        }
        for argv in [
            &["s4", "--iglob", "*.MD", "-g", "!skip.md", "x"][..],
            &["s4", "-g", "!skip.md", "--iglob", "*.MD", "x"],
        ] {
            assert_eq!(
                walk_names(&args(argv), &[d.path().into()], d.path()),
                ["keep.md", "skip.md"],
                "{argv:?}"
            );
        }
    }

    #[test]
    fn globs_are_relative_to_cwd() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir(d.path().join("sub")).unwrap();
        std::fs::write(d.path().join("sub/x.md"), "x").unwrap();
        let a = args(&["s4", "-g", "sub/*.md", "x"]);
        assert_eq!(
            walk_names(&a, &[d.path().join("sub")], d.path()),
            ["sub/x.md"]
        );
    }

    /// `binary_probe` of a walked file with `argv`'s pattern and context: its cut, and where it
    /// detects the binary data.
    fn probe(argv: &[&str], raw: &[u8]) -> (u64, Option<u64>) {
        let a = args(argv);
        let m = matcher(&a, &a.patterns().unwrap().patterns).unwrap();
        let mut s = streaming_searcher(&a, false, false);
        let p = binary_probe(&mut s, &m, std::io::Cursor::new(raw)).unwrap();
        (p.cut, p.binary_offset)
    }

    /// Code review r9, Codex #1: the cut is where the bytes searched before the buffer with the
    /// NUL end, also when the pattern finds no line before it (`^needle` on `# needle`) and with
    /// a context, whose last two lines grep-searcher carries into that buffer.
    #[test]
    fn binary_probe_cuts_where_the_bytes_searched_end() {
        let edge = b"# needle edge\n";
        let mut v = b"# needle 0\n".to_vec();
        v.extend_from_slice(&b"y".repeat(65_536 - edge.len() - v.len() - 1));
        v.push(b'\n');
        v.extend_from_slice(edge);
        v.extend_from_slice(b"x\0x\n");
        for argv in [
            &["s4", "^needle"][..],
            &["s4", "-A1", "^needle"],
            &["s4", "y"],
        ] {
            assert_eq!(probe(argv, &v), (65_536, Some(65_537)), "{argv:?}");
        }
        // A UTF-8 BOM, stripped: the same bytes searched.
        let bom = [&b"\xEF\xBB\xBF"[..], &v].concat();
        assert_eq!(
            probe(&["s4", "-A1", "^needle"], &bom),
            (65_536, Some(65_537))
        );
        // The decoder reads the first three bytes for a BOM: grep-searcher searches `a\n` and
        // detects the NUL in its next read, or in the first.
        assert_eq!(probe(&["s4", "^needle"], b"a\nb\0\n"), (2, Some(3)));
        assert_eq!(probe(&["s4", "^needle"], b"a\n\0\n"), (0, Some(2)));
        assert_eq!(probe(&["s4", "^needle"], b"# a\nb\0\n"), (0, Some(5)));
    }

    /// UTF-16, whose reads the decoder transcodes: the cut is the last line reported, or at a
    /// walked file's binary data the bytes searched.
    #[test]
    fn binary_probe_of_utf16_cuts_at_the_bytes_searched() {
        let mut raw = vec![0xFF, 0xFE];
        for c in "a\nb\n\0\n".encode_utf16() {
            raw.extend_from_slice(&c.to_le_bytes());
        }
        let (cut, offset) = probe(&["s4", "^needle"], &raw);
        assert_eq!(offset, Some(4));
        assert!(cut <= 4, "{cut}");
    }
}
