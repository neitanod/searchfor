#!/usr/bin/env bash
# M3 acceptance (spec §11 #13, M3 plan D15), on a generated corpus of HTML, XML, CSV, TSV, SRT
# and VTT files (small ones and one big file of each Format), against ripgrep 14.1.1:
#   - a pattern no file matches, and --no-trail: sfor ≤ 1.1 × rg;
#   - a pattern matching under 5 % of the files, the big ones included: sfor − rg ≤ the
#     largest matching file ÷ 50 MB/s + all matching bytes ÷ (50 MB/s × threads) + 0.05 s
#     (a file is walked by one thread);
#   - a dense pattern (every non-empty line of the CSV files matches, once): time and RSS
#     reported, not bounded (D16); sfor must still exit 0 and print every line rg prints.
# Usage: scripts/bench-m3.sh [CORPUS_DIR]  (generated there when missing; a temporary one by
# default). Needs hyperfine, jq, awk, GNU stat and /usr/bin/time. SFOR_BIN replaces the sfor
# under test (SFOR_BIN=scripts/fake-sfor-exit-2.sh must make it exit 1). The reference is
# SFOR_RG, else /usr/bin/rg, which must be ripgrep 14.1.1 (M3 review r1, Codex #8): another rg
# may come first on PATH.
set -euo pipefail
export LC_ALL=C
cargo build --release --quiet
sfor="${SFOR_BIN:-./target/release/sfor}"
export SFOR_BIN_PATH="$sfor"
rg="${SFOR_RG:-/usr/bin/rg}"
if ! "$rg" --version 2>/dev/null | head -1 | grep -qx 'ripgrep 14.1.1'; then
  echo "FAIL: the reference $rg is not ripgrep 14.1.1"; exit 1
fi
export SFOR_RG_PATH="$rg"
iso=$(mktemp -d)
trap 'rm -rf "$iso"' EXIT
mkdir -p "$iso/formats" "$iso/grammars"
export SFOR_FORMATS_DIR="$iso/formats" SFOR_GRAMMARS_DIR="$iso/grammars" SFOR_GRAMMAR_INDEX="file://$iso/none.json"
unset RIPGREP_CONFIG_PATH
threads=$(nproc); [ "$threads" -gt 12 ] && threads=12
dir="${1:-$iso/corpus}"
sparse='zqsparse'; none='zzqq-searchfor-bench-none'; dense='^.'
fail=0

