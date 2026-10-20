//! End-to-end tests of how `s4 --mcp` ends while a search runs (code review r1 of M5a: Codex
//! #1, Fable #4; M5b plan, Task 6.2): a signal to the server, a server that dies, or a client
//! that goes away, leaves no search behind. The `linux` module's searches read a FIFO no one
//! writes; the native cases, on every system, hold a Helper Processor of a regular file.
mod common;

/// The Linux cases of M5a: a FIFO, signals and /proc.
#[cfg(target_os = "linux")]
mod linux {
    use super::common::Fixture;
    use serde_json::json;
    use std::io::{Read, Write};
    use std::os::unix::process::ExitStatusExt;
    use std::process::{Child, Stdio};
    use std::time::{Duration, Instant};

    /// How long the server may take to end once told to; it kills one child and reaps it.
    const PROMPT: Duration = Duration::from_secs(3);

    /// The server, with a `search` call over a FIFO no one writes sent to it, and its child, the
    /// search: found by its parent pid once it started.
    struct Running {
        server: Child,
        search: libc::pid_t,
    }

    impl Drop for Running {
        fn drop(&mut self) {
            // A test that failed leaves no process behind.
            let _ = self.server.kill();
            let _ = self.server.wait();
            if alive(self.search) {
                // SAFETY: kill has no memory effects.
                unsafe { libc::kill(self.search, libc::SIGKILL) };
            }
        }
    }

