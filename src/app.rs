//! The pipeline (plan, Task 9.1): command line, actions, walk, search, Processor, filters and
//! printers, with ripgrep's exit codes (design notes 9 to 11).
use crate::cli::{self, Args, CliError, ColorWhen};
use crate::filter;
use crate::formats::{self, Registry, detector::Detector};
use crate::model::{Described, FileMatches, FileStats, Input, Item, Line, Name, Submatch, Trail};
use crate::output::{
    self, Layout, RenderOptions, json::JsonOptions, json::RunStats, plain::PlainSink, summary,
};
use crate::processors::{self, Processor};
use crate::search::{self, CollectSink, MaxCountBytes};
use crate::source::Source;
use crate::trail::LineIndex;
use grep_matcher::Matcher;
use grep_printer::{
    ColorSpecs, JSON, JSONBuilder, Standard, StandardBuilder, Stats, Summary, SummaryBuilder,
    SummaryKind,
};
use grep_regex::RegexMatcher;
use grep_searcher::{Searcher, Sink, SinkContext, SinkFinish, SinkMatch};
use ignore::{DirEntry, WalkState};
use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::time::Instant;
use termcolor::{Buffer, BufferWriter, ColorChoice};

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

fn load_registry(msgs: &Messages, user_dir: Option<&Path>) -> Registry {
    let (registry, warnings) = Registry::load(user_dir, &|p| processors::by_name(p).is_some());
    for w in warnings {
        msgs.warn(w);
    }
    registry
}

