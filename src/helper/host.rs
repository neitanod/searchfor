//! The sfor side of protocol v1 (spec §4.4, design note 5): starting helpers, feeding them from
//! many search threads, and what happens when they fail. Every process has a writer thread
//! (D31), so no caller ever blocks on a pipe, and one idempotent close that kills when needed,
//! always reaps, and waits for its pipe threads at most `PIPE_WAIT` (D32).
use super::protocol::{PROTOCOL, Reply, Request, line};
use super::{Control, HelperError, HostConfig, Launcher, Mode, Proc, Warn};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering::SeqCst};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::time::{Duration, Instant};

const SHUTDOWN_WAIT: Duration = Duration::from_secs(1);
const PIPE_WAIT: Duration = Duration::from_secs(1);
const MAX_FAILURES: u32 = 3;

/// Why a request got no answer.
enum Fail {
    /// The process is gone or broken: every request on it fails (counts as a failure).
    Gone(String),
    /// A JSON answer with this request's id that does not fit the protocol (D41).
    Malformed(String),
}

type Waiter = mpsc::Sender<Result<Reply, Fail>>;

/// Runs `argv` with its three pipes, in sfor's working directory and process group (D32).
pub struct CommandLauncher {
    pub argv: Vec<String>,
}

struct Child(std::process::Child);

impl Control for Child {
    fn kill(&mut self) {
        let _ = self.0.kill();
    }
    fn wait(&mut self, within: Duration) -> bool {
        let t = Instant::now();
        loop {
            match self.0.try_wait() {
                Ok(Some(_)) | Err(_) => return true,
                Ok(None) if t.elapsed() >= within => return false,
                Ok(None) => std::thread::sleep(Duration::from_millis(5)),
            }
        }
    }
    fn reap(&mut self) {
        let _ = self.0.wait();
    }
}

impl Launcher for CommandLauncher {
    fn launch(&self) -> std::io::Result<Proc> {
        use std::process::{Command, Stdio};
        let (prog, args) = self
            .argv
            .split_first()
            .ok_or_else(|| std::io::Error::other("empty command"))?;
        let mut c = Command::new(prog)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        Ok(Proc {
            stdin: Box::new(c.stdin.take().expect("piped")),
            stdout: Box::new(c.stdout.take().expect("piped")),
            stderr: Some(Box::new(c.stderr.take().expect("piped"))),
            control: Box::new(Child(c)),
        })
    }
}

fn fail_all(pending: &Mutex<HashMap<u64, Waiter>>, alive: &AtomicBool, why: &str) {
    // `alive` first: a request registered after this sees it and fails at once.
    alive.store(false, SeqCst);
    for (_, w) in pending.lock().unwrap().drain() {
        let _ = w.send(Err(Fail::Gone(why.to_string())));
    }
}

/// One launched process.
struct Conn {
    /// To the writer thread, which owns stdin; None once stdin is closed.
    queue: Mutex<Option<mpsc::Sender<Vec<u8>>>>,
    pending: Arc<Mutex<HashMap<u64, Waiter>>>,
    in_flight: AtomicUsize,
    alive: Arc<AtomicBool>,
    /// Its failure was counted (several requests in flight see the same crash).
    counted: AtomicBool,
    closed: AtomicBool,
    /// D42: set by `abort`; silences the stderr relay.
    aborted: Arc<AtomicBool>,
    /// Reaped already: `abort` and `close` may both run (Ctrl-C during a failure).
    reaped: Arc<AtomicBool>,
    /// Shared with the reader thread, which reaps the process when its output ends.
    control: Arc<Mutex<Box<dyn Control>>>,
    /// One per pipe thread: a message when it ends.
    ends: Mutex<Vec<mpsc::Receiver<()>>>,
}

impl Conn {
    fn spawn(p: Proc, format: &str, warn: &Warn) -> Arc<Conn> {
        let pending: Arc<Mutex<HashMap<u64, Waiter>>> = Default::default();
        let alive = Arc::new(AtomicBool::new(true));
        let mut ends = Vec::new();
        let mut thread = |f: Box<dyn FnOnce() + Send>| {
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                f();
                let _ = tx.send(());
            });
            ends.push(rx);
        };
        // Writer: owns stdin. A kill makes a blocked write fail; dropping the queue closes stdin.
        let (qtx, qrx) = mpsc::channel::<Vec<u8>>();
        let (pend, live, mut stdin) = (pending.clone(), alive.clone(), p.stdin);
        thread(Box::new(move || {
            for buf in qrx {
                if stdin.write_all(&buf).and_then(|_| stdin.flush()).is_err() {
                    fail_all(&pend, &live, "closed its input");
                    return;
                }
            }
        }));
        let control: Arc<Mutex<Box<dyn Control>>> = Arc::new(Mutex::new(p.control));
        let reaped = Arc::new(AtomicBool::new(false));
        // Reader (D41).
        let (pend, live, stdout) = (pending.clone(), alive.clone(), p.stdout);
        let (ctl, done) = (control.clone(), reaped.clone());
        thread(Box::new(move || {
            let mut why = "exited".to_string();
            for l in BufReader::new(stdout).lines() {
                let Ok(l) = l else {
                    why = "closed its output".into();
                    break;
                };
                if l.trim().is_empty() {
                    continue;
                }
                let v: serde_json::Value = match serde_json::from_str(&l) {
                    Ok(v) => v,
                    Err(e) => {
                        why = format!("wrote a line that is not JSON ({e})");
                        break;
                    }
                };
                let Some(id) = v.get("id").and_then(serde_json::Value::as_u64) else {
                    why = "wrote a line without an id".into();
                    break;
                };
                // An id nobody waits for (an answer after its timeout) is dropped.
                let Some(w) = pend.lock().unwrap().remove(&id) else {
                    continue;
                };
                let r = serde_json::from_value::<Reply>(v).map_err(|e| {
                    Fail::Malformed(format!(
                        "answered with fields that do not fit protocol v1 ({e})"
                    ))
                });
                let _ = w.send(r);
            }
            fail_all(&pend, &live, &why);
            // The output ended or broke: collect the process now, even with nothing in flight,
            // so a helper that exits between requests is not a zombie until the next request or
            // the shutdown (review r5, Codex #1). The kill only matters for a process that broke
            // its output and is still alive; on one that exited it is harmless, since it is not
            // reaped yet and its pid cannot be reused. A `close` in progress holds `control`, so
            // this waits for it and then finds the process reaped.
            let mut c = ctl.lock().unwrap();
            if !done.load(SeqCst) {
                c.kill();
                if !done.swap(true, SeqCst) {
                    c.reap();
                }
            }
        }));
        // Set by `abort` before its kill: a helper's dying words on Ctrl-C (Python's
        // KeyboardInterrupt traceback) are not relayed, as ripgrep prints nothing then. Not
        // `alive`: `close` clears that too, and a helper may legitimately write on the EOF of a
        // shutdown (review r5, Fable #2).
        let aborted = Arc::new(AtomicBool::new(false));
        if let Some(err) = p.stderr {
            let (warn, format, quiet) = (warn.clone(), format.to_string(), aborted.clone());
            thread(Box::new(move || {
                for l in BufReader::new(err).lines().map_while(Result::ok) {
                    if !quiet.load(SeqCst) {
                        warn(format!("sfor: {format}: {l}"));
                    }
                }
            }));
        }
        Arc::new(Conn {
            queue: Mutex::new(Some(qtx)),
            pending,
            in_flight: AtomicUsize::new(0),
            alive,
            counted: AtomicBool::new(false),
            closed: AtomicBool::new(false),
            aborted,
            reaped,
            control,
            ends: Mutex::new(ends),
        })
    }

    fn alive(&self) -> bool {
        self.alive.load(SeqCst)
    }

    /// Registers `req` and queues it for the writer. Never blocks (D31). None: the process was
    /// already gone, so nothing was sent (Fable r2 #7: not a failure of this request).
    fn send(&self, req: &Request) -> Option<mpsc::Receiver<Result<Reply, Fail>>> {
        let (tx, rx) = mpsc::channel();
        let id = req.id();
        self.pending.lock().unwrap().insert(id, tx);
        let queued = self.alive()
            && self
                .queue
                .lock()
                .unwrap()
                .as_ref()
                .is_some_and(|q| q.send(line(req).into_bytes()).is_ok());
        if !queued {
            self.pending.lock().unwrap().remove(&id);
            return None;
        }
        Some(rx)
    }

    /// D42: kill and reap now, without the grace or the pipe wait of `close` (Ctrl-C).
    fn abort(&self) {
        self.aborted.store(true, SeqCst);
        self.alive.store(false, SeqCst);
        let mut c = self.control.lock().unwrap();
        c.kill();
        if !self.reaped.swap(true, SeqCst) {
            c.reap();
        }
    }

    /// Closes stdin once the queued lines are written (oneshot; shutdown).
    fn close_input(&self) {
        self.queue.lock().unwrap().take();
    }

    /// The one way a process ends (D32): stdin closed, a second of grace unless `kill`, kill,
    /// reap, then at most PIPE_WAIT for the pipe threads; a descendant that inherited a pipe
    /// can keep one alive, and that thread is left behind. Idempotent.
    fn close(&self, kill: bool) {
        if self.closed.swap(true, SeqCst) {
            return;
        }
        self.alive.store(false, SeqCst);
        self.close_input();
        {
            let mut c = self.control.lock().unwrap();
            if !self.reaped.load(SeqCst) && (kill || !c.wait(SHUTDOWN_WAIT)) {
                c.kill();
            }
            if !self.reaped.swap(true, SeqCst) {
                c.reap();
            }
        }
        let deadline = Instant::now() + PIPE_WAIT;
        for end in self.ends.lock().unwrap().drain(..) {
            let _ = end.recv_timeout(deadline.saturating_duration_since(Instant::now()));
        }
    }
}

