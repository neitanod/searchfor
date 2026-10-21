//! searchfor-example-helper: the example Helper Processor (M2 plan, D11). It describes INI files
//! over protocol v1 (spec §4.4): a `[name]` line opens a `section` up to the next section line,
//! and a `key = value` or `key: value` line is a `key`. Lines starting with `;` or `#` are
//! comments. Environment variables, read once at start, make it misbehave on purpose for
//! s4's tests; each holds a substring of the request's path:
//!
//! - `SEARCHFOR_EXAMPLE_SLOW`: sleep 300 ms before answering.
//! - `SEARCHFOR_EXAMPLE_CRASH`: exit with code 3.
//! - `SEARCHFOR_EXAMPLE_HANG`: sleep until killed.
//! - `SEARCHFOR_EXAMPLE_GARBAGE`: write `not json` instead of an answer.
//! - `SEARCHFOR_EXAMPLE_ERROR`: answer an `error`.
//!
//! And at start: `SEARCHFOR_EXAMPLE_LOG` (a file to append `start <pid>` to), `SEARCHFOR_EXAMPLE_TRACE`
//! (a file to append `begin <pid> <path>` and `end <pid> <path>` to around each request, the
//! `end` before the answer), `SEARCHFOR_EXAMPLE_NOEXIT` (when set, keep running after the EOF of
//! stdin, until killed), `SEARCHFOR_EXAMPLE_EXIT_AT_START` (when set, exit with code 1 right
//! after the `start` line, before answering `hello`), `SEARCHFOR_EXAMPLE_STDERR`
//! (a line to write to stderr), `SEARCHFOR_EXAMPLE_FORK` (when set, start a copy of this binary
//! in `--test-hold-pipes` mode with this process's stdout and stderr, as a careless helper's
//! child would).
//!
//! M5b's test controls (`test_modes`, `Hold`): `--test-echo-args`, `--test-hold-pipes`,
//! `--test-pre-upper`, `--test-pre-fail`, `--test-console-driver` (Windows),
//! `SEARCHFOR_EXAMPLE_ACT_AS`, `SEARCHFOR_EXAMPLE_PIDS`, `SEARCHFOR_EXAMPLE_WAIT_READY` and
//! `SEARCHFOR_EXAMPLE_WAIT_RELEASE`.
//!
//! Two knobs coordinate requests through the trace, so a test of concurrency depends on what
//! s4 sends and not on the scheduler (M3 code review r2, Codex #5 and Fable #1); each needs
//! `SEARCHFOR_EXAMPLE_TRACE`, and waits at most `BARRIER_LIMIT`:
//!
//! - `SEARCHFOR_EXAMPLE_BARRIER` (`<n> <total>`): after its `begin`, every request waits until `n`
//!   requests are in flight, or all of the `total` that have not ended.
//! - `SEARCHFOR_EXAMPLE_OVERTAKE` (a substring of paths): the first request whose path holds it
//!   waits until a request that began after it has ended.
#[cfg(windows)]
mod console;

use anyhow::Context;
use searchfor::helper::protocol::{MatchPos, WireName};
use searchfor::helper::serve::{Handler, Info, serve};
use searchfor::model::Trail;
use searchfor::trail::{Outline, OutlineSegment, TrailCursor, names_of};
use std::io::Write;
use std::path::Path;
use std::time::Duration;

/// The test knobs: each, when set, a substring of the paths it applies to.
#[derive(Default)]
struct Knobs {
    slow: Option<String>,
    crash: Option<String>,
    hang: Option<String>,
    garbage: Option<String>,
    error: Option<String>,
}

impl Knobs {
    fn from_env() -> Self {
        let var = |k: &str| std::env::var(k).ok();
        Self {
            slow: var("SEARCHFOR_EXAMPLE_SLOW"),
            crash: var("SEARCHFOR_EXAMPLE_CRASH"),
            hang: var("SEARCHFOR_EXAMPLE_HANG"),
            garbage: var("SEARCHFOR_EXAMPLE_GARBAGE"),
            error: var("SEARCHFOR_EXAMPLE_ERROR"),
        }
    }

