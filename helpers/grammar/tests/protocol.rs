//! The Grammar Helper over its pipes (spec §4.4, §7), without s4: the Lua build the workspace
//! tests by default, and each of the six Grammars of v1.0 when built with its feature alone
//! (`--no-default-features --features <lang>`; M5b plan, Task 8.3).
use searchfor::formats::definition::FormatDefinition;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

struct H {
    child: Child,
    stdin: Option<ChildStdin>,
    out: BufReader<ChildStdout>,
}

#[cfg(feature = "lua")]
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
        let mut child = Command::new(env!("CARGO_BIN_EXE_searchfor-grammar"))
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
        self.hello_as("lua", params, threads)
    }
    fn hello_as(&mut self, format: &str, params: Value, threads: usize) -> Value {
        self.send(
            json!({"op":"hello","id":0,"protocol":1,"format":format,"params":params,"threads":threads}),
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

#[cfg(feature = "lua")]
#[test]
fn hello_answers_protocol_one_and_the_helper_name() {
    let mut h = H::start();
    let r = h.hello(4);
    assert_eq!(r["id"], 0, "{r}");
    assert_eq!(r["protocol"], 1, "{r}");
    assert_eq!(r["name"], "searchfor-grammar-lua", "{r}");
}

#[cfg(feature = "lua")]
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

#[cfg(feature = "lua")]
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

#[cfg(feature = "lua")]
#[test]
fn a_file_over_max_bytes_is_not_parsed() {
    // Spec §5.3 (review m2-code-r1, Codex #2): s4 stops such a file before the request, and
    // the helper refuses it too, for `trails` and `names`.
    let mut h = H::start();
    let mut params = lua_params();
    params["max_bytes"] = json!(1);
    h.hello_with(params, 4);
    h.trails(1, &fixture(), &[60]);
    h.send(json!({"op":"names","id":2,"path":fixture()}));
    // The helper serves `concurrent`ly, so the two answers come in either order.
    let mut answers = [h.recv(), h.recv()];
    answers.sort_by_key(|r| r["id"].as_u64());
    for (id, r) in [1, 2].into_iter().zip(answers) {
        assert_eq!(r["id"], id, "{r}");
        assert!(r["trails"].is_null() && r["names"].is_null(), "{r}");
        let e = r["error"].as_str().unwrap_or_else(|| panic!("{r}"));
        assert!(e.contains("max_bytes"), "{e}");
    }
}

#[cfg(feature = "lua")]
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

#[cfg(feature = "lua")]
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
/// `cargo test --release -p searchfor-grammar -- --ignored`.
#[cfg(all(target_os = "linux", feature = "lua"))]
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

/// The workspace root: the build script and the index paths are relative to it.
fn root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read_json(rel: &str) -> Value {
    let path = root().join(rel);
    let src = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&src).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn index_validator() -> jsonschema::Validator {
    jsonschema::validator_for(&read_json("docs/grammar-index.schema.json")).unwrap()
}

fn errors(v: &jsonschema::Validator, index: &Value) -> Vec<String> {
    match v.validate(index) {
        Ok(()) => Vec::new(),
        Err(e) => e.map(|e| format!("{}: {e}", e.instance_path)).collect(),
    }
}

/// An index entry as the build script prints it, for a system and a binary URL.
fn entry(name: &str, system: &str, url: &str, sha: &str) -> Value {
    json!({
        "name": name,
        "version": "0.2.0",
        "source": "https://crates.io/crates/tree-sitter-lua",
        "license": "MIT",
        "definition": "name = \"lua\"\n",
        "binaries": {system: {"url": url, "sha256": sha}},
    })
}

/// The six Grammars of v1.0 (Q57.gramaticas), the only ones the committed index may hold.
const SIX: [&str; 6] = ["c", "cpp", "java", "ruby", "c-sharp", "elixir"];

/// The committed index is valid against the schema and s4 reads it; it lists no Grammar until
/// the `grammars-v1` release publishes them, and then exactly the six (Task 11.4, gate 3).
#[test]
fn the_committed_index_is_valid_and_empty_or_the_six() {
    let index = read_json("grammars/index.json");
    assert_eq!(errors(&index_validator(), &index), Vec::<String>::new());
    let bytes = serde_json::to_vec(&index).unwrap();
    let parsed = searchfor::grammars::index::Index::parse(&bytes).unwrap();
    let mut names: Vec<&str> = parsed.grammars.iter().map(|g| g.name.as_str()).collect();
    names.sort_unstable();
    let mut six = SIX.to_vec();
    six.sort_unstable();
    assert!(names.is_empty() || names == six, "{names:?}");
}

/// Each Definition has one source, `grammars/<lang>/<lang>.toml`; the helper's copy under
/// `definitions/` is the same text, and it describes its Grammar as `--install-grammar`
/// requires (name, grammar and processor).
#[test]
fn the_six_definitions_have_one_source_and_install_as_their_grammar() {
    for lang in SIX {
        let source = std::fs::read_to_string(root().join(format!("grammars/{lang}/{lang}.toml")))
            .unwrap_or_else(|e| panic!("{lang}: {e}"));
        let copy = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(format!("definitions/{lang}.toml")),
        )
        .unwrap_or_else(|e| panic!("{lang}: {e}"));
        assert_eq!(
            copy, source,
            "definitions/{lang}.toml differs from grammars/{lang}/"
        );
        let def = FormatDefinition::parse(&source, |p| p == "treesitter").unwrap();
        assert_eq!(def.name, lang);
        assert_eq!(def.processor, "treesitter");
        assert_eq!(def.params["grammar"].as_str(), Some(lang), "{lang}");
    }
}

/// The Grammar this build carries (one of the six), the fixture's extension, the Trail of
/// the fixture's `needle` and the names `names` lists, in order.
#[cfg(not(feature = "lua"))]
const SAMPLE: (&str, &str, &[&str], &[&str]) = if cfg!(feature = "c") {
    ("c", "sample.c", &["sample"], &["Sample", "sample"])
} else if cfg!(feature = "cpp") {
    (
        "cpp",
        "sample.cpp",
        &["demo", "Sample", "run"],
        &["demo", "Sample", "run"],
    )
} else if cfg!(feature = "java") {
    (
        "java",
        "Sample.java",
        &["Sample", "run"],
        &["Sample", "run"],
    )
} else if cfg!(feature = "ruby") {
    (
        "ruby",
        "sample.rb",
        &["Demo", "Sample", "run"],
        &["Demo", "Sample", "run"],
    )
} else if cfg!(feature = "c-sharp") {
    (
        "c-sharp",
        "sample.cs",
        &["Demo", "Sample", "Run"],
        &["Demo", "Sample", "Run"],
    )
} else {
    (
        "elixir",
        "sample.ex",
        &["Sample", "run"],
        &["Sample", "run"],
    )
};

/// Task 8.3: the helper of one of the six Grammars, with the Definition the index ships,
/// gives its fixture's `needle` the Trail the compiled-in Grammars give (one Segment per
/// enclosing definition, by name) and lists the same names.
#[cfg(not(feature = "lua"))]
#[test]
fn the_grammar_gives_the_trail_and_the_names_of_its_fixture() {
    let (lang, file, trail, listed) = SAMPLE;
    let src = std::fs::read_to_string(root().join(format!("grammars/{lang}/{lang}.toml"))).unwrap();
    let def = FormatDefinition::parse(&src, |p| p == "treesitter").unwrap();
    let params = serde_json::to_value(&def.params).unwrap();
    let fixture = format!("{}/tests/fixtures/{file}", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&fixture).unwrap();
    let at = text.find("needle").unwrap() as u64;
    let mut h = H::start();
    let r = h.hello_as(lang, params, 2);
    assert_eq!(r["name"], format!("searchfor-grammar-{lang}"), "{r}");
    h.trails(1, &fixture, &[at]);
    let r = h.recv();
    assert_eq!(names(&r), vec![trail.to_vec()], "{r}");
    h.send(json!({"op":"names","id":2,"path":fixture}));
    let r = h.recv();
    let list = r["names"].as_array().unwrap_or_else(|| panic!("{r}"));
    let texts: Vec<&str> = list.iter().map(|n| n["text"].as_str().unwrap()).collect();
    assert_eq!(texts, listed, "{r}");
}

#[test]
fn the_schema_accepts_an_entry_and_refuses_broken_ones() {
    let v = index_validator();
    let sha = "0123456789abcdef".repeat(4);
    let wrap = |e: Value| json!({"version": 1, "grammars": [e]});
    let ok = [
        entry(
            "lua",
            "linux-x86_64",
            "https://example.org/searchfor-grammar-lua",
            &sha,
        ),
        entry(
            "lua",
            "macos-aarch64",
            "file:///tmp/searchfor-grammar-lua",
            &sha,
        ),
        entry("c-sharp", "windows-x86_64", "file:///C:/g.exe", &sha),
    ];
    for e in ok {
        let index = wrap(e);
        assert_eq!(errors(&v, &index), Vec::<String>::new(), "{index}");
    }
    // The keys of the five release targets (M5b spec §8.1).
    for system in [
        "linux-x86_64",
        "linux-aarch64",
        "macos-x86_64",
        "macos-aarch64",
        "windows-x86_64",
    ] {
        let index = wrap(entry("c", system, "https://example.org/x", &sha));
        assert_eq!(errors(&v, &index), Vec::<String>::new(), "{index}");
    }
    let mut no_license = entry("lua", "linux-x86_64", "https://example.org/x", &sha);
    no_license.as_object_mut().unwrap().remove("license");
    let bad = [
        (
            "a name outside [a-z0-9_]",
            entry("Lua", "linux-x86_64", "https://example.org/x", &sha),
        ),
        (
            "a leading hyphen in the name",
            entry("-c", "linux-x86_64", "https://example.org/x", &sha),
        ),
        (
            "a path in the name",
            entry("../lua", "linux-x86_64", "https://example.org/x", &sha),
        ),
        (
            "an http URL",
            entry("lua", "linux-x86_64", "http://example.org/x", &sha),
        ),
        (
            "a short SHA-256",
            entry("lua", "linux-x86_64", "https://example.org/x", "abc"),
        ),
        (
            "an upper-case SHA-256",
            entry(
                "lua",
                "linux-x86_64",
                "https://example.org/x",
                &sha.to_uppercase(),
            ),
        ),
        (
            "a system without an arch",
            entry("lua", "linux", "https://example.org/x", &sha),
        ),
        ("no license", no_license),
    ];
    for (why, e) in bad {
        assert!(!v.is_valid(&wrap(e)), "accepted {why}");
    }
    assert!(
        !v.is_valid(&json!({"version": 2, "grammars": []})),
        "accepted version 2"
    );
    assert!(!v.is_valid(&json!({"version": 1})), "accepted no grammars");
}

/// Task 8.3: the build script's entry, wrapped as an index, validates against the schema, and
/// s4's installer takes the binary it names through its `file://` URL, checking its SHA-256;
/// the installed binary is a working Lua Grammar Helper for this system. It rebuilds the helper
/// in release, so it runs only with `SEARCHFOR_TEST_BUILD_SCRIPT=1`.
#[test]
fn the_build_script_prints_a_valid_index_entry() {
    if std::env::var_os("SEARCHFOR_TEST_BUILD_SCRIPT").is_none() {
        eprintln!(
            "skipped: set SEARCHFOR_TEST_BUILD_SCRIPT=1 to run scripts/build-grammar-helper.sh"
        );
        return;
    }
    let out = tempfile_dir("searchfor-gh");
    let o = Command::new("bash")
        .arg("scripts/build-grammar-helper.sh")
        .arg("lua")
        .arg(&out)
        .current_dir(root())
        .stderr(Stdio::inherit())
        .output()
        .unwrap();
    assert!(o.status.success(), "{:?}", o.status);
    let e: Value = serde_json::from_slice(&o.stdout).unwrap();
    eprintln!("{e:#}");
    let index = json!({"version": 1, "grammars": [e]});
    assert_eq!(errors(&index_validator(), &index), Vec::<String>::new());
    let bytes = serde_json::to_vec(&index).unwrap();
    let parsed = searchfor::grammars::index::Index::parse(&bytes).unwrap();
    let lua = parsed.get("lua").unwrap();
    let def = std::fs::read_to_string(root().join("helpers/grammar/definitions/lua.toml")).unwrap();
    assert_eq!(lua.definition, def);
    assert_eq!(lua.version, "0.2.0");
    let system = searchfor::grammars::index::system();
    let bin = lua
        .binaries
        .get(&system)
        .unwrap_or_else(|| panic!("no {system} in {e}"));
    let suffix = format!("-{system}{}", std::env::consts::EXE_SUFFIX);
    assert!(bin.url.ends_with(&suffix), "{}", bin.url);
    let installed_dir = out.join("installed");
    let installed = searchfor::grammars::install::install(&parsed, "lua", &installed_dir).unwrap();
    let mut child = Command::new(&installed.binary)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut h = H {
        stdin: child.stdin.take(),
        out: BufReader::new(child.stdout.take().unwrap()),
        child,
    };
    assert_eq!(h.hello(1)["name"], "searchfor-grammar-lua");
    drop(h);
    let _ = std::fs::remove_dir_all(out);
}

/// A new directory under the system's temporary directory, named after `prefix` and this
/// process.
fn tempfile_dir(prefix: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("{prefix}-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}
