#!/usr/bin/env bash
# Acceptance criterion 13: sfor ≤ 1.5 × rg on a pattern matching < 5 % of the files, and
# sfor --no-trail ≤ 1.1 × rg (Hilo 8386), on two corpora: Markdown notes, and a tree with
# binary files, where ripgrep stops reading each file at its first NUL (code review r1,
# Fable #2).
# Usage: scripts/bench.sh [NOTES_DIR [PATTERN [BINARY_DIR [BINARY_PATTERN]]]]
# Needs hyperfine (cargo install hyperfine --locked) and jq; exit code 1 (no Match) is not a
# failure.
set -euo pipefail
notes_dir="${1:-$HOME/robotin/knowledge}"
notes_pattern="${2:-ExecStart}"
binary_dir="${3:-/usr/lib/x86_64-linux-gnu}"
binary_pattern="${4:-zzqq-searchfor-bench}"
cargo build --release --quiet

# bench NAME DIR PATTERN: both ratios on one corpus; exits 1 when one is over its limit.
bench() {
  local name="$1"
  export SFOR_BENCH_DIR="$2" SFOR_BENCH_PATTERN="$3"
  local total hits
  total=$(rg --files -- "$SFOR_BENCH_DIR" | wc -l)
  hits=$(rg -l -e "$SFOR_BENCH_PATTERN" -- "$SFOR_BENCH_DIR" | wc -l || true)
  echo "== $name: $SFOR_BENCH_DIR, files: $total, matching: $hits"
  awk -v h="$hits" -v t="$total" 'BEGIN { if (h * 20 >= t) { print "pattern matches 5% or more of the files"; exit 1 } }'
  hyperfine --ignore-failure --warmup 3 --runs 10 --export-json "/tmp/sfor-bench-$name.json" \
    'rg --color never -e "$SFOR_BENCH_PATTERN" -- "$SFOR_BENCH_DIR"' \
    './target/release/sfor --color never --flat -e "$SFOR_BENCH_PATTERN" -- "$SFOR_BENCH_DIR"'
  jq -r '.results | "trail mode / rg: \(.[1].median / .[0].median)"' "/tmp/sfor-bench-$name.json"
  jq -e '.results[1].median / .results[0].median <= 1.5' "/tmp/sfor-bench-$name.json" >/dev/null
  # Hilo 8386: the search itself must be as fast as ripgrep; --no-trail runs only ripgrep's engine.
  hyperfine --ignore-failure --warmup 3 --runs 10 --export-json "/tmp/sfor-bench-$name-nt.json" \
    'rg --color never -e "$SFOR_BENCH_PATTERN" -- "$SFOR_BENCH_DIR"' \
    './target/release/sfor --color never --no-trail -e "$SFOR_BENCH_PATTERN" -- "$SFOR_BENCH_DIR"'
  jq -r '.results | "--no-trail / rg: \(.[1].median / .[0].median)"' "/tmp/sfor-bench-$name-nt.json"
  jq -e '.results[1].median / .results[0].median <= 1.1' "/tmp/sfor-bench-$name-nt.json" >/dev/null
}

bench notes "$notes_dir" "$notes_pattern"
bench binaries "$binary_dir" "$binary_pattern"