    /// Misbehaves as the knobs that match `path` say; an error is the request's answer.
    fn apply(&self, path: &Path) -> anyhow::Result<()> {
        // Knobs name paths with `/` (`/gen/`); Windows paths have `\` between components.
        let p = path.to_string_lossy().replace('\\', "/");
        let hit = |k: &Option<String>| k.as_deref().is_some_and(|s| p.contains(s));
        if hit(&self.slow) {
            std::thread::sleep(Duration::from_millis(300));
        }
        if hit(&self.crash) {
            std::process::exit(3);
        }
        if hit(&self.hang) {
            loop {
                std::thread::sleep(Duration::from_secs(3600));
            }
        }
        if hit(&self.garbage) {
            let mut out = std::io::stdout().lock();
            let _ = out.write_all(b"not json\n");
            let _ = out.flush();
            drop(out);
            // No answer follows for this request: s4 kills a helper that writes a line it
            // cannot attribute (D41), so this thread waits for that.
            loop {
                std::thread::sleep(Duration::from_secs(3600));
            }
        }
        if hit(&self.error) {
            anyhow::bail!("SEARCHFOR_EXAMPLE_ERROR matched {p}");
        }
        Ok(())
    }
}

/// The Outline of an INI file: sections and keys.
fn outline(src: &[u8]) -> Outline {
    let mut segs = Vec::new();
    // The open section: its Segment's index in `segs`.
    let mut open: Option<usize> = None;
    let mut start = 0;
    while start < src.len() {
        let end = memchr_newline(src, start);
        let next = (end + 1).min(src.len());
        let line = &src[start..end];
        let lead = line.iter().take_while(|b| b.is_ascii_whitespace()).count();
        let body = trim_end(&line[lead..]);
        let first = body.first().copied();
        if first == Some(b'[') && body.last() == Some(&b']') && body.len() >= 2 {
            if let Some(i) = open.take() {
                let s: &mut OutlineSegment = &mut segs[i];
                s.range.end = start;
            }
            let inner = &body[1..body.len() - 1];
            let skip = inner.iter().take_while(|b| b.is_ascii_whitespace()).count();
            let name = trim_end(&inner[skip..]);
            let at = start + lead + 1 + skip;
            segs.push(named("section", name, at, start..src.len(), start..end));
            open = Some(segs.len() - 1);
        } else if !matches!(first, None | Some(b';') | Some(b'#'))
            && let Some(sep) = body.iter().position(|&b| b == b'=' || b == b':')
        {
            let key = trim_end(&body[..sep]);
            if !key.is_empty() {
                let at = start + lead;
                segs.push(named("key", key, at, start..next, at..at + key.len()));
            }
        }
        start = next;
    }
    Outline::build(segs, src)
}

/// A Segment named by the source bytes `name`, which start at `at`.
fn named(
    kind: &str,
    name: &[u8],
    at: usize,
    range: std::ops::Range<usize>,
    label: std::ops::Range<usize>,
) -> OutlineSegment {
    let text = String::from_utf8_lossy(name).into_owned();
    let mut s = OutlineSegment::new(kind, text.clone(), range, label);
    // Lossy decoding changes the length of invalid UTF-8; the piece then maps all of it.
    s.name = Some(vec![(0..text.len(), at..at + name.len())]);
    s.name_anchor = at;
    s
}

fn memchr_newline(src: &[u8], from: usize) -> usize {
    src[from..]
        .iter()
        .position(|&b| b == b'\n')
        .map_or(src.len(), |i| from + i)
}

fn trim_end(b: &[u8]) -> &[u8] {
    let n = b
        .iter()
        .rev()
        .take_while(|b| b.is_ascii_whitespace())
        .count();
    &b[..b.len() - n]
}

struct Ini {
    knobs: Knobs,
    /// `SEARCHFOR_EXAMPLE_TRACE`.
    trace: Option<std::path::PathBuf>,
    /// `SEARCHFOR_EXAMPLE_BARRIER`: (n, total).
    barrier: Option<(usize, usize)>,
    /// `SEARCHFOR_EXAMPLE_OVERTAKE`.
    overtake: Option<String>,
    /// `SEARCHFOR_EXAMPLE_WAIT_READY` and `SEARCHFOR_EXAMPLE_WAIT_RELEASE`.
    hold: Hold,
}

