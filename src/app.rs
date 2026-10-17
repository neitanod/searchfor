//! The pipeline (plan, Task 9.1): command line, actions, walk, search, Processor, filters and
//! printers, with ripgrep's exit codes (design notes 9 to 11).
use crate::cli::{self, Args, CliError, ColorWhen};
use crate::filter;
use crate::formats::{self, Registry, detector::Detector};
use crate::model::{Described, FileMatches, FileStats, Input, Item, Line, Name, Submatch, Trail};
use crate::output::{
    self, Layout, RenderOptions,
    json::JsonOptions,
    json::RunStats,
    stream::{StreamFile, StreamSink},
    summary,
};
use crate::processors::{self, Processor};
use crate::search::{self, CollectSink, MaxCount};
use crate::source::Source;
use crate::trail::{LineIndex, Outline};
use grep_matcher::Matcher;
use grep_printer::{
    ColorSpecs, JSON, JSONBuilder, Standard, StandardBuilder, Stats, Summary, SummaryBuilder,
    SummaryKind,
};
use grep_regex::RegexMatcher;
use grep_searcher::{BinaryDetection, Searcher, Sink, SinkContext, SinkFinish, SinkMatch};
use ignore::{DirEntry, WalkState};
use std::ffi::OsString;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::time::Instant;
use termcolor::{Buffer, BufferWriter, ColorChoice, WriteColor};

pub fn run(argv: Vec<OsString>) -> i32 {
    let args = match cli::parse(argv) {
        Ok(a) => a,
        Err(CliError::Clap(e)) => {
            let _ = e.print();
            return e.exit_code();
        }
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    match main(&args) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("sfor: {e:#}");
            2
        }
    }
}

/// Error and warning messages on stderr; `--no-messages` silences them, and an error still
/// makes the exit code 2 (as ripgrep).
struct Messages {
    silent: bool,
    errored: AtomicBool,
}

impl Messages {
    fn error(&self, msg: impl std::fmt::Display) {
        self.errored.store(true, Relaxed);
        if !self.silent {
            eprintln!("sfor: {msg}");
        }
    }

    fn warn(&self, msg: impl std::fmt::Display) {
        if !self.silent {
            eprintln!("{msg}");
        }
    }
}

/// The built-in Definitions, those of the installed Grammar Helpers and the user's (D26).
fn load_registry(msgs: &Messages, user_dir: Option<&Path>) -> Registry {
    let (registry, warnings) = Registry::load_all(
        user_dir,
        crate::grammars::grammars_dir().as_deref(),
        &processors::Processors::known,
    );
    for w in warnings {
        msgs.warn(w);
    }
    registry
}

/// The Processors of one run (design note 3), warning on stderr unless `--no-messages`.
fn run_processors(args: &Args, threads: usize) -> processors::Processors {
    let silent = args.no_messages;
    let warn: crate::helper::Warn = std::sync::Arc::new(move |m: String| {
        if !silent {
            eprintln!("{m}");
        }
    });
    processors::Processors::new(threads, warn).with_grammars(crate::grammars::grammars_dir())
}

/// `--list-grammars` (spec §7): the compiled-in and installed Grammars, and those the index
/// offers for this system; an unreadable index is a warning, and the exit code stays 0 (D28).
fn list_grammars(msgs: &Messages) -> i32 {
    use crate::grammars::{self, index::Index, install};
    let index = grammars::index::fetch(&grammars::index_url())
        .and_then(|b| Index::parse(&b))
        .map_err(|e| format!("{e:#}"));
    if let Err(e) = &index {
        msgs.warn(format_args!(
            "sfor: the Grammar index is not available: {e}"
        ));
    }
    let installed = grammars::grammars_dir()
        .map(|d| grammars::installed(&d))
        .unwrap_or_default();
    print!(
        "{}",
        install::list(
            processors::treesitter::grammars::COMPILED_IN,
            &installed,
            index.as_ref().map_err(String::as_str),
        )
    );
    0
}

/// `--install-grammar <LANG>` (spec §7): exit 0 once installed, 2 on any error.
fn install_grammar(lang: &str) -> i32 {
    use crate::grammars::{self, index::Index, install};
    let done = (|| {
        let dir = grammars::grammars_dir().ok_or_else(|| {
            anyhow::anyhow!("there is no Grammar directory (no data directory, and SFOR_GRAMMARS_DIR is not set)")
        })?;
        let index = Index::parse(&grammars::index::fetch(&grammars::index_url())?)?;
        install::install(&index, lang, &dir)
    })();
    match done {
        Ok(i) => {
            println!("installed {} {} in {}", i.name, i.version, i.dir.display());
            0
        }
        Err(e) => {
            eprintln!("sfor: {e:#}");
            2
        }
    }
}

fn main(args: &Args) -> anyhow::Result<i32> {
    let msgs = Messages {
        silent: args.no_messages,
        errored: AtomicBool::new(false),
    };
    // Actions (step 3).
    if args.list_grammars {
        return Ok(list_grammars(&msgs));
    }
    if let Some(lang) = &args.install_grammar {
        return Ok(install_grammar(lang));
    }
    if args.list_formats || args.export_formats.is_some() {
        let user_dir = formats::user_formats_dir();
        let registry = load_registry(&msgs, user_dir.as_deref());
        if args.list_formats {
            // Built for the listing only: it reads the Grammar directory and starts nothing.
            let procs = run_processors(args, 1);
            print!("{}", registry.list_with(&|d| procs.available(d)));
            return Ok(0);
        }
        let dir = args.export_formats.as_deref().expect("checked above");
        return Ok(match formats::export_builtin(dir, user_dir.as_deref()) {
            Ok(written) => {
                for p in written {
                    println!("{}", p.display());
                }
                0
            }
            Err(e) => {
                eprintln!("sfor: {e:#}");
                2
            }
        });
    }
    if args.type_list {
        std::io::stdout()
            .lock()
            .write_all(search::RG_TYPES.as_bytes())?;
        return Ok(0);
    }

    // Patterns and paths (step 4).
    if args.pattern.is_none() && args.regexp.is_empty() && args.pattern_files.is_empty() {
        eprintln!("sfor: a pattern is required: give one, or use -e or -f");
        return Ok(2);
    }
    let pats = args.patterns()?;
    let (paths, implicit_stdin, implicit_cwd) = if !pats.paths.is_empty() {
        (pats.paths, false, false)
    } else if !pats.stdin_used && grep_cli::is_readable_stdin() {
        (vec![PathBuf::from("-")], true, false)
    } else {
        (vec![PathBuf::from(".")], false, true)
    };
    // Nothing can match: ripgrep 14.1.1 searches nothing and exits 1, in every mode and without
    // a JSON summary, once its file types and globs are valid (an unknown type is still an
    // error); the patterns are not compiled. We validate our Formats as well.
    if pats.patterns.is_empty() || args.max_count == Some(0) {
        search::walker(args, &paths, &std::env::current_dir()?)?;
        if !args.formats.is_empty() {
            let registry = load_registry(&msgs, formats::user_formats_dir().as_deref());
            Detector::new(&registry, &args.formats)?;
        }
        return Ok(1);
    }
    let matcher = search::matcher(args, &pats.patterns)?;
    let one_file = paths.len() == 1 && (paths[0] == Path::new("-") || !paths[0].is_dir());
    let with_filename = if args.with_filename {
        true
    } else if args.no_filename {
        false
    } else {
        !one_file
    };
    let tty = std::io::IsTerminal::is_terminal(&std::io::stdout());
    let color = match args.color {
        ColorWhen::Never => ColorChoice::Never,
        ColorWhen::Auto if tty => ColorChoice::Auto,
        ColorWhen::Auto => ColorChoice::Never,
        ColorWhen::Always => ColorChoice::Always,
        ColorWhen::Ansi => ColorChoice::AlwaysAnsi,
    };
    // ripgrep 14.1.1 runs `-v --count-matches` as `-v --count` (hiargs.rs:129).
    let summary_mode = if args.count || (args.count_matches && args.invert_match) {
        Some(summary::Mode::Count)
    } else if args.count_matches {
        Some(summary::Mode::CountMatches)
    } else if args.files_with_matches {
        Some(summary::Mode::FilesWithMatches)
    } else if args.files_without_match {
        Some(summary::Mode::FilesWithoutMatch)
    } else {
        None
    };
    let context = args.before() > 0 || args.after() > 0;
    let mode = if args.quiet && args.json && !args.structural() {
        Mode::QuietJson
    } else if args.quiet && !args.structural() {
        Mode::Quiet
    } else if !args.quiet && (args.no_trail || (summary_mode.is_some() && !args.structural())) {
        Mode::Passthrough
    } else {
        Mode::Trail
    };
    let layout = if args.json {
        Layout::Json
    } else if args.tree {
        Layout::Tree
    } else if args.flat {
        Layout::Flat
    } else if tty {
        Layout::Tree
    } else {
        Layout::Flat
    };
    let heading = tty;
    // The separator between files, which BufferWriter prints between two non-empty file
    // outputs, as ripgrep's. One file needs none, so its output is printed as it grows
    // (`Ctx::spill`), and a closed pipe stops the search as rg's (code review r4 replay).
    let separator = match mode {
        _ if one_file => None,
        Mode::Passthrough if args.json || summary_mode.is_some() => None,
        Mode::Passthrough if heading => Some(Vec::new()),
        Mode::Passthrough if context => Some(b"--".to_vec()),
        Mode::Trail if summary_mode.is_some() || args.quiet => None,
        Mode::Trail if layout == Layout::Tree => Some(Vec::new()),
        Mode::Trail if layout == Layout::Flat && context => Some(b"--".to_vec()),
        _ => None,
    };
    // With one thread, ripgrep 14.1.1's printer writes it instead, before a file's first line
    // (hiargs.rs:625-634), so none goes before a file whose output is only the binary message
    // (code review r5, Fable #2): passthrough's printer, and the flat layout as rg's. The tree
    // prints a path line for that file too, and the separator goes with the path line, as rg's.
    let sequential = search::one_thread(args, one_file);
    let line_separator = match mode {
        Mode::Passthrough if sequential => separator.clone(),
        Mode::Trail if sequential && layout == Layout::Flat => separator.clone(),
        _ => None,
    };
    let separator = separator.filter(|_| line_separator.is_none());
    let mut out = BufferWriter::stdout(color);
    out.separator(separator.clone());
    let width = if tty && layout != Layout::Json {
        terminal_size::terminal_size().map_or(args.fragment_width, |(w, _)| w.0 as usize)
    } else {
        args.fragment_width
    };
    let ctx = Ctx {
        args,
        matcher,
        msgs,
        in_memory: search::rg_in_memory(&paths, implicit_stdin),
        paths,
        strip_dot: implicit_cwd,
        with_filename,
        summary: summary_mode,
        out,
        quit: AtomicBool::new(false),
        matched: AtomicBool::new(false),
        started: Instant::now(),
        stats: Mutex::new(RunStats::default()),
        spill: separator.is_none() && sequential,
        sequential,
        line_separator,
        printed: AtomicBool::new(false),
        processors: run_processors(
            args,
            if sequential {
                1
            } else {
                search::walker_threads(args)
            },
        ),
    };
    match mode {
        Mode::Quiet => ctx.walk(
            || search::streaming_searcher(args, false, ctx.in_memory),
            |s, t| ctx.quiet_file(s, &t),
        )?,
        Mode::QuietJson => {
            ctx.walk(
                || search::streaming_searcher(args, false, ctx.in_memory),
                |s, t| ctx.quiet_json_file(s, &t),
            )?;
            ctx.print_summary();
        }
        Mode::Passthrough => {
            let line_number =
                passthrough_line_number(args, summary_mode.is_some(), tty, &ctx.paths);
            ctx.walk(
                || {
                    let searcher =
                        search::printing_searcher(args, false, ctx.in_memory, line_number);
                    (searcher, ctx.printer(heading))
                },
                |(s, p), t| ctx.passthrough_file(s, p, &t),
            )?;
            if args.json {
                ctx.print_summary();
            }
        }
        Mode::Trail => {
            let user_dir = formats::user_formats_dir();
            let registry = load_registry(&ctx.msgs, user_dir.as_deref());
            let detector = Detector::new(&registry, &args.formats)?;
            let ro = RenderOptions {
                layout,
                line_numbers: !args.no_line_number,
                with_filename,
                width,
                max_columns: args.max_columns,
                context,
                expand: args.expand,
                binary: args.binary_choice,
            };
            ctx.walk(
                || TrailState {
                    stream: search::streaming_searcher(args, false, false),
                    source: search::searcher(args, false, true),
                    buf: ctx.out.buffer(),
                    binary_quit: false,
                },
                |st, t| ctx.trail_file(st, &t, &detector, &ro),
            )?;
            if layout == Layout::Json && summary_mode.is_none() {
                ctx.print_summary();
            }
        }
    }
    let matched = ctx.matched.load(Relaxed);
    let errored = ctx.msgs.errored.load(Relaxed);
    Ok(if matched && (args.quiet || !errored) {
        0
    } else if errored {
        2
    } else {
        1
    })
}

