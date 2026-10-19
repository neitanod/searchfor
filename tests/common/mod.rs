//! Shared helpers for the end-to-end and ripgrep compatibility tests: the fixture copy and
//! the launchers (plan, Chunk 9, «The fixture copy»).
#![allow(dead_code)]

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

/// A copy of `tests/fixtures/` (dotfiles included) with an empty `.git` at its root, so that
/// `.gitignore` files apply to rg and s4 alike, an empty formats directory for
/// `SEARCHFOR_FORMATS_DIR`, and an empty Grammar directory for `SEARCHFOR_GRAMMARS_DIR`, whose index
/// does not exist, so no test reads the user's Grammars or the network (M2 plan, Task 10.4).
pub struct Fixture {
    pub dir: tempfile::TempDir,
    pub formats: tempfile::TempDir,
    pub grammars: tempfile::TempDir,
}

impl Fixture {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        copy_dir(&source_dir(), dir.path());
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        Self {
            dir,
            formats: tempfile::tempdir().unwrap(),
            grammars: tempfile::tempdir().unwrap(),
        }
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    pub fn file(&self, rel: &str) -> PathBuf {
        self.dir.path().join(rel)
    }

    /// The `file://` URL of an index that does not exist, in the Grammar directory.
    pub fn no_index(&self) -> String {
        format!(
            "file://{}",
            self.grammars.path().join("no-index.json").display()
        )
    }

    /// s4 in the fixture copy, stdin from /dev/null, `--color never` unless `args` sets it.
    pub fn s4<I, S>(&self, args: I) -> Command
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut c = Command::new(assert_cmd::cargo::cargo_bin("s4"));
        c.current_dir(self.path())
            .env("SEARCHFOR_FORMATS_DIR", self.formats.path())
            .env("SEARCHFOR_GRAMMARS_DIR", self.grammars.path())
            .env("SEARCHFOR_GRAMMAR_INDEX", self.no_index())
            .env_remove("SEARCHFOR_CONFIG_PATH")
            .env_remove("RIPGREP_CONFIG_PATH")
            .stdin(Stdio::null());
        let args: Vec<_> = args.into_iter().map(|a| a.as_ref().to_owned()).collect();
        if !args
            .iter()
            .any(|a| a.to_string_lossy().starts_with("--color"))
        {
            c.arg("--color").arg("never");
        }
        c.args(args);
        c
    }

    /// The reference rg (`rg_path`) in the fixture copy, stdin from /dev/null, `--color never`
    /// unless set.
    pub fn rg<I, S>(&self, args: I) -> Command
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut c = Command::new(rg_path());
        c.current_dir(self.path())
            .env_remove("RIPGREP_CONFIG_PATH")
            .stdin(Stdio::null());
        let args: Vec<_> = args.into_iter().map(|a| a.as_ref().to_owned()).collect();
        if !args
            .iter()
            .any(|a| a.to_string_lossy().starts_with("--color"))
        {
            c.arg("--color").arg("never");
        }
        c.args(args);
        c
    }
}

pub fn source_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let target = to.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            copy_dir(&e.path(), &target);
        } else {
            std::fs::copy(e.path(), &target).unwrap();
        }
    }
}

