//! Every document `--docs` reads is extracted in a child process: the same `sfor` binary run as
//! `sfor __extract-document`, a worker that reads one request a line (`KIND MAX_TEXT PATH`)
//! and writes each answer, with a limit on its memory (M4 plan, D30): its own count of the
//! bytes it holds (`super::memory`, D31), the same on every system, and on Unix `RLIMIT_DATA`
//! a little above, as a second barrier. The limits of each
//! Extractor (D17) give the precise warnings; this one holds whatever path a reader takes,
//! lopdf's inside included: a worker past its memory dies, one past `WALL` on a document is
//! killed, the document gets one warning, and the search goes on with a new worker.
//!
//! The answer: a header line, then the text, then the Outline, each Segment as a few lines
//! (`write_extracted`). At most `CHILDREN` workers run at once; a worker serves up to `SERVES`
//! documents, so the memory one leaves behind does not decide the next one's.
use super::{DocError, Extracted, Kind};
use crate::model::Segment;
use crate::trail::{Outline, OutlineSegment};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// The first argument that makes `sfor` a worker of `extract`, with `PARENT` set. A word that
/// starts with `--` is a flag to the command line, never a positional pattern (ripgrep and sfor
/// refuse an unknown one), so no search can mean it; and `PARENT`, which only `spawn` sets,
/// ties it to the sfor that started it (M4 plan r4, Codex #4).
pub const SUBCOMMAND: &str = "--__extract-document";
/// The pid of the worker's parent, which the worker checks: it ends when the parent does.
const PARENT: &str = "SFOR_READER_OF";
/// The most memory a worker may allocate (its heap, as `super::memory` counts it; a file it maps
/// for reading does not count).
pub const MEMORY_MIB: u64 = 1024;
/// How far above the counter's limit `RLIMIT_DATA` stands, on Unix: room for what the counter
/// does not see (the allocator's own pages, the threads' stacks), so that in most readers the
/// counter cuts first. Where many small blocks spend more than this on the allocator's headers,
/// `RLIMIT_DATA` cuts first, and the warning is the same (M4 plan, D31; r8, Fable #4).
#[cfg(unix)]
const HEADROOM_MIB: u64 = 64;
/// The most workers at once: with `MEMORY_MIB` each, what all of them may take together.
pub const CHILDREN: usize = 4;
/// The most time one document may take.
pub const WALL: Duration = Duration::from_secs(60);
/// The most documents one worker reads before a new one replaces it.
const SERVES: usize = 64;
/// How long a worker gives itself for one document, past the parent's `WALL`: an orphan, whose
/// parent and watchdog are gone, ends there (M4 plan r5, Fable #1: a CPU limit over the
/// worker's life ended a long but valid run). `SFOR_READER_SECONDS` lowers it, for the tests.
const OWN_WALL: Duration = Duration::from_secs(WALL.as_secs() + 5);
/// How long a dead worker's stderr may take to be read to its end: at once, unless another
/// process holds a copy of the pipe.
const DRAINED: Duration = Duration::from_secs(1);
/// The exit code of a worker past its own `OWN_WALL`.
const LATE: i32 = 3;

struct Worker {
    /// Shared with the watchdog, which kills it past `WALL` (M4 plan r4, Fable #4), and with
    /// the Supervisor, which kills and reaps it on a signal: every kill and wait goes through
    /// this one `Child` (M4 plan r5, Codex #1).
    child: Arc<Mutex<Child>>,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    /// The last bytes of its stderr: why it died.
    errors: Arc<Mutex<Vec<u8>>>,
    /// Disconnected when the thread that reads its stderr into `errors` has read it to the end.
    drained: std::sync::mpsc::Receiver<()>,
    served: usize,
}

static IDLE: Mutex<Vec<Worker>> = Mutex::new(Vec::new());
static RUNNING: Mutex<usize> = Mutex::new(0);
static FREED: std::sync::Condvar = std::sync::Condvar::new();

/// One of the `CHILDREN` places for a worker, held while it reads a document.
struct Slot;

impl Slot {
    fn take() -> Slot {
        let mut n = RUNNING.lock().unwrap_or_else(|e| e.into_inner());
        while *n >= CHILDREN {
            n = FREED.wait(n).unwrap_or_else(|e| e.into_inner());
        }
        *n += 1;
        Slot
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        *RUNNING.lock().unwrap_or_else(|e| e.into_inner()) -= 1;
        FREED.notify_one();
    }
}