/// ripgrep 14.1.1's default for line numbers in passthrough (hiargs.rs:200-221): `-n` and `-N`
/// first, always with `--json`, never with a summary, and else on a tty unless stdin, implicit
/// or `-`, is all that is searched (hiargs.rs:214 and 1128), so a pipe reads as a pipe.
fn passthrough_line_number(args: &Args, summary: bool, tty: bool, paths: &[PathBuf]) -> bool {
    let only_stdin = paths.len() == 1 && paths[0] == Path::new("-");
    if args.line_number {
        true
    } else if args.no_line_number {
        false
    } else if args.json {
        true
    } else {
        !summary && tty && !only_stdin
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// `-q` without structural flags: no Source, stop at the first Match (step 10).
    Quiet,
    /// `--json -q` without structural flags: every file searched for the `summary` only, as
    /// ripgrep 14.1.1's summary printer with statistics (code review r2, Codex #7).
    QuietJson,
    /// grep-printer's own output (design note 9).
    Passthrough,
    /// Sources, Processors, filters and our printers.
    Trail,
}

/// One file to search: its printed path, its path on disk (None for stdin), whether it was
/// named on the command line (design note 3), and whether the path is a regular file, which
/// can be opened again and read the same (not stdin, `/dev/stdin` or a FIFO).
struct Target {
    path: PathBuf,
    fs: Option<PathBuf>,
    explicit: bool,
    regular: bool,
}

/// What a Trail-mode thread keeps between files: a searcher for files and stdin (BOM sniffing
/// on, a reader), one for Sources (already decoded), and the output buffer.
struct TrailState {
    stream: Searcher,
    source: Searcher,
    buf: Buffer,
    /// The last `describe`d file's search quit at binary data (`BinaryDetection::quit`).
    binary_quit: bool,
}

/// What `stream_file` searches: the file or stdin as it is; the Source, in memory as ripgrep
/// searches explicit files; or the Source of a file a reader search already found a Match in,
/// searched again as that first search read the file.
#[derive(Clone, Copy)]
enum Streamed<'s> {
    Raw,
    Source(&'s Source),
    Again(&'s Source),
}

/// A reader that keeps a copy of what it reads: stdin, or a path that cannot be read twice,
/// searched before its Source exists.
struct Tee<R> {
    inner: R,
    read: Vec<u8>,
}

impl<R: Read> Read for Tee<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.read.extend_from_slice(&buf[..n]);
        Ok(n)
    }
}

