#!/usr/bin/env bash
# Acceptance criterion 13: sfor ≤ 1.5 × rg on a pattern matching < 5 % of the files, and
# sfor --no-trail ≤ 1.1 × rg (Hilo 8386).
# Needs hyperfine (cargo install hyperfine --locked) and jq.
set -euo pipefail
export SFOR_BENCH_DIR="${1:-$HOME/robotin/knowledge}"
export SFOR_BENCH_PATTERN="${2:-ExecStart}"
cargo build --release --quiet
total=$(rg --files -- "$SFOR_BENCH_DIR" | wc -l)
hits=$(rg -l -e "$SFOR_BENCH_PATTERN" -- "$SFOR_BENCH_DIR" | wc -l || true)
echo "files: $total, matching: $hits"
awk -v h="$hits" -v t="$total" 'BEGIN { if (h * 20 >= t) { print "pattern matches 5% or more of the files"; exit 1 } }'
hyperfine --warmup 3 --runs 10 --export-json /tmp/sfor-bench.json \
  'rg --color never -e "$SFOR_BENCH_PATTERN" -- "$SFOR_BENCH_DIR"' \
  './target/release/sfor --color never --flat -e "$SFOR_BENCH_PATTERN" -- "$SFOR_BENCH_DIR"'
jq -r '.results | "trail mode / rg: \(.[1].median / .[0].median)"' /tmp/sfor-bench.json
jq -e '.results[1].median / .results[0].median <= 1.5' /tmp/sfor-bench.json >/dev/null
# Hilo 8386: the search itself must be as fast as ripgrep; --no-trail runs only ripgrep's engine.
hyperfine --warmup 3 --runs 10 --export-json /tmp/sfor-bench-nt.json \
  'rg --color never -e "$SFOR_BENCH_PATTERN" -- "$SFOR_BENCH_DIR"' \
  './target/release/sfor --color never --no-trail -e "$SFOR_BENCH_PATTERN" -- "$SFOR_BENCH_DIR"'
jq -r '.results | "--no-trail / rg: \(.[1].median / .[0].median)"' /tmp/sfor-bench-nt.json
jq -e '.results[1].median / .results[0].median <= 1.1' /tmp/sfor-bench-nt.json >/dev/null
