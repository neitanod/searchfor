#!/usr/bin/env bash
# M4 acceptance (spec §11 #13, M4 plan D23), on a corpus that scripts/gen-m4-corpus.py writes:
# 1 080 documents of the nine document Formats, 300 object files copied from the system, and
# 20 000 text files (enough files that s4's start, about 2.5 ms more than rg's, decides at
# most a few hundredths of a ratio: M3 plan, «Verified»; M4 plan r6), against ripgrep 14.1.1.
# Documents are searched by default; --no-docs turns that off (grill, 8750):
#   - with --no-docs and without --binary, a pattern no file matches, a sparse one and
#     --no-trail: s4 ≤ 1.1 × rg (a document or a walked object costs what it costs rg: 8385);
#   - the default over the text files and the object files, a pattern no file matches: s4 ≤
#     1.05 × s4 --no-docs (no document, no cost; the object files have no extension, so the
#     default reads their first bytes: M4 plan r1, Fable #5). Its ratio to rg is printed: over
#     these small files it is s4's own, M1–M3's, 1.04–1.10 × with or without documents, which
#     put a ceiling of 1.1 × rg here at the mercy of the noise (M4 plan r6, Fable #5);
#   - the default over 20 000 files of about 100 bytes without an extension, seven levels deep
#     (as .git/objects or bin/), written to a temporary directory, a pattern no file matches,
#     with and without the Trail: s4 ≤ 1.05 × s4 --no-docs. A name without an extension may be
#     a document's, so the default reads its first bytes; the search reads on from them (code
#     review r6, Fable #1: the corpus's few large object files hid what it cost a small file);
#     and with -L through the same tree, which has a link to one of its directories: s4 -L ≤
#     1.05 × s4 -L --no-docs (a name is resolved only where the walker followed a link: code
#     review r7, Fable #2);
#   - with --no-docs, s4 against M3's (SEARCHFOR_BASE_BIN, the sfor of 191dbaf), the sparse
#     pattern over the whole corpus: ≤ 1.03 ×, the median of three series (what M4 adds with
#     --no-docs, the readers' memory counter of D31 included; M4 plan r7, Fable #2); skipped
#     without SEARCHFOR_BASE_BIN;
#   - --binary with the sparse pattern over the object files: s4 − rg ≤ the largest matching
#     object ÷ 100 MB/s + the matching objects ÷ (100 MB/s × threads) + 0.05 s;
#   - the default with the sparse pattern over the documents: s4 ≤ rg over their texts
#     (texts/) + the texts' bytes ÷ (2 MB/s × threads) + 0.1 s, and s4 finds the lines rg finds;
#   - the default with a pattern no file matches over the whole corpus, where every document
#     is extracted: s4 ≤ 1.1 × rg + the texts' bytes ÷ (2 MB/s × threads) + 0.1 s.
# Usage: scripts/bench-m4.sh [CORPUS_DIR]  (generated there when missing; a temporary one by
# default). Needs python3, hyperfine, jq, awk and GNU stat. SEARCHFOR_BIN replaces the s4 under
# test; SEARCHFOR_RG, else /usr/bin/rg, is the reference and must be ripgrep 14.1.1;
# SEARCHFOR_BASE_BIN is M3's sfor, for the docs-off case (s4 --no-docs against it).
set -euo pipefail
export LC_ALL=C
cargo build --release --quiet
s4="${SEARCHFOR_BIN:-./target/release/s4}"
export SEARCHFOR_BIN_PATH="$s4"
rg="${SEARCHFOR_RG:-/usr/bin/rg}"
if ! "$rg" --version 2>/dev/null | head -1 | grep -qx 'ripgrep 14.1.1'; then
  echo "FAIL: the reference $rg is not ripgrep 14.1.1"; exit 1
fi
export SEARCHFOR_RG_PATH="$rg"
iso=$(mktemp -d)
trap 'rm -rf "$iso"' EXIT
mkdir -p "$iso/formats" "$iso/grammars"
export SEARCHFOR_FORMATS_DIR="$iso/formats" SEARCHFOR_GRAMMARS_DIR="$iso/grammars" SEARCHFOR_GRAMMAR_INDEX="file://$iso/none.json"
# M3's sfor (SEARCHFOR_BASE_BIN) reads the same overrides under their names of then.
export SFOR_FORMATS_DIR="$iso/formats" SFOR_GRAMMARS_DIR="$iso/grammars" SFOR_GRAMMAR_INDEX="file://$iso/none.json"
unset RIPGREP_CONFIG_PATH
threads=$(nproc); [ "$threads" -gt 12 ] && threads=12
dir="${1:-$iso/corpus}"
# Split in two, so no file holds them, this script included (M3 code review r1).
sparse='zqm4''sparse'; none='zzqq-searchfor-m4-''absent'
fail=0
[ -d "$dir" ] || python3 "$(dirname "$0")/gen-m4-corpus.py" "$dir" "$sparse"
export SEARCHFOR_BENCH_DIR="$dir" SEARCHFOR_BENCH_PATTERN="$sparse" SEARCHFOR_BENCH_NONE="$none"

