//! Memory and time ceilings of the Trail pipeline (code review r1, Fable #1). Ignored by
//! default because they write a 50 MB file and run the binary several times; run them with
//! `cargo test --release --test perf -- --ignored --nocapture`. `SEARCHFOR_BIN` runs them against
//! another build of s4 (to see them fail before a fix). They measure wall time, so they run
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
    std::env::var_os("SEARCHFOR_BIN")
        .map(Into::into)
        .unwrap_or_else(|| assert_cmd::cargo::cargo_bin("s4"))
}

/// Wall time and peak RSS in bytes of `bin args` (s4 or rg), stdout discarded, measured by
/// `/usr/bin/time -v`; the run must exit with `code` (M3 code review r2, Codex #6: a binary
/// that fails before searching would otherwise pass every ceiling). None when /usr/bin/time is
/// missing.
fn measure(bin: &Path, fx: &Fixture, args: &[&str], code: i32) -> Option<(Duration, u64)> {
    measure_with(bin, fx, args, None, code)
}

/// `measure` with stdin read from `stdin`, a file in the fixture copy.
fn measure_with(
    bin: &Path,
    fx: &Fixture,
    args: &[&str],
    stdin: Option<&str>,
    code: i32,
) -> Option<(Duration, u64)> {
    measure_to(bin, fx, args, stdin, Out::Null, code).map(|(t, rss, _)| (t, rss))
}

/// `measure` with stdout read and thrown away: ripgrep stops at the first Match when its
/// stdout is /dev/null, as `-q` does, so a comparison with rg must not give it /dev/null. A
/// thread reads it to the end and keeps none of it: the output of a dense file is gigabytes
/// (M5b plan, Task 10.3).
fn measure_drained(bin: &Path, fx: &Fixture, args: &[&str], code: i32) -> Option<(Duration, u64)> {
    measure_to(bin, fx, args, None, Out::Drain, code).map(|(t, rss, _)| (t, rss))
}

/// What `measure_to` does with the measured program's stdout.
enum Out {
    /// /dev/null.
    Null,
    /// Read and returned.
    Keep,
    /// Read to the end and thrown away.
    Drain,
}

/// Whether the perf tests assert ratios of time and wall-time ceilings: only with
/// `SEARCHFOR_PERF_TIMING=1` (M5b plan, D18). Memory, refusal, exit code and output assertions
/// run always; time on a shared CI runner is noise.
fn timing_enabled() -> bool {
    std::env::var_os("SEARCHFOR_PERF_TIMING").is_some_and(|v| v == "1")
}

/// `measure` with stdout read and returned, for the Trails of a positive search.
fn measure_out(
    bin: &Path,
    fx: &Fixture,
    args: &[&str],
    code: i32,
) -> Option<(Duration, u64, String)> {
    measure_to(bin, fx, args, None, Out::Keep, code)
}

fn measure_to(
    bin: &Path,
    fx: &Fixture,
    args: &[&str],
    stdin: Option<&str>,
    out: Out,
    code: i32,
) -> Option<(Duration, u64, String)> {
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
        .env("SEARCHFOR_FORMATS_DIR", fx.formats.path())
        .env("SEARCHFOR_GRAMMARS_DIR", fx.grammars.path())
        .env("SEARCHFOR_GRAMMAR_INDEX", fx.no_index())
        .env_remove("SEARCHFOR_CONFIG_PATH")
        .env_remove("RIPGREP_CONFIG_PATH")
        .stdin(match stdin {
            None => Stdio::null(),
            Some(f) => Stdio::from(std::fs::File::open(fx.file(f)).unwrap()),
        })
        .stdout(match out {
            Out::Null => Stdio::null(),
            Out::Keep | Out::Drain => Stdio::piped(),
        })
        .stderr(Stdio::piped());
    let started = Instant::now();
    let mut child = c.spawn().unwrap();
    let drain = match out {
        Out::Drain => child.stdout.take().map(|mut o| {
            std::thread::spawn(move || std::io::copy(&mut o, &mut std::io::sink()).unwrap())
        }),
        _ => None,
    };
    let o = child.wait_with_output().unwrap();
    let elapsed = started.elapsed();
    if let Some(d) = drain {
        d.join().unwrap();
    }
    let err = String::from_utf8_lossy(&o.stderr);
    // /usr/bin/time exits with the command's code.
    assert_eq!(
        o.status.code(),
        Some(code),
        "{} {args:?} exited with {:?}, not {code}: {err}",
        bin.display(),
        o.status
    );
    let kb: u64 = err
        .lines()
        .find_map(|l| {
            l.trim()
                .strip_prefix("Maximum resident set size (kbytes): ")
        })
        .unwrap_or_else(|| panic!("no RSS in: {err}"))
        .parse()
        .unwrap();
    Some((
        elapsed,
        kb * 1024,
        String::from_utf8_lossy(&o.stdout).into_owned(),
    ))
}

/// The fastest of three runs, each exiting with `code`, and the largest RSS seen.
fn best_of_three(fx: &Fixture, args: &[&str], code: i32) -> Option<(Duration, u64)> {
    best_of_three_bin(&sfor_bin(), fx, args, code)
}

fn best_of_three_bin(
    bin: &Path,
    fx: &Fixture,
    args: &[&str],
    code: i32,
) -> Option<(Duration, u64)> {
    let mut best: Option<(Duration, u64)> = None;
    for _ in 0..3 {
        let (t, rss) = measure(bin, fx, args, code)?;
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
    let Some((t, rss)) = best_of_three(&fx, &trail, 0) else {
        eprintln!("skipping: /usr/bin/time not found");
        return;
    };
    let mut plain = vec!["--no-docs", "--no-trail"];
    plain.extend(args.iter().filter(|a| **a != "--flat"));
    plain.push(file);
    let (base, base_rss) = best_of_three(&fx, &plain, 0).unwrap();
    eprintln!(
        "{trail:?}: {t:?}, RSS {} MB; --no-trail: {base:?}, RSS {} MB; file {} MB",
        rss / 1_048_576,
        base_rss / 1_048_576,
        SIZE / 1_048_576
    );
    assert!(rss < 4 * SIZE as u64, "RSS {rss} is not below 4 × {SIZE}");
    if timing_enabled() {
        assert!(
            t <= base * 3,
            "{t:?} is more than 3 × the {base:?} of --no-trail"
        );
    }
}

#[test]
#[cfg_attr(target_os = "linux", ignore)]
#[cfg_attr(
    not(target_os = "linux"),
    ignore = "requires GNU time -v RSS measurement"
)]
fn inverted_search_on_a_big_plain_file_stays_under_the_ceilings() {
    let _alone = alone();
    ceiling("big.txt", &["--flat", "-v", "zzz"]);
}

#[test]
#[cfg_attr(target_os = "linux", ignore)]
#[cfg_attr(
    not(target_os = "linux"),
    ignore = "requires GNU time -v RSS measurement"
)]
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
    let Some((t, rss)) = best_of_three(&fx, &trail, 0) else {
        eprintln!("skipping: /usr/bin/time not found");
        return;
    };
    let (parse, parse_rss) = best_of_three(&fx, &["--in-headings", "zzz", file], 1).unwrap();
    let (base, _) = best_of_three(&fx, &["--no-docs", "--no-trail", "-v", "zzz", file], 0).unwrap();
    eprintln!(
        "{trail:?}: {t:?}, RSS {} MB; Processor alone: {parse:?}, RSS {} MB; --no-trail: {base:?}",
        rss / 1_048_576,
        parse_rss / 1_048_576,
    );
    assert!(
        rss < parse_rss + 3 * SIZE as u64,
        "RSS {rss} is not below {parse_rss} + 3 × {SIZE}"
    );
    if timing_enabled() {
        assert!(
            t <= (parse + base) * 3,
            "{t:?} is more than 3 × ({parse:?} + {base:?})"
        );
    }
}

#[test]
#[cfg_attr(target_os = "linux", ignore)]
#[cfg_attr(
    not(target_os = "linux"),
    ignore = "requires GNU time -v RSS measurement"
)]
fn inverted_search_on_a_big_markdown_file_stays_under_the_ceilings() {
    let _alone = alone();
    markdown_ceiling(&["--flat", "-v", "zzz"]);
}

#[test]
#[cfg_attr(target_os = "linux", ignore)]
#[cfg_attr(
    not(target_os = "linux"),
    ignore = "requires GNU time -v RSS measurement"
)]
fn tree_output_of_a_big_markdown_file_stays_under_the_ceilings() {
    let _alone = alone();
    markdown_ceiling(&["--tree", "text"]);
}

/// Code review r1, Fable #2: a walked binary file without Matches costs what it costs ripgrep,
/// which stops reading it at the NUL in its first buffer (spec §3.1, search first).
#[test]
#[cfg_attr(target_os = "linux", ignore)]
#[cfg_attr(
    not(target_os = "linux"),
    ignore = "requires GNU time -v RSS measurement"
)]
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
    let Some((t, _)) = best_of_three(&fx, &["--flat", "zzqq", "bin"], 1) else {
        eprintln!("skipping: /usr/bin/time not found");
        return;
    };
    let (base, _) = best_of_three(&fx, &["--no-docs", "--no-trail", "zzqq", "bin"], 1).unwrap();
    eprintln!("40 walked binaries of 7 MiB: --flat {t:?}, --no-trail {base:?}");
    if timing_enabled() {
        assert!(
            t.as_secs_f64() <= base.as_secs_f64() * 1.5,
            "{t:?} is more than 1.5 × the {base:?} of --no-trail"
        );
    }
}

/// `count` copies of `unit`, written to `name` in the fixture copy.
fn repeated(fx: &Fixture, name: &str, unit: &str, count: usize) -> String {
    std::fs::write(fx.file(name), unit.repeat(count)).unwrap();
    name.to_string()
}

/// Code review r2, Fable #2 and Codex #4: an explicit Markdown file (searched in memory, as
/// ripgrep) without Matches costs its search; its Processor does not run (spec §3.1).
#[test]
#[cfg_attr(target_os = "linux", ignore)]
#[cfg_attr(
    not(target_os = "linux"),
    ignore = "requires GNU time -v RSS measurement"
)]
fn an_explicit_markdown_file_without_matches_skips_the_processor() {
    let _alone = alone();
    let fx = Fixture::new();
    let unit = "# Heading\n\nsome text\n";
    let file = repeated(&fx, "heads.md", unit, 20 * 1024 * 1024 / unit.len());
    let Some((t, _)) = best_of_three(&fx, &["--flat", "zzqq", &file], 1) else {
        eprintln!("skipping: /usr/bin/time not found");
        return;
    };
    let (base, _) = best_of_three(&fx, &["--no-docs", "--no-trail", "zzqq", &file], 1).unwrap();
    eprintln!("20 MB explicit Markdown, no Match: --flat {t:?}, --no-trail {base:?}");
    if timing_enabled() {
        assert!(
            t <= base * 3,
            "{t:?} is more than 3 × the {base:?} of --no-trail"
        );
    }
}

/// Code review r2, Fable #1: headings inside list items (or blockquotes) close their Sections in
/// one pass. 2 MB of `- # Head` items cost at most 3 × the same items without headings (before
/// the fix: 40 s against 0.4 s). The review asked for 3 × `--no-trail`, which no dense Markdown
/// file reaches: pulldown-cmark alone parses these 2 MB in 130 ms, and `--no-trail` takes 30 ms.
#[test]
#[cfg_attr(target_os = "linux", ignore)]
#[cfg_attr(
    not(target_os = "linux"),
    ignore = "requires GNU time -v RSS measurement"
)]
fn headings_inside_items_stay_under_the_ceiling() {
    let _alone = alone();
    let fx = Fixture::new();
    let size = 2 * 1024 * 1024;
    let (unit, plain) = ("- # Head\n  text\n", "- Head\n  text\n");
    let file = repeated(&fx, "items.md", unit, size / unit.len());
    let reference = repeated(&fx, "plain-items.md", plain, size / plain.len());
    let Some((t, _)) = best_of_three(&fx, &["--flat", "text", &file], 0) else {
        eprintln!("skipping: /usr/bin/time not found");
        return;
    };
    let (base, _) = best_of_three(&fx, &["--flat", "text", &reference], 0).unwrap();
    let (raw, _) = best_of_three(&fx, &["--no-docs", "--no-trail", "text", &file], 0).unwrap();
    eprintln!(
        "2 MB of headings in items: {t:?}; items without headings: {base:?}; --no-trail: {raw:?}"
    );
    if timing_enabled() {
        assert!(
            t <= base * 3,
            "{t:?} is more than 3 × the {base:?} of the items without headings"
        );
    }
}

/// Code review r2, Fable #3: `--json` writes its messages without intermediate values, and a
/// single file's output is printed as it grows instead of held until the end.
#[test]
#[cfg_attr(target_os = "linux", ignore)]
#[cfg_attr(
    not(target_os = "linux"),
    ignore = "requires GNU time -v RSS measurement"
)]
fn json_of_a_big_plain_file_stays_under_the_ceilings() {
    let _alone = alone();
    ceiling("big.txt", &["--json", "-v", "zzz"]);
}

