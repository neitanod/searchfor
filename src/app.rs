//! The pipeline (plan, Task 9.1): command line, actions, walk, search, Processor, filters and
//! printers, with ripgrep's exit codes (design notes 9 to 11).
use crate::cli::{self, Args, CliError, ColorWhen};
use crate::docs;
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
use crate::preprocess::{Preprocess, Through, Transformed};
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
use ignore::{DirEntry, WalkBuilder, WalkState};
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
    let code = match main(&args) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("s4: {e:#}");
            2
        }
    };
    // After a SIGINT or SIGTERM s4 dies by it, as ripgrep, never with a code of its own.
    crate::helper::host::cancel::GLOBAL.hold_if_cancelled();
    code
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
        if !self.silent && !stopped() {
            eprintln!("s4: {msg}");
        }
    }

    fn warn(&self, msg: impl std::fmt::Display) {
        if !self.silent && !stopped() {
            eprintln!("{msg}");
        }
    }
}

/// A SIGINT or SIGTERM reached s4, which is about to die by it: from then on it prints
/// nothing, neither on stdout nor on stderr, as rg prints nothing after Ctrl-C.
fn stopped() -> bool {
    crate::helper::host::cancel::stopped()
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
        if !silent && !stopped() {
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
        msgs.warn(format_args!("s4: the Grammar index is not available: {e}"));
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
            anyhow::anyhow!("there is no Grammar directory (no data directory, and SEARCHFOR_GRAMMARS_DIR is not set)")
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
            eprintln!("s4: {e:#}");
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
                eprintln!("s4: {e:#}");
                2
            }
        });
    }
    if args.type_list {
        // ripgrep 14.1.1 validates the file types, the globs and the `--pre-glob` globs before
        // it prints the list, and reads no pattern (code review r4, Fable #5).
        let cwd = std::env::current_dir()?;
        search::walker(args, &[], &cwd)?;
        Preprocess::new(args, &cwd)?;
        std::io::stdout()
            .lock()
            .write_all(search::RG_TYPES.as_bytes())?;
        return Ok(0);
    }

    // Patterns and paths (step 4).
    if args.pattern.is_none() && args.regexp.is_empty() && args.pattern_files.is_empty() {
        eprintln!("s4: a pattern is required: give one, or use -e or -f");
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
    // As ripgrep 14.1.1 (hiargs.rs), the file types, the globs and the `--pre-glob` globs are
    // validated in this order before a pattern is compiled (code review r3, Codex #4). The
    // walker is built once, here, and walks the paths (code review r4, Fable #1).
    let cwd = std::env::current_dir()?;
    let walker = search::walker(args, &paths, &cwd)?;
    let pre = Preprocess::new(args, &cwd)?;
    // Nothing can match: ripgrep 14.1.1 searches nothing and exits 1, in every mode and without
    // a JSON summary, once its file types and globs are valid (an unknown type is still an
    // error); the patterns are not compiled. We validate our Formats as well.
    if pats.patterns.is_empty() || args.max_count == Some(0) {
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
    // The Format Definitions: for the Trails, and with --docs (the default) for every mode,
    // which must tell a document by its name before the search (M4 plan, design note 3).
    // Without a Definition of the user's and no override: loaded once a file may be a document
    // by the built-in document Formats' names (`Ctx::detector`), so a search without one costs
    // what it costs with --no-docs (code review r7, Codex #3 and Fable #3); an installed
    // Grammar's Definition claims no name as a document's (Fable M4 code r8 #3). Else loaded
    // here: the Trails need them for every file, an unknown override is an error before the
    // search, and a user's Definition may claim any name as a document, and warns first.
    let user_formats = formats::user_formats_dir();
    let registry = std::sync::OnceLock::new();
    let detector = std::sync::OnceLock::new();
    let lazy = args.docs
        && matches!(mode, Mode::Passthrough | Mode::Quiet | Mode::QuietJson)
        && args.formats.is_empty()
        && !formats::any_user_definition(user_formats.as_deref());
    if (matches!(mode, Mode::Trail) || args.docs) && !lazy {
        let r = registry.get_or_init(|| load_registry(&msgs, user_formats.as_deref()));
        let _ = detector.set(Detector::new(r, &args.formats)?.with_docs(args.docs));
    }
    let ctx = Ctx {
        args,
        matcher,
        registry: &registry,
        detector: &detector,
        roots: Default::default(),
        follow: args.follow && !args.no_follow,
        links: Default::default(),
        linked: AtomicBool::new(false),
        msgs,
        pre,
        in_memory: search::rg_in_memory(&paths, implicit_stdin),
        paths,
        walker,
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
            let detector = ctx.detector();
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
                    unnumbered: search::printing_searcher(args, false, false, false),
                    source: search::searcher(args, false, true),
                    buf: ctx.out.buffer(),
                    binary_quit: false,
                    failed: None,
                },
                |st, t| ctx.trail_file(st, &t, detector, &ro),
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
    /// False also for a file read through `--pre`, `-z` or an Extractor: what is searched is
    /// not the file's bytes, and can be read only once, as a pipe (M4 plan, design note 3).
    regular: bool,
    /// How the file is read when not as it is.
    through: Option<Through>,
    /// A document's text and Outline, extracted in `Ctx::prepare` (M4 plan, design note 4).
    doc: Option<std::sync::Arc<docs::Extracted>>,
    /// The first bytes `Ctx::document` read of a file that is no document, which its first
    /// search reads instead of opening the file again (`Ctx::reader`).
    held: std::cell::Cell<Option<Held>>,
}

/// The first bytes of a file without an extension, read to tell whether it is a document,
/// and the file opened past them; None once they are the whole file. A file that is no
/// document is searched from them, so it is opened and read as with --no-docs (code review
/// r6, Fable #1).
struct Held {
    head: Vec<u8>,
    rest: Option<File>,
}

/// A reader of a file from the first bytes `Ctx::document` read of it and on through the file,
/// each of whose reads fills the buffer as the file's own read would: what is left of the head,
/// and the file after it in the same call. The searcher's first read then holds what rg
/// 14.1.1's holds, and binary data is detected in the same read (code review r7, Fable #1).
struct HeadThen {
    head: Vec<u8>,
    at: usize,
    rest: File,
}

impl Read for HeadThen {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = (self.head.len() - self.at).min(buf.len());
        buf[..n].copy_from_slice(&self.head[self.at..self.at + n]);
        self.at += n;
        if n == buf.len() {
            return Ok(n);
        }
        match self.rest.read(&mut buf[n..]) {
            Ok(m) => Ok(n + m),
            // What was copied is returned; the next read meets the error again.
            Err(_) if n > 0 => Ok(n),
            Err(e) => Err(e),
        }
    }
}