enum CallError {
    Timeout,
    Gone(String),
    Malformed(String),
}

/// A receiver that already holds «exited»: for `hello`, where a process gone before it
/// answered is a handshake failure.
fn gone() -> mpsc::Receiver<Result<Reply, Fail>> {
    let (tx, rx) = mpsc::channel();
    let _ = tx.send(Err(Fail::Gone("exited".into())));
    rx
}

fn wait(rx: mpsc::Receiver<Result<Reply, Fail>>, deadline: Instant) -> Result<Reply, CallError> {
    match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
        Ok(Ok(r)) => Ok(r),
        Ok(Err(Fail::Gone(w))) => Err(CallError::Gone(w)),
        Ok(Err(Fail::Malformed(m))) => Err(CallError::Malformed(m)),
        Err(mpsc::RecvTimeoutError::Timeout) => Err(CallError::Timeout),
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(CallError::Gone("exited".into())),
    }
}

#[derive(Default)]
struct State {
    conns: Vec<Arc<Conn>>,
    /// Processes being started (their `hello` is in flight).
    starting: usize,
    failures: u32,
    disabled: bool,
    /// The warning of a disable caused by a request's failure, held until the warning of
    /// that request's file is out (review m2-code-r1, Fable #5).
    notice: Option<String>,
}

/// The helper processes of one Definition for one run.
pub struct Host {
    cfg: HostConfig,
    launcher: Box<dyn Launcher>,
    warn: Warn,
    st: Mutex<State>,
    freed: Condvar,
    next_id: AtomicU64,
    /// The lifecycle every launch goes through (D42): the process-wide one, or a test's own.
    sup: &'static cancel::Supervisor,
}

impl Host {
    pub fn new(cfg: HostConfig, launcher: Box<dyn Launcher>, warn: Warn) -> Self {
        Self::with_supervisor(cfg, launcher, warn, &cancel::GLOBAL)
    }

    /// Tests give each Host a Supervisor of its own (`Box::leak`), so cancelling one never
    /// touches another test's helpers.
    pub(crate) fn with_supervisor(
        cfg: HostConfig,
        launcher: Box<dyn Launcher>,
        warn: Warn,
        sup: &'static cancel::Supervisor,
    ) -> Self {
        Self {
            cfg,
            launcher,
            warn,
            st: Mutex::default(),
            freed: Condvar::new(),
            next_id: AtomicU64::new(1),
            sup,
        }
    }

    pub fn format(&self) -> &str {
        &self.cfg.format
    }

    /// Sends the request `make` builds with a fresh id, and returns its answer. The deadline
    /// covers queuing, writing and answering (D31); it starts once a copy takes the request,
    /// so waiting for a copy to start (or to be free) does not count against it (spec §4.4,
    /// review m2-code-r1, Fable #4).
    pub fn request(&self, make: impl FnOnce(u64) -> Request) -> Result<Reply, HelperError> {
        let req = make(self.next_id.fetch_add(1, SeqCst));
        if self.cfg.mode == Mode::Oneshot {
            return self.oneshot(&req);
        }
        // A copy that exited with nothing in flight is replaced once, without counting it.
        for attempt in 0..2 {
            let conn = self.checkout().inspect_err(|_| self.flush_notice())?;
            let deadline = Instant::now() + self.cfg.timeout;
            let sent = conn.send(&req);
            let out = sent.map(|rx| self.outcome(&conn, wait(rx, deadline)));
            self.release(&conn);
            match out {
                // Once cancelled, every failure is `Disabled`: the file is Plain, and the
                // pipeline prints no warning for it (D33, review r4, Fable #3).
                Some(Err(_)) if self.sup.cancelled() => return Err(HelperError::Disabled),
                Some(out) => return out,
                None if attempt == 0 => continue,
                None => break,
            }
        }
        Err(HelperError::Crashed("the helper exited".into()))
    }

    fn release(&self, conn: &Conn) {
        conn.in_flight.fetch_sub(1, SeqCst);
        let _st = self.st.lock().unwrap();
        self.freed.notify_all();
    }

    fn outcome(&self, conn: &Conn, r: Result<Reply, CallError>) -> Result<Reply, HelperError> {
        match r {
            Ok(Reply { error: Some(e), .. }) => Err(HelperError::Answer(e)),
            Ok(reply) => Ok(reply),
            Err(CallError::Malformed(m)) => Err(HelperError::Protocol(m)),
            Err(CallError::Timeout) => {
                self.failed(conn);
                Err(HelperError::Timeout(self.cfg.timeout.as_millis()))
            }
            Err(CallError::Gone(why)) => {
                self.failed(conn);
                Err(HelperError::Crashed(format!("the helper {why}")))
            }
        }
    }

    /// D40: `hello` and the request, stdin closed, then both answers, under one deadline.
    fn oneshot(&self, req: &Request) -> Result<Reply, HelperError> {
        if self.st.lock().unwrap().disabled {
            return Err(HelperError::Disabled);
        }
        let conn = self.launch()?;
        let deadline = Instant::now() + self.cfg.timeout;
        let hello = conn
            .send(&Request::hello(&self.cfg.format, &self.cfg.params, 1))
            .unwrap_or_else(gone);
        let answer = conn.send(req).unwrap_or_else(gone);
        conn.close_input();
        let out = match wait(hello, deadline) {
            Ok(r) if r.protocol == Some(PROTOCOL) && r.error.is_none() => {
                self.outcome(&conn, wait(answer, deadline))
            }
            Ok(r) => {
                self.disable(refusal(&r));
                Err(HelperError::Disabled)
            }
            // No answer to hello, a process that exits or writes a broken line before it, is
            // a handshake failure, as in `start` (Codex r2 #5; m2-code-r1, Codex #5).
            Err(e) => {
                conn.close(true);
                self.disable(self.no_hello(e));
                Err(HelperError::Disabled)
            }
        };
        conn.close(false);
        if out.is_err() && self.sup.cancelled() {
            return Err(HelperError::Disabled);
        }
        out
    }

