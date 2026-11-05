# searchfor M6 (v1.1) Implementation Plan

> **For agentic workers:** implement each chunk in its own worktree, using
> `superpowers:subagent-driven-development` or `superpowers:executing-plans` and
> `test-driven-development`. Write the test, observe RED, implement, observe GREEN,
> then commit. Steps use checkbox (`- [ ]`) syntax. Read `CONTEXT.md`, the ADRs,
> `docs/specs/2026-11-04-searchfor-m6-design.md` (the M6 spec), the cited parts of
> the v1 and M5b specs, and `grill-respuestas.md`, including Questionnaire 49.
> The M6 spec wins; report a disagreement before changing its meaning. Code,
> comments, this plan and commit messages are English; user documentation also
> has its Spanish copy. Commits and documentation carry no assistant attribution.

**Limits every chunk keeps**

- **M6 publishes nothing.** There are no tags, releases, `cargo publish`, npm,
  Homebrew changes, installation into the user's environment, or release dispatches.
  The product version becomes `1.1.0` in `Cargo.toml` in C12. Publication of v1.0
  is separate, from M5b's commit. A successful CI run authorizes no publication.
- **Nothing is pushed until the orchestrator says so.** Chunk commits stay on
  their branches. The orchestrator owns merges, branch pushes and native CI.
- **Tests have no network.** Work entirely `--offline`, with the cached lock.
  Test processes use temporary directories, synthetic files and `file://` indexes.
  Native CI setup may provision the already pinned tools; test steps stay offline.
- **Tests never touch `~/.config` or real platform data.** Use `common::Fixture`:
  `SEARCHFOR_FORMATS_DIR`, `SEARCHFOR_GRAMMARS_DIR`, `SEARCHFOR_GRAMMAR_INDEX` are
  temporary; remove `SEARCHFOR_CONFIG_PATH` and `RIPGREP_CONFIG_PATH`. A config
  test supplies its own temporary config. Never mutate the process environment
  from a parallel Rust test: put settings on its child `Command`.
- **Low priority.** Every Linux cargo command, including nested builds, is
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo …`. Measured programs and rg
  inherit `nice -n 19 ionice -c3`; record `uptime`. Native macOS CI uses
  `CARGO_BUILD_JOBS=8 nice -n 19 cargo …`; Windows uses jobs=8 and BelowNormal
  process priority. CI does not require Linux's `ionice` on those systems.
- **The ripgrep 14.1.1 `=` pins stay.** Keep ADR 0001 and tree-sitter `=0.24.7`.
  **No new crate**, including new direct dependencies on transitive crates.
  Mail uses a small local RFC 2047 decoder plus existing `encoding_rs`, rather
  than adding `base64` to Cargo.toml. No `cargo update`, fetch or Grammar install.
- **One target directory per chunk.** Set exactly
  `/home/sebas/robotin/apps/mdgrep/.workspace/t-798-searchfor-m5b-v1-0-windows-macos-ci-y-releases-despues-m6/targets/m6-c<N>`
  as `CARGO_TARGET_DIR`. Nested builds inherit it. Save small check reports and
  the release size, then delete that directory after the chunk's handoff. Never
  delete another chunk's directory. The authoring session only writes this plan;
  its optional check may use the default ignored `target/` as requested.
- **Size stop.** Measure the stripped Linux x86-64 release binary in every chunk
  that changes executable code and after each tanda. Above **22,800,000 bytes**,
  stop that chunk and report before proceeding or starting the next tanda.
  Criterion 22's hard ceiling remains **23,000,000 bytes**. Only Sebastián can
  change the ceiling or move a Format family to a Helper Processor.
- **Native streaming.** Each new Processor implements `prepare`, `capability`,
  `Prepared::streams`, `Cursor::trail_at` and `open_from`; Outline Processors get
  these through the existing blanket implementation and `OutlinePrepared`.
  Model code remains a test oracle. A model fallback that happens to print the
  correct snapshots does not pass. C11 covers dense memory/time and C12 extends
  the in-process stream/model differential; C10 covers its binary layouts.
- **Local and CI evidence are distinct.** Every task inherits the Linux checks
  below, including Windows GNU all-targets clippy. Native path normalization,
  Windows MSVC and macOS arm64/x86-64 behavior are verified by the four-system CI
  section. A missing tool is recorded as missing, never as a successful check.

**Goal:** describe all forty M6 Formats, decorating the bytes rg already finds,
with stable Trails, structural flags, bounded streaming, bilingual docs and 1.1.0.

**Architecture:** extend indexed path detection, keypath syntaxes and JSON views,
markup composition and object layouts. Add seven scanner Processors. Prepare
compact source ranges and name ingredients once; construct owned Segments only
when needed by Matches or name search. SQL, mail and chat prepare incrementally
through the last requested unit. Keep the matching engine and Helper protocol.

**Tech stack:** Rust 1.90, edition 2024; the existing memchr, globset 0.4.15,
encoding_rs 0.8.34, serde/toml, goblin, tree-sitter 0.24.7, insta and test helpers.
Existing `.gitattributes` already preserves fixture and snapshot bytes.

**Branch:** `t-798-searchfor-m6`, checked base `59bb971`; chunk branches
start from Task 0's commit on it (D25). Chunk branches are
`t-798-searchfor-m6-c<N>`. Symbols and paths below were checked against this
base. Line references may move; identify edits by symbol. This document specifies
proposed code and expected RED/GREEN; authoring it does not execute those tests.

## Grill decisions this plan implements

| Source | Decision | Chunk / verification |
|---|---|---|
| #3, ADR 0001 | Full rg flag compatibility; fixed reference 14.1.1 | C1, C10, C12.2 differential |
| #4 | `--within`, `--expand`, `--in-headings` | C2–C10, shared test contract |
| #5–#6 | Declarative detection, embedded TOML, user replacement/export | C1, C12.1 |
| #7, 8454, ADR 0005 | External Processors; compiled v1 Grammars, extras outside | C7, C12.1 `.po` guide; no new Grammar |
| #9–#10, 8365 | Trail vocabulary, signatures in tree, rg JSON extensions | All Format tasks, C12.2 schema/differential |
| 8375 | English primary; Spanish docs; review functional increments | C12; per-chunk checks |
| 8381 | Colored presentation and safe control-character display | Shared snapshots, C11 fuzz; existing rendering |
| 8385 | Matching stays rg's; decoration only; structureless Match has empty Trail | Every chunk; C11.1–C11.3 |
| 8390 | Decorate only bytes rg finds; archives excluded; decode names only | C9, C10, C12.2 |
| 8612 | Release binary at most 23,000,000 bytes | Every size check, C12.4, stop at 22.8 MB |
| 8742 / 8750 | Extract documents by default, terminal and pipe alike | C1 doc fast path preservation; C11.3 corpus |
| Q49.A | AsciiDoc, rST, Org, LaTeX, Jupyter | C5, C6.1 |
| Q49.B | Configurations, SSH, build files, nginx, Apache, HCL | C2–C4 |
| Q49.C | SQL, diff, Vue, Svelte, Blade, GraphQL, Protobuf, HAR | C3.3, C6.2, C7, C8 |
| Q49.D | Office/PDF/EPUB already delivered, searched by default | Existing regression suite; C12 table includes them |
| Q49.E | Images, ID3/MP4, Wasm, Java; SQLite already delivered | C10; archives remain excluded by 8390 |
| Q49.F | Mail, calendars, contacts, BibTeX, chats; subtitles/JSONL already delivered | C2.4, C6.3, C7.3, C9 |
| Q49.G | Extra Formats after M5; valuable earlier Formats stay | Prescribed chunk graph; existing regressions |
| Q49.Other | Blank; performance and graceful fallback comments apply | C11; no extra Formats inferred |
| Q57.plataformas | Existing five release targets, four native test systems | Orchestrator CI; no packaging changes |
| Q57.fuzz-ci / #9835 | Budgeted fuzz; flat dense time reported, JSON bounded | C10.5, C11.1–C11.2; orchestrator CI |
| 12653–12664, especially 12659 and 12664 | Develop M6 now on its own branch; 1.0 tag from M5b, M6 is 1.1 | This branch, C12.3 (CI versions from Cargo.toml), C12.4; publication excluded |
| Hilo 855 #12687 | Two open options, both answered since | Question 1 A (#12694), question 2 B (#12726); notes 2 and 3 |
| Hilo 855 #12694 | Question 1 answered A: detection by name and path | D21; C1.3, C9.3 |
| Hilo 855 #12726 (accepting #12697) | Question 2 answered B: HTML scripts with three limits | D24; C7.4, C11.2, C12.1, C12.2 |

## Design decisions

| Id | Decision | Why |
|---|---|---|
| D1 | Preserve all twelve chunks and four tandas | The orchestrator fixed the graph; spec §15 now states the four tandas (plan r1, Fable #10) |
| D2 | Each CLI file copies the short test prelude below | C2–C4 can start in parallel without depending on C1's test utilities or editing tests/common |
| D3 | C2–C4 keep forced-format tests; orchestrator runs added automatic-detection variants after C1 merges | Avoids a mid-tanda dependency; both forced and automatic paths remain covered |
| D4 | Index compound extensions by terminal extension; path-glob candidates by mandatory literal token | A name outside a glob's necessary literal parts skips the matcher; criterion 51 verifies the cost |
| D5 | Arbitrary user globs without useful literals get a separate slow bucket | Correct globset semantics must survive user patterns such as `**/*`; the built-in fast-path claim is measured with built-ins only |
| D6 | Sort each glob once into spec §4.1's signal 4 (writes the extension, before it) or signal 6 (leaves it open, after it); signal 4 is found through the extension lookup | The corrected spec (`0fed3df`) resolves the Telegram contradiction this way; keying signal 4 by extension keeps every other extension at today's cost |
| D7 | Compact borrowed ranges for common scanners; no full serde_json::Value tree | Late names and 50 MB/s require bounded name allocations and no whole JSON DOM |
| D8 | SQL/mail/chat scan lazily and cache only unit offsets and current-unit detail | A whole-file eager index would violate criterion 52 even if its index were compact |
| D9 | A JSON view is a prepared scanner projection, not a mapping DSL | Three built-in shapes need source-aware labels and fallback, not a new public language |
| D10 | Script composition translates every range and Name piece back to the parent Source | Slicing alone produces wrong lines, Sections and --in-headings anchors |
| D11 | Embedded scripts use the Registry's active JS/TS/TSX Definitions, but compiled Grammars only | Honors custom queries/max_bytes without allowing a component to launch a Grammar Helper |
| D12 | Add nested binary range maps alongside legacy Map; preserve Map::parse | Sect's single flat section plus closest symbol cannot represent nested IFDs/boxes, source-name labels or Java string ownership |
| D13 | Try object candidates in Registry order, validating a layout before selecting it | `cafebabe` needs structural validation; magic alone cannot choose Java vs fat Mach-O |
| D14 | Literal/generated name pieces remain separate through truncation/decoding | `--in-headings` must search source names, never `cell`, `row`, `stage` or synthetic indexes |
| D15 | Per-chunk broken input follows each Format's permissive rules | INI/HTML/headings have recovery or no syntax errors; blanket empty-tail expectations would contradict their specific rules |
| D16 | C10 owns binary fuzz/perf in its CLI test; C11 owns text fuzz/perf in `tests/fuzz_m6.rs` | Both can work in parallel without importing not-yet-merged binary fixture writers; spec §13 names both files |
| D17 | Move the central stream/model differential edits to C12.2 | C10 also needs src/app.rs for object routing; two tanda-3 chunks must not edit it concurrently |
| D18 | C12.2 owns the central rg_compat_m6 suite | It can consume completed text and binary fixture writers without a same-tanda dependency |
| D19 | Existing object --in-headings policy stays as v1 §5.7 | §9 says binary rules are unchanged; legacy object names() stays empty. New binary ownership still excludes its own method-name label |
| D20 | C12.4 refreshes the root package's own entry of Cargo.lock with the bump, and nothing else | Cargo.lock records `searchfor 1.0.0`; with 1.1.0 in Cargo.toml every `--locked` command is refused for certain («the lock file … needs to be updated but --locked was passed», checked on `d14a4c1` with `cargo metadata --offline --locked`). CI runs every cargo command with `--locked`, so refreshing that one entry is the only way to keep it; `cargo update --offline --workspace` rewrites exactly the line `version = "1.0.0"` under `name = "searchfor"` and moves no other crate (checked: `1 insertion(+), 1 deletion(-)`). No dependency, helper or package-channel version moves (plan review r2, Fable #4) |
| D21 | WhatsApp exports are detected by `_chat.txt` and one glob, `*WhatsApp*.txt`; no localized name lists | The app's name is in the export's file name in every language; exact lists need sources this repo lacks (plan r1, Codex #4). A `.txt` the glob claims that is no export fails the chat scanner's first-line check and shows as rg shows it (8385), so a false positive costs nothing visible. Still detection by name and path (answer 1 = A, Hilo 855 #12694) |
| D22 | CI's version checks read the version from Cargo.toml, once | Bumping to 1.1.0 would otherwise fail nine checks written for 1.0.0 (plan r1, Codex #1); C12.3 does it before C12.4 bumps |
| D23 | `Segment.level` becomes `Option<usize>` in tanda 1 (C1.4) | Org needs levels above 255; widening it in C5 would break C6's Jupyter headings at the tanda-2 merge and `tree.rs`'s cast under clippy (plan r1, Fable #4) |
| D24 | HTML reads its JavaScript and `module` scripts and its JSON-LD; other types, scripts over 1 MiB and lines over 1 000 bytes keep 1.0's Trail | Sebastián's answer B with the three limits of #12697 (Hilo 855 #12726). The 1 000-byte line is the minification cap: the repo had none to reuse, hand-written code stays under formatters' 80–120 columns, and a minified bundle's Trails say nothing while being the costly parse (spec §8.3) |
| D25 | Task 0, the orchestrator's preparatory commit, frees the existing tests from every name, extension and count M6 adds, before tanda 1 | Chunks run in parallel worktrees, so a fix in one chunk's branch reaches its siblings only at the tanda's integration, and their inherited suites would stay red until then (plan review r2, Codex #1, Fable #1 and #2) |

## Chunk graph and file ownership

**Ownership adjustment:** C11's extension of the existing differential in
`src/app.rs` moves to **C12.2**, because C10 modifies `Ctx::object` there in the
same tanda. C12.2 also owns `tests/rg_compat_m6.rs`, which needs C10's finished
fixture writers. C11 still owns its per-Processor cursor/model checks in
`tests/fuzz_m6.rs`. C1 owns `src/formats/**` in tanda 1, and the type of
`Segment.level` with its conversions (Task 1.4, D23; `tree.rs`'s heading arm is
not one of the shared kind arms); C10 may add only the
object-candidate iterator in `detector.rs` in tanda 3. C2 owns keypath core in
tanda 1 and hands it to C6 in tanda 2. These are sequential handoffs. C7 owns
`src/processors/mod.rs`'s per-run component Definition plumbing in tanda 2;
other chunks add their own by_name/module lines there as registration edits.
No dependency or chunk moves. Registration points are intentionally shared:
`src/formats/builtin.rs`, `processors::by_name`/module declarations,
`src/output/tree.rs::label` kind arms, `docs/json-schema.json`'s kind description.
Each chunk contributes its own lines; the orchestrator merges these conflicts.

```text
Task 0 (orchestrator, on t-798-searchfor-m6): the preparatory commit
                         every tanda-1 chunk branch starts from its commit
Tanda 1 (parallel):  C1 Detector    C2 keypath    C3 blocks    C4 buildfile
                         \             |             |            /
                         all four integrated and size checked
Tanda 2 (parallel):  C5 headings   C6 JSON views   C7 markup
                    C8 sql/diff   C9 mail/chat
                         all five integrated and size checked
Tanda 3 (parallel):  C10 object layouts     C11 cost/dense/fuzz
                         both integrated and size checked
Tanda 4:            C12 Docs and 1.1.0
Then:               orchestrator — CI on the four systems
```

C2–C4 retain explicit `--format` throughout. After tanda 1, the **orchestrator**
runs each test binary with `SEARCHFOR_M6_AUTO=1`: the same primary fixtures run
again without `--format`. This variable controls tests only, never s4.
C5–C9 use automatic variants at handoff too, Telegram included (note 1 below).
Each chunk owns `tests/cli_m6_<topic>.rs` and its own snapshots. All fixture
basenames are unique across groups/chunks.

**Every chunk branch of every tanda starts from Task 0's commit or a later
integration of it** (plan review r2, Codex #1, Fable #1 and #2). Task 0 is the
orchestrator's preparatory commit on `t-798-searchfor-m6`, made before tanda 1's
four branches are created: it removes every dependency an existing test has on
what M6 adds, so a chunk that registers its TOMLs keeps the inherited suite
green in its own worktree, with no fix waiting in a sibling branch. It moves the
`m2_07` count out of C2 (C2's first commit no longer carries it), moves the
`.ini` claim of `tests/mcp_m5.rs`'s `blocked` Definition, the `Makefile`
claim of the Detector's unit-test `makey` and the `config` file name of one
`tests/cli_m5.rs` test (the new `tests/fixtures/config/` directory takes it),
and lists every other hit of the sweep with where it goes. No chunk edits the
lines Task 0 changed afterwards. No test in `tests/` asserts a `user (` origin (grep on `efabcde`:
only `tests/cli.rs:177` asserts `overridden (`), so no chunk owns INI-origin
expectations. C12 owns `tests/guide.rs` and the version test in `tests/cli_m5.rs`.

### Spec discrepancies and open-question change notes

1. **Telegram detection, resolved in the spec:** the draft of §4.1 put every
   glob after the extension, so `ChatExport_2024/result.json` was `json`, while
   §4.2/#43 required `telegram`. The spec now splits the glob signal: a glob whose
   last component writes a literal extension (`result.json`, `messages*.html`,
   `*.conf`) is signal 4, tried before the extension and winning over it; every
   other glob (`Makefile.*`, `.env.*`, `**/.ssh/config`) is signal 6, after it.
   #43 now states both Telegram paths and that a `result.json` elsewhere stays
   JSON; #51's corpus adds `.json`/`.html` names, and `.txt` ones for WhatsApp's
   glob (note 4). C1.2 sorts the globs, C1.3
   asserts the precedence, C6.3/C7.3 run automatic detection like every other
   Format, C11.3 measures the cost. #43 is no longer blocked.
2. **Question 1, answered A** (Sebastián, Hilo 855 #12694): app names and paths
   only. The tasks B would have changed (C1.3, C9.3, C11.3, C12.1) stay as A.
3. **Question 2, answered B with three limits** (Sebastián, Hilo 855 #12726,
   accepting #12697; D24): HTML's scripts get Trails when they have no `type`, a
   JavaScript type or `module`; JSON-LD gets keypath's JSON Trails; any other
   type, a script over 1 MiB and one with a line over 1 000 bytes keep 1.0's
   Trail. Tasks: C7.4 (`formats/html.toml`, the six limit cases, v1 snapshots
   unchanged) and the `same.html` assertion of C7.1; C11.2's `m6_html_scripts_cost`;
   C12.1 documents the change in `docs/rg-compat.md`, the READMEs and
   `docs/formats{,.es}.md`; C12.2 adds the HTML fixture to the stream/model
   differential. Script range translation is C7.1's, unchanged.
4. **WhatsApp by one glob (D21):** the spec's localized name lists, which needed
   sources this repository lacks (plan r1, Codex #4), became `*WhatsApp*.txt`
   plus `_chat.txt`. The glob writes `.txt`, so it is signal 4 under `txt` and
   every `.txt` pays the comparison against its literal part `WhatsApp`, which
   C11.3's corpus measures (spec §4.1 cost, #51). The spec's §10.4 now states the
   chat scanner's first-line check that makes a false positive invisible; the
   plan had none, and C9.3 adds it.
5. **Version metadata:** user scope says version in Cargo.toml only. C12.3
   makes CI's checks read it from there (D22), so the version stays written in
   one place. Cargo.lock repeats the root package's version as metadata, and
   `--locked` refuses a lock that disagrees with Cargo.toml, so C12.4 refreshes
   that one entry with the bump (D20): it is the only way compatible with CI's
   `--locked`, and it moves no pin and no helper or package version.
6. **Spec amended by plan review r1:** §4.1's cost paragraph, §4.2's WhatsApp
   row, §10.4 (one glob and the first-line check), #43 and #51 (note 4); §13
   names `tests/fuzz_m6.rs` and `tests/cli_m6_object.rs` for the fuzz (D16);
   §15 states the four tandas (D1); §14 no longer asks to change `user` origin
   expectations, since no test asserts one (chunk graph); Open question 1 is
   marked answered. After #12726: §2, §5.1, §8.3, §11, §14, #47, #53 and Open
   question 2 state HTML's scripts and their three limits (note 3, D24).
   Plan review r2: §15 names the orchestrator's preparatory commit (Task 0, D25).

### Common command convention

In each chunk worktree (N is that chunk's integer), export once:

```bash
export CARGO_TARGET_DIR="/home/sebas/robotin/apps/mdgrep/.workspace/t-798-searchfor-m5b-v1-0-windows-macos-ci-y-releases-despues-m6/targets/m6-c${N}"
export SEARCHFOR_RG="${SEARCHFOR_RG:-/usr/bin/rg}"
export SEARCHFOR_REQUIRE_REFERENCE=1
```

Every `Run RED`/`Run GREEN` below is a literal Linux command. Expected failure
messages describe semantic assertions, not exact harness punctuation. A selected
command must execute at least one test; zero selected fails the procedure.
`INSTA_UPDATE=always` is used only while creating/reviewing new snapshots; GREEN
runs set `INSTA_UPDATE=no`. Never accept unrelated snapshot changes.

After each task's GREEN, run the chunk test file. After the final task, run:

```bash
CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo fmt --all --check
CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo clippy --offline --locked --workspace --all-targets -- -D warnings
CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo clippy --offline --locked --target x86_64-pc-windows-gnu --all-targets -- -D warnings
CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --workspace
CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo build --offline --locked --release --bin s4
strip --strip-all "$CARGO_TARGET_DIR/release/s4"
stat -c '%s' "$CARGO_TARGET_DIR/release/s4"
```

Record exact bytes and stop above 22,800,000. A missing cross toolchain/cache is
reported to the orchestrator; native CI is still required. Snapshot/fixture
commits use an explicit file list from the task, never `git add -A`. After the
handoff report, delete exactly the exported directory:
`test -n "$CARGO_TARGET_DIR" && rm -rf -- "$CARGO_TARGET_DIR"`.
The orchestrator checks that it is the designated `m6-cN` path before deletion.

### Complete CLI test prelude (copy into each topic file)

C2–C9 use this complete prelude in their own `tests/cli_m6_<topic>.rs`.
C1 and C10 have dedicated launch code. No shared test-support file is added.
A primary fixture below is committed under `tests/fixtures/<path>` with exactly
the bytes in its Rust literal; `seed` asserts that correspondence. Additional
edge fixtures live only in the temporary fixture copy through `put`.
For each primary fixture, commit a sibling `broken-<basename>` with bytes given
by that task's broken case; run `check` on it too to create its three snapshots.

```rust
mod common;
use common::{Fixture, code, run, stderr, stdout};
use serde_json::Value;

fn put(fx: &Fixture, file: &str, src: &[u8]) {
    let path = fx.file(file);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, src).unwrap();
}
fn seed(fx: &Fixture, file: &str, src: &[u8]) {
    let original = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures").join(file);
    assert_eq!(std::fs::read(original).unwrap(), src, "fixture bytes: {file}");
    assert_eq!(std::fs::read(fx.file(file)).unwrap(), src);
}
fn output(fx: &Fixture, format: &str, file: &str, pattern: &str,
          flags: &[&str]) -> std::process::Output {
    run(&mut fx.s4(["--no-docs", "--format", format])
        .args(flags).args([pattern, file]))
}
fn matches(out: &std::process::Output) -> Vec<Value> {
    assert_eq!(code(out), 0, "{}", stderr(out));
    assert!(out.stderr.is_empty(), "{}", stderr(out));
    stdout(out).lines().map(|l| serde_json::from_str::<Value>(l).unwrap())
        .filter(|v| v["type"] == "match").map(|v| v["data"].clone()).collect()
}
fn paths(rows: &[Value]) -> Vec<String> {
    rows.iter().map(|r| r["trail"].as_array().unwrap().iter()
        .map(|s| s["name"].as_str().unwrap()).collect::<Vec<_>>().join(" › ")).collect()
}
fn check(fx: &Fixture, format: &str, file: &str, expected: &[&str]) {
    let json = output(fx, format, file, "needle", &["--json"]);
    let rows = matches(&json);
    assert_eq!(paths(&rows), expected, "{file}");
    assert!(rows.iter().all(|r| r["format"] == format));
    common::assert_valid_json_schema(&stdout(&json));
    for (layout, flag) in [("tree", "--tree"), ("flat", "--flat"), ("json", "--json")] {
        let out = output(fx, format, file, "needle", &[flag]);
        assert_eq!(code(&out), 0, "{}", stderr(&out));
        assert!(out.stderr.is_empty());
        let text = if layout == "json" { common::mask_json_times(&stdout(&out)) }
                   else { stdout(&out) };
        let stem = file.replace(['/', '.'], "_");
        insta::assert_snapshot!(format!("m6_{format}_{stem}_{layout}"), text);
    }
    if std::env::var_os("SEARCHFOR_M6_AUTO").is_some_and(|s| s == "1")
        && !file.contains("/broken-") {
        let auto = run(&mut fx.s4(["--no-docs", "--json", "needle", file]));
        assert_eq!(matches(&auto), rows, "automatic detection: {file}");
    }
    let src = std::fs::read(fx.file(file)).unwrap();
    let crlf = String::from_utf8_lossy(&src).replace('\n', "\r\n");
    put(fx, "m6-crlf.fixture", crlf.as_bytes());
    let copy = output(fx, format, "m6-crlf.fixture", "needle", &["--json"]);
    let copy = matches(&copy);
    assert_eq!(paths(&copy), paths(&rows));
    for (a, b) in rows.iter().zip(copy) {
        assert_eq!(a["trail"], b["trail"], "CRLF Sections: {file}");
    }
}
fn flags(fx: &Fixture, format: &str, file: &str, within: &str,
         source_name: &str, generated_pattern: &str) {
    let rows = matches(&output(fx, format, file, "needle",
                              &["--json", "--within", within, "--expand"]));
    assert!(!rows.is_empty());
    for row in rows {
        let section = &row["section"];
        assert!(section["start_line"].as_u64().unwrap() > 0);
        assert!(section["end_line"].as_u64().unwrap() >= section["start_line"].as_u64().unwrap());
        assert!(section["text"].as_str().unwrap().contains("needle"));
    }
    let found = output(fx, format, file, source_name, &["--json", "--in-headings"]);
    assert!(!matches(&found).is_empty(), "name: {source_name}");
    // "" where the fixture's names hold no added word. Otherwise the pattern
    // must match the name's text (no `^…$` around a name that also holds file
    // text, as `VEVENT Team sync`), so it fails only because the added word has
    // no source piece, never because the anchors miss.
    if !generated_pattern.is_empty() {
        let absent = output(fx, format, file, generated_pattern, &["--json", "--in-headings"]);
        assert_eq!(code(&absent), 1, "generated name matched: {}", stdout(&absent));
        assert!(absent.stderr.is_empty());
    }
}
```

`check` compares JSON Match data only in its automatic variant, so elapsed times
cannot affect the comparison. The CRLF source copies are UTF-8 primary fixtures;
invalid-UTF-8 and UTF-16 cases use separate byte-level tests below. `flags` runs
only on a fixture with a real deepest Section; binary expansion stays rows.

## Task 0: Preparatory commit (orchestrator, before tanda 1)

**Who and where:** the orchestrator, directly on `t-798-searchfor-m6`, before it
creates any chunk branch. Tanda 1's four branches start from this commit, and
every later tanda from an integration that contains it (chunk graph). Plan
review r2, Codex #1, Fable #1 and #2: one class of problem, existing tests that
break when a new built-in Definition claims what they claim.

**Files:** `tests/cli_m2.rs`, `tests/mcp_m5.rs`, `tests/cli_m5.rs` (one test,
not C12's version test) and the `#[cfg(test)]` module of
`src/formats/detector.rs`. No production code, no fixture, no snapshot.

**Edit 1, `tests/cli_m2.rs::m2_07_list_and_export_the_new_formats`:** the count
of exported files comes from the built-in list, so each TOML a chunk registers
keeps it right. Replace

```rust
        std::fs::read_dir(&target).unwrap().count(),
        // Markdown, M2's Formats, M3's html, xml, csv, tsv, srt and vtt, and M4's elf, pe,
        // macho and nine document Formats.
        1 + FORMATS.len() + 6 + 12
    );
```

with

```rust
        std::fs::read_dir(&target).unwrap().count(),
        // Every built-in Definition, so a Format added later keeps this count right.
        searchfor::formats::builtin::BUILTIN.len()
    );
```

**Edit 2, `tests/mcp_m5.rs::NativeRun`:** the `blocked` Definition claims
`.ini`, which M6's built-in `ini` takes first (built-ins precede user
Definitions in the Registry, `detector.rs::Detector::new`'s
`by_extension.entry(e).or_insert(i)`); the helper would never start and the
three tests that use `NativeRun` would time out at 20 s. It moves to `.blk`,
which no v1 or M6 Definition claims, so the test no longer depends on the
built-in `ini` at all (a `name='ini'` replacement would depend on the
replace-by-name rule instead). In the doc comment of `struct NativeRun`, replace

```rust
/// A server whose one search is held by the example helper's wait knobs (M5b plan, Task 2.3)
/// on a regular `.ini` file, and the pids of that search and of its helper, which the helper
/// logs once it holds the call. Dropping it releases the helper and kills the server.
```

with

```rust
/// A server whose one search is held by the example helper's wait knobs (M5b plan, Task 2.3)
/// on a `.blk` file, an extension only this test's Definition claims (M6 plan, Task 0), and
/// the pids of that search and of its helper, which the helper logs once it holds the call.
/// Dropping it releases the helper and kills the server.
```

and in `NativeRun::start` replace these three lines, one for one:

```rust
        std::fs::write(fx.file("blocked.ini"), "[section]\nk=needle\n").unwrap();
            "name='blocked'\nextensions=['ini']\nprocessor='external'\n[params]\ncommand=[{:?}]\nmode='pool'\ntimeout_ms=30000\n",
            "params":{"name":"search","arguments":{"pattern":"needle","paths":["blocked.ini"]}}});
```

```rust
        std::fs::write(fx.file("blocked.blk"), "[section]\nk=needle\n").unwrap();
            "name='blocked'\nextensions=['blk']\nprocessor='external'\n[params]\ncommand=[{:?}]\nmode='pool'\ntimeout_ms=30000\n",
            "params":{"name":"search","arguments":{"pattern":"needle","paths":["blocked.blk"]}}});
```

**Edit 3, `src/formats/detector.rs`, `tests::registry` and its three
assertions:** `makey` claims the exact name `Makefile`, which M6's built-in
`make` takes first (`by_name.entry(n).or_insert(i)`); it moves to `Makey`, a
name no Definition of spec §4.2 claims. Replace the doc comment and the
Definition of `registry()`:

```rust
    /// The built-ins plus `bashy` (shebang), `myfy` (magic) and `makey` (exact name). `myfy`'s
    /// magic is not ELF's, which the built-in `elf` claims since M4.
```

```rust
            "name='makey'\nnames=['Makefile']\nprocessor='markdown'",
```

with

```rust
    /// The built-ins plus `bashy` (shebang), `myfy` (magic) and `makey` (exact name). `myfy`'s
    /// magic is not ELF's, which the built-in `elf` claims since M4, and `makey`'s name is no
    /// built-in's: `Makefile` is M6's `make` (M6 plan, Task 0).
```

```rust
            "name='makey'\nnames=['Makey']\nprocessor='markdown'",
```

and in the three assertions that expect `"makey"` replace `"Makefile"` by
`"Makey"`, nothing else. In `signals_in_order` the shorter assertion then fits
one line, as `cargo fmt` writes it:

```rust
        assert_eq!(
            det.detect(Path::new("Makefile"), b"").unwrap().name,
            "makey"
        );
```

becomes

```rust
        assert_eq!(det.detect(Path::new("Makey"), b"").unwrap().name, "makey");
```

and the other two keep their layout:
`the_path_alone_decides_unless_a_format_reads_the_content`
(`assert_eq!(name("Makey"), Some(Some(&"makey".to_string())));`) and
`earlier_signals_beat_later_ones` (`det.detect(Path::new("Makey"), b"\x7fMYF")`).
The other `"Makefile"` strings of the module (`the_built_in_document_names_are_the_document_formats`,
`only_some_names_may_be_documents`, `names_and_extensions_pick_the_first_active_definition`,
`names_without_an_extension_are_decided_without_walking_every_definition`)
compare two answers of the same Detector, or `may_be_document`, which a
non-document name claim does not change; they stay.

**Edit 4, `tests/cli_m5.rs::m5b_mcp_config_trap_needs_no_executable_script`:**
the test writes its config file as `fx.file("config")`, at the root of the
fixture copy; C2, C3 and C4 add the directory `tests/fixtures/config/`
(`config/m6.ini`, `config/m6.tf`, `config/m6.dockerfile`…), which
`common::Fixture` copies into every test's tree, so the write fails with
`IsADirectory` (found by the sweep's probe; no review named it). Replace

```rust
    let fx = Fixture::new();
    let config = fx.file("config");
```

with

```rust
    let fx = Fixture::new();
    // Not `config`: M6's fixtures add a `config/` directory to the copied tree (M6 plan, Task 0).
    let config = fx.file("mcp-trap-config");
```

**The sweep (on `d14a4c1`).** Every test and test helper (`tests/**`, the
`#[cfg(test)]` modules of `src/**`, `helpers/**`, the fixtures,
`docs/examples/**`, `tests/guide.rs`) was searched for a user or test
Definition, fixture name or expectation that claims an extension, exact name,
path glob, compound extension or magic of spec §4.2, and for a file that M6
would make structured. Then a probe ran the whole suite with temporary built-ins
claiming every §4.2 extension, name and `mbox`'s magic (with the `csv`
Processor, `headers = false`, so any Match in a claimed file gains a Trail):
first on `d14a4c1` as it is (seven failures: edits 1–3), then with edits 1–3
and copies of the 31 primary fixture paths this plan adds, each with two
`needle` lines (one failure: edit 4), then that test with edit 4. `paths`
globs and compound extensions do not exist before C1, so the probe could not
declare them; the text search covers them. Hits, by file and line:

