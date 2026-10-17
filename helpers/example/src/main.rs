//! sfor-example-helper: the example Helper Processor (M2 plan, D11). It describes INI files
//! over protocol v1 (spec §4.4): a `[name]` line opens a `section` up to the next section line,
//! and a `key = value` or `key: value` line is a `key`. Lines starting with `;` or `#` are
//! comments. Environment variables, read once at start, make it misbehave on purpose for
//! sfor's tests; each holds a substring of the request's path:
//!
//! - `SFOR_EXAMPLE_SLOW`: sleep 300 ms before answering.
//! - `SFOR_EXAMPLE_CRASH`: exit with code 3.
//! - `SFOR_EXAMPLE_HANG`: sleep until killed.
//! - `SFOR_EXAMPLE_GARBAGE`: write `not json` instead of an answer.
//! - `SFOR_EXAMPLE_ERROR`: answer an `error`.
//!
//! And at start: `SFOR_EXAMPLE_LOG` (a file to append `start <pid>` to), `SFOR_EXAMPLE_TRACE`
//! (a file to append `begin <pid> <path>` and `end <pid> <path>` to around each request, the
//! `end` before the answer), `SFOR_EXAMPLE_NOEXIT` (when set, keep running after the EOF of
//! stdin, until killed), `SFOR_EXAMPLE_STDERR`
//! (a line to write to stderr), `SFOR_EXAMPLE_FORK` (when set, start `sleep 30` with this
//! process's stdout and stderr, as a careless helper's child would).
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
            slow: var("SFOR_EXAMPLE_SLOW"),
            crash: var("SFOR_EXAMPLE_CRASH"),
            hang: var("SFOR_EXAMPLE_HANG"),
            garbage: var("SFOR_EXAMPLE_GARBAGE"),
            error: var("SFOR_EXAMPLE_ERROR"),
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
            // No answer follows for this request: sfor kills a helper that writes a line it
            // cannot attribute (D41), so this thread waits for that.
            loop {
                std::thread::sleep(Duration::from_secs(3600));
            }
        }
        if hit(&self.error) {
            anyhow::bail!("SFOR_EXAMPLE_ERROR matched {p}");
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
    /// `SFOR_EXAMPLE_TRACE`.
    trace: Option<std::path::PathBuf>,
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

    /// `f` between a `begin` and an `end` line of the trace.
    fn traced<T>(&self, path: &Path, f: impl FnOnce() -> T) -> T {
        self.trace("begin", path);
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
    if let Ok(log) = std::env::var("SFOR_EXAMPLE_LOG") {
        let f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log);
        if let Ok(mut f) = f {
            let _ = writeln!(f, "start {}", std::process::id());
        }
    }
    if let Ok(line) = std::env::var("SFOR_EXAMPLE_STDERR") {
        eprintln!("{line}");
    }
    if std::env::var_os("SFOR_EXAMPLE_FORK").is_some() {
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
    let trace = std::env::var_os("SFOR_EXAMPLE_TRACE").map(Into::into);
    let info = Info {
        name: "sfor-example-helper",
        version: env!("CARGO_PKG_VERSION"),
    };
    if let Err(e) = serve(std::io::stdin().lock(), std::io::stdout(), info, |_| {
        Ok(Ini { knobs, trace })
    }) {
        eprintln!("sfor-example-helper: {e:#}");
        std::process::exit(1);
    }
    if std::env::var_os("SFOR_EXAMPLE_NOEXIT").is_some() {
        loop {
            std::thread::sleep(Duration::from_secs(3600));
        }
    }
}