enum Printer<'a> {
    Standard(Standard<SpillLines<'a>>),
    Summary(Summary<SpillLines<'a>>),
    Json(JSON<SpillLines<'a>>),
}

struct Ctx<'a> {
    args: &'a Args,
    matcher: RegexMatcher,
    msgs: Messages,
    paths: Vec<PathBuf>,
    /// The current directory was searched implicitly: print `md/a.md`, not `./md/a.md`.
    strip_dot: bool,
    in_memory: bool,
    with_filename: bool,
    summary: Option<summary::Mode>,
    out: BufferWriter,
    /// Set to stop every worker: `-q` found a Match, or stdout was closed.
    quit: AtomicBool,
    /// A file had a kept Match (or, for --files-without-match, was listed).
    matched: AtomicBool,
    started: Instant,
    /// JSON totals of the printed files, for the final `summary` message.
    stats: Mutex<RunStats>,
    /// A file's rows may be printed while it is searched (`Spill`): no other file's output can
    /// come between them, and no separator goes between two prints.
    spill: bool,
    /// ripgrep 14.1.1 would search with one thread (`search::one_thread`).
    sequential: bool,
    /// The separator written before a file's first line once something was printed, where the
    /// BufferWriter does not print it (see `main`).
    line_separator: Option<Vec<u8>>,
    /// Something was printed: the flat layout's `line_separator` goes before the next file.
    printed: AtomicBool,
    /// The run's Processors (design note 3), dropped with `Ctx`, which closes every helper's
    /// stdin (spec §4.4, Lifetime).
    processors: processors::Processors,
}

impl Ctx<'_> {
    /// Walks the paths, sequentially where ripgrep 14.1.1 uses one thread (`--sort path`, `-j1`,
    /// one file), else in parallel with one state per thread (design note 11), and calls `visit`
    /// for every file to search.
    fn walk<S: Send>(
        &self,
        init: impl Fn() -> S + Sync,
        visit: impl Fn(&mut S, Target) + Sync,
    ) -> anyhow::Result<()> {
        let cwd = std::env::current_dir()?;
        let w = search::walker(self.args, &self.paths, &cwd)?;
        if self.sequential {
            let mut state = init();
            for r in w.build() {
                if self.quit.load(Relaxed) {
                    break;
                }
                match r {
                    Ok(d) => {
                        if let Some(t) = self.target(&d) {
                            visit(&mut state, t);
                        }
                    }
                    Err(e) => self.msgs.error(e),
                }
            }
        } else {
            let (init, visit) = (&init, &visit);
            w.build_parallel().run(|| {
                let mut state = init();
                Box::new(move |r| {
                    if self.quit.load(Relaxed) {
                        return WalkState::Quit;
                    }
                    match r {
                        Ok(d) => {
                            if let Some(t) = self.target(&d) {
                                visit(&mut state, t);
                            }
                        }
                        Err(e) => self.msgs.error(e),
                    }
                    if self.quit.load(Relaxed) {
                        WalkState::Quit
                    } else {
                        WalkState::Continue
                    }
                })
            });
        }
        Ok(())
    }

    /// ripgrep's choice of what to search: stdin, every file named on the command line, and
    /// regular files found by walking.
    fn target(&self, d: &DirEntry) -> Option<Target> {
        if d.is_stdin() {
            return Some(Target {
                path: PathBuf::from("<stdin>"),
                fs: None,
                explicit: true,
                regular: false,
            });
        }
        let is_dir = d.file_type().is_some_and(|t| t.is_dir());
        let explicit = d.depth() == 0 && !is_dir;
        if is_dir || (!explicit && !d.file_type().is_some_and(|t| t.is_file())) {
            return None;
        }
        let p = d.path();
        let shown = if self.strip_dot {
            p.strip_prefix("./").unwrap_or(p)
        } else {
            p
        };
        // The walker only yields regular files, and its file type costs no system call; a path
        // named on the command line may be a link to anything, so it alone is looked up.
        let regular = d.file_type().is_some_and(|t| t.is_file()) || (explicit && rewindable(p));
        Some(Target {
            path: shown.to_path_buf(),
            fs: Some(p.to_path_buf()),
            explicit,
            regular,
        })
    }

    /// Searches a file on disk or stdin, streaming (passthrough and the quiet path). A file
    /// searched in memory is mapped.
    fn search_raw<S: Sink<Error = std::io::Error>>(
        &self,
        searcher: &mut Searcher,
        t: &Target,
        sink: S,
    ) -> std::io::Result<()> {
        searcher.set_binary_detection(search::binary_detection(self.args, t.explicit));
        let m = &self.matcher;
        match &t.fs {
            None => searcher.search_reader(m, std::io::stdin().lock(), sink),
            Some(p) if self.in_memory => searcher.search_path(m, p, sink),
            Some(p) => searcher.search_reader(m, File::open(p)?, sink),
        }
    }

    fn print(&self, buf: &Buffer) {
        if !buf.as_slice().is_empty() {
            self.printed.store(true, Relaxed);
        }
        if let Err(e) = self.out.print(buf) {
            // A closed pipe (`sfor x | head`) ends the run quietly, as ripgrep.
            if e.kind() != std::io::ErrorKind::BrokenPipe {
                self.msgs.error(e);
            }
            self.quit.store(true, Relaxed);
        }
    }

    fn print_summary(&self) {
        let mut stats = self.stats.lock().expect("stats").clone();
        stats.elapsed_total = self.started.elapsed();
        let mut buf = self.out.buffer();
        let _ = writeln!(buf, "{}", output::json::summary(&stats));
        self.print(&buf);
    }

    fn quiet_file(&self, searcher: &mut Searcher, t: &Target) {
        let mut sink = CollectSink::new(&self.matcher, self.args.invert_match, true);
        match self.search_raw(searcher, t, &mut sink) {
            Err(e) => self.msgs.error(format_args!("{}: {e}", t.path.display())),
            Ok(()) => {
                if sink.items.iter().any(|i| matches!(i, Item::Match(_))) {
                    self.matched.store(true, Relaxed);
                    self.quit.store(true, Relaxed);
                }
            }
        }
    }

    /// `--json -q`: the file's statistics for the summary, as grep-printer 0.2.2's summary
    /// printer (summary.rs:648 and 726) counts them: every Match line and Match, up to `-m`, and
    /// every file searched. A walked file whose binary data stopped the search counts its
    /// Matches but has none (summary.rs:760).
    fn quiet_json_file(&self, searcher: &mut Searcher, t: &Target) {
        let mut sink = StatsSink {
            matcher: &self.matcher,
            invert: self.args.invert_match,
            max: MaxCount::summary(search::max_count(self.args)),
            stats: FileStats::default(),
            lines: 0,
            started: Instant::now(),
        };
        if let Err(e) = self.search_raw(searcher, t, &mut sink) {
            self.msgs.error(format_args!("{}: {e}", t.path.display()));
            return;
        }
        let quit = search::binary_detection(self.args, t.explicit)
            .quit_byte()
            .is_some();
        if sink.lines > 0 && !(quit && sink.stats.binary_offset.is_some()) {
            self.matched.store(true, Relaxed);
        }
        self.count_quiet_json(&sink.stats, sink.lines > 0);
    }

    /// `--json -q`: adds a searched file to the summary.
    fn count_quiet_json(&self, s: &FileStats, matched: bool) {
        if self.args.json && self.args.quiet {
            let mut stats = output::json::file_stats(s, 0);
            stats.searches_with_match = u64::from(matched);
            *self.stats.lock().expect("stats") += &stats;
        }
    }

    /// grep-printer's printer for the mode, with ripgrep's defaults (design note 9) and its
    /// `-m`, which grep-printer 0.2.2's printers count.
    fn printer(&self, heading: bool) -> Printer<'_> {
        let specs = ColorSpecs::new(&grep_printer::default_color_specs());
        let max = search::max_count(self.args);
        if let Some(mode) = self.summary {
            let kind = match mode {
                summary::Mode::Count => SummaryKind::Count,
                summary::Mode::CountMatches => SummaryKind::CountMatches,
                summary::Mode::FilesWithMatches => SummaryKind::PathWithMatch,
                summary::Mode::FilesWithoutMatch => SummaryKind::PathWithoutMatch,
            };
            return Printer::Summary(
                SummaryBuilder::new()
                    .color_specs(specs)
                    .kind(kind)
                    .path(self.with_filename)
                    .exclude_zero(true)
                    .separator_field(b":".to_vec())
                    .max_matches(max)
                    .build(self.spill_lines()),
            );
        }
        if self.args.json {
            return Printer::Json(
                JSONBuilder::new()
                    .max_matches(max)
                    .build(self.spill_lines()),
            );
        }
        Printer::Standard(
            StandardBuilder::new()
                .color_specs(specs)
                .heading(heading)
                .path(self.with_filename)
                .max_columns(self.args.max_columns)
                .max_matches(max)
                .separator_search(self.line_separator.clone())
                .build(self.spill_lines()),
        )
    }

    /// The writer of grep-printer's printers: one file's output, printed as it grows where
    /// `Ctx::spill` allows it (code review r3, Fable #3).
    fn spill_lines(&self) -> SpillLines<'_> {
        SpillLines {
            buf: self.out.buffer(),
            out: self.spill.then_some(&self.out),
            line_end: 0,
            scanned: 0,
        }
    }

    fn passthrough_file(&self, searcher: &mut Searcher, printer: &mut Printer, t: &Target) {
        let path = t.path.as_path();
        let (result, has_match, buf) = match printer {
            Printer::Standard(p) => {
                p.get_mut().clear();
                let mut sink = p.sink_with_path(&self.matcher, path);
                let r = self.search_raw(searcher, t, &mut sink);
                let m = sink.has_match();
                (r, m, &p.get_mut().buf)
            }
            Printer::Summary(p) => {
                p.get_mut().clear();
                let mut sink = p.sink_with_path(&self.matcher, path);
                let r = self.search_raw(searcher, t, &mut sink);
                let m = sink.has_match();
                (r, m, &p.get_mut().buf)
            }
            Printer::Json(p) => {
                p.get_mut().clear();
                let mut sink = p.sink_with_path(&self.matcher, path);
                let r = self.search_raw(searcher, t, &mut sink);
                let m = sink.has_match();
                if m {
                    *self.stats.lock().expect("stats") += &run_stats(sink.stats());
                }
                (r, m, &p.get_mut().buf)
            }
        };
        match result {
            // A closed pipe while the printer printed: the run ends quietly, as in `print`,
            // and the file does not count as matched (rg 14.1.1 exits 1).
            Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => {
                self.quit.store(true, Relaxed);
                return;
            }
            Err(e) => self.msgs.error(format_args!("{}: {e}", path.display())),
            Ok(()) => {}
        }
        if has_match {
            self.matched.store(true, Relaxed);
        }
        self.print(buf);
    }

    fn trail_file(&self, st: &mut TrailState, t: &Target, detector: &Detector, ro: &RenderOptions) {
        // Without structural flags, rows are printed as the searcher reports them: Plain files,
        // and files on disk whose Processor builds an Outline (code review r1, Fable #1).
        if !self.args.structural() && self.summary.is_none() && !self.args.quiet {
            match detector.detect_by_path(&t.path) {
                Some(None) => return self.plain_file(st, t, ro),
                Some(Some(def)) if t.regular => {
                    if let Some(p) = processors::by_name(&def.processor)
                        && builds_outline(p)
                        && self.outlined_file(st, t, ro, def, p)
                    {
                        return;
                    }
                }
                _ => {}
            }
        }
        let Ok(found) = self.describe(st, t, detector) else {
            return;
        };
        let kept = found
            .as_ref()
            .is_some_and(|(_, d)| d.file.matches().next().is_some());
        if self.args.quiet {
            // With --json, rg's summary printer reads past the first Match and drops the
            // Matches of a search that quit at binary data (code review r7, Codex #1);
            // `binary_quit` is false when -m stopped it first. Without --json it stops at the
            // first Match, before the binary data.
            if kept && !(self.args.json && st.binary_quit) {
                self.matched.store(true, Relaxed);
                // With --json the run goes on, for the summary (`quiet_json_file`).
                if !self.args.json {
                    self.quit.store(true, Relaxed);
                }
            }
            return;
        }
        let buf = &mut st.buf;
        buf.clear();
        if let Some(mode) = self.summary {
            let d = found.as_ref().map(|(_, d)| d).filter(|_| kept);
            // rg 14.1.1's summary printer prints nothing for a file whose search quit at binary
            // data (grep-printer-0.2.2/src/summary.rs:744-766); `binary_quit` is false when -m
            // stopped the search first (`describe`), and -l stops at the first Match, before
            // the binary data. Its count is zero, which for --files-without-match still counts
            // as a Match in the exit code (`SummarySink::has_match`).
            if st.binary_quit && mode != summary::Mode::FilesWithMatches {
                if mode == summary::Mode::FilesWithoutMatch {
                    self.matched.store(true, Relaxed);
                }
                return;
            }
            match summary::render(mode, &t.path, d, self.with_filename, buf) {
                Ok(true) => self.matched.store(true, Relaxed),
                Ok(false) => {}
                Err(e) => self.msgs.error(e),
            }
            self.print(buf);
            return;
        }
        let Some((src, d)) = found else { return };
        if !kept {
            // The summary counts the search's Matches, before the filters (spec §6.4).
            if ro.layout == Layout::Json {
                *self.stats.lock().expect("stats") += &output::json::file_stats(&d.file.stats, 0);
            }
            return;
        }
        self.matched.store(true, Relaxed);
        let written = match ro.layout {
            Layout::Tree => output::tree::render(&d, src.bytes(), ro, buf),
            Layout::Flat => output::flat::render(&d, src.bytes(), ro, self.lead(), buf),
            Layout::Json => {
                let jo = self.json_options(ro);
                let mut v = Vec::new();
                output::json::render_with_stats(&d, src.bytes(), &jo, &mut v).and_then(
                    |(_, stats)| {
                        *self.stats.lock().expect("stats") += &stats;
                        buf.write_all(&v)
                    },
                )
            }
        };
        if let Err(e) = written {
            self.msgs.error(e);
        }
        self.print(buf);
    }

    /// The after context `-m` lets run after the last Match allowed: none for -c,
    /// --count-matches and -q, which stop at the limit as grep-printer 0.2.2's summary printer
    /// (`MaxCount::summary`; code review r3, Codex #1).
    fn max_count_after(&self) -> usize {
        if self.summary.is_some() || self.args.quiet {
            0
        } else {
            self.args.after()
        }
    }

    /// What the flat layout writes before a file's first line: `line_separator`, once something
    /// was printed.
    fn lead(&self) -> Option<&[u8]> {
        self.line_separator
            .as_deref()
            .filter(|_| self.printed.load(Relaxed))
    }

    fn json_options(&self, ro: &RenderOptions) -> JsonOptions {
        JsonOptions {
            line_numbers: ro.line_numbers,
            width: ro.width,
            expand: ro.expand,
        }
    }

    /// A Plain file without structural flags, printed row by row as the searcher reports
    /// them, without a model of the file (code review r1, Fable #1). In memory, as ripgrep
    /// (explicit files, at most 10), through the Source, which says whether a BOM makes the
    /// search a reader one; else straight from the file or stdin.
    fn plain_file(&self, st: &mut TrailState, t: &Target, ro: &RenderOptions) {
        match &t.fs {
            Some(p) if self.in_memory => match Source::open(p, t.explicit) {
                Ok(src) => self.stream_file(st, t, ro, Streamed::Source(&src), None, None),
                Err(e) => self.msgs.error(format_args!("{}: {e}", t.path.display())),
            },
            _ => self.stream_file(st, t, ro, Streamed::Raw, None, None),
        }
    }

    /// A file whose Processor builds an Outline, printed as `plain_file` with the Trail of each
    /// row computed as it is printed. The Processor runs only for a file with a Match (spec
    /// §3.1): a file searched with a reader is searched first, up to its first Match, and read
    /// only when it has one; a file searched in memory is read, and its Source searched up to
    /// its first Match. Then the search runs again over the Source. Returns false, printing nothing, for the files the model's path handles: one
    /// with binary data (no Trails, step 7) or a BOM (whose reader search reads differently).
    fn outlined_file(
        &self,
        st: &mut TrailState,
        t: &Target,
        ro: &RenderOptions,
        def: &formats::definition::FormatDefinition,
        p: &dyn Processor,
    ) -> bool {
        let Some(path) = &t.fs else { return false };
        if !self.in_memory {
            let mut probe = CollectSink::new(&self.matcher, self.args.invert_match, true);
            st.stream
                .set_binary_detection(search::binary_detection(self.args, t.explicit));
            let searched = File::open(path)
                .and_then(|f| st.stream.search_reader(&self.matcher, f, &mut probe));
            match searched {
                Err(e) => {
                    self.msgs.error(format_args!("{}: {e}", t.path.display()));
                    return true;
                }
                Ok(()) if !probe.items.iter().any(|i| matches!(i, Item::Match(_))) => {
                    return true;
                }
                Ok(()) => {}
            }
        }
        let src = match Source::open(path, t.explicit) {
            Ok(mut s) => {
                s.path = t.path.clone();
                s
            }
            Err(e) => {
                self.msgs.error(format_args!("{}: {e}", t.path.display()));
                return true;
            }
        };
        if src.had_bom || memchr::memchr(0, src.bytes()).is_some() {
            return false;
        }
        let input = Input {
            bytes: src.bytes(),
            path: &src.path,
            format: &def.name,
            disk: None,
        };
        let outline_of = || p.outline_of(&input, &def.params);
        let outline = if self.in_memory {
            // Searched in memory as ripgrep: the search first, here too (code review r2,
            // Fable #2 and Codex #4).
            st.source
                .set_binary_detection(search::binary_detection(self.args, t.explicit));
            let invert = self.args.invert_match;
            match if_matched(
                &mut st.source,
                &self.matcher,
                invert,
                src.bytes(),
                outline_of,
            ) {
                Err(e) => {
                    self.msgs.error(format_args!("{}: {e}", t.path.display()));
                    return true;
                }
                Ok(None) => return true,
                Ok(Some(o)) => o,
            }
        } else {
            outline_of()
        };
        // A failing Processor is a warning and leaves the file without Trails.
        let outline = match outline {
            Some(Ok(o)) => Some(o),
            Some(Err(e)) => {
                self.msgs
                    .warn(format_args!("sfor: {}: {e:#}", src.path.display()));
                None
            }
            None => return false,
        };
        let how = if self.in_memory {
            Streamed::Source(&src)
        } else {
            Streamed::Again(&src)
        };
        self.stream_file(st, t, ro, how, outline.as_ref(), Some(&def.name));
        true
    }

    /// Searches one file with a `StreamSink` writing into the thread's buffer, and prints it.
    fn stream_file(
        &self,
        st: &mut TrailState,
        t: &Target,
        ro: &RenderOptions,
        how: Streamed,
        outline: Option<&Outline>,
        format: Option<&str>,
    ) {
        let jo = self.json_options(ro);
        st.buf.clear();
        let explicit = t.explicit;
        let (searcher, src) = match how {
            Streamed::Raw => (&mut st.stream, None),
            // A BOM: the bytes as read, decoded by the searcher as ripgrep's (`Source::as_read`).
            Streamed::Source(src) if src.had_bom => (&mut st.stream, Some(src)),
            Streamed::Source(src) => (&mut st.source, Some(src)),
            // No BOM: the Source's bytes are the file's, read again as the first search read them.
            Streamed::Again(src) => (&mut st.stream, Some(src)),
        };
        let file = StreamFile {
            path: &t.path,
            explicit,
            format,
            src: src.map(Source::bytes),
            outline,
            max_count: search::max_count(self.args),
            lead: self.lead(),
        };
        searcher.set_binary_detection(search::binary_detection(self.args, explicit));
        let mut w = Spill {
            buf: &mut st.buf,
            out: self.spill.then_some(&self.out),
            printed: &self.printed,
        };
        let mut sink =
            StreamSink::new(&mut w, &self.matcher, self.args.invert_match, ro, &jo, file);
        let m = &self.matcher;
        let r = match (how, &t.fs) {
            (Streamed::Raw, None) => searcher.search_reader(m, std::io::stdin().lock(), &mut sink),
            (Streamed::Raw, Some(p)) => match File::open(p) {
                Ok(f) => searcher.search_reader(m, f, &mut sink),
                Err(e) => Err(e),
            },
            (Streamed::Source(src), _) if src.had_bom => {
                searcher.search_reader(m, std::io::Cursor::new(src.as_read()), &mut sink)
            }
            (Streamed::Source(src), _) => searcher.search_slice(m, src.bytes(), &mut sink),
            (Streamed::Again(src), _) => {
                searcher.search_reader(m, std::io::Cursor::new(src.bytes()), &mut sink)
            }
        };
        let result = r.and_then(|()| Ok((sink.has_match, sink.close()?)));
        match result {
            // A closed pipe while `Spill` printed: the run ends quietly, as in `print`.
            Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => {
                self.quit.store(true, Relaxed);
                st.buf.clear();
            }
            Err(e) => {
                self.msgs.error(format_args!("{}: {e}", t.path.display()));
                st.buf.clear();
            }
            Ok((has_match, stats)) => {
                if let Some(stats) = stats {
                    *self.stats.lock().expect("stats") += &stats;
                }
                if has_match {
                    self.matched.store(true, Relaxed);
                }
            }
        }
        self.print(&st.buf);
    }

    /// The search first, and the Source only for a file with Matches (spec §3.1). Explicit
    /// files searched in memory, as ripgrep (`rg_in_memory`), are read first and searched in
    /// the Source. Every other file, and stdin, is searched with a reader, which stops at the
    /// binary data of a walked file and at `-m` before the end of stdin; then the Source is read
    /// (stdin and every path that is not a regular file: the bytes the search read, kept by a
    /// `Tee`). Err: an error, already reported.
    fn search_first(
        &self,
        st: &mut TrailState,
        t: &Target,
        detector: &Detector,
    ) -> Result<Option<(Source, FileMatches)>, ()> {
        let report = |e: &dyn std::fmt::Display| {
            self.msgs.error(format_args!("{}: {e}", t.path.display()));
        };
        let open = |p: &Path| match Source::open(p, t.explicit) {
            Ok(mut s) => {
                s.path = t.path.clone();
                Ok(s)
            }
            Err(e) => {
                report(&e);
                Err(())
            }
        };
        if let (Some(p), true) = (&t.fs, self.in_memory) {
            let src = open(p)?;
            // A BOM: a reader search that decodes it, as ripgrep's (`Source::as_read`).
            let in_memory = !src.had_bom;
            let searcher = if in_memory {
                &mut st.source
            } else {
                &mut st.stream
            };
            searcher.set_binary_detection(search::binary_detection(self.args, t.explicit));
            return match search::search_source_stats(
                searcher,
                &self.matcher,
                &src,
                in_memory,
                search::max_count(self.args),
            ) {
                Ok((fm, stats)) => {
                    st.binary_quit = self.quit_at_binary(t, &stats);
                    self.count_quiet_json(&stats, fm.is_some());
                    Ok(fm.map(|fm| (src, fm)))
                }
                Err(e) => {
                    report(&format_args!("{e:#}"));
                    Err(())
                }
            };
        }
        let started = Instant::now();
        let mut sink = CollectSink::new(&self.matcher, self.args.invert_match, false)
            .limit(search::max_count(self.args));
        st.stream
            .set_binary_detection(search::binary_detection(self.args, t.explicit));
        // What cannot be read twice (stdin, `/dev/stdin`, a FIFO) is kept as it is read (code
        // review r2, Codex #1).
        let mut kept = None;
        let r = match &t.fs {
            Some(p) if t.regular => {
                File::open(p).and_then(|f| st.stream.search_reader(&self.matcher, f, &mut sink))
            }
            fs => {
                let inner: Box<dyn Read> = match fs {
                    None => Box::new(std::io::stdin().lock()),
                    Some(p) => match std::fs::File::open(p) {
                        Ok(f) => Box::new(f),
                        Err(e) => {
                            report(&e);
                            return Err(());
                        }
                    },
                };
                let tee = kept.insert(Tee {
                    inner,
                    read: Vec::new(),
                });
                st.stream.search_reader(&self.matcher, tee, &mut sink)
            }
        };
        if let Err(e) = r {
            report(&e);
            return Err(());
        }
        let stopped = sink.stopped();
        let mut stats = sink.stats;
        stats.elapsed = started.elapsed();
        let found = sink.items.iter().any(|i| matches!(i, Item::Match(_)));
        st.binary_quit = self.quit_at_binary(t, &stats);
        self.count_quiet_json(&stats, found);
        if !found {
            return Ok(None);
        }
        let src = match (kept, &t.fs) {
            (Some(mut tee), _) => {
                if stopped
                    && stats.binary_offset.is_none()
                    && self.complete_input(t, &tee.read, detector)
                    && let Err(e) = std::io::copy(&mut tee, &mut std::io::sink())
                {
                    report(&e);
                    return Err(());
                }
                Source::from_bytes(t.path.clone(), tee.read, t.explicit)
            }
            (None, Some(p)) => open(p)?,
            (None, None) => unreachable!("stdin is always kept"),
        };
        let fm = FileMatches {
            path: t.path.clone(),
            explicit: t.explicit,
            items: sink.items,
            stats,
        };
        Ok(Some((src, fm)))
    }

    /// Whether an input the search stopped reading at `-m` (`read` so far) is read to its end
    /// before its Source is built: when it has an Outline and the output shows where its
    /// Segments end (`--expand`'s Sections, `--json`'s `end_line`), which the rest of the input
    /// decides (code review r3, Fable #1). Otherwise the answer comes at the limit, as rg's:
    /// also for an input with binary data, which gets no Processor (code review r4, Codex #1).
    fn complete_input(&self, t: &Target, read: &[u8], detector: &Detector) -> bool {
        if !(self.args.expand || self.args.json) {
            return false;
        }
        let head = read[..read.len().min(crate::source::READ_HEAD)].to_vec();
        let partial = Source::from_bytes(t.path.clone(), head, t.explicit);
        detector
            .detect(&partial.path, partial.head())
            .is_some_and(|d| self.processors.describes(d))
    }

    /// Steps 6 to 8 for one file: its search (or with --in-headings its names), the Source of a
    /// file with Matches, Trails and filters. Err: an error, already reported. Ok(None):
    /// searched, nothing found. Ok(Some): the search found Matches; the filters may have left
    /// none.
    fn describe(
        &self,
        st: &mut TrailState,
        t: &Target,
        detector: &Detector,
    ) -> Result<Option<(Source, Described)>, ()> {
        st.binary_quit = false;
        let found = if self.args.in_headings {
            self.names_of(st, t, detector)?
        } else {
            self.search_first(st, t, detector)?.map(|(src, fm)| {
                let def = detector.detect(&src.path, src.head());
                // A binary file's coordinates follow NUL conversion: no Processor (step 7). A
                // Format without a Processor, or whose Processor fails, is Plain (D33).
                let described = def
                    .filter(|_| fm.stats.binary_offset.is_none())
                    .and_then(|d| {
                        let p = self.processors.for_definition(d)?;
                        let disk = disk_of(t, &src);
                        let input = Input {
                            bytes: src.bytes(),
                            path: &src.path,
                            format: &d.name,
                            disk: disk.as_deref(),
                        };
                        Some((d, self.trails(&*p, &input, &d.params, &fm)?))
                    });
                let (format, trails) = match described {
                    Some((d, trails)) => (Some(d.name.clone()), trails),
                    // A binary file keeps its Format's name, as M1.
                    None if fm.stats.binary_offset.is_some() => {
                        (def.map(|d| d.name.clone()), empty_trails(&fm))
                    }
                    None => (None, empty_trails(&fm)),
                };
                let d = Described {
                    file: fm,
                    format,
                    trails,
                    sections: Vec::new(),
                };
                (src, d)
            })
        };
        let Some((src, mut d)) = found else {
            return Ok(None);
        };
        let searched_to = d.file.items.last().map_or(0, |i| item_line(i).line_number);
        if let Some(w) = &self.args.within {
            let wanted = filter::parse_within(w);
            let keep: Vec<Vec<bool>> = d
                .trails
                .iter()
                .map(|ts| ts.iter().map(|t| filter::within(t, &wanted)).collect())
                .collect();
            filter::retain(&mut d, &keep, self.args.before(), self.args.after());
            // -m counts the lines the filter kept (code review r1, Fable #3).
            if let Some(m) = self.args.max_count {
                filter::limit(&mut d, m, self.args.before(), self.max_count_after());
                if d.file.stats.binary_offset.is_some() && !self.detected_by(&d, t, searched_to) {
                    d.file.stats.binary_offset = None;
                    d.file.stats.binary_cut = None;
                    st.binary_quit = false;
                }
            }
        }
        if self.args.expand && !self.args.in_headings {
            d.sections = filter::sections(&d);
        }
        Ok(Some((src, d)))
    }

    /// --in-headings: the Source and its names, and the Matches in them. A Plain file has no
    /// names: searched, nothing found (code review r1, Fable #5), and not read when its path
    /// says so.
    fn names_of(
        &self,
        st: &mut TrailState,
        t: &Target,
        detector: &Detector,
    ) -> Result<Option<(Source, Described)>, ()> {
        // A Plain file has no names. rg searches it, which shows in the statistics of
        // `-q --json` and, at a walked file's binary data, in --files-without-match (code review
        // r7, Codex #2 and Fable #4): it is searched only for those, without a Source (code
        // review r8, Fable #2).
        let plain = matches!(detector.detect_by_path(&t.path), Some(None));
        let quits = search::binary_detection(self.args, t.explicit)
            .quit_byte()
            .is_some();
        if plain {
            if (self.args.json && self.args.quiet)
                || (quits && self.summary == Some(summary::Mode::FilesWithoutMatch))
            {
                self.search_unnamed(st, t)?;
            }
            return Ok(None);
        }
        let (mut src, binary) = match &t.fs {
            Some(p) if t.regular => {
                let src = match Source::open(p, t.explicit) {
                    Ok(mut s) => {
                        s.path = t.path.clone();
                        s
                    }
                    Err(e) => {
                        self.msgs.error(format_args!("{}: {e}", t.path.display()));
                        return Err(());
                    }
                };
                let binary = self.binary_probe(st, t, &src)?;
                (src, binary)
            }
            _ => self.read_probed(st, t)?,
        };
        // Without Matches, rg reads on to the binary data; `heading_matches` decides otherwise.
        st.binary_quit = binary
            .as_ref()
            .is_some_and(|b| b.quit && b.offset.is_some());
        if let Some(b) = binary.as_ref().filter(|b| b.quit && b.cut == 0) {
            // rg 14.1.1 reports no line of a walked file before its binary data.
            let stats = FileStats {
                bytes_searched: b.bytes_searched,
                ..FileStats::default()
            };
            self.count_quiet_json(&stats, false);
            return Ok(None);
        }
        // The bytes are the file's own until the binary data changes them.
        let mut replaced = false;
        if let Some(b) = &binary {
            // The names of the bytes rg searches: up to the buffer it quits at, or with NUL
            // bytes turned into line breaks by a reader search.
            let bytes = if b.quit {
                Some(src.bytes()[..b.cut].to_vec())
            } else if b.reader {
                Some(
                    src.bytes()
                        .iter()
                        .map(|&c| if c == 0 { b'\n' } else { c })
                        .collect(),
                )
            } else {
                None
            };
            if let Some(bytes) = bytes {
                src = Source::from_bytes(src.path.clone(), bytes, src.explicit);
                replaced = true;
            }
        }
        let def = detector.detect(&src.path, src.head());
        // A Format without a Processor is Plain: no names (D33).
        let Some((p, def)) = def.and_then(|d| Some((self.processors.for_definition(d)?, d))) else {
            self.count_unnamed(st, binary.as_ref(), &src);
            return Ok(None);
        };
        let disk = if replaced { None } else { disk_of(t, &src) };
        let input = Input {
            bytes: src.bytes(),
            path: &src.path,
            format: &def.name,
            disk: disk.as_deref(),
        };
        let names = match p.names(&input, &def.params) {
            Ok(n) => n,
            Err(e) => {
                // A failing Processor skips the file with one warning; an unavailable one
                // (a disabled helper) says nothing more (spec §4.4, Failures).
                if e.downcast_ref::<crate::helper::Unavailable>().is_none() {
                    self.msgs
                        .warn(format_args!("sfor: {}: {e:#}", src.path.display()));
                }
                return Err(());
            }
        };
        let mut found = self.heading_matches(&src, &names);
        // What rg detects: `-m` and its after context may stop its search first.
        let detected = binary.as_ref().and_then(|b| {
            let items = found.as_ref().map_or(&[][..], |f| &f.0.items[..]);
            match b.offset {
                Some(offset) => {
                    let stopped = found
                        .as_ref()
                        .is_some_and(|f| self.stops_before(&f.0, b.cut));
                    (!stopped).then_some((offset, b.cut))
                }
                // In memory: the first line rg reports with a NUL (`Core::detect_binary`).
                None => items.iter().find_map(|i| {
                    let l = item_line(i);
                    let at = memchr::memchr(0, l.bytes(src.bytes()))?;
                    Some((l.absolute_offset + at as u64, l.absolute_offset as usize))
                }),
            }
        });
        let quit = binary.as_ref().is_some_and(|b| b.quit);
        st.binary_quit = quit && detected.is_some();
        if let (Some((offset, cut)), Some((file, ..))) = (detected, found.as_mut()) {
            let s = &mut file.stats;
            s.binary_offset = Some(offset);
            // The items rg prints before it detects the binary data (design note 3).
            s.binary_cut = Some(
                file.items
                    .iter()
                    .take_while(|i| item_line(i).absolute_offset < cut as u64)
                    .count(),
            );
            if quit {
                s.bytes_searched = binary.as_ref().map_or(0, |b| b.bytes_searched);
            }
        }
        let stats = found.as_ref().map_or_else(
            || FileStats {
                bytes_searched: match &binary {
                    Some(b) if st.binary_quit => b.bytes_searched,
                    _ => src.bytes().len() as u64,
                },
                ..FileStats::default()
            },
            |f| f.0.stats.clone(),
        );
        self.count_quiet_json(&stats, found.is_some());
        Ok(found.map(|(file, trails, sections)| {
            let d = Described {
                file,
                format: Some(def.name.clone()),
                trails,
                // With --expand, the Section of each named Segment (code review r1, Fable #4).
                sections: if self.args.expand {
                    sections
                } else {
                    Vec::new()
                },
            };
            (src, d)
        }))
    }

    /// --in-headings: searches a Plain file, which has no names, as rg searches it, with a reader
    /// and no Source: for `-q --json`'s summary and for whether its search quits at binary data.
    fn search_unnamed(&self, st: &mut TrailState, t: &Target) -> Result<(), ()> {
        let report = |e: &dyn std::fmt::Display| {
            self.msgs.error(format_args!("{}: {e}", t.path.display()));
        };
        let reader: Box<dyn Read> = match &t.fs {
            None => Box::new(std::io::stdin().lock()),
            Some(p) => match File::open(p) {
                Ok(f) => Box::new(f),
                Err(e) => {
                    report(&e);
                    return Err(());
                }
            },
        };
        let detection = search::binary_detection(self.args, t.explicit);
        let quit = detection.quit_byte().is_some();
        st.stream.set_binary_detection(detection);
        let probe = match search::binary_probe(&mut st.stream, &self.matcher, reader) {
            Ok(p) => p,
            Err(e) => {
                report(&format_args!("{e:#}"));
                return Err(());
            }
        };
        st.binary_quit = quit && probe.binary_offset.is_some();
        let stats = FileStats {
            bytes_searched: probe.bytes_searched,
            ..FileStats::default()
        };
        self.count_quiet_json(&stats, false);
        Ok(())
    }

    /// --in-headings, `-q --json`: adds a Source without names (Plain) to the summary, searched
    /// as rg searches it: up to the binary data a walked file quits at, or whole.
    fn count_unnamed(&self, st: &TrailState, binary: Option<&BinaryProbe>, src: &Source) {
        let bytes_searched = match binary {
            Some(b) if st.binary_quit => b.bytes_searched,
            _ => src.bytes().len() as u64,
        };
        let stats = FileStats {
            bytes_searched,
            ..FileStats::default()
        };
        self.count_quiet_json(&stats, false);
    }

    /// The search behind `stats` stopped at binary data, as a walked file's does.
    fn quit_at_binary(&self, t: &Target, stats: &FileStats) -> bool {
        stats.binary_offset.is_some()
            && search::binary_detection(self.args, t.explicit)
                .quit_byte()
                .is_some()
    }

    /// --within with `-m`: rg's search, which `-m` and its after context stop, detects the
    /// binary data the unlimited search found (`searched_to`: the line it reported last). rg
    /// stops after the window of the `-m`th Match kept, so it detects the binary data when the
    /// window keeps a line reported after the detection, or, at a walked file's binary data
    /// (`quit`), when the window runs past the last line the search reported.
    fn detected_by(&self, d: &Described, t: &Target, searched_to: u64) -> bool {
        let Some(mth) = self
            .args
            .max_count
            .and_then(|m| usize::try_from(m).ok()?.checked_sub(1))
            .and_then(|i| d.file.matches().nth(i))
        else {
            return true;
        };
        let window_end = mth
            .line_number
            .saturating_add(self.max_count_after() as u64);
        let quit = search::binary_detection(self.args, t.explicit)
            .quit_byte()
            .is_some();
        d.file
            .stats
            .binary_cut
            .is_some_and(|c| c < d.file.items.len())
            || (quit && window_end > searched_to)
    }

    /// `file`'s search stops at `-m` and its after context, as rg's (`search::MaxCount`), before
    /// `cut`, where rg detects the binary data: the last line of the window ends there or
    /// earlier. `file` holds the lines of the window; a window that runs past them runs past
    /// the end of its Source, which for a Source cut at binary data is past `cut`.
    fn stops_before(&self, file: &FileMatches, cut: usize) -> bool {
        let Some(m) = self.args.max_count else {
            return false;
        };
        let Some(mth) = usize::try_from(m)
            .ok()
            .and_then(|m| m.checked_sub(1))
            .and_then(|i| file.matches().nth(i))
        else {
            return false;
        };
        let window_end = mth
            .line_number
            .saturating_add(self.max_count_after() as u64);
        file.items.last().is_some_and(|i| {
            let l = item_line(i);
            l.line_number >= window_end && l.absolute_offset as usize + l.len <= cut
        })
    }

    /// --in-headings: where ripgrep 14.1.1's search of `src`, a regular file, detects its binary
    /// data, found as `search_first` would search it, with its searcher and binary policy (an
    /// input that cannot be read twice: `read_probed`). A reader search detects it when it
    /// fills the buffer with the NUL, after the lines before that buffer, which
    /// `search::binary_probe` finds keeping no line (code review r7, Codex #4). A search in
    /// memory looks for a NUL in its first 64 KiB (grep-searcher 0.1.14's
    /// `DEFAULT_BUFFER_CAPACITY`, `SliceByLine::run`) and then in each line it reports (`offset`
    /// None; code review r7, Codex #3). None: no binary data, or `-a`.
    fn binary_probe(
        &self,
        st: &mut TrailState,
        t: &Target,
        src: &Source,
    ) -> Result<Option<BinaryProbe>, ()> {
        let detection = search::binary_detection(self.args, t.explicit);
        let quit = detection.quit_byte().is_some();
        if (!quit && detection.convert_byte().is_none()) || !src.bytes().contains(&0) {
            return Ok(None);
        }
        let in_memory = t.fs.is_some() && self.in_memory && !src.had_bom;
        if in_memory {
            let ahead = &src.bytes()[..src.bytes().len().min(64 * 1024)];
            return Ok(Some(BinaryProbe {
                offset: memchr::memchr(0, ahead).map(|i| i as u64),
                cut: 0,
                quit,
                reader: false,
                bytes_searched: 0,
            }));
        }
        // Stopped at the binary data also where rg converts it: the buffers before it are the
        // same (`search::binary_probe`).
        st.stream.set_binary_detection(BinaryDetection::quit(b'\0'));
        let probe = match search::binary_probe(
            &mut st.stream,
            &self.matcher,
            std::io::Cursor::new(src.as_read()),
        ) {
            Ok(p) => p,
            Err(e) => {
                self.msgs.error(format_args!("{}: {e:#}", t.path.display()));
                return Err(());
            }
        };
        Ok(BinaryProbe::read(probe, quit, src))
    }

    /// --in-headings on what cannot be read twice (stdin, `/dev/stdin`, a FIFO): its Source, and
    /// the binary data rg's search of it detects, found by `search::binary_probe` on the input
    /// itself while a `Tee` keeps what it reads. A pipe gives its bytes in reads as they come, and
    /// rg detects binary data in the read that brings it: a NUL that comes after the `-m`th Match
    /// in a later read is never seen (code review r8, Codex #1). Then the rest is read.
    fn read_probed(
        &self,
        st: &mut TrailState,
        t: &Target,
    ) -> Result<(Source, Option<BinaryProbe>), ()> {
        let report = |e: &dyn std::fmt::Display| {
            self.msgs.error(format_args!("{}: {e}", t.path.display()));
        };
        let inner: Box<dyn Read> = match &t.fs {
            None => Box::new(std::io::stdin().lock()),
            Some(p) => match File::open(p) {
                Ok(f) => Box::new(f),
                Err(e) => {
                    report(&e);
                    return Err(());
                }
            },
        };
        let mut tee = Tee {
            inner,
            read: Vec::new(),
        };
        let detection = search::binary_detection(self.args, t.explicit);
        let quit = detection.quit_byte().is_some();
        let mut probe = None;
        if quit || detection.convert_byte().is_some() {
            // Stopped at the binary data, as `binary_probe`; `tee` keeps the rest for the Source.
            st.stream.set_binary_detection(BinaryDetection::quit(b'\0'));
            match search::binary_probe(&mut st.stream, &self.matcher, &mut tee) {
                Ok(p) => probe = Some(p),
                Err(e) => {
                    report(&format_args!("{e:#}"));
                    return Err(());
                }
            }
        }
        if let Err(e) = std::io::copy(&mut tee, &mut std::io::sink()) {
            report(&e);
            return Err(());
        }
        let src = Source::from_bytes(t.path.clone(), tee.read, t.explicit);
        let binary = probe.and_then(|p| BinaryProbe::read(p, quit, &src));
        Ok((src, binary))
    }

    /// `trails[i][k]` from the Processor's one Trail per anchor. None: the Processor failed,
    /// and the file is Plain (D33), with a warning unless it is `helper::Unavailable`.
    fn trails(
        &self,
        p: &dyn Processor,
        input: &Input,
        params: &toml::Table,
        fm: &FileMatches,
    ) -> Option<Vec<Vec<Trail>>> {
        let anchors: Vec<Vec<usize>> = fm.matches().map(|m| m.anchors()).collect();
        let all: Vec<usize> = anchors.iter().flatten().copied().collect();
        match p.trails(input, params, &all) {
            Ok(flat) if flat.len() == all.len() => {
                let mut it = flat.into_iter();
                Some(
                    anchors
                        .iter()
                        .map(|a| it.by_ref().take(a.len()).collect())
                        .collect(),
                )
            }
            Ok(_) => {
                self.msgs.warn(format_args!(
                    "sfor: {}: the Processor returned the wrong number of Trails",
                    input.path.display()
                ));
                None
            }
            Err(e) => {
                if e.downcast_ref::<crate::helper::Unavailable>().is_none() {
                    self.msgs
                        .warn(format_args!("sfor: {}: {e:#}", input.path.display()));
                }
                None
            }
        }
    }

    /// Step 8: Matches of the pattern in Segment names, as Match lines of the Source, with
    /// `-m` and `-A`/`-B`/`-C` applied as the searcher would. None when nothing matched.
    fn heading_matches(&self, src: &Source, names: &[Name]) -> Option<HeadingFound> {
        let bytes = src.bytes();
        let mut hits: Vec<(usize, usize, Trail, Section)> = Vec::new();
        for n in names {
            // --expand prints the Section of the named Segment.
            let section = n.trail.last().map(|s| (s.line, s.end_line));
            // RegexMatcher's error type is NoError: the search cannot fail.
            let _ = self.matcher.find_iter(n.text.as_bytes(), |m| {
                for r in n.source_ranges(m.start()..m.end()) {
                    hits.push((r.start, r.end, n.trail_at(r.start), section));
                }
                true
            });
        }
        hits.sort_by_key(|h| (h.0, h.1));
        hits.dedup_by_key(|h| (h.0, h.1));
        let li = LineIndex::new(bytes);
        let mut lines: Vec<HeadingLine> = Vec::new();
        for (s, e, t, sec) in hits {
            let l = li.line_of(s);
            match lines.last_mut() {
                Some(last) if last.0 == l => {
                    last.1.push((s, e));
                    last.2.push(t);
                    last.3.push(sec);
                }
                _ => lines.push((l, vec![(s, e)], vec![t], vec![sec])),
            }
        }
        // -m as ripgrep 14.1.1 counts it (`search::MaxCount`): the first `m` lines, then the
        // Matches in the after context of the last, which ends there. With --within, -m
        // applies after the filter (`describe`).
        let (before, after) = (self.args.before() as u64, self.max_count_after() as u64);
        let mut window_end = u64::MAX;
        if let Some(m) = search::max_count(self.args) {
            let m = usize::try_from(m).unwrap_or(usize::MAX);
            if let Some(&(last, ..)) = m.checked_sub(1).and_then(|i| lines.get(i)) {
                window_end = last.saturating_add(after);
                let kept = m + lines[m..].iter().take_while(|l| l.0 <= window_end).count();
                lines.truncate(kept);
            }
        }
        if lines.is_empty() {
            return None;
        }
        let last_line = if bytes.is_empty() {
            0
        } else {
            li.line_of(bytes.len() - 1)
        };
        let range = |l: u64| {
            let s = li.line_start(l).unwrap_or(bytes.len());
            s..li.line_start(l + 1).unwrap_or(bytes.len())
        };
        // The lines to print: every Match line with its context, clamped to the file (saturating:
        // `-A` may be huge, code review r1 Codex #11). Groups that touch are merged, so each
        // line is visited once. Line number → index into `lines` for Matches, None for context.
        let mut spans: Vec<(u64, u64)> = Vec::new();
        for (l, ..) in &lines {
            let (s, e) = (
                l.saturating_sub(before).max(1),
                l.saturating_add(after)
                    .min(window_end)
                    .min(last_line)
                    .max(*l),
            );
            match spans.last_mut() {
                Some(last) if s <= last.1.saturating_add(1) => last.1 = last.1.max(e),
                _ => spans.push((s, e)),
            }
        }
        let mut next = lines.iter().enumerate().peekable();
        let order = spans.into_iter().flat_map(|(s, e)| s..=e).map(|l| {
            let m = next.next_if(|(_, (ml, ..))| *ml == l).map(|(i, _)| i);
            (l, m)
        });
        let mut items = Vec::new();
        let mut trails = Vec::with_capacity(lines.len());
        let mut sections = Vec::with_capacity(lines.len());
        let mut matches = 0;
        for (l, m) in order {
            let r = range(l);
            let line = Line {
                line_number: l,
                absolute_offset: r.start as u64,
                len: r.len(),
                owned: None,
                submatches: Vec::new(),
            };
            match m {
                Some(i) => {
                    let (_, subs, ts, secs) = &lines[i];
                    let submatches: Vec<Submatch> = subs
                        .iter()
                        .map(|&(s, e)| Submatch {
                            start: s - r.start,
                            end: e.min(r.end).max(s) - r.start,
                        })
                        .collect();
                    matches += submatches.len() as u64;
                    trails.push(ts.clone());
                    sections.push(secs.clone());
                    items.push(Item::Match(Line { submatches, ..line }));
                }
                None => items.push(Item::Context(line)),
            }
        }
        let stats = FileStats {
            bytes_searched: bytes.len() as u64,
            matched_lines: lines.len() as u64,
            matches,
            ..FileStats::default()
        };
        let file = FileMatches {
            path: src.path.clone(),
            explicit: src.explicit,
            items,
            stats,
        };
        Some((file, trails, sections))
    }
}