/// Starts the worker process, from one thread that lives as long as sfor: Linux ends a worker
/// when the thread that started it ends (`PR_SET_PDEATHSIG`), and the walk's threads end one
/// by one while others still use the pool.
fn start() -> std::io::Result<Child> {
    type Ask = std::sync::mpsc::Sender<std::io::Result<Child>>;
    static SPAWNER: std::sync::OnceLock<Mutex<std::sync::mpsc::Sender<Ask>>> =
        std::sync::OnceLock::new();
    let spawner = SPAWNER.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<Ask>();
        std::thread::spawn(move || {
            for reply in rx {
                let child = std::env::current_exe().and_then(|exe| {
                    Command::new(exe)
                        .arg(SUBCOMMAND)
                        .arg(memory_mib().to_string())
                        .env(PARENT, std::process::id().to_string())
                        .stdin(Stdio::piped())
                        .stdout(Stdio::piped())
                        .stderr(Stdio::piped())
                        .spawn()
                });
                let _ = reply.send(child);
            }
        });
        Mutex::new(tx)
    });
    let (tx, rx) = std::sync::mpsc::channel();
    let sent = spawner.lock().unwrap_or_else(|e| e.into_inner()).send(tx);
    sent.map_err(|_| std::io::Error::other("the reader's spawner ended"))?;
    rx.recv()
        .unwrap_or_else(|_| Err(std::io::Error::other("the reader's spawner ended")))
}

/// Whether `args` are a worker's: `SUBCOMMAND` first, `PARENT` set.
pub fn is_worker(args: &[std::ffi::OsString]) -> bool {
    args.len() == 3
        && args[1] == SUBCOMMAND
        && args[2].to_str().is_some_and(|m| m.parse::<u64>().is_ok())
        && std::env::var_os(PARENT).is_some()
}

/// Starts a worker inside the Supervisor's lifecycle (M2 plan, D42): a SIGINT or SIGTERM to
/// sfor kills and reaps it, and no worker starts after one (M4 plan r4, Codex #2).
fn spawn() -> std::io::Result<Worker> {
    let started = crate::helper::host::cancel::GLOBAL.launch_reader(start);
    let shared = match started {
        Ok(c) => c?,
        Err(_) => return Err(std::io::Error::other("sfor is ending")),
    };
    let (stdin, stdout, mut stderr) = pipes(&shared)?;
    let errors = Arc::new(Mutex::new(Vec::new()));
    let keep = errors.clone();
    let (done, drained) = std::sync::mpsc::channel();
    std::thread::spawn(move || drain(&mut stderr, &keep, done));
    Ok(Worker {
        stdin,
        stdout: BufReader::new(stdout),
        child: shared,
        errors,
        drained,
        served: 0,
    })
}

/// Reads the worker's stderr to its end, keeping its last 4 096 bytes in `errors`; `done`
/// drops when it returns.
fn drain(stderr: &mut impl Read, errors: &Mutex<Vec<u8>>, done: std::sync::mpsc::Sender<()>) {
    let mut buf = [0u8; 4096];
    while let Ok(n @ 1..) = stderr.read(&mut buf) {
        let mut e = errors.lock().unwrap_or_else(|e| e.into_inner());
        e.extend_from_slice(&buf[..n]);
        let over = e.len().saturating_sub(4096);
        e.drain(..over);
    }
    drop(done);
}

/// The worker's pipes. A signal's `cancel` may have reaped it between its start and here, and
/// `Child::wait` closes its stdin: then sfor is ending (M4 plan r6, Fable #6).
fn pipes(child: &Mutex<Child>) -> std::io::Result<(ChildStdin, ChildStdout, ChildStderr)> {
    let mut c = child.lock().unwrap_or_else(|e| e.into_inner());
    match (c.stdin.take(), c.stdout.take(), c.stderr.take()) {
        (Some(i), Some(o), Some(e)) => Ok((i, o, e)),
        _ => Err(std::io::Error::other("sfor is ending")),
    }
}

