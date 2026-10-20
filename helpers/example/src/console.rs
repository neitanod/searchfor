//! `--test-console-driver S4 FIXTURE LOG RESULT` (M5b plan, Task 2.3), Windows only: the console
//! Ctrl-C of `m5b_console_ctrl_c_kills_and_reaps_managed_processes`, sent where it reaches only
//! the processes of the test.
//!
//! The driver starts a copy of itself with a console of its own (`CREATE_NEW_CONSOLE`): a
//! console event goes to every process attached to the console, so it never reaches the test
//! runner. That copy ignores Ctrl-C through a handler (handlers are not inherited), keeps Ctrl-C
//! processing on for its children, and runs `S4 needle gen --flat -j 2` in `FIXTURE` with the
//! example helper's wait knobs. Once two helper copies hold a request, it sends
//! `GenerateConsoleCtrlEvent(CTRL_C_EVENT, 0)`, waits for s4, and writes
//! `{"status":<s4's exit code>,"alive":[<pids still running>]}` to `RESULT`: the pids are those
//! of `LOG` (`SEARCHFOR_EXAMPLE_LOG`) and of the readiness file, given up to 5 s to end. Every
//! wait has a deadline, past which s4 is killed and reaped and the driver fails.
use std::ffi::OsString;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{CloseHandle, WAIT_TIMEOUT};
use windows_sys::Win32::System::Console::{
    CTRL_C_EVENT, GenerateConsoleCtrlEvent, SetConsoleCtrlHandler,
};
use windows_sys::Win32::System::Threading::{
    CREATE_NEW_CONSOLE, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
    WaitForSingleObject,
};

/// Set in the copy that owns the private console.
const INNER: &str = "SEARCHFOR_EXAMPLE_CONSOLE_INNER";
const READY_LIMIT: Duration = Duration::from_secs(60);
const EXIT_LIMIT: Duration = Duration::from_secs(30);
const REAP_LIMIT: Duration = Duration::from_secs(5);

fn fail(msg: impl std::fmt::Display) -> ! {
    eprintln!("searchfor-example-helper: --test-console-driver: {msg}");
    std::process::exit(1);
}

pub(crate) fn driver(args: Vec<OsString>) -> ! {
    let [s4, fixture, log, result] =
        <[OsString; 4]>::try_from(args).unwrap_or_else(|_| fail("needs S4 FIXTURE LOG RESULT"));
    if std::env::var_os(INNER).is_none() {
        let me = std::env::current_exe().unwrap_or_else(|e| fail(e));
        let status = Command::new(me)
            .arg("--test-console-driver")
            .args([&s4, &fixture, &log, &result])
            .env(INNER, "1")
            .creation_flags(CREATE_NEW_CONSOLE)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .status()
            .unwrap_or_else(|e| fail(e));
        std::process::exit(status.code().unwrap_or(1));
    }
    inner(
        Path::new(&s4),
        Path::new(&fixture),
        Path::new(&log),
        Path::new(&result),
    )
}

unsafe extern "system" fn ignore(_event: u32) -> i32 {
    1
}

fn inner(s4: &Path, fixture: &Path, log: &Path, result: &Path) -> ! {
    // SAFETY: a null handler with FALSE turns Ctrl-C processing on, which s4 inherits; the
    // handler, which children do not inherit, keeps this process alive through the event.
    unsafe {
        if SetConsoleCtrlHandler(None, 0) == 0 || SetConsoleCtrlHandler(Some(ignore), 1) == 0 {
            fail(std::io::Error::last_os_error());
        }
    }
    let ready = fixture.join("console-ready.log");
    let release = fixture.join("console-release");
    let mut child = Command::new(s4)
        .args(["needle", "gen", "--flat", "-j", "2", "--color", "never"])
        .current_dir(fixture)
        .env("SEARCHFOR_EXAMPLE_LOG", log)
        .env("SEARCHFOR_EXAMPLE_WAIT_READY", &ready)
        .env("SEARCHFOR_EXAMPLE_WAIT_RELEASE", &release)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap_or_else(|e| fail(e));
    let began = Instant::now();
    while lines(&ready).len() < 2 {
        if began.elapsed() > READY_LIMIT {
            give_up(&mut child, "two helper copies never held a request");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    // SAFETY: group 0: every process attached to this private console.
    if unsafe { GenerateConsoleCtrlEvent(CTRL_C_EVENT, 0) } == 0 {
        let e = std::io::Error::last_os_error();
        give_up(&mut child, &format!("GenerateConsoleCtrlEvent: {e}"));
    }
    let sent = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) if sent.elapsed() > EXIT_LIMIT => {
                give_up(&mut child, "s4 outlived its Ctrl-C")
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(e) => give_up(&mut child, &e.to_string()),
        }
    };
    let mut pids: Vec<u32> = lines(log)
        .iter()
        .filter_map(|l| l.strip_prefix("start ")?.trim().parse().ok())
        .collect();
    pids.extend(lines(&ready).iter().filter_map(|l| helper_pid(l)));
    pids.sort_unstable();
    pids.dedup();
    let ended = Instant::now();
    let mut alive: Vec<u32> = pids.clone();
    while !alive.is_empty() && ended.elapsed() < REAP_LIMIT {
        std::thread::sleep(Duration::from_millis(10));
        alive.retain(|&p| running(p));
    }
    let alive: Vec<String> = alive.iter().map(u32::to_string).collect();
    let json = format!(
        "{{\"status\":{},\"alive\":[{}]}}",
        status.code().map_or(u32::MAX, |c| c as u32),
        alive.join(",")
    );
    if let Err(e) = std::fs::write(result, json) {
        fail(e);
    }
    std::process::exit(0);
}

/// Kills and reaps s4, then fails.
fn give_up(child: &mut std::process::Child, why: &str) -> ! {
    let _ = child.kill();
    let _ = child.wait();
    fail(why)
}

/// The complete lines of `path`.
fn lines(path: &Path) -> Vec<String> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let complete = text.rfind('\n').map_or("", |k| &text[..k]);
    complete.lines().map(str::to_string).collect()
}

/// The `helper_pid` of one readiness line.
fn helper_pid(line: &str) -> Option<u32> {
    let rest = line.split("\"helper_pid\":").nth(1)?;
    rest.trim_end_matches(['}', ' ']).trim().parse().ok()
}

fn running(pid: u32) -> bool {
    // SAFETY: the handle is checked, waited on without blocking, and closed.
    unsafe {
        let h = OpenProcess(
            PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
            0,
            pid,
        );
        if h.is_null() {
            return false;
        }
        let r = WaitForSingleObject(h, 0) == WAIT_TIMEOUT;
        CloseHandle(h);
        r
    }
}
