#!/usr/bin/env bash
# M2 acceptance (spec §11 #13, plan D22), on code corpora:
#   - a pattern no file matches, and --no-trail: s4 ≤ 1.1 × rg;
#   - a pattern matching under 5 % of the files: s4 − rg ≤ matching bytes ÷ (2.5 MB/s × threads);
#   - a dense pattern: reported only.
# Usage: scripts/bench-code.sh [DIR_A [SPARSE_A [DIR_B [SPARSE_B]]]]
# Needs hyperfine, jq and GNU stat. SEARCHFOR_BIN replaces the s4 under test (to check that a
# broken one fails the script: SEARCHFOR_BIN=scripts/fake-s4-exit-2.sh must make it exit 1).
set -euo pipefail
# Decimal points in awk's output, whatever the user's locale.
export LC_ALL=C
registry=$(ls -d "$HOME"/.cargo/registry/src/index.crates.io-*/ 2>/dev/null | head -1)
dir_a="${1:-$registry}"; sparse_a="${2:-impl Drop for}"
dir_b="${3:-$HOME/robotin/apps}"; sparse_b="${4:-TODO}"
# Built at run time, so no file holds it, this script included (M3 code review r1): split in
# two, the corpus ~/robotin/apps never matches it.
none='zzqq-searchfor-bench-''absent'
cargo build --release --quiet
s4="${SEARCHFOR_BIN:-./target/release/s4}"
export SEARCHFOR_BIN_PATH="$s4"
# Isolation (review r1, Codex #12): no user Definitions or Grammars, no network, no rg config.
iso=$(mktemp -d)
trap 'rm -rf "$iso"' EXIT
mkdir -p "$iso/formats" "$iso/grammars"
export SEARCHFOR_FORMATS_DIR="$iso/formats" SEARCHFOR_GRAMMARS_DIR="$iso/grammars" SEARCHFOR_GRAMMAR_INDEX="file://$iso/none.json"
unset RIPGREP_CONFIG_PATH
# The code Formats of M2, for the parsing budget (Fable #7): rg's types for them.
code_types=(-t php -t js -t ts -t sh -t go -t rust -t py -t json -t yaml -t toml)
# ripgrep 14.1.1's default: the available parallelism, at most 12 (search.rs, walker_threads).
threads=$(nproc); [ "$threads" -gt 12 ] && threads=12
fail=0

# median NAME CMD_RG CMD_S4 → prints s4/rg and sets RG_S, SEARCHFOR_S (seconds).
median() {
  hyperfine --ignore-failure --warmup 3 --runs 20 --export-json "/tmp/searchfor-bench-code-$1.json" "$2" "$3" >/dev/null
  RG_S=$(jq '.results[0].median' "/tmp/searchfor-bench-code-$1.json")
  SEARCHFOR_S=$(jq '.results[1].median' "/tmp/searchfor-bench-code-$1.json")
  awk -v a="$SEARCHFOR_S" -v b="$RG_S" -v n="$1" 'BEGIN { printf "%-22s rg %.3f s  s4 %.3f s  ratio %.2f\n", n, b, a, a / b }'
}

# check NAME EXPECTED SEARCHFOR_FLAG RG_ARGS…: one run of each program, outputs in $iso (review r4,
# Codex #4, Fable #4). s4 must exit as rg does and write to stderr only what rg writes (an
# unreadable file warns in both, and both may exit 2 — Codex r3 #4); it prints at least as
# many lines as rg (`--flat` prints a line once per Trail; `--no-trail` exactly rg's). The
# pattern must behave as the benchmark assumes, on rg's own output: EXPECTED=found means rg
# printed Matches, EXPECTED=none means it printed none (rg's exit code is not proof: with an
# unreadable file it is 2 either way — Codex r4 #1).
check() {
  local name="$1" expected="$2" flag="$3"; shift 3
  local code=0 rgcode=0
  "$s4" --color never "$flag" "$@" >"$iso/s4.out" 2>"$iso/s4.err" || code=$?
  rg --color never "$@" >"$iso/rg.out" 2>"$iso/rg.err" || rgcode=$?
  local err rgerr
  err=$(sed 's/^s4: //' "$iso/s4.err" | sort)
  rgerr=$(sed 's/^rg: //' "$iso/rg.err" | sort)
  if [ "$code" != "$rgcode" ] || [ "$err" != "$rgerr" ]; then
    echo "  FAIL $name: s4 exited $code, rg $rgcode; or stderr differs from rg's: ${err:0:300}"; fail=1; return
  fi
  if { [ "$expected" = found ] && [ ! -s "$iso/rg.out" ]; } || { [ "$expected" = none ] && [ -s "$iso/rg.out" ]; }; then
    echo "  FAIL $name: rg's output says the pattern does not behave as assumed ($expected)"; fail=1; return
  fi
  local ours rgs
  ours=$(wc -l <"$iso/s4.out")
  rgs=$(wc -l <"$iso/rg.out")
  if [ "$ours" -lt "$rgs" ] || { [ "$flag" = --no-trail ] && [ "$ours" != "$rgs" ]; }; then
    echo "  FAIL $name: $ours lines, rg printed $rgs"; fail=1
  fi
}