/// Every line matching in a big Markdown file: the Processor once, plus at most 1.5 × what
/// grep-printer's own JSON (`--no-trail`) takes to write the same messages without Trails
/// (before the fix: 46.7 s against 0.9 s + 3.3 s); RSS as `markdown_ceiling`.
#[test]
#[cfg_attr(target_os = "linux", ignore)]
#[cfg_attr(
    not(target_os = "linux"),
    ignore = "requires GNU time -v RSS measurement"
)]
fn json_of_a_big_markdown_file_stays_under_the_ceilings() {
    let _alone = alone();
    let fx = Fixture::new();
    let p = big_file(fx.path(), "big.md");
    let file = p.file_name().unwrap().to_str().unwrap();
    let Some((t, rss)) = best_of_three(&fx, &["--json", "text", file], 0) else {
        eprintln!("skipping: /usr/bin/time not found");
        return;
    };
    let (parse, parse_rss) = best_of_three(&fx, &["--in-headings", "zzz", file], 1).unwrap();
    let (raw, _) =
        best_of_three(&fx, &["--no-docs", "--no-trail", "--json", "text", file], 0).unwrap();
    eprintln!(
        "--json text: {t:?}, RSS {} MB; Processor alone: {parse:?}; --no-trail --json: {raw:?}",
        rss / 1_048_576
    );
    assert!(
        rss < parse_rss + 3 * SIZE as u64,
        "RSS {rss} is not below {parse_rss} + 3 × {SIZE}"
    );
    if timing_enabled() {
        assert!(
            t <= parse + raw * 3 / 2,
            "{t:?} is more than {parse:?} + 1.5 × {raw:?}"
        );
    }
}

/// Code review r4, Fable #1: grep-printer's JSON printer writes each message in many pieces,
/// and the output printed as it grows looks for the last line break only when it prints: on
/// 10 MB of Match lines, `--json --no-trail` takes at most 1.25 × rg's `--json` (before the
/// fix: 1.51 s against 1.11 s, 1.36 ×).
#[test]
#[cfg_attr(target_os = "linux", ignore)]
#[cfg_attr(
    not(target_os = "linux"),
    ignore = "requires GNU time -v RSS measurement"
)]
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
        let Some((base, _)) = best_of_three_bin(rg, &fx, &args, 0) else {
            eprintln!("skipping: /usr/bin/time not found");
            return;
        };
        args.splice(1..1, ["--no-docs", "--no-trail"]);
        let (t, _) = best_of_three(&fx, &args, 0).unwrap();
        eprintln!("10 MB {pattern:?}: --json --no-trail {t:?}, rg --json {base:?}");
        // It runs in about rg's time; the margin is for a loaded machine, and the bug it
        // guards against was 1.36 × (code review r5, Fable #5).
        if timing_enabled() {
            assert!(
                t.as_secs_f64() <= base.as_secs_f64() * 1.25,
                "{t:?} is more than 1.25 × the {base:?} of rg --json"
            );
        }
    }
}

/// Code review r7, Codex #4: `--in-headings` finds where ripgrep detects the binary data of
/// stdin by searching it for every line, keeping no line: 8 MiB of Markdown on stdin with a NUL
/// at its end costs at most 2 × its size more than without the NUL (the NUL-converted copy of
/// the Source). Before the fix the search kept an empty submatch per line: 289 MiB against
/// 97 MiB.
#[test]
#[cfg_attr(target_os = "linux", ignore)]
#[cfg_attr(
    not(target_os = "linux"),
    ignore = "requires GNU time -v RSS measurement"
)]
fn the_binary_probe_of_in_headings_keeps_no_lines() {
    let _alone = alone();
    let fx = Fixture::new();
    let body = "body text\n".repeat(8 * 1024 * 1024 / 10);
    let size = body.len() as u64;
    std::fs::write(fx.file("binary.in"), format!("# needle\n\n{body}\0\n")).unwrap();
    std::fs::write(fx.file("text.in"), format!("# needle\n\n{body}\n")).unwrap();
    let args = ["--in-headings", "--format", "markdown", "-c", "needle", "-"];
    let Some((_, nul)) = measure_with(&sfor_bin(), &fx, &args, Some("binary.in"), 0) else {
        eprintln!("skipping: /usr/bin/time not found");
        return;
    };
    let (_, text) = measure_with(&sfor_bin(), &fx, &args, Some("text.in"), 0).unwrap();
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
#[cfg_attr(target_os = "linux", ignore)]
#[cfg_attr(
    not(target_os = "linux"),
    ignore = "requires GNU time -v RSS measurement"
)]
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
        0,
    ) else {
        eprintln!("skipping: /usr/bin/time not found");
        return;
    };
    // `--files-without-match` prints the file and exits 0, as rg 14.1.1; `-q` finds nothing.
    for (mode, code) in [(&["--files-without-match"][..], 0), (&["-q", "--json"], 1)] {
        let mut args = vec!["--in-headings"];
        args.extend_from_slice(mode);
        args.extend_from_slice(&["zzz", "plain"]);
        let (_, rss) = measure(&sfor_bin(), &fx, &args, code).unwrap();
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
/// `SEARCHFOR_REQUIRE_REFERENCE=1`) when it is missing.
fn rg_bin() -> Option<std::path::PathBuf> {
    common::reference_rg().then(common::rg_path)
}

/// The fastest of three runs of `bin args` with their output read, each exiting with `code`,
/// and the largest RSS seen.
fn best_of_three_drained(
    bin: &Path,
    fx: &Fixture,
    args: &[&str],
    code: i32,
) -> Option<(Duration, u64)> {
    let mut best: Option<(Duration, u64)> = None;
    for _ in 0..3 {
        let (t, rss) = measure_drained(bin, fx, args, code)?;
        best = Some(best.map_or((t, rss), |(bt, br)| (bt.min(t), br.max(rss))));
    }
    best
}

/// s4's and rg's best wall time over three runs of `args` (rg's without `--flat`), with
/// their output read, and their largest RSS; None when rg 14.1.1 or /usr/bin/time is missing.
/// Every caller searches for a pattern that is there: both must exit 0.
fn against_rg(fx: &Fixture, args: &[&str]) -> Option<((Duration, u64), (Duration, u64))> {
    let rg = rg_bin()?;
    let Some(s4) = best_of_three_drained(&sfor_bin(), fx, args, 0) else {
        eprintln!("skipping: /usr/bin/time not found");
        return None;
    };
    let rg_args: Vec<&str> = args.iter().copied().filter(|a| *a != "--flat").collect();
    let rg = best_of_three_drained(&rg, fx, &rg_args, 0)?;
    Some((s4, rg))
}

/// `s4 --color never args` (`SEARCHFOR_BIN` if set) in the fixture copy: stdout and stderr.
fn sfor_output(fx: &Fixture, args: &[&str]) -> (String, String) {
    let o = common::run(
        Command::new(sfor_bin())
            .arg("--color")
            .arg("never")
            .args(args)
            .current_dir(fx.path())
            .env("SEARCHFOR_FORMATS_DIR", fx.formats.path())
            .env("SEARCHFOR_GRAMMARS_DIR", fx.grammars.path())
            .env("SEARCHFOR_GRAMMAR_INDEX", fx.no_index())
            .env_remove("SEARCHFOR_CONFIG_PATH")
            .env_remove("RIPGREP_CONFIG_PATH")
            .stdin(Stdio::null()),
    );
    assert_eq!(common::code(&o), 0, "{args:?}: {}", common::stderr(&o));
    (common::stdout(&o), common::stderr(&o))
}

/// M2 plan, Task 11.4: keypath walks a 50 MB minified JSON at 50 MB/s at least, and its
/// collector keeps the three Trails it needs, not the keys.
#[test]
#[cfg_attr(target_os = "linux", ignore)]
#[cfg_attr(
    not(target_os = "linux"),
    ignore = "requires GNU time -v RSS measurement"
)]
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
        "50 MB JSON: s4 {t:?}, RSS {} MiB; rg {base:?}, RSS {} MiB",
        rss / 1_048_576,
        base_rss / 1_048_576
    );
    if timing_enabled() {
        assert!(
            t <= base + Duration::from_secs(1),
            "{t:?} is more than rg's {base:?} + 1 s"
        );
    }
    assert!(
        rss <= base_rss + 32 * 1_048_576,
        "RSS {rss} is more than rg's {base_rss} + 32 MiB"
    );
}

/// D21: a code file over `max_bytes` (16 MiB by default) is not parsed: one warning naming
/// `max_bytes`, Plain output, and at most 2 × rg's time.
#[test]
#[cfg_attr(target_os = "linux", ignore)]
#[cfg_attr(
    not(target_os = "linux"),
    ignore = "requires GNU time -v RSS measurement"
)]
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
    eprintln!("20 MB JavaScript over max_bytes: s4 {t:?}, rg {base:?}");
    if timing_enabled() {
        assert!(t <= base * 2, "{t:?} is more than 2 × rg's {base:?}");
    }
}

/// D3 and Task 3.2: a 1 MB Cargo.lock broken 300 lines before its end by an unclosed `"""`
/// keeps the Trail of the `needle` just before the error, within rg's time + 2 s.
#[test]
#[cfg_attr(target_os = "linux", ignore)]
#[cfg_attr(
    not(target_os = "linux"),
    ignore = "requires GNU time -v RSS measurement"
)]
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
    eprintln!("1 MB broken Cargo.lock: s4 {t:?}, rg {base:?}; {out}");
    if timing_enabled() {
        assert!(
            t <= base + Duration::from_secs(2),
            "{t:?} is more than rg's {base:?} + 2 s"
        );
    }
}

/// D2: tree-sitter-yaml breaks over 32 768 lines; keypath gives the Trail of a `needle` at the
/// end of 40 000 lines, within rg's time + 1 s.
#[test]
#[cfg_attr(target_os = "linux", ignore)]
#[cfg_attr(
    not(target_os = "linux"),
    ignore = "requires GNU time -v RSS measurement"
)]
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
    eprintln!("40 000-line YAML: s4 {t:?}, rg {base:?}");
    if timing_enabled() {
        assert!(
            t <= base + Duration::from_secs(1),
            "{t:?} is more than rg's {base:?} + 1 s"
        );
    }
}

/// Review m2-code-r1, Codex #3: 40 000 `[[a]]` tables, each with a `[[a.b]]`, and a `needle`
/// in the last one keep the TOML Outline linear: within rg's time + 1 s (it took 7 s when every
/// header searched every other one for the table that covers it).
#[test]
#[cfg_attr(target_os = "linux", ignore)]
#[cfg_attr(
    not(target_os = "linux"),
    ignore = "requires GNU time -v RSS measurement"
)]
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
    eprintln!("40 000 [[a]] tables: s4 {t:?}, rg {base:?}");
    if timing_enabled() {
        assert!(
            t <= base + Duration::from_secs(1),
            "{t:?} is more than rg's {base:?} + 1 s"
        );
    }
}

/// M3 review r1, Codex #1 and Fable #1: a parent with 200 000 distinct child tags costs
/// constant time per child (82 s before the sibling index, against rg's 0.01 s).
#[test]
#[cfg_attr(target_os = "linux", ignore)]
#[cfg_attr(
    not(target_os = "linux"),
    ignore = "requires GNU time -v RSS measurement"
)]
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
    if timing_enabled() {
        assert!(
            t <= base + Duration::from_millis(500),
            "{t:?} against rg's {base:?}"
        );
    }
}

/// M3 review r1, Codex #2: one Match in 50 MB of line breaks keeps no index of the lines
/// (467 MiB of RSS before the fix, against rg's 56 MiB).
#[test]
#[cfg_attr(target_os = "linux", ignore)]
#[cfg_attr(
    not(target_os = "linux"),
    ignore = "requires GNU time -v RSS measurement"
)]
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
    if timing_enabled() {
        assert!(
            t <= base + Duration::from_secs(1),
            "{t:?} against rg's {base:?}"
        );
    }
    assert!(
        rss <= base_rss + 32 * 1_048_576,
        "RSS {rss} is more than rg's {base_rss} + 32 MiB"
    );
}

/// One of M3 review r2's linear walks (Codex #1): `body` in `name`, one Match, s4 within rg's
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
    eprintln!("{name}: s4 {t:?}, rg {base:?}");
    if timing_enabled() {
        assert!(
            t <= base + Duration::from_secs(1),
            "{name}: {t:?} against rg's {base:?}"
        );
    }
}

