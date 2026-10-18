//! End-to-end tests of Helper Processors and Grammar installs (M2 plan, Task 11.2): the example
//! helper (`helpers/example`) behind an `external` Definition, in its three modes and with its
//! test knobs, and the Lua Grammar Helper installed from a `file://` index. No test uses the
//! network or the user's Definitions and Grammars (`Fixture` isolates them).
//!
//! The knobs of the example helper match a substring of the path it is asked about; they name
//! the file (`db.ini`, `/gen/`) rather than a bare stem, so the random name of the fixture's
//! temporary directory cannot match them by chance.
mod common;

use common::{Fixture, code, run, stderr, stdout};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

/// The example helper, built once per test run.
fn example_helper() -> PathBuf {
    common::workspace_bin("sfor-example-helper", "sfor-example-helper", &[])
}

/// Writes `ini.toml` into the fixture's formats directory: the example helper as an `external`
/// Definition for `.ini` files.
fn ini(fx: &Fixture, mode: &str, timeout_ms: u64) {
    let bin = example_helper();
    ini_command(fx, &[bin.to_str().unwrap()], mode, timeout_ms);
}

/// `ini` with another command.
fn ini_command(fx: &Fixture, command: &[&str], mode: &str, timeout_ms: u64) {
    let def = format!(
        "name = 'ini'\nextensions = ['ini']\nprocessor = 'external'\n[params]\ncommand = {command:?}\nmode = '{mode}'\ntimeout_ms = {timeout_ms}\n"
    );
    std::fs::write(fx.formats.path().join("ini.toml"), def).unwrap();
}

/// `start` lines the example helper wrote to its log: one per process.
fn starts(log: &Path) -> usize {
    std::fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .filter(|l| l.starts_with("start "))
        .count()
}

/// The pids of the `start` lines of the log.
fn pids(log: &Path) -> Vec<u32> {
    std::fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.strip_prefix("start ")?.trim().parse().ok())
        .collect()
}

/// `n` files `gen/g<i>.ini`, each with a `needle` under a section.
fn generate(fx: &Fixture, n: usize) {
    let dir = fx.file("gen");
    std::fs::create_dir_all(&dir).unwrap();
    for i in 1..=n {
        std::fs::write(dir.join(format!("g{i:02}.ini")), "[s]\nk = needle\n").unwrap();
    }
}

/// What `sfor needle ini --flat --sort path` prints with the example helper working.
const INI_TRAILS: &str = "\
ini/app.ini:3: server › host │ host = needle.example
ini/app.ini:8: client › name │ name = needle client
ini/bom.ini:3: server › host │ host = needle.example
ini/bom.ini:8: client › name │ name = needle client
ini/db.ini:2: primary › url │ url = postgres://needle
ini/web.ini:2: http › listen │ listen = needle:80
";

/// The Trails of `app.ini` as `sfor --flat` prints them for a single input.
const APP_TRAILS: &str = "\
3: server › host │ host = needle.example
8: client › name │ name = needle client
";

fn flat_ini(fx: &Fixture, extra: &[&str], env: &[(&str, &str)]) -> Output {
    let mut args = vec!["needle", "ini", "--flat", "--sort", "path"];
    args.extend_from_slice(extra);
    let mut c = fx.sfor(&args);
    for (k, v) in env {
        c.env(k, v);
    }
    run(&mut c)
}

#[test]
fn h01_every_mode_gives_the_same_trails() {
    // Spec §11 #15.
    for mode in ["pool", "concurrent", "oneshot"] {
        let fx = Fixture::new();
        ini(&fx, mode, 2000);
        let o = flat_ini(&fx, &[], &[]);
        assert_eq!(code(&o), 0, "{mode}: {}", stderr(&o));
        assert_eq!(stderr(&o), "", "{mode}");
        assert_eq!(stdout(&o), INI_TRAILS, "{mode}");
    }
}

/// One request in the trace of the example helper (`SFOR_EXAMPLE_TRACE`).
#[derive(Debug, Clone)]
struct Event {
    begin: bool,
    pid: u32,
    path: String,
}

fn trace(log: &Path) -> Vec<Event> {
    std::fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .map(|l| {
            let mut f = l.splitn(3, ' ');
            let (what, pid, path) = (f.next().unwrap(), f.next().unwrap(), f.next().unwrap());
            Event {
                begin: what == "begin",
                pid: pid.parse().unwrap(),
                path: path.to_string(),
            }
        })
        .collect()
}