| Hit | What M6 does to it | Where it goes |
|---|---|---|
| `tests/cli_m2.rs:182-187`, `m2_07`'s fixed count | Fails with the first registered TOML (seen in the probe) | Task 0, edit 1 |
| `tests/mcp_m5.rs:268`, `272`, `290` (doc `215`): `blocked` claims `.ini` | `m5b_client_output_disconnect_stops_a_blocked_search` (357), `m5b_server_death_stops_a_blocked_search` (365) and also `m5b_client_input_eof_leaves_no_search` (373, through `NativeRun::finite_search`) fail at 20 s (seen in the probe) | Task 0, edit 2 |
| `src/formats/detector.rs:465` `makey` `names=['Makefile']`, asserted at `482`, `504`, `547` | Three unit tests fail once `make.toml` registers (seen in the probe) | Task 0, edit 3 |
| `tests/helpers.rs:31` (`ini()`), `tests/cli_m2.rs:279`, `tests/cli_m5.rs:1704`: user Definitions named `ini` | Replace the built-in `ini` by name; pass in the probe | No change |
| `tests/helpers.rs:265-276` (`h03b`): user `cfg` with `extensions=['cfg']` | M6's `ini` claims `cfg`, but the same test writes a user `ini` that replaces the built-in and claims only `ini`, so `.cfg` stays the user `cfg`'s; passes in the probe | No change |
| `tests/fixtures/ini/{app,bom,db,web}.ini`, `tests/helpers.rs` `gen/*.ini`, `long.ini`, `ini/link.ini` | Built-in `ini` files from C2 on; every test that searches them writes the user `ini` first or forces a Format; no snapshot names them | No change |
| `tests/guide.rs:31-75` with `docs/examples/{ini.toml,sample.ini,ini-helper.py,ini-pre.py}` | The test uses `--format ini-guide` and `--no-trail --pre`, so it passes; the guide's `ini.toml` (`name="ini-guide"`, `extensions=["ini"]`) would lose `.ini` files to the built-in `ini` for a reader who copies it (intended: M6 describes INI) | C12.1, as planned: the example moves to `.po` and the INI files are removed |
| `tests/rg_compat.rs:518` `types/prod.env` | Built-in `env`; the test compares `--no-trail` output and the file column of `--flat`, which a Trail does not change; runs only with rg 14.1.1, absent here, so read and not run | No change |
| `src/formats/mod.rs:279`, `plain.object(b"\xca\xfe\xba\xbe")` is `macho` | M6's `java-class` also claims `cafebabe`; it registers after `macho` and `object(head)` stays the first candidate | No change; C10.4 states it |
| `tests/fixtures/markup/page.html` and v1's `html` fuzz target | HTML reads its scripts (D24) | C7.4: v1 snapshots unchanged, `--test fuzz` rerun (Fable #3) |
| `*.ini` names in the unit tests of `src/helper/{host,protocol}.rs`, `src/processors/{external,mod}.rs` and `helpers/example/tests/protocol.rs` | Requests built in process, with no Detector | No change |
| `src/search.rs:844-846`, `tests/rg_compat.rs:514`: `*.env` | rg's file-type table, no Definition | No change |
| Binary magics of group E (`ffd8ff`, `89504e47…`, `WEBP`, `II*\0`/`MM\0*`, `ID3`, `ftyp`, `\0asm`) and `mbox`'s `From ` on a name without extension | No test, fixture or writer produces them; `cafebabe` appears only in the row above and in `cli_m4`'s fat Mach-O writers, valid fat files that stay `macho` (C10.4's ordered validation) | No change |
| Path globs, compound extensions and names of §4.2 (`.env.*`, `**/.ssh/…`, `nginx/`, `apache2/`, `httpd/`, `systemd/`, `.git/config`, `Dockerfile*`, `Containerfile*`, `Makefile.*`, `[Jj]ustfile`, `.blade.php`, `*WhatsApp*.txt`, `_chat.txt`, `ChatExport_*`, `DataExport_*`) | Nothing in the tests, fixtures or examples names them | No change |
| `tests/cli_m5.rs:1781`, `fx.file("config")` | The new `tests/fixtures/config/` directory takes the name: `IsADirectory` (seen in the probe) | Task 0, edit 4 |
| The other 30 fixtures the chunks add under `tests/fixtures/{config,code-c,docs-a,mail,chat}` | `common::Fixture` copies them into every test's tree; the suite passes with them in the probe, and `tests/rg_compat.rs`'s whole-tree cases (skipped here: no rg 14.1.1) compare s4 with rg over the same tree, `--no-trail` | No change |

**Verification** (the orchestrator, in its checkout of `t-798-searchfor-m6`):

```bash
export CARGO_TARGET_DIR="/home/sebas/robotin/apps/mdgrep/.workspace/t-798-searchfor-m5b-v1-0-windows-macos-ci-y-releases-despues-m6/targets/m6-t0"
CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test cli_m2 m2_07 -- --nocapture
CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test mcp_m5 -- --nocapture
CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test cli_m5 m5b_mcp_config_trap -- --nocapture
CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --lib formats::detector -- --nocapture
CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo fmt --all --check
CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo clippy --offline --locked --workspace --all-targets -- -D warnings
CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --workspace
test -n "$CARGO_TARGET_DIR" && rm -rf -- "$CARGO_TARGET_DIR"
```

Expected: every command passes on today's code, Task 0 alone (checked on
`d14a4c1` in a scratch worktree: the workspace suite 1018 passed, 0 failed, 63
ignored, the reference-rg tests returning early without rg 14.1.1; fmt and
clippy clean). The four edits
change no assertion's meaning: they move a claim or a file name off a name M6
takes. Then
`git add -- tests/cli_m2.rs tests/mcp_m5.rs tests/cli_m5.rs src/formats/detector.rs` and
`git commit -m "test: free existing tests from names the M6 Formats and fixtures take"`.
Record the commit; it is the base of every tanda-1 chunk branch.

## Chunk 1: Detector

Owns `src/formats/{definition,detector,mod}.rs` and
`tests/cli_m6_detector.rs`, and in Task 1.4 the type of `Segment.level` with its
conversions (D23). Registration lines are shared as declared above.
C1 tests synthetic user Definitions with `processor='markdown'`, so C2–C10
registration does not have to exist yet.

### Task 1.1: Definition paths and indexed compound extensions

**Files:** modify `src/formats/definition.rs::FormatDefinition`,
`src/formats/detector.rs::{Detector,index,detect,detect_by_path}`;
create `tests/cli_m6_detector.rs::{registry,selected,m6_compound}`.

New public field, with no change to parse's signature:

```rust
// Inside FormatDefinition, next to names; default preserves old Definitions.
#[serde(default)]
pub paths: Vec<String>,
```

- [ ] **Step 1 — RED:** create the test file with this complete code.

```rust
mod common;
use searchfor::formats::{Registry, definition::FormatDefinition, detector::Detector};
use std::path::Path;
fn registry(defs: &[(&str, &str)]) -> (tempfile::TempDir, Registry) {
    let dir = tempfile::tempdir().unwrap();
    for (name, signals) in defs {
        std::fs::write(dir.path().join(format!("{name}.toml")),
            format!("name='{name}'\nprocessor='markdown'\n{signals}\n")).unwrap();
    }
    let (r, warnings) = Registry::load(Some(dir.path()), &|_| true);
    assert!(warnings.is_empty(), "{warnings:?}");
    (dir, r)
}
fn selected(d: &Detector<'_>, path: &str) -> Option<String> {
    let got = d.detect(Path::new(path), b"").map(|x| x.name.clone());
    if let Some(early) = d.detect_by_path(Path::new(path)) {
        assert_eq!(early.map(|x| x.name.clone()), got, "{path}");
    }
    got
}
#[test]
fn m6_compound() {
    let parsed = FormatDefinition::parse(
        "name='x'\nprocessor='markdown'\npaths=['X.*']", |_| true).unwrap();
    assert_eq!(parsed.paths, ["X.*"]);
    let (_dir, r) = registry(&[
        ("blade", "extensions=['blade.php']"),
        ("special", "names=['special.blade.php']"),
    ]);
    let d = Detector::new(&r, &[]).unwrap();
    for (path, want) in [("cart.blade.php", "blade"), ("cart.BLADE.PHP", "blade"),
        ("a.b.cart.blade.php", "blade"), ("cart.php", "php"),
        ("special.blade.php", "special")] {
        assert_eq!(selected(&d, path).as_deref(), Some(want), "{path}");
    }
    let d = Detector::new(&r, &["*.php=markdown".into()]).unwrap();
    assert_eq!(selected(&d, "cart.blade.php").as_deref(), Some("markdown"));
    assert!(FormatDefinition::parse("name='x'\nprocessor='markdown'\npaths=3", |_| true).is_err());
    let old = FormatDefinition::parse("name='x'\nprocessor='markdown'", |_| true).unwrap();
    assert!(old.paths.is_empty());
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test cli_m6_detector m6_compound -- --nocapture`.
  Expected RED: missing `paths` field/unknown field, then Blade incorrectly PHP.
- [ ] **Step 3:** add the serde-default field. In `index`, separate simple
  extensions from two-part compounds, lowercase both once. Keep a set/map keyed
  by the final extension so only candidate `.php` names split their penultimate
  extension. Match exactly the final two extensions, including mixed case;
  preserve first Definition in Registry order, exact-name priority and overrides.
  `detect_by_path` must use the same lookup without reading bytes. Plain `.txt`
  must keep the existing allocation-free lowercase-extension lookup.
- [ ] **Step 4:** rerun the exact command: `m6_compound ... ok`.
  Linux: check serde validation and precedence; native CI: repeat all cases on
  both macOS architectures and Windows with case-sensitive name expectations.
- [ ] **Step 5:** commit with
  `git commit -m "feat: add compound Format extensions and path declarations"`.

### Task 1.2: literal-indexed glob candidates and document fast paths

**Files:** modify `src/formats/detector.rs::{Detector,index,detect,detect_by_path,may_be_document}`,
`src/formats/mod.rs::read_definition` only if validation needs its warning;
append `m6_paths` in `tests/cli_m6_detector.rs`.
Public detector signatures stay unchanged. Private candidate index:

```rust
// src/formats/detector.rs, proposed private types.
struct PathRule { definition: usize, matcher: globset::GlobMatcher, basename: bool }
// A necessary literal of a signal-4 file name; a name without it skips the matcher.
enum NameFilter { Prefix(Vec<u8>), Infix(memchr::memmem::Finder<'static>) }
struct PathIndex {
    // Spec §4.1 signal 4: globs that write the extension, keyed by that lowercase
    // extension and filtered by a literal run of the file name: the whole name
    // `result.json`, the prefix `messages`, the infix `WhatsApp`.
    before_extension: std::collections::HashMap<String, Vec<(NameFilter, usize)>>,
    // Signal 6: the other globs, by mandatory literal token, after the extension.
    by_token: std::collections::HashMap<Vec<u8>, Vec<usize>>,
    fallback: Vec<usize>,
    rules: Vec<PathRule>,
}
```

- [ ] **Step 1 — RED:** append these complete tests.

```rust
#[test]
fn m6_paths() {
    let (_dir, r) = registry(&[
        ("env", "names=['.env']\nextensions=['env']\npaths=['.env.*']"),
        ("dockerfile", "names=['Dockerfile']\nextensions=['dockerfile']\npaths=['Dockerfile.*']"),
        ("ssh-config", "paths=['**/.ssh/config','**/.ssh/config.d/*']"),
        ("nginx", "names=['nginx.conf']\npaths=['**/nginx/**/*.conf','**/nginx/sites-enabled/*']"),
        ("make", "names=['Makefile','makefile']\npaths=['Makefile.*']"),
        ("systemd", "paths=['**/systemd/**/*.{path,target,conf}']"),
    ]);
    let d = Detector::new(&r, &[]).unwrap();
    for (path, want) in [
        (".env.production", "env"), ("Dockerfile.dev", "dockerfile"),
        ("prod.Dockerfile", "dockerfile"), ("/home/x/.ssh/config", "ssh-config"),
        ("/home/x/.ssh/config.d/local", "ssh-config"),
        ("/etc/nginx/sites-enabled/default", "nginx"),
        ("/etc/nginx/a/b.conf", "nginx"), ("nginx.conf", "nginx"),
        ("Makefile.debug", "make"), ("Makefile.toml", "toml"),
        ("/etc/systemd/system/demo.path", "systemd"),
    ] {
        assert_eq!(selected(&d, path).as_deref(), Some(want), "{path}");
    }
    for path in ["other.conf", "MAKEFILE", "other.asc", "Otherfile.toml.bak"] {
        assert!(selected(&d, path).is_none(), "{path}");
    }
    assert!(d.detect_by_path(Path::new("other.conf")).unwrap().is_none());
    // A user glob with no mandatory literal still follows full globset semantics.
    let (_any, any) = registry(&[("any", "paths=['**/*']")]);
    assert_eq!(selected(&Detector::new(&any, &[]).unwrap(), "a/b.unknown").as_deref(), Some("any"));
    // A user document Definition's `paths` reach the pre-search document gate,
    // which answers only with --docs (`may_be_document` is false without it).
    let docs = tempfile::tempdir().unwrap();
    std::fs::write(docs.path().join("doc-path.toml"),
        "name='doc-path'\nprocessor='docs'\npaths=['Book.*']\n[params]\nextractor='docx'\n").unwrap();
    let (r, warnings) = Registry::load(Some(docs.path()), &|_| true);
    assert!(warnings.is_empty(), "{warnings:?}");
    let d = Detector::new(&r, &[]).unwrap().with_docs(true);
    assert!(d.may_be_document(Path::new("Book.raw")));
    assert!(!d.may_be_document(Path::new("Other.raw")));
    assert!(!Detector::new(&r, &[]).unwrap().may_be_document(Path::new("Book.raw")));
}
#[test]
fn m6_path_separators_are_native() {
    let (_dir, r) = registry(&[("ssh-config", "paths=['**/.ssh/config']"),
        ("nginx", "paths=['**/nginx/sites-enabled/*']")]);
    let d = Detector::new(&r, &[]).unwrap();
    let paths = if cfg!(windows) {
        [(r"C:\Users\x\.ssh\config", "ssh-config"),
         (r"C:\etc\nginx\sites-enabled\default", "nginx")]
    } else {
        [("/home/x/.ssh/config", "ssh-config"),
         ("/etc/nginx/sites-enabled/default", "nginx")]
    };
    for (path, want) in paths {
        assert_eq!(selected(&d, path).as_deref(), Some(want));
    }
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test cli_m6_detector m6_path -- --nocapture`.
  Expected RED: `.env.production`/SSH/nginx remain Plain.
- [ ] **Step 3:** compile globs once and sort each into spec §4.1's signal 4
  or 6: signal 4 when the text after the last `.` of the last component is a
  literal extension or a `{…}` list of literals with no `*`, `?` or `[`. File a
  signal-4 rule under each lowercase extension it writes, with the longest
  literal run of its last component before the extension as the `NameFilter`:
  a prefix when the component starts with it (`messages` of `messages*.html`,
  all of `result.json`), else an infix found with `memchr::memmem`
  (`WhatsApp` of `*WhatsApp*.txt`). In `detect` and `detect_by_path`, after the
  compound extension and before `by_extension`, look up the already-normalized
  extension there (with the built-ins `json`, `html` and `txt` have entries); an
  empty entry costs nothing more, a nonempty one compares the filter and runs
  the GlobMatcher only on a pass. Select a necessary literal component or
  basename prefix/suffix for every built-in rule: `.ssh`, `ssh`, `nginx`,
  `apache2`, `httpd`, `systemd`, `git`, `ChatExport_`, `DataExport_`, `.env.`,
  `Dockerfile.`, `Containerfile.`, `Makefile.`, and the infix `WhatsApp`.
  Token absence must imply the rule cannot match; alternative literals in braces
  need a shared mandatory component or multiple conservative buckets. Candidate
  buckets filter, then the original GlobMatcher decides. Deduplicate rule IDs and
  choose Registry order, not bucket order. Basename patterns see only file_name;
  slash-containing patterns see the complete native Path. Preserve globset's
  Windows normalization and Unix backslash-as-literal behavior. Invalid globs in
  user Definitions warn with filename and `paths`; invalid built-ins fail tests.
  Maintain separate doc candidate rules for `processor='docs'`, including user
  compound extensions/paths. Built-in pre-Registry document gates remain unchanged:
  M6 adds no document Extractor. A non-document path rule never forces a pre-search
  Source open. Criterion 51's timing evidence is C11.3.
- [ ] **Step 4:** rerun the command: both tests PASS. Linux exercises `/`;
  Windows GNU clippy checks both cfg branches; only Windows native CI validates
  globset's handling of drive paths with `\`. Both macOS jobs run `/` cases.
- [ ] **Step 5:** commit with
  `git commit -m "feat: index path glob detection without scanning every rule"`.

### Task 1.3: detection table contract and Telegram precedence

**Files:** append `m6_detection_table` and `m6_telegram_precedence` in
`tests/cli_m6_detector.rs`; modify `detector.rs` only for findings consistent
with §4.1's eight signals. Binary candidate selection is C10.4.

- [ ] **Step 1 — RED:** append the complete table-driven test below.

```rust
#[test]
fn m6_detection_table() {
    let definitions = [
        ("asciidoc", "extensions=['adoc','asciidoc']"),
        ("rst", "extensions=['rst','rest']"), ("org", "extensions=['org']"),
        ("latex", "extensions=['tex','ltx','latex']"),
        ("jupyter", "extensions=['ipynb']"),
        ("ini", "names=['.gitconfig','.gitmodules','.editorconfig','.npmrc','.pypirc','.flake8','.pylintrc','.coveragerc','.hgrc']\nextensions=['ini','cfg','cnf','inf','desktop']\npaths=['**/.git/config','**/git/config']"),
        ("systemd", "extensions=['service','socket','timer','mount','automount']\npaths=['**/systemd/**/*.{path,target,slice,scope,swap,network,netdev,link,conf}']"),
        ("properties", "extensions=['properties']"),
        ("env", "names=['.env']\nextensions=['env']\npaths=['.env.*']"),
        ("ssh-config", "names=['ssh_config','sshd_config']\npaths=['**/.ssh/config','**/.ssh/config.d/*','**/ssh/ssh_config.d/*.conf','**/ssh/sshd_config.d/*.conf']"),
        ("nginx", "names=['nginx.conf']\npaths=['**/nginx/**/*.conf','**/nginx/sites-available/*','**/nginx/sites-enabled/*']"),
        ("apache", "names=['httpd.conf','apache2.conf','.htaccess']\npaths=['**/apache2/**/*.conf','**/httpd/**/*.conf','**/apache2/sites-available/*','**/apache2/sites-enabled/*']"),
        ("hcl", "extensions=['tf','tfvars','hcl','nomad']"),
        ("dockerfile", "names=['Dockerfile','Containerfile','dockerfile']\nextensions=['dockerfile','containerfile']\npaths=['Dockerfile.*','Containerfile.*']"),
        ("make", "names=['Makefile','makefile','GNUmakefile','BSDmakefile']\nextensions=['mk','mak','make']\npaths=['Makefile.*']"),
        ("just", "names=['justfile','Justfile','.justfile']\nextensions=['just']"),
        ("sql", "extensions=['sql','ddl','dml']"), ("diff", "extensions=['diff','patch','rej']"),
        ("vue", "extensions=['vue']"), ("svelte", "extensions=['svelte']"),
        ("blade", "extensions=['blade.php']"),
        ("graphql", "extensions=['graphql','gql','graphqls']"),
        ("protobuf", "extensions=['proto']"), ("har", "extensions=['har']"),
        ("eml", "extensions=['eml']"), ("mbox", "names=['mbox']\nextensions=['mbox','mbx']\nmagic=[{hex='46726f6d20'}]"),
        ("icalendar", "extensions=['ics','ical','ifb']"),
        ("vcard", "extensions=['vcf','vcard']"), ("bibtex", "extensions=['bib']"),
        ("whatsapp", "names=['_chat.txt']\npaths=['*WhatsApp*.txt']"),
        ("telegram", "paths=['**/ChatExport_*/result.json','**/DataExport_*/result.json']"),
        ("telegram-html", "paths=['**/ChatExport_*/messages*.html']"),
    ];
    let (_dir, r) = registry(&definitions);
    let d = Detector::new(&r, &[]).unwrap();
    for (name, _) in definitions {
        let def = r.get(name).unwrap();
        for exact in &def.names {
            assert_eq!(selected(&d, exact).as_deref(), Some(name), "{exact}");
        }
        for extension in &def.extensions {
            assert_eq!(selected(&d, &format!("sample.{extension}")).as_deref(), Some(name));
        }
    }
    assert_eq!(d.detect(Path::new("Inbox"), b"From sender\n").unwrap().name, "mbox");
    assert!(d.detect(Path::new("Inbox.txt"), b"From sender\n").is_none());
}
#[test]
fn m6_telegram_precedence() {
    let (_dir, r) = registry(&[
        ("telegram", "paths=['**/ChatExport_*/result.json','**/DataExport_*/result.json']"),
        ("telegram-html", "paths=['**/ChatExport_*/messages*.html']"),
        ("conf-json", "paths=['**/conf/*.json']"),
        ("whatsapp", "names=['_chat.txt']\npaths=['*WhatsApp*.txt']"),
        ("make", "names=['Makefile']\npaths=['Makefile.*']"),
        ("env", "names=['.env']\npaths=['.env.*']"),
    ]);
    let d = Detector::new(&r, &[]).unwrap();
    // Spec §4.1 signal 4: a glob that writes the extension wins over the extension.
    for (path, want) in [
        ("x/ChatExport_2024/result.json", "telegram"),
        ("x/DataExport_2024/result.json", "telegram"),
        ("x/ChatExport_2024/messages.html", "telegram-html"),
        ("x/ChatExport_2024/messages2.html", "telegram-html"),
        ("x/conf/app.json", "conf-json"),
        // Outside the folders the claimed extension keeps the file.
        ("x/result.json", "json"), ("x/messages.html", "html"),
        ("x/ChatExport_2024/other.json", "json"), ("x/ChatExport_2024/index.html", "html"),
        // WhatsApp's one glob: the app's name in the export's name, any language.
        ("x/WhatsApp Chat with Ana.txt", "whatsapp"), ("x/WhatsApp Chat - Ana.txt", "whatsapp"),
        ("x/Chat de WhatsApp con Ana.txt", "whatsapp"), ("x/_chat.txt", "whatsapp"),
        // Signal 6: a glob that leaves the extension open comes after it.
        ("Makefile.toml", "toml"), (".env.json", "json"), ("Makefile.debug", "make"),
    ] {
        assert_eq!(selected(&d, path).as_deref(), Some(want), "{path}");
    }
    for path in ["x/notes.txt", "x/WhatsApp/notes.txt", "x/WhatsApp Chat with Ana.log"] {
        assert!(selected(&d, path).is_none(), "{path}");
    }
    // An override still wins over signal 4.
    let d = Detector::new(&r, &["*.json=json".into()]).unwrap();
    assert_eq!(selected(&d, "x/ChatExport_2024/result.json").as_deref(), Some("json"));
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test cli_m6_detector -- --nocapture`.
  Expected RED before path support; after Tasks 1.1–1.2 this is a preservation
  check and may already pass. Record that honestly; add no artificial failure.
- [ ] **Step 3:** verify table spelling against M6 §4.2. C2–C10 use these literal
  signals in their real TOMLs; C12.2 verifies the real built-ins, not these test
  substitutes. WhatsApp has no localized name lists (D21): `*WhatsApp*.txt` and
  `_chat.txt` are its whole signal.
  `m6_telegram_precedence` is RED until Step 3 of Task 1.2 sorts the globs; if
  Task 1.2 landed first it may already pass, and that is recorded as such.