/// What `binary_probe` finds about a Source with binary data.
struct BinaryProbe {
    /// rg's `binary_offset` when its search detects the binary data at `cut`. None: a search in
    /// memory, which detects it only in a line it reports.
    offset: Option<u64>,
    /// Where the bytes rg searches before the buffer with the binary data end: their names are
    /// searched, also in lines its pattern does not find (code review r9, Codex #1).
    cut: usize,
    /// Walked: rg stops at `cut` (`BinaryDetection::quit`). Otherwise it converts.
    quit: bool,
    /// A reader search, whose conversion turns NUL bytes into line breaks.
    reader: bool,
    /// rg's `bytes_searched` when it stops at `cut`.
    bytes_searched: u64,
}

impl BinaryProbe {
    /// What a reader search of `src` (`search::binary_probe`) found. None: no binary data.
    fn read(probe: search::BinarySink, quit: bool, src: &Source) -> Option<Self> {
        let offset = probe.binary_offset?;
        // The end of the bytes searched before the buffer with the binary data.
        let cut = usize::try_from(probe.cut).unwrap_or(usize::MAX);
        Some(Self {
            offset: Some(offset),
            cut: cut.min(src.bytes().len()),
            quit,
            reader: true,
            bytes_searched: probe.bytes_searched,
        })
    }
}