    /// Every process starts here, inside the Supervisor's lifecycle (D42): refused once the run
    /// is cancelled, and registered before a cancellation can miss it.
    fn launch(&self) -> Result<Arc<Conn>, HelperError> {
        let started = self.sup.launch(|| {
            self.launcher
                .launch()
                .map(|p| Conn::spawn(p, &self.cfg.format, &self.warn))
        });
        match started {
            Err(cancel::Cancelled) => Err(HelperError::Disabled),
            Ok(Ok(conn)) => Ok(conn),
            Ok(Err(e)) => {
                self.disable(format!(
                    "cannot start `{}`: {e}",
                    self.cfg.command.join(" ")
                ));
                Err(HelperError::Disabled)
            }
        }
    }

    /// A connection with room for one more request, started if the mode allows it.
    fn checkout(&self) -> Result<Arc<Conn>, HelperError> {
        let limit = match self.cfg.mode {
            Mode::Pool => self.cfg.threads,
            Mode::Concurrent | Mode::Oneshot => 1,
        };
        let mut st = self.st.lock().unwrap();
        loop {
            if st.disabled {
                return Err(HelperError::Disabled);
            }
            // Dead copies leave the pool through `close`, which reaps them (D32).
            let mut dead = Vec::new();
            st.conns.retain(|c| {
                c.alive() || {
                    dead.push(c.clone());
                    false
                }
            });
            if !dead.is_empty() {
                drop(st);
                for c in dead {
                    c.close(true);
                }
                st = self.st.lock().unwrap();
                continue;
            }
            let free = match self.cfg.mode {
                // Codex #14: never more than `threads` requests in flight to the process.
                Mode::Concurrent => st
                    .conns
                    .first()
                    .filter(|c| c.in_flight.load(SeqCst) < self.cfg.threads)
                    .cloned(),
                _ => st
                    .conns
                    .iter()
                    .find(|c| c.in_flight.load(SeqCst) == 0)
                    .cloned(),
            };
            if let Some(c) = free {
                c.in_flight.fetch_add(1, SeqCst);
                return Ok(c);
            }
            if st.conns.len() + st.starting < limit {
                st.starting += 1;
                drop(st);
                let started = self.start();
                st = self.st.lock().unwrap();
                st.starting -= 1;
                self.freed.notify_all();
                let c = started?;
                c.in_flight.fetch_add(1, SeqCst);
                st.conns.push(c.clone());
                return Ok(c);
            }
            st = self.freed.wait(st).unwrap();
        }
    }

    /// Launches a process and exchanges `hello` (spec §4.4, 1).
    fn start(&self) -> Result<Arc<Conn>, HelperError> {
        let conn = self.launch()?;
        let threads = if self.cfg.mode == Mode::Concurrent {
            self.cfg.threads
        } else {
            1
        };
        let hello = Request::hello(&self.cfg.format, &self.cfg.params, threads);
        let why = match wait(
            conn.send(&hello).unwrap_or_else(gone),
            Instant::now() + self.cfg.timeout,
        ) {
            Ok(r) if r.protocol == Some(PROTOCOL) && r.error.is_none() => return Ok(conn),
            Ok(r) => refusal(&r),
            Err(e) => self.no_hello(e),
        };
        conn.close(true);
        self.disable(why);
        Err(HelperError::Disabled)
    }

    /// The warning for a process that gave no answer to `hello`.
    fn no_hello(&self, e: CallError) -> String {
        match e {
            CallError::Timeout => format!(
                "did not answer hello within {} ms",
                self.cfg.timeout.as_millis()
            ),
            CallError::Gone(w) | CallError::Malformed(w) => format!("{w} before answering hello"),
        }
    }

    /// A crash, a timeout or an unattributable line: close (kill, reap) the copy and count it
    /// once (spec §4.4).
    fn failed(&self, c: &Conn) {
        if c.counted.swap(true, SeqCst) {
            return;
        }
        c.close(true);
        let disable = {
            let mut st = self.st.lock().unwrap();
            st.failures += 1;
            st.failures >= MAX_FAILURES && !st.disabled
        };
        if disable {
            self.disable_with(format!("the helper failed {MAX_FAILURES} times"), true);
        }
    }

    fn disable(&self, why: String) {
        self.disable_with(why, false);
    }

    /// Disables the Definition with one warning; `later`: held as the notice, for the caller
    /// to print after the warning of the failing file (`take_notice`).
    fn disable_with(&self, why: String, later: bool) {
        let msg = format!(
            "sfor: {}: {why}; its files print without Trails for the rest of this run",
            self.cfg.format
        );
        let now = {
            let mut st = self.st.lock().unwrap();
            let first = !std::mem::replace(&mut st.disabled, true);
            if first && later {
                st.notice = Some(msg.clone());
            }
            first && !later
        };
        if now && !self.sup.cancelled() {
            (self.warn)(msg);
        }
        self.freed.notify_all();
    }

    /// The disable notice not printed yet, if any; the caller prints it.
    pub fn take_notice(&self) -> Option<String> {
        let n = self.st.lock().unwrap().notice.take();
        n.filter(|_| !self.sup.cancelled())
    }

    /// Prints the notice held, if no one did (a later request, the shutdown).
    fn flush_notice(&self) {
        if let Some(n) = self.take_notice() {
            (self.warn)(n);
        }
    }

    /// End of the run: no more requests; every process gets EOF, a second, then a kill, and is
    /// reaped (spec §4.4, Lifetime). Not a warning: a shutdown is not a failure. The copies
    /// close at once, so their second of grace and of pipes is spent together, not once per
    /// copy (review m2-code-r1, Fable #1).
    pub fn shutdown(&self) {
        self.flush_notice();
        let conns = {
            let mut st = self.st.lock().unwrap();
            st.disabled = true;
            std::mem::take(&mut st.conns)
        };
        self.freed.notify_all();
        std::thread::scope(|s| {
            for c in &conns {
                s.spawn(move || c.close(false));
            }
        });
    }
}

/// D42: one lifecycle for every helper process of this sfor process. Every launch runs under
/// the Supervisor's lock: it is refused once the run is cancelled, and the new process is
/// registered before the lock is released, so a cancellation either comes first (nothing
/// starts) or sees the process (it is killed and reaped). The process-wide Supervisor installs
/// its SIGINT/SIGTERM watcher before the first spawn; on a signal it cancels, kills and reaps
/// every helper — one that ignores SIGINT, or a signal sent to sfor's pid alone, included — and
/// then sfor dies by that signal with its default disposition, as ripgrep would. A run that
/// launches no helper installs nothing.
pub(crate) mod cancel {
    use super::Conn;
    use std::sync::{Arc, Mutex, Weak};

    pub(crate) struct Supervisor {
        state: Mutex<Lifecycle>,
        /// Only the process-wide Supervisor watches signals.
        signals: bool,
    }

    struct Lifecycle {
        cancelled: bool,
        live: Vec<Weak<Conn>>,
    }

    #[derive(Debug, PartialEq, Eq)]
    pub(crate) struct Cancelled;

    pub(crate) static GLOBAL: Supervisor = Supervisor::new(true);

    impl Supervisor {
        pub(crate) const fn new(signals: bool) -> Self {
            Self {
                state: Mutex::new(Lifecycle {
                    cancelled: false,
                    live: Vec::new(),
                }),
                signals,
            }
        }