median() {
  # 40 runs: a ratio of two 45 ms medians of 15 runs moved by ±3 % between runs (M4 plan r1).
  hyperfine --ignore-failure --warmup 5 --runs 40 --export-json "$iso/$1.json" "$2" "$3" >/dev/null
  if ! jq -e '[.results[].exit_codes[]] | all(. == 0 or . == 1)' "$iso/$1.json" >/dev/null; then
    echo "  FAIL $1: a timed run did not exit 0 or 1"; fail=1
  fi
  RG_S=$(jq '.results[0].median' "$iso/$1.json")
  S4_S=$(jq '.results[1].median' "$iso/$1.json")
  # The two commands' names: rg and s4, unless given.
  awk -v a="$S4_S" -v b="$RG_S" -v n="$1" -v x="${4:-rg}" -v y="${5:-s4}" \
    'BEGIN { printf "%-14s %s %.3f s  %s %.3f s  ratio %.2f\n", n, x, b, y, a, a / b }'
}
# within [F]: the second command took at most F (1.1 by default) × the first's time.
within() {
  local f="${1:-1.1}"
  awk -v a="$S4_S" -v b="$RG_S" -v f="$f" 'BEGIN { exit !(a <= f * b) }' || { echo "  FAIL: over $f × the first"; fail=1; }
}

# lines NAME FILE_S4 FILE_RG: s4 printed at least the lines rg printed.
lines() {
  local a b; a=$(wc -l <"$2"); b=$(wc -l <"$3")
  [ "$a" -ge "$b" ] || { echo "  FAIL $1: s4 printed $a lines, rg $b"; fail=1; }
}

docs=$(find "$dir/docs" -type f | wc -l); objs=$(find "$dir/bin" -type f | wc -l)
echo "== $dir: $docs documents, $objs object files, threads $threads"
[ "$objs" -gt 0 ] || { echo "FAIL: no object file was copied"; exit 1; }

# 1. With --no-docs and without --binary: rg's cost.
for q in "$none" "$sparse"; do
  rc=0; "$s4" --color never --no-docs --no-trail -e "$q" -- "$dir" >"$iso/s.out" 2>"$iso/s.err" || rc=$?
  rrc=0; "$rg" --color never -e "$q" -- "$dir" >"$iso/r.out" 2>"$iso/r.err" || rrc=$?
  [ "$rc" = "$rrc" ] && cmp -s <(sort "$iso/s.out") <(sort "$iso/r.out") && [ ! -s "$iso/s.err" ] \
    || { echo "  FAIL --no-trail $q: not rg's output"; fail=1; }
done
median none '"$SEARCHFOR_RG_PATH" --color never -e "$SEARCHFOR_BENCH_NONE" -- "$SEARCHFOR_BENCH_DIR"' \
  '"$SEARCHFOR_BIN_PATH" --color never --flat --no-docs -e "$SEARCHFOR_BENCH_NONE" -- "$SEARCHFOR_BENCH_DIR"'; within
median sparse '"$SEARCHFOR_RG_PATH" --color never -e "$SEARCHFOR_BENCH_PATTERN" -- "$SEARCHFOR_BENCH_DIR"' \
  '"$SEARCHFOR_BIN_PATH" --color never --flat --no-docs -e "$SEARCHFOR_BENCH_PATTERN" -- "$SEARCHFOR_BENCH_DIR"'; within
median no-trail '"$SEARCHFOR_RG_PATH" --color never -e "$SEARCHFOR_BENCH_PATTERN" -- "$SEARCHFOR_BENCH_DIR"' \
  '"$SEARCHFOR_BIN_PATH" --color never --no-docs --no-trail -e "$SEARCHFOR_BENCH_PATTERN" -- "$SEARCHFOR_BENCH_DIR"'; within

# 2. The default where there is no document: text files, and object files without an
# extension. What M4 adds is the default against s4 --no-docs; the ratio to rg is printed only.
median docs-none '"$SEARCHFOR_RG_PATH" --color never -e "$SEARCHFOR_BENCH_NONE" -- "$SEARCHFOR_BENCH_DIR/text" "$SEARCHFOR_BENCH_DIR/bin"' \
  '"$SEARCHFOR_BIN_PATH" --color never --flat -e "$SEARCHFOR_BENCH_NONE" -- "$SEARCHFOR_BENCH_DIR/text" "$SEARCHFOR_BENCH_DIR/bin"'
