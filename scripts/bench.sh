#!/usr/bin/env bash
# Acceptance criterion 13: s4 ≤ 1.5 × rg on a pattern matching < 5 % of the files, and
# s4 --no-trail ≤ 1.1 × rg (Hilo 8386), on two corpora: Markdown notes, and a tree with
# binary files, where ripgrep stops reading each file at its first NUL (code review r1,
# Fable #2).
# Usage: scripts/bench.sh [NOTES_DIR [PATTERN [BINARY_DIR [BINARY_PATTERN]]]]
# Needs hyperfine (cargo install hyperfine --locked) and jq. SEARCHFOR_RG, else /usr/bin/rg, is the
# reference and must be ripgrep 14.1.1 (M5b spec §5.2); exit code 1 (no Match) is not a
# failure.
set -euo pipefail
notes_dir="${1:-$HOME/robotin/knowledge}"
notes_pattern="${2:-ExecStart}"
binary_dir="${3:-/usr/lib/x86_64-linux-gnu}"
binary_pattern="${4:-zzqq-searchfor-bench}"
cargo build --release --quiet
rg="${SEARCHFOR_RG:-/usr/bin/rg}"
if ! "$rg" --version 2>/dev/null | head -1 | grep -qx 'ripgrep 14.1.1'; then
  echo "FAIL: the reference $rg is not ripgrep 14.1.1"; exit 1
fi
export SEARCHFOR_RG_PATH="$rg"
# Isolation (M5a plan, review r1, Codex #5): no user Definitions or Grammars, no network, no
# configuration file of rg or of s4, for hyperfine's runs too.
iso=$(mktemp -d)
trap 'rm -rf "$iso"' EXIT
mkdir -p "$iso/formats" "$iso/grammars"
export SEARCHFOR_FORMATS_DIR="$iso/formats" SEARCHFOR_GRAMMARS_DIR="$iso/grammars" SEARCHFOR_GRAMMAR_INDEX="file://$iso/none.json"
unset RIPGREP_CONFIG_PATH SEARCHFOR_CONFIG_PATH

# bench NAME DIR PATTERN: both ratios on one corpus; exits 1 when one is over its limit.
bench() {
  local name="$1"
  export SEARCHFOR_BENCH_DIR="$2" SEARCHFOR_BENCH_PATTERN="$3"
  local total hits
  total=$("$rg" --files -- "$SEARCHFOR_BENCH_DIR" | wc -l)
  hits=$("$rg" -l -e "$SEARCHFOR_BENCH_PATTERN" -- "$SEARCHFOR_BENCH_DIR" | wc -l || true)
  echo "== $name: $SEARCHFOR_BENCH_DIR, files: $total, matching: $hits"
  awk -v h="$hits" -v t="$total" 'BEGIN { if (h * 20 >= t) { print "pattern matches 5% or more of the files"; exit 1 } }'
  hyperfine --ignore-failure --warmup 5 --runs 30 --export-json "/tmp/searchfor-bench-$name.json" \
    '"$SEARCHFOR_RG_PATH" --color never -e "$SEARCHFOR_BENCH_PATTERN" -- "$SEARCHFOR_BENCH_DIR"' \
    './target/release/s4 --color never --flat -e "$SEARCHFOR_BENCH_PATTERN" -- "$SEARCHFOR_BENCH_DIR"'
  jq -r '.results | "trail mode / rg: \(.[1].median / .[0].median)"' "/tmp/searchfor-bench-$name.json"
  jq -e '.results[1].median / .results[0].median <= 1.5' "/tmp/searchfor-bench-$name.json" >/dev/null
  # Hilo 8386: the search itself must be as fast as ripgrep; --no-trail runs only ripgrep's engine.
  hyperfine --ignore-failure --warmup 5 --runs 30 --export-json "/tmp/searchfor-bench-$name-nt.json" \
    '"$SEARCHFOR_RG_PATH" --color never -e "$SEARCHFOR_BENCH_PATTERN" -- "$SEARCHFOR_BENCH_DIR"' \
    './target/release/s4 --color never --no-trail -e "$SEARCHFOR_BENCH_PATTERN" -- "$SEARCHFOR_BENCH_DIR"'
  jq -r '.results | "--no-trail / rg: \(.[1].median / .[0].median)"' "/tmp/searchfor-bench-$name-nt.json"
  jq -e '.results[1].median / .results[0].median <= 1.1' "/tmp/searchfor-bench-$name-nt.json" >/dev/null
}

bench notes "$notes_dir" "$notes_pattern"
bench binaries "$binary_dir" "$binary_pattern"