/// The most requests in flight at once over the trace: across every process, and in one.
fn in_flight(events: &[Event]) -> (usize, usize) {
    let mut all = 0usize;
    let mut per: std::collections::HashMap<u32, usize> = Default::default();
    let (mut max_all, mut max_one) = (0, 0);
    for e in events {
        let n = per.entry(e.pid).or_default();
        if e.begin {
            all += 1;
            *n += 1;
        } else {
            all -= 1;
            *n -= 1;
        }
        max_all = max_all.max(all);
        max_one = max_one.max(*n);
    }
    (max_all, max_one)
}

/// stdout's lines, sorted: a search without `--sort` prints files in any order.
fn sorted_lines(o: &Output) -> Vec<String> {
    let mut v: Vec<String> = stdout(o).lines().map(String::from).collect();
    v.sort();
    v
}

#[test]
fn h02_a_pool_runs_at_most_the_thread_count() {
    // Without `--sort` the search runs on `-j 3` threads (review m2-code-r1, Codex #4). The
    // helper's barrier (`SFOR_EXAMPLE_BARRIER`) holds every request until 3 are in flight, or
    // all that are left, so the trace shows exactly 3 at once whatever the scheduler does (M3
    // code review r2, Codex #5): a 4th that sfor sent would arrive while 3 are held. A pool
    // then needs 3 processes, one request each; timeouts are tested elsewhere (h06), and 30 s is
    // more than a loaded machine takes (Fable #1).
    for (mode, processes, per_process) in [("pool", 3, 1), ("concurrent", 1, 3), ("oneshot", 12, 1)]
    {
        let fx = Fixture::new();
        generate(&fx, 12);
        ini(&fx, mode, 30_000);
        let log = fx.file("helper.log");
        let tr = fx.file("trace.log");
        let o = run(fx
            .sfor(["needle", "gen", "--flat", "-j", "3"])
            .env("SFOR_EXAMPLE_BARRIER", "3 12")
            .env("SFOR_EXAMPLE_LOG", &log)
            .env("SFOR_EXAMPLE_TRACE", &tr));
        assert_eq!(code(&o), 0, "{mode}: {}", stderr(&o));
        assert_eq!(starts(&log), processes, "{mode}: processes");
        let events = trace(&tr);
        assert_eq!(events.len(), 24, "{mode}: {events:?}");
        let (all, one) = in_flight(&events);
        assert_eq!(all, 3, "{mode}: {all} requests in flight at most");
        assert_eq!(one, per_process, "{mode}: {one} in one process");
        let want: Vec<String> = (1..=12)
            .map(|i| format!("gen/g{i:02}.ini:2: s › k │ k = needle"))
            .collect();
        assert_eq!(sorted_lines(&o), want, "{mode}");
    }
}

#[test]
fn h03_out_of_order_answers_reach_their_files() {
    // One `concurrent` process, `-j 4`, no `--sort`: the first `s*` request is held until a
    // request sent after it is answered (`SFOR_EXAMPLE_OVERTAKE`), so an answer overtakes
    // another whatever order the walker gives the files (M3 code review r2, Fable #1), and each
    // answer still reaches its own file (review m2-code-r1, Codex #4). The other seven `s*`
    // files come after it and are not held.
    let fx = Fixture::new();
    let dir = fx.file("gen");
    std::fs::create_dir_all(&dir).unwrap();
    for i in 1..=8 {
        for kind in ["s", "f"] {
            let body = format!("[{kind}{i}]\nk = needle\n");
            std::fs::write(dir.join(format!("{kind}{i}.ini")), body).unwrap();
        }
    }
    ini(&fx, "concurrent", 30_000);
    let log = fx.file("helper.log");
    let tr = fx.file("trace.log");
    let o = run(fx
        .sfor(["needle", "gen", "--flat", "-j", "4"])
        .env("SFOR_EXAMPLE_OVERTAKE", "/gen/s")
        .env("SFOR_EXAMPLE_LOG", &log)
        .env("SFOR_EXAMPLE_TRACE", &tr));
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    assert_eq!(starts(&log), 1);
    let events = trace(&tr);
    let begins: Vec<&str> = events
        .iter()
        .filter(|e| e.begin)
        .map(|e| e.path.as_str())
        .collect();
    let ends: Vec<&str> = events
        .iter()
        .filter(|e| !e.begin)
        .map(|e| e.path.as_str())
        .collect();
    assert_eq!(begins.len(), 16, "{events:?}");
    let pos = |v: &[&str], p: &str| v.iter().position(|x| *x == p).unwrap();
    let overtaken = begins.iter().any(|a| {
        begins
            .iter()
            .any(|b| pos(&begins, a) < pos(&begins, b) && pos(&ends, b) < pos(&ends, a))
    });
    assert!(overtaken, "no answer overtook another: {events:?}");
    let mut want: Vec<String> = (1..=8)
        .flat_map(|i| ["s", "f"].map(|k| format!("gen/{k}{i}.ini:2: {k}{i} › k │ k = needle")))
        .collect();
    want.sort();
    assert_eq!(sorted_lines(&o), want);
}