median docs-added '"$SEARCHFOR_BIN_PATH" --color never --flat --no-docs -e "$SEARCHFOR_BENCH_NONE" -- "$SEARCHFOR_BENCH_DIR/text" "$SEARCHFOR_BENCH_DIR/bin"' \
  '"$SEARCHFOR_BIN_PATH" --color never --flat -e "$SEARCHFOR_BENCH_NONE" -- "$SEARCHFOR_BENCH_DIR/text" "$SEARCHFOR_BENCH_DIR/bin"' \
  's4 --no-docs' s4; within 1.05
python3 -I - "$iso/noext" <<'__NOEXT__'
import os, sys
n = 0
for a in range(4):
    for b in range(4):
        for c in range(5):
            d = os.path.join(sys.argv[1], f"d{a}", f"d{b}", f"d{c}", "e", "f", "g")
            os.makedirs(d)
            for _ in range(250):
                with open(os.path.join(d, f"f{n:05x}"), "w") as f:
                    f.write(f"line {n:05} of a small file without an extension\n" * 2)
                n += 1
os.symlink("d2", os.path.join(sys.argv[1], "d3", "linked"))
__NOEXT__
export SEARCHFOR_BENCH_NOEXT="$iso/noext"
median noext-rg '"$SEARCHFOR_RG_PATH" --color never -e "$SEARCHFOR_BENCH_NONE" -- "$SEARCHFOR_BENCH_NOEXT"' \
  '"$SEARCHFOR_BIN_PATH" --color never --flat -e "$SEARCHFOR_BENCH_NONE" -- "$SEARCHFOR_BENCH_NOEXT"'
median noext-added '"$SEARCHFOR_BIN_PATH" --color never --flat --no-docs -e "$SEARCHFOR_BENCH_NONE" -- "$SEARCHFOR_BENCH_NOEXT"' \
  '"$SEARCHFOR_BIN_PATH" --color never --flat -e "$SEARCHFOR_BENCH_NONE" -- "$SEARCHFOR_BENCH_NOEXT"' \
  's4 --no-docs' s4; within 1.05
median noext-passthru '"$SEARCHFOR_BIN_PATH" --color never --no-trail --no-docs -e "$SEARCHFOR_BENCH_NONE" -- "$SEARCHFOR_BENCH_NOEXT"' \
  '"$SEARCHFOR_BIN_PATH" --color never --no-trail -e "$SEARCHFOR_BENCH_NONE" -- "$SEARCHFOR_BENCH_NOEXT"' \
  's4 --no-docs' s4; within 1.05
median noext-L-added '"$SEARCHFOR_BIN_PATH" --color never --flat -L --no-docs -e "$SEARCHFOR_BENCH_NONE" -- "$SEARCHFOR_BENCH_NOEXT"' \
  '"$SEARCHFOR_BIN_PATH" --color never --flat -L -e "$SEARCHFOR_BENCH_NONE" -- "$SEARCHFOR_BENCH_NOEXT"' \
  's4 -L --no-docs' 's4 -L'; within 1.05
median noext-L-passthru '"$SEARCHFOR_BIN_PATH" --color never --no-trail -L --no-docs -e "$SEARCHFOR_BENCH_NONE" -- "$SEARCHFOR_BENCH_NOEXT"' \
  '"$SEARCHFOR_BIN_PATH" --color never --no-trail -L -e "$SEARCHFOR_BENCH_NONE" -- "$SEARCHFOR_BENCH_NOEXT"' \
  's4 -L --no-docs' 's4 -L'; within 1.05
if [ -n "${SEARCHFOR_BASE_BIN:-}" ]; then
  # Three series in alternating order, and the median of their ratios: one series of the same
  # code with and without D31's counter gave 0.98–1.04 here (M4 plan r7).
  export SEARCHFOR_BASE_PATH="$SEARCHFOR_BASE_BIN"
  m3='"$SEARCHFOR_BASE_PATH" --color never --flat -e "$SEARCHFOR_BENCH_PATTERN" -- "$SEARCHFOR_BENCH_DIR"'
  m4='"$SEARCHFOR_BIN_PATH" --color never --flat --no-docs -e "$SEARCHFOR_BENCH_PATTERN" -- "$SEARCHFOR_BENCH_DIR"'
  ratios=""
  for s in 1 2 3; do
    if [ "$s" = 2 ]; then
      median docs-off-$s "$m4" "$m3" 's4 M4' 's4 M3' >/dev/null; r=$(awk -v a="$S4_S" -v b="$RG_S" 'BEGIN { print b / a }')
    else
      median docs-off-$s "$m3" "$m4" 's4 M3' 's4 M4' >/dev/null; r=$(awk -v a="$S4_S" -v b="$RG_S" 'BEGIN { print a / b }')
    fi
    ratios="$ratios $r"
  done
  r=$(echo $ratios | tr ' ' '\n' | sort -g | sed -n 2p)
  printf "%-14s s4 M4 / s4 M3, the median of%s: %.3f\n" docs-off "$(printf ' %.3f' $ratios)" "$r"
  awk -v r="$r" 'BEGIN { exit !(r <= 1.03) }' || { echo "  FAIL: over 1.03 × M3"; fail=1; }
