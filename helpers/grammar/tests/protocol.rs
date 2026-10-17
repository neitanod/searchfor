//! The Lua Grammar Helper over its pipes (spec §4.4, §7), without sfor.
use searchfor::formats::definition::FormatDefinition;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

struct H {
    child: Child,
    stdin: Option<ChildStdin>,
    out: BufReader<ChildStdout>,
}

fn fixture() -> String {
    format!("{}/tests/fixtures/player.lua", env!("CARGO_MANIFEST_DIR"))
}

/// The `[params]` of the Definition the index ships with the helper, as `hello` carries them.
fn lua_params() -> Value {
    let path = format!("{}/definitions/lua.toml", env!("CARGO_MANIFEST_DIR"));
    let src = std::fs::read_to_string(path).unwrap();
    let def = FormatDefinition::parse(&src, |p| p == "treesitter").unwrap();
    serde_json::to_value(&def.params).unwrap()
}

impl H {
    fn start() -> H {
        let mut child = Command::new(env!("CARGO_BIN_EXE_sfor-grammar"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
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
    fn hello_with(&mut self, params: Value, threads: usize) -> Value {
        self.send(
            json!({"op":"hello","id":0,"protocol":1,"format":"lua","params":params,"threads":threads}),
        );
        self.recv()
    }
    fn hello(&mut self, threads: usize) -> Value {
        self.hello_with(lua_params(), threads)
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
        .unwrap_or_else(|| panic!("no trails: {reply}"))
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
fn hello_answers_protocol_one_and_the_helper_name() {
    let mut h = H::start();
    let r = h.hello(4);
    assert_eq!(r["id"], 0, "{r}");
    assert_eq!(r["protocol"], 1, "{r}");
    assert_eq!(r["name"], "sfor-grammar-lua", "{r}");
}

#[test]
fn trails_of_a_method_and_a_local_function() {
    let mut h = H::start();
    h.hello(4);
    let src = std::fs::read_to_string(fixture()).unwrap();
    let a = src.find("needle").unwrap() as u64;
    let b = src.rfind("needle").unwrap() as u64;
    assert_ne!(a, b);
    h.trails(1, &fixture(), &[a, b]);
    let r = h.recv();
    assert_eq!(r["id"], 1);
    assert_eq!(names(&r), vec![vec!["Player:update"], vec!["helper"]]);
    let seg = &r["trails"][0][0];
    assert_eq!(seg["kind"], "function", "{r}");
    assert_eq!(
        (seg["line"].as_u64(), seg["end_line"].as_u64()),
        (Some(3), Some(5)),
        "{r}"
    );
}

#[test]
fn names_list_the_functions() {
    let mut h = H::start();
    h.hello(4);
    h.send(json!({"op":"names","id":1,"path":fixture()}));
    let r = h.recv();
    assert_eq!(r["id"], 1);
    let src = std::fs::read_to_string(fixture()).unwrap();
    let list = r["names"].as_array().unwrap_or_else(|| panic!("{r}"));
    let texts: Vec<&str> = list.iter().map(|n| n["text"].as_str().unwrap()).collect();
    assert_eq!(texts, ["Player:update", "helper"]);
    for n in list {
        let text = n["text"].as_str().unwrap();
        let p: Vec<usize> = n["pieces"][0]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as usize)
            .collect();
        assert_eq!(&src[p[2]..p[3]], text, "{n}");
    }
}

#[test]
fn a_query_that_does_not_compile_is_an_error_naming_the_query() {
    let mut h = H::start();
    let mut params = lua_params();
    params["query"] = json!("(no_such_node) @segment");
    let r = h.hello_with(params, 4);
    assert_eq!(r["id"], 0, "{r}");
    assert!(r["protocol"].is_null(), "{r}");
    let e = r["error"].as_str().unwrap_or_else(|| panic!("{r}"));
    assert!(e.contains("query"), "{e}");
}

#[test]
fn another_grammar_is_an_error_naming_both() {
    let mut h = H::start();
    let mut params = lua_params();
    params["grammar"] = json!("python");
    let r = h.hello_with(params, 4);
    assert_eq!(r["id"], 0, "{r}");
    let e = r["error"].as_str().unwrap_or_else(|| panic!("{r}"));
    assert!(e.contains("`lua`") && e.contains("`python`"), "{e}");
}

/// 8458: memory does not grow from one file to the next (D23). Linux only. It takes 266 s in
/// release (each request parses 140 KB), so it runs with the ceilings:
/// `cargo test --release -p sfor-grammar -- --ignored`.
#[cfg(target_os = "linux")]
#[test]
#[ignore]
fn memory_stays_flat_over_five_thousand_requests() {
    let big = std::env::temp_dir().join(format!("big-{}.lua", std::process::id()));
    let mut s = String::new();
    for i in 0..4000 {
        let body = if i == 3999 { "\"needle\"" } else { "i" };
        s.push_str(&format!("local function f{i}(i)\n  return {body}\nend\n\n"));
    }
    std::fs::write(&big, &s).unwrap();
    let at = s.rfind("needle").unwrap() as u64;
    let mut h = H::start();
    h.hello(4);
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
        let r = h.recv();
        assert_eq!(r["id"], id);
        if id == 1 {
            assert_eq!(names(&r), vec![vec!["f3999"]]);
        }
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
