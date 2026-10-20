#!/usr/bin/env bash
# M5a acceptance (M5a plan, D18), on the corpus of scripts/gen-m4-corpus.py (1 080 documents,
# 300 object files, 20 000 text files), against ripgrep 14.1.1:
#   - with --no-docs and --no-trail, a sparse pattern: `-P`, `-U`, `--column -b -o` and
#     `--passthru`-free output flags print rg's lines, and s4 ≤ 1.1 × rg;
#   - `--files` and `--files --sort path`: the same files as rg over the corpus, and s4 ≤ 1.1 ×
#     rg over a generated tree of 100 000 files, where rg takes tens of milliseconds and a
#     3 ms difference is not 10 % (M5a plan, review r1, Fable #5);
#   - the Trail layouts with rg's output flags (`--column -b`, `-o`, `--vimgrep`, `-r`,
#     `--trim`, `--stats`): ≤ 1.1 × s4 --flat with no flag;
#   - with --no-docs, M5a's s4 against M4's (SEARCHFOR_BASE_BIN, the s4 of 522bf96), the sparse
#     pattern over the whole corpus, `--flat` and `--no-trail`: ≤ 1.03 ×, the median of three
#     alternated series (what M5a adds to a search that uses none of its flags); skipped
#     without SEARCHFOR_BASE_BIN;
#   - the release binary at most 23 MB (8612), its size printed.
# Usage: scripts/bench-m5.sh [CORPUS_DIR]  (generated there when missing; a temporary one by
# default). Needs python3, hyperfine, jq, awk and GNU stat. SEARCHFOR_BIN replaces the s4 under
# test; SEARCHFOR_RG, else /usr/bin/rg, is the reference and must be ripgrep 14.1.1.
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
unset RIPGREP_CONFIG_PATH SEARCHFOR_CONFIG_PATH
dir="${1:-$iso/corpus}"
# Split in two, so no file holds it, this script included (M3 code review r1).
sparse='zqm4''sparse'
fail=0
[ -d "$dir" ] || python3 "$(dirname "$0")/gen-m4-corpus.py" "$dir" "$sparse"
export SEARCHFOR_BENCH_DIR="$dir" SEARCHFOR_BENCH_PATTERN="$sparse"

median() {
  hyperfine --ignore-failure --warmup 5 --runs 40 --export-json "$iso/$1.json" "$2" "$3" >/dev/null
  if ! jq -e '[.results[].exit_codes[]] | all(. == 0 or . == 1)' "$iso/$1.json" >/dev/null; then
    echo "  FAIL $1: a timed run did not exit 0 or 1"; fail=1
  fi
  A_S=$(jq '.results[0].median' "$iso/$1.json")
  B_S=$(jq '.results[1].median' "$iso/$1.json")
  awk -v a="$B_S" -v b="$A_S" -v n="$1" -v x="${4:-rg}" -v y="${5:-s4}" \
    'BEGIN { printf "%-16s %s %.3f s  %s %.3f s  ratio %.2f\n", n, x, b, y, a, a / b }'
}
within() {
  local f="${1:-1.1}"
  awk -v a="$B_S" -v b="$A_S" -v f="$f" 'BEGIN { exit !(a <= f * b) }' || { echo "  FAIL: over $f × the first"; fail=1; }
}
# same NAME ARGS...: `s4 --no-docs --no-trail ARGS` prints rg's lines (sorted: the threads).
same() {
  local name=$1; shift
  rc=0; "$s4" --color never --no-docs --no-trail "$@" >"$iso/s.out" 2>"$iso/s.err" || rc=$?
  rrc=0; "$rg" --color never "$@" >"$iso/r.out" 2>"$iso/r.err" || rrc=$?
  [ "$rc" = "$rrc" ] && cmp -s <(sort "$iso/s.out") <(sort "$iso/r.out") \
    || { echo "  FAIL $name: not rg's output"; fail=1; }
}

echo "== $dir"
r='"$SEARCHFOR_RG_PATH" --color never'
n='"$SEARCHFOR_BIN_PATH" --color never --no-docs --no-trail'
for flags in "-P" "-U" "--column -b -o" "--vimgrep" "-r X --trim"; do
  # shellcheck disable=SC2086
  same "$flags" $flags -e "$sparse" -- "$dir"
  median "no-trail $flags" "$r $flags -e \"\$SEARCHFOR_BENCH_PATTERN\" -- \"\$SEARCHFOR_BENCH_DIR\"" \
    "$n $flags -e \"\$SEARCHFOR_BENCH_PATTERN\" -- \"\$SEARCHFOR_BENCH_DIR\""; within
