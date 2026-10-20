# searchfor M5b (v1.0) Implementation Plan

> **For agentic workers:** implement each chunk in its own worktree, using
> `superpowers:subagent-driven-development` or `superpowers:executing-plans` and
> `test-driven-development`. Write the test, observe RED, implement, observe GREEN,
> then commit. Steps use checkbox (`- [ ]`) syntax. Read `CONTEXT.md`, the ADRs,
> `docs/specs/2026-10-20-searchfor-m5b-design.md` (the M5b spec), the v1 spec and
> `grill-respuestas.md`, including Questionnaire 57, before starting. The spec wins;
> report a disagreement before changing its meaning. Code, comments, this plan and
> commit messages are English; user documentation also has its Spanish copy.
> Commits and documentation carry no assistant attribution.

**Limits every chunk keeps**

- **Nothing is published without Sebastián's OK at that publication.** Each of
  M5b §8.6's five actions has a separate **Ask Sebastián** checkbox in C11.
  The chunk reaching a checkbox stops and reports the concrete artifacts, external
  changes and irreversible consequences. Approval of one action authorizes that
  action alone. Preparing workflows, packages and local artifacts is authorized.
- **Nothing is pushed until the orchestrator says so.** Commit workflows on the
  chunk branch. The orchestrator owns branch pushes, merges and the four-system
  CI stage after tanda 3. Tags and channel publication additionally require the
  corresponding Sebastián approval. Never dispatch the generated `release.yml`
  by hand (`workflow_dispatch`, which takes a `tag` input) with a real tag
  outside its publication gate: it publishes as if the tag had been pushed.
- **Tests have no network.** CI setup may download pinned tools and dependencies;
  the test processes use local fixtures and `file://` indexes. Installation smoke
  tests use the job's archives through a local server or local package overrides.
- **Tests never touch `~/.config` or the real platform data directory.** Set
  `SEARCHFOR_FORMATS_DIR`, `SEARCHFOR_GRAMMARS_DIR`, `SEARCHFOR_GRAMMAR_INDEX`
  under a `TempDir`, and remove `SEARCHFOR_CONFIG_PATH` and
  `RIPGREP_CONFIG_PATH`. A configuration test sets its own temporary config.
- **Low priority on the shared Linux machine.** Every cargo invocation, including
  nested builds and examples in this plan, runs as
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo …`. Run measured binaries and
  reference rg through `nice -n 19 ionice -c3`; record `uptime` beside each
  measurement. Native CI uses `CARGO_BUILD_JOBS=8`; macOS uses `nice -n 19`
  (there is no ionice), and Windows starts the process with BelowNormal priority.
- **ripgrep 14.1.1 pins stay.** Keep every ADR 0001 `=` pin and tree-sitter
  `=0.24.7`; check the lock after dependency additions. Updating the reference
  engine is outside this milestone.
- **Disk is 92% full.** Each chunk exports exactly one
  `CARGO_TARGET_DIR=<work-folder>/targets/c<N>`, inherited by nested cargo and
  dist. Delete that directory after recording its checks and handing off needed
  small artifacts. Never create a second build tree in the same chunk.
- **Allowed new crates, offline:** `windows-sys` 0.61 only under
  `[target.'cfg(windows)'.dependencies]`, features `Win32_Foundation`,
  `Win32_Security`, `Win32_System_JobObjects`, `Win32_System_Threading`,
  `Win32_System_Console`, `Win32_System_Pipes`, `Win32_Storage_FileSystem`,
  `Win32_System_IO`; helper-only `tree-sitter-c =0.23.4`,
  `tree-sitter-cpp =0.23.4`, `tree-sitter-java =0.23.5`, `tree-sitter-ruby
  =0.23.1`, `tree-sitter-c-sharp =0.23.1`, and `tree-sitter-elixir =0.3.5`, every
  one an ABI-14 grammar (`LANGUAGE_VERSION 14` in its `src/parser.c`, checked by the
  orchestrator on 20/10), which the `=0.24.7` runtime loads. The later C 0.24.2 and
  C# 0.23.5 are ABI 15 and are not used (D-ABI below). `tikv-jemallocator` 0.7 is conditional on §8.1's
  musl measurement. macOS kqueue uses the existing `libc`. Resolve from the
  local registry with `--offline`, then commit Cargo.lock with the manifest.
- `dist` 0.33.0 is already available at
  `/home/sebas/robotin/apps/mdgrep/.workspace/t-798-searchfor-m5b-v1-0-windows-macos-ci-y-releases-despues-m6/tools/dist`.
  Invoke this binary; pin the same version in distribution configuration.
- Linux checks and Windows GNU cross-checks are local evidence. Windows MSVC
  linking, console events, Job nesting and both macOS architectures require CI.
  So does everything that needs a musl C toolchain: this machine has the Rust
  musl target but no musl C compiler (`musl-gcc`, `x86_64-linux-musl-gcc`), PCRE2
  is compiled from C, and nothing is installed with sudo. The musl build and the
  musl-versus-glibc measurement of Task 11.2 run in the Linux CI job, whose runner
  installs `musl-tools`.
  Never describe cross-compilation as a native test pass.

**Goal:** ship a reviewable v1.0 that works on four native systems, bounds dense
Match retention and per-file overhead, and prepares all distribution channels.

**Architecture:** keep ripgrep's matching and walking crates. Put process creation,
parent death and cancellation behind one platform layer. Prepare each Processor
once and ask its ordered cursor for one shared Trail at a time; keep the model
adapter as a correctness oracle. Bound parallel output by spooling above 8 MiB,
preserving whole-file output locking, and let dist generate release machinery.

**Tech stack:** Rust 1.90, edition 2024; existing grep/search/termcolor/tree-sitter
crates, windows-sys 0.61, libc kqueue, GitHub Actions and dist 0.33.0. Grammar
Helpers remain protocol-v1 programs; no protocol version change is planned.

**Branch:** `t-798-searchfor-m5b`, base `f1a8936`, current task worktree. Chunk
branches are `t-798-searchfor-m5b-c<N>`. The orchestrator creates/merges them
following `using-git-worktrees` and `finishing-a-development-branch`. Paths and
line references below were checked against this base; locate edits by symbol
because preceding tasks move lines. This document contains proposed tests and
interfaces, rather than prebuilt implementation diffs. RED/GREEN outcomes below
are expected outcomes for implementers, not claims that this author ran them.

## Grill decisions this plan implements

| Source | Decision | Chunk / verification |
|---|---|---|
| 8375 | Every rg flag first, then Windows/macOS; English and Spanish docs | C1–C6/C8, four-system CI, C12 |
| #7 | External Processors in v1 | C2, C9 adapter, C12 examples |
| #8, 8454, ADR 0005 | Compiled-in v1 Grammars; extras as easy-to-install Helpers | C8, C11 |
| #18 | GitHub Releases, crates.io, Homebrew, npm | C11, five publication gates |
| #20 | Reserve/publish crate only with authorization | C11 gate 5; v1 supersedes the old v0.1 timing |
| 8365 | rg JSON plus fields, streaming consumers | C9, C10 differential and RSS |
| 8366 | `neitanod/searchfor` | C11, C12 |
| 8381 | Coloured console output | C3 ANSI eligibility, C9 bounded writer |
| 8385 | Search as fast as rg; generic output without structure; M6 later | C3, C9, C10; no M6 Formats added |
| 8458–8459 | Free per-file resources; pool/concurrent/oneshot | C2 lifecycle, C8 helper tests, C12 guide |
| 8612 | Linux release binary ≤ 23,000,000 bytes | C11 size gate |
| 8750 | Extract documents by default on a terminal and a pipe | C5, C12; benchmarks explicitly name `--no-docs` |
| 8837 | MCP is `s4 --mcp` | C6 |
| 8986 | `/cpd` on every functional advance: whenever something new works and the whole suite passes, merge to `main`, push, `cargo install --path . --locked --force` from `main`, copy the skill | The orchestrator, after each tanda that leaves s4 working (not a chunk's job) |
| Hilo 18/10 (grill-respuestas, «the cost of `--docs`») | Finding where the per-file cost over rg comes from is recommended for M5 | C3 |
| Q57.crate / Q57.npm | Both names are `searchfor`, npm unscoped | C11 |
| Q57.brew | `neitanod/homebrew-tap`, formula `searchfor` | C11 |
| Q57.plataformas | Five release targets; Windows arm64 later | C11 |
| Q57.linux-static | Static musl Linux archives | C11 musl probe |
| Q57.instaladores | Shell and PowerShell installers, no MSI | C11 |
| Q57.firma | Keyless GitHub artifact attestations | C11 RC verification, C12 instructions |
| Q57.codesign | Authenticode/notarization outside v1.0 | C11 distribution scope |
| Q57.grammar-host | Separate `grammars-v1` release | C11 gate 3 |
| Q57.gramaticas | c, cpp, java, ruby, c-sharp, elixir | C8, C11 matrix |
| Q57.fuzz-ci | Short on push, long nightly; unchanged main skips | C4 |
| Q57.cuando | Release after M5b with approval per channel | C11 gates, orchestrator CI |
| Q57.olvido | Left blank; creates no new requirement | No inferred feature |

8986 is Sebastián's message in the Hilo of M5a (Task 795): «/cpd en cada avance que ya queda
funcional si s4 no se rompe». It is not in `grill-respuestas.md` because it rules the workflow,
not the product.

## Design decisions

| Id | Decision | Why |
|---|---|---|
| D1 | Keep the prescribed three tandas and chunk IDs, including the missing C7 | Stable orchestration and separate ownership. C7 does not exist: the orchestrator numbered the chunks up front, and the Windows presentation work (colours, hyperlinks) ended up split between C1 and C3 with no chunk of its own |
| D2 | Use existing `common::rg_path()` in integration tests; repeat a tiny local resolver in unit tests/scripts that cannot import common | Existing helper at tests/common/mod.rs:159 already honors SEARCHFOR_RG; avoids a C4 startup dependency |
| D3 | Platform layer owns a managed Child and explicit SpawnSpec, not a Windows std::Command post-spawn patch | std spawn closes the primary thread handle; correct suspended assignment needs CreateProcessW and ResumeThread on the original thread |
| D4 | One non-inheritable process-wide Job plus a reader's retained nested Job | Kill-on-close covers descendants; committed-memory limits remain a second barrier beside the counter |
| D5 | Windows handler signals a prestarted coordinator and waits for cleanup | Reuses Supervisor ordering without taking lifecycle locks inside the console callback |
| D6 | kqueue registration is followed by a second parent-PID check | Closes the death-before-registration race; idle children also terminate |
| D7 | macOS search semantics always use readers, even with `--mmap`; Source/document mappings remain separate | Matches grep-searcher's macOS rule and keeps bytes_searched/binary conversion stable |
| D8 | Optimize only after paired measurements, allocator last; keep the counter unless measured overhead exceeds 1% | Avoids attributing per-file cost to unmeasured suspects or weakening document limits |
| D9 | Ordered `Prepared`/`Cursor` plus a default model adapter; `Rc<[Segment]>` stays worker-local | Object-safe Processor compatibility and bounded Match retention without cross-thread atomics |
| D10 | A prepared object owns its Outline/tree and creates a borrowing cursor | Avoids a self-referential owned Outline/TrailCursor struct |
| D11 | Keep protocol-v1 Helper Processors and object rows on explicit model adapters initially | External helpers receive a whole anchor list; changing their protocol is outside §6. Code-family dense tests use compiled-in Rust, not Helpers |
| D12 | Parallel ANSI/plain output spools at 8 MiB; console-API buffers retain termcolor metadata | Raw byte copies lose Wincon colours. Document this platform limitation; native console output remains correct |
| D13 | Differential oracle is an in-process test driver, with explicit stream/model selection | Tests every fixture/mode without introducing a user-facing switch or process-wide environment races |
| D14 | Dense preparation baseline is the same bytes with a marker matching once; report both R0 and Rone | Directly implements §6.3's density inequality; preparation remains separately visible |
| D15 | Stable tags publish by workflow; prerelease runs build/attest only, crate is a separate manual gate | §8.6 requires five distinct approvals in order |
| D16 | The CI downloads rg 14.1.1's official archives and checks these SHA-256, computed by the orchestrator on 20/10 and equal to the `.sha256` files the release publishes: `ripgrep-14.1.1-x86_64-unknown-linux-musl.tar.gz` 4cf9f2741e6c465ffdb7c26f38056a59e2a2544b51f7cc128ef28337eeae4d8e; `ripgrep-14.1.1-aarch64-apple-darwin.tar.gz` 24ad76777745fbff131c8fbc466742b011f925bfa4fffa2ded6def23b5b937be; `ripgrep-14.1.1-x86_64-apple-darwin.tar.gz` fc87e78f7cb3fea12d69072e7ef3b21509754717b746368fd40d88963630e2b3; `ripgrep-14.1.1-x86_64-pc-windows-msvc.zip` d0f534024c42afd6cb4d38907c25cd2b249b79bbe6cc1dbee8e3e37c2b6e25a1. URL: `https://github.com/BurntSushi/ripgrep/releases/download/14.1.1/<archive>` | A checksum written in the workflow catches a replaced archive; the tests themselves stay offline |
| D-ABI | The six Grammar Helper crates are pinned at ABI-14 releases: C =0.23.4, C++ =0.23.4, Java =0.23.5, Ruby =0.23.1, C# =0.23.1, Elixir =0.3.5 | tree-sitter stays `=0.24.7`, which loads ABI 13–14; C 0.24.2 and C# 0.23.5 are ABI 15 (checked in their `src/parser.c`, 20/10) |
| D17 | Grammar names allow internal hyphens, reject traversal; each helper build enables exactly one Grammar | `c-sharp` is required and current valid_name excludes it; multi-feature builds otherwise duplicate grammar() |
| D18 | Pure time ratios opt in with SEARCHFOR_PERF_TIMING=1; memory and bomb checks remain active on Linux CI | Shared runner jitter should not relax functional limits or RSS checks |
| D19 | dist 0.33.0 generates release YAML; inspect plan JSON and generated archives instead of hand-maintaining its internals | The installed generator is the source of truth for release job details |
| D20 | The native Windows launcher resolves `SpawnSpec.program` itself with std's algorithm (separator: as given; bare name: the child's `PATH`, the directory of `current_exe`, System32, the Windows directory, and the parent's `PATH` last; `.exe` is appended only when the name contains no `.`; never the cwd), passes the absolute path as `lpApplicationName` and returns `NotFound` otherwise | `CreateProcessW` with a null `lpApplicationName` searches the current directory first, which std never does. Without this, a `python.exe` left in the directory where the user runs `s4` would run as the `command` of any Helper Processor, reopening the hole that spec §4.2 closes for `--pre` |

## Chunk graph and file ownership

**Ownership adjustments:** C1 also owns the hyperlink expectation tests in
`src/output/flat.rs` and `src/output/tree.rs`, because Windows URL portability
belongs with its hyperlink work. C4 owns `src/docs/pdf.rs` and
`src/docs/sqlite.rs` reference-tool test setup. Move the rg resolver edits in
`src/cli/rg_flags.rs` to C8 and the one in
`src/processors/treesitter/mod.rs` to C10, whose chunks already own these files.
C4 owns existing `scripts/bench*.sh`; C3 owns only the new `bench-m5b.sh`.
Move `src/processors/external.rs` temporary cleanup from C8 to C10.
YAML/TOML prepared-factory scaffolds land in C9; ownership of
`src/processors/keypath/yaml.rs` and `toml.rs` transfers to C10 in tanda 3.
This explicit sequential handoff leaves C9's mod/json/collect files unchanged
in C10 and creates no same-tanda overlap.
C2 owns root Cargo.toml/Cargo.lock in tanda 1; C8 owns lock updates in tanda 2;
C11 owns them in tanda 3. No parallel chunks edit the same file. There are no
moves of chunk dependencies or tandas.

```text
Tanda 1, parallel:  C1     C2     C3     C4
                    |      |      |
Tanda 2, parallel:  C9 <---+ C3    C5 <- C2
                    ^             C6 <- C2
                    C1            C8 <- C2
Tanda 3, parallel:  C10 <- C9
                   C11 <- C2,C4,C5,C8
                   C12 <- C8
Then: orchestrator's CI on the four systems, fixes in batches, release gates.
```

The arrows are dependency declarations, not merge order within a tanda.
Tanda 2 starts after all of tanda 1 merges; tanda 3 after all of tanda 2.
When integrating tanda 2, the orchestrator deletes `Supervisor::launch_reader`
from `src/helper/host.rs` in the merge commit, after C5 and C6 have both migrated
their calls to `launch_managed` (C2 migrated its own host.rs test in tanda 1); no
chunk owns that deletion.
C11 stages publication-ready artifacts, then waits for the orchestrator's final
CI and C12 docs before any tag. C12 amends the old spec after implementing its
new tests; it does not change this source-of-truth spec.

### Common command convention

In each chunk's own worktree, run once (replace N with its chunk number):

```bash
export CARGO_TARGET_DIR="$(dirname "$PWD")/targets/cN"
export SEARCHFOR_RG="${SEARCHFOR_RG:-/usr/bin/rg}"
export SEARCHFOR_REQUIRE_REFERENCE=1
```

Commands shown below are Linux local commands. Native CI translations keep
`--offline` once setup has fetched the lock's crates, `--locked`, package,
features and test filters. All child cargo calls inherit CARGO_TARGET_DIR.
A test-selection command must report at least one executed test; zero selected
is a failure of the procedure. The whole suite includes existing ignored
robustness/perf checks through explicitly selected invocations where indicated.

After each chunk's last GREEN, run:

```bash
CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo fmt --all --check
CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo clippy --offline --workspace --all-targets --locked -- -D warnings
CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo check --offline --target x86_64-pc-windows-gnu --all-targets --locked
```

At intermediate tanda-1 states other chunks still own compile failures in
Windows tests. Record exact known failures; repeat the cross-check after the
tanda merges and require GREEN there. Keep the cross-check in C5/C6/C8/C9/C10
as well; native-only APIs still need their named CI jobs.

## Chunk 1: M5a leftovers and output tests

Owns `src/output/mod.rs`, `src/output/hyperlink.rs`, hyperlink test sections of
`flat.rs`/`tree.rs`, `tests/cli_m5.rs` and only its snapshots.

### Task 1.1: regression tests for all three expand findings

**Files:** modify `tests/cli_m5.rs` (append tests below),
`src/output/mod.rs:1292` (`SectionBlock`, `section_rows`, `shown_rows`,
`SectionWalk`). Snapshot changes are limited to the three corrected regressions.
Public API: unchanged; row ownership stays the source offset of a Match's start.

- [ ] **Step 1 — RED tests:** append this complete test code, using existing `ok`.

```rust
#[test]
fn m5b_expand_replacement_continuations_belong_to_start_section() {
    let fx = Fixture::new();
    std::fs::write(fx.file("a.md"), "# A\nneedle\n# B\nlast\nneedle\n").unwrap();
    let out = ok(&fx, &[
        "--tree", "--expand", "-U", "-r", "X",
        "needle\\n# B\\nlast|needle", "a.md",
    ]);
    assert_eq!(out.matches("X").count(), 2, "{out}");
    assert_eq!(out.matches("last").count(), 0, "{out}");
    // The replacement consumes the continuation; Section B must not print it again.
    assert_eq!(out.lines().filter(|l| l.ends_with(":X")).count(), 2, "{out}");
}

#[test]
fn m5b_expand_vimgrep_newline_replacement_covers_the_source_item() {
    let fx = Fixture::new();
    std::fs::write(fx.file("a.md"), "# A\nprefix needle suffix\n").unwrap();
    let out = ok(&fx, &[
        "--flat", "--expand", "--vimgrep", "-r", "X\nY",
        "needle", "a.md",
    ]);
    assert_eq!(out.matches("prefix").count(), 1, "{out}");
    // --vimgrep keeps the first replacement line; the rest of the original
    // item is covered, so suffix must not reappear as source context.
    assert_eq!(out.matches("suffix").count(), 0, "{out}");
    assert!(out.contains("prefix X"), "{out}");
    assert!(!out.contains("Y"), "{out}");
    assert!(!out.contains("needle"), "{out}");
}

#[test]
fn m5b_expand_only_matching_keeps_a_row_after_a_gap_outside_section() {
    let fx = Fixture::new();
    std::fs::write(fx.file("a.md"), "# A\nneedle\n# B\n\nlast\n").unwrap();
    let out = ok(&fx, &[
        "--tree", "--expand", "-U", "-o", "needle\\n# B\\n\\nlast", "a.md",
    ]);
    assert_eq!(out.matches("needle").count(), 1, "{out}");
    assert_eq!(out.matches("last").count(), 1, "{out}");
    assert!(out.lines().any(|l| l.ends_with("5:last")), "{out}");
    assert!(out.lines().any(|l| l.ends_with("4-")), "{out}");
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test cli_m5 m5b_expand -- --nocapture`.
  Expected RED: duplicate continuation/context, or missing `last`/gap assertion.
  Confirm each of the three fails independently. If fixture formatting masks a
  finding, reduce the fixture against `SectionWalk` and retain the same contract;
  record actual RED, rather than weakening an assertion to get a red result.
- [ ] **Step 3:** in `SectionWalk`, assign a Match's complete row group to the
  Section containing its start. In `shown_rows`, distinguish covered source
  bytes from replacement display rows; newline replacement covers the complete
  original item. After the Section boundary, continue walking every row of the
  same Match, emitting empty-line gaps as context. Keep per-submatch coordinates,
  `-o`, null-data and replacement highlights unchanged. Avoid rescanning all
  Sections per Match (m5_22's linearity remains required).
- [ ] **Step 4:** rerun the command: three PASS. Then run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test cli_m5`.
  Expected GREEN including m5_22; compare snapshots before accepting any update.
- [ ] **Step 5:** commit these files with
  `git commit -m "fix: preserve expand row ownership and continuations"`.

### Task 1.2: portable MCP fixture and portable versus Unix-only mmap checks

**Files:** modify `tests/cli_m5.rs` (`m5_11`, `m5_27`, direct rg calls).
Use existing `common::rg_path() -> PathBuf`; no shared-file dependency on C4.

- [ ] **Step 1:** replace m5_11's executable shell configuration trap with a
  config containing `--generate=man` and `--regexp=ABSENT_CONFIG_SENTINEL`.
  Preserve its checks for missing paths, limits, bytes, names and Sections.
  Add the complete portable regression below.

```rust
#[test]
fn m5b_mcp_config_trap_needs_no_executable_script() {
    let fx = Fixture::new();
    let config = fx.file("config");
    std::fs::write(&config, "--generate=man\n--regexp=ABSENT_CONFIG_SENTINEL\n").unwrap();
    let mut cmd = fx.s4(["--mcp"]);
    cmd.env("SEARCHFOR_CONFIG_PATH", config)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped());
    let mut child = cmd.spawn().unwrap();
    let call = json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
        "params":{"name":"search","arguments":{
            "pattern":"needle-pkg","paths":["md/guide.md"]}}});
    writeln!(child.stdin.take().unwrap(), "{call}").unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(result.status.success());
    let reply: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(reply["id"], 1);
    assert_eq!(reply["result"]["structuredContent"]["matches"][0]["line"], 12);
}

#[test]
fn m5b_no_mmap_keeps_output_portable() {
    let fx = Fixture::new();
    let bytes = format!("# Head\n{}needle\n", "x\n".repeat(4_200_000));
    std::fs::write(fx.file("large.md"), bytes).unwrap();
    for mode in ["--flat", "--json"] {
        let a = run(&mut fx.s4([mode, "needle", "large.md"]));
        let b = run(&mut fx.s4([mode, "--no-mmap", "needle", "large.md"]));
        assert_eq!(code(&a), code(&b));
        if mode == "--json" {
            assert_eq!(common::mask_json_times(&stdout(&a)),
                       common::mask_json_times(&stdout(&b)));
        } else {
            assert_eq!(a.stdout, b.stdout);
        }
    }
}
```

- [ ] **Step 2:** run the cli_m5 filter above and the Windows GNU all-targets
  cross-check. Expected original RED: unguarded PermissionsExt in m5_11 and
  Unix-only process/mapping APIs in m5_27 prevent compilation. The portable
  behaviour tests may already pass Linux; record them as preservation checks.
- [ ] **Step 3:** remove PermissionsExt entirely from m5_11. Split m5_27 into
  its portable observable output test and its Linux `/proc/PID/maps` test with
  `#[cfg(target_os = "linux")]` and an explanatory comment. Keep Unix signal
  checks under `cfg(unix)` and verify macOS supports each syscall actually used.
  Replace every fixed rg command in this file with `common::rg_path()`.