/// The wait knobs (M5b plan, Task 2.3): at its first `trails` request the copy appends
/// `{"search_pid":…,"helper_pid":…}` to the `ready` file, in one write, watches s4 (the pid in
/// `PROCESS_PARENT_ENV`) so it dies with it, and every `trails` answer waits until the `release`
/// file exists, at most `RELEASE_LIMIT`.
struct Hold {
    ready: Option<std::path::PathBuf>,
    release: Option<std::path::PathBuf>,
    logged: std::sync::Once,
}

/// The longest a `trails` answer waits for the release file.
const RELEASE_LIMIT: Duration = Duration::from_secs(30);

impl Hold {
    fn from_env() -> Self {
        Self {
            ready: std::env::var_os("SEARCHFOR_EXAMPLE_WAIT_READY").map(Into::into),
            release: std::env::var_os("SEARCHFOR_EXAMPLE_WAIT_RELEASE").map(Into::into),
            logged: std::sync::Once::new(),
        }
    }

    fn before_trails(&self) {
        if let Some(ready) = &self.ready {
            self.logged.call_once(|| {
                let search = std::env::var(searchfor::platform::PROCESS_PARENT_ENV)
                    .ok()
                    .and_then(|p| p.parse::<u32>().ok());
                let line = format!(
                    "{{\"search_pid\":{},\"helper_pid\":{}}}\n",
                    search.map_or("null".to_string(), |p| p.to_string()),
                    std::process::id()
                );
                append(ready, &line);
                if let Some(pid) = search
                    && let Err(e) = searchfor::platform::orphan_dies(pid)
                {
                    eprintln!("searchfor-example-helper: cannot watch s4: {e}");
                }
            });
        }
        if let Some(release) = &self.release {
            let deadline = std::time::Instant::now() + RELEASE_LIMIT;
            while !release.exists() && std::time::Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
}

/// Appends `text` to `path` in one write, so the lines of concurrent copies never interleave.
fn append(path: &Path, text: &str) {
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = f.write_all(text.as_bytes());
    }
}

/// The longest a request waits at a barrier: far more than a loaded machine needs to send the
/// requests it waits for, and less than the 30 s timeout of the tests that use it.
const BARRIER_LIMIT: Duration = Duration::from_secs(20);

/// One line of the trace: (a `begin`, its pid and path).
type TraceLine = (bool, String);

/// The complete lines of the trace file, in order.
fn trace_lines(t: &Path) -> Vec<TraceLine> {
    let text = std::fs::read_to_string(t).unwrap_or_default();
    let complete = text.rfind('\n').map_or("", |k| &text[..k]);
    complete
        .lines()
        .filter_map(|l| {
            let (what, rest) = l.split_once(' ')?;
            Some((what == "begin", rest.to_string()))
        })
        .collect()
}

impl Ini {
    fn read(&self, path: &Path) -> anyhow::Result<Vec<u8>> {
        let src = std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
        self.knobs.apply(path)?;
        Ok(src)
    }

    /// Appends `<what> <pid> <path>` to the trace file, in one write.
    fn trace(&self, what: &str, path: &Path) {
        let Some(t) = &self.trace else { return };
        let line = format!("{what} {} {}\n", std::process::id(), path.display());
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(t)
        {
            let _ = f.write_all(line.as_bytes());
        }
    }

