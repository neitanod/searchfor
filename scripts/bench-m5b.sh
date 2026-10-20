#!/usr/bin/env bash
set -euo pipefail
check_ratios() {
  python3 - "$@" <<'PY'
import sys
nt, nd, default = map(float, sys.argv[1:])
raise SystemExit(0 if nt <= 1.05 and nd <= 1.05 and default <= 1.05 else 1)
PY
}
if [[ ${1:-} == --self-test ]]; then
  check_ratios 1.05 1.05 1.05
  if check_ratios 1.0501 1 1; then exit 10; fi
  if check_ratios 1 1.0501 1; then exit 11; fi
  if check_ratios 1 1 1.0501; then exit 12; fi
  printf '%s\n' 'three independent 1.05 gates: PASS'
  exit 0
fi
# M5b's per-file cost (M5b spec §7, plan Task 3.1), over the M4 text corpus: 20 000 files of 40
# lines (scripts/gen-m4-corpus.py writes them under CORPUS/text) and a pattern no file holds.
#
# scripts/bench-m5b.sh [--self-test] [--probe|--accept] CORPUS
# scripts/bench-m5b.sh --compare A B CORPUS
#   --self-test        checks the three gates' evaluator (`check_ratios`), which --accept calls.
#   --probe            N = 5 000, 20 000 and 80 000 files, -j 1, 2, 4 and 12: rg, s4 --no-docs
#                      --no-trail, s4 --no-docs and the default, alternated, with each N's raw
#                      samples, an intercept and a slope per tool, mode and -j (T = a + N × b),
#                      `strace -f -c` syscall counts and `perf stat` counters per -j, and the
#                      detector's variants (upper-case extensions, no extension) at N = 20 000.
#                      Exits 0 once every measurement is valid; the ratios are reported only.
#   --accept           the three gates, 40 pairs each, alternated after warmups, at each tool's
#                      default -j: s4 --no-docs --no-trail ≤ 1.05 × rg, s4 --no-docs ≤ 1.05 × rg,
#                      the default ≤ 1.05 × s4 --no-docs; with SEARCHFOR_BASE_BIN also s4
#                      --no-docs ≤ 1.03 × the previous milestone's (M4 and M5a's ceiling). Exits
#                      1 when a ratio is over its ceiling, 0 when every one passes.
#   --compare A B      the --accept run (N = 20 000, the same -j) with the s4 binaries A and B
#                      in place of s4 and rg, for each of the three s4 modes: the ratio of the
#                      medians B/A and every raw sample. Exits 0 when both ran validly; it never
#                      decides pass or fail from the ratio (used by Task 11.2).
# CORPUS is a directory: the M4 corpus, or a new one. The text units it lacks are written there
# (CORPUS/text, as gen-m4-corpus.py writes them; the probe's under CORPUS/m5b).
# Environment:
#   SEARCHFOR_BIN            the release s4 under test, required (not used by --compare)
#   SEARCHFOR_RG             the reference, ripgrep 14.1.1; /usr/bin/rg by default
#   SEARCHFOR_BENCH_REPORT   the JSON report written, required by every measurement
#   SEARCHFOR_BENCH_VARIANT  a label stored in the report (baseline, worker-matcher, ...)
#   SEARCHFOR_BASE_BIN       an earlier milestone's s4, for its ceiling (--accept)
#   SEARCHFOR_BENCH_PAIRS    pairs per --accept/--compare series (40); SEARCHFOR_BENCH_ROUNDS
#                            rounds per --probe series (10); SEARCHFOR_BENCH_WARMUP (3)
# A missing CORPUS, a reference that is not 14.1.1, an s4 that does not run, or a measured run
# that does not exit 1 (no file holds the pattern) exits 2 with a diagnostic. Run it as
# `nice -n 19 ionice -c3 bash scripts/bench-m5b.sh …`: the measured processes inherit both.
export LC_ALL=C
die() { printf 'bench-m5b: %s\n' "$*" >&2; exit 2; }
mode=${1:-}
case "$mode" in
  --probe | --accept) [[ $# -eq 2 ]] || die "usage: $0 $mode CORPUS"; corpus=$2 ;;
  --compare)
    [[ $# -eq 4 ]] || die "usage: $0 --compare A B CORPUS"
    a=$2 b=$3 corpus=$4 ;;
  *) die "usage: $0 [--self-test] [--probe|--accept] CORPUS | --compare A B CORPUS" ;;
esac
[[ -n $corpus ]] || die "CORPUS is empty"
[[ -n ${SEARCHFOR_BENCH_REPORT:-} ]] || die "SEARCHFOR_BENCH_REPORT, the report's path, is required"
version_of() { "$1" --version 2>/dev/null | head -1 || true; }
if [[ $mode == --compare ]]; then
  for bin in "$a" "$b"; do
    [[ $(version_of "$bin") == searchfor* || $(version_of "$bin") == s4* ]] \
      || die "$bin is not an s4 that runs: $(version_of "$bin")"
  done
  rg=
else
  [[ -n ${SEARCHFOR_BIN:-} ]] || die "SEARCHFOR_BIN, the release s4 under test, is required"
  [[ $(version_of "$SEARCHFOR_BIN") == searchfor* || $(version_of "$SEARCHFOR_BIN") == s4* ]] \
    || die "SEARCHFOR_BIN=$SEARCHFOR_BIN is not an s4 that runs: $(version_of "$SEARCHFOR_BIN")"
  rg=${SEARCHFOR_RG:-/usr/bin/rg}
  [[ $(version_of "$rg") == 'ripgrep 14.1.1' ]] \
    || die "the reference $rg is not ripgrep 14.1.1: $(version_of "$rg")"
  if [[ -n ${SEARCHFOR_BASE_BIN:-} ]]; then
    [[ -x $SEARCHFOR_BASE_BIN ]] || die "SEARCHFOR_BASE_BIN=$SEARCHFOR_BASE_BIN is not a program"
  fi
fi
command -v python3 >/dev/null || die "python3 is required"
mkdir -p "$corpus" || die "cannot create CORPUS $corpus"
# No configuration, no Definition and no Grammar of the user's: a temporary directory each.
iso=$(mktemp -d)
trap 'rm -rf "$iso"' EXIT
mkdir -p "$iso/formats" "$iso/grammars"
export SEARCHFOR_FORMATS_DIR="$iso/formats" SEARCHFOR_GRAMMARS_DIR="$iso/grammars"
export SEARCHFOR_GRAMMAR_INDEX="file://$iso/none.json"
unset RIPGREP_CONFIG_PATH SEARCHFOR_CONFIG_PATH
rc=0
python3 -I - "$mode" "$corpus" "$rg" "${a:-}" "${b:-}" <<'PY' || rc=$?
import json, os, platform, shutil, statistics, subprocess, sys, tempfile, time

mode, corpus, rg, bin_a, bin_b = sys.argv[1:6]
s4 = os.environ.get("SEARCHFOR_BIN", "")
base = os.environ.get("SEARCHFOR_BASE_BIN", "")
report = os.environ["SEARCHFOR_BENCH_REPORT"]
variant = os.environ.get("SEARCHFOR_BENCH_VARIANT", "")
pairs = int(os.environ.get("SEARCHFOR_BENCH_PAIRS", "40"))
rounds = int(os.environ.get("SEARCHFOR_BENCH_ROUNDS", "10"))
warmup = int(os.environ.get("SEARCHFOR_BENCH_WARMUP", "3"))
# Split in two, so no file holds it, this script included (M3 code review r1).
NONE = "zzqq-searchfor-m5b-" "absent"
LINES = 40


class Invalid(Exception):
    pass


def text_units(d, n, ext=".txt", start=0):
    """The M4 text units (gen-m4-corpus.py): `t{i:04d}.txt`, 40 lines each; only those missing."""
    os.makedirs(d, exist_ok=True)
    for i in range(start, start + n):
        p = os.path.join(d, f"t{i:04d}{ext}")
        if not os.path.exists(p):
            with open(p, "w") as f:
                f.write("".join(f"plain line {k} of file {i}\n" for k in range(LINES)))
    return d


def uptime():
    try:
        return subprocess.run(["uptime"], capture_output=True, text=True).stdout.strip()
    except OSError:
        return " ".join(f"{x:.2f}" for x in os.getloadavg())


def run(cmd):
    """One measured run: wall seconds, CPU seconds (user + system) and peak RSS."""
    t0 = time.perf_counter()
    p = subprocess.Popen(cmd, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
    err = p.stderr.read()
    _, status, ru = os.wait4(p.pid, 0)
    wall = time.perf_counter() - t0
    code = os.waitstatus_to_exitcode(status)
    if code != 1:
        raise Invalid(f"{' '.join(cmd)} exited {code}, not 1 (no file holds the pattern): {err[:300]!r}")
    if err:
        raise Invalid(f"{' '.join(cmd)} wrote to stderr: {err[:300]!r}")
    # ru_maxrss is in KiB on Linux, in bytes on macOS.
    rss = ru.ru_maxrss * (1 if sys.platform == "darwin" else 1024)
    return {"wall": wall, "cpu": ru.ru_utime + ru.ru_stime, "rss": rss}


def series(cmds, n):
    """`n` rounds of every command, each round starting at the next command (pairs: alternated),
    after `warmup` untimed runs of each."""
    for _ in range(warmup):
        for c in cmds.values():
            run(c)
    names = list(cmds)
    samples = {k: [] for k in names}
    for r in range(n):
        for k in names[r % len(names):] + names[: r % len(names)]:
            samples[k].append(run(cmds[k]))
    return samples


def medians(samples):
    return {k: {m: statistics.median(s[m] for s in v) for m in ("wall", "cpu", "rss")} for k, v in samples.items()}


def ratio(med, num, den):
    return med[num]["wall"] / med[den]["wall"]


def s4_modes(bin_, j=None):
    jj = ["-j", str(j)] if j else []
    base_ = [bin_, "--color", "never", *jj]
    out = {
        "nt": base_ + ["--no-docs", "--no-trail", "-e", NONE, "--"],
        "nd": base_ + ["--no-docs", "-e", NONE, "--"],
        "default": base_ + ["-e", NONE, "--"],
    }
    return out


def rg_cmd(j=None):
    jj = ["-j", str(j)] if j else []
    return [rg, "--color", "never", *jj, "-e", NONE, "--"]


def fit(points):
    """Least squares T = intercept + N × slope over (N, median wall) points."""
    xs, ys = [p[0] for p in points], [p[1] for p in points]
    mx, my = statistics.fmean(xs), statistics.fmean(ys)
    sxx = sum((x - mx) ** 2 for x in xs)
    slope = sum((x - mx) * (y - my) for x, y in zip(xs, ys)) / sxx if sxx else 0.0
    return {"intercept_s": my - slope * mx, "slope_s_per_file": slope}


def strace_counts(cmd):
    if not shutil.which("strace"):
        return {"available": False, "reason": "strace is not installed"}
    with tempfile.NamedTemporaryFile("r", suffix=".strace") as f:
        p = subprocess.run(["strace", "-f", "-c", "-o", f.name, *cmd],
                           stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
        if p.returncode != 1:
            return {"available": False, "reason": f"strace exited {p.returncode}: {p.stderr[-300:]!r}"}
        counts = {}
        for line in f.read().splitlines():
            parts = line.split()
            # % time, seconds, usecs/call, calls, [errors], syscall
            if len(parts) >= 5 and parts[-1] not in ("total", "syscall") and parts[3].isdigit():
                counts[parts[-1]] = int(parts[3])
        return {"available": True, "calls": counts, "total": sum(counts.values())}


PERF_EVENTS = "cycles,instructions,branches,branch-misses,cache-references,cache-misses"


def perf_counters(cmd):
    if not shutil.which("perf"):
        return {"available": False, "reason": "perf is not installed"}
    p = subprocess.run(["perf", "stat", "-x,", "-e", PERF_EVENTS, "--", *cmd],
                       stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True)
    if p.returncode != 1:
        return {"available": False, "reason": f"perf stat exited {p.returncode}: {p.stderr.strip()[:300]}"}
    counters, missing = {}, []
    for line in p.stderr.splitlines():
        f = line.split(",")
        if len(f) >= 3 and f[2] in PERF_EVENTS.split(","):
            if f[0].strip().isdigit():
                counters[f[2]] = int(f[0])
            else:
                missing.append(f"{f[2]}: {f[0]}")
    if not counters:
        return {"available": False, "reason": "; ".join(missing) or p.stderr.strip()[:300]}
    return {"available": True, "counters": counters, "unsupported": missing}


def environment():
    return {
        "variant": variant,
        "mode": mode,
        "started": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
        "uptime": uptime(),
        "platform": platform.platform(),
        "cpus": os.cpu_count(),
        "s4": s4 or None,
        "rg": rg or None,
        "pattern": NONE,
        "warmup": warmup,
    }


def gate(name, med, num, den, ceiling):
    r = ratio(med, num, den)
    print(f"{name:<34} {num} {med[num]['wall']:.4f} s / {den} {med[den]['wall']:.4f} s = {r:.3f} (ceiling {ceiling})")
    return {"ratio": r, "ceiling": ceiling, "pass": r <= ceiling, "numerator": num, "denominator": den}


def accept():
    text = text_units(os.path.join(corpus, "text"), 20000)
    out = environment()
    out["corpus"] = text
    out["pairs"] = pairs
    out["jobs"] = "default (each tool's own)"
    m = s4_modes(s4)
    runs = {}
    gates = {}
    for name, (num_cmd, den_cmd, ceiling) in {
        "no_docs_no_trail_over_rg": (("nt", m["nt"] + [text]), ("rg", rg_cmd() + [text]), 1.05),
        "no_docs_over_rg": (("nd", m["nd"] + [text]), ("rg", rg_cmd() + [text]), 1.05),
        "default_over_no_docs": (("default", m["default"] + [text]), ("nd", m["nd"] + [text]), 1.05),
    }.items():
        up = uptime()
        samples = series(dict([den_cmd, num_cmd]), pairs)
        med = medians(samples)
        gates[name] = gate(name, med, num_cmd[0], den_cmd[0], ceiling)
        runs[name] = {"uptime": up, "medians": med, "samples": samples}
    if base:
        up = uptime()
        cmds = {"base_nd": [base, "--color", "never", "--no-docs", "-e", NONE, "--", text], "nd": m["nd"] + [text]}
        samples = series(cmds, pairs)
        med = medians(samples)
        gates["no_docs_over_base"] = gate("no_docs_over_base", med, "nd", "base_nd", 1.03)
        runs["no_docs_over_base"] = {"uptime": up, "medians": med, "samples": samples}
    else:
        print("no_docs_over_base                  skipped: SEARCHFOR_BASE_BIN is not set")
    out["gates"] = gates
    out["series"] = runs
    out["uptime_end"] = uptime()
    return out


def compare():
    text = text_units(os.path.join(corpus, "text"), 20000)
    out = environment()
    out["a"], out["b"] = bin_a, bin_b
    out["corpus"] = text
    out["pairs"] = pairs
    out["jobs"] = "default (each tool's own)"
    ma, mb = s4_modes(bin_a), s4_modes(bin_b)
    res = {}
    for k in ("nt", "nd", "default"):
        up = uptime()
        samples = series({"A": ma[k] + [text], "B": mb[k] + [text]}, pairs)
        med = medians(samples)
        r = ratio(med, "B", "A")
        cpu = med["B"]["cpu"] / med["A"]["cpu"]
        print(f"{k:<8} A {med['A']['wall']:.4f} s  B {med['B']['wall']:.4f} s  B/A {r:.3f}  (CPU B/A {cpu:.3f})")
        res[k] = {"uptime": up, "ratio_b_over_a": r, "cpu_ratio_b_over_a": cpu, "medians": med,
                  "samples": samples}
    out["modes"] = res
    out["uptime_end"] = uptime()
    return out


def probe():
    out = environment()
    out["rounds"] = rounds
    sizes = {5000: text_units(os.path.join(corpus, "m5b", "n5000"), 5000),
             20000: text_units(os.path.join(corpus, "text"), 20000),
             80000: text_units(os.path.join(corpus, "m5b", "n80000"), 80000)}
    out["corpora"] = {str(k): v for k, v in sizes.items()}
    jobs = [1, 2, 4, 12]
    data = {}
    for n, d in sizes.items():
        for j in jobs:
            m = s4_modes(s4, j)
            cmds = {"rg": rg_cmd(j) + [d], "nt": m["nt"] + [d], "nd": m["nd"] + [d], "default": m["default"] + [d]}
            up = uptime()
            samples = series(cmds, rounds)
            med = medians(samples)
            data[f"n{n}-j{j}"] = {"n": n, "j": j, "uptime": up, "medians": med, "samples": samples,
                                  "ratios": {"nt_over_rg": ratio(med, "nt", "rg"),
                                             "nd_over_rg": ratio(med, "nd", "rg"),
                                             "default_over_nd": ratio(med, "default", "nd")}}
            r = data[f"n{n}-j{j}"]["ratios"]
            print(f"N={n:<6} -j{j:<3} rg {med['rg']['wall']:.4f} s  nt/rg {r['nt_over_rg']:.3f}  "
                  f"nd/rg {r['nd_over_rg']:.3f}  default/nd {r['default_over_nd']:.3f}  [{up}]")
    out["series"] = data
    fits = {}
    for j in jobs:
        for tool in ("rg", "nt", "nd", "default"):
            pts = [(n, data[f"n{n}-j{j}"]["medians"][tool]["wall"]) for n in sizes]
            fits[f"{tool}-j{j}"] = fit(pts)
    out["fits"] = fits
    for j in jobs:
        print(f"-j{j:<3} " + "  ".join(
            f"{t} {fits[f'{t}-j{j}']['intercept_s'] * 1e3:.1f} ms + N × {fits[f'{t}-j{j}']['slope_s_per_file'] * 1e6:.2f} µs"
            for t in ("rg", "nt", "nd", "default")))
    # The detector's variants: upper-case extensions and names without one, N = 20 000.
    variants = {"upper": text_units(os.path.join(corpus, "m5b", "upper"), 20000, ".TXT"),
                "noext": text_units(os.path.join(corpus, "m5b", "noext"), 20000, "")}
    vdata = {}
    for name, d in variants.items():
        m = s4_modes(s4)
        cmds = {"rg": rg_cmd() + [d], "nt": m["nt"] + [d], "nd": m["nd"] + [d], "default": m["default"] + [d]}
        up = uptime()
        samples = series(cmds, rounds)
        med = medians(samples)
        vdata[name] = {"corpus": d, "uptime": up, "medians": med, "samples": samples,
                       "ratios": {"nt_over_rg": ratio(med, "nt", "rg"), "nd_over_rg": ratio(med, "nd", "rg"),
                                  "default_over_nd": ratio(med, "default", "nd")}}
        r = vdata[name]["ratios"]
        print(f"{name:<12} nt/rg {r['nt_over_rg']:.3f}  nd/rg {r['nd_over_rg']:.3f}  default/nd {r['default_over_nd']:.3f}  [{up}]")
    out["variants"] = vdata
    # Profilers, at N = 20 000 for each -j: what is missing is said, not skipped silently.
    prof = {}
    d = sizes[20000]
    for j in jobs:
        m = s4_modes(s4, j)
        cmds = {"rg": rg_cmd(j) + [d], "nt": m["nt"] + [d], "nd": m["nd"] + [d], "default": m["default"] + [d]}
        prof[f"j{j}"] = {k: {"strace": strace_counts(c), "perf": perf_counters(c)} for k, c in cmds.items()}
    out["profilers"] = prof
    avail = {"strace": all(v["strace"]["available"] for p in prof.values() for v in p.values()),
             "perf": all(v["perf"]["available"] for p in prof.values() for v in p.values())}
    out["profilers_available"] = avail
    for tool, ok in avail.items():
        if not ok:
            reasons = {v[tool].get("reason") for p in prof.values() for v in p.values() if not v[tool]["available"]}
            print(f"{tool}: unavailable: {'; '.join(sorted(r for r in reasons if r))}")
    for j in jobs:
        tot = {k: v["strace"].get("total") for k, v in prof[f"j{j}"].items()}
        print(f"strace -j{j:<3} total syscalls " + "  ".join(f"{k} {v}" for k, v in tot.items()))
    # The three gates, named, at -j12 and N = 20 000 (reported only: --accept decides).
    r = data["n20000-j12"]["ratios"]
    out["gates"] = {"no_docs_no_trail_over_rg": r["nt_over_rg"], "no_docs_over_rg": r["nd_over_rg"],
                    "default_over_no_docs": r["default_over_nd"]}
    out["uptime_end"] = uptime()
    return out


try:
    out = {"--probe": probe, "--accept": accept, "--compare": compare}[mode]()
except Invalid as e:
    print(f"bench-m5b: invalid measurement: {e}", file=sys.stderr)
    raise SystemExit(2)
with open(report, "w") as f:
    json.dump(out, f, indent=1)
print(f"report: {report}")
if mode == "--accept":
    g = out["gates"]
    print("GATES " + " ".join(f"{g[k]['ratio']:.6f}" for k in
                              ("no_docs_no_trail_over_rg", "no_docs_over_rg", "default_over_no_docs")))
    extra = g.get("no_docs_over_base")
    raise SystemExit(3 if extra and not extra["pass"] else 0)
PY
[[ $rc -eq 0 || $rc -eq 3 ]] || exit 2
if [[ $mode == --accept ]]; then
  # The three gates go through the evaluator --self-test checks.
  read -r nt nd def < <(python3 -I -c 'import json,sys; g=json.load(open(sys.argv[1]))["gates"]; print(g["no_docs_no_trail_over_rg"]["ratio"], g["no_docs_over_rg"]["ratio"], g["default_over_no_docs"]["ratio"])' "$SEARCHFOR_BENCH_REPORT")
  if check_ratios "$nt" "$nd" "$def" && [[ $rc -eq 0 ]]; then
    echo "accept: PASS"; exit 0
  fi
  echo "accept: FAIL (a ratio is over its ceiling)"; exit 1
fi
exit 0