/// Extracts the document at `path` in a worker. A worker taken from the pool that died
/// between documents is replaced once (M4 plan r4, Fable #7).
pub fn extract(kind: Kind, path: &Path, max_text: usize) -> Result<Extracted, DocError> {
    let _slot = Slot::take();
    // The path as its bytes, after their count: any path a file can have (M4 plan r4,
    // Codex #3).
    let p = path_bytes(path);
    let mut fresh = false;
    loop {
        let idle = IDLE.lock().unwrap_or_else(|e| e.into_inner()).pop();
        let mut w = match idle {
            Some(w) => w,
            None => {
                fresh = true;
                spawn()
                    .map_err(|e| DocError::broken(format!("its reader could not start ({e})")))?
            }
        };
        let sent = writeln!(w.stdin, "{} {max_text} {}", kind.name(), p.len())
            .and_then(|_| w.stdin.write_all(&p))
            .and_then(|_| w.stdin.flush());
        if sent.is_err() && !fresh {
            gone(&w);
            continue;
        }
        return answer(w, sent.is_ok());
    }
}

/// Waits for the worker's answer, at most `WALL`.
fn answer(mut w: Worker, sent: bool) -> Result<Extracted, DocError> {
    let child = w.child.clone();
    let (done, wait) = std::sync::mpsc::channel::<()>();
    let watchdog = std::thread::spawn(move || {
        let late = wait.recv_timeout(WALL).is_err();
        if late {
            let _ = child.lock().unwrap_or_else(|e| e.into_inner()).kill();
        }
        late
    });
    let answer = if sent {
        read_extracted(&mut w.stdout)
    } else {
        None
    };
    let _ = done.send(());
    let late = watchdog.join().unwrap_or(false);
    match answer {
        Some(a) if !late => {
            w.served += 1;
            let big = a.as_ref().is_ok_and(|e| e.text.len() > 64 << 20);
            if w.served < SERVES && !big {
                IDLE.lock().unwrap_or_else(|e| e.into_inner()).push(w);
            } else {
                end(w);
            }
            a
        }
        _ => {
            let status = gone(&w);
            if late {
                return Err(DocError::broken(format!(
                    "reading it took more than {} s",
                    WALL.as_secs()
                )));
            }
            // Reaped, the worker has closed its stderr: what it wrote last, the line of
            // `handle_alloc_error` before an abort, is all in `errors` once the thread that
            // reads it ends, which may be after its stdout ended here (M4 plan r8, Codex #2,
            // Fable #3).
            let _ = w.drained.recv_timeout(DRAINED);
            let errors =
                String::from_utf8_lossy(&w.errors.lock().unwrap_or_else(|e| e.into_inner()))
                    .into_owned();
            Err(why(status, &errors))
        }
    }
}

/// Kills and waits for a worker that will not answer.
fn gone(w: &Worker) -> Option<std::process::ExitStatus> {
    let mut child = w.child.lock().unwrap_or_else(|e| e.into_inner());
    let _ = child.kill();
    child.wait().ok()
}

/// Ends the idle workers and waits for them, when `sfor` is done: a worker's memory is then
/// counted with its parent's (`getrusage` of the children, which `/usr/bin/time` reports).
pub fn finish() {
    let idle = std::mem::take(&mut *IDLE.lock().unwrap_or_else(|e| e.into_inner()));
    for w in idle {
        end(w);
    }
}

/// Ends a worker: its stdin closes, it exits, and it is waited for.
fn end(w: Worker) {
    let Worker { child, stdin, .. } = w;
    drop(stdin);
    let _ = child.lock().unwrap_or_else(|e| e.into_inner()).wait();
}

#[cfg(unix)]
fn path_bytes(p: &Path) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    p.as_os_str().as_bytes().to_vec()
}

#[cfg(not(unix))]
fn path_bytes(p: &Path) -> Vec<u8> {
    p.to_string_lossy().into_owned().into_bytes()
}

#[cfg(unix)]
fn path_of(b: Vec<u8>) -> std::path::PathBuf {
    use std::os::unix::ffi::OsStringExt;
    std::ffi::OsString::from_vec(b).into()
}

#[cfg(not(unix))]
fn path_of(b: Vec<u8>) -> std::path::PathBuf {
    String::from_utf8_lossy(&b).into_owned().into()
}