        /// Runs `start` (the spawn and `Conn::spawn`) under the lifecycle lock. A signal that
        /// arrives meanwhile waits for the lock, so it finds the new process registered.
        pub(super) fn launch<E>(
            &self,
            start: impl FnOnce() -> Result<Arc<Conn>, E>,
        ) -> Result<Result<Arc<Conn>, E>, Cancelled> {
            if self.signals {
                watch(); // before the first spawn
            }
            let mut st = self.state.lock().unwrap();
            if st.cancelled {
                return Err(Cancelled);
            }
            let started = start();
            if let Ok(c) = &started {
                st.live.retain(|w| w.strong_count() > 0);
                st.live.push(Arc::downgrade(c));
            }
            Ok(started)
        }

        /// The run was cancelled: hosts stop warning, since sfor is about to die by the signal
        /// and ripgrep prints nothing on Ctrl-C (review r4, Fable #3).
        pub(crate) fn cancelled(&self) -> bool {
            self.state.lock().unwrap().cancelled
        }

        /// Marks the lifecycle cancelled — every later `launch` is refused — and kills and reaps
        /// every registered process, in parallel. Processes being started hold the lock, so
        /// they finish registering first and are in the list.
        pub(crate) fn cancel(&self) {
            let live: Vec<Arc<Conn>> = {
                let mut st = self.state.lock().unwrap();
                st.cancelled = true;
                st.live.iter().filter_map(Weak::upgrade).collect()
            };
            std::thread::scope(|s| {
                for c in &live {
                    s.spawn(move || c.abort());
                }
            });
        }
    }

    #[cfg(unix)]
    fn watch() {
        // Here, not at the top: Windows builds have no watcher and would warn (Fable r4 #6).
        use std::sync::OnceLock;
        static WATCHER: OnceLock<()> = OnceLock::new();
        WATCHER.get_or_init(|| {
            use signal_hook::consts::{SIGINT, SIGTERM};
            // If the handler cannot be installed, Ctrl-C keeps M1's behaviour.
            let Ok(mut signals) = signal_hook::iterator::Signals::new([SIGINT, SIGTERM]) else {
                return;
            };
            std::thread::spawn(move || {
                if let Some(sig) = signals.forever().next() {
                    GLOBAL.cancel();
                    let _ = signal_hook::low_level::emulate_default_handler(sig);
                    std::process::exit(128 + sig); // not reached: the default action ends sfor
                }
            });
        });
    }

    /// Windows' console Ctrl-C is M5's.
    #[cfg(not(unix))]
    fn watch() {}
}

fn refusal(r: &Reply) -> String {
    match &r.error {
        Some(e) => format!("refused hello: {e}"),
        None => format!("speaks protocol {:?}; sfor speaks {PROTOCOL}", r.protocol),
    }
}

/// The fake launcher, shared with C9's `external.rs` tests (every item is `pub(crate)`).
#[cfg(test)]
pub(crate) mod testing {
    use super::*;
    use crate::helper::protocol::{MatchPos, Reply, Request, line};
    use crate::helper::{Control, HostConfig, Launcher, Mode, Proc};
    use std::io::{BufRead, BufReader, Write};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::SeqCst};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    pub(crate) type Lines = std::io::Lines<BufReader<std::io::PipeReader>>;
    pub(crate) type Body =
        Arc<dyn Fn(&mut Lines, &mut std::io::PipeWriter, &AtomicBool) + Send + Sync>;

    pub(crate) struct Fake {
        pub(crate) body: Body,
        pub(crate) launches: Arc<AtomicUsize>,
        pub(crate) reaps: Arc<AtomicUsize>,
        pub(crate) fail: bool,
        /// Runs at the start of every launch (the barrier tests of D42).
        pub(crate) hook: Option<Arc<dyn Fn() + Send + Sync>>,
        /// How long a kill takes (the barrier tests of D42).
        pub(crate) slow_kill: Duration,
    }

    pub(crate) struct FakeControl {
        pub(crate) killed: Arc<AtomicBool>,
        pub(crate) done: Arc<AtomicBool>,
        pub(crate) reaps: Arc<AtomicUsize>,
        pub(crate) slow_kill: Duration,
    }

    impl Control for FakeControl {
        fn kill(&mut self) {
            std::thread::sleep(self.slow_kill);
            self.killed.store(true, SeqCst);
        }
        fn wait(&mut self, within: Duration) -> bool {
            let t = Instant::now();
            while !self.done.load(SeqCst) {
                if t.elapsed() > within {
                    return false;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            true
        }
        fn reap(&mut self) {
            // A fake "exits" when its body returns; reaping waits for that, as `waitpid` does.
            assert!(
                self.wait(Duration::from_secs(10)),
                "reap of a fake that never ended"
            );
            self.reaps.fetch_add(1, SeqCst);
        }
    }

    impl Launcher for Fake {
        fn launch(&self) -> std::io::Result<Proc> {
            if let Some(h) = &self.hook {
                h();
            }
            if self.fail {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "no such command",
                ));
            }
            self.launches.fetch_add(1, SeqCst);
            let (in_r, in_w) = std::io::pipe()?;
            let (out_r, mut out_w) = std::io::pipe()?;
            let (killed, done) = (
                Arc::new(AtomicBool::new(false)),
                Arc::new(AtomicBool::new(false)),
            );
            let (body, k, d) = (self.body.clone(), killed.clone(), done.clone());
            std::thread::spawn(move || {
                let mut lines = BufReader::new(in_r).lines();
                body(&mut lines, &mut out_w, &k);
                d.store(true, SeqCst);
            });
            let (reaps, slow_kill) = (self.reaps.clone(), self.slow_kill);
            Ok(Proc {
                stdin: Box::new(in_w),
                stdout: Box::new(out_r),
                stderr: None,
                control: Box::new(FakeControl {
                    killed,
                    done,
                    reaps,
                    slow_kill,
                }),
            })
        }
    }

    pub(crate) fn req(l: &str) -> Request {
        serde_json::from_str(l).unwrap()
    }

    /// Answers `hello`, then `answer` for every request until EOF or `None` (a crash).
    pub(crate) fn helper(
        answer: impl Fn(&Request) -> Option<Reply> + Send + Sync + 'static,
    ) -> Body {
        Arc::new(move |lines, out, _| {
            let Some(Ok(h)) = lines.next() else { return };
            let Request::Hello { .. } = req(&h) else {
                return;
            };
            out.write_all(
                line(&Reply {
                    id: 0,
                    protocol: Some(1),
                    ..Default::default()
                })
                .as_bytes(),
            )
            .unwrap();
            for l in lines.map_while(Result::ok) {
                let r = req(&l);
                match answer(&r) {
                    Some(a) => out.write_all(line(&a).as_bytes()).unwrap(),
                    None => return,
                }
            }
        })
    }

    /// One Trail with one Segment named after the path, per Match.
    pub(crate) fn echo(r: &Request) -> Option<Reply> {
        let Request::Trails { id, path, matches } = r else {
            return None;
        };
        let seg = crate::model::Segment {
            kind: "key".into(),
            name: path.clone(),
            line: 1,
            end_line: 1,
            level: None,
            signature: None,
        };
        Some(Reply {
            id: *id,
            trails: Some(vec![vec![seg]; matches.len()]),
            ..Default::default()
        })
    }

    pub(crate) fn cfg(mode: Mode, threads: usize, timeout_ms: u64) -> HostConfig {
        HostConfig {
            format: "ini".into(),
            command: vec!["fake".into()],
            mode,
            timeout: Duration::from_millis(timeout_ms),
            params: toml::Table::new(),
            threads,
        }
    }

    pub(crate) struct Run {
        pub(crate) host: Arc<Host>,
        pub(crate) launches: Arc<AtomicUsize>,
        pub(crate) reaps: Arc<AtomicUsize>,
        pub(crate) warnings: Arc<Mutex<Vec<String>>>,
        pub(crate) sup: &'static cancel::Supervisor,
    }

    pub(crate) fn run(mode: Mode, threads: usize, timeout_ms: u64, body: Body) -> Run {
        let launches = Arc::new(AtomicUsize::new(0));
        let reaps = Arc::new(AtomicUsize::new(0));
        let fake = Fake {
            body,
            launches: launches.clone(),
            reaps: reaps.clone(),
            fail: false,
            hook: None,
            slow_kill: Duration::ZERO,
        };
        run_with(mode, threads, timeout_ms, fake, launches, reaps)
    }

    /// `run` with a Fake built by the test and a Supervisor of the Host's own.
    pub(crate) fn run_with(
        mode: Mode,
        threads: usize,
        timeout_ms: u64,
        fake: Fake,
        launches: Arc<AtomicUsize>,
        reaps: Arc<AtomicUsize>,
    ) -> Run {
        let warnings = Arc::new(Mutex::new(Vec::new()));
        let w = warnings.clone();
        let sup: &'static cancel::Supervisor = Box::leak(Box::new(cancel::Supervisor::new(false)));
        let host = Arc::new(Host::with_supervisor(
            cfg(mode, threads, timeout_ms),
            Box::new(fake),
            Arc::new(move |m| w.lock().unwrap().push(m)),
            sup,
        ));
        Run {
            host,
            launches,
            reaps,
            warnings,
            sup,
        }
    }

    pub(crate) fn trails(h: &Host, path: &str) -> Result<Reply, crate::helper::HelperError> {
        h.request(|id| Request::Trails {
            id,
            path: path.into(),
            matches: vec![MatchPos { line: 1, byte: 0 }],
        })
    }

    pub(crate) fn name_of(r: &Reply) -> &str {
        &r.trails.as_ref().unwrap()[0][0].name
    }
}

