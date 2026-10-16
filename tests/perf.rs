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
    let mut v = Vec::with_capacity(SIZE + line.len());
    while v.len() < SIZE {
        v.extend_from_slice(line);
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

/// Wall time and peak RSS in bytes of `sfor args`, stdout discarded, measured by
/// `/usr/bin/time -v`. None when /usr/bin/time is missing.
fn measure(fx: &Fixture, args: &[&str]) -> Option<(Duration, u64)> {
    let time = Path::new("/usr/bin/time");
    if !time.exists() {
        return None;
    }
    let mut c = Command::new(time);
    c.arg("-v")
        .arg(sfor_bin())
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
    let mut best: Option<(Duration, u64)> = None;
    for _ in 0..3 {
        let (t, rss) = measure(fx, args)?;
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
