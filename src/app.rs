//! The pipeline (plan, Task 9.1): command line, actions, walk, search, Processor, filters and
//! printers, with ripgrep's exit codes (design notes 9 to 11).
use crate::cli::{self, Args, CliError, ColorWhen};
use crate::filter;
use crate::formats::{self, Registry, detector::Detector};
use crate::model::{
    ContextLine, Described, FileMatches, FileStats, Input, Item, LineMatch, Name, Submatch, Trail,
};
use crate::output::{self, Layout, RenderOptions, json::JsonOptions, json::RunStats, summary};
use crate::processors::{self, Processor};
use crate::search::{self, CollectSink};
use crate::source::Source;
use crate::trail::LineIndex;
use grep_matcher::Matcher;
use grep_printer::{
    ColorSpecs, JSON, JSONBuilder, Standard, StandardBuilder, Stats, Summary, SummaryBuilder,
    SummaryKind,
};
use grep_regex::RegexMatcher;
use grep_searcher::{Searcher, Sink};
use ignore::{DirEntry, WalkState};
use std::collections::BTreeMap;
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
    if pats.patterns.is_empty() {
        return Ok(1);
    }
    let matcher = search::matcher(args, &pats.patterns)?;
    let (paths, implicit_stdin, implicit_cwd) = if !pats.paths.is_empty() {
        (pats.paths, false, false)
    } else if !pats.stdin_used && grep_cli::is_readable_stdin() {
        (vec![PathBuf::from("-")], true, false)
    } else {
        (vec![PathBuf::from(".")], false, true)
    };
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
                || (search::searcher(args, false, true), ctx.out.buffer()),
                |(s, b), t| ctx.trail_file(s, b, &t, &detector, &ro),
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
                let mut sink = p.sink_with_path(&self.matcher, path);
                let r = self.search_raw(searcher, t, &mut sink);
                let m = sink.has_match();
                if m {
                    *self.stats.lock().expect("stats") += &run_stats(sink.stats());
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

    fn trail_file(
        &self,
        searcher: &mut Searcher,
        buf: &mut Buffer,
        t: &Target,
        detector: &Detector,
        ro: &RenderOptions,
    ) {
        let Some((src, d)) = self.describe(searcher, t, detector) else {
            return;
        };
        let d = d.filter(|d| d.file.matches().next().is_some());
        if self.args.quiet {
            if d.is_some() {
                self.matched.store(true, Relaxed);
                self.quit.store(true, Relaxed);
            }
            return;
        }
        buf.clear();
        if let Some(mode) = self.summary {
            match summary::render(mode, &t.path, d.as_ref(), self.with_filename, buf) {
                Ok(true) => self.matched.store(true, Relaxed),
                Ok(false) => {}
                Err(e) => self.msgs.error(e),
            }
            self.print(buf);
            return;
        }
        let Some(d) = d else { return };
        self.matched.store(true, Relaxed);
        let written = match ro.layout {
            Layout::Tree => output::tree::render(&d, src.bytes(), ro, buf),
            Layout::Flat => output::flat::render(&d, src.bytes(), ro, buf),
            Layout::Json => {
                let jo = JsonOptions {
                    line_numbers: ro.line_numbers,
                    width: ro.width,
                    expand: ro.expand,
                };
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

    /// Steps 7 and 8: the Source, its search (or its names with --in-headings), Trails and
    /// filters. None: the file is not reported (an error, or Plain with --in-headings);
    /// `Some((src, None))`: searched, nothing found.
    fn describe(
        &self,
        searcher: &mut Searcher,
        t: &Target,
        detector: &Detector,
    ) -> Option<(Source, Option<Described>)> {
        let src = match &t.fs {
            None => {
                let mut v = Vec::new();
                if let Err(e) = std::io::stdin().lock().read_to_end(&mut v) {
                    self.msgs.error(format_args!("<stdin>: {e}"));
                    return None;
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
                    return None;
                }
            },
        };
        let def = detector.detect(&src.path, src.head());
        let processor: Option<&dyn Processor> = def.and_then(|d| processors::by_name(&d.processor));
        let input = Input {
            bytes: src.bytes(),
            path: &src.path,
            format: def.map_or("", |d| d.name.as_str()),
        };
        let params = def.map(|d| &d.params);
        let (file, trails) = if self.args.in_headings {
            let (Some(p), Some(params)) = (processor, params) else {
                return None;
            };
            let names = match p.names(&input, params) {
                Ok(n) => n,
                Err(e) => {
                    self.msgs
                        .warn(format_args!("sfor: {}: {e:#}", src.path.display()));
                    return None;
                }
            };
            match self.heading_matches(&src, &names) {
                Some(found) => found,
                None => return Some((src, None)),
            }
        } else {
            let in_memory = self.in_memory && !src.had_bom;
            searcher.set_binary_detection(search::binary_detection(self.args, src.explicit));
            let fm = match search::search_source(searcher, &self.matcher, &src, in_memory, false) {
                Ok(Some(fm)) => fm,
                Ok(None) => return Some((src, None)),
                Err(e) => {
                    self.msgs
                        .error(format_args!("{}: {e:#}", src.path.display()));
                    return None;
                }
            };
            let trails = match (processor, params) {
                // A binary file's coordinates follow NUL conversion: no Processor (step 7).
                (Some(p), Some(params)) if fm.stats.binary_offset.is_none() => {
                    self.trails(p, &input, params, &fm)
                }
                _ => empty_trails(&fm),
            };
            (fm, trails)
        };
        let mut d = Described {
            file,
            format: def.map(|d| d.name.clone()),
            trails,
            sections: Vec::new(),
        };
        if let Some(w) = &self.args.within {
            let wanted = filter::parse_within(w);
            let keep: Vec<Vec<bool>> = d
                .trails
                .iter()
                .map(|ts| ts.iter().map(|t| filter::within(t, &wanted)).collect())
                .collect();
            filter::retain(&mut d, &keep, self.args.before(), self.args.after());
        }
        if self.args.expand {
            d.sections = filter::sections(&d);
        }
        Some((src, Some(d)))
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
    ) -> Option<(FileMatches, Vec<Vec<Trail>>)> {
        let bytes = src.bytes();
        let mut hits: Vec<(usize, usize, Trail)> = Vec::new();
        for n in names {
            let parent: Trail = n.trail[..n.trail.len().saturating_sub(1)].to_vec();
            // RegexMatcher's error type is NoError: the search cannot fail.
            let _ = self.matcher.find_iter(n.text.as_bytes(), |m| {
                for r in n.source_ranges(m.start()..m.end()) {
                    hits.push((r.start, r.end, parent.clone()));
                }
                true
            });
        }
        hits.sort_by_key(|h| (h.0, h.1));
        hits.dedup_by_key(|h| (h.0, h.1));
        let li = LineIndex::new(bytes);
        let mut lines: Vec<HeadingLine> = Vec::new();
        for (s, e, t) in hits {
            let l = li.line_of(s);
            match lines.last_mut() {
                Some(last) if last.0 == l => {
                    last.1.push((s, e));
                    last.2.push(t);
                }
                _ => lines.push((l, vec![(s, e)], vec![t])),
            }
        }
        if let Some(m) = self.args.max_count {
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
        // Line number → index into `lines` for Matches, None for context.
        let mut order: BTreeMap<u64, Option<usize>> = BTreeMap::new();
        let (before, after) = (self.args.before() as u64, self.args.after() as u64);
        for (i, (l, _, _)) in lines.iter().enumerate() {
            order.insert(*l, Some(i));
        }
        for (l, _, _) in &lines {
            for c in l.saturating_sub(before).max(1)..=(l + after).min(last_line) {
                order.entry(c).or_insert(None);
            }
        }
        let mut items = Vec::with_capacity(order.len());
        let mut trails = Vec::with_capacity(lines.len());
        let mut matches = 0;
        for (l, m) in order {
            let r = range(l);
            let line = bytes[r.clone()].to_vec();
            match m {
                Some(i) => {
                    let (_, subs, ts) = &lines[i];
                    let submatches: Vec<Submatch> = subs
                        .iter()
                        .map(|&(s, e)| Submatch {
                            start: s - r.start,
                            end: e.min(r.end).max(s) - r.start,
                        })
                        .collect();
                    matches += submatches.len() as u64;
                    trails.push(ts.clone());
                    items.push(Item::Match(LineMatch {
                        line_number: l,
                        absolute_offset: r.start as u64,
                        bytes: line,
                        submatches,
                    }));
                }
                None => items.push(Item::Context(ContextLine {
                    line_number: l,
                    absolute_offset: r.start as u64,
                    bytes: line,
                    submatches: Vec::new(),
                })),
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
        Some((file, trails))
    }
}

/// A line with Matches in Segment names: its number, the byte ranges of the Matches in the
/// Source, and the Trail of each.
type HeadingLine = (u64, Vec<(usize, usize)>, Vec<Trail>);

/// One empty Trail per submatch (one per inverted Match): Plain files and binary files.
fn empty_trails(fm: &FileMatches) -> Vec<Vec<Trail>> {
    fm.matches()
        .map(|m| vec![Vec::new(); m.submatches.len().max(1)])
        .collect()
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
