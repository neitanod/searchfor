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
    /// `Fixture::slashed`: s4 prints `/` between a path's components on every system.
    slash_paths: bool,
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
            slash_paths: false,
        }
    }

    /// A fixture whose `s4` prints the paths it walks with `/` on every system: on Windows it
    /// adds `--path-separator /` unless `args` sets a separator (M5b spec §4.2: rg's Windows
    /// paths have `\` between the components the walker joins). For the tests that assert how
    /// a Trail layout presents a path; the comparisons with rg use `new`, whose paths stay
    /// native on both sides. Only path bytes change: searched bytes, JSON paths and patterns
    /// are never rewritten.
    pub fn slashed() -> Self {
        Self {
            slash_paths: true,
            ..Self::new()
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
        let given = |flag: &str| args.iter().any(|a| a.to_string_lossy().starts_with(flag));
        if self.slash_paths && cfg!(windows) && !given("--path-separator") {
            c.arg("--path-separator").arg("/");
        }
        if !given("--color") {
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
    // Low priority on Linux, as every cargo of the plan (M5b plan, Limits); offline and
    // locked, as the suite itself. CARGO_TARGET_DIR, when set, is inherited.
    let mut c = match low_priority_prefix() {
        Some(prefix) => {
            let mut c = Command::new(&prefix[0]);
            c.args(&prefix[1..]).arg(&cargo);
            c
        }
        None => Command::new(&cargo),
    };
    c.current_dir(env!("CARGO_MANIFEST_DIR")).args([
        "build",
        "--quiet",
        "--offline",
        "--locked",
        "-p",
        package,
        "--bin",
        bin,
    ]);
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

/// `nice -n 19 ionice -c3` on Linux when both are on `PATH`, else nothing: the words put
/// before a nested cargo so that it builds at the lowest CPU and I/O priority.
fn low_priority_prefix() -> Option<Vec<PathBuf>> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let find = |name: &str| {
        std::env::split_paths(&std::env::var_os("PATH")?)
            .map(|d| d.join(name))
            .find(|p| p.is_file())
    };
    let (nice, ionice) = (find("nice")?, find("ionice")?);
    Some(vec![nice, "-n".into(), "19".into(), ionice, "-c3".into()])
}

/// The reference ripgrep (M5b spec §5.2): `select_rg` over this process's `SEARCHFOR_RG`, this
/// system, and whether `/usr/bin/rg` exists.
pub fn rg_path() -> PathBuf {
    select_rg(
        std::env::var_os("SEARCHFOR_RG"),
        cfg!(unix),
        Path::new("/usr/bin/rg").is_file(),
    )
}

/// The reference ripgrep, given the environment it is chosen from: `override_path` (the value
/// of `SEARCHFOR_RG`) when set, as given and never searched on `PATH`; else on Unix
/// `/usr/bin/rg` when it exists (another rg, such as 15.x, may come first on `PATH`; M3 review
/// r1, Codex #8); else `rg` from `PATH`, the last resort (a Mac with rg under
/// `/opt/homebrew/bin`, Windows). `reference_rg` then checks that it is 14.1.1.
pub fn select_rg(
    override_path: Option<std::ffi::OsString>,
    unix: bool,
    usr_bin_rg_exists: bool,
) -> PathBuf {
    if let Some(p) = override_path {
        return p.into();
    }
    if unix && usr_bin_rg_exists {
        return PathBuf::from("/usr/bin/rg");
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

/// How long a fuzz test runs (M5b spec §5.1, plan Task 4.2): a deadline, a cap on the cases,
/// or both. `permits(completed)` says whether case number `completed` (from 0) may run.
pub struct FuzzBudget {
    deadline: Option<std::time::Instant>,
    /// A budget of more than 0 s runs at least one case, however late its test starts.
    at_least_one: bool,
    count: Option<usize>,
}

impl FuzzBudget {
    /// A deadline `seconds` from now and a cap of `count` cases, each when given. Neither
    /// given is no limit: a fuzz test always gives one (`fuzz_budget`).
    pub fn new(seconds: Option<u64>, count: Option<usize>) -> Self {
        Self {
            deadline: seconds
                .map(|s| std::time::Instant::now() + std::time::Duration::from_secs(s)),
            at_least_one: seconds.is_some_and(|s| s > 0),
            count,
        }
    }

    pub fn permits(&self, completed: usize) -> bool {
        if self.count.is_some_and(|c| completed >= c) {
            return false;
        }
        match self.deadline {
            Some(d) => (completed == 0 && self.at_least_one) || std::time::Instant::now() < d,
            None => true,
        }
    }

    /// Whether a deadline bounds the run (`SEARCHFOR_FUZZ_SECONDS`).
    pub fn timed(&self) -> bool {
        self.deadline.is_some()
    }
}

/// Seconds since the Unix epoch at which every fuzz test of this run stops: set by a fuzz test
/// for the child processes it supervises, which share its deadline.
pub const FUZZ_DEADLINE_VAR: &str = "SEARCHFOR_FUZZ_DEADLINE_MS";

/// A fuzz setting from the environment: absent, or a number; anything else stops the run
/// with a message rather than fuzzing with a default the user did not ask for.
fn fuzz_var<T: std::str::FromStr>(name: &str) -> Option<T> {
    let v = std::env::var_os(name)?;
    let parsed = v.to_str().and_then(|s| s.trim().parse().ok());
    Some(parsed.unwrap_or_else(|| panic!("{name}={v:?} is not a non-negative integer")))
}

/// `SEARCHFOR_FUZZ_SEED`, else `default`.
pub fn fuzz_seed(default: u64) -> u64 {
    fuzz_var("SEARCHFOR_FUZZ_SEED").unwrap_or(default)
}

/// `SEARCHFOR_FUZZ_N`, the inputs of one round, when set; 0 is refused, as a run that could
/// never test anything.
pub fn fuzz_n() -> Option<usize> {
    let n = fuzz_var("SEARCHFOR_FUZZ_N")?;
    assert!(n > 0, "SEARCHFOR_FUZZ_N=0 runs no case");
    Some(n)
}

/// The deadline every fuzz test of this binary shares, and when it is, in milliseconds since
/// the Unix epoch: a supervising parent's (`FUZZ_DEADLINE_VAR`), else `SEARCHFOR_FUZZ_SECONDS`
/// from the first call. With neither, the run is counted, not timed.
fn fuzz_deadline() -> Option<(std::time::Instant, u128, bool)> {
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
    static DEADLINE: std::sync::OnceLock<Option<(Instant, u128, bool)>> =
        std::sync::OnceLock::new();
    *DEADLINE.get_or_init(|| {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        if let Some(at) = fuzz_var::<u128>(FUZZ_DEADLINE_VAR) {
            let left = Duration::from_millis(at.saturating_sub(now_ms) as u64);
            // Case 0 of the run is the first child's, which runs it however late it starts;
            // a later child starts past case 0, so runs nothing past the deadline.
            return Some((Instant::now() + left, at, true));
        }
        let secs: u64 = fuzz_var("SEARCHFOR_FUZZ_SECONDS")?;
        assert!(secs > 0, "SEARCHFOR_FUZZ_SECONDS=0 runs no case");
        let at = now_ms + u128::from(secs) * 1000;
        Some((Instant::now() + Duration::from_secs(secs), at, secs > 0))
    })
}

/// The budget of one fuzz test whose round of N inputs makes `cases_per_round` cases. Counted
/// (no `SEARCHFOR_FUZZ_SECONDS`): one round. Timed: rounds until the deadline shared by the
/// binary's tests, and at least one case; an explicit `SEARCHFOR_FUZZ_N` still caps the run at
/// one round. A test checks `permits` before each round as well as before each case, so a
/// round without cases never spins.
pub fn fuzz_budget(cases_per_round: usize) -> FuzzBudget {
    match fuzz_deadline() {
        None => FuzzBudget::new(None, Some(cases_per_round)),
        Some((deadline, _, at_least_one)) => FuzzBudget {
            deadline: Some(deadline),
            at_least_one,
            count: fuzz_n().map(|_| cases_per_round),
        },
    }
}

/// The environment a supervised fuzz child needs to share this run's deadline.
pub fn fuzz_child_env() -> Option<(&'static str, String)> {
    fuzz_deadline().map(|(_, at, _)| (FUZZ_DEADLINE_VAR, at.to_string()))
}

/// Where a failing fuzz case is saved (`SEARCHFOR_FUZZ_ARTIFACT_DIR`), when set.
pub fn fuzz_artifact_dir() -> Option<PathBuf> {
    std::env::var_os("SEARCHFOR_FUZZ_ARTIFACT_DIR").map(PathBuf::from)
}

/// A fuzz case as saved for replay: the test, the round's seed and N, the case's index in the
/// round, its command line, and its files.
pub struct FuzzCase<'a> {
    pub test: &'a str,
    pub seed: u64,
    pub n: usize,
    pub case: usize,
    pub argv: &'a [&'a std::ffi::OsStr],
    pub files: &'a [(&'a str, &'a [u8])],
    pub stderr: &'a [u8],
    pub what: &'a str,
}

/// The exact bytes of `arg`: the Unix bytes, or the UTF-16LE units of a Windows string, so a
/// command line that is not Unicode replays as it ran.
fn os_bytes(arg: &std::ffi::OsStr) -> Vec<u8> {
    #[cfg(unix)]
    {
        std::os::unix::ffi::OsStrExt::as_bytes(arg).to_vec()
    }
    #[cfg(windows)]
    {
        std::os::windows::ffi::OsStrExt::encode_wide(arg)
            .flat_map(u16::to_le_bytes)
            .collect()
    }
    #[cfg(not(any(unix, windows)))]
    {
        arg.to_string_lossy().into_owned().into_bytes()
    }
}

/// Saves `case` under `SEARCHFOR_FUZZ_ARTIFACT_DIR`, when set, in a directory of its own:
/// `case.txt` (what failed, the seed, N, the case and the replay variables), `argv-<i>.bin`
/// (each argument's exact bytes), `stderr.bin`, and each file under `files/`. Returns it.
pub fn save_fuzz_case(case: &FuzzCase) -> Option<PathBuf> {
    save_fuzz_case_in(&fuzz_artifact_dir()?, case)
}

/// `save_fuzz_case` under `root`.
pub fn save_fuzz_case_in(root: &Path, case: &FuzzCase) -> Option<PathBuf> {
    let dir = root.join(format!("{}-{}-{}", case.test, case.seed, case.case));
    std::fs::create_dir_all(dir.join("files")).ok()?;
    let argv: Vec<String> = case
        .argv
        .iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    let text = format!(
        "test: {}\nfailure: {}\nreplay: SEARCHFOR_FUZZ_SEED={} SEARCHFOR_FUZZ_N={} (case {} of the round)\nargv (lossy; exact bytes in argv-*.bin): {argv:?}\n",
        case.test, case.what, case.seed, case.n, case.case
    );
    let _ = std::fs::write(dir.join("case.txt"), text);
    for (i, a) in case.argv.iter().enumerate() {
        let _ = std::fs::write(dir.join(format!("argv-{i}.bin")), os_bytes(a));
    }
    let _ = std::fs::write(dir.join("stderr.bin"), case.stderr);
    for (name, bytes) in case.files {
        let _ = std::fs::write(dir.join("files").join(name), bytes);
    }
    Some(dir)
}

/// The case a fuzz test is about to run, written to `SEARCHFOR_FUZZ_ARTIFACT_DIR` before it
/// runs and removed when the test ends normally: a crash that ends the whole test process
/// leaves the seed, N and the case behind for the workflow to upload.
pub struct FuzzInFlight {
    path: Option<PathBuf>,
}

impl FuzzInFlight {
    pub fn new(test: &str) -> Self {
        let path = fuzz_artifact_dir().map(|d| d.join(format!("in-flight-{test}.txt")));
        if let Some(p) = &path {
            let _ = std::fs::create_dir_all(p.parent().unwrap());
        }
        Self { path }
    }

    /// Records the case about to run.
    pub fn at(&self, what: impl FnOnce() -> String) {
        if let Some(p) = &self.path {
            let _ = std::fs::write(p, what());
        }
    }

    /// The test ended without a crash: nothing to keep.
    pub fn finish(self) {
        if let Some(p) = &self.path {
            let _ = std::fs::remove_file(p);
        }
    }
}

/// A child process that is killed and reaped when this goes out of scope, a panic included,
/// so a fuzz case that hangs never outlives its test.
pub struct Reaped(pub std::process::Child);

impl Drop for Reaped {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Runs `cmd` with stdout discarded and stderr collected, killed and reaped after `limit`.
/// Returns its exit status (None when it was killed) and stderr.
pub fn run_limited(
    cmd: &mut Command,
    limit: std::time::Duration,
) -> (Option<std::process::ExitStatus>, Vec<u8>) {
    let started = std::time::Instant::now();
    let mut child = Reaped(
        cmd.stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let mut pipe = child.0.stderr.take().unwrap();
    let reader = std::thread::spawn(move || {
        let mut e = Vec::new();
        let _ = std::io::Read::read_to_end(&mut pipe, &mut e);
        e
    });
    let status = loop {
        if let Some(st) = child.0.try_wait().unwrap() {
            break Some(st);
        }
        if started.elapsed() > limit {
            break None;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    };
    // Killed and reaped here when it is still running, so its stderr ends.
    drop(child);
    (status, reader.join().unwrap())
}