else
  echo "docs-off       skipped: SEARCHFOR_BASE_BIN, the sfor of 191dbaf, is not set"
fi

# 3. --binary over the object files.
"$s4" --color never --flat --binary -e "$sparse" -- "$dir/bin" >"$iso/s.out" 2>"$iso/s.err" || true
"$rg" --color never --binary -l -e "$sparse" -- "$dir/bin" >"$iso/r.out" || true
[ ! -s "$iso/s.err" ] || { echo "  FAIL --binary: stderr: $(head -3 "$iso/s.err")"; fail=1; }
lines binary "$iso/s.out" "$iso/r.out"
sizes=$(tr '\n' '\0' <"$iso/r.out" | xargs -0 -r stat -c %s)
bytes=$(echo "$sizes" | awk '{ s += $1 } END { print s + 0 }')
largest=$(echo "$sizes" | awk '$1 > m { m = $1 } END { print m + 0 }')
median binary '"$SEARCHFOR_RG_PATH" --color never --binary -e "$SEARCHFOR_BENCH_PATTERN" -- "$SEARCHFOR_BENCH_DIR/bin"' \
  '"$SEARCHFOR_BIN_PATH" --color never --flat --binary -e "$SEARCHFOR_BENCH_PATTERN" -- "$SEARCHFOR_BENCH_DIR/bin"'
awk -v a="$S4_S" -v b="$RG_S" -v by="$bytes" -v big="$largest" -v t="$threads" \
  'BEGIN { budget = big / 100e6 + by / (100e6 * t) + 0.05; printf "  extra %.3f s, budget %.3f s\n", a - b, budget; exit !(a - b <= budget) }' \
  || { echo "  FAIL: over the budget"; fail=1; }

# 4. The default over the documents, against rg over their texts.
"$s4" --color never --no-trail -e "$sparse" -- "$dir/docs" >"$iso/s.out" 2>"$iso/s.err" || true
"$rg" --color never -e "$sparse" -- "$dir/texts" >"$iso/r.out" || true
[ ! -s "$iso/s.err" ] || { echo "  FAIL documents: stderr: $(head -3 "$iso/s.err")"; fail=1; }
lines docs "$iso/s.out" "$iso/r.out"
text=$(find "$dir/texts" -type f -printf '%s\n' | awk '{ s += $1 } END { print s + 0 }')
median docs '"$SEARCHFOR_RG_PATH" --color never -e "$SEARCHFOR_BENCH_PATTERN" -- "$SEARCHFOR_BENCH_DIR/texts"' \
  '"$SEARCHFOR_BIN_PATH" --color never --flat -e "$SEARCHFOR_BENCH_PATTERN" -- "$SEARCHFOR_BENCH_DIR/docs"'
awk -v a="$S4_S" -v b="$RG_S" -v by="$text" -v t="$threads" \
  'BEGIN { budget = by / (2e6 * t) + 0.1; printf "  extra %.3f s, budget %.3f s (%d MB of text)\n", a - b, budget, by / 1e6; exit !(a - b <= budget) }' \
  || { echo "  FAIL: over the budget"; fail=1; }

# 5. The default over the whole corpus, where every document is extracted although none holds
# the pattern: what a search with no flag costs (grill, 8750).
median default '"$SEARCHFOR_RG_PATH" --color never -e "$SEARCHFOR_BENCH_NONE" -- "$SEARCHFOR_BENCH_DIR"' \
  '"$SEARCHFOR_BIN_PATH" --color never --flat -e "$SEARCHFOR_BENCH_NONE" -- "$SEARCHFOR_BENCH_DIR"'
awk -v a="$S4_S" -v b="$RG_S" -v by="$text" -v t="$threads" \
  'BEGIN { budget = 0.1 * b + by / (2e6 * t) + 0.1; printf "  extra %.3f s over rg, budget %.3f s\n", a - b, budget; exit !(a - b <= budget) }' \
  || { echo "  FAIL: over the budget"; fail=1; }
exit "$fail"
