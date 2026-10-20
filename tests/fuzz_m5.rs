//! M5a (M5a plan, Chunk 10): combinations of ripgrep's output and search flags with s4's own,
//! run over the fixtures and over files with CRLF lines, NUL records, Latin-1 and long lines.
//! Nothing may panic, every run ends within 10 s, and the exit code is 0, 1, or 2 with a
//! message. The generator is seeded: `SEARCHFOR_FUZZ_SEED` and `SEARCHFOR_FUZZ_N` (runs)
//! change the seed and the count; `SEARCHFOR_FUZZ_SECONDS` repeats rounds of runs, round r with
//! the seed plus r, until that many seconds have passed (M5b plan, Task 4.2). A failure prints
//! its command line and how to replay it, and is saved under `SEARCHFOR_FUZZ_ARTIFACT_DIR`.
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

/// Runs of a counted round, and of each round of a timed one (`SEARCHFOR_FUZZ_SECONDS`).
const COUNTED_N: usize = 150;
const TIMED_N: usize = 30;

#[test]
fn m5_flag_combinations_never_panic() {
    const TEST: &str = "m5_flag_combinations_never_panic";
    let fx = Fixture::new();
    write_files(&fx);
    let paths: &[&[&str]] = &[
        &["md"],
        &["m5"],
        &["md/guide.md"],
        &["m5/records.dat", "m5/crlf.md"],
        &[],
    ];
    let base = common::fuzz_seed(0x5eed_f00d_cafe_0005);
    let budget = common::fuzz_budget(common::fuzz_n().unwrap_or(COUNTED_N));
    let n = common::fuzz_n().unwrap_or(if budget.timed() { TIMED_N } else { COUNTED_N });
    let in_flight = common::FuzzInFlight::new(TEST);
    let mut failures = Vec::new();
    let mut tried = 0;
    // Round r is the counted run of seed `base + r`: SEARCHFOR_FUZZ_SEED and SEARCHFOR_FUZZ_N
    // replay any of its cases.
    'rounds: for round in 0u64.. {
        if !budget.permits(tried) {
            break;
        }
        let seed = base.wrapping_add(round);
        let mut rng = Rng(seed);
        for case in 0..n {
            if !budget.permits(tried) {
                break 'rounds;
            }
            let mut args: Vec<&str> = Vec::new();
            for _ in 0..1 + rng.below(5) {
                args.extend_from_slice(FLAGS[rng.below(FLAGS.len())]);
            }
            args.push("-e");
            args.push(PATTERNS[rng.below(PATTERNS.len())]);
            args.push("--");
            args.extend_from_slice(paths[rng.below(paths.len())]);
            in_flight.at(|| {
                format!(
                    "SEARCHFOR_FUZZ_SEED={seed} SEARCHFOR_FUZZ_N={n} case {case}: s4 {args:?}\n"
                )
            });
            tried += 1;
            let started = Instant::now();
            // A run that does not end within the limit is killed and reaped: a hang fails the
            // case, not the test run.
            let (status, err) = common::run_limited(&mut fx.s4(&args), LIMIT);
            let took = started.elapsed();
            let text = String::from_utf8_lossy(&err);
            let code = status.and_then(|s| s.code()).unwrap_or(-1);
            let bad = text.contains("panicked")
                || status.is_none()
                || !matches!(code, 0..=2)
                || (code == 2 && err.is_empty());
            if bad {
                let what = format!("s4 {} (exit {code}, {took:?})", args.join(" "));
                let argv: Vec<&std::ffi::OsStr> = args.iter().map(std::ffi::OsStr::new).collect();
                let saved = common::save_fuzz_case(&common::FuzzCase {
                    test: TEST,
                    seed,
                    n,
                    case,
                    argv: &argv,
                    files: &[],
                    stderr: &err,
                    what: &what,
                });
                failures.push(format!(
                    "{what}, replay with SEARCHFOR_FUZZ_SEED={seed} SEARCHFOR_FUZZ_N={n} (case {case}){}:\n{text}",
                    saved.map(|d| format!(", saved in {}", d.display())).unwrap_or_default()
                ));
            }
        }
    }
    in_flight.finish();
    eprintln!("fuzz: {tried} runs of s4, {} failures", failures.len());
    assert!(tried > 0, "no run of s4 within the budget");
    assert!(
        failures.is_empty(),
        "{} run(s) failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