#[test]
fn h03b_copies_that_ignore_eof_close_together() {
    // Review m2-code-r1, Fable #1: two Definitions, several copies each, every one ignoring
    // the EOF of its stdin. They all get their second of grace at once, so the run ends about
    // a second after the search instead of a second per copy (spec §4.4, Lifetime).
    let fx = Fixture::new();
    let bin = example_helper();
    for (name, ext) in [("ini", "ini"), ("cfg", "cfg")] {
        let def = format!(
            "name = '{name}'\nextensions = ['{ext}']\nprocessor = 'external'\n[params]\ncommand = [{:?}]\nmode = 'pool'\ntimeout_ms = 2000\n",
            bin.to_str().unwrap()
        );
        std::fs::write(fx.formats.path().join(format!("{name}.toml")), def).unwrap();
    }
    let dir = fx.file("gen");
    std::fs::create_dir_all(&dir).unwrap();
    for i in 1..=8 {
        for ext in ["ini", "cfg"] {
            std::fs::write(dir.join(format!("g{i:02}.{ext}")), "[s]\nk = needle\n").unwrap();
        }
    }
    let log = fx.file("helper.log");
    let started = Instant::now();
    let o = run(fx
        .sfor(["needle", "gen", "--flat", "-j", "4"])
        .env("SFOR_EXAMPLE_SLOW", "/gen/")
        .env("SFOR_EXAMPLE_NOEXIT", "1")
        .env("SFOR_EXAMPLE_LOG", &log));
    let took = started.elapsed();
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    assert_eq!(stdout(&o).lines().count(), 16);
    let copies = starts(&log);
    assert!(copies >= 4, "{copies} copies");
    // 16 requests of 300 ms on 4 threads (1.2 s), then one second of grace for all of them;
    // one second per copy would be over 5 s.
    assert!(
        took < Duration::from_millis(4500),
        "{took:?} for {copies} copies"
    );
    for pid in pids(&log) {
        assert!(!alive(pid), "copy {pid} outlived sfor");
    }
}

#[test]
fn h04_a_crash_is_plain_with_a_warning_and_a_restart() {
    // Spec §11 #11.
    let fx = Fixture::new();
    ini(&fx, "pool", 2000);
    let log = fx.file("helper.log");
    let log_s = log.to_str().unwrap();
    let o = flat_ini(
        &fx,
        &["-j", "1"],
        &[
            ("SFOR_EXAMPLE_CRASH", "db.ini"),
            ("SFOR_EXAMPLE_LOG", log_s),
        ],
    );
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let want = INI_TRAILS.replace(
        "ini/db.ini:2: primary › url │ url = postgres://needle",
        "ini/db.ini:2: url = postgres://needle",
    );
    assert_eq!(stdout(&o), want);
    let err = stderr(&o);
    let lines: Vec<_> = err.lines().collect();
    assert_eq!(lines.len(), 1, "{err}");
    assert!(
        lines[0].contains("ini") && lines[0].contains("ini/db.ini"),
        "{err}"
    );
    assert_eq!(starts(&log), 2);
}

#[test]
fn h05_three_failures_disable_the_definition() {
    let fx = Fixture::new();
    generate(&fx, 6);
    ini(&fx, "pool", 2000);
    let o = run(fx
        .sfor(["needle", "gen", "--flat", "--sort", "path", "-j", "1"])
        .env("SFOR_EXAMPLE_CRASH", "/gen/"));
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let err = stderr(&o);
    let lines: Vec<_> = err.lines().collect();
    assert_eq!(lines.len(), 4, "{err}");
    let disabled: Vec<_> = lines
        .iter()
        .filter(|l| l.contains("failed 3 times"))
        .collect();
    assert_eq!(disabled.len(), 1, "{err}");
    // The three failures are the first three files', and nothing follows the fourth line.
    for i in 1..=3 {
        let file = format!("gen/g{i:02}.ini");
        assert_eq!(
            lines.iter().filter(|l| l.contains(&file)).count(),
            1,
            "{err}"
        );
    }
    assert!(!err.contains("gen/g04.ini"), "{err}");
    // The notice that disables the Definition follows the warning of the file that caused it
    // (review m2-code-r1, Fable #5).
    assert!(lines[2].contains("gen/g03.ini"), "{err}");
    assert!(lines[3].contains("failed 3 times"), "{err}");
    let want: String = (1..=6)
        .map(|i| format!("gen/g{i:02}.ini:2: k = needle\n"))
        .collect();
    assert_eq!(stdout(&o), want);
}