/// The line of a Match or of a context line.
fn item_line(i: &Item) -> &Line {
    match i {
        Item::Match(l) | Item::Context(l) => l,
    }
}

/// A Section as 1-based inclusive lines, or None.
type Section = Option<(u64, u64)>;

/// What `heading_matches` finds: the Match lines, and per submatch its Trail and Section.
type HeadingFound = (FileMatches, Vec<Vec<Trail>>, Vec<Vec<Section>>);

/// A line with Matches in Segment names: its number, the byte ranges of the Matches in the
/// Source, and the Trail and the Section of the named Segment of each.
type HeadingLine = (u64, Vec<(usize, usize)>, Vec<Trail>, Vec<Section>);

/// Spec §3.1 for bytes searched in memory: the search, stopped at its first Match, and `then`
/// (the Processor) only when it found one. None: no Match.
fn if_matched<T>(
    searcher: &mut Searcher,
    matcher: &RegexMatcher,
    invert: bool,
    bytes: &[u8],
    then: impl FnOnce() -> T,
) -> std::io::Result<Option<T>> {
    let mut probe = CollectSink::new(matcher, invert, true).in_memory();
    searcher.search_slice(matcher, bytes, &mut probe)?;
    Ok(probe
        .items
        .iter()
        .any(|i| matches!(i, Item::Match(_)))
        .then(then))
}