corpus() {
  local name="$1" dir="$2" sparse="$3"
  export SEARCHFOR_BENCH_DIR="$dir" SEARCHFOR_BENCH_PATTERN="$sparse" SEARCHFOR_BENCH_NONE="$none"
  local total hits bytes
  total=$(rg --files -- "$dir" | wc -l)
  hits=$( (rg -l -e "$sparse" -- "$dir" || true) | wc -l)
  bytes=$( (rg -l -0 "${code_types[@]}" -e "$sparse" -- "$dir" || true) | xargs -0 -r stat -c %s | awk '{ s += $1 } END { print s + 0 }')
  echo "== $name: $dir, files $total, matching $hits ($bytes bytes of code and data), threads $threads"
  awk -v h="$hits" -v t="$total" 'BEGIN { if (h * 20 >= t) { print "the sparse pattern matches 5% or more of the files"; exit 1 } }'
  if rg -q -e "$none" -- "$dir"; then echo "  FAIL: '$none' matches in $dir"; fail=1; return; fi
  # Before timing: every command does what it is timed for. A failed check skips this corpus's
  # timings only; a failure of an earlier corpus does not skip this one.
  local prior="$fail"
  fail=0
  check none none --flat -e "$none" -- "$dir"
  check sparse found --flat -e "$sparse" -- "$dir"
  check no-trail found --no-trail -e "$sparse" -- "$dir"
  [ "$fail" = 0 ] || return 0
  fail="$prior"
  median "$name-none" 'rg --color never -e "$SEARCHFOR_BENCH_NONE" -- "$SEARCHFOR_BENCH_DIR"' \
    '"$SEARCHFOR_BIN_PATH" --color never --flat -e "$SEARCHFOR_BENCH_NONE" -- "$SEARCHFOR_BENCH_DIR"'
  awk -v a="$SEARCHFOR_S" -v b="$RG_S" 'BEGIN { exit !(a <= 1.1 * b) }' || { echo "  FAIL: over 1.1 × rg"; fail=1; }
  median "$name-no-trail" 'rg --color never -e "$SEARCHFOR_BENCH_PATTERN" -- "$SEARCHFOR_BENCH_DIR"' \
    '"$SEARCHFOR_BIN_PATH" --color never --no-trail -e "$SEARCHFOR_BENCH_PATTERN" -- "$SEARCHFOR_BENCH_DIR"'
  awk -v a="$SEARCHFOR_S" -v b="$RG_S" 'BEGIN { exit !(a <= 1.1 * b) }' || { echo "  FAIL: over 1.1 × rg"; fail=1; }
  median "$name-sparse" 'rg --color never -e "$SEARCHFOR_BENCH_PATTERN" -- "$SEARCHFOR_BENCH_DIR"' \
    '"$SEARCHFOR_BIN_PATH" --color never --flat -e "$SEARCHFOR_BENCH_PATTERN" -- "$SEARCHFOR_BENCH_DIR"'
  awk -v a="$SEARCHFOR_S" -v b="$RG_S" -v by="$bytes" -v t="$threads" \
    'BEGIN { budget = by / (2.5e6 * t); printf "  extra %.3f s, budget %.3f s\n", a - b, budget; exit !(a - b <= budget) }' \
    || { echo "  FAIL: over the parsing budget"; fail=1; }
}

corpus registry "$dir_a" "$sparse_a"
corpus apps "$dir_b" "$sparse_b"
export SEARCHFOR_BENCH_DIR="$dir_a" SEARCHFOR_BENCH_PATTERN='fn new\('
median registry-dense 'rg --color never -e "$SEARCHFOR_BENCH_PATTERN" -- "$SEARCHFOR_BENCH_DIR"' \
  '"$SEARCHFOR_BIN_PATH" --color never --flat -e "$SEARCHFOR_BENCH_PATTERN" -- "$SEARCHFOR_BENCH_DIR"'
exit "$fail"
