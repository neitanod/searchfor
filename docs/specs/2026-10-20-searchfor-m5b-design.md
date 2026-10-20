# searchfor M5b (v1.0) — design

Status: draft · 2026-10-20 · Task 798 (Hilo 855). This document is the spec of M5b. It adds to
[the v1 design](2026-10-16-searchfor-design.md) (called «the spec» below, its sections `§N`),
which stays the source of truth for everything M5b does not change; M5b's plan amends the spec's
§3 item 5, §7, §10 and §11 to match what this document decides. Decisions of Sebastián:
[grill-respuestas.md](grill-respuestas.md) (`#N` are its questions, four-digit numbers are Hilo
messages); `Q57.<id>` names a question of Questionnaire 57 (Hilo 855, 20/10), whose answers are
in the last section of grill-respuestas.md.
Vocabulary: [../../CONTEXT.md](../../CONTEXT.md).

## 1. Goal

v1.0 is the first release anyone can install: s4 works the same on Linux, macOS and Windows,
CI proves it on the three systems on every push, and one tag produces signed archives for every
platform, the Homebrew formula, the npm package and the crate. Along the way M5b removes the two
costs M1–M5a left measured and unbounded: a file where most lines match (spec §3, item 5) and
the per-file cost s4 adds over rg on many small files (Hilo, 18/10).

## 2. Scope

### In M5b

| Part | What | Source |
|---|---|---|
| M5a leftovers | The three findings the last code review of M5a left open (§3) | Hilo 855, handoff |
| Windows and macOS | s4, its helpers and its whole suite build and pass on `x86_64-pc-windows-msvc`, `aarch64-apple-darwin` and `x86_64-apple-darwin` (§4) | 8375, M5a plan D1 |
| Job Object | A Windows Job Object over every document reader and Helper Processor (§4.3) | M4 plan D31 |
| CI | GitHub Actions on the three systems, the fuzz in CI (§5) | M5a plan D1, Sebastián's recommendation (M5a, 9008) |
| Dense files | The Trail pipeline stops holding a file's Matches and Trails (§6) | spec §3 item 5 |
| Per-file cost | s4 over many small files within 1.05 × rg (§7) | Hilo 18/10 |
| Releases | `dist`: archives, checksums, installers, Homebrew tap, npm package, crate (§8) | #18, #20 |
| Signing | GitHub artifact attestations over every archive (§8.4) | M5a plan D1, Q57.firma |
| Grammar Helpers | Per-platform binaries built by the release pipeline and listed in `grammars/index.json` (§9) | spec §7, #8 |
| External Processor guide | `docs/external-processors.md` and its Spanish copy (§10) | spec §4.4, #7 |
| v1.0 | Version 1.0.0, README and skill updated (§11) | — |

### Out of M5b

- Every publication without Sebastián's go-ahead at the moment it happens (§8.6).
- Authenticode and Apple notarization (Q57.codesign): Homebrew, npm, cargo and the install
  scripts do not trigger SmartScreen or Gatekeeper.