/// One empty Trail per submatch (one per inverted Match): Plain files and binary files.
/// Whether the stateless built-in `p` builds Outlines (`Processor::outline_of`), asked on an
/// empty Source: `outlined_file` would otherwise search and read a file with Matches whose
/// Processor has none (keypath) only to hand it to `describe`, which does both again.
fn builds_outline(p: &dyn Processor) -> bool {
    let input = Input {
        bytes: b"",
        path: Path::new(""),
        format: "",
        disk: None,
    };
    p.outline_of(&input, &toml::Table::new()).is_some()
}

/// The canonical path of the file on disk whose bytes are exactly `src`'s: a regular file
/// outside /dev and /proc, searched without a BOM or UTF-16 transcoding, whose length on disk
/// is the Source's. The helper gets this resolved path, never the one typed: `stdin` typed in
/// /dev, `/dev/stdin`, `/dev/fd/0` or a symlink to them name the helper's own protocol pipe
/// in the helper's process (review r2, Fable #1; r3, Codex #2). Resolved here, a stream
/// redirected from a file becomes that file (safe: same bytes), and a pipe or a FIFO does not
/// resolve to a regular file. Anything else reaches the helper as a temporary copy (spec §4.4).
/// Called only for files with Matches (D8, review r4, Codex #2).
fn disk_of(t: &Target, src: &Source) -> Option<PathBuf> {
    let real = std::fs::canonicalize(t.fs.as_deref()?).ok()?;
    if real.starts_with("/dev") || real.starts_with("/proc") {
        return None;
    }
    let m = std::fs::metadata(&real).ok()?;
    (m.is_file() && !src.had_bom && m.len() == src.bytes().len() as u64).then_some(real)
}