    /// Waits, after this request's `begin`, as `SEARCHFOR_EXAMPLE_BARRIER` and
    /// `SEARCHFOR_EXAMPLE_OVERTAKE` say, polling the trace; at most `BARRIER_LIMIT`.
    fn wait(&self, path: &Path) {
        let Some(t) = &self.trace else { return };
        if self.barrier.is_none() && self.overtake.is_none() {
            return;
        }
        let me = format!("{} {}", std::process::id(), path.display());
        let deadline = std::time::Instant::now() + BARRIER_LIMIT;
        while std::time::Instant::now() < deadline {
            let lines = trace_lines(t);
            let ends = lines.iter().filter(|(b, _)| !b).count();
            let in_flight = lines.len() - 2 * ends;
            let barrier = self
                .barrier
                .is_none_or(|(n, total)| in_flight >= n.min(total.saturating_sub(ends)));
            let overtaken = self.overtake.as_deref().is_none_or(|sub| {
                let Some(at) = lines.iter().position(|(b, l)| *b && *l == me) else {
                    return false;
                };
                let first = !lines[..at].iter().any(|(b, l)| *b && l.contains(sub));
                !first
                    || !me.contains(sub)
                    || lines[at + 1..]
                        .iter()
                        .any(|(b, later)| *b && lines.iter().any(|(e, l)| !*e && l == later))
            });
            if barrier && overtaken {
                return;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    /// `f` between a `begin` and an `end` line of the trace.
    fn traced<T>(&self, path: &Path, f: impl FnOnce() -> T) -> T {
        self.trace("begin", path);
        self.wait(path);
        let out = f();
        self.trace("end", path);
        out
    }
}

impl Handler for Ini {
    fn trails(&self, path: &Path, matches: &[MatchPos]) -> anyhow::Result<Vec<Trail>> {
        self.hold.before_trails();
        self.traced(path, || {
            let src = self.read(path)?;
            let o = outline(&src);
            let mut cursor = TrailCursor::new(&o);
            Ok(matches
                .iter()
                .map(|m| cursor.trail(m.byte as usize))
                .collect())
        })
    }

    fn names(&self, path: &Path) -> anyhow::Result<Vec<WireName>> {
        self.traced(path, || {
            let src = self.read(path)?;
            Ok(names_of(&outline(&src))
                .iter()
                .map(WireName::from)
                .collect())
        })
    }
}

fn start_up() {
    if let Ok(log) = std::env::var("SEARCHFOR_EXAMPLE_LOG") {
        // One write: `writeln!` may write in pieces, which the appends of concurrent copies
        // interleave (one `start` line lost in 20 runs of h02 under load).
        append(Path::new(&log), &format!("start {}\n", std::process::id()));
    }
    if std::env::var_os("SEARCHFOR_EXAMPLE_EXIT_AT_START").is_some() {
        std::process::exit(1);
    }
    if let Ok(line) = std::env::var("SEARCHFOR_EXAMPLE_STDERR") {
        eprintln!("{line}");
    }
    if std::env::var_os("SEARCHFOR_EXAMPLE_FORK").is_some() {
        // Left running on purpose: it holds stdout and stderr open after this process exits.
        fork_holder();
    }
}

/// Starts one descendant of this binary in `--test-hold-pipes` mode, with this process's
/// stdout and stderr and without `SEARCHFOR_EXAMPLE_FORK`, so it starts none of its own. A
/// copy acting as another program (`SEARCHFOR_EXAMPLE_ACT_AS`) passes that on: its descendant
/// acts the same way.
fn fork_holder() {
    if let Ok(me) = std::env::current_exe() {
        let _ = std::process::Command::new(me)
            .arg("--test-hold-pipes")
            .env_remove("SEARCHFOR_EXAMPLE_FORK")
            .stdin(std::process::Stdio::null())
            .spawn();
    }
}

/// `--test-hold-pipes` (M5b plan, Task 2.3): a careless helper's child, portable. It sleeps
/// `secs` with the stdout and stderr it inherited, answers nothing, and appends its pid to
/// `SEARCHFOR_EXAMPLE_PIDS` when set; with `SEARCHFOR_EXAMPLE_FORK` it first starts one
/// descendant in the same mode.
fn hold_pipes(secs: u64) -> ! {
    if let Some(pids) = std::env::var_os("SEARCHFOR_EXAMPLE_PIDS") {
        append(Path::new(&pids), &format!("{}\n", std::process::id()));
    }
    if std::env::var_os("SEARCHFOR_EXAMPLE_FORK").is_some() {
        fork_holder();
    }
    std::thread::sleep(Duration::from_secs(secs));
    std::process::exit(0);
}

/// The `--pre` fixtures (M5b plan, Task 2.3), without a shell: `upper` writes the file's bytes
/// with ASCII letters uppercased and exits 0; `fail` writes `oops` on stderr and exits 3.
fn pre(mode: &str, path: Option<std::ffi::OsString>) -> ! {
    let Some(path) = path else {
        eprintln!("searchfor-example-helper: --test-pre-{mode} needs a file");
        std::process::exit(2);
    };
    if mode == "fail" {
        eprintln!("oops");
        std::process::exit(3);
    }
    match std::fs::read(&path) {
        Ok(bytes) => {
            let mut out = std::io::stdout().lock();
            let _ = out.write_all(&bytes.to_ascii_uppercase());
            let _ = out.flush();
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!(
                "searchfor-example-helper: {}: {e}",
                Path::new(&path).display()
            );
            std::process::exit(2);
        }
    }
}

/// `--test-echo-args OUT ARG...` (M5b plan, Task 2.1): writes the `Debug` text of each argument
/// after `OUT`, one per line, to the file `OUT`, so a test can compare the arguments a real
/// process received with those it was started with. Everything after `OUT` is data, even when
/// it starts with `--`.
fn echo_args(mut args: impl Iterator<Item = std::ffi::OsString>) -> ! {
    let Some(out) = args.next() else {
        eprintln!("searchfor-example-helper: --test-echo-args needs a file");
        std::process::exit(2);
    };
    let text: String = args.map(|a| format!("{a:?}\n")).collect();
    if let Err(e) = std::fs::write(&out, text) {
        eprintln!("searchfor-example-helper: {e}");
        std::process::exit(1);
    }
    std::process::exit(0);
}

/// The test controls, before the normal protocol service. `SEARCHFOR_EXAMPLE_ACT_AS=hold-pipes`
/// wins over any argument, so a copy can stand in for a program s4 starts with fixed arguments
/// (a `--hostname-bin` program, a `gzip` on PATH): it holds its pipes for 300 s, longer than any
/// test waits. A copy named `s4-test-pre-upper` or `s4-test-pre-fail` is that `--pre` fixture
/// without a switch, since `--pre` takes a single executable.
fn test_modes() {
    if std::env::var("SEARCHFOR_EXAMPLE_ACT_AS").as_deref() == Ok("hold-pipes") {
        hold_pipes(300);
    }
    let mut args = std::env::args_os();
    let stem = args.next().and_then(|a| {
        Path::new(&a)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
    });
    match stem.as_deref() {
        Some("s4-test-pre-upper") => pre("upper", args.next()),
        Some("s4-test-pre-fail") => pre("fail", args.next()),
        _ => {}
    }
    let Some(mode) = args.next() else { return };
    match mode.to_str() {
        Some("--test-echo-args") => echo_args(args),
        Some("--test-hold-pipes") => hold_pipes(30),
        Some("--test-pre-upper") => pre("upper", args.next()),
        Some("--test-pre-fail") => pre("fail", args.next()),
        #[cfg(windows)]
        Some("--test-console-driver") => console::driver(args.collect()),
        _ => {}
    }
}

fn main() {
    test_modes();
    start_up();
    let knobs = Knobs::from_env();
    let trace = std::env::var_os("SEARCHFOR_EXAMPLE_TRACE").map(Into::into);
    let barrier = std::env::var("SEARCHFOR_EXAMPLE_BARRIER")
        .ok()
        .and_then(|v| {
            let (n, total) = v.split_once(' ')?;
            Some((n.parse().ok()?, total.parse().ok()?))
        });
    let overtake = std::env::var("SEARCHFOR_EXAMPLE_OVERTAKE").ok();
    let hold = Hold::from_env();
    let info = Info {
        name: "searchfor-example-helper",
        version: env!("CARGO_PKG_VERSION"),
    };
    if let Err(e) = serve(std::io::stdin().lock(), std::io::stdout(), info, |_| {
        Ok(Ini {
            knobs,
            trace,
            barrier,
            overtake,
            hold,
        })
    }) {
        eprintln!("searchfor-example-helper: {e:#}");
        std::process::exit(1);
    }
    if std::env::var_os("SEARCHFOR_EXAMPLE_NOEXIT").is_some() {
        loop {
            std::thread::sleep(Duration::from_secs(3600));
        }
    }
}