    fn start(fx: &Fixture) -> Running {
        let fifo = fx.file("fifo");
        let c = std::ffi::CString::new(fifo.to_str().unwrap()).unwrap();
        // SAFETY: a valid C string.
        assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
        let mut c = fx.s4(["--mcp"]);
        c.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // The server watches the signals it was not started ignoring: a test runner in the
        // background of a shell ignores SIGINT, and would pass that on.
        // SAFETY: signal is async-signal-safe, and nothing else runs in the child before exec.
        unsafe {
            use std::os::unix::process::CommandExt;
            c.pre_exec(|| {
                libc::signal(libc::SIGINT, libc::SIG_DFL);
                libc::signal(libc::SIGTERM, libc::SIG_DFL);
                Ok(())
            });
        }
        // Held by a `Running` at once, so that a failed step below still ends it.
        let mut r = Running {
            server: c.spawn().unwrap(),
            search: 0,
        };
        let call = json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"search",
            "arguments":{"pattern":"needle","paths":["fifo"]}}});
        let input = r.server.stdin.as_mut().unwrap();
        writeln!(input, "{call}").unwrap();
        input.flush().unwrap();
        let pid = r.server.id() as libc::pid_t;
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if let Some(&search) = children(pid).first() {
                r.search = search;
                return r;
            }
            assert!(Instant::now() < deadline, "the search did not start");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// The processes whose parent is `pid`.
    fn children(pid: libc::pid_t) -> Vec<libc::pid_t> {
        let mut found = Vec::new();
        for e in std::fs::read_dir("/proc").unwrap().flatten() {
            let Some(p) = e.file_name().to_str().and_then(|n| n.parse().ok()) else {
                continue;
            };
            if parent(p) == Some(pid) {
                found.push(p);
            }
        }
        found
    }

    /// The state and the parent pid in `/proc/PID/stat`, after the command's parenthesis.
    fn stat(pid: libc::pid_t) -> Option<(char, libc::pid_t)> {
        let s = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        let mut rest = s[s.rfind(')')? + 1..].split_whitespace();
        let state = rest.next()?.chars().next()?;
        Some((state, rest.next()?.parse().ok()?))
    }

    fn parent(pid: libc::pid_t) -> Option<libc::pid_t> {
        stat(pid).map(|(_, ppid)| ppid)
    }

    /// Whether `pid` runs: neither gone nor a zombie.
    fn alive(pid: libc::pid_t) -> bool {
        stat(pid).is_some_and(|(state, _)| state != 'Z')
    }

    /// Waits for the server to exit, at most `PROMPT`: how it ended and how long it took.
    fn ended(r: &mut Running) -> (std::process::ExitStatus, Duration) {
        let t = Instant::now();
        loop {
            if let Some(status) = r.server.try_wait().unwrap() {
                return (status, t.elapsed());
            }
            assert!(
                t.elapsed() < PROMPT,
                "the server did not end within {PROMPT:?}"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// Code review r1 of M5a, Codex #1: a SIGTERM or SIGINT to the server ends its search, which
    /// it kills and reaps before dying by the signal, as rg (143, 130 in a shell).
    #[test]
    fn a_signal_to_the_server_ends_its_search() {
        for sig in [libc::SIGTERM, libc::SIGINT] {
            let fx = Fixture::new();
            let mut r = start(&fx);
            // SAFETY: kill has no memory effects.
            assert_eq!(unsafe { libc::kill(r.server.id() as libc::pid_t, sig) }, 0);
            let (status, took) = ended(&mut r);
            assert_eq!(status.signal(), Some(sig), "{sig}: {status}");
            assert!(took < PROMPT, "{sig}: {took:?}");
            // Reaped by the server, before it died: not even a zombie is left.
            assert!(stat(r.search).is_none(), "{sig}: {:?}", stat(r.search));
        }
    }

    /// Code review r1 of M5a, Codex #1: a signal the server does not handle, SIGHUP or SIGKILL,
    /// still ends its search: the kernel kills it with its parent (`PR_SET_PDEATHSIG`).
    #[test]
    fn a_search_dies_with_its_server() {
        for sig in [libc::SIGHUP, libc::SIGKILL] {
            let fx = Fixture::new();
            let mut r = start(&fx);
            // SAFETY: kill has no memory effects.
            assert_eq!(unsafe { libc::kill(r.server.id() as libc::pid_t, sig) }, 0);
            let (status, _) = ended(&mut r);
            assert_eq!(status.signal(), Some(sig), "{sig}: {status}");
            let t = Instant::now();
            while alive(r.search) {
                assert!(
                    t.elapsed() < PROMPT,
                    "{sig}: the search outlived its server: {:?}",
                    stat(r.search)
                );
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }

    /// Code review r1 of M5a, Fable #4: a client that closes the server's stdin and stdout while a
    /// search runs ends it: the server stops the search, and exits with 0 and no message, as
    /// nothing is left to answer.
    #[test]
    fn a_client_that_goes_away_ends_the_search() {
        let fx = Fixture::new();
        let mut r = start(&fx);
        drop(r.server.stdin.take());
        drop(r.server.stdout.take());
        let mut stderr = r.server.stderr.take().unwrap();
        let (status, took) = ended(&mut r);
        let mut message = String::new();
        stderr.read_to_string(&mut message).unwrap();
        assert_eq!((status.code(), message.as_str()), (Some(0), ""), "{status}");
        assert!(took < PROMPT, "{took:?}");
        assert!(stat(r.search).is_none(), "{:?}", stat(r.search));
    }
}

/// M5b plan, Task 6.2: on every system, a client that sends one call and closes its input
/// gets the answer, and the server then exits with 0.
#[test]
fn m5b_mcp_transcript_finishes_after_client_input_eof() {
    use std::io::Write;
    let fx = common::Fixture::new();
    let mut c = fx.s4(["--mcp"]);
    c.stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = c.spawn().unwrap();
    let call = serde_json::json!({"jsonrpc":"2.0","id":7,"method":"tools/call",
        "params":{"name":"search","arguments":{
            "pattern":"needle-pkg","paths":["md/guide.md"]}}});
    writeln!(child.stdin.take().unwrap(), "{call}").unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["id"], 7);
    let found = &v["result"]["structuredContent"]["matches"];
    assert_eq!(found.as_array().unwrap().len(), 1);
    assert_eq!(found[0]["line"], 12);
    assert_eq!(
        found[0]["trail"].as_array().unwrap().last().unwrap(),
        "bash"
    );
}

/// A server whose one search is held by the example helper's wait knobs (M5b plan, Task 2.3)
/// on a regular `.ini` file, and the pids of that search and of its helper, which the helper
/// logs once it holds the call. Dropping it releases the helper and kills the server.
struct NativeRun {
    server: std::process::Child,
    search: u32,
    helper: u32,
    _fx: common::Fixture,
    release: std::path::PathBuf,
}

/// Whether `pid` runs: on Linux neither gone nor a zombie; elsewhere as the system reports it.
fn m5b_alive(pid: u32) -> bool {
    #[cfg(target_os = "linux")]
    {
        let text = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap_or_default();
        text.rfind(')')
            .and_then(|n| text[n + 1..].split_whitespace().next())
            .is_some_and(|s| s != "Z")
    }
    // Observation only: the search is the server's child and is reaped by it.
    #[cfg(all(unix, not(target_os = "linux")))]
    {
        // SAFETY: kill with signal 0 only checks that the process exists.
        unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::{
            Foundation::CloseHandle,
            System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject},
        };
        // SAFETY: the handle is checked before use and closed after it.
        unsafe {
            let h = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
            if h.is_null() {
                return false;
            }
            // WAIT_TIMEOUT: the process has not ended.
            let running = WaitForSingleObject(h, 0) == 258;
            CloseHandle(h);
            running
        }
    }
}

impl NativeRun {
    fn start(finite: bool) -> Self {
        use std::{
            io::Write,
            process::Stdio,
            time::{Duration, Instant},
        };
        let fx = common::Fixture::new();
        std::fs::write(fx.file("blocked.ini"), "[section]\nk=needle\n").unwrap();
        let bin =
            common::workspace_bin("searchfor-example-helper", "searchfor-example-helper", &[]);
        let def = format!(
            "name='blocked'\nextensions=['ini']\nprocessor='external'\n[params]\ncommand=[{:?}]\nmode='pool'\ntimeout_ms=30000\n",
            bin.to_str().unwrap()
        );
        std::fs::write(fx.formats.path().join("blocked.toml"), def).unwrap();
        let ready = fx.file("ready.json");
        let release = fx.file("release");
        if finite {
            std::fs::write(&release, b"go").unwrap();
        }
        let mut command = fx.s4(["--mcp"]);
        command
            .env("SEARCHFOR_EXAMPLE_WAIT_READY", &ready)
            .env("SEARCHFOR_EXAMPLE_WAIT_RELEASE", &release)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut server = command.spawn().unwrap();
        let call = serde_json::json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
            "params":{"name":"search","arguments":{"pattern":"needle","paths":["blocked.ini"]}}});
        writeln!(server.stdin.as_mut().unwrap(), "{call}").unwrap();
        let deadline = Instant::now() + Duration::from_secs(20);
        let v: serde_json::Value = loop {
            if let Ok(bytes) = std::fs::read(&ready)
                && let Ok(v) = serde_json::from_slice(&bytes)
            {
                break v;
            }
            if Instant::now() >= deadline {
                let _ = server.kill();
                let _ = server.wait();
                panic!("MCP helper did not report readiness");
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        Self {
            server,
            search: v["search_pid"].as_u64().unwrap() as u32,
            helper: v["helper_pid"].as_u64().unwrap() as u32,
            _fx: fx,
            release,
        }
    }
    fn blocked_search() -> Self {
        Self::start(false)
    }
    fn finite_search() -> Self {
        Self::start(true)
    }
    fn close_client_output(&mut self) {
        drop(self.server.stdout.take());
    }
    fn close_client_input(&mut self) {
        drop(self.server.stdin.take());
    }
    fn kill_server(&mut self) {
        self.server.kill().unwrap();
        self.server.wait().unwrap();
    }
    /// Whether the search and its helper both ended within `within`; the server is reaped
    /// meanwhile when it exits.
    fn wait_search_dead(&mut self, within: std::time::Duration) -> bool {
        let start = std::time::Instant::now();
        loop {
            let _ = self.server.try_wait();
            if !m5b_alive(self.search) && !m5b_alive(self.helper) {
                return true;
            }
            if start.elapsed() >= within {
                return false;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
}

impl Drop for NativeRun {
    fn drop(&mut self) {
        let _ = std::fs::write(&self.release, b"cleanup");
        let _ = self.server.kill();
        let _ = self.server.wait();
    }
}

/// A client that closes the server's stdout while a search is blocked stops that search.
#[test]
fn m5b_client_output_disconnect_stops_a_blocked_search() {
    let mut r = NativeRun::blocked_search();
    r.close_client_output();
    assert!(r.wait_search_dead(std::time::Duration::from_secs(10)));
}

/// A server that dies, killed outright, takes its blocked search and helper with it.
#[test]
fn m5b_server_death_stops_a_blocked_search() {
    let mut r = NativeRun::blocked_search();
    r.kill_server();
    assert!(r.wait_search_dead(std::time::Duration::from_secs(10)));
}

/// A client that closes its input after a finite call leaves no search behind.
#[test]
fn m5b_client_input_eof_leaves_no_search() {
    let mut r = NativeRun::finite_search();
    r.close_client_input();
    // This fixture is finite: queued requests complete after a half-close.
    assert!(r.wait_search_dead(std::time::Duration::from_secs(10)));
}