- [ ] **Step 4:** cli_m5 GREEN locally; Windows compile errors in this file gone.
  CI: both portable cases run on all four systems; only the `/proc` case is
  absent outside Linux and appears in `docs/platforms.md` (C4 inventory).
- [ ] **Step 5:** commit with
  `git commit -m "test: make M5 output and MCP fixtures portable"`.

### Task 1.3: hyperlink expectations use rg's URL rules

**Files:** modify `src/output/hyperlink.rs:130` (`encode_path`, tests),
`src/output/mod.rs` hyperlink test, `src/output/flat.rs:509`,
`src/output/tree.rs:410`, `tests/cli_m5.rs` (`m5_14`).
New internal interface, to separate lexical Windows conversion from I/O:

```rust
// src/output/hyperlink.rs, private; unit tests in this module can call it.
fn windows_url_path(canonical: &str) -> Option<Vec<u8>>;
```

- [ ] **Step 1:** add the following complete unit test.

```rust
#[test]
fn windows_urls_keep_drive_and_unc_prefixes() {
    assert_eq!(windows_url_path(r"\\?\C:\a b\f.md").unwrap(), b"/C:/a%20b/f.md");
    assert_eq!(windows_url_path(r"\\?\UNC\server\share\a b.md").unwrap(),
               b"//server/share/a%20b.md");
    assert_eq!(windows_url_path(r"\\?\D:\percent%\x#y").unwrap(),
               b"/D:/percent%25/x%23y");
    assert!(windows_url_path("relative/file").is_none());
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --lib windows_urls_keep_drive_and_unc_prefixes`.
  Expected RED: missing function. Also retain native URL test expected RED on
  Windows from `canonicalize().display()` including the verbatim prefix.
- [ ] **Step 3:** extract the current Windows prefix conversion into the lexical
  helper and use its byte encoder. Change all expectations to independently
  derive `/C:/…` or `//server/share/…`, then percent-encode the exact bytes that
  rg encodes (including `%`, `#`, space). Never format `canonical.display()` as
  a URL. In cli_m5 compare the OSC-8 target against `fx.rg` with identical
  `--hyperlink-format`/`--color=ansi`, stripping only visible text; for tree's
  header compare the target at line 1. Use `--color=ansi` in byte assertions.
- [ ] **Step 4:** same unit command PASS; run cli_m5 and output unit tests.
  Linux verifies lexical drive/UNC conversion; native Windows CI verifies actual
  canonical paths and console/ANSI distinction. An actual UNC share is optional;
  lexical UNC is mandatory and requires no network share.
- [ ] **Step 5:** commit with
  `git commit -m "test: compare hyperlinks using native ripgrep URL rules"`.

## Chunk 2: Process layer

Owns `src/platform/**` (new), `src/helper/**`, `src/lib.rs`, root Cargo.toml
and lock, `tests/helpers.rs`, and `helpers/example/**`. Its API is the contract
C5, C6 and C8 consume; publish the API commit before those chunks start.

### Task 2.1: managed process contract and Windows suspended assignment

**Files:** create `src/platform/mod.rs`, `src/platform/windows.rs`,
`src/platform/unix.rs`; modify `src/lib.rs:1` (module export),
`Cargo.toml:92` and Cargo.lock (windows-sys); modify `src/helper/host.rs:27`
(`CommandLauncher`, `Child` wrapper, `reap`; add `Supervisor::launch_managed`
beside `launch_reader`, which stays untouched in tanda 1).
New interfaces (public inside the library; no CLI contract added):

```rust
// src/platform/mod.rs
use std::{ffi::OsString, io, path::PathBuf, process::ExitStatus, time::Duration};
pub enum Input { Null, Piped, File(std::fs::File), Inherit }
pub enum Output { Null, Piped, Inherit }
pub struct SpawnSpec {
    pub program: OsString,
    pub args: Vec<OsString>,
    pub cwd: Option<PathBuf>,
    // None removes a variable; unset entries inherit the parent's environment.
    pub env: Vec<(OsString, Option<OsString>)>,
    pub stdin: Input,
    pub stdout: Output,
    pub stderr: Output,
}
impl SpawnSpec {
    pub fn new(program: impl Into<OsString>) -> Self;
}
pub struct Limits { pub process_memory_bytes: Option<usize> }
pub struct Child {
    pub stdin: Option<PipeWriter>,
    pub stdout: Option<PipeReader>,
    pub stderr: Option<PipeReader>,
    // Owns the process handle and nested Job for the whole worker lifetime.
}
pub struct PipeReader { /* owned read end; impl Read + Send */ }
pub struct PipeWriter { /* owned write end; impl Write + Send */ }
pub struct WaitToken { /* independently owned process wait capability */ }
pub fn spawn(spec: SpawnSpec, limits: Limits) -> io::Result<Child>;
impl Child {
    pub fn id(&self) -> u32;
    pub fn kill(&mut self) -> io::Result<()>;
    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>>;
    pub fn wait(&mut self) -> io::Result<ExitStatus>;
    pub fn wait_token(&self) -> io::Result<WaitToken>;
}
impl WaitToken {
    // Wait without owning the Supervisor's Child lock; does not reap.
    pub fn wait(&self, timeout: Option<Duration>) -> io::Result<bool>;
}
```

`Limits { process_memory_bytes: None }` is the common Job only. Unix ignores
this optional Job barrier because the reader separately sets rlimits. Preserve
Unix inherited process groups. `Child::Drop` closes handles; every owner retains
explicit kill/reap obligations. The Supervisor stores
`Weak<Mutex<platform::Child>>`; C5/C6 use `Arc<Mutex<platform::Child>>`.

- [ ] **Step 1:** add the complete tests below to `src/platform/mod.rs`.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    #[ignore = "child entry point invoked by managed_spawn_preserves_argv"]
    fn child_echo_arguments() {
        if let Some(p) = std::env::var_os("SEARCHFOR_PLATFORM_ECHO") {
            std::fs::write(p, std::env::var_os("SEARCHFOR_PLATFORM_VALUE").unwrap()
                .to_string_lossy().as_bytes()).unwrap();
        }
    }

    #[test]
    fn managed_spawn_preserves_argv_and_pipes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("echo file");
        let mut s = SpawnSpec::new(std::env::current_exe().unwrap().into_os_string());
        s.args = ["--exact", "platform::tests::child_echo_arguments", "--ignored",
                  "--nocapture"].into_iter().map(Into::into).collect();
        s.env = vec![
            ("SEARCHFOR_PLATFORM_ECHO".into(), Some(path.clone().into_os_string())),
            ("SEARCHFOR_PLATFORM_VALUE".into(), Some("a b\\\"é%".into())),
        ];
        s.stdout = Output::Piped;
        s.stderr = Output::Piped;
        let mut c = spawn(s, Limits { process_memory_bytes: None }).unwrap();
        let token = c.wait_token().unwrap();
        let mut out = String::new();
        c.stdout.take().unwrap().read_to_string(&mut out).unwrap();
        assert!(token.wait(Some(Duration::from_secs(10))).unwrap());
        assert!(c.wait().unwrap().success(), "{out}");
        assert_eq!(std::fs::read_to_string(path).unwrap(), "a b\\\"é%");
    }

    #[test]
    fn spawn_failure_returns_error_without_a_live_child() {
        let s = SpawnSpec::new("searchfor-guaranteed-missing-platform-program");
        assert!(spawn(s, Limits { process_memory_bytes: None }).is_err());
    }
}
```

The Windows module also gets this complete lexical quoting test; its helper
is private `fn quote_windows_arg(arg: &[u16]) -> Vec<u16>` and is used by the
CreateProcessW command-line builder, not a separate test implementation.

```rust
#[test]
fn windows_argument_quoting_covers_empty_quotes_and_trailing_slashes() {
    let q = |arg: &str| String::from_utf16(&quote_windows_arg(
        &arg.encode_utf16().collect::<Vec<_>>())).unwrap();
    // The builder always quotes each argument, including simple ones.
    assert_eq!(q(""), "\"\"");
    assert_eq!(q("a b"), "\"a b\"");
    assert_eq!(q("a\"b"), "\"a\\\"b\"");
    assert_eq!(q("a b\\"), "\"a b\\\\\"");
    assert_eq!(q("plain"), "\"plain\"");
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --lib platform::tests`.
  Expected RED: missing module/types/functions. The ignored child is invoked
  explicitly by the parent; the normal run must execute both parent tests. On
  Windows, the RED of `windows_bare_program_name_does_not_search_cwd` is observed
  by removing the resolution (leaving `lpApplicationName` null): the probe then
  finds the fake in its cwd and the parent test fails.
- [ ] **Step 3:** implement Unix delegation and the native Windows launcher.
  Build UTF-16 program/argv/environment using Windows quoting compatible with
  Rust's Command: double backslashes before quotes and before a closing quote;
  preserve non-Unicode OsStrings, inherited env removals, and cwd. Add a lexical
  quote unit table for empty arguments, trailing backslash, quotes and spaces.
  Create pipes with inheritable child ends and non-inheritable parent ends;
  use STARTUPINFOEXW's handle list so concurrent starts inherit only their own
  three handles. Allocate its attribute storage aligned and release it on error.
  Create every Job with null security attributes (non-inheritable) and set the
  kill-on-close flag before spawning the first child. Keep the root Job in a
  OnceLock-owned RAII object. A reader Job adds PROCESS_MEMORY to its limits.
  Check every Win32 call and return last_os_error on failure.

The critical ordering is exactly this; `Created` owns PROCESS_INFORMATION and
all child handles until ownership is transferred to Child:

```rust
// src/platform/windows.rs: algorithm inside the native launcher.
let flags = CREATE_SUSPENDED | EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT;
let mut created = create_process(&spec, flags, &inherited_handles)?;
// created has a Drop rollback that TerminateProcess + waits unless committed.
assign(root_job.handle(), created.process())?;
if let Some(bytes) = limits.process_memory_bytes {
    let reader_job = memory_job(bytes)?;
    assign(reader_job.handle(), created.process())?;
    created.retain_reader_job(reader_job);
}
if unsafe { ResumeThread(created.thread()) } == u32::MAX {
    return Err(std::io::Error::last_os_error());
}
created.into_child() // closes hThread; retains hProcess and the reader Job
```

Never use unstable `ChildExt::main_thread_handle`, resume before assignment,
or replace the strict sequence with a post-spawn handshake. Rollback kills and
waits even when assignment to the second Job fails. Use DuplicateHandle for
WaitToken, WaitForSingleObject outside locks, and GetExitCodeProcess afterward.
Unbounded wait occurs only after termination or on a separate wait token.
Migrate CommandLauncher and its Control implementation in this task; retain
its three writer/reader threads, timeouts and failure semantics.

`Supervisor::launch_reader` takes `FnOnce() -> io::Result<std::process::Child>`
and its two callers are files C2 does not own: `src/mcp.rs` (C6) and
`src/docs/isolated.rs` (C5). C2 therefore leaves `launch_reader` and those two
callers unchanged in tanda 1 and adds `Supervisor::launch_managed` beside it,
taking `FnOnce() -> io::Result<platform::Child>` and storing the
`Weak<Mutex<platform::Child>>`. C2 also migrates, in tanda 1, its own third
caller: the host.rs unit test
`cancel_kills_and_reaps_every_live_process_and_refuses_new_ones` (it calls
`launch_reader(|| Command::new("sleep").arg("30").spawn())` at
`src/helper/host.rs:1824`) now goes through `launch_managed`. Both launchers
coexist, so the tanda-1 workspace compiles and C2 can give its workspace GREEN.
In tanda 2, C5 and C6 each migrate their own call to `launch_managed` and delete
nothing. The orchestrator deletes `launch_reader` in the merge commit of tanda 2,
once no caller remains (an unused `pub(crate) fn` is `dead_code` and fails
`clippy -D warnings`).

The native launcher never lets `CreateProcessW` search for the program. It
resolves `SpawnSpec.program` with the algorithm of std's `resolve_exe` /
`search_paths` and passes the resulting absolute path as `lpApplicationName`:
if `program` contains a path separator it is used as given; otherwise it is
searched, in std's order, in the child's `PATH` (from `SpawnSpec.env`), the
directory of `current_exe`, System32, the Windows directory, and the parent's
`PATH` last, and never in the current directory. As in std, `.exe` is appended
to a bare name only when the name contains no `.`. A program that is not found
returns `io::ErrorKind::NotFound` without creating a process. The implicit
search of `CreateProcessW` with a null `lpApplicationName` walks the current
directory of the **parent** process, not the `lpCurrentDirectory` given to the
child, so the regression runs in its own process whose current directory holds
the fake, like Task 8.1's `--pre` probe. Windows adds both tests to
`src/platform/windows.rs`:

```rust
#[test]
#[ignore = "isolated child for Windows spawn resolution"]
fn windows_spawn_resolution_probe() {
    let mut s = SpawnSpec::new("s4-fake-helper");
    // Harmless if it ever ran: the fake is a copy of cmd.exe.
    s.args = ["/c", "exit", "7"].into_iter().map(Into::into).collect();
    // Resolving the name in the process cwd would return Ok(child) and run it.
    let err = spawn(s, Limits { process_memory_bytes: None }).err().unwrap();
    assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
}

#[test]
fn windows_bare_program_name_does_not_search_cwd() {
    let dir = tempfile::tempdir().unwrap();
    // A runnable executable named like a bare program, only in the cwd.
    let cmd = std::path::Path::new(&std::env::var_os("SystemRoot").unwrap())
        .join("System32").join("cmd.exe");
    std::fs::copy(cmd, dir.path().join("s4-fake-helper.exe")).unwrap();
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "platform::windows::tests::windows_spawn_resolution_probe",
               "--ignored"])
        .current_dir(dir.path())
        .env("PATH", dir.path().join("empty-path"))
        .status().unwrap();
    assert!(status.success());
}
```

The probe's `PATH` names no directory holding `s4-fake-helper.exe`, which sits
only in the probe's working directory, and the fake is never the test binary
(that would spawn the whole suite recursively). Even if it ran, it would exit
with code 7 and the probe would still fail on `NotFound`, never pass on the
exit code. The tests are Windows-only; Linux cross-checks compile them.

- [ ] **Step 4:** rerun unit tests and
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --workspace --locked --test helpers`.
  Linux GREEN preserves h01–h17 behaviour. Cross-check all targets. Windows CI
  adds actual start, nested Job, invalid-program and rollback execution; verify
  all handles close under repeated spawn/exit, not only first launch.
- [ ] **Step 5:** commit with
  `git commit -m "feat: add managed platform processes and suspended Windows Jobs"`.

### Task 2.2: parent-death API and race-free macOS kqueue watcher

**Files:** create `src/platform/macos.rs`; modify `src/platform/mod.rs`,
`src/platform/unix.rs`, `src/lib.rs:22` (early internal parent initialization).
The existing Linux reader parent variable remains `SEARCHFOR_READER_OF`.
MCP uses a separate internal environment variable so it is initialized before CLI.

```rust
// src/platform/mod.rs
pub const PROCESS_PARENT_ENV: &str = "SEARCHFOR_PROCESS_PARENT";
// Register once in the child, before starting background work.
pub fn orphan_dies(expected_parent: u32) -> std::io::Result<()>;
pub fn initialize_parent_watch() -> std::io::Result<()>;
```

- [ ] **Step 1:** append these complete macOS-only unit tests in macos.rs.
  The child entry is an ignored test, isolated from the parent test process.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    #[ignore = "child entry for a parent mismatch; exits this process"]
    fn orphan_probe() {
        crate::platform::orphan_dies(u32::MAX).unwrap();
        std::thread::sleep(Duration::from_secs(30));
        panic!("an orphan survived registration");
    }

    #[test]
    fn already_dead_parent_is_detected_before_work() {
        let began = Instant::now();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "platform::macos::tests::orphan_probe", "--ignored"])
            .status().unwrap();
        assert!(!status.success());
        assert!(began.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn watching_the_actual_parent_does_not_end_this_child() {
        let parent = unsafe { libc::getppid() } as u32;
        crate::platform::orphan_dies(parent).unwrap();
        std::thread::sleep(Duration::from_millis(20));
    }
}
```

- [ ] **Step 2:** local Linux check verifies cfg isolation; native macOS command
  `CARGO_BUILD_JOBS=8 nice -n 19 cargo test --offline --locked --lib platform::macos::tests`
  (native macOS, no ionice) is RED before the
  API/watcher exists (or after undoing its body). A Linux pass is not its RED.
- [ ] **Step 3:** Linux retains PR_SET_PDEATHSIG + getppid comparison. macOS
  creates kqueue, registers EVFILT_PROC/NOTE_EXIT with EV_ADD|EV_ONESHOT, then
  rechecks getppid; ESRCH, an already queued NOTE_EXIT or a parent mismatch all
  exit immediately. An unexpected registration failure is an error, not silent
  loss of the guarantee. The watcher owns and closes the descriptor. Keep the
  65-second active-document watchdog independent. Windows uses Job membership;
  the API succeeds there without a polling thread.

```rust
// macOS registration core; check both kevent's return and EV_ERROR.data.
let mut event: libc::kevent = unsafe { std::mem::zeroed() };
event.ident = expected_parent as libc::uintptr_t;
event.filter = libc::EVFILT_PROC;
event.flags = libc::EV_ADD | libc::EV_ONESHOT;
event.fflags = libc::NOTE_EXIT;
let rc = unsafe { libc::kevent(kq, &event, 1, std::ptr::null_mut(), 0, std::ptr::null()) };
if rc < 0 { return parent_watch_error_or_exit(std::io::Error::last_os_error()); }
if unsafe { libc::getppid() } as u32 != expected_parent { std::process::exit(2); }
// Move kq into a watcher thread; loop on EINTR, exit immediately on NOTE_EXIT.
```

For MCP starts on macOS set PROCESS_PARENT_ENV in SpawnSpec; initialize the
watch before app::run. Readers call orphan_dies with SEARCHFOR_READER_OF in C5.
Helper lifecycle on Unix retains EOF and D42; do not change ADR 0005's SIGKILL
contract for user-written helpers.

- [ ] **Step 4:** Linux unit/workspace check GREEN; macOS arm64 and Intel CI
  both run the tests and C5/C6's real orphan tests. Include an idle reader/search
  case: the active watchdog alone would leave an orphan alive.
- [ ] **Step 5:** commit with
  `git commit -m "feat: watch macOS process parents with kqueue"`.

### Task 2.3: Windows console cancellation and portable helper test controls

**Files:** modify `src/helper/host.rs:824` (`cancel::watch`, `STOPPED`, `cancel`),
`src/platform/windows.rs` (console bridge), `tests/helpers.rs` (`alive`, h03b,
new tests), `helpers/example/src/main.rs:295` (`start_up`, child test modes),
`helpers/example/tests/protocol.rs` (Linux RSS reason).

```rust
// src/platform/mod.rs; callback runs on the coordinator, outside Win32 handler.
pub fn watch_console(stopped: &'static std::sync::atomic::AtomicBool, cancel: fn())
    -> std::io::Result<()>;
// helpers/example CLI test controls, parsed before normal protocol service:
// --test-hold-pipes: sleep 30 s, inherited output, no protocol handling.
// --test-pre-upper PATH: uppercase ASCII bytes to stdout, exit 0.
// --test-pre-fail PATH: stderr "oops", exit 3.
// When argv[0] stem is s4-test-pre-upper or s4-test-pre-fail, use that
// same mode without a switch (test copies binary to the sibling filename).
// Normal protocol mode also accepts SEARCHFOR_EXAMPLE_WAIT_READY (JSON log path)
// and SEARCHFOR_EXAMPLE_WAIT_RELEASE (file to wait for before answering trails).
// Log {search_pid, helper_pid}; search_pid comes from PROCESS_PARENT_ENV,
// which CommandLauncher sets to its own PID on every helper launch.
// Only this test wait knob installs orphan_dies(search_pid) in the helper.
// --test-console-driver S4 FIXTURE LOG RESULT: own isolated Windows console;
// wait for logged helper/reader readiness, GenerateConsoleCtrlEvent(CTRL_C_EVENT, 0),
// wait for s4, write {"status":u32,"alive":[u32,...]} to RESULT.
```

- [ ] **Step 1:** replace shell `sleep` descendants with a self-invocation of
  `--test-hold-pipes`, keeping SEARCHFOR_EXAMPLE_FORK as the existing test knob.
  Parse --test-pre-upper/--test-pre-fail before normal service, so C5/C8
  can copy the binary to s4-test-pre-upper[.exe] / s4-test-pre-fail[.exe];
  argv[0] stem selects the mode when --pre takes only one executable. Add normal-protocol WAIT_READY/WAIT_RELEASE controls:
  record both PIDs atomically at first trails request, watch the parent, then
  wait on the release file with a 30-second deadline. CommandLauncher writes
  PROCESS_PARENT_ENV=current PID into its child SpawnSpec. These controls are
  test-only example features; normal helper behaviour stays unchanged.
  Normalize only paths used for knob matching (`\\` to `/`), never emitted paths.
  Add this complete integration test to tests/helpers.rs:

```rust
#[test]
#[cfg(windows)]
fn m5b_console_ctrl_c_kills_and_reaps_managed_processes() {
    let fx = Fixture::new();
    ini(&fx, "pool", 2000);
    generate(&fx, 8);
    let result = fx.file("console-result.json");
    let out = Command::new(example_helper())
        .arg("--test-console-driver")
        .arg(assert_cmd::cargo::cargo_bin("s4"))
        .arg(fx.path()).arg(fx.file("helper.log")).arg(&result)
        .env("SEARCHFOR_FORMATS_DIR", fx.formats.path())
        .env("SEARCHFOR_GRAMMARS_DIR", fx.grammars.path())
        .env("SEARCHFOR_GRAMMAR_INDEX", fx.no_index())
        .env_remove("SEARCHFOR_CONFIG_PATH")
        .env_remove("RIPGREP_CONFIG_PATH")
        .output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(result).unwrap()).unwrap();
    assert_eq!(v["status"].as_u64(), Some(0xC000013A));
    assert_eq!(v["alive"], serde_json::json!([]));
    assert!(pids(&fx.file("helper.log")).len() >= 2);
}