/// What a Trail-mode thread keeps between files: a searcher for files and stdin (BOM sniffing
/// on, a reader), one for Sources (already decoded), and the output buffer.
struct TrailState {
    stream: Searcher,
    /// `stream` without line numbers: a search that only finds out whether a file has a Match,
    /// or whose lines are numbered afterwards (`search::number_lines`).
    unnumbered: Searcher,
    source: Searcher,
    buf: Buffer,
    /// The last `describe`d file's search quit at binary data (`BinaryDetection::quit`).
    binary_quit: bool,
    /// The last `describe`d file's search failed, after what it found is printed.
    failed: Option<Failure>,
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

/// What `Ctx::open_input` gives: the output of `--pre` or of a decompressor, which is closed
/// after the search, or any other input.
enum Opened<'t> {
    Read(Box<dyn Read + 't>),
    Transformed(Transformed),
}

impl Opened<'_> {
    /// The transform's failure, past the search (`Transformed::close`).
    fn close(&mut self) -> std::io::Result<()> {
        match self {
            Opened::Read(_) => Ok(()),
            Opened::Transformed(t) => t.close(),
        }
    }

    /// What is read from now on is past the search (`Transformed::rest`).
    fn rest(&mut self) {
        if let Opened::Transformed(t) = self {
            t.rest();
        }
    }
}

impl Read for Opened<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Opened::Read(r) => r.read(buf),
            Opened::Transformed(t) => t.read(buf),
        }
    }
}

/// A search of `input` and then its close, whose errors count as ripgrep 14.1.1 counts them:
/// the search's first, and the close's only after a search without errors.
fn closing<T>(searched: std::io::Result<T>, input: &mut Opened) -> std::io::Result<T> {
    let closed = input.close();
    let v = searched?;
    closed?;
    Ok(v)
}

/// A search that failed after it found Matches, of an input rg reads as it searches: with one
/// thread rg prints what it found before the error (code review r1, Codex #3).
struct Failure {
    error: std::io::Error,
    /// The search's own read failed: rg's printers never reached the file's end, so a summary
    /// line and a JSON `end` are not printed. False: the close failed, after the search.
    in_search: bool,
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
    /// Walks `paths`, with the file types and the globs (`search::walker`).
    walker: WalkBuilder,
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
    /// `--pre` and `-z`.
    pre: Preprocess,
    /// The Format Definitions and their Detector, which with --docs tells documents by their
    /// name or magic bytes: loaded in `main`, or once needed (`Ctx::detector`).
    registry: &'a std::sync::OnceLock<Registry>,
    detector: &'a std::sync::OnceLock<Detector<'a>>,
    /// With --docs, each path to search and how its name resolves, and whether some root may
    /// have a name of a descriptor below it (`reaches`) or is not resolved: resolved once a
    /// file may be a document (`Ctx::roots`).
    roots: std::sync::OnceLock<(Vec<(PathBuf, Resolved)>, bool)>,
    /// `--follow`: the walker follows links below the roots.
    follow: bool,
    /// With --docs and `--follow`, each directory below a root the walker reached by following
    /// a link, by the bytes of its name, and how it resolves, recorded when the walker yields
    /// it, before any name below it (`Ctx::through_descriptors`).
    links: std::sync::RwLock<Links>,
    /// `links` is not empty.
    linked: AtomicBool,
}

