//! Memory and time ceilings of the Trail pipeline (code review r1, Fable #1). Ignored by
//! default because they write a 50 MB file and run the binary several times; run them with
//! `cargo test --release --test perf -- --ignored --nocapture`. `SFOR_BIN` runs them against
//! another build of sfor (to see them fail before a fix). They measure wall time, so they run
//! one at a time (`alone`): cargo's default of one thread per test made them load each other
//! (code review r6, Fable #1).
mod common;

use common::Fixture;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// Taken by every perf test for its whole run.
static LOCK: Mutex<()> = Mutex::new(());

/// The perf tests measuring right now.
static RUNNING: AtomicUsize = AtomicUsize::new(0);

/// Held by a perf test while it measures; no other perf test measures meanwhile. `Drop` runs
/// before the guard is released.
struct Alone {
    _guard: MutexGuard<'static, ()>,
}

impl Drop for Alone {
    fn drop(&mut self) {
        RUNNING.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Called first by every test: waits for the test measuring now to end.
fn alone() -> Alone {
    // A test that failed poisons the lock; the next one still runs alone.
    let guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let before = RUNNING.fetch_add(1, Ordering::SeqCst);
    assert_eq!(before, 0, "another perf test is measuring at the same time");
    Alone { _guard: guard }
}

const SIZE: usize = 50 * 1024 * 1024;

/// A file of `SIZE` bytes made of short lines, none of which contains `zzz`.
fn big_file(dir: &Path, name: &str) -> std::path::PathBuf {
    let line = b"line of text here\n";
    let mut v = Vec::with_capacity(SIZE + 64);
    let mut n = 0;
    while v.len() < SIZE {
        // Markdown gets a heading every 1000 lines, so every row has a Trail.
        if name.ends_with(".md") && n % 1000 == 0 {
            v.extend_from_slice(format!("# Section {n}\n\n").as_bytes());
        }
        v.extend_from_slice(line);
        n += 1;
    }
    let p = dir.join(name);
    std::fs::write(&p, v).unwrap();
    p
}

fn sfor_bin() -> std::path::PathBuf {
    std::env::var_os("SFOR_BIN")
        .map(Into::into)
        .unwrap_or_else(|| assert_cmd::cargo::cargo_bin("sfor"))
}

/// Wall time and peak RSS in bytes of `bin args` (sfor or rg), stdout discarded, measured by
/// `/usr/bin/time -v`. None when /usr/bin/time is missing.
fn measure(bin: &Path, fx: &Fixture, args: &[&str]) -> Option<(Duration, u64)> {
    measure_with(bin, fx, args, None)
}

/// `measure` with stdin read from `stdin`, a file in the fixture copy.
fn measure_with(
    bin: &Path,
    fx: &Fixture,
    args: &[&str],
    stdin: Option<&str>,
) -> Option<(Duration, u64)> {
    measure_to(bin, fx, args, stdin, Stdio::null())
}

/// `measure` with stdout read and thrown away: ripgrep stops at the first Match when its
/// stdout is /dev/null, as `-q` does, so a comparison with rg must not give it /dev/null.
fn measure_drained(bin: &Path, fx: &Fixture, args: &[&str]) -> Option<(Duration, u64)> {
    measure_to(bin, fx, args, None, Stdio::piped())
}

fn measure_to(
    bin: &Path,
    fx: &Fixture,
    args: &[&str],
    stdin: Option<&str>,
    stdout: Stdio,
) -> Option<(Duration, u64)> {
    let time = Path::new("/usr/bin/time");
    if !time.exists() {
        common::missing_reference("/usr/bin/time");
        return None;
    }
    let mut c = Command::new(time);
    c.arg("-v")
        .arg(bin)
        .args(args)
        .current_dir(fx.path())
        .env("SFOR_FORMATS_DIR", fx.formats.path())
        .env("SFOR_GRAMMARS_DIR", fx.grammars.path())
        .env("SFOR_GRAMMAR_INDEX", fx.no_index())
        .env_remove("RIPGREP_CONFIG_PATH")
        .stdin(match stdin {
            None => Stdio::null(),
            Some(f) => Stdio::from(std::fs::File::open(fx.file(f)).unwrap()),
        })
        .stdout(stdout)
        .stderr(Stdio::piped());
    let started = Instant::now();
    let o = c.output().unwrap();
    let elapsed = started.elapsed();
    let err = String::from_utf8_lossy(&o.stderr);
    let kb: u64 = err
        .lines()
        .find_map(|l| {
            l.trim()
                .strip_prefix("Maximum resident set size (kbytes): ")
        })
        .unwrap_or_else(|| panic!("no RSS in: {err}"))
        .parse()
        .unwrap();
    Some((elapsed, kb * 1024))
}

/// The fastest of three runs and the largest RSS seen.
fn best_of_three(fx: &Fixture, args: &[&str]) -> Option<(Duration, u64)> {
    best_of_three_bin(&sfor_bin(), fx, args)
}

fn best_of_three_bin(bin: &Path, fx: &Fixture, args: &[&str]) -> Option<(Duration, u64)> {
    let mut best: Option<(Duration, u64)> = None;
    for _ in 0..3 {
        let (t, rss) = measure(bin, fx, args)?;
        best = Some(match best {
            None => (t, rss),
            Some((bt, br)) => (bt.min(t), br.max(rss)),
        });
    }
    best
}

fn ceiling(name: &str, args: &[&str]) {
    let fx = Fixture::new();
    let p = big_file(fx.path(), name);
    let file = p.file_name().unwrap().to_str().unwrap();
    let mut trail = args.to_vec();
    trail.push(file);
    let Some((t, rss)) = best_of_three(&fx, &trail) else {
        eprintln!("skipping: /usr/bin/time not found");
        return;
    };
    let mut plain = vec!["--no-trail"];
    plain.extend(args.iter().filter(|a| **a != "--flat"));
    plain.push(file);
    let (base, base_rss) = best_of_three(&fx, &plain).unwrap();
    eprintln!(
        "{trail:?}: {t:?}, RSS {} MB; --no-trail: {base:?}, RSS {} MB; file {} MB",
        rss / 1_048_576,
        base_rss / 1_048_576,
        SIZE / 1_048_576
    );
    assert!(rss < 4 * SIZE as u64, "RSS {rss} is not below 4 × {SIZE}");
    assert!(
        t <= base * 3,
        "{t:?} is more than 3 × the {base:?} of --no-trail"
    );
}

#[test]
#[ignore]
fn inverted_search_on_a_big_plain_file_stays_under_the_ceilings() {
    let _alone = alone();
    ceiling("big.txt", &["--flat", "-v", "zzz"]);
}

#[test]
#[ignore]
fn every_line_matching_in_a_big_plain_file_stays_under_the_ceilings() {
    let _alone = alone();
    ceiling("big.txt", &["--flat", "text"]);
}

/// A Markdown file pays its Processor (pulldown-cmark's tree of the whole file) once; the rows
/// add their output and no model: RSS under the Processor's own (measured with `--in-headings`
/// and a pattern no name has, which parses and prints nothing) plus 3 × the file, and time
/// within 3 × the Processor's plus `--no-trail`'s.
fn markdown_ceiling(args: &[&str]) {
    let fx = Fixture::new();
    let p = big_file(fx.path(), "big.md");
    let file = p.file_name().unwrap().to_str().unwrap();
    let mut trail = args.to_vec();
    trail.push(file);
    let Some((t, rss)) = best_of_three(&fx, &trail) else {
        eprintln!("skipping: /usr/bin/time not found");
        return;
    };
    let (parse, parse_rss) = best_of_three(&fx, &["--in-headings", "zzz", file]).unwrap();
    let (base, _) = best_of_three(&fx, &["--no-trail", "-v", "zzz", file]).unwrap();
    eprintln!(
        "{trail:?}: {t:?}, RSS {} MB; Processor alone: {parse:?}, RSS {} MB; --no-trail: {base:?}",
        rss / 1_048_576,
        parse_rss / 1_048_576,
    );
    assert!(
        rss < parse_rss + 3 * SIZE as u64,
        "RSS {rss} is not below {parse_rss} + 3 × {SIZE}"
    );
    assert!(
        t <= (parse + base) * 3,
        "{t:?} is more than 3 × ({parse:?} + {base:?})"
    );
}

#[test]
#[ignore]
fn inverted_search_on_a_big_markdown_file_stays_under_the_ceilings() {
    let _alone = alone();
    markdown_ceiling(&["--flat", "-v", "zzz"]);
}

#[test]
#[ignore]
fn tree_output_of_a_big_markdown_file_stays_under_the_ceilings() {
    let _alone = alone();
    markdown_ceiling(&["--tree", "text"]);
}

/// Code review r1, Fable #2: a walked binary file without Matches costs what it costs ripgrep,
/// which stops reading it at the NUL in its first buffer (spec §3.1, search first).
#[test]
#[ignore]
fn walked_binary_files_are_not_read_before_the_search() {
    let _alone = alone();
    let fx = Fixture::new();
    let dir = fx.file("bin");
    std::fs::create_dir(&dir).unwrap();
    let mut body = vec![b'x'; 7 * 1024 * 1024];
    body[100] = 0;
    for i in 0..40 {
        std::fs::write(dir.join(format!("lib{i}.so")), &body).unwrap();
    }
    let Some((t, _)) = best_of_three(&fx, &["--flat", "zzqq", "bin"]) else {
        eprintln!("skipping: /usr/bin/time not found");
        return;
    };
    let (base, _) = best_of_three(&fx, &["--no-trail", "zzqq", "bin"]).unwrap();
    eprintln!("40 walked binaries of 7 MiB: --flat {t:?}, --no-trail {base:?}");
    assert!(
        t.as_secs_f64() <= base.as_secs_f64() * 1.5,
        "{t:?} is more than 1.5 × the {base:?} of --no-trail"
    );
}

/// `count` copies of `unit`, written to `name` in the fixture copy.
fn repeated(fx: &Fixture, name: &str, unit: &str, count: usize) -> String {
    std::fs::write(fx.file(name), unit.repeat(count)).unwrap();
    name.to_string()
}

/// Code review r2, Fable #2 and Codex #4: an explicit Markdown file (searched in memory, as
/// ripgrep) without Matches costs its search; its Processor does not run (spec §3.1).
#[test]
#[ignore]
fn an_explicit_markdown_file_without_matches_skips_the_processor() {
    let _alone = alone();
    let fx = Fixture::new();
    let unit = "# Heading\n\nsome text\n";
    let file = repeated(&fx, "heads.md", unit, 20 * 1024 * 1024 / unit.len());
    let Some((t, _)) = best_of_three(&fx, &["--flat", "zzqq", &file]) else {
        eprintln!("skipping: /usr/bin/time not found");
        return;
    };
    let (base, _) = best_of_three(&fx, &["--no-trail", "zzqq", &file]).unwrap();
    eprintln!("20 MB explicit Markdown, no Match: --flat {t:?}, --no-trail {base:?}");
    assert!(
        t <= base * 3,
        "{t:?} is more than 3 × the {base:?} of --no-trail"
    );
}

/// Code review r2, Fable #1: headings inside list items (or blockquotes) close their Sections in
/// one pass. 2 MB of `- # Head` items cost at most 3 × the same items without headings (before
/// the fix: 40 s against 0.4 s). The review asked for 3 × `--no-trail`, which no dense Markdown
/// file reaches: pulldown-cmark alone parses these 2 MB in 130 ms, and `--no-trail` takes 30 ms.
#[test]
#[ignore]
fn headings_inside_items_stay_under_the_ceiling() {
    let _alone = alone();
    let fx = Fixture::new();
    let size = 2 * 1024 * 1024;
    let (unit, plain) = ("- # Head\n  text\n", "- Head\n  text\n");
    let file = repeated(&fx, "items.md", unit, size / unit.len());
    let reference = repeated(&fx, "plain-items.md", plain, size / plain.len());
    let Some((t, _)) = best_of_three(&fx, &["--flat", "text", &file]) else {
        eprintln!("skipping: /usr/bin/time not found");
        return;
    };
    let (base, _) = best_of_three(&fx, &["--flat", "text", &reference]).unwrap();
    let (raw, _) = best_of_three(&fx, &["--no-trail", "text", &file]).unwrap();
    eprintln!(
        "2 MB of headings in items: {t:?}; items without headings: {base:?}; --no-trail: {raw:?}"
    );
    assert!(
        t <= base * 3,
        "{t:?} is more than 3 × the {base:?} of the items without headings"
    );
}

/// Code review r2, Fable #3: `--json` writes its messages without intermediate values, and a
/// single file's output is printed as it grows instead of held until the end.
#[test]
#[ignore]
fn json_of_a_big_plain_file_stays_under_the_ceilings() {
    let _alone = alone();
    ceiling("big.txt", &["--json", "-v", "zzz"]);
}

/// Every line matching in a big Markdown file: the Processor once, plus at most 1.5 × what
/// grep-printer's own JSON (`--no-trail`) takes to write the same messages without Trails
/// (before the fix: 46.7 s against 0.9 s + 3.3 s); RSS as `markdown_ceiling`.
#[test]
#[ignore]
fn json_of_a_big_markdown_file_stays_under_the_ceilings() {
    let _alone = alone();
    let fx = Fixture::new();
    let p = big_file(fx.path(), "big.md");
    let file = p.file_name().unwrap().to_str().unwrap();
    let Some((t, rss)) = best_of_three(&fx, &["--json", "text", file]) else {
        eprintln!("skipping: /usr/bin/time not found");
        return;
    };
    let (parse, parse_rss) = best_of_three(&fx, &["--in-headings", "zzz", file]).unwrap();
    let (raw, _) = best_of_three(&fx, &["--no-trail", "--json", "text", file]).unwrap();
    eprintln!(
        "--json text: {t:?}, RSS {} MB; Processor alone: {parse:?}; --no-trail --json: {raw:?}",
        rss / 1_048_576
    );
    assert!(
        rss < parse_rss + 3 * SIZE as u64,
        "RSS {rss} is not below {parse_rss} + 3 × {SIZE}"
    );
    assert!(
        t <= parse + raw * 3 / 2,
        "{t:?} is more than {parse:?} + 1.5 × {raw:?}"
    );
}

/// Code review r4, Fable #1: grep-printer's JSON printer writes each message in many pieces,
/// and the output printed as it grows looks for the last line break only when it prints: on
/// 10 MB of Match lines, `--json --no-trail` takes at most 1.25 × rg's `--json` (before the
/// fix: 1.51 s against 1.11 s, 1.36 ×).
#[test]
#[ignore]
fn json_without_trails_of_a_big_file_is_as_fast_as_ripgrep() {
    let _alone = alone();
    let Some(rg) = rg_bin() else {
        return;
    };
    let rg = rg.as_path();
    let fx = Fixture::new();
    let line = "line of text here\n";
    let file = repeated(&fx, "ten.txt", line, 10 * 1024 * 1024 / line.len());
    for pattern in [&["-v", "zzz"][..], &["text"][..]] {
        let mut args = vec!["--json"];
        args.extend(pattern);
        args.push(&file);
        let Some((base, _)) = best_of_three_bin(rg, &fx, &args) else {
            eprintln!("skipping: /usr/bin/time not found");
            return;
        };
        args.insert(1, "--no-trail");
        let (t, _) = best_of_three(&fx, &args).unwrap();
        eprintln!("10 MB {pattern:?}: --json --no-trail {t:?}, rg --json {base:?}");
        // It runs in about rg's time; the margin is for a loaded machine, and the bug it
        // guards against was 1.36 × (code review r5, Fable #5).
        assert!(
            t.as_secs_f64() <= base.as_secs_f64() * 1.25,
            "{t:?} is more than 1.25 × the {base:?} of rg --json"
        );
    }
}

/// Code review r7, Codex #4: `--in-headings` finds where ripgrep detects the binary data of
/// stdin by searching it for every line, keeping no line: 8 MiB of Markdown on stdin with a NUL
/// at its end costs at most 2 × its size more than without the NUL (the NUL-converted copy of
/// the Source). Before the fix the search kept an empty submatch per line: 289 MiB against
/// 97 MiB.
#[test]
#[ignore]
fn the_binary_probe_of_in_headings_keeps_no_lines() {
    let _alone = alone();
    let fx = Fixture::new();
    let body = "body text\n".repeat(8 * 1024 * 1024 / 10);
    let size = body.len() as u64;
    std::fs::write(fx.file("nul.in"), format!("# needle\n\n{body}\0\n")).unwrap();
    std::fs::write(fx.file("text.in"), format!("# needle\n\n{body}\n")).unwrap();
    let args = ["--in-headings", "--format", "markdown", "-c", "needle", "-"];
    let Some((_, nul)) = measure_with(&sfor_bin(), &fx, &args, Some("nul.in")) else {
        eprintln!("skipping: /usr/bin/time not found");
        return;
    };
    let (_, text) = measure_with(&sfor_bin(), &fx, &args, Some("text.in")).unwrap();
    eprintln!(
        "8 MiB on stdin: RSS {} MiB with a NUL, {} MiB without",
        nul / 1_048_576,
        text / 1_048_576
    );
    assert!(
        nul < text + 2 * size,
        "RSS {nul} is not below {text} + 2 × {size}"
    );
}

/// Code review r8, Fable #2: with `--in-headings`, a walked Plain file has no names, and rg's
/// search of it shows only in `--files-without-match` (at its binary data) and in `-q --json`.
/// It is searched with a reader, as `--within ''` searches it, without reading it whole: 50 MiB
/// without a NUL cost 52 MiB more than `--within ''` before the fix.
#[test]
#[ignore]
fn a_walked_plain_file_is_not_read_whole_with_in_headings() {
    let _alone = alone();
    let fx = Fixture::new();
    let dir = fx.file("plain");
    std::fs::create_dir(&dir).unwrap();
    big_file(&dir, "big.log");
    let Some((_, base)) = measure(
        &sfor_bin(),
        &fx,
        &["--within", "", "--files-without-match", "zzz", "plain"],
    ) else {
        eprintln!("skipping: /usr/bin/time not found");
        return;
    };
    for mode in [&["--files-without-match"][..], &["-q", "--json"]] {
        let mut args = vec!["--in-headings"];
        args.extend_from_slice(mode);
        args.extend_from_slice(&["zzz", "plain"]);
        let (_, rss) = measure(&sfor_bin(), &fx, &args).unwrap();
        eprintln!(
            "{args:?}: RSS {} MiB; --within '': {} MiB",
            rss / 1_048_576,
            base / 1_048_576
        );
        assert!(
            rss < base + SIZE as u64 / 4,
            "{args:?}: RSS {rss} is not below {base} + {SIZE} / 4"
        );
    }
}

/// The reference rg 14.1.1 (`common::rg_path`), or None (with a note, or a failure under
/// `SFOR_REQUIRE_REFERENCE=1`) when it is missing.
fn rg_bin() -> Option<std::path::PathBuf> {
    common::reference_rg().then(common::rg_path)
}

/// The fastest of three runs of `bin args` with their output read, and the largest RSS seen.
fn best_of_three_drained(bin: &Path, fx: &Fixture, args: &[&str]) -> Option<(Duration, u64)> {
    let mut best: Option<(Duration, u64)> = None;
    for _ in 0..3 {
        let (t, rss) = measure_drained(bin, fx, args)?;
        best = Some(best.map_or((t, rss), |(bt, br)| (bt.min(t), br.max(rss))));
    }
    best
}

/// sfor's and rg's best wall time over three runs of `args` (rg's without `--flat`), with
/// their output read, and their largest RSS; None when rg 14.1.1 or /usr/bin/time is missing.
fn against_rg(fx: &Fixture, args: &[&str]) -> Option<((Duration, u64), (Duration, u64))> {
    let rg = rg_bin()?;
    let Some(sfor) = best_of_three_drained(&sfor_bin(), fx, args) else {
        eprintln!("skipping: /usr/bin/time not found");
        return None;
    };
    let rg_args: Vec<&str> = args.iter().copied().filter(|a| *a != "--flat").collect();
    let rg = best_of_three_drained(&rg, fx, &rg_args)?;
    Some((sfor, rg))
}

/// `sfor --color never args` (`SFOR_BIN` if set) in the fixture copy: stdout and stderr.
fn sfor_output(fx: &Fixture, args: &[&str]) -> (String, String) {
    let o = common::run(
        Command::new(sfor_bin())
            .arg("--color")
            .arg("never")
            .args(args)
            .current_dir(fx.path())
            .env("SFOR_FORMATS_DIR", fx.formats.path())
            .env("SFOR_GRAMMARS_DIR", fx.grammars.path())
            .env("SFOR_GRAMMAR_INDEX", fx.no_index())
            .env_remove("RIPGREP_CONFIG_PATH")
            .stdin(Stdio::null()),
    );
    assert_eq!(common::code(&o), 0, "{args:?}: {}", common::stderr(&o));
    (common::stdout(&o), common::stderr(&o))
}

/// M2 plan, Task 11.4: keypath walks a 50 MB minified JSON at 50 MB/s at least, and its
/// collector keeps the three Trails it needs, not the keys.
#[test]
#[ignore]
fn keypath_json_scans_at_fifty_megabytes_a_second() {
    let _alone = alone();
    let fx = Fixture::new();
    let mut v = String::with_capacity(SIZE + 1024);
    v.push('[');
    let mut i = 0usize;
    while v.len() < SIZE {
        if i > 0 {
            v.push(',');
        }
        // Three needles: the first object, one in the middle, the last.
        let name = if i == 0 || i == 400_000 {
            "needle"
        } else {
            "item"
        };
        v.push_str(&format!(
            r#"{{"id":{i},"name":"{name}","price":{}.5,"tags":["a","b"],"meta":{{"ok":true,"n":null}}}}"#,
            i % 1000
        ));
        i += 1;
    }
    v.push_str(r#",{"id":-1,"name":"needle"}]"#);
    assert_eq!(v.matches("needle").count(), 3, "{i} objects");
    std::fs::write(fx.file("big.json"), &v).unwrap();
    drop(v);
    let args = ["--flat", "needle", "big.json"];
    let (out, err) = sfor_output(&fx, &args);
    assert_eq!(err, "");
    // `--flat` prints the one line once per Trail.
    let trails: Vec<_> = out
        .lines()
        .map(|l| l.split_once(" │ ").map_or(l, |(t, _)| t))
        .collect();
    let last = format!("1: [{i}] › name");
    assert_eq!(
        trails,
        ["1: [0] › name", "1: [400000] › name", last.as_str()],
        "{out:.300}"
    );
    let Some(((t, rss), (base, base_rss))) = against_rg(&fx, &args) else {
        return;
    };
    eprintln!(
        "50 MB JSON: sfor {t:?}, RSS {} MiB; rg {base:?}, RSS {} MiB",
        rss / 1_048_576,
        base_rss / 1_048_576
    );
    assert!(
        t <= base + Duration::from_secs(1),
        "{t:?} is more than rg's {base:?} + 1 s"
    );
    assert!(
        rss <= base_rss + 32 * 1_048_576,
        "RSS {rss} is more than rg's {base_rss} + 32 MiB"
    );
}

/// D21: a code file over `max_bytes` (16 MiB by default) is not parsed: one warning naming
/// `max_bytes`, Plain output, and at most 2 × rg's time.
#[test]
#[ignore]
fn treesitter_skips_files_over_max_bytes() {
    let _alone = alone();
    let fx = Fixture::new();
    let mut v = String::with_capacity(21_000_000);
    let mut i = 0;
    while v.len() < 20_000_000 {
        v.push_str(&format!(
            "function f{i}(a, b) {{\n  return a + b * {i};\n}}\n"
        ));
        if i == 200_000 {
            v.push_str("// needle\n");
        }
        i += 1;
    }
    std::fs::write(fx.file("big.js"), &v).unwrap();
    // The same bytes with no Format: what Plain prints.
    std::fs::hard_link(fx.file("big.js"), fx.file("big.txt")).unwrap();
    drop(v);
    let (out, err) = sfor_output(&fx, &["--flat", "needle", "big.js"]);
    assert_eq!(err.lines().count(), 1, "{err}");
    assert!(err.contains("max_bytes"), "{err}");
    let (plain, _) = sfor_output(&fx, &["--flat", "needle", "big.txt"]);
    assert_eq!(out, plain);
    let Some(((t, _), (base, _))) = against_rg(&fx, &["--flat", "needle", "big.js"]) else {
        return;
    };
    eprintln!("20 MB JavaScript over max_bytes: sfor {t:?}, rg {base:?}");
    assert!(t <= base * 2, "{t:?} is more than 2 × rg's {base:?}");
}

/// D3 and Task 3.2: a 1 MB Cargo.lock broken 300 lines before its end by an unclosed `"""`
/// keeps the Trail of the `needle` just before the error, within rg's time + 2 s.
#[test]
#[ignore]
fn broken_toml_recovers_in_bounded_time() {
    let _alone = alone();
    let fx = Fixture::new();
    let mut lock =
        String::from("# This file is automatically @generated by Cargo.\nversion = 4\n\n");
    let mut i = 0;
    while lock.len() < 1_000_000 {
        i += 1;
        lock.push_str(&format!(
            "[[package]]\nname = \"crate-{i}\"\nversion = \"1.0.{i}\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\nchecksum = \"{:064x}\"\ndependencies = [\n \"crate-{}\",\n \"crate-{}\",\n]\n\n",
            i * 7919,
            i + 1,
            i + 2
        ));
    }
    let lines: Vec<&str> = lock.lines().collect();
    // The `[[package]]` line nearest to 300 lines before the end, so the insertion falls
    // between two tables, never inside a `dependencies` array.
    let at = (0..lines.len() - 300)
        .rev()
        .find(|&k| lines[k] == "[[package]]")
        .unwrap();
    let mut broken = String::new();
    for (k, l) in lines.iter().enumerate() {
        if k == at {
            broken.push_str("note = \"needle\"\n");
            broken.push_str("description = \"\"\"\n");
        }
        broken.push_str(l);
        broken.push('\n');
    }
    std::fs::write(fx.file("Cargo.lock"), &broken).unwrap();
    let args = ["--flat", "needle", "Cargo.lock"];
    let (out, _) = sfor_output(&fx, &args);
    assert_eq!(out.lines().count(), 1, "{out}");
    assert!(out.contains(" › note │ "), "{out}");
    let Some(((t, _), (base, _))) = against_rg(&fx, &args) else {
        return;
    };
    eprintln!("1 MB broken Cargo.lock: sfor {t:?}, rg {base:?}; {out}");
    assert!(
        t <= base + Duration::from_secs(2),
        "{t:?} is more than rg's {base:?} + 2 s"
    );
}

/// D2: tree-sitter-yaml breaks over 32 768 lines; keypath gives the Trail of a `needle` at the
/// end of 40 000 lines, within rg's time + 1 s.
#[test]
#[ignore]
fn yaml_beyond_the_tree_sitter_limit() {
    let _alone = alone();
    let fx = Fixture::new();
    let mut v = String::from("# 40 000 lines\nitems:\n");
    for i in 0..19_998 {
        v.push_str(&format!("  - name: item {i}\n    size: {i}\n"));
    }
    v.push_str("last:\n  key: needle\n");
    assert_eq!(v.lines().count(), 40_000);
    std::fs::write(fx.file("big.yaml"), &v).unwrap();
    let args = ["--flat", "needle", "big.yaml"];
    let (out, err) = sfor_output(&fx, &args);
    assert_eq!(err, "");
    assert_eq!(out, "40000: last › key │   key: needle\n");
    let Some(((t, _), (base, _))) = against_rg(&fx, &args) else {
        return;
    };
    eprintln!("40 000-line YAML: sfor {t:?}, rg {base:?}");
    assert!(
        t <= base + Duration::from_secs(1),
        "{t:?} is more than rg's {base:?} + 1 s"
    );
}

/// Review m2-code-r1, Codex #3: 40 000 `[[a]]` tables, each with a `[[a.b]]`, and a `needle`
/// in the last one keep the TOML Outline linear: within rg's time + 1 s (it took 7 s when every
/// header searched every other one for the table that covers it).
#[test]
#[ignore]
fn many_arrays_of_tables_stay_linear() {
    let _alone = alone();
    let fx = Fixture::new();
    let mut v = String::new();
    for _ in 0..40_000 {
        v.push_str("[[a]]\nx = \"none\"\n[[a.b]]\ny = 1\n");
    }
    v.push_str("[[a]]\nx = \"needle\"\n");
    std::fs::write(fx.file("many.toml"), &v).unwrap();
    let args = ["--flat", "needle", "many.toml"];
    let (out, err) = sfor_output(&fx, &args);
    assert_eq!(err, "");
    assert_eq!(out, "160002: a[40000] › x │ x = \"needle\"\n");
    let Some(((t, _), (base, _))) = against_rg(&fx, &args) else {
        return;
    };
    eprintln!("40 000 [[a]] tables: sfor {t:?}, rg {base:?}");
    assert!(
        t <= base + Duration::from_secs(1),
        "{t:?} is more than rg's {base:?} + 1 s"
    );
}

/// M3 review r1, Codex #1 and Fable #1: a parent with 200 000 distinct child tags costs
/// constant time per child (82 s before the sibling index, against rg's 0.01 s).
#[test]
#[ignore]
fn markup_many_distinct_sibling_tags_stay_linear() {
    let _alone = alone();
    let fx = Fixture::new();
    let mut v = String::from("<r>");
    for i in 0..200_000 {
        v.push_str(&format!("<row{i}/>"));
    }
    v.push_str("<x>needle</x></r>\n");
    std::fs::write(fx.file("siblings.xml"), &v).unwrap();
    let args = ["--flat", "needle", "siblings.xml"];
    let (out, _) = sfor_output(&fx, &args);
    assert!(out.starts_with("1: r › x │ "), "{out:.100}");
    let Some(((t, _), (base, _))) = against_rg(&fx, &args) else {
        return;
    };
    assert!(
        t <= base + Duration::from_millis(500),
        "{t:?} against rg's {base:?}"
    );
}

/// M3 review r1, Codex #2: one Match in 50 MB of line breaks keeps no index of the lines
/// (467 MiB of RSS before the fix, against rg's 56 MiB).
#[test]
#[ignore]
fn markup_keeps_no_index_of_the_lines() {
    let _alone = alone();
    let fx = Fixture::new();
    let mut v = String::with_capacity(SIZE + 64);
    v.push_str("<r>\n");
    v.push_str(&"\n".repeat(SIZE));
    v.push_str("<x>needle</x></r>\n");
    std::fs::write(fx.file("blank.xml"), &v).unwrap();
    drop(v);
    let args = ["--flat", "needle", "blank.xml"];
    let (out, _) = sfor_output(&fx, &args);
    assert!(out.ends_with(": r › x │ <x>needle</x></r>\n"), "{out:.100}");
    let Some(((t, rss), (base, base_rss))) = against_rg(&fx, &args) else {
        return;
    };
    assert!(
        t <= base + Duration::from_secs(1),
        "{t:?} against rg's {base:?}"
    );
    assert!(
        rss <= base_rss + 32 * 1_048_576,
        "RSS {rss} is more than rg's {base_rss} + 32 MiB"
    );
}

/// One of M3 review r2's linear walks (Codex #1): `body` in `name`, one Match, sfor within rg's
/// time + 1 s, and the Match's Trail with `segments` Segments (256 at most, D25).
fn markup_linear(name: &str, body: String, segments: usize) {
    let fx = Fixture::new();
    std::fs::write(fx.file(name), &body).unwrap();
    let (out, _) = sfor_output(&fx, &["--json", "needle", name]);
    let m = out
        .lines()
        .find(|l| l.contains(r#""type":"match""#))
        .unwrap();
    assert_eq!(m.matches(r#""kind":"tag""#).count(), segments, "{name}");
    let Some(((t, _), (base, _))) = against_rg(&fx, &["--flat", "needle", name]) else {
        return;
    };
    eprintln!("{name}: sfor {t:?}, rg {base:?}");
    assert!(
        t <= base + Duration::from_secs(1),
        "{name}: {t:?} against rg's {base:?}"
    );
}

/// 40 000 nested `div`s: a start tag finds the `p` it ends without searching the stack (3.96 s
/// before `html::Reach`).
#[test]
#[ignore]
fn markup_deep_nesting_stays_linear() {
    let _alone = alone();
    let body = format!(
        "{}needle{}",
        "<div>".repeat(40_000),
        "</div>".repeat(40_000)
    );
    markup_linear("deep.html", body, 256);
}

/// 20 000 end tags with no element of their tag open: each is ignored at once (2.88 s before
/// the open counts).
#[test]
#[ignore]
fn markup_end_tags_with_nothing_open_stay_linear() {
    let _alone = alone();
    let body = format!(
        "{}{}needle{}",
        "<x>".repeat(20_000),
        "</q>".repeat(20_000),
        "</x>".repeat(20_000)
    );
    markup_linear("unmatched.html", body, 256);
}

/// 20 000 elements ended together at the end of the file, after 200 000 spaces: the white
/// space is trimmed once (5.65 s before).
#[test]
#[ignore]
fn markup_implied_ends_at_one_byte_stay_linear() {
    let _alone = alone();
    let body = format!("{}needle{}", "<x>".repeat(20_000), " ".repeat(200_000));
    markup_linear("trim.html", body, 256);
}

/// One M3 Format at 50 MB (M3 plan, Task 7.1): `unit(i, needle)` writes the i-th repetition,
/// with `needle` at i = 0, in the middle and at the end. sfor must print three rows whose
/// Trails start with `first` and `last`, within rg's time + 1 s (50 MB/s at least) and rg's
/// RSS + 32 MiB: the walk keeps three short stacks, not the file's structure.
fn m3_ceiling(
    file: &str,
    head: &str,
    unit: impl Fn(usize, &str) -> String,
    tail: &str,
    first: &str,
    last: &str,
) {
    let fx = Fixture::new();
    let mut v = String::with_capacity(SIZE + 4096);
    v.push_str(head);
    let mut i = 0;
    let mut middle = None;
    while v.len() < SIZE {
        let needle = if i == 0 || (middle.is_none() && v.len() >= SIZE / 2) {
            if i > 0 {
                middle = Some(i);
            }
            "needle"
        } else {
            "hay"
        };
        v.push_str(&unit(i, needle));
        i += 1;
    }
    v.push_str(&unit(i, "needle"));
    v.push_str(tail);
    assert_eq!(v.matches("needle").count(), 3, "{file}");
    std::fs::write(fx.file(file), &v).unwrap();
    drop(v);
    let args = ["--flat", "needle", file];
    let (out, err) = sfor_output(&fx, &args);
    assert_eq!(err, "", "{file}");
    let trails: Vec<&str> = out
        .lines()
        .map(|l| l.split_once(" │ ").map_or("", |(t, _)| t))
        .collect();
    assert_eq!(trails.len(), 3, "{file}: {out:.400}");
    // `{n}` is the 1-based position of the last unit: its book, row or cue number.
    let last = last.replace("{n}", &(i + 1).to_string());
    assert!(
        trails[0].ends_with(first),
        "{file}: {:?} ≠ …{first}",
        trails[0]
    );
    assert!(
        trails[2].ends_with(&last),
        "{file}: {:?} ≠ …{last}",
        trails[2]
    );
    let Some(((t, rss), (base, base_rss))) = against_rg(&fx, &args) else {
        return;
    };
    eprintln!(
        "50 MB {file}: sfor {t:?}, RSS {} MiB; rg {base:?}, RSS {} MiB",
        rss / 1_048_576,
        base_rss / 1_048_576
    );
    assert!(
        t <= base + Duration::from_secs(1),
        "{file}: {t:?} is more than rg's {base:?} + 1 s"
    );
    assert!(
        rss <= base_rss + 32 * 1_048_576,
        "{file}: RSS {rss} is more than rg's {base_rss} + 32 MiB"
    );
}

#[test]
#[ignore]
fn markup_html_walks_at_fifty_megabytes_a_second() {
    let _alone = alone();
    m3_ceiling(
        "big.html",
        "<!DOCTYPE html>\n<html><body>\n<main id=\"all\">\n",
        |i, n| {
            format!(
                "<section class=\"s\">\n  <h2>Part {i}</h2>\n  <ul><li>one<li>two <b>{n}</b><li>three</ul>\n  <p>text<br>\n</section>\n"
            )
        },
        "</main>\n</body></html>\n",
        ": main#all › section.s[1] › ul › li[2] › b",
        " › ul › li[2] › b",
    );
}

#[test]
#[ignore]
fn markup_xml_walks_at_fifty_megabytes_a_second() {
    let _alone = alone();
    m3_ceiling(
        "big.xml",
        "<?xml version=\"1.0\"?>\n<catalog>\n",
        |i, n| {
            format!(
                "  <book id=\"b{i}\">\n    <title>{n}</title>\n    <!-- c -->\n    <price>{i}</price>\n  </book>\n"
            )
        },
        "</catalog>\n",
        ": catalog › book[1] › title",
        ": catalog › book[{n}] › title",
    );
}

#[test]
#[ignore]
fn csv_reads_at_fifty_megabytes_a_second() {
    let _alone = alone();
    m3_ceiling(
        "big.csv",
        "id,Name,Plan,Notes\n",
        |i, n| {
            // Only units without a needle are quoted, so the three needles are all in `Plan`
            // (Fable r1 #9).
            if i % 7 == 3 && n == "hay" {
                format!("{i},Ana,pro,\"two, \"\"quoted\"\"\nlines {n}\"\n")
            } else {
                format!("{i},Bea,{n},plain notes here\n")
            }
        },
        "",
        ": row 1 › Plan",
        // Unit i is row i + 1.
        ": row {n} › Plan",
    );
}

#[test]
#[ignore]
fn subtitles_read_at_fifty_megabytes_a_second() {
    let _alone = alone();
    m3_ceiling(
        "big.srt",
        "",
        |i, n| {
            format!(
                "{}\r\n00:00:01,000 --> 00:00:02,000\r\nline of {n}\r\nsecond line\r\n\r\n",
                i + 1
            )
        },
        "",
        ": cue 1 00:00:01,000",
        ": cue {n} 00:00:01,000",
    );
}

#[test]
#[ignore]
fn tsv_reads_at_fifty_megabytes_a_second() {
    let _alone = alone();
    m3_ceiling(
        "big.tsv",
        "id\tName\tPlan\tNotes\n",
        |i, n| format!("{i}\tBea\t{n}\tplain \"notes\" here\n"),
        "",
        ": row 1 › Plan",
        ": row {n} › Plan",
    );
}

#[test]
#[ignore]
fn vtt_reads_at_fifty_megabytes_a_second() {
    let _alone = alone();
    m3_ceiling(
        "big.vtt",
        "WEBVTT\n\nNOTE a note\n\n",
        |_, n| format!("00:00:01.000 --> 00:00:02.000 align:start\nline of {n}\nsecond line\n\n"),
        "",
        ": cue 1 00:00:01.000",
        ": cue {n} 00:00:01.000",
    );
}