/// 40 000 nested `div`s: a start tag finds the `p` it ends without searching the stack (3.96 s
/// before `html::Reach`).
#[test]
#[cfg_attr(target_os = "linux", ignore)]
#[cfg_attr(
    not(target_os = "linux"),
    ignore = "requires GNU time -v RSS measurement"
)]
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
#[cfg_attr(target_os = "linux", ignore)]
#[cfg_attr(
    not(target_os = "linux"),
    ignore = "requires GNU time -v RSS measurement"
)]
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
#[cfg_attr(target_os = "linux", ignore)]
#[cfg_attr(
    not(target_os = "linux"),
    ignore = "requires GNU time -v RSS measurement"
)]
fn markup_implied_ends_at_one_byte_stay_linear() {
    let _alone = alone();
    let body = format!("{}needle{}", "<x>".repeat(20_000), " ".repeat(200_000));
    markup_linear("trim.html", body, 256);
}

/// One M3 Format at 50 MB (M3 plan, Task 7.1): `unit(i, needle)` writes the i-th repetition,
/// with `needle` at i = 0, in the middle and at the end. s4 must print three rows whose
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
        "50 MB {file}: s4 {t:?}, RSS {} MiB; rg {base:?}, RSS {} MiB",
        rss / 1_048_576,
        base_rss / 1_048_576
    );
    if timing_enabled() {
        assert!(
            t <= base + Duration::from_secs(1),
            "{file}: {t:?} is more than rg's {base:?} + 1 s"
        );
    }
    assert!(
        rss <= base_rss + 32 * 1_048_576,
        "{file}: RSS {rss} is more than rg's {base_rss} + 32 MiB"
    );
}

#[test]
#[cfg_attr(target_os = "linux", ignore)]
#[cfg_attr(
    not(target_os = "linux"),
    ignore = "requires GNU time -v RSS measurement"
)]
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
#[cfg_attr(target_os = "linux", ignore)]
#[cfg_attr(
    not(target_os = "linux"),
    ignore = "requires GNU time -v RSS measurement"
)]
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
#[cfg_attr(target_os = "linux", ignore)]
#[cfg_attr(
    not(target_os = "linux"),
    ignore = "requires GNU time -v RSS measurement"
)]
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
#[cfg_attr(target_os = "linux", ignore)]
#[cfg_attr(
    not(target_os = "linux"),
    ignore = "requires GNU time -v RSS measurement"
)]
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
#[cfg_attr(target_os = "linux", ignore)]
#[cfg_attr(
    not(target_os = "linux"),
    ignore = "requires GNU time -v RSS measurement"
)]
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
#[cfg_attr(target_os = "linux", ignore)]
#[cfg_attr(
    not(target_os = "linux"),
    ignore = "requires GNU time -v RSS measurement"
)]
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

/// M3 code review r1, Codex #1: a header cell is cut at 40 characters, so its size must not
/// cost memory per escape or per invalid byte. Before the fix an 8 MB cell of `""` cost 446 MiB
/// and one of Latin-1 bytes 327 MiB, against 14 MiB with `--no-trail`. What stays is linear: the
/// Source and csv-core's decoded header, whose buffer doubles.
#[test]
#[cfg_attr(target_os = "linux", ignore)]
#[cfg_attr(
    not(target_os = "linux"),
    ignore = "requires GNU time -v RSS measurement"
)]
fn a_huge_header_cell_costs_no_memory_per_escape() {
    let _alone = alone();
    let fx = Fixture::new();
    let size = 8 * 1024 * 1024;
    // The Trail of the Match: the column named by its cell, cut at 40 characters.
    let cells = [
        (
            "escapes.csv",
            [&b"\""[..], &b"\"\"".repeat(size / 2), b"\"\nneedle\n"].concat(),
            "\"",
        ),
        (
            "latin1.csv",
            [&[0xe9u8].repeat(size)[..], b"\nneedle\n"].concat(),
            "\u{FFFD}",
        ),
    ];
    for (file, body, unit) in &cells {
        std::fs::write(fx.file(file), body).unwrap();
        let Some((_, base)) = measure(
            &sfor_bin(),
            &fx,
            &["--no-docs", "--no-trail", "needle", file],
            0,
        ) else {
            eprintln!("skipping: /usr/bin/time not found");
            return;
        };
        let row = format!("2: row 1 › {}… │ needle\n", unit.repeat(39));
        // `--in-headings zzz` reads the header and finds nothing: exit 1.
        for (mode, code) in [(&["--flat"][..], 0), (&["--in-headings", "zzz"], 1)] {
            let mut args = mode.to_vec();
            if code == 0 {
                args.extend(["--color", "never", "needle"]);
            }
            args.push(file);
            let (t, rss, out) = measure_out(&sfor_bin(), &fx, &args, code).unwrap();
            assert_eq!(out, if code == 0 { row.as_str() } else { "" }, "{file}");
            eprintln!(
                "{file} {args:?}: {t:?}, RSS {} MiB; --no-trail: {} MiB",
                rss / 1_048_576,
                base / 1_048_576
            );
            assert!(
                rss < base + 3 * size as u64,
                "{file} {args:?}: RSS {rss} is not below {base} + 3 × {size}"
            );
        }
    }
}

/// M3 code review r1, Fable #4: an open element costs its Frame, and only an element with
/// children that can be Segments counts them. Before the fix a million nested elements (7 MB)
/// cost 295 MiB, about 300 bytes each.
#[test]
#[cfg_attr(target_os = "linux", ignore)]
#[cfg_attr(
    not(target_os = "linux"),
    ignore = "requires GNU time -v RSS measurement"
)]
fn a_million_nested_elements_cost_little_each() {
    let _alone = alone();
    let fx = Fixture::new();
    let n = 1_000_000;
    let body = ["<a>".repeat(n), "needle".into(), "</a>".repeat(n)].concat();
    for file in ["nest.xml", "nest.html"] {
        std::fs::write(fx.file(file), &body).unwrap();
        let Some((_, base)) = measure(
            &sfor_bin(),
            &fx,
            &["--no-docs", "--no-trail", "needle", file],
            0,
        ) else {
            eprintln!("skipping: /usr/bin/time not found");
            return;
        };
        let args = ["--color", "never", "--flat", "needle", file];
        let (t, rss, out) = measure_out(&sfor_bin(), &fx, &args, 0).unwrap();
        // 256 Segments (D25), which a flat Trail shows as its ends.
        assert!(out.starts_with("1: a › … › a │ "), "{file}: {out:.100}");
        assert_eq!(out.lines().count(), 1, "{file}");
        eprintln!(
            "{file}: {t:?}, RSS {} MiB; --no-trail: {} MiB",
            rss / 1_048_576,
            base / 1_048_576
        );
        assert!(
            rss < base + 128 * n as u64,
            "{file}: RSS {rss} is not below {base} + 128 × {n}"
        );
    }
}

/// M3 code review r2, Codex #1: an HTML name and a cue identifier are cut at 40 characters, so
/// their size must not cost memory per piece. Before the fix 4 million classes cost 160 MiB
/// and an 8 MB identifier of Latin-1 bytes 297 MiB, against 21 and 22 MiB with `--no-trail`.
#[test]
#[cfg_attr(target_os = "linux", ignore)]
#[cfg_attr(
    not(target_os = "linux"),
    ignore = "requires GNU time -v RSS measurement"
)]
fn a_huge_name_costs_no_memory_per_piece() {
    let _alone = alone();
    let fx = Fixture::new();
    let size = 8 * 1024 * 1024;
    let files = [
        (
            "classes.html",
            [
                &b"<div class=\""[..],
                &b"a ".repeat(size / 2),
                b"\">\nneedle\n</div>\n",
            ]
            .concat(),
            format!("2: div{}… │ needle\n", ".a".repeat(18)),
        ),
        (
            "latin1.srt",
            [
                &[0xe9u8].repeat(size)[..],
                b"\n00:00:01,000 --> 00:00:02,000\nneedle\n",
            ]
            .concat(),
            format!("3: cue {}… 00:00:01,000 │ needle\n", "\u{FFFD}".repeat(39)),
        ),
    ];
    for (file, body, row) in &files {
        std::fs::write(fx.file(file), body).unwrap();
        let Some((_, base)) = measure(
            &sfor_bin(),
            &fx,
            &["--no-docs", "--no-trail", "needle", file],
            0,
        ) else {
            eprintln!("skipping: /usr/bin/time not found");
            return;
        };
        let args = ["--color", "never", "--flat", "needle", file];
        let (t, rss, out) = measure_out(&sfor_bin(), &fx, &args, 0).unwrap();
        assert_eq!(&out, row, "{file}");
        eprintln!(
            "{file}: {t:?}, RSS {} MiB; --no-trail: {} MiB",
            rss / 1_048_576,
            base / 1_048_576
        );
        assert!(
            rss < base + 3 * size as u64,
            "{file}: RSS {rss} is not below {base} + 3 × {size}"
        );
    }
}

/// M3 code review r2, Fable #3: `trails` builds the names of the columns some anchor is in, not
/// of every header cell. Before the fix a header of a million columns (4 MB) cost 270 MiB, about
/// 270 bytes per cell, against 13 MiB with `--no-trail`; what stays is the header's fields and
/// decoded text, 40 bytes per cell.
#[test]
#[cfg_attr(target_os = "linux", ignore)]
#[cfg_attr(
    not(target_os = "linux"),
    ignore = "requires GNU time -v RSS measurement"
)]
fn a_header_of_a_million_columns_names_only_the_columns_matched() {
    let _alone = alone();
    let fx = Fixture::new();
    let n = 1_000_000;
    let mut body = (1..=n)
        .map(|k| format!("c{k}"))
        .collect::<Vec<_>>()
        .join(",");
    body.push_str("\nneedle,x\n");
    std::fs::write(fx.file("wide.csv"), &body).unwrap();
    let Some((_, base)) = measure(
        &sfor_bin(),
        &fx,
        &["--no-docs", "--no-trail", "needle", "wide.csv"],
        0,
    ) else {
        eprintln!("skipping: /usr/bin/time not found");
        return;
    };
    let args = ["--color", "never", "--flat", "needle", "wide.csv"];
    let (t, rss, out) = measure_out(&sfor_bin(), &fx, &args, 0).unwrap();
    assert_eq!(out, "2: row 1 › c1 │ needle,x\n");
    eprintln!(
        "a million columns: {t:?}, RSS {} MiB; --no-trail: {} MiB",
        rss / 1_048_576,
        base / 1_048_576
    );
    assert!(
        rss < base + 64 * n as u64,
        "RSS {rss} is not below {base} + 64 × {n}"
    );
}

/// M3 code review r3, Fable #1: the rows of one line, one per Trail, cost the line once and not
/// once per row. Before the fix 20 000 Matches in one CSV row took 6.3 s with `--flat`, against
/// 0.03 s for the same line in a `.txt` file.
#[test]
#[cfg_attr(target_os = "linux", ignore)]
#[cfg_attr(
    not(target_os = "linux"),
    ignore = "requires GNU time -v RSS measurement"
)]
fn the_rows_of_one_wide_line_cost_the_line_once() {
    let _alone = alone();
    let fx = Fixture::new();
    let n = 20_000;
    let header = (0..n).map(|k| format!("h{k}")).collect::<Vec<_>>();
    let row = vec!["needle"; n];
    let body = format!("{}\n{}\n", header.join(","), row.join(","));
    std::fs::write(fx.file("wide.csv"), &body).unwrap();
    std::fs::write(fx.file("wide.txt"), &body).unwrap();
    let (out, _) = sfor_output(&fx, &["--flat", "needle", "wide.csv"]);
    assert_eq!(out.lines().count(), n, "{out:.200}");
    assert!(out.starts_with("2: row 1 › h0 │ needle,"), "{out:.100}");
    for layout in ["--flat", "--tree"] {
        let Some((base, _)) = best_of_three(&fx, &[layout, "needle", "wide.txt"], 0) else {
            eprintln!("skipping: /usr/bin/time not found");
            return;
        };
        let (t, _) = best_of_three(&fx, &[layout, "needle", "wide.csv"], 0).unwrap();
        eprintln!("{n} Matches in one CSV row, {layout}: {t:?}; as .txt: {base:?}");
        if timing_enabled() {
            assert!(
                t <= base * 5 + Duration::from_millis(250),
                "{layout}: {t:?} is more than 5 × the {base:?} of the .txt + 0.25 s"
            );
        }
    }
}

// M4 (plan, Task 13.1): object files and documents.

use searchfor::docs::fixture as doc;
use searchfor::processors::object::fixture as obj;

/// An ELF whose `.rodata` is `SIZE` bytes of NUL-separated strings, with `needle` in the first,
/// the middle and the last one, and `symbols` symbols spread over it.
fn big_elf(fx: &Fixture, symbols: usize) -> &'static str {
    let mut ro = Vec::with_capacity(SIZE + 64);
    let mut n = 0usize;
    let mut middle = false;
    while ro.len() < SIZE {
        let word: &[u8] = if n == 0 || (!middle && ro.len() >= SIZE / 2) {
            middle = n > 0;
            b"version needle!\0"
        } else {
            b"some string of hay\0"
        };
        ro.extend_from_slice(word);
        n += 1;
    }
    ro.extend_from_slice(b"last needle!\0");
    let mut spec = obj::Spec::new()
        .section(".text", b"\xc3\0\0\0")
        .section(".rodata", &ro);
    let step = (ro.len() / symbols.max(1)).max(1);
    for i in 0..symbols {
        spec = spec.symbol(&format!("sym_{i}"), 1, (i * step) as u64);
    }
    std::fs::write(fx.file("big.elf"), obj::elf(&spec).file).unwrap();
    "big.elf"
}