/// The worker's own limit for one document, as `own_watchdog` reads it.
fn reading_limit() -> Duration {
    std::env::var("SFOR_READER_SECONDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .map_or(OWN_WALL, Duration::from_secs)
}

/// `MEMORY_MIB`, or for the tests of this limit `SFOR_READER_MIB`.
fn memory_mib() -> u64 {
    std::env::var("SFOR_READER_MIB")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(MEMORY_MIB)
}

/// The worker's `RLIMIT_DATA` in MiB: `HEADROOM_MIB` above its counter's `mib`, or
/// `SFOR_READER_RLIMIT_MIB`, for the test that the counter cuts where no `RLIMIT_DATA` would.
#[cfg(unix)]
fn rlimit_mib(mib: u64) -> u64 {
    std::env::var("SFOR_READER_RLIMIT_MIB")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(mib.saturating_add(HEADROOM_MIB))
}

/// Why a worker gave no answer.
fn why(status: Option<std::process::ExitStatus>, errors: &str) -> DocError {
    if status.is_some_and(|s| s.code() == Some(LATE)) {
        return DocError::broken(format!(
            "reading it took more than {} s",
            reading_limit().as_secs()
        ));
    }
    if errors.contains("memory allocation of") || errors.contains("out of memory") {
        return warn_memory(memory_mib());
    }
    DocError::broken(match status {
        Some(s) => format!("its reader stopped ({s})"),
        None => "its reader stopped".to_string(),
    })
}

/// The worker: reads `KIND MAX_TEXT LEN` lines from stdin, each followed by the `LEN` bytes
/// of a path, and answers each. Returns its exit code.
pub fn child(args: &[std::ffi::OsString]) -> i32 {
    let mib = args
        .get(2)
        .and_then(|m| m.to_string_lossy().parse().ok())
        .unwrap_or(MEMORY_MIB);
    // First, so what the worker allocates from here on is counted (D31).
    super::memory::arm(super::memory::bytes_of_mib(mib));
    limit(mib);
    orphan_dies();
    let reading = own_watchdog();
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let mut out = std::io::BufWriter::new(std::io::stdout().lock());
    let mut line = String::new();
    loop {
        line.clear();
        match input.read_line(&mut line) {
            Ok(0) => return 0,
            Ok(_) => {}
            Err(_) => return 2,
        }
        let mut parts = line.trim_end_matches('\n').split(' ');
        let (Some(kind), Some(max), Some(len)) = (
            parts.next().and_then(Kind::from_name),
            parts.next().and_then(|m| m.parse::<usize>().ok()),
            parts.next().and_then(|n| n.parse::<usize>().ok()),
        ) else {
            return 2;
        };
        let mut p = vec![0; len];
        if input.read_exact(&mut p).is_err() {
            return 2;
        }
        // A refusal before this document (reading the request) is not its own.
        super::memory::take_refused();
        reading.store(now_ms(), std::sync::atomic::Ordering::SeqCst);
        let answer = match read(&path_of(p)) {
            Ok(bytes) => {
                let got = std::panic::catch_unwind(|| super::extract_with(kind, &bytes, max));
                got.unwrap_or_else(|_| Err(DocError::broken("its reader failed")))
            }
            Err(e) => Err(DocError::broken(e)),
        };
        reading.store(0, std::sync::atomic::Ordering::SeqCst);
        let answer = refused_memory(answer, super::memory::take_refused(), mib);
        if write_extracted(&answer, &mut out)
            .and_then(|_| out.flush())
            .is_err()
        {
            return 2;
        }
    }
}

/// Milliseconds since the worker's first look at the clock, never 0: a monotonic clock, as the
/// parent's `WALL` counts, which a wall clock set forward or a resume from suspend does not
/// move (M4 plan r6, Codex #2, Fable #3).
fn now_ms() -> u64 {
    static ORIGIN: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    ORIGIN
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_millis() as u64
        + 1
}

/// The worker's own watchdog, per document: when one document takes longer than `OWN_WALL`
/// (or `SFOR_READER_SECONDS`), the worker exits with `LATE`. It ends an orphan on any system;
/// with a parent, the parent's `WALL` comes first. Returns when the current document started
/// (0: none).
fn own_watchdog() -> Arc<std::sync::atomic::AtomicU64> {
    let limit = reading_limit();
    let reading = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let r = reading.clone();
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_millis(100));
            let at = r.load(std::sync::atomic::Ordering::SeqCst);
            if at != 0 && now_ms().saturating_sub(at) > limit.as_millis() as u64 {
                std::process::exit(LATE);
            }
        }
    });
    reading
}