done

# --files, unsorted and sorted.
cmp -s <("$s4" --files -- "$dir" | sort) <("$rg" --files -- "$dir" | sort) || { echo "  FAIL --files: not rg's files"; fail=1; }
cmp -s <("$s4" --files --sort path -- "$dir") <("$rg" --files --sort path -- "$dir") || { echo "  FAIL --files --sort path"; fail=1; }
# Timed over a tree of 100 000 small files in 250 directories.
export SEARCHFOR_BENCH_FILES="$iso/files"
python3 - "$SEARCHFOR_BENCH_FILES" <<'PY'
import os, sys
for d in range(250):
    p = os.path.join(sys.argv[1], f"d{d:03}")
    os.makedirs(p)
    for f in range(400):
        with open(os.path.join(p, f"f{f:03}.txt"), "w") as out:
            out.write("x\n")
PY
median files '"$SEARCHFOR_RG_PATH" --files -- "$SEARCHFOR_BENCH_FILES"' '"$SEARCHFOR_BIN_PATH" --files -- "$SEARCHFOR_BENCH_FILES"'; within
median files-sorted '"$SEARCHFOR_RG_PATH" --files --sort path -- "$SEARCHFOR_BENCH_FILES"' \
  '"$SEARCHFOR_BIN_PATH" --files --sort path -- "$SEARCHFOR_BENCH_FILES"'; within

# The Trail layouts with rg's output flags, against the same layout without them.
f='"$SEARCHFOR_BIN_PATH" --color never --flat --no-docs'
for flags in "--column -b" "-o" "--vimgrep" "-r X" "--trim" "--stats"; do
  median "flat $flags" "$f -e \"\$SEARCHFOR_BENCH_PATTERN\" -- \"\$SEARCHFOR_BENCH_DIR\"" \
    "$f $flags -e \"\$SEARCHFOR_BENCH_PATTERN\" -- \"\$SEARCHFOR_BENCH_DIR\"" s4 "s4 $flags"; within
done

if [ -n "${SEARCHFOR_BASE_BIN:-}" ]; then
  # Three series in alternating order, and the median of their ratios (M4 plan r7).
  export SEARCHFOR_BASE_PATH="$SEARCHFOR_BASE_BIN"
  for layout in --flat --no-trail; do
    m4="\"\$SEARCHFOR_BASE_PATH\" --color never $layout --no-docs -e \"\$SEARCHFOR_BENCH_PATTERN\" -- \"\$SEARCHFOR_BENCH_DIR\""
    m5="\"\$SEARCHFOR_BIN_PATH\" --color never $layout --no-docs -e \"\$SEARCHFOR_BENCH_PATTERN\" -- \"\$SEARCHFOR_BENCH_DIR\""
    ratios=""
    for s in 1 2 3; do
      if [ "$s" = 2 ]; then
        median "m5-added-$s" "$m5" "$m4" 's4 M5a' 's4 M4' >/dev/null; x=$(awk -v a="$B_S" -v b="$A_S" 'BEGIN { print b / a }')
      else
        median "m5-added-$s" "$m4" "$m5" 's4 M4' 's4 M5a' >/dev/null; x=$(awk -v a="$B_S" -v b="$A_S" 'BEGIN { print a / b }')
      fi
      ratios="$ratios $x"
    done
    x=$(echo $ratios | tr ' ' '\n' | sort -g | sed -n 2p)
    printf "%-16s s4 M5a / s4 M4 %s, the median of%s: %.3f\n" m5-added "$layout" "$(printf ' %.3f' $ratios)" "$x"
    awk -v r="$x" 'BEGIN { exit !(r <= 1.03) }' || { echo "  FAIL: over 1.03 × M4"; fail=1; }
  done
else
  echo "m5-added         skipped: SEARCHFOR_BASE_BIN, the s4 of 522bf96, is not set"
fi

size=$(stat -c %s "$s4")
printf "binary           %d bytes (%.1f MB), ceiling 23 MB\n" "$size" "$(awk -v s="$size" 'BEGIN { print s / 1e6 }')"
[ "$size" -le 23000000 ] || { echo "  FAIL: over 23 MB"; fail=1; }
exit "$fail"