fn object_ceiling(symbols: usize) {
    let fx = Fixture::new();
    let file = big_elf(&fx, symbols);
    let args = ["--flat", "needle", file];
    let (out, err) = sfor_output(&fx, &args);
    assert_eq!(err, "");
    assert!(out.lines().count() >= 3, "{out:.400}");
    assert!(
        out.lines()
            .all(|l| l.starts_with("0x") && l.contains("ELF x86-64 › .rodata")),
        "{out:.400}"
    );
    let Some(rg) = rg_bin() else { return };
    let Some((t, rss)) = best_of_three_drained(&sfor_bin(), &fx, &args, 0) else {
        return;
    };
    // rg -a searches the whole file, as s4 does for an object file with a Match.
    let (base, base_rss) = best_of_three_drained(&rg, &fx, &["-a", "needle", file], 0).unwrap();
    eprintln!(
        "50 MB ELF, {symbols} symbols: s4 {t:?}, RSS {} MiB; rg -a {base:?}, RSS {} MiB",
        rss / 1_048_576,
        base_rss / 1_048_576
    );
    if timing_enabled() {
        assert!(
            t <= base + Duration::from_secs(1),
            "{t:?} is more than rg -a's {base:?} + 1 s"
        );
    }
    assert!(
        rss <= base_rss + 64 * 1_048_576,
        "RSS {rss} is more than rg -a's {base_rss} + 64 MiB"
    );
}

#[test]
#[cfg_attr(target_os = "linux", ignore)]
#[cfg_attr(
    not(target_os = "linux"),
    ignore = "requires GNU time -v RSS measurement"
)]
fn object_rows_of_a_big_elf_stay_under_rg_a() {
    let _alone = alone();
    object_ceiling(0);
}

#[test]
#[cfg_attr(target_os = "linux", ignore)]
#[cfg_attr(
    not(target_os = "linux"),
    ignore = "requires GNU time -v RSS measurement"
)]
fn an_elf_with_two_hundred_thousand_symbols_stays_under_rg_a() {
    let _alone = alone();
    object_ceiling(200_000);
}

/// A document of each kind with `needle` in its first, middle and last unit; the Extractor's
/// text of it is written beside it as `NAME.txt`, so rg has the same text to search.
fn big_document(fx: &Fixture, kind: &str, units: usize) -> (String, usize) {
    let text = |i: usize| {
        let n = if i == 0 || i == units / 2 || i == units - 1 {
            "needle"
        } else {
            "hay"
        };
        format!("Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do {n} {i}.")
    };
    let bytes = match kind {
        "docx" | "odt" => {
            let mut blocks = Vec::new();
            for i in 0..units {
                if i % 1000 == 0 {
                    blocks.push(doc::h(1, &format!("Chapter {i}")));
                }
                blocks.push(doc::p(&text(i)));
            }
            if kind == "docx" {
                doc::docx(&blocks)
            } else {
                doc::odt(&blocks)
            }
        }
        "xlsx" | "ods" => {
            let rows: Vec<[String; 3]> = (0..units)
                .map(|i| [format!("item {i}"), format!("{}", i * 3), text(i)])
                .collect();
            let refs: Vec<Vec<&str>> = rows
                .iter()
                .map(|r| r.iter().map(String::as_str).collect())
                .collect();
            let rows: Vec<&[&str]> = refs.iter().map(Vec::as_slice).collect();
            if kind == "xlsx" {
                doc::xlsx(&[("Sheet", &rows)])
            } else {
                doc::ods(&[("Sheet", &rows)])
            }
        }
        "pptx" => {
            let slides: Vec<[String; 2]> = (0..units)
                .map(|i| [format!("Slide {i}"), text(i)])
                .collect();
            let refs: Vec<Vec<&str>> = slides
                .iter()
                .map(|s| s.iter().map(String::as_str).collect())
                .collect();
            let slides: Vec<(&[&str], Option<&str>)> =
                refs.iter().map(|s| (s.as_slice(), None)).collect();
            doc::pptx(&slides)
        }
        "epub" => {
            let chapters: Vec<(String, String)> = (0..units / 1000)
                .map(|c| {
                    (
                        format!("Chapter {c}"),
                        (c * 1000..(c + 1) * 1000)
                            .map(|i| format!("<p>{}</p>", text(i)))
                            .collect(),
                    )
                })
                .collect();
            let refs: Vec<(&str, &str)> = chapters
                .iter()
                .map(|(a, b)| (a.as_str(), b.as_str()))
                .collect();
            doc::epub(&refs, true)
        }
        "pdf" => {
            let lines: Vec<String> = (0..units).map(text).collect();
            let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
            let pages: Vec<&[&str]> = refs.chunks(50).collect();
            doc::pdf(&pages, &[(1, "Start", 1)])
        }
        k => panic!("{k}"),
    };
    let name = format!("big.{kind}");
    std::fs::write(fx.file(&name), &bytes).unwrap();
    let kind = searchfor::docs::Kind::from_name(kind).unwrap();
    let e = searchfor::docs::extract(kind, &bytes).unwrap();
    std::fs::write(fx.file(&format!("{name}.txt")), &e.text).unwrap();
    (name, e.text.len())
}

/// D23: an Extractor reads its document at `rate` MB/s of text or faster, on top of rg's search
/// of that text and 1 s; its memory stays under rg's plus six times the text, plus 400 bytes
/// per cell Segment (`cells`: a spreadsheet's), plus 64 MiB. It is searched with no flag,
/// which extracts it (grill, 8750).
fn document_ceiling(kind: &str, units: usize, rate: f64, cells: usize) {
    let fx = Fixture::new();
    let (name, text) = big_document(&fx, kind, units);
    let args = ["--flat", "needle", name.as_str()];
    let (out, err) = sfor_output(&fx, &args);
    assert_eq!(err, "", "{kind}");
    assert_eq!(out.lines().count(), 3, "{kind}: {out:.400}");
    let Some(rg) = rg_bin() else { return };
    let Some((t, rss)) = best_of_three_drained(&sfor_bin(), &fx, &args, 0) else {
        return;
    };
    let txt = format!("{name}.txt");
    let (base, base_rss) = best_of_three_drained(&rg, &fx, &["needle", txt.as_str()], 0).unwrap();
    let budget =
        base + Duration::from_secs_f64(text as f64 / (rate * 1e6)) + Duration::from_secs(1);
    eprintln!(
        "{kind}: {} MB of text: s4 {t:?} (budget {budget:?}), RSS {} MiB; rg on the text {base:?}, RSS {} MiB",
        text / 1_000_000,
        rss / 1_048_576,
        base_rss / 1_048_576
    );
    if timing_enabled() {
        assert!(t <= budget, "{kind}: {t:?} is over {budget:?}");
    }
    let bound = base_rss + 6 * text as u64 + 400 * cells as u64 + 64 * 1_048_576;
    assert!(rss <= bound, "{kind}: RSS {rss} is over {bound}");
}

#[test]
#[cfg_attr(target_os = "linux", ignore)]
#[cfg_attr(
    not(target_os = "linux"),
    ignore = "requires GNU time -v RSS measurement"
)]
fn documents_extract_within_their_ceilings() {
    let _alone = alone();
    // The rates are about a third of what the M4 probe measured (M4 plan, D23).
    for (kind, units, rate, cells) in [
        ("docx", 200_000, 12.0, 0),
        ("odt", 200_000, 10.0, 0),
        ("epub", 200_000, 20.0, 0),
        ("pdf", 100_000, 7.0, 0),
        ("xlsx", 200_000, 2.0, 600_000),
        ("ods", 200_000, 1.0, 600_000),
        ("pptx", 20_000, 1.3, 0),
    ] {
        document_ceiling(kind, units, rate, cells);
    }
}

/// D17: a document whose part decompresses past 256 MiB is refused with a warning, quickly and
/// within 256 MiB plus rg's memory and 64 MiB, and is then searched as rg searches it. The part
/// is 512 MiB, past that bound: read whole, a part of 300 MiB stayed under it (RSS 304 MiB
/// without `Zip::bytes`' `take`; M4 plan, Task 13.1, Step 2).
#[test]
#[cfg_attr(target_os = "linux", ignore)]
#[cfg_attr(
    not(target_os = "linux"),
    ignore = "requires GNU time -v RSS measurement"
)]
fn a_zip_bomb_is_refused_within_its_limit() {
    let _alone = alone();
    let fx = Fixture::new();
    let mut xml = String::from("<w:document><w:body><w:p><w:r><w:t>");
    xml.push_str(&" ".repeat(512 << 20));
    xml.push_str("needle</w:t></w:r></w:p></w:body></w:document>");
    let bomb = doc::zip(&[("word/document.xml", xml.into_bytes())], &[]);
    std::fs::write(fx.file("bomb.docx"), &bomb).unwrap();
    let Some(rg) = rg_bin() else { return };
    let args = ["--docs", "--flat", "needle", "bomb.docx"];
    let (t, rss, _) = measure_to(&sfor_bin(), &fx, &args, None, Out::Null, 1).unwrap();
    let (_, base_rss) = measure(&rg, &fx, &["needle", "bomb.docx"], 1).unwrap();
    eprintln!(
        "zip bomb of {} KiB: {t:?}, RSS {} MiB",
        bomb.len() >> 10,
        rss / 1_048_576
    );
    if timing_enabled() {
        assert!(t <= Duration::from_secs(5), "{t:?}");
    }
    assert!(rss <= base_rss + (256 + 64) * 1_048_576, "RSS {rss}");
    let o = common::run(
        Command::new(sfor_bin())
            .args(args)
            .current_dir(fx.path())
            .env_remove("SEARCHFOR_CONFIG_PATH")
            .env_remove("RIPGREP_CONFIG_PATH")
            .stdin(Stdio::null()),
    );
    assert!(
        common::stderr(&o).contains("bomb.docx: docx: word/document.xml is over the 256 MiB limit"),
        "{}",
        common::stderr(&o)
    );
}

/// A one-page PDF whose only font, without `/Encoding`, has a ToUnicode map that decompresses
/// to `inflated` bytes of spaces.
fn pdf_font_bomb(inflated: usize) -> Vec<u8> {
    let content = b"BT /F1 12 Tf 72 760 Td (needle here) Tj ET".to_vec();
    one_page_pdf(content, false, Some(vec![b' '; inflated]))
}

/// A one-page PDF drawing `content` (compressed when `compress`) with a Type 1 font, without
/// `/Encoding`, whose ToUnicode map (compressed) is `map`.
fn one_page_pdf(content: Vec<u8>, compress: bool, map: Option<Vec<u8>>) -> Vec<u8> {
    use lopdf::{Dictionary, Document, Object, Stream};
    let mut doc = Document::with_version("1.5");
    let mut font = Dictionary::new();
    font.set("Type", Object::Name(b"Font".to_vec()));
    font.set("Subtype", Object::Name(b"Type1".to_vec()));
    font.set("BaseFont", Object::Name(b"Helvetica".to_vec()));
    if let Some(map) = map {
        let mut map = Stream::new(Dictionary::new(), map);
        map.compress().unwrap();
        font.set("ToUnicode", Object::Reference(doc.add_object(map)));
    }
    let font_id = doc.add_object(font);
    let mut content = Stream::new(Dictionary::new(), content);
    if compress {
        content.compress().unwrap();
    }
    let content_id = doc.add_object(content);
    let pages_id = doc.new_object_id();
    let mut fonts = Dictionary::new();
    fonts.set("F1", Object::Reference(font_id));
    let mut res = Dictionary::new();
    res.set("Font", Object::Dictionary(fonts));
    let mut page = Dictionary::new();
    page.set("Type", Object::Name(b"Page".to_vec()));
    page.set("Parent", Object::Reference(pages_id));
    page.set("Contents", Object::Reference(content_id));
    page.set("Resources", Object::Dictionary(res));
    let page_id = doc.add_object(page);
    let mut pages = Dictionary::new();
    pages.set("Type", Object::Name(b"Pages".to_vec()));
    pages.set("Count", Object::Integer(1));
    pages.set("Kids", Object::Array(vec![Object::Reference(page_id)]));
    let media = vec![0.into(), 0.into(), 612.into(), 792.into()];
    pages.set("MediaBox", Object::Array(media));
    doc.objects.insert(pages_id, Object::Dictionary(pages));
    let mut cat = Dictionary::new();
    cat.set("Type", Object::Name(b"Catalog".to_vec()));
    cat.set("Pages", Object::Reference(pages_id));
    let cat_id = doc.add_object(cat);
    doc.trailer.set("Root", Object::Reference(cat_id));
    let mut out = Vec::new();
    doc.save_to(&mut out).unwrap();
    out
}