# gen DIR: small files (every 200th holds the sparse word) and one big file (9–19 MB) per
# Format, whose last line holds it.
gen() {
  mkdir -p "$1"
  awk -v d="$1" -v sp="$sparse" 'BEGIN {
    for (i = 0; i < 8000; i++) {
      w = (i % 200 == 0) ? sp : "word"
      f = sprintf("%s/p%04d.html", d, i)
      printf "<!DOCTYPE html>\n<html><body>\n<main id=\"m\">\n" > f
      for (j = 0; j < 40; j++) printf "<section class=\"s\"><h2>T %d</h2><ul><li>one<li>%s</ul><p>text<br>\n</section>\n", j, (j == 20 ? w : "x") > f
      printf "</main></body></html>\n" > f; close(f)
      if (i % 2) continue
      f = sprintf("%s/c%04d.xml", d, i)
      printf "<?xml version=\"1.0\"?>\n<catalog>\n" > f
      for (j = 0; j < 60; j++) printf "  <book id=\"b%d\"><title>%s</title><price>%d</price></book>\n", j, (j == 30 ? w : "t"), j > f
      printf "</catalog>\n" > f; close(f)
      f = sprintf("%s/u%04d.tsv", d, i)
      printf "id\tName\tPlan\n" > f
      for (j = 0; j < 80; j++) printf "%d\tAna\t%s plan\n", j, (j == 40 ? w : "pro") > f
      close(f)
      f = sprintf("%s/t%04d.csv", d, i)
      printf "id,Name,Plan,Notes\n" > f
      for (j = 0; j < 80; j++) printf "%d,Ana,%s plan,\"a, \"\"b\"\"\"\n", j, (j == 40 ? w : "pro") > f
      close(f)
      if (i % 4) continue
      f = sprintf("%s/s%04d.srt", d, i)
      for (j = 1; j <= 60; j++) printf "%d\r\n00:00:%02d,000 --> 00:00:%02d,500\r\nline %s\r\n\r\n", j, j % 60, j % 60, (j == 30 ? w : "x") > f
      close(f)
      f = sprintf("%s/v%04d.vtt", d, i)
      printf "WEBVTT\n\n" > f
      for (j = 1; j <= 60; j++) printf "00:%02d.000 --> 00:%02d.500\nline %s\n\n", j % 60, j % 60, (j == 30 ? w : "x") > f
      close(f)
    }
  }'
  awk -v d="$1" -v sp="$sparse" 'BEGIN {
    f = d "/big.html"; printf "<html><body>\n" > f
    for (j = 0; j < 150000; j++) printf "<div class=\"r\"><ul><li>one<li>two</ul><p>row %d<br>\n</div>\n", j > f
    printf "<p>%s</p></body></html>\n", sp > f; close(f)
    f = d "/big.xml"; printf "<catalog>\n" > f
    for (j = 0; j < 260000; j++) printf "  <book id=\"b%d\"><title>t</title><price>%d</price></book>\n", j, j > f
    printf "  <book><title>%s</title></book>\n</catalog>\n", sp > f; close(f)
    f = d "/big.csv"; printf "id,Name,Plan,Notes\n" > f
    for (j = 0; j < 500000; j++) printf "%d,Bea,pro plan,\"x, \"\"y\"\"\nz\"\n", j > f
    printf "%d,Bea,%s,end\n", j, sp > f; close(f)
    f = d "/big.tsv"; printf "id\tName\tPlan\n" > f
    for (j = 0; j < 700000; j++) printf "%d\tBea\tpro plan\n", j > f
    printf "%d\tBea\t%s\n", j, sp > f; close(f)
    f = d "/big.vtt"; printf "WEBVTT\n\nNOTE generated\n\n" > f
    for (j = 1; j <= 300000; j++) printf "00:00:01.000 --> 00:00:02.000 align:start\nline of text\n\n" > f
    printf "00:00:01.000 --> 00:00:02.000\n%s\n", sp > f; close(f)
    f = d "/big.srt"
    for (j = 1; j <= 300000; j++) printf "%d\n00:00:01,000 --> 00:00:02,000\nline of text\nsecond line\n\n", j > f
    printf "%d\n00:00:01,000 --> 00:00:02,000\n%s\n", j, sp > f; close(f)
  }'
}
[ -d "$dir" ] || gen "$dir"
export SFOR_BENCH_DIR="$dir" SFOR_BENCH_PATTERN="$sparse" SFOR_BENCH_NONE="$none"

# median NAME CMD_RG CMD_SFOR → prints sfor/rg and sets RG_S, SFOR_S (seconds).
median() {
  hyperfine --ignore-failure --warmup 3 --runs 20 --export-json "$iso/$1.json" "$2" "$3" >/dev/null
  # --ignore-failure lets a run that found nothing (exit 1) be timed; a crash, an OOM kill or an
  # error (exit 2) during the timing still fails the benchmark (M3 review r1, Codex #11).
  if ! jq -e '[.results[].exit_codes[]] | all(. == 0 or . == 1)' "$iso/$1.json" >/dev/null; then
    echo "  FAIL $1: a timed run did not exit 0 or 1"; fail=1
  fi
  RG_S=$(jq '.results[0].median' "$iso/$1.json")
  SFOR_S=$(jq '.results[1].median' "$iso/$1.json")
  awk -v a="$SFOR_S" -v b="$RG_S" -v n="$1" 'BEGIN { printf "%-14s rg %.3f s  sfor %.3f s  ratio %.2f\n", n, b, a, a / b }'
}

# check NAME FLAG RG_ARGS…: sfor exits as rg does, writes to stderr what rg writes, and prints
# at least rg's lines (exactly rg's with --no-trail).
check() {
  local name="$1" flag="$2"; shift 2
  local code=0 rgcode=0
  "$sfor" --color never "$flag" "$@" >"$iso/sfor.out" 2>"$iso/sfor.err" || code=$?
  "$rg" --color never "$@" >"$iso/rg.out" 2>"$iso/rg.err" || rgcode=$?
  local e1=0 e2=0
  [ -s "$iso/sfor.err" ] && e1=1
  [ -s "$iso/rg.err" ] && e2=1
  if [ "$code" != "$rgcode" ] || [ "$e1" != "$e2" ]; then
    echo "  FAIL $name: sfor exited $code, rg $rgcode, or stderr differs"; fail=1; return
  fi
  local ours rgs
  ours=$(wc -l <"$iso/sfor.out"); rgs=$(wc -l <"$iso/rg.out")
  if [ "$ours" -lt "$rgs" ] || { [ "$flag" = --no-trail ] && [ "$ours" != "$rgs" ]; }; then
    echo "  FAIL $name: $ours lines, rg printed $rgs"; fail=1
  fi
}