#[test]
fn m5b_alive_checks_a_live_and_a_reaped_child() {
    let mut c = Command::new(example_helper()).arg("--test-hold-pipes")
        .stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
    let pid = c.id();
    assert!(alive(pid));
    c.kill().unwrap();
    c.wait().unwrap();
    assert!(!alive(pid));
}
```

Also add this complete portable mode test to tests/helpers.rs:

```rust
#[test]
fn m5b_native_pre_fixtures_work_without_shell_scripts() {
    let fx = Fixture::new();
    let input = fx.file("input");
    std::fs::write(&input, b"needle\n").unwrap();
    for (stem, want_code, want_out, want_err) in [
        ("s4-test-pre-upper", 0, &b"NEEDLE\n"[..], &b""[..]),
        ("s4-test-pre-fail", 3, &b""[..], &b"oops\n"[..]),
    ] {
        let path = fx.file(&format!("{stem}{}", std::env::consts::EXE_SUFFIX));
        std::fs::copy(example_helper(), &path).unwrap();
        let out = Command::new(path).arg(&input).output().unwrap();
        assert_eq!(out.status.code(), Some(want_code));
        assert_eq!(out.stdout, want_out);
        assert_eq!(out.stderr, want_err);
    }
}
```

- [ ] **Step 2:** run locally:
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test helpers m5b_ -- --nocapture`
  and
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test helpers h03b -- --nocapture`. Expected RED for the new
  helper mode/alive portability. Windows native RED is missing console cleanup
  or status other than 0xC000013A. Cross-check catches unguarded Unix imports.
- [ ] **Step 3:** implement alive by Linux `/proc` excluding zombies, macOS
  kill(pid,0) plus a wait/reap-aware observation for owned children, and Windows
  OpenProcess(SYNCHRONIZE|QUERY_LIMITED_INFORMATION) + zero-time wait. Close all
  observation handles. Keep readiness polling bounded and reap via RAII cleanup
  on assertion failure. h03b runs unmodified behaviour on every system after
  changing its `/gen/` knob comparison to normalized path components.

cancel::watch passes &STOPPED to platform::watch_console. The console callback
accepts CTRL_C_EVENT and CTRL_BREAK_EVENT, sets that same STOPPED atomic,
and signals an already created event. It never spawns a thread or holds a
Supervisor lock. The coordinator executes GLOBAL.cancel(), which stops starts,
kills and reaps under the existing lifecycle rules, then calls ExitProcess.
The callback waits for that completion so returning TRUE cannot race a normal
exit with code 0. The wait is bounded by `CLEANUP_WAIT_MS: u32 = 2_000`
(milliseconds, as `WaitForSingleObject` takes it), which is `SHUTDOWN_WAIT`
(`src/helper/host.rs:13`, 1 s) plus a margin: `cancel()` applies the per-copy
grace of `SHUTDOWN_WAIT` to every helper copy in parallel scoped threads, so the
normal total is about 1 s, while the readers' `reap()` kills and waits without a
bound. When the wait expires, because a helper survives `TerminateProcess`
or a lock is held, the callback calls `ExitProcess(0xC000013A)` anyway, and the
kill-on-close root Job terminates whatever remains. Repeated events share the
same idempotent cleanup, so a second Ctrl-C cannot wait longer than the first.
rg always exits on Ctrl-C and so must s4.

```rust
// Windows callback core. EVENT and a pointer to the Supervisor STOPPED atomic
// are installed before SetConsoleCtrlHandler; the callback uses that same flag.
unsafe extern "system" fn console_control(kind: u32) -> i32 {
    if kind != CTRL_C_EVENT && kind != CTRL_BREAK_EVENT { return 0; }
    // STOP_FLAG: OnceLock<&'static AtomicBool>, initialized from the argument.
    STOP_FLAG.get().expect("installed before handler").store(
        true, std::sync::atomic::Ordering::SeqCst);
    unsafe { SetEvent(CANCEL_EVENT); }
    // Bounded: a hung cleanup must not make Ctrl-C a no-op.
    if unsafe { WaitForSingleObject(CLEANUP_DONE, CLEANUP_WAIT_MS) } != WAIT_OBJECT_0 {
        unsafe { ExitProcess(0xC000013A) };
    }
    1
}
// coordinator: cancel(); SetEvent(CLEANUP_DONE); ExitProcess(0xC000013A)
```

The example driver uses CREATE_NEW_CONSOLE in an isolated process, installs
its own ignore handler, reenables Ctrl-C in the s4 child, and broadcasts only
inside that private console. GenerateConsoleCtrlEvent(CTRL_C_EVENT, arbitrary
pid) is invalid as a targeting strategy. Driver deadlines kill/reap on failure.
Include both a busy helper and a busy reader in its generated fixture; C5
extends the result checks after reader integration. Restrict driver cfg to
Windows; the normal example protocol tests remain portable.

- [ ] **Step 4:** Linux helpers and workspace tests GREEN; Windows all-targets
  compiles. Windows CI verifies the actual console event, root Job descendant
  death, h03b, and at least 20 repeated starts/cancels without a leaked process.
  Unix h16 signal/ignored-signal tests keep their original contract. h16, h17
  and h18 are `#[cfg(target_os = "linux")]` today and observe processes through
  `/proc`; spec §4.2 says Ctrl-C on macOS behaves as on Linux, and signals exist
  there, so skipping them would describe a test implementation detail, not an
  absent behaviour (criterion 34). This task therefore adds a macOS variant of
  h16, h17 and h18 that observes liveness with `kill(pid, 0)` and reaps its own
  children, as already done for `alive`, and runs it on both macOS CI jobs.
  Case (c) of h16 also uses `/proc/PID/fd` to know that s4 has already opened
  the FIFO before it sends SIGINT (`tests/helpers.rs:791-798`); the macOS
  variant detects that with `libc::proc_pidinfo(PROC_PIDLISTFDS)` or
  `lsof -p PID` (shipped with macOS), inside the same bounded `wait_for`, and
  never replaces it with a fixed sleep. h17 and h18 do not use `/proc`
  (`tests/helpers.rs:804-870`), so for them the change is only widening the
  `cfg` to `unix`.
- [ ] **Step 5:** commit with
  `git commit -m "feat: cancel Windows helpers through the shared Supervisor"`.

## Chunk 3: Search path: per-file cost and macOS mmap

Owns `src/app.rs`, `src/search.rs`, `src/source.rs`, `src/matcher.rs`,
`src/formats/detector.rs`, new `scripts/bench-m5b.sh`, `tests/rg_compat.rs`.
Measure before editing hot paths. Handoff baseline data to C9/C10/C11.

### Task 3.1: reproducible per-file probe before optimization

**Files:** create `scripts/bench-m5b.sh`; modify only its own test/self-test
entry points. Existing benchmarks remain C4's property in this tanda.
Script interface:

```bash
# scripts/bench-m5b.sh [--self-test] [--probe|--accept] CORPUS
# SEARCHFOR_BIN: release s4, required
# SEARCHFOR_RG: reference 14.1.1, Unix fallback /usr/bin/rg
# SEARCHFOR_BENCH_REPORT: output JSON path, required for measurements
# SEARCHFOR_BENCH_VARIANT: descriptive label (baseline, worker-matcher, ...)
# SEARCHFOR_BASE_BIN: existing previous milestone binary for old ceilings
# --probe: N=5000,20000,80000, j=1,2,4,12; strace -f -c and perf stat
# --accept: M4 text corpus, 40 paired alternated runs after warmups
# --compare A B: the M4 corpus probe with the two s4 binaries A and B (no rg),
#   40 pairs alternated in one process on one machine; prints the ratio of
#   medians B/A and every raw sample as JSON (used by Task 11.2)
```

- [ ] **Step 1:** begin the script with this complete self-test. It exercises
  the actual gate function that production report evaluation must call.

```bash
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
# Implement measurement mode in Step 3.
```

- [ ] **Step 2:** first run the self-test against a deliberately stubbed
  `check_ratios() { return 0; }`: expected exit 10. Restore the shown function;
  `bash scripts/bench-m5b.sh --self-test` prints PASS and exits 0. This is TDD
  for the measurement evaluator; it does not claim an optimization is needed.
- [ ] **Step 3:** implement measurement in prose's specified order: validate
  `SEARCHFOR_RG --version`; generate the M4 text units (40 lines, 20,000 files),
  preserve case and no-extension variants for detector probes, warm the cache,
  alternate rg/s4 order on each pair, collect raw wall/CPU/RSS and uptime,
  calculate ratios of medians with all raw samples retained. Keep a temp
  formats/grammars/index, unset both configuration variables before any run.
  Redirect measured output to null and assert exit 1 for absent patterns.
  Treat a wrong exit code or version as a failed benchmark.
  Collect strace syscall counts and perf cycles/instructions/branches/cache
  samples for each thread count; report missing optional profilers explicitly.
  Fit T=intercept+N*slope independently per tool/mode; keep each N's samples.
  Implement `--compare A B` on the same measurement core: it takes two s4
  binaries instead of s4 and rg, alternates which of the pair runs first on each
  pair, and reports median(B)/median(A) with all raw wall/CPU/RSS samples; the
  exit code is 0 when both binaries ran validly and 2 otherwise, and the script
  never decides pass or fail from the ratio.
- [ ] **Step 4:** build release with the common prefix, then run the baseline
  probe under nice/ionice, with uptime logged beside each series. Expected
  output: JSON containing raw pairs, medians, intercept/slope and profiler
  availability, and three separately named gates. Baseline may exceed 1.05;
  record RED ratios, never replace them by the research's historical numbers.
  Exact local measurement commands (CORPUS is the M4 generated corpus path):

```bash
CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo build --offline --locked --release
SEARCHFOR_BIN="$CARGO_TARGET_DIR/release/s4" SEARCHFOR_BENCH_REPORT="$CARGO_TARGET_DIR/baseline.json" SEARCHFOR_BENCH_VARIANT=baseline nice -n 19 ionice -c3 bash scripts/bench-m5b.sh --probe "$CORPUS"
SEARCHFOR_BIN="$CARGO_TARGET_DIR/release/s4" SEARCHFOR_BENCH_REPORT="$CARGO_TARGET_DIR/accept.json" SEARCHFOR_BENCH_VARIANT=accept nice -n 19 ionice -c3 bash scripts/bench-m5b.sh --accept "$CORPUS"
```

  Probe exits 0 after collecting valid measurements; accept exits 1 when any
  ratio exceeds its ceiling, and 0 only when all three pass. Missing CORPUS,
  wrong reference or invalid binary exits 2 with a diagnostic.
- [ ] **Step 5:** commit with
  `git commit -m "perf: add paired per-file benchmark and independent gates"`.

### Task 3.2: worker matcher, direct reader and path/classification reuse

**Files:** modify `src/app.rs:421` (`Ctx`, worker closure, `Target`, `reader`,
`search_raw`, `Out::print`), `src/matcher.rs:14` (`PatternMatcher` cloning),
`src/formats/detector.rs` (the per-call `ext.to_ascii_lowercase()` copies in
`may_be_document` at :129, `detect` at :212 and `builtin_may_be_document` at :252;
`may_be_document` itself starts at :117).
Keep C9's later pipeline integration independent of output layout changes.

```rust
// src/app.rs, private worker state; owns cache-local matcher clone.
struct Worker {
    matcher: crate::matcher::PatternMatcher,
    // Existing searchers and output scratch move here, reused per worker.
}
// Existing Target keeps one filesystem path; displayed spelling is borrowed or
// represented by a cheap prefix view, rather than another per-file PathBuf.
// src/formats/detector.rs, private helper, Cow only allocates for ASCII uppercase.
fn normalized_extension(ext: &str) -> std::borrow::Cow<'_, str>;
```

- [ ] **Step 1:** add these complete unit tests in detector.rs's tests module.

```rust
#[test]
fn lowercase_extensions_borrow_the_existing_bytes() {
    use std::borrow::Cow;
    assert!(matches!(normalized_extension("txt"), Cow::Borrowed("txt")));
    assert!(matches!(normalized_extension(""), Cow::Borrowed("")));
    assert_eq!(normalized_extension("TXT"), "txt");
    assert_eq!(normalized_extension("DocX"), "docx");
}
```

Add this complete test in tests/rg_compat.rs; it compares both engines and worker
counts while retaining native path bytes.