/// A PDF 1.5 whose one object stream decompresses to `inflated` bytes of `1 `, an array of
/// small integers, referred to by a cross-reference stream (M4 plan r2, Fable #1: lopdf parses
/// object streams whole while it loads the file).
fn pdf_objstm_bomb(inflated: usize) -> Vec<u8> {
    let content = b"BT /F1 12 Tf 72 700 Td (needle) Tj ET";
    let mut big = b"7 0\n[".to_vec();
    big.extend(b"1 ".repeat(inflated / 2));
    big.push(b']');
    let mut packed = lopdf::Stream::new(lopdf::Dictionary::new(), big);
    packed.compress().unwrap();
    let stream =
        |dict: String, data: &[u8]| [dict.as_bytes(), b"\nstream\n", data, b"\nendstream"].concat();
    let page =
        "<< /Type /Page /Parent 2 0 R /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>";
    let objs: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Count 1 /Kids [3 0 R] /MediaBox [0 0 612 792] >>".to_vec(),
        page.as_bytes().to_vec(),
        stream(format!("<< /Length {} >>", content.len()), content),
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec(),
        stream(
            format!(
                "<< /Type /ObjStm /N 1 /First 4 /Length {} /Filter /FlateDecode >>",
                packed.content.len()
            ),
            &packed.content,
        ),
    ];
    let mut body = b"%PDF-1.5\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offsets.push(body.len());
        body.extend(format!("{} 0 obj\n", i + 1).as_bytes());
        body.extend(o);
        body.extend(b"\nendobj\n");
    }
    let xref_at = body.len();
    let row =
        |t: u8, a: usize, b: u16| [&[t][..], &(a as u32).to_be_bytes(), &b.to_be_bytes()].concat();
    let mut xref = row(0, 0, 65535);
    for o in &offsets {
        xref.extend(row(1, *o, 0));
    }
    // Object 7 is the first of object stream 6; object 8 is the cross-reference stream.
    xref.extend(row(2, 6, 0));
    xref.extend(row(1, xref_at, 0));
    let dict = format!(
        "8 0 obj\n<< /Type /XRef /Size 9 /W [1 4 2] /Root 1 0 R /Length {} >>",
        xref.len()
    );
    body.extend(stream(dict, &xref));
    body.extend(format!("\nendobj\nstartxref\n{xref_at}\n%%EOF\n").as_bytes());
    body
}

/// Runs `s4 --docs --flat needle <name>` on the bomb `bytes`: it ends within 5 s and
/// `extra_mib` plus rg's memory and 64 MiB, warning `warning`, and exits with `code`, as rg
/// searching the file as it is.
fn refuses_the_bomb(name: &str, bytes: &[u8], extra_mib: u64, warning: &str, code: i32) {
    let fx = Fixture::new();
    std::fs::write(fx.file(name), bytes).unwrap();
    let Some(rg) = rg_bin() else { return };
    let args = ["--docs", "--flat", "needle", name];
    let (t, rss, _) = measure_to(&sfor_bin(), &fx, &args, None, Out::Null, code).unwrap();
    let (_, base_rss) = measure(&rg, &fx, &["needle", name], code).unwrap();
    eprintln!(
        "{name} of {} KiB: {t:?}, RSS {} MiB",
        bytes.len() >> 10,
        rss / 1_048_576
    );
    if timing_enabled() {
        assert!(t <= Duration::from_secs(5), "{t:?}");
    }
    assert!(rss <= base_rss + (extra_mib + 64) * 1_048_576, "RSS {rss}");
    let o = common::run(
        Command::new(sfor_bin())
            .args(args)
            .current_dir(fx.path())
            .env_remove("SEARCHFOR_CONFIG_PATH")
            .env_remove("RIPGREP_CONFIG_PATH")
            .stdin(Stdio::null()),
    );
    assert!(
        common::stderr(&o).contains(warning),
        "{}",
        common::stderr(&o)
    );
    if warning.is_empty() {
        assert_eq!(common::stderr(&o), "");
    }
}

/// D17 (Fable r1 #1): a PDF whose font's ToUnicode map decompresses past 64 MiB is refused with
/// a warning, quickly and within 64 MiB plus rg's memory and 64 MiB, and is then searched as
/// rg searches it.
#[test]
#[cfg_attr(target_os = "linux", ignore)]
#[cfg_attr(
    not(target_os = "linux"),
    ignore = "requires GNU time -v RSS measurement"
)]
fn a_pdf_bomb_is_refused_within_its_limit() {
    let _alone = alone();
    refuses_the_bomb(
        "bomb.pdf",
        &pdf_font_bomb(300 << 20),
        64,
        "bomb.pdf: pdf: a font's ToUnicode map is over the 64 MiB limit",
        // The page's content stream is not compressed: rg finds the needle in it.
        0,
    );
}

/// D17 (Fable r1 #3): a workbook of sheets under 256 MiB each and over 1 GiB together is
/// refused with a warning, quickly and within 256 MiB plus rg's memory and 64 MiB, and is then
/// searched as rg searches it.
#[test]
#[cfg_attr(target_os = "linux", ignore)]
#[cfg_attr(
    not(target_os = "linux"),
    ignore = "requires GNU time -v RSS measurement"
)]
fn a_zip_bomb_of_many_parts_is_refused_within_its_limit() {
    let _alone = alone();
    let names = ["S1", "S2", "S3", "S4", "S5", "S6"];
    let row: &[&str] = &["needle"];
    let sheets: Vec<(&str, &[&[&str]])> = names
        .iter()
        .map(|n| (*n, std::slice::from_ref(&row)))
        .collect();
    let pad = " ".repeat(200 << 20);
    let bomb = doc::rezip(&doc::xlsx(&sheets), |name, part| {
        if !name.starts_with("xl/worksheets/") {
            return part;
        }
        String::from_utf8(part)
            .unwrap()
            .replace("</sheetData>", &format!("{pad}</sheetData>"))
            .into_bytes()
    });
    refuses_the_bomb(
        "bomb.xlsx",
        &bomb,
        256,
        "bomb.xlsx: xlsx: the whole document, decompressed, is over the 1024 MiB limit",
        1,
    );
}

/// D17 (M4 plan r2, Fable #1, Codex #3): a page of 63 MiB of `h`, 33 million operations, is
/// refused by the work budget as it is read, without becoming a list of operations: lopdf's
/// took 6 GB for one.
#[test]
#[cfg_attr(target_os = "linux", ignore)]
#[cfg_attr(
    not(target_os = "linux"),
    ignore = "requires GNU time -v RSS measurement"
)]
fn a_pdf_page_of_millions_of_operations_is_refused_within_its_limit() {
    let _alone = alone();
    refuses_the_bomb(
        "ops.pdf",
        &one_page_pdf(b"h\n".repeat(63 << 19), true, None),
        64,
        "ops.pdf: pdf: its content streams run more than 8020000 operations",
        1,
    );
}

/// D17 (M4 plan r2, Fable #1): an object stream of 63 MiB of small integers is refused before
/// lopdf parses it: parsed, 32 MiB took 2 GB.
#[test]
#[cfg_attr(target_os = "linux", ignore)]
#[cfg_attr(
    not(target_os = "linux"),
    ignore = "requires GNU time -v RSS measurement"
)]
fn a_pdf_object_stream_bomb_is_refused_within_its_limit() {
    let _alone = alone();
    refuses_the_bomb(
        "objstm.pdf",
        &pdf_objstm_bomb(63 << 20),
        64,
        "objstm.pdf: pdf: its object streams hold more than 2000000 tokens",
        // The page's content stream is not compressed: rg finds the needle in it.
        0,
    );
}

/// D17 (M4 plan r2, Codex #1): a ToUnicode range of 2³² codes is read as one range: lopdf's
/// reader listed every code.
#[test]
#[cfg_attr(target_os = "linux", ignore)]
#[cfg_attr(
    not(target_os = "linux"),
    ignore = "requires GNU time -v RSS measurement"
)]
fn a_pdf_map_range_of_every_code_is_read_within_its_limit() {
    let _alone = alone();
    let map = b"/CIDInit /ProcSet findresource begin 12 dict begin begincmap\n\
        1 begincodespacerange <00> <FF> endcodespacerange\n2 beginbfrange\n\
        <00> <FF> <0000>\n<00000000> <FFFFFFFF> <0041>\nendbfrange\nendcmap end end\n";
    let content = b"BT /F1 12 Tf 72 700 Td (needle) Tj ET".to_vec();
    refuses_the_bomb(
        "range.pdf",
        &one_page_pdf(content, true, Some(map.to_vec())),
        64,
        "",
        0,
    );
}

/// D17 (M4 plan r2, Codex #2): a ToUnicode map that gives a code 256 letters, and a string of
/// 4 MiB of it, is decoded at most `MAX_GROWTH` bytes for each byte of the string: it made 512 MiB
/// of text, 9 s and 1.7 GB, from a 5 KB file.
#[test]
#[cfg_attr(target_os = "linux", ignore)]
#[cfg_attr(
    not(target_os = "linux"),
    ignore = "requires GNU time -v RSS measurement"
)]
fn a_pdf_map_of_long_text_is_read_within_its_limit() {
    let _alone = alone();
    let mut map = b"/CIDInit /ProcSet findresource begin 12 dict begin begincmap\n\
        1 begincodespacerange <00> <FF> endcodespacerange\n1 beginbfchar\n<41> <"
        .to_vec();
    map.extend(b"006E".repeat(256));
    map.extend(b">\nendbfchar\nendcmap end end\n");
    let mut content = b"BT /F1 12 Tf 72 700 Td (".to_vec();
    content.extend(vec![b'A'; 4 << 20]);
    content.extend(b") Tj ET");
    refuses_the_bomb(
        "long.pdf",
        &one_page_pdf(content, true, Some(map)),
        64,
        "",
        1,
    );
}

/// D30 (M4 plan r3, Codex #3, Fable #2): an encrypted PDF whose 64 object streams hold 1 MiB
/// of small integers each, which lopdf parses while it decrypts, where no budget of ours is
/// called: its reader passes 1 GiB and is ended, with one warning, within 10 s. Made by qpdf;
/// without it the test says so and passes.
#[test]
#[cfg_attr(target_os = "linux", ignore)]
#[cfg_attr(
    not(target_os = "linux"),
    ignore = "requires GNU time -v RSS measurement"
)]
fn a_pdf_past_the_readers_memory_is_refused_within_its_limit() {
    let _alone = alone();
    let fx = Fixture::new();
    let mut doc = lopdf::Document::load_mem(&doc::pdf(&[&["needle"]], &[])).unwrap();
    let big: Vec<lopdf::Object> = (0..64)
        .map(|_| {
            lopdf::Object::Reference(doc.add_object(lopdf::Object::Array(vec![1.into(); 1 << 19])))
        })
        .collect();
    let root = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
    let cat = doc.get_object_mut(root).unwrap().as_dict_mut().unwrap();
    cat.set("Big", lopdf::Object::Array(big));
    let options = lopdf::SaveOptions::builder()
        .use_object_streams(true)
        .use_xref_streams(true)
        .max_objects_per_stream(1)
        .build();
    let mut plain = Vec::new();
    doc.save_with_options(&mut plain, options).unwrap();
    std::fs::write(fx.file("plain.pdf"), &plain).unwrap();
    let made = Command::new("qpdf")
        .args([
            "--encrypt",
            "",
            "",
            "128",
            "--use-aes=y",
            "--",
            "--object-streams=preserve",
            "plain.pdf",
            "enc.pdf",
        ])
        .current_dir(fx.path())
        .status();
    if !made.is_ok_and(|s| s.success()) {
        eprintln!("skipping: qpdf is missing");
        return;
    }
    let Some(rg) = rg_bin() else { return };
    let args = ["--docs", "--flat", "needle", "enc.pdf"];
    let (t, rss, _) = measure_to(&sfor_bin(), &fx, &args, None, Out::Null, 1).unwrap();
    let (_, base_rss) = measure(&rg, &fx, &["needle", "enc.pdf"], 1).unwrap();
    eprintln!("enc.pdf: {t:?}, RSS {} MiB", rss / 1_048_576);
    if timing_enabled() {
        assert!(t <= Duration::from_secs(10), "{t:?}");
    }
    assert!(rss <= base_rss + (1024 + 64) * 1_048_576, "RSS {rss}");
    let o = common::run(
        Command::new(sfor_bin())
            .args(args)
            .current_dir(fx.path())
            .env_remove("SEARCHFOR_CONFIG_PATH")
            .env_remove("RIPGREP_CONFIG_PATH")
            .stdin(Stdio::null()),
    );
    assert!(
        common::stderr(&o)
            .contains("enc.pdf: pdf: reading it needed more than the 1024 MiB a reader may use"),
        "{}",
        common::stderr(&o)
    );
}