fn empty_trails(fm: &FileMatches) -> Vec<Vec<Trail>> {
    fm.matches()
        .map(|m| vec![Vec::new(); m.submatches.len().max(1)])
        .collect()
}

/// A regular file, which can be opened again and read the same: not `/dev/stdin` or a FIFO.
fn rewindable(p: &Path) -> bool {
    std::fs::metadata(p).is_ok_and(|m| m.is_file())
}

/// What `Spill` holds before it prints: a file's output is printed in pieces of about this size.
const SPILL: usize = 64 * 1024;

/// A thread's output buffer that, with `out`, prints what it holds every `SPILL` bytes, so one
/// file's output is not held in memory until the file ends (code review r2, Fable #3). Only
/// where nothing can come between the pieces (`Ctx::spill`).
struct Spill<'a> {
    buf: &'a mut Buffer,
    out: Option<&'a BufferWriter>,
    printed: &'a AtomicBool,
}

impl Write for Spill<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let n = self.buf.write(bytes)?;
        if let Some(out) = self.out
            && self.buf.as_slice().len() >= SPILL
        {
            self.printed.store(true, Relaxed);
            out.print(self.buf)?;
            self.buf.clear();
        }
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl WriteColor for Spill<'_> {
    fn supports_color(&self) -> bool {
        self.buf.supports_color()
    }

    fn set_color(&mut self, spec: &termcolor::ColorSpec) -> std::io::Result<()> {
        self.buf.set_color(spec)
    }

    fn reset(&mut self) -> std::io::Result<()> {
        self.buf.reset()
    }
}