#[test]
fn h06_a_timeout_kills_and_goes_on() {
    let fx = Fixture::new();
    ini(&fx, "pool", 300);
    let started = Instant::now();
    let o = flat_ini(&fx, &[], &[("SFOR_EXAMPLE_HANG", "web.ini")]);
    let took = started.elapsed();
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    assert!(
        stderr(&o).contains("did not answer within 300 ms"),
        "{}",
        stderr(&o)
    );
    assert!(took < Duration::from_secs(5), "took {took:?}");
    let want = INI_TRAILS.replace(
        "ini/web.ini:2: http › listen │ listen = needle:80",
        "ini/web.ini:2: listen = needle:80",
    );
    assert_eq!(stdout(&o), want);
}

#[test]
fn h07_a_broken_line_is_a_failure() {
    let fx = Fixture::new();
    ini(&fx, "pool", 2000);
    let o = flat_ini(&fx, &[], &[("SFOR_EXAMPLE_GARBAGE", "web.ini")]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    // The plan's test said `not protocol JSON`; the host the plan specifies (Task 4.2) writes
    // `wrote a line that is not JSON (…)`.
    assert!(
        stderr(&o).contains("wrote a line that is not JSON"),
        "{}",
        stderr(&o)
    );
    let want = INI_TRAILS.replace(
        "ini/web.ini:2: http › listen │ listen = needle:80",
        "ini/web.ini:2: listen = needle:80",
    );
    assert_eq!(stdout(&o), want);
}

/// `INI_TRAILS` as Plain lines: `path:line: text`.
fn ini_plain() -> String {
    INI_TRAILS
        .lines()
        .map(|l| {
            let (head, text) = l.split_once(" │ ").unwrap();
            let at = head.find(": ").unwrap();
            format!("{}: {text}\n", &head[..at])
        })
        .collect()
}

#[test]
fn h08_a_missing_command_is_one_warning() {
    let fx = Fixture::new();
    ini_command(&fx, &["/no/such/helper"], "pool", 2000);
    let o = flat_ini(&fx, &[], &[]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let err = stderr(&o);
    assert_eq!(err.lines().count(), 1, "{err}");
    assert!(err.contains("cannot start"), "{err}");
    assert_eq!(stdout(&o), ini_plain());
}

#[test]
fn h08b_a_oneshot_helper_that_exits_before_hello_is_one_warning() {
    // Spec §4.4 (review m2-code-r1, Codex #5): the first process that exits before answering
    // `hello` disables the Definition, in `oneshot` as in the other modes: one start, one
    // warning, Plain output.
    let fx = Fixture::new();
    let log = fx.file("starts.log");
    let script = format!("echo start >> '{}'; exit 1", log.display());
    ini_command(&fx, &["/bin/sh", "-c", &script], "oneshot", 2000);
    let o = flat_ini(&fx, &[], &[]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let err = stderr(&o);
    assert_eq!(err.lines().count(), 1, "{err}");
    assert!(err.contains("before answering hello"), "{err}");
    assert_eq!(stdout(&o), ini_plain());
    assert_eq!(std::fs::read_to_string(&log).unwrap().lines().count(), 1);
}

/// The entries of `dir` named `sfor-*`.
fn sfor_dirs(dir: &Path) -> Vec<String> {
    let mut v: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("sfor-"))
        .collect();
    v.sort();
    v
}

#[test]
fn h09_stdin_goes_through_a_temporary_file() {
    let fx = Fixture::new();
    ini(&fx, "pool", 2000);
    // sfor's temporary directory is the system's (`TMPDIR`); a private one keeps other tests'
    // runs out of the listing.
    let tmp = tempfile::tempdir().unwrap();
    let before = sfor_dirs(tmp.path());
    let o = run(fx
        .sfor(["--format", "ini", "needle", "-", "--flat"])
        .env("TMPDIR", tmp.path())
        .stdin(std::fs::File::open(fx.file("ini/app.ini")).unwrap()));
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    assert_eq!(stderr(&o), "");
    assert_eq!(stdout(&o), APP_TRAILS);
    assert_eq!(sfor_dirs(tmp.path()), before);
}

#[test]
fn h10_a_bom_file_keeps_its_offsets() {
    let fx = Fixture::new();
    ini(&fx, "pool", 2000);
    let app = run(&mut fx.sfor(["needle", "ini/app.ini", "--flat"]));
    let bom = run(&mut fx.sfor(["needle", "ini/bom.ini", "--flat"]));
    assert_eq!(stdout(&app), APP_TRAILS, "{}", stderr(&app));
    assert_eq!(stdout(&bom), APP_TRAILS, "{}", stderr(&bom));
    // A Match at the start of a line: an offset counted without the BOM, read by the helper in
    // the file with it, would land 3 bytes early, on the key of the line before (`client › retries`).
    for pattern in ["needle", "^name"] {
        let app = run(&mut fx.sfor([pattern, "ini/app.ini", "--tree"]));
        let bom = run(&mut fx.sfor([pattern, "ini/bom.ini", "--tree"]));
        assert_eq!(
            stdout(&bom).replace("ini/bom.ini", "ini/app.ini"),
            stdout(&app),
            "{pattern}"
        );
    }
    // A Match on a key's name is not inside the key (D18).
    let bom = run(&mut fx.sfor(["^name", "ini/bom.ini", "--flat"]));
    assert_eq!(stdout(&bom), "8: client │ name = needle client\n");
}

#[test]
fn h11_in_headings_asks_for_names() {
    let fx = Fixture::new();
    ini(&fx, "pool", 2000);
    let o = run(&mut fx.sfor(["--in-headings", "^host$", "ini", "--flat", "--sort", "path"]));
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let out = stdout(&o);
    let lines: Vec<_> = out.lines().collect();
    assert_eq!(lines.len(), 2, "{out}");
    assert!(lines[0].starts_with("ini/app.ini:3:"), "{out}");
    assert!(lines[1].starts_with("ini/bom.ini:3:"), "{out}");
    assert!(lines.iter().all(|l| l.contains("host")), "{out}");
}

#[test]
fn h12_helper_stderr_is_relayed() {
    let fx = Fixture::new();
    ini(&fx, "pool", 2000);
    let knob = [("SFOR_EXAMPLE_STDERR", "hello from the helper")];
    let o = flat_ini(&fx, &["-j", "1"], &knob);
    assert_eq!(code(&o), 0);
    assert!(
        stderr(&o).contains("sfor: ini: hello from the helper"),
        "{}",
        stderr(&o)
    );
    assert_eq!(stdout(&o), INI_TRAILS);
    let o = flat_ini(&fx, &["-j", "1", "--no-messages"], &knob);
    assert_eq!(code(&o), 0);
    assert_eq!(stderr(&o), "");
    assert_eq!(stdout(&o), INI_TRAILS);
}

#[cfg(target_os = "linux")]
fn alive(pid: u32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

#[cfg(target_os = "linux")]
#[test]
fn h13_helpers_exit_with_the_run() {
    for mode in ["pool", "concurrent", "oneshot"] {
        let fx = Fixture::new();
        ini(&fx, mode, 2000);
        let log = fx.file("helper.log");
        let o = flat_ini(
            &fx,
            &["-j", "4"],
            &[("SFOR_EXAMPLE_LOG", log.to_str().unwrap())],
        );
        assert_eq!(stdout(&o), INI_TRAILS, "{mode}: {}", stderr(&o));
        let pids = pids(&log);
        assert!(!pids.is_empty(), "{mode}");
        let left: Vec<_> = pids.into_iter().filter(|p| alive(*p)).collect();
        assert!(left.is_empty(), "{mode}: still under /proc: {left:?}");
    }
}

/// Hex SHA-256 of `bytes`.
fn sha256(bytes: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// A `file://` index in `dir` with one entry, `lua`, whose binary for this system is the Lua
/// Grammar Helper copied to `dir/payload`; `sha256` replaces its checksum when given.
fn lua_index(dir: &Path, sha256_override: Option<&str>) -> String {
    lua_index_with(dir, sha256_override, "")
}

/// `lua_index`, with `params` appended to the `[params]` of its Definition.
fn lua_index_with(dir: &Path, sha256_override: Option<&str>, params: &str) -> String {
    let bin = common::workspace_bin("sfor-grammar", "sfor-grammar", &["lua"]);
    let payload = dir.join("payload");
    std::fs::copy(&bin, &payload).unwrap();
    let sum = sha256(&std::fs::read(&payload).unwrap());
    let definition = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/helpers/grammar/definitions/lua.toml"
    ))
    .unwrap()
        + params;
    let mut binaries = serde_json::Map::new();
    binaries.insert(
        searchfor::grammars::index::system(),
        serde_json::json!({
            "url": format!("file://{}", payload.display()),
            "sha256": sha256_override.unwrap_or(&sum),
        }),
    );
    let index = serde_json::json!({
        "version": 1,
        "grammars": [{
            "name": "lua",
            "version": "0.2.0",
            "source": "https://crates.io/crates/tree-sitter-lua",
            "license": "MIT",
            "definition": definition,
            "binaries": binaries,
        }],
    });
    let path = dir.join("index.json");
    std::fs::write(&path, serde_json::to_string_pretty(&index).unwrap()).unwrap();
    format!("file://{}", path.display())
}

#[test]
fn h14_install_a_grammar_and_use_it() {
    // Spec §11 #17.
    let fx = Fixture::new();
    let tmp = tempfile::tempdir().unwrap();
    let index = lua_index(tmp.path(), None);
    let sfor = |args: &[&str]| run(fx.sfor(args).env("SFOR_GRAMMAR_INDEX", &index));
    let o = sfor(&["--install-grammar", "lua"]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let o = sfor(&["--list-grammars"]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let list = stdout(&o);
    assert!(
        list.lines()
            .any(|l| l.starts_with("lua ") && l.contains("installed") && l.contains("0.2.0")),
        "{list}"
    );
    let o = sfor(&["--list-formats"]);
    let formats = stdout(&o);
    assert!(
        formats
            .lines()
            .any(|l| l.starts_with("lua ") && l.contains("installed (")),
        "{formats}"
    );
    std::fs::copy(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/helpers/grammar/tests/fixtures/player.lua"
        ),
        fx.file("player.lua"),
    )
    .unwrap();
    let o = sfor(&["needle", "player.lua", "--flat"]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    assert_eq!(stderr(&o), "");
    let out = stdout(&o);
    let lines: Vec<_> = out.lines().collect();
    assert_eq!(lines.len(), 2, "{out}");
    assert!(lines[0].starts_with("4: Player:update │ "), "{out}");
    assert!(lines[1].starts_with("8: helper │ "), "{out}");
}

#[test]
fn h15b_a_grammar_helper_keeps_max_bytes() {
    // Spec §5.3: a file over `max_bytes` is Plain, with one warning per Format, also when a
    // Grammar Helper serves the Format (review m2-code-r1, Codex #2). Both `trails` and
    // `names` stop before the helper.
    let fx = Fixture::new();
    let tmp = tempfile::tempdir().unwrap();
    let index = lua_index_with(tmp.path(), None, "max_bytes = 10\n");
    let sfor = |args: &[&str]| run(fx.sfor(args).env("SFOR_GRAMMAR_INDEX", &index));
    let o = sfor(&["--install-grammar", "lua"]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let player = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/helpers/grammar/tests/fixtures/player.lua"
    );
    std::fs::create_dir(fx.file("lua")).unwrap();
    for f in ["a.lua", "b.lua"] {
        std::fs::copy(player, fx.file("lua").join(f)).unwrap();
    }
    let o = sfor(&["needle", "lua", "--flat", "--sort", "path"]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    assert_eq!(
        stdout(&o),
        "lua/a.lua:4:   self.x = self.x + dt -- needle\n\
         lua/a.lua:8:   return \"needle\"\n\
         lua/b.lua:4:   self.x = self.x + dt -- needle\n\
         lua/b.lua:8:   return \"needle\"\n"
    );
    assert_eq!(
        stderr(&o),
        "sfor: lua/a.lua: larger than `max_bytes` (10); files this large are not parsed (this warning is shown once)\n"
    );
    let o = sfor(&["--in-headings", "update", "lua", "--flat", "--sort", "path"]);
    assert_eq!(code(&o), 1, "{}", stdout(&o));
    assert_eq!(stderr(&o).matches("max_bytes").count(), 1, "{}", stderr(&o));
}

#[test]
fn h15_a_wrong_checksum_installs_nothing() {
    let fx = Fixture::new();
    let tmp = tempfile::tempdir().unwrap();
    let index = lua_index(tmp.path(), Some(&"0".repeat(64)));
    let o = run(fx
        .sfor(["--install-grammar", "lua"])
        .env("SFOR_GRAMMAR_INDEX", &index));
    assert_eq!(code(&o), 2, "{}", stderr(&o));
    assert!(stderr(&o).contains("SHA-256"), "{}", stderr(&o));
    assert!(!fx.grammars.path().join("lua").exists());
}

/// Polls `f` every 10 ms until it holds or `limit` passes.
fn wait_for(limit: Duration, mut f: impl FnMut() -> bool) -> bool {
    let end = Instant::now() + limit;
    loop {
        if f() {
            return true;
        }
        if Instant::now() >= end {
            return false;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Waits up to `limit` for `child` to exit.
#[cfg(target_os = "linux")]
fn exit_within(child: &mut std::process::Child, limit: Duration) -> std::process::ExitStatus {
    let mut status = None;
    wait_for(limit, || {
        status = child.try_wait().unwrap();
        status.is_some()
    });
    status.unwrap_or_else(|| {
        let _ = child.kill();
        let _ = child.wait();
        panic!("sfor did not exit within {limit:?}")
    })
}

#[cfg(target_os = "linux")]
fn sigint(target: &str) {
    let ok = Command::new("kill")
        .args(["-INT", "--", target])
        .status()
        .unwrap()
        .success();
    assert!(ok, "kill -INT -- {target}");
}

#[cfg(target_os = "linux")]
#[test]
fn h16_ctrl_c_ends_every_helper() {
    use std::os::unix::process::{CommandExt, ExitStatusExt};
    const SIGINT: i32 = 2;
    for group in [true, false] {
        let fx = Fixture::new();
        let bin = example_helper();
        let script = format!("trap '' INT; exec '{}'", bin.display());
        // The request for web.ini hangs; the timeout is far beyond the test's limits.
        ini_command(&fx, &["sh", "-c", &script], "pool", 60_000);
        let log = fx.file("helper.log");
        let mut c = fx.sfor(["needle", "ini/web.ini", "--flat"]);
        c.env("SFOR_EXAMPLE_HANG", "web.ini")
            .env("SFOR_EXAMPLE_LOG", &log)
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if group {
            c.process_group(0);
        }
        let mut child = c.spawn().unwrap();
        assert!(
            wait_for(Duration::from_secs(10), || starts(&log) > 0),
            "no helper started"
        );
        let pid = child.id();
        // (a) the terminal's Ctrl-C reaches the whole group, whose id is sfor's pid; (b) sfor
        // alone.
        sigint(&if group {
            format!("-{pid}")
        } else {
            pid.to_string()
        });
        let status = exit_within(&mut child, Duration::from_secs(2));
        assert_eq!(status.signal(), Some(SIGINT), "group {group}: {status:?}");
        let left: Vec<_> = pids(&log).into_iter().filter(|p| alive(*p)).collect();
        assert!(
            left.is_empty(),
            "group {group}: still under /proc: {left:?}"
        );
    }
    // (c) No helper: nothing is installed, and SIGINT kills sfor as it kills rg.
    let fx = Fixture::new();
    let fifo = fx.file("never.md");
    assert!(
        Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success()
    );
    // Read and write, so opening it does not wait for a reader, and it never ends.
    let _writer = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&fifo)
        .unwrap();
    let mut child = fx
        .sfor(["--format", "markdown", "needle", "never.md"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let fd_dir = format!("/proc/{}/fd", child.id());
    let opened = wait_for(Duration::from_secs(10), || {
        std::fs::read_dir(&fd_dir).is_ok_and(|d| {
            d.filter_map(|e| std::fs::read_link(e.ok()?.path()).ok())
                .any(|t| t == fifo)
        })
    });
    assert!(opened, "sfor did not open the FIFO");
    sigint(&child.id().to_string());
    let status = exit_within(&mut child, Duration::from_secs(2));
    assert_eq!(status.signal(), Some(SIGINT), "{status:?}");
}

#[cfg(target_os = "linux")]
#[test]
fn h17_a_helpers_child_cannot_hold_sfor() {
    let fx = Fixture::new();
    ini(&fx, "pool", 2000);
    let started = Instant::now();
    let o = flat_ini(&fx, &["-j", "1"], &[("SFOR_EXAMPLE_FORK", "1")]);
    let took = started.elapsed();
    assert!(took < Duration::from_secs(4), "took {took:?}");
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    assert_eq!(stdout(&o), INI_TRAILS);
}

/// Runs `c` with `ini/app.ini` on stdin, as a regular file or through a pipe.
#[cfg(target_os = "linux")]
fn with_app_on_stdin(fx: &Fixture, mut c: Command, pipe: bool) -> (Output, Duration) {
    let app = std::fs::read(fx.file("ini/app.ini")).unwrap();
    let started = Instant::now();
    let o = if pipe {
        let mut child = c
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut input = child.stdin.take().unwrap();
        let writer = std::thread::spawn(move || {
            use std::io::Write;
            let _ = input.write_all(&app);
        });
        let o = child.wait_with_output().unwrap();
        writer.join().unwrap();
        o
    } else {
        run(c.stdin(std::fs::File::open(fx.file("ini/app.ini")).unwrap()))
    };
    (o, started.elapsed())
}

#[cfg(target_os = "linux")]
fn assert_app_trails(what: &str, o: &Output, took: Duration) {
    let err = stderr(o);
    assert_eq!(code(o), 0, "{what}: {err}");
    assert!(!err.contains("did not answer"), "{what}: {err}");
    let out = stdout(o);
    assert!(
        out.contains("server › host │ host = needle.example")
            && out.contains("client › name │ name = needle client"),
        "{what}: {out}"
    );
    assert!(took < Duration::from_secs(4), "{what}: took {took:?}");
}

#[cfg(target_os = "linux")]
#[test]
fn h18_streams_named_on_the_command_line_go_through_a_temporary_file() {
    let fx = Fixture::new();
    ini(&fx, "pool", 2000);
    let named = |path: &str| fx.sfor(["--format", "ini", "needle", path, "--flat"]);
    for pipe in [true, false] {
        let (o, t) = with_app_on_stdin(&fx, named("/dev/stdin"), pipe);
        assert_app_trails(&format!("/dev/stdin, pipe {pipe}"), &o, t);
    }
    let (o, t) = with_app_on_stdin(&fx, named("/dev/fd/0"), false);
    assert_app_trails("/dev/fd/0, file", &o, t);
    // A FIFO filled by a writer thread.
    let fifo = fx.file("app.fifo");
    assert!(
        Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success()
    );
    let app = std::fs::read(fx.file("ini/app.ini")).unwrap();
    let to = fifo.clone();
    let writer = std::thread::spawn(move || {
        use std::io::Write;
        // Opening for writing waits for sfor to open it for reading.
        let mut f = std::fs::OpenOptions::new().write(true).open(to).unwrap();
        let _ = f.write_all(&app);
    });
    let started = Instant::now();
    let o = run(&mut named(fifo.to_str().unwrap()));
    assert_app_trails("FIFO", &o, started.elapsed());
    writer.join().unwrap();
    // `stdin` in /dev names sfor's own stdin.
    for pipe in [true, false] {
        let mut c = named("stdin");
        c.current_dir("/dev");
        let (o, t) = with_app_on_stdin(&fx, c, pipe);
        assert_app_trails(&format!("stdin in /dev, pipe {pipe}"), &o, t);
    }
    // A symlink to /dev/stdin, with the Format of its own name.
    std::os::unix::fs::symlink("/dev/stdin", fx.file("ini/link.ini")).unwrap();
    for pipe in [true, false] {
        let c = fx.sfor(["needle", "ini/link.ini", "--flat"]);
        let (o, t) = with_app_on_stdin(&fx, c, pipe);
        assert_app_trails(&format!("ini/link.ini, pipe {pipe}"), &o, t);
    }
}

#[test]
fn h19_a_failing_helper_is_plain_like_rg() {
    // D33: a file whose Processor fails prints as Plain, exactly as a `.txt` copy of it.
    let fx = Fixture::new();
    ini(&fx, "pool", 2000);
    let line = format!("k = {}needle{}\n", "x".repeat(2500), "y".repeat(2490));
    assert_eq!(line.trim_end().len(), 5000);
    let body = format!("[s]\n{line}");
    std::fs::write(fx.file("long.ini"), &body).unwrap();
    std::fs::write(fx.file("long.txt"), &body).unwrap();
    let knob = ("SFOR_EXAMPLE_CRASH", "long.ini");
    for layout in [&["--flat"][..], &["--tree"], &[]] {
        let mut ini_args = vec!["needle", "long.ini"];
        ini_args.extend_from_slice(layout);
        let mut txt_args = vec!["needle", "long.txt"];
        txt_args.extend_from_slice(layout);
        let got = run(fx.sfor(&ini_args).env(knob.0, knob.1));
        let want = run(&mut fx.sfor(&txt_args));
        assert_eq!(code(&got), 0, "{layout:?}: {}", stderr(&got));
        assert!(stderr(&got).contains("long.ini"), "{}", stderr(&got));
        // The tree names its file.
        assert_eq!(
            stdout(&got).replace("long.ini", "long.txt"),
            stdout(&want),
            "{layout:?}"
        );
    }
    let o = run(fx
        .sfor(["needle", "long.ini", "--json"])
        .env(knob.0, knob.1));
    let m: Vec<serde_json::Value> = stdout(&o)
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .filter(|v: &serde_json::Value| v["type"] == "match")
        .collect();
    assert_eq!(m.len(), 1, "{}", stdout(&o));
    assert!(m[0]["data"]["format"].is_null(), "{}", m[0]);
}