#[cfg(test)]
mod tests {
    use super::testing::*;
    use super::*;
    use crate::helper::Mode;
    use crate::helper::protocol::{MatchPos, Reply, Request, line};
    use std::io::Write;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::SeqCst};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};
    #[test]
    fn every_mode_answers_with_the_helpers_trails() {
        for mode in [Mode::Pool, Mode::Concurrent, Mode::Oneshot] {
            let r = run(mode, 2, 2000, helper(echo));
            assert_eq!(
                name_of(&trails(&r.host, "a.ini").unwrap()),
                "a.ini",
                "{mode:?}"
            );
            assert_eq!(
                name_of(&trails(&r.host, "b.ini").unwrap()),
                "b.ini",
                "{mode:?}"
            );
            let expected = if mode == Mode::Oneshot { 2 } else { 1 };
            assert_eq!(r.launches.load(SeqCst), expected, "{mode:?}");
            r.host.shutdown();
        }
    }

    #[test]
    fn a_pool_never_runs_more_copies_than_threads_and_gives_each_one_request() {
        let busy = Arc::new(AtomicUsize::new(0));
        let max = Arc::new(AtomicUsize::new(0));
        let (b, m) = (busy.clone(), max.clone());
        let body = helper(move |r| {
            let now = b.fetch_add(1, SeqCst) + 1;
            m.fetch_max(now, SeqCst);
            std::thread::sleep(Duration::from_millis(30));
            b.fetch_sub(1, SeqCst);
            echo(r)
        });
        let r = run(Mode::Pool, 3, 2000, body);
        std::thread::scope(|s| {
            for i in 0..12 {
                let h = r.host.clone();
                s.spawn(move || assert!(trails(&h, &format!("{i}.ini")).is_ok()));
            }
        });
        assert!(
            r.launches.load(SeqCst) <= 3,
            "{} copies",
            r.launches.load(SeqCst)
        );
        assert!(max.load(SeqCst) <= 3);
        r.host.shutdown();
    }

    #[test]
    fn concurrent_answers_in_any_order_reach_their_request() {
        // Reads four requests, then answers them last to first.
        let body: Body = Arc::new(|lines, out, _| {
            let _ = lines.next();
            out.write_all(
                line(&Reply {
                    id: 0,
                    protocol: Some(1),
                    ..Default::default()
                })
                .as_bytes(),
            )
            .unwrap();
            let mut got = Vec::new();
            for l in lines.map_while(Result::ok) {
                got.push(req(&l));
                if got.len() == 4 {
                    for r in got.drain(..).rev() {
                        out.write_all(line(&echo(&r).unwrap()).as_bytes()).unwrap();
                    }
                }
            }
        });
        let r = run(Mode::Concurrent, 4, 2000, body);
        std::thread::scope(|s| {
            for i in 0..4 {
                let h = r.host.clone();
                s.spawn(move || {
                    let path = format!("{i}.ini");
                    assert_eq!(name_of(&trails(&h, &path).unwrap()), path);
                });
            }
        });
        assert_eq!(r.launches.load(SeqCst), 1);
        r.host.shutdown();
    }

    #[test]
    fn hello_carries_params_and_threads_and_a_wrong_protocol_disables() {
        let seen = Arc::new(Mutex::new(None));
        let s = seen.clone();
        let body: Body = Arc::new(move |lines, out, _| {
            let h = lines.next().unwrap().unwrap();
            *s.lock().unwrap() = Some(req(&h));
            out.write_all(
                line(&Reply {
                    id: 0,
                    protocol: Some(2),
                    ..Default::default()
                })
                .as_bytes(),
            )
            .unwrap();
        });
        let r = run(Mode::Concurrent, 7, 2000, body);
        assert_eq!(
            trails(&r.host, "a.ini"),
            Err(crate::helper::HelperError::Disabled)
        );
        assert_eq!(
            trails(&r.host, "b.ini"),
            Err(crate::helper::HelperError::Disabled)
        );
        let Some(Request::Hello {
            threads, format, ..
        }) = seen.lock().unwrap().clone()
        else {
            panic!()
        };
        assert_eq!((threads, format.as_str()), (7, "ini"));
        let w = r.warnings.lock().unwrap();
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(w[0].contains("ini") && w[0].contains("protocol"), "{w:?}");
    }

    #[test]
    fn an_error_answer_fails_only_its_request() {
        let r = run(
            Mode::Pool,
            1,
            2000,
            helper(|r| match r {
                Request::Trails { id, path, .. } if path == "bad.ini" => Some(Reply {
                    id: *id,
                    error: Some("cannot parse".into()),
                    ..Default::default()
                }),
                other => echo(other),
            }),
        );
        assert_eq!(
            trails(&r.host, "bad.ini"),
            Err(crate::helper::HelperError::Answer("cannot parse".into()))
        );
        assert!(trails(&r.host, "good.ini").is_ok());
        assert_eq!(r.launches.load(SeqCst), 1);
    }

    #[test]
    fn a_crash_fails_its_request_and_the_next_one_starts_a_new_copy() {
        let r = run(
            Mode::Pool,
            1,
            2000,
            helper(|r| match r {
                Request::Trails { path, .. } if path == "crash.ini" => None,
                other => echo(other),
            }),
        );
        let t = Instant::now();
        assert!(matches!(
            trails(&r.host, "crash.ini"),
            Err(crate::helper::HelperError::Crashed(_))
        ));
        assert!(trails(&r.host, "next.ini").is_ok());
        assert_eq!(r.launches.load(SeqCst), 2);
        // The crash is seen at once, not at the timeout.
        assert!(
            t.elapsed() < Duration::from_millis(1000),
            "{:?}",
            t.elapsed()
        );
    }

    #[test]
    fn three_failures_disable_the_definition_with_one_warning() {
        let r = run(Mode::Pool, 1, 2000, helper(|_| None));
        for _ in 0..3 {
            assert!(matches!(
                trails(&r.host, "x.ini"),
                Err(crate::helper::HelperError::Crashed(_))
            ));
        }
        assert_eq!(
            trails(&r.host, "x.ini"),
            Err(crate::helper::HelperError::Disabled)
        );
        assert_eq!(r.launches.load(SeqCst), 3);
        assert_eq!(r.warnings.lock().unwrap().len(), 1);
    }

    #[test]
    fn a_timeout_fails_the_request_and_kills_the_copy() {
        let body: Body = Arc::new(|lines, out, killed| {
            let _ = lines.next();
            out.write_all(
                line(&Reply {
                    id: 0,
                    protocol: Some(1),
                    ..Default::default()
                })
                .as_bytes(),
            )
            .unwrap();
            let _ = lines.next();
            while !killed.load(SeqCst) {
                std::thread::sleep(Duration::from_millis(5));
            }
        });
        let r = run(Mode::Pool, 1, 100, body);
        let t = Instant::now();
        assert_eq!(
            trails(&r.host, "slow.ini"),
            Err(crate::helper::HelperError::Timeout(100))
        );
        assert!(
            t.elapsed() < Duration::from_millis(1500),
            "{:?}",
            t.elapsed()
        );
        // The killed copy is gone: the next request starts another.
        let _ = trails(&r.host, "again.ini");
        assert_eq!(r.launches.load(SeqCst), 2);
    }

    #[test]
    fn a_line_that_is_not_json_fails_and_restarts() {
        let body: Body = Arc::new(|lines, out, _| {
            let _ = lines.next();
            out.write_all(
                line(&Reply {
                    id: 0,
                    protocol: Some(1),
                    ..Default::default()
                })
                .as_bytes(),
            )
            .unwrap();
            for l in lines.map_while(Result::ok) {
                if l.contains("garbage.ini") {
                    out.write_all(b"this is not json\n").unwrap();
                } else {
                    out.write_all(line(&echo(&req(&l)).unwrap()).as_bytes())
                        .unwrap();
                }
            }
        });
        let r = run(Mode::Pool, 1, 2000, body);
        assert!(
            matches!(trails(&r.host, "garbage.ini"), Err(crate::helper::HelperError::Crashed(m)) if m.contains("not JSON"))
        );
        assert!(trails(&r.host, "fine.ini").is_ok());
    }

    #[test]
    fn a_command_that_cannot_start_disables_with_one_warning() {
        let warnings = Arc::new(Mutex::new(Vec::new()));
        let w = warnings.clone();
        let fake = Fake {
            body: helper(echo),
            launches: Arc::new(AtomicUsize::new(0)),
            reaps: Arc::new(AtomicUsize::new(0)),
            fail: true,
            hook: None,
            slow_kill: Duration::ZERO,
        };
        // A Supervisor of its own: `Host::new` would use GLOBAL, which installs the signal
        // watcher in the test binary (Fable r4 #5).
        let sup: &'static cancel::Supervisor = Box::leak(Box::new(cancel::Supervisor::new(false)));
        let host = Host::with_supervisor(
            cfg(Mode::Pool, 4, 2000),
            Box::new(fake),
            Arc::new(move |m| w.lock().unwrap().push(m)),
            sup,
        );
        for _ in 0..5 {
            assert_eq!(
                trails(&host, "a.ini"),
                Err(crate::helper::HelperError::Disabled)
            );
        }
        let w = warnings.lock().unwrap();
        assert_eq!(w.len(), 1);
        assert!(w[0].contains("cannot start"), "{w:?}");
    }

    #[test]
    fn shutdown_closes_stdin_so_the_helper_exits() {
        let exited = Arc::new(AtomicBool::new(false));
        let e = exited.clone();
        let inner = helper(echo);
        let body: Body = Arc::new(move |lines, out, k| {
            inner(lines, out, k);
            e.store(true, SeqCst);
        });
        let r = run(Mode::Pool, 1, 2000, body);
        assert!(trails(&r.host, "a.ini").is_ok());
        let t = Instant::now();
        r.host.shutdown();
        assert!(exited.load(SeqCst));
        // It exited on the EOF: no grace second, no kill.
        assert!(
            t.elapsed() < Duration::from_millis(500),
            "{:?}",
            t.elapsed()
        );
        assert_eq!(
            trails(&r.host, "b.ini"),
            Err(crate::helper::HelperError::Disabled)
        );
    }

    /// Answers `hello` and then never reads again, until killed (review r1, Codex #1).
    fn deaf() -> Body {
        Arc::new(|lines, out, killed| {
            let _ = lines.next();
            out.write_all(
                line(&Reply {
                    id: 0,
                    protocol: Some(1),
                    ..Default::default()
                })
                .as_bytes(),
            )
            .unwrap();
            while !killed.load(SeqCst) {
                std::thread::sleep(Duration::from_millis(5));
            }
        })
    }

    #[test]
    fn a_request_larger_than_the_pipe_times_out_when_the_helper_does_not_read() {
        let r = run(Mode::Pool, 1, 200, deaf());
        let matches = vec![MatchPos { line: 1, byte: 0 }; 300_000]; // ~8 MB of JSON
        let t = Instant::now();
        let got = r.host.request(|id| Request::Trails {
            id,
            path: "big.ini".into(),
            matches,
        });
        assert_eq!(got, Err(crate::helper::HelperError::Timeout(200)));
        assert!(t.elapsed() < Duration::from_secs(3), "{:?}", t.elapsed());
        r.host.shutdown();
        assert_eq!(r.reaps.load(SeqCst), r.launches.load(SeqCst));
    }

    #[test]
    fn every_process_is_reaped() {
        let r = run(
            Mode::Pool,
            1,
            2000,
            helper(|r| match r {
                Request::Trails { path, .. } if path == "crash.ini" => None,
                other => echo(other),
            }),
        );
        let _ = trails(&r.host, "crash.ini");
        assert!(trails(&r.host, "a.ini").is_ok());
        // The crashed copy was reaped when it failed, before the run ends (D32).
        assert_eq!(r.reaps.load(SeqCst), 1);
        r.host.shutdown();
        assert_eq!(r.reaps.load(SeqCst), 2);
    }

    #[test]
    fn a_descendant_holding_the_pipes_cannot_hold_the_run() {
        // The helper hands a copy of its stdout to a "child" that outlives it (D32).
        let inner = helper(echo);
        let body: Body = Arc::new(move |lines, out, k| {
            let child = out.try_clone().unwrap();
            std::thread::spawn(move || {
                let _keep = child;
                std::thread::sleep(Duration::from_secs(10));
            });
            inner(lines, out, k);
        });
        let r = run(Mode::Pool, 1, 2000, body);
        assert!(trails(&r.host, "a.ini").is_ok());
        let t = Instant::now();
        r.host.shutdown();
        assert!(t.elapsed() < Duration::from_secs(3), "{:?}", t.elapsed());
    }

    #[test]
    fn concurrent_never_exceeds_its_capacity() {
        // Answers each request 50 ms after reading it, from its own thread.
        let outstanding = Arc::new(AtomicUsize::new(0));
        let max = Arc::new(AtomicUsize::new(0));
        let (o, m) = (outstanding.clone(), max.clone());
        let body: Body = Arc::new(move |lines, out, _| {
            let _ = lines.next();
            out.write_all(
                line(&Reply {
                    id: 0,
                    protocol: Some(1),
                    ..Default::default()
                })
                .as_bytes(),
            )
            .unwrap();
            let out = Arc::new(Mutex::new(out.try_clone().unwrap()));
            std::thread::scope(|s| {
                for l in lines.map_while(Result::ok) {
                    let now = o.fetch_add(1, SeqCst) + 1;
                    m.fetch_max(now, SeqCst);
                    let (out, o) = (out.clone(), o.clone());
                    s.spawn(move || {
                        std::thread::sleep(Duration::from_millis(50));
                        o.fetch_sub(1, SeqCst);
                        out.lock()
                            .unwrap()
                            .write_all(line(&echo(&req(&l)).unwrap()).as_bytes())
                            .unwrap();
                    });
                }
            });
        });
        let r = run(Mode::Concurrent, 2, 2000, body);
        std::thread::scope(|s| {
            for i in 0..6 {
                let h = r.host.clone();
                s.spawn(move || assert!(trails(&h, &format!("{i}.ini")).is_ok()));
            }
        });
        assert!(max.load(SeqCst) <= 2, "{} in flight", max.load(SeqCst));
        r.host.shutdown();
    }

    #[test]
    fn a_line_without_an_id_fails_every_pending_request() {
        // D41: an unattributable line breaks the process.
        let body: Body = Arc::new(|lines, out, _| {
            let _ = lines.next();
            out.write_all(
                line(&Reply {
                    id: 0,
                    protocol: Some(1),
                    ..Default::default()
                })
                .as_bytes(),
            )
            .unwrap();
            let mut n = 0;
            for _ in lines.map_while(Result::ok) {
                n += 1;
                if n == 3 {
                    out.write_all(b"{\"oops\":1}\n").unwrap();
                }
            }
        });
        let r = run(Mode::Concurrent, 3, 2000, body);
        std::thread::scope(|s| {
            for i in 0..3 {
                let h = r.host.clone();
                s.spawn(move || {
                    let got = trails(&h, &format!("{i}.ini"));
                    assert!(matches!(&got, Err(crate::helper::HelperError::Crashed(m)) if m.contains("without an id")), "{got:?}");
                });
            }
        });
    }

    #[test]
    fn a_malformed_answer_with_its_id_fails_only_that_request() {
        // D41: valid JSON with the right id, but `trails` is not a list of Trails.
        let body: Body = Arc::new(|lines, out, _| {
            let _ = lines.next();
            out.write_all(
                line(&Reply {
                    id: 0,
                    protocol: Some(1),
                    ..Default::default()
                })
                .as_bytes(),
            )
            .unwrap();
            for l in lines.map_while(Result::ok) {
                match req(&l) {
                    Request::Trails { id, path, .. } if path == "odd.ini" => out
                        .write_all(format!("{{\"id\":{id},\"trails\":\"nope\"}}\n").as_bytes())
                        .unwrap(),
                    other => out
                        .write_all(line(&echo(&other).unwrap()).as_bytes())
                        .unwrap(),
                }
            }
        });
        let r = run(Mode::Pool, 1, 2000, body);
        assert!(matches!(
            trails(&r.host, "odd.ini"),
            Err(crate::helper::HelperError::Protocol(_))
        ));
        assert!(trails(&r.host, "next.ini").is_ok());
        assert_eq!(r.launches.load(SeqCst), 1);
    }

    #[test]
    fn oneshot_disables_a_helper_that_never_answers_hello() {
        // Review r2, Codex #5: the handshake policy of the other modes, in oneshot too.
        let body: Body = Arc::new(|lines, _out, killed| {
            for _ in lines.map_while(Result::ok) {}
            while !killed.load(SeqCst) {
                std::thread::sleep(Duration::from_millis(5));
            }
        });
        let r = run(Mode::Oneshot, 1, 100, body);
        assert_eq!(
            trails(&r.host, "a.ini"),
            Err(crate::helper::HelperError::Disabled)
        );
        assert_eq!(
            trails(&r.host, "b.ini"),
            Err(crate::helper::HelperError::Disabled)
        );
        assert_eq!(r.launches.load(SeqCst), 1);
        assert_eq!(r.reaps.load(SeqCst), 1);
        assert_eq!(r.warnings.lock().unwrap().len(), 1);
    }

    #[test]
    fn oneshot_disables_a_helper_that_exits_or_garbles_before_hello() {
        // Review m2-code-r1, Codex #5: spec §4.4 disables the Definition at the first process
        // that exits (or writes a broken line) before answering `hello`, with one warning.
        let exits: Body = Arc::new(|_lines, _out, _| {});
        let garbles: Body = Arc::new(|_lines, out, _| {
            out.write_all(b"not json\n").unwrap();
        });
        for body in [exits, garbles] {
            let r = run(Mode::Oneshot, 1, 2000, body);
            for f in ["a.ini", "b.ini", "c.ini", "d.ini"] {
                assert_eq!(
                    trails(&r.host, f),
                    Err(crate::helper::HelperError::Disabled),
                    "{f}"
                );
            }
            assert_eq!(r.launches.load(SeqCst), 1);
            let w = r.warnings.lock().unwrap();
            assert_eq!(w.len(), 1, "{w:?}");
            assert!(w[0].contains("before answering hello"), "{w:?}");
        }
    }

    #[test]
    fn shutdown_closes_every_copy_at_once() {
        // Review m2-code-r1, Fable #1: four copies that ignore the EOF of stdin get their
        // second of grace together, not one after another (spec §4.4, Lifetime).
        let deaf: Body = Arc::new(|lines, out, killed| {
            let _ = lines.next();
            out.write_all(
                line(&Reply {
                    id: 0,
                    protocol: Some(1),
                    ..Default::default()
                })
                .as_bytes(),
            )
            .unwrap();
            for l in lines.map_while(Result::ok) {
                std::thread::sleep(Duration::from_millis(100));
                out.write_all(line(&echo(&req(&l)).unwrap()).as_bytes())
                    .unwrap();
            }
            while !killed.load(SeqCst) {
                std::thread::sleep(Duration::from_millis(5));
            }
        });
        let r = run(Mode::Pool, 4, 2000, deaf);
        std::thread::scope(|s| {
            for i in 0..4 {
                let h = r.host.clone();
                s.spawn(move || assert!(trails(&h, &format!("{i}.ini")).is_ok()));
            }
        });
        assert_eq!(r.launches.load(SeqCst), 4);
        let t = Instant::now();
        r.host.shutdown();
        let took = t.elapsed();
        assert_eq!(r.reaps.load(SeqCst), 4);
        assert!(took >= SHUTDOWN_WAIT, "{took:?}: no grace");
        assert!(
            took < SHUTDOWN_WAIT + Duration::from_millis(500),
            "{took:?} for four copies: one grace each"
        );
    }

    #[test]
    fn the_timeout_counts_from_the_queue_not_from_the_start_of_the_copy() {
        // Review m2-code-r1, Fable #4: spec §4.4 counts `timeout_ms` from when sfor queues the
        // request. A copy that takes 350 ms to answer `hello` and 300 ms per request answers
        // within a 500 ms timeout; counted from before the start it would take 650 ms.
        let body: Body = Arc::new(|lines, out, _| {
            let _ = lines.next();
            std::thread::sleep(Duration::from_millis(350));
            out.write_all(
                line(&Reply {
                    id: 0,
                    protocol: Some(1),
                    ..Default::default()
                })
                .as_bytes(),
            )
            .unwrap();
            for l in lines.map_while(Result::ok) {
                std::thread::sleep(Duration::from_millis(300));
                out.write_all(line(&echo(&req(&l)).unwrap()).as_bytes())
                    .unwrap();
            }
        });
        for mode in [Mode::Pool, Mode::Concurrent] {
            let r = run(mode, 1, 500, body.clone());
            let t = Instant::now();
            assert_eq!(
                name_of(&trails(&r.host, "a.ini").unwrap()),
                "a.ini",
                "{mode:?}"
            );
            let took = t.elapsed();
            assert!(took >= Duration::from_millis(650), "{mode:?}: {took:?}");
            assert!(took < Duration::from_millis(1000), "{mode:?}: {took:?}");
            r.host.shutdown();
        }
    }

    #[test]
    fn a_copy_that_exits_between_requests_is_not_a_failure() {
        // Review r2, Fable #7: each copy answers one request and exits with nothing in
        // flight; five such exits never reach the three-failure limit.
        let body: Body = Arc::new(|lines, out, _| {
            let _ = lines.next();
            out.write_all(
                line(&Reply {
                    id: 0,
                    protocol: Some(1),
                    ..Default::default()
                })
                .as_bytes(),
            )
            .unwrap();
            if let Some(Ok(l)) = lines.next() {
                out.write_all(line(&echo(&req(&l)).unwrap()).as_bytes())
                    .unwrap();
            }
        });
        let r = run(Mode::Pool, 1, 2000, body);
        for i in 0..5 {
            assert!(trails(&r.host, &format!("{i}.ini")).is_ok(), "request {i}");
            std::thread::sleep(Duration::from_millis(50));
        }
        assert_eq!(r.launches.load(SeqCst), 5);
        assert!(r.warnings.lock().unwrap().is_empty());
        // Codex r5 #1: the last copy exited after its answer and is reaped by its reader, with
        // no further request and no shutdown.
        let t = Instant::now();
        while r.reaps.load(SeqCst) < 5 {
            assert!(
                t.elapsed() < Duration::from_secs(2),
                "{} of 5 reaped",
                r.reaps.load(SeqCst)
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn cancel_kills_and_reaps_every_live_process_and_refuses_new_ones() {
        // D42: what the signal watcher does, on the Host's own Supervisor.
        let r = run(Mode::Pool, 2, 2000, deaf());
        std::thread::scope(|s| {
            for i in 0..2 {
                let h = r.host.clone();
                s.spawn(move || {
                    let got = h.request(|id| Request::Trails {
                        id,
                        path: format!("{i}.ini"),
                        matches: vec![],
                    });
                    assert_eq!(got, Err(crate::helper::HelperError::Disabled));
                });
            }
            std::thread::sleep(Duration::from_millis(100));
            r.sup.cancel();
        });
        assert_eq!(r.reaps.load(SeqCst), r.launches.load(SeqCst));
        let before = r.launches.load(SeqCst);
        assert_eq!(
            trails(&r.host, "late.ini"),
            Err(crate::helper::HelperError::Disabled)
        );
        assert_eq!(r.launches.load(SeqCst), before);
        // Nothing is printed while sfor is about to die by the signal (Fable r4 #3).
        assert!(
            r.warnings.lock().unwrap().is_empty(),
            "{:?}",
            r.warnings.lock().unwrap()
        );
    }

    #[test]
    fn a_signal_during_the_first_launch_still_reaps_it() {
        // Codex r3 #1: the cancellation arrives while the first process is being spawned.
        let inside = Arc::new(std::sync::Barrier::new(2));
        let go = Arc::new(std::sync::Barrier::new(2));
        let (i2, g2) = (inside.clone(), go.clone());
        let (launches, reaps) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
        let fake = Fake {
            body: deaf(),
            launches: launches.clone(),
            reaps: reaps.clone(),
            fail: false,
            hook: Some(Arc::new(move || {
                i2.wait();
                g2.wait();
            })),
            slow_kill: Duration::ZERO,
        };
        let r = run_with(Mode::Pool, 1, 2000, fake, launches, reaps);
        std::thread::scope(|s| {
            let h = r.host.clone();
            s.spawn(move || {
                let _ = trails(&h, "a.ini");
            });
            inside.wait(); // the spawn has begun, under the Supervisor's lock
            let sup = r.sup;
            let canceller = s.spawn(move || sup.cancel());
            std::thread::sleep(Duration::from_millis(50)); // the cancel is waiting for the lock
            go.wait();
            canceller.join().unwrap();
        });
        assert_eq!(r.launches.load(SeqCst), 1);
        assert_eq!(r.reaps.load(SeqCst), 1);
    }

    #[test]
    fn a_launch_during_the_cleanup_is_refused() {
        // Codex r3 #1: while the kills run, a worker needing a new copy gets none.
        let (launches, reaps) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
        let fake = Fake {
            body: deaf(),
            launches: launches.clone(),
            reaps: reaps.clone(),
            fail: false,
            hook: None,
            slow_kill: Duration::from_millis(300),
        };
        let r = run_with(Mode::Pool, 2, 5000, fake, launches, reaps);
        std::thread::scope(|s| {
            let h = r.host.clone();
            s.spawn(move || {
                let _ = trails(&h, "busy.ini"); // copy 1, busy until killed
            });
            std::thread::sleep(Duration::from_millis(100));
            let sup = r.sup;
            s.spawn(move || sup.cancel()); // its kill takes 300 ms
            std::thread::sleep(Duration::from_millis(100));
            // `abort` marked copy 1 dead before its slow kill, so `checkout` closes it (waiting
            // for the kill) and asks for a new copy, which the cancelled Supervisor refuses.
            assert_eq!(
                trails(&r.host, "second.ini"),
                Err(crate::helper::HelperError::Disabled)
            );
        });
        assert_eq!(r.launches.load(SeqCst), 1);
        assert_eq!(r.reaps.load(SeqCst), 1);
    }

    #[test]
    fn oneshot_works_with_a_helper_that_reads_to_eof_first() {
        // Fable #1 / D40: hello and the request arrive before EOF; both answers come after it.
        let body: Body = Arc::new(|lines, out, _| {
            let all: Vec<Request> = lines.map_while(Result::ok).map(|l| req(&l)).collect();
            assert_eq!(all.len(), 2);
            out.write_all(
                line(&Reply {
                    id: 0,
                    protocol: Some(1),
                    ..Default::default()
                })
                .as_bytes(),
            )
            .unwrap();
            out.write_all(line(&echo(&all[1]).unwrap()).as_bytes())
                .unwrap();
        });
        let r = run(Mode::Oneshot, 1, 2000, body);
        assert_eq!(name_of(&trails(&r.host, "a.ini").unwrap()), "a.ini");
        assert_eq!(r.reaps.load(SeqCst), 1);
    }

    /// A Fake whose process has a stderr too, with `oops` written on it.
    struct Noisy(Fake);

    impl crate::helper::Launcher for Noisy {
        fn launch(&self) -> std::io::Result<crate::helper::Proc> {
            let mut p = self.0.launch()?;
            let (err_r, mut err_w) = std::io::pipe()?;
            err_w.write_all(b"oops\n")?;
            p.stderr = Some(Box::new(err_r));
            Ok(p)
        }
    }

    #[test]
    fn helper_stderr_lines_are_relayed_as_warnings() {
        let warnings = Arc::new(Mutex::new(Vec::new()));
        let w = warnings.clone();
        let fake = Fake {
            body: helper(echo),
            launches: Arc::new(AtomicUsize::new(0)),
            reaps: Arc::new(AtomicUsize::new(0)),
            fail: false,
            hook: None,
            slow_kill: Duration::ZERO,
        };
        let sup: &'static cancel::Supervisor = Box::leak(Box::new(cancel::Supervisor::new(false)));
        let host = Host::with_supervisor(
            cfg(Mode::Pool, 1, 2000),
            Box::new(Noisy(fake)),
            Arc::new(move |m| w.lock().unwrap().push(m)),
            sup,
        );
        assert!(trails(&host, "a.ini").is_ok());
        // The shutdown waits for the pipe threads, the stderr relay included.
        host.shutdown();
        assert_eq!(*warnings.lock().unwrap(), ["sfor: ini: oops"]);
    }
}
