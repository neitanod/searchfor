//! Memory and time ceilings of the Trail pipeline (code review r1, Fable #1). Ignored by
//! default because they write a 50 MB file and run the binary several times; run them with
//! `cargo test --release --test perf -- --ignored --nocapture`. `SFOR_BIN` runs them against
//! another build of sfor (to see them fail before a fix).
mod common;

use common::Fixture;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

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
    let time = Path::new("/usr/bin/time");
    if !time.exists() {
        return None;
    }
    let mut c = Command::new(time);
    c.arg("-v")
        .arg(bin)
        .args(args)
        .current_dir(fx.path())
        .env("SFOR_FORMATS_DIR", fx.formats.path())
        .env_remove("RIPGREP_CONFIG_PATH")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
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
    ceiling("big.txt", &["--flat", "-v", "zzz"]);
}

#[test]
#[ignore]
fn every_line_matching_in_a_big_plain_file_stays_under_the_ceilings() {
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
    markdown_ceiling(&["--flat", "-v", "zzz"]);
}

#[test]
#[ignore]
fn tree_output_of_a_big_markdown_file_stays_under_the_ceilings() {
    markdown_ceiling(&["--tree", "text"]);
}

/// Code review r1, Fable #2: a walked binary file without Matches costs what it costs ripgrep,
/// which stops reading it at the NUL in its first buffer (spec §3.1, search first).
#[test]
#[ignore]
fn walked_binary_files_are_not_read_before_the_search() {
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
    ceiling("big.txt", &["--json", "-v", "zzz"]);
}

/// Every line matching in a big Markdown file: the Processor once, plus at most 1.5 × what
/// grep-printer's own JSON (`--no-trail`) takes to write the same messages without Trails
/// (before the fix: 46.7 s against 0.9 s + 3.3 s); RSS as `markdown_ceiling`.
#[test]
#[ignore]
fn json_of_a_big_markdown_file_stays_under_the_ceilings() {
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
    let rg = Path::new("/usr/bin/rg");
    if !common::reference_rg() || !rg.exists() {
        return;
    }
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
