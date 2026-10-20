//! The example helper over its pipes (spec §4.4), without s4.
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

struct H {
    child: Child,
    stdin: Option<ChildStdin>,
    out: BufReader<ChildStdout>,
}

fn fixture() -> String {
    format!("{}/tests/fixtures/app.ini", env!("CARGO_MANIFEST_DIR"))
}

impl H {
    fn start(env: &[(&str, &str)]) -> H {
        let mut c = Command::new(env!("CARGO_BIN_EXE_searchfor-example-helper"));
        c.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        for (k, v) in env {
            c.env(k, v);
        }
        let mut child = c.spawn().unwrap();
        let stdin = child.stdin.take();
        let out = BufReader::new(child.stdout.take().unwrap());
        H { child, stdin, out }
    }
    fn send(&mut self, v: Value) {
        let s = self.stdin.as_mut().unwrap();
        writeln!(s, "{v}").unwrap();
        s.flush().unwrap();
    }
    fn recv(&mut self) -> Value {
        let mut l = String::new();
        self.out.read_line(&mut l).unwrap();
        serde_json::from_str(&l).unwrap_or_else(|e| panic!("{e}: {l:?}"))
    }
    fn hello(&mut self, threads: usize) -> Value {
        self.send(
            json!({"op":"hello","id":0,"protocol":1,"format":"ini","params":{},"threads":threads}),
        );
        self.recv()
    }
    fn trails(&mut self, id: u64, path: &str, bytes: &[u64]) {
        let m: Vec<Value> = bytes
            .iter()
            .map(|b| json!({"line": 1, "byte": b}))
            .collect();
        self.send(json!({"op":"trails","id":id,"path":path,"matches":m}));
    }
}

impl Drop for H {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn names(reply: &Value) -> Vec<Vec<String>> {
    reply["trails"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| {
            t.as_array()
                .unwrap()
                .iter()
                .map(|s| s["name"].as_str().unwrap().to_string())
                .collect()
        })
        .collect()
}

#[test]
fn hello_then_trails_of_sections_and_keys() {
    let mut h = H::start(&[]);
    assert_eq!(h.hello(1)["protocol"], 1);
    let src = std::fs::read_to_string(fixture()).unwrap();
    let a = src.find("needle.example").unwrap() as u64;
    let b = src.find("needle client").unwrap() as u64;
    h.trails(1, &fixture(), &[a, b]);
    let r = h.recv();
    assert_eq!(r["id"], 1);
    assert_eq!(
        names(&r),
        vec![vec!["server", "host"], vec!["client", "name"]]
    );
}

#[test]
fn concurrent_requests_are_answered_as_they_finish() {
    let mut h = H::start(&[("SEARCHFOR_EXAMPLE_SLOW", "slow")]);
    h.hello(4);
    let slow = std::env::temp_dir().join(format!("slow-{}.ini", std::process::id()));
    std::fs::copy(fixture(), &slow).unwrap();
    h.trails(1, slow.to_str().unwrap(), &[0]);
    h.trails(2, &fixture(), &[0]);
    assert_eq!(
        (h.recv()["id"].as_u64(), h.recv()["id"].as_u64()),
        (Some(2), Some(1))
    );
    let _ = std::fs::remove_file(slow);
}

#[test]
fn eof_ends_the_helper() {
    let mut h = H::start(&[]);
    h.hello(1);
    h.stdin = None;
    let t = Instant::now();
    while h.child.try_wait().unwrap().is_none() {
        assert!(
            t.elapsed() < Duration::from_secs(2),
            "still running after EOF"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(h.child.wait().unwrap().success());
}

#[test]
fn errors_answer_and_the_helper_goes_on() {
    let mut h = H::start(&[("SEARCHFOR_EXAMPLE_ERROR", "app")]);
    h.hello(1);
    h.trails(1, &fixture(), &[0]);
    assert!(h.recv()["error"].is_string());
    h.trails(2, "/no/such/file.ini", &[0]);
    assert!(h.recv()["error"].as_str().unwrap().contains("file.ini"));
}

#[test]
fn names_list_sections_and_keys() {
    let mut h = H::start(&[]);
    h.hello(1);
    h.send(json!({"op":"names","id":1,"path":fixture()}));
    let r = h.recv();
    assert_eq!(r["id"], 1);
    let src = std::fs::read_to_string(fixture()).unwrap();
    let list = r["names"].as_array().unwrap();
    let texts: Vec<&str> = list.iter().map(|n| n["text"].as_str().unwrap()).collect();
    assert_eq!(
        texts,
        ["server", "host", "port", "client", "retries", "name"]
    );
    for n in list {
        let text = n["text"].as_str().unwrap();
        let pieces = n["pieces"].as_array().unwrap();
        assert_eq!(pieces.len(), 1, "{n}");
        let p: Vec<usize> = pieces[0]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as usize)
            .collect();
        assert_eq!((p[0], p[1]), (0, text.len()), "{n}");
        assert_eq!(&src[p[2]..p[3]], text, "{n}");
    }
}

/// 8458: memory does not grow from one file to the next (D23). Linux only. It takes 21 s in
/// release and 158 s in debug, so it runs with the ceilings:
/// `cargo test --release -p searchfor-example-helper -- --ignored`.
#[cfg(target_os = "linux")]
#[test]
#[ignore = "perf: 158 s in debug; RSS is read from /proc, so it runs on Linux alone"]
fn memory_stays_flat_over_five_thousand_requests() {
    let big = std::env::temp_dir().join(format!("big-{}.ini", std::process::id()));
    let mut s = String::new();
    for i in 0..4000 {
        s.push_str(&format!("[section{i}]\nkey = needle {i}\n\n"));
    }
    std::fs::write(&big, &s).unwrap();
    let at = s.rfind("needle").unwrap() as u64;
    let mut h = H::start(&[]);
    h.hello(1);
    let rss = |pid: u32| -> u64 {
        let st = std::fs::read_to_string(format!("/proc/{pid}/status")).unwrap();
        let kb = st
            .lines()
            .find(|l| l.starts_with("VmRSS:"))
            .unwrap()
            .split_whitespace()
            .nth(1)
            .unwrap();
        kb.parse::<u64>().unwrap() * 1024
    };
    let mut at_500 = 0;
    for id in 1..=5000u64 {
        h.trails(id, big.to_str().unwrap(), &[at]);
        assert_eq!(h.recv()["id"], id);
        if id == 500 {
            at_500 = rss(h.child.id());
        }
    }
    let at_5000 = rss(h.child.id());
    let _ = std::fs::remove_file(big);
    eprintln!("RSS after 500 requests: {at_500} bytes; after 5000: {at_5000} bytes");
    assert!(
        at_5000 <= at_500 + 2 * 1024 * 1024,
        "RSS grew from {at_500} to {at_5000}"
    );
}
