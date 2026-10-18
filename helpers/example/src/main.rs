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
//! stdin, until killed), `SEARCHFOR_EXAMPLE_STDERR`
//! (a line to write to stderr), `SEARCHFOR_EXAMPLE_FORK` (when set, start `sleep 30` with this
//! process's stdout and stderr, as a careless helper's child would).
//!
//! Two knobs coordinate requests through the trace, so a test of concurrency depends on what
//! s4 sends and not on the scheduler (M3 code review r2, Codex #5 and Fable #1); each needs
//! `SEARCHFOR_EXAMPLE_TRACE`, and waits at most `BARRIER_LIMIT`:
//!
//! - `SEARCHFOR_EXAMPLE_BARRIER` (`<n> <total>`): after its `begin`, every request waits until `n`
//!   requests are in flight, or all of the `total` that have not ended.
//! - `SEARCHFOR_EXAMPLE_OVERTAKE` (a substring of paths): the first request whose path holds it
//!   waits until a request that began after it has ended.
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
        let p = path.to_string_lossy();
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
        let f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log);
        // One write: `writeln!` may write in pieces, which the appends of concurrent copies
        // interleave (one `start` line lost in 20 runs of h02 under load).
        if let Ok(mut f) = f {
            let _ = f.write_all(format!("start {}\n", std::process::id()).as_bytes());
        }
    }
    if let Ok(line) = std::env::var("SEARCHFOR_EXAMPLE_STDERR") {
        eprintln!("{line}");
    }
    if std::env::var_os("SEARCHFOR_EXAMPLE_FORK").is_some() {
        // Left running on purpose: it holds stdout and stderr open after this process exits.
        let _ = std::process::Command::new("sleep")
            .arg("30")
            .stdin(std::process::Stdio::null())
            .spawn();
    }
}

fn main() {
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