/// A reader that asked for memory and was refused (`try_reserve`, `ErrorKind::OutOfMemory`,
/// whose message ends `out of memory`) warns as one that died of it (M4 plan r4, Fable #2);
/// and so does one that was `refused` memory during the document whatever it answered, an
/// `Ok` included, whose text is then dropped: a reader that went on without the memory may
/// have read less than the document holds (M4 plan r8, Codex #1).
fn refused_memory(
    a: Result<Extracted, DocError>,
    refused: bool,
    mib: u64,
) -> Result<Extracted, DocError> {
    match a {
        Err(DocError::Broken(m)) if m.ends_with("out of memory") => Err(warn_memory(mib)),
        _ if refused => Err(warn_memory(mib)),
        a => a,
    }
}

fn warn_memory(mib: u64) -> DocError {
    DocError::broken(format!(
        "reading it needed more than the {mib} MiB a reader may use"
    ))
}

/// The worker ends with its parent, which holds its watchdog (M4 plan r4, Codex #2): on Linux
/// the kernel kills it then (`PR_SET_PDEATHSIG`), checked against the parent's pid in case the
/// parent ended first; elsewhere the worker's own watchdog (`own_watchdog`) ends an orphan at
/// its document's `OWN_WALL`.
#[cfg(target_os = "linux")]
fn orphan_dies() {
    // SAFETY: prctl with an integer argument, and getppid, have no memory effects.
    unsafe {
        libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
        let parent = std::env::var(PARENT)
            .ok()
            .and_then(|p| p.parse::<i32>().ok());
        if parent != Some(libc::getppid()) {
            std::process::exit(2);
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn orphan_dies() {}

/// The file's bytes: mapped when it is large, as `Source::open` reads it.
fn read(path: &Path) -> std::io::Result<Bytes> {
    let file = std::fs::File::open(path)?;
    if file.metadata()?.len() >= 1 << 20 {
        // SAFETY: read-only map of a file the child only reads.
        return Ok(Bytes::Mapped(unsafe { memmap2::Mmap::map(&file)? }));
    }
    let mut v = Vec::new();
    (&file).read_to_end(&mut v)?;
    Ok(Bytes::Owned(v))
}

enum Bytes {
    Owned(Vec<u8>),
    Mapped(memmap2::Mmap),
}

impl std::ops::Deref for Bytes {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        match self {
            Bytes::Owned(v) => v,
            Bytes::Mapped(m) => m,
        }
    }
}

/// The worker's limits: `RLIMIT_DATA` a little above its counter's `mib` (the counter is
/// armed already), and no core file.
#[cfg(unix)]
fn limit(mib: u64) {
    let set = |what, n: u64| {
        let r = libc::rlimit {
            rlim_cur: n as libc::rlim_t,
            rlim_max: n as libc::rlim_t,
        };
        // SAFETY: setrlimit reads the struct it is given.
        unsafe { libc::setrlimit(what, &r) };
    };
    set(libc::RLIMIT_DATA, rlimit_mib(mib).saturating_mul(1 << 20));
    // No core file and no crash report for a hostile document (M4 plan r4, Fable #3).
    set(libc::RLIMIT_CORE, 0);
    #[cfg(target_os = "linux")]
    // SAFETY: prctl with integer arguments.
    unsafe {
        libc::prctl(libc::PR_SET_DUMPABLE, 0);
    }
}

#[cfg(not(unix))]
fn limit(_mib: u64) {}

/// The answer: `ok TEXT_LEN SEGMENTS CUT` and the text and Segments, or `err KIND` and the
/// error's lines.
fn write_extracted(a: &Result<Extracted, DocError>, out: &mut impl Write) -> std::io::Result<()> {
    let e = match a {
        Ok(e) => e,
        Err(DocError::Encrypted) => return writeln!(out, "err encrypted"),
        Err(DocError::Broken(m)) => return writeln!(out, "err broken\n{}", one(m)),
        Err(DocError::TooLarge(what, n)) => {
            return writeln!(out, "err too-large {n}\n{}", one(what));
        }
    };
    let segs = &e.outline.segments;
    writeln!(out, "ok {} {} {}", e.text.len(), segs.len(), e.cut as u8)?;
    out.write_all(&e.text)?;
    for s in segs {
        let g = &s.segment;
        let names = s.name.as_ref().map_or(String::new(), |n| {
            n.iter()
                .map(|(a, b)| format!("{} {} {} {}", a.start, a.end, b.start, b.end))
                .collect::<Vec<_>>()
                .join(" ")
        });
        writeln!(
            out,
            "{} {} {} {} {} {} {} {} {} {} {}",
            g.line,
            g.end_line,
            g.level.map_or(-1, i32::from),
            s.range.start,
            s.range.end,
            s.label.start,
            s.label.end,
            s.name_anchor,
            s.parent.map_or(-1, |p| p as i64),
            s.hidden as u8,
            s.name.is_some() as u8,
        )?;
        writeln!(
            out,
            "{}\n{}\n{}\n{names}",
            one(&g.kind),
            one(&g.name),
            g.signature
                .as_deref()
                .map_or("-".into(), |s| format!("+{}", one(s)))
        )?;
    }
    Ok(())
}

/// `s` on one line: its line breaks and backslashes escaped.
fn one(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

fn unone(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some(c) => out.push(c),
            None => {}
        }
    }
    out
}

/// Reads what `write_extracted` wrote; None when it is not that.
fn read_extracted(r: &mut impl BufRead) -> Option<Result<Extracted, DocError>> {
    let mut line = String::new();
    let mut next = |r: &mut dyn BufRead| -> Option<String> {
        line.clear();
        (r.read_line(&mut line).ok()? > 0).then(|| line.trim_end_matches('\n').to_string())
    };
    let head = next(r)?;
    let mut words = head.split(' ');
    match words.next()? {
        "err" => {
            return Some(Err(match words.next()? {
                "encrypted" => DocError::Encrypted,
                "broken" => DocError::Broken(unone(&next(r)?)),
                "too-large" => {
                    let n = words.next()?.parse().ok()?;
                    DocError::TooLarge(unone(&next(r)?), n)
                }
                _ => return None,
            }));
        }
        "ok" => {}
        _ => return None,
    }
    let len: usize = words.next()?.parse().ok()?;
    let count: usize = words.next()?.parse().ok()?;
    let cut = words.next()? == "1";
    let mut text = Vec::new();
    (&mut *r).take(len as u64).read_to_end(&mut text).ok()?;
    if text.len() != len {
        return None;
    }
    let mut segments = Vec::with_capacity(count.min(1 << 20));
    for _ in 0..count {
        let nums = next(r)?;
        let n: Vec<i64> = nums
            .split(' ')
            .map(|w| w.parse().ok())
            .collect::<Option<_>>()?;
        let [
            line,
            end_line,
            level,
            rs,
            re,
            ls,
            le,
            anchor,
            parent,
            hidden,
            named,
        ] = n[..]
        else {
            return None;
        };
        let kind = unone(&next(r)?);
        let name = unone(&next(r)?);
        let signature = next(r)?;
        let signature = signature.strip_prefix('+').map(unone);
        let pieces = next(r)?;
        let p: Vec<usize> = pieces
            .split(' ')
            .filter(|w| !w.is_empty())
            .map(|w| w.parse().ok())
            .collect::<Option<_>>()?;
        let u = |v: i64| usize::try_from(v).ok();
        segments.push(OutlineSegment {
            segment: Segment {
                kind,
                name,
                line: line as u64,
                end_line: end_line as u64,
                level: u8::try_from(level).ok(),
                signature,
            },
            range: u(rs)?..u(re)?,
            label: u(ls)?..u(le)?,
            name: (named == 1).then(|| {
                p.chunks(4)
                    .filter(|c| c.len() == 4)
                    .map(|c| (c[0]..c[1], c[2]..c[3]))
                    .collect()
            }),
            name_anchor: u(anchor)?,
            parent: u(parent),
            hidden: hidden == 1,
        });
    }
    Some(Ok(Extracted {
        text,
        outline: Outline { segments },
        cut,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(a: Result<Extracted, DocError>) -> Option<Result<Extracted, DocError>> {
        let mut buf = Vec::new();
        write_extracted(&a, &mut buf).unwrap();
        read_extracted(&mut &buf[..])
    }

    /// Fable r6 #6: a worker that a signal's `cancel` reaped before `spawn` took its pipes (its
    /// stdin closed by `Child::wait`) is an error, never a panic.
    #[cfg(unix)]
    #[test]
    fn a_worker_reaped_before_its_pipes_are_taken_is_an_error() {
        let mut c = Command::new("true")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        c.wait().unwrap();
        let reaped = Mutex::new(c);
        let e = pipes(&reaped).expect_err("no pipes");
        assert_eq!(e.to_string(), "sfor is ending");
    }

    /// A refused `try_reserve` is the memory warning; and so is any answer of a document during
    /// which the counter refused memory, an `Ok` included, its partial text dropped (M4 plan r8,
    /// Codex #1: lopdf ends a stream where `read_to_end` failed, and the document read as
    /// empty).
    /// Codex r8 #2, Fable r8 #3: the warning of a worker that died of its memory never depends
    /// on whether the thread that reads its stderr ran before its stdout ended. Here that
    /// thread starts 300 ms late, after the worker wrote `memory allocation of …` and exited.
    #[cfg(unix)]
    #[test]
    fn a_dead_workers_stderr_is_read_to_its_end_before_why() {
        let shared = Arc::new(Mutex::new(
            Command::new("sh")
                .args([
                    "-c",
                    "echo 'memory allocation of 8 bytes failed' >&2; exit 134",
                ])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        ));
        let (stdin, stdout, mut stderr) = pipes(&shared).unwrap();
        let errors = Arc::new(Mutex::new(Vec::new()));
        let keep = errors.clone();
        let (done, drained) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            drain(&mut stderr, &keep, done);
        });
        let w = Worker {
            child: shared,
            stdin,
            stdout: BufReader::new(stdout),
            errors,
            drained,
            served: 0,
        };
        assert_eq!(
            answer(w, true).unwrap_err(),
            DocError::broken(format!(
                "reading it needed more than the {} MiB a reader may use",
                memory_mib()
            ))
        );
    }

    #[test]
    fn memory_refused_is_the_memory_warning() {
        let warning = DocError::broken("reading it needed more than the 16 MiB a reader may use");
        let oom = std::io::Error::from(std::io::ErrorKind::OutOfMemory);
        let e = refused_memory(
            Err(DocError::broken(format!("word/document.xml: {oom}"))),
            false,
            16,
        );
        assert_eq!(e.unwrap_err(), warning);
        let e = refused_memory(Err(DocError::Encrypted), false, 16);
        assert_eq!(e.unwrap_err(), DocError::Encrypted);
        let mut b = super::super::Builder::new(super::super::MAX_TEXT);
        b.line("a partial needle");
        let partial = b.finish();
        let e = refused_memory(Ok(partial.clone()), true, 16);
        assert_eq!(e.unwrap_err(), warning);
        let e = refused_memory(Err(DocError::Encrypted), true, 16);
        assert_eq!(e.unwrap_err(), warning);
        let e = refused_memory(Ok(partial.clone()), false, 16);
        assert_eq!(e.unwrap().text, partial.text);
    }

    #[test]
    fn an_answer_reads_back_as_it_was_written() {
        // Built with the Builder, so the test needs no writer (the writers are stubs until
        // tanda 3).
        let mut b = super::super::Builder::new(super::super::MAX_TEXT);
        b.heading(1, "Intro\\with a \"quote\"");
        b.line("needle");
        let t = b.open("table", "table 1");
        b.start_row(None);
        b.cell("column", "A", "1");
        b.cell("column", "B", "needle");
        b.end_row();
        b.close(t);
        let e = b.finish();
        assert!(!e.outline.segments.is_empty());
        let back = round_trip(Ok(e.clone())).unwrap().unwrap();
        assert_eq!(back.text, e.text);
        assert_eq!(back.outline, e.outline);
        assert_eq!(back.cut, e.cut);
        for err in [
            DocError::Encrypted,
            DocError::Broken("two\nlines \\ here".into()),
            DocError::TooLarge("word/document.xml".into(), 256),
        ] {
            assert_eq!(round_trip(Err(err.clone())).unwrap().unwrap_err(), err);
        }
        assert!(read_extracted(&mut &b"ok 10 0 0\nshort"[..]).is_none());
        assert!(read_extracted(&mut &b""[..]).is_none());
    }
}
