//! End-to-end tests of how `s4 --mcp` ends while a search runs (code review r1 of M5a: Codex
//! #1, Fable #4): a signal to the server, or a client that goes away, leaves no search behind.
//! Each search here reads a FIFO no one writes, so it runs until something ends it.
#![cfg(target_os = "linux")]
mod common;

use common::Fixture;
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