fn main(args: &Args) -> anyhow::Result<i32> {
    let msgs = Messages {
        silent: args.no_messages,
        errored: AtomicBool::new(false),
    };
    // Actions (step 3).
    if args.list_formats || args.export_formats.is_some() {
        let user_dir = formats::user_formats_dir();
        let registry = load_registry(&msgs, user_dir.as_deref());
        if args.list_formats {
            print!("{}", registry.list());
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
    let summary_mode = if args.count {
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
    let mode = if args.quiet && !args.structural() {
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
    // The separator BufferWriter prints between two non-empty file outputs, as ripgrep's.
    let separator = match mode {
        Mode::Passthrough if args.json || summary_mode.is_some() => None,
        Mode::Passthrough if heading => Some(Vec::new()),
        Mode::Passthrough if context => Some(b"--".to_vec()),
        Mode::Trail if summary_mode.is_some() || args.quiet => None,
        Mode::Trail if layout == Layout::Tree => Some(Vec::new()),
        Mode::Trail if layout == Layout::Flat && context => Some(b"--".to_vec()),
        _ => None,
    };
    let mut out = BufferWriter::stdout(color);
    out.separator(separator);
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
    };
    match mode {
        Mode::Quiet => ctx.walk(
            || search::streaming_searcher(args, false, ctx.in_memory),
            |s, t| ctx.quiet_file(s, &t),
        )?,
        Mode::Passthrough => {
            let line_number = if args.line_number {
                true
            } else if args.no_line_number {
                false
            } else if args.json {
                true
            } else {
                summary_mode.is_none() && tty
            };
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
                },
                |st, t| ctx.trail_file(st, &t, &detector, &ro),
            )?;
            if layout == Layout::Json && summary_mode.is_none() && !args.quiet {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// `-q` without structural flags: no Source, stop at the first Match (step 10).
    Quiet,
    /// grep-printer's own output (design note 9).
    Passthrough,
    /// Sources, Processors, filters and our printers.
    Trail,
}

/// One file to search: its printed path, its path on disk (None for stdin), and whether it
/// was named on the command line (design note 3).
struct Target {
    path: PathBuf,
    fs: Option<PathBuf>,
    explicit: bool,
}

/// What a Trail-mode thread keeps between files: a searcher for files and stdin (BOM sniffing
/// on, a reader), one for Sources (already decoded), and the output buffer.
struct TrailState {
    stream: Searcher,
    source: Searcher,
    buf: Buffer,
}

/// A reader that keeps a copy of what it reads: stdin, searched before its Source exists.
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

enum Printer {
    Standard(Standard<Buffer>),
    Summary(Summary<Buffer>),
    Json(JSON<Buffer>),
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
}

impl Ctx<'_> {
    /// Walks the paths, sequentially under `--sort path` or `-j1`, else in parallel with one
    /// state per thread (design note 11), and calls `visit` for every file to search.
    fn walk<S: Send>(
        &self,
        init: impl Fn() -> S + Sync,
        visit: impl Fn(&mut S, Target) + Sync,
    ) -> anyhow::Result<()> {
        let cwd = std::env::current_dir()?;
        let w = search::walker(self.args, &self.paths, &cwd)?;
        let sequential = self.args.sort.as_deref() == Some("path") || self.args.threads == Some(1);
        if sequential {
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
        Some(Target {
            path: shown.to_path_buf(),
            fs: Some(p.to_path_buf()),
            explicit,
        })
    }

    /// Searches a file on disk or stdin, streaming (passthrough and the quiet path).
    fn search_raw<S: Sink<Error = std::io::Error>>(
        &self,
        searcher: &mut Searcher,
        t: &Target,
        sink: S,
    ) -> std::io::Result<()> {
        searcher.set_binary_detection(search::binary_detection(self.args, t.explicit));
        match &t.fs {
            None => searcher.search_reader(&self.matcher, std::io::stdin().lock(), sink),
            Some(p) => searcher.search_path(&self.matcher, p, sink),
        }
    }

    fn print(&self, buf: &Buffer) {
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

    /// grep-printer's printer for the mode, with ripgrep's defaults (design note 9).
    fn printer(&self, heading: bool) -> Printer {
        let specs = ColorSpecs::new(&grep_printer::default_color_specs());
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
                    .build(self.out.buffer()),
            );
        }
        if self.args.json {
            return Printer::Json(JSONBuilder::new().build(self.out.buffer()));
        }
        Printer::Standard(
            StandardBuilder::new()
                .color_specs(specs)
                .heading(heading)
                .path(self.with_filename)
                .max_columns(self.args.max_columns)
                .build(self.out.buffer()),
        )
    }

    fn passthrough_file(&self, searcher: &mut Searcher, printer: &mut Printer, t: &Target) {
        let path = t.path.as_path();
        let (result, has_match, buf) = match printer {
            Printer::Standard(p) => {
                p.get_mut().clear();
                let mut sink = p.sink_with_path(&self.matcher, path);
                let r = self.search_raw(searcher, t, &mut sink);
                let m = sink.has_match();
                (r, m, p.get_mut())
            }
            Printer::Summary(p) => {
                p.get_mut().clear();
                let mut sink = p.sink_with_path(&self.matcher, path);
                let r = self.search_raw(searcher, t, &mut sink);
                let m = sink.has_match();
                (r, m, p.get_mut())
            }
            Printer::Json(p) => {
                p.get_mut().clear();
                let mut sink = MaxCountSink {
                    inner: p.sink_with_path(&self.matcher, path),
                    bytes: MaxCountBytes::default(),
                    byte_count: 0,
                };
                let r = self.search_raw(searcher, t, &mut sink);
                let m = sink.inner.has_match();
                // A reader search: stdin, a walked file, or one with a BOM (see Source::had_bom).
                let reader = !self.in_memory || t.fs.as_deref().is_none_or(starts_with_bom);
                let searched = sink.bytes.bytes_searched(sink.byte_count, reader);
                let mut stats = run_stats(sink.inner.stats());
                if m {
                    stats.bytes_searched = stats.bytes_searched - sink.byte_count + searched;
                    *self.stats.lock().expect("stats") += &stats;
                }
                let byte_count = sink.byte_count;
                drop(sink);
                if searched != byte_count {
                    patch_bytes_searched(p.get_mut(), byte_count, searched);
                }
                (r, m, p.get_mut())
            }
        };
        if let Err(e) = result {
            self.msgs.error(format_args!("{}: {e}", path.display()));
        }
        if has_match {
            self.matched.store(true, Relaxed);
        }
        self.print(buf);
    }

    fn trail_file(&self, st: &mut TrailState, t: &Target, detector: &Detector, ro: &RenderOptions) {
        if !self.args.structural()
            && self.summary.is_none()
            && !self.args.quiet
            && matches!(detector.detect_by_path(&t.path), Some(None))
        {
            return self.plain_file(st, t, ro);
        }
        let Ok(found) = self.describe(st, t, detector) else {
            return;
        };
        let kept = found
            .as_ref()
            .is_some_and(|(_, d)| d.file.matches().next().is_some());
        if self.args.quiet {
            if kept {
                self.matched.store(true, Relaxed);
                self.quit.store(true, Relaxed);
            }
            return;
        }
        let buf = &mut st.buf;
        buf.clear();
        if let Some(mode) = self.summary {
            let d = found.as_ref().map(|(_, d)| d).filter(|_| kept);
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
            Layout::Flat => output::flat::render(&d, src.bytes(), ro, buf),
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

    fn json_options(&self, ro: &RenderOptions) -> JsonOptions {
        JsonOptions {
            line_numbers: ro.line_numbers,
            width: ro.width,
            expand: ro.expand,
        }
    }

    /// A Plain file without structural flags, printed row by row as the searcher reports
    /// them, without a model of the file (code review r1, Fable #1).
    fn plain_file(&self, st: &mut TrailState, t: &Target, ro: &RenderOptions) {
        let jo = self.json_options(ro);
        let invert = self.args.invert_match;
        st.buf.clear();
        let explicit = t.explicit;
        let result = match &t.fs {
            // In memory as ripgrep (explicit files, at most 10): through the Source, which says
            // whether the BOM makes the search a reader one.
            Some(p) if self.in_memory => match Source::open(p, explicit) {
                Err(e) => Err(e),
                Ok(src) => {
                    let reader = src.had_bom;
                    let mut sink = PlainSink::new(
                        &mut st.buf,
                        &self.matcher,
                        invert,
                        ro,
                        &jo,
                        &t.path,
                        explicit,
                        reader,
                    );
                    st.source
                        .set_binary_detection(search::binary_detection(self.args, explicit));
                    let r = if reader {
                        st.source.search_reader(
                            &self.matcher,
                            std::io::Cursor::new(src.bytes()),
                            &mut sink,
                        )
                    } else {
                        st.source
                            .search_slice(&self.matcher, src.bytes(), &mut sink)
                    };
                    r.and_then(|()| Ok((sink.has_match, sink.close()?)))
                }
            },
            fs => {
                let mut sink = PlainSink::new(
                    &mut st.buf,
                    &self.matcher,
                    invert,
                    ro,
                    &jo,
                    &t.path,
                    explicit,
                    true,
                );
                st.stream
                    .set_binary_detection(search::binary_detection(self.args, explicit));
                let r = match fs {
                    None => {
                        st.stream
                            .search_reader(&self.matcher, std::io::stdin().lock(), &mut sink)
                    }
                    Some(p) => st.stream.search_path(&self.matcher, p, &mut sink),
                };
                r.and_then(|()| Ok((sink.has_match, sink.close()?)))
            }
        };
        match result {
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
    /// (stdin: the bytes the search read, kept by a `Tee`). Err: an error, already reported.
    fn search_first(
        &self,
        st: &mut TrailState,
        t: &Target,
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
            st.source
                .set_binary_detection(search::binary_detection(self.args, t.explicit));
            let in_memory = !src.had_bom;
            return match search::search_source(
                &mut st.source,
                &self.matcher,
                &src,
                in_memory,
                false,
            ) {
                Ok(fm) => Ok(fm.map(|fm| (src, fm))),
                Err(e) => {
                    report(&format_args!("{e:#}"));
                    Err(())
                }
            };
        }
        let started = Instant::now();
        let mut sink = CollectSink::new(&self.matcher, self.args.invert_match, false);
        st.stream
            .set_binary_detection(search::binary_detection(self.args, t.explicit));
        let mut stdin = None;
        let r = match &t.fs {
            Some(p) => st.stream.search_path(&self.matcher, p, &mut sink),
            None => {
                let tee = stdin.insert(Tee {
                    inner: std::io::stdin().lock(),
                    read: Vec::new(),
                });
                st.stream.search_reader(&self.matcher, tee, &mut sink)
            }
        };
        if let Err(e) = r {
            report(&e);
            return Err(());
        }
        if !sink.items.iter().any(|i| matches!(i, Item::Match(_))) {
            return Ok(None);
        }
        let src = match (&t.fs, stdin) {
            (Some(p), _) => open(p)?,
            (None, tee) => Source::from_bytes(
                t.path.clone(),
                tee.map(|t| t.read).unwrap_or_default(),
                true,
            ),
        };
        let mut stats = sink.stats;
        stats.elapsed = started.elapsed();
        let fm = FileMatches {
            path: t.path.clone(),
            explicit: t.explicit,
            items: sink.items,
            stats,
        };
        Ok(Some((src, fm)))
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
        let found = if self.args.in_headings {
            self.names_of(t, detector)?
        } else {
            self.search_first(st, t)?.map(|(src, fm)| {
                let def = detector.detect(&src.path, src.head());
                let trails = match def.and_then(|d| Some((processors::by_name(&d.processor)?, d))) {
                    // A binary file's coordinates follow NUL conversion: no Processor (step 7).
                    Some((p, d)) if fm.stats.binary_offset.is_none() => {
                        let input = Input {
                            bytes: src.bytes(),
                            path: &src.path,
                            format: &d.name,
                        };
                        self.trails(p, &input, &d.params, &fm)
                    }
                    _ => empty_trails(&fm),
                };
                let d = Described {
                    file: fm,
                    format: def.map(|d| d.name.clone()),
                    trails,
                    sections: Vec::new(),
                };
                (src, d)
            })
        };
        let Some((src, mut d)) = found else {
            return Ok(None);
        };
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
                filter::limit(&mut d, m, self.args.before(), self.args.after());
            }
        }
        if self.args.expand && !self.args.in_headings {
            d.sections = filter::sections(&d);
        }
        Ok(Some((src, d)))
    }

    /// --in-headings: the Source and its names, and the Matches in them. A Plain file is not
    /// reported.
    fn names_of(&self, t: &Target, detector: &Detector) -> Result<Option<(Source, Described)>, ()> {
        let src = match &t.fs {
            None => {
                let mut v = Vec::new();
                if let Err(e) = std::io::stdin().lock().read_to_end(&mut v) {
                    self.msgs.error(format_args!("<stdin>: {e}"));
                    return Err(());
                }
                Source::from_bytes(t.path.clone(), v, true)
            }
            Some(p) => match Source::open(p, t.explicit) {
                Ok(mut s) => {
                    s.path = t.path.clone();
                    s
                }
                Err(e) => {
                    self.msgs.error(format_args!("{}: {e}", t.path.display()));
                    return Err(());
                }
            },
        };
        let def = detector.detect(&src.path, src.head());
        let Some((p, def)) = def.and_then(|d| Some((processors::by_name(&d.processor)?, d))) else {
            return Err(());
        };
        let input = Input {
            bytes: src.bytes(),
            path: &src.path,
            format: &def.name,
        };
        let names = match p.names(&input, &def.params) {
            Ok(n) => n,
            Err(e) => {
                self.msgs
                    .warn(format_args!("sfor: {}: {e:#}", src.path.display()));
                return Err(());
            }
        };
        Ok(self
            .heading_matches(&src, &names)
            .map(|(file, trails, sections)| {
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

    /// `trails[i][k]` from the Processor's one Trail per anchor; a failing Processor is a
    /// warning and leaves the file without Trails.
    fn trails(
        &self,
        p: &dyn Processor,
        input: &Input,
        params: &toml::Table,
        fm: &FileMatches,
    ) -> Vec<Vec<Trail>> {
        let anchors: Vec<Vec<usize>> = fm.matches().map(|m| m.anchors()).collect();
        let all: Vec<usize> = anchors.iter().flatten().copied().collect();
        match p.trails(input, params, &all) {
            Ok(flat) if flat.len() == all.len() => {
                let mut it = flat.into_iter();
                anchors
                    .iter()
                    .map(|a| it.by_ref().take(a.len()).collect())
                    .collect()
            }
            Ok(_) => {
                self.msgs.warn(format_args!(
                    "sfor: {}: the Processor returned the wrong number of Trails",
                    input.path.display()
                ));
                empty_trails(fm)
            }
            Err(e) => {
                self.msgs
                    .warn(format_args!("sfor: {}: {e:#}", input.path.display()));
                empty_trails(fm)
            }
        }
    }

    /// Step 8: Matches of the pattern in Segment names, as Match lines of the Source, with
    /// `-m` and `-A`/`-B`/`-C` applied as the searcher would. None when nothing matched.
    fn heading_matches(
        &self,
        src: &Source,
        names: &[Name],
    ) -> Option<HeadingFound> {
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
        // With --within, -m applies after the filter (`describe`).
        if let Some(m) = self.args.max_count.filter(|_| self.args.within.is_none()) {
            lines.truncate(usize::try_from(m).unwrap_or(usize::MAX));
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
        let (before, after) = (self.args.before() as u64, self.args.after() as u64);
        let mut spans: Vec<(u64, u64)> = Vec::new();
        for (l, ..) in &lines {
            let (s, e) = (
                l.saturating_sub(before).max(1),
                l.saturating_add(after).min(last_line).max(*l),
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

/// A Section as 1-based inclusive lines, or None.
type Section = Option<(u64, u64)>;

/// What `heading_matches` finds: the Match lines, and per submatch its Trail and Section.
type HeadingFound = (FileMatches, Vec<Vec<Trail>>, Vec<Vec<Section>>);

/// A line with Matches in Segment names: its number, the byte ranges of the Matches in the
/// Source, and the Trail and the Section of the named Segment of each.
type HeadingLine = (u64, Vec<(usize, usize)>, Vec<Trail>, Vec<Section>);

/// One empty Trail per submatch (one per inverted Match): Plain files and binary files.
fn empty_trails(fm: &FileMatches) -> Vec<Vec<Trail>> {
    fm.matches()
        .map(|m| vec![Vec::new(); m.submatches.len().max(1)])
        .collect()
}

/// grep-printer 0.3.1's JSON sink with ripgrep 14.1.1's `bytes_searched` at `-m` beside it
/// (`MaxCountBytes`): everything is forwarded, and the searcher's byte count is kept.
struct MaxCountSink<S> {
    inner: S,
    bytes: MaxCountBytes,
    byte_count: u64,
}

impl<S: Sink<Error = std::io::Error>> Sink for MaxCountSink<S> {
    type Error = std::io::Error;

    fn begin(&mut self, s: &Searcher) -> std::io::Result<bool> {
        self.bytes = MaxCountBytes::for_searcher(s);
        self.inner.begin(s)
    }

    fn matched(&mut self, s: &Searcher, m: &SinkMatch<'_>) -> std::io::Result<bool> {
        self.bytes.matched(m);
        self.inner.matched(s, m)
    }

    fn context(&mut self, s: &Searcher, c: &SinkContext<'_>) -> std::io::Result<bool> {
        self.bytes.context(c);
        self.inner.context(s, c)
    }

    fn context_break(&mut self, s: &Searcher) -> std::io::Result<bool> {
        self.inner.context_break(s)
    }

    fn binary_data(&mut self, s: &Searcher, offset: u64) -> std::io::Result<bool> {
        self.inner.binary_data(s, offset)
    }

    fn finish(&mut self, s: &Searcher, f: &SinkFinish) -> std::io::Result<()> {
        self.byte_count = f.byte_count();
        self.inner.finish(s, f)
    }
}

/// The file starts with a byte-order mark that makes grep-searcher read it through a reader
/// even when memory maps are on (UTF-8, UTF-16LE, UTF-16BE).
fn starts_with_bom(p: &Path) -> bool {
    let mut head = [0u8; 3];
    let n = std::fs::File::open(p)
        .and_then(|mut f| f.read(&mut head))
        .unwrap_or(0);
    let head = &head[..n];
    head.starts_with(b"\xEF\xBB\xBF")
        || head.starts_with(b"\xFF\xFE")
        || head.starts_with(b"\xFE\xFF")
}

/// Rewrites `bytes_searched` in the file's `end` message, its last line.
fn patch_bytes_searched(buf: &mut Buffer, from: u64, to: u64) {
    let out = buf.as_slice();
    let body = out.strip_suffix(b"\n").unwrap_or(out);
    let start = body.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
    let old = format!("\"bytes_searched\":{from},");
    let Some(at) = out[start..]
        .windows(old.len())
        .position(|w| w == old.as_bytes())
        .map(|i| start + i)
    else {
        return;
    };
    let mut patched = out[..at].to_vec();
    patched.extend_from_slice(format!("\"bytes_searched\":{to},").as_bytes());
    patched.extend_from_slice(&out[at + old.len()..]);
    buf.clear();
    let _ = buf.write_all(&patched);
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
