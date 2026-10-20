//! M5a (M5a plan, Chunk 10): combinations of ripgrep's output and search flags with s4's own,
//! run over the fixtures and over files with CRLF lines, NUL records, Latin-1 and long lines.
//! Nothing may panic, every run ends within 10 s, and the exit code is 0, 1, or 2 with a
//! message. The generator is seeded: `SEARCHFOR_FUZZ_SEED` and `SEARCHFOR_FUZZ_N` (runs)
//! change the seed and the count; a failure prints its command line.
mod common;

use common::Fixture;
use std::time::{Duration, Instant};

/// xorshift64*, as tests/fuzz.rs.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

/// The flags a run picks from, each a group of arguments.
#[rustfmt::skip]
const FLAGS: &[&[&str]] = &[
    &["--flat"], &["--tree"], &["--json"], &["--no-trail"], &["--heading"], &["--no-heading"],
    &["-p"], &["--vimgrep"], &["-o"], &["--column"], &["-b"], &["-n"], &["-N"], &["-H"], &["-I"],
    &["-0"], &["--path-separator", "\\"], &["--trim"], &["-r", "<$0>"], &["-r", ""],
    &["--passthru"], &["-C1"], &["-A2"], &["-B1"], &["--context-separator", "##"],
    &["--no-context-separator"], &["--field-match-separator", "="],
    &["--field-context-separator", "~"], &["-M", "12"], &["--max-columns-preview"], &["-m1"],
    &["-c"], &["--count-matches"], &["-l"], &["--files-without-match"], &["--include-zero"],
    &["-v"], &["-i"], &["-w"], &["-x"], &["-F"], &["-U"], &["--multiline-dotall"], &["-P"],
    &["--engine", "auto"], &["--crlf"], &["--null-data"], &["--no-unicode"],
    &["-E", "latin1"], &["-E", "utf-16le"], &["-E", "none"], &["--stop-on-nonmatch"],
    &["--stats"], &["--sort", "path"], &["--sortr", "modified"], &["--color", "always"],
    &["--colors", "match:fg:blue"], &["--colors", "trail:fg:yellow"],
    &["--hyperlink-format", "default"], &["--within", "Install"], &["--expand"],
    &["--in-headings"], &["--mmap"], &["--no-mmap"], &["--line-buffered"], &["-a"],
    &["--binary"], &["--no-docs"],
];

const PATTERNS: &[&str] = &[
    "needle",
    "needle-\\w+",
    "^",
    "$",
    "e",
    "(?i)NEEDLE",
    "\\n",
    "needle\\n",
    "a|b|c",
    "\\w+$",
    "ñ",
    "Install",
];

fn write_files(fx: &Fixture) {
    let dir = fx.file("m5");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("crlf.md"), b"# Install\r\n\r\na needle\r\nb\r\n").unwrap();
    std::fs::write(
        dir.join("records.dat"),
        b"one needle\ntwo\0three\0needle four\n",
    )
    .unwrap();
    std::fs::write(dir.join("latin.md"), b"# Caf\xe9\n\nneedle caf\xe9\n").unwrap();
    std::fs::write(
        dir.join("long.md"),
        format!(
            "# Long\n\n{} needle {} needle\n",
            "x".repeat(300),
            "ñ".repeat(200)
        ),
    )
    .unwrap();
}

/// The longest a run may take.
const LIMIT: Duration = Duration::from_secs(10);

fn count() -> usize {
    std::env::var("SEARCHFOR_FUZZ_N")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(150)
}

fn seed() -> u64 {
    std::env::var("SEARCHFOR_FUZZ_SEED")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(0x5eed_f00d_cafe_0005)
}

#[test]
fn m5_flag_combinations_never_panic() {
    let fx = Fixture::new();
    write_files(&fx);
    let mut rng = Rng(seed());
    let paths: &[&[&str]] = &[
        &["md"],
        &["m5"],
        &["md/guide.md"],
        &["m5/records.dat", "m5/crlf.md"],
        &[],
    ];
    let mut failures = Vec::new();
    for _ in 0..count() {
        let mut args: Vec<&str> = Vec::new();
        for _ in 0..1 + rng.below(5) {
            args.extend_from_slice(FLAGS[rng.below(FLAGS.len())]);
        }
        args.push("-e");
        args.push(PATTERNS[rng.below(PATTERNS.len())]);
        args.push("--");
        args.extend_from_slice(paths[rng.below(paths.len())]);
        let started = Instant::now();
        let mut child = fx
            .s4(&args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let mut stderr_pipe = child.stderr.take().unwrap();
        let reader = std::thread::spawn(move || {
            let mut e = String::new();
            let _ = std::io::Read::read_to_string(&mut stderr_pipe, &mut e);
            e
        });
        // A run that does not end within the limit is killed: a hang fails the case, not the
        // test run.
        let status = loop {
            if let Some(st) = child.try_wait().unwrap() {
                break Some(st);
            }
            if started.elapsed() > LIMIT {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        let took = started.elapsed();
        let err = reader.join().unwrap();
        let code = status.and_then(|s| s.code()).unwrap_or(-1);
        let bad = err.contains("panicked")
            || status.is_none()
            || !matches!(code, 0..=2)
            || (code == 2 && err.is_empty());
        if bad {
            failures.push(format!(
                "s4 {} (exit {code}, {took:?}):\n{err}",
                args.join(" ")
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} run(s) failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