/// `bin args` run in the fixture copy as the other measures run it, stdout and stderr
/// discarded; it must exit with `code`.
#[cfg(unix)]
fn command(bin: &Path, fx: &Fixture, args: &[&str]) -> Command {
    let mut c = Command::new(bin);
    c.args(args)
        .current_dir(fx.path())
        .env("SEARCHFOR_FORMATS_DIR", fx.formats.path())
        .env("SEARCHFOR_GRAMMARS_DIR", fx.grammars.path())
        .env("SEARCHFOR_GRAMMAR_INDEX", fx.no_index())
        .env_remove("SEARCHFOR_CONFIG_PATH")
        .env_remove("RIPGREP_CONFIG_PATH")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    c
}

/// The CPU time, user and system, of s4 with `args`, from `getrusage` of this process's
/// children: the perf tests run one at a time (`alone`), so only this run ends meanwhile.
#[cfg(unix)]
fn cpu_time(fx: &Fixture, args: &[&str], code: i32) -> Duration {
    let children = || {
        // SAFETY: getrusage writes the struct it is given.
        let mut ru: libc::rusage = unsafe { std::mem::zeroed() };
        assert_eq!(
            unsafe { libc::getrusage(libc::RUSAGE_CHILDREN, &mut ru) },
            0
        );
        let us = |t: libc::timeval| t.tv_sec as u64 * 1_000_000 + t.tv_usec as u64;
        Duration::from_micros(us(ru.ru_utime) + us(ru.ru_stime))
    };
    let before = children();
    let status = command(&sfor_bin(), fx, args).status().unwrap();
    assert_eq!(status.code(), Some(code), "{args:?}");
    children() - before
}

/// The system calls s4 with `args` makes, all its threads and children included, as
/// `strace -f -c` counts them; None without strace, or where it cannot trace.
#[cfg(target_os = "linux")]
fn system_calls(fx: &Fixture, args: &[&str], code: i32) -> Option<u64> {
    let out = fx.path().join("strace.txt");
    let _ = std::fs::remove_file(&out);
    let mut c = command(Path::new("strace"), fx, &["-f", "-c", "-o"]);
    c.arg(&out).arg(sfor_bin()).args(args).env("LC_ALL", "C");
    let status = c.status().ok()?;
    let counted = std::fs::read_to_string(&out).ok()?;
    // `% time  seconds  usecs/call  calls  [errors]  total`
    let calls = counted
        .lines()
        .find(|l| l.trim_end().ends_with(" total"))?
        .split_whitespace()
        .nth(3)?
        .parse()
        .ok()?;
    assert_eq!(status.code(), Some(code), "strace s4 {args:?}");
    Some(calls)
}

/// Code review r6, Fable #1: by default, a file without an extension that is no document costs
/// what it costs with --no-docs (spec §11 #26: at most 1.05 ×): no second open of the file and
/// no walk of its name through the process's descriptors. Over 20 000 files of 96 bytes without
/// an extension, seven levels deep (as `.git/objects` or `bin/`), with one thread, the default
/// makes at most 1.05 × the system calls of --no-docs, counted by strace when it can trace, and
/// takes at most 1.1 × its CPU time, the median ratio of 15 pairs of runs back to back
/// (`paired_cpu_ratio`; the margin is the noise of a loaded machine), with and without the
/// Trail. With `-L` too, through a tree that
/// has a link to one of its directories (code review r7, Fable #2): a name below a root is
/// resolved only where the walker followed a link.
#[cfg(target_os = "linux")]
#[test]
#[ignore]
fn files_without_an_extension_cost_by_default_what_they_cost_without_documents() {
    let _alone = alone();
    let fx = Fixture::new();
    let mut n = 0;
    for a in 0..4 {
        for b in 0..4 {
            for c in 0..5 {
                let dir = fx.file(&format!("noext/d{a}/d{b}/d{c}/e/f/g"));
                std::fs::create_dir_all(&dir).unwrap();
                for _ in 0..250 {
                    let line = format!("line {n:05} of a small file without an extension\n");
                    std::fs::write(dir.join(format!("f{n:05x}")), line.repeat(2)).unwrap();
                    n += 1;
                }
            }
        }
    }
    assert_eq!(n, 20_000);
    std::os::unix::fs::symlink("d2", fx.file("noext/d3/linked")).unwrap();
    for (follow, trail) in [
        (&[][..], &["--no-trail"][..]),
        (&[], &[]),
        (&["-L"], &["--no-trail"]),
        (&["-L"], &[]),
    ] {
        let trail = &[follow, trail].concat();
        let with = |docs: &[&'static str]| {
            let mut a = vec!["-j1"];
            a.extend_from_slice(docs);
            a.extend_from_slice(trail);
            a.extend(["zzqq-absent", "noext"]);
            a
        };
        let (default, off) = (with(&[]), with(&["--no-docs"]));
        match (system_calls(&fx, &default, 1), system_calls(&fx, &off, 1)) {
            (Some(d), Some(o)) => {
                eprintln!("{trail:?}: {d} system calls by default, {o} with --no-docs");
                if timing_enabled() {
                    assert!(d as f64 <= 1.05 * o as f64, "{d} > 1.05 × {o}");
                }
            }
            _ => eprintln!("{trail:?}: strace cannot trace here: system calls not counted"),
        }
        let ratio = paired_cpu_ratio(&fx, &default, &off, 1, 15);
        eprintln!("{trail:?}: CPU {ratio:.3} × by default that of --no-docs");
        if timing_enabled() {
            assert!(ratio <= 1.1, "{trail:?}: CPU {ratio:.3} × > 1.1 ×");
        }
    }
}

/// The CPU time of s4 with `a` over its CPU time with `b`, both exiting with `code`: the median
/// of the ratios of `pairs` pairs of runs, the two runs of a pair one right after the other and
/// in alternated order. The load of the machine weighs on both runs of a pair alike, and a
/// burst of it in one run moves one ratio, which the median leaves out (code review m4-code r8,
/// Fable #6: the medians of each side, nine runs apart, gave 1.117 × on a loaded machine where
/// the system calls gave 0.81 ×).
#[cfg(target_os = "linux")]
fn paired_cpu_ratio(fx: &Fixture, a: &[&str], b: &[&str], code: i32, pairs: usize) -> f64 {
    let mut ratios = Vec::new();
    let (mut ta, mut tb) = (Vec::new(), Vec::new());
    for i in 0..pairs {
        let (x, y) = if i % 2 == 0 {
            let x = cpu_time(fx, a, code);
            (x, cpu_time(fx, b, code))
        } else {
            let y = cpu_time(fx, b, code);
            (cpu_time(fx, a, code), y)
        };
        ratios.push(x.as_secs_f64() / y.as_secs_f64().max(1e-6));
        ta.push(x);
        tb.push(y);
    }
    ratios.sort_by(f64::total_cmp);
    ta.sort();
    tb.sort();
    let m = pairs / 2;
    eprintln!(
        "  medians {:?} and {:?} ({:.3} ×), minimums {:?} and {:?} ({:.3} ×)",
        ta[m],
        tb[m],
        ta[m].as_secs_f64() / tb[m].as_secs_f64(),
        ta[0],
        tb[0],
        ta[0].as_secs_f64() / tb[0].as_secs_f64()
    );
    ratios[m]
}

/// The system calls s4 with `args` makes, as `strace -f -e trace=…` writes them, one per line;
/// None without strace, or where it cannot trace.
#[cfg(target_os = "linux")]
fn traced(fx: &Fixture, args: &[&str], calls: &str, code: i32) -> Option<String> {
    let out = fx.path().join("trace.txt");
    let _ = std::fs::remove_file(&out);
    let mut c = command(Path::new("strace"), fx, &["-f", "-e"]);
    c.arg(format!("trace={calls}")).arg("-o").arg(&out);
    c.arg(sfor_bin()).args(args).env("LC_ALL", "C");
    let status = c.status().ok()?;
    let trace = std::fs::read_to_string(&out).ok()?;
    assert_eq!(status.code(), Some(code), "strace s4 {args:?}");
    Some(trace)
}

/// Code review r7, Codex #3 and Fable #3: a search that finds no document costs what it costs
/// with --no-docs. One explicit Plain file, with and without the Trail: the Format Definitions
/// are loaded only once a file may be a document, or for a Trail, and the names are resolved
/// only then (the `readlink` and `getcwd` of --no-docs); s4 by default takes at most 1.05 × the CPU time of
/// --no-docs, the medians of 31 alternated runs. A file without an extension is opened once:
/// its first bytes, read to tell a document, are what is searched, in memory too.
#[cfg(target_os = "linux")]
#[test]
#[ignore]
fn one_file_that_is_no_document_costs_what_it_costs_without_documents() {
    let _alone = alone();
    let fx = Fixture::new();
    std::fs::create_dir_all(fx.file("one")).unwrap();
    let text = "a line of a small plain file\n".repeat(20);
    std::fs::write(fx.file("one/small.txt"), &text).unwrap();
    std::fs::write(fx.file("one/small"), &text).unwrap();
    // Lines of the calls, without the exit line.
    let calls = |args: &[&str]| {
        traced(&fx, args, "readlink,getcwd", 1)
            .map(|t| t.lines().filter(|l| !l.contains("+++")).count())
    };
    let txt = ["--no-trail", "zzqq-absent", "one/small.txt"];
    match (calls(&txt), calls(&[&["--no-docs"][..], &txt].concat())) {
        (Some(d), Some(o)) => assert_eq!(d, o, "readlink and getcwd by default and with --no-docs"),
        _ => eprintln!("strace cannot trace here: system calls not looked at"),
    }
    for trail in [&["--no-trail"][..], &[]] {
        for (file, code) in [("one/small", 0), ("one/small", 1)] {
            let pattern = if code == 0 { "plain" } else { "zzqq-absent" };
            let mut a = trail.to_vec();
            a.extend([pattern, file]);
            if let Some(trace) = traced(&fx, &a, "open,openat", code) {
                let opened = trace
                    .lines()
                    .filter(|l| l.contains("\"one/small\""))
                    .count();
                assert_eq!(opened, 1, "{a:?}: {trace}");
            }
        }
        let with = |docs: &[&'static str]| {
            let mut a = trail.to_vec();
            a.extend_from_slice(docs);
            a.extend(["zzqq-absent", "one/small.txt"]);
            a
        };
        let (default, off) = (with(&[]), with(&["--no-docs"]));
        let (mut d, mut o) = (Vec::new(), Vec::new());
        for i in 0..31 {
            if i % 2 == 0 {
                d.push(cpu_time(&fx, &default, 1));
                o.push(cpu_time(&fx, &off, 1));
            } else {
                o.push(cpu_time(&fx, &off, 1));
                d.push(cpu_time(&fx, &default, 1));
            }
        }
        d.sort();
        o.sort();
        let (d, o) = (d[15], o[15]);
        eprintln!("{trail:?}: CPU {d:?} by default, {o:?} with --no-docs");
        if timing_enabled() {
            assert!(
                d.as_secs_f64() <= 1.05 * o.as_secs_f64(),
                "{d:?} > 1.05 × {o:?}"
            );
        }
    }
}