total=$("$rg" --files -- "$dir" | wc -l)
hits=$( ("$rg" -l -e "$sparse" -- "$dir" || true) | wc -l)
sizes=$( ("$rg" -l -0 -e "$sparse" -- "$dir" || true) | xargs -0 -r stat -c %s)
bytes=$(echo "$sizes" | awk '{ s += $1 } END { print s + 0 }')
largest=$(echo "$sizes" | awk '$1 > m { m = $1 } END { print m + 0 }')
echo "== $dir: files $total, matching $hits ($bytes bytes), threads $threads"
awk -v h="$hits" -v t="$total" 'BEGIN { if (h * 20 >= t) { print "the sparse pattern matches 5% or more of the files"; exit 1 } }'
check none --flat -e "$none" -- "$dir"
check sparse --flat -e "$sparse" -- "$dir"
check no-trail --no-trail -e "$sparse" -- "$dir"
[ "$fail" = 0 ] || exit 1
median none '"$SFOR_RG_PATH" --color never -e "$SFOR_BENCH_NONE" -- "$SFOR_BENCH_DIR"' \
  '"$SFOR_BIN_PATH" --color never --flat -e "$SFOR_BENCH_NONE" -- "$SFOR_BENCH_DIR"'
awk -v a="$SFOR_S" -v b="$RG_S" 'BEGIN { exit !(a <= 1.1 * b) }' || { echo "  FAIL: over 1.1 × rg"; fail=1; }
median no-trail '"$SFOR_RG_PATH" --color never -e "$SFOR_BENCH_PATTERN" -- "$SFOR_BENCH_DIR"' \
  '"$SFOR_BIN_PATH" --color never --no-trail -e "$SFOR_BENCH_PATTERN" -- "$SFOR_BENCH_DIR"'
awk -v a="$SFOR_S" -v b="$RG_S" 'BEGIN { exit !(a <= 1.1 * b) }' || { echo "  FAIL: over 1.1 × rg"; fail=1; }
median sparse '"$SFOR_RG_PATH" --color never -e "$SFOR_BENCH_PATTERN" -- "$SFOR_BENCH_DIR"' \
  '"$SFOR_BIN_PATH" --color never --flat -e "$SFOR_BENCH_PATTERN" -- "$SFOR_BENCH_DIR"'
awk -v a="$SFOR_S" -v b="$RG_S" -v by="$bytes" -v big="$largest" -v t="$threads" \
  'BEGIN { budget = big / 50e6 + by / (50e6 * t) + 0.05; printf "  extra %.3f s, budget %.3f s\n", a - b, budget; exit !(a - b <= budget) }' \
  || { echo "  FAIL: over the budget"; fail=1; }
# Dense: every non-empty line of the CSV files matches `^.`, once. Checked like the others (exit
# code, stderr, lines), then timed; its RSS comes from /usr/bin/time (D16: reported only).
check dense --flat -e "$dense" -g '*.csv' -- "$dir"
export SFOR_BENCH_PATTERN="$dense"
median dense '"$SFOR_RG_PATH" --color never -e "$SFOR_BENCH_PATTERN" -g "*.csv" -- "$SFOR_BENCH_DIR"' \
  '"$SFOR_BIN_PATH" --color never --flat -e "$SFOR_BENCH_PATTERN" -g "*.csv" -- "$SFOR_BENCH_DIR"'
for prog in "$rg" "$sfor"; do
  flag=(); [ "$prog" = "$sfor" ] && flag=(--flat)
  rc=0
  /usr/bin/time -f '%M' -o "$iso/rss" "$prog" --color never "${flag[@]}" -e "$dense" -g '*.csv' -- "$dir" >/dev/null 2>"$iso/dense.err" || rc=$?
  [ "$rc" = 0 ] || { echo "  FAIL dense: $prog exited $rc"; fail=1; }
  printf '  dense RSS %s: %s MiB\n' "$(basename "$prog")" "$(( $(tail -1 "$iso/rss") / 1024 ))"
done
exit "$fail"
