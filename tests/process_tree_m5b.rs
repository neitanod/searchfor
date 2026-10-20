//! Windows: a forcibly terminated s4 leaves no `--hostname-bin` program, no `-z`
//! decompressor and none of their descendants (Task 8.1, D21); `--pre` batch files run as
//! rg runs them (D22).
#![cfg(windows)]
mod common;

use common::Fixture;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn example_helper() -> PathBuf {
    common::workspace_bin("searchfor-example-helper", "searchfor-example-helper", &[])
}

fn alive(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, WAIT_TIMEOUT};
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
    };
    // SAFETY: plain Win32 calls; the handle is closed before returning.
    unsafe {
        let h = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
        if h as usize == 0 {
            return false;
        }
        let running = WaitForSingleObject(h, 0) == WAIT_TIMEOUT;
        CloseHandle(h);
        running
    }
}

fn announced_pids(file: &Path) -> Vec<u32> {
    std::fs::read_to_string(file)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.trim().parse().ok())
        .collect()
}

/// Kills and reaps whatever a process-tree test started when the test ends, by
/// success or by panic: s4 (if still held) and every announced PID still alive.
/// The panic message is built before this runs, so the evidence is kept.
struct TreeCleanup {
    s4: Option<Child>,
    announced: PathBuf,
}

impl Drop for TreeCleanup {
    fn drop(&mut self) {
        if let Some(mut c) = self.s4.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        for pid in announced_pids(&self.announced) {
            if alive(pid) {
                let _ = Command::new("taskkill")
                    .args(["/F", "/T", "/PID", &pid.to_string()])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
            }
        }
    }
}

/// Waits until the synthetic program and its descendant announced themselves
/// (exactly two PIDs), kills s4 within 20 s of its start, and requires both to be
/// gone soon after; the synthetic program sleeps 300 s, so only the Job can end it.
fn kill_s4_and_expect_no_survivors(s4: Child, announced: &Path) {
    let spawned = Instant::now();
    let mut guard = TreeCleanup {
        s4: Some(s4),
        announced: announced.to_path_buf(),
    };
    let deadline = spawned + Duration::from_secs(20);
    while announced_pids(announced).len() < 2 {
        assert!(
            Instant::now() < deadline,
            "the synthetic program never started: {:?}",
            announced_pids(announced)
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    let started = announced_pids(announced);
    assert_eq!(
        started.len(),
        2,
        "exactly the program and one descendant: {started:?}"
    );
    // s4 stays in the guard until both calls succeed, so a failure still reaps it.
    let s4 = guard.s4.as_mut().unwrap();
    s4.kill().unwrap();
    s4.wait().unwrap();
    guard.s4.take();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let left: Vec<u32> = started.iter().copied().filter(|p| alive(*p)).collect();
        if left.is_empty() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "survivors after s4 was killed: {left:?}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn m5b_killed_s4_leaves_no_hostname_program_or_descendant() {
    let fx = Fixture::new();
    let announced = fx.file("hostname-pids.txt");
    let bin = fx.file("hostname-hold.exe");
    std::fs::copy(example_helper(), &bin).unwrap();
    let input = fx.file("a.txt");
    std::fs::write(&input, "needle\n").unwrap();
    // `default` is `file://{path}` on Windows (grep-printer's aliases), with no
    // `{host}`, so it would never run --hostname-bin; `file` has `{host}` everywhere.
    let mut c = fx.s4(["--hyperlink-format", "file", "--color=always", "needle"]);
    c.arg("--hostname-bin")
        .arg(&bin)
        .arg(&input)
        .env("SEARCHFOR_EXAMPLE_ACT_AS", "hold-pipes")
        .env("SEARCHFOR_EXAMPLE_FORK", "1")
        .env("SEARCHFOR_EXAMPLE_PIDS", &announced)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    kill_s4_and_expect_no_survivors(c.spawn().unwrap(), &announced);
}

#[test]
fn m5b_killed_s4_leaves_no_decompressor_or_descendant() {
    let fx = Fixture::new();
    let announced = fx.file("zip-pids.txt");
    let bin_dir = fx.file("bin");
    std::fs::create_dir_all(&bin_dir).unwrap();
    // `-z` resolves `gzip` through PATH; this copy comes first.
    std::fs::copy(example_helper(), bin_dir.join("gzip.exe")).unwrap();
    let input = fx.file("a.txt.gz");
    std::fs::write(&input, "not really gzip\n").unwrap();
    let path = std::env::join_paths(
        std::iter::once(bin_dir).chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();
    let mut c = fx.s4(["-z", "needle"]);
    c.arg(&input)
        .env("PATH", path)
        .env("SEARCHFOR_EXAMPLE_ACT_AS", "hold-pipes")
        .env("SEARCHFOR_EXAMPLE_FORK", "1")
        .env("SEARCHFOR_EXAMPLE_PIDS", &announced)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    kill_s4_and_expect_no_survivors(c.spawn().unwrap(), &announced);
}

#[test]
fn m5b_pre_batch_files_match_ripgrep() {
    if !common::reference_rg() {
        return;
    }
    let fx = Fixture::new();
    let dir = fx.file("pre dir");
    std::fs::create_dir_all(&dir).unwrap();
    for ext in ["cmd", "bat"] {
        let script = dir.join(format!("show.{ext}"));
        std::fs::write(&script, "@echo off\r\ntype %1\r\n").unwrap();
        for name in [
            "plain.txt",
            "with space.txt",
            "a&b.txt",
            "c^d.txt",
            "100%.txt",
            "(x).txt",
            "é ñ.txt",
        ] {
            let file = dir.join(name);
            std::fs::write(&file, "needle one\nother\n").unwrap();
            let args = [
                "--pre",
                script.to_str().unwrap(),
                "--no-heading",
                "--color=never",
                "needle",
                file.to_str().unwrap(),
            ];
            let mut s = fx.s4(args);
            s.args(["--no-docs", "--no-trail"]);
            let a = common::run(&mut s);
            let b = common::run(&mut fx.rg(args));
            assert_eq!(common::code(&a), common::code(&b), "{ext} {name}");
            assert_eq!(a.stdout, b.stdout, "{ext} {name}");
        }
    }
}