- Windows on arm64 (Q57.plataformas): a 1.x adds it with one line of the `dist` configuration.
- The Formats of M6 (#22 follow-ups, 8385).
- Packages for Debian, AUR, Scoop, winget (#18 chose four channels).

## 3. M5a leftovers

Each one gets a failing test first (spec §11 #30's tests, `tests/cli_m5.rs`):

1. **`--expand -U -r` repeats a Match's continuations** (Codex, M5a code r4, MINOR;
   `src/output/mod.rs`, `SectionWalk`): a row belongs to the Section where its Match starts.
   `printf '# A\nneedle\n# B\nlast\nneedle\n' | s4 --format markdown --tree --expand -U -r X 'needle\n# B\nlast|needle'`
   prints the continuation of the Match of Section A once, under A.
2. **`--expand --vimgrep -r` with a replacement holding a newline** (Fable, M5a code r4, MINOR;
   `src/output/mod.rs`, `shown_rows`): the row of a replaced Match covers the whole item, so no
   piece of the Source line comes back as a context row.
3. **`--expand -U -o` drops rows after a gap past the Section's end** (Fable, M5a code r4, NIT):
   a row of the same Match after an empty line prints, with the gap as context, as it does inside
   the Section.

## 4. Windows and macOS

### 4.1 What «works» means

- `cargo build`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo fmt --check` and the whole
  suite pass on Linux x86-64, macOS arm64, macOS x86-64 and Windows x86-64, in CI (§5).
- **A test that cannot run on a system is skipped there with its reason**, never left failing:
  `#[cfg_attr(windows, ignore = "<reason>")]` or a `cfg` on the test. Every skipped test is
  listed, with its reason, in `docs/platforms.md` (new), and the count per system is printed by
  CI. A test is skipped only when the behaviour it checks does not exist on that system
  (signals on Windows, `RLIMIT_DATA`, `/proc`); a test that only uses Unix tools (`sh`,
  `printf`) or `/`-paths is rewritten to run everywhere.
- **The output matches ripgrep 14.1.1 on each system**: with `--no-trail`, s4 prints what that
  system's rg prints (spec §11 #1, #29), and the reference tests run against the rg 14.1.1
  binary CI downloads for that system (§5.2).

### 4.2 Behaviour per system

| Area | Linux (as today) | macOS | Windows |
|---|---|---|---|
| Paths printed | `/` | `/` | rg's: the path as the walker gives it, `\` between components; `--path-separator` replaces it as in rg |
| Hyperlinks (`--hyperlink-format`) | `file://{host}{path}` | same | rg's: `file:///C:/…`, the drive letter kept |
| mmap | as rg | **never for a search** (grep-searcher's rule on macOS, M5a plan D9): every search s4 runs in memory (`rg_in_memory`, `src/search.rs`) reads instead, so binary detection and `bytes_searched` stay rg's; the `Source` mapping of files ≥ 8 MiB and a document reader's mapping stay (a reading for the Processor, not a search, output unchanged) | as rg |
| Hidden files | dot-files | dot-files | dot-files and rg's handling of the hidden attribute (the `ignore` crate does it) |
| Colours | ANSI | ANSI | as rg: `termcolor` writes ANSI where the console accepts it and uses the console API otherwise; `--color=ansi` forces ANSI. A file's output written as it grows (`SpillLines`, `src/app.rs`) copies only bytes, which loses the colours of a console-API buffer, so on Windows it spills only ANSI buffers |
| `--pre` command | as rg | as rg | resolved as rg resolves it (`grep_cli::resolve_binary`), so a program in the current directory is never run by accident |
| MCP server (`s4 --mcp`) | as now | as now | waits for a search's process outside the lock that a cancel needs, notices a client that closed its end, and its searches run in the Job Object |
| Document readers' memory | counter (D31) + `RLIMIT_DATA` | counter + `RLIMIT_DATA` where setrlimit accepts it (failure ignored, as now) | counter + Job Object (§4.3) |
| Document readers' `setrlimit` | applied | applied, and its return value checked: a refusal is one debug message, the counter still holds (today the error is dropped) | — |
| Reader or MCP search dies with s4 | `PR_SET_PDEATHSIG` | the child watches its parent with `kqueue` (`EVFILT_PROC`, `NOTE_EXIT`) and exits at once; the 65 s watchdog stays | the Job Object's kill-on-close (§4.3) |
| Ctrl-C | SIGINT/SIGTERM handler kills and reaps helpers and readers, s4 dies by the signal (M2 plan D42) | same as Linux | a console control handler does the same kill and reap, then s4 exits as an unhandled Ctrl-C does (exit status `0xC000013A`, rg's on Windows) |
| Helpers' process group | s4's | s4's | a Job Object (§4.3); the console delivers Ctrl-C to every process attached to it |
| Data and config dirs | `$XDG_DATA_HOME/searchfor`, `~/.config/searchfor` | `dirs`' (`~/Library/Application Support/searchfor`) | `dirs`' (`%APPDATA%\searchfor`) |
| Grammar Helper binary name | `<lang>` | `<lang>` | `<lang>.exe` |
| Temporary files for helpers (spec §4.4, Paths) | `std::env::temp_dir()` | same | same; removed after the answer, also when the helper still holds them open (retry, then leave it for the system) |

The directories of the data and configuration rows are what s4 already computes through `dirs`;
M5b documents them per system in the README and `--help` and tests them on each system.

### 4.3 The Job Object (Windows)

Every process s4 starts on Windows — a document reader (`--__extract-document`), a Helper
Processor, a Grammar Helper, a `--pre` command — is assigned to one Job Object s4 creates at the
first start, with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`: when s4 ends, however it ends, the system
ends them all. Document readers get a second Job Object each, nested, with
`JOB_OBJECT_LIMIT_PROCESS_MEMORY` at the reader's limit plus `HEADROOM_MIB` — the second barrier
`RLIMIT_DATA` is on Unix (M4 plan D31); a reader the Job Object stops gives the memory warning of
M4 plan D30, as one `RLIMIT_DATA` stops. `--pre` commands are in the first Job Object only (no
memory limit, as rg). A process that starts before its assignment cannot escape: s4 starts each
child suspended (`CREATE_SUSPENDED`), assigns it, and then resumes it. The Windows calls come
from `windows-sys`, a direct dependency under `[target.'cfg(windows)'.dependencies]` with only
the features they need (as `libc` and `signal-hook` are under `cfg(unix)` today); the Job
Object's handle is never inherited by a child. `JOB_OBJECT_LIMIT_PROCESS_MEMORY` limits committed
memory, so a reader past it sees an allocation refused rather than being killed: the counter's
refusal path (M4 plan D31) turns it into the memory warning, as it does with `RLIMIT_DATA`.

### 4.4 Where the work is

The inventory of every Unix-only path is the plan's input: Codex's static reading of 20/10
(`reviews/research/platform.md` in the task's folder, outside the repository), which counts 958
`#[test]` declarations (665 in `src/`, 277 in `tests/`, 16 in the helpers). Measured on 20/10:
`cargo check --all-targets --target x86_64-pc-windows-gnu` compiles the library and the binary
with warnings only, and four test files do not compile (`tests/cli_m4.rs`, `tests/cli_m5.rs`,
`tests/rg_compat.rs`, `tests/helpers.rs`: Unix APIs used outside a `cfg`). An ignored test is
still compiled, so those four get `cfg`s or portable rewrites before any `ignore`.

Also from that reading, kept as requirements:

- The `alive` check of `tests/helpers.rs` and the limiter of `tests/cli_m4.rs` get one
  implementation per system.
- `tests/perf.rs` measures RSS with GNU `time -v`: on macOS and Windows the perf tests that need
  it are skipped with that reason (perf stays a Linux measurement, §5.1).
- The Grammar Helper build (`scripts/build-grammar-helper.sh`) names the binary `<lang>.exe` on
  Windows and takes it from cargo's output for the target, not from a fixed path.
- `.gitattributes` keeps every fixture's bytes (`-text` under `tests/fixtures/`), so a Windows
  checkout does not turn LF into CRLF.
- PCRE2's JIT is off on `aarch64-apple-darwin` (pcre2-sys disables it there): `-P` works, slower,
  and `--pcre2-version` says JIT is unavailable, as that system's rg.

## 5. CI

### 5.1 Workflows

- `.github/workflows/ci.yml`: on every push to any branch and every pull request. A matrix of
  four native jobs: `ubuntu-24.04` (x86-64), `macos-15` (arm64), `macos-15-intel` (x86-64) and
  `windows-2022` (x86-64, MSVC); each runs `cargo fmt --check` (Linux only), `cargo clippy --all-targets
  -- -D warnings`, `cargo build --release`, and `cargo test --workspace --release` with
  `SEARCHFOR_REQUIRE_REFERENCE=1`. Caches `~/.cargo` and `target/` per system.
- **Short fuzz** in the same workflow, Linux only: `tests/fuzz.rs` and `tests/fuzz_m5.rs` with a
  time budget of 120 s each (`SEARCHFOR_FUZZ_SECONDS`).
- `.github/workflows/fuzz-nightly.yml`: every night (cron) and on demand, the two fuzz targets
  for 30 minutes each on Linux and 10 minutes on macOS and Windows; a failure keeps its seed and
  case as an artifact (Q57.fuzz-ci). A scheduled run first compares `main`'s head with the
  commit of the workflow's last successful scheduled run, and ends at once, successfully, when
  they are equal: a night with no new commit runs nothing (Sebastián's comment on Q57.fuzz-ci).
- The perf tests (`tests/perf.rs`) and the `scripts/bench-*.sh` scripts stay local: shared
  runners are too noisy for 1.03–1.1 × ceilings. CI runs `tests/perf.rs` only for what it checks
  besides time (RSS ceilings, the refusal of bombs), with the time assertions behind
  `SEARCHFOR_PERF_TIMING=1`, which CI does not set.

### 5.2 The rg reference

The tests find rg 14.1.1 through `SEARCHFOR_RG` (today `/usr/bin/rg` is fixed in places; M5b
makes every test and script read `SEARCHFOR_RG`, falling back to `/usr/bin/rg` on Unix). CI
downloads ripgrep 14.1.1's official archive for its system from GitHub, checks its SHA-256
against a value written in the workflow, and sets `SEARCHFOR_RG`. The tests never download
anything (M5a plan, outline).

### 5.3 Pushing the workflows

The orchestrator pushes the workflows to the task's branch once they pass locally where they
can (`act` is not required); CI on a public repository costs nothing. The release workflow
(§8) runs only on a `v*` tag, and no tag is pushed without Sebastián's go-ahead (§8.6).

## 6. Dense files

### 6.1 What changes

Today Markdown already streams in the common case (`Outline` + `StreamSink`, `src/app.rs`,
`src/output/stream.rs`); keypath (JSON, YAML, TOML), CSV, subtitles, markup and tree-sitter build
the whole model of the file (`FileMatches` → `Described`, one Trail per Match) before printing,
and the `--json` fallback writes the whole file to a `Vec<u8>` and then copies it to the output
buffer.

M5b extends the Markdown approach to every Processor: a Processor prepares what it needs of the
file once (its Outline: a tree-sitter tree, the keypath index, the CSV header and record
offsets, the cue table, the markup element tree), and then a cursor gives the Trail of each
Match in file order, with consecutive equal Trails shared (`Rc<[Segment]>`). Rows are printed as
they are computed; the `--json` fallback writes into the same bounded output. A Processor that
cannot give Trails in order keeps the model path, and the plan names which (none is expected).

Modes keep their output byte for byte: `--tree` (keeps the previous Trail and its pending rows),
`--within` (local to each Trail; `-m` counts after the filter), `--expand` (by closed Section),
`--in-headings` (names produced in file order), `-c`, `-l`, `--stats`, `--json`'s `end` and
`summary` (counters), `--sort` (sorts files, not Matches). Snapshots do not change; a
differential test compares the stream path with the model path over every fixture and mode.

### 6.2 Output of a file while other threads search

With several search threads, a file's output is kept until the file ends so files do not
interleave (as rg). A dense file's output can be large: past 8 MiB, the worker writes it to a
temporary file of the run and copies it to stdout under the stdout lock when the file ends;
sequential runs (`-j1`, `--sort`) write it as it grows, as today (`Ctx::spill`).

### 6.3 Criterion

`tests/perf.rs`, on 50 MiB files where every line matches, one per Format family (Markdown,
JSON Lines, CSV, YAML, a code file, HTML, SubRip), in the one-line layout and with `--json`,
each against the same command with `--no-trail` (time `T₀`, peak RSS `R₀`), release build:

- **Memory**: peak RSS ≤ `R₀` + the Processor's preparation of that file + 32 MiB, where the
  preparation is measured as the same file searched with a pattern that matches once. Densely
  and sparsely matched, the same bytes: RSS(dense) ≤ RSS(one Match) + 32 MiB, in every Format
  and with `--json`. This is the check that the cost no longer grows with the Matches.
- **Time**: ≤ the preparation's time + 4 × `T₀` + 0.1 s in the one-line layout, + 6 × `T₀` +
  0.1 s with `--json` (an estimate from Codex's reading of the code, 20/10; the plan's probe
  measures it and may tighten it, never loosen it without saying so).
- The M5a check (`m5_22`, every mode below 6 × from N to 4 N Matches) stays green.

The spec's §3 item 5 and §11 #13 change from «measured and reported, not bounded» to this
criterion.

## 7. Per-file cost

The M4 benchmark's text corpus (20 000 files of 40 lines) costs s4 1.04–1.11 × rg with a pattern
no file matches, also with `--no-trail --no-docs` (spec §11 #26; Hilo 18/10). The code reading of
20/10 ranks the suspects; the strongest is that s4 shares one Rust regex matcher among the search
threads, where rg gives each worker a clone (regex-automata keeps its cache pool per clone).

M5b measures each suspect alone (`strace -c`, `perf stat` over `-j1/2/4/12`, the intercept and
slope over corpora of 5 000, 20 000 and 80 000 files) and fixes what it confirms, in order: a
matcher per worker; one path representation per file and no boxed reader for a plain file; no
shared atomic for an empty output; no lowercase copy of an already-lowercase extension;
classifying a file's Format once. The allocator counter of M4 plan D31 is measured last: it stays
unless it costs more than 1 % (a separate reader binary is the alternative the plan weighs).

**Criterion** (`scripts/bench-m5b.sh`, the M4 text corpus, 40 runs alternated, a pattern no file
matches): `s4 --no-docs --no-trail` ≤ 1.05 × rg, `s4 --no-docs` ≤ 1.05 × rg, and the default ≤
1.05 × `s4 --no-docs`. Every other ceiling of §11 #13, #26 and #32 stays. If a cause is confirmed
and cannot be removed without changing output, the plan reports it with its measured share, and
the criterion that cannot be met is a question for Sebastián, not a silent relaxation.

## 8. Releases

### 8.1 `dist`

`dist` (axodotdev's, formerly cargo-dist), its version pinned in `dist-workspace.toml`, builds
the release from a `v*` tag:

- **Targets** (Q57.plataformas): `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl`
  (static, Q57.linux-static and #18), `aarch64-apple-darwin`, `x86_64-apple-darwin`,
  `x86_64-pc-windows-msvc`.
- **Archives**: `.tar.xz` for Linux and macOS, `.zip` for Windows, each with `s4`, the two
  licenses and both READMEs; a `.sha256` per archive and one `sha256.sum`.
- **Installers** (Q57.instaladores): the shell and PowerShell scripts `dist` generates, the
  Homebrew formula and the npm package.
- **Only `s4`** is in the archives; `helpers/example` and `helpers/grammar` are workspace
  members that `dist` must not package (`dist = false` in their `Cargo.toml`).

The musl binary's allocator: musl's `malloc` is slow under many threads. s4's global allocator
is the counter of M4 plan D31 over `System`; on musl the counter wraps another allocator (ripgrep
uses jemalloc on musl) if the measurement on the musl build shows s4 over 1.1 × the glibc build
on the M4 corpus. The counter keeps its contract either way.

### 8.2 Homebrew

The formula `dist` generates goes to the tap repository `neitanod/homebrew-tap` as `searchfor`
(Q57.brew): `brew install neitanod/tap/searchfor` installs `s4`. The release workflow pushes it
with a token stored as the secret `HOMEBREW_TAP_TOKEN`. CI on macOS checks the generated formula
with `brew install --formula ./searchfor.rb` against the job's own archive before any release.

### 8.3 npm

The package `searchfor` (Q57.npm), generated by `dist`: `npm install -g searchfor` puts `s4` on
the `PATH`, downloading the archive of the running platform from the GitHub Release and checking
its checksum. CI checks it with `npm pack` and a local install of the packed file. Published with
the secret `NPM_TOKEN`.

### 8.4 crates.io

The crate `searchfor` (Q57.crate): `cargo package` and `cargo publish --dry-run` pass in CI
(the package includes `formats/`, `grammars/index.json`, the licenses and the READMEs, and
excludes `tests/fixtures` above the crates.io size limit). `cargo install searchfor` builds `s4`
(spec §10: PCRE2 from the system when pkg-config finds it).

### 8.5 Signing

Every archive, installer script and checksum file of a release gets a GitHub artifact
attestation (`dist`'s `github-attestations = true`, Sigstore, keyless: the signature is bound to
the workflow and commit that built it; Q57.firma). The README tells how to verify one with
`gh attestation verify <file> --repo neitanod/searchfor`. No private key exists, so none can
leak. The M5a outline's «tested locally with a throwaway key» becomes: `dist plan` and
`dist build` run locally and in CI, and the attestation step is checked on the first release
candidate (§8.6, step 2) with `gh attestation verify` before `v1.0.0`.

### 8.6 Where the plan stops for Sebastián

Each of these is its own checkbox in the plan; the orchestrator posts what is ready, what the
step will publish, and what cannot be undone, blocks the Task, and goes on only with his OK:

1. Creating `neitanod/homebrew-tap` and the secrets `HOMEBREW_TAP_TOKEN` and `NPM_TOKEN` (only
   Sebastián can create the tokens).
2. Pushing the first tag (`v1.0.0-rc.1`): it creates a GitHub Release (marked pre-release) with
   archives and attestations. The release candidate publishes nothing to npm, Homebrew or
   crates.io (`dist`'s `publish-prereleases = false`).
3. Uploading the Grammar Helper binaries and pushing the `grammars/index.json` that lists them
   (§9).
4. Pushing `v1.0.0`: the GitHub Release, the Homebrew formula and the npm package.
5. `cargo publish` (a crate version can be yanked, never deleted).

## 9. Grammar Helper binaries

- **Which Grammars** (Q57.gramaticas): six to start, c, cpp, java, ruby, c-sharp and elixir;
  each is a Format Definition in `grammars/<lang>/<lang>.toml` and a crate under
  `helpers/grammar` features, built with `scripts/build-grammar-helper.sh` (M2).
- **Where**: a GitHub Release of its own, `grammars-v1`, separate from s4's releases
  (Q57.grammar-host); the workflow `grammars.yml` builds every Grammar for the five targets of
  §8.1 on a `grammars-v*` tag, writes each binary's SHA-256, and produces the new
  `grammars/index.json` as an artifact. The index committed to `main` is what
  `s4 --install-grammar` reads (spec §7), so a Grammar is installable only after that commit.
- `docs/grammar-index.schema.json` validates the index in CI; `s4 --install-grammar <lang>` is
  tested on each system against a `file://` index built by the test (no network).

## 10. External Processor guide

`docs/external-processors.md` (and `docs/external-processors.es.md`): what a Helper Processor
is, protocol v1 message by message (spec §4.4), the three modes and when to use each, the
Definition fields, failures and how s4 reports them, memory rules (8458), testing a helper with
`s4 --format <name>`, and two worked examples: a Python script in `oneshot` mode, and the same
Format done first with `--pre` (text conversion, no Trail) and then as a Helper Processor (the
Trail), to show when each fits. The examples are files under `docs/examples/` and a test runs
them (skipped where Python 3 is missing, with the reason).

## 11. v1.0

- `Cargo.toml` version `1.0.0`; `--version` prints it.
- README and README.es: install per channel and per system, verification of attestations, the
  supported systems, the platform differences of §4.2; the skill (`skills/searchfor/SKILL.md`)
  mentions Windows paths.
- `docs/rg-compat.md` notes any per-system deviation the CI found.

## 12. Acceptance criteria

New, numbered after spec §11 #33:

34. The suite passes on Linux x86-64, macOS arm64, macOS x86-64 and Windows x86-64 in CI with
    `SEARCHFOR_REQUIRE_REFERENCE=1`; every test skipped on a system is listed with its reason in
    `docs/platforms.md`, and none is skipped for a reason other than a missing system feature.
35. On each system, `s4 --no-docs --no-trail` prints what that system's rg 14.1.1 prints for the
    cases of `tests/rg_compat.rs` and `tests/rg_compat_m5.rs` (paths with `\` on Windows).
36. On Windows, a document reader past its memory limit gives the memory warning (the Job
    Object), and after Ctrl-C (a `GenerateConsoleCtrlEvent` test) no reader or helper outlives
    s4; on macOS, the readers' counter gives the warning (`m4_22`).
37. Dense files: the criterion of §6.3 in `tests/perf.rs`.
38. Per-file cost: the criterion of §7 in `scripts/bench-m5b.sh`.
39. `dist plan` lists the five targets and the four installers; `dist build` for the running
    system produces an archive whose `s4 --version` prints `1.0.0`; the Homebrew formula
    installs on macOS, the npm package installs from `npm pack`, `cargo publish --dry-run`
    passes — all in CI.
40. The Grammar Helpers of §9 build for the five targets; `s4 --install-grammar` installs one
    from a `file://` index on each system and its Trails match the compiled-in ones' form.
41. The examples of `docs/external-processors.md` run and give their documented output.
42. The M5a leftovers of §3 have their tests.

## Open questions

None. Questionnaire 57 is answered (its last question, «what am I forgetting», was left blank).