/// Fable M4 code r8 #3: an installed Grammar claims no name as a document's (its Definition's
/// processor is `treesitter`, `grammars::install` refuses any other), so with one installed a
/// search of a file that is no document still loads no Format Definition: s4 opens no file of
/// the Grammar. Once a file may be a document, the Definitions load with the Grammar's, as
/// they load for the Trails.
#[cfg(target_os = "linux")]
#[test]
fn an_installed_grammar_leaves_a_search_without_documents_unloaded() {
    let fx = Fixture::new();
    std::fs::create_dir_all(fx.file("one")).unwrap();
    std::fs::write(fx.file("one/small.txt"), "a line of a small plain file\n").unwrap();
    let lua = fx.grammars.path().join("lua");
    std::fs::create_dir(&lua).unwrap();
    std::fs::write(lua.join("searchfor-grammar-lua"), b"bin").unwrap();
    std::fs::write(
        lua.join("lua.toml"),
        "name = \"lua\"\nextensions = [\"lua\"]\nprocessor = \"treesitter\"\n\n[params]\n\
         grammar = \"lua\"\nquery = \"(function_declaration) @segment\"\n",
    )
    .unwrap();
    std::fs::write(lua.join("installed.json"), br#"{"version":"1"}"#).unwrap();
    let grammar = |trace: &str| {
        trace
            .lines()
            .filter(|l| l.contains("installed.json") || l.contains("lua.toml"))
            .count()
    };
    for mode in [&["--no-trail"][..], &["-q"], &["-l"], &["-c"]] {
        let mut a = mode.to_vec();
        a.extend(["zzqq-absent", "one/small.txt"]);
        let Some(trace) = traced(&fx, &a, "open,openat", 1) else {
            eprintln!("strace cannot trace here: system calls not looked at");
            return;
        };
        assert_eq!(grammar(&trace), 0, "{a:?}: {trace}");
    }
    let a = ["-c", "needle", "docs/shop.sqlite"];
    let trace = traced(&fx, &a, "open,openat", 0).unwrap();
    assert!(grammar(&trace) > 0, "{a:?}: {trace}");
    let o = fx.s4(a).output().unwrap();
    assert_eq!(common::stdout(&o), "9\n", "{}", common::stderr(&o));
}

/// M5a plan, D7 and Task 10.1: PCRE2 and multi-line searches cost what rg's cost, with
/// `--no-trail` over a 10 MB file (PCRE2's scratch space per thread: `matcher::Pcre2`).
#[test]
#[cfg_attr(target_os = "linux", ignore)]
#[cfg_attr(
    not(target_os = "linux"),
    ignore = "requires GNU time -v RSS measurement"
)]
fn pcre2_and_multiline_without_trails_are_as_fast_as_ripgrep() {
    let _alone = alone();
    let Some(rg) = rg_bin() else {
        return;
    };
    let fx = Fixture::new();
    let line = "line 1234 of text here\n";
    let file = repeated(&fx, "ten.txt", line, 10 * 1024 * 1024 / line.len());
    for flags in [&["-P", r"of(?= text)"][..], &["-U", "here\\nline 1"]] {
        let mut args = vec!["-c"];
        args.extend(flags);
        args.push(&file);
        let Some((base, _)) = best_of_three_bin(&rg, &fx, &args, 0) else {
            eprintln!("skipping: /usr/bin/time not found");
            return;
        };
        args.splice(0..0, ["--no-docs", "--no-trail"]);
        let (t, _) = best_of_three(&fx, &args, 0).unwrap();
        eprintln!("10 MB {flags:?}: s4 {t:?}, rg {base:?}");
        if timing_enabled() {
            assert!(
                t.as_secs_f64() <= base.as_secs_f64() * 1.25,
                "{t:?} is more than 1.25 × the {base:?} of rg"
            );
        }
    }
}

/// `--files` lists 20 000 files from the walker's threads, as rg does: within 1.25 × rg's
/// time (the sequential walk of the probe's first version took 6 ×).
#[test]
#[cfg_attr(target_os = "linux", ignore)]
#[cfg_attr(
    not(target_os = "linux"),
    ignore = "requires GNU time -v RSS measurement"
)]
fn files_lists_as_fast_as_ripgrep() {
    let _alone = alone();
    let Some(rg) = rg_bin() else {
        return;
    };
    let fx = Fixture::new();
    for a in 0..20 {
        let d = fx.file(&format!("many/d{a}"));
        std::fs::create_dir_all(&d).unwrap();
        for b in 0..1000 {
            std::fs::write(d.join(format!("f{b}.txt")), "x").unwrap();
        }
    }
    let Some((base, _)) = best_of_three_drained(&rg, &fx, &["--files", "many"], 0) else {
        eprintln!("skipping: /usr/bin/time not found");
        return;
    };
    let (t, _) = best_of_three_drained(&sfor_bin(), &fx, &["--files", "many"], 0).unwrap();
    eprintln!("--files over 20 000 files: s4 {t:?}, rg {base:?}");
    if timing_enabled() {
        assert!(t.as_secs_f64() <= base.as_secs_f64() * 1.25 + 0.01);
    }
}

/// M5a plan, D4: the rows of rg's output flags in the Trail layouts cost about what the
/// default rows cost: a 50 MB Markdown file with one Match in 1 000 lines, each of `--column
/// -b`, `-o`, `--vimgrep`, `-r` and `--trim` within 1.25 × the default's time and its RSS
/// plus 16 MiB (`-o`, `--vimgrep` and `-r` take the model's path, `output::rg::streams`).
#[test]
#[cfg_attr(target_os = "linux", ignore)]
#[cfg_attr(
    not(target_os = "linux"),
    ignore = "requires GNU time -v RSS measurement"
)]
fn output_flags_in_the_trail_layouts_stay_under_the_ceilings() {
    let _alone = alone();
    let fx = Fixture::new();
    let mut text = String::with_capacity(SIZE + 64);
    let mut n = 0;
    while text.len() < SIZE {
        if n % 1000 == 0 {
            text.push_str(&format!("# Section {n}\n\nthe needle of {n}\n"));
        }
        text.push_str("line of text here\n");
        n += 1;
    }
    std::fs::write(fx.file("big.md"), text).unwrap();
    let Some((base, base_rss)) = best_of_three(&fx, &["--flat", "needle", "big.md"], 0) else {
        eprintln!("skipping: /usr/bin/time not found");
        return;
    };
    for flags in [
        &["--column", "-b"][..],
        &["-o"],
        &["--vimgrep"],
        &["-r", "N"],
        &["--trim"],
    ] {
        let mut args = vec!["--flat"];
        args.extend(flags);
        args.extend(["needle", "big.md"]);
        let (t, rss) = best_of_three(&fx, &args, 0).unwrap();
        eprintln!("{flags:?}: {t:?} {rss} B, default {base:?} {base_rss} B");
        if timing_enabled() {
            assert!(
                t.as_secs_f64() <= base.as_secs_f64() * 1.25,
                "{flags:?}: {t:?}"
            );
        }
        assert!(rss <= base_rss + (16 << 20), "{flags:?}: {rss} B");
    }
}

/// M5a plan, review r1, Codex #5: every benchmark runs isolated, hyperfine's runs too: its own
/// Definitions, Grammars and index, and neither configuration file, so none reads `~/.config`
/// or runs a `--pre` of the user's over the corpus.
#[test]
fn benchmarks_run_isolated() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts");
    for name in [
        "bench.sh",
        "bench-code.sh",
        "bench-m3.sh",
        "bench-m4.sh",
        "bench-m5.sh",
    ] {
        let script = std::fs::read_to_string(dir.join(name)).unwrap();
        let first_run = script.find("hyperfine --ignore-failure").unwrap();
        for wanted in [
            "unset RIPGREP_CONFIG_PATH SEARCHFOR_CONFIG_PATH\n",
            r#"export SEARCHFOR_FORMATS_DIR="$iso/formats" SEARCHFOR_GRAMMARS_DIR="$iso/grammars" SEARCHFOR_GRAMMAR_INDEX="file://$iso/none.json""#,
        ] {
            let at = script.find(wanted);
            assert!(
                at.is_some_and(|at| at < first_run),
                "{name}: no `{wanted}` before its first run"
            );
        }
    }
}

/// Code review r3 of M5a, Codex #1 and #3 (`m5_22` in `tests/cli_m5.rs`, extended here): the
/// Trail layouts of `--null-data` records cost time that grows with their Matches, not with
/// their square. Each case runs over N and over 4 N, and the second must cost less than six
/// times the first (CPU time of s4, best of three, `cpu_time`). `records`: N records `needle\n`, each Match
/// ending at its NUL, so the search finds each Match's record at once and only the rows could
/// cost more; `-r` replaces the NULs away, so looking back for the record's start from each
/// Match read the item from its start (Codex #3). `sections`: one record of N Markdown
/// Sections, each holding a Match, where each Section printed the record's rows of every
/// other Section (Codex #1, and Fable #1 with `-o` and `--vimgrep`). Not ignored: it measures
/// CPU time, which other tests running beside it do not change, and runs a few seconds.
#[cfg(unix)]
#[test]
fn null_data_records_and_their_sections_cost_linear_time() {
    let _alone = alone();
    let fx = Fixture::new();
    for n in [1_000, 4_000, 10_000, 40_000] {
        std::fs::write(fx.file(&format!("records{n}.txt")), b"needle\n\0".repeat(n)).unwrap();
        let sections: String = (0..n).map(|i| format!("# H{i}\nneedle\n")).collect();
        std::fs::write(fx.file(&format!("sections{n}.md")), format!("{sections}\0")).unwrap();
    }
    let record = ["-U", "--null-data", "needle\\n\\x00", "records@.txt"];
    let cases: Vec<(usize, Vec<&str>)> = [
        (&["--vimgrep"][..], &record[..]),
        (&["--vimgrep", "-b"], &record),
        (&["--vimgrep", "-P"], &record),
        (&["--vimgrep", "-r", "X\n"], &record),
        (&["--vimgrep", "-b", "-P", "-r", "X\n"], &record),
        (&["--vimgrep", "-P", "--no-unicode", "-r", "X\n"], &record),
    ]
    .iter()
    .map(|(f, a)| (10_000, [*f, *a].concat()))
    .chain(
        [
            &["--tree"][..],
            &["--flat"],
            &["--tree", "-o"],
            &["--flat", "--vimgrep"],
            &["--tree", "-r", "X"],
        ]
        .iter()
        .map(|f| {
            let a: &[&str] = &["--expand", "--null-data", "needle", "sections@.md"];
            (1_000, [*f, a].concat())
        }),
    )
    .collect();
    let mut slow = Vec::new();
    for (n, case) in &cases {
        let time = |n: usize| -> Duration {
            let args: Vec<String> = case
                .iter()
                .map(|a| a.replace('@', &n.to_string()))
                .collect();
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            (0..3).map(|_| cpu_time(&fx, &args, 0)).min().unwrap()
        };
        let (small, large) = (time(*n), time(4 * n));
        eprintln!("{case:?}: {small:?} and {large:?}");
        if large >= small * 6 {
            slow.push(format!("{case:?}: {small:?} and {large:?}"));
        }
    }
    if timing_enabled() {
        assert!(
            slow.is_empty(),
            "4 N Matches cost six times N or more:\n{}",
            slow.join("\n")
        );
    }
}

#[cfg(target_os = "linux")]
fn dense_m5b_bytes(format: &str) -> Vec<u8> {
    dense_m5b_bytes_of(format, SIZE)
}

/// `dense_m5b_bytes` of `size` bytes.
#[cfg(target_os = "linux")]
fn dense_m5b_bytes_of(format: &str, size: usize) -> Vec<u8> {
    // Equal-length rows make exact 50 MiB without a syntactically broken tail.
    let row = match format {
        "md" => "# needle heading\nneedle body\n".to_owned(),
        "jsonl" => "{\"needle\":\"needle\"}\n".to_owned(),
        "csv" => "needle,needle\n".to_owned(),
        "yaml" => "needle: needle\n".to_owned(),
        "rs" => "fn needle() { let needle = 1; }\n".to_owned(),
        "html" => "<p>needle</p>\n".to_owned(),
        "srt" => "1\n00:00:01,000 --> 00:00:02,000\nneedle\n\n".to_owned(),
        _ => unreachable!(),
    };
    let mut out = Vec::with_capacity(size);
    // A unique match on the same bytes measures Source + preparation.
    let marker = match format {
        "jsonl" => "{\"ONCE_M5B\":\"needle\"}\n",
        "csv" => "ONCE_M5B,needle\n",
        "yaml" => "ONCE_M5B: needle\n",
        "rs" => "// ONCE_M5B needle\n",
        "html" => "<!-- ONCE_M5B needle -->\n",
        "srt" => "1\n00:00:00,000 --> 00:00:00,500\nONCE_M5B needle\n\n",
        _ => "# ONCE_M5B needle\n",
    };
    out.extend_from_slice(marker.as_bytes());
    while out.len() + row.len() <= size {
        out.extend_from_slice(row.as_bytes());
    }
    // Tail padding is outside syntax where possible; exact bytes are identical
    // for dense/one-Match/--no-trail runs, including tolerated malformed tails.
    out.resize(size, b' ');
    out
}