/// A file's output from grep-printer's printers that, with `out`, prints the complete lines
/// it held before a write once it holds `SPILL` bytes: the line being written stays until the
/// next spill or the end of the file. Only where nothing can come between the pieces
/// (`Ctx::spill`).
struct SpillLines<'a> {
    buf: Buffer,
    out: Option<&'a BufferWriter>,
    /// The end of the last complete line found in `buf[..scanned]`. The line breaks are looked
    /// for only once `buf` holds `SPILL` bytes: grep-printer's JSON printer writes a message in
    /// many pieces (code review r4, Fable #1).
    line_end: usize,
    /// How much of `buf` was looked through for a line break.
    scanned: usize,
}

impl SpillLines<'_> {
    /// For the next file.
    fn clear(&mut self) {
        self.buf.clear();
        self.line_end = 0;
        self.scanned = 0;
    }
}

impl Write for SpillLines<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let start = self.buf.as_slice().len();
        let n = self.buf.write(bytes)?;
        let Some(out) = self.out else {
            return Ok(n);
        };
        if self.buf.as_slice().len() < SPILL {
            return Ok(n);
        }
        // What was written before this piece: the line being written stays.
        if let Some(j) = memchr::memrchr(b'\n', &self.buf.as_slice()[self.scanned..start]) {
            self.line_end = self.scanned + j + 1;
        }
        self.scanned = start;
        let flushable = self.line_end;
        if flushable > 0 {
            let held = self.buf.as_slice();
            let mut head = out.buffer();
            head.write_all(&held[..flushable])?;
            let mut rest = out.buffer();
            rest.write_all(&held[flushable..])?;
            out.print(&head)?;
            self.buf = rest;
            self.line_end = 0;
            self.scanned -= flushable;
        }
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl WriteColor for SpillLines<'_> {
    fn supports_color(&self) -> bool {
        self.buf.supports_color()
    }

    fn set_color(&mut self, spec: &termcolor::ColorSpec) -> std::io::Result<()> {
        self.buf.set_color(spec)
    }

    fn reset(&mut self) -> std::io::Result<()> {
        self.buf.reset()
    }
}

/// The counting part of grep-printer 0.2.2's summary sink with statistics, for `--json -q`.
struct StatsSink<'m> {
    matcher: &'m RegexMatcher,
    invert: bool,
    max: MaxCount,
    stats: FileStats,
    /// Match lines, the summary printer's `match_count`.
    lines: u64,
    started: Instant,
}

impl Sink for StatsSink<'_> {
    type Error = std::io::Error;

    fn begin(&mut self, s: &Searcher) -> std::io::Result<bool> {
        self.max.begin(s);
        self.started = Instant::now();
        Ok(true)
    }

    fn matched(&mut self, _s: &Searcher, m: &SinkMatch<'_>) -> std::io::Result<bool> {
        self.max.matched();
        self.lines += 1;
        self.stats.matched_lines += m.lines().count() as u64;
        let subs = search::match_submatches(self.matcher, self.invert, m);
        self.stats.matches += subs.len() as u64;
        Ok(self.max.go_on())
    }

    fn context(&mut self, _s: &Searcher, c: &SinkContext<'_>) -> std::io::Result<bool> {
        self.max.context(c);
        Ok(self.max.go_on())
    }

    fn finish(&mut self, _s: &Searcher, f: &SinkFinish) -> std::io::Result<()> {
        self.stats.elapsed = self.started.elapsed();
        self.stats.binary_offset = f.binary_byte_offset();
        self.stats.bytes_searched = f.byte_count();
        Ok(())
    }
}

/// grep-printer's per-file stats as the `RunStats` the summary adds up.
fn run_stats(s: &Stats) -> RunStats {
    RunStats {
        elapsed_total: Default::default(),
        elapsed: s.elapsed(),
        searches: s.searches(),
        searches_with_match: s.searches_with_match(),
        bytes_searched: s.bytes_searched(),
        bytes_printed: s.bytes_printed(),
        matched_lines: s.matched_lines(),
        matches: s.matches(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_outline_processors_take_the_streaming_path() {
        assert!(builds_outline(processors::by_name("markdown").unwrap()));
        assert!(!builds_outline(processors::by_name("keypath").unwrap()));
    }

    fn searcher_for(argv: &[&str]) -> (Searcher, RegexMatcher, bool) {
        let mut v = vec!["sfor"];
        v.extend_from_slice(argv);
        let a = cli::parse(v.iter().map(Into::into).collect()).unwrap();
        let m = search::matcher(&a, &a.patterns().unwrap().patterns).unwrap();
        (search::searcher(&a, true, true), m, a.invert_match)
    }

    /// Code review r5, Fable #1: on a tty, ripgrep 14.1.1 numbers the lines unless stdin is
    /// all it searches, implicit or `-` (hiargs.rs:214 and 1128), so `cat x | rg foo` reads as
    /// a pipe.
    #[test]
    fn stdin_alone_has_no_line_numbers_on_a_tty() {
        let a = |argv: &[&str]| {
            let mut v = vec!["sfor", "--no-trail"];
            v.extend_from_slice(argv);
            cli::parse(v.iter().map(Into::into).collect()).unwrap()
        };
        let stdin = [PathBuf::from("-")];
        let two = [PathBuf::from("-"), PathBuf::from("a.md")];
        let file = [PathBuf::from("a.md")];
        let dev = [PathBuf::from("/dev/stdin")];
        let plain = a(&["x"]);
        assert!(!passthrough_line_number(&plain, false, true, &stdin));
        assert!(passthrough_line_number(&plain, false, true, &two));
        assert!(passthrough_line_number(&plain, false, true, &file));
        assert!(passthrough_line_number(&plain, false, true, &dev));
        assert!(!passthrough_line_number(&plain, false, false, &file));
        assert!(!passthrough_line_number(&plain, true, true, &file));
        // -n, --json and -N decide before the tty does.
        assert!(passthrough_line_number(
            &a(&["-n", "x"]),
            false,
            true,
            &stdin
        ));
        assert!(passthrough_line_number(
            &a(&["--json", "x"]),
            false,
            true,
            &stdin
        ));
        assert!(!passthrough_line_number(
            &a(&["-N", "x"]),
            false,
            true,
            &file
        ));
    }

    /// Code review r2, Fable #2 and Codex #4: an explicit file searched in memory runs its
    /// Processor only when the search found a Match (spec §3.1).
    #[test]
    fn the_processor_runs_only_after_a_match() {
        let calls = std::cell::Cell::new(0);
        let processor = || calls.set(calls.get() + 1);
        let (mut s, m, invert) = searcher_for(&["needle"]);
        let found = if_matched(&mut s, &m, invert, b"# A\nnothing here\n", processor).unwrap();
        assert!(found.is_none());
        assert_eq!(calls.get(), 0);
        let found = if_matched(&mut s, &m, invert, b"# A\na needle\n", processor).unwrap();
        assert!(found.is_some());
        assert_eq!(calls.get(), 1);
        // -v: a line without the pattern is a Match.
        let (mut s, m, invert) = searcher_for(&["-v", "needle"]);
        assert!(
            if_matched(&mut s, &m, invert, b"needle\n", processor)
                .unwrap()
                .is_none()
        );
        assert!(
            if_matched(&mut s, &m, invert, b"needle\nx\n", processor)
                .unwrap()
                .is_some()
        );
        assert_eq!(calls.get(), 2);
    }
}