```rust
#[test]
fn m5b_worker_matchers_keep_reference_output() {
    if !common::reference_rg() { return; }
    let fx = Fixture::new();
    for i in 0..32 {
        std::fs::write(fx.file(&format!("worker-{i}.txt")), "ñ needle\nother\n").unwrap();
    }
    for engine in ["default", "pcre2"] {
        for j in ["1", "2", "4", "12"] {
            let args = ["--no-heading", "--color=never", "--sort=path", "-j", j,
                        "--engine", engine, "needle", "."];
            let mut s = fx.s4(args);
            s.args(["--no-docs", "--no-trail"]);
            let a = run(&mut s);
            let b = run(&mut fx.rg(args));
            assert_eq!(code(&a), code(&b), "{engine}/{j}");
            assert_eq!(a.stdout, b.stdout, "{engine}/{j}");
        }
    }
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --lib lowercase_extensions_borrow`
  (RED: helper missing) and Task 3.1's exact baseline benchmark.
  Run preservation check:
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test rg_compat m5b_worker_matchers`.
  The functional differential is a preservation check; unsorted parallel
  searches must also be measured, since `--sort` deliberately forces one worker.
- [ ] **Step 3:** clone PatternMatcher once per walker worker, use that clone
  in every sink/search in the worker (Rust cache pool per clone; PCRE2 retains
  its existing thread-local behaviour). Measure this change alone first.
  Next remove duplicate Target path allocation and use a direct `File` reader
  branch for untransformed plain files; Held complete/partial input remains
  an enum with concrete Cursor/HeadThen variants. Preserve the prefix read by
  document magic detection; never reopen a non-rewindable stream.
  Add empty-buffer early return before `Out.bytes.fetch_add`, then measure.
  Normalize extension with a borrowed lowercase fast path. Cache the Format
  classification per file and Processor capability per Definition; do not call
  outline_of(empty) to discover capability. Measure each change separately,
  including docs-on/off, uppercase and no-extension files.
- [ ] **Step 4:** run rg_compat, detector tests and new benchmark acceptance.
  Expected GREEN: exact output and ≤1.05 gates. Measure the allocator counter
  last with an experimental local variant, never committed with protection
  disabled: compare System versus Counter under identical pairs. Keep Counter
  if cost ≤1%. If >1%, report measured share and a separate-reader executable
  alternative with archive/version/launch consequences; retain the counter
  until a concrete compliant change is proven. A missed gate blocks acceptance
  and becomes a measured question to Sebastián, not a larger ceiling.
- [ ] **Step 5:** commit with
  `git commit -m "perf: reuse worker matchers and remove measured per-file costs"`.

### Task 3.3: macOS reader searches, ANSI spill eligibility and portable pipe tests

**Files:** modify `src/search.rs:29` (`rg_in_memory`),
`src/app.rs:1332` (`search_raw`, all search_slice decisions),
`src/source.rs:103` (`Source::open_as` preservation tests),
`src/app.rs:3286` (`SpillLines`), `tests/rg_compat.rs:1275,2036` (PTY/unread).
New private predicate:

```rust
fn can_spill_bytes(buffer: &termcolor::Buffer) -> bool;
// true for plain/ANSI; on Windows false for console-API buffers.
```

- [ ] **Step 1:** add the following test to search.rs's existing test module.

```rust
#[test]
#[cfg(target_os = "macos")]
fn macos_never_searches_a_source_as_a_memory_map() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("file");
    std::fs::write(&p, "needle\n").unwrap();
    for flag in ["--mmap", "--no-mmap"] {
        let a = crate::cli::parse(["s4", flag, "needle"].into_iter().map(Into::into).collect()).unwrap();
        assert!(!rg_in_memory(&a, std::slice::from_ref(&p), false));
    }
}
```

This uses the verified cli::parse entry, including order resolution.
Add this complete portable differential to rg_compat.rs:

```rust
#[test]
fn m5b_mmap_binary_stats_follow_the_native_reference() {
    if !common::reference_rg() { return; }
    let fx = Fixture::new();
    std::fs::write(fx.file("mapped.bin"), b"needle\0x\nneedle\n").unwrap();
    for flag in ["--mmap", "--no-mmap"] {
        let args = ["--json", flag, "needle", "mapped.bin"];
        let a = run(fx.s4(args).args(["--no-trail", "--no-docs"]));
        let b = run(&mut fx.rg(args));
        assert_eq!(code(&a), code(&b));
        assert_eq!(common::mask_json_times(&stdout(&a)),
                   common::mask_json_times(&stdout(&b)));
    }
}
```

Add this complete Windows test to `tests/rg_compat.rs` for spec §4.2's hidden
file row; rg decides what the hidden attribute means and s4 must match it with
and without `--hidden`:

```rust
#[test]
#[cfg(windows)]
fn m5b_windows_hidden_attribute_matches_the_reference() {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_HIDDEN, SetFileAttributesW};
    if !common::reference_rg() { return; }
    let fx = Fixture::new();
    std::fs::write(fx.file("visible.txt"), "needle\n").unwrap();
    std::fs::write(fx.file("attr-hidden.txt"), "needle\n").unwrap();
    let wide: Vec<u16> = fx.file("attr-hidden.txt").as_os_str()
        .encode_wide().chain(Some(0)).collect();
    assert_ne!(unsafe { SetFileAttributesW(wide.as_ptr(), FILE_ATTRIBUTE_HIDDEN) }, 0);
    for extra in [&[][..], &["--hidden"][..]] {
        let mut args = vec!["--no-heading", "--color=never", "--sort=path"];
        args.extend_from_slice(extra);
        args.extend(["needle", "."]);
        let a = run(fx.s4(args.clone()).args(["--no-trail", "--no-docs"]));
        let b = run(&mut fx.rg(args));
        assert_eq!(code(&a), code(&b), "{extra:?}");
        assert_eq!(a.stdout, b.stdout, "{extra:?}");
    }
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test rg_compat m5b_mmap_binary_stats`
  and the header's Windows GNU cross-check. Native macOS runs
  `CARGO_BUILD_JOBS=8 nice -n 19 cargo test --offline --locked --lib macos_never_searches`.
  Linux cross-check is RED at unread's unguarded Unix API;
  macOS native test is RED when Always returns true. Existing Source mapping
  tests must remain GREEN: ≥8 MiB preparation mapping is still permitted.
- [ ] **Step 3:** make rg_in_memory false on macOS before inspecting mmap_mode.
  Audit own search_slice branches: reader binary semantics must survive even
  when preparation Source is mapped; feed a Cursor reader where required.
  Preserve document-reader mapping and Source::Mapping::Never for --no-mmap.
  SpillLines tests buffer.is_ansi() on Windows before copying byte slices;
  native console-API buffers print through BufferWriter with metadata intact.
  For slow pipes replace printf by Rust writes and use PeekNamedPipe on Windows
  or cfg'd FIONREAD constants on Linux/macOS, with bounded readiness loops.
  A Windows anonymous pipe has no Unix fd; use AsRawHandle only under cfg.
  PTY tests use Linux `script -qefc`, macOS `script -q /dev/null COMMAND …`,
  and Windows a console-driver test for terminal behaviour; Unix PTY ioctl-only
  tests are explicitly absent on Windows, with their missing feature recorded.
  No test is skipped merely because `sh` or util-linux flags were used before.
  All rg commands in rg_compat.rs use common::rg_path().
- [ ] **Step 4:** rg_compat and Source tests GREEN on Linux; Windows GNU checks
  compile. Native macOS verifies binary offsets and bytes_searched; Windows CI
  runs `m5b_windows_hidden_attribute_matches_the_reference` and checks slow
  pipes, path separator, ANSI and console output.
- [ ] **Step 5:** commit with
  `git commit -m "fix: preserve native mmap pipe and console search semantics"`.

## Chunk 4: Test base and CI

Owns `.gitattributes`, `tests/common/**`, `tests/cli.rs`, `cli_m2.rs`,
`cli_m3.rs`, `rg_compat_m5.rs`, `fuzz.rs`, `fuzz_m5.rs`, existing benchmark
scripts, reference setup tests in `src/docs/pdf.rs`/`sqlite.rs`,
`.github/workflows/ci.yml`, `fuzz-nightly.yml`, and `docs/platforms.md`.

### Task 4.1: isolated reference and fixture setup everywhere

**Files:** modify `tests/common/mod.rs:115,159` (`workspace_bin`, `rg_path`),
listed integration tests and benchmark scripts; create `.gitattributes`.
Public test helper signatures stay:

```rust
pub fn rg_path() -> std::path::PathBuf;
pub fn reference_rg() -> bool;
pub fn workspace_bin(package: &str, bin: &str, features: &[&str]) -> std::path::PathBuf;
// New pure helper, tested without mutating process-wide environment:
pub fn select_rg(override_path: Option<std::ffi::OsString>, unix: bool,
                 usr_bin_rg_exists: bool) -> std::path::PathBuf;
```

`common` itself carries no tests: a `#[cfg(test)]` module in `tests/common/mod.rs`
is compiled and run by every integration binary that declares `mod common;`
(twelve today), so the same tests would run twelve times. They live in
`tests/cli.rs`, which C4 already owns.

- [ ] **Step 1:** add these complete tests to tests/cli.rs.

```rust
mod m5b_tests {
    use super::common::select_rg;
    use std::path::{Path, PathBuf};
    #[test]
    fn explicit_reference_wins_without_path_search() {
        assert_eq!(select_rg(Some("/tmp/rg reference".into()), true, true),
                   PathBuf::from("/tmp/rg reference"));
        assert_eq!(select_rg(Some(r"C:\tools\rg.exe".into()), false, false),
                   PathBuf::from(r"C:\tools\rg.exe"));
        assert_eq!(select_rg(None, true, true), PathBuf::from("/usr/bin/rg"));
        // Unix without /usr/bin/rg (a Mac with Homebrew): PATH is the last resort.
        assert_eq!(select_rg(None, true, false), PathBuf::from("rg"));
        assert_eq!(select_rg(None, false, false), PathBuf::from("rg"));
    }
    #[test]
    fn fixture_attributes_keep_synthetic_bytes() {
        let attrs = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join(".gitattributes")).unwrap();
        let has = |rule: &str| attrs.lines().any(|l| l.trim() == rule);
        // Repo-wide: checkout never rewrites LF to CRLF, so docs/examples,
        // insta snapshots and fixtures keep their exact bytes on Windows.
        assert!(has("* -text"));
        assert!(has("*.sh text eol=lf"));
        assert!(has("tests/fixtures/** -text"));
    }
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test cli m5b_tests`.
  Expected RED: missing select_rg/attributes. C1/C3 already use rg_path unchanged.
- [ ] **Step 3:** delegate rg_path to select_rg(var_os, cfg!(unix),
  Path::new("/usr/bin/rg").exists()); keep the Unix order exactly
  `SEARCHFOR_RG`, then /usr/bin/rg if it exists, then `rg` from PATH as the last
  resort (spec §5.2 asks for the /usr/bin/rg fallback and does not forbid the
  next one; without it every comparison skips silently on a Mac with rg in
  /opt/homebrew/bin and no SEARCHFOR_REQUIRE_REFERENCE). Windows uses the PATH
  `rg`. Verify the version via
  reference_rg. Replace every fixed/PATH rg in owned tests/scripts. Unit tests
  in unrelated modules are owned by C8/C10 as graph notes say. In scripts use
  `rg="${SEARCHFOR_RG:-/usr/bin/rg}"`, quote "$rg", validate 14.1.1.
  Add `.gitattributes` before a Windows checkout, in this order: the repo-wide
  `* -text` first (docs/examples/sample.ini is compared byte for byte by Task
  12.1 and `tests/snapshots/*.snap` are insta snapshots, so neither may be
  converted on checkout; the Windows runner images may set `core.autocrlf=true`,
  unverified), then `*.sh text eol=lf`, then `tests/fixtures/** -text`; the
  later, more specific rules win, including deliberately unusual synthetic .sh
  fixture bytes. Rewrite shell/chmod-based
  generators as Rust byte writes or self-contained helper programs; add native
  path expectations only where path spelling is asserted. Never globally turn
  backslashes into slashes in searched bytes, JSON paths or regexes.
  workspace_bin invokes cargo through low priority on Linux, passes --offline
  --locked and inherited target dir, and obtains executable from Cargo JSON.
  PDF reference tools may skip only if optional; CI explicitly installs the
  tools required by SEARCHFOR_REQUIRE_REFERENCE. SQLite's python reference
  uses the CI-configured Python executable, including Windows `python`.
- [ ] **Step 4:** command GREEN (the two tests run once, in the `cli` binary);
  run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test cli --test cli_m2 --test cli_m3 --test rg_compat_m5`.
  Linux GREEN; Windows cross-check compiles after tanda 1 merges. Native CI
  compares raw --no-trail bytes to native rg, including drive paths.
- [ ] **Step 5:** commit with
  `git commit -m "test: isolate references and preserve fixture bytes on checkout"`.

### Task 4.2: bounded fuzz duration, replay and nightly no-change gate

**Files:** modify `tests/common/mod.rs`, `tests/fuzz.rs` and `fuzz_m5.rs`;
create `.github/workflows/fuzz-nightly.yml`.

```rust
// tests/common/mod.rs
pub struct FuzzBudget { /* deadline and optional count cap */ }
impl FuzzBudget {
    pub fn new(seconds: Option<u64>, count: Option<usize>) -> Self;
    pub fn permits(&self, completed: usize) -> bool;
}
// SEARCHFOR_FUZZ_SECONDS overrides normal count-based duration; when seconds is
// set alone, repeat generation until deadline, rather than ending at default N.
// Explicit SEARCHFOR_FUZZ_N caps cases even when a deadline is set.
// SEARCHFOR_FUZZ_ARTIFACT_DIR names local seed/case/args/stderr output on failure.
```

- [ ] **Step 1:** add this complete unit test in `mod m5b_tests` of `tests/cli.rs`, with
  `use super::common::FuzzBudget`.

```rust
#[test]
fn fuzz_budget_combines_explicit_count_with_a_deadline() {
    assert!(!FuzzBudget::new(Some(0), None).permits(0));
    assert!(FuzzBudget::new(Some(60), Some(2)).permits(1));
    assert!(!FuzzBudget::new(Some(60), Some(2)).permits(2));
    assert!(FuzzBudget::new(Some(60), None).permits(1_000_000));
    assert!(!FuzzBudget::new(None, Some(0)).permits(0));
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test cli fuzz_budget_combines`.
  Expected RED: missing budget. Time-budget fuzz runs must execute at least one
  case when seconds > 0; validate zero/invalid input without infinite looping.
- [ ] **Step 3:** integrate one shared deadline across each fuzz binary's loops,
  preserving existing seed and count replay. Keep child timeout per case (2 s
  document limit, 10 s CLI limit), RAII kill/reap on timeout, and shrink/replay
  identifiers. Save generated bytes, seed, case index and argv before failing;
  parent workflow uploads artifacts even on failure. Do not serialize arbitrary
  OsStrings lossily for exact replay; keep platform-specific bytes too.
  Implement nightly decision with the following complete gate script embedded
  in a github-script step; it is pure comparison once API data is loaded.

```javascript
// actions/github-script step, result-encoding: string; permissions actions:read.
const branch = 'main';
const {data: head} = await github.rest.repos.getCommit({
  ...context.repo, ref: branch,
});
const runs = await github.paginate(github.rest.actions.listWorkflowRuns, {
  ...context.repo, workflow_id: 'fuzz-nightly.yml', branch,
  event: 'schedule', status: 'success', per_page: 100,
});
const last = runs.find(r => r.id !== context.runId &&
  r.conclusion === 'success' && r.head_branch === branch);
const run = context.eventName === 'workflow_dispatch' || !last ||
  last.head_sha !== head.sha;
core.setOutput('sha', head.sha);
core.setOutput('run', String(run));
return String(run);
```

Scheduled skipped runs themselves succeed with the same head and retain the
baseline, which is correct. Paginate past non-success runs; compare successful
scheduled runs only. Gate exports SHA and the fuzz matrix checks out that exact
SHA so the decision and tested commit cannot race. Manual runs always execute.
Matrix seconds: Linux 1800 each, macOS arm64/Intel and Windows 600 each.
No-change gate job runs before dependency downloads/build. Add failure upload
with seed artifacts even when the test process crashes.

- [ ] **Step 4:** unit command GREEN; run both fuzz binaries with
  SEARCHFOR_FUZZ_SECONDS=2 and explicit seed 12345 via the common cargo prefix.
  Exact short-run commands:
  `SEARCHFOR_FUZZ_SECONDS=2 SEARCHFOR_FUZZ_SEED=12345 CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test fuzz -- --nocapture`
  and the same command with `--test fuzz_m5`.
  Expected GREEN, more than zero cases, duration budget applied. Simulate the
  gate locally with equal/different/no previous SHA and manual event fixtures;
  expected decisions false/true/true/true. Native nightly execution is CI-only.
- [ ] **Step 5:** commit with
  `git commit -m "ci: budget fuzz runs and skip unchanged scheduled commits"`.

### Task 4.3: four native jobs, verified rg archives and skip inventory

**Files:** create `.github/workflows/ci.yml`, `docs/platforms.md`;
modify common tests to validate workflow contract. C11 later extends CI with
packaging jobs; keep build/test job IDs stable.

- [ ] **Step 1:** add this complete workflow contract test to tests/cli.rs.

```rust
#[test]
fn m5b_ci_names_four_native_systems_and_required_reference() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let ci = std::fs::read_to_string(root.join(".github/workflows/ci.yml")).unwrap();
    for marker in ["ubuntu-24.04", "macos-15", "macos-15-intel", "windows-2022",
                   "SEARCHFOR_REQUIRE_REFERENCE", "SEARCHFOR_RG", "120",
                   "cargo clippy", "cargo test", "--workspace", "--release"] {
        assert!(ci.contains(marker), "missing workflow contract: {marker}");
    }
    let docs = std::fs::read_to_string(root.join("docs/platforms.md")).unwrap();
    for reason in ["/proc", "GNU time", "POSIX", "PCRE2", "arm64"] {
        assert!(docs.contains(reason), "missing platform inventory: {reason}");
    }
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test cli m5b_ci_names`.
  Expected RED: files absent. This contract test supplements actual CI execution.
- [ ] **Step 3:** write a four-row include matrix with explicit runner, triple,
  official rg-14.1.1 archive, executable suffix and pinned SHA-256. Setup installs
  Rust 1.90 and C toolchain, PCRE2_SYS_STATIC=1, Python sqlite3 and supported
  decompression/reference tools; set LC_ALL=C where GNU time is parsed.
  Fetch and verify the official archive before extraction; use sha256sum on
  Linux, shasum -a 256 on macOS, Get-FileHash on Windows. Values must be real
  64-hex digests checked against the official release metadata already in local
  cache, or filled/verified by CI setup before the orchestrator pushes. Never
  commit fake all-zero/example digests as a passing setup. Offline absence of
  authoritative digests is an explicitly reported CI preparation item.

Build job body must implement these exact commands (priority wrapper per host):

```yaml
# These are job steps, inserted after checkout, tool/reference setup and cache.
- name: Format (Linux)
  if: runner.os == 'Linux'
  run: cargo fmt --all --check
- name: Clippy
  run: cargo clippy --workspace --all-targets --locked -- -D warnings
- name: Build release
  run: cargo build --workspace --release --locked
- name: Full suite
  run: cargo test --workspace --release --locked
  env:
    SEARCHFOR_REQUIRE_REFERENCE: '1'
- name: Short Processor fuzz (Linux)
  if: runner.os == 'Linux'
  run: cargo test --release --locked --test fuzz -- --nocapture
  env:
    SEARCHFOR_FUZZ_SECONDS: '120'
- name: Short CLI fuzz (Linux)
  if: runner.os == 'Linux'
  run: cargo test --release --locked --test fuzz_m5 -- --nocapture
  env:
    SEARCHFOR_FUZZ_SECONDS: '120'
```

Cache ~/.cargo and the single target dir by OS/arch/toolchain/lock; never share
MSVC/GNU or arm64/Intel objects. Schedule Linux RSS/bomb tests from perf with
--ignored in a serial invocation after C10, without SEARCHFOR_PERF_TIMING.
Print test discovery plus ignored and cfg-excluded inventory counts for each
system. The inventory identifies test function, file, platform and missing
feature; cfg-excluded tests are counted from the maintained inventory, ignored
tests from test output. Compare inventory against `cargo test -- --list`.
Document Linux-only GNU time, strace and /proc; Unix signals/non-UTF8 bytes;
Windows console equivalents; macOS PCRE2 JIT unavailability on arm64. Every
rewrite in C1/C2/C3/C5/C6/C8/C10 sends C4 its inventory row before merge.
`/proc` is documented as the Linux way of observing processes, never as the
reason a macOS test is absent: the Ctrl-C tests h16, h17 and h18 have a macOS
variant (Task 2.3) and are not skipped inventory rows there.

- [ ] **Step 4:** contract test GREEN locally, fmt/check/clippy local checks as
  available. CI jobs must actually execute the native suite and print skip
  counts. Configuring a runner label is not evidence that the runner ran.
- [ ] **Step 5:** commit with
  `git commit -m "ci: test four native systems against pinned ripgrep"`.

## Chunk 5: Document readers on every system

Depends on C2; owns `src/docs/isolated.rs`, `src/docs/memory.rs`,
`tests/cli_m4.rs`. C11 may later change only allocator backing on musl.

### Task 5.1: nested memory Job and lossless reader paths

**Files:** modify `src/docs/isolated.rs:96` (`spawn`, pool worker Child type),
`:282` (`bytes_of`, `path_of`), `:322` (`rlimit_mib`), `:331` (`why`);
`src/docs/memory.rs` counter preservation tests; `tests/cli_m4.rs` limiter.

```rust
// isolated.rs private helpers, using the existing HEADROOM_MIB = 64.
fn reader_limits(mib: u64) -> crate::platform::Limits;
fn bytes_of(path: &std::path::Path) -> Vec<u8>;
fn path_of(bytes: Vec<u8>) -> std::path::PathBuf;
// Windows path wire encoding is little-endian u16 units, not UTF-8 lossy text.
// Existing SEARCHFOR_READER_MIB, SEARCHFOR_READER_RLIMIT_MIB and
// SEARCHFOR_READER_SECONDS retain their meanings. No new user limit flag.
```

- [ ] **Step 1:** append these complete tests in isolated.rs's tests module.

```rust
#[test]
fn reader_job_has_the_counter_budget_plus_headroom() {
    let limits = reader_limits(32);
    #[cfg(windows)]
    assert_eq!(limits.process_memory_bytes, Some((32 + HEADROOM_MIB as usize) << 20));
    #[cfg(not(windows))]
    assert!(limits.process_memory_bytes.is_none());
}

#[test]
#[cfg(windows)]
fn reader_wire_path_roundtrips_unpaired_utf16() {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    let units = [b'C' as u16, 58, 92, 0xD800, b'x' as u16];
    let path = std::path::PathBuf::from(std::ffi::OsString::from_wide(&units));
    let got = path_of(bytes_of(&path));
    assert_eq!(got.as_os_str().encode_wide().collect::<Vec<_>>(), units);
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --lib reader_job_has`.
  Expected RED: missing reader_limits. Windows path test is RED under original
  lossy conversion; Windows all-targets check compiles both new branches.
- [ ] **Step 3:** start workers through platform::spawn with common Job plus
  nested process_memory_bytes = bytes_of_mib(mib+HEADROOM_MIB), saturating.
  Make HEADROOM_MIB: u64 = 64 unconditional (it is cfg(unix) at base).
  Retain the nested Job while the pooled worker is idle and active. Preserve
  central spawn thread (Linux PDEATHSIG thread lifetime) and shared Supervisor
  registration. Replace std::Child type references throughout the file with
  platform::Child; pipe code uses platform reader/writer. Counter stays armed
  first in child(); remember refusal even if the parser swallows an OOM.
  Keep warning text `reading it needed more than the N MiB a reader may use`.
  Recognize allocation failures caused by Windows committed-memory refusal;
  never assume the Job kills on reaching PROCESS_MEMORY. Use counter refusal,
  fallible allocation error and stderr/exit-status diagnostics together.
  Serialize Windows OsStrings as u16 little endian in this internal protocol;
  check even length before decoding and retain Unix raw-byte encoding.
  Migrate the reader start in `spawn` from `Supervisor::launch_reader` to
  `Supervisor::launch_managed` (added by C2 beside it, taking a
  `platform::Child`); do not delete `launch_reader` (the orchestrator removes it in
  the tanda-2 merge commit).
- [ ] **Step 4:** rerun unit tests and
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test cli_m4 m4_22 -- --nocapture`.
  Expected GREEN on Linux/macOS/Windows. Windows CI separately lowers the Job
  barrier above a permissive counter to prove the second barrier is active.
  Existing memory::Counter tests run unchanged on all systems.
- [ ] **Step 5:** commit with
  `git commit -m "feat: constrain Windows document readers with nested Jobs"`.

### Task 5.2: checked rlimits, orphan protection and document mappings

**Files:** modify `src/docs/isolated.rs:462,480,509` (`orphan_dies`, `read`,
`limit`), tests/cli_m4.rs (`limiter`, portability of scripts and signals).

```rust
// Private injectable syscall seam, without changing the public reader contract.
fn apply_limit(
    resource: libc::c_int, bytes: u64,
    set: impl FnOnce(libc::c_int, &libc::rlimit) -> std::io::Result<()>,
) -> std::io::Result<()>; // cfg(unix)
```

- [ ] **Step 1:** add this complete Unix unit test in isolated.rs.

```rust
#[test]
#[cfg(unix)]
fn refused_rlimit_is_observable_and_keeps_the_counter_path() {
    let result = apply_limit(libc::RLIMIT_DATA as libc::c_int, 64 << 20,
        |_, _| Err(std::io::Error::from_raw_os_error(libc::EPERM)));
    assert_eq!(result.unwrap_err().raw_os_error(), Some(libc::EPERM));
    let ok = apply_limit(libc::RLIMIT_DATA as libc::c_int, 64 << 20, |_, r| {
        assert_eq!(r.rlim_cur, 64 << 20);
        assert_eq!(r.rlim_max, 64 << 20);
        Ok(())
    });
    assert!(ok.is_ok());
}

#[test]
fn document_mapping_is_independent_of_search_mapping_policy() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mapped-document");
    std::fs::write(&path, vec![b'x'; (1 << 20) + 1]).unwrap();
    let bytes = read(&path).unwrap();
    assert!(matches!(&bytes, Bytes::Mapped(_)));
    assert_eq!(bytes.len(), (1 << 20) + 1);
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --lib refused_rlimit`.
  Expected RED: missing seam. Mapping test is a preservation check on all hosts.
- [ ] **Step 3:** check every setrlimit result. macOS refusal emits exactly one
  debug message per failed resource setup; extraction continues under Counter.
  Success stays quiet. Use the correct libc resource type on each Unix target
  behind the integer seam. Keep RLIMIT_CORE and Linux PR_SET_DUMPABLE settings.
  Replace reader orphan_dies implementation with C2 API and expected parent
  parsed from SEARCHFOR_READER_OF, rejecting malformed/missing parent early.
  Keep mapped document reads even on macOS; failed mapping falls back to read
  only when it is a mapping failure, preserving real open/read errors.
  tests/cli_m4's memory limiter uses Unix rlimits on Linux, counter-dominant
  verification on macOS, and platform Limits on Windows. Rewrite m4_08's shell
  preprocessor using a native test helper mode (C2 example binary) that uppercases
  file bytes or exits 3 with `oops`; it passes its full path to --pre. Preserve
  gzip comparison where the decompressor feature exists; CI provides tools.
  Guard actual /proc/signal-only assertions with cfg and a precise reason.
- [ ] **Step 4:** run cli_m4 and reader unit tests GREEN; Windows GNU compiles.
  macOS CI verifies m4_22 counter warning and real parent-death after SIGKILL
  while idle. Windows console driver from C2 now includes a reader and verifies
  memory warning and zero surviving reader/helper descendants. Linux keeps all
  existing signal/rlimit tests, including synthetic hostile-document fixtures.
- [ ] **Step 5:** commit with
  `git commit -m "fix: check reader limits and preserve native parent death"`.

## Chunk 6: MCP on every system

Depends on C2; owns `src/mcp.rs` and `tests/mcp_m5.rs`.

### Task 6.1: wait outside locks and watch disconnected Windows clients

**Files:** modify `src/mcp.rs:73,880,921,948,1008` (`Child`, `Watched`,
`exited`, `Searcher::start`, `Running::end`); modify tests/mcp_m5.rs cfg layout.

```rust
// mcp.rs; native client identity replaces Option<i32> on Windows.
#[cfg(unix)]
type ClientHandle = std::os::fd::RawFd;
#[cfg(windows)]
type ClientHandle = std::os::windows::io::OwnedHandle;
// Running child is Arc<Mutex<platform::Child>>; Watched reads platform::PipeReader.
fn exited(token: &crate::platform::WaitToken) -> std::io::Result<()>;
// WaitToken is captured under Child lock, then waited outside it.
```

- [ ] **Step 1:** add this complete unit test to mcp.rs's tests module.
  It exercises the exact wait ordering via a small private helper to be added.

```rust
#[test]
fn waiting_for_exit_does_not_hold_the_cancel_lock() {
    use std::sync::{Arc, Mutex, mpsc};
    use std::time::Duration;
    let lock = Arc::new(Mutex::new(()));
    let (started_tx, started_rx) = mpsc::channel();
    let (cancel_tx, cancel_rx) = mpsc::channel();
    let child = lock.clone();
    let waiter = std::thread::spawn(move || {
        wait_without_lock(&child, || {
            started_tx.send(()).unwrap();
            cancel_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        });
    });
    started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let acquired = lock.try_lock().is_ok();
    cancel_tx.send(()).unwrap();
    waiter.join().unwrap();
    assert!(acquired, "a cancel could not acquire the Child lock");
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --lib waiting_for_exit`.
  Expected RED: missing wait_without_lock helper. Add signature
  `fn wait_without_lock<T>(child: &Mutex<T>, wait: impl FnOnce())` only if used
  by production ordering: lock only to capture the WaitToken, explicitly drop
  the guard, then invoke wait. A helper unused by production is insufficient.
- [ ] **Step 3:** route starts through platform::spawn under
  `Supervisor::launch_managed` (C2 added it beside `launch_reader`; C5 migrates
  its own call in `src/docs/isolated.rs`). C6 only migrates its own call; once no caller of `launch_reader`
  remains, the orchestrator deletes `launch_reader` in the tanda-2 merge commit;
  C6 does not touch `src/helper/host.rs` and does not depend on C5's merge.
  Capture WaitToken once before starting blocking wait; release mutex, wait,
  reacquire only for final reap/status. Unix waitid remains supported where
  safe, or use bounded try_wait under brief locks; Windows WaitForSingleObject
  uses duplicated handle. Cancellation can always acquire Child while waiting.
  For Windows Watched, poll child output availability with PeekNamedPipe and
  monitor the client's server-output handle with a zero-byte WriteFile probe
  to detect ERROR_BROKEN_PIPE/NO_DATA; this produces no protocol bytes. Use a
  bounded poll interval, distinguish temporary empty pipe from EOF, and keep
  reads off the cancel lock. If that probe is not reliable on the native runner,
  use overlapped pipe I/O/cancellation with explicit owned handles; prove it by
  the disconnect test, rather than treating a successful type check as proof.
  Set PROCESS_PARENT_ENV for macOS; Linux pre_exec safety/PDEATHSIG semantics
  remain intact through the native spawn backend. Windows root Job includes
  the search, and its nested readers may join their own Jobs safely.
- [ ] **Step 4:** same unit test GREEN and existing mcp unit tests GREEN.
  Linux tests/mcp_m5.rs's three signal/FIFO cases keep running. Windows compile
  check verifies handle types; native disconnect and cancellation follow below.
- [ ] **Step 5:** commit with
  `git commit -m "fix: make MCP waits cancellable across platforms"`.

### Task 6.2: native MCP termination and connection tests

**Files:** modify `tests/mcp_m5.rs` (move Linux-specific helpers to a cfg module,
add portable transcript and Windows/macOS modules); `src/mcp.rs` test seams.
No new public CLI/env control; use real files/pipes and native process handles.

- [ ] **Step 1:** add this complete portable test outside the Linux cfg module.

```rust
#[test]
fn m5b_mcp_transcript_finishes_after_client_input_eof() {
    use std::io::Write;
    let fx = common::Fixture::new();
    let mut c = fx.s4(["--mcp"]);
    c.stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = c.spawn().unwrap();
    let call = serde_json::json!({"jsonrpc":"2.0","id":7,"method":"tools/call",
        "params":{"name":"search","arguments":{
            "pattern":"needle-pkg","paths":["md/guide.md"]}}});
    writeln!(child.stdin.take().unwrap(), "{call}").unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["id"], 7);
    let found = &v["result"]["structuredContent"]["matches"];
    assert_eq!(found.as_array().unwrap().len(), 1);
    assert_eq!(found[0]["line"], 12);
    assert_eq!(found[0]["trail"].as_array().unwrap().last().unwrap(), "bash");
}
```

Use this complete lifecycle fixture outside the Linux-only module. It blocks
on a real .ini file's Helper Processor, so named-pipe filesystem support is
not an unproved prerequisite. C2 owns the example helper wait controls.

```rust
struct NativeRun {
    server: std::process::Child,
    search: u32,
    helper: u32,
    _fx: common::Fixture,
    release: std::path::PathBuf,
}

fn m5b_alive(pid: u32) -> bool {
    #[cfg(target_os = "linux")]
    {
        let text = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap_or_default();
        return text.rfind(')').and_then(|n| text[n+1..].split_whitespace().next())
            .is_some_and(|s| s != "Z");
    }
    #[cfg(all(unix, not(target_os = "linux")))]
    { unsafe { libc::kill(pid as libc::pid_t, 0) == 0 } }
    #[cfg(windows)]
    {
        use windows_sys::Win32::{Foundation::CloseHandle, System::Threading::{
            OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE}};
        unsafe {
            let h = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
            if h.is_null() { return false; }
            let running = WaitForSingleObject(h, 0) == 258;
            CloseHandle(h);
            running
        }
    }
}

impl NativeRun {
    fn start(finite: bool) -> Self {
        use std::{io::Write, process::Stdio, time::{Duration, Instant}};
        let fx = common::Fixture::new();
        std::fs::write(fx.file("blocked.ini"), "[section]\nk=needle\n").unwrap();
        let bin = common::workspace_bin("searchfor-example-helper", "searchfor-example-helper", &[]);
        let def = format!("name='blocked'\nextensions=['ini']\nprocessor='external'\n[params]\ncommand=[{:?}]\nmode='pool'\ntimeout_ms=30000\n", bin.to_str().unwrap());
        std::fs::write(fx.formats.path().join("blocked.toml"), def).unwrap();
        let ready = fx.file("ready.json");
        let release = fx.file("release");
        if finite { std::fs::write(&release, b"go").unwrap(); }
        let mut command = fx.s4(["--mcp"]);
        command.env("SEARCHFOR_EXAMPLE_WAIT_READY", &ready)
            .env("SEARCHFOR_EXAMPLE_WAIT_RELEASE", &release)
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
        let mut server = command.spawn().unwrap();
        let call = serde_json::json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
            "params":{"name":"search","arguments":{"pattern":"needle","paths":["blocked.ini"]}}});
        writeln!(server.stdin.as_mut().unwrap(), "{call}").unwrap();
        let deadline = Instant::now() + Duration::from_secs(20);
        let v: serde_json::Value = loop {
            if let Ok(bytes) = std::fs::read(&ready) {
                if let Ok(v) = serde_json::from_slice(&bytes) { break v; }
            }
            if Instant::now() >= deadline {
                let _ = server.kill(); let _ = server.wait();
                panic!("MCP helper did not report readiness");
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        Self { server, search: v["search_pid"].as_u64().unwrap() as u32,
            helper: v["helper_pid"].as_u64().unwrap() as u32, _fx: fx, release }
    }
    fn blocked_search() -> Self { Self::start(false) }
    fn finite_search() -> Self { Self::start(true) }
    fn close_client_output(&mut self) { drop(self.server.stdout.take()); }
    fn close_client_input(&mut self) { drop(self.server.stdin.take()); }
    fn kill_server(&mut self) { self.server.kill().unwrap(); self.server.wait().unwrap(); }
    fn wait_search_dead(&mut self, within: std::time::Duration) -> bool {
        let start = std::time::Instant::now();
        loop {
            let _ = self.server.try_wait();
            if !m5b_alive(self.search) && !m5b_alive(self.helper) { return true; }
            if start.elapsed() >= within { return false; }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
}
impl Drop for NativeRun {
    fn drop(&mut self) {
        let _ = std::fs::write(&self.release, b"cleanup");
        let _ = self.server.kill(); let _ = self.server.wait();
    }
}
```

Add these complete native lifecycle tests with that fixture:

```rust
#[test]
fn m5b_client_output_disconnect_stops_a_blocked_search() {
    let mut r = NativeRun::blocked_search();
    r.close_client_output();
    assert!(r.wait_search_dead(std::time::Duration::from_secs(3)));
}
#[test]
fn m5b_server_death_stops_a_blocked_search() {
    let mut r = NativeRun::blocked_search();
    r.kill_server();
    assert!(r.wait_search_dead(std::time::Duration::from_secs(3)));
}
#[test]
fn m5b_client_input_eof_leaves_no_search() {
    let mut r = NativeRun::finite_search();
    r.close_client_input();
    // This fixture is finite: queued requests complete after a half-close.
    assert!(r.wait_search_dead(std::time::Duration::from_secs(3)));
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test mcp_m5 m5b_ -- --nocapture`.
  Expected RED before C6: transcript absent outside Linux and native lifecycle
  fixture missing; observe behavioural RED on Windows/macOS with the fix undone.
- [ ] **Step 3:** retain the existing Linux FIFO tests in their cfg module.
  The complete new NativeRun fixture above instead blocks a Helper Processor
  on a regular file, observes PIDs from an explicit readiness log, and exercises
  the same search-child cancellation. On Windows use native wait handles;
  on macOS kill(pid,0) is observation only and all owned children are reaped.
  The helper's parent watch is enabled solely by its test wait controls.
  Keep the client stdin/output independently owned so each closure is real.
  Input EOF is a half-close: drain requests already read and then exit after
  their replies. Output disconnection cancels a blocked search immediately.
  The input-EOF lifecycle test uses a finite search and checks eventual clean
  exit; it must not expect cancellation of an indefinitely blocked call.
  Preserve single-request JSON-RPC sequencing.
  All readiness loops have a deadline and all guards clean up on panic.
- [ ] **Step 4:** Linux mcp_m5 GREEN; cross-check compiles. Native CI runs all
  three lifecycle cases on macOS arm64/Intel and Windows, preserving existing
  protocol limits, errors, no-config and byte/path handling tests.
- [ ] **Step 5:** commit with
  `git commit -m "test: cover native MCP disconnect and parent termination"`.

## Chunk 8: --pre, Grammars and the six Grammar Helpers

Depends on C2; owns `src/preprocess.rs`, `src/grammars/**`, `src/cli/**`,
`helpers/grammar/**`, `scripts/build-grammar-helper.sh`, `grammars/**`,
`docs/grammar-index.schema.json`, root Cargo.lock for this tanda.

### Task 8.1: safe --pre resolution and managed transport

**Files:** modify `src/preprocess.rs:34,80` (`Preprocess::new`, `open`,
`Transformed`), `src/cli/rg_flags.rs:453` (unit reference resolver).

```rust
// preprocess.rs, Windows transport replacing CommandReader's hidden spawn.
struct ManagedCommandReader { /* managed Child, stdout, stderr drain, exit state */ }
impl std::io::Read for ManagedCommandReader { /* native stdout bytes */ }
impl ManagedCommandReader {
    fn start(program: &std::path::Path, path: &std::path::Path) -> std::io::Result<Self>;
    fn close(&mut self) -> std::io::Result<()>;
}
// Transformed retains the existing Read/closing API consumed by app.rs.
// --pre uses grep_cli::resolve_binary before creating a managed SpawnSpec.
```

- [ ] **Step 1:** add this complete test to preprocess.rs's tests module.

```rust
#[test]
fn pre_resolution_matches_the_library_reference() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing preprocessor");
    let argv = vec!["s4".into(), "--pre".into(), missing.clone().into_os_string(),
                    "needle".into()];
    let args = crate::cli::parse(argv).unwrap();
    let expected = grep_cli::resolve_binary(&missing).unwrap();
    let got = Preprocess::new(&args, dir.path()).unwrap();
    assert_eq!(got.pre.as_deref(), Some(expected.as_path()));
    let input = dir.path().join("input");
    std::fs::write(&input, "needle").unwrap();
    assert!(got.open(&Through::Pre, &input).is_err());
}
```

The Windows bare-name regression runs in its own process so PATH/cwd changes
cannot race other tests. Add both complete tests to preprocess.rs:

```rust
#[test]
#[cfg(windows)]
#[ignore = "isolated child for Windows pre resolution"]
fn windows_pre_resolution_probe() {
    let args = crate::cli::parse(vec!["s4".into(), "--pre".into(),
        "s4-fake-pre.exe".into(), "needle".into()]).unwrap();
    assert!(Preprocess::new(&args, &std::env::current_dir().unwrap()).is_err());
}
#[test]
#[cfg(windows)]
fn windows_pre_bare_name_does_not_search_cwd() {
    let d = tempfile::tempdir().unwrap();
    let exe = std::env::current_exe().unwrap();
    std::fs::copy(&exe, d.path().join("s4-fake-pre.exe")).unwrap();
    let status = std::process::Command::new(exe)
        .args(["--exact", "preprocess::tests::windows_pre_resolution_probe", "--ignored"])
        .current_dir(d.path()).env("PATH", d.path().join("empty-path"))
        .status().unwrap();
    assert!(status.success());
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --lib pre_resolution_matches`.
  The portable absolute-path case already passes: resolve_binary is a no-op
  for absolute paths, even missing ones. Behavioural RED is the separate
  Windows bare-name test below; preserve spawn-time missing-program errors.
- [ ] **Step 3:** resolve --pre just as rg's grep_cli::resolve_binary does,
  keeping diagnostics and last-flag/pre-glob precedence. In Windows create it
  through C2's suspended root-Job launcher; no memory Job. Give it the file as
  stdin and as argv path, concurrently drain stderr, wait/reap on close,
  preserve `preprocessor command could not start/failed` messages and status.
  Reproduce grep-cli's async_stderr behaviour without an unbounded stderr
  deadlock. Keep decompression semantics, and route any directly started
  decompressor through the same managed mechanism if owned by this process.
  Never call CommandReaderBuilder::build on Windows and then assign its child:
  its hidden spawn would violate the assignment-before-resume guarantee.
  Add the Windows test in the next code block: cwd contains a native fake
  program and PATH excludes cwd; a bare name cannot execute accidentally. Reuse the native preprocessor test helper C5 uses.
  rg_flags unit tests use SEARCHFOR_RG first, then native fallback; no test
  invokes documentation-writing developer test during CI.
- [ ] **Step 4:** preprocess and cli_m4 comparisons GREEN locally; cross-check
  Windows; native Windows CI proves cwd resolution, Job cleanup and error
  parity against its downloaded rg. macOS follows grep_cli resolution too.
- [ ] **Step 5:** commit with
  `git commit -m "fix: resolve preprocessors safely and manage their lifecycle"`.

### Task 8.2: index names, file URLs, directory documentation and cleanup

**Files:** modify `src/grammars/index.rs:35,79` (`valid_name`, `fetch`),
`src/grammars/install.rs:50,80` (`replace`, rollback), `src/grammars/mod.rs`
(binary_name, directory tests), `src/cli/mod.rs` help, and
`src/processors/external.rs` temporary cleanup is **deferred to C10** (ownership
note: C10 owns this Processor file in tanda 3; C8 specifies the contract here).
Modify `docs/grammar-index.schema.json` (it already exists), and create
`tests/grammar_dirs.rs` (new, owned by C8).

```rust
// index.rs private; accepts existing unescaped local test URLs as well as
// standard percent-encoded drive/UNC file URLs. Reject malformed escapes/NUL.
fn file_url_path(url: &str) -> anyhow::Result<std::path::PathBuf>;
// Schema: version=1, grammar identity/name/version/source/license/definition,
// per-system binaries with https/file URL and 64-hex SHA-256.
```

- [ ] **Step 1:** append these complete index unit tests.

```rust
#[test]
fn grammar_identifiers_include_c_sharp_without_traversal() {
    for name in ["c", "cpp", "java", "ruby", "c-sharp", "elixir", "old_name"] {
        assert!(valid_name(name), "{name}");
    }
    for name in ["", ".", "..", "../c", "c/sharp", "c\\sharp", "-c", "c-", "C"] {
        assert!(!valid_name(name), "{name}");
    }
}
#[test]
fn file_urls_decode_spaces_and_percent_once() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("a b%20");
    std::fs::write(&p, b"local index").unwrap();
    let raw = p.to_str().unwrap().replace('\\', "/");
    let raw = if cfg!(windows) { format!("/{raw}") } else { raw };
    let url = format!("file://{}", raw.replace('%', "%25").replace(' ', "%20"));
    assert_eq!(fetch(&url).unwrap(), b"local index");
    assert!(file_url_path("file:///x%00y").is_err());
    assert!(file_url_path("file:///x%Q0").is_err());
}
#[test]
fn index_schema_accepts_required_c_sharp_identity() {
    let schema: serde_json::Value = serde_json::from_str(include_str!(
        "../../docs/grammar-index.schema.json")).unwrap();
    let doc = serde_json::json!({"version":1,"grammars":[{
        "name":"c-sharp","version":"0.23.1","source":"https://example.org/csharp",
        "license":"MIT","definition":"name = 'c-sharp'",
        "binaries":{"windows-x86_64":{"url":"file:///C:/g.exe",
            "sha256":"a".repeat(64)}}}]});
    assert!(jsonschema::is_valid(&schema, &doc));
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --lib grammars::index::tests`.
  Expected RED: c-sharp rejected by `valid_name`, file escapes treated literally,
  and `index_schema_accepts_required_c_sharp_identity` fails because the existing
  schema's `name` pattern `^[a-z0-9_]+$` rejects `c-sharp`.
- [ ] **Step 3:** permit single/internal hyphens while rejecting leading/trailing
  ones and all separators/dots; preserve old underscores. Implement file URL
  parsing with existing dependencies or a small strict decoder (no new URL
  crate authorized): localhost local paths, /C:/ drive prefix, UNC authority on
  Windows, byte-preserving Unix percent-decoding, UTF-8-to-OsString on Windows.
  Keep HTTPS-only redirects and existing connect/global ceilings unchanged.
  Atomic install keeps old install after failure; Windows open executables
  may prevent rename, so fail cleanly with old binary intact and retry only
  bounded sharing violations. Document native dirs via dirs::config_dir and
  data_dir, never hard-code Linux directories into Windows/macOS help.
  In the existing schema, change the `name` pattern to allow single internal
  hyphens (`^[a-z0-9]+([-_][a-z0-9]+)*$`, or an equivalent that still rejects
  leading/trailing separators), and check that the `propertyNames` pattern of
  `binaries`, `^[a-z0-9]+-[a-z0-9_]+$`, still accepts all five system keys
  (`linux-x86_64`, `linux-aarch64`, `macos-x86_64`, `macos-aarch64`,
  `windows-x86_64`); it does, so leave it unless that check fails.
  C10's RunTemp cleanup retries deletion after response when a helper still
  holds a file, then relinquishes it to system temp cleanup; always use temp_dir.
- [ ] **Step 4:** unit tests GREEN; install rollback/hash mismatch existing
  tests GREEN. Native CI tests drive paths and replacement sharing failures. Add
  this complete per-system test to `tests/grammar_dirs.rs` (spec §4.2's data and
  config directory row); it never writes to a real user directory.

```rust
use assert_cmd::Command;

#[test]
fn m5b_help_names_the_native_data_and_config_directories() {
    let home = tempfile::tempdir().unwrap();
    let mut cmd = Command::cargo_bin("s4").unwrap();
    cmd.arg("--help")
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join("xdg-config"))
        .env("XDG_DATA_HOME", home.path().join("xdg-data"))
        .env("APPDATA", home.path().join("appdata"))
        .env("LOCALAPPDATA", home.path().join("localappdata"))
        .env_remove("SEARCHFOR_FORMATS_DIR")
        .env_remove("SEARCHFOR_GRAMMARS_DIR")
        .env_remove("SEARCHFOR_CONFIG_PATH")
        .env_remove("RIPGREP_CONFIG_PATH");
    let out = String::from_utf8(cmd.output().unwrap().stdout).unwrap();
    // Unix: the overrides above decide `dirs`. Windows resolves Known Folders
    // and ignores them; the directory is only printed there, never written.
    let (data, config) = if cfg!(windows) {
        (dirs::data_dir().unwrap(), dirs::config_dir().unwrap())
    } else if cfg!(target_os = "macos") {
        let support = home.path().join("Library").join("Application Support");
        (support.clone(), support)
    } else {
        (home.path().join("xdg-data"), home.path().join("xdg-config"))
    };
    assert!(out.contains(&data.join("searchfor").join("grammars").display().to_string()), "{out}");
    assert!(out.contains(&config.join("searchfor").join("formats").display().to_string()), "{out}");
}
```
- [ ] **Step 5:** commit with
  `git commit -m "feat: support portable grammar indexes and c-sharp names"`.

### Task 8.3: six features, real Grammar loading and target-aware build artifacts

**Files:** modify `helpers/grammar/Cargo.toml`, `helpers/grammar/src/main.rs`
(where `grammar()` lives) and `helpers/grammar/tests/protocol.rs`; create `helpers/grammar/definitions/{c,cpp,java,ruby,c-sharp,elixir}.toml`
and `grammars/<lang>/<lang>.toml` for each of the six; modify
`scripts/build-grammar-helper.sh`. Definitions have one source of truth under
`grammars/`; helper copies are generated/checked identical if retained.

```toml
# helpers/grammar/Cargo.toml additions; each build uses --no-default-features.
[features]
c = ["dep:tree-sitter-c"]
cpp = ["dep:tree-sitter-cpp"]
java = ["dep:tree-sitter-java"]
ruby = ["dep:tree-sitter-ruby"]
c-sharp = ["dep:tree-sitter-c-sharp"]
elixir = ["dep:tree-sitter-elixir"]
# optional dependency versions, all pinned exactly and all ABI 14:
# C =0.23.4, CPP =0.23.4, Java =0.23.5, Ruby =0.23.1, C# =0.23.1, Elixir =0.3.5.
```

**ABI (D-ABI):** C 0.24.2 and C# 0.23.5 declare `LANGUAGE_VERSION 15` in
`src/parser.c`, which the `=0.24.7` runtime cannot load; C 0.23.4 and C# 0.23.1 declare 14
(checked by the orchestrator, 20/10, both in the local registry). Use those. Do not edit a
grammar's generated C and do not move the tree-sitter pin. The load test of Step 1 proves each of
the six loads.

- [ ] **Step 1:** put this complete feature-sensitive test in
  `helpers/grammar/src/main.rs`.

```rust
#[cfg(test)]
mod m5b_tests {
    use super::*;
    #[test]
    fn selected_grammar_actually_loads_and_parses() {
        let (name, language) = grammar();
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&language).unwrap_or_else(|e| panic!("{name}: {e}"));
        let source = match name {
            "c" | "cpp" => "int sample(void) { return 1; }",
            "java" => "class Sample { int run() { return 1; } }",
            "ruby" => "class Sample\n def run\n  1\n end\nend\n",
            "c-sharp" => "class Sample { int Run() { return 1; } }",
            "elixir" => "defmodule Sample do\n def run(), do: 1\nend\n",
            "lua" => "function sample() return 1 end",
            other => panic!("unexpected grammar: {other}"),
        };
        let tree = parser.parse(source, None).unwrap();
        assert!(!tree.root_node().has_error(), "{name}: {}", tree.root_node().to_sexp());
    }
}
```

- [ ] **Step 2:** run the complete six-feature loop:

```bash
for lang in c cpp java ruby c-sharp elixir; do
  CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked -p searchfor-grammar --no-default-features --features "$lang" selected_grammar_actually_loads_and_parses
done
```

  Keep each exit status separately (continue collecting all six failures). Expected RED: missing
  features.
- [ ] **Step 3:** implement exactly-one-feature selection with compile_error
  for zero or conflicting Grammar features; default Lua remains for existing
  workspace tests. Queries capture function/class/module names and signatures
  using actual cached node-types, with a tiny fixture per language. Assert
  expected `sample`/`Sample`/`run` names in protocol trails and names replies.
  Each request frees tree/source/Outline; preserve 500/5000-request RSS checks.
  The six pinned crates are ABI 14 (D-ABI), so every load test goes green.
  Rewrite `the_committed_index_is_empty_and_valid` in
  `helpers/grammar/tests/protocol.rs`, which today requires
  `{"version": 1, "grammars": []}`: Task 11.4's gate 3 commits the index with six
  Grammars and the test would start failing, and no other chunk names it. The new
  contract is "valid against the schema, and either empty or holding exactly the
  six names of Q57.gramaticas (c, cpp, java, ruby, c-sharp, elixir)".

Script interface and output are fixed now:

```bash
# scripts/build-grammar-helper.sh LANG OUT_DIR BASE_URL [TARGET]
# TARGET defaults to rustc -vV host; CARGO_TARGET_DIR is inherited.
# cargo build --release --no-default-features --features "$lang" --target "$target"
# uses --message-format=json to find compiler-artifact.executable for target name
# searchfor-grammar; never assumes target/release nor strips an .exe suffix.
# output: searchfor-grammar-<lang>-<os>-<arch>[.exe] and one index Entry JSON.
# derive index key linux/macOS-as-macos/windows + x86_64/aarch64 from TARGET.
```

Use cargo metadata package selection for Grammar version; c-sharp's crate name
is tree-sitter-c-sharp. Compute SHA on exact copied bytes. On cross builds parse
Cargo's target artifact, not a host helper left in target/release. Keep script
Bash invocation explicit in Windows CI; execution of .exe is native.

- [ ] **Step 4:** six load tests and existing helper protocol tests GREEN. Run the script for each Linux feature and the
  Windows GNU target, check suffix and checksum, install one through file://
  index and search it. CI builds six × five release targets in C11; native
  install tests run on the four host systems. 
- [ ] **Step 5:** commit with
  `git commit -m "feat: build six target-aware Grammar Helpers"`.

## Chunk 9: Dense streaming, core

Depends on C1 and C3; owns `src/app.rs`, `src/output/**`,
`src/processors/mod.rs`, `csv.rs`, `keypath/**`, `src/model.rs`,
`src/trail.rs`, `src/filter.rs`. This task's YAML/TOML cursor hooks are handed
off to C10 without overlapping files in tanda 3 (see Task 10.2).

### Task 9.1: ordered Processor API with a model adapter

**Files:** modify `src/processors/mod.rs:21`, `src/model.rs:23`,
`src/trail.rs:170`; create `src/processors/cursor.rs` (C9 owns it).
New object-safe interfaces:

```rust
// model.rs
pub type SharedTrail = std::rc::Rc<[Segment]>;
// processors/mod.rs
pub trait Cursor {
    // Non-decreasing byte offsets. Repeating an offset is allowed.
    fn trail_at(&mut self, byte: usize) -> anyhow::Result<crate::model::SharedTrail>;
}
pub trait Prepared {
    fn cursor(&self) -> Box<dyn Cursor + '_>;
    fn names(&self) -> anyhow::Result<Vec<crate::model::Name>>;
    fn streams(&self) -> bool { false }
}
// Added provided method on Processor, preserving existing implementations:
fn prepare<'a>(
    &'a self, input: crate::model::Input<'a>, params: &'a toml::Table,
) -> anyhow::Result<Box<dyn Prepared + 'a>>;
// Default ModelPrepared borrows Processor/Input/params; cursor delegates the
// existing trails() contract. It is explicitly classified as model fallback.
// Native prepared cursors override streams() with true.
```

Prepared is worker-local, as is Rc; Processor retains Send+Sync. Keep
`Trail = Vec<Segment>` and wire serde contracts unchanged for old model users.
The native OutlinePrepared owns Outline and produces borrowing TrailCursor;
never return a cursor borrowing a temporary Outline from prepare().

- [ ] **Step 1:** append this complete test in processors/mod.rs's tests module.

```rust
#[test]
fn a_legacy_processor_gets_a_working_default_cursor() {
    struct Legacy;
    impl Processor for Legacy {
        fn trails(&self, _: &Input, _: &toml::Table, a: &[usize])
            -> anyhow::Result<Vec<Trail>> {
            Ok(a.iter().map(|_| vec![crate::model::Segment {
                kind: "test".into(), name: "same".into(), line: 1, end_line: 2,
                level: None, signature: None,
            }]).collect())
        }
        fn names(&self, _: &Input, _: &toml::Table) -> anyhow::Result<Vec<Name>> {
            Ok(Vec::new())
        }
    }
    let params = toml::Table::new();
    let input = Input { bytes: b"x\ny\n", path: std::path::Path::new("x"),
                        format: "test", disk: None };
    let legacy = Legacy;
    let prepared = legacy.prepare(input, &params).unwrap();
    let mut cursor = prepared.cursor();
    let a = cursor.trail_at(0).unwrap();
    let b = cursor.trail_at(2).unwrap();
    assert_eq!(a.as_ref(), b.as_ref());
    assert!(std::rc::Rc::ptr_eq(&a, &b));
    assert!(!prepared.streams());
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --lib a_legacy_processor`.
  Expected RED: prepare missing. Test preserves a complete old Processor impl.
- [ ] **Step 3:** implement default adapter without collecting all anchors;
  its per-anchor call is compatibility only, never selected for dense built-ins.
  Use ModelPrepared<P: Processor + ?Sized> so a default method can store &Self
  without a Self: Sized bound that would break dynamic Processor calls.
  Production selects the existing one-shot whole-file model when streams=false;
  it does not send one full helper request per anchor through this adapter.
  Override prepare in the OutlineProcessor blanket impl so Markdown keeps its
  native cursor and prepares its Outline once.
  Cache consecutive equal complete Segments, including kind, range, signature
  and level; sharing by names alone is wrong. Native Outline cursor uses index
  identity and only clones on identity change. Handle non-monotone requests
  explicitly (reset or error), never silently return a stale Trail.
  Add capability enum if needed to distinguish native cursor versus fallback
  from C3's pre-search Format classification without parsing empty input.
- [ ] **Step 4:** command GREEN plus existing Processor, model and trail tests.
  Cross-check Windows; Rc never crosses workers. External/object fallback is
  named and documented (D11); built-in dense families must report streams=true.
- [ ] **Step 5:** commit with
  `git commit -m "refactor: add ordered Processor cursors with model compatibility"`.

### Task 9.2: bound output with a parallel spool and direct JSON writer

**Files:** create `src/output/spool.rs`; modify `src/output/mod.rs`,
`src/output/json.rs:57,109`, `src/app.rs:1668,1873,3207` (`Out`, `spill`, JSON
fallback). New internal interfaces:

```rust
pub const FILE_SPOOL_THRESHOLD: usize = 8 * 1024 * 1024;
pub struct FileSpool { /* memory prefix or unnamed tempfile; no retained duplicate */ }
impl FileSpool {
    pub fn new(dir: &std::path::Path) -> std::io::Result<Self>;
    pub fn is_spilled(&self) -> bool;
    pub fn memory_len(&self) -> usize;
    pub fn copy_to(&mut self, out: &mut dyn std::io::Write) -> std::io::Result<u64>;
}
// FileSpool implements Write. The run owns its tempdir, workers own spools.
// render_with_stats already writes to dyn Write: pass FileSpool/Spill directly.
```

- [ ] **Step 1:** add these complete tests to src/output/spool.rs.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    #[test]
    fn spills_after_eight_mib_and_releases_the_memory_prefix() {
        let d = tempfile::tempdir().unwrap();
        let mut s = FileSpool::new(d.path()).unwrap();
        s.write_all(&vec![b'a'; FILE_SPOOL_THRESHOLD]).unwrap();
        assert!(!s.is_spilled());
        s.write_all(b"z").unwrap();
        assert!(s.is_spilled());
        assert_eq!(s.memory_len(), 0);
        let mut out = Vec::new();
        assert_eq!(s.copy_to(&mut out).unwrap(), FILE_SPOOL_THRESHOLD as u64 + 1);
        assert!(out[..FILE_SPOOL_THRESHOLD].iter().all(|&b| b == b'a'));
        assert_eq!(out[FILE_SPOOL_THRESHOLD], b'z');
        drop(s);
        assert_eq!(std::fs::read_dir(d.path()).unwrap().count(), 0);
    }
    #[test]
    fn one_large_write_never_duplicates_the_whole_output_in_ram() {
        let d = tempfile::tempdir().unwrap();
        let mut s = FileSpool::new(d.path()).unwrap();
        s.write_all(&vec![b'x'; FILE_SPOOL_THRESHOLD * 2]).unwrap();
        assert!(s.is_spilled());
        assert_eq!(s.memory_len(), 0);
        let mut count = std::io::sink();
        assert_eq!(s.copy_to(&mut count).unwrap(), (FILE_SPOOL_THRESHOLD * 2) as u64);
    }
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --lib output::spool::tests`.
  Expected RED: missing spool implementation. Threshold includes exactly 8 MiB
  in memory and spills on the next byte, including a single huge write.
- [ ] **Step 3:** create the file lazily on crossing threshold with
  `tempfile::tempfile_in(dir)` (unnamed, mode 0600, deleted by the OS when closed
  or when `s4` dies, so a crash between the spill and the copy leaves nothing on
  disk), write memory prefix once, release Vec capacity, continue file writes.
  On completion seek to start, acquire stdout lock once for whole file, copy with
  bounded scratch and add emitted byte counts once; dropping the spool releases
  the file. Keep separators atomic
  with the file; sorted/-j1 output keeps existing progressive Spill semantics.
  On BrokenPipe stop quietly and clean temp output. On error preserve already
  emitted bytes only where existing sequential behaviour does; parallel output
  must not become interleaved or silently discard valid partial errors.
  JSON fallback calls render_with_stats directly into the bounded writer;
  remove whole-file Vec then Buffer copy. JSON message scratch may be reused
  but cap retained capacity after exceptional huge messages.
  Windows spool path is used for plain and ANSI buffers only. Console-API
  metadata goes through termcolor; either retain metadata in bounded chunks
  with replay under one file lock or keep existing console Buffer fallback
  explicitly documented (D12). Never copy opaque Wincon bytes as coloured output.
- [ ] **Step 4:** unit tests GREEN; output snapshots unchanged except C1 fixes.
  Linux can validate spool cleanup, max retained prefix, binary output and
  BrokenPipe; Windows CI validates console colours and file atomicity separately.
- [ ] **Step 5:** commit with
  `git commit -m "perf: spool parallel file output and stream JSON fallback"`.

### Task 9.3: incremental rows, filters and every-fixture model differential

**Files:** modify `src/app.rs:1573,1777,2208,2374,2757`,
`src/output/stream.rs:44,125`, `src/output/mod.rs` (`Rows`, `SectionWalk`),
`src/filter.rs:55` (within/context/max), `src/model.rs` (shared transient rows).
Test driver lives in app.rs under cfg(test), avoiding a user-facing flag.

```rust
// app.rs, cfg(test), used by its own unit tests.
#[derive(Clone, Copy)]
enum Pipeline { Model, Stream }
struct Captured { stdout: Vec<u8>, stderr: Vec<u8>, code: i32 }
// Internal per-file command used by the real engine, with injectable dirs.
struct FileCommand {
    args: crate::cli::Args,
    cwd: std::path::PathBuf,
    formats: std::path::PathBuf,
    grammars: std::path::PathBuf,
    index: String,
}
fn execute_file(
    command: &FileCommand, pipeline: Pipeline,
    stdout: &mut dyn termcolor::WriteColor, stderr: &mut dyn std::io::Write,
) -> anyhow::Result<i32>;
fn compare_fixture_command(
    path: &std::path::Path, args: &[&str], pipeline: Pipeline,
) -> Captured;
// In production, select native cursors automatically; Model remains oracle.
```

- [ ] **Step 1:** add the following complete app test driver and unit test.
  execute_file is the production per-file engine seam defined above, implemented
  in Step 3; the driver has no hidden fixture or environment mutations.

```rust
fn compare_fixture_command(path: &std::path::Path, args: &[&str], pipeline: Pipeline) -> Captured {
    let dir = tempfile::tempdir().unwrap();
    let formats = dir.path().join("formats");
    let grammars = dir.path().join("grammars");
    std::fs::create_dir(&formats).unwrap();
    std::fs::create_dir(&grammars).unwrap();
    let mut argv: Vec<std::ffi::OsString> = vec!["s4".into(), "--no-config".into()];
    argv.extend(args.iter().map(std::ffi::OsString::from));
    argv.push(path.as_os_str().to_owned());
    let args = match crate::cli::parse(argv) {
        Ok(a) => a,
        Err(e) => return Captured { stdout: Vec::new(), stderr: format!("{e}").into_bytes(), code: 2 },
    };
    let command = FileCommand { args, cwd: std::env::current_dir().unwrap(),
        formats, grammars, index: format!("file://{}", dir.path().join("missing-index").display()) };
    let mut out = termcolor::Buffer::no_color();
    let mut stderr = Vec::new();
    let code = match execute_file(&command, pipeline, &mut out, &mut stderr) {
        Ok(code) => code,
        Err(e) => { use std::io::Write; writeln!(stderr, "{e}").unwrap(); 2 },
    };
    Captured { stdout: out.as_slice().to_vec(), stderr, code }
}

#[test]
fn m5b_stream_matches_model_for_every_fixture_and_mode() {
    fn files(p: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(p).unwrap() {
            let e = entry.unwrap();
            if e.file_type().unwrap().is_dir() { files(&e.path(), out); }
            else { out.push(e.path()); }
        }
    }
    fn normalized(out: &[u8], json: bool) -> Vec<u8> {
        if !json { return out.to_vec(); }
        fn times(v: &mut serde_json::Value) {
            match v {
                serde_json::Value::Object(m) => {
                    for (key, value) in m {
                        if key == "elapsed" || key == "elapsed_total" {
                            *value = serde_json::json!({"secs":0,"nanos":0,"human":"0.000000s"});
                        } else { times(value); }
                    }
                }
                serde_json::Value::Array(a) => a.iter_mut().for_each(times),
                _ => {}
            }
        }
        let mut result = Vec::new();
        for line in out.split(|&b| b == b'\n').filter(|l| !l.is_empty()) {
            let mut v: serde_json::Value = serde_json::from_slice(line).unwrap();
            times(&mut v);
            serde_json::to_writer(&mut result, &v).unwrap();
            result.push(b'\n');
        }
        result
    }
    let mut paths = Vec::new();
    files(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures"), &mut paths);
    paths.sort();
    let modes: &[&[&str]] = &[
        &["--flat"], &["--tree"], &["--json"], &["--flat", "--stats"],
        &["--tree", "-C2"], &["--flat", "-o"], &["--flat", "--vimgrep"],
        &["--flat", "-r", "X\nY"], &["--tree", "--expand"],
        &["--json", "--expand"], &["--flat", "--within", "Install", "-m1", "-C1"],
        &["--flat", "--in-headings"], &["-c"], &["--count-matches"], &["-l"],
        &["--flat", "--sort", "path"], &["--tree", "-U", "-o"],
        &["--flat", "--null-data"], &["--tree", "--crlf"],
        &["--flat", "--passthru"], &["--flat", "-v"],
    ];
    for path in paths {
        for &mode in modes {
            for pattern in ["needle", "needle\\n[^\\n]*", "ABSENT_SENTINEL"] {
                let mut args = mode.to_vec();
                args.extend(["--color=never", "--no-docs", "--regexp", pattern]);
                let a = compare_fixture_command(&path, &args, Pipeline::Model);
                let b = compare_fixture_command(&path, &args, Pipeline::Stream);
                assert_eq!(a.code, b.code, "{path:?} {args:?}");
                assert_eq!(a.stderr, b.stderr, "{path:?} {args:?}");
                let json = mode.contains(&"--json");
                let strip_stats = |bytes: &[u8]| {
                    let s = String::from_utf8_lossy(bytes);
                    s.lines().filter(|l| !l.ends_with(" seconds") &&
                        !l.ends_with(" seconds spent searching"))
                        .collect::<Vec<_>>().join("\n").into_bytes()
                };
                let (ao, bo) = if mode.contains(&"--stats") {
                    (strip_stats(&a.stdout), strip_stats(&b.stdout))
                } else { (a.stdout, b.stdout) };
                assert_eq!(normalized(&ao, json), normalized(&bo, json), "{path:?} {args:?}");
            }
        }
    }
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --lib m5b_stream_matches_model -- --nocapture`.
  Expected RED: missing driver/selection. Then observe RED by routing a
  non-Markdown family through its unfinished cursor. Invalid flag combinations
  compare errors/codes too; success cases must include every family.
- [ ] **Step 3:** factor the real per-file engine to accept a writer, local
  configuration and explicit pipeline selector for tests. Driver creates local
  Registry/Processors with empty temp configuration; never env::set_var in a
  multithreaded test. Capture real warnings/status and use the same engine for
  both paths, replacing only the pipeline choice. Stream each Match item and
  its submatch Trails into Rows; retain current item and context window, not
  `FileMatches.items`, flattened anchors or Vec<Vec<Trail>> for whole file.
  Preserve TreeState across calls, previous Trail, before-context and pending
  after-context. within filters per submatch; max-count advances after kept
  Matches, JSON stats retain pre-filter search counters. Expand emits each
  closed Section once, assigned by C1's Match-start rule, without retaining
  every row for the file. Large Sections may be scanned from prepared Source
  or spooled; avoid reparsing complete Source per Section or per anchor.
  Names arrive in source order, grouping same-line name submatches and carrying
  decoded-name-to-Source pieces; count/search stats match the model.
  Count/list modes keep existing no-structure fast path and incremental
  filtered count; JSON end/summary and --stats reduce counters at completion.
  --sort orders files and still streams within each file.
- [ ] **Step 4:** command GREEN for all fixture/mode cases; add all synthetic
  m5 multiline, Latin-1, BOM, empty Match and replacement fixtures to the driver.
  Run cli_m5 including m5_22, rg_compat and output tests. Snapshot updates are
  limited to C1's bugs. Native CI repeats differential with host paths.
- [ ] **Step 5:** commit with
  `git commit -m "perf: stream decorated rows through persistent filter state"`.

### Task 9.4: CSV and JSON/JSON Lines cursors

**Files:** modify `src/processors/csv.rs:158,388,454`,
`src/processors/keypath/mod.rs:60`, `json.rs`, `collect.rs`.
C9 prepares keypath hooks needed by C10 without a later edit to these files.

```rust
// Private implementations of Prepared/Cursor.
struct CsvPrepared<'a> { /* borrowed Source, parsed Params, reusable Header */ }
struct CsvCursor<'a> { /* Records, current record, line counter, header cache */ }
struct JsonPrepared<'a> { /* borrowed Source, root boundaries/ancestor metadata */ }
struct JsonCursor<'a> { /* ordered value scanner and current structural stack */ }
// keypath module exposes pub(crate) prepare_syntax(input, params) returning
// native Prepared for all three syntaxes; C10 fills yaml/toml module factories.
```

- [ ] **Step 1:** add this complete test in csv.rs tests.

```rust
#[test]
fn csv_cursor_matches_model_across_multiline_records() {
    let src = b"Name,Plan\nfirst,needle\n\"two\nlines\",needle\nlast,needle\n";
    let params = toml::Table::new();
    let input = crate::model::Input { bytes: src, path: std::path::Path::new("x.csv"),
        format: "csv", disk: None };
    let anchors: Vec<_> = memchr::memmem::find_iter(src, b"needle").collect();
    let expected = Csv.trails(&input, &params, &anchors).unwrap();
    let prepared = Csv.prepare(input, &params).unwrap();
    assert!(prepared.streams());
    let mut cursor = prepared.cursor();
    for (at, trail) in anchors.into_iter().zip(expected) {
        assert_eq!(cursor.trail_at(at).unwrap().as_ref(), trail.as_slice());
    }
}
```

Add this complete test in keypath/mod.rs tests.

```rust
#[test]
fn json_cursor_matches_model_for_repeated_values_and_broken_tail() {
    for src in [
        &b"{\"a\":\"needle\"}\n{\"b\":[\"needle\",\"needle\"]}\n"[..],
        &b"{\"a\":{\"b\":\"needle\"},\"bad\":[\"needle\""[..],
    ] {
        let params = self::params("json");
        let input = Input { bytes: src, path: Path::new("x.jsonl"), format: "json", disk: None };
        let anchors: Vec<_> = memchr::memmem::find_iter(src, b"needle").collect();
        let expected = Keypath.trails(&input, &params, &anchors).unwrap();
        let prepared = Keypath.prepare(input, &params).unwrap();
        assert!(prepared.streams());
        let mut cursor = prepared.cursor();
        for (at, trail) in anchors.into_iter().zip(expected) {
            assert_eq!(cursor.trail_at(at).unwrap().as_ref(), trail.as_slice());
        }
    }
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --lib cursor_matches_model`.
  Expected RED: default adapter reports streams=false. Values/Sections are
  compared deeply, including line/end_line and malformed-tail recovery.
- [ ] **Step 3:** CSV reads header once, scans Records once, waits for quoted
  record close before returning ranges, caches decoded column names lazily,
  and retains only current row fields/Trail. Keep delimiter inference and
  header-name own-label exclusions unchanged. JSON handles multiple root
  values by value boundaries, not blindly splitting at physical newline.
  For one giant object perform one preparation pass for ancestor ends and a
  monotone cursor scan; preserve escapes, hidden array Segments, syntax error
  boundary and Source offsets. Never call outline_for with a growing list of
  anchors on every Match. Use a compact structural index where closure metadata
  is required; release per-root value state after it cannot be referenced.
- [ ] **Step 4:** both tests GREEN; full C9 differential GREEN. Record 50 MiB
  CSV/JSONL flat and JSON memory/time baseline versus sparse/--no-trail for C10;
  dense output with several workers must cross FileSpool threshold and leave
  no whole-file match/Trail vectors in worker state. Note in that test that on
  the shared Linux machine `/tmp` is tmpfs (`df` shows it), so the spool lives in
  RAM there and an RSS measurement does not see the spilled bytes; the test
  checks spill and cleanup, not disk usage, and its `TMPDIR` points at a disk
  directory when one is available.
- [ ] **Step 5:** commit with
  `git commit -m "perf: add record and value cursors for CSV and JSON"`.

## Chunk 10: Dense streaming, the other Processors and the criterion

Depends on C9; owns remaining `src/processors/**` including subtitles, markup,
tree-sitter, external temp cleanup, and YAML/TOML preparation after C9's scaffold
handoff; owns `tests/perf.rs`. C9's mod.rs dispatch already calls those modules'
prepared factories, initially wrappers, so C10 does not edit mod.rs/csv/json.

### Task 10.1: subtitle, markup and tree-sitter prepared cursors

**Files:** modify `src/processors/subtitles.rs:117,203`,
`src/processors/markup/mod.rs:81,263,461`,
`src/processors/treesitter/mod.rs:74,260` and its cursor modules if split;
modify `src/processors/external.rs:170` (`RunTemp` cleanup).

```rust
// Each module exports its factory with C9's signature:
pub(crate) fn prepared<'a>(
    input: crate::model::Input<'a>, params: &'a toml::Table,
) -> anyhow::Result<Box<dyn super::Prepared + 'a>>;
// TreeSitter's factory also borrows its run's cached query/grammar state.
// No new user-facing flags or helper protocol fields.
```

- [ ] **Step 1:** add these complete tests to the respective module test blocks.

```rust
// subtitles.rs
#[test]
fn subtitle_preparation_has_ordered_shared_trails() {
    use crate::processors::Processor;
    let src = b"1\n00:00:01,000 --> 00:00:03,000\nneedle needle\n\n2\n00:00:04,000 --> 00:00:05,000\nneedle\n";
    let params: toml::Table = toml::from_str("syntax = 'srt'").unwrap();
    let input = crate::model::Input { bytes: src, path: std::path::Path::new("x.srt"),
        format: "srt", disk: None };
    let anchors: Vec<_> = memchr::memmem::find_iter(src, b"needle").collect();
    let expected = Subtitles.trails(&input, &params, &anchors).unwrap();
    let prepared = Subtitles.prepare(input, &params).unwrap();
    assert!(prepared.streams());
    let mut c = prepared.cursor();
    let mut last = None;
    for (i, at) in anchors.into_iter().enumerate() {
        let t = c.trail_at(at).unwrap();
        assert_eq!(t.as_ref(), expected[i].as_slice());
        if i == 1 { assert!(std::rc::Rc::ptr_eq(last.as_ref().unwrap(), &t)); }
        last = Some(t);
    }
}
```

```rust
// markup/mod.rs
#[test]
fn markup_preparation_keeps_sibling_numbers_and_implicit_closures() {
    use crate::processors::Processor;
    let params: toml::Table = toml::from_str("syntax = 'html'").unwrap();
    for src in [
        &b"<ul><li>needle<li>needle</ul>"[..],
        &b"<nav><p>needle</p><p>needle</p></nav>"[..],
    ] {
        let input = crate::model::Input { bytes: src, path: std::path::Path::new("x.html"),
            format: "html", disk: None };
        let anchors: Vec<_> = memchr::memmem::find_iter(src, b"needle").collect();
        let expected = Markup.trails(&input, &params, &anchors).unwrap();
        let prepared = Markup.prepare(input, &params).unwrap();
        assert!(prepared.streams());
        let mut c = prepared.cursor();
        for (at, t) in anchors.into_iter().zip(expected) {
            assert_eq!(c.trail_at(at).unwrap().as_ref(), t.as_slice());
        }
    }
}
```

```rust
// treesitter/mod.rs
#[test]
fn code_preparation_uses_one_tree_and_matches_legacy_trails() {
    use crate::processors::Processor;
    let params: toml::Table = toml::from_str(r#"
        grammar = 'rust'
        query = '(function_item name: (identifier) @name) @segment'
        kinds = { function_item = 'function' }
    "#).unwrap();
    let src = b"fn sample() { let needle = 1; let again = needle; }\n";
    let input = crate::model::Input { bytes: src, path: std::path::Path::new("x.rs"),
        format: "rust", disk: None };
    let ts = TreeSitter::default();
    let anchors: Vec<_> = memchr::memmem::find_iter(src, b"needle").collect();
    let expected = ts.trails(&input, &params, &anchors).unwrap();
    let prepared = ts.prepare(input, &params).unwrap();
    assert!(prepared.streams());
    let mut c = prepared.cursor();
    let a = c.trail_at(anchors[0]).unwrap();
    let b = c.trail_at(anchors[1]).unwrap();
    assert_eq!(a.as_ref(), expected[0].as_slice());
    assert_eq!(b.as_ref(), expected[1].as_slice());
    assert!(std::rc::Rc::ptr_eq(&a, &b));
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --lib preparation_ -- --nocapture`.
  Expected RED: model wrappers report streams=false, or incomplete cursor
  metadata differs on a future sibling/closure. The test names are new API tests.
- [ ] **Step 3:** subtitles retain current cue and next cue lookahead; close
  cue before emitting its first Trail, preserving timestamp signature/BOM/CRLF.
  Markup does one preparation pass for sibling totals and explicit/implicit
  closure boundaries, then a monotone cursor. First sibling's `[1]` must be
  known before emitting its Trail; preserve malformed XML's empty tail and
  control-character name escaping. Avoid rescanning parents for each anchor.
  Tree-sitter parses once, runs one ordered query/Outline preparation, then
  uses TrailCursor; keep max_bytes/refusal and malformed tree behaviour.
  Preserve complete Section ends in JSON and expand. Native cursor objects
  free their tree/index/Source borrows at file completion, not run completion.
  The ignored real-corpus tree-sitter test uses SEARCHFOR_RG/CARGO_HOME and
  gets an explicit developer-only reason; unit fixtures stay portable.
  Implement RunTemp bounded retry on Windows sharing violation after helper
  answer, then leave a uniquely named system-temp file for system cleanup.
- [ ] **Step 4:** tests GREEN, then run C9's every-fixture differential and
  cli_m5 including m5_22. Linux checks include malformed and huge sibling/cue
  fixtures; Windows GNU compiles; native CI repeats outputs on all systems.
- [ ] **Step 5:** commit with
  `git commit -m "perf: stream subtitle markup and code Trails after preparation"`.

### Task 10.2: YAML and TOML ordered factories with explicit preparation cost

**Files:** modify `src/processors/keypath/yaml.rs`, `toml.rs` (factories
scaffolded by C9); avoid C9's mod/json/collect files. Test locally in these
modules against existing Keypath::trails; no undocumented model exception.

- [ ] **Step 1:** append this complete shared-style test in yaml.rs, adjusting
  only the module's existing imports by fully qualified names as shown.

```rust
#[test]
fn yaml_second_document_is_known_before_the_first_trail_prints() {
    use crate::processors::{Processor, keypath::Keypath};
    let src = b"a: needle\n---\nb: needle\n";
    let params: ::toml::Table = ::toml::from_str("syntax = 'yaml'").unwrap();
    let input = crate::model::Input { bytes: src, path: std::path::Path::new("x.yaml"),
        format: "yaml", disk: None };
    let anchors: Vec<_> = memchr::memmem::find_iter(src, b"needle").collect();
    let expected = Keypath.trails(&input, &params, &anchors).unwrap();
    let p = prepared(input, &params).unwrap();
    assert!(p.streams());
    let mut c = p.cursor();
    for (at, trail) in anchors.into_iter().zip(expected) {
        assert_eq!(c.trail_at(at).unwrap().as_ref(), trail.as_slice());
    }
}
```

Add this complete TOML test:

```rust
#[test]
fn toml_cursor_preserves_table_arrays_and_broken_tail() {
    use crate::processors::{Processor, keypath::Keypath};
    let src = b"[[products]]\nname='needle'\n[[products]]\nname='needle'\nbroken=[\n";
    let params: ::toml::Table = ::toml::from_str("syntax = 'toml'").unwrap();
    let input = crate::model::Input { bytes: src, path: std::path::Path::new("x.toml"),
        format: "toml", disk: None };
    let anchors: Vec<_> = memchr::memmem::find_iter(src, b"needle").collect();
    let expected = Keypath.trails(&input, &params, &anchors).unwrap();
    let p = prepared(input, &params).unwrap();
    assert!(p.streams());
    let mut c = p.cursor();
    for (at, trail) in anchors.into_iter().zip(expected) {
        assert_eq!(c.trail_at(at).unwrap().as_ref(), trail.as_slice());
    }
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --lib cursor_preserves_table_arrays`
  and the same command with `yaml_second_document`. Expected RED before native
  factories replace wrappers. If C9's wrapper already reports native Outline
  support, assert its single-prepare counter in the test seam and observe RED
  with repeated per-anchor parsing; do not artificially break correct output.
- [ ] **Step 3:** YAML preparation performs the necessary prepass for document
  count and container closure metadata; convert character offsets to bytes
  once. Cursor advances by events in source order and retains equal shared
  Trails. TOML keeps toml_edit's whole Document and recovered-prefix Outline,
  then cursor resolves anchors without reparsing per Match. Their preparation
  memory is permitted and measured separately; their Match/Trail history is
  bounded. If either cannot preserve ordered output, name the exact remaining
  model family here and in final report, with measured §6.3 failure; none is
  expected. External/object adapters remain D11's protocol/binary exception.
- [ ] **Step 4:** both tests GREEN, full keypath tests and differential GREEN;
  native CI checks offsets/encoding. Record prep RSS/time for perf baselines.
- [ ] **Step 5:** commit with
  `git commit -m "perf: prepare YAML and TOML once for ordered Trail cursors"`.

### Task 10.3: seven-family 50 MiB criterion and portable perf execution

**Files:** modify `tests/perf.rs:104,183,1896,2016,2277` (measurement,
time gates, strace cfg, new dense tests). API of existing measurement helpers
stays; add `timing_enabled() -> bool`, based exactly on
SEARCHFOR_PERF_TIMING=1. RSS is bytes; MiB always means 1,048,576 bytes.

- [ ] **Step 1:** append this complete Linux-only dense test and generator.

```rust
#[cfg(target_os = "linux")]
fn dense_m5b_bytes(format: &str) -> Vec<u8> {
    // Equal-length rows make exact 50 MiB without a syntactically broken tail.
    let row = match format {
        "md" => "# needle heading\nneedle body\n".to_owned(),
        "jsonl" => "{\"needle\":\"needle\"}\n".to_owned(),
        "csv" => "needle,needle\n".to_owned(),
        "yaml" => "needle: needle\n".to_owned(),
        "rs" => "fn needle() { let needle = 1; }\n".to_owned(),
        "html" => "<p>needle</p>\n".to_owned(),
        "srt" => "1\n00:00:01,000 --> 00:00:02,000\nneedle\n\n".to_owned(),
        _ => unreachable!(),
    };
    let mut out = Vec::with_capacity(SIZE);
    // A unique match on the same bytes measures Source + preparation.
    let marker = match format {
        "jsonl" => "{\"ONCE_M5B\":\"needle\"}\n",
        "csv" => "ONCE_M5B,needle\n",
        "yaml" => "ONCE_M5B: needle\n",
        "rs" => "// ONCE_M5B needle\n",
        "html" => "<!-- ONCE_M5B needle -->\n",
        "srt" => "1\n00:00:00,000 --> 00:00:00,500\nONCE_M5B needle\n\n",
        _ => "# ONCE_M5B needle\n",
    };
    out.extend_from_slice(marker.as_bytes());
    while out.len() + row.len() <= SIZE { out.extend_from_slice(row.as_bytes()); }
    // Tail padding is outside syntax where possible; exact bytes are identical
    // for dense/one-Match/--no-trail runs, including tolerated malformed tails.
    out.resize(SIZE, b' ');
    out
}

#[test]
#[ignore = "50 MiB RSS criterion; selected explicitly in Linux CI"]
#[cfg(target_os = "linux")]
fn m5b_dense_seven_families_have_bounded_match_retention() {
    let _alone = alone();
    let fx = Fixture::new();
    let mut rust: toml::Table = toml::from_str(include_str!("../formats/rust.toml")).unwrap();
    rust["params"].as_table_mut().unwrap().insert("max_bytes".into(),
        toml::Value::Integer((SIZE + 1) as i64));
    std::fs::write(fx.formats.path().join("rust.toml"), toml::to_string(&rust).unwrap()).unwrap();
    for format in ["md", "jsonl", "csv", "yaml", "rs", "html", "srt"] {
        let name = format!("dense.{format}");
        let bytes = dense_m5b_bytes(format);
        assert_eq!(bytes.len(), SIZE);
        std::fs::write(fx.file(&name), bytes).unwrap();
        let sample = common::run(&mut fx.s4(["--no-docs", "--json", "-m3", "needle", &name]));
        assert!(sample.status.success());
        let messages: Vec<serde_json::Value> = sample.stdout.split(|&b| b == b'\n')
            .filter(|l| !l.is_empty()).map(|l| serde_json::from_slice(l).unwrap()).collect();
        let structured = messages.iter().filter(|m| m["type"] == "match").any(|m| {
            let data = &m["data"];
            assert!(data["format"].is_string(), "Plain fallback for {format}");
            data["trail"].as_array().is_some_and(|a| !a.is_empty()) ||
                data["submatches"].as_array().is_some_and(|a| a.iter().any(|sub|
                    sub["trail"].as_array().is_some_and(|t| !t.is_empty())))
        });
        assert!(structured, "no structured Match in {format}");
        let raw_sample = common::run(&mut fx.s4(["--no-docs", "--json", "--no-trail", "-m1", "needle", &name]));
        for line in raw_sample.stdout.split(|&b| b == b'\n').filter(|l| !l.is_empty()) {
            let v: serde_json::Value = serde_json::from_slice(line).unwrap();
            if v["type"] == "match" { assert!(v["data"].get("trail").is_none()); }
        }
        for layout in ["--flat", "--json"] {
            let args = ["--no-docs", layout, "-j1", "needle|^", name.as_str()];
            let once = ["--no-docs", layout, "-j1", "ONCE_M5B", name.as_str()];
            let raw = ["--no-docs", layout, "--no-trail", "-j1", "needle|^", name.as_str()];
            let (td, rd) = measure_drained(&sfor_bin(), &fx, &args, 0).unwrap();
            let (tp, rp) = measure_drained(&sfor_bin(), &fx, &once, 0).unwrap();
            let (t0, r0) = measure_drained(&sfor_bin(), &fx, &raw, 0).unwrap();
            assert!(rd <= rp + (32 << 20), "{format}/{layout}: dense {rd}, once {rp}");
            if timing_enabled() {
                let factor = if layout == "--json" { 6.0 } else { 4.0 };
                assert!(td.as_secs_f64() <= tp.as_secs_f64() +
                    factor * t0.as_secs_f64() + 0.1,
                    "{format}/{layout}: dense {td:?}, prep {tp:?}, base {t0:?}");
            }
            eprintln!("m5b {format} {layout} dense={td:?}/{rd} once={tp:?}/{rp} raw={t0:?}/{r0}");
        }
    }
}
```

Add this complete multi-worker test in perf.rs as well. It uses the same
bytes in the two density cases, with four workers and JSON output past 8 MiB.

```rust
#[test]
#[ignore = "multi-worker dense RSS; selected explicitly in Linux CI"]
#[cfg(target_os = "linux")]
fn m5b_dense_parallel_output_is_bounded() {
    let _alone = alone();
    let fx = Fixture::new();
    let files = fx.file("parallel");
    std::fs::create_dir(&files).unwrap();
    for n in 0..4 {
        std::fs::write(files.join(format!("{n}.csv")), dense_m5b_bytes("csv")).unwrap();
    }
    let dense = ["--no-docs", "--json", "-j4", "needle|^", "parallel"];
    let once = ["--no-docs", "--json", "-j4", "ONCE_M5B", "parallel"];
    let (_, rd) = measure_drained(&sfor_bin(), &fx, &dense, 0).unwrap();
    let (_, rp) = measure_drained(&sfor_bin(), &fx, &once, 0).unwrap();
    assert!(rd <= rp + (32 << 20), "parallel dense={rd} once={rp}");
}
```

- [ ] **Step 2:** build release. Before running the whole test, measure once the
  `rs` family alone: run `measure_drained` on `dense.rs` with the once pattern
  `ONCE_M5B` (so tree-sitter parses all ~1.5 million lines of the 50 MiB file,
  and the size of that tree is not estimated anywhere in this plan) and record
  `rp`. If `rp` exceeds 4 GiB, the chunk stops there and reports to the
  orchestrator, as a question for Sebastián, whether the code-family fixture
  should be smaller than 50 MiB; it does not let the test discover the limit
  through an out-of-memory kill on this shared machine or a CI runner. Then run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --release --test perf m5b_dense_seven -- --ignored --nocapture`.
  Expected RED before C9/C10: CSV/JSON and others retain per-Match models and
  exceed Rone+32 MiB. Baseline can be run with SEARCHFOR_BIN pointing to saved
  M5a binary. The new code first fails on missing timing_enabled if unimplemented.
- [ ] **Step 3:** first make measure_drained truly discard stdout in a drain
  thread: at base it calls measure_out and retains the complete String, which
  would consume gigabytes in the test parent for dense JSON. Preserve
  measure_out for bounded positive-output assertions. Track byte counts and
  read to EOF, rather than redirecting stdout to /dev/null (which lets rg stop
  at its first Match). Then implement timing_enabled, gate all old performance ratio and
  wall-time ceilings, including strace/time scaling probes, under the opt-in.
  Keep memory, bomb refusal, exit code and correctness assertions active in
  Linux CI. Mark every GNU-time-dependent test cfg_attr outside Linux with
  reason `requires GNU time -v RSS measurement`; cfg strace imports/functions
  Linux-only. Keep portable static benchmark-isolation tests running everywhere.
  Explicitly run ignored memory/refusal tests in CI; `cargo test` alone omits
  them. Add a several-worker dense directory variant using the same seven
  files and Rone/Rdense comparison, verifying whole-file atomicity and spool
  cleanup. C9's spool unit tests verify the 8 MiB threshold independently.
  Pattern `needle|^` matches every physical line, including cue separators,
  while keeping value anchors that exercise Trails. YAML duplicate keys are
  accepted as events by the existing parser; assert that remains structured.
  The temporary Rust Definition above raises max_bytes beyond 50 MiB so code
  preparation is real. The explicit --no-trail baseline is verified by the
  raw JSON smoke assertion above. Require nonempty Trails for every family and verify expected
  Match counts, rather than passing a perf ceiling with a Plain fallback.
  Preparation is time/RSS of the once-pattern command on the same bytes, as
  specified; report R0, Rone and inferred incremental preparation separately.
  The one density inequality `rd <= rp + 32 MiB` is mandatory. (A second one,
  `rd <= r0 + rp + 32 MiB`, was dropped: `rp` already contains `r0`, so it counted
  the base twice and was weaker than the first.)
- [ ] **Step 4:** command GREEN with timing unset (RSS), then repeat with
  SEARCHFOR_PERF_TIMING=1 on a quiet local machine with uptime. Expected GREEN
  at spec factors 4/6 + prep + 0.1 s; any failure is reported and investigated.
  Keep m5_22 green. CI on macOS/Windows lists precise perf exclusions; native
  functional bomb tests in cli_m4 still run on those systems.
- [ ] **Step 5:** commit with
  `git commit -m "test: bound dense memory and gate noisy timing assertions"`.

## Chunk 11: Releases

Depends on C2, C4, C5, C8; owns root Cargo.toml/Cargo.lock for tanda 3,
`dist-workspace.toml`, helpers' Cargo.toml dist metadata,
`.github/workflows/release.yml`, `grammars.yml`, packaging jobs in `ci.yml`,
and conditional musl allocator backing in `src/docs/memory.rs`.
C11 ends in a prepared handoff; actual publication resumes only through the
five gates after final four-system CI.

### Task 11.1: dist configuration, package contents and generated workflow

**Files:** create `dist-workspace.toml`; modify root `Cargo.toml:3`
(version/package include), `Cargo.lock`, `helpers/example/Cargo.toml`,
`helpers/grammar/Cargo.toml` (`package.metadata.dist.dist = false`);
generate `.github/workflows/release.yml`. Add packaging validation script as
embedded workflow bash/Python, avoiding an extra owned script file.
Configuration contract (verify field placement against installed dist 0.33.0):

```toml
[dist]
cargo-dist-version = "0.33.0"
ci = ["github"]
installers = ["shell", "powershell", "homebrew", "npm"]
targets = [
  "x86_64-unknown-linux-musl", "aarch64-unknown-linux-musl",
  "aarch64-apple-darwin", "x86_64-apple-darwin", "x86_64-pc-windows-msvc",
]
unix-archive = ".tar.xz"
windows-archive = ".zip"
github-attestations = true
publish-prereleases = false
tap = "neitanod/homebrew-tap"
npm-package = "searchfor"
# Explicitly include LICENSE-MIT, LICENSE-APACHE, README.md, README.es.md.
# Only s4 is an application; no example/helper binary in s4 archives.
```

- [ ] **Step 1:** use this complete validation test in the packaging CI step
  (also runnable locally on dist plan JSON). It discovers schema recursively so
  field nesting does not depend on an unverified dist manifest layout.

```bash
python3 - "$CARGO_TARGET_DIR/dist-plan.json" <<'PY'
import json, sys
p = json.load(open(sys.argv[1], encoding='utf-8'))
strings = []
def walk(x):
    if isinstance(x, dict):
        for k, v in x.items():
            strings.append(str(k)); walk(v)
    elif isinstance(x, list):
        for v in x: walk(v)
    elif isinstance(x, str): strings.append(x)
walk(p)
text = '\n'.join(strings)
for target in ['x86_64-unknown-linux-musl', 'aarch64-unknown-linux-musl',
               'aarch64-apple-darwin', 'x86_64-apple-darwin', 'x86_64-pc-windows-msvc']:
    assert target in text, target
for installer in ['shell', 'powershell', 'homebrew', 'npm']:
    assert any(installer in s for s in strings), installer
assert not any('searchfor-example-helper' in s for s in strings)
assert not any('searchfor-grammar-' in s for s in strings)
assert '1.0.0' in text
print('five targets, four installers, only s4: PASS')
PY
```

- [ ] **Step 2:** run `dist plan --output-format=json` through the installed
  binary, write output inside CARGO_TARGET_DIR, run validator. Expected RED
  at base: dist configuration absent/only default target or 0.5.0 version.
  Record generator errors rather than fabricating a plan JSON to satisfy it.
- [ ] **Step 3:** set version 1.0.0 and package includes for src, formats,
  grammars/index.json, licenses, both READMEs and required build metadata.
  Exclude large synthetic tests/fixtures from crate but preserve tests used by
  cargo package verification if included; check `cargo package --list` and
  embedded include_str paths. Set helper dist=false without changing publish=false.
  Run installed `dist init --yes --skip-generate --ci github --hosting github`
  with all five --target and all four --installer options, reconcile config,
  then `dist generate`. Never hand-write a substitute release graph.
  Keep release trigger restricted to `v*`; C11 grammars workflow owns grammars-v*.
  Generated job patches keep NPM_TOKEN and HOMEBREW_TAP_TOKEN in publish jobs
  only, permissions contents:write/id-token:write/attestations:write only where
  required. Stable publish dependencies wait for successful builds/tests.
  Ensure prerelease classification and `publish-prereleases=false` actually
  prevent npm/tap/crate publication on RC. Crate publication stays manual.
  Generated files have a regeneration procedure and checked `dist generate`
  diff so future regeneration cannot silently remove safety conditions.
- [ ] **Step 4:** plan validator GREEN. Run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo package --offline --locked --allow-dirty`
  and locally `dist build --target x86_64-unknown-linux-gnu` as a host-only probe
  if supported, without adding GNU to configured release targets. The local probe
  uses the `x86_64-unknown-linux-gnu` target because this machine cannot build
  musl (no musl C toolchain); the musl archive is built and verified only in the
  Linux CI job, where `dist` installs `musl-tools`. Unpack the probe archive and
  run its own s4 --version, expected `s4 1.0.0`; CI does the same with the musl
  archive. Verify file listing contains one executable + four docs/licenses.
  Do not run `release.yml` by `workflow_dispatch` with a real tag (see the limits
  at the top); validate it with `dist plan` and the build-only paths.
- [ ] **Step 5:** commit with
  `git commit -m "build: prepare version 1.0 distribution with dist"`.

### Task 11.2: musl measurement and conditional allocator

**Files:** modify `src/docs/memory.rs` only if measured necessary;
root Cargo.toml/Cargo.lock add target-specific tikv-jemallocator only then.
No new allocator in helpers; the library retains exactly one global Counter.

```rust
// memory.rs backing alias, if threshold requires it:
#[cfg(all(target_os = "linux", target_env = "musl"))]
use tikv_jemallocator::Jemalloc as Backing;
#[cfg(not(all(target_os = "linux", target_env = "musl")))]
use std::alloc::System as Backing;
// Counter delegates alloc/dealloc/realloc/alloc_zeroed to this backing;
// accounting/refused/arm semantics remain byte-for-byte the same contract.
```

- [ ] **Step 1:** the measurement runs in the Linux CI job, not on this machine:
  the runner installs `musl-tools`, while this machine has no musl C toolchain
  (`cargo build -p pcre2-sys --target x86_64-unknown-linux-musl` fails here with
  `failed to find tool "x86_64-linux-musl-gcc"`) and nothing is installed with
  sudo. Add a Linux CI step that builds the GNU and musl release binaries in the
  same target dir (sequentially) and runs
  `scripts/bench-m5b.sh --compare <glibc-s4> <musl-s4>` (Task 3.1) in the same job
  on the same runner, so the 40 pairs are alternated within one process and the
  runner noise cancels in part in the relative ratio; same thread counts and
  priority, `uptime` printed, and the median musl/GNU ratio plus the raw samples
  go to the job log. The decision rule is unchanged (a ratio above 1.1 triggers
  jemalloc), with one addition: a ratio inside the band 1.05 to 1.15 is not
  decided alone, because spec §5.1 calls shared runners too noisy for ceilings
  this tight; it is reported to Sebastián with the raw samples, who either
  accepts the call or installs `musl-tools` on this machine so the same
  `--compare` runs here. The result is recorded in this plan and in the commit
  body, copied from that CI log, as a CI-evidence entry.
  Complete evaluation test for the measurement decision:

```bash
python3 - <<'PY'
def needs_allocator(musl, glibc):
    return musl / glibc > 1.1
assert not needs_allocator(1.10, 1.0)
assert needs_allocator(1.10001, 1.0)
print('strict musl >1.1 trigger: PASS')
PY
```

- [ ] **Step 2:** actual baseline RED for optimization is ratio >1.1. If ≤1.1,
  record it, skip allocator mutation, retain System and pass this task through
  measurement evidence. Do not add jemalloc merely because rg uses it.
- [ ] **Step 3:** if triggered, add tikv-jemallocator 0.7 only under Linux musl
  target dependencies, wrap Backing inside Counter; preserve count refusal for
  fallible/infallible allocations, realloc shrink/grow and counter disabled
  fast path. Reuse existing memory.rs tests; add this complete backing smoke:

```rust
#[test]
fn selected_backing_can_allocate_reallocate_and_free() {
    use std::alloc::{GlobalAlloc, Layout};
    let allocator = Counter::new();
    let small = Layout::from_size_align(32, 8).unwrap();
    unsafe {
        let p = allocator.alloc(small);
        assert!(!p.is_null());
        p.write_bytes(0x5a, 32);
        let q = allocator.realloc(p, small, 64);
        assert!(!q.is_null());
        assert_eq!(std::slice::from_raw_parts(q, 32), &[0x5a; 32]);
        allocator.dealloc(q, Layout::from_size_align(64, 8).unwrap());
    }
}
```

- [ ] **Step 4:** run memory tests locally under GNU with the common cargo
  prefix and under musl in the Linux CI job, then repeat the paired benchmark
  there. Expected GREEN ratio ≤1.1 and unchanged m4_22/memory warning. Linux aarch64 static linkage and runtime are release CI
  checks. Verify no dynamic libc dependency via readelf; record unstripped
  Linux x86-64 release size ≤23,000,000 bytes. A missed threshold or size cap is
  reported, not loosened. No mimalloc/extra crate is authorized here.
- [ ] **Step 5:** commit the measured report (from the CI log) in the commit body
  and any conditional changes with `git commit -m "perf: validate static Linux allocator and reader limits"`.
  If no file change is needed, attach measurement evidence to chunk handoff;
  do not create an empty commit solely for this step.

### Task 11.3: local installers, package smoke tests and attestations

**Files:** modify `.github/workflows/ci.yml` (new packaging jobs) and generated
release.yml attestation coverage. Tests are complete job command blocks below;
setup may fetch tools, the smoke commands use only local artifacts.

- [ ] **Step 1:** add this complete archive smoke test to the appropriate
  packaging job after native dist build (ARTIFACT_DIR is its unpacked archive).

```bash
python3 - "$ARTIFACT_DIR" <<'PY'
from pathlib import Path
import os, subprocess, sys
root = Path(sys.argv[1])
files = [p for p in root.rglob('*') if p.is_file()]
for name in ['LICENSE-MIT', 'LICENSE-APACHE', 'README.md', 'README.es.md']:
    assert any(p.name == name for p in files), name
exes = [p for p in files if p.name in ('s4', 's4.exe')]
assert len(exes) == 1
v = subprocess.check_output([str(exes[0]), '--version'], text=True).strip()
assert v == 's4 1.0.0', v
assert not any('grammar' in p.name or 'example-helper' in p.name for p in files)
print('native archive smoke: PASS')
PY
```

- [ ] **Step 2:** run against missing/base archive; expected RED absent package
  or old version. On each host the built binary must be executed from archive,
  not accidentally from target/release or PATH.
- [ ] **Step 3:** prepare native packaging jobs for `dist plan`, `dist build`,
  macOS Homebrew, npm and cargo publish dry-run. Generate `.sha256` per archive
  and sha256.sum covering every archive. Test corruption rejection by flipping
  one byte in a copied archive and requiring verification failure.
  Formula smoke rewrites the generated formula's URL/checksum to the job's
  own `file://` archive and runs the exact install command below. Keep the
  original generated formula for the release; the local fixture copy is untracked.

```bash
brew install --formula ./searchfor.rb
"$(brew --prefix)/bin/s4" --version
# Expected: s4 1.0.0 from the local-archive formula.
```

For npm inspect dist's generated installer to identify its download base hook.
Prepare a local package copy with archive URLs/checksums pointing to a temporary
loopback HTTP server, or file assets supported by its installer. Supply every
requested asset locally, deny outbound network, then execute:

```bash
npm pack --ignore-scripts
npm install --prefix "$NPM_SMOKE_DIR" ./searchfor-1.0.0.tgz
"$NPM_SMOKE_DIR/node_modules/.bin/s4" --version
# Expected: s4 1.0.0, install made zero external requests.
```

On Windows invoke the generated .cmd shim. --ignore-scripts applies only to
pack, not install; the test must exercise installation/checksum logic. Repeat
with a corrupt local archive and require nonzero install status.
Crate setup populates registry cache; the test runs:

```bash
CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo package --offline --locked --allow-dirty
CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo publish --dry-run --offline --locked --allow-dirty
```

Expected no upload, package builds using included formats/index/licenses/docs.
If Cargo's dry-run still requires index metadata, preload it in setup and fail
rather than permitting tests to fetch it. Check dist plan has four installers.
Attest every archive, installer script, per-archive checksum and sha256.sum;
inspect generated subjects rather than assuming github-attestations covers
scripts/checksums. Add explicit attest-build-provenance steps for omissions.
Local dist builds cannot produce GitHub OIDC attestations; RC gate verifies
actual remote attestations before stable publication.

- [ ] **Step 4:** all local smoke commands GREEN with outbound network denied;
  macOS formula and Windows npm/native archive tests GREEN only in their CI
  jobs. `cargo publish --dry-run` never becomes `cargo publish` in CI.
  Ensure stable publish jobs consume the exact validated artifacts.
- [ ] **Step 5:** commit with
  `git commit -m "ci: verify local installers packages and attestation subjects"`.

### Task 11.4: Grammar release artifacts and schema validation

**Files:** create `.github/workflows/grammars.yml`; index update is generated
as a CI artifact, and committed only after gate 3. Modify grammars/index.json
only at that authorized later step; keep it empty until then.

- [ ] **Step 1:** add this complete workflow artifact validation step. The
  merged staging index includes six Entries, each with exactly five system keys.

```bash
python3 - "$GRAMMAR_ARTIFACT_DIR/index.json" <<'PY'
import json, re, sys
x = json.load(open(sys.argv[1], encoding='utf-8'))
assert x['version'] == 1
names = {'c','cpp','java','ruby','c-sharp','elixir'}
systems = {'linux-x86_64','linux-aarch64','macos-aarch64','macos-x86_64','windows-x86_64'}
assert {e['name'] for e in x['grammars']} == names
for e in x['grammars']:
    assert set(e['binaries']) == systems
    assert e['name'] in e['definition']
    for system, b in e['binaries'].items():
        assert re.fullmatch('[0-9a-f]{64}', b['sha256'])
        assert '/releases/download/grammars-v1/' in b['url']
        assert b['url'].endswith('.exe') == system.startswith('windows-')
print('six grammars, thirty target binaries: PASS')
PY
```

- [ ] **Step 2:** run against current grammars/index.json (expected RED: no
  Entries). Never edit the committed index to pretend released assets exist.
- [ ] **Step 3:** publication triggers only grammars-v*. Also expose a
  workflow_call or workflow_dispatch build-only path from branch CI, with
  publish=false and no contents:write permission; it produces the exact same
  thirty binaries/index as artifacts before any publication approval.
  Build six independent
  --no-default-features jobs for each of five targets, use C8 artifact discovery,
  hash after copying, upload per-job Entries/binaries, then merge deterministically
  sorted by name/system, reject duplicate/missing keys. Validate against
  docs/grammar-index.schema.json using existing Rust jsonschema in CI test or a
  setup-installed validator; tests validate local JSON only. Matrix job failure
  prevents any upload of a public release/index. Index artifact uses final
  grammars-v1 URLs but remains uncommitted/unpublished until gate 3.
  Native host smoke installs one helper per language from a file:// index
  constructed from the job's binaries; verify name/signature/Section outputs.
  For Linux arm64 use native arm64 runner or explicit cross build + execution
  environment in setup; a linker-only build is insufficient to claim runtime.
  CI verifies all checksums and no-network install on four native systems.
- [ ] **Step 4:** validator and schema GREEN on merged artifact; all thirty
  binaries build after C8 ABI resolution. Helpers/example are absent from s4
  archives. Installation corruption/rollback tests remain GREEN.
- [ ] **Step 5:** commit with
  `git commit -m "ci: build six Grammar Helpers for all release targets"`.

### Task 11.5: five publication gates, in spec order

**Files:** review generated release.yml, grammars.yml, dist-workspace.toml,
Cargo.toml, staged grammar index. This task records external actions after
approval; it never conflates preparing artifacts with permission to publish.
Before gate 1, final docs, native CI, acceptance tables and package smoke tests
must be reviewable. Remaining ABI or packaging blockers are reported explicitly.

- [ ] **Step 1 — prepare:** assemble commit SHA, passing native job links,
  five archives + docs/licenses manifests, checksums, dist plan, formula,
  npm tarball, crate dry-run output and Grammar artifact manifest. Provide
  exact intended repository/tag/package versions and rollback consequences.
- [ ] **Ask Sebastián 1 — create tap and secrets:** ask to create
  `neitanod/homebrew-tap`, `HOMEBREW_TAP_TOKEN`, `NPM_TOKEN`. Stop the chunk and
  report. Only Sebastián creates the tokens; never print token values or store
  them in tracked files. Continue after his explicit OK and secret setup.
- [ ] **Step 2 — prepare RC:** ensure RC tag resolves to the reviewed commit.
  dist may require a prerelease package version or --force-tag to reconcile
  1.0.0 versus v1.0.0-rc.1; test `dist plan --tag v1.0.0-rc.1 --force-tag`
  locally and inspect every publish-prereleases guard before proposing push.
  Do not change the stable package version irreversibly merely for the test.
- [ ] **Ask Sebastián 2 — push first RC tag:** propose `v1.0.0-rc.1`, which
  creates a public prerelease and attestations, with no npm/tap/crate publish.
  Stop and report. After OK the orchestrator pushes that tag and reads every
  job; verify every archive/script/checksum subject with
  `gh attestation verify <file> --repo neitanod/searchfor`. A failed verification
  blocks all later publication. Download and verify all five target archives;
  execute each on its native platform and record provenance commit/workflow.
- [ ] **Step 3 — prepare Grammar release:** present thirty built binaries,
  exact SHA-256s, validated index diff, grammars-v1 asset names and file://
  smoke results. List what committing the index changes: besides the
  discoverability of six Grammars, the test
  `the_committed_index_is_empty_and_valid` in `helpers/grammar/tests/protocol.rs`,
  which Task 8.3 rewrote to accept either an empty index or exactly the six
  Q57.gramaticas names, must pass with the committed index. Check installable URLs will exist before index commit.
- [ ] **Ask Sebastián 3 — publish Grammar assets and index:** approve uploading
  the six × five Grammar binaries to `grammars-v1` and pushing the index that
  points at them. Stop and report. After OK publish assets, verify remote bytes
  match hashes, commit/push grammars/index.json through the orchestrator.
  An index commit alone makes a Grammar discoverable; assets must precede it.
- [ ] **Step 4 — prepare stable:** rebuild/plan at the exact stable commit
  including released Grammar index and final bilingual docs. Check all packages
  still print 1.0.0 and provenance subjects match. RC approval does not authorize
  stable publication. Present Homebrew/npm irreversible version effects.
- [ ] **Ask Sebastián 4 — push stable v1.0.0:** approve GitHub Release, tap
  formula update and npm `searchfor@1.0.0`. Stop and report. After OK the
  orchestrator pushes v1.0.0, monitors publication and verifies attestations
  and installed channel binaries. Crate remains unpublished at this step.
- [ ] **Step 5 — prepare crate:** show final package listing, dry-run output,
  name/version and included formats/index/docs/licenses. State that a crates.io
  version can be yanked but never deleted or reused.
- [ ] **Ask Sebastián 5 — cargo publish:** stop and report before executing
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo publish --locked`.
  Only after explicit OK publish the reviewed package, then verify its version
  and package checksum. This command is never run as a test or prerelease job.
- [ ] **Step 6 — commit/report:** commit any authorized index/version/docs
  changes with `git commit -m "release: record approved version 1.0 artifacts"`.
  Report each channel's actual result and any partial publication separately.

## Chunk 12: Docs

Depends on C8; owns `docs/external-processors.md`, its `.es.md` copy,
`docs/examples/**`, new `tests/guide.rs`, README.md, README.es.md,
`skills/searchfor/SKILL.md`, `docs/rg-compat.md` if behaviour deviations are found,
and the four v1-spec amendments. It consumes actual C2/C5/C8 behaviour and
C11's package contract; publication waits for these final docs.

### Task 12.1: executable External Processor guide examples

**Files:** create `docs/external-processors.md`,
`docs/external-processors.es.md`, `docs/examples/ini-helper.py`,
`ini-pre.py`, `pre-launcher.rs`, `ini.toml`, `sample.ini`; create `tests/guide.rs`.
The Python scripts have this executable contract (no shell/chmod needed):

```python
# ini-pre.py PATH: UTF-8 text conversion, output only values, one per line.
# ini-helper.py: protocol v1 oneshot; hello -> protocol=1; trails -> one Trail
# per supplied byte anchor; names -> WireName-compatible entries. JSON-lines
# input/output, stderr diagnostics, EOF completes the request batch.
# Fixture sample.ini:
# [server]
# host = needle.example
# Portability: invoke [PYTHON, SCRIPT] as the Helper Definition command (no shebang there; --pre recipes are below).
```

- [ ] **Step 1:** create tests/guide.rs with this complete test code.

```rust
mod common;
use common::{Fixture, code, run, stderr, stdout};
use std::path::Path;
use std::process::Command;

fn python() -> Option<String> {
    for name in ["python3", "python"] {
        if Command::new(name).args(["-c", "import sys; assert sys.version_info.major == 3"])
            .status().is_ok_and(|s| s.success()) { return Some(name.into()); }
    }
    None
}

#[test]
fn m5b_external_processor_guide_examples_run_as_documented() {
    let Some(py) = python() else {
        eprintln!("skipping guide examples: Python 3 is not installed");
        return;
    };
    let fx = Fixture::new();
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/examples");
    std::fs::copy(examples.join("sample.ini"), fx.file("sample.ini")).unwrap();
    let script = examples.join("ini-helper.py");
    let command = vec![py.clone(), script.to_string_lossy().into_owned()];
    let mut def = toml::Table::new();
    def.insert("name".into(), "ini-guide".into());
    def.insert("extensions".into(), toml::Value::Array(vec!["ini".into()]));
    def.insert("processor".into(), "external".into());
    let mut params = toml::Table::new();
    params.insert("command".into(), toml::Value::Array(command.into_iter().map(Into::into).collect()));
    params.insert("mode".into(), "oneshot".into());
    def.insert("params".into(), toml::Value::Table(params));
    std::fs::write(fx.formats.path().join("ini-guide.toml"), toml::to_string(&def).unwrap()).unwrap();
    let out = run(&mut fx.s4(["--flat", "--format", "ini-guide", "needle", "sample.ini"]));
    assert_eq!(code(&out), 0, "{}", stderr(&out));
    assert_eq!(stderr(&out), "");
    assert_eq!(stdout(&out), "2: server › host │ host = needle.example\n");
    let pre = Command::new(&py).arg(examples.join("ini-pre.py"))
        .arg(fx.file("sample.ini")).output().unwrap();
    assert!(pre.status.success());
    assert_eq!(pre.stdout, b"needle.example\n");
    let launcher = fx.file(&format!("ini-pre-launcher{}", std::env::consts::EXE_SUFFIX));
    let compiled = Command::new("rustc").arg("--edition=2024")
        .arg(examples.join("pre-launcher.rs")).arg("-o").arg(&launcher)
        .status().unwrap();
    assert!(compiled.success());
    let converted = run(fx.s4(["--no-docs", "--no-trail", "--pre"])
        .arg(&launcher).args(["needle", "sample.ini"])
        .env("SEARCHFOR_EXAMPLE_PYTHON", &py)
        .env("SEARCHFOR_EXAMPLE_CONVERTER", examples.join("ini-pre.py")));
    assert_eq!(code(&converted), 0, "{}", stderr(&converted));
    assert_eq!(stdout(&converted), "needle.example\n");
    assert_eq!(stderr(&converted), "");
    let doc = std::fs::read_to_string(examples.parent().unwrap().join("external-processors.md")).unwrap();
    assert!(doc.contains("2: server › host │ host = needle.example"));
    assert!(doc.contains("needle.example"));
}
```

Also add this complete protocol transcript test in guide.rs:

```rust
#[test]
fn m5b_guide_names_and_errors_keep_request_ids() {
    use std::io::Write;
    let Some(py) = python() else {
        eprintln!("skipping guide protocol: Python 3 is not installed"); return;
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/examples");
    let mut c = Command::new(py).arg(root.join("ini-helper.py"))
        .stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped())
        .spawn().unwrap();
    let mut input = c.stdin.take().unwrap();
    for v in [
        serde_json::json!({"id":0,"op":"hello","protocol":1,"format":"ini-guide","params":{},"threads":1}),
        serde_json::json!({"id":8,"op":"names","path":root.join("sample.ini")}),
        serde_json::json!({"id":9,"op":"unknown"}),
    ] { writeln!(input, "{v}").unwrap(); }
    drop(input);
    let out = c.wait_with_output().unwrap();
    assert!(out.status.success());
    let replies: Vec<serde_json::Value> = out.stdout.split(|&b| b == b'\n')
        .filter(|l| !l.is_empty()).map(|l| serde_json::from_slice(l).unwrap()).collect();
    assert_eq!(replies.len(), 3);
    assert_eq!(replies[0]["protocol"], 1);
    assert_eq!(replies[1]["id"], 8);
    assert!(replies[1]["names"].as_array().unwrap().iter().any(|n| n["text"] == "host"));
    assert_eq!(replies[2]["id"], 9);
    assert!(replies[2]["error"].is_string());
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test guide m5b_external -- --nocapture`.
  Expected RED: example/doc files absent. CI installs Python so this test
  executes; optional local absence is printed and listed in platforms.md.
- [ ] **Step 3:** implement complete scripts, preserving byte offsets rather
  than Python character positions, names pieces and one Trail per anchor.
  oneshot reads all stdin then writes hello/request answers with matching ids;
  unknown op returns request error. Demonstrate hello/trails/names/error
  messages verbatim in both languages, mode choice, timeout/failure fallback,
  disabling bad hello, restart, resource release between files and concurrent
  ids. Document all Definition fields and safe temporary paths/permissions.
  Show the same Format first through --pre (converted text, no structural
  Trail), then through the Helper Processor (server › host). --pre takes one
  executable: pre-launcher.rs runs SEARCHFOR_EXAMPLE_PYTHON with
  SEARCHFOR_EXAMPLE_CONVERTER and its own PATH argument, inherits the three
  standard streams, and returns the Python process exit status. The test
  compiles that native launcher and executes the actual s4 --pre recipe on
  every host. Document `rustc docs/examples/pre-launcher.rs -o ini-pre-launcher`
  and the two environment variables. Document both recipes by system: on Unix
  the natural recipe is a script with a shebang line (`#!/usr/bin/env python3`,
  executable bit), which is how rg users already write `--pre`; on Windows
  there is no shebang, so the compiled launcher is the recipe. The test keeps
  compiling the launcher on all four systems.
- [ ] **Step 4:** guide test GREEN on Linux and all four CI systems with Python;
  run `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test guide m5b_guide_names`
  for the complete names/error transcript test as well.
  Verify the Spanish copy shows the same flags, JSON fields and output.
- [ ] **Step 5:** commit with
  `git commit -m "docs: add runnable bilingual External Processor guide"`.

### Task 12.2: installation docs, platform differences and spec amendments

**Files:** modify README.md, README.es.md, skills/searchfor/SKILL.md,
docs/rg-compat.md if CI establishes deviations,
docs/specs/2026-10-16-searchfor-design.md (§3 item 5, §7, §10, §11);
modify tests/guide.rs (doc contract test).

- [ ] **Step 1:** add this complete doc test to guide.rs.

```rust
#[test]
fn m5b_distribution_docs_and_spec_cover_the_v1_contract() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for file in ["README.md", "README.es.md"] {
        let doc = std::fs::read_to_string(root.join(file)).unwrap();
        for command in ["cargo install searchfor", "npm install -g searchfor",
                        "brew install neitanod/tap/searchfor",
                        "gh attestation verify", "--install-grammar", "--no-docs"] {
            assert!(doc.contains(command), "{file}: {command}");
        }
        for system in ["Windows", "macOS", "Linux"] { assert!(doc.contains(system)); }
    }
    let skill = std::fs::read_to_string(root.join("skills/searchfor/SKILL.md")).unwrap();
    assert!(skill.contains("Windows"));
    assert!(skill.contains("--within"));
    let spec = std::fs::read_to_string(root.join("docs/specs/2026-10-16-searchfor-design.md")).unwrap();
    assert!(spec.contains("32 MiB"));
    assert!(spec.contains("grammars-v1"));
    for number in 34..=42 {
        assert!(spec.lines().any(|l| l.starts_with(&format!("{number}."))), "{number}");
    }
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test guide m5b_distribution_docs`.
  Expected RED: old distribution text/criteria and missing verification commands.
- [ ] **Step 3:** README includes exact supported native/release architectures,
  static Linux binaries, all four channels plus generated shell/PowerShell
  installers, licenses, checksum and provenance verification. Explain
  platform config/data dirs, rg path/hyperlink drive behaviour, hidden attribute,
  Windows ANSI/console distinction, macOS search-mmap versus preparation-mmap,
  reader memory protection, Windows Ctrl-C and arm64 macOS PCRE2 JIT limits.
  Skill adds Windows quoted path recipes and s4 versus searchfor naming.
  English/Spanish agree; published URLs/names are verified at release gates.
  Amend old §3 item 5 and #13 to the bounded §6.3 criterion, §7 to the six
  Grammar Helpers/index/platform names, §10 to five targets/channels/attestations
  and staged approvals, §11 append criteria 34–42 verbatim in meaning. Preserve
  earlier accepted criteria and deviations; link the M5b design as amendment.
  Do not extend M6 scope or promise Windows arm64/notarization in v1.0.
- [ ] **Step 4:** doc test GREEN, guide examples GREEN, format/check and
  manual bilingual review. docs/rg-compat.md receives only established native
  differences with a reproducing test; unverified predictions remain risks.
- [ ] **Step 5:** commit with
  `git commit -m "docs: describe version 1 installation platforms and acceptance"`.

## CI on the four systems — orchestrator stage

This is an orchestrator stage after tanda 3, **not a chunk**. Chunk agents
commit and report locally; they do not push branches on their own.

- [ ] Merge the three tandas, verify single-owner file changes and run the full
  Linux checks, Windows GNU --all-targets check, references and local performance
  gates. Preserve release artifacts/reports outside disposable target trees.
- [ ] Confirm every official rg digest is verified, workflows are committed,
  and setup dependencies are available. Push only the task branch when the
  orchestrator authorizes it; this creates no release tag or channel package.
- [ ] Read the four named native jobs: ubuntu-24.04 x86-64, macos-15 arm64,
  macos-15-intel x86-64, windows-2022 MSVC x86-64. Record actual job URLs,
  target/toolchain, reference version, test counts and inventory exclusions.
- [ ] Triage failures in batches by owner. Start each fix with a red regression
  on the failing system or a portable reduced equivalent; then implement,
  rerun that job and the Linux preservation tests. Do not accept a cfg skip for
  an ordinary path/shell/line-ending portability problem.
- [ ] Anticipate deduced failures from the static research: Darwin waitid/poll
  differences, setrlimit refusal, kqueue registration race; MSVC warning cleanup,
  nested Jobs under Actions, console-API colour replay, named/anonymous pipe
  disconnect detection, Windows open-executable rename and temp deletion;
  Python/sqlite/compressor availability; macOS script PTY options and arm64
  PCRE2 JIT expectation; fixture line endings and path/OSC-8 byte expectations.
  Reproduce each before claiming it fixed.
- [ ] Run explicit Linux RSS/bomb tests and short fuzz jobs; inspect nightly
  no-change gate with same/different SHA, scheduled/manual events and a failed
  previous run. Confirm failure artifacts contain replayable seed and bytes.
- [ ] Read the Linux job's musl-versus-glibc ratio (Task 11.2) from its log and
  record it in the plan; above 1.1 sends C11 back to add jemalloc.
- [ ] Verify packaging native jobs, local Homebrew/npm installs, crate dry-run,
  all five archives/30 Grammar artifacts and the acceptance matrix below.
- [ ] Report GREEN/remaining blockers to Sebastián. Resume C11's individual
  Ask Sebastián gates only after this report. A successful branch push is not
  approval for the RC, stable tag, index publication or cargo publish.

## Acceptance criteria 34–42 and spec coverage

| Criterion | Required evidence | Tasks |
|---|---|---|
| 34 | Four native suites with REQUIRE_REFERENCE=1; skip inventory/reasons/counts | 1.2, 2.3, 3.3, 4.1–4.3, orchestrator CI |
| 35 | Native --no-docs --no-trail reference bytes, both rg compatibility files | 3.2, 3.3, 4.1, 4.3 |
| 36 | Windows memory warning, real console Ctrl-C leaves no readers/helpers; macOS m4_22 | 2.1, 2.3, 5.1, 5.2 |
| 37 | Seven 50 MiB families, flat/JSON RSS and local opt-in time; m5_22 | 9.1–9.4, 10.1–10.3 |
| 38 | Forty paired M4 text runs, all three 1.05 gates | 3.1, 3.2 |
| 39 | dist five targets/four installers, archive version, formula/npm/crate dry-run CI | 11.1–11.3 |
| 40 | Six × five Grammar builds, schema, file:// install and Trails on native systems | 8.2, 8.3, 11.4 |
| 41 | Both worked guide examples execute with documented output | 12.1 |
| 42 | Each M5a finding has its own red regression | 1.1 |

| M5b section | Coverage |
|---|---|
| §1 Goal | All chunks plus orchestrator CI; C11 gates finish publication |
| §2 Scope | Header/D15/D17, C8 six extras, C12 preserves M6 boundary |
| §3 Leftovers | C1.1, three independent regressions |
| §4.1 Build/suite/skips/reference | C4.1/C4.3, all per-task native checks |
| §4.2 Platform behaviour | C1 links, C2 Ctrl-C/parents, C3 mmap/colours/paths, C5 readers, C6 MCP, C8 pre/dirs/temp cleanup handoff to C10 |
| §4.3 Job Object | C2.1/C2.3, C5.1, C6.1, C8.1 |
| §4.4 Inventory | C1.2/C2.3/C3.3/C4.3/C5.2/C10.3 |
| §5.1 Workflows/fuzz/perf | C4.2/C4.3, C10.3 |
| §5.2 rg | C4.1/C4.3; C1/C3/C8/C10 own-file resolver edits |
| §5.3 Push | Header and orchestrator CI stage |
| §6.1 Ordered cursors/modes/differential | C9.1/C9.3/C9.4, C10.1/C10.2 |
| §6.2 Parallel spool | C9.2, C10.3 multi-worker test |
| §6.3 Criterion | C10.3, C3 baseline handoff |
| §7 Per-file cost | C3.1/C3.2, allocator measured last |
| §8.1 dist/musl | C11.1/C11.2 (musl build and measurement in the Linux CI job) |
| §8.2 Homebrew | C11.3 and gates 1/4 |
| §8.3 npm | C11.3 and gates 1/4 |
| §8.4 crates.io | C11.1/C11.3 and gate 5 |
| §8.5 Signing | C11.3, gate 2, C12.2 |
| §8.6 Approvals | C11.5, five separate Ask Sebastián checkboxes |
| §9 Grammar binaries | C8.2/C8.3/C11.4 and gate 3 |
| §10 Guide | C12.1 |
| §11 v1.0 | C11.1/C12.2 |
| §12 Criteria | Table above |

## Risks and what is verified only in CI

| Risk / status | Local Linux evidence | Native CI evidence / response |
|---|---|---|
| std spawn loses suspended thread handle, **verified API constraint** | Windows GNU type-checks own CreateProcessW wrapper | MSVC start/assign/resume rollback, concurrent handle inheritance, nested Actions Jobs |
| Console Ctrl-C and colour metadata, **deduced until native run** | Lexical/ANSI tests, cfg cross-check | Separate-console GenerateConsoleCtrlEvent test; status 0xC000013A, no live descendants |
| macOS kqueue/rlimit/poll, **deduced from source and APIs** | cfg isolation, injectable failure tests | arm64 and Intel parent death idle/active, checked refusal logging, MCP disconnect |
| Search mmap differs from Source/document mmap | Linux binary/reference and Source tests | macOS bytes_searched/binary semantics, preserved preparation mappings |
| Windows process/file-sharing rules | Hash/rollback algorithms and wire path encoding tests | Temp files still open, exe replacement failure, drive/UNC URL and native path bytes |
| 1.05 per-file gate may remain missed | Paired raw data, strace/perf, slopes, allocator cost last | Timing stays local; report share and ask Sebastián before any criterion change |
| Whole-tree preparation may dominate dense RSS | Same bytes sparse/dense baselines, seven families, bounded spool | CI Linux RSS only; never infer bounded history merely from shared Trail types |
| No musl C toolchain on this machine (no `musl-gcc`; PCRE2 builds from C; no sudo) | Rust musl target installed, GNU probe only | Linux job installs `musl-tools`, builds the musl archive and prints the musl/glibc ratio (> 1.1 triggers jemalloc) |
| Reference digests unavailable offline | Configuration/digest validation and exact version checks | Setup obtains official bytes/checksums; orchestrator verifies before push |
| dist schema/package installer details | Installed 0.33.0 help, generated plan/build validation | Native formula/npm install from own archives, five-target archive manifests |
| OIDC attestations cannot be issued locally | Subject list/configuration review, dist plan/build | First approved RC verifies every archive/script/checksum with gh |
| MSVC/static C builds differ from MinGW | Windows GNU all-targets check | Actual MSVC C compilation/link, PCRE2 arm64 macOS JIT unavailable |
| Synthetic robustness fixtures can be expensive | Per-child deadlines, reader budgets, serial RSS, target cleanup | All systems retain functional refusal tests; Linux measures RSS without timing ratios |

Every chunk hands off: commit IDs, exact RED/GREEN commands/results, native-only
pending checks, measured evidence paths, skipped-test inventory rows and any
unresolved spec conflict. Delete its sole target dir after handoff. The plan
is implemented only when acceptance evidence exists; planned PASS messages are
instructions to verify, rather than results already obtained.