/// The executable `bin` of the workspace member `package`, built with `features` (D29: cargo
/// only exposes `CARGO_BIN_EXE_*` for the root package's own binaries). Built once per
/// (package, features) per test binary; a build failure panics with cargo's stderr.
pub fn workspace_bin(package: &str, bin: &str, features: &[&str]) -> PathBuf {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    type Built = Mutex<HashMap<(String, String, Vec<String>), PathBuf>>;
    static BUILT: OnceLock<Built> = OnceLock::new();
    let key = (
        package.to_string(),
        bin.to_string(),
        features.iter().map(|f| f.to_string()).collect::<Vec<_>>(),
    );
    // Held for the build, so concurrent tests wait for one cargo instead of starting several.
    let mut built = BUILT.get_or_init(Default::default).lock().unwrap();
    if let Some(p) = built.get(&key) {
        return p.clone();
    }
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let mut c = Command::new(cargo);
    c.current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["build", "--quiet", "-p", package, "--bin", bin]);
    if !features.is_empty() {
        c.arg("--features").arg(features.join(","));
    }
    let o = c
        .arg("--message-format=json")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(
        o.status.success(),
        "cargo build -p {package} --bin {bin} failed:\n{}",
        String::from_utf8_lossy(&o.stderr)
    );
    let exe = String::from_utf8_lossy(&o.stdout)
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter(|m| m["reason"] == "compiler-artifact" && m["target"]["name"] == bin)
        .find_map(|m| m["executable"].as_str().map(PathBuf::from))
        .unwrap_or_else(|| panic!("cargo built no executable `{bin}` for {package}"));
    built.insert(key, exe.clone());
    exe
}

/// The reference ripgrep: `SEARCHFOR_RG` when set, else `/usr/bin/rg` when it exists (another rg,
/// such as 15.x, may come first on `PATH`), else `rg` on `PATH` (M3 review r1, Codex #8).
pub fn rg_path() -> PathBuf {
    if let Some(p) = std::env::var_os("SEARCHFOR_RG") {
        return p.into();
    }
    let usr = Path::new("/usr/bin/rg");
    if usr.is_file() {
        return usr.to_path_buf();
    }
    PathBuf::from("rg")
}

/// A comparison or measurement that cannot run: a note, or a failure when
/// `SEARCHFOR_REQUIRE_REFERENCE=1`, which acceptance runs set so that a skipped comparison never
/// counts as passed (M3 review r1, Codex #8). Returns false.
pub fn missing_reference(what: &str) -> bool {
    if std::env::var_os("SEARCHFOR_REQUIRE_REFERENCE").is_some_and(|v| v == "1") {
        panic!("{what} is required (SEARCHFOR_REQUIRE_REFERENCE=1) and missing");
    }
    eprintln!("skipping: {what} not found");
    false
}

/// True when `rg_path()` is ripgrep 14.1.1; otherwise `missing_reference`.
pub fn reference_rg() -> bool {
    let ok = Command::new(rg_path())
        .arg("--version")
        .env_remove("RIPGREP_CONFIG_PATH")
        .output()
        .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).starts_with("ripgrep 14.1.1"));
    ok || missing_reference(&format!("reference rg 14.1.1 at {}", rg_path().display()))
}

pub fn run(c: &mut Command) -> Output {
    c.output().unwrap()
}

pub fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

pub fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

pub fn code(o: &Output) -> i32 {
    o.status.code().unwrap_or(-1)
}

/// `--json` output with every `elapsed` of `end` and `summary` replaced by zeros, so it can be
/// snapshotted.
pub fn mask_json_times(out: &str) -> String {
    fn mask(v: &mut serde_json::Value) {
        match v {
            serde_json::Value::Object(m) => {
                for (k, x) in m.iter_mut() {
                    if k == "elapsed" || k == "elapsed_total" {
                        *x = serde_json::json!({"secs": 0, "nanos": 0, "human": "0s"});
                    } else {
                        mask(x);
                    }
                }
            }
            serde_json::Value::Array(a) => a.iter_mut().for_each(mask),
            _ => {}
        }
    }
    let mut s = String::new();
    for line in out.lines() {
        let mut v: serde_json::Value = serde_json::from_str(line).unwrap();
        mask(&mut v);
        s.push_str(&serde_json::to_string(&v).unwrap());
        s.push('\n');
    }
    s
}

/// Every `match` message of `--json` output validates against `docs/json-schema.json`.
pub fn assert_valid_json_schema(out: &str) {
    let schema: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/docs/json-schema.json"
        ))
        .unwrap(),
    )
    .unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    for line in out.lines() {
        let v: serde_json::Value = serde_json::from_str(line).unwrap();
        if v["type"] == "match" {
            assert!(validator.is_valid(&v["data"]), "{line}");
        }
    }
}