impl<'a> Ctx<'a> {
    /// Walks the paths, sequentially where ripgrep 14.1.1 uses one thread (`--sort path`, `-j1`,
    /// one file), else in parallel with one state per thread (design note 11), and calls `visit`
    /// for every file to search.
    fn walk<S: Send>(
        &self,
        init: impl Fn() -> S + Sync,
        visit: impl Fn(&mut S, Target) + Sync,
    ) -> anyhow::Result<()> {
        let w = &self.walker;
        if self.sequential {
            let mut state = init();
            let mut above = Above::default();
            for r in w.build() {
                if self.quit.load(Relaxed) {
                    break;
                }
                match r {
                    Ok(d) => {
                        if let Some(t) = self.target(&d, &mut above) {
                            visit(&mut state, self.prepare(t));
                        }
                    }
                    Err(e) => self.msgs.error(e),
                }
            }
        } else {
            let (init, visit) = (&init, &visit);
            w.build_parallel().run(|| {
                let mut state = init();
                let mut above = Above::default();
                Box::new(move |r| {
                    if self.quit.load(Relaxed) {
                        return WalkState::Quit;
                    }
                    match r {
                        Ok(d) => {
                            if let Some(t) = self.target(&d, &mut above) {
                                visit(&mut state, self.prepare(t));
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
    fn target(&self, d: &DirEntry, above: &mut Above) -> Option<Target> {
        if d.is_stdin() {
            return Some(Target {
                path: PathBuf::from("<stdin>"),
                fs: None,
                explicit: true,
                regular: false,
                through: None,
                doc: None,
                held: Default::default(),
            });
        }
        // A root that is a link to a directory is a directory, as ripgrep 14.1.1's
        // `Haystack::is_dir` follows it; below the roots, a link is skipped either way, so
        // only a root pays the look-up (code review r6, Fable #2).
        let is_dir = d.file_type().is_some_and(|t| t.is_dir())
            || (d.depth() == 0 && d.path_is_symlink() && d.path().is_dir());
        let explicit = d.depth() == 0 && !is_dir;
        if is_dir && d.depth() > 0 && self.follow && self.args.docs && d.path_is_symlink() {
            let resolved = resolve(d.path());
            let mut links = self.links.write().unwrap_or_else(|e| e.into_inner());
            let key = d.path().as_os_str().as_encoded_bytes().to_vec();
            links.insert(key, (d.path().to_path_buf(), resolved));
            self.linked.store(true, Relaxed);
        }
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
        // Only a regular file can be a document: its first bytes are read before the search
        // and the file is opened again, which a FIFO, a descriptor or stdin by name would not
        // survive (code review of the M4 plan r1, Codex #3).
        let mut held = None;
        let through = self.pre.through(p).or_else(|| {
            regular
                .then(|| self.document(d, &mut held, above))
                .flatten()
        });
        Some(Target {
            path: shown.to_path_buf(),
            fs: Some(p.to_path_buf()),
            explicit,
            regular: regular && through.is_none(),
            through,
            doc: None,
            held: std::cell::Cell::new(held),
        })
    }

    /// The Detector, loaded on the first call when `main` did not load it: then there is no
    /// override and no Definition of the user's, whose directory is not read again, so the
    /// Registry is `main`'s: the built-in Definitions and the installed Grammars'.
    fn detector(&self) -> &'a Detector<'a> {
        self.detector.get_or_init(|| {
            let r = self
                .registry
                .get_or_init(|| load_registry(&self.msgs, None));
            Detector::new(r, &self.args.formats)
                .expect("without overrides")
                .with_docs(self.args.docs)
        })
    }

    /// With --docs, the Detector when `p` may be a document (`Detector::may_be_document`):
    /// before it is loaded, as the built-in document Formats tell by the name, which are all
    /// there are then (`main`).
    fn may_be_document(&self, p: &Path) -> Option<&'a Detector<'a>> {
        if !self.args.docs
            || (self.detector.get().is_none() && !formats::detector::builtin_may_be_document(p))
        {
            return None;
        }
        Some(self.detector()).filter(|det| det.may_be_document(p))
    }

    /// `Ctx::roots`, resolved on the first call.
    fn roots(&self) -> &(Vec<(PathBuf, Resolved)>, bool) {
        self.roots.get_or_init(|| {
            let roots: Vec<(PathBuf, Resolved)> = self
                .paths
                .iter()
                .filter(|p| p.as_path() != Path::new("-"))
                .map(|p| (p.clone(), resolve(p)))
                .collect();
            let reach = roots.iter().any(|(_, r)| match r {
                Resolved::At(at) => reaches(at),
                Resolved::Descriptors | Resolved::Unknown => true,
            });
            (roots, reach)
        })
    }

    /// With --docs, `Through::Docs` for a file whose Format is a document Format: by its name,
    /// or, for a name without an extension, by its first bytes (read only then, and kept in
    /// `held` when the file is no document).
    fn document(
        &self,
        d: &DirEntry,
        held: &mut Option<Held>,
        above: &mut Above,
    ) -> Option<Through> {
        let p = d.path();
        let det = self.may_be_document(p)?;
        // `/dev/stdin` redirected from a file is a regular file, which the reader would open
        // as its own stdin (code review r1, Codex #1), and so is a file under a directory
        // named by a descriptor (code review r2, Codex #2).
        if self.through_descriptors(d, above) {
            return None;
        }
        let def = match det.detect_by_path(p) {
            Some(d) => d?,
            None => {
                let mut head = Vec::with_capacity(crate::source::READ_HEAD);
                let mut f = File::open(p).ok()?;
                (&mut f)
                    .take(crate::source::READ_HEAD as u64)
                    .read_to_end(&mut head)
                    .ok()?;
                // A document Format's magic bytes, before every Definition's.
                let def = det
                    .document_magic(&head)
                    .then(|| det.detect(p, &head))
                    .flatten();
                if def.is_none_or(|def| def.processor != "docs") {
                    // Shorter than asked for: the file ended within it.
                    let rest = (head.len() == crate::source::READ_HEAD).then_some(f);
                    *held = Some(Held { head, rest });
                    return None;
                }
                def?
            }
        };
        if def.processor != "docs" {
            return None;
        }
        let extractor = def.params.get("extractor")?.as_str()?;
        docs::Kind::from_name(extractor)?;
        Some(Through::Docs(def.name.clone(), extractor.to_string()))
    }

    /// Whether the walked `d` names a file through this process's descriptors
    /// (`through_descriptors`), resolved once per root (`Ctx::roots`) and, with `--follow`, once
    /// per link the walker followed below it (`Ctx::links`): a file the walker found below
    /// them is the resolved name of the nearest and the names after it, none of them a link,
    /// so only those names are looked at, with no system call. A name that is a link itself is
    /// resolved whole (code review r6 and r7, Fable #1 and #2).
    fn through_descriptors(&self, d: &DirEntry, above: &mut Above) -> bool {
        let (roots, reach) = self.roots();
        if d.depth() > 0 && !self.follow && !reach {
            return false;
        }
        let p = d.path();
        if d.depth() > 0 && self.follow && d.path_is_symlink() {
            return through_descriptors(p);
        }
        // The nearest followed link above it, else its root: a walked name is under one of
        // them, and with one root, under it. The walker yields a directory's files one after
        // the other, mostly to one thread, which looks the link up once for them (`Above`).
        let linked = match d.depth() > 1 && self.linked.load(Relaxed) {
            true => {
                let name = p.as_os_str().as_encoded_bytes();
                let end = name
                    .iter()
                    .rposition(|&b| std::path::is_separator(b as char));
                let dir = &name[..end.unwrap_or(0)];
                if above.dir != dir {
                    let links = self.links.read().unwrap_or_else(|e| e.into_inner());
                    above.link = nearest_link(&links, dir, d.depth() - 1).cloned();
                    above.dir.clear();
                    above.dir.extend_from_slice(dir);
                }
                above.link.as_ref()
            }
            false => None,
        };
        let base = linked.or_else(|| match roots.as_slice() {
            [only] => Some(only),
            roots => p
                .ancestors()
                .nth(d.depth())
                .and_then(|r| roots.iter().find(|(q, _)| q == r)),
        });
        match base {
            Some((_, Resolved::Descriptors)) => true,
            Some((_, Resolved::Unknown)) if d.depth() == 0 => false,
            Some((from, Resolved::At(at))) => {
                if d.depth() == 0 || !reaches(at) {
                    return false;
                }
                let mut at = at.clone();
                let below = p.strip_prefix(from).unwrap_or(Path::new(""));
                below.iter().any(|part| {
                    at.push(part);
                    a_descriptor(&at)
                })
            }
            _ => through_descriptors(p),
        }
    }

    /// Extracts a document's text (`Through::Docs`). A file whose bytes are not of its
    /// document Format is searched as it is, silently; one the Extractor cannot read is too,
    /// with one warning, `s4: <path>: <format>: <reason>` (spec §5.8, M4 plan D10).
    fn prepare(&self, mut t: Target) -> Target {
        let Some(Through::Docs(name, extractor)) = &t.through else {
            return t;
        };
        let name = name.clone();
        let kind = docs::Kind::from_name(extractor).expect("checked by `document`");
        let fs = t.fs.clone().expect("only files are documents");
        let as_is = |mut t: Target| {
            t.through = None;
            t.regular = rewindable(&fs);
            t
        };
        // Its first bytes only, which tell its Format; the reader reads the rest (code review
        // r1, Fable #4). On an error the search opens it again and reports it as ripgrep would.
        if !kind.looks_like(&head_of(&fs)) {
            return as_is(t);
        }
        // In a child process with limits of its own (M4 plan, D30).
        match docs::isolated::extract(kind, &fs, docs::MAX_TEXT) {
            Ok(e) => {
                if e.cut {
                    self.msgs.warn(format_args!(
                        "s4: {}: {name}: the text is cut at {} MiB",
                        t.path.display(),
                        docs::MAX_TEXT >> 20
                    ));
                }
                t.doc = Some(std::sync::Arc::new(e));
                t
            }
            // A SIGINT or SIGTERM cancelled the run: its readers were killed and no new one
            // starts, and s4 is about to die by the signal, so nothing is printed, as
            // ripgrep prints nothing then (spec §4.4, §5.8; M4 plan r6, Fable #1).
            Err(_) if crate::helper::host::cancel::GLOBAL.cancelled() => as_is(t),
            Err(e) => {
                // A Ctrl-C that ended the reader before s4 saw it stops s4 here.
                crate::helper::host::cancel::GLOBAL.settle();
                self.msgs
                    .warn(format_args!("s4: {}: {name}: {e}", t.path.display()));
                as_is(t)
            }
        }
    }

    /// What is searched for `t`: stdin, a document's text, the output of `--pre` or of a
    /// decompressor, or the file.
    fn open_input<'t>(&'t self, t: &'t Target) -> std::io::Result<Opened<'t>> {
        let Some(p) = &t.fs else {
            return Ok(Opened::Read(Box::new(std::io::stdin().lock())));
        };
        if let Some(doc) = &t.doc {
            return Ok(Opened::Read(Box::new(std::io::Cursor::new(
                doc.text.as_slice(),
            ))));
        }
        match &t.through {
            // The path printed, as rg 14.1.1 gives the command and the decompressor (`./a.txt`
            // walked is `a.txt`): the same file (code review r1, Fable #1).
            Some(how @ (Through::Pre | Through::Zip)) => {
                self.pre.open(how, &t.path).map(Opened::Transformed)
            }
            _ => Ok(Opened::Read(self.reader(t, p)?)),
        }
    }

    /// A reader of the file at `p`: the first time, from the bytes `Ctx::document` read of it
    /// and on through the file it opened (`Held`), else the file opened by its name.
    fn reader<'t>(&self, t: &'t Target, p: &Path) -> std::io::Result<Box<dyn Read + 't>> {
        Ok(match t.held.take() {
            Some(Held { head, rest: None }) => Box::new(std::io::Cursor::new(head)),
            Some(Held {
                head,
                rest: Some(f),
            }) => Box::new(HeadThen {
                head,
                at: 0,
                rest: f,
            }),
            None => Box::new(File::open(p)?),
        })
    }

    /// The Source of the file at `p`: from the bytes `Ctx::document` read of it when they are
    /// the whole file and no search read them yet, else read by its name.
    fn source(&self, t: &Target, p: &Path) -> std::io::Result<Source> {
        match t.held.take() {
            Some(Held { head, rest: None }) => {
                Ok(Source::from_bytes(p.to_path_buf(), head, t.explicit))
            }
            _ => Source::open(p, t.explicit),
        }
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
            // The first bytes `Ctx::document` read, when they are the whole file, are what the
            // memory map would hold (code review r7, Fable #3).
            Some(p) if self.in_memory && t.through.is_none() => match t.held.take() {
                Some(Held { head, rest: None }) => searcher.search_slice(m, &head, sink),
                _ => searcher.search_path(m, p, sink),
            },
            _ => {
                let mut input = self.open_input(t)?;
                let searched = searcher.search_reader(m, &mut input, sink);
                closing(searched, &mut input)
            }
        }
    }

    fn print(&self, buf: &Buffer) {
        if stopped() {
            self.quit.store(true, Relaxed);
            return;
        }
        if !buf.as_slice().is_empty() {
            self.printed.store(true, Relaxed);
        }
        if let Err(e) = self.out.print(buf) {
            // A closed pipe (`s4 x | head`) ends the run quietly, as ripgrep.
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
                if m && r.is_ok() {
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
            // A failed search counts no Match. rg 14.1.1 with one thread has printed what its
            // printer wrote before the error; with more it drops the file's buffer (code
            // review r1, Codex #3).
            Err(e) => {
                self.msgs.error(format_args!("{}: {e}", path.display()));
                if self.sequential {
                    self.print(buf);
                }
                return;
            }
            Ok(()) => {}
        }
        if has_match {
            self.matched.store(true, Relaxed);
        }
        self.print(buf);
    }

    fn trail_file(&self, st: &mut TrailState, t: &Target, detector: &Detector, ro: &RenderOptions) {
        self.trail_file_printed(st, t, detector, ro);
        // A search that failed after its Matches: the error comes after them (`Failure`).
        if let Some(f) = st.failed.take() {
            self.msgs
                .error(format_args!("{}: {}", t.path.display(), f.error));
        }
    }

    /// `trail_file` but for the error of a search that failed after its Matches, which counts
    /// none of them: no Match for the exit code nor for the JSON summary.
    fn trail_file_printed(
        &self,
        st: &mut TrailState,
        t: &Target,
        detector: &Detector,
        ro: &RenderOptions,
    ) {
        // Without structural flags, rows are printed as the searcher reports them: Plain files,
        // and files on disk whose Processor builds an Outline (code review r1, Fable #1).
        if !self.args.structural()
            && self.summary.is_none()
            && !self.args.quiet
            && t.through.is_none()
        {
            match detector.detect_by_path(&t.path) {
                Some(None) if self.plain_file(st, t, ro, detector) => return,
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
        let failed = st.failed.as_ref().map(|f| f.in_search);
        let matched = || {
            if failed.is_none() {
                self.matched.store(true, Relaxed);
            }
        };
        if self.args.quiet {
            // With --json, rg's summary printer reads past the first Match and drops the
            // Matches of a search that quit at binary data (code review r7, Codex #1);
            // `binary_quit` is false when -m stopped it first. Without --json it stops at the
            // first Match, before the binary data.
            if kept && !(self.args.json && st.binary_quit) && failed.is_none() {
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
                    matched();
                }
                return;
            }
            // A summary line is printed at the end of the search, which a failed read never
            // reaches.
            if failed == Some(true) {
                return;
            }
            match summary::render(mode, &t.path, d, self.with_filename, buf) {
                Ok(true) => matched(),
                Ok(false) => {}
                Err(e) => self.msgs.error(e),
            }
            self.print(buf);
            return;
        }
        let Some((src, d)) = found else { return };
        if !kept {
            // The summary counts the search's Matches, before the filters (spec §6.4).
            if ro.layout == Layout::Json && failed.is_none() {
                *self.stats.lock().expect("stats") += &output::json::file_stats(&d.file.stats, 0);
            }
            return;
        }
        matched();
        let written = match ro.layout {
            Layout::Tree => output::tree::render(&d, src.bytes(), ro, buf),
            Layout::Flat => output::flat::render(&d, src.bytes(), ro, self.lead(), buf),
            Layout::Json => {
                let jo = self.json_options(ro);
                let mut v = Vec::new();
                output::json::render_with_stats(&d, src.bytes(), &jo, &mut v).and_then(
                    |(_, stats)| {
                        match failed {
                            None => *self.stats.lock().expect("stats") += &stats,
                            // rg's JSON printer writes `end` at the end of the search.
                            Some(true) => without_end(&mut v),
                            Some(false) => {}
                        }
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
    /// search a reader one; else straight from the file or stdin. Returns false, printing
    /// nothing, for a file with a Match whose first bytes are an object Format's magic: its
    /// name said Plain, but it may be an object file (spec §4.1, M4 plan D5).
    fn plain_file(
        &self,
        st: &mut TrailState,
        t: &Target,
        ro: &RenderOptions,
        detector: &Detector,
    ) -> bool {
        let object = |head: &[u8]| !self.args.invert_match && detector.object(head).is_some();
        match &t.fs {
            Some(p) if self.in_memory && t.through.is_none() => match self.source(t, p) {
                Ok(src) if object(src.head()) => return false,
                Ok(src) => self.stream_file(st, t, ro, Streamed::Source(&src), None, None),
                Err(e) => self.msgs.error(format_args!("{}: {e}", t.path.display())),
            },
            // A file without Matches costs what it costs ripgrep: no line is counted (grill 8385).
            Some(p) if t.regular && self.probe(st, t, p) != Some(true) => {}
            // Its first bytes are read only once it has a Match.
            Some(p) if t.regular && object(&head_of(p)) => return false,
            _ => self.stream_file(st, t, ro, Streamed::Raw, None, None),
        }
        true
    }

    /// Whether the file on disk at `path` has a Match, searched up to its first one without
    /// counting lines. None: an error, already reported.
    fn probe(&self, st: &mut TrailState, t: &Target, path: &Path) -> Option<bool> {
        let mut probe = CollectSink::new(&self.matcher, self.args.invert_match, true);
        st.unnumbered
            .set_binary_detection(search::binary_detection(self.args, t.explicit));
        let searched = self
            .reader(t, path)
            .and_then(|f| st.unnumbered.search_reader(&self.matcher, f, &mut probe));
        match searched {
            Err(e) => {
                self.msgs.error(format_args!("{}: {e}", t.path.display()));
                None
            }
            Ok(()) => Some(probe.items.iter().any(|i| matches!(i, Item::Match(_)))),
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
        if !self.in_memory && self.probe(st, t, path) != Some(true) {
            return true;
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
                    .warn(format_args!("s4: {}: {e:#}", src.path.display()));
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
            (Streamed::Raw, _) => match self.open_input(t) {
                Ok(mut f) => {
                    let searched = searcher.search_reader(m, &mut f, &mut sink);
                    closing(searched, &mut f)
                }
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
        let open = |p: &Path| match self.source(t, p) {
            Ok(mut s) => {
                s.path = t.path.clone();
                Ok(s)
            }
            Err(e) => {
                report(&e);
                Err(())
            }
        };
        if let (Some(p), true) = (&t.fs, self.in_memory && t.through.is_none()) {
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
        let collect = || {
            CollectSink::new(&self.matcher, self.args.invert_match, false)
                .limit(search::max_count(self.args))
        };
        let mut sink = collect();
        let binary = search::binary_detection(self.args, t.explicit);
        // Lines are counted only in a file with a Match, as ripgrep without -n (grill 8385).
        st.unnumbered.set_binary_detection(binary.clone());
        // What cannot be read twice (stdin, `/dev/stdin`, a FIFO) is kept as it is read (code
        // review r2, Codex #1).
        let mut kept = None;
        let r = match &t.fs {
            Some(p) if t.regular => self
                .reader(t, p)
                .and_then(|f| st.unnumbered.search_reader(&self.matcher, f, &mut sink)),
            _ => {
                let inner = match self.open_input(t) {
                    Ok(r) => r,
                    Err(e) => {
                        report(&e);
                        return Err(());
                    }
                };
                let tee = kept.insert(Tee {
                    inner,
                    read: Vec::new(),
                });
                st.unnumbered.search_reader(&self.matcher, tee, &mut sink)
            }
        };
        // What rg reads as it searches (stdin, a pipe, `--pre`, `-z`) may fail after Matches,
        // which rg with one thread has printed; a file is searched again, and fails whole.
        let partial = kept.is_some() && self.sequential;
        let mut failed = match r {
            Err(e) if !partial => {
                report(&e);
                return Err(());
            }
            Err(e) => Some(Failure {
                error: e,
                in_search: true,
            }),
            Ok(()) => None,
        };
        let stopped = sink.stopped();
        let mut stats = sink.stats;
        stats.elapsed = started.elapsed();
        let found = sink.items.iter().any(|i| matches!(i, Item::Match(_)));
        st.binary_quit = self.quit_at_binary(t, &stats);
        if failed.is_none()
            && let Some(tee) = &mut kept
        {
            // An input the search stopped reading at `-m` is read on only where the output needs
            // its end (`complete_input`), past what rg reads: a failure there is its close's.
            let rest = found
                && stopped
                && stats.binary_offset.is_none()
                && self.complete_input(t, &tee.read, detector);
            if rest {
                tee.inner.rest();
            }
            let read = match rest {
                true => std::io::copy(tee, &mut std::io::sink()).map(drop),
                false => Ok(()),
            };
            if let Err(e) = closing(read, &mut tee.inner) {
                if !partial {
                    report(&e);
                    return Err(());
                }
                failed = Some(Failure {
                    error: e,
                    in_search: false,
                });
            }
        }
        if failed.is_none() {
            self.count_quiet_json(&stats, found);
        }
        st.failed = failed;
        if !found {
            return Ok(None);
        }
        let src = match (kept, &t.fs) {
            (Some(tee), _) => Source::from_bytes(t.path.clone(), tee.read, t.explicit),
            (None, Some(p)) => open(p)?,
            (None, None) => unreachable!("stdin is always kept"),
        };
        let mut items = sink.items;
        if stats.binary_offset.is_none() {
            search::number_lines(&mut items, src.bytes());
        } else {
            // The searcher counts a NUL it converted as a line terminator: the bytes as the
            // first search read them, searched again counting lines.
            st.stream.set_binary_detection(binary);
            let mut again = collect();
            if let Err(e) = st.stream.search_reader(
                &self.matcher,
                std::io::Cursor::new(src.as_read()),
                &mut again,
            ) {
                report(&e);
                return Err(());
            }
            items = again.items;
        }
        let fm = FileMatches {
            path: t.path.clone(),
            explicit: t.explicit,
            items,
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
        st.failed = None;
        let found = if self.args.in_headings {
            self.names_of(st, t, detector)?
        } else {
            let Some((src, fm)) = self.search_first(st, t, detector)? else {
                return Ok(None);
            };
            if let Some(doc) = &t.doc {
                // A document: the Trails of its Extractor's Outline (M4 plan, design note 4).
                let Some(Through::Docs(name, _)) = &t.through else {
                    unreachable!("a document is read through its Extractor")
                };
                let d = Described {
                    trails: outline_trails(&doc.outline, &fm),
                    file: fm,
                    format: Some(name.clone()),
                    sections: Vec::new(),
                    offsets: false,
                };
                return self.filter(st, t, src, d);
            }
            // What a transform outputs is no object file nor document, whatever its bytes or
            // --format say: Plain (spec §5.9; code review of the M4 plan r1, Codex #11).
            let transformed = matches!(t.through, Some(Through::Pre | Through::Zip));
            if !transformed && let Some(d) = self.object(st, detector, &src, &fm)? {
                return self.filter(st, t, src, d);
            }
            Some({
                let def = detector.detect(&src.path, src.head()).filter(|d| {
                    !(transformed && matches!(d.processor.as_str(), "object" | "docs"))
                });
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
                    offsets: false,
                };
                (src, d)
            })
        };
        let Some((src, d)) = found else {
            return Ok(None);
        };
        self.filter(st, t, src, d)
    }

    /// `--within` and `--expand` over a described file.
    fn filter(
        &self,
        st: &mut TrailState,
        t: &Target,
        src: Source,
        mut d: Described,
    ) -> Result<Option<(Source, Described)>, ()> {
        let searched_to = d.file.items.last().map_or(0, |i| item_line(i).line_number);
        if let Some(w) = &self.args.within {
            let wanted = filter::parse_within(w);
            let keep: Vec<Vec<bool>> = d
                .trails
                .iter()
                .map(|ts| ts.iter().map(|t| filter::within(t, &wanted)).collect())
                .collect();
            // An object file's rows have no context: a row the filter drops is gone, and -m
            // counts the rows it keeps (D8).
            if d.offsets {
                filter::retain_rows(&mut d, &keep, self.args.max_count);
            } else {
                filter::retain(&mut d, &keep, self.args.before(), self.args.after());
            }
            // -m counts the lines the filter kept (code review r1, Fable #3).
            if let Some(m) = self.args.max_count.filter(|_| !d.offsets) {
                filter::limit(&mut d, m, self.args.before(), self.max_count_after());
                if d.file.stats.binary_offset.is_some() && !self.detected_by(&d, t, searched_to) {
                    d.file.stats.binary_offset = None;
                    d.file.stats.binary_cut = None;
                    st.binary_quit = false;
                }
            }
        }
        // An object file's Sections are byte ranges of a binary: --expand prints its rows (M4
        // plan, D8).
        if self.args.expand && !self.args.in_headings && !d.offsets {
            d.sections = filter::sections(&d);
        }
        Ok(Some((src, d)))
    }

    /// An object file with Matches (spec §5.7): its rows, the printable strings that hold the
    /// Matches of a search of the whole file, each with the Trail of each Match. None: not an
    /// object file goblin reads, or `-v`, whose lines are the non-matching bytes. The search
    /// that found the file's first Match is rg's; this one reads on past the binary data, so it
    /// costs only a file with a Match (8385).
    fn object(
        &self,
        st: &mut TrailState,
        detector: &Detector,
        src: &Source,
        fm: &FileMatches,
    ) -> Result<Option<Described>, ()> {
        if self.args.invert_match {
            return Ok(None);
        }
        let def = match detector.detect(&src.path, src.head()) {
            Some(d) if d.processor == "object" => d,
            Some(_) => return Ok(None),
            None => match detector.object(src.head()) {
                Some(d) => d,
                None => return Ok(None),
            },
        };
        let Some(map) = processors::object::Map::parse(src.bytes()) else {
            return Ok(None);
        };
        let started = Instant::now();
        st.source.set_binary_detection(BinaryDetection::none());
        let mut sink = CollectSink::new(&self.matcher, false, false).in_memory();
        if let Err(e) = st
            .source
            .search_slice(&self.matcher, src.bytes(), &mut sink)
        {
            self.msgs.error(format_args!("{}: {e}", fm.path.display()));
            return Err(());
        }
        let hits: Vec<std::ops::Range<usize>> = sink
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Match(l) => Some(l),
                Item::Context(_) => None,
            })
            .flat_map(|l| {
                let base = l.absolute_offset as usize;
                l.submatches
                    .iter()
                    .map(move |s| base + s.start..base + s.end)
            })
            .collect();
        let lines = LineIndex::new(src.bytes());
        let rows = processors::object::rows::rows(
            src.bytes(),
            &hits,
            search::max_count(self.args),
            &lines,
        );
        let trails = rows
            .iter()
            .map(|r| {
                r.submatches
                    .iter()
                    .map(|s| map.trail(r.absolute_offset as usize + s.start, &lines))
                    .collect()
            })
            .collect();
        let stats = FileStats {
            bytes_searched: src.bytes().len() as u64,
            matched_lines: rows.len() as u64,
            matches: rows.iter().map(|r| r.submatches.len() as u64).sum(),
            binary_offset: None,
            binary_cut: None,
            elapsed: started.elapsed(),
        };
        st.binary_quit = false;
        Ok(Some(Described {
            file: FileMatches {
                path: fm.path.clone(),
                explicit: fm.explicit,
                items: rows.into_iter().map(Item::Match).collect(),
                stats,
            },
            format: Some(def.name.clone()),
            trails,
            sections: Vec::new(),
            offsets: true,
        }))
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
                let src = match self.source(t, p) {
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
        // A document's names are its Outline's: its headings (M4 plan, D11).
        let (names, format) = if let (Some(doc), Some(Through::Docs(name, _))) =
            (&t.doc, &t.through)
        {
            (crate::trail::names_of(&doc.outline), name.clone())
        } else {
            let def = detector.detect(&src.path, src.head());
            // A Format without a Processor is Plain: no names (D33).
            let Some((p, def)) = def.and_then(|d| Some((self.processors.for_definition(d)?, d)))
            else {
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
                            .warn(format_args!("s4: {}: {e:#}", src.path.display()));
                    }
                    return Err(());
                }
            };
            (names, def.name.clone())
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
                format: Some(format),
                offsets: false,
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
        let mut reader = match self.open_input(t) {
            Ok(r) => r,
            Err(e) => {
                report(&e);
                return Err(());
            }
        };
        let detection = search::binary_detection(self.args, t.explicit);
        let quit = detection.quit_byte().is_some();
        st.stream.set_binary_detection(detection);
        let probed = search::binary_probe(&mut st.stream, &self.matcher, &mut reader);
        let closed = reader.close().map_err(anyhow::Error::from);
        let probe = match probed.and_then(|p| closed.map(|()| p)) {
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
        let in_memory = t.fs.is_some() && self.in_memory && t.through.is_none() && !src.had_bom;
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
        let inner = match self.open_input(t) {
            Ok(r) => r,
            Err(e) => {
                report(&e);
                return Err(());
            }
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
        let read = std::io::copy(&mut tee, &mut std::io::sink());
        if let Err(e) = closing(read, &mut tee.inner) {
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
                    "s4: {}: the Processor returned the wrong number of Trails",
                    input.path.display()
                ));
                None
            }
            Err(e) => {
                if e.downcast_ref::<crate::helper::Unavailable>().is_none() {
                    self.msgs
                        .warn(format_args!("s4: {}: {e:#}", input.path.display()));
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
                // One Match over contiguous pieces (`a&lt;b`, a replaced byte between letters)
                // is one hit: its Source ranges are joined where they touch, never across two
                // Matches (M3 review r3, Codex #1).
                let mut joined: Vec<std::ops::Range<usize>> = Vec::new();
                for r in n.source_ranges(m.start()..m.end()) {
                    match joined.last_mut() {
                        Some(last) if last.end == r.start && !r.is_empty() => last.end = r.end,
                        _ => joined.push(r),
                    }
                }
                for r in joined {
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
    // Bytes from `--pre`, `-z` or an Extractor are not the file's (spec §4.4, Paths).
    if t.through.is_some() {
        return None;
    }
    let real = std::fs::canonicalize(t.fs.as_deref()?).ok()?;
    if real.starts_with("/dev") || real.starts_with("/proc") {
        return None;
    }
    let m = std::fs::metadata(&real).ok()?;
    (m.is_file() && !src.had_bom && m.len() == src.bytes().len() as u64).then_some(real)
}

/// One file's JSON messages without the last, its `end`.
fn without_end(v: &mut Vec<u8>) {
    let body = v.strip_suffix(b"\n").unwrap_or(v);
    let last = body.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
    debug_assert!(v[last..].starts_with(br#"{"type":"end""#));
    v.truncate(last);
}

/// `trails[i][k]` of the Matches of `fm` from an Outline, as `OutlineProcessor` gives them.
fn outline_trails(o: &Outline, fm: &FileMatches) -> Vec<Vec<Trail>> {
    let mut c = crate::trail::TrailCursor::new(o);
    fm.matches()
        .map(|m| m.anchors().into_iter().map(|a| c.trail(a)).collect())
        .collect()
}

fn empty_trails(fm: &FileMatches) -> Vec<Vec<Trail>> {
    fm.matches()
        .map(|m| vec![Vec::new(); m.submatches.len().max(1)])
        .collect()
}

/// The first 16 bytes of the file at `p`, enough for every object magic and document
/// signature; empty on an error, which the search reports.
fn head_of(p: &Path) -> Vec<u8> {
    let mut head = Vec::with_capacity(16);
    let _ = File::open(p).and_then(|f| f.take(16).read_to_end(&mut head));
    head
}

/// Whether `p` names a file through this process's own descriptors, in any of its parts,
/// itself or by a link on the way: `/proc/<pid>/fd`, `/proc/self/fd`, `/proc/thread-self/fd`,
/// `/proc/<pid>/task/<tid>/fd`, `/dev/fd`, `/dev/stdin`, `/dev/stdout` or `/dev/stderr`. A
/// reader opens a document again by its name in another process, which would open its own
/// descriptors by such a name (spec §5.8, "Only a regular file is a document"; code review r2,
/// Codex #2); a regular file anywhere else, `/dev/shm` included, is read by name (Fable #1).
/// The name is resolved part by part, each part looked at before its link is followed, since
/// a descriptor's link leads out of /proc to the file it is; true past 40 links, which no open
/// survives either.
fn through_descriptors(p: &Path) -> bool {
    matches!(resolve(p), Resolved::Descriptors)
}

/// Whether a name of a descriptor (`a_descriptor`) can be below the link-free absolute `at`:
/// every one is under /dev or /proc.
fn reaches(at: &Path) -> bool {
    // By its bytes, as it is asked for every walked name (code review r7, Fable #2).
    let at = at.as_os_str().as_encoded_bytes();
    let under = |top: &[u8]| at.starts_with(top) && at.get(top.len()).is_none_or(|&b| b == b'/');
    at == b"/" || under(b"/dev") || under(b"/proc")
}

/// The followed links below the roots, by the bytes of their names (`Ctx::links`).
type Links = std::collections::HashMap<Vec<u8>, (PathBuf, Resolved)>;

/// The nearest of `dir`, the bytes of the name of a walked directory at `depth`, and the
/// names above it down to the root's child, that is in `links`: each is `dir` up to one of its
/// separators, as the walker joins a name to the one above it with one.
fn nearest_link<'l>(links: &'l Links, dir: &[u8], depth: usize) -> Option<&'l (PathBuf, Resolved)> {
    let mut name = dir;
    for up in 0..depth {
        if up > 0 {
            let end = name
                .iter()
                .rposition(|&b| std::path::is_separator(b as char))?;
            name = &name[..end];
        }
        if let Some(linked) = links.get(name) {
            return Some(linked);
        }
    }
    None
}

/// What a walking thread keeps of the last name `Ctx::through_descriptors` looked at below a
/// followed link: the bytes of the directory it is in, and the nearest followed link above it.
#[derive(Default)]
struct Above {
    dir: Vec<u8>,
    link: Option<(PathBuf, Resolved)>,
}

/// How a name resolves, part by part (`through_descriptors`).
#[derive(Clone)]
enum Resolved {
    /// Through this process's descriptors.
    Descriptors,
    /// Elsewhere: the absolute name without links it resolves to.
    At(PathBuf),
    /// Missing or unreadable: opening it fails too, and the search reports it.
    Unknown,
}

#[cfg(unix)]
fn resolve(p: &Path) -> Resolved {
    use std::ffi::OsString;
    // The parts of `q` after its root, last first, to be taken with `pop`.
    let parts = |q: &Path| -> Vec<OsString> {
        let mut v: Vec<OsString> = q
            .components()
            .filter(|c| !matches!(c, std::path::Component::RootDir))
            .map(|c| c.as_os_str().to_owned())
            .collect();
        v.reverse();
        v
    };
    let Ok(abs) = std::path::absolute(p) else {
        return Resolved::Unknown;
    };
    let mut todo = parts(&abs);
    let mut at = PathBuf::from("/");
    let mut links = 0;
    while let Some(part) = todo.pop() {
        if part == ".." {
            at.pop();
            continue;
        }
        if part == "." {
            continue;
        }
        at.push(&part);
        if a_descriptor(&at) {
            return Resolved::Descriptors;
        }
        match std::fs::read_link(&at) {
            Ok(to) => {
                links += 1;
                if links > 40 {
                    return Resolved::Descriptors;
                }
                at.pop();
                if to.is_absolute() {
                    at = PathBuf::from("/");
                }
                todo.extend(parts(&to));
            }
            // Not a link.
            Err(e) if e.kind() == std::io::ErrorKind::InvalidInput => {}
            // Missing or unreadable: opening it fails too, and the search reports it.
            Err(_) => return Resolved::Unknown,
        }
    }
    Resolved::At(at)
}

/// No Windows path names a descriptor of its own.
#[cfg(not(unix))]
fn resolve(_: &Path) -> Resolved {
    Resolved::Unknown
}

#[cfg(not(unix))]
fn a_descriptor(_: &Path) -> bool {
    false
}

/// Whether the link-free absolute `at` is, or is under, a name of this process's descriptors
/// (`through_descriptors`).
#[cfg(unix)]
fn a_descriptor(at: &Path) -> bool {
    let id = |s: &std::ffi::OsStr| {
        s == "self"
            || s == "thread-self"
            || s.to_str()
                .is_some_and(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
    };
    let c: Vec<&std::ffi::OsStr> = at.iter().skip(1).collect();
    match c.as_slice() {
        [dev, fd, ..] if *dev == "dev" && *fd == "fd" => true,
        [dev, s] if *dev == "dev" && (*s == "stdin" || *s == "stdout" || *s == "stderr") => true,
        [proc, pid, fd, ..] if *proc == "proc" && id(pid) && *fd == "fd" => true,
        [proc, pid, task, tid, fd, ..] => {
            *proc == "proc" && id(pid) && *task == "task" && id(tid) && *fd == "fd"
        }
        _ => false,
    }
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
            if stopped() {
                self.buf.clear();
                return Ok(n);
            }
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
        if flushable > 0 && stopped() {
            self.clear();
        } else if flushable > 0 {
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
        let mut v = vec!["s4"];
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
            let mut v = vec!["s4", "--no-trail"];
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

    /// Code review r4, Fable #1: the walker, with its file types and globs, is built once per
    /// run; validating them before the pattern (code review r3, Codex #4) built it twice.
    #[test]
    fn a_run_builds_its_walker_once() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("a.md");
        std::fs::write(&f, "# A\nnothing here\n").unwrap();
        for extra in [&["-q"][..], &["-q", "-j1"], &["-q", "--docs"]] {
            let mut v = vec!["s4"];
            v.extend_from_slice(extra);
            v.extend(["needle", f.to_str().unwrap()]);
            let a = cli::parse(v.iter().map(Into::into).collect()).unwrap();
            search::WALKERS.with(|n| n.set(0));
            assert_eq!(main(&a).unwrap(), 1);
            assert_eq!(search::WALKERS.with(|n| n.get()), 1, "{v:?}");
        }
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