#[test]
#[ignore = "50 MiB RSS criterion; selected explicitly in Linux CI"]
#[cfg(target_os = "linux")]
fn m5b_dense_seven_families_have_bounded_match_retention() {
    let _alone = alone();
    let fx = Fixture::new();
    let mut rust: toml::Table = toml::from_str(include_str!("../formats/rust.toml")).unwrap();
    rust["params"]
        .as_table_mut()
        .unwrap()
        .insert("max_bytes".into(), toml::Value::Integer((SIZE + 1) as i64));
    std::fs::write(
        fx.formats.path().join("rust.toml"),
        toml::to_string(&rust).unwrap(),
    )
    .unwrap();
    for format in ["md", "jsonl", "csv", "yaml", "rs", "html", "srt"] {
        let name = format!("dense.{format}");
        let bytes = dense_m5b_bytes(format);
        assert_eq!(bytes.len(), SIZE);
        std::fs::write(fx.file(&name), bytes).unwrap();
        let sample = common::run(&mut fx.s4(["--no-docs", "--json", "-m3", "needle", &name]));
        assert!(sample.status.success());
        let messages: Vec<serde_json::Value> = sample
            .stdout
            .split(|&b| b == b'\n')
            .filter(|l| !l.is_empty())
            .map(|l| serde_json::from_slice(l).unwrap())
            .collect();
        let structured = messages.iter().filter(|m| m["type"] == "match").any(|m| {
            let data = &m["data"];
            assert!(data["format"].is_string(), "Plain fallback for {format}");
            data["trail"].as_array().is_some_and(|a| !a.is_empty())
                || data["submatches"].as_array().is_some_and(|a| {
                    a.iter()
                        .any(|sub| sub["trail"].as_array().is_some_and(|t| !t.is_empty()))
                })
        });
        assert!(structured, "no structured Match in {format}");
        let raw_sample =
            common::run(&mut fx.s4(["--no-docs", "--json", "--no-trail", "-m1", "needle", &name]));
        for line in raw_sample
            .stdout
            .split(|&b| b == b'\n')
            .filter(|l| !l.is_empty())
        {
            let v: serde_json::Value = serde_json::from_slice(line).unwrap();
            if v["type"] == "match" {
                assert!(v["data"].get("trail").is_none());
            }
        }
        for layout in ["--flat", "--json"] {
            let args = ["--no-docs", layout, "-j1", "needle|^", name.as_str()];
            let once = ["--no-docs", layout, "-j1", "ONCE_M5B", name.as_str()];
            let raw = [
                "--no-docs",
                layout,
                "--no-trail",
                "-j1",
                "needle|^",
                name.as_str(),
            ];
            let (td, rd) = measure_drained(&sfor_bin(), &fx, &args, 0).unwrap();
            let (tp, rp) = measure_drained(&sfor_bin(), &fx, &once, 0).unwrap();
            let (t0, r0) = measure_drained(&sfor_bin(), &fx, &raw, 0).unwrap();
            assert!(
                rd <= rp + (32 << 20),
                "{format}/{layout}: dense {rd}, once {rp}"
            );
            if timing_enabled() {
                let factor = if layout == "--json" { 6.0 } else { 4.0 };
                assert!(
                    td.as_secs_f64() <= tp.as_secs_f64() + factor * t0.as_secs_f64() + 0.1,
                    "{format}/{layout}: dense {td:?}, prep {tp:?}, base {t0:?}"
                );
            }
            eprintln!("m5b {format} {layout} dense={td:?}/{rd} once={tp:?}/{rp} raw={t0:?}/{r0}");
        }
    }
}

/// M5b spec §6.3 with several search threads: each may hold up to the 8 MiB of a file's
/// output that sends it to a spool file (§6.2), so the allowance is 32 MiB plus that per thread
/// (code review r3: with four threads the run was 32.9 MiB over one Match, with two 15.5 and
/// with one 0.1).
#[test]
#[ignore = "multi-worker dense RSS; selected explicitly in Linux CI"]
#[cfg(target_os = "linux")]
fn m5b_dense_parallel_output_is_bounded() {
    use searchfor::output::spool::FILE_SPOOL_THRESHOLD;
    let _alone = alone();
    let fx = Fixture::new();
    let files = fx.file("parallel");
    std::fs::create_dir(&files).unwrap();
    for n in 0..4 {
        std::fs::write(files.join(format!("{n}.csv")), dense_m5b_bytes("csv")).unwrap();
    }
    let threads = 4;
    let j = format!("-j{threads}");
    let dense = ["--no-docs", "--json", &j, "needle|^", "parallel"];
    let once = ["--no-docs", "--json", &j, "ONCE_M5B", "parallel"];
    let (_, rd) = measure_drained(&sfor_bin(), &fx, &dense, 0).unwrap();
    let (_, rp) = measure_drained(&sfor_bin(), &fx, &once, 0).unwrap();
    let allowance = (32 << 20) + threads * FILE_SPOOL_THRESHOLD as u64;
    assert!(
        rd <= rp + allowance,
        "parallel dense={rd} once={rp} allowance={allowance}"
    );
}

/// Plan Task 10.3, Step 3: four workers over the seven dense families at once. Each file's
/// output, past the 8 MiB that sends it to a spool file, comes out whole, never interleaved
/// with another file's, and the spool files are gone when s4 ends.
#[test]
#[ignore = "several-worker dense directory; selected explicitly in Linux CI"]
#[cfg(target_os = "linux")]
fn m5b_dense_directory_keeps_each_file_whole_and_removes_its_spool() {
    use std::io::BufRead;
    let _alone = alone();
    let fx = Fixture::new();
    let dir = fx.file("seven");
    let spool = fx.file("spool");
    std::fs::create_dir(&dir).unwrap();
    std::fs::create_dir(&spool).unwrap();
    let families = ["md", "jsonl", "csv", "yaml", "rs", "html", "srt"];
    for format in families {
        let bytes = dense_m5b_bytes_of(format, SIZE / 8);
        std::fs::write(dir.join(format!("dense.{format}")), bytes).unwrap();
    }
    let mut child = fx
        .s4(["--no-docs", "--flat", "-j4", "needle|^", "seven"])
        .env("TMPDIR", &spool)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    // (file, bytes of its block), in the order the blocks come; read as they come and kept
    // only as counts.
    let mut blocks: Vec<(String, u64)> = Vec::new();
    let mut out = std::io::BufReader::new(child.stdout.take().unwrap());
    let mut line = Vec::new();
    while out.read_until(b'\n', &mut line).unwrap() > 0 {
        let path = line.split(|&b| b == b':').next().unwrap();
        match blocks.last_mut() {
            Some((p, n)) if p.as_bytes() == path => *n += line.len() as u64,
            _ => blocks.push((
                String::from_utf8_lossy(path).into_owned(),
                line.len() as u64,
            )),
        }
        line.clear();
    }
    assert!(child.wait().unwrap().success());
    let mut files: Vec<&str> = blocks.iter().map(|(p, _)| p.as_str()).collect();
    files.sort();
    let mut expected: Vec<String> = families
        .iter()
        .map(|f| format!("seven/dense.{f}"))
        .collect();
    expected.sort();
    assert_eq!(files, expected, "one block per file: {blocks:?}");
    assert!(
        blocks.iter().all(|(_, n)| *n > 8 << 20),
        "every output spooled: {blocks:?}"
    );
    let left: Vec<_> = std::fs::read_dir(&spool).unwrap().collect();
    assert!(left.is_empty(), "spool files left: {left:?}");
}

/// Code review r1, Codex #2: with several workers, a file's output past the 8 MiB that sends
/// it to a spool file is copied to stdout in bounded pieces also when one line of it, a JSON
/// message, is far longer than that: the copy holds no second or third copy of the line, and
/// takes time linear in its length (the spool scanned everything held for a line break at each
/// 64 KiB read). The other file makes the run parallel.
#[test]
#[ignore = "64 MiB line spooled by a worker; selected explicitly in Linux CI"]
#[cfg(target_os = "linux")]
fn m5b_a_spooled_line_longer_than_the_threshold_is_copied_in_bounded_pieces() {
    let _alone = alone();
    let fx = Fixture::new();
    const LINE: usize = 64 << 20;
    let mut long = Vec::with_capacity(LINE + 1);
    long.extend_from_slice(b"needle ");
    long.resize(LINE, b'x');
    long.push(b'\n');
    std::fs::write(fx.file("long.txt"), &long).unwrap();
    drop(long);
    std::fs::write(fx.file("short.txt"), "needle\n").unwrap();
    let args = |extra: [&'static str; 2]| {
        let mut a = vec!["--no-docs", "--json"];
        a.extend(extra);
        a.extend(["long.txt", "short.txt"]);
        a
    };
    let Some((tp, rp)) = measure_drained(&sfor_bin(), &fx, &args(["-j2", "needle"]), 0) else {
        return;
    };
    // The same output written as it grows, by one thread: no spool.
    let (ts, rs) = measure_drained(&sfor_bin(), &fx, &args(["-j1", "needle"]), 0).unwrap();
    // The same bytes read, and nothing printed.
    let (_, rn) = measure_drained(&sfor_bin(), &fx, &args(["-j2", "ABSENT_M5B"]), 1).unwrap();
    eprintln!(
        "64 MiB line: -j2 {tp:?}/{rp}, -j1 {ts:?}/{rs}, nothing printed {rn}; uptime {}",
        uptime()
    );
    // The message is built once (`JsonFile`'s scratch, freed before the copy) and written to the
    // spool file; the copy adds 64 KiB.
    let message = LINE as u64 + 1024;
    assert!(
        rp <= rn + message + (16 << 20),
        "-j2 peak RSS {rp} is more than {rn} + one message + 16 MiB"
    );
    assert!(
        tp <= ts * 2 + Duration::from_millis(500),
        "-j2 took {tp:?}, more than 2 × -j1's {ts:?} + 0.5 s"
    );
}

/// Code review r1, Codex #1 (M5b spec §6.1): --within, --expand and --stats stream a dense
/// file as the one-line layout does, keeping no model of its Matches: what a dense file costs
/// over the same bytes with one Match is the same bounded amount at 2 MiB and at 8 MiB.
#[test]
#[ignore = "dense RSS of the structural modes; selected explicitly in Linux CI"]
#[cfg(target_os = "linux")]
fn m5b_structural_modes_keep_no_model_of_a_dense_file() {
    let _alone = alone();
    let fx = Fixture::new();
    for mib in [2usize, 8] {
        let name = format!("dense{mib}.csv");
        let mut bytes = b"ONCE_M5B,needle\n".to_vec();
        while bytes.len() + 14 <= mib << 20 {
            bytes.extend_from_slice(b"needle,needle\n");
        }
        std::fs::write(fx.file(&name), &bytes).unwrap();
        drop(bytes);
        // --within keeps the first column; its one Match, in the header, has no Trail.
        let modes: [(&[&str], i32); 4] = [
            (&["--within", "ONCE_M5B"], 1),
            (&["--expand"], 0),
            (&["--stats"], 0),
            (&["--expand", "--within", "row"], 1),
        ];
        for (mode, once_code) in modes {
            let run = |pattern: &'static str, code: i32| {
                let mut args = vec!["--no-docs", "--flat", "-j1"];
                args.extend_from_slice(mode);
                args.extend([pattern, name.as_str()]);
                measure_drained(&sfor_bin(), &fx, &args, code)
            };
            let Some((td, rd)) = run("needle|^", 0) else {
                return;
            };
            let (tp, rp) = run("ONCE_M5B", once_code).unwrap();
            eprintln!(
                "{mib} MiB {mode:?}: dense {td:?}/{rd}, one Match {tp:?}/{rp}; uptime {}",
                uptime()
            );
            assert!(
                rd <= rp + (16 << 20),
                "{mib} MiB {mode:?}: dense peak RSS {rd} is more than {rp} + 16 MiB"
            );
        }
    }
}

/// Code review r2, Fable #2 (M5b spec §6.1): --expand keeps the Matches of the Sections that
/// may still print, and lets go of the others. A Markdown `# Top` whose `##` Sections cover the
/// rest of the file is open to the end and is never a Section, so the run held every Match: 97
/// MB of peak RSS at 20 000 Sections, as 0.5.0's model, 20 MB with `--no-trail`. What --expand
/// costs over the same search without it, whose Outline grows with the Sections, is the same
/// at 5 000 and at 20 000 Sections.
#[test]
#[ignore = "--expand RSS over a growing Markdown file; selected explicitly in Linux CI"]
#[cfg(target_os = "linux")]
fn m5b_expand_lets_go_of_the_matches_of_printed_sections() {
    let _alone = alone();
    let fx = Fixture::new();
    let mut over = Vec::new();
    for n in [5_000usize, 20_000] {
        let name = format!("nested{n}.md");
        let mut text = String::from("# Top\n");
        for i in 0..n {
            text.push_str(&format!("## S{i}\nneedle a\nneedle b\nneedle c\n"));
        }
        std::fs::write(fx.file(&name), text).unwrap();
        for layout in ["--flat", "--tree"] {
            let run = |expand: &[&'static str]| {
                let mut args = vec!["--no-docs", layout, "-j1"];
                args.extend(expand);
                args.extend(["needle", name.as_str()]);
                measure_drained(&sfor_bin(), &fx, &args, 0)
            };
            let Some((te, re)) = run(&["--expand"]) else {
                return;
            };
            let (tr, rr) = run(&[]).unwrap();
            eprintln!(
                "{n} Sections {layout}: --expand {te:?}/{re}, rows {tr:?}/{rr}; uptime {}",
                uptime()
            );
            over.push((n, layout, re.saturating_sub(rr)));
        }
    }
    for layout in ["--flat", "--tree"] {
        let over = |n: usize| {
            over.iter()
                .find(|p| p.0 == n && p.1 == layout)
                .map(|p| p.2)
                .unwrap()
        };
        let (small, large) = (over(5_000), over(20_000));
        assert!(
            large <= small + (4 << 20),
            "{layout}: --expand costs {large} bytes more at 20 000 Sections, {small} at 5 000"
        );
    }
}

/// The load averages the measurement ran under, for its report (`/proc/loadavg`).
#[cfg(target_os = "linux")]
fn uptime() -> String {
    std::fs::read_to_string("/proc/loadavg")
        .unwrap_or_default()
        .trim()
        .to_owned()
}