- [ ] **Step 4:** rerun GREEN locally and in all four native jobs. These `/`
  strings run as written everywhere; the `\` forms belong to
  `m6_path_separators_are_native`, whose Windows array also gains
  `(r"C:\x\ChatExport_2024\result.json", "telegram")` with a telegram rule.
- [ ] **Step 5:** commit with
  `git commit -m "test: cover M6 detection signals and Telegram's glob precedence"`.

### Task 1.4: widen `Segment.level` before tanda 2

**Files:** modify `src/model.rs::Segment.level`, `src/output/tree.rs::label`
(heading arm, plus a test in its `tests` module), `src/processors/markdown.rs`
(heading assignment), `src/docs/mod.rs::{Builder::chapter,Builder::heading}`,
`src/docs/isolated.rs` (the extracted-Segment writer and reader) and
`src/output/mod.rs::fixture::h`. No tanda-1 chunk sets a level; C5 (Org) and C6
(Jupyter headings) write `level: Some(n)` with `n: usize` on this type.

```rust
// src/model.rs::Segment; serde representation remains a JSON integer.
pub level: Option<usize>,
```

- [ ] **Step 1 — RED:** append to `src/output/tree.rs`'s `tests` module.

```rust
#[test]
fn heading_label_keeps_a_level_above_255() {
    let mut s = crate::output::fixture::seg("heading", "Deep", 1, 1);
    s.level = Some(300);
    assert_eq!(label(&s), format!("{} Deep", "#".repeat(300)));
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --lib heading_label_keeps_a_level_above_255 -- --nocapture`.
  Expected RED: `300` does not fit `u8` (the deny-by-default `overflowing_literals`).
- [ ] **Step 3:** change the field's type. `tree.rs`: drop the cast,
  `"#".repeat(s.level.unwrap_or(1).max(1))` (`as usize` on a `usize` fails
  `clippy::unnecessary_cast` under `-D warnings`). `markdown.rs`:
  `Some(level as usize)`, values 1..6 unchanged. `docs/mod.rs` and
  `output::fixture::h`: `Some(usize::from(level))`, their public `u8` arguments
  unchanged. `isolated.rs`: write `g.level.map_or(-1, |l| l as i64)` and read
  `usize::try_from(level).ok()`. Grep `\.level` for any other Segment use.
- [ ] **Step 4:** rerun the command: PASS; run every Markdown, document and
  output unit and snapshot test, which must not change, then the inherited fmt
  and both clippy commands.
- [ ] **Step 5:** commit with
  `git commit -m "refactor: let a heading level exceed 255"`.

## Chunk 2: keypath syntaxes

Owns new `src/processors/keypath/{lines,ini,properties,env,ssh,ical,bibtex}.rs`,
keypath `mod.rs` dispatch in tanda 1, its eight Definitions and
`tests/cli_m6_keypath.rs`. Copy the complete CLI prelude into this file.
Use each Definition's exact signals from Task 1.3; parameters below are complete
M6 additions. Register each TOML in `builtin::BUILTIN`. C6 inherits dispatch
ownership in tanda 2. Every scanner uses byte offsets of the searched Source.

C2 starts from Task 0's commit, where `tests/cli_m2.rs::m2_07` already counts
`searchfor::formats::builtin::BUILTIN.len()` and `tests/mcp_m5.rs`'s `blocked`
Definition claims `.blk`; C2 edits neither file. After each TOML it registers,
`CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test cli_m2 m2_07 -- --nocapture`
and `… --test mcp_m5 -- --nocapture` must still PASS.

For each primary fixture listed in these tests, create its `broken-` sibling
using the explicit malformed/recovery source in that task; test it through
`check` with the shown Trails, and commit its three new snapshots as well.
The Format-specific tests below fix whether malformed input recovers or ends.

### Task 2.1: INI and systemd with a compact native line cursor

**Files:** create `keypath/lines.rs::{LinePrepared,LineCursor}` and
`keypath/ini.rs::{outline,prepare}`; modify `keypath/mod.rs::{Syntax,syntax,outline_for,prepare_syntax,Keypath::names,Keypath::capability}`;
create `formats/ini.toml`, `formats/systemd.toml`; create
`tests/cli_m6_keypath.rs::m6_ini`, `tests/fixtures/config/m6.ini`,
`tests/fixtures/config/m6.service` and broken siblings.

Public Processor API stays as checked in `processors/mod.rs`. Extend the internal
syntax enum with `Ini, Properties, Env, Ssh, Ical, Bibtex`. Each new syntax module:

```rust
pub(crate) fn prepare<'a>(input: crate::model::Input<'a>, params: &'a ::toml::Table)
    -> anyhow::Result<Box<dyn crate::processors::Prepared + 'a>>;
pub(crate) fn outline(src: &[u8], params: &::toml::Table,
    anchors: Option<&[usize]>) -> anyhow::Result<crate::trail::Outline>;
// ini and systemd: [params] syntax = "ini"; name_chars = 40.
// kinds: "key" for both sections and assignments.
```
`anchors=None` enumerates names; `Some(sorted)` keeps only enclosing Segments.
Use this module interface for all six new syntaxes; retain existing JSON/YAML/TOML
walk functions and signatures unchanged.

- [ ] **Step 1 — RED:** add these complete tests and their literal primary fixture bytes.

```rust
#[test]
fn m6_ini() {
    let fx = Fixture::new();
    let ini = b"root=top\n[server]\nhost = needle.example\n  continued needle\n  port: 80\n[remote \"origin\"]\nurl=needle.git\n[a.b]\nk=needle\n";
    seed(&fx, "config/m6.ini", ini);
    check(&fx, "ini", "config/m6.ini", &["server › host", "server › host",
        "remote \"origin\" › url", "a.b › k"]);
    flags(&fx, "ini", "config/m6.ini", "server", "host", "");
    let service = b"[Unit]\nDescription=demo\n[Service]\nExecStart=/usr/bin/needle \\\n --serve\n";
    seed(&fx, "config/m6.service", service);
    check(&fx, "systemd", "config/m6.service", &["Service › ExecStart"]);
    flags(&fx, "systemd", "config/m6.service", "Service", "ExecStart", "");
    let bad = b"[s]\nk=needle\n[broken\nneedle tail\n[s]\nk=needle again\n";
    put(&fx, "broken.ini", bad);
    assert_eq!(paths(&matches(&output(&fx,"ini","broken.ini","needle",&["--json"]))),
        ["s › k", "s › k", "s › k"]);
    let names = matches(&output(&fx,"ini","config/m6.ini","server",&["--json"]));
    assert_eq!(paths(&names), [""]);
    let same = b"[s]\nk=needle\n[s]\nk=needle\n";
    put(&fx, "repeat.ini", same);
    let rows = matches(&output(&fx,"ini","repeat.ini","needle",&["--json"]));
    assert_ne!(rows[0]["trail"][0]["line"], rows[1]["trail"][0]["line"]);
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test cli_m6_keypath m6_ini -- --nocapture`.
  Expected RED: `ini`/`systemd` are unknown Formats or ini syntax is rejected.
- [ ] **Step 3:** scan physical lines once, stripping LF and an optional preceding CR only.
  INI headers are one key (dots and quoted subsection names remain literal).
  Close the previous section/key at its next sibling; repeated spellings retain
  distinct identities and source ranges. Resolve backslash continuations and
  indented-value continuation unless that line contains `=` or `:`. Header and
  key label ranges exclude their own-name Matches. A malformed header is ordinary
  text of the preceding key/section; INI has no terminal syntax error.
  `LinePrepared` stores compact offsets and borrowed label pieces, with no owned
  String per value. Its cursor shares equal complete Trails and returns a safe
  `open_from` including any still-open ancestor. Avoid `LineIndex` per Match.
  Add the eight dispatch arms and native capability for new syntaxes only.
  The same source-aware scanner backs both model and preparation, with separate
  selection logic; C11 tests every byte and backward cursor queries.
- [ ] **Step 4:** rerun the exact command with `INSTA_UPDATE=always` for new
  snapshots; inspect the tree, flat and JSON snapshots against the assertions.
  Rerun with `INSTA_UPDATE=no`: selected tests PASS, stderr empty.
  All tests run on Linux and all four native CI systems; Windows GNU all-targets clippy also applies.
- [ ] **Step 5:** stage exactly the files above, the named fixtures and this
  topic's snapshots; commit with
  `git commit -m "feat: add native INI and systemd keypath Trails"`.

### Task 2.2: properties escapes and multiline env values

**Files:** create `keypath/properties.rs` and `keypath/env.rs` (`outline`, `prepare`);
create `formats/properties.toml`, `formats/env.toml`;
append `m6_properties_env` in `tests/cli_m6_keypath.rs`;
create `tests/fixtures/config/m6.properties`, `tests/fixtures/config/.env.m6` and broken siblings.

Use Task 2.1's signatures. Definitions have respectively
`syntax='properties'` and `syntax='env'`, `name_chars=40`. Kinds: `key`.
Decoded key pieces map each escaped source span to its decoded text span; a
synthetic ellipsis has no source piece.

- [ ] **Step 1 — RED:** add these complete tests and their literal primary fixture bytes.

```rust
#[test]
fn m6_properties_env() {
    let fx = Fixture::new();
    let prop = b"# comment\nspring.datasource.url=jdbc:needle://db\ncaf\\u00e9\\:key : needle\\\n continued\n";
    seed(&fx, "config/m6.properties", prop);
    check(&fx, "properties", "config/m6.properties",
        &["spring.datasource.url", "café:key"]);
    flags(&fx,"properties","config/m6.properties","spring.datasource.url","café","");
    let env = b"# config\nexport DATABASE_URL=postgres://needle@db/app\nTEXT=\"first\nneedle # literal\nlast\" # end\n";
    seed(&fx, "config/.env.m6", env);
    check(&fx,"env","config/.env.m6",&["DATABASE_URL","TEXT"]);
    flags(&fx,"env","config/.env.m6","TEXT","DATABASE_URL","");
    put(&fx,"bad.properties",b"k=needle\nbad\\uQQQQ=needle\nlast=needle\n");
    let bad = output(&fx,"properties","bad.properties","needle",&["--json"]);
    assert!(bad.stderr.is_empty());
    assert_eq!(matches(&bad).len(),3);
    put(&fx,"bad.env",b"A=needle\nB=\"needle\nneedle tail\n");
    assert_eq!(paths(&matches(&output(&fx,"env","bad.env","needle",&["--json"]))),
        ["A","B","B"]);
    let own = matches(&output(&fx,"env","config/.env.m6","DATABASE_URL",&["--json"]));
    assert_eq!(paths(&own),[""]);
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test cli_m6_keypath m6_properties_env -- --nocapture`.
  Expected RED: new syntaxes reject input or decoded café:name/quoted Sections differ.
- [ ] **Step 3:** read Java-properties logical lines: first unescaped `=`, `:`, or whitespace
  ends a key; whitespace after the separator is outside the name. Comments begin
  with `#` or `!`; odd trailing backslashes continue a value. Decode the specified
  key escapes and valid Unicode pairs, replacing invalid Unicode with U+FFFD;
  preserve a malformed escape literally when it cannot be decoded, and continue
  reading (permissive scanner, documented in formats docs). Dots remain literal.
  For env, accept `export KEY=`, quoted multiline values and comments outside
  quotes. An unclosed value owns the tail, as a permissive continuation; record
  this recovery policy explicitly, since §7.4 gives no terminal error rule.
  Cut the visible name at name_chars while preserving decoded Name pieces.
  Reuse the compact line preparation and keep syntax-specific state separate.
- [ ] **Step 4:** rerun the exact command with `INSTA_UPDATE=always` for new
  snapshots; inspect the tree, flat and JSON snapshots against the assertions.
  Rerun with `INSTA_UPDATE=no`: selected tests PASS, stderr empty.
  All tests run on Linux and all four native CI systems; Windows GNU all-targets clippy also applies.
- [ ] **Step 5:** stage exactly the files above, the named fixtures and this
  topic's snapshots; commit with
  `git commit -m "feat: describe properties keys and multiline env assignments"`.

### Task 2.3: SSH Host and Match sections

**Files:** create `keypath/ssh.rs::{outline,prepare}`, `formats/ssh-config.toml`;
append `m6_ssh` in `tests/cli_m6_keypath.rs`; create
`tests/fixtures/config/ssh/ssh_config` and its broken sibling.

Use Task 2.1's signatures; `[params] syntax='ssh'; name_chars=40`.
Section and keyword kinds are `key`; visible section name preserves `Host` or
`Match` and the arguments as written. Includes remain text values.

- [ ] **Step 1 — RED:** add these complete tests and their literal primary fixture bytes.

```rust
#[test]
fn m6_ssh() {
    let fx=Fixture::new();
    let src=b"User root\nHost github.com gitlab.com\n  IdentityFile ~/.ssh/needle_ed25519\nMatch user ana\n  ProxyCommand=needle --proxy\nInclude /never/read/needle\n";
    seed(&fx,"config/ssh/ssh_config",src);
    check(&fx,"ssh-config","config/ssh/ssh_config",&[
        "Host github.com gitlab.com › IdentityFile",
        "Match user ana › ProxyCommand", "Match user ana › Include"]);
    flags(&fx,"ssh-config","config/ssh/ssh_config","IdentityFile","github","");
    put(&fx,"broken-ssh",b"hOsT example\nidentityfile needle\nUnrecognized needle\nMatch\nUser needle\n");
    assert_eq!(paths(&matches(&output(&fx,"ssh-config","broken-ssh","needle",&["--json"]))),
        ["hOsT example › identityfile","hOsT example › Unrecognized","Match › User"]);
    assert_eq!(paths(&matches(&output(&fx,"ssh-config","config/ssh/ssh_config","IdentityFile",&["--json"]))),
        ["Host github.com gitlab.com"]);
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test cli_m6_keypath m6_ssh -- --nocapture`.
  Expected RED: unknown Format or SSH syntax.
- [ ] **Step 3:** compare keywords ASCII-case-insensitively while preserving display spelling.
  `Host` and `Match` start sibling sections, and every other keyword starts a key
  with whitespace or `=` as separator; keys before a section remain root keys.
  An unknown keyword is still a key, an empty Host/Match section closes normally
  at its next sibling, and comment/blank lines preserve the current section.
  Never follow Include, open an SSH file, or read the user's home directory.
  Native CI verifies name-only detection separately after C1 integration.
- [ ] **Step 4:** rerun the exact command with `INSTA_UPDATE=always` for new
  snapshots; inspect the tree, flat and JSON snapshots against the assertions.
  Rerun with `INSTA_UPDATE=no`: selected tests PASS, stderr empty.
  All tests run on Linux and all four native CI systems; Windows GNU all-targets clippy also applies.
- [ ] **Step 5:** stage exactly the files above, the named fixtures and this
  topic's snapshots; commit with
  `git commit -m "feat: describe SSH configuration sections without following includes"`.

### Task 2.4: iCalendar, vCard and BibTeX

**Files:** create `keypath/ical.rs`, `keypath/bibtex.rs` (`outline`, `prepare`);
create `formats/icalendar.toml`, `formats/vcard.toml`, `formats/bibtex.toml`;
append `m6_ical_bibtex`; create `tests/fixtures/config/m6.ics`,
`tests/fixtures/config/m6.vcf`, `tests/fixtures/docs-a/m6.bib` and broken siblings.

Use Task 2.1's signatures; `syntax='ical'` for both calendar/contact,
`syntax='bibtex'` for BibTeX, `name_chars=40`. New kinds: `component`, `entry`;
properties and fields use `key`. `signature` is present on BibTeX entries.

- [ ] **Step 1 — RED:** add these complete tests and their literal primary fixture bytes.

```rust
#[test]
fn m6_ical_bibtex() {
    let fx=Fixture::new();
    let ics=b"BEGIN:VCALENDAR\nBEGIN:VEVENT\nDESCRIPTION:Bring the needle\n folded needle\nSUMMARY:Team sync\nEND:VEVENT\nEND:VCALENDAR\n";
    seed(&fx,"config/m6.ics",ics);
    check(&fx,"icalendar","config/m6.ics",&[
        "VEVENT Team sync › DESCRIPTION","VEVENT Team sync › DESCRIPTION"]);
    flags(&fx,"icalendar","config/m6.ics","Team sync","Team sync","");
    let vcf="BEGIN:VCARD\nEMAIL;TYPE=work:ana@needle.example\nFN:Ana Pérez\nEND:VCARD\n";
    seed(&fx,"config/m6.vcf",vcf.as_bytes());
    check(&fx,"vcard","config/m6.vcf",&["VCARD Ana Pérez › EMAIL"]);
    flags(&fx,"vcard","config/m6.vcf","EMAIL","Ana","");
    let bib=b"@article{knuth1984,\n title = {Literate needle {programming}},\n note = \"needle\" # suffix,\n}\n@comment{needle ignored}\n";
    seed(&fx,"docs-a/m6.bib",bib);
    check(&fx,"bibtex","docs-a/m6.bib",&["knuth1984 › title","knuth1984 › note",""]);
    flags(&fx,"bibtex","docs-a/m6.bib","knuth1984","knuth1984","");
    put(&fx,"bad.ics",b"BEGIN:VCALENDAR\nBEGIN:VEVENT\nDESCRIPTION:needle\nEND:UNKNOWN\nSUMMARY:One\nEND:VCALENDAR\nneedle\n");
    assert_eq!(paths(&matches(&output(&fx,"icalendar","bad.ics","needle",&["--json"]))),
        ["VEVENT One › DESCRIPTION",""]);
    put(&fx,"bad.bib",b"@article{a,\n title={needle\n@book(b, title=\"needle\")\n");
    assert_eq!(paths(&matches(&output(&fx,"bibtex","bad.bib","needle",&["--json"]))),
        ["a › title","b › title"]);
    put(&fx,"kinds.ics",b"BEGIN:VCALENDAR\nBEGIN:VEVENT\nX:needle\nEND:VEVENT\nBEGIN:VEVENT\nX:needle\nEND:VEVENT\nEND:VCALENDAR\n");
    assert_eq!(paths(&matches(&output(&fx,"icalendar","kinds.ics","needle",&["--json"]))),
        ["VEVENT[1] › X","VEVENT[2] › X"]);
    put(&fx,"kinds.vcf",b"BEGIN:VCARD\nEMAIL:needle\nEND:VCARD\nBEGIN:VCARD\nEMAIL:needle\nEND:VCARD\n");
    assert_eq!(paths(&matches(&output(&fx,"vcard","kinds.vcf","needle",&["--json"]))),
        ["VCARD[1] › EMAIL","VCARD[2] › EMAIL"]);
    // `X` of BEGIN:X is the file's text (§3 rule 4, §10.2), with or without SUMMARY/FN.
    for (format,file,name,lines) in [("icalendar","config/m6.ics","VEVENT",vec![2u64]),
        ("icalendar","kinds.ics","VEVENT",vec![2,5]),("vcard","config/m6.vcf","VCARD",vec![1]),
        ("vcard","kinds.vcf","VCARD",vec![1,4])] {
        let rows=matches(&output(&fx,format,file,name,&["--json","--in-headings"]));
        let got:Vec<u64>=rows.iter().map(|r|r["line_number"].as_u64().unwrap()).collect();
        assert_eq!(got,lines,"{file}: {name}");
    }
    // The ordinal is added: it matches the name's text and has no source piece.
    for (format,file) in [("icalendar","kinds.ics"),("vcard","kinds.vcf")] {
        let out=output(&fx,format,file,"\\[1\\]",&["--json","--in-headings"]);
        assert_eq!(code(&out),1,"{file}: {}",stdout(&out));
    }
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test cli_m6_keypath m6_ical_bibtex -- --nocapture`.
  Expected RED: missing syntaxes, late SUMMARY/FN or BibTeX nesting fails, or
  `--in-headings VEVENT` finds nothing.
- [ ] **Step 3:** unfold content lines with byte-piece maps; strip parameters from property
  labels only. During preparation collect SUMMARY/FN/TZID anywhere in a component
  and sibling ordinals, hide one enclosing VCALENDAR, then emit labels before
  answering the first Match. BEGIN/END close the innermost matching component and
  its descendants; a stray END is text. A component's name carries a source
  piece over the `X` of its `BEGIN:X` line and one over its SUMMARY/FN/TZID value
  (unfolded, one piece per physical line); the joining space and the `[N]`
  ordinal are added and carry none (spec §3 rule 4, §10.2). BibTeX scans braced/parenthesized entries, nested
  braced values, quoted values, bare values and `#` concatenations. `@string`
  and `@preamble` are entry names as specified; `@comment` is entirely outside
  structure. Recover an unclosed entry at the next line-start @, without losing
  preceding valid Trails. Cap visible depth at 256 with an iterative lexical
  stack. Names from later fields must not be rescanned for every Match.
- [ ] **Step 4:** rerun the exact command with `INSTA_UPDATE=always` for new
  snapshots; inspect the tree, flat and JSON snapshots against the assertions.
  Rerun with `INSTA_UPDATE=no`: selected tests PASS, stderr empty.
  All tests run on Linux and all four native CI systems; Windows GNU all-targets clippy also applies.
- [ ] **Step 5:** stage exactly the files above, the named fixtures and this
  topic's snapshots; commit with
  `git commit -m "feat: add calendar contact and BibTeX keypath syntaxes"`.

## Chunk 3: blocks

Owns `src/processors/blocks/{mod,nginx,apache,hcl,graphql,protobuf}.rs`,
`formats/{nginx,apache,hcl,graphql,protobuf}.toml`,
`tests/cli_m6_blocks.rs` and its fixtures/snapshots. Copy the CLI prelude.
Definitions use exactly Task 1.3's signals, `processor='blocks'`,
`syntax` equal to the filename stem, and `name_chars=40`.
Module/by_name registration adds `"blocks" => Some(&blocks::Blocks)`.

### Task 3.1: nginx and Apache block scopes and late server names

**Files:** create `blocks/mod.rs::{Blocks,BlockPrepared,BlockCursor}`,
`blocks/nginx.rs::scan`, `blocks/apache.rs::scan`; create their TOMLs;
create `tests/cli_m6_blocks.rs::m6_servers`;
create `tests/fixtures/config/nginx/nginx.conf`, `m6-apache.conf` and broken siblings.

New public Processor (same trait signatures as `Processor` in the header):

```rust
pub struct Blocks;
// Blocks implements Processor; capability(valid syntax) = Capability::Native.
// Private syntax scanners feed the preparation through this module-local API:
fn scan(src: &[u8], name_chars: usize) -> anyhow::Result<BlockIndex>;
// BlockIndex is a compact ordered table of source ranges, label pieces,
// parents and late-name ingredients. BlockCursor borrows it.
```
`nginx`/`apache` kinds are `block`, `key`; other syntax kinds are listed below.

- [ ] **Step 1 — RED:** add these complete tests and their literal primary fixture bytes.

```rust
#[test]
fn m6_servers() {
    let fx=Fixture::new();
    let nginx=b"http {\n server {\n  location /api {\n   proxy_pass http://needle:8080;\n  }\n  server_name example.com;\n }\n}\n";
    seed(&fx,"config/nginx/nginx.conf",nginx);
    check(&fx,"nginx","config/nginx/nginx.conf",&[
        "http › server example.com › location /api › proxy_pass"]);
    flags(&fx,"nginx","config/nginx/nginx.conf","example.com","example.com","");
    let apache=b"<VirtualHost *:443>\n <Directory /var/www>\n  Require ip 10.0.0.0/8 # needle\n </Directory>\n ServerName example.com\n</VirtualHost>\n";
    seed(&fx,"config/apache2/apache2.conf",apache);
    check(&fx,"apache","config/apache2/apache2.conf",&[
        "VirtualHost *:443 example.com › Directory /var/www › Require"]);
    flags(&fx,"apache","config/apache2/apache2.conf","example.com","example.com","");
    put(&fx,"bad-nginx",b"http { k needle; }\n}\nk needle;\n");
    assert_eq!(paths(&matches(&output(&fx,"nginx","bad-nginx","needle",&["--json"]))),
        ["http › k",""]);
    put(&fx,"bad-apache",b"<Directory /x>\nX needle\n</unknown>\nY needle\n");
    assert_eq!(paths(&matches(&output(&fx,"apache","bad-apache","needle",&["--json"]))),
        ["Directory /x › X","Directory /x › Y"]);
    put(&fx,"quoted-nginx",b"http { set $x '}; needle'; # { ignored\n k \"{;needle\"; }\n");
    assert_eq!(paths(&matches(&output(&fx,"nginx","quoted-nginx","needle",&["--json"]))),
        ["http › set","http › k"]);
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test cli_m6_blocks m6_servers -- --nocapture`.
  Expected RED: unknown blocks Processor or empty/incorrect server Trails.
- [ ] **Step 3:** scan block/directive boundaries with quoted-string and comment state.
  nginx semicolons finish keys; block opening labels contain the directive and
  its arguments, never body bytes. The first server_name within the server's own
  scope appends its first value; preparation finalizes it before any cursor.
  Apache section names compare without case and can close an ancestor plus its
  children; unknown end tags are ignored. Append first ServerName to VirtualHost,
  excluding nested unrelated sections. Support backslash physical-line
  continuation. Own-name source pieces point at both opening arguments and the
  later server-name value, and labels exclude those source spans from their own
  Trail. A stray nginx `}` terminates structure there; open blocks end at EOF.
  Index offsets compactly rather than allocating OutlineSegments per directive;
  trails() builds an anchor-filtered model, names() collects searchable pieces.
- [ ] **Step 4:** rerun the exact command with `INSTA_UPDATE=always` for new
  snapshots; inspect the tree, flat and JSON snapshots against the assertions.
  Rerun with `INSTA_UPDATE=no`: selected tests PASS, stderr empty.
  All tests run on Linux and all four native CI systems; Windows GNU all-targets clippy also applies.
- [ ] **Step 5:** stage exactly the files above, the named fixtures and this
  topic's snapshots; commit with
  `git commit -m "feat: describe nginx and Apache configuration blocks"`.

### Task 3.2: HCL attributes, lists, interpolation and heredocs

**Files:** create `blocks/hcl.rs::scan`, `formats/hcl.toml`;
modify `blocks/mod.rs` HCL dispatch; append `m6_hcl`;
create `tests/fixtures/config/m6.tf` and `broken-m6.tf`.

No additional trait API. HCL `syntax='hcl'`; kinds `block`, `key`, `element`.
Block labels remove quotes from type labels; array element names append zero-based
`[N]` to the holder key, as JSON does.

- [ ] **Step 1 — RED:** add these complete tests and their literal primary fixture bytes.

```rust
#[test]
fn m6_hcl() {
    let fx=Fixture::new();
    let src=b"resource \"aws_instance\" \"web\" {\n tags = {\n  Name = \"needle-web\"\n }\n ingress = [\n  { port = 80 },\n  { name = \"needle\" }\n ]\n script = <<-EOT\n  echo needle ${not_a_block}\n  EOT\n}\n";
    seed(&fx,"config/m6.tf",src);
    check(&fx,"hcl","config/m6.tf",&[
        "resource aws_instance web › tags › Name",
        "resource aws_instance web › ingress[1] › name",
        "resource aws_instance web › script"]);
    flags(&fx,"hcl","config/m6.tf","tags","Name","\\[1\\]");
    put(&fx,"hcl-lex",br#"x "y" {
 value = "${join("}", ["needle", "%{if true}x%{endif}"])}"
 /* { */ other = "needle" // }
}
}
needle
"#);
    assert_eq!(paths(&matches(&output(&fx,"hcl","hcl-lex","needle",&["--json"]))),
        ["x y › value","x y › other",""]);
    put(&fx,"hcl-name",b"resource \"needle\" \"web\" {\n k=1\n}\n");
    assert_eq!(paths(&matches(&output(&fx,"hcl","hcl-name","needle",&["--json"]))),[""]);
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test cli_m6_blocks m6_hcl -- --nocapture`.
  Expected RED: HCL syntax unsupported or interpolation produces false blocks.
- [ ] **Step 3:** track `{}` object/block braces and `[]` lists with a nonrecursive stack.
  Recognize attributes by `=` outside strings; a braced RHS is an object under
  its key, never a block named `tags =`. Keep a key holding a list hidden while
  its elements use holder[N]. Skip `#`, `//`, `/* */`, quoted literals, nested
  interpolation/directive expression delimiters, and heredoc bodies through the
  matching marker (indent allowed only for `<<-`). Template braces inside a
  string never change outer scope. Use a separate lexical stack after visible
  depth 256 so balanced input can recover its outer structure. Stray structural
  closer ends structure; incomplete scopes close at EOF. No HCL evaluation,
  include, variable substitution or Terraform address inference.
- [ ] **Step 4:** rerun the exact command with `INSTA_UPDATE=always` for new
  snapshots; inspect the tree, flat and JSON snapshots against the assertions.
  Rerun with `INSTA_UPDATE=no`: selected tests PASS, stderr empty.
  All tests run on Linux and all four native CI systems; Windows GNU all-targets clippy also applies.
- [ ] **Step 5:** stage exactly the files above, the named fixtures and this
  topic's snapshots; commit with
  `git commit -m "feat: add HCL block attribute and collection Trails"`.

### Task 3.3: GraphQL definitions and Protobuf members

**Files:** create `blocks/graphql.rs`, `blocks/protobuf.rs` (`scan`), their TOMLs;
append `m6_schema_blocks`; create `tests/fixtures/code-c/m6.graphql`,
`tests/fixtures/code-c/m6.proto` and broken siblings; add only their kind lines
in `tree::label` and schema description.

GraphQL kinds: `type`, `interface`, `input`, `enum`, `union`, `scalar`,
`schema`, `directive`, `extend`, `query`, `mutation`, `subscription`, `fragment`,
`field`. Protobuf kinds: `message`, `enum`, `service`, `rpc`, `oneof`, `extend`,
`field`. `tree::label`'s existing fallback already prints `kind name`; preserve
that for these kinds. No new JSON fields or enum restriction.

- [ ] **Step 1 — RED:** add these complete tests and their literal primary fixture bytes.

```rust
#[test]
fn m6_schema_blocks() {
    let fx=Fixture::new();
    let gql=b"type User {\n email: String! # needle\n}\nquery GetUser {\n user(id: 1) {\n  posts {\n   title(needle: true)\n  }\n }\n}\n";
    seed(&fx,"code-c/m6.graphql",gql);
    check(&fx,"graphql","code-c/m6.graphql",&["User › email","GetUser › user › posts › title"]);
    flags(&fx,"graphql","code-c/m6.graphql","User","email","");
    let proto=b"message User {\n string email = 1 [json_name = \"needle\"];\n oneof choice {\n  string nick = 2; // needle\n }\n}\nservice Users {\n rpc Get (User) returns (User) { option note = \"needle\"; }\n}\n";
    seed(&fx,"code-c/m6.proto",proto);
    check(&fx,"protobuf","code-c/m6.proto",&["User › email","User › choice › nick","Users › Get"]);
    flags(&fx,"protobuf","code-c/m6.proto","User","email","");
    for (format,src,want) in [
        ("graphql", "query A { x(arg: \"needle }\") }\n}\nneedle\n", vec!["A › x", ""]),
        ("protobuf", "message A { string x = 1; // needle\n}\n}\nneedle\n", vec!["A › x", ""]),
    ] {
        put(&fx,"bad-schema",src.as_bytes());
        assert_eq!(paths(&matches(&output(&fx,format,"bad-schema","needle",&["--json"]))),want);
    }
    put(&fx,"anonymous.gql",b"{ user { id(needle:true) } }\n");
    assert_eq!(paths(&matches(&output(&fx,"graphql","anonymous.gql","needle",&["--json"]))),
        ["query › user › id"]);
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test cli_m6_blocks m6_schema_blocks -- --nocapture`.
  Expected RED: missing schema syntaxes or fields flatten to parent blocks.
- [ ] **Step 3:** lex GraphQL normal and triple-quoted block strings and # comments. Read
  named definitions, anonymous query blocks, aliases and argument/selection
  nesting; fields with arguments/selection sets open field scopes, as do all type
  fields. Union/scalar/directive declarations without bodies still delimit their
  own Sections. Add cases for every declared keyword through a table of generated
  one-definition files (type/interface/input/enum field forms, remaining keywords
  with a value or selection), using the exact keyword set above. Protobuf lexes
  strings and line/block comments, nested message/enum/service/rpc/oneof/extend,
  field/enum-value names; package/import/option alone add no Segment. Exclude
  member's name from its own Trail while the rest of the member line, including
  an end-of-line comment, belongs to it. A stray closer ends structure and open
  scopes close at EOF. Run the same compact-index cursor tests as server formats.
- [ ] **Step 4:** rerun the exact command with `INSTA_UPDATE=always` for new
  snapshots; inspect the tree, flat and JSON snapshots against the assertions.
  Rerun with `INSTA_UPDATE=no`: selected tests PASS, stderr empty.
  All tests run on Linux and all four native CI systems; Windows GNU all-targets clippy also applies.
- [ ] **Step 5:** stage exactly the files above, the named fixtures and this
  topic's snapshots; commit with
  `git commit -m "feat: add GraphQL and Protobuf structural Trails"`.

## Chunk 4: buildfile

Owns `src/processors/buildfile/{mod,dockerfile,make,just}.rs`,
`formats/{dockerfile,make,just}.toml`, `tests/cli_m6_buildfile.rs`, its unique
fixtures and snapshots. Copy the CLI prelude. Register `Buildfile` through
by_name; Definitions use Task 1.3 signals and `name_chars=40`.
C4 starts from Task 0's commit, where the Detector unit tests' `makey` claims
`Makey`, so registering `make.toml` (which claims `Makefile`) leaves
`cargo test --offline --locked --lib formats::detector` green; C4 edits no file
of `src/formats/`.

### Task 4.1: Docker stages, instructions and heredocs

**Files:** create `buildfile/mod.rs::{Buildfile,BuildPrepared,BuildCursor}`,
`buildfile/dockerfile.rs::scan`, `formats/dockerfile.toml`;
create `tests/cli_m6_buildfile.rs::m6_dockerfile` and
`tests/fixtures/config/m6.dockerfile` plus broken sibling.

Public `pub struct Buildfile;` implements the checked Processor contract,
including native prepare/capability. `[params] syntax='dockerfile'`.
Kinds: `stage`, `instruction`; stage signature is FROM's physical opening line.
Instruction labels are their literal keywords, with source pieces.

- [ ] **Step 1 — RED:** add these complete tests and their literal primary fixture bytes.

```rust
#[test]
fn m6_dockerfile() {
    let fx=Fixture::new();
    let src=b"ARG BASE=alpine\nFROM ${BASE} AS build\nRUN echo start \\\n # continuation comment\n needle-build --release\nRUN <<EOF\nneedle heredoc\nEOF\nFROM scratch\nCOPY --from=build /needle /app\n";
    seed(&fx,"config/m6.dockerfile",src);
    check(&fx,"dockerfile","config/m6.dockerfile",&[
        "build › RUN","build › RUN","stage 1 › COPY"]);
    flags(&fx,"dockerfile","config/m6.dockerfile","build","build","^stage [0-9]+$");
    put(&fx,"docker-escape",b"# escape=`\nFROM alpine\nRUN echo `\n needle\n");
    assert_eq!(paths(&matches(&output(&fx,"dockerfile","docker-escape","needle",&["--json"]))),
        ["stage 0 › RUN"]);
    put(&fx,"bad-docker",b"FROM alpine AS build\nRUN <<END\nneedle\nFROM fake needle\n");
    assert_eq!(paths(&matches(&output(&fx,"dockerfile","bad-docker","needle",&["--json"]))),
        ["build › RUN","build › RUN"]);
    assert_eq!(paths(&matches(&output(&fx,"dockerfile","config/m6.dockerfile","build",&["--json","-m1"]))),
        [""]);
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test cli_m6_buildfile m6_dockerfile -- --nocapture`.
  Expected RED: unknown buildfile or stage/instruction Sections absent.
- [ ] **Step 3:** scan leading parser directives for the escape character, instructions with
  continuation lines and comment lines skipped within continuation, and one or
  multiple RUN/COPY heredoc bodies through their terminators. A FROM starts a
  stage; AS name is case-insensitive syntax but preserves name text; unnamed
  stages use zero-based stage N. ARG before any FROM has just its instruction.
  Exclude the FROM stage-name span from stage membership; instruction own keyword
  likewise excludes itself. Keep bodies verbatim, without shell parsing or
  reading external files. An unclosed heredoc owns EOF, so fake FROM text inside
  it opens nothing. Index compactly and provide native open_from.
- [ ] **Step 4:** rerun the exact command with `INSTA_UPDATE=always` for new
  snapshots; inspect the tree, flat and JSON snapshots against the assertions.
  Rerun with `INSTA_UPDATE=no`: selected tests PASS, stderr empty.
  All tests run on Linux and all four native CI systems; Windows GNU all-targets clippy also applies.
- [ ] **Step 5:** stage exactly the files above, the named fixtures and this
  topic's snapshots; commit with
  `git commit -m "feat: describe Dockerfile stages and instruction Sections"`.

### Task 4.2: Makefile rules, variables and conditional scopes

**Files:** create `buildfile/make.rs::scan`, `formats/make.toml`; append `m6_make`;
create `tests/fixtures/config/m6.mk` and broken sibling.

`[params] syntax='make'`. Kinds: `target`, `key`, `conditional`.
Target and conditional `signature` preserve their complete opening rule line.
Recipe prefix is tab until `.RECIPEPREFIX` assigns a first character.

- [ ] **Step 1 — RED:** add these complete tests and their literal primary fixture bytes.

```rust
#[test]
fn m6_make() {
    let fx=Fixture::new();
    let src=b"ifeq ($(OS),Windows_NT)\nrelease test: deps\n\t@echo needle\n\n# within recipe gap\n\techo needle again\nelse\nNAME := needle\nendif\ndefine BODY\nneedle body\nendef\n.RECIPEPREFIX = >\n%.o: %.c\n>echo needle pattern\n";
    seed(&fx,"config/m6.mk",src);
    check(&fx,"make","config/m6.mk",&[
        "ifeq ($(OS),Windows_NT) › release test", "ifeq ($(OS),Windows_NT) › release test",
        "ifeq ($(OS),Windows_NT) › NAME","BODY","%.o"]);
    flags(&fx,"make","config/m6.mk","release test","release","");
    put(&fx,"make-lex",b"OBJ := $(OBJ:.o=.c)\nKEY += needle \\\n more\nfoo:: deps\n\techo needle\nendif\nendef\nneedle\n");
    assert_eq!(paths(&matches(&output(&fx,"make","make-lex","needle",&["--json"]))),
        ["KEY","foo",""]);
    for operator in ["=",":=","::=","?=","+=","!="] {
        let src=format!("override export NAME {operator} needle\n");
        put(&fx,"operators.mk",src.as_bytes());
        assert_eq!(paths(&matches(&output(&fx,"make","operators.mk","needle",&["--json"]))),["NAME"]);
    }
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test cli_m6_buildfile m6_make -- --nocapture`.
  Expected RED: Make syntax unsupported or variable substitutions mistaken for rules.
- [ ] **Step 3:** recognize operators longest-first before classifying rule colons. Skip
  balanced $(...) and ${...} while scanning separators, including nested variable
  references and static-pattern colons. Parse target text literally up to the
  rule separator; include tab/prefix recipe lines, comments and blank gaps until
  an unrelated non-recipe statement. A conditional owns its else branches;
  variable/define blocks close at endef or EOF. Unknown endif/endef stays text.
  Include export/override combinations and `.RECIPEPREFIX` state in preparation.
  Shell recipe content is opaque, so colons, # and apparent Make directives
  inside it never affect structure. Match source line numbers across CRLF.
- [ ] **Step 4:** rerun the exact command with `INSTA_UPDATE=always` for new
  snapshots; inspect the tree, flat and JSON snapshots against the assertions.
  Rerun with `INSTA_UPDATE=no`: selected tests PASS, stderr empty.
  All tests run on Linux and all four native CI systems; Windows GNU all-targets clippy also applies.
- [ ] **Step 5:** stage exactly the files above, the named fixtures and this
  topic's snapshots; commit with
  `git commit -m "feat: describe Makefile targets variables and conditionals"`.

### Task 4.3: just recipes and aliases

**Files:** create `buildfile/just.rs::scan`, `formats/just.toml`; append `m6_just`;
create `tests/fixtures/config/m6.just` and broken sibling.

`[params] syntax='just'`. Kinds: `recipe`, `key`.
Recipe name is its identifier only; signature is the recipe declaration line.
Attributes and a leading @ belong to the declaration syntax.

- [ ] **Step 1 — RED:** add these complete tests and their literal primary fixture bytes.

```rust
#[test]
fn m6_just() {
    let fx=Fixture::new();
    let src=b"# needle documentation\n[linux]\n@deploy host='prod': build\n    rsync -a build/ host:/srv/needle/\n\nURL := 'needle'\nalias ship := deploy # needle\n";
    seed(&fx,"config/m6.just",src);
    check(&fx,"just","config/m6.just",&["","deploy","URL","ship"]);
    flags(&fx,"just","config/m6.just","deploy","deploy","");
    put(&fx,"bad.just",b"run:\n    echo needle\nstray line needle\nother:\n    needle\n");
    assert_eq!(paths(&matches(&output(&fx,"just","bad.just","needle",&["--json"]))),
        ["run","","other"]);
    let own=matches(&output(&fx,"just","config/m6.just","@deploy",&["--json"]));
    assert_eq!(paths(&own),[""]);
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test cli_m6_buildfile m6_just -- --nocapture`.
  Expected RED: just syntax unsupported or documentation incorrectly inside recipe.
- [ ] **Step 3:** recognize optional attributes and @, then the recipe identifier,
  parameters and dependency separator outside quoted/default expressions.
  Recipe body is the following indented lines; preceding documentation comments
  remain outside it. Variables and alias RHS text are key Sections. Unrecognized
  top-level lines close the previous recipe and remain Plain; an open body ends
  at EOF. Retain source-only searchable names; never evaluate interpolation,
  load an import, run just or invoke a shell. Finish compact cursors for all
  three syntaxes and hand automatic-detection tests to the orchestrator.
- [ ] **Step 4:** rerun the exact command with `INSTA_UPDATE=always` for new
  snapshots; inspect the tree, flat and JSON snapshots against the assertions.
  Rerun with `INSTA_UPDATE=no`: selected tests PASS, stderr empty.
  All tests run on Linux and all four native CI systems; Windows GNU all-targets clippy also applies.
- [ ] **Step 5:** stage exactly the files above, the named fixtures and this
  topic's snapshots; commit with
  `git commit -m "feat: describe just recipes and aliases"`.

## Chunk 5: headings

Owns `src/processors/headings/{mod,asciidoc,rst,org,latex}.rs`, four TOMLs,
`tests/cli_m6_headings.rs`, fixtures and snapshots. Copy the CLI prelude.
Register `Headings` by name. Its valid syntaxes return native compact cursors;
small model outlines may use `OutlinePrepared`, with no model fallback.
`Segment.level` is already `Option<usize>` (C1.4, D23); C5 sets it, never
changes its type. This touches neither another tanda-2 chunk's scanner nor its
test file.

### Task 5.1: AsciiDoc and reStructuredText headings and blocks

**Files:** create `headings/mod.rs::{Headings,HeadingPrepared,HeadingCursor}`,
`headings/asciidoc.rs::scan`, `headings/rst.rs::scan`;
create `formats/asciidoc.toml`, `formats/rst.toml`;
create `tests/cli_m6_headings.rs::m6_adoc_rst`;
create `tests/fixtures/docs-a/m6.adoc`, `m6.rst` and broken siblings.

`pub struct Headings;` implements Processor with the existing trait signatures.
Definitions: `processor='headings'`, `syntax='asciidoc'` or `'rst'`,
`name_chars=40`, signals in Task 1.3. Kinds `document`, `heading`, `code`,
`directive`. Headings/code keep opening-line `signature`; heading `level`
is the semantic depth, not the marker count in AsciiDoc.

- [ ] **Step 1 — RED:** add these complete tests and their literal primary fixture bytes.

```rust
#[test]
fn m6_adoc_rst() {
    let fx=Fixture::new();
    let adoc=b"= Deploy guide\n== Install\n=== Linux\n[source,bash]\n----\napt install needle-pkg\n== needle literal heading\n----\n";
    seed(&fx,"docs-a/m6.adoc",adoc);
    check(&fx,"asciidoc","docs-a/m6.adoc",&[
        "Deploy guide › Install › Linux › bash","Deploy guide › Install › Linux › bash"]);
    flags(&fx,"asciidoc","docs-a/m6.adoc","Linux","Install","");
    let rst=b"Install\n=======\n\nLinux\n-----\n\n.. code-block:: bash\n   :linenos:\n\n   apt install needle-pkg\n\n.. note::\n\n   needle note\n";
    seed(&fx,"docs-a/m6.rst",rst);
    check(&fx,"rst","docs-a/m6.rst",&["Install › Linux › bash","Install › Linux › note"]);
    flags(&fx,"rst","docs-a/m6.rst","Linux","Linux","");
    put(&fx,"bad.adoc",b"= Guide\n== A\n....\n== needle\nneedle\n");
    assert_eq!(paths(&matches(&output(&fx,"asciidoc","bad.adoc","needle",&["--json"]))),
        ["Guide › A › literal","Guide › A › literal"]);
    put(&fx,"bad.rst",b"Title\n=====\n\nText::\n\n   needle\n\nnext needle\n");
    assert_eq!(paths(&matches(&output(&fx,"rst","bad.rst","needle",&["--json"]))),
        ["Title › literal","Title"]);
    assert_eq!(paths(&matches(&output(&fx,"asciidoc","docs-a/m6.adoc","Linux",&["--json"]))),
        ["Deploy guide › Install"]);
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test cli_m6_headings m6_adoc_rst -- --nocapture`.
  Expected RED: missing headings Processor or styles/code scopes wrong.
- [ ] **Step 3:** implement AsciiDoc =/accepted # headings with required space; top title is
  document, subsequent marker counts map to levels 1–5. Legacy two-line titles
  are text. All eight listed block delimiters suppress heading parsing; listing,
  literal and fences expose code kinds while comment/passthrough/other blocks
  stay opaque with no added Segment. `[source,LANG]` supplies the following
  listing name. Include directives are never followed. rST remembers style
  (punctuation plus overline flag) in first-occurrence order; underline length
  must cover the text. No document-title promotion. Code directives own options
  and indented body, other directives use directive kind, comments remain opaque,
  and `::` opens literal indented content. Dedent closes the block. Open blocks
  end at EOF. Prepare compact spans and explicit level-parent ranges; no tables
  or lists are invented for either syntax.
- [ ] **Step 4:** rerun the exact command with `INSTA_UPDATE=always` for new
  snapshots; inspect the tree, flat and JSON snapshots against the assertions.
  Rerun with `INSTA_UPDATE=no`: selected tests PASS, stderr empty.
  All tests run on Linux and all four native CI systems; Windows GNU all-targets clippy also applies.
- [ ] **Step 5:** stage exactly the files above, the named fixtures and this
  topic's snapshots; commit with
  `git commit -m "feat: add AsciiDoc and reStructuredText heading Trails"`.

### Task 5.2: Org headings, source blocks and header columns

**Files:** create `headings/org.rs::scan`, `formats/org.toml`; append `m6_org`;
create `tests/fixtures/docs-a/m6.org`, broken sibling.

`syntax='org'`, `name_chars=40`. Kinds `document`, `heading`, `code`, `column`.
Org levels above 255 use the `Option<usize>` of Task 1.4, with no new JSON field.
The 256 limit concerns simultaneously visible Segments, not star count.

- [ ] **Step 1 — RED:** add these complete tests and their literal primary fixture bytes.

```rust
#[test]
fn m6_org() {
    let fx=Fixture::new();
    let src=b"#+TITLE: Budget\n* TODO Costs :work:\n| Item | Price |\n|------+-------|\n| Hosting | needle 9 |\n#+BEGIN_SRC bash\n* needle text\n#+END_SRC\n#+BEGIN_EXAMPLE\nneedle example\n#+END_EXAMPLE\n";
    seed(&fx,"docs-a/m6.org",src);
    check(&fx,"org","docs-a/m6.org",&[
        "Budget › TODO Costs :work: › Price", "Budget › TODO Costs :work: › bash",
        "Budget › TODO Costs :work: › example"]);
    flags(&fx,"org","docs-a/m6.org","Costs","TODO","");
    let rows=matches(&output(&fx,"org","docs-a/m6.org","needle",&["--json","--expand"]));
    assert_eq!(rows[0]["section"]["start_line"],3);
    assert_eq!(rows[0]["section"]["end_line"],5);
    put(&fx,"deep.org",format!("{} Deep\nneedle\n", "*".repeat(300)).as_bytes());
    let deep=matches(&output(&fx,"org","deep.org","needle",&["--json"]));
    assert_eq!(deep[0]["trail"][0]["level"],300);
    put(&fx,"bad.org",b"* A\n#+begin_src\n* needle\nneedle\n");
    assert_eq!(paths(&matches(&output(&fx,"org","bad.org","needle",&["--json"]))),
        ["A › src","A › src"]);
    put(&fx,"no-header.org",b"* A\n| plain | needle |\n");
    assert_eq!(paths(&matches(&output(&fx,"org","no-header.org","needle",&["--json"]))),["A"]);
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test cli_m6_headings m6_org -- --nocapture`.
  Expected RED: Org unsupported, column missing or level 300 not kept.
- [ ] **Step 3:** read star count followed by space, preserving TODO/priority/tag text in
  names. TITLE supplies a document root. Source/example blocks compare markers
  without case; absent source language is src. Other BEGIN blocks suppress all
  heading/table parsing without a Segment. A first table row followed by a rule
  supplies header cells, and later data-cell byte ranges select column Segments
  with the table's entire Section. Literal separators/index punctuation have no
  searchable name pieces. Use iterative nesting counters
  past 256; a high single heading level remains faithfully numeric.
- [ ] **Step 4:** rerun the exact command with `INSTA_UPDATE=always` for new
  snapshots; inspect the tree, flat and JSON snapshots against the assertions.
  Rerun with `INSTA_UPDATE=no`: selected tests PASS, stderr empty.
  All tests run on Linux and all four native CI systems; Windows GNU all-targets clippy also applies.
- [ ] **Step 5:** stage exactly the files above, the named fixtures and this
  topic's snapshots; commit with
  `git commit -m "feat: describe Org headings source blocks and table columns"`.

### Task 5.3: LaTeX balanced titles and environment-bounded headings

**Files:** create `headings/latex.rs::scan`, `formats/latex.toml`; append `m6_latex`;
create `tests/fixtures/docs-a/m6.tex` and broken sibling.

`syntax='latex'`, `name_chars=40`,
`hidden_environments=['document','itemize','enumerate','description']`.
Kinds `document`, `heading`, `environment`, `code`; environment signature is its
begin line; section commands map in order to levels 1–7, starred forms included.

- [ ] **Step 1 — RED:** add these complete tests and their literal primary fixture bytes.

```rust
#[test]
fn m6_latex() {
    let fx=Fixture::new();
    let src=br#"\title{A Study of Needles}
\begin{document}
\section{Methods}
\subsection[Short]{Sampling}
\begin{figure}
\caption{needle distribution}
\end{figure}
\begin{minted}{bash}
\section{needle text}
\end{minted}
\end{document}
"#;
    seed(&fx,"docs-a/m6.tex",src);
    check(&fx,"latex","docs-a/m6.tex",&[
        "A Study of Needles › Methods › Sampling › figure",
        "A Study of Needles › Methods › Sampling › bash"]);
    flags(&fx,"latex","docs-a/m6.tex","Sampling","Sampling","");
    put(&fx,"tex-edge",br#"\section*{One {balanced}
 title}
\begin{figure}
\subsection{Inner}
needle
\end{figure}
needle
% \section{ignored}
\% needle
\end{never}
"#);
    assert_eq!(paths(&matches(&output(&fx,"latex","tex-edge","needle",&["--json"]))),
        ["One {balanced} title › figure › Inner", "One {balanced} title", "One {balanced} title"]);
    put(&fx,"bad.tex",br#"\section{A}
\begin{verbatim}
\section{needle}
needle
"#);
    assert_eq!(paths(&matches(&output(&fx,"latex","bad.tex","needle",&["--json"]))),
        ["A › verbatim","A › verbatim"]);
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test cli_m6_headings m6_latex -- --nocapture`.
  Expected RED: LaTeX unsupported or heading survives the environment that bounds it.
- [ ] **Step 3:** lex escaped percent vs comment, balanced multi-line braced titles and
  optional short titles. Preserve title text as written rather than expanding
  macros; a title over several lines joins them with one space, one source piece
  per line, as Markdown's setext headings and `Name` do (`model.rs::Name::text`).
  Ignore commands inside verbatim/Verbatim/comment/listing code bodies;
  lstlisting language options and minted's language argument name code Sections.
  Ordinary environments nest with explicit ranges. Hidden environments retain
  containment while disappearing from Trails/names. Close each heading no later
  than its enclosing environment; stray end of an unopened name closes nothing.
  A missing end extends to EOF. Do not read input/include paths. Run depth,
  CRLF and source-label exclusion through the common tests and C11 fuzz.
- [ ] **Step 4:** rerun the exact command with `INSTA_UPDATE=always` for new
  snapshots; inspect the tree, flat and JSON snapshots against the assertions.
  Rerun with `INSTA_UPDATE=no`: selected tests PASS, stderr empty.
  All tests run on Linux and all four native CI systems; Windows GNU all-targets clippy also applies.
- [ ] **Step 5:** stage exactly the files above, the named fixtures and this
  topic's snapshots; commit with
  `git commit -m "feat: add LaTeX title and environment Trails"`.

## Chunk 6: JSON views

Owns `src/processors/keypath/views/{mod,jupyter,har,telegram}.rs`,
keypath dispatch/JSON event exposure in tanda 2, three Definitions,
`tests/cli_m6_views.rs` and fixtures. Copy the CLI prelude.
No generic serde_json DOM or user mapping language. Reuse the existing scanner's
error boundary, decoded-string pieces, compact end census and native cursor.

### Task 6.1: Jupyter cells, Markdown heading ranges and outputs

**Files:** create `keypath/views/mod.rs::{View,ViewPrepared,ViewCursor}`,
`views/jupyter.rs::project`; modify `keypath/{mod,json}.rs` view dispatch/event
exposure; create `formats/jupyter.toml`; create `tests/cli_m6_views.rs::m6_jupyter`;
create `tests/fixtures/docs-a/m6.ipynb` and broken sibling.

New internal projection API, used by all three views:

```rust
pub(crate) enum View { Jupyter, Har, Telegram }
pub(crate) fn prepare<'a>(input: crate::model::Input<'a>, params: &'a ::toml::Table,
    view: View) -> anyhow::Result<Box<dyn crate::processors::Prepared + 'a>>;
```
Definition `[params] syntax='json'; view='jupyter'; name_chars=40`.
Kinds added by this view: `heading`, `cell`, `output`; retained keys keep `key`.
Headings carry level (`Some(n)` with `n: usize`, Task 1.4) and source signature; cell/output labels use signatures
`cell N (code|markdown|raw)` and `output N (OUTPUT_TYPE)`.

- [ ] **Step 1 — RED:** add these complete tests and their literal primary fixture bytes.

```rust
#[test]
fn m6_jupyter() {
    let fx=Fixture::new();
    // `###` delimiters: the sources hold `"#` and `"##`, which end shorter ones.
    let src=br###"{
 "cells": [
  {"cell_type":"markdown","source":["# Analysis\n","## Load data\n"]},
  {"cell_type":"code","source":["df = load('needle.csv')\n"],
   "outputs":[{"output_type":"execute_result","data":{"text/plain":["needle"]}}]},
  {"cell_type":"raw","source":["needle raw"]}
 ],
 "nbformat":4
}
"###;
    seed(&fx,"docs-a/m6.ipynb",src);
    check(&fx,"jupyter","docs-a/m6.ipynb",&[
        "Analysis › Load data › cell 2",
        "Analysis › Load data › cell 2 › output 1 › text/plain",
        "Analysis › Load data › cell 3"]);
    flags(&fx,"jupyter","docs-a/m6.ipynb","cell 2","Analysis","^cell [0-9]+$");
    put(&fx,"not-notebook",br#"{"source":{"k":"needle"}}"#);
    assert_eq!(paths(&matches(&output(&fx,"jupyter","not-notebook","needle",&["--json"]))),["source › k"]);
    put(&fx,"bad-notebook",b"{\"cells\":[{\"cell_type\":\"code\",\"source\":[\"needle\"]}],\"bad\":nope}\nneedle\n");
    let rows=matches(&output(&fx,"jupyter","bad-notebook","needle",&["--json"]));
    assert_eq!(paths(&rows),["cell 1",""]);
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test cli_m6_views m6_jupyter -- --nocapture`.
  Expected RED: jupyter unknown or raw cells/source indexes remain in Trails.
- [ ] **Step 3:** expose decoded JSON tokens plus original ranges without changing existing
  json::walk/prepare callers. Determine root cells shape during preparation and
  fall back to unprojected JSON if absent/non-array. Recognize Markdown heading
  lines at decoded source-line starts, including source stored as a string or
  list of strings. End heading Sections before the next same/higher heading,
  including across cells. Hide cells/source/outputs/data/text wrapper keys and
  source indexes only in this view; retain metadata, arbitrary keys and output
  MIME keys. Cells are one-based, outputs one-based per cell. Heading label
  ownership overrides enclosing cell display at the name span to preserve parent
  Trails. Prepare all late shape/name ingredients once; retain decoded-source
  pieces for name search without searching the notebook as extracted text.
- [ ] **Step 4:** rerun the exact command with `INSTA_UPDATE=always` for new
  snapshots; inspect the tree, flat and JSON snapshots against the assertions.
  Rerun with `INSTA_UPDATE=no`: selected tests PASS, stderr empty.
  All tests run on Linux and all four native CI systems; Windows GNU all-targets clippy also applies.
- [ ] **Step 5:** stage exactly the files above, the named fixtures and this
  topic's snapshots; commit with
  `git commit -m "feat: project Jupyter JSON into notebook Trails"`.

### Task 6.2: HAR entries and named request metadata

**Files:** create `views/har.rs::project`, `formats/har.toml`; append `m6_har`;
create `tests/fixtures/code-c/m6.har` and broken sibling.

`syntax='json'; view='har'; name_chars=60`.
New kind `entry`, existing `key`/`element` elsewhere. Entry signature carries
method, full URL and response status; visible name removes the URL scheme.
Named header/cookie/queryString/postData.params elements replace index labels.

- [ ] **Step 1 — RED:** add these complete tests and their literal primary fixture bytes.

```rust
#[test]
fn m6_har() {
    let fx=Fixture::new();
    let src=br#"{"log":{
 "entries":[{
  "response":{"content":{"text":"needle"},"status":200},
  "request":{"method":"GET","url":"https://api.example.com/v1/users",
   "headers":[{"name":"Authorization","value":"needle"}],
   "cookies":[{"name":"session","value":"needle"}],
   "queryString":[{"name":"q","value":"needle"}],
   "postData":{"params":[{"name":"body","value":"needle"}]}}
 }],"pages":[{"title":"Login","id":"needle"}]}}
"#;
    seed(&fx,"code-c/m6.har",src);
    check(&fx,"har","code-c/m6.har",&[
        "GET api.example.com/v1/users › response › content › text",
        "GET api.example.com/v1/users › request › header Authorization › value",
        "GET api.example.com/v1/users › request › cookie session › value",
        "GET api.example.com/v1/users › request › queryString q › value",
        "GET api.example.com/v1/users › request › postData › param body › value",
        "Login › id"]);
    flags(&fx,"har","code-c/m6.har","response","Authorization","^header ");
    put(&fx,"bad-har",b"{\"log\":{\"entries\":[{\"request\":{\"method\":\"GET\",\"url\":\"http://x\"},\"x\":\"needle\"}]}}\nnope\nneedle\n");
    assert_eq!(paths(&matches(&output(&fx,"har","bad-har","needle",&["--json"]))),
        ["GET x › x",""]);
    let rows=matches(&output(&fx,"har","code-c/m6.har","needle",&["--json"]));
    assert!(rows[0]["trail"][0]["signature"].as_str().unwrap().contains("https://api.example.com/v1/users"));
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test cli_m6_views m6_har -- --nocapture`.
  Expected RED: HAR unknown, late URL unavailable or arrays retain indexes.
- [ ] **Step 3:** hide log, replace each entries element with a named entry, and build
  title-based pages labels. Read request method/URL and response status even
  when physically after the Match. Use prefixes header/cookie/queryString/param
  as the visible singular labels fixed by these fixtures; those prefixes are
  generated, while the corresponding name field is searchable. Keep unrelated
  keys unchanged. Missing optional HAR fields use their existing JSON element
  label rather than inventing values; a non-HAR root keeps plain JSON Trails.
  Limit visible HAR names to 60 characters; retain full URL/status in signature
  and safe visible escaping. Test bounded preparation with C11's dense HAR.
- [ ] **Step 4:** rerun the exact command with `INSTA_UPDATE=always` for new
  snapshots; inspect the tree, flat and JSON snapshots against the assertions.
  Rerun with `INSTA_UPDATE=no`: selected tests PASS, stderr empty.
  All tests run on Linux and all four native CI systems; Windows GNU all-targets clippy also applies.
- [ ] **Step 5:** stage exactly the files above, the named fixtures and this
  topic's snapshots; commit with
  `git commit -m "feat: project HAR entries and request metadata names"`.

### Task 6.3: Telegram single-chat and account JSON views

**Files:** create `views/telegram.rs::project`, `formats/telegram.toml`;
append `m6_telegram_json`; create
`tests/fixtures/chat/ChatExport_m6/result.json` and broken sibling.

`syntax='json'; view='telegram'; name_chars=40`; paths in Task 1.3.
Kinds `chat`, `message`; text/text_entities keys stay, their array indexes hide.
The fixture's `ChatExport_m6/result.json` path is detected as `telegram` by
spec §4.1 signal 4 (C1.3), so the prelude's automatic variant runs on it.

- [ ] **Step 1 — RED:** add these complete tests and their literal primary fixture bytes.

```rust
#[test]
fn m6_telegram_json() {
    let fx=Fixture::new();
    let src=br#"{
 "name":"Ana",
 "messages":[
  {"text":"needle","date":"2024-01-31T10:04:12","from":"Ana"},
  {"date":"2024-01-31T10:05:00","from":"Ana",
   "text":["needle",{"type":"bold","text":"needle"}],
   "text_entities":[{"type":"plain","text":"needle"}]}
 ]
}
"#;
    seed(&fx,"chat/ChatExport_m6/result.json",src);
    // SEARCHFOR_M6_AUTO=1 also checks detection by the ChatExport_* path.
    check(&fx,"telegram","chat/ChatExport_m6/result.json",&[
        "Ana › 2024-01-31T10:04:12 Ana › text",
        "Ana › 2024-01-31T10:05:00 Ana › text",
        "Ana › 2024-01-31T10:05:00 Ana › text_entities › text"]);
    flags(&fx,"telegram","chat/ChatExport_m6/result.json","Ana","Ana","");
    put(&fx,"account",br#"{"chats":{"list":[{"name":"Work","messages":[{"text":"needle","from":"Ana","date":"2024"}]}]}}"#);
    assert_eq!(paths(&matches(&output(&fx,"telegram","account","needle",&["--json"]))),
        ["Work › 2024 Ana › text"]);
    put(&fx,"bad-telegram",b"{\"name\":\"Ana\",\"messages\":[{\"date\":\"2024\",\"from\":\"Ana\",\"text\":\"needle\"}]}\nnope\nneedle\n");
    assert_eq!(paths(&matches(&output(&fx,"telegram","bad-telegram","needle",&["--json"]))),
        ["Ana › 2024 Ana › text",""]);
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test cli_m6_views m6_telegram_json -- --nocapture`.
  Expected RED: Telegram shape unrecognized or late sender/date missing.
- [ ] **Step 3:** recognize chat roots and account chats.list, hiding account wrappers in
  projected Trails. Message labels combine date as written and from; absent
  sender/date omits the missing ingredient, with no new source text. Hide indexes
  of text/text_entities arrays only, retaining arbitrary entity-object keys.
  Multiple submatches on one physical line retain per-submatch Trails even when
  the line's Trail is just that of its first submatch. Project malformed JSON only
  before its scanner's error boundary; fallback shape keeps JSON semantics.
  Keep all late-name lookup in preparation and source-aware label exclusions.
  The automatic variant needs C1 integrated; before that, run forced mode only.
- [ ] **Step 4:** rerun the exact command with `INSTA_UPDATE=always` for new
  snapshots; inspect the tree, flat and JSON snapshots against the assertions.
  Rerun with `INSTA_UPDATE=no`: selected tests PASS, stderr empty.
  All tests run on Linux and all four native CI systems; Windows GNU all-targets clippy also applies.
- [ ] **Step 5:** stage exactly the files above, the named fixtures and this
  topic's snapshots; commit with
  `git commit -m "feat: project Telegram JSON chats and message labels"`.

## Chunk 7: markup for components

Owns `markup/{mod,scan,components,templates,telegram}.rs`, required treesitter
range composition, four new TOMLs, `tests/cli_m6_markup.rs`, its fixtures and
snapshots. Copy the CLI prelude. C7 also owns per-run configured-markup plumbing
in `processors/mod.rs` and `src/app.rs` in tanda 2. New module/by_name lines
from C5/C8/C9 remain shared registration conflicts. C7 also owns
`formats/html.toml`: Task 7.4 turns HTML's scripts on with spec §8.3's three
limits (Open question 2 answered B, D24).

### Task 7.1: Vue script composition with source coordinates

**Files:** create `markup/components.rs::{ScriptDefinitions,ComponentMarkup}`;
modify `markup/mod.rs::{Params,params,Markup::prepare,Markup::trails,Markup::names}`,
`treesitter/mod.rs::outline` via a new offset adapter, `processors/mod.rs`
(`ProcessorRef`, configured lookup), `src/app.rs` Processor lookups;
create `formats/vue.toml`, `tests/cli_m6_markup.rs::m6_vue`;
create `tests/fixtures/code-c/m6.vue` and broken sibling.

New interfaces (retain existing public functions too):

```rust
// markup/components.rs
pub struct ScriptDefinitions { pub javascript: toml::Table,
    pub typescript: toml::Table, pub tsx: toml::Table }
pub struct ComponentMarkup { /* owns ScriptDefinitions, per-run query cache */ }
impl ComponentMarkup {
    pub fn new(definitions: ScriptDefinitions) -> Self;
}
// processors/mod.rs
// Add ProcessorRef::Configured(Arc<dyn Processor>) and its Deref arm.
pub fn for_definition_in<'a>(&'a self,
    def: &crate::formats::definition::FormatDefinition,
    registry: &crate::formats::Registry) -> Option<ProcessorRef<'a>>;
// treesitter/mod.rs; a new function, not a changed outline signature.
pub(crate) fn shifted_outline(c: &Compiled, tree: &tree_sitter::Tree,
    script: &[u8], source: &[u8], base: usize) -> crate::trail::Outline;
```
`for_definition_in` is a method on Processors. It returns a cached configured
markup Processor only for scripts=true and delegates every other lookup.
App call sites in `heading_file`, `describe` and name search use it once their
Registry is available. `complete_input::describes` remains a capability query.
Rework `Ctx::outliner` and `Ctx::outliners` too: cache an enum containing either
a static built-in reference or `Arc<dyn Processor>`, initialized in the existing
OnceLock after Registry loading. `outliner` returns a borrow from that immutable
cache, so native streaming retains configured markup for the whole run without
borrowing a temporary ProcessorRef or a Mutex guard. All configured processors
are Send+Sync; construction/query caching happens once per Definition.
Do not force Registry loading before a Match/document candidate.
Vue: `syntax='html'; scripts=true; name_chars=40`, kinds unchanged inside scripts.

- [ ] **Step 1 — RED:** add these complete tests and their literal primary fixture bytes.

```rust
#[test]
fn m6_vue() {
    let fx=Fixture::new();
    let src=b"<template>\n<ul class=\"items\"><li>x</li><li>needle template</li></ul>\n</template>\n<script lang=\"ts\">\nfunction addItem() {\n  const value: string = 'needle';\n}\n</script>\n<style>.needle { color:red }</style>\n";
    seed(&fx,"code-c/m6.vue",src);
    check(&fx,"vue","code-c/m6.vue",&[
        "template › ul.items › li[2]","script › addItem","style"]);
    flags(&fx,"vue","code-c/m6.vue","addItem","addItem","\\[2\\]");
    let expanded=matches(&output(&fx,"vue","code-c/m6.vue","needle",&["--json","--within","addItem","--expand"]));
    assert_eq!(expanded[0]["section"]["start_line"],5);
    assert_eq!(expanded[0]["section"]["end_line"],7);
    // HTML's scripts are still off here; Task 7.4 turns them on and changes this
    // expectation to `script › addItem` (the `lang="ts"` picks TypeScript there too).
    put(&fx,"same.html",src);
    assert_eq!(paths(&matches(&output(&fx,"html","same.html","needle",&["--json"]))),
        ["template › ul.items › li[2]","script","style"]);
    put(&fx,"unknown.vue",b"<script lang='coffee'>\nneedle\n</script>\n<script>function f(){\nneedle\n}</script>\n");
    assert_eq!(paths(&matches(&output(&fx,"vue","unknown.vue","needle",&["--json"]))),
        ["script[1]","script[2] › f"]);
    // A user copy of the TypeScript Definition, to which the test adds `max_bytes`
    // (the built-in has none), applies to this script range alone. toml 1.1.8's
    // `IndexMut` panics on a missing key, so the key is inserted.
    let mut definition: toml::Table=toml::from_str(include_str!("../formats/typescript.toml")).unwrap();
    definition.get_mut("params").unwrap().as_table_mut().unwrap()
        .insert("max_bytes".into(),toml::Value::Integer(1));
    std::fs::write(fx.formats.path().join("typescript.toml"),toml::to_string(&definition).unwrap()).unwrap();
    let capped=output(&fx,"vue","code-c/m6.vue","needle",&["--json"]);
    assert_eq!(paths(&matches(&capped)),["template › ul.items › li[2]","script","style"]);
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test cli_m6_markup m6_vue -- --nocapture`.
  Expected RED: scripts remain raw, line translation wrong or configured max_bytes ignored.
- [ ] **Step 3:** retain markup census and streaming behavior; script content ranges use
  HTML raw-text end rules. Select JS, TS/TypeScript or TSX by lang (default/js
  JavaScript); absent compiled grammar or excess max_bytes silently keeps only
  the enclosing script Segment. Compile the active Format Definition's query
  once per run, parse only the script slice, then translate Section ranges,
  label ranges, Name pieces and anchors by base. Recompute line/end_line against
  the full Source. Bound script Segments to content so they never reach an HTML
  sibling. Do not use TreeSitter::SizeLimit's warning path for expected oversized
  components. `Markup`'s stateless entry uses built-in script Definitions for
  direct Processor tests; the app's configured instance uses Registry overrides.
  Native prepare caches script trees only once; cursor merges their Trails
  below script. Add `open_from` and `open_from_with` explicitly to the existing
  MarkupCursor (which currently implements neither), using census ranges and
  the outer markup/template scope. Test both default HTML and component cursors. Leave
  `html.toml` alone in this task (Task 7.4 owns its scripts), and rerun every existing HTML
  snapshot.
- [ ] **Step 4:** rerun the exact command with `INSTA_UPDATE=always` for new
  snapshots; inspect the tree, flat and JSON snapshots against the assertions.
  Rerun with `INSTA_UPDATE=no`: selected tests PASS, stderr empty.
  All tests run on Linux and all four native CI systems; Windows GNU all-targets clippy also applies.
- [ ] **Step 5:** stage exactly the files above, the named fixtures and this
  topic's snapshots; commit with
  `git commit -m "feat: compose Vue script Trails with parent Source coordinates"`.

### Task 7.2: Svelte blocks and Blade paired directives

**Files:** create `markup/templates.rs::{TemplateSyntax,scan}`; modify markup
scanner/census to merge template events; create `formats/svelte.toml`,
`formats/blade.toml`; append `m6_templates`; create
`tests/fixtures/code-c/m6.svelte`, `m6.blade.php` and broken siblings.

New parameters: `templates='svelte'|'blade'` (absent means off).
Svelte sets `scripts=true`; Blade leaves scripts off. Both use `syntax='html'`.
New Segment kind `block`. Template events carry opening/closing byte ranges and
name pieces, sharing the HTML stack's depth/closure rules.

- [ ] **Step 1 — RED:** add these complete tests and their literal primary fixture bytes.

```rust
#[test]
fn m6_templates() {
    let fx=Fixture::new();
    let svelte=b"<ul>\n{#each items as item}\n<li>{item.needle}</li>\n{:else}\n<li>needle empty</li>\n{/each}\n</ul>\n";
    seed(&fx,"code-c/m6.svelte",svelte);
    check(&fx,"svelte","code-c/m6.svelte",&[
        "ul › #each items as item › li[1]","ul › #each items as item › li[2]"]);
    flags(&fx,"svelte","code-c/m6.svelte","items","items","");
    let blade=b"@section('content')\n<div class=\"cart\">\n@foreach($items as $item)\n<li>{{ $item->needle }}</li>\n@endforeach\n</div>\n@endsection\n";
    seed(&fx,"code-c/m6.blade.php",blade);
    check(&fx,"blade","code-c/m6.blade.php",&[
        "@section('content') › div.cart › @foreach($items as $item) › li"]);
    flags(&fx,"blade","code-c/m6.blade.php","items","items","");
    put(&fx,"blade-inline",b"@section('x','needle')\n{{-- @if(fake) needle --}}\n@verbatim\n@if(fake) needle\n@endverbatim\nneedle\n");
    assert_eq!(paths(&matches(&output(&fx,"blade","blade-inline","needle",&["--json"]))),
        ["","","@verbatim",""]);
    put(&fx,"svelte-close",b"{#if ready}\n<div>needle\n{/if}\nneedle\n{/missing}\n");
    assert_eq!(paths(&matches(&output(&fx,"svelte","svelte-close","needle",&["--json"]))),
        ["#if ready › div",""]);
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test cli_m6_markup m6_templates -- --nocapture`.
  Expected RED: new parameters ignored or HTML scopes survive a template close.
- [ ] **Step 3:** merge template open/close events with HTML elements in one containment
  stack. Svelte supports if/each/await/key/snippet; else/then branch markers stay
  inside, and a matching closer closes its innermost block plus intervening
  elements. Blade handles every paired directive listed in §8.5 and special
  section closers show/stop/overwrite. Distinguish inline two-argument section
  from a paired section with balanced argument scanning. Comments and verbatim
  suppress directive parsing; PHP bodies remain text. Ignore unmatched closers.
  Record accurate opening-name pieces; a Match in a block name is outside it.
  Do not enable template parsing inside script/style or Blade comments.
- [ ] **Step 4:** rerun the exact command with `INSTA_UPDATE=always` for new
  snapshots; inspect the tree, flat and JSON snapshots against the assertions.
  Rerun with `INSTA_UPDATE=no`: selected tests PASS, stderr empty.
  All tests run on Linux and all four native CI systems; Windows GNU all-targets clippy also applies.
- [ ] **Step 5:** stage exactly the files above, the named fixtures and this
  topic's snapshots; commit with
  `git commit -m "feat: describe Svelte and Blade template block Trails"`.

### Task 7.3: Telegram HTML view and joined sender inheritance

**Files:** create `markup/telegram.rs::project`, `formats/telegram-html.toml`;
append `m6_telegram_html`; create
`tests/fixtures/chat/ChatExport_m6/messages.html` and broken sibling.

New `view='telegram'` on `syntax='html'`; `scripts` stays false.
New kind `message`. This view detects Telegram class structure during
preparation and otherwise retains ordinary HTML Trails. No Extractor is involved.

- [ ] **Step 1 — RED:** add these complete tests and their literal primary fixture bytes.

```rust
#[test]
fn m6_telegram_html() {
    let fx=Fixture::new();
    let src=b"<html><body><div class='page_wrap'><div class='history'>\n<div class='message default' id='message1'>\n<div class='pull_right date details' title='31.01.2024 10:04:12'>time</div>\n<div class='from_name'>Ana</div>\n<div class='text'>needle</div>\n</div>\n<div class='message default joined' id='message2'>\n<div class='date details' title='31.01.2024 10:05:00'>time</div>\n<div class='text'>needle</div>\n</div>\n</div></div></body></html>\n";
    seed(&fx,"chat/ChatExport_m6/messages.html",src);
    check(&fx,"telegram-html","chat/ChatExport_m6/messages.html",&[
        "31.01.2024 10:04:12 Ana","31.01.2024 10:05:00 Ana"]);
    flags(&fx,"telegram-html","chat/ChatExport_m6/messages.html","Ana","Ana","");
    put(&fx,"generic.html",b"<main><p>needle</p></main>\n");
    assert_eq!(paths(&matches(&output(&fx,"telegram-html","generic.html","needle",&["--json"]))),
        ["main › p"]);
    put(&fx,"bad-telegram.html",b"<div class='message default'><div class='date' title='2024'>x</div><div class='from_name'>Ana</div><div class='text'>needle\n");
    assert_eq!(paths(&matches(&output(&fx,"telegram-html","bad-telegram.html","needle",&["--json"]))),["2024 Ana"]);
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test cli_m6_markup m6_telegram_html -- --nocapture`.
  Expected RED: Telegram HTML remains generic div paths or joined sender disappears.
- [ ] **Step 3:** recognize the export's class tokens independent of attribute order or
  quote style. Read date title and sender text (entities decoded with source
  pieces), inherit sender only for joined messages, hide template wrappers and
  message-internal layout tags, retaining one semantic message Section. Late
  date/sender elements are preparation ingredients, never per-Match rescans.
  Ordinary HTML fallback applies when template classes are absent; incomplete
  HTML follows v1 permissive EOF behavior. With C1 integrated, the automatic
  variant detects `ChatExport_m6/messages.html` by signal 4; document template
  dependence in C12.
- [ ] **Step 4:** rerun the exact command with `INSTA_UPDATE=always` for new
  snapshots; inspect the tree, flat and JSON snapshots against the assertions.
  Rerun with `INSTA_UPDATE=no`: selected tests PASS, stderr empty.
  All tests run on Linux and all four native CI systems; Windows GNU all-targets clippy also applies.
- [ ] **Step 5:** stage exactly the files above, the named fixtures and this
  topic's snapshots; commit with
  `git commit -m "feat: project Telegram HTML messages and inherited senders"`.

### Task 7.4: HTML scripts with the three limits (Open question 2 = B)

**Files:** modify `markup/components.rs` (`ScriptKind` and the caps),
`markup/mod.rs::{Params,params}`, `formats/html.toml`; append `m6_html_scripts`
to `tests/cli_m6_markup.rs`; create `tests/fixtures/code-c/m6-scripts.html`
and its broken sibling. Spec §8.3 «Which scripts are read» and «HTML's output
changes» (Sebastián, Hilo 855 #12726, D24).

```rust
// markup/components.rs: what a <script>'s attributes make of its content.
pub(crate) enum ScriptKind { Code(Lang), JsonLd, Text }
pub(crate) fn script_kind(type_attr: Option<&[u8]>, lang: Option<&[u8]>) -> ScriptKind;
```

`formats/html.toml` gains, under `[params]`, `scripts = true`,
`script_max_bytes = 1048576` and `script_max_line = 1000`, each with a one-line
comment giving spec §8.3's reason. Vue and Svelte set neither cap.

- [ ] **Step 1 — RED:** append this complete test and its literal primary fixture bytes.

```rust
#[test]
fn m6_html_scripts() {
    let fx=Fixture::new();
    let src=b"<html><head>\n<script>\nfunction onClick() {\n  return 'needle';\n}\n</script>\n<script type=\"module\">\nexport function load() { return 'needle'; }\n</script>\n<script type=\"application/ld+json\">\n{\"@type\":\"Product\",\"offers\":{\"price\":\"needle\"}}\n</script>\n<script type=\"text/x-template\">\nfunction tpl() { needle }\n</script>\n</head></html>\n";
    seed(&fx,"code-c/m6-scripts.html",src);
    check(&fx,"html","code-c/m6-scripts.html",&[
        "head › script[1] › onClick","head › script[2] › load",
        "head › script[3] › offers › price","head › script[4]"]);
    flags(&fx,"html","code-c/m6-scripts.html","onClick","onClick","");
    let one=|file:&str,src:&[u8]|{put(&fx,file,src);paths(&matches(&output(&fx,"html",file,"needle",&["--json"])))};
    // A JavaScript MIME type, trimmed and without case; any other type is text.
    assert_eq!(one("case.html",b"<script type=\" Text/JavaScript \">function f(){ 'needle' }</script>\n"),["script › f"]);
    assert_eq!(one("plain.html",b"<script type=\"text/plain\">function f(){ 'needle' }</script>\n"),["script"]);
    // script_max_line = 1000 bytes: a 1000-byte line is read, a 1001-byte one is minified.
    let line=|n:usize|format!("<script>\nfunction edge() {{ 'needle'; }}\nvar x=\"{}\";\n</script>\n","a".repeat(n-9));
    assert_eq!(one("edge.html",line(1000).as_bytes()),["script › edge"]);
    assert_eq!(one("min.html",line(1001).as_bytes()),["script"]);
    // script_max_bytes = 1 MiB: past it, the element's Trail.
    let big=format!("<script>\nfunction big() {{ 'needle'; }}\n{}</script>\n","// pad\n".repeat((1<<20)/7+1));
    assert_eq!(one("big.html",big.as_bytes()),["script"]);
    // v1.0's HTML snapshots stay as they are: page.html's script declares no function.
    let page=output(&fx,"html","markup/page.html","needle",&["--json"]);
    assert!(paths(&matches(&page)).iter().any(|p|p=="head › script"));
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test cli_m6_markup m6_html_scripts -- --nocapture`.
  Expected RED: HTML scripts unread (`head › script[1]` alone), JSON-LD without
  key Trails, or a cap not applied.
- [ ] **Step 3:** classify each `<script>` from its start tag once, in the
  census: `type` absent, empty, `module` or one of spec §8.3's JavaScript MIME
  types (trimmed, ASCII case-insensitive) is `Code`, with `lang` picking TS/TSX
  as in Task 7.1; `application/ld+json` is `JsonLd`; anything else is `Text`.
  Read the caps from params (absent means none). Parse a `Code` or `JsonLd`
  script lazily, the first time a Match or the name search lands in it, after
  checking its content length against `script_max_bytes` and its longest line
  (one `memchr` pass for `\n`, a CR before it excluded) against
  `script_max_line`; a script past either is `Text` for the run. `JsonLd` uses
  keypath's JSON outline over the script slice, translated by base like the
  Grammar outlines (D10): ranges, labels, Name pieces, anchors. No change to
  keypath's code; C7 calls its existing outline entry. `Text` keeps the
  `script` Segment alone, as in 1.0. The native cursor and `open_from` of Task
  7.1 cover HTML too. Then rerun every v1 HTML snapshot with
  `INSTA_UPDATE=no` (`cargo test --offline --locked --test cli_m3`) and the
  markup unit tests: none may change. Update `m6_vue`'s `same.html` expectation
  to `["template › ul.items › li[2]","script › addItem","style"]`, the one
  existing M6 assertion this task changes.
  Then run v1's fuzz, whose `html` target now reaches the scripts:
  `tests/fuzz.rs` builds that target from `formats/html.toml`'s `[params]`
  (`fuzz.rs::params`, the `html` row of `targets`) and its `MARKUP` tokens and
  `markup_doc` tags include `<script>` and `</script>`, so with `scripts = true`
  random markup gets its scripts parsed as JavaScript:
  `SEARCHFOR_FUZZ_N=8 CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test fuzz -- --nocapture`.
  Both generated-input tests (`generated_inputs_do_not_panic_the_processors`
  and `generated_files_do_not_panic_sfor`) must PASS with no panic and no case
  past the 2 s deadline; a saved case is fixed in `markup/components.rs`, never
  by narrowing the tokens (plan review r2, Fable #3).
- [ ] **Step 4:** rerun the exact command with `INSTA_UPDATE=always` for new
  snapshots; inspect the tree, flat and JSON snapshots against the assertions.
  Rerun with `INSTA_UPDATE=no`: selected tests PASS, stderr empty. Rerun the
  `SEARCHFOR_FUZZ_N=8 … --test fuzz` command of Step 3: PASS.
  All tests run on Linux and all four native CI systems; Windows GNU all-targets clippy also applies.
- [ ] **Step 5:** stage exactly the files above, the named fixtures and this
  topic's snapshots; commit with
  `git commit -m "feat: give HTML scripts their JavaScript and JSON-LD Trails within three limits"`.

## Chunk 8: sql and diff

Owns `src/processors/sql/{mod,lex,units}.rs`, `src/processors/diff.rs`,
`formats/{sql,diff}.toml`, `tests/cli_m6_sql_diff.rs` and fixtures/snapshots.
Copy the CLI prelude. Register `Sql` and `Diff`. Neither processor evaluates
source content or invokes an external program.

### Task 8.1: SQL statement scanner and CREATE TABLE columns

**Files:** create `sql/mod.rs::{Sql,SqlPrepared,SqlCursor}`, `sql/lex.rs` lexer,
`sql/units.rs` incremental statement index; create `formats/sql.toml`;
create `tests/cli_m6_sql_diff.rs::m6_sql_statements`;
create `tests/fixtures/code-c/m6-schema.sql` and broken sibling.

`pub struct Sql;` implements Processor, native capability and preparation.
Definition: `processor='sql'`, `name_chars=40`; no dialect parameter.
Kinds `statement`, `column`, later `row`. Private lazy-unit boundary:

```rust
// sql/units.rs: scanner state advances monotonically on demand.
fn next_statement(&mut self) -> Option<std::ops::Range<usize>>;
```
Statement names preserve leading keyword spelling and decoded identifiers.
Fixtures fix uppercase spelling for deterministic expected names.

- [ ] **Step 1 — RED:** add these complete tests and their literal primary fixture bytes.

```rust
#[test]
fn m6_sql_statements() {
    let fx=Fixture::new();
    let src=b"CREATE TABLE users (\n id integer,\n email text NOT NULL, -- needle\n CONSTRAINT pk PRIMARY KEY(id)\n);\nCREATE OR REPLACE VIEW active_users AS SELECT 'needle' AS label;\n";
    seed(&fx,"code-c/m6-schema.sql",src);
    check(&fx,"sql","code-c/m6-schema.sql",&[
        "CREATE TABLE users › email","CREATE OR REPLACE VIEW active_users"]);
    flags(&fx,"sql","code-c/m6-schema.sql","email","email","");
    put(&fx,"sql-quotes",br#"SELECT '; needle', "semi;name", `back;tick`, [brack;et];
GO
DELIMITER $$
CREATE PROCEDURE p() BEGIN SELECT 'needle;'; END$$
DELIMITER ;
SELECT $tag$needle ;$tag$;
"#);
    assert_eq!(paths(&matches(&output(&fx,"sql","sql-quotes","needle",&["--json"]))),
        ["SELECT","CREATE PROCEDURE p","SELECT"]);
    put(&fx,"bad.sql",b"SELECT 'needle';\nSELECT 'unclosed\nneedle\n");
    assert_eq!(paths(&matches(&output(&fx,"sql","bad.sql","needle",&["--json"]))),["SELECT",""]);
    put(&fx,"sql-own",b"CREATE TABLE needle (id int);\n");
    assert_eq!(paths(&matches(&output(&fx,"sql","sql-own","needle",&["--json"]))),[""]);
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test cli_m6_sql_diff m6_sql_statements -- --nocapture`.
  Expected RED: SQL unknown or semicolons in literals split statements.
- [ ] **Step 3:** recognize statements ending at semicolon, standalone GO, or the current
  DELIMITER outside lexed strings/identifiers/comments. Include quoted doubled
  delimiters, backticks, brackets, tagged dollar quotes, --/#/block comments.
  Statement names cover the §8.1 leading-keyword/object forms, with identifier
  quotes removed through source pieces. Build a bounded schema table for earlier
  CREATE TABLE column names only as scanning reaches it; constraints are text of
  the statement. An unclosed string/comment invalidates structure starting at
  its statement, preserving earlier statements. Preparation is lazy: discover
  boundaries through the unit requested by the cursor, cache compact ranges and
  current-unit details, and stop after that unit. Never scan a 1 GiB unmatched
  tail to build a complete eager statement index. names() may read all units
  because --in-headings explicitly searches names throughout the file.
- [ ] **Step 4:** rerun the exact command with `INSTA_UPDATE=always` for new
  snapshots; inspect the tree, flat and JSON snapshots against the assertions.
  Rerun with `INSTA_UPDATE=no`: selected tests PASS, stderr empty.
  All tests run on Linux and all four native CI systems; Windows GNU all-targets clippy also applies.
- [ ] **Step 5:** stage exactly the files above, the named fixtures and this
  topic's snapshots; commit with
  `git commit -m "feat: add native SQL statement and schema column Trails"`.

### Task 8.2: INSERT tuples and COPY rows without a dump-sized Outline

**Files:** modify `sql/{mod,units}.rs` row/column lookup; append `m6_sql_rows`;
create `tests/fixtures/code-c/m6-dump.sql` and broken sibling.

No new public methods. `row` names are `row N` (one-based per statement);
`column` names come from explicit INSERT/COPY columns, else preceding same-table
CREATE schema, else `column K` (one-based). Generated names have no Name pieces.

- [ ] **Step 1 — RED:** add these complete tests and their literal primary fixture bytes.

```rust
#[test]
fn m6_sql_rows() {
    let fx=Fixture::new();
    let src=b"CREATE TABLE users (id int,email text);\nINSERT INTO users VALUES (1,'plain'),(2,'needle@example.com');\nINSERT INTO users (email,id) VALUES ('needle',3);\nCOPY public.users (id,email) FROM stdin;\n1\tplain\n2\tneedle@example.com\n\\.\nINSERT INTO unknown VALUES (1,'needle');\n";
    seed(&fx,"code-c/m6-dump.sql",src);
    check(&fx,"sql","code-c/m6-dump.sql",&[
        "INSERT INTO users › row 2 › email","INSERT INTO users › row 1 › email",
        "COPY public.users › row 2 › email","INSERT INTO unknown › row 1 › column 2"]);
    flags(&fx,"sql","code-c/m6-dump.sql","email","email","^row [0-9]+$");
    put(&fx,"tuple.sql",b"INSERT INTO t (a,b) VALUES ('needle',fn(1,2)),('x','needle');\n");
    let row=matches(&output(&fx,"sql","tuple.sql","needle",&["--json"])).remove(0);
    assert_eq!(row["submatches"].as_array().unwrap().len(),2);
    assert_eq!(row["submatches"][1]["trail"][1]["name"],"row 2");
    assert_eq!(row["submatches"][1]["trail"][2]["name"],"b");
    put(&fx,"broken-copy.sql",b"COPY t (a) FROM stdin;\nneedle\n");
    assert_eq!(paths(&matches(&output(&fx,"sql","broken-copy.sql","needle",&["--json"]))),
        ["COPY t › row 1 › a"]);
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test cli_m6_sql_diff m6_sql_rows -- --nocapture`.
  Expected RED: dump values lack row/column structure or commas inside expressions split columns.
- [ ] **Step 3:** within a matching INSERT statement, count tuples and top-level values
  lexically; commas in nested expressions and strings do not split fields. Build
  row/value ranges only as queried, sharing parent statement names. COPY's data
  mode reads physical tab-separated rows through a standalone backslash-dot,
  treating escaped sequences as bytes, not decoding them for search. Unterminated
  COPY data ends at EOF. Explicit column lists win over prior CREATE schemas;
  match normalized qualified table identities without conflating unrelated
  schemas. Support several Matches on one line with distinct per-submatch Trails.
  open_from must let closed rows go while retaining the open statement if a later
  Match might end at it. C11 checks RSS and reads through first-unit-only files.
- [ ] **Step 4:** rerun the exact command with `INSTA_UPDATE=always` for new
  snapshots; inspect the tree, flat and JSON snapshots against the assertions.
  Rerun with `INSTA_UPDATE=no`: selected tests PASS, stderr empty.
  All tests run on Linux and all four native CI systems; Windows GNU all-targets clippy also applies.
- [ ] **Step 5:** stage exactly the files above, the named fixtures and this
  topic's snapshots; commit with
  `git commit -m "feat: stream SQL INSERT and COPY row column Trails"`.

### Task 8.3: Unified and combined diffs, commits and stdin

**Files:** create `src/processors/diff.rs::{Diff,DiffPrepared,DiffCursor}`,
`formats/diff.toml`; append `m6_diff`; create
`tests/fixtures/code-c/m6.patch` and broken sibling.

`pub struct Diff;` implements native Processor. Definition `processor='diff'`,
`name_chars=40`. Kinds `commit`, `file`, `hunk`. Signatures keep commit/header
lines; rename signature is `old → new`. A hunk label is not a searchable added
`+N` alone; its context is source text.

- [ ] **Step 1 — RED:** add these complete tests and their literal primary fixture bytes.

```rust
#[test]
fn m6_diff() {
    use std::io::Write;
    let fx=Fixture::new();
    let src=b"commit 3f2a9c1234567890\nAuthor: A\n\n    Fix the needle crash\n\ndiff --git a/src/app.rs b/src/app.rs\n--- a/src/app.rs\n+++ b/src/app.rs\n@@ -1,1 +120,1 @@ fn describe(\n+    let needle = 1;\n";
    seed(&fx,"code-c/m6.patch",src);
    check(&fx,"diff","code-c/m6.patch",&[
        "", "3f2a9c1 Fix the needle crash › src/app.rs › +120 fn describe("]);
    flags(&fx,"diff","code-c/m6.patch","describe","describe","");
    let mut child=fx.s4(["--no-docs","--json","--format","diff","needle"])
        .stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped()).spawn().unwrap();
    child.stdin.take().unwrap().write_all(src).unwrap();
    assert_eq!(paths(&matches(&child.wait_with_output().unwrap())),
        ["", "3f2a9c1 Fix the needle crash › src/app.rs › +120 fn describe("]);
    put(&fx,"combined.patch",b"--- a/gone\n+++ /dev/null\n@@@ -3,1 -3,1 +0,0 @@@ context\n--needle\n--- a/next\n+++ b/next\n@@ -1,99 +2,99 @@\n+needle\n");
    assert_eq!(paths(&matches(&output(&fx,"diff","combined.patch","needle",&["--json"]))),
        ["gone › -3 context","next › +2"]);
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test cli_m6_sql_diff m6_diff -- --nocapture`.
  Expected RED: diff unknown or commit/file/hunk relationships incorrect.
- [ ] **Step 3:** index commit/file/hunk delimiters once and lazily create their names.
  Commit names combine first seven hex digits and subject stripped of PATCH
  prefix; own subject/hash text is outside that commit. Decode git quoted paths
  with source mapping, remove a/ and b/ prefixes only where git adds them, use
  old path for deletions. Read rename metadata for signature. Unified old/new
  pairs and combined @@@ are accepted. Hunk count mismatches stop at the next
  header regardless of declared counts. Header own-label Matches follow the
  explicit file/hunk parent rules. Lines outside files retain commit only;
  stdin forced Format gives identical semantic Trails. No patch application.
- [ ] **Step 4:** rerun the exact command with `INSTA_UPDATE=always` for new
  snapshots; inspect the tree, flat and JSON snapshots against the assertions.
  Rerun with `INSTA_UPDATE=no`: selected tests PASS, stderr empty.
  All tests run on Linux and all four native CI systems; Windows GNU all-targets clippy also applies.
- [ ] **Step 5:** stage exactly the files above, the named fixtures and this
  topic's snapshots; commit with
  `git commit -m "feat: add commit file and hunk Trails for diff streams"`.

## Chunk 9: mail and chat

Owns `src/processors/mail/{mod,mime,names}.rs`, `src/processors/chat.rs`,
three TOMLs, `tests/cli_m6_mail_chat.rs` and unique fixtures/snapshots.
Copy the CLI prelude. Register `Mail` and `Chat`. Both use lazy top-level unit
scanners, so no preparation pass visits an unmatched gigabyte tail.

### Task 9.1: RFC 5322 headers and mbox message Sections

**Files:** create `mail/mod.rs::{Mail,MailPrepared,MailCursor}`, `mail/names.rs`
header unfolding/name decoding; create `formats/eml.toml`, `formats/mbox.toml`;
create `tests/cli_m6_mail_chat.rs::m6_mail`; create
`tests/fixtures/mail/m6.eml`, `m6.mbox` and broken siblings.

`pub struct Mail;` implements Processor, native prepare/capability.
Definitions: `processor='mail'`, `[params] syntax='eml'|'mbox'; name_chars=40`.
Kinds `message`, `header`, `part`; `.eml` omits message root, simple body is
part `body`; mbox message signature is decoded `From · Date · Subject`.
Header names are field names as written, not their values.

- [ ] **Step 1 — RED:** add these complete tests and their literal primary fixture bytes.

```rust
#[test]
fn m6_mail() {
    let fx=Fixture::new();
    let eml=b"From: ana@example.com\nDate: Wed, 31 Jan 2024 10:00:00 +0000\nSubject: Your needle invoice\nX-Long: first\n needle folded\n\nneedle body\n";
    seed(&fx,"mail/m6.eml",eml);
    check(&fx,"eml","mail/m6.eml",&["Subject","X-Long","body"]);
    flags(&fx,"eml","mail/m6.eml","body","Subject","^body$");
    let mbox=b"From ana@example.com Wed Jan 31 10:00:00 2024\nSubject: Budget\nFrom: Ana\nDate: Today\n\nneedle one\n>From escaped needle\n\nFrom bob@example.com Wed Jan 31 11:00:00 2024\nSubject: Next\n\nneedle two\n";
    seed(&fx,"mail/m6.mbox",mbox);
    check(&fx,"mbox","mail/m6.mbox",&["Budget › body","Budget › body","Next › body"]);
    flags(&fx,"mbox","mail/m6.mbox","Budget","Budget","^body$");
    put(&fx,"bad.eml",b"Subject: One\nX-A: needle\nnot a header needle\nneedle tail\n");
    assert_eq!(paths(&matches(&output(&fx,"eml","bad.eml","needle",&["--json"]))),
        ["X-A","body","body"]);
    put(&fx,"subject.mbox",b"From a x\nSubject: needle name\n\nneedle body\n");
    assert_eq!(paths(&matches(&output(&fx,"mbox","subject.mbox","needle",&["--json"]))),
        ["Subject","needle name › body"]);
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test cli_m6_mail_chat m6_mail -- --nocapture`.
  Expected RED: mail unknown or header folding/mbox delimiters wrong.
- [ ] **Step 3:** unfold header names/values through source pieces, retaining physical
  header Sections. An invalid header line starts the body even without the
  separating blank. For mbox, recognize From-space only at offset zero or after
  a blank line, never >From; index message boundaries lazily, then read only
  headers/parts of a message with Matches. Decode RFC 2047 B/Q words with a local
  bounded decoder and encoding_rs, joining adjacent encoded words as RFC 2047
  requires; invalid encoded words remain literal. Missing subject uses generated
  (no subject). Distinguish the message's own subject-name spans from its header
  child so own-name Matches have header but exclude message. Return an explicit
  open_from for message/part/header scopes; name search is the intentional full
  enumeration path. No transfer decoding of bodies.
- [ ] **Step 4:** rerun the exact command with `INSTA_UPDATE=always` for new
  snapshots; inspect the tree, flat and JSON snapshots against the assertions.
  Rerun with `INSTA_UPDATE=no`: selected tests PASS, stderr empty.
  All tests run on Linux and all four native CI systems; Windows GNU all-targets clippy also applies.
- [ ] **Step 5:** stage exactly the files above, the named fixtures and this
  topic's snapshots; commit with
  `git commit -m "feat: describe mail headers bodies and mbox messages"`.

### Task 9.2: Nested MIME parts and decoded attachment names

**Files:** create `mail/mime.rs::scan_parts`; modify mail preparation/name decoding;
append `m6_mime`; create `tests/fixtures/mail/m6-mime.eml` and broken sibling.

Part names use IMAP one-based numbering (`part 1.2 text/plain`) or
`attachment NAME`. Nested part/header kinds are unchanged. The outer multipart
container has no generated extra part root; its children begin at part 1.
RFC 2231 continuations feed decoded attachment-name source pieces.

- [ ] **Step 1 — RED:** add these complete tests and their literal primary fixture bytes.

```rust
#[test]
fn m6_mime() {
    let fx=Fixture::new();
    let src=b"Subject: =?UTF-8?Q?Budget_=C3=A9?=\nContent-Type: multipart/mixed; boundary=OUT\n\n--OUT\nContent-Type: multipart/alternative; boundary=IN\n\n--IN\nContent-Type: text/plain\n\nneedle plain\n--IN--\n--OUT\nContent-Type: text/plain\nContent-Disposition: attachment; filename*=UTF-8''caf%C3%A9.txt\n\nneedle attachment\n--OUT--\n";
    seed(&fx,"mail/m6-mime.eml",src);
    check(&fx,"eml","mail/m6-mime.eml",&[
        "part 1 multipart/alternative › part 1.1 text/plain","attachment café.txt"]);
    flags(&fx,"eml","mail/m6-mime.eml","café.txt","café","^part [0-9.]+ ");
    put(&fx,"encoded.mbox",b"From a x\nSubject: =?UTF-8?B?QnVkZ2V0?=\n\nneedle\n");
    assert_eq!(paths(&matches(&output(&fx,"mbox","encoded.mbox","needle",&["--json"]))),["Budget › body"]);
    let decoded=output(&fx,"mbox","encoded.mbox","Budget",&["--json","--in-headings"]);
    assert_eq!(matches(&decoded).len(),1);
    put(&fx,"bad-mime.eml",b"Content-Type: multipart/mixed; boundary=X\n\n--X\nContent-Type: text/plain\n\nneedle\n");
    assert_eq!(paths(&matches(&output(&fx,"eml","bad-mime.eml","needle",&["--json"]))),["part 1 text/plain"]);
    put(&fx,"encoded-body.eml",b"Content-Transfer-Encoding: base64\n\nbmVlZGxl\n");
    let absent=output(&fx,"eml","encoded-body.eml","needle",&["--json"]);
    assert_eq!(code(&absent),1);
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test cli_m6_mail_chat m6_mime -- --nocapture`.
  Expected RED: parts flatten or encoded filenames leak undecoded text.
- [ ] **Step 3:** read MIME parameter quoting, nested multipart boundaries and closure
  markers with an iterative stack bounded for visible depth. A boundary closes
  its current part and descendants; an unclosed boundary ends at the enclosing
  message end. Include each part's headers as child header Segments. Decode
  RFC 2231 charset/language/percent data and ordered filename continuations,
  falling back to RFC 2047/plain filenames. Retain source offsets for decoded
  attachment names and hide synthetic numbering from name search. Never decode
  base64/quoted-printable bodies or write an attachment. Avoid searching a unit
  for the same boundary per Match; cache just its current matching message's
  MIME index. C11 checks lazy 1 GiB mbox timing/RSS.
- [ ] **Step 4:** rerun the exact command with `INSTA_UPDATE=always` for new
  snapshots; inspect the tree, flat and JSON snapshots against the assertions.
  Rerun with `INSTA_UPDATE=no`: selected tests PASS, stderr empty.
  All tests run on Linux and all four native CI systems; Windows GNU all-targets clippy also applies.
- [ ] **Step 5:** stage exactly the files above, the named fixtures and this
  topic's snapshots; commit with
  `git commit -m "feat: describe MIME parts and decoded attachment names"`.

### Task 9.3: WhatsApp dates, multiline messages and name-only detection

**Files:** create `src/processors/chat.rs::{Chat,ChatPrepared,ChatCursor}`,
`formats/whatsapp.toml`; append `m6_whatsapp`;
create `tests/fixtures/chat/_chat.txt`, its broken sibling.
`formats/whatsapp.toml` signals: `names=['_chat.txt']`, `paths=['*WhatsApp*.txt']` (D21).

`pub struct Chat;` implements native Processor. Definition `processor='chat'`,
`name_chars=40`; no locale/date-order parameter. Kinds `day`, `message`.
A day uses date as written, a message uses time as written plus sender, or time
alone for system messages. Neither date order nor calendar timezone is inferred.

- [ ] **Step 1 — RED:** add these complete tests and their literal primary fixture bytes.

```rust
#[test]
fn m6_whatsapp() {
    let fx=Fixture::new();
    let src="31/12/23, 21:41 - Ana: where's the needle?\ncontinued needle\n31/12/23, 21:42 - system needle\n[01/01/24, 09:00:01] Bob: needle\n";
    seed(&fx,"chat/_chat.txt",src.as_bytes());
    check(&fx,"whatsapp","chat/_chat.txt",&["31/12/23 › 21:41 Ana",
        "31/12/23 › 21:41 Ana","31/12/23 › 21:42","01/01/24 › 09:00:01 Bob"]);
    flags(&fx,"whatsapp","chat/_chat.txt","Ana","Ana","");
    for line in ["12/31/23, 9:41 PM - Ana: needle\n",
        "\u{200e}[31/12/23, 21:41:05] Ana: needle\n",
        "12/31/23, 9:41\u{202f}PM - Ana: needle\n"] {
        put(&fx,"date-case",line.as_bytes());
        let rows=matches(&output(&fx,"whatsapp","date-case","needle",&["--json"]));
        assert_eq!(rows[0]["trail"].as_array().unwrap().len(),2);
        assert!(rows[0]["trail"][1]["name"].as_str().unwrap().ends_with("Ana"));
    }
    put(&fx,"bad-chat",b"31/12/23, 21:41 - Ana: needle\nnot a date needle\n99/no date needle\n");
    assert_eq!(paths(&matches(&output(&fx,"whatsapp","bad-chat","needle",&["--json"]))),
        ["31/12/23 › 21:41 Ana","31/12/23 › 21:41 Ana","31/12/23 › 21:41 Ana"]);
    put(&fx,"renamed.txt",src.as_bytes());
    let auto=run(&mut fx.s4(["--no-docs","--json","needle","renamed.txt"]));
    assert!(matches(&auto).iter().all(|r|r["format"].is_null()));
    // The glob finds an export by the app's name in any language.
    put(&fx,"Chat de WhatsApp con Ana.txt",src.as_bytes());
    let auto=matches(&run(&mut fx.s4(["--no-docs","--json","needle","Chat de WhatsApp con Ana.txt"])));
    assert!(auto.iter().all(|r|r["format"]=="whatsapp"));
    assert_eq!(paths(&auto)[0],"31/12/23 › 21:41 Ana");
    // A `.txt` the glob claims that is no export: its first line opens no
    // message, so every Match keeps an empty Trail, even after a date line.
    let notes=b"WhatsApp backup notes: needle\n31/12/23, 21:41 - Ana: needle\n";
    put(&fx,"WhatsApp notes.txt",notes);
    let rows=matches(&run(&mut fx.s4(["--no-docs","--json","needle","WhatsApp notes.txt"])));
    assert_eq!(paths(&rows),["",""]);
    put(&fx,"bom-chat",[b"\xef\xbb\xbf".as_slice(),"\u{200e}".as_bytes(),src.as_bytes()].concat().as_slice());
    assert_eq!(paths(&matches(&output(&fx,"whatsapp","bom-chat","needle",&["--json"])))[0],"31/12/23 › 21:41 Ana");
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test cli_m6_mail_chat m6_whatsapp -- --nocapture`.
  Expected RED: chat unknown, opener/continuation/date groups wrong, or the
  non-export `WhatsApp notes.txt` decorated.
- [ ] **Step 3:** recognize only the listed Android/iOS timestamp shapes at physical line
  starts, including optional directional mark and narrow no-break space before
  AM/PM. Keep original date/time spelling in name pieces; do not reorder dd/mm.
  Group consecutive messages with the same literal date, retaining distinct
  day identities if that date reappears later. Sender ends at the first sender
  separator after the timestamp; system messages lack it. Opener name spans
  exclude their own Segments, while the body on the same line remains inside.
  Preparation reads through only the requested day: first-unit tests use a
  short first day and a distinct following date, so its boundary is discoverable.
  First-line check (spec §10.4 Failure): after a BOM and a U+200E, the first
  line must be a message opener; otherwise the Processor prepares no Segment
  and every Match has an empty Trail, so a `.txt` the glob claims that is no
  export shows as rg shows it (8385). The check decides structure only: the
  Format stays the one detection chose by name, as option A requires. Detection
  is `_chat.txt` plus `*WhatsApp*.txt`, with no localized name lists (D21). A
  renamed export uses explicit --format; no sniffing is added under option A.
- [ ] **Step 4:** rerun the exact command with `INSTA_UPDATE=always` for new
  snapshots; inspect the tree, flat and JSON snapshots against the assertions.
  Rerun with `INSTA_UPDATE=no`: selected tests PASS, stderr empty.
  All tests run on Linux and all four native CI systems; Windows GNU all-targets clippy also applies.
- [ ] **Step 5:** stage exactly the files above, the named fixtures and this
  topic's snapshots; commit with
  `git commit -m "feat: describe WhatsApp day and message Trails"`.

## Chunk 10: object layouts

Owns `src/processors/object/{mod,layouts,tiff,jpeg,png,webp,xmp,id3,mp4,wasm,class}.rs`,
`object/fixture/m6.rs` plus its module declaration in existing `fixture.rs`, eight
TOMLs, `tests/cli_m6_object.rs`, its snapshots. C10 owns `src/app.rs::Ctx::object`
and object-candidate calls in tanda 3, and `detector.rs::object_candidates`.
C11 touches none of these. Existing ELF/PE/Mach-O remain on their verified reader.

### Task 10.1: nested binary maps, TIFF IFDs and JPEG metadata

**Files:** create `object/layouts.rs::{BinaryMap,BinaryNode}`, `tiff.rs::read`,
`jpeg.rs::read`, `object/fixture/m6.rs::{tiff,jpeg}`;
modify `object/mod.rs` dispatch and `fixture.rs` module declaration;
create `formats/{jpeg,tiff}.toml`, `tests/cli_m6_object.rs` prelude and tests.

New interfaces, alongside the checked legacy `Map::parse` and `Map::trail`:

```rust
// object/layouts.rs
pub struct BinaryMap<'a> { /* bounded nodes and byte ownership intervals */ }
impl<'a> BinaryMap<'a> {
    pub fn parse(bytes: &'a [u8], layout: &str) -> Option<Self>;
    pub fn trail(&self, at: usize, lines: &crate::trail::LineIndex)
        -> crate::model::Trail;
    #[doc(hidden)]
    pub fn parse_loudly(bytes: &'a [u8], layout: &str) -> Option<Self>;
}
// object/mod.rs, unify both representations for the row pipeline.
pub enum LayoutMap<'a> { Legacy(Map<'a>), Extra(layouts::BinaryMap<'a>) }
impl<'a> LayoutMap<'a> {
    pub fn parse(bytes: &'a [u8], params: &toml::Table) -> Option<Self>;
    pub fn trail(&self, at: usize, lines: &crate::trail::LineIndex)
        -> crate::model::Trail;
}
```

Each Definition has `processor='object'`, `[params] layout='jpeg'|'tiff'`,
`name_chars=40`, no extensions or names, and §4.2 magic entries. All eight layouts
follow this convention; Java's Definition name is `java-class`, layout is `class`.
Kinds `object`, `section`, `key`, `symbol`, `class`, `method`, `field` as applicable.
Legacy `Object::names` stays empty under v1 §5.7; structural name search is not
expanded to binaries. Rows and `--expand` retain object semantics.

- [ ] **Step 1 — RED:** create `tests/cli_m6_object.rs` with the complete prelude
  and test below. All fixture functions below live in `object/fixture/m6.rs`.

```rust
mod common;
use common::{Fixture,code,run,stderr,stdout};
use searchfor::processors::object::fixture::m6 as binary;
use serde_json::Value;
fn rows(out: &std::process::Output) -> Vec<Value> {
    assert_eq!(code(out),0,"{}",stderr(out));
    assert!(out.stderr.is_empty(),"{}",stderr(out));
    stdout(out).lines().map(|l|serde_json::from_str::<Value>(l).unwrap())
        .filter(|v|v["type"]=="match").map(|v|v["data"].clone()).collect()
}
fn names(row: &Value) -> String {
    row["trail"].as_array().unwrap().iter().map(|s|s["name"].as_str().unwrap())
        .collect::<Vec<_>>().join(" › ")
}
fn binary_case(format: &str, bytes: &[u8], want: &str) {
    let fx=Fixture::new();
    std::fs::write(fx.file("m6-media.bin"),bytes).unwrap();
    let out=run(&mut fx.s4(["--no-docs","--json","needle","m6-media.bin"]));
    let found=rows(&out);
    assert!(!found.is_empty());
    assert_eq!(found[0]["format"],format);
    assert_eq!(names(&found[0]),want);
    common::assert_valid_json_schema(&stdout(&out));
    for (layout,flag) in [("tree","--tree"),("flat","--flat"),("json","--json")] {
        let out=run(&mut fx.s4(["--no-docs",flag,"needle","m6-media.bin"]));
        assert_eq!(code(&out),0,"{}",stderr(&out));
        assert!(out.stderr.is_empty());
        let out=if layout=="json"{common::mask_json_times(&stdout(&out))}else{stdout(&out)};
        insta::assert_snapshot!(format!("m6_{format}_{layout}"),out);
    }
    let expanded=run(&mut fx.s4(["--no-docs","--flat","--expand","needle","m6-media.bin"]));
    let flat=run(&mut fx.s4(["--no-docs","--flat","needle","m6-media.bin"]));
    assert_eq!(expanded.stdout,flat.stdout);
    let within=run(&mut fx.s4(["--no-docs","--json","--within",want,"needle","m6-media.bin"]));
    assert!(!rows(&within).is_empty());
}
#[test]
fn m6_tiff_jpeg() {
    binary_case("tiff",&binary::tiff(b"needle-cam 3000"),"TIFF › IFD0 › Model");
    binary_case("jpeg",&binary::jpeg(b"needle-cam 3000"),"JPEG › APP1 Exif › IFD0 › Model");
}
```

Complete small fixture writers:

```rust
pub fn tiff(text: &[u8]) -> Vec<u8> {
    let mut b=b"II\x2a\0\x08\0\0\0".to_vec();
    b.extend(1u16.to_le_bytes());             // IFD0 entry count
    b.extend(0x0110u16.to_le_bytes());        // Model, ASCII
    b.extend(2u16.to_le_bytes());
    b.extend(((text.len()+1) as u32).to_le_bytes());
    if text.len()+1<=4 {
        let mut value=[0;4]; value[..text.len()].copy_from_slice(text); b.extend(value);
    } else { b.extend(26u32.to_le_bytes()); }
    b.extend(0u32.to_le_bytes());             // next IFD
    if text.len()+1>4 { b.extend(text); b.push(0); }
    b
}
pub fn jpeg(text: &[u8]) -> Vec<u8> {
    let payload=[b"Exif\0\0".as_slice(),&tiff(text)].concat();
    let mut b=vec![0xff,0xd8,0xff,0xe1];
    b.extend(((payload.len()+2) as u16).to_be_bytes());
    b.extend(payload); b.extend([0xff,0xd9]); b
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test cli_m6_object m6_tiff_jpeg -- --nocapture`.
  Expected RED: missing fixture module/layout reader, then undecorated binary.
- [ ] **Step 3:** use checked arithmetic for every unit/table/value range.
  TIFF handles II/MM order, inline values ≤4 bytes, IFD0/IFD1, Exif/GPS/Interop
  pointers and a visited-offset set to stop cycles/repeated work. Validate count
  against remaining bytes before reserving; unknown tags show `tag 0xNNNN`.
  Borrow source names where possible. JPEG scans length-prefixed segments, COM,
  APP1 Exif/XMP, restart/no-length markers and entropy data escapes; do not decode
  pixels. Malformed overall layout returns None without a warning. A malformed
  inner packet may keep its validated container; §9 XMP has explicit recovery.
  Nested maps have explicit parents and byte-ownership intervals; they do not
  force unrelated discontiguous owners into Outline's containment tree.
- [ ] **Step 4:** integrate row dispatch through Task 10.4's API as a minimal
  first slice, then rerun with new snapshots reviewed. Linux and all native CI
  run these generated fixtures; no checkout endian conversion is possible.
- [ ] **Step 5:** commit with
  `git commit -m "feat: describe TIFF and JPEG metadata with bounded binary maps"`.

### Task 10.2: PNG, WebP and source-relative XMP

**Files:** create `object/{png,webp,xmp}.rs` readers and
`formats/{png,webp}.toml`; append writer functions and `m6_png_webp_xmp`.
Public binary map interface is Task 10.1; XMP composes existing XML markup ranges
inside packet bounds, suppressing ancestors through rdf:Description.

- [ ] **Step 1 — RED:** append complete writers and tests.

```rust
// object/fixture/m6.rs
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc=!0u32;
    for &b in bytes {
        crc^=b as u32;
        for _ in 0..8 { crc=(crc>>1)^if crc&1!=0{0xedb88320}else{0}; }
    }
    !crc
}
fn png_chunk(kind: &[u8;4], data: &[u8]) -> Vec<u8> {
    let mut b=(data.len() as u32).to_be_bytes().to_vec();
    b.extend(kind); b.extend(data); b.extend(crc32(&b[4..]).to_be_bytes()); b
}
pub fn png(text: &[u8]) -> Vec<u8> {
    let mut b=b"\x89PNG\r\n\x1a\n".to_vec();
    b.extend(png_chunk(b"IHDR", &[0,0,0,1,0,0,0,1,8,2,0,0,0]));
    b.extend(png_chunk(b"tEXt", &[b"Comment\0".as_slice(),text].concat()));
    b.extend(png_chunk(b"IEND", b"")); b
}
pub fn webp(text: &[u8]) -> Vec<u8> {
    let data=tiff(text);
    let mut body=b"WEBP".to_vec();
    body.extend(b"EXIF"); body.extend((data.len() as u32).to_le_bytes()); body.extend(data);
    if body.len()%2!=0 {body.push(0);}
    let mut b=b"RIFF".to_vec(); b.extend((body.len() as u32).to_le_bytes()); b.extend(body); b
}
pub fn jpeg_xmp(packet: &[u8]) -> Vec<u8> {
    let data=[b"http://ns.adobe.com/xap/1.0/\0".as_slice(),packet].concat();
    let mut b=vec![0xff,0xd8,0xff,0xe1];
    b.extend(((data.len()+2) as u16).to_be_bytes());b.extend(data);b.extend([0xff,0xd9]);b
}
// tests/cli_m6_object.rs
#[test]
fn m6_png_webp_xmp() {
    binary_case("png",&binary::png(b"needle"),"PNG › tEXt Comment");
    binary_case("webp",&binary::webp(b"needle"),"WebP › EXIF › IFD0 › Model");
    let xmp=b"<x:xmpmeta><rdf:RDF><rdf:Description><dc:title><rdf:Alt><rdf:li>needle</rdf:li></rdf:Alt></dc:title></rdf:Description></rdf:RDF></x:xmpmeta>";
    let fx=Fixture::new();
    let data=binary::jpeg_xmp(xmp);
    std::fs::write(fx.file("xmp.bin"),data).unwrap();
    let out=run(&mut fx.s4(["--no-docs","--json","needle","xmp.bin"]));
    assert_eq!(names(&rows(&out)[0]),"JPEG › APP1 XMP › dc:title › rdf:Alt › rdf:li");
    let broken=binary::jpeg_xmp(b"<rdf:Description><dc:title>ok</wrong>needle");
    std::fs::write(fx.file("bad-xmp.bin"),broken).unwrap();
    let out=run(&mut fx.s4(["--no-docs","--json","needle","bad-xmp.bin"]));
    assert_eq!(names(&rows(&out)[0]),"JPEG › APP1 XMP");
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test cli_m6_object m6_png_webp_xmp -- --nocapture`.
  Expected RED: absent formats/layouts or generic XMP Segment only.
- [ ] **Step 3:** PNG walks chunk lengths/CRC fields without decompressing
  zTXt/iTXt payloads; uncompressed iTXt text, tEXt keyword, eXIf TIFF and XMP
  packet ranges get structure. WebP requires RIFF/WEBP plus bounded chunks and
  word padding, dispatching EXIF/XMP payloads through the shared readers. The
  metadata scanner validates container structure, not pixel decodability; these
  synthetic metadata-only files are deliberate. TIFF offsets are relative to
  the TIFF packet start, XML offsets to the XML packet start; translate every
  range back to the containing Source before building Trail line fields. XMP
  hides repeated wrapper ancestors, preserves XML paths after Description and
  falls back to XMP container after its first XML error. No XML reads outside
  the packet and no recursive expansion of EXIF pointers or entities.
- [ ] **Step 4:** rerun with snapshot creation/review, then updates disabled:
  PASS. Linux/native CI run identical bytes; Latin-1 MP4 box names arrive next.
- [ ] **Step 5:** commit with
  `git commit -m "feat: describe PNG WebP and XMP packet metadata"`.

### Task 10.3: ID3 versions and MP4 box paths

**Files:** create `object/{id3,mp4}.rs`, `formats/{id3,mp4}.toml`;
append writer functions and `m6_id3_mp4` in this chunk's files.
Interfaces remain Task 10.1. ID3 labels identify version; MP4 major brand labels
the image. Frame/box kinds are section, tag values retain raw searched bytes.

- [ ] **Step 1 — RED:** append these complete fixture writers and tests.

```rust
// object/fixture/m6.rs
fn syncsafe(n: usize) -> [u8;4] {
    [((n>>21)&127) as u8,((n>>14)&127) as u8,((n>>7)&127) as u8,(n&127) as u8]
}
pub fn id3(text: &[u8]) -> Vec<u8> {
    let mut frame=b"TIT2".to_vec();frame.extend(syncsafe(text.len()+1));
    frame.extend([0,0,3]);frame.extend(text);
    let mut b=b"ID3\x04\0\0".to_vec();b.extend(syncsafe(frame.len()));b.extend(frame);b
}
fn box_bytes(kind: &[u8;4], data: &[u8]) -> Vec<u8> {
    let mut b=((data.len()+8) as u32).to_be_bytes().to_vec();b.extend(kind);b.extend(data);b
}
pub fn mp4(text: &[u8]) -> Vec<u8> {
    let data=box_bytes(b"data",&[&[0,0,0,1,0,0,0,0][..],text].concat());
    let item=box_bytes(&[0xa9,b'n',b'a',b'm'],&data);
    let ilst=box_bytes(b"ilst",&item);
    let meta=box_bytes(b"meta",&[&[0,0,0,0][..],&ilst].concat());
    let udta=box_bytes(b"udta",&meta);
    [box_bytes(b"ftyp",b"M4A \0\0\0\0M4A "),box_bytes(b"moov",&udta)].concat()
}
// tests/cli_m6_object.rs
#[test]
fn m6_id3_mp4() {
    binary_case("id3",&binary::id3(b"needle song"),"ID3v2.4 › TIT2");
    binary_case("mp4",&binary::mp4(b"needle"),"MP4 M4A › moov › udta › meta › ilst › ©nam");
    let fx=Fixture::new();
    let mut bytes=binary::id3(b"song");
    let mut tag=vec![0;128];tag[..3].copy_from_slice(b"TAG");tag[3..9].copy_from_slice(b"needle");
    bytes.extend(tag);std::fs::write(fx.file("v1.bin"),bytes).unwrap();
    let out=run(&mut fx.s4(["--no-docs","--json","needle","v1.bin"]));
    assert_eq!(names(&rows(&out)[0]),"ID3v1 › title");
    std::fs::write(fx.file("only-v1.bin"),[b"audio".as_slice(),&[0;10],b"TAGneedle"].concat()).unwrap();
    let plain=run(&mut fx.s4(["--no-docs","--json","needle","only-v1.bin"]));
    for row in stdout(&plain).lines().filter_map(|l|serde_json::from_str::<Value>(l).ok()) {
        if row["type"]=="match" {assert!(row["data"]["format"].is_null());}
    }
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test cli_m6_object m6_id3_mp4 -- --nocapture`.
  Expected RED: missing layouts or wrong box/frame ownership.
- [ ] **Step 3:** ID3 supports v2.2 3-byte ids/24-bit sizes, v2.3 sizes,
  v2.4 syncsafe sizes, flags/extended headers/footer, and trailing 128-byte
  ID3v1 alongside an initial detected v2 tag. Do not transform frame bytes for
  searching; unsynchronization/compression/encryption can suppress inner detail
  while retaining safe frame scope. Decode TXXX/WXXX descriptions for names,
  keep COMM label as COMM. MP4 walks bounded 32/64-bit lengths and zero-to-parent
  lengths, container type whitelist and meta's four-byte full-box header.
  mdat stays an opaque section. Read four-byte box kinds as Latin-1, trim brand
  padding in image name. ilst's data box is an implementation carrier: §9's
  example ends at ©nam, so **hide data under ilst items** with the explicit
  MP4 expectation ending at ©nam; keep the carrier
  bounds for validation. Other boxes retain their box paths. No old-QuickTime
  detection or FLAC/Ogg/HEIC-specific extraction is added.
- [ ] **Step 4:** rerun after enforcing §9's exact MP4 example, review all
  snapshots, then updates disabled: PASS on Linux and all native jobs.
- [ ] **Step 5:** commit with
  `git commit -m "feat: describe ID3 frames and MP4 metadata boxes"`.

### Task 10.4: Wasm, Java and Registry-ordered magic fallbacks

**Files:** create `object/{wasm,class}.rs`, `formats/{wasm,java-class}.toml`;
modify `detector.rs::object_candidates`, `src/app.rs::Ctx::object`,
object dispatch, fixture writers and `m6_wasm_java`.

```rust
// Detector method; object(head) remains the cheap first-candidate presence probe.
pub fn object_candidates<'a>(&'a self, head: &'a [u8])
    -> impl Iterator<Item = &'r crate::formats::definition::FormatDefinition> + 'a;
```

- [ ] **Step 1 — RED:** append complete binary writers and tests.

```rust
// object/fixture/m6.rs
fn leb(mut n: usize) -> Vec<u8> {
    let mut b=Vec::new();loop {let mut x=(n&127) as u8;n>>=7;if n!=0{x|=128;}b.push(x);if n==0{break;}}b
}
fn wasm_section(id: u8, data: &[u8]) -> Vec<u8> {
    let mut b=vec![id];b.extend(leb(data.len()));b.extend(data);b
}
pub fn wasm(text: &[u8]) -> Vec<u8> {
    let mut b=b"\0asm\x01\0\0\0".to_vec();
    b.extend(wasm_section(1,&[1,0x60,0,0]));
    b.extend(wasm_section(3,&[1,0]));b.extend(wasm_section(5,&[1,0,1]));
    b.extend(wasm_section(7,&[1,5,b't',b'o',b't',b'a',b'l',0,0]));
    b.extend(wasm_section(10,&[1,2,0,0x0b]));
    let mut data=vec![1,0,0x41,0,0x0b];data.extend(leb(text.len()));data.extend(text);
    b.extend(wasm_section(11,&data));b
}
pub fn class(text: &[u8]) -> Vec<u8> {
    fn utf(b: &mut Vec<u8>, s: &[u8]) {b.push(1);b.extend((s.len() as u16).to_be_bytes());b.extend(s);}
    let mut b=b"\xca\xfe\xba\xbe\0\0\0\x3d\0\x0a".to_vec();
    utf(&mut b,b"com/example/Cart");b.extend([7,0,1]);
    utf(&mut b,b"java/lang/Object");b.extend([7,0,3]);
    utf(&mut b,b"addItem");utf(&mut b,b"()V");utf(&mut b,b"Code");
    utf(&mut b,text);b.extend([8,0,8]);
    b.extend([0,0x21,0,2,0,4,0,0,0,0,0,1]); // access,this,super,interfaces,fields,methods
    b.extend([0,9,0,5,0,6,0,1,0,7]);       // method and one Code attribute
    b.extend(16u32.to_be_bytes());
    b.extend([0,1,0,0,0,0,0,4,0x12,9,0x57,0xb1,0,0,0,0]);
    b.extend([0,0]);b
}
// tests/cli_m6_object.rs
#[test]
fn m6_wasm_java() {
    binary_case("wasm",&binary::wasm(b"needle not found"),"WebAssembly › data › data[0]");
    binary_case("java-class",&binary::class(b"needle merged"),"Java 17 › com.example.Cart › addItem");
    let fx=Fixture::new();
    std::fs::write(fx.file("Cart.bin"),binary::class(b"needle")).unwrap();
    let out=run(&mut fx.s4(["--no-docs","--json","addItem","Cart.bin"]));
    assert_eq!(names(&rows(&out)[0]),"Java 17 › com.example.Cart");
    use searchfor::processors::object::fixture::{Spec,macho,fat};
    let spec=Spec::new().section("__TEXT,__cstring",b"needle\0");
    let thin=macho(&spec);
    let (universal,_starts)=fat(&[(0x01000007,thin.file)]);
    std::fs::write(fx.file("fat.bin"),universal).unwrap();
    let out=run(&mut fx.s4(["--no-docs","--json","needle","fat.bin"]));
    assert_eq!(rows(&out)[0]["format"],"macho");
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test cli_m6_object m6_wasm_java -- --nocapture`.
  Expected RED: missing layouts or `cafebabe` selected as macho with no class rows.
- [ ] **Step 3:** Wasm reads bounded unsigned LEB128 sizes and all named sections
  in §9, import module.field, export names, data segments and code bodies; a late
  name custom section supplies function names, else func[N]. Reject overlong or
  overflowing lengths before slicing. Unknown/custom sections stay section
  scopes. Java reads constant-pool tags, two-slot long/double, class/member tables
  and Code attribute bounds. Map string constants to their first loading method
  in declaration order, walking JVM fixed lengths, wide, tableswitch and
  lookupswitch with checked offsets/padding. Resolve ldc/ldc_w String references,
  then member-name owner, else constant pool. Method own-name ranges have class
  Trail; field/method associations are explicit ownership spans rather than
  containment inferred from physical constant-pool placement. Decode only labels
  from modified UTF-8, never bytes searched. Unknown version gets Java class M.m.
  For every candidate magic match, validate LayoutMap in Registry order; failed
  fat Mach-O proceeds to Java, valid fat Mach-O stops there. An explicit object
  --format restricts to that Definition, and another path-decided Format still
  prevents object decoration as in v1. All invalid candidates fall back to rg.
  `java-class.toml` registers after `macho.toml` in `builtin::BUILTIN`, so
  `object(head)` still answers `macho` for `\xca\xfe\xba\xbe` and the v1 unit
  test `formats::tests::the_m4_definitions_load_and_detect`
  (`src/formats/mod.rs:279` on `d14a4c1`) stays as it is (Task 0's sweep).
- [ ] **Step 4:** rerun using the checked `fat(&[(u32, Vec<u8>)]) -> (Vec<u8>, Vec<usize>)`
  fixture signature. Review snapshots: class, Wasm and valid universal Mach-O all PASS on
  Linux and native CI. Walked binary policy is verified in Task 10.5.
- [ ] **Step 5:** commit with
  `git commit -m "feat: describe Wasm and Java layouts with ordered magic fallback"`.
### Task 10.5: binary modes, malformed lengths, fuzz and rate

**Files:** append `m6_binary_modes`, `m6_binary_mutations`,
`m6_binary_mutation_case`, `m6_binary_rate` to `tests/cli_m6_object.rs`;
add `BinaryMap::ranges` for test inspection in `layouts.rs`.
No mutation or timing code is linked into the s4 runtime.

```rust
// layouts.rs, hidden diagnostic API: source byte ranges, not decoded payloads.
#[doc(hidden)]
pub fn ranges(&self) -> impl Iterator<Item = std::ops::Range<usize>> + '_;
```

- [ ] **Step 1 — RED:** append these complete tests.

```rust
fn binaries() -> Vec<(&'static str,Vec<u8>)> {
    vec![("jpeg",binary::jpeg(b"needle")),("png",binary::png(b"needle")),
        ("webp",binary::webp(b"needle")),("tiff",binary::tiff(b"needle")),
        ("id3",binary::id3(b"needle")),("mp4",binary::mp4(b"needle")),
        ("wasm",binary::wasm(b"needle")),("class",binary::class(b"needle"))]
}
#[test]
fn m6_binary_modes() {
    // As rg_compat.rs: without rg 14.1.1 return, or panic under SEARCHFOR_REQUIRE_REFERENCE=1.
    if !common::reference_rg() {return;}
    let fx=Fixture::new();
    std::fs::create_dir(fx.file("m6-binaries")).unwrap();
    for (layout,b) in binaries() {
        let file=format!("m6-binaries/{layout}.bin");
        std::fs::write(fx.file(&file),b).unwrap();
        for flags in [vec!["-v"],vec!["-c"],vec!["-l"],vec!["-q"],vec!["--no-trail"]] {
            let s=run(&mut fx.s4(["--no-docs"]).args(&flags).args(["needle",&file]));
            let mut args=flags.clone();args.retain(|s|*s!="--no-trail");
            let r=run(&mut fx.rg(args.iter().copied().chain(["needle",file.as_str()])));
            assert_eq!(code(&s),code(&r));assert_eq!(s.stdout,r.stdout);assert_eq!(s.stderr,r.stderr);
        }
    }
    let s=run(&mut fx.s4(["--no-docs","--flat","needle","m6-binaries"]));
    let r=run(&mut fx.rg(["needle","m6-binaries"]));
    assert_eq!(code(&s),code(&r));assert_eq!(s.stdout,r.stdout);
    for binary_flag in ["--binary","-a"] {
        let out=run(&mut fx.s4(["--no-docs","--json",binary_flag,"needle","m6-binaries"]));
        assert!(rows(&out).iter().all(|r|r["format"].is_string()));
    }
}
#[test]
fn m6_binary_mutation_case() {
    let Some(path)=std::env::var_os("SEARCHFOR_M6_BINARY_CASE") else{return};
    let bytes=std::fs::read(path).unwrap();
    let layout=std::env::var("SEARCHFOR_M6_BINARY_LAYOUT").unwrap();
    if let Some(map)=searchfor::processors::object::layouts::BinaryMap::parse_loudly(&bytes,&layout) {
        for range in map.ranges() {assert!(range.start<=range.end && range.end<=bytes.len());}
        let lines=searchfor::trail::LineIndex::new(&bytes);
        for at in 0..bytes.len() {
            for s in map.trail(at,&lines) {assert!(s.line>0 && s.end_line>=s.line);}
        }
    }
}
#[test]
fn m6_binary_mutations() {
    let fx=Fixture::new();
    for (layout,seed) in binaries() {
        for n in 0..seed.len()+4 {
            let mut b=seed.clone();
            if n<seed.len(){b.truncate(n)}else{for x in b.iter_mut().skip(n- seed.len()).step_by(7){*x=0xff;}}
            std::fs::write(fx.file("case.bin"),&b).unwrap();
            let mut cmd=std::process::Command::new(std::env::current_exe().unwrap());
            cmd.args(["--exact","m6_binary_mutation_case","--nocapture"])
                .env("SEARCHFOR_M6_BINARY_CASE",fx.file("case.bin"))
                .env("SEARCHFOR_M6_BINARY_LAYOUT",layout);
            let (status,err)=common::run_limited(&mut cmd,std::time::Duration::from_secs(2));
            assert!(status.is_some_and(|s|s.success()),"{layout}/{n}: {}",String::from_utf8_lossy(&err));
            let (status,err)=common::run_limited(
                &mut fx.s4(["--no-docs","--json","needle","case.bin"]),
                std::time::Duration::from_secs(2));
            assert!(status.and_then(|s|s.code()).is_some_and(|c|c==0||c==1),"{layout}/{n}: {err:?}");
        }
    }
}
#[test]
#[ignore="local binary-layout rate; selected explicitly"]
fn m6_binary_rate() {
    use searchfor::processors::object::layouts::BinaryMap;
    for (layout,mut bytes) in binaries() {
        // Repeat a valid small file for bounded parser overhead; bytes/sec is
        // measured on bytes actually supplied, not padding a single tiny unit.
        let unit=bytes.clone();let repeats=(100_000_000/unit.len()).max(1);
        let started=std::time::Instant::now();
        for _ in 0..repeats {
            assert!(BinaryMap::parse_loudly(&bytes,layout).is_some());
            std::hint::black_box(&mut bytes);
        }
        let rate=(unit.len()*repeats) as f64/started.elapsed().as_secs_f64();
        eprintln!("{layout}: {rate:.0} bytes/s");
        if std::env::var_os("SEARCHFOR_PERF_TIMING").is_some_and(|s|s=="1") {assert!(rate>=100_000_000.0);}
    }
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test cli_m6_object m6_binary -- --nocapture`.
  Expected RED: missing bounds API or malformed input violates bounds/fallback.
  A mutation case with no env is only the supervisor entry point, never a counted
  fuzz run; `m6_binary_mutations` must execute all eight layouts.
- [ ] **Step 3:** fix unchecked offsets, cycles and multiplicative allocations
  found by truncation/corruption. Retain original rg behavior in summary/invert
  modes and walked binary files. Add declared-length bombs by replacing every
  length/count field with its maximum value and repeating pointer targets;
  the complete supplemental table below supplies those byte edits. Preserve
  before-error ownership only where §9 explicitly permits inner recovery;
  invalid top-level layouts are rg fallbacks. Use per-case child supervision so
  panics, aborts and hangs fail with reproducible layout/case bytes. Save the
  in-flight bytes/seed through common fuzz artifact helpers before each child.
- [ ] **Step 4:** run the command GREEN, then
  `SEARCHFOR_PERF_TIMING=1 CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --release --test cli_m6_object m6_binary_rate -- --ignored --nocapture`.
  Expected ≥100,000,000 bytes/s per layout; also measure each large layout fixture
  described in the supplemental matrix so tiny hot-cache speed alone is
  insufficient. CI runs modes/mutations on all four systems; Linux verifies
  bounded RSS with the same child harness as C11. Timing is local evidence.
- [ ] **Step 5:** commit with
  `git commit -m "test: bound malformed binary layouts and preserve rg modes"`.

## Chunk 11: cost, dense and fuzz

Owns `tests/perf.rs`, `tests/fuzz_m6.rs`, `scripts/bench-m6.sh` and
`scripts/gen-m6-corpus.py`. C10 covers its binary fuzz/rates; this chunk covers
every text syntax, JSON view and markup extension without editing their source.
The in-process app differential and central rg comparison are C12.2.

### Task 11.1: every-byte cursor/model and supervised text mutations

**Files:** create `tests/fuzz_m6.rs::{targets,check_case,m6_cursor_model,
m6_mutation_child,m6_mutations}`. The test file owns its tiny seed loader;
no new dependency on C10's fixture API.

- [ ] **Step 1 — RED:** create the file with this complete code.

```rust
mod common;
use searchfor::{formats::Registry,model::Input,processors::{Processor,Processors,Capability}};
use std::path::Path;
fn targets() -> Vec<(String,toml::Table,Vec<u8>)> {
    let (r,w)=Registry::load(None,&Processors::known);assert!(w.is_empty());
    let wanted=["asciidoc","rst","org","latex","jupyter","ini","systemd",
        "properties","env","ssh-config","nginx","apache","hcl","dockerfile",
        "make","just","sql","diff","vue","svelte","blade","graphql","protobuf",
        "har","eml","mbox","icalendar","vcard","bibtex","whatsapp","telegram","telegram-html"];
    let files=["docs-a/m6.adoc","docs-a/m6.rst","docs-a/m6.org","docs-a/m6.tex",
        "docs-a/m6.ipynb","config/m6.ini","config/m6.service","config/m6.properties",
        "config/.env.m6","config/ssh/ssh_config","config/nginx/nginx.conf","config/apache2/apache2.conf",
        "config/m6.tf","config/m6.dockerfile","config/m6.mk","config/m6.just",
        "code-c/m6-dump.sql","code-c/m6.patch","code-c/m6.vue","code-c/m6.svelte",
        "code-c/m6.blade.php","code-c/m6.graphql","code-c/m6.proto","code-c/m6.har",
        "mail/m6.eml","mail/m6.mbox","config/m6.ics","config/m6.vcf","docs-a/m6.bib",
        "chat/_chat.txt","chat/ChatExport_m6/result.json","chat/ChatExport_m6/messages.html"];
    wanted.into_iter().zip(files).map(|(name,file)| {
        let d=r.get(name).unwrap();
        let src=std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(file)).unwrap();
        (name.into(),d.params.clone(),src)
    }).collect()
}
fn processor(name: &str) -> &'static dyn Processor {
    let (r,w)=Registry::load(None,&Processors::known);assert!(w.is_empty());
    searchfor::processors::by_name(&r.get(name).unwrap().processor).unwrap()
}
fn check_case(name: &str, params: &toml::Table, bytes: &[u8]) {
    let p=processor(name);
    let input=Input{bytes,path:Path::new("m6"),format:name,disk:None};
    assert_eq!(p.capability(params),Capability::Native);
    let at:Vec<_>=(0..=bytes.len()).collect();
    let expected=p.trails(&input,params,&at).unwrap();
    let prepared=p.prepare(input,params).unwrap();assert!(prepared.streams());
    let mut c=prepared.cursor();
    for (&a,want) in at.iter().zip(&expected) {
        assert_eq!(c.trail_at(a).unwrap().as_ref(),want.as_slice(),"{name}/{a}");
        assert!(c.open_from().is_some_and(|s|s<=a),"{name}/{a}: no safe open_from");
        for s in want {
            assert!(s.line>0 && s.end_line>=s.line);
            assert!(s.end_line<=1+bytes.iter().filter(|&&b|b==b'\n').count() as u64);
        }
        let one=c.trail_at(a).unwrap();let two=c.trail_at(a).unwrap();
        assert!(std::rc::Rc::ptr_eq(&one,&two),"{name}/{a}: equal Trails not shared");
    }
    for &a in at.iter().rev().step_by(3) {assert_eq!(c.trail_at(a).unwrap().as_ref(),expected[a].as_slice());}
    for n in prepared.names().unwrap() {
        assert!(n.anchor<=bytes.len());
        for (text,source) in n.pieces {
            assert!(text.start<=text.end && text.end<=n.text.len());
            assert!(source.start<=source.end && source.end<=bytes.len());
            assert!(n.text.is_char_boundary(text.start)&&n.text.is_char_boundary(text.end));
        }
    }
}
#[test]
fn m6_cursor_model() {
    for (name,params,src) in targets() {check_case(&name,&params,&src);}
}
#[test]
fn m6_mutation_child() {
    let Some(path)=std::env::var_os("SEARCHFOR_M6_FUZZ_CASE") else{return};
    let name=std::env::var("SEARCHFOR_M6_FUZZ_FORMAT").unwrap();
    let (r,w)=Registry::load(None,&Processors::known);assert!(w.is_empty());
    let bytes=std::fs::read(path).unwrap();
    check_case(&name,&r.get(&name).unwrap().params,&bytes);
}
#[test]
fn m6_mutations() {
    let fx=common::Fixture::new();
    let seed=common::fuzz_seed(0x6_2026_1104);
    let mut rng=seed;
    let cases=targets();let n=common::fuzz_n().unwrap_or(64);
    let budget=common::fuzz_budget(cases.len()*n);
    let flight=common::FuzzInFlight::new("m6_mutations");
    // First the Format × operation product, before any deadline is checked, so
    // every Format meets all four operations; after it the RNG picks them.
    let product=cases.len()*4;
    let mut seen=vec![[false;4];cases.len()];
    let mut completed=0;
    while completed<product || budget.permits(completed) {
        let target=completed%cases.len();
        let (name,_,src)=&cases[target];
        rng^=rng<<13;rng^=rng>>7;rng^=rng<<17;
        let op=if completed<product {completed/cases.len()} else {((rng>>32) as usize)%4};
        seen[target][op]=true;
        let mut b=src.clone();
        let at=(rng as usize)%(b.len()+1);
        match op {
            0=>b.truncate(at),
            1=>if at<b.len(){b[at]^=0xff},
            2=>{let part=b[..at].to_vec();b.extend(part);},
            _=>{b.splice(at..at,b"\0\xff}\n".iter().copied());},
        }
        std::fs::write(fx.file("mutated"),&b).unwrap();
        flight.at(||format!("seed={seed} case={completed} rng={rng} format={name}"));
        let argv=[std::ffi::OsStr::new(name.as_str())];
        let saved=common::save_fuzz_case(&common::FuzzCase{test:"m6_mutations",seed,n,
            case:completed,argv:&argv,files:&[("case.bin",&b)],stderr:b"",what:"in flight"});
        let mut cmd=std::process::Command::new(std::env::current_exe().unwrap());
        cmd.args(["--exact","m6_mutation_child","--nocapture"])
            .env("SEARCHFOR_M6_FUZZ_CASE",fx.file("mutated"))
            .env("SEARCHFOR_M6_FUZZ_FORMAT",name);
        let (status,err)=common::run_limited(&mut cmd,std::time::Duration::from_secs(2));
        assert!(status.is_some_and(|s|s.success()),"seed={seed} case={completed} {name}: {}",String::from_utf8_lossy(&err));
        let (status,err)=common::run_limited(&mut fx.s4(["--no-docs","--json","--format",name,"needle","mutated"]),std::time::Duration::from_secs(2));
        assert!(status.and_then(|s|s.code()).is_some_and(|c|c==0||c==1),"{name}: {err:?}");
        if let Some(saved)=saved{std::fs::remove_dir_all(saved).unwrap();}
        completed+=1;
    }
    assert!(completed>=product);
    for ((name,_,_),ops) in cases.iter().zip(&seen) {
        assert!(ops.iter().all(|&o|o),"{name}: an operation never ran: {ops:?}");
    }
    flight.finish();
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test fuzz_m6 m6_cursor_model -- --nocapture`.
  Expected RED: an unregistered/new Processor or incorrect streaming/open_from.
  Then run `SEARCHFOR_FUZZ_N=4 CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test fuzz_m6 m6_mutations -- --nocapture`.
  Expected RED for any bounds, panic or broken recovery bug found; otherwise
  record this as new preservation coverage, without claiming an observed fail.
- [ ] **Step 3:** repair findings in the owning chunk through the orchestrator,
  rather than editing other chunks' scanners here. Retain the same model/cursor
  equality for every byte, repeated offset, backward query and same-Trail Rc.
  Add deterministic 200,000-depth inputs and hostile long names from the
  supplemental matrix; mutation fuzz alone does not guarantee those boundaries.
  Drop unused imports under clippy. When adding an actual replay mode, preserve
  the saved exact bytes and use the child env above, so replay requires no RNG
  alignment. The first 128 cases are the product of the 32 text Formats and the
  four operations, run before the shared deadline is checked; the RNG picks the
  operation only after it, and the test asserts that every Format met all four
  (plan r1, Codex #5: `completed % 4` gave each Format one operation forever).
- [ ] **Step 4:** run both commands GREEN, then
  `SEARCHFOR_FUZZ_SECONDS=120 CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --release --test fuzz_m6 m6_mutations -- --nocapture`.
  Native CI runs counted small corpus on all four systems and budgeted fuzz per
  its workflow policy; time-limited tests supervise each case at 2 s everywhere.
- [ ] **Step 5:** commit with
  `git commit -m "test: fuzz every M6 text syntax and compare native cursors"`.

### Task 11.2: nine dense families, 50 MB scans and first-unit gigabyte files

**Files:** extend `tests/perf.rs::{m6_perf_dir,m6_dense_bytes,m6_dense,m6_sparse,m6_first_unit,m6_html_scripts_cost}`,
reusing checked `alone`, `measure_drained`, `sfor_bin`, `rg_bin`, `timing_enabled`.
Tests stay Linux-gated where `/usr/bin/time -v` provides peak RSS. Functional
small versions of the generators run on all systems through the CLI tests.

- [ ] **Step 1 — RED:** append the complete generator and tests below.

```rust
/// A disk directory for the 50 MiB and 1 GiB files, never `/tmp`: on a tmpfs
/// `/tmp` they sit in RAM beside the RSS being measured, and a small one fails
/// with ENOSPC. `SEARCHFOR_PERF_DIR`, else `CARGO_TARGET_DIR`, else `target/`.
#[cfg(target_os="linux")]
fn m6_perf_dir() -> tempfile::TempDir {
    let base=std::env::var_os("SEARCHFOR_PERF_DIR").or_else(||std::env::var_os("CARGO_TARGET_DIR"))
        .map(std::path::PathBuf::from)
        .unwrap_or_else(||std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target"));
    std::fs::create_dir_all(&base).unwrap();
    tempfile::tempdir_in(base).unwrap()
}
#[cfg(target_os="linux")]
fn m6_dense_bytes(format: &str,size: usize) -> Vec<u8> {
    let (head,unit,tail)=match format {
        "asciidoc"=>("= ONCE_M6\n","== needle\nneedle\n",""),
        "ini"=>("[ONCE_M6]\n","k=needle\n",""),
        "nginx"=>("http { # ONCE_M6\n","k needle;\n","}\n"),
        "make"=>("# ONCE_M6\n","k := needle\n",""),
        "sql"=>("-- ONCE_M6\nINSERT INTO t(a) VALUES\n","('needle'),\n","('needle');\n"),
        "diff"=>("diff --git a/ONCE_M6 b/ONCE_M6\n--- a/ONCE_M6\n+++ b/ONCE_M6\n@@ -0,0 +1,99999999 @@\n","+needle\n",""),
        "mbox"=>("From a x\nSubject: ONCE_M6\n\n","needle\n",""),
        "whatsapp"=>("31/12/23, 21:41 - ONCE_M6: needle\n","31/12/23, 21:42 - Ana: needle\n",""),
        "har"=>("{\"log\":{\"comment\":\"ONCE_M6\",\"entries\":[\n",
          "{\"request\":{\"method\":\"GET\",\"url\":\"http://x\"},\"x\":\"needle\"},\n",
          "{\"request\":{\"method\":\"GET\",\"url\":\"http://x\"},\"x\":\"needle\"}]}}\n"),
        _=>unreachable!(),
    };
    let mut b=head.as_bytes().to_vec();
    while b.len()+unit.len()+tail.len()<=size {b.extend(unit.as_bytes());}
    let pad=size-b.len()-tail.len();b.extend(std::iter::repeat_n(b' ',pad));
    b.extend(tail.as_bytes());assert_eq!(b.len(),size);b
}
#[test]
#[ignore="M6 50 MiB dense RSS; selected explicitly on Linux"]
#[cfg(target_os="linux")]
fn m6_dense() {
    let _alone=alone();let fx=Fixture::new();
    let big=m6_perf_dir();let file=big.path().join("dense.m6");let file=file.to_str().unwrap();
    for format in ["asciidoc","ini","nginx","make","sql","diff","mbox","whatsapp","har"] {
        let src=m6_dense_bytes(format,50<<20);std::fs::write(file,src).unwrap();
        let sample=common::run(&mut fx.s4(["--no-docs","--json","--format",format,"-m3","needle",file]));
        assert_eq!(common::code(&sample),0);
        assert!(common::stdout(&sample).lines().filter_map(|s|serde_json::from_str::<serde_json::Value>(s).ok())
            .any(|v|v["type"]=="match"&&!v["data"]["trail"].as_array().unwrap().is_empty()));
        for layout in ["--flat","--json"] {
            let base=["--no-docs",layout,"--format",format,"-j1"];
            let dense=[base.as_slice(),&["needle|^",file]].concat();
            let once=[base.as_slice(),&["ONCE_M6",file]].concat();
            let raw=[base.as_slice(),&["--no-trail","needle|^",file]].concat();
            let (td,rd)=measure_drained(&sfor_bin(),&fx,&dense,0).unwrap();
            let (tp,rp)=measure_drained(&sfor_bin(),&fx,&once,0).unwrap();
            let (t0,r0)=measure_drained(&sfor_bin(),&fx,&raw,0).unwrap();
            assert!(rd<=rp+(32<<20),"{format}/{layout}: {rd} > {rp}+32MiB");
            if timing_enabled()&&layout=="--json" {
                assert!(td.as_secs_f64()<=tp.as_secs_f64()+6.0*t0.as_secs_f64()+0.1);
            }
            eprintln!("m6 {format}/{layout}: dense={td:?}/{rd} once={tp:?}/{rp} raw={t0:?}/{r0}");
        }
    }
}
#[test]
#[ignore="M6 50 MB sparse processor rate; local timing"]
#[cfg(target_os="linux")]
fn m6_sparse() {
    let _alone=alone();let fx=Fixture::new();let rg=rg_bin().unwrap();
    let big=m6_perf_dir();let file=big.path().join("sparse.m6");let file=file.to_str().unwrap();
    for format in ["asciidoc","ini","nginx","make","sql","diff","mbox","whatsapp","har"] {
        let mut src=m6_dense_bytes(format,50_000_000);
        // Exactly three original needle occurrences become unique patterns;
        // every other byte, including structure and ordinary needles, is kept.
        let positions:Vec<_>=memchr::memmem::find_iter(&src,b"needle").collect();
        for &at in [positions[0],positions[positions.len()/2],*positions.last().unwrap()].iter() {
            src[at..at+6].copy_from_slice(b"M6HIT!");
        }
        std::fs::write(file,src).unwrap();
        let (ts,_)=measure_drained(&sfor_bin(),&fx,&["--no-docs","--flat","--format",format,"M6HIT!",file],0).unwrap();
        let (tr,_)=measure_drained(&rg,&fx,&["M6HIT!",file],0).unwrap();
        if timing_enabled(){assert!(ts<=tr+std::time::Duration::from_secs(1),"{format}: {ts:?}/{tr:?}");}
    }
}
#[test]
#[ignore="M6 first unit of 1 GiB files; local time and Linux RSS"]
#[cfg(target_os="linux")]
fn m6_first_unit() {
    use std::io::Write;
    let _alone=alone();let fx=Fixture::new();let rg=rg_bin().unwrap();
    let big=m6_perf_dir();let file=big.path().join("large.m6");let file=file.to_str().unwrap();
    for (format,first,rest) in [
        ("sql","INSERT INTO t(a) VALUES ('needle');\n","INSERT INTO t(a) VALUES ('plain');\n"),
        ("mbox","From a x\nSubject: First\n\nneedle\n\n","From b x\nSubject: Later\n\nplain\n\n"),
        ("whatsapp","01/01/24, 10:00 - Ana: needle\n","02/01/24, 10:00 - Bob: plain\n"),
    ] {
        let mut f=std::fs::File::create(file).unwrap();f.write_all(first.as_bytes()).unwrap();
        let unit=rest.repeat(32768);let mut left=(1usize<<30)-first.len();
        while left>0 {let n=left.min(unit.len());f.write_all(&unit.as_bytes()[..n]).unwrap();left-=n;}
        drop(f);
        let (ts,rs)=measure_drained(&sfor_bin(),&fx,&["--no-docs","--flat","--format",format,"needle",file],0).unwrap();
        let (tr,rr)=measure_drained(&rg,&fx,&["needle",file],0).unwrap();
        assert!(rs<=rr+(64<<20),"{format}: RSS {rs}/{rr}");
        if timing_enabled(){assert!(ts<=tr+std::time::Duration::from_millis(500),"{format}: {ts:?}/{tr:?}");}
        std::fs::remove_file(file).unwrap();
    }
}
#[test]
#[ignore="M6 50 MB HTML page of scripts; local timing"]
#[cfg(target_os="linux")]
fn m6_html_scripts_cost() {
    // Spec §11 «Scripts in HTML», #53: Matches inside three scripts parse those
    // three only; Matches outside every script parse none.
    let _alone=alone();let fx=Fixture::new();let rg=rg_bin().unwrap();
    let big=m6_perf_dir();let file=big.path().join("scripts.html");let file=file.to_str().unwrap();
    // 66 KB scripts of short lines, under both caps, about 50 MB in all.
    let body="function f() { return 'plain'; }\n".repeat(2000);
    let script=format!("<script>\n{body}</script>\n<p>text</p>\n");
    let count=50_000_000/script.len();
    for (case,inside) in [("inside",true),("outside",false)] {
        let mut page=String::from("<html><body>\n");
        for i in 0..count {
            let hit=[0,count/2,count-1].contains(&i);
            if hit&&inside {page.push_str(&script.replacen("'plain'","'M6HIT!'",1));}
            else if hit {page.push_str(&script.replacen("<p>text</p>","<p>M6HIT!</p>",1));}
            else {page.push_str(&script);}
        }
        page.push_str("</body></html>\n");
        std::fs::write(file,&page).unwrap();
        let out=common::run(&mut fx.s4(["--no-docs","--json","--format","html","M6HIT!",file]));
        assert_eq!(common::code(&out),0);
        let rows:Vec<serde_json::Value>=common::stdout(&out).lines()
            .filter_map(|l|serde_json::from_str::<serde_json::Value>(l).ok())
            .filter(|v|v["type"]=="match").collect();
        assert_eq!(rows.len(),3,"{case}");
        for r in &rows {
            let last=r["data"]["trail"].as_array().unwrap().last().unwrap()["name"].as_str().unwrap();
            assert_eq!(last=="f",inside,"{case}: {last}");
        }
        let (ts,_)=measure_drained(&sfor_bin(),&fx,&["--no-docs","--flat","--format","html","M6HIT!",file],0).unwrap();
        let (tr,_)=measure_drained(&rg,&fx,&["M6HIT!",file],0).unwrap();
        eprintln!("m6 html scripts/{case}: s4={ts:?} rg={tr:?}");
        if timing_enabled(){assert!(ts<=tr+std::time::Duration::from_secs(1),"{case}: {ts:?}/{tr:?}");}
    }
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --release --test perf m6_ -- --ignored --test-threads 1 --nocapture`.
  Expected RED for any dense-retention violation, missing structure or eager
  whole-file preparation. A baseline already satisfying a bound is recorded.
- [ ] **Step 3:** benchmark literal same bytes for dense/once/raw, report R0
  as well as Rone. The time assertion applies only to JSON; flat time is reported.
  Add the per-family supplemental 50 MB cases for reused markup and non-HAR
  views/syntax variants so the nine criterion-50 representatives do not substitute
  for every scanner's rate requirement. `sql`'s sparse third Match may be in its
  final tuple; verify row/column identity before accepting timing. Check
  first-unit output is structured and names the first unit, rather than timing
  a Plain fallback. The next WhatsApp day boundary must be read only as lookahead,
  with no parsing of its gigabyte of messages. No `-m1` shortcut in criterion 52.
  Add multithreaded dense structural-mode coverage using N×8 MiB spool allowance
  as M5b does; complete supplemental test code appears below.
  `m6_html_scripts_cost` (D24) needs C7.4 merged, which tanda 2 guarantees: a
  Trail ending in `f` proves the script was parsed. The `outside` case holds
  rg + 1 s with 50 MB of unmatched scripts; parsing all of them would, by our
  estimate of tree-sitter's rate (not measured yet), take several seconds, so
  the bound catches an eager parse. Record both times.
- [ ] **Step 4:** rerun GREEN and repeat with `SEARCHFOR_PERF_TIMING=1` locally.
  Linux CI runs RSS assertions with timing disabled. macOS and Windows execute
  portable functional and fuzz tests; their timing/RSS is not inferred from GNU
  time. Record all ratios and raw byte sizes, keeping MiB distinct from MB.
- [ ] **Step 5:** commit with
  `git commit -m "test: extend dense and large-file ceilings to M6 processors"`.
### Task 11.3: reproducible per-file and path-detection measurements

**Files:** create `scripts/gen-m6-corpus.py`, `scripts/bench-m6.sh`;
append `m6_benchmark_gate_selftest` to `tests/perf.rs`. No core scanner edits.

- [ ] **Step 1 — RED:** add this complete evaluator test.

```rust
#[test]
fn m6_benchmark_gate_selftest() {
    let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let out=std::process::Command::new("bash").arg(root.join("scripts/bench-m6.sh"))
        .arg("--self-test").output().unwrap();
    assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));
    assert_eq!(out.stdout,b"M6 cost gates: PASS\n");
}
```

Complete corpus generator, stored as `scripts/gen-m6-corpus.py`:

```python
import json, pathlib, sys
root = pathlib.Path(sys.argv[1]); fixtures = pathlib.Path(__file__).resolve().parents[1] / 'tests/fixtures'
files = [p for group in ('docs-a', 'config', 'code-c', 'mail', 'chat')
         for p in (fixtures / group).rglob('*') if p.is_file() and 'broken-' not in p.name]
files = [p for p in files if b'needle' in p.read_bytes()]
assert files, 'M6 fixtures missing'
meta = {'matching_bytes': 0, 'largest_matching_bytes': 0, 'files': 0, 'matching_files': 0}
for i in range(20000):
    source = files[i % len(files)]; raw = source.read_bytes()
    matched = i % 100 == 0
    if matched:
        raw = raw.replace(b'needle', b'M6MARK', 1).replace(b'needle', b'normal')
        meta['matching_files'] += 1; meta['matching_bytes'] += len(raw)
        meta['largest_matching_bytes'] = max(meta['largest_matching_bytes'], len(raw))
    else:
        raw = raw.replace(b'needle', b'normal')
    path = root / 'mixed' / str(i) / source.relative_to(fixtures)
    path.parent.mkdir(parents=True, exist_ok=True); path.write_bytes(raw); meta['files'] += 1
for i in range(100000):
    # Spec #51: unclaimed names, plus the .json/.html/.txt ones that signal 4
    # consults (Telegram's globs and WhatsApp's `*WhatsApp*.txt`).
    d = root / 'unclaimed' / str(i // 1000)
    if i % 100 == 0:
        p = d / f'r{i}' / ('result.json', 'messages.html', 'notes.txt', 'chat.txt')[(i // 100) % 4]
    else:
        p = d / f'file-{i}.{("unclaimed", "json", "html", "conf", "txt")[i % 5]}'
    p.parent.mkdir(parents=True, exist_ok=True); p.write_bytes(b'ordinary content\n' * 40)
(root / 'metadata.json').write_text(json.dumps(meta))
```

Complete benchmark driver `scripts/bench-m6.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail
# One Python program: --self-test and the measurements decide with the same
# `decide` and `verdict` (plan r1, Codex #6).
[[ $# == 1 && ${1} == --self-test ]] ||
  [[ $# == 1 && -n ${SEARCHFOR_BIN:-} && -n ${SEARCHFOR_BASE_BIN:-} && -n ${SEARCHFOR_BENCH_REPORT:-} ]]
python3 -I - "$1" <<'PY'
import json, os, pathlib, statistics, subprocess, sys, tempfile, time
def decide(medians,limit,additive=0):
    # medians: one (s4, reference) pair of medians per series. An additive gate
    # bounds the median excess in seconds; any other gate the median ratio.
    ratio=statistics.median(a/b for a,b in medians)
    excess=statistics.median(a-b for a,b in medians)
    return ratio,excess,(excess<=additive if additive else ratio<=limit)
def verdict(gates): return 0 if gates and all(g['ok'] for g in gates) else 1
if sys.argv[1]=='--self-test':
    for limit in (1.02,1.03,1.05,1.1):
        assert decide([(limit,1)]*3,limit)[2]
        assert not decide([(limit+0.0001,1)]*3,limit)[2]
        # The median of three series decides, not the worst or the best one.
        assert decide([(limit+0.5,1),(limit,1),(1,1)],limit)[2]
        assert not decide([(limit+0.5,1),(limit+0.001,1),(1,1)],limit)[2]
    # Additive gates, with values exact in binary: the ratio does not count.
    for additive in (0.25,0.0625):
        assert decide([(1+additive,1)]*3,0,additive)[2]
        assert not decide([(1+additive+0.001,1)]*3,0,additive)[2]
        assert decide([(0.25+additive,0.25)]*3,0,additive)[2]
    assert verdict([{'ok':True},{'ok':True}])==0
    assert verdict([{'ok':True},{'ok':False}])==1 and verdict([])==1
    print('M6 cost gates: PASS')
    raise SystemExit(0)
root=pathlib.Path(sys.argv[1]).resolve()
s4=os.environ['SEARCHFOR_BIN']; base=os.environ['SEARCHFOR_BASE_BIN']; rg=os.environ.get('SEARCHFOR_RG','/usr/bin/rg')
assert subprocess.check_output([rg,'--version']).startswith(b'ripgrep 14.1.1')
meta=json.loads((root/'metadata.json').read_text()); report={'raw':[], 'gates':[], 'uptime':subprocess.check_output(['uptime']).decode()}
assert 0<meta['matching_files']/meta['files']<.05
threads=int(os.environ.get('SEARCHFOR_BENCH_THREADS',os.cpu_count() or 1))
def sample(tool,args,pattern,path):
    command=[tool,'--no-config','-j',str(threads),*args,pattern,str(path)]
    start=time.perf_counter()
    result=subprocess.run(command,stdout=subprocess.DEVNULL,stderr=subprocess.PIPE,env=env)
    elapsed=time.perf_counter()-start
    assert result.returncode==(0 if pattern=='M6MARK' else 1),(command,result.returncode,result.stderr)
    report['raw'].append({'command':command,'seconds':elapsed})
    return elapsed
def paired(label,a,b,pattern,path,limit,additive=0):
    medians=[]
    for series in range(3):
        samples=[[],[]]
        for warm in range(3):
            sample(a[0],a[1],pattern,path);sample(b[0],b[1],pattern,path)
        for pair in range(40):
            for side in ((0,1) if pair%2==0 else (1,0)):
                tool,args=(a,b)[side];samples[side].append(sample(tool,args,pattern,path))
        ta,tb=map(statistics.median,samples);medians.append((ta,tb))
    ratio,excess,ok=decide(medians,limit,additive)
    report['gates'].append({'name':label,'medians':medians,'ratio':ratio,'excess':excess,'limit':additive or limit,'ok':ok})
    print(label,ratio,excess,'PASS' if ok else 'FAIL',flush=True)
with tempfile.TemporaryDirectory() as iso:
    env=os.environ.copy();env.pop('RIPGREP_CONFIG_PATH',None);env.pop('SEARCHFOR_CONFIG_PATH',None)
    for var,leaf in [('SEARCHFOR_FORMATS_DIR','formats'),('SEARCHFOR_GRAMMARS_DIR','grammars')]:
        p=pathlib.Path(iso)/leaf;p.mkdir();env[var]=str(p)
    env['SEARCHFOR_GRAMMAR_INDEX']='file://'+str(pathlib.Path(iso)/'missing.json')
    raw=['--no-docs','--no-trail']; flat=['--no-docs','--flat']
    paired('mixed-no-match-no-trail',(s4,raw),(rg,[]),'ABSENT_M6',root/'mixed',1.1)
    paired('mixed-no-match-trail',(s4,flat),(rg,[]),'ABSENT_M6',root/'mixed',1.1)
    paired('mixed-sparse',(s4,flat),(rg,[]),'M6MARK',root/'mixed',0,
           meta['largest_matching_bytes']/50_000_000+meta['matching_bytes']/(50_000_000*threads)+.05)
    paired('unclaimed-detection',(s4,flat),(base,flat),'ABSENT_M6',root/'unclaimed',1.02)
    # Existing M4 text corpus must already exist under root/m4/text.
    m4=root/'m4/text';assert m4.is_dir()
    paired('m4-no-trail-rg',(s4,raw),(rg,[]),'ABSENT_M6',m4,1.05)
    paired('m4-trail-rg',(s4,['--no-docs']),(rg,[]),'ABSENT_M6',m4,1.05)
    paired('m4-default-no-docs',(s4,[]),(s4,['--no-docs']),'ABSENT_M6',m4,1.05)
    for flags in (raw,flat):
        paired('m4-vs-m5b-'+','.join(flags),(s4,flags),(base,flags),'ABSENT_M6',m4,1.03)
pathlib.Path(os.environ['SEARCHFOR_BENCH_REPORT']).write_text(json.dumps(report,indent=2))
raise SystemExit(verdict(report['gates']))
PY
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test perf m6_benchmark_gate_selftest -- --nocapture`.
  Expected RED: driver absent; after addition PASS. Its boundary tests (ratio,
  additive, median of three series, exit verdict) exercise the same `decide` and
  `verdict` the measurements call, so a regression in a real gate fails it; they
  verify the evaluator, not performance. Timing needs real binaries next.
- [ ] **Step 3:** copy the known M5b binary built from 59bb971 before M6
  changes into `/tmp/searchfor-m6-baseline/s4`, recording its SHA-256, build
  profile, toolchain and size. Reuse that artifact; do not checkout/build another
  target tree per chunk. Set SEARCHFOR_BASE_BIN to that absolute path. Generate
  corpora under a dedicated `/tmp` directory, run existing gen-m4-corpus there
  using its documented invocation, then run this generator. Record every sample,
  three alternated 40-pair series, median ratios and machine load. Extend mixed
  no-match measurement to decorated mode as well: no matches must skip every
  Processor. Validate selected path rules and real FORMAT labels with a separate
  small matched sample so no accidental Plain fallback earns a fast time.
- [ ] **Step 4:** run with the release binary and required env:
  `nice -n 19 ionice -c3 bash scripts/bench-m6.sh /tmp/searchfor-m6-corpus`.
  Expected nine gates PASS; preserve report even on failure. Local Linux timings
  decide #49/#51. Native CI runs the evaluator and structural path tests; shared
  runner time does not replace these measurements. A missing baseline or corpus
  is a failing prerequisite. No threshold is silently relaxed.
- [ ] **Step 5:** commit with
  `git commit -m "perf: measure M6 detection and per-file decoration cost"`.

## Chunk 12: Docs and 1.1.0

Starts after C10 and C11 integrate. Owns docs, skill, `tests/guide.rs`,
`tests/rg_compat_m6.rs`, the central app differential extension, CLI help/man
wording, the version checks of `.github/workflows/ci.yml` (C12.3, before the
orchestrator's CI inventory commit) and the root version. No release machinery.

### Task 12.1: format reference, bilingual docs and the .po guide

**Files:** modify `README.md`, `README.es.md`, `CONTEXT.md`,
`skills/searchfor/SKILL.md`, `docs/external-processors{,.es}.md`,
`src/cli/mod.rs::Args` definition help, `tests/guide.rs`,
`src/cli/rg_flags.rs::DEVIATIONS` and the `docs/rg-compat.md` it generates;
create `docs/formats.md`, `docs/formats.es.md`,
`docs/examples/{sample.po,po-helper.py,po-pre.py,po.toml}`;
remove superseded `docs/examples/{sample.ini,ini-helper.py,ini-pre.py,ini.toml}`
only after all guide links/tests use PO. `pre-launcher.rs` remains.

Definition interface for the worked example:

```toml
name = "po-guide"
extensions = ["po"]
processor = "external"
[params]
command = ["python3", "po-helper.py"]
mode = "oneshot"
```

- [ ] **Step 1 — RED:** use the complete synthetic sample below, encoded UTF-8
  with LF, as `docs/examples/sample.po`.

```po
msgctxt "menu"
msgid "Open"
msgstr "needle"
```

Complete `po-helper.py` example (protocol v1, zero third-party libraries):

```python
#!/usr/bin/env python3
import json, pathlib, sys

def value(start, line, number):
    # The quoted value of a one-line `keyword "..."`: its decoded text, and the
    # Source bytes between the quotes, which an escape makes longer than the text.
    body = line.rstrip(b'\r\n')
    first, last = body.index(b'"'), len(body) - 1
    return (json.loads(body[first:].decode('utf-8', 'replace')),
            start + first + 1, start + last, number)

def read(path):
    raw = pathlib.Path(path).read_bytes()
    offset = 0; context = None; msgid = None; records = []
    for number, line in enumerate(raw.splitlines(keepends=True), 1):
        if line.startswith(b'msgctxt "'): context = value(offset, line, number)
        elif line.startswith(b'msgid "'): msgid = value(offset, line, number)
        elif line.startswith(b'msgstr ') and msgid:
            records.append((offset, offset + len(line), number, context, msgid))
            context = msgid = None
        offset += len(line)
    return records

def segments(record):
    # Two Segments, so the Trail separator stays out of any name: the context
    # (when there is one) and the entry, each named by its source value.
    start, end, number, context, msgid = record
    named = [('context', context)] if context else []
    named.append(('entry', msgid))
    return [(kind, v, {'kind':kind, 'name':v[0], 'line':v[3], 'end_line':number})
            for kind, v in named]

def trail(record):
    return [segment for _, _, segment in segments(record)]

for line in sys.stdin:
    request = json.loads(line); ident = request.get('id'); op = request.get('op', 'trails')
    try:
        if op == 'hello': response = {'id':ident, 'protocol':1}
        elif op == 'names':
            result = []
            for record in read(request['path']):
                found = segments(record)
                for depth, (kind, (text, src_start, src_end, number), _) in enumerate(found):
                    # One piece over the value between the quotes, in bytes.
                    result.append({'text':text,
                                   'pieces':[[0, len(text.encode('utf-8')), src_start, src_end]],
                                   'anchor':src_start,
                                   'trail':[s for _, _, s in found[:depth + 1]]})
            response = {'id':ident, 'names':result}
        elif op == 'trails':
            records = read(request['path']); result = []
            for match in request['matches']:
                anchor = match['byte']
                result.append(next((trail(r) for r in records if r[0] <= anchor < r[1]), []))
            response = {'id':ident, 'trails':result}
        else: response = {'id':ident, 'error':'unknown operation'}
    except Exception as error: response = {'id':ident, 'error':str(error)}
    sys.stdout.buffer.write((json.dumps(response, ensure_ascii=False)+'\n').encode('utf-8'))
    sys.stdout.buffer.flush()
```

`po-pre.py` emits only translations; this example explicitly documents its small
one-line PO subset (a production helper must implement full PO multiline syntax):

```python
#!/usr/bin/env python3
import json, pathlib, sys
for line in pathlib.Path(sys.argv[1]).read_text(encoding='utf-8').splitlines():
    if line.startswith('msgstr '): print(json.loads(line[7:]))
```

Append this complete test to `tests/guide.rs` before changing the old examples:

```rust
#[test]
fn m6_po_guide_runs() {
    let Some(py)=python() else {panic!("Python 3 required for M6 guide acceptance")};
    let fx=Fixture::new();
    let root=Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/examples");
    std::fs::copy(root.join("sample.po"),fx.file("sample.po")).unwrap();
    let plain=run(&mut fx.s4(["--no-docs","--json","needle","sample.po"]));
    assert_eq!(code(&plain),0);
    for line in stdout(&plain).lines() {
        let value:serde_json::Value=serde_json::from_str(line).unwrap();
        if value["type"]=="match"{assert!(value["data"]["format"].is_null());}
    }
    let def=toml::toml! {
        name="po-guide"
        extensions=["po"]
        processor="external"
        [params]
        mode="oneshot"
    };
    let mut def=def;
    def.get_mut("params").unwrap().as_table_mut().unwrap().insert("command".into(),
        toml::Value::Array(vec![py.clone().into(),root.join("po-helper.py").to_string_lossy().into_owned().into()]));
    std::fs::write(fx.formats.path().join("po.toml"),toml::to_string(&def).unwrap()).unwrap();
    let described=run(&mut fx.s4(["--flat","needle","sample.po"]));
    assert_eq!(code(&described),0,"{}",stderr(&described));
    assert_eq!(stdout(&described),"3: menu › Open │ msgstr \"needle\"\n");
    // `names` maps each name to its value between the quotes: --in-headings finds
    // the entry's name, with its parent's Trail, and never the keyword.
    let named=run(&mut fx.s4(["--flat","--in-headings","Open","sample.po"]));
    assert_eq!(code(&named),0,"{}",stderr(&named));
    assert_eq!(stdout(&named),"2: menu │ msgid \"Open\"\n");
    let keyword=run(&mut fx.s4(["--flat","--in-headings","msgstr","sample.po"]));
    assert_eq!(code(&keyword),1,"{}",stdout(&keyword));
    let pre=Command::new(&py).arg(root.join("po-pre.py")).arg(fx.file("sample.po")).output().unwrap();
    assert!(pre.status.success());assert_eq!(pre.stdout,b"needle\n");
    let launcher=fx.file(&format!("po-pre-launcher{}",std::env::consts::EXE_SUFFIX));
    assert!(Command::new("rustc").arg("--edition=2024").arg(root.join("pre-launcher.rs"))
        .arg("-o").arg(&launcher).status().unwrap().success());
    let converted=run(fx.s4(["--no-docs","--no-trail","--pre"]).arg(launcher)
        .args(["needle","sample.po"]).env("SEARCHFOR_EXAMPLE_PYTHON",&py)
        .env("SEARCHFOR_EXAMPLE_CONVERTER",root.join("po-pre.py")));
    assert_eq!(code(&converted),0);assert_eq!(stdout(&converted),"needle\n");
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test guide m6_po_guide_runs -- --nocapture`.
  Expected RED: sample/helper absent, then guide Trail not yet implemented.
- [ ] **Step 3:** use the checked protocol `Request::Trails` Match field `byte` from
  `external.rs` (not a relative line offset).
  Move every old guide test/transcript to the PO sample, retaining names/error
  request-id tests. Existing INI override tests keep their custom helper behavior and
  need no edit, since none asserts the `user` origin (chunk graph); never rename
  these tests' user Definition to evade replacement behavior. Record HTML's
  script Trails (D24, spec §8.3 and §14) as the one change to an existing
  Format's output since 1.0: `docs/rg-compat.md` is generated, so append a
  section «Trails that changed since 1.0» to the end of `DEVIATIONS` in
  `src/cli/rg_flags.rs`, with the 1.0 and 1.1 line of a Match in a page's
  `onClick`, the three limits and the note that `--no-trail` and every flag row
  are unchanged; regenerate with
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --lib rg_compat_doc -- --ignored`
  and check `rg_compat_doc_is_current` passes. The HTML row of both READMEs and
  `docs/formats{,.es}.md` say it too. Add one Formats table
  listing all v1+M6 Formats with one example Trail each to both READMEs and move
  detail into the two new references. Document all limits, error recovery,
  name-only chat overrides, stdin diff use, syntax/view parameters and binary
  raw-byte behavior. Extend the skill description and list and show config,
  SQL row and diff examples. CONTEXT's Segment definition adds exactly the M6
  concept list in §14. CLI Definition help mentions paths/compound extensions;
  man output is generated by existing `--generate man`, not hand-maintained.
  New kinds add description text only to the JSON schema. Preserve free strings.
- [ ] **Step 4:** all guide tests GREEN on Linux; native CI requires Python3
  and runs helper/--pre on all four systems. Run existing CLI/helper tests for
  INI overrides. Review every English/Spanish example for identical semantics.
- [ ] **Step 5:** commit with
  `git commit -m "docs: document M6 Formats and move helper examples to gettext"`.
### Task 12.2: integrated rg, streaming and real-Definition acceptance

**Files:** create `tests/rg_compat_m6.rs`; extend
`src/app.rs::tests` beside `m5b_stream_matches_model_for_every_fixture_and_mode`;
modify `docs/specs/2026-10-16-searchfor-design.md` sections 2, 4.1, 4.2, 4.3, 5, 11;
modify `docs/rg-compat.md` only for actual changed deviations.

- [ ] **Step 1 — RED:** create the complete integration target below.

```rust
mod common;
use common::{Fixture,run,code,stdout};
use std::path::Path;
fn files(root: &Path,out: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(root).unwrap() {
        let entry=entry.unwrap();
        if entry.file_type().unwrap().is_dir(){files(&entry.path(),out)}else{out.push(entry.path());}
    }
}
#[test]
fn m6_rg_over_every_fixture() {
    if !common::reference_rg() {return;}
    let fx=Fixture::new();let mut paths=Vec::new();
    for group in ["docs-a","config","code-c","mail","chat"] {files(&fx.file(group),&mut paths);}
    use searchfor::processors::object::fixture::m6 as b;
    for (name,bytes) in [("jpeg",b::jpeg(b"needle")),("png",b::png(b"needle")),
        ("webp",b::webp(b"needle")),("tiff",b::tiff(b"needle")),("id3",b::id3(b"needle")),
        ("mp4",b::mp4(b"needle")),("wasm",b::wasm(b"needle")),("class",b::class(b"needle"))] {
        let path=fx.file(&format!("{name}.bin"));std::fs::write(&path,bytes).unwrap();paths.push(path);
    }
    assert!(paths.len()>=40);paths.sort();
    for path in paths {
        for flags in [vec![],vec!["--json"],vec!["-n"],vec!["-o"],vec!["-v"],
            vec!["-c"],vec!["-l"],vec!["-q"],vec!["-U"],vec!["--crlf"]] {
            let mut s=fx.s4(["--no-docs","--no-trail"]);s.args(&flags).arg("needle").arg(&path);
            let mut r=fx.rg(&flags);r.arg("needle").arg(&path);
            let (s,r)=(run(&mut s),run(&mut r));
            assert_eq!(code(&s),code(&r),"{path:?}/{flags:?}");
            assert_eq!(s.stderr,r.stderr,"{path:?}/{flags:?}");
            if flags.contains(&"--json") {
                assert_eq!(common::mask_json_times(&stdout(&s)),common::mask_json_times(&stdout(&r)));
            }else{assert_eq!(s.stdout,r.stdout,"{path:?}/{flags:?}");}
        }
    }
}
#[test]
fn m6_real_definitions_export_and_override() {
    use searchfor::{formats::Registry,processors::Processors};
    let (r,w)=Registry::load(None,&Processors::known);assert!(w.is_empty());
    let names=["asciidoc","rst","org","latex","jupyter","ini","systemd","properties",
        "env","ssh-config","nginx","apache","hcl","dockerfile","make","just","sql","diff",
        "vue","svelte","blade","graphql","protobuf","har","jpeg","png","webp","tiff","id3",
        "mp4","wasm","java-class","eml","mbox","icalendar","vcard","bibtex","whatsapp","telegram","telegram-html"];
    for name in names {assert!(r.get(name).is_some(),"{name}");}
    let fx=Fixture::new();let export=fx.file("exported");
    let out=run(fx.s4(["--export-formats"]).arg(&export));assert_eq!(code(&out),0);
    for name in names {assert!(export.join(format!("{name}.toml")).is_file());}
    std::fs::write(fx.formats.path().join("ini.toml"),"name='ini'\nextensions=['ini']\nprocessor='markdown'\n").unwrap();
    let out=run(&mut fx.s4(["--list-formats"]));assert_eq!(code(&out),0);
    assert!(stdout(&out).lines().any(|l|l.starts_with("ini ")&&l.contains("overridden")));
}
```

Complete new central differential test, inside the existing app test module:

```rust
#[test]
fn m6_stream_matches_model() {
    let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let cases=[("asciidoc","docs-a/m6.adoc"),("rst","docs-a/m6.rst"),
        ("org","docs-a/m6.org"),("latex","docs-a/m6.tex"),("jupyter","docs-a/m6.ipynb"),
        ("ini","config/m6.ini"),("systemd","config/m6.service"),
        ("properties","config/m6.properties"),("env","config/.env.m6"),
        ("ssh-config","config/ssh/ssh_config"),("nginx","config/nginx/nginx.conf"),
        ("apache","config/apache2/apache2.conf"),("hcl","config/m6.tf"),
        ("dockerfile","config/m6.dockerfile"),("make","config/m6.mk"),("just","config/m6.just"),
        ("sql","code-c/m6-dump.sql"),("diff","code-c/m6.patch"),("vue","code-c/m6.vue"),
        ("svelte","code-c/m6.svelte"),("blade","code-c/m6.blade.php"),
        ("graphql","code-c/m6.graphql"),("protobuf","code-c/m6.proto"),("har","code-c/m6.har"),
        ("eml","mail/m6.eml"),("mbox","mail/m6.mbox"),("icalendar","config/m6.ics"),
        ("vcard","config/m6.vcf"),("bibtex","docs-a/m6.bib"),("whatsapp","chat/_chat.txt"),
        ("telegram","chat/ChatExport_m6/result.json"),("telegram-html","chat/ChatExport_m6/messages.html"),
        // D24: HTML's scripts, JSON-LD and the text-typed script, streamed natively too.
        ("html","code-c/m6-scripts.html")];
    fn normalized(bytes: &[u8],json: bool)->Vec<u8>{
        if !json{return bytes.to_vec()}
        fn mask(v:&mut serde_json::Value){match v{
            serde_json::Value::Object(m)=>for(k,v)in m{if k=="elapsed"||k=="elapsed_total"{
                *v=serde_json::json!({"secs":0,"nanos":0,"human":"0s"});}else{mask(v)}},
            serde_json::Value::Array(a)=>for v in a{mask(v)},_=>{}}}
        let mut out=Vec::new();for l in bytes.split(|&b|b==b'\n').filter(|l|!l.is_empty()){
            let mut v=serde_json::from_slice(l).unwrap();mask(&mut v);
            serde_json::to_writer(&mut out,&v).unwrap();out.push(b'\n');}out
    }
    for (format,file) in cases {
        for mode in [vec!["--flat"],vec!["--tree"],vec!["--json"],vec!["--json","--expand"],
            vec!["--flat","--within",""],vec!["--tree","--expand","-C1"],
            vec!["--json","--in-headings"],vec!["--flat","--stats"],
            vec!["--json","--within","","-m1"],vec!["--flat","--expand","--within",""],
            vec!["--flat","-o"],vec!["--json","-U"],vec!["--flat","-r","X"]] {
            let mut args=vec!["--no-docs","--format",format];args.extend(&mode);args.push("needle");
            let a=compare_fixture_command(&root.join(file),&args,Pipeline::Model);
            let b=compare_fixture_command(&root.join(file),&args,Pipeline::Stream);
            assert_eq!(a.code,b.code,"{format}/{mode:?}");assert_eq!(a.stderr,b.stderr);
            let json=mode.contains(&"--json");assert_eq!(normalized(&a.stdout,json),normalized(&b.stdout,json));
            if !mode.contains(&"-o")&&!mode.contains(&"-r") {
                assert_eq!(b.described,0,"{format}/{mode:?}: model fallback");
            }
        }
    }
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test rg_compat_m6 -- --nocapture`
  and
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --lib m6_stream_matches_model -- --nocapture`.
  Expected RED: missing registration, output differences or a hidden model path.
- [ ] **Step 3:** compare the actual forty Definitions' signals/params with
  Task 1.3's table, including all aliases, magic offsets and empty binary extension
  lists, and Telegram's two globs and WhatsApp's `*WhatsApp*.txt` sorted into
  signal 4 (a real-Definition copy of C1.3's precedence cases). Extend central cases to broken fixtures via
  the supplemental manifest, retaining forced Format so renamed bad inputs work.
  Existing recursive M5b differential already sees new fixtures but uses
  unrelated within filters; this explicit matrix proves native use per family.
  Amend v1 §2 Formats, §4.1 order, §4.2 paths/compound syntax, §4.3 Processor
  rows, §5 pointer to the M6 spec and §11 criteria 43–56. State permissive
  scanner recoveries and any actual presentation deviation in rg-compat, without
  changing `--no-trail` guarantees or default documents. Every acceptance row
  below must point to executed evidence before the milestone is marked complete.
- [ ] **Step 4:** both commands GREEN; run existing rg compatibility targets,
  all snapshot tests and schema validation. Native CI runs these exact targets
  on four systems; compare timings only after masking existing timing fields.
- [ ] **Step 5:** commit with
  `git commit -m "test: integrate M6 rg compatibility and streaming acceptance"`.

### Task 12.3: CI's version checks read Cargo.toml

**Files:** modify `.github/workflows/ci.yml` (the `package` job only); append
`m6_ci_versions_come_from_cargo_toml` to `tests/cli_m5.rs`. Nothing is
published: the `package` job keeps building and checking without uploading, and
its only `cargo publish` stays `--dry-run` (D22, header «M6 publishes nothing»).

The `package` job writes `1.0.0` in nine checks (verified on `59bb971`): line
385 (`dist plan`), 488 (the two planned tags), 532 (archive smoke), 726, 745,
755, 762 and 774 (installers: shell, npm tarball, npm shim, Homebrew URL,
Homebrew) and 797 (the crate's file name). Bumping to 1.1.0 in C12.4 fails all
of them. After this task the version is written in `Cargo.toml` only.

- [ ] **Step 1 — RED:** append this complete test.

```rust
#[test]
fn m6_ci_versions_come_from_cargo_toml() {
    let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let ci=std::fs::read_to_string(root.join(".github/workflows/ci.yml")).unwrap();
    let version=env!("CARGO_PKG_VERSION");
    let written:Vec<String>=ci.lines().enumerate().filter(|(_,l)|l.contains(version))
        .map(|(i,l)|format!("{}: {}",i+1,l.trim())).collect();
    assert!(written.is_empty(),"ci.yml writes the version instead of reading it: {written:#?}");
    // One place reads it, and every check uses what it read.
    assert_eq!(ci.matches("tomllib.load(open('Cargo.toml', 'rb'))['package']['version']").count(),1);
    for read in ["assert os.environ['VERSION'] in text",
        "rc, stable = plan(f'v{version}-rc.1'), plan(f'v{version}')",
        "assert v == f\"s4 {os.environ['VERSION']}\", v",
        "tgz = copy / f\"searchfor-{os.environ['VERSION']}.tgz\"",
        "releases/download/v{os.environ['VERSION']}/",
        "ls -l \"target/package/searchfor-$VERSION.crate\""] {
        assert!(ci.contains(read),"missing: {read}");
    }
    assert_eq!(ci.matches("== want").count(),3,"the three installer version checks");
    // The no-publish rule: the one command that runs `cargo publish` is a dry run.
    let publish:Vec<&str>=ci.lines().map(|l|l.trim().trim_start_matches("$PRIO "))
        .filter(|l|l.starts_with("cargo publish")).collect();
    assert_eq!(publish.len(),1,"{publish:?}");
    assert!(publish[0].contains("--dry-run"),"{}",publish[0]);
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test cli_m5 m6_ci_versions_come_from_cargo_toml -- --nocapture`.
  Expected RED: the nine `1.0.0` lines, listed with their numbers.
- [ ] **Step 3:** in the `package` job, right after `actions/setup-python`
  (3.12, whose `tomllib` reads TOML), add the one step that reads the version:

```yaml
      # The one place the checks below take the version from: Cargo.toml's.
      - name: Version from Cargo.toml
        run: |
          python3 - >> "$GITHUB_ENV" <<'PY'
          import tomllib
          print('VERSION=' + tomllib.load(open('Cargo.toml', 'rb'))['package']['version'])
          PY
```

  Then replace each written version: in `dist plan`, import `os` and
  `assert os.environ['VERSION'] in text`; in `Release workflow`, set
  `version = os.environ['VERSION']` before
  `rc, stable = plan(f'v{version}-rc.1'), plan(f'v{version}')`; in
  `Archive smoke`, `assert v == f"s4 {os.environ['VERSION']}", v`; in `Local
  installers`, `want = f"s4 {os.environ['VERSION']}"` after `def version`,
  the three `version(...) == want` checks,
  `tgz = copy / f"searchfor-{os.environ['VERSION']}.tgz"` and
  `release = f"https://github.com/neitanod/searchfor/releases/download/v{os.environ['VERSION']}/"`;
  in `Crate (Linux)`, `ls -l "target/package/searchfor-$VERSION.crate"`. The
  three comments that name 1.0.0 (lines 357, 425, 665) say «the Cargo.toml
  version» instead. Change nothing else in the workflow: the tag pattern, the
  patches, the permissions, the guards and the dry run stay as they are.
- [ ] **Step 4:** rerun the command: PASS. Locally check the reading step's
  script with
  `python3 -c "import tomllib; print('VERSION=' + tomllib.load(open('Cargo.toml', 'rb'))['package']['version'])"`
  (prints `VERSION=1.0.0` before C12.4) and the edited heredocs with
  `python3 -m py_compile` on each extracted script. The `package` job runs only
  in native CI, which is its evidence; record it as pending until then.
- [ ] **Step 5:** commit with `git add -- .github/workflows/ci.yml tests/cli_m5.rs` and
  `git commit -m "ci: read the version the package checks expect from Cargo.toml"`.

### Task 12.4: version 1.1.0 and final local verification

**Files:** modify only the version in `Cargo.toml` and, with it, the root
package's own entry of `Cargo.lock` (D20, Step 3; no dependency update); update
`tests/cli_m5.rs::m5_26_the_version_is_1_0` to the M6 assertion below, renaming it
`m6_the_version_is_1_1`. Add the measured size to the implementation handoff
and the existing criterion-22 record in v1 spec §11.

- [ ] **Step 1 — RED:** replace the existing version test with this code.

```rust
#[test]
fn m6_the_version_is_1_1() {
    let fx=Fixture::new();
    let out=run(&mut fx.s4(["--version"]));
    assert_eq!(code(&out),0);assert_eq!(stdout(&out),"s4 1.1.0\n");
    assert!(out.stderr.is_empty());
}
```

- [ ] **Step 2:** run
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo test --offline --locked --test cli_m5 m6_the_version_is_1_1 -- --nocapture`.
  Expected RED: `s4 1.0.0` versus `s4 1.1.0`.
- [ ] **Step 3:** change Cargo.toml's package.version to 1.1.0, leaving helper,
  npm, dist and release versions alone. Cargo.lock still records
  `searchfor 1.0.0`, so every `--locked` command is now refused for certain
  («the lock file … needs to be updated but --locked was passed to prevent
  this»); CI runs every cargo command with `--locked`, so refreshing the root
  package's own entry is the only way compatible with it (D20). Refresh exactly
  that entry, offline:
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo update --offline --workspace`
  (it prints `Updating searchfor v1.0.0 (…) -> v1.1.0` and locks no other
  package). Then check that no other crate moved:
  `git diff --numstat -- Cargo.lock` reports one added and one deleted line, and
  `git diff -U0 -- Cargo.lock` shows only `-version = "1.0.0"` /
  `+version = "1.1.0"` under `name = "searchfor"`. Any other changed line
  (a dependency, a helper crate, a checksum) stops the task: restore the lock
  with `git checkout -- Cargo.lock` and report it. Then run the locked command
  of Step 2: GREEN.
- [ ] **Step 4:** run the complete inherited fmt/clippy/native+GNU clippy/suite,
  C10/C11 explicitly ignored checks, the release build and stripped size command.
  Record Linux byte size and compare both ceilings; a >22.8 MB measurement stops
  before CI handoff. The orchestrator then runs native CI; #56 remains pending
  until all four systems pass at the same integrated commit. Review git diff to
  verify only intended M6 work and documentation changes. This authoring task
  itself leaves only this plan as a new file.
- [ ] **Step 5:** stage with
  `git add -- Cargo.toml Cargo.lock tests/cli_m5.rs docs/specs/2026-10-16-searchfor-design.md`
  and commit with
  `git commit -m "chore: prepare searchfor 1.1.0 after M6 verification"`.
## Supplemental executable matrices (part of their owning tasks)

These tests close the cross-cutting requirements of §3/§12/§13. They are included
in the task counts above. Each owning chunk implements its rows before GREEN;
the table is centralized here to make broken-input policy and fixture bytes
reviewable together. These are complete tests, not a request to design more tests.

### Committed broken fixtures, with three snapshots each

Copy this test into each of the eight topic files for C2–C9. Set `owned` to its
listed array, and retain only its rows in the local table. Create each named
`tests/fixtures/<file>` with the literal bytes below before running RED. This
explicit manifest supersedes earlier prose that generically called a sibling
`broken-<basename>`; these are the exact filenames and expected Trails.
The files intentionally have no automatic-detection requirement: corruption
and renaming are tested under the owner's `--format`. Primary fixtures still
run automatic variants, Telegram's included.

| Topic | `owned` array |
|---|---|
| keypath | `["ini","systemd","properties","env","ssh-config","icalendar","vcard","bibtex"]` |
| blocks | `["nginx","apache","hcl","graphql","protobuf"]` |
| buildfile | `["dockerfile","make","just"]` |
| headings | `["asciidoc","rst","org","latex"]` |
| views | `["jupyter","har","telegram"]` |
| markup | `["vue","svelte","blade","telegram-html","html"]` |
| sql_diff | `["sql","diff"]` |
| mail_chat | `["eml","mbox","whatsapp"]` |

```rust
#[test]
fn m6_broken_fixtures() {
    // In each topic file use exactly its owned array from the table above.
    let owned: &[&str]=&["ini","systemd","properties","env","ssh-config","icalendar","vcard","bibtex"];
    let cases: &[(&str,&str,&[u8],&[&str])]=&[
        ("ini","config/broken-m6.ini",b"[s]\nk=needle\n[broken\nneedle\n",&["s › k","s › k"]),
        ("systemd","config/broken-m6.service",b"[Service]\nExecStart=needle\\\n needle\n",&["Service › ExecStart","Service › ExecStart"]),
        ("properties","config/broken-m6.properties",b"k=needle\nbad\\uQQQQ=needle\n",&["k","bad\\uQQQQ"]),
        ("env","config/broken-env.m6",b"A=needle\nB=\"needle\nneedle\n",&["A","B","B"]),
        ("ssh-config","config/ssh/broken-ssh_config",b"Host x\nK needle\nUnknown needle\n",&["Host x › K","Host x › Unknown"]),
        ("icalendar","config/broken-m6.ics",b"BEGIN:VEVENT\nDESCRIPTION:needle\nEND:UNKNOWN\nSUMMARY:One\n",&["VEVENT One › DESCRIPTION"]),
        ("vcard","config/broken-m6.vcf",b"BEGIN:VCARD\nEMAIL:needle\nFN:Ana\n",&["VCARD Ana › EMAIL"]),
        ("bibtex","docs-a/broken-m6.bib",b"@article{a,\n title={needle\n@book{b, title={needle}}\n",&["a › title","b › title"]),
        ("nginx","config/nginx/broken-nginx.conf",b"http { k needle; }\n}\nk needle;\n",&["http › k",""]),
        ("apache","config/apache2/broken-apache.conf",b"<Directory /x>\nK needle\n</unknown>\nK needle\n",&["Directory /x › K","Directory /x › K"]),
        ("hcl","config/broken-m6.tf",b"x { k=\"needle\" }\n}\nneedle\n",&["x › k",""]),
        ("graphql","code-c/broken-m6.graphql",b"query A { x(needle:true) }\n}\nneedle\n",&["A › x",""]),
        ("protobuf","code-c/broken-m6.proto",b"message A { string x=1; // needle\n}\n}\nneedle\n",&["A › x",""]),
        ("dockerfile","config/broken-m6.dockerfile",b"FROM alpine AS build\nRUN <<END\nneedle\nFROM needle\n",&["build › RUN","build › RUN"]),
        ("make","config/broken-m6.mk",b"a:\n\techo needle\nendif\nendef\nneedle\n",&["a",""]),
        ("just","config/broken-m6.just",b"a:\n    needle\nstray needle\n",&["a",""]),
        ("asciidoc","docs-a/broken-m6.adoc",b"= T\n== A\n....\nneedle\n== needle\n",&["T › A › literal","T › A › literal"]),
        ("rst","docs-a/broken-m6.rst",b"A\n=\n\n.. code:: bash\n\n   needle\n",&["A › bash"]),
        ("org","docs-a/broken-m6.org",b"* A\n#+BEGIN_SRC\nneedle\n* needle\n",&["A › src","A › src"]),
        ("latex","docs-a/broken-m6.tex",b"\\section{A}\n\\begin{verbatim}\nneedle\n\\section{needle}\n",&["A › verbatim","A › verbatim"]),
        ("jupyter","docs-a/broken-m6.ipynb",b"{\"cells\":[{\"cell_type\":\"code\",\"source\":[\"needle\"]}]}\nnope\nneedle\n",&["cell 1",""]),
        ("har","code-c/broken-m6.har",b"{\"log\":{\"entries\":[{\"request\":{\"method\":\"GET\",\"url\":\"http://x\"},\"x\":\"needle\"}]}}\nnope\nneedle\n",&["GET x › x",""]),
        ("telegram","chat/ChatExport_m6/broken-result.json",b"{\"name\":\"Ana\",\"messages\":[{\"date\":\"2024\",\"from\":\"Ana\",\"text\":\"needle\"}]}\nnope\nneedle\n",&["Ana › 2024 Ana › text",""]),
        ("vue","code-c/broken-m6.vue",b"<script lang='coffee'>\nneedle\n",&["script"]),
        ("svelte","code-c/broken-m6.svelte",b"{#if x}\n<div>needle\n",&["#if x › div"]),
        ("blade","code-c/broken-m6.blade.php",b"@if($x)\n<div>needle\n",&["@if($x) › div"]),
        ("html","code-c/broken-m6-scripts.html",b"<script type=\"application/ld+json\">\n{\"a\":\"needle\",\n</script>\nneedle\n",&["script › a",""]),
        ("telegram-html","chat/ChatExport_m6/broken-messages.html",b"<div class='message'><div class='date' title='2024'>x</div><div class='from_name'>Ana</div><div class='text'>needle\n",&["2024 Ana"]),
        ("sql","code-c/broken-m6.sql",b"SELECT 'needle';\nSELECT 'open\nneedle\n",&["SELECT",""]),
        ("diff","code-c/broken-m6.patch",b"--- a/x\n+++ b/x\n@@ -1,99 +2,99 @@\n+needle\n--- a/y\n+++ b/y\n@@ -1 +3 @@\n+needle\n",&["x › +2","y › +3"]),
        ("eml","mail/broken-m6.eml",b"Subject: One\nX: needle\nnot a header needle\n",&["X","body"]),
        ("mbox","mail/broken-m6.mbox",b"From a x\nSubject: One\n\nneedle\n>From needle\n",&["One › body","One › body"]),
        ("whatsapp","chat/broken-_chat.txt",b"31/12/23, 21:41 - Ana: needle\nnot a date needle\n",&["31/12/23 › 21:41 Ana","31/12/23 › 21:41 Ana"]),
    ];
    let fx=Fixture::new();let mut count=0;
    for &(format,file,src,want) in cases {
        if !owned.contains(&format){continue;}
        seed(&fx,file,src);check(&fx,format,file,want);count+=1;
    }
    assert_eq!(count,owned.len());
}
```

The second SQL and MIME primary fixtures share their Format's malformed case:
criterion 45 requires one broken fixture per Format, not one per example file.
A permissive scanner's broken fixture deliberately retains structure: #45 is
read with each Format's explicit recovery rule, rather than forcing all tails
empty even in HTML, INI, headings or unfinished MIME.

### Definition-kind and template keyword completeness (C3.3 and C7.2)

Append this complete test to `tests/cli_m6_blocks.rs`:

```rust
#[test]
fn m6_schema_keyword_kinds() {
    let fx=Fixture::new();
    let graphql=[
        ("type","type A { x: String # needle\n}\n","A › x"),
        ("interface","interface A { x: String # needle\n}\n","A › x"),
        ("input","input A { x: String # needle\n}\n","A › x"),
        ("enum","enum A { X # needle\n}\n","A › X"),
        ("union","union A = B | C # needle\n","A"),
        ("scalar","scalar A # needle\n","A"),
        ("schema","schema { query: A # needle\n}\n","schema › query"),
        ("directive","directive @a on FIELD # needle\n","a"),
        ("extend","extend type A { x: String # needle\n}\n","A › x"),
        ("query","query A { x(needle:true) }\n","A › x"),
        ("mutation","mutation A { x(needle:true) }\n","A › x"),
        ("subscription","subscription A { x(needle:true) }\n","A › x"),
        ("fragment","fragment A on B { x(needle:true) }\n","A › x"),
    ];
    for (kind,src,want) in graphql {
        put(&fx,"keyword.gql",src.as_bytes());
        let rows=matches(&output(&fx,"graphql","keyword.gql","needle",&["--json"]));
        assert_eq!(paths(&rows),[want]);assert_eq!(rows[0]["trail"][0]["kind"],kind);
    }
    for (kind,src,want) in [
        ("message","message A { string x=1; // needle\n}\n","A › x"),
        ("enum","enum A { X=0; // needle\n}\n","A › X"),
        ("service","service A { // needle\n}\n","A"),
        ("rpc","service A { rpc B (X) returns (Y) { // needle\n} }\n","A › B"),
        ("oneof","message A { oneof B { string x=1; // needle\n} }\n","A › B › x"),
        ("extend","extend A { string x=1; // needle\n}\n","A › x"),
    ] {
        put(&fx,"keyword.proto",src.as_bytes());
        let rows=matches(&output(&fx,"protobuf","keyword.proto","needle",&["--json"]));
        assert_eq!(paths(&rows),[want]);
        assert!(rows[0]["trail"].as_array().unwrap().iter().any(|s|s["kind"]==kind));
    }
    put(&fx,"strings.gql",b"query A { alias: user(arg: \"\"\"needle } {\"\"\") { id } }\n");
    assert_eq!(paths(&matches(&output(&fx,"graphql","strings.gql","needle",&["--json"]))),["A › alias"]);
}
```

Append this test to `tests/cli_m6_markup.rs`:

```rust
#[test]
fn m6_template_keyword_pairs() {
    let fx=Fixture::new();
    for name in ["if","each","await","key","snippet"] {
        let src=format!("{{#{name} x}}\n<p>needle</p>\n{{/{name}}}\nneedle\n");
        put(&fx,"all.svelte",src.as_bytes());
        assert_eq!(paths(&matches(&output(&fx,"svelte","all.svelte","needle",&["--json"]))),
            [format!("#{name} x › p"),String::new()]);
    }
    for name in ["section","if","foreach","forelse","for","while","switch","push",
        "component","slot","php","once","auth","guest","isset","empty","unless","verbatim"] {
        let argument=if ["php","once","auth","guest","verbatim"].contains(&name){""}else{"('x')"};
        let src=format!("@{name}{argument}\nneedle\n@end{name}\nneedle\n");
        put(&fx,"all.blade.php",src.as_bytes());
        assert_eq!(paths(&matches(&output(&fx,"blade","all.blade.php","needle",&["--json"]))),
            [format!("@{name}{argument}"),String::new()]);
    }
    for closer in ["show","stop","overwrite"] {
        let src=format!("@section('x')\nneedle\n@{closer}\nneedle\n");
        put(&fx,"section.blade.php",src.as_bytes());
        assert_eq!(paths(&matches(&output(&fx,"blade","section.blade.php","needle",&["--json"]))),
            ["@section('x')",""]);
    }
}
```

### Meaningful generated-name exclusion and encoding tests (C2–C9)

`flags`'s generated-name argument is optional: use the empty string where that
Format's tested fixture has no generated text. Only run its negative assertion
when the string is nonempty. The SQL row, Docker stage, notebook cell and HAR/MIME
prefix tests below carry the actual generated-name evidence. Source-only Formats
still test own-name parent Trails and source name search.

Append this test to the indicated owner file for its rows, using its prelude.
The whole table is shown to make every literal and expected status executable.

```rust
#[test]
fn m6_real_generated_names() {
    let fx=Fixture::new();
    let owned: &[&str]=&["jupyter","har"]; // views; use this topic's formats for other owners
    let cases: &[(&str,&[u8],&str)]=&[
        ("dockerfile",b"FROM alpine\nRUN needle\n","^stage [0-9]+$"),
        ("sql",b"INSERT INTO t VALUES ('needle');\n","^row [0-9]+$|^column [0-9]+$"),
        ("jupyter",b"{\"cells\":[{\"cell_type\":\"code\",\"source\":[\"needle\"]}]}","^cell [0-9]+$"),
        ("har",b"{\"log\":{\"entries\":[{\"request\":{\"method\":\"GET\",\"url\":\"http://x\",\"headers\":[{\"name\":\"Auth\",\"value\":\"needle\"}]}}]}}","^header "),
        ("eml",b"Content-Type: multipart/mixed; boundary=X\n\n--X\nContent-Type: text/plain\n\nneedle\n--X--\n","^part [0-9]+ "),
        ("vue",b"<ul><li>x</li><li>needle</li></ul>","\\[2\\]"),
    ];
    for &(format,src,generated) in cases {
        if !owned.contains(&format){continue;}
        put(&fx,"generated.fixture",src);
        assert_eq!(code(&output(&fx,format,"generated.fixture",generated,&["--json","--in-headings"])),1);
    }
}
#[test]
fn m6_encoding_and_lone_cr() {
    let fx=Fixture::new();
    // Copy into each owner and select one representative format/valid source:
    // keypath: ini, blocks: nginx, buildfile: make, headings: asciidoc,
    // views: har, markup: vue, sql_diff: sql, mail_chat: eml.
    let (format,src)=("ini","[Café]\nk=needle\n");
    put(&fx,"encoding.fixture",src.as_bytes());
    let lf=paths(&matches(&output(&fx,format,"encoding.fixture","needle",&["--json"])));
    let utf16:Vec<u8>=[vec![0xff,0xfe],src.encode_utf16().flat_map(u16::to_le_bytes).collect()].concat();
    put(&fx,"utf16.fixture",&utf16);
    assert_eq!(paths(&matches(&output(&fx,format,"utf16.fixture","needle",&["--json"]))),lf);
    put(&fx,"bom.fixture",&[b"\xef\xbb\xbf".as_slice(),src.as_bytes()].concat());
    assert_eq!(paths(&matches(&output(&fx,format,"bom.fixture","needle",&["--json"]))),lf);
}
```

The concrete representative source literals replacing the INI tuple are:
nginx `http { k needle; }\n`, make `KEY=needle\n`, AsciiDoc
`= Café\n== A\nneedle\n`, HAR
`{"log":{"entries":[{"request":{"method":"GET","url":"http://x"},"x":"needle"}]}}\n`,
Vue `<main>needle</main>\n`, SQL `SELECT 'needle';\n`, EML `Subject: Café\n\nneedle\n`.
Use these exact valid sources. Add lone CR to each source's content value before
needle and assert the Match's line_number equals the original; compare Trail
names while stripping the inserted CR if the CR lies in a name. CRLF equivalence
for every primary/broken fixture is already checked by `check`.

### Hostile depth, name bounds and own-label ownership (C11.1)

Append the complete test below to `tests/fuzz_m6.rs`:

```rust
#[test]
fn m6_depth_and_names() {
    let (r,w)=Registry::load(None,&Processors::known);assert!(w.is_empty());
    for (format,open,close) in [("nginx","x {","}"),("hcl","x {","}"),
        ("protobuf","message X {","}"),("vue","<x>","</x>"),
        ("svelte","{#if x}","{/if}"),("blade","@if($x)\n","@endif\n"),
        ("latex","\\begin{x}\n","\\end{x}\n"),("icalendar","BEGIN:X\n","END:X\n")] {
        let src=format!("{}needle{}",open.repeat(200000),close.repeat(200000));
        let def=r.get(format).unwrap();let p=processor(format);
        let input=Input{bytes:src.as_bytes(),path:Path::new("depth"),format,disk:None};
        let at=src.find("needle").unwrap();let prep=p.prepare(input,&def.params).unwrap();
        let t=prep.cursor().trail_at(at).unwrap();assert!(t.len()<=256);
    }
    for (name,params,src) in targets() {
        let p=processor(&name);let input=Input{bytes:&src,path:Path::new("names"),format:&name,disk:None};
        for n in p.names(&input,&params).unwrap() {
            for (text,source) in n.pieces {
                assert!(text.end<=n.text.len()&&source.end<=src.len());
            }
            for s in &n.trail {assert!(s.name.chars().count()<=60,"{name}: {}",s.name);}
        }
    }
}
```

Deep inputs require the same 2 s child supervision as mutations; implement the
loop as child-selected cases under `m6_mutation_child`, keeping a single anchor
rather than allocating expected Trails for every byte of a multi-megabyte depth
fixture. Add the hostile long-name source by replacing each primary fixture's
first source name with exactly 1 MiB of `a`, followed by `\x1b[31m` and invalid
UTF-8 `\xff`; library names use U+FFFD and bounded visible characters, tree/flat
snapshots escape control bytes in names and JSON retains decoded values. Search
still sees original bytes. The generated fixture size and mutation are exact;
no third-party content is involved.

### Binary boundary edits and large generated fixtures (C10.5)

For each writer's small valid seed, generate a broken variant by these edits,
then run it through `m6_binary_mutation_case` and the CLI within 2 s. Also save
three snapshots of its tree/flat/JSON fallback output using format-specific
snapshot names `m6_broken_<layout>_<layout-name>`. Compare invalid-layout output
with rg; inner XMP recovery is already asserted separately in Task 10.2.

| Layout | Exact hostile edit | Large valid fixture generator |
|---|---|---|
| TIFF | bytes 4..8 = `ff ff ff ff`; separately IFD next-pointer at 22..26 = 8 LE | `tiff(text)` with text 50,000,000 bytes; tag count/offset writer handles it |
| JPEG | bytes 4..6 = `ff ff`; cut before payload's end | Repeat APP1 Exif segments, each `jpeg(needle+59,000 a)` without SOI/EOI, between one SOI/EOI until ≥50 MB |
| PNG | bytes 8..12 = `ff ff ff ff` | Repeat complete tEXt chunks from png writer, each ≤60 KB, between signature/IHDR and IEND until ≥50 MB; CRC each chunk |
| WebP | bytes 16..20 = `ff ff ff ff` | Repeat EXIF chunks from webp writer, update RIFF size, preserve even padding, until ≥50 MB |
| ID3 | bytes 6..10 = `7f 7f 7f 7f`; separately frame size max syncsafe | Repeat TIT2 frames of 60 KB, update tag syncsafe size, ≤256 MiB |
| MP4 | first moov box size = 1 then missing 64-bit size; separately zero-sized child with siblings | Add bounded mdat of 50 MB after valid ftyp/moov; metadata remains readable |
| Wasm | after id 1, encode length as `ff ff ff ff 7f` | One data segment of 50 MB via `wasm(text)`; section/data LEB sizes update |
| Java | cp_count bytes 8..10 = `ff ff`; separately truncate Code operand | Append one class attribute with a declared 50 MB payload after updating class attributes_count; UTF8 constant lengths stay ≤65,535 |

Declared length/count maximums must fail quickly without preallocating the
claimed memory. Binary tests report at least 100 MB/s for the complete supplied
large files as well as the small-file repeated-call test. CLI's Source reading
and map construction are measured separately; each report says which it times.
C10 also runs its `-m`, per-submatch, replacement and expansion regressions with
legacy M4 tests so a new layout cannot change the row engine's semantics.

## CI on the four systems — orchestrator stage

This is an orchestrator stage after tanda 4, **not a chunk**. Chunk agents
commit and report locally; they never push. A GREEN run authorizes no tag,
release, `cargo publish` or package (header, «M6 publishes nothing»).

- [ ] Merge the four tandas, check that every file outside the registration
  points has one owner (chunk graph above), and rerun the full Linux checks of
  the common command convention, the Windows GNU all-targets clippy, the
  explicitly ignored C10/C11 tests and C11.3's benchmark at the integrated
  commit. Record the stripped Linux size; above 22,800,000 bytes, stop here.
- [ ] Before the first push, commit the CI inventory the chunks leave open, with
  an explicit file list: in `.github/workflows/ci.yml`, a `Short M6 fuzz (Linux)`
  step running `--test fuzz_m6` with `SEARCHFOR_FUZZ_SECONDS: '120'` and the
  artifact directory, beside the `fuzz` and `fuzz_m5` steps; in
  `.github/workflows/fuzz-nightly.yml`, `--test fuzz_m6` in the `--no-run` build
  and its own run step; in `docs/platforms.md` «Tests excluded by `cfg`», one
  row per `#[cfg(target_os="linux")]` M6 test (`m6_dense`, `m6_sparse`,
  `m6_first_unit` and any other C11.2 adds), excluded on `macos-arm64`,
  `macos-x86_64`, `windows` for `/usr/bin/time -v` peak RSS, owner C11. The
  inventory step only checks rows that exist, so a missing row passes silently;
  verify each row against `cargo test -- --list` on Linux before pushing.
  C12.3 already made the `package` job's version checks read Cargo.toml; this
  commit leaves them alone.
- [ ] Push only the task branch when the orchestrator authorizes it:
  `git push origin t-798-searchfor-m6`. No tag, no `workflow_dispatch` of
  `release.yml`.
- [ ] Read the run: `gh run list --branch t-798-searchfor-m6 --workflow ci`, then
  `gh run view <id>` and `gh run view <id> --log-failed`. The four `build` jobs
  are linux (ubuntu-24.04), macos-arm64 (macos-15), macos-x86_64
  (macos-15-intel) and windows (windows-2022, MSVC); the `package` job builds
  without publishing. Record per job its URL, rg reference version, the
  «Tests that did not run on this system» summary line (listed, ignored,
  selected, skipped, excluded by cfg) and the duration of the Linux «Memory and
  bomb checks» step, which now also runs C11.2's 50 MiB and 1 GiB tests.
- [ ] Fix in batches by owner. For each failure: write the red regression first,
  on the failing system or as a portable reduced equivalent that fails on Linux
  too; implement; rerun the full local Linux suite and the Windows GNU clippy;
  commit the batch with an explicit file list; push once per batch, never one
  push per fix. Do not accept a `cfg` skip for an ordinary path, line-ending or
  file-name portability problem; a `cfg` exclusion needs a system API that is
  absent, and a `docs/platforms.md` row.
- [ ] Anticipate the failures deduced from the static reading below (none has
  been observed yet) and reproduce each before calling it fixed:
  - **Windows, path globs:** C1.2's literal tokens taken by splitting the path's
    bytes on `/` miss every bucket for `C:\Users\x\.ssh\config`, so
    `m6_path_separators_are_native` and the automatic variants of `ssh/`,
    `nginx/`, `apache2/` and `ChatExport_m6/` fixtures fall back to Plain or JSON.
    Tokens come from `Path::components()`; the GlobMatcher keeps globset's own
    normalization.
  - **Windows, printed paths:** a snapshot of a walked directory shows `\`.
    Walked-directory outputs are compared with rg (C10.5's `m6-binaries`,
    C12.2), never snapshotted; a snapshot that names a walked path is the bug.
  - **Windows, helper programs:** `m6_benchmark_gate_selftest` starts `bash`,
    which may resolve to `System32\bash.exe` (WSL) before Git's bash, and the
    script calls `python3`; pin Git's bash through the runner's PATH order or
    skip only the shell launch with a `docs/platforms.md` row, keeping the
    Python evaluator's boundary checks running.
  - **Windows and macOS, case-insensitive file systems:** a test that `put`s
    `Makefile` and `makefile`, `Justfile` and `justfile`, or `Dockerfile` and
    `dockerfile` into one temporary directory overwrites the first; give each
    spelling its own directory. C1's tests use path strings and are unaffected.
  - **Windows, file names:** fixture or `put` names with `:`, `*`, `?`, `"`,
    `<`, `>`, `|` or a reserved stem (`con`, `aux`, `nul`, `prn`, `com1`) fail to
    be created. The planned names (`WhatsApp Chat with Ana.txt`,
    `Chat de WhatsApp con Ana.txt`, `WhatsApp notes.txt`, `_chat.txt`, `.env.m6`)
    are valid; check any added during fixes.
  - **Both, the 2 s case deadline:** fuzz children and hostile-depth cases
    (C10.5, C11.1) run in release mode in CI; macos-15-intel and windows-2022
    start processes more slowly. A timeout is first reproduced locally under
    `taskset -c 0` before any budget change, which only Sebastián may approve.
  - **macOS, `bash` 3.2:** `/bin/bash` there lacks bash-4 features;
    `scripts/bench-m6.sh` must keep to `[[ ]]`, arrays and here-documents.
  - **Both, binary rg equality:** C10.5's `-v`, `-c`, `-l`, `-q` comparisons over
    walked JPEG, MP3 and class files rest on rg's binary detection, whose
    macOS mmap semantics M5b's plan lists as a CI-only risk; a difference in `--json`
    statistics is compared after the existing masking, never by relaxing it.
- [ ] Rerun until the four `build` jobs and `package` pass at one commit. Then
  #56 is met; record the commit, the four job URLs and the Linux size in the
  acceptance table's evidence and in v1 spec §11's criterion-22 record.
- [ ] Report GREEN or the remaining blockers to Sebastián; both open
  questions are answered (notes 2 and 3). The branch push is not approval to tag 1.1.0.

## Acceptance criteria 43–56 and spec coverage

| Criterion | Required evidence | Tasks |
|---|---|---|
| 43 Detection | Every §4.2 row by name, extension, path and magic; compound over PHP; signal 4 Telegram paths and the JSON/HTML elsewhere; WhatsApp's `*WhatsApp*.txt` in two languages and a plain `.txt` unclaimed; `Makefile.toml` TOML; `.conf` Plain; native `\` paths; `cafebabe` both ways; real Definitions | 1.1, 1.2, 1.3, 9.3, 10.4, 12.2; Windows/macOS in orchestrator CI |
| 44 Every Format | Primary fixture, tree/flat/JSON snapshots, each example Trail of §6–§10, automatic variant; `--no-trail` and `--json --no-trail` equal rg over every fixture | 2.1–2.4, 3.1–3.3, 4.1–4.3, 5.1–5.3, 6.1–6.3, 7.1–7.3, 8.1–8.3, 9.1–9.3, 10.1–10.4; 12.2 `rg_compat_m6` |
| 45 Rules of §3 | Committed broken fixture per Format with three snapshots; `flags` source-name and generated-name searches; own-name parent Trail; encoding, BOM, UTF-16 and lone CR | Supplemental matrices in C2–C9; 10.5; 11.1 `m6_depth_and_names` |
| 46 Binaries | Walked JPEG/MP3/class without `--binary` equal rg; named files carry §9's Trails; `-v`, `-c`, `--no-trail` equal rg | 10.1–10.5; 12.2 |
| 47 Components | `script › addItem`, Svelte `#each`, Blade `@foreach`; HTML scripts: no-type and `module` function Trails, JSON-LD key Trail, `text/x-template`, > 1 MiB and > 1 000-byte line keep `script`; every v1 HTML snapshot unchanged; stream equals model on the HTML fixture | 7.1, 7.2, 7.4, 12.2 |
| 48 Fuzz | Every new text Processor and binary layout, no panic, 2 s per case, Segments inside the file; every text Format meets all four mutation operations | 11.1, 10.5; `fuzz_m6` CI steps added by the orchestrator |
| 49 Per-file cost | `scripts/bench-m6.sh` gates: mixed no-match ≤ 1.1 × rg, sparse additive bound, M4 corpus 1.05 gates, M6 against M5b ≤ 1.03 ×; the self-test checks the same evaluator | 11.3 (local Linux timings) |
| 50 Dense | Nine 50 MiB families, memory bound, JSON time bound, flat time reported | 11.2 `m6_dense` |
| 51 Detection cost | 100 000 unclaimed names including `.json`, `.html`, `.txt`, `result.json`, `messages.html`, `notes.txt` outside Telegram folders, ≤ 1.02 × M5b | 1.2 (index), 11.3 `unclaimed-detection` |
| 52 Large files | 1 GiB SQL, mbox, WhatsApp with a first-unit Match: rg time + 0.5 s, rg RSS + 64 MiB | 11.2 `m6_first_unit` |
| 53 50 MB files | Each new text family, three Matches, rg time + 1 s; a 50 MB HTML page of scripts with its Matches inside three scripts and outside all | 11.2 `m6_sparse`, `m6_html_scripts_cost` |
| 54 Size | Stripped Linux size per chunk and tanda, ≤ 23,000,000, stop at 22,800,000, recorded | Every chunk's final checks; 12.4; orchestrator CI |
| 55 Docs | `docs/formats{,.es}.md`, README tables, `.po` guide run by `tests/guide.rs`, skill names; HTML's script Trails in `docs/rg-compat.md` «Trails that changed since 1.0» | 12.1 |
| 56 Version | `s4 --version` prints 1.1.0; criterion 34 with the M6 tests on four systems; the `package` job's checks read the version from Cargo.toml | 12.3, 12.4; orchestrator CI |

| M6 spec section | Coverage |
|---|---|
| §1 Goal, §2 Scope | All chunks; header limits; C12 docs list the Formats |
| §3 Rules | Prelude `check`/`flags`, supplemental broken, encoding and depth matrices |
| §4 Detection | C1.1–C1.3 (signals 3, 4, 6), C10.4 (magic order), C12.2 (real Definitions) |
| §5 Processors | C2–C10; D11 (no new Grammar); size checks; header (no crate) |
| §6 Group A | C5.1–C5.3, C6.1 |
| §7 Group B | C2.1–C2.3, C3.1–C3.2, C4.1–C4.3 |
| §8 Group C | C8.1–C8.3, C7.1–C7.2, C7.4 (HTML scripts), C3.3, C6.2 |
| §9 Group E | C10.1–C10.5 |
| §10 Group F | C9.1–C9.3, C2.4, C6.3, C7.3 |
| §11 Cost and streaming | Header «native streaming», C11.2, C11.3, C10.5, C12.2 differential |
| §12 Windows and macOS | C1.2 separators, prelude CRLF copy, C10 writers, orchestrator CI |
| §13 Tests | All chunks; C12.2 `rg_compat_m6`; fuzz in `tests/fuzz_m6.rs` (D16) |
| §14 Docs | C12.1; schema and tree label lines in each chunk's registration |
| §15 Parts | Chunk graph, D1 |
| §16 Criteria | Table above |
| Open questions | Question 1 answered A (note 2, D21) in C1.3, C9.3; question 2 answered B (note 3, D24) in C7.4, C11.2, C12.1, C12.2 |

## Risks and what is verified only in CI

| Risk / status | Local Linux evidence | Native CI evidence / response |
|---|---|---|
| Signal 4 adds work to every `.json`, `.html` and `.txt` file before the search, **deduced, measured by #51** | C11.3 corpus with those names; C1.2 keys signal 4 by extension and filters by a literal run of the name | Timing stays local; CI runs the evaluator self-test only |
| globset's `\` normalization on Windows, **read in globset 0.4.15's source per spec §4.1, not run** | `/` cases; GNU clippy compiles the `cfg!(windows)` branch | windows job: `m6_path_separators_are_native` and automatic variants under `ssh/`, `nginx/`, `ChatExport_m6/` |
| Case-insensitive file systems collapse name spellings, **deduced** | Detector tests use path strings | macOS and Windows jobs; fix by separate directories |
| 2 s fuzz deadline on slower runners, **deduced** | `taskset -c 0` reproduction | macos-15-intel and windows-2022 timings; a budget change needs Sebastián |
| `fuzz_m6` absent from both workflows and cfg rows absent from `docs/platforms.md`, **verified on `59bb971`** | `cargo test -- --list` | Orchestrator's pre-push inventory commit |
| Linux CI's «Memory and bomb checks» runs every ignored `perf` test, now with 9 × 50 MiB and 3 × 1 GiB generated files, **runner disk and time not verified** | Same tests locally with `SEARCHFOR_PERF_TIMING=1`; the files go to `m6_perf_dir` (`SEARCHFOR_PERF_DIR`, else `CARGO_TARGET_DIR`, else `target/`), never a tmpfs `/tmp` | Record step duration and free disk of the target directory's file system; if the runner refuses, report instead of dropping the test |
| `bash`/`python3` resolution on Windows, **deduced** | Self-test on Linux | windows job; PATH pinning or a documented row |
| Binary size near 22.8 MB with seven new scanners | Stripped size per chunk and tanda | Linux job builds the same release; stop rule unchanged |
| Cargo.lock records `searchfor 1.0.0` (D20): with 1.1.0 every `--locked` command is refused, **checked on `d14a4c1`** | C12.4 Step 3 refreshes the root entry with `cargo update --offline --workspace` and checks that `git diff --numstat` reports one line added and one deleted | CI's `--locked` commands at the integrated commit |
| The `package` job's version checks at 1.1.0 (D22), **the reading step runs only in native CI** | C12.3's test: no version written in ci.yml, one read, the dry run kept | All four `package` jobs at the integrated commit |
| Telegram's export template changes its class names | C7.3 generic-HTML fallback test | None needed; the Match survives, the decoration falls back |
| `*WhatsApp*.txt` claims a `.txt` that is no export (D21), **deduced** | C9.3's `WhatsApp notes.txt`: the first-line check leaves every Trail empty | None needed; the Matches show as rg shows them |
| The export's name differs in a language without «WhatsApp» in it, **not verified: no localized source here** | `_chat.txt` and the glob | A renamed export is read with `--format`, as the docs show |
| HTML's Trails change from 1.0 in scripts with functions (D24): a script reading 1.0's `--json` sees deeper Trails, **by design** | C7.4: v1 snapshots unchanged, the six limit cases; C12.2 stream = model | Documented in `docs/rg-compat.md`; `--no-trail` unchanged (`rg_compat_m6`) |
| v1's fuzz of `html` (`tests/fuzz.rs`) now parses JavaScript: its target takes `html.toml`'s `[params]`, `scripts = true` included, and its tokens include `<script>` and `</script>`, **deduced from the code, run by C7.4** | C7.4 Steps 3–4: `SEARCHFOR_FUZZ_N=8 … --test fuzz`, both generated-input tests, 2 s per case | `ci.yml`'s `--test fuzz` step and `fuzz-nightly.yml` run it unchanged on the four systems; a timeout is reproduced under `taskset -c 0` first |
| The JavaScript MIME type list of spec §8.3 is quoted from memory, **not checked against WHATWG HTML here** | C7.4 tests `text/javascript` with spaces and case, `module`, `text/plain` | A missing equivalent keeps 1.0's Trail, never loses a Match |
| Timing criteria #49–#53 on shared runners | Local Linux, three alternated series, `uptime` recorded | CI timing is never evidence for them |

Every chunk hands off: commit IDs, exact RED/GREEN commands and results,
native-only pending checks, measured evidence paths, the stripped size and any
spec conflict found. It deletes its sole target directory after handoff. The
plan is implemented only when acceptance evidence exists; the planned PASS
messages are instructions to verify, not results already obtained.
