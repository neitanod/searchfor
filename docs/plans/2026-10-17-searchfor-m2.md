# searchfor M2 (v0.2: code, key paths and Helper Processors) Implementation Plan

> **For agentic workers:** each chunk below is implemented by one subagent in its own git
> worktree, following `test-driven-development`: every task writes its tests first, runs them
> and sees them fail (RED), implements, and runs them green (GREEN). Steps use checkbox
> (`- [ ]`) syntax. Read `CONTEXT.md` (vocabulary), `docs/specs/2026-10-16-searchfor-design.md`
> (the spec; §4.3, §4.4, §5.2, §5.3 and §7 are M2's), `docs/adr/0005-…md` and this plan's
> «Design notes» before starting. The spec wins over this plan if they disagree, and the
> disagreement is reported. Everything is written in English: code, comments, docs, commit
> messages.

**Goal:** `sfor` v0.2, which adds Trails for code (PHP, JavaScript, TypeScript, TSX, Bash, Go,
Rust, Python, with their tree-sitter Grammars compiled in), key paths for YAML, JSON and TOML,
and Helper Processors — separate programs that sfor starts once per search and feeds over
protocol v1 — used both by `processor = "external"` Definitions and by Grammar Helpers installed
with `--install-grammar`.

**Architecture:** the M1 pipeline stays as it is: search first, then the Detector, then the
Processor of the file's Format, only for files with Matches. Two built-in Processors join
`markdown`: `keypath`, over native parsers (a JSON scanner of our own, `saphyr-parser`,
`toml_edit`), and `treesitter`, which runs the query of the Format Definition over a Grammar
compiled into sfor. A per-run registry, `Processors`, replaces `processors::by_name` in the
pipeline: it hands out the built-in Processors and, for `external` Definitions and Grammars that
are not compiled in, a `HelperProcessor` that implements the same `Processor` trait by talking
to a Helper Processor through a `Host` (pool, concurrent or oneshot). The protocol types live in
one module used by both sides: the host in sfor, and `serve`, the loop that searchfor's own
helpers (the example helper and the Grammar Helper template, both workspace members) run.

**Tech stack:** Rust 1.90 (edition 2024), as M1. New dependencies, versions checked on
crates.io on 2026-10-17 and built together in a probe crate with rustc 1.90 (see «Verified for
this plan»): `tree-sitter` 0.24.7 and the ABI 14 Grammar crates `tree-sitter-php` 0.23.11,
`tree-sitter-javascript` 0.23.1, `tree-sitter-typescript` 0.23.2, `tree-sitter-bash` 0.23.3,
`tree-sitter-go` 0.23.4, `tree-sitter-rust` 0.23.3, `tree-sitter-python` 0.23.6, all pinned
with `=` (D1: the newer ones need a `regex` that breaks M1's ripgrep pins), `streaming-iterator`
0.1.9, `signal-hook` 0.3.18 (Unix only, D42), `toml_edit`
0.25.17 (feature `parse` only), `saphyr-parser` 0.2.2, `ureq` 3.4.2 (default features: rustls,
gzip), `sha2` 0.11.1; `tempfile` 3 moves from dev-dependencies to dependencies. The Grammar
Helper template adds `tree-sitter-lua` 0.2.0 (ABI 14, built with tree-sitter 0.24.7 in the
probe).

**Reference ripgrep:** 14.1.1, as M1. Every rule of M1 about comparing with `rg` still applies.

**Branch:** work branch `t-768-searchfor-m2`, which starts from `t-768-searchfor`, worktree
`/home/sebas/robotin/apps/mdgrep/.workspace/t-768-mdgrep/wt/m2`. Before chunk 1, merge the
final M1 state of `t-768-searchfor` into it (M1 is closing its code review rounds while this plan
is reviewed); line numbers quoted below are those of commit `46a4b74` and are only hints:
locate code by the names given. Each chunk branches `t-768-searchfor-m2-c<N>` from the work
branch and is merged back after its tanda, as `ciclo` stage 7 describes.

---

## Why M2, and what stays for M3–M5

M1 shipped Markdown. M2 is the milestone of «code and configuration»: the Formats most searched
after notes, and the Helper Processor mechanism that 8451–8459 made the one way to run a
Processor outside sfor. The milestone table of the M1 plan is updated with the decisions taken
since (8390–8395, 8451–8459):

| Milestone | Release | Content |
|---|---|---|
| M1 (done) | v0.1 | Source, Detector, Format Definitions, Processor contract, Markdown, core ripgrep flags, tree / one-line / JSON output, Fragments, `--within`, `--expand`, `--in-headings`, skill, crates.io |
| **M2 (this plan)** | v0.2 | `keypath` (YAML, JSON, TOML) over native parsers; `treesitter` with eight compiled-in Grammars; Helper Processor protocol v1 with `pool`, `concurrent` and `oneshot` modes; `processor = "external"`; Grammar Helper template, `--install-grammar`, `--list-grammars`, the index format and a build script; an example helper; code benchmark and helper memory tests |
| M3 | v0.3 | HTML and XML (deciding whether they use a Grammar), CSV, subtitles (.srt, .vtt; 8380), the extra text Formats of Questionnaire 49 |
| M4 | v0.4 | Object files (ELF, PE, Mach-O), `--docs` extractors (PDF, Word/ODT, Excel/ODS, PowerPoint/ODP, EPUB, SQLite; 8393–8395), ripgrep's `--pre`, the binary Formats kept by 8390 |
| M5 | v1.0 | MCP server, the rest of ripgrep's flags, `dist` (GitHub Releases, Homebrew tap, npm), Windows and macOS in CI; for Helper Processors: Grammar Helper binaries built and published for every system, `grammars/index.json` filled from the release, signed releases, the guide for writing an External Processor |

**What M2 leaves out, and why:**

- **Building and publishing Grammar Helper binaries for every system** (delegated decision D13).
  A Grammar Helper is one binary per Grammar, OS and architecture; building them needs exactly
  the matrix and release pipeline that M5 builds for sfor itself (`dist`, Windows and macOS
  runners). Doing it in M2 means building that pipeline twice, and an index with only Linux
  binaries would promise systems nobody tested. M2 ships everything the client needs —
  `--install-grammar`, `--list-grammars`, the index schema, the template, a script that builds a
  helper and its index entry for the running system — and tests it end to end with a `file://`
  index. `grammars/index.json` is committed with an empty list.
- **The guide for writing an External Processor** (M5): the protocol is fixed in spec §4.4 from
  M2 and tested by the example helper; the guide with recipes in Python and PHP waits for the
  Windows check of M5, so it is written once.
- **HTML and XML**: spec §7 leaves to M3 whether they use a Grammar.
- **A tree-sitter-yaml fallback for YAML that saphyr-parser cannot parse** (D3): the research
  proposed it; M2 keeps the Trails before the first syntax error instead, which needs no second
  YAML implementation. M3 may revisit it with real Helm charts.

## Grill decisions this plan implements

| # | Decision | Where in M2 |
|---|---|---|
| 1 | YAML, JSON and TOML key paths, and code through tree-sitter, in v1 | C2, C3, C6 |
| 5 | Shebangs decide the Format of scripts | C1 (D15), C6 Definitions |
| 6 | Embedded TOML Definitions, user directory, `--export-formats` (new built-ins export too) | C3, C6, C9 |
| 7 | External Processors in any language | C4, C5, C9, C11 |
| 8 + comments | Every v1 Grammar in the default install; extra ones easy, with an install flag | C6, C7, C8, C9, C10 |
| 10 | Names in the one-line output, signatures in the tree | C6, C10 |
| 8361 | WebAssembly Grammars — superseded by 8454 | ADR 0005 |
| 8375 | Everything in English | all chunks |
| 8381 | Coloured output | C10 (Trails keep the cyan of M1) |
| 8385 | Search as fast as ripgrep; the added cost only in presentation; no structure → ripgrep's output | C1 (D15), C6 (D24), C11 benchmark |
| 8451 | Processors as separate executables, started once, many files over IPC | C4, C5, C9 |
| 8453 + 8454 | v1 Grammars compiled in; extras (`--install-grammar`, External Processors) as helper binaries speaking JSON lines; WASM out | C1, C6, C7, C8, C9 |
| 8455 | Once per search: start with the first file of the Format, exit on EOF, restart after a crash (that file is Plain); optional `mode = "oneshot"` | C4, C11 |
| 8458 + 8459 | Release resources between files; process several in parallel: ids and any-order answers (`concurrent`), or a pool of copies up to the thread count (`pool`); the Definition names the mode; a memory test over thousands of files | C4, C5, C8, C11 |

Decisions 2, 3, 4, 9, 11–21 and 8362–8395 were implemented by M1 or belong to M3–M5.

## Decisions delegated to this plan

Sebastián left these open; each one is taken here, with its reason, and the spec carries the
ones it describes marked «M2 plan».

| Id | Decision | Reason |
|---|---|---|
| D1 | `tree-sitter` =0.24.7 with ABI 14 Grammar crates (php 0.23.11, javascript 0.23.1, typescript 0.23.2, bash 0.23.3, go 0.23.4, rust 0.23.3, python 0.23.6; lua 0.2.0 for the Grammar Helper), all pinned with `=` | M1 pins `regex-automata =0.4.7` and `regex-syntax =0.8.4`, ripgrep 14.1.1's, for rg's Unicode tables (ADR 0001). tree-sitter needs the `regex` crate: 0.27 asks `regex ^1.12.3` (→ regex-automata ^0.4.12), 0.26 `regex-syntax ^0.8.6`, 0.25 `^0.8.5`; 0.24.7 asks `regex ^1.10.6` and `regex-syntax ^0.8.4`, and regex 1.10.6 accepts automata 0.4.7 and syntax 0.8.4 (cargo index cache, review r2, Codex #1). With M1's whole Cargo.toml and lock, adding these crates resolves `regex` 1.10.6 and leaves every M1 version where it was, and M1's 265 tests pass (`realtree` probe). tree-sitter 0.24 loads ABI 13–14 only, so the Grammars are their last ABI 14 releases; the eight queries give the same Trails as with 0.27 (`ts024` probe). API differences from 0.27: `QueryMatch::captures` is a field, `Language::version()` instead of `abi_version()`. Moving to a newer tree-sitter moves with the ripgrep baseline of ADR 0001; the `=` pins keep `cargo install searchfor` (which resolves without our lock) on the same versions |
| D2 | `keypath` over native parsers: own JSON scanner, `saphyr-parser`, `toml_edit` | tree-sitter-yaml breaks over 32 768 lines; tree-sitter's TOML tree gives `products.variants[0]` instead of `products[1].variants[0]`; the JSON scanner is 27× faster (research §5) |
| D3 | No tree-sitter-yaml fallback; a syntax error keeps the Trails before it, silently (YAML: the events before the error; TOML: the longest prefix ending at a line start before the error that parses, at most 256 tries; JSON: the walk up to the error) | One YAML implementation; Helm templates are often not valid YAML, and a warning per file would bury the output. saphyr-parser read `a: {{ .Values.x }}` without error (as flow mappings) in the probe |
| D4 | `pool` is the default mode | A helper that answers one line at a time is correct in `pool`; `concurrent` needs a helper that handles ids, so it is opt-in |
| D5 | `timeout_ms` defaults to 5000; a timeout kills the process | A stuck helper cannot be trusted with the next request; killing it frees the pool slot |
| D6 | Three crashes in a run disable the Definition; a command that cannot start disables it at once | A crash loop would cost a process start and a warning per file |
| D7 | A `hello` line opens every process, with `params` and `threads` | The helper compiles its query once per process and sizes its worker pool; the protocol number is checked once |
| D8 | `Input` gains `disk: Option<&Path>`: the canonical path of a regular file outside /dev and /proc whose bytes sfor searched unchanged (no BOM, no UTF-16) and whose length on disk equals the Source's; anything else reaches the helper as a temporary file. Non-UTF-8 paths and stdin too. The typed path is never sent: `stdin` typed in /dev, `/dev/stdin` or a symlink to it would name the helper's own pipe | Offsets must count the bytes the helper reads; a helper must never reopen a stream (`/dev/stdin` is its own protocol channel); the `Processor` trait itself is unchanged |
| D9 | Helper stderr lines are relayed as warnings `sfor: <format>: <line>` | Authors see their errors; `--no-messages` silences them like every other warning |
| D10 | `processor = "external"` works from M2 | It is the same code as Grammar Helpers (8453: one mechanism), and the example helper needs it end to end; M5 keeps the guide and the publishing |
| D11 | The example helper is a Rust workspace member, `helpers/example`, describing INI files (`[section] › key`), with test knobs in environment variables | Portable to M5's Windows CI with no Python or shell in the tests; its own crate tests run it through `CARGO_BIN_EXE_…`; INI is simple enough to read at a glance and absent from sfor's built-ins |
| D12 | The Grammar Helper template is a workspace member, `helpers/grammar`, one cargo feature per Grammar; M2 builds Lua; Grammar Helpers always run in `concurrent` mode | We build every Grammar Helper, so the mode can be fixed; Lua is small, MIT and not compiled into sfor |
| D13 | M2 ships the index client, schema, template and build script; M5 builds, publishes and signs the binaries and fills the index | See «What M2 leaves out» |
| D14 | Index systems are `<os>-<arch>` from `std::env::consts` (`linux-x86_64`, `macos-aarch64`, `windows-x86_64`) | Known at compile time on every target, with no build script |
| D15 | Magic bytes and shebangs are read only for files without an extension | Otherwise the shebangs of the code Definitions make every `.txt`, `.c` or `.log` lose M1's fast printing path (`detect_by_path` would answer «needs the content»), against 8385 |
| D16 | The one-line output prints names only: `Cart › addItem` (spec §5.3 and §11 #5 reworded) | Grill #10 says names; «class Cart» was an example of a kind word that no other Format prints |
| D17 | Captures are `@segment`, `@name`, optional `@prefix` and `@signature`; Go receivers are `@prefix` (`Server.Start`); Rust `impl` blocks are named by their type, kind `impl` | `@segment` is the spec's word (the research used `@scope`); one prefix rule instead of a template language |
| D18 | A Segment's label is its `@name` node | A Match on the name of a function is not inside it (as Markdown headings), a Match in its parameters is |
| D19 | `keypath` kinds `key` and `element`; the key that holds an array is hidden; YAML documents are `document N`, 1-based | Gives `server › ports[2] › host` with one Segment per step |
| D20 | JSON accepts comments and trailing commas, and reads several top-level values in turn; extensions `json`, `jsonc`, `jsonl`, `geojson`, `webmanifest`, names `.babelrc`, `.eslintrc` | tsconfig and editor settings are JSON with comments; JSON Lines get one Trail per line for free |
| D21 | `treesitter` skips files over `max_bytes` (default 16 MiB), with one warning per Format per run | At 3 MB/s one such file costs over five seconds |
| D22 | Code benchmark criterion: no-match and `--no-trail` ≤ 1.1× rg; with < 5 % matching files, extra time ≤ matching bytes ÷ (2.5 MB/s × threads) | Measured 3.0 MB/s per thread for parse plus query on 404 real Rust files (11.5 MB, 3.8 s); a fixed 1.5× would be impossible for code, and the formula is exactly 8385's «cost only in files with Matches» |
| D23 | Memory test: resident memory after 5 000 requests ≤ the value after 500 + 2 MiB, Linux only (`/proc/<pid>/status`) | Allocators keep some memory; growth per file would pass 2 MiB well before 5 000 files |
| D24 | `treesitter` implements `Processor` directly and runs the query only over the bytes between the first and the last anchor | The query took 1.4 s of the 3.8 s measured; `QueryCursor::set_byte_range` keeps every ancestor of an anchor |
| D31 | Each helper process has a writer thread: a request is queued to it, so the caller's deadline covers writing and answering, and a kill never waits for a lock the writer holds | A helper that stops reading would otherwise block sfor forever on a full pipe (review r1, Codex #1) |
| D32 | One idempotent close per process: stdin closed, one second of grace, kill, then always reap; the pipe threads get at most one more second and are then left behind. Helpers stay in sfor's process group; signals are D42's | No zombies, and a descendant that inherited the pipes cannot keep sfor alive. Helpers stay in sfor's process group, so the terminal's Ctrl-C reaches them too; D42 covers the helpers that do not stop on it. A helper that starts processes must stop them when it exits (spec §4.4) |
| D33 | A Processor that is unavailable (Grammar missing, helper disabled) or fails makes the file Plain: `format` `null` and empty Trails, which the printers already render as Plain (`path:line: text`, spec §6.2) | Empty Trails under a Format name were neither Plain nor structured (review r1, Codex #6) |
| D34 | The JSON walker is iterative, with an explicit stack; past 256 nested containers it opens no more Segments, and it validates numbers, literals and strings (escapes, no control characters) | A deep file must not overflow the call stack, and `{"a": nope}` is a syntax error, not a key path |
| D35 | A hidden Segment that has a name stays searchable by `--in-headings`: `names_of` keeps named hidden Segments and still leaves hidden ancestors out of the Trails it builds | `ports` holds an array, so it is hidden in Trails, but it is a key (spec §8); Markdown's hidden Segments (tables) have no name, so M1 is unchanged |
| D36 | TOML Sections follow headers: a header's Section runs to the next header outside it, and an ancestor with no explicit table whose Section covers that header (`a` of `[a.b]`) gets a Segment per header, with that header's Section. Paths are lists of components, never dotted strings | A parent spanning first to last child would swallow `[x]` between `[a.b]` and `[a.c]`; `"a.b"` is one key |
| D37 | `--install-grammar` checks the Definition in the index (`name`, `processor = "treesitter"`, `grammar` equal to the Grammar), stages in a unique directory, holds a lock per Grammar (`File::lock`, std ≥ 1.89) and restores the previous install when the final rename fails | An index must not install a Definition for another Format or an `external` command; two installs must not erase each other |
| D38 | ureq runs with `https_only(true)` (checked on every redirect hop, ureq `run.rs:149`), a 10 s connect timeout and a 300 s global one | The first URL being `https://` does not stop a redirect to `http://`; offline listing must not hang |
| D39 | The treesitter Processor's compiled queries and warnings live in a per-run instance owned by `Processors`, one `OnceLock` per Definition | A second run in one process (M5's MCP server) must see new `params`; concurrent first calls must compile and warn once |
| D40 | `oneshot` writes `hello` and the request, closes stdin, and then reads both answers | A script that reads its stdin to EOF before answering is the point of `oneshot` (spec §4.4, review r1, Fable #1) |
| D42 | Supervised cancellation (Unix): one lifecycle (`cancel::Supervisor`) for every helper process. Its SIGINT/SIGTERM watcher (`signal-hook`) is installed before the first spawn; every launch runs under its lock, is refused once cancelled, and registers its process before the lock is released. On a signal it marks the run cancelled, kills and reaps every registered helper (SIGKILL, so one that ignores or handles SIGINT goes too, and one being spawned at that moment is registered first), and then sfor dies by that same signal with its default disposition (`signal_hook::low_level::emulate_default_handler`). A run that starts no helper installs nothing, as M1 | sfor dying at once would leave a helper that ignores SIGINT, or a SIGINT sent to sfor's pid only, behind and unreaped (review r2, Codex #2); re-raising the signal keeps sfor's exit status the one rg has. Windows (console Ctrl-C) is M5's |
| D41 | A line from a helper that is not JSON or has no `id` breaks the process (killed, its requests fail, one failure counted); a JSON line with a known `id` whose fields do not fit the protocol fails that request only | Only a line with an id can be attributed to a request |
| D25 | Benchmark corpora: the cargo registry sources (`~/.cargo/registry/src/index.crates.io-*`, 29 788 files on 2026-10-17) and `~/robotin/apps` (11 297 files) | The first exists wherever cargo does; the second mixes PHP, JS, Go, Python and Bash |
| D26 | Installed Grammar Definitions load between the built-ins and the user directory, with origin `installed` | A user file still overrides everything (#6) |
| D27 | TSX is its own Format (`tsx`, grammar `tsx`); `.jsx` is JavaScript | The TypeScript crate ships two Grammars; the JavaScript Grammar parses JSX |
| D28 | `--list-grammars` with an unreachable index warns and exits 0; there is no `--uninstall-grammar` (delete the directory) | Listing must work offline; uninstalling was not asked for |
| D29 | Tests in the root package build the workspace helpers with `cargo build -p … --message-format=json`, once per test binary | Cargo only exposes `CARGO_BIN_EXE_*` for the package's own binaries |
| D30 | Grammar and helper names must match `^[a-z0-9_]+$` | They become directory and file names; an index must not write outside the Grammar directory |

## Verified for this plan (2026-10-17)

Outputs in `/tmp/subtask/m2-plan/outputs/`; the probe crates in `/tmp/subtask/m2-plan/probe*`.

- Review round 2 (`/tmp/subtask/m2-plan/realtree`, `ts024`, `tsp`, `sig`): M1's own
  Cargo.toml, Cargo.lock, src and tests plus every M2 dependency at the versions of Task 1.1:
  `cargo build` adds 55 packages, changes no M1 version (`regex-automata` 0.4.7,
  `regex-syntax` 0.8.4, `memchr` 2.7.4, `aho-corasick` 1.1.3, `bstr` 1.10.0 stay; `regex`
  1.10.6 comes in), and `cargo test` passes 265 tests. With tree-sitter 0.24.7 the seven code
  Grammars and lua 0.2.0 report ABI 14 and the queries give the Trails of Task 6.2 and of
  `player.lua`; tree-sitter-lua 0.4.1 is ABI 15 and fails with `LanguageError { version: 15 }`.
  The plan's treesitter module and its tests compile and pass (28 tests) against 0.24.7 with
  the two API changes of D1. `signal-hook` 0.3.18: a parent with a SIGINT-ignoring child, sent
  SIGINT to its pid or to its group, kills and reaps the child and then dies by SIGINT
  (`emulate_default_handler`).
- `cargo search`: the versions of «Tech stack» (the round 2 pins replace the tree-sitter ones). `cargo info`: tree-sitter 0.27.1
  `rust-version 1.90`; toml_edit 1.85; saphyr-parser 1.85.0; ureq 1.85; sha2 1.88.
- A probe with tree-sitter 0.27.1, the seven code Grammar crates, toml_edit and saphyr-parser
  built with rustc 1.90 in 12 s (incremental) and ran the seven queries of the research
  (`reviews/m2-queries/*.scm`) on its samples without parse errors: php 11 Segments, js 12,
  ts 11 (ABI 14), bash 3, go 6, rust 10, python 7. Stripped, with thin LTO: 8.5 MB; with `ureq`
  and `sha2` added: 10.7 MB.
- `tree_sitter::Query`, `Language` and `Tree` are `Send + Sync`, `Parser` is `Send`.
- saphyr-parser 0.2.2: in `ñandú: 1` the value's `Marker.index` is 7 — characters, not bytes
  (9) — while `line` and `col` are right (col counts characters).
- toml_edit 0.25.17: `"…".parse::<toml_edit::Document<String>>()` keeps byte spans
  (`ImDocument` is deprecated in favour of `Document`). On the fixture `shop.toml` of C3: a
  header table's `span()` is its header (`[server]`), an array-of-tables element's is its
  `[[products]]` header, an implicit table's (`database` of `[database.replica]`, `owner` of
  `owner.name`) is its key; `Key::span()` is the key; a value's `Item::span()` is the value only,
  quotes included; array elements and inline-table values have their own spans.
- saphyr-parser 0.2.2 on the fixture `config.yaml` of C3: a scalar's span covers the scalar (a
  literal block scalar `|` through its last line); `MappingEnd` and `SequenceEnd` sit at the next
  token (the next key, or the end of the file), not at the end of the last value, so containers
  are closed at the end of their last scalar; `a: {{ .Values.x }}` parses as flow mappings,
  without an error.
- The renamed queries of C6 on its eight fixtures give the Trails listed in Task 6.3 (probe
  `trails`, output `fixture-trails.txt`), with no parse errors.
- ureq 3.4.2 downloaded `tree-sitter-json.wasm` from a GitHub release (following the redirect)
  and sha2 gave the SHA-256 the research recorded.
- `std::io::pipe` exists in rustc 1.90.
- tree-sitter-lua 0.5.0 builds with tree-sitter 0.27.1; `(function_declaration name: (_) @name)
  @segment` captures `Player:update` and `helper` on C8's fixture (round 2: the same holds for
  lua 0.2.0 with tree-sitter 0.24.7, the pair D1 chose).
- `toml::Value::try_from(serde_json::Map)` gives a `Table` (toml 1.1), and
  `body_mut().with_config().limit(…).read_to_vec()` compiles against ureq 3.4.2.
- Review round 1 (`/tmp/subtask/m2-plan/kp`, `hp`, `probe027/src/bin/r1.rs`): the plan's
  collector and iterative JSON walker compile with M1's `trail.rs` and pass their tests,
  200 000-deep nesting included (`kp`); the plan's `protocol.rs`, `helper/mod.rs` and `host.rs`
  compile without warnings and their 24 tests pass 20 runs in a row (`hp`); ureq's agent with
  `https_only(true)` refuses `http://`; the TOML walk back recovers the 15-byte prefix of
  `[a]\nb="needle"\nx=[\n[c]\nd="needle"\n` (error at byte 23) and of an unclosed `"""`;
  toml_edit and saphyr-parser stop deep nesting with an error («max recursion depth met»,
  «recursion limit exceeded at byte 258»); `File::lock` works on rustc 1.90; tempfile 3.27.0
  has `TempDir::keep` and `Builder::tempdir_in`.
- Native parse plus query of the 404 Rust files of the cargo registry matching `impl Drop for`:
  11.5 MB in 3.8 s on one thread (parse 2.45 s, query 1.4 s).

## M2 acceptance criteria

M2 is done when all of these hold; each names the test or script that measures it.

1. `cargo build --workspace`, `cargo clippy --workspace --all-targets -- -D warnings` and
   `cargo test --workspace` pass, every M1 test included.
2. Spec §11 #2 for the eleven new Formats: a fixture and tree, one-line and JSON snapshots each
   (C10, `m2_01`).
3. Spec §11 #4: `server › ports[2] › host` in `config.yaml` (C3 unit test, C10 `m2_02`).
4. Spec §11 #5: `Cart › addItem` in the one-line output, the class and method signatures in the
   tree (C6, C10 `m2_03`).
5. The Trails of Task 6.2 for the eight code Formats, the TOML ones of Task 3.2
   (`products[1] › variants[0] › sku`), and a YAML file of 40 000 lines with its Trail (C3).
6. Spec §11 #11 and #15: the example helper gives the same Trails in `pool`, `concurrent` and
   `oneshot`; a pool never runs more copies than `-j`; a crash is Plain plus one warning, the
   next file starts a new copy, the exit code is ripgrep's (C4 unit tests, C11 `h01`–`h19`).
   Every process sfor starts is reaped, a helper that stops reading cannot stall sfor, Ctrl-C
   ends the helpers, and a helper's child cannot hold sfor (C4, C11 `h13`, `h16`, `h17`).
7. Spec §11 #16: the example helper and the Lua Grammar Helper keep their resident memory
   within 2 MiB from request 500 to request 5 000 (C5, C8).
8. Spec §11 #17: `--install-grammar lua` from a `file://` index installs a working Grammar
   Helper, and a wrong SHA-256 installs nothing (C7 unit tests, C11 `h14`, `h15`).
9. Spec §11 #13 on code: `scripts/bench-code.sh` exits 0 (no-match and `--no-trail` ≤ 1.1 × rg;
   sparse extra time within the parsing budget of D22), and M1's `scripts/bench.sh` still exits
   0 (C11).
10. The C11 ceilings of `tests/perf.rs` pass in release.
11. `target/release/sfor` is at most 20 MB (measured in C10 and reported; the probe with the
    Grammars, keypath, ureq and sha2 alone was 10.7 MB stripped).
12. `--list-grammars` lists the eight compiled-in Grammars offline (C10 `m2_08`), and a run
    whose files have no Match starts no helper and prints no warning (C10 `m2_13`).
13. No document in the repository presents WebAssembly Grammars as current (C12; the M1 plan
    and ADR 0003 stay as history).

## Design notes the chunks share

1. **The Processor contract stays.** `Processor::{trails, names, outline_of}` and
   `OutlineProcessor` keep their signatures. Two additive changes: `Input` gains
   `disk: Option<&'a Path>` (D8), and the pipeline asks a per-run `Processors` registry instead
   of `processors::by_name`, because a Helper Processor has state that lives for one run.
   `by_name` stays for the stateless built-ins (`markdown`, `keypath`); `treesitter` keeps
   per-run state (D39), so `Processors` owns its instance.
2. **Search first, describe later**, as M1: a Helper Processor starts when the first file of its
   Format with a Match needs Trails, never during the walk. Before the search the pipeline only
   asks side-effect-free questions (`by_name` for the streaming path, `Processors::describes`
   for `complete_input`): no host is built, no warning is printed, and the installed Grammars
   are read once per run (review r1, Codex #5).
3. **`Processors`** (C1, completed in C9) owns, for one run: the thread count, the warning sink,
   one `HelperProcessor` per Definition that needs a helper (created on first use), the set of
   Definitions disabled for the run, and the run's temporary directory. Dropping it closes every
   helper's stdin and waits for them (one second, then kill).
4. **One protocol module.** `src/helper/protocol.rs` (C1) holds the wire types, used by the host
   (`src/helper/host.rs`, C4) and by `serve` (`src/helper/serve.rs`, C5). Neither side builds
   JSON by hand.
5. **Host.** A `Host` serves one Definition. It owns connections (`Conn`), each a launched
   process with a writer thread that owns its stdin (D31), a reader thread that parses each
   stdout line and hands the reply to the request with the same id (D41), and a stderr thread
   that relays lines as warnings; one idempotent close stops, kills and reaps it (D32). A
   `Conn` has a capacity: 1 in `pool` and `oneshot`, `threads` in `concurrent`. The process is
   reached through a `Launcher` trait, so the host's tests run fake helpers in threads over
   `std::io::pipe` and the real launcher runs `std::process::Command`.
6. **keypath collects only what an anchor needs.** Each syntax walks its file once and reports
   open and close events in file order to a collector (`keypath/collect.rs`, C2). For `trails`
   the collector keeps only the Segments open at some anchor, so a 50 MB JSON file with three
   Matches keeps three short stacks, not millions of keys; for `names` it keeps every key.
7. **treesitter** (C6) keeps one `Parser` per thread and Grammar (`thread_local!`), and one
   compiled query per Format Definition (`OnceLock<Mutex<HashMap<…>>>`), compiled on first use;
   a query that does not compile is one warning per run and the Format's files are Plain. Its
   core (`compile`, `outline`) takes a `tree_sitter::Language`, so the Grammar Helper template
   (C8) reuses it unchanged.
8. **Grammar Helpers** (C9): a `treesitter` Definition whose `grammar` is not compiled in is
   served by the installed binary `<grammars dir>/<lang>/sfor-grammar-<lang>`, through the same
   `HelperProcessor`, in `concurrent` mode, with the Definition's `params` in `hello`.
9. **Detector rule (D15).** Magic bytes and shebangs are consulted only when the file name has
   no extension, in `detect` and in `detect_by_path`.
10. **Workspace.** The root `Cargo.toml` gains `[workspace] members = ["helpers/example",
    "helpers/grammar"]`; both members have `publish = false`. `cargo test` at the root tests
    sfor only; `cargo test --workspace` tests everything, and every «tests pass» step below
    means `cargo test --workspace`. After each tanda: `cargo build --workspace`,
    `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`.
11. **New fixtures and M1's tests.** C2, C3 and C6 add files under `tests/fixtures/`, which
    M1's end-to-end and compatibility tests copy whole. Tests that search the whole fixture
    tree compare with rg under `--no-trail` or check with `contains`, so they should stay
    green; t08 (`misc/min.json`) is the one known change, owned by C3. Each of these chunks
    runs the whole suite; an M1 test that changes for another reason is reported, not edited,
    and fixed in the tanda's integration (C3 owns `tests/cli.rs` in tanda 3, C10 in tanda 4).
12. **Snapshots** as M1: insta, `INSTA_UPDATE=always cargo test`, every new `.snap` read in full
    and checked by eye before committing; the chunk report pastes one snapshot per Format.

## File structure

```
Cargo.toml                                   deps + [workspace] (only C1 edits it; C5 and C8 add one
                                             dev-dependency to their members, which touches Cargo.lock)
formats/{php,javascript,typescript,tsx,bash,go,rust,python}.toml   code Definitions (C6)
formats/{json,yaml,toml}.toml                keypath Definitions (C3)
grammars/index.json                          the Grammar index, empty list in M2 (C8)
docs/grammar-index.schema.json               schema of the index (C8)
src/model.rs                                 Input.disk (C1)
src/processors/mod.rs                        by_name + Processors, ProcessorRef (C1, C9)
src/processors/external.rs                   RunTemp (C1); HelperProcessor: Processor over a Host (C9)
src/processors/keypath/{mod,collect,json}.rs keypath Processor, collector, JSON (C2)
src/processors/keypath/{yaml,toml}.rs        YAML and TOML (C3)
src/processors/treesitter/{mod,grammars}.rs  treesitter Processor, compiled-in Grammars (C6)
src/helper/{mod,protocol}.rs                 wire types (C1)
src/helper/host.rs                           Host, Conn, Launcher, modes, failures (C4)
src/helper/serve.rs                          serve(): the loop of a Rust helper (C5)
src/grammars/{mod,index,install}.rs          Grammar directory, index, install, list (C7)
src/formats/{mod,definition}.rs              installed Definitions, Origin::Installed (C9)
src/formats/detector.rs                      D15 (C1)
src/formats/builtin.rs                       new built-ins (C6, then C3)
src/cli/mod.rs                               --install-grammar, --list-grammars (C10)
src/app.rs                                   Processors per run, Input.disk, actions (C1, C10)
src/output/tree.rs                           labels of key and element Segments (C10)
helpers/example/                             sfor-example-helper, INI (C5)
helpers/grammar/                             sfor-grammar template, Lua (C8)
scripts/build-grammar-helper.sh              builds one Grammar Helper + index entry (C8)
scripts/bench-code.sh                        code benchmark (C11)
tests/fixtures/data/                         JSON, YAML, TOML fixtures (C2, C3)
tests/fixtures/code/                         one fixture per code Format (C6)
tests/fixtures/ini/                          INI fixtures for the example helper (C11)
tests/cli_m2.rs                              end-to-end tests of the new Formats (C10)
tests/helpers.rs                             end-to-end tests of Helper Processors and install (C11)
tests/common/mod.rs                          workspace_bin() (C11)
tests/perf.rs                                keypath and treesitter ceilings (C11)
README.md, README.es.md, skills/searchfor/SKILL.md   (C12)
```

## Chunk graph

| Chunk | Content | Depends on | Files and folders (exclusive within its tanda) | Tanda |
|---|---|---|---|---|
| C1 | Dependencies, workspace, stubs, wire types, `Processors`, `Input.disk`, Detector rule D15 | — | `Cargo.toml`, `Cargo.lock`, `src/lib.rs`, `src/model.rs`, `src/search.rs` (`walker_threads` public), `src/processors/mod.rs`, stubs of every new `src/**` file, `src/helper/{mod,protocol}.rs`, `src/formats/detector.rs`, the tests of `src/formats/{mod,definition}.rs`, the `Input` literals of `src/app.rs`, `src/output/stream.rs`, `src/filter.rs`, `helpers/example/{Cargo.toml,src/main.rs}` and `helpers/grammar/{Cargo.toml,src/main.rs}` stubs | 1 |
| C2 | keypath Processor, collector, JSON walker; named hidden Segments in `names_of` | C1 | `src/processors/keypath/{mod,collect,json}.rs`, `src/trail.rs` (`names_of`), `tests/fixtures/data/*.json` | 2 |
| C4 | Host (with the fake launcher in `host.rs`'s `testing` module) | C1 | `src/helper/host.rs` | 2 |
| C5 | `serve` and the example helper with its memory test | C1 | `src/helper/serve.rs`, `helpers/example/**`, `Cargo.lock` (the member's dev-dependency) | 2 |
| C6 | treesitter Processor, compiled-in Grammars, code Definitions | C1 | `src/processors/treesitter/**`, `formats/{php,javascript,typescript,tsx,bash,go,rust,python}.toml`, `src/formats/builtin.rs`, `tests/fixtures/code/**` | 2 |
| C7 | Grammar directory, index, install, list (library) | C1 | `src/grammars/**` | 2 |
| C3 | YAML and TOML, keypath Definitions, the min.json test | C2 | `src/processors/keypath/{yaml,toml}.rs`, `formats/{json,yaml,toml}.toml`, `src/formats/builtin.rs`, `tests/fixtures/data/*.{yaml,yml,toml}`, `tests/cli.rs` (t08 only), `tests/snapshots/cli__t08_fragments.snap` | 3 |
| C8 | Grammar Helper template (Lua), its memory test, build script, index schema | C5, C6 | `helpers/grammar/**`, `scripts/build-grammar-helper.sh`, `grammars/index.json`, `docs/grammar-index.schema.json`, `Cargo.lock` (the member's dev-dependency) | 3 |
| C9 | `HelperProcessor`; Grammar Helpers in `Processors`; installed Definitions; unavailable Formats | C4, C6, C7 | `src/processors/external.rs`, `src/processors/mod.rs`, `src/formats/{mod,definition}.rs` | 3 |
| C10 | Pipeline: `Processors` per run, `Input.disk`, Plain fallback, flags and actions, tree labels, end-to-end Format tests | C1–C9 | `src/app.rs`, `src/cli/mod.rs`, `src/output/tree.rs`, `tests/common/mod.rs` (Grammar isolation of `Fixture`), `tests/cli_m2.rs`, `tests/snapshots/cli_m2__*.snap` | 4 |
| C11 | Helper and install end-to-end tests, code benchmark, perf ceilings | C10 | `tests/helpers.rs`, `tests/common/mod.rs` (`workspace_bin`), `tests/perf.rs`, `tests/fixtures/ini/**`, `scripts/bench-code.sh`, `scripts/fake-sfor-exit-2.sh` | 5 |
| C12 | README, skill, spec reconciliation | C10 | `README.md`, `README.es.md`, `skills/searchfor/SKILL.md`, `docs/specs/2026-10-16-searchfor-design.md`, `docs/json-schema.json` | 5 |

Twelve chunks in five tandas. C6 and C3 both edit `src/formats/builtin.rs`, and C1, C5 and
C8 `Cargo.lock`, always in different tandas (C5 and C8 only add a member's dev-dependency). C1 writes every `mod` declaration, every dependency and a stub for every new file, so
later chunks only fill their own files; a chunk uses another chunk's API only through the
signatures fixed in this plan, and C10 is where everything links.

---

## Chunk 1: Dependencies, workspace, wire types, `Processors`, Detector rule

### Task 1.0: bring in the final M1

- [ ] `git merge --no-edit t-768-searchfor` into `t-768-searchfor-m2`, then `cargo test`; every
  M1 test passes before anything else changes. If the merge conflicts in `docs/`, keep both
  texts and report it.

### Task 1.1: Cargo.toml and the workspace

**Files:** `Cargo.toml`, `Cargo.lock`, `helpers/example/{Cargo.toml,src/main.rs}`,
`helpers/grammar/{Cargo.toml,src/main.rs}`. This is the only task of M2 that edits the root
`Cargo.toml`.

- [ ] **Step 1:** add to `[dependencies]`, keeping M1's pinned lines untouched:

```toml
# Code Grammars compiled in (ADR 0005). tree-sitter 0.24.7 is the newest whose `regex`
# requirement (^1.10.6, regex-syntax ^0.8.4) accepts the ripgrep 14.1.1 pins above; 0.25+
# would move regex-automata and regex-syntax, and with them rg's Unicode tables (M2 plan, D1).
# It loads ABI 14 Grammars, hence these Grammar versions. Keep the `=`: a cargo update must
# not move them, and `cargo install searchfor` resolves without our lock.
tree-sitter = "=0.24.7"
streaming-iterator = "0.1"
tree-sitter-bash = "=0.23.3"
tree-sitter-go = "=0.23.4"
tree-sitter-javascript = "=0.23.1"
tree-sitter-php = "=0.23.11"
tree-sitter-python = "=0.23.6"
tree-sitter-rust = "=0.23.3"
tree-sitter-typescript = "=0.23.2"
# keypath (spec §4.3): spans for TOML, events with markers for YAML.
toml_edit = { version = "0.25", default-features = false, features = ["parse"] }
saphyr-parser = "0.2"
# --install-grammar (spec §7).
sha2 = "0.11"
ureq = "3"
# Temporary files for Helper Processors (spec §4.4, Paths).
tempfile = "3"

# Ctrl-C with Helper Processors running (spec §4.4, Lifetime; M2 plan, D42).
[target.'cfg(unix)'.dependencies]
signal-hook = "0.3"
```

  and remove `tempfile` from `[dev-dependencies]` (a normal dependency serves the tests too).
  At the end of the file:

```toml
[workspace]
members = ["helpers/example", "helpers/grammar"]
```

- [ ] **Step 2:** the two members, so the workspace resolves. `helpers/example/Cargo.toml`:

```toml
[package]
name = "sfor-example-helper"
version = "0.2.0-dev"
edition = "2024"
rust-version = "1.90"
license = "MIT OR Apache-2.0"
publish = false
description = "Example Helper Processor for searchfor: INI sections and keys, over protocol v1"

[dependencies]
anyhow = "1"
searchfor = { path = "../.." }
```

  `helpers/grammar/Cargo.toml`:

```toml
[package]
name = "sfor-grammar"
version = "0.2.0-dev"
edition = "2024"
rust-version = "1.90"
license = "MIT OR Apache-2.0"
publish = false
description = "Grammar Helper template for searchfor: one tree-sitter Grammar per build"

[features]
default = ["lua"]
lua = ["dep:tree-sitter-lua"]

[dependencies]
anyhow = "1"
searchfor = { path = "../.." }
tree-sitter = "=0.24.7"
tree-sitter-lua = { version = "=0.2.0", optional = true }
```

  Both `src/main.rs` stubs: `fn main() {}` with a `//!` line naming the chunk that fills them
  (C5, C8).

- [ ] **Step 3:** `cargo build --workspace` resolves and compiles. Use `cargo build`, never
  `cargo generate-lockfile` or `cargo update`, so M1's locked versions stay. Then check the
  ripgrep pins survived: `cargo tree -i regex-automata` and `cargo tree -i regex-syntax` show
  only 0.4.7 and 0.8.4, `grep -A1 'name = "regex"$' Cargo.lock` shows 1.10.x, and the diff of
  `Cargo.lock` only adds packages (one new copy of `base64` and of `windows-sys` beside M1's
  is expected). `cargo test` passes all M1 tests. Record the versions in the report.

### Task 1.2: module tree and stubs

**Files:** `src/lib.rs`, `src/processors/mod.rs` (module lines), stubs.

- [ ] `src/lib.rs` gains `pub mod grammars;` and `pub mod helper;`.
- [ ] `src/processors/mod.rs` gains `pub mod external; pub mod keypath; pub mod treesitter;`.
- [ ] Stubs, each a file whose first line is `//!` naming its chunk:
  - `src/helper/mod.rs` (filled in Task 1.3), `src/helper/protocol.rs` (Task 1.3),
    `src/helper/host.rs` (C4), `src/helper/serve.rs` (C5);
  - `src/processors/external.rs` (C9; `RunTemp` in Task 1.5),
    `src/processors/keypath/{mod,collect,json,yaml,toml}.rs` (C2, C3),
    `src/processors/treesitter/{mod,grammars}.rs` (C6);
  - `src/grammars/{mod,index,install}.rs` (C7).
- [ ] The stubs that other code names in C1 hold these exact items, so later chunks only fill
  bodies:

```rust
// src/processors/keypath/mod.rs (C2 fills)
pub mod collect;
pub mod json;
pub mod toml;
pub mod yaml;
/// keypath: YAML, JSON and TOML key paths (spec §5.2).
pub struct Keypath;
impl super::Processor for Keypath {
    // `toml` is the sibling module here: the crate is `::toml`.
    fn trails(&self, _: &crate::model::Input, _: &::toml::Table, _: &[usize]) -> anyhow::Result<Vec<crate::model::Trail>> {
        anyhow::bail!("keypath is not implemented yet (C2)")
    }
    fn names(&self, _: &crate::model::Input, _: &::toml::Table) -> anyhow::Result<Vec<crate::model::Name>> {
        anyhow::bail!("keypath is not implemented yet (C2)")
    }
}

// src/processors/keypath/yaml.rs (C3 fills)
pub fn walk(_src: &[u8], _c: &mut super::collect::Collector) -> usize { 0 }

// src/processors/keypath/toml.rs (C3 fills)
pub fn outline(_src: &[u8]) -> crate::trail::Outline { crate::trail::Outline::default() }

// src/processors/keypath/collect.rs (C2 fills; yaml.rs names it)
pub struct Collector<'a> { _anchors: &'a [usize] }

// src/processors/treesitter/mod.rs (C6 fills)
pub mod grammars;
/// treesitter: code Trails from a query (spec §5.3). Per-run state (D39): `Processors` owns
/// one instance per run.
#[derive(Default)]
pub struct TreeSitter { /* C6 */ }
impl super::Processor for TreeSitter { /* the same two bails, naming C6 */ }

// src/processors/treesitter/grammars.rs (C6 fills)
/// The Grammars compiled into sfor (spec §7).
pub const COMPILED_IN: &[&str] = &[];
pub fn language(_name: &str) -> Option<tree_sitter::Language> { None }

// src/helper/host.rs (C4 fills)
use super::{HostConfig, Launcher, Proc, Warn};
/// Runs `argv` with stdin, stdout and stderr piped, in sfor's working directory.
pub struct CommandLauncher { pub argv: Vec<String> }
impl Launcher for CommandLauncher {
    fn launch(&self) -> std::io::Result<Proc> { unimplemented!("C4") }
}
/// The helper processes of one Definition for one run (design note 5).
pub struct Host { _cfg: HostConfig }
impl Host {
    pub fn new(cfg: HostConfig, _launcher: Box<dyn Launcher>, _warn: Warn) -> Self { Self { _cfg: cfg } }
    pub fn request(&self, _req: impl FnOnce(u64) -> super::protocol::Request) -> Result<super::protocol::Reply, super::HelperError> { unimplemented!("C4") }
    pub fn shutdown(&self) {}
}

// src/processors/external.rs (C9 fills HelperProcessor; RunTemp is Task 1.5)
pub struct HelperProcessor { /* C9 */ }
impl HelperProcessor {
    pub fn new(_cfg: crate::helper::HostConfig, _launcher: Box<dyn crate::helper::Launcher>, _tmp: std::sync::Arc<RunTemp>, _warn: crate::helper::Warn) -> Self { unimplemented!("C9") }
    pub fn shutdown(&self) {}
}
impl super::Processor for HelperProcessor { /* the same two bails, naming C9 */ }
```

  `src/helper/serve.rs`, `src/processors/keypath/json.rs` and `src/grammars/*.rs` hold only
  their `//!` line. `src/processors/external.rs` also gets `RunTemp` and `TempFile` now, with
  the code of Task 1.5 (their test stays in Task 1.5).

- [ ] **Do not build yet.** These stubs name the types Task 1.3 writes (`HostConfig`,
  `Launcher`, `Proc`, `Warn`, `HelperError`, `Request`, `Reply`), so the crate compiles for the
  first time at the end of Task 1.3, which commits this task's files with its own (review r1,
  Codex #16, Fable #4).

### Task 1.3: the wire types (src/helper/protocol.rs) and the helper configuration (src/helper/mod.rs)

**Files:** `src/helper/protocol.rs`, `src/helper/mod.rs`, `src/model.rs` (`Deserialize` on
`Segment`).

- [ ] **Step 1: failing tests**, at the end of `src/helper/protocol.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_are_one_json_object_per_line() {
        let r = Request::Trails {
            id: 7,
            path: "api/user.proto".into(),
            matches: vec![MatchPos { line: 42, byte: 1730 }],
        };
        assert_eq!(
            line(&r),
            "{\"op\":\"trails\",\"id\":7,\"path\":\"api/user.proto\",\"matches\":[{\"line\":42,\"byte\":1730}]}\n"
        );
        let mut params = toml::Table::new();
        params.insert("grammar".into(), toml::Value::String("lua".into()));
        let h = Request::hello("proto", &params, 8);
        assert_eq!(
            line(&h),
            "{\"op\":\"hello\",\"id\":0,\"protocol\":1,\"format\":\"proto\",\"params\":{\"grammar\":\"lua\"},\"threads\":8}\n"
        );
    }

    #[test]
    fn replies_parse_and_ignore_unknown_fields() {
        let r: Reply = serde_json::from_str(
            r#"{"id":7,"trails":[[{"kind":"message","name":"User","line":40,"end_line":55}]],"extra":true}"#,
        )
        .unwrap();
        assert_eq!(r.id, 7);
        let t = r.trails.unwrap();
        assert_eq!(t[0][0].name, "User");
        assert_eq!(t[0][0].level, None);
        let e: Reply = serde_json::from_str(r#"{"id":3,"error":"no such file"}"#).unwrap();
        assert_eq!(e.error.as_deref(), Some("no such file"));
        let h: Reply = serde_json::from_str(r#"{"id":0,"protocol":1,"name":"x"}"#).unwrap();
        assert_eq!(h.protocol, Some(1));
    }

    #[test]
    fn names_convert_both_ways() {
        let w: WireName = serde_json::from_str(
            r#"{"text":"User","pieces":[[0,4,1702,1706]],"anchor":1702,"trail":[]}"#,
        )
        .unwrap();
        let n = w.clone().into_name();
        assert_eq!(n.pieces, vec![(0..4, 1702..1706)]);
        assert!(n.labels.is_empty());
        assert_eq!(WireName::from(&n), w);
    }

    #[test]
    fn requests_parse_back_for_helpers() {
        let r = Request::Names { id: 2, path: "a.ini".into() };
        let back: Request = serde_json::from_str(line(&r).trim_end()).unwrap();
        assert_eq!(back, r);
    }
}
```

  and at the end of `src/helper/mod.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn params(src: &str) -> toml::Table {
        toml::from_str(src).unwrap()
    }

    #[test]
    fn external_config_reads_command_mode_and_timeout() {
        let c = HostConfig::external("ini", &params("command = ['ini-trail', '-q']\nmode = 'concurrent'\ntimeout_ms = 300"), 6).unwrap();
        assert_eq!(c.command, ["ini-trail", "-q"]);
        assert_eq!(c.mode, Mode::Concurrent);
        assert_eq!(c.timeout, Duration::from_millis(300));
        assert_eq!(c.threads, 6);
        let d = HostConfig::external("ini", &params("command = ['x']"), 1).unwrap();
        assert_eq!((d.mode, d.timeout), (Mode::Pool, Duration::from_millis(5000)));
        assert_eq!(HostConfig::external("ini", &params("command = ['x']\nmode = 'oneshot'"), 1).unwrap().mode, Mode::Oneshot);
    }

    #[test]
    fn external_config_errors_name_the_field() {
        let e = |src| HostConfig::external("ini", &params(src), 1).unwrap_err();
        assert!(e("").contains("`command`"));
        assert!(e("command = []").contains("`command`"));
        assert!(e("command = 'x'").contains("`command`"));
        assert!(e("command = ['x']\nmode = 'fast'").contains("`mode`"));
        assert!(e("command = ['x']\ntimeout_ms = 0").contains("`timeout_ms`"));
    }
}
```

- [ ] **Step 2:** `cargo test helper::` fails to compile (the types are missing, and so are
  the ones Task 1.2's stubs name). That is the RED.

- [ ] **Step 3:** `src/model.rs`: `Segment` derives `Deserialize` too, with `#[serde(default)]`
  on `level` and `signature`. `src/helper/protocol.rs`:

```rust
//! Helper Processor protocol v1 (spec §4.4): the lines sfor and a helper exchange. The host
//! (`host.rs`) and the helper loop (`serve.rs`) both use these types; nothing builds JSON by hand.
use crate::model::{Name, Trail};
use serde::{Deserialize, Serialize};

pub const PROTOCOL: u32 = 1;

/// A Match in a `trails` request: its 1-based line and its absolute byte offset in `path`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchPos {
    pub line: u64,
    pub byte: u64,
}

/// One line from sfor to a helper.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum Request {
    Hello {
        id: u64,
        protocol: u32,
        format: String,
        params: serde_json::Map<String, serde_json::Value>,
        threads: usize,
    },
    Trails { id: u64, path: String, matches: Vec<MatchPos> },
    Names { id: u64, path: String },
}

impl Request {
    /// The first line of every process (spec §4.4, 1). Id 0 is reserved for it.
    pub fn hello(format: &str, params: &toml::Table, threads: usize) -> Self {
        let params = match serde_json::to_value(params) {
            Ok(serde_json::Value::Object(m)) => m,
            _ => serde_json::Map::new(),
        };
        Request::Hello { id: 0, protocol: PROTOCOL, format: format.into(), params, threads }
    }

    pub fn id(&self) -> u64 {
        match self {
            Request::Hello { id, .. } | Request::Trails { id, .. } | Request::Names { id, .. } => *id,
        }
    }
}

/// One line from a helper to sfor. Exactly one of `protocol` (to `hello`), `trails`, `names`
/// or `error` is expected; unknown fields are ignored.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Reply {
    pub id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trails: Option<Vec<Trail>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub names: Option<Vec<WireName>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// A `names` entry on the wire: `pieces` as `[text_start, text_end, src_start, src_end]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireName {
    pub text: String,
    pub pieces: Vec<[usize; 4]>,
    pub anchor: usize,
    pub trail: Trail,
}

impl WireName {
    /// The model's Name. Labels are not on the wire: `Name::trail_at` then leaves out only the
    /// named Segment, as it documents.
    pub fn into_name(self) -> Name {
        Name {
            text: self.text,
            pieces: self.pieces.iter().map(|p| (p[0]..p[1], p[2]..p[3])).collect(),
            trail: self.trail,
            labels: Vec::new(),
            anchor: self.anchor,
        }
    }
}

impl From<&Name> for WireName {
    fn from(n: &Name) -> Self {
        WireName {
            text: n.text.clone(),
            pieces: n.pieces.iter().map(|(t, s)| [t.start, t.end, s.start, s.end]).collect(),
            anchor: n.anchor,
            trail: n.trail.clone(),
        }
    }
}

/// `r` as one protocol line, `\n` included.
pub fn line<T: Serialize>(r: &T) -> String {
    let mut s = serde_json::to_string(r).expect("protocol types always serialize");
    s.push('\n');
    s
}
```

  A Segment on the wire is `model::Segment`, the Segment of `--json` (spec §6.4), so `use
  crate::model::Segment` is not needed. `src/helper/mod.rs`:

```rust
//! Helper Processors (spec §4.4, ADR 0005): the configuration of a helper, how a process is
//! launched, and the errors of a request.
pub mod host;
pub mod protocol;
pub mod serve;

use std::io::{Read, Write};
use std::sync::Arc;
use std::time::Duration;

/// Where warnings go: the pipeline prints them on stderr unless `--no-messages`.
pub type Warn = Arc<dyn Fn(String) + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Pool,
    Concurrent,
    Oneshot,
}

pub const DEFAULT_TIMEOUT: Duration = Duration::from_millis(5000);

#[derive(Debug, Clone)]
pub struct HostConfig {
    /// The Format Definition's name: the key of the host and the prefix of its warnings.
    pub format: String,
    pub command: Vec<String>,
    pub mode: Mode,
    pub timeout: Duration,
    /// Sent in `hello`.
    pub params: toml::Table,
    /// The search's thread count: the pool's size, and the requests in flight in `concurrent`.
    pub threads: usize,
}

impl HostConfig {
    /// From the `[params]` of an `external` Definition: `command` (array of strings, required),
    /// `mode` ("pool" by default, "concurrent", "oneshot"), `timeout_ms` (positive, 5000).
    pub fn external(format: &str, params: &toml::Table, threads: usize) -> Result<Self, String> {
        let command: Vec<String> = match params.get("command").and_then(|v| v.as_array()) {
            Some(a) if !a.is_empty() && a.iter().all(|v| v.is_str()) => {
                a.iter().map(|v| v.as_str().unwrap().to_string()).collect()
            }
            _ => return Err("`command` must be a non-empty array of strings".into()),
        };
        let mode = match params.get("mode").map(|v| v.as_str()) {
            None => Mode::Pool,
            Some(Some("pool")) => Mode::Pool,
            Some(Some("concurrent")) => Mode::Concurrent,
            Some(Some("oneshot")) => Mode::Oneshot,
            Some(_) => return Err("`mode` must be \"pool\", \"concurrent\" or \"oneshot\"".into()),
        };
        let timeout = match params.get("timeout_ms") {
            None => DEFAULT_TIMEOUT,
            Some(v) => match v.as_integer() {
                Some(ms) if ms > 0 => Duration::from_millis(ms as u64),
                _ => return Err("`timeout_ms` must be a positive integer".into()),
            },
        };
        Ok(Self { format: format.into(), command, mode, timeout, params: params.clone(), threads: threads.max(1) })
    }
}

/// A launched helper: its three pipes and a way to stop it.
pub struct Proc {
    pub stdin: Box<dyn Write + Send>,
    pub stdout: Box<dyn Read + Send>,
    pub stderr: Option<Box<dyn Read + Send>>,
    pub control: Box<dyn Control>,
}

pub trait Control: Send {
    fn kill(&mut self);
    /// Waits up to `within` for the process to exit; true when it did.
    fn wait(&mut self, within: Duration) -> bool;
    /// Collects the exit status after an exit or a kill, so no zombie is left (D32). Blocks
    /// until the process is gone; call it after `kill` or after `wait` returned true.
    fn reap(&mut self);
}

/// Starts a helper process. The real one runs a command; tests run threads over pipes.
pub trait Launcher: Send + Sync {
    fn launch(&self) -> std::io::Result<Proc>;
}

/// A Processor that cannot describe this file: the pipeline prints it as Plain, without a
/// warning of its own (D33). Returned inside `anyhow::Error`; the pipeline finds it with
/// `downcast_ref`.
#[derive(Debug, thiserror::Error)]
#[error("unavailable")]
pub struct Unavailable;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum HelperError {
    /// The Definition is disabled for this run; its files print as Plain, silently.
    #[error("disabled for this run")]
    Disabled,
    #[error("{0}")]
    Answer(String),
    #[error("did not answer within {0} ms")]
    Timeout(u128),
    #[error("{0}")]
    Crashed(String),
    /// A JSON answer with a known id whose fields do not fit the protocol (D41).
    #[error("{0}")]
    Protocol(String),
}
```

- [ ] **Step 4:** `cargo build --workspace` compiles, `cargo test` passes (the stubs are
  unreachable: no Definition names `keypath` or `treesitter` yet), and `cargo test helper::`
  passes. Commit «Add M2's dependencies, module stubs and protocol v1's wire types» with
  `git add -- Cargo.toml Cargo.lock src helpers`.

### Task 1.4: `Input.disk`

**Files:** `src/model.rs` and every `Input { … }` literal, found with `rg -n 'Input \{' src
tests`: at `46a4b74`, three in `src/app.rs` (`outlined_file`, `describe`, `names_of`) and four in
tests, `src/processors/mod.rs:87`, `src/processors/markdown.rs:402`, `src/output/stream.rs:494`
and `src/filter.rs:472`.

- [ ] `Input` gains, documented:

```rust
    /// The file on disk whose bytes are exactly `bytes`, for Processors that read the file
    /// themselves (Helper Processors, spec §4.4). None for stdin, for a Source whose BOM was
    /// removed or whose UTF-16 was transcoded, and for extracted text.
    pub disk: Option<&'a std::path::Path>,
```

- [ ] Every literal gets `disk: None`; C10 sets the real value in `app.rs`. `cargo test`
  passes unchanged. Commit «Add Input.disk for Processors that read the file».

### Task 1.5: `Processors`, the per-run registry, and `RunTemp`

**Files:** `src/processors/mod.rs`, `src/processors/external.rs` (`RunTemp` only),
`src/search.rs` (`pub fn walker_threads`), `src/app.rs` (the `Registry::load` call).

- [ ] **Step 1: failing tests** in `src/processors/mod.rs`:

```rust
    use crate::formats::definition::FormatDefinition;
    use std::sync::{Arc, Mutex};

    fn def(src: &str) -> FormatDefinition {
        FormatDefinition::parse(src, Processors::known).unwrap()
    }

    fn collecting() -> (crate::helper::Warn, Arc<Mutex<Vec<String>>>) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let s = seen.clone();
        (Arc::new(move |m| s.lock().unwrap().push(m)), seen)
    }

    #[test]
    fn known_processors_include_the_m2_ones() {
        for p in ["markdown", "keypath", "treesitter", "external"] {
            assert!(Processors::known(p), "{p}");
        }
        assert!(!Processors::known("plain"));
    }

    #[test]
    fn built_ins_come_from_by_name() {
        let (w, seen) = collecting();
        let ps = Processors::new(4, w);
        assert!(matches!(ps.for_definition(&def("name='m'\nprocessor='markdown'")), Some(ProcessorRef::BuiltIn(_))));
        assert!(seen.lock().unwrap().is_empty());
    }

    #[test]
    fn a_missing_grammar_warns_once_and_gives_none() {
        let (w, seen) = collecting();
        let ps = Processors::new(4, w);
        let d = def("name='lua'\nprocessor='treesitter'\n[params]\ngrammar='lua'\nquery=''");
        assert!(ps.for_definition(&d).is_none());
        assert!(ps.for_definition(&d).is_none());
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 1, "{seen:?}");
        assert!(seen[0].contains("lua") && seen[0].contains("--list-grammars"), "{seen:?}");
    }

    #[test]
    fn an_external_definition_without_command_warns_once() {
        let (w, seen) = collecting();
        let ps = Processors::new(4, w);
        let d = def("name='ini'\nprocessor='external'");
        assert!(ps.for_definition(&d).is_none());
        assert!(ps.for_definition(&d).is_none());
        assert_eq!(seen.lock().unwrap().len(), 1);
        assert!(seen.lock().unwrap()[0].contains("`command`"));
    }

    #[test]
    fn run_temp_writes_and_removes_files() {
        let t = external::RunTemp::new();
        let f = t.write(std::path::Path::new("dir/a.ini"), b"[s]\nk=v\n").unwrap();
        assert!(f.path().to_string_lossy().ends_with("-a.ini"), "{}", f.path().display());
        assert_eq!(std::fs::read(f.path()).unwrap(), b"[s]\nk=v\n");
        let p = f.path().to_path_buf();
        drop(f);
        assert!(!p.exists());
        let s = t.write(std::path::Path::new("<stdin>"), b"x").unwrap();
        assert!(s.path().to_string_lossy().ends_with("-_stdin_"), "{}", s.path().display());
    }
```

  Replace `by_name_knows_markdown_only` with a test that `by_name` knows `markdown` and
  `keypath` and nothing else (`treesitter` and `external` keep per-run state, so `Processors`
  serves them), and add one that `describes` answers without warning: true for markdown and a
  compiled-in Grammar (after C6; until then the test uses markdown and `external`), false for
  a missing Grammar, and `seen` stays empty.

- [ ] **Step 2:** RED: `cargo test processors::` fails to compile.

- [ ] **Step 3:** implement. In `src/processors/mod.rs`:

```rust
/// The stateless built-in Processors, by the name in a Format Definition's `processor`. The
/// pipeline uses it before the search, for the streaming path of Outline Processors; it has no
/// side effects.
pub fn by_name(name: &str) -> Option<&'static dyn Processor> {
    match name {
        "markdown" => Some(&markdown::Markdown),
        "keypath" => Some(&keypath::Keypath),
        _ => None,
    }
}

/// A Processor for one file: a built-in, the run's treesitter, or the Helper Processor of its
/// Definition.
pub enum ProcessorRef<'a> {
    BuiltIn(&'a dyn Processor),
    Helper(std::sync::Arc<external::HelperProcessor>),
}

impl std::ops::Deref for ProcessorRef<'_> {
    type Target = dyn Processor;
    fn deref(&self) -> &Self::Target {
        match self {
            ProcessorRef::BuiltIn(p) => *p,
            ProcessorRef::Helper(h) => h.as_ref(),
        }
    }
}

/// The Processors of one run (design note 3): the built-ins, and one Helper Processor per
/// Definition that needs one, started on first use and stopped when this is dropped.
pub struct Processors {
    threads: usize,
    warn: crate::helper::Warn,
    helpers: std::sync::Mutex<std::collections::HashMap<String, std::sync::Arc<external::HelperProcessor>>>,
    /// Definitions disabled for this run, already warned about.
    disabled: std::sync::Mutex<std::collections::HashSet<String>>,
    tmp: std::sync::Arc<external::RunTemp>,
    /// The run's treesitter (D39).
    treesitter: treesitter::TreeSitter,
}

impl Processors {
    pub fn new(threads: usize, warn: crate::helper::Warn) -> Self {
        Self {
            threads: threads.max(1),
            warn,
            helpers: Default::default(),
            disabled: Default::default(),
            tmp: std::sync::Arc::new(external::RunTemp::new()),
            treesitter: treesitter::TreeSitter::default(),
        }
    }

    /// The Processor names a Format Definition may use (`Registry::load`).
    pub fn known(name: &str) -> bool {
        by_name(name).is_some() || matches!(name, "treesitter" | "external")
    }

    /// Whether files of `def` may get Trails, without building anything or warning: for
    /// `complete_input`, which runs before the search (design note 2). C9 adds installed
    /// Grammars.
    pub fn describes(&self, def: &crate::formats::definition::FormatDefinition) -> bool {
        match def.processor.as_str() {
            "external" => !self.disabled.lock().unwrap().contains(&def.name),
            "treesitter" => {
                let grammar = def.params.get("grammar").and_then(|v| v.as_str()).unwrap_or("");
                treesitter::grammars::language(grammar).is_some()
            }
            p => by_name(p).is_some(),
        }
    }

    /// The Processor for files of `def`, or None (the file is Plain). Warns once per run for a
    /// Definition that cannot work: a Grammar not available, an `external` without a valid
    /// `command`.
    pub fn for_definition(&self, def: &crate::formats::definition::FormatDefinition) -> Option<ProcessorRef<'_>> {
        if self.disabled.lock().unwrap().contains(&def.name) {
            return None;
        }
        match def.processor.as_str() {
            "external" => self.helper(def, || {
                crate::helper::HostConfig::external(&def.name, &def.params, self.threads)
            }),
            "treesitter" => {
                let grammar = def.params.get("grammar").and_then(|v| v.as_str()).unwrap_or("");
                if treesitter::grammars::language(grammar).is_some() {
                    return Some(ProcessorRef::BuiltIn(&self.treesitter));
                }
                // C9: an installed Grammar Helper serves it here.
                self.disable(def, format!(
                    "sfor: {}: grammar `{grammar}` is neither compiled in nor installed (see --list-grammars)",
                    def.name
                ));
                None
            }
            p => by_name(p).map(ProcessorRef::BuiltIn),
        }
    }

    fn helper(
        &self,
        def: &crate::formats::definition::FormatDefinition,
        cfg: impl FnOnce() -> Result<crate::helper::HostConfig, String>,
    ) -> Option<ProcessorRef<'_>> {
        let mut map = self.helpers.lock().unwrap();
        if let Some(h) = map.get(&def.name) {
            return Some(ProcessorRef::Helper(h.clone()));
        }
        match cfg() {
            Err(e) => {
                drop(map);
                self.disable(def, format!("sfor: {}: {e}", def.name));
                None
            }
            Ok(cfg) => {
                let launcher = Box::new(crate::helper::host::CommandLauncher { argv: cfg.command.clone() });
                let h = std::sync::Arc::new(external::HelperProcessor::new(cfg, launcher, self.tmp.clone(), self.warn.clone()));
                map.insert(def.name.clone(), h.clone());
                Some(ProcessorRef::Helper(h))
            }
        }
    }

    fn disable(&self, def: &crate::formats::definition::FormatDefinition, msg: String) {
        if self.disabled.lock().unwrap().insert(def.name.clone()) {
            (self.warn)(msg);
        }
    }

    /// Closes every helper's stdin and waits for it (spec §4.4, Lifetime). Idempotent.
    pub fn shutdown(&self) {
        let helpers: Vec<_> = self.helpers.lock().unwrap().drain().map(|(_, h)| h).collect();
        for h in helpers {
            h.shutdown();
        }
    }
}

impl Drop for Processors {
    fn drop(&mut self) {
        self.shutdown();
    }
}
```

  In `src/processors/external.rs`, `RunTemp` in full (C4 adds `HelperProcessor` around it):

```rust
/// The run's temporary directory (spec §4.4, Paths), created on first use and removed with the
/// last `RunTemp` handle.
pub struct RunTemp {
    dir: std::sync::OnceLock<Result<tempfile::TempDir, String>>,
    next: std::sync::atomic::AtomicU64,
}

/// A temporary copy of a Source, removed when dropped (after the helper answered).
pub struct TempFile(std::path::PathBuf);

impl TempFile {
    pub fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

impl RunTemp {
    pub fn new() -> Self {
        Self { dir: std::sync::OnceLock::new(), next: std::sync::atomic::AtomicU64::new(0) }
    }

    /// A new file holding `bytes`, named `<n>-<file name of name>` so the helper still sees the
    /// extension.
    pub fn write(&self, name: &std::path::Path, bytes: &[u8]) -> std::io::Result<TempFile> {
        let dir = self
            .dir
            .get_or_init(|| tempfile::Builder::new().prefix("sfor-").tempdir().map_err(|e| e.to_string()))
            .as_ref()
            .map_err(|e| std::io::Error::other(e.clone()))?;
        let n = self.next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        // `<stdin>` and other names with characters Windows refuses become `_` (M5's Windows).
        let file: String = name
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_else(|| "stdin".into())
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') { c } else { '_' })
            .collect();
        let p = dir.path().join(format!("{n}-{file}"));
        std::fs::write(&p, bytes)?;
        Ok(TempFile(p))
    }
}

impl Default for RunTemp {
    fn default() -> Self {
        Self::new()
    }
}
```

  `src/search.rs`: `fn walker_threads` becomes `pub fn walker_threads` (C10 sizes the pools
  with it). `src/app.rs`: `load_registry` passes `&processors::Processors::known` instead of
  `&|p| processors::by_name(p).is_some()`, so a user Definition may say `external`.

- [ ] **Step 4:** `cargo test` passes. Commit «Add the per-run Processor registry and the run's
  temporary files».

### Task 1.6: the Detector reads content only for files without an extension (D15)

**Files:** `src/formats/detector.rs`, and the `known` helpers of the tests in
`src/formats/{mod,definition}.rs`.

- [ ] **Step 1: tests first.** In `detector.rs`'s tests: `known` becomes
  `crate::processors::Processors::known` (so the built-ins of C3 and C6 load in every test);
  `bashy`'s shebang becomes `fish` (C6's built-in `bash` claims `bash`, and built-ins come
  first); `signals_in_order` uses `#!/usr/bin/env -S fish -e`. New test:

```rust
    #[test]
    fn content_signals_need_a_file_without_extension() {
        let reg = registry();
        let det = Detector::new(&reg, &[]).unwrap();
        // A shebang or magic bytes decide only when the name has no extension (M2 plan, D15).
        assert_eq!(det.detect(Path::new("run"), b"#!/usr/bin/env fish\n").unwrap().name, "bashy");
        assert!(det.detect(Path::new("run.txt"), b"#!/usr/bin/env fish\n").is_none());
        assert!(det.detect(Path::new("prog.bin"), b"\x7fELF\x02").is_none());
        assert_eq!(det.detect(Path::new(".bashrc_like"), b"\x7fELF").unwrap().name, "elfy");
        // So a file with an unclaimed extension is Plain from its path alone.
        assert_eq!(det.detect_by_path(Path::new("data.txt")), Some(None));
        assert_eq!(det.detect_by_path(Path::new("data")), None);
    }
```

  In `the_path_alone_decides_unless_a_format_reads_the_content`, `name("data.txt")` becomes
  `name("data")` (the extension-less case still needs the content). In `formats/mod.rs` and
  `definition.rs` tests, `known` becomes `crate::processors::Processors::known`;
  `list_shows_name_origin_and_extensions` stops counting lines and finds its two lines by name:

```rust
        let line = |name: &str| lines.iter().find(|l| l.starts_with(&format!("{name} "))).copied().unwrap();
        assert!(line("markdown").contains("built-in"));
        assert!(line("markdown").ends_with("md,markdown,mdx,mkd,mdown"));
        assert!(line("notes").contains(&format!("user ({})", d.path().join("notes.toml").display())));
        assert!(line("notes").ends_with("note,nt"));
        assert_eq!(lines.len(), builtin::BUILTIN.len() + 1);
```

- [ ] **Step 2:** RED: `content_signals_need_a_file_without_extension` fails on `run.txt`.

- [ ] **Step 3:** in `Detector::detect`, after the extension signal and before magic bytes:

```rust
        // Magic bytes and shebangs decide only for a name without extension (M2 plan, D15).
        if path.extension().is_some() {
            return None;
        }
```

  and in `detect_by_path`, before computing `by_content`:

```rust
        if path.extension().is_some() {
            return Some(None);
        }
```

  Update both doc comments. Rust's `Path::extension` gives None for `.bashrc`, so dotfiles keep
  their content signals.

- [ ] **Step 4:** `cargo test` passes, including every M1 end-to-end test. Commit «Read magic
  bytes and shebangs only for files without an extension».

---

## Chunk 2: keypath — the collector and JSON

Rules: spec §5.2 and design note 6. Kinds `key` and `element`; the key that holds an array is
hidden; a key's Section runs from the key to the end of its value, its label is the key token
(quotes included), and its name piece is the text between the quotes (verbatim when there is
no backslash, decoded otherwise). An element's label is empty.

### Task 2.1: collect.rs

**Files:** `src/processors/keypath/collect.rs`.

- [ ] **Step 1: failing tests**:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn seg(kind: &str, name: &str, start: usize) -> OutlineSegment {
        OutlineSegment::new(kind, name, start..start, start..start)
    }

    /// a [0,20) { b [2,8) , c [10,18) { d [12,16) } }
    fn walk(c: &mut Collector) {
        c.open(seg("key", "a", 0));
        c.open(seg("key", "b", 2));
        c.close(8);
        c.open(seg("key", "c", 10));
        c.open(seg("key", "d", 12));
        c.close(16);
        c.close(18);
        c.close(20);
    }

    fn names(o: &Outline) -> Vec<&str> {
        o.segments.iter().map(|s| s.segment.name.as_str()).collect()
    }

    #[test]
    fn keeps_only_segments_that_hold_an_anchor() {
        let src = vec![b'x'; 30];
        let mut c = Collector::for_anchors(&[13]);
        walk(&mut c);
        assert_eq!(names(&c.finish(20, &src)), ["a", "c", "d"]);
        let mut c = Collector::for_anchors(&[3, 25]);
        walk(&mut c);
        assert_eq!(names(&c.finish(30, &src)), ["a", "b"]);
    }

    #[test]
    fn a_start_is_inside_and_an_end_is_outside() {
        let src = vec![b'x'; 30];
        let mut c = Collector::for_anchors(&[2, 8]);
        walk(&mut c);
        // 2 is inside b; 8 is past b's end, still inside a.
        assert_eq!(names(&c.finish(20, &src)), ["a", "b"]);
    }

    #[test]
    fn names_keep_everything_and_finish_closes_what_is_open() {
        let src = vec![b'x'; 30];
        let mut c = Collector::for_names();
        c.open(seg("key", "a", 0));
        c.open(seg("key", "b", 2));
        // A syntax error at 5: both are closed there.
        let o = c.finish(5, &src);
        assert_eq!(names(&o), ["a", "b"]);
        assert!(o.segments.iter().all(|s| s.range.end == 5));
    }

    #[test]
    fn hide_top_and_reveal() {
        let src = vec![b'x'; 30];
        let mut c = Collector::for_names();
        c.open(seg("key", "ports", 0));
        c.hide_top();
        c.open(seg("element", "ports[0]", 2));
        c.close(4);
        c.close(6);
        let mut d = seg("document", "document 1", 6);
        d.hidden = true;
        c.open(d);
        c.close(8);
        c.reveal("document");
        let o = c.finish(30, &src);
        let hidden: Vec<bool> = o.segments.iter().map(|s| s.hidden).collect();
        assert_eq!(hidden, [true, false, false]);
    }
}
```

- [ ] **Step 2:** RED (`Collector` missing).

- [ ] **Step 3:**

```rust
//! Builds an Outline from the open and close events of one walk over a file, in file order
//! (design note 6). For Trails it keeps only the Segments open at some anchor, so memory grows
//! with depth × anchors, not with the file; for `--in-headings` it keeps every Segment.
use crate::trail::{Outline, OutlineSegment};

pub struct Collector<'a> {
    /// Sorted ascending.
    anchors: &'a [usize],
    all: bool,
    /// First anchor not yet passed.
    next: usize,
    /// Open Segments, innermost last, with whether an anchor fell inside. A kept Segment has
    /// only kept Segments under it in the stack.
    stack: Vec<(OutlineSegment, bool)>,
    out: Vec<OutlineSegment>,
}

impl<'a> Collector<'a> {
    pub fn for_anchors(anchors: &'a [usize]) -> Self {
        debug_assert!(anchors.is_sorted());
        Self { anchors, all: false, next: 0, stack: Vec::new(), out: Vec::new() }
    }

    pub fn for_names() -> Collector<'static> {
        Collector { anchors: &[], all: true, next: 0, stack: Vec::new(), out: Vec::new() }
    }

    /// Every anchor before `at` lies inside every open Segment.
    fn pass(&mut self, at: usize) {
        let before = self.next;
        while self.next < self.anchors.len() && self.anchors[self.next] < at {
            self.next += 1;
        }
        if self.next > before {
            for (_, keep) in self.stack.iter_mut().rev() {
                if *keep {
                    break;
                }
                *keep = true;
            }
        }
    }

    /// A Segment starting at `seg.range.start`; `close` sets its end.
    pub fn open(&mut self, seg: OutlineSegment) {
        self.pass(seg.range.start);
        self.stack.push((seg, false));
    }

    /// Hides the innermost open Segment: a key or element whose value is an array (spec §5.2).
    pub fn hide_top(&mut self) {
        if let Some((s, _)) = self.stack.last_mut() {
            s.hidden = true;
        }
    }

    /// Shows every Segment of `kind`, closed or open: YAML documents once there are two.
    pub fn reveal(&mut self, kind: &str) {
        for s in self.out.iter_mut().chain(self.stack.iter_mut().map(|(s, _)| s)) {
            if s.segment.kind == kind {
                s.hidden = false;
            }
        }
    }

    /// Closes the innermost open Segment at `end` (exclusive).
    pub fn close(&mut self, end: usize) {
        self.pass(end);
        if let Some((mut s, keep)) = self.stack.pop() {
            s.range.end = end.max(s.range.start);
            if keep || self.all {
                self.out.push(s);
            }
        }
    }

    /// Closes what is still open at `end` (the end of the file, or a syntax error) and builds
    /// the Outline.
    pub fn finish(mut self, end: usize, src: &[u8]) -> Outline {
        while !self.stack.is_empty() {
            self.close(end);
        }
        Outline::build(self.out, src)
    }
}
```

- [ ] **Step 4:** GREEN. Commit «Add the keypath collector».

### Task 2.2: json.rs and the Keypath Processor

**Files:** `src/processors/keypath/{mod,json}.rs`, `tests/fixtures/data/config.json`,
`tests/fixtures/data/broken.json`, `tests/fixtures/data/lines.jsonl`.

`tests/fixtures/data/config.json` (16 lines; line numbers matter):

```json
{
  // Deploy settings
  "server": {
    "host": "example.org",
    "ports": [
      {"host": "a.example", "port": 80},
      {"host": "b.example", "port": 443},
      {"host": "needle.example", "port": 8443},
    ],
    "matrix": [[1, 2], [3, "needle"]]
  },
  /* a block
     comment with needle */
  "caf\u00e9": "needle",
  "needle_key": true
}
```

`tests/fixtures/data/broken.json`: `{"a": {"b": "needle"}, "c": ], "d": "needle"}` and a
newline. `tests/fixtures/data/lines.jsonl`: two lines, `{"a": "needle"}` and
`{"b": {"c": "needle"}}`.

- [ ] **Step 1: failing tests** in `src/processors/keypath/mod.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Input;
    use std::path::Path;

    // `toml` here is the sibling module: the crate is `::toml`.
    pub(super) fn params(syntax: &str) -> ::toml::Table {
        ::toml::from_str(&format!("syntax = '{syntax}'")).unwrap()
    }

    /// The Trail names of every `needle` in `src`, in order.
    pub(super) fn needles(src: &[u8], syntax: &str) -> Vec<Vec<String>> {
        let anchors: Vec<usize> = memchr::memmem::find_iter(src, b"needle").collect();
        let input = Input { bytes: src, path: Path::new("x"), format: syntax, disk: None };
        Keypath
            .trails(&input, &params(syntax), &anchors)
            .unwrap()
            .iter()
            .map(|t| t.iter().map(|s| s.name.clone()).collect())
            .collect()
    }

    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/data").join(name)).unwrap()
    }

    #[test]
    fn json_keys_indexes_and_comments() {
        assert_eq!(
            needles(&fixture("config.json"), "json"),
            vec![
                vec!["server", "ports[2]", "host"],
                vec!["server", "matrix[1][1]"],
                vec![],          // inside a comment, between top-level keys
                vec!["café"],
                vec![],          // in the key's own text (its label)
            ]
        );
    }

    #[test]
    fn json_sections_and_kinds() {
        let src = fixture("config.json");
        let at = memchr::memmem::find(&src, b"needle.example").unwrap();
        let input = Input { bytes: &src, path: Path::new("x"), format: "json", disk: None };
        let t = &Keypath.trails(&input, &params("json"), &[at]).unwrap()[0];
        let got: Vec<(&str, &str, u64, u64)> =
            t.iter().map(|s| (s.kind.as_str(), s.name.as_str(), s.line, s.end_line)).collect();
        assert_eq!(got, [("key", "server", 3, 11), ("element", "ports[2]", 8, 8), ("key", "host", 8, 8)]);
    }

    #[test]
    fn json_top_level_arrays_lines_and_errors() {
        assert_eq!(needles(b"[1, [2, \"needle\"]]\n", "json"), vec![vec!["[1][1]"]]);
        assert_eq!(needles(&fixture("lines.jsonl"), "json"), vec![vec!["a"], vec!["b", "c"]]);
        // The Trails before the syntax error stay; the ones after it are empty (spec §5.2).
        assert_eq!(needles(&fixture("broken.json"), "json"), vec![vec!["a", "b"], vec![]]);
    }

    #[test]
    fn json_names_carry_decoded_pieces() {
        let src = fixture("config.json");
        let input = Input { bytes: &src, path: Path::new("x"), format: "json", disk: None };
        let names = Keypath.names(&input, &params("json")).unwrap();
        let cafe = names.iter().find(|n| n.text == "café").unwrap();
        let raw = memchr::memmem::find(&src, b"caf\\u00e9").unwrap();
        assert_eq!(cafe.pieces, vec![(0.."café".len(), raw..raw + "caf\\u00e9".len())]);
        let host = names.iter().find(|n| n.text == "host").unwrap();
        assert_eq!(host.pieces[0].0.len(), host.pieces[0].1.len());
        assert_eq!(names.iter().filter(|n| n.text == "host").count(), 4);
    }

    #[test]
    fn anchors_in_any_order_get_their_own_trails() {
        let src = br#"{"a": "needle", "b": "needle"}"#;
        let first = memchr::memmem::find(src, b"needle").unwrap();
        let second = memchr::memmem::rfind(src, b"needle").unwrap();
        let input = Input { bytes: src, path: Path::new("x"), format: "json", disk: None };
        let t = Keypath.trails(&input, &params("json"), &[second, first]).unwrap();
        assert_eq!((t[0][0].name.as_str(), t[1][0].name.as_str()), ("b", "a"));
    }

    #[test]
    fn a_big_file_keeps_only_the_segments_of_its_anchors() {
        let mut src = String::from("{");
        for i in 0..20_000 {
            src.push_str(&format!("\"k{i}\": {{\"v\": {i}}},"));
        }
        src.push_str("\"last\": {\"v\": \"needle\"}}");
        let at = src.find("needle").unwrap();
        let o = outline_for(src.as_bytes(), Syntax::Json, &[at]);
        assert_eq!(o.segments.len(), 2);
    }

    #[test]
    fn lexical_errors_end_the_walk() {
        // JSONC allows comments and trailing commas, nothing else (D34, review r1, Codex #9).
        for bad in [
            r#"{"a": nope, "b": "needle"}"#,
            r#"{"a": 01, "b": "needle"}"#,
            r#"{"a": 1.e5, "b": "needle"}"#,
            r#"{"a": "x\q", "b": "needle"}"#,
            "{\"a\": \"tab\there\", \"b\": \"needle\"}",
            r#"{"a": truex, "b": "needle"}"#,
        ] {
            assert_eq!(needles(bad.as_bytes(), "json"), vec![Vec::<String>::new()], "{bad}");
        }
        assert_eq!(needles(br#"{"a": -0.5e+3, "b": "needle"}"#, "json"), vec![vec!["b"]]);
        assert_eq!(needles(br#"{"a": "\u00e9\n", "b": "needle"}"#, "json"), vec![vec!["b"]]);
    }

    #[test]
    fn deep_nesting_does_not_touch_the_call_stack() {
        // 200 000 nested objects, far past what recursion would survive on an 8 MiB stack:
        // the walk is iterative, and Segments stop at MAX_DEPTH (D34).
        let n = 200_000;
        let mut src = "{\"a\":".repeat(n);
        src.push_str("\"needle\"");
        src.push_str(&"}".repeat(n));
        let t = &needles(src.as_bytes(), "json")[0];
        assert_eq!(t.len(), json::MAX_DEPTH);
        let mut arrays = "[".repeat(n);
        arrays.push_str("\"needle\"");
        arrays.push_str(&"]".repeat(n));
        assert_eq!(needles(arrays.as_bytes(), "json").len(), 1);
    }

    #[test]
    fn syntax_is_required() {
        let input = Input { bytes: b"{}", path: Path::new("x"), format: "json", disk: None };
        let e = Keypath.trails(&input, &::toml::Table::new(), &[0]).unwrap_err();
        assert!(e.to_string().contains("`syntax`"), "{e}");
    }
}
```

- [ ] **Step 2:** RED.

- [ ] **Step 3:** `json.rs`, iterative (D34):

```rust
//! The JSON walker of `keypath` (spec §5.2): one iterative pass over the bytes with an explicit
//! stack, so nesting never touches the call stack. Comments (`//`, `/* */`) and trailing commas
//! are accepted (JSONC); everything else must be JSON: numbers, `true`, `false`, `null`, and
//! strings with valid escapes and no control characters. Several top-level values (JSON Lines)
//! are read in turn. Past `MAX_DEPTH` nested containers no more Segments open, so a deeper
//! Match gets the Trail of the deepest one (M2 plan, D34).
use super::collect::Collector;
use crate::trail::OutlineSegment;
use std::ops::Range;

pub const MAX_DEPTH: usize = 256;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Want {
    KeyOrEnd,
    Colon,
    Value,
    ValueOrEnd,
    CommaOrEnd,
}

/// One open object or array.
struct Frame {
    object: bool,
    want: Want,
    /// The next element's index (arrays).
    n: usize,
    /// The name an array's elements extend: `ports` → `ports[0]`. Empty past MAX_DEPTH.
    holder: String,
    /// The current member (key or element) opened a Segment.
    member: bool,
    /// The key read, waiting for its value: its decoded name, its range (quotes included),
    /// and whether it had escapes.
    key: Option<(String, Range<usize>, bool)>,
}

struct Scan<'s> {
    s: &'s [u8],
    i: usize,
}

impl Scan<'_> {
    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    /// Whitespace and comments.
    fn ws(&mut self) {
        loop {
            while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
                self.i += 1;
            }
            let rest = &self.s[self.i.min(self.s.len())..];
            if rest.starts_with(b"//") {
                self.i += memchr::memchr(b'\n', rest).unwrap_or(rest.len());
            } else if rest.starts_with(b"/*") {
                self.i += memchr::memmem::find(&rest[2..], b"*/").map_or(rest.len(), |k| k + 4);
            } else {
                return;
            }
        }
    }

    /// A string at `i` (on its `"`): its range, quotes included, and whether it has escapes.
    fn string(&mut self) -> Result<(Range<usize>, bool), usize> {
        let start = self.i;
        self.i += 1;
        let mut escaped = false;
        loop {
            match self.peek() {
                None => return Err(self.s.len()),
                Some(b'"') => {
                    self.i += 1;
                    return Ok((start..self.i, escaped));
                }
                Some(b'\\') => {
                    escaped = true;
                    match self.s.get(self.i + 1) {
                        Some(b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't') => self.i += 2,
                        Some(b'u') if self.s.get(self.i + 2..self.i + 6).is_some_and(|h| h.iter().all(u8::is_ascii_hexdigit)) => self.i += 6,
                        _ => return Err(self.i),
                    }
                }
                Some(b) if b < 0x20 => return Err(self.i),
                Some(_) => self.i += 1,
            }
        }
    }

    fn digits(&mut self) -> bool {
        let start = self.i;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.i += 1;
        }
        self.i > start
    }

    /// A scalar ends at a delimiter: `truex` and `01` are errors.
    fn delimited(&self) -> Result<(), usize> {
        match self.peek() {
            None | Some(b',' | b'}' | b']' | b' ' | b'\t' | b'\n' | b'\r' | b'/') => Ok(()),
            _ => Err(self.i),
        }
    }

    /// A number (RFC 8259 grammar), `true`, `false` or `null`.
    fn scalar(&mut self) -> Result<(), usize> {
        for lit in [&b"true"[..], b"false", b"null"] {
            if self.s[self.i..].starts_with(lit) {
                self.i += lit.len();
                return self.delimited();
            }
        }
        let start = self.i;
        if self.peek() == Some(b'-') {
            self.i += 1;
        }
        match self.peek() {
            Some(b'0') => self.i += 1,
            Some(b'1'..=b'9') => {
                self.digits();
            }
            _ => return Err(start),
        }
        if self.peek() == Some(b'.') {
            self.i += 1;
            if !self.digits() {
                return Err(self.i);
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.i += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.i += 1;
            }
            if !self.digits() {
                return Err(self.i);
            }
        }
        self.delimited()
    }
}

/// The text of a JSON string's contents, escapes decoded (lossy on invalid UTF-8). `string`
/// already checked the escapes.
pub(super) fn decode(raw: &[u8], escaped: bool) -> String {
    if !escaped {
        return String::from_utf8_lossy(raw).into_owned();
    }
    let mut out = String::new();
    let mut bytes = Vec::new();
    let mut i = 0;
    let flush = |bytes: &mut Vec<u8>, out: &mut String| {
        out.push_str(&String::from_utf8_lossy(bytes));
        bytes.clear();
    };
    while i < raw.len() {
        if raw[i] != b'\\' || i + 1 >= raw.len() {
            bytes.push(raw[i]);
            i += 1;
            continue;
        }
        flush(&mut bytes, &mut out);
        let c = raw[i + 1];
        i += 2;
        match c {
            b'n' => out.push('\n'),
            b't' => out.push('\t'),
            b'r' => out.push('\r'),
            b'b' => out.push('\u{8}'),
            b'f' => out.push('\u{c}'),
            b'u' => {
                let hex = |at: usize| raw.get(at..at + 4).and_then(|h| std::str::from_utf8(h).ok()).and_then(|h| u32::from_str_radix(h, 16).ok());
                let Some(hi) = hex(i) else { out.push('\u{fffd}'); continue };
                i += 4;
                let cp = if (0xD800..0xDC00).contains(&hi) && raw.get(i..i + 2) == Some(b"\\u") {
                    match hex(i + 2) {
                        Some(lo) if (0xDC00..0xE000).contains(&lo) => {
                            i += 6;
                            0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00)
                        }
                        _ => hi,
                    }
                } else {
                    hi
                };
                out.push(char::from_u32(cp).unwrap_or('\u{fffd}'));
            }
            other => out.push(other as char),
        }
    }
    flush(&mut bytes, &mut out);
    out
}

/// Starts the value at `sc.i`: pushes a Frame for a container, or reads a scalar whole.
fn begin(sc: &mut Scan, stack: &mut Vec<Frame>, holder: String, last_end: &mut usize) -> Result<(), usize> {
    let frame = |object, want, holder| Frame { object, want, n: 0, holder, member: false, key: None };
    match sc.peek() {
        Some(b'{') => {
            sc.i += 1;
            stack.push(frame(true, Want::KeyOrEnd, String::new()));
        }
        Some(b'[') => {
            sc.i += 1;
            stack.push(frame(false, Want::ValueOrEnd, holder));
        }
        Some(b'"') => {
            sc.string()?;
            *last_end = sc.i;
        }
        Some(_) => {
            sc.scalar()?;
            *last_end = sc.i;
        }
        None => return Err(sc.i),
    }
    Ok(())
}

/// Walks `src`; returns where it stopped: `src.len()`, or the byte of the first syntax error.
pub fn walk(src: &[u8], c: &mut Collector) -> usize {
    let mut sc = Scan { s: src, i: 0 };
    let mut stack: Vec<Frame> = Vec::new();
    // Segments open in the collector: at most MAX_DEPTH.
    let mut open = 0usize;
    // End of the last complete value: where its member's Segment closes.
    let mut last_end = 0usize;
    loop {
        sc.ws();
        let Some(k) = stack.len().checked_sub(1) else {
            if sc.i >= src.len() {
                return src.len();
            }
            if let Err(at) = begin(&mut sc, &mut stack, String::new(), &mut last_end) {
                return at.min(src.len());
            }
            continue;
        };
        let (object, want) = (stack[k].object, stack[k].want);
        let Some(b) = sc.peek() else { return src.len() };
        match (want, b) {
            (Want::KeyOrEnd, b'}') | (Want::ValueOrEnd, b']') => {
                sc.i += 1;
                last_end = sc.i;
                stack.pop();
            }
            (Want::CommaOrEnd, b',') => {
                sc.i += 1;
                let f = &mut stack[k];
                if std::mem::take(&mut f.member) {
                    c.close(last_end);
                    open -= 1;
                }
                f.want = if object { Want::KeyOrEnd } else { Want::ValueOrEnd };
            }
            (Want::CommaOrEnd, b'}') if object => {
                sc.i += 1;
                if stack[k].member {
                    c.close(last_end);
                    open -= 1;
                }
                stack.pop();
                last_end = sc.i;
            }
            (Want::CommaOrEnd, b']') if !object => {
                sc.i += 1;
                if stack[k].member {
                    c.close(last_end);
                    open -= 1;
                }
                stack.pop();
                last_end = sc.i;
            }
            (Want::KeyOrEnd, b'"') => {
                let (r, escaped) = match sc.string() {
                    Ok(x) => x,
                    Err(at) => return at.min(src.len()),
                };
                let name = if open < MAX_DEPTH { decode(&src[r.start + 1..r.end - 1], escaped) } else { String::new() };
                stack[k].key = Some((name, r, escaped));
                stack[k].want = Want::Colon;
            }
            (Want::Colon, b':') => {
                sc.i += 1;
                stack[k].want = Want::Value;
            }
            (Want::Value | Want::ValueOrEnd, _) => {
                let f = &mut stack[k];
                let holder = if object {
                    let (name, r, _) = f.key.take().expect("Want::Value follows a key");
                    if open < MAX_DEPTH {
                        let inner = r.start + 1..r.end - 1;
                        let mut seg = OutlineSegment::new("key", name.clone(), r.start..r.start, r.clone());
                        seg.name = Some(vec![(0..name.len(), inner.clone())]);
                        seg.name_anchor = inner.start;
                        c.open(seg);
                        f.member = true;
                        open += 1;
                    }
                    name
                } else {
                    let name = if open < MAX_DEPTH { format!("{}[{}]", f.holder, f.n) } else { String::new() };
                    f.n += 1;
                    if open < MAX_DEPTH {
                        c.open(OutlineSegment::new("element", name.clone(), sc.i..sc.i, sc.i..sc.i));
                        f.member = true;
                        open += 1;
                    }
                    name
                };
                if f.member && b == b'[' {
                    c.hide_top();
                }
                f.want = Want::CommaOrEnd;
                if let Err(at) = begin(&mut sc, &mut stack, holder, &mut last_end) {
                    return at.min(src.len());
                }
            }
            _ => return sc.i,
        }
    }
}
```

  The match arms touch `stack[k]` through short borrows that end before `begin` takes the
  whole stack; if the borrow checker wants it, copy the two fields first, as written.

  `mod.rs`:

```rust
//! keypath: YAML, JSON and TOML key paths (spec §5.2, design note 6).
pub mod collect;
pub mod json;
pub mod toml;
pub mod yaml;

use super::Processor;
use crate::model::{Input, Name, Trail};
use crate::trail::{names_of, Outline, TrailCursor};
use collect::Collector;

pub struct Keypath;
// Tests of yaml.rs and toml.rs reach `outline_for`, `Syntax` and `tests::params` through
// `super::super`.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Syntax {
    Json,
    Yaml,
    Toml,
}

fn syntax(params: &::toml::Table) -> anyhow::Result<Syntax> {
    match params.get("syntax").and_then(|v| v.as_str()) {
        Some("json") => Ok(Syntax::Json),
        Some("yaml") => Ok(Syntax::Yaml),
        Some("toml") => Ok(Syntax::Toml),
        _ => anyhow::bail!("keypath needs `syntax` = \"json\", \"yaml\" or \"toml\" in [params]"),
    }
}

/// The Outline of `src` restricted to the Segments that hold an anchor (`anchors` sorted).
pub(crate) fn outline_for(src: &[u8], syntax: Syntax, anchors: &[usize]) -> Outline {
    match syntax {
        Syntax::Toml => toml::outline(src), // C3: TOML files are small; the whole Outline.
        _ => {
            let mut c = Collector::for_anchors(anchors);
            let end = walk(syntax, src, &mut c);
            c.finish(end, src)
        }
    }
}

fn walk(syntax: Syntax, src: &[u8], c: &mut Collector) -> usize {
    match syntax {
        Syntax::Json => json::walk(src, c),
        Syntax::Yaml => yaml::walk(src, c),
        Syntax::Toml => unreachable!("TOML builds its Outline directly"),
    }
}

impl Processor for Keypath {
    fn trails(&self, input: &Input, params: &::toml::Table, anchors: &[usize]) -> anyhow::Result<Vec<Trail>> {
        let syntax = syntax(params)?;
        let mut order: Vec<usize> = (0..anchors.len()).collect();
        order.sort_by_key(|&i| anchors[i]);
        let sorted: Vec<usize> = order.iter().map(|&i| anchors[i]).collect();
        let o = outline_for(input.bytes, syntax, &sorted);
        let mut cursor = TrailCursor::new(&o);
        let mut out = vec![Vec::new(); anchors.len()];
        for (k, &i) in order.iter().enumerate() {
            out[i] = cursor.trail(sorted[k]);
        }
        Ok(out)
    }

    fn names(&self, input: &Input, params: &::toml::Table) -> anyhow::Result<Vec<Name>> {
        let o = match syntax(params)? {
            Syntax::Toml => toml::outline(input.bytes),
            s => {
                let mut c = Collector::for_names();
                let end = walk(s, input.bytes, &mut c);
                c.finish(end, input.bytes)
            }
        };
        Ok(names_of(&o))
    }
}
```

  Until C3, `yaml::walk` and `toml::outline` are the stubs C1 wrote (review r1, Fable #5: C2
  does not touch those files). `toml` the module shadows the `toml` crate inside `keypath`:
  refer to the crate as `::toml`.

- [ ] **Step 4:** GREEN: `cargo test keypath`. Check by eye that `json_sections_and_kinds`
  gives `server` lines 3–11. Commit «Add the keypath Processor with JSON».

### Task 2.3: named hidden Segments stay searchable (D35)

**Files:** `src/trail.rs` (`names_of`), `src/processors/keypath/mod.rs` (test).

- [ ] **Step 1: failing tests.** In `keypath/mod.rs`:

```rust
    #[test]
    fn keys_that_hold_arrays_are_searchable_names() {
        let src = br#"{"server": {"ports": [80, 443]}}"#;
        let input = Input { bytes: src, path: Path::new("x"), format: "json", disk: None };
        let names = Keypath.names(&input, &params("json")).unwrap();
        let ports = names.iter().find(|n| n.text == "ports").expect("ports is a key");
        // Its Trail is itself under `server`, as for any named Segment.
        let t: Vec<&str> = ports.trail.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(t, ["server", "ports"]);
    }
```

  and in `src/trail.rs`'s tests, that a hidden Segment *without* a name (a Markdown table)
  still gives no Name, so M1's `--in-headings` output does not change.
- [ ] **Step 2:** RED (`names_of` filters `!s.hidden`, `src/trail.rs:232` at `46a4b74`).
- [ ] **Step 3:** in `names_of`, the filter becomes `s.name.is_some()`, and the chains that
  build `trail` and `labels` keep a hidden Segment only when it is the named one itself
  (`!x.hidden || j == i`). Update the doc comment («Every Segment that has a name range…;
  hidden ancestors are left out of its Trail»).
- [ ] **Step 4:** GREEN, and every M1 test still passes. Commit «Keep keys that hold arrays
  searchable by --in-headings».

---

## Chunk 3: keypath — YAML, TOML and their Definitions

### Task 3.1: yaml.rs

**Files:** `src/processors/keypath/yaml.rs`, `tests/fixtures/data/config.yaml`,
`tests/fixtures/data/multi.yaml`.

`tests/fixtures/data/config.yaml` (19 lines; the probe of «Verified for this plan» printed its
events):

```yaml
# Deploy settings
server:
  name: "ñandú"
  ports:
    - host: a.example
      port: 80
    - host: b.example
      port: 443
    - host: needle.example
      port: 8443
  motd: |
    line one
    a needle in a block

defaults: &defaults
  retries: 3
worker:
  <<: *defaults
  tag: needle
```

`tests/fixtures/data/multi.yaml`:

```yaml
first: needle
---
second:
  - needle
```

- [ ] **Step 1: failing tests**, in `yaml.rs`, using `keypath::tests::needles` from Task 2.2
  (a child module sees its parent's private test module, so `mod.rs` needs no change):

```rust
#[cfg(test)]
mod tests {
    use super::super::tests::needles;
    use std::path::Path;

    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/data").join(name)).unwrap()
    }

    #[test]
    fn yaml_keys_sequences_block_scalars_and_merge_keys() {
        assert_eq!(
            needles(&fixture("config.yaml"), "yaml"),
            vec![
                vec!["server", "ports[2]", "host"],   // spec §11 #4
                vec!["server", "motd"],
                vec!["worker", "tag"],
            ]
        );
    }

    #[test]
    fn character_offsets_become_bytes() {
        // "ñandú" is 5 characters and 7 bytes: a Trail after it must still be right.
        let src = "a: ñandú\nb:\n  c: needle\n".as_bytes();
        assert_eq!(needles(src, "yaml"), vec![vec!["b", "c"]]);
    }

    #[test]
    fn documents_appear_only_when_there_are_two() {
        assert_eq!(needles(&fixture("multi.yaml"), "yaml"), vec![vec!["document 1", "first"], vec!["document 2", "second[0]"]]);
        assert_eq!(needles(b"only: needle\n", "yaml"), vec![vec!["only"]]);
    }

    #[test]
    fn a_syntax_error_keeps_the_trails_before_it() {
        // A block sequence entry where a key is expected: saphyr-parser 0.2.2 stops at line 3
        // ("did not find expected key"), verified in the probe.
        let src = b"a:\n  b: needle\n- x\nd: needle\n";
        assert_eq!(needles(src, "yaml"), vec![vec!["a", "b"], vec![]]);
    }

    #[test]
    fn past_the_tree_sitter_yaml_row_limit() {
        // tree-sitter-yaml fails over 32 768 lines (spec §4.3); the native parser does not.
        let mut s = String::new();
        for i in 0..40_000 {
            s.push_str(&format!("k{i}: v\n"));
        }
        s.push_str("last:\n  deep: needle\n");
        assert_eq!(needles(s.as_bytes(), "yaml"), vec![vec!["last", "deep"]]);
    }

    #[test]
    fn flow_collections_end_at_their_bracket() {
        let src = b"a: {}\nb: needle\n";
        let o = super::super::outline_for(src, super::super::Syntax::Yaml, &[3]);
        let a = o.segments.iter().find(|s| s.segment.name == "a").unwrap();
        assert_eq!(a.range, 0..5);
    }

    #[test]
    fn a_multi_line_key_has_one_piece_per_line() {
        let src = b"? multi\n  line key\n: needle\n";
        let input = crate::model::Input { bytes: src, path: Path::new("x"), format: "yaml", disk: None };
        let names = super::super::Keypath.names(&input, &super::super::tests::params("yaml")).unwrap();
        let n = names.iter().find(|n| n.text == "multi line key").expect("the key");
        let lines: Vec<&str> = n.pieces.iter().map(|(_, r)| std::str::from_utf8(&src[r.clone()]).unwrap()).collect();
        assert_eq!(lines, ["multi", "line key"]);
    }

    #[test]
    fn an_escaped_line_break_keeps_each_part_on_its_line() {
        // saphyr gives "foobar": no space marks the join, so the pieces come from the source.
        let src = b"? \"foo\\\n  bar\"\n: needle\n";
        let input = crate::model::Input { bytes: src, path: Path::new("x"), format: "yaml", disk: None };
        let names = super::super::Keypath.names(&input, &super::super::tests::params("yaml")).unwrap();
        let n = names.iter().find(|n| n.text == "foobar").expect("the key");
        let line_of = |r: &std::ops::Range<usize>| src[..r.start].iter().filter(|&&b| b == b'\n').count() + 1;
        let bar = n.pieces.iter().find(|(t, _)| t.end == "foobar".len()).unwrap();
        assert_eq!(line_of(&bar.1), 2, "{:?}", n.pieces);
        assert_eq!(n.pieces.len(), 2);
    }

    #[test]
    fn deep_flow_nesting_is_a_syntax_error_not_a_crash() {
        // saphyr-parser stops at its recursion limit with an error (probe: "recursion limit
        // exceeded at byte 258"); the Trails before it stay.
        let mut s = String::from("a: needle\nb: ");
        s.push_str(&"[".repeat(100_000));
        s.push_str(&"]".repeat(100_000));
        s.push('\n');
        assert_eq!(needles(s.as_bytes(), "yaml"), vec![vec!["a"]]);
    }

    #[test]
    fn complex_keys_are_named_by_their_text() {
        let src = b"? [a, b]\n: needle\n";
        assert_eq!(needles(src, "yaml"), vec![vec!["[a, b]"]]);
    }
}
```

- [ ] **Step 2:** RED.

- [ ] **Step 3:** implement. The events (verified on the fixture): a scalar's span covers it, a
  literal block scalar through its last line; `MappingEnd`/`SequenceEnd` sit at the next token,
  so a container closes at `last_end`, the end of its last scalar or alias; `Marker.index`
  counts characters, so bytes come from `line()` (1-based) and `col()` (characters):

```rust
//! The YAML walker of `keypath` (spec §5.2), over saphyr-parser's events.
use super::collect::Collector;
use crate::trail::{LineIndex, OutlineSegment};
use saphyr_parser::{Event, Marker, Parser, ScalarStyle};

enum Frame {
    /// A mapping; `key` is true between a key and the end of its value.
    Map { key: bool, key_name: String },
    /// A sequence and the name its elements extend (`ports` → `ports[0]`).
    Seq { holder: String, n: usize },
    /// A complex key (`? [a, b]`) being skipped: where it started and its nesting.
    Complex { start: usize, depth: usize },
    Doc,
}

pub fn walk(src: &[u8], c: &mut Collector) -> usize {
    let Ok(text) = std::str::from_utf8(src) else { return 0 };
    let lines = LineIndex::new(src);
    let byte = |m: &Marker| -> usize {
        let start = lines.line_start(m.line() as u64).unwrap_or(src.len()).min(src.len());
        start + text[start..].char_indices().nth(m.col()).map_or(text.len() - start, |(b, _)| b)
    };
    let mut frames: Vec<Frame> = Vec::new();
    let mut docs = 0;
    let mut last_end = 0;
    let mut parser = Parser::new_from_str(text);
    while let Some(next) = parser.next() {
        let (ev, span) = match next {
            Ok(e) => e,
            Err(e) => return byte(e.marker()).min(src.len()),
        };
        let (s, e) = (byte(&span.start), byte(&span.end));
        if let Some(Frame::Complex { start, depth }) = frames.last_mut() {
            match ev {
                Event::MappingStart(..) | Event::SequenceStart(..) => *depth += 1,
                Event::MappingEnd | Event::SequenceEnd => {
                    *depth -= 1;
                    // A flow collection's end event spans its `]` or `}` (verified: `? [a, b]`).
                    last_end = last_end.max(e);
                }
                Event::Scalar(..) | Event::Alias(_) => last_end = e,
                _ => {}
            }
            if *depth == 0 {
                let start = *start;
                frames.pop();
                let raw = &text[start..last_end];
                let first = raw.lines().next().unwrap_or("");
                let name: String = first.chars().take(40).collect();
                c.open(OutlineSegment::new("key", name.clone(), start..start, start..last_end));
                if let Some(Frame::Map { key, key_name }) = frames.last_mut() {
                    *key = true;
                    *key_name = name;
                }
            }
            continue;
        }
        match ev {
            Event::DocumentStart(_) => {
                docs += 1;
                if docs == 2 {
                    c.reveal("document");
                }
                let mut d = OutlineSegment::new("document", format!("document {docs}"), s..s, s..s);
                d.hidden = docs == 1;
                c.open(d);
                frames.push(Frame::Doc);
            }
            Event::DocumentEnd => {
                frames.pop();
                c.close(last_end.max(s));
            }
            Event::MappingStart(..) | Event::SequenceStart(..) if expecting_key(&frames) => {
                frames.push(Frame::Complex { start: s, depth: 1 });
            }
            Event::MappingStart(..) => {
                begin_value(&mut frames, c, s);
                frames.push(Frame::Map { key: false, key_name: String::new() });
            }
            Event::SequenceStart(..) => {
                let at_top = matches!(frames.last(), None | Some(Frame::Doc));
                let holder = begin_value(&mut frames, c, s);
                if !at_top {
                    // The key or element that holds the sequence is hidden (spec §5.2).
                    c.hide_top();
                }
                frames.push(Frame::Seq { holder, n: 0 });
            }
            Event::MappingEnd | Event::SequenceEnd => {
                // A flow collection's end event spans its `]` or `}` (`a: {}` ends at byte 5); a
                // block collection's is empty, at the next token, so it ends at its last scalar.
                if e > s {
                    last_end = last_end.max(e);
                }
                frames.pop();
                end_value(&mut frames, c, last_end);
            }
            Event::Scalar(value, style, ..) if expecting_key(&frames) => {
                last_end = e;
                let mut k = OutlineSegment::new("key", value.to_string(), s..s, s..e);
                k.name = Some(vec![(0..value.len(), name_source(text, s..e, &value, style))]);
                k.name_anchor = s;
                c.open(k);
                if let Some(Frame::Map { key, key_name }) = frames.last_mut() {
                    *key = true;
                    *key_name = value.to_string();
                }
            }
            Event::Scalar(..) | Event::Alias(_) => {
                last_end = e;
                begin_value(&mut frames, c, s);
                end_value(&mut frames, c, e);
            }
            _ => {}
        }
    }
    src.len()
}
```

  The sketch above leaves three helpers to write, with these contracts (tests decide):
  - `expecting_key(&frames)`: the top frame is `Map { key: false, .. }`.
  - `begin_value(&mut frames, c, s) -> String`: in a `Seq`, opens the element Segment
    `"{holder}[{n}]"` (kind `element`, empty label) at `s`, increments `n`, and returns that
    name; in a `Map` with an open key, returns the key's name (no Segment: the key already
    holds the value); at the top of a document, returns `""`.
  - `end_value(&mut frames, c, end)`: when the top frame is a `Map` with an open key, closes
    the key at `end` and sets `key = false`; when it is a `Seq`, closes the element at `end`.
  - `name_pieces(text, s..e, value, style) -> Vec<(Range, Range)>` (replacing the single
    `name_source` piece of the sketch, which becomes `k.name = Some(name_pieces(…))`): one
    verbatim piece `s..e` when `text[s..e] == value` (plain), one over `s+1..e-1` when the scalar
    is quoted and the inside equals `value`. A key that spans several lines (only explicit `?`
    keys can) gets one piece per source line, as M1 maps each piece to the line it starts on
    (`app.rs` `heading_matches`). The pieces come from decoding the scalar's source line by
    line, keeping each line's source range, never from splitting `value` afterwards (review
    r2, Codex #3):
    - plain: each line's text without its indentation; lines are joined by one space (an
      empty line by a line break);
    - double-quoted: each line's text (the first after the opening `"`, the others without
      their indentation, the last before the closing `"`), escapes decoded (`\\`, `\"`, `\n`,
      `\t`, `\xNN`, `\uNNNN`, `\UNNNNNNNN`, and the others of YAML 1.2); a line that ends in an
      unescaped `\` joins the next one with nothing (`"foo\` + `bar"` is `foobar`), any other
      line break folds into one space (an empty line into a line break);
    - single-quoted: as double-quoted, with `''` as the only escape.
    Each line's decoded text is one decoded piece over that line's source part (verbatim when
    they are equal). When the joined pieces do not equal saphyr's `value`, the key gets one
    decoded piece over `s..e`, which still finds it by name.
  - `ScanError::marker()` gives the error's position (verified in saphyr-parser 0.2.2's
    source, `scanner.rs`).

- [ ] **Step 4:** GREEN. Commit «Add YAML key paths over saphyr-parser».

### Task 3.2: toml.rs

**Files:** `src/processors/keypath/toml.rs`, `tests/fixtures/data/shop.toml`.

`tests/fixtures/data/shop.toml` (the probe of «Verified for this plan» printed its spans):

```toml
title = "shop"
owner.name = "needle owner"

[server]
host = "example.org"
ports = [80, 443, 8443]

[[products]]
name = "first"

[[products]]
name = "second"
tags = ["a", "needle"]

[[products.variants]]
sku = "needle-sku"

[database.replica]
url = "postgres://needle"
inline = { user = "needle" }
```

`tests/fixtures/data/interleaved.toml` (subtables between other tables, an explicit parent
whose Section stops before its later subtable, and quoted keys holding `.` and `[`; its spans
were printed by the probe, `toml-interleaved.txt`):

```toml
[a.b]
k = "needle 1"

[x]
k = "needle 2"

[a.c]
k = "needle 3"

["a.b"]
k = "needle 4"

[site."x[0]"]
k = "needle 5"

[p]
k = "needle 6"

[q]
k = 1

[p.r]
k = "needle 7"
```

- [ ] **Step 1: failing tests** in `toml.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::super::tests::needles;
    use std::path::Path;

    fn fixture() -> Vec<u8> {
        std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/data/shop.toml")).unwrap()
    }

    #[test]
    fn toml_tables_arrays_of_tables_dotted_and_inline() {
        assert_eq!(
            needles(&fixture(), "toml"),
            vec![
                vec!["owner", "name"],
                vec!["products[1]", "tags[1]"],
                vec!["products[1]", "variants[0]", "sku"],
                vec!["database", "replica", "url"],
                vec!["database", "replica", "inline", "user"],
            ]
        );
    }

    #[test]
    fn a_table_runs_to_the_next_header_outside_it() {
        let src = fixture();
        let o = super::outline(&src);
        let p1 = o.segments.iter().find(|s| s.segment.name == "products[1]").unwrap();
        let db = o.segments.iter().find(|s| s.segment.name == "database").unwrap();
        // products[1] covers its [[products.variants]] and ends where [database.replica] starts.
        assert_eq!((p1.segment.line, p1.segment.end_line), (11, 17));
        assert_eq!(db.segment.line, 18);
        let ports = o.segments.iter().find(|s| s.segment.name == "ports").unwrap();
        assert!(ports.hidden);
        // The key of an array of tables: hidden in Trails, still a name.
        assert!(o.segments.iter().filter(|s| s.segment.name == "products").all(|s| s.hidden));
        assert!(crate::trail::names_of(&o).iter().any(|n| n.text == "products"));
    }

    #[test]
    fn interleaved_tables_and_quoted_keys() {
        let src = std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/data/interleaved.toml")).unwrap();
        assert_eq!(
            needles(&src, "toml"),
            vec![
                vec!["a", "b", "k"],
                vec!["x", "k"],          // never `a › x`: no Segment spans [a.b] to [a.c]
                vec!["a", "c", "k"],
                vec!["a.b", "k"],        // one key, quoted
                vec!["site", "x[0]", "k"],
                vec!["p", "k"],
                vec!["p", "r", "k"],     // [p]'s Section ended at [q]; `p` is given again
            ]
        );
    }

    #[test]
    fn dotted_keys_spread_over_a_table_do_not_wrap_their_neighbours() {
        let src = b"a.x = \"needle\"\nb = \"needle\"\na.y = \"needle\"\n";
        assert_eq!(needles(src, "toml"), vec![vec!["a", "x"], vec!["b"], vec!["a", "y"]]);
    }

    #[test]
    fn a_syntax_error_keeps_the_trails_before_it() {
        // toml_edit reports these at a later line than where the damage starts (review r1,
        // Codex #10); the walk back line by line finds the last prefix that parses.
        for src in [
            &b"[a]\nb = \"needle\"\n[c\nd = \"needle\"\n"[..],
            b"[a]\nb=\"needle\"\nx=[\n[c]\nd=\"needle\"\n",
            b"[a]\nb = \"needle\"\ns = \"\"\"\nopen\n[c]\nd = \"needle\"\n",
        ] {
            assert_eq!(needles(src, "toml"), vec![vec!["a", "b"], vec![]], "{}", String::from_utf8_lossy(src));
        }
    }

    #[test]
    fn recovery_does_not_assume_valid_prefixes_are_monotone() {
        // Review r3, Codex #3: cuts at lines 20, 19, 17, 13, 5 and 1 all fall inside a string or
        // give an empty document; the prefix ending at line 15 is the one that keeps `k`.
        let mut src = String::from("a = \"\"\"\n");
        for i in 2..=12 {
            src.push_str(&format!("line {i}\n"));
        }
        src.push_str("\"\"\"\n");
        src.push_str("k = \"needle\"\n");
        src.push_str("b = \"\"\"\n");
        for i in 16..=19 {
            src.push_str(&format!("open {i}\n"));
        }
        assert_eq!(needles(src.as_bytes(), "toml"), vec![vec!["k"]]);
    }
}
```

- [ ] **Step 2:** RED.

- [ ] **Step 3:** implement `pub fn outline(src: &[u8]) -> Outline`, building every Segment
  (TOML files are small; Cargo.lock at toml_edit's measured 150 ms/MB is the large case). No
  Segment ever covers a union of discontiguous parts (D36):

  1. `std::str::from_utf8` or an empty Outline. Parse with
     `text.parse::<toml_edit::Document<String>>()`. **On an error** (`TomlError::span()`,
     verified in toml_edit 0.25.17's source; `None` counts as the end of the file), cut `text`
     at the start of the error's line and parse the prefix; while that fails, step back 1, 2,
     4, 8… lines until a prefix parses (the candidate) or the start of the file. Valid prefixes
     are not monotone — a cut inside a multi-line string fails though a later one parses — so
     no interval is ruled out: next, every cut skipped above the candidate is tried from the
     highest down, and the first that parses wins; otherwise the candidate does (review r3,
     Codex #3). The whole recovery stops when the bytes it parsed pass 32 MiB (review r2,
     Fable #4: one parse costs 44–54 ms on a 1 MB Cargo.lock), keeping the best prefix found
     so far, or an empty Outline. Verified on Codex's counterexample in the probe
     (`tomlrec.txt`): a valid multi-line string on lines 1–13, `k = "needle"` on 14 and one left
     open on 15–19 recover the prefix that ends at line 15, which keeps `k`. Verified in the probe (`r1-probe.txt`): for
     `[a]\nb="needle"\nx=[\n[c]\nd="needle"\n` toml_edit reports byte 23 (line 5), and the
     walk back reaches the 15-byte prefix `[a]\nb="needle"\n` (cuts at lines 5, 4 and 3); an
     unclosed `"""` string recovers the same way. toml_edit's own recursion limit («max recursion depth met») is such
     an error.
  2. **Paths are components.** A path is `Vec<Step>`, `enum Step { Key(String), Index(usize) }`,
     from the decoded keys: `["a.b"]` is one `Key("a.b")`, never two. A path is under another
     when the other is a prefix of its steps.
  3. **Headers.** Walk the document and collect every explicit header: a table that is neither
     implicit nor dotted (`Table::span()` is its header), and every element of an array of
     tables (its `[[…]]` header; `Index(i)` counts the elements of that array under the same
     parent). Sort by start. A header's Section runs from its header to the start of the next
     header whose path is not under it, or to the end of the file.
  4. **Header Segments.** For each header, outermost first (`Outline::build` sorts stably, so
     a parent pushed first stays before a child with the same range): for each proper prefix
     of its path, if no explicit header with exactly that prefix has a Section that contains
     this header, push a Segment for that step (kind `key`, or `element` named `key[i]` for an
     index step) with this header's Section as range and this header's span as label; then the
     header's own Segment (kind `key`, or `element` `key[i]`), with its Section and its header
     span as label. A `Key` step followed by an `Index` step — `products` of `[[products]]` — is
     the key that holds an array: it is pushed with `hidden = true` and labelled by its key's
     span inside the header, so the Trail reads `products[1] › …` and `--in-headings
     '^products$'` still finds it (spec §5.2, D19, D35; review r2, Fable #2). So `[a.b]`, `[x]`, `[a.c]` give `a` twice, each over one header's Section,
     and `[p.r]` after `[q]` gets its own `p`.
  5. **Key-values**, inside each table's items: a key with a value gets a Segment from the key's
     start to the value's end (`Item::span()` is the value only, quotes included), labelled by
     its key span, with a name piece verbatim when the key span's text equals the key and one
     decoded piece over the span otherwise. A dotted key (`a.x = 1`, a dotted table) gives each
     component a Segment over that one key-value: from the first byte of its line after
     indentation to the value's end. An array value hides its key and gives each element
     (`Value::span()`) a Segment `key[i]`, recursively for nested arrays and inline tables; an
     inline table value recurses into its keys.
  6. `Outline::build(segments, src)`.

  `toml_edit` gives byte spans into `text`, which is `src`. Name pieces follow `Name`'s rules.

- [ ] **Step 4:** GREEN: `cargo test keypath`. Commit «Add TOML key paths over toml_edit».

### Task 3.3: the keypath Definitions, and t08

**Files:** `formats/{json,yaml,toml}.toml`, `src/formats/builtin.rs`, `tests/cli.rs` (t08),
`tests/snapshots/cli__t08_fragments.snap`.

- [ ] **Step 1:** the Definitions:

```toml
# formats/json.toml — JSON key paths (spec §5.2). Comments and trailing commas are accepted.
name       = "json"
extensions = ["json", "jsonc", "jsonl", "geojson", "webmanifest"]
names      = [".babelrc", ".eslintrc"]
processor  = "keypath"

[params]
syntax = "json"
```

```toml
# formats/yaml.toml — YAML key paths (spec §5.2).
name       = "yaml"
extensions = ["yaml", "yml"]
processor  = "keypath"

[params]
syntax = "yaml"
```

```toml
# formats/toml.toml — TOML key paths (spec §5.2).
name       = "toml"
extensions = ["toml"]
names      = ["Cargo.lock", "Pipfile", "poetry.lock", "uv.lock"]
processor  = "keypath"

[params]
syntax = "toml"
```

  and three lines in `BUILTIN`, after markdown and the code Definitions of C6 (alphabetical
  after `markdown`, which stays first: `definition.rs`'s test reads `BUILTIN[0]`).

- [ ] **Step 2:** `cargo test` now fails in `t08_long_line_becomes_fragments_with_counts`:
  `misc/min.json` is a JSON array, so its line gets the Trail `[0]`. Run
  `cargo run -- needle tests/fixtures/misc/min.json --flat --fragment-width 120 --color never`
  and read the prefix it prints (expected `1: [0] │ `; to verify here). Change the test's
  `strip_prefix("1: ")` to that prefix, keep every other assertion, regenerate the snapshot
  with `INSTA_UPDATE=always cargo test --test cli t08`, and read the new `.snap` in full: every
  Fragment still fits in 120 characters and the omitted counts still add up.

- [ ] **Step 3:** `cargo test --workspace` passes. Commit «Add the JSON, YAML and TOML
  Definitions».

---

## Chunk 4: the Host

Rules: spec §4.4 and design note 5. The host is the most concurrent code of searchfor: every
test below runs fake helpers in threads over `std::io::pipe`, so the timing of each case is
under the test's control, and none of them needs a real process.

### Task 4.1: test harness and failing tests

**Files:** `src/helper/host.rs` (tests module). The harness below (`Body`, `Fake`,
`FakeControl`, `req`, `helper`, `echo`) goes in `#[cfg(test)] pub(crate) mod testing`, and the
tests module starts with `use super::testing::*;`: C9's `external.rs` tests reuse it.

- [ ] **Step 1:** the fake launcher. A fake helper is a closure that gets the request lines,
  a writer for its answers and a `killed` flag it must check while it waits, so `Control::kill`
  can stop it:

```rust
/// The fake launcher, shared with C9's `external.rs` tests (every item is `pub(crate)`).
#[cfg(test)]
pub(crate) mod testing {
    use super::*;
    use crate::helper::protocol::{line, MatchPos, Reply, Request};
    use crate::helper::{Control, HostConfig, Launcher, Mode, Proc};
    use std::io::{BufRead, BufReader, Write};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::SeqCst};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    pub(crate) type Lines = std::io::Lines<BufReader<std::io::PipeReader>>;
    pub(crate) type Body = Arc<dyn Fn(&mut Lines, &mut std::io::PipeWriter, &AtomicBool) + Send + Sync>;

    pub(crate) struct Fake {
        pub(crate) body: Body,
        pub(crate) launches: Arc<AtomicUsize>,
        pub(crate) reaps: Arc<AtomicUsize>,
        pub(crate) fail: bool,
        /// Runs at the start of every launch (the barrier tests of D42).
        pub(crate) hook: Option<Arc<dyn Fn() + Send + Sync>>,
        /// How long a kill takes (the barrier tests of D42).
        pub(crate) slow_kill: Duration,
    }

    pub(crate) struct FakeControl {
        pub(crate) killed: Arc<AtomicBool>,
        pub(crate) done: Arc<AtomicBool>,
        pub(crate) reaps: Arc<AtomicUsize>,
        pub(crate) slow_kill: Duration,
    }

    impl Control for FakeControl {
        fn kill(&mut self) {
            std::thread::sleep(self.slow_kill);
            self.killed.store(true, SeqCst);
        }
        fn wait(&mut self, within: Duration) -> bool {
            let t = Instant::now();
            while !self.done.load(SeqCst) {
                if t.elapsed() > within {
                    return false;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            true
        }
        fn reap(&mut self) {
            // A fake "exits" when its body returns; reaping waits for that, as `waitpid` does.
            assert!(self.wait(Duration::from_secs(10)), "reap of a fake that never ended");
            self.reaps.fetch_add(1, SeqCst);
        }
    }

    impl Launcher for Fake {
        fn launch(&self) -> std::io::Result<Proc> {
            if let Some(h) = &self.hook {
                h();
            }
            if self.fail {
                return Err(std::io::Error::new(std::io::ErrorKind::NotFound, "no such command"));
            }
            self.launches.fetch_add(1, SeqCst);
            let (in_r, in_w) = std::io::pipe()?;
            let (out_r, mut out_w) = std::io::pipe()?;
            let (killed, done) = (Arc::new(AtomicBool::new(false)), Arc::new(AtomicBool::new(false)));
            let (body, k, d) = (self.body.clone(), killed.clone(), done.clone());
            std::thread::spawn(move || {
                let mut lines = BufReader::new(in_r).lines();
                body(&mut lines, &mut out_w, &k);
                d.store(true, SeqCst);
            });
            let (reaps, slow_kill) = (self.reaps.clone(), self.slow_kill);
            Ok(Proc { stdin: Box::new(in_w), stdout: Box::new(out_r), stderr: None, control: Box::new(FakeControl { killed, done, reaps, slow_kill }) })
        }
    }

    pub(crate) fn req(l: &str) -> Request {
        serde_json::from_str(l).unwrap()
    }

    /// Answers `hello`, then `answer` for every request until EOF or `None` (a crash).
    pub(crate) fn helper(answer: impl Fn(&Request) -> Option<Reply> + Send + Sync + 'static) -> Body {
        Arc::new(move |lines, out, _| {
            let Some(Ok(h)) = lines.next() else { return };
            let Request::Hello { .. } = req(&h) else { return };
            out.write_all(line(&Reply { id: 0, protocol: Some(1), ..Default::default() }).as_bytes()).unwrap();
            for l in lines.map_while(Result::ok) {
                let r = req(&l);
                match answer(&r) {
                    Some(a) => out.write_all(line(&a).as_bytes()).unwrap(),
                    None => return,
                }
            }
        })
    }

    /// One Trail with one Segment named after the path, per Match.
    pub(crate) fn echo(r: &Request) -> Option<Reply> {
        let Request::Trails { id, path, matches } = r else { return None };
        let seg = crate::model::Segment { kind: "key".into(), name: path.clone(), line: 1, end_line: 1, level: None, signature: None };
        Some(Reply { id: *id, trails: Some(vec![vec![seg]; matches.len()]), ..Default::default() })
    }

    pub(crate) fn cfg(mode: Mode, threads: usize, timeout_ms: u64) -> HostConfig {
        HostConfig { format: "ini".into(), command: vec!["fake".into()], mode, timeout: Duration::from_millis(timeout_ms), params: toml::Table::new(), threads }
    }

    pub(crate) struct Run {
        pub(crate) host: Arc<Host>,
        pub(crate) launches: Arc<AtomicUsize>,
        pub(crate) reaps: Arc<AtomicUsize>,
        pub(crate) warnings: Arc<Mutex<Vec<String>>>,
        pub(crate) sup: &'static cancel::Supervisor,
    }

    pub(crate) fn run(mode: Mode, threads: usize, timeout_ms: u64, body: Body) -> Run {
        let launches = Arc::new(AtomicUsize::new(0));
        let reaps = Arc::new(AtomicUsize::new(0));
        let fake = Fake { body, launches: launches.clone(), reaps: reaps.clone(), fail: false, hook: None, slow_kill: Duration::ZERO };
        run_with(mode, threads, timeout_ms, fake, launches, reaps)
    }

    /// `run` with a Fake built by the test and a Supervisor of the Host's own.
    pub(crate) fn run_with(mode: Mode, threads: usize, timeout_ms: u64, fake: Fake, launches: Arc<AtomicUsize>, reaps: Arc<AtomicUsize>) -> Run {
        let warnings = Arc::new(Mutex::new(Vec::new()));
        let w = warnings.clone();
        let sup: &'static cancel::Supervisor = Box::leak(Box::new(cancel::Supervisor::new(false)));
        let host = Arc::new(Host::with_supervisor(cfg(mode, threads, timeout_ms), Box::new(fake), Arc::new(move |m| w.lock().unwrap().push(m)), sup));
        Run { host, launches, reaps, warnings, sup }
    }

    pub(crate) fn trails(h: &Host, path: &str) -> Result<Reply, crate::helper::HelperError> {
        h.request(|id| Request::Trails { id, path: path.into(), matches: vec![MatchPos { line: 1, byte: 0 }] })
    }

    pub(crate) fn name_of(r: &Reply) -> &str {
        &r.trails.as_ref().unwrap()[0][0].name
    }
}
```

- [ ] **Step 2:** the tests, in their own module, which starts with:

```rust
#[cfg(test)]
mod tests {
    use super::testing::*;
    use super::*;
    use crate::helper::protocol::{line, MatchPos, Reply, Request};
    use crate::helper::Mode;
    use std::io::Write;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::SeqCst};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};
```

  and closes after the last test below. The tests:

```rust
    #[test]
    fn every_mode_answers_with_the_helpers_trails() {
        for mode in [Mode::Pool, Mode::Concurrent, Mode::Oneshot] {
            let r = run(mode, 2, 2000, helper(echo));
            assert_eq!(name_of(&trails(&r.host, "a.ini").unwrap()), "a.ini", "{mode:?}");
            assert_eq!(name_of(&trails(&r.host, "b.ini").unwrap()), "b.ini", "{mode:?}");
            let expected = if mode == Mode::Oneshot { 2 } else { 1 };
            assert_eq!(r.launches.load(SeqCst), expected, "{mode:?}");
            r.host.shutdown();
        }
    }

    #[test]
    fn a_pool_never_runs_more_copies_than_threads_and_gives_each_one_request() {
        let busy = Arc::new(AtomicUsize::new(0));
        let max = Arc::new(AtomicUsize::new(0));
        let (b, m) = (busy.clone(), max.clone());
        let body = helper(move |r| {
            let now = b.fetch_add(1, SeqCst) + 1;
            m.fetch_max(now, SeqCst);
            std::thread::sleep(Duration::from_millis(30));
            b.fetch_sub(1, SeqCst);
            echo(r)
        });
        let r = run(Mode::Pool, 3, 2000, body);
        std::thread::scope(|s| {
            for i in 0..12 {
                let h = r.host.clone();
                s.spawn(move || assert!(trails(&h, &format!("{i}.ini")).is_ok()));
            }
        });
        assert!(r.launches.load(SeqCst) <= 3, "{} copies", r.launches.load(SeqCst));
        assert!(max.load(SeqCst) <= 3);
        r.host.shutdown();
    }

    #[test]
    fn concurrent_answers_in_any_order_reach_their_request() {
        // Reads four requests, then answers them last to first.
        let body: Body = Arc::new(|lines, out, _| {
            let _ = lines.next();
            out.write_all(line(&Reply { id: 0, protocol: Some(1), ..Default::default() }).as_bytes()).unwrap();
            let mut got = Vec::new();
            for l in lines.map_while(Result::ok) {
                got.push(req(&l));
                if got.len() == 4 {
                    for r in got.drain(..).rev() {
                        out.write_all(line(&echo(&r).unwrap()).as_bytes()).unwrap();
                    }
                }
            }
        });
        let r = run(Mode::Concurrent, 4, 2000, body);
        std::thread::scope(|s| {
            for i in 0..4 {
                let h = r.host.clone();
                s.spawn(move || {
                    let path = format!("{i}.ini");
                    assert_eq!(name_of(&trails(&h, &path).unwrap()), path);
                });
            }
        });
        assert_eq!(r.launches.load(SeqCst), 1);
        r.host.shutdown();
    }

    #[test]
    fn hello_carries_params_and_threads_and_a_wrong_protocol_disables() {
        let seen = Arc::new(Mutex::new(None));
        let s = seen.clone();
        let body: Body = Arc::new(move |lines, out, _| {
            let h = lines.next().unwrap().unwrap();
            *s.lock().unwrap() = Some(req(&h));
            out.write_all(line(&Reply { id: 0, protocol: Some(2), ..Default::default() }).as_bytes()).unwrap();
        });
        let r = run(Mode::Concurrent, 7, 2000, body);
        assert_eq!(trails(&r.host, "a.ini"), Err(crate::helper::HelperError::Disabled));
        assert_eq!(trails(&r.host, "b.ini"), Err(crate::helper::HelperError::Disabled));
        let Some(Request::Hello { threads, format, .. }) = seen.lock().unwrap().clone() else { panic!() };
        assert_eq!((threads, format.as_str()), (7, "ini"));
        let w = r.warnings.lock().unwrap();
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(w[0].contains("ini") && w[0].contains("protocol"), "{w:?}");
    }

    #[test]
    fn an_error_answer_fails_only_its_request() {
        let r = run(Mode::Pool, 1, 2000, helper(|r| match r {
            Request::Trails { id, path, .. } if path == "bad.ini" => Some(Reply { id: *id, error: Some("cannot parse".into()), ..Default::default() }),
            other => echo(other),
        }));
        assert_eq!(trails(&r.host, "bad.ini"), Err(crate::helper::HelperError::Answer("cannot parse".into())));
        assert!(trails(&r.host, "good.ini").is_ok());
        assert_eq!(r.launches.load(SeqCst), 1);
    }

    #[test]
    fn a_crash_fails_its_request_and_the_next_one_starts_a_new_copy() {
        let r = run(Mode::Pool, 1, 2000, helper(|r| match r {
            Request::Trails { path, .. } if path == "crash.ini" => None,
            other => echo(other),
        }));
        assert!(matches!(trails(&r.host, "crash.ini"), Err(crate::helper::HelperError::Crashed(_))));
        assert!(trails(&r.host, "next.ini").is_ok());
        assert_eq!(r.launches.load(SeqCst), 2);
    }

    #[test]
    fn three_failures_disable_the_definition_with_one_warning() {
        let r = run(Mode::Pool, 1, 2000, helper(|_| None));
        for _ in 0..3 {
            assert!(matches!(trails(&r.host, "x.ini"), Err(crate::helper::HelperError::Crashed(_))));
        }
        assert_eq!(trails(&r.host, "x.ini"), Err(crate::helper::HelperError::Disabled));
        assert_eq!(r.launches.load(SeqCst), 3);
        assert_eq!(r.warnings.lock().unwrap().len(), 1);
    }

    #[test]
    fn a_timeout_fails_the_request_and_kills_the_copy() {
        let body: Body = Arc::new(|lines, out, killed| {
            let _ = lines.next();
            out.write_all(line(&Reply { id: 0, protocol: Some(1), ..Default::default() }).as_bytes()).unwrap();
            let _ = lines.next();
            while !killed.load(SeqCst) {
                std::thread::sleep(Duration::from_millis(5));
            }
        });
        let r = run(Mode::Pool, 1, 100, body);
        let t = Instant::now();
        assert_eq!(trails(&r.host, "slow.ini"), Err(crate::helper::HelperError::Timeout(100)));
        assert!(t.elapsed() < Duration::from_millis(1500), "{:?}", t.elapsed());
        // The killed copy is gone: the next request starts another.
        let _ = trails(&r.host, "again.ini");
        assert_eq!(r.launches.load(SeqCst), 2);
    }

    #[test]
    fn a_line_that_is_not_json_fails_and_restarts() {
        let body: Body = Arc::new(|lines, out, _| {
            let _ = lines.next();
            out.write_all(line(&Reply { id: 0, protocol: Some(1), ..Default::default() }).as_bytes()).unwrap();
            for l in lines.map_while(Result::ok) {
                if l.contains("garbage.ini") {
                    out.write_all(b"this is not json\n").unwrap();
                } else {
                    out.write_all(line(&echo(&req(&l)).unwrap()).as_bytes()).unwrap();
                }
            }
        });
        let r = run(Mode::Pool, 1, 2000, body);
        assert!(matches!(trails(&r.host, "garbage.ini"), Err(crate::helper::HelperError::Crashed(m)) if m.contains("not JSON")));
        assert!(trails(&r.host, "fine.ini").is_ok());
    }

    #[test]
    fn a_command_that_cannot_start_disables_with_one_warning() {
        let warnings = Arc::new(Mutex::new(Vec::new()));
        let w = warnings.clone();
        let fake = Fake { body: helper(echo), launches: Arc::new(AtomicUsize::new(0)), reaps: Arc::new(AtomicUsize::new(0)), fail: true, hook: None, slow_kill: Duration::ZERO };
        let host = Host::new(cfg(Mode::Pool, 4, 2000), Box::new(fake), Arc::new(move |m| w.lock().unwrap().push(m)));
        for _ in 0..5 {
            assert_eq!(trails(&host, "a.ini"), Err(crate::helper::HelperError::Disabled));
        }
        let w = warnings.lock().unwrap();
        assert_eq!(w.len(), 1);
        assert!(w[0].contains("cannot start"), "{w:?}");
    }

    #[test]
    fn shutdown_closes_stdin_so_the_helper_exits() {
        let exited = Arc::new(AtomicBool::new(false));
        let e = exited.clone();
        let inner = helper(echo);
        let body: Body = Arc::new(move |lines, out, k| {
            inner(lines, out, k);
            e.store(true, SeqCst);
        });
        let r = run(Mode::Pool, 1, 2000, body);
        assert!(trails(&r.host, "a.ini").is_ok());
        r.host.shutdown();
        assert!(exited.load(SeqCst));
        assert_eq!(trails(&r.host, "b.ini"), Err(crate::helper::HelperError::Disabled));
    }
```

```rust
    /// Answers `hello` and then never reads again, until killed (review r1, Codex #1).
    fn deaf() -> Body {
        Arc::new(|lines, out, killed| {
            let _ = lines.next();
            out.write_all(line(&Reply { id: 0, protocol: Some(1), ..Default::default() }).as_bytes()).unwrap();
            while !killed.load(SeqCst) {
                std::thread::sleep(Duration::from_millis(5));
            }
        })
    }

    #[test]
    fn a_request_larger_than_the_pipe_times_out_when_the_helper_does_not_read() {
        let r = run(Mode::Pool, 1, 200, deaf());
        let matches = vec![MatchPos { line: 1, byte: 0 }; 300_000]; // ~8 MB of JSON
        let t = Instant::now();
        let got = r.host.request(|id| Request::Trails { id, path: "big.ini".into(), matches });
        assert_eq!(got, Err(crate::helper::HelperError::Timeout(200)));
        assert!(t.elapsed() < Duration::from_secs(3), "{:?}", t.elapsed());
        r.host.shutdown();
        assert_eq!(r.reaps.load(SeqCst), r.launches.load(SeqCst));
    }

    #[test]
    fn every_process_is_reaped() {
        let r = run(Mode::Pool, 1, 2000, helper(|r| match r {
            Request::Trails { path, .. } if path == "crash.ini" => None,
            other => echo(other),
        }));
        let _ = trails(&r.host, "crash.ini");
        assert!(trails(&r.host, "a.ini").is_ok());
        // The crashed copy was reaped when it failed, before the run ends (D32).
        assert_eq!(r.reaps.load(SeqCst), 1);
        r.host.shutdown();
        assert_eq!(r.reaps.load(SeqCst), 2);
    }

    #[test]
    fn a_descendant_holding_the_pipes_cannot_hold_the_run() {
        // The helper hands a copy of its stdout to a "child" that outlives it (D32).
        let inner = helper(echo);
        let body: Body = Arc::new(move |lines, out, k| {
            let child = out.try_clone().unwrap();
            std::thread::spawn(move || {
                let _keep = child;
                std::thread::sleep(Duration::from_secs(10));
            });
            inner(lines, out, k);
        });
        let r = run(Mode::Pool, 1, 2000, body);
        assert!(trails(&r.host, "a.ini").is_ok());
        let t = Instant::now();
        r.host.shutdown();
        assert!(t.elapsed() < Duration::from_secs(3), "{:?}", t.elapsed());
    }

    #[test]
    fn concurrent_never_exceeds_its_capacity() {
        // Answers each request 50 ms after reading it, from its own thread.
        let outstanding = Arc::new(AtomicUsize::new(0));
        let max = Arc::new(AtomicUsize::new(0));
        let (o, m) = (outstanding.clone(), max.clone());
        let body: Body = Arc::new(move |lines, out, _| {
            let _ = lines.next();
            out.write_all(line(&Reply { id: 0, protocol: Some(1), ..Default::default() }).as_bytes()).unwrap();
            let out = Arc::new(Mutex::new(out.try_clone().unwrap()));
            std::thread::scope(|s| {
                for l in lines.map_while(Result::ok) {
                    let now = o.fetch_add(1, SeqCst) + 1;
                    m.fetch_max(now, SeqCst);
                    let (out, o) = (out.clone(), o.clone());
                    s.spawn(move || {
                        std::thread::sleep(Duration::from_millis(50));
                        o.fetch_sub(1, SeqCst);
                        out.lock().unwrap().write_all(line(&echo(&req(&l)).unwrap()).as_bytes()).unwrap();
                    });
                }
            });
        });
        let r = run(Mode::Concurrent, 2, 2000, body);
        std::thread::scope(|s| {
            for i in 0..6 {
                let h = r.host.clone();
                s.spawn(move || assert!(trails(&h, &format!("{i}.ini")).is_ok()));
            }
        });
        assert!(max.load(SeqCst) <= 2, "{} in flight", max.load(SeqCst));
        r.host.shutdown();
    }

    #[test]
    fn a_line_without_an_id_fails_every_pending_request() {
        // D41: an unattributable line breaks the process.
        let body: Body = Arc::new(|lines, out, _| {
            let _ = lines.next();
            out.write_all(line(&Reply { id: 0, protocol: Some(1), ..Default::default() }).as_bytes()).unwrap();
            let mut n = 0;
            for _ in lines.map_while(Result::ok) {
                n += 1;
                if n == 3 {
                    out.write_all(b"{\"oops\":1}\n").unwrap();
                }
            }
        });
        let r = run(Mode::Concurrent, 3, 2000, body);
        std::thread::scope(|s| {
            for i in 0..3 {
                let h = r.host.clone();
                s.spawn(move || {
                    let got = trails(&h, &format!("{i}.ini"));
                    assert!(matches!(&got, Err(crate::helper::HelperError::Crashed(m)) if m.contains("without an id")), "{got:?}");
                });
            }
        });
    }

    #[test]
    fn a_malformed_answer_with_its_id_fails_only_that_request() {
        // D41: valid JSON with the right id, but `trails` is not a list of Trails.
        let body: Body = Arc::new(|lines, out, _| {
            let _ = lines.next();
            out.write_all(line(&Reply { id: 0, protocol: Some(1), ..Default::default() }).as_bytes()).unwrap();
            for l in lines.map_while(Result::ok) {
                match req(&l) {
                    Request::Trails { id, path, .. } if path == "odd.ini" => {
                        out.write_all(format!("{{\"id\":{id},\"trails\":\"nope\"}}\n").as_bytes()).unwrap()
                    }
                    other => out.write_all(line(&echo(&other).unwrap()).as_bytes()).unwrap(),
                }
            }
        });
        let r = run(Mode::Pool, 1, 2000, body);
        assert!(matches!(trails(&r.host, "odd.ini"), Err(crate::helper::HelperError::Protocol(_))));
        assert!(trails(&r.host, "next.ini").is_ok());
        assert_eq!(r.launches.load(SeqCst), 1);
    }

    #[test]
    fn oneshot_disables_a_helper_that_never_answers_hello() {
        // Review r2, Codex #5: the handshake policy of the other modes, in oneshot too.
        let body: Body = Arc::new(|lines, _out, killed| {
            for _ in lines.map_while(Result::ok) {}
            while !killed.load(SeqCst) {
                std::thread::sleep(Duration::from_millis(5));
            }
        });
        let r = run(Mode::Oneshot, 1, 100, body);
        assert_eq!(trails(&r.host, "a.ini"), Err(crate::helper::HelperError::Disabled));
        assert_eq!(trails(&r.host, "b.ini"), Err(crate::helper::HelperError::Disabled));
        assert_eq!(r.launches.load(SeqCst), 1);
        assert_eq!(r.reaps.load(SeqCst), 1);
        assert_eq!(r.warnings.lock().unwrap().len(), 1);
    }

    #[test]
    fn a_copy_that_exits_between_requests_is_not_a_failure() {
        // Review r2, Fable #7: each copy answers one request and exits with nothing in
        // flight; five such exits never reach the three-failure limit.
        let body: Body = Arc::new(|lines, out, _| {
            let _ = lines.next();
            out.write_all(line(&Reply { id: 0, protocol: Some(1), ..Default::default() }).as_bytes()).unwrap();
            if let Some(Ok(l)) = lines.next() {
                out.write_all(line(&echo(&req(&l)).unwrap()).as_bytes()).unwrap();
            }
        });
        let r = run(Mode::Pool, 1, 2000, body);
        for i in 0..5 {
            assert!(trails(&r.host, &format!("{i}.ini")).is_ok(), "request {i}");
            std::thread::sleep(Duration::from_millis(50));
        }
        assert_eq!(r.launches.load(SeqCst), 5);
        assert!(r.warnings.lock().unwrap().is_empty());
    }

    #[test]
    fn cancel_kills_and_reaps_every_live_process_and_refuses_new_ones() {
        // D42: what the signal watcher does, on the Host's own Supervisor.
        let r = run(Mode::Pool, 2, 2000, deaf());
        std::thread::scope(|s| {
            for i in 0..2 {
                let h = r.host.clone();
                s.spawn(move || {
                    let _ = h.request(|id| Request::Trails { id, path: format!("{i}.ini"), matches: vec![] });
                });
            }
            std::thread::sleep(Duration::from_millis(100));
            r.sup.cancel();
        });
        assert_eq!(r.reaps.load(SeqCst), r.launches.load(SeqCst));
        let before = r.launches.load(SeqCst);
        assert_eq!(trails(&r.host, "late.ini"), Err(crate::helper::HelperError::Disabled));
        assert_eq!(r.launches.load(SeqCst), before);
    }

    #[test]
    fn a_signal_during_the_first_launch_still_reaps_it() {
        // Codex r3 #1: the cancellation arrives while the first process is being spawned.
        let inside = Arc::new(std::sync::Barrier::new(2));
        let go = Arc::new(std::sync::Barrier::new(2));
        let (i2, g2) = (inside.clone(), go.clone());
        let (launches, reaps) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
        let fake = Fake {
            body: deaf(),
            launches: launches.clone(),
            reaps: reaps.clone(),
            fail: false,
            hook: Some(Arc::new(move || {
                i2.wait();
                g2.wait();
            })),
            slow_kill: Duration::ZERO,
        };
        let r = run_with(Mode::Pool, 1, 2000, fake, launches, reaps);
        std::thread::scope(|s| {
            let h = r.host.clone();
            s.spawn(move || {
                let _ = trails(&h, "a.ini");
            });
            inside.wait(); // the spawn has begun, under the Supervisor's lock
            let sup = r.sup;
            let canceller = s.spawn(move || sup.cancel());
            std::thread::sleep(Duration::from_millis(50)); // the cancel is waiting for the lock
            go.wait();
            canceller.join().unwrap();
        });
        assert_eq!(r.launches.load(SeqCst), 1);
        assert_eq!(r.reaps.load(SeqCst), 1);
    }

    #[test]
    fn a_launch_during_the_cleanup_is_refused() {
        // Codex r3 #1: while the kills run, a worker needing a new copy gets none.
        let (launches, reaps) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
        let fake = Fake { body: deaf(), launches: launches.clone(), reaps: reaps.clone(), fail: false, hook: None, slow_kill: Duration::from_millis(300) };
        let r = run_with(Mode::Pool, 2, 5000, fake, launches, reaps);
        std::thread::scope(|s| {
            let h = r.host.clone();
            s.spawn(move || {
                let _ = trails(&h, "busy.ini"); // copy 1, busy until killed
            });
            std::thread::sleep(Duration::from_millis(100));
            let sup = r.sup;
            s.spawn(move || sup.cancel()); // its kill takes 300 ms
            std::thread::sleep(Duration::from_millis(100));
            // Copy 1 is busy, so this request would start copy 2: refused.
            assert_eq!(trails(&r.host, "second.ini"), Err(crate::helper::HelperError::Disabled));
        });
        assert_eq!(r.launches.load(SeqCst), 1);
        assert_eq!(r.reaps.load(SeqCst), 1);
    }

    #[test]
    fn oneshot_works_with_a_helper_that_reads_to_eof_first() {
        // Fable #1 / D40: hello and the request arrive before EOF; both answers come after it.
        let body: Body = Arc::new(|lines, out, _| {
            let all: Vec<Request> = lines.map_while(Result::ok).map(|l| req(&l)).collect();
            assert_eq!(all.len(), 2);
            out.write_all(line(&Reply { id: 0, protocol: Some(1), ..Default::default() }).as_bytes()).unwrap();
            out.write_all(line(&echo(&all[1]).unwrap()).as_bytes()).unwrap();
        });
        let r = run(Mode::Oneshot, 1, 2000, body);
        assert_eq!(name_of(&trails(&r.host, "a.ini").unwrap()), "a.ini");
        assert_eq!(r.reaps.load(SeqCst), 1);
    }
```

  Plus one test for stderr relay: a `Launcher` whose `Proc.stderr` is the read end of a third pipe the fake
  writes `oops\n` to; after a request, `warnings` contains `sfor: ini: oops`.

- [ ] **Step 3:** RED: `cargo test helper::host` fails (the stub's `unimplemented!`).

### Task 4.2: host.rs

**Files:** `src/helper/host.rs`.

- [ ] **Step 1:** implement:

```rust
//! The sfor side of protocol v1 (spec §4.4, design note 5): starting helpers, feeding them from
//! many search threads, and what happens when they fail. Every process has a writer thread
//! (D31), so no caller ever blocks on a pipe, and one idempotent close that kills when needed,
//! always reaps, and waits for its pipe threads at most `PIPE_WAIT` (D32).
use super::protocol::{line, Reply, Request, PROTOCOL};
use super::{Control, HelperError, HostConfig, Launcher, Mode, Proc, Warn};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering::SeqCst};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

const SHUTDOWN_WAIT: Duration = Duration::from_secs(1);
const PIPE_WAIT: Duration = Duration::from_secs(1);
const MAX_FAILURES: u32 = 3;

/// Why a request got no answer.
enum Fail {
    /// The process is gone or broken: every request on it fails (counts as a failure).
    Gone(String),
    /// A JSON answer with this request's id that does not fit the protocol (D41).
    Malformed(String),
}

type Waiter = mpsc::Sender<Result<Reply, Fail>>;

/// Runs `argv` with its three pipes, in sfor's working directory and process group (D32).
pub struct CommandLauncher {
    pub argv: Vec<String>,
}

struct Child(std::process::Child);

impl Control for Child {
    fn kill(&mut self) {
        let _ = self.0.kill();
    }
    fn wait(&mut self, within: Duration) -> bool {
        let t = Instant::now();
        loop {
            match self.0.try_wait() {
                Ok(Some(_)) | Err(_) => return true,
                Ok(None) if t.elapsed() >= within => return false,
                Ok(None) => std::thread::sleep(Duration::from_millis(5)),
            }
        }
    }
    fn reap(&mut self) {
        let _ = self.0.wait();
    }
}

impl Launcher for CommandLauncher {
    fn launch(&self) -> std::io::Result<Proc> {
        use std::process::{Command, Stdio};
        let (prog, args) = self.argv.split_first().ok_or_else(|| std::io::Error::other("empty command"))?;
        let mut c = Command::new(prog).args(args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
        Ok(Proc {
            stdin: Box::new(c.stdin.take().expect("piped")),
            stdout: Box::new(c.stdout.take().expect("piped")),
            stderr: Some(Box::new(c.stderr.take().expect("piped"))),
            control: Box::new(Child(c)),
        })
    }
}

fn fail_all(pending: &Mutex<HashMap<u64, Waiter>>, alive: &AtomicBool, why: &str) {
    // `alive` first: a request registered after this sees it and fails at once.
    alive.store(false, SeqCst);
    for (_, w) in pending.lock().unwrap().drain() {
        let _ = w.send(Err(Fail::Gone(why.to_string())));
    }
}

/// One launched process.
struct Conn {
    /// To the writer thread, which owns stdin; None once stdin is closed.
    queue: Mutex<Option<mpsc::Sender<Vec<u8>>>>,
    pending: Arc<Mutex<HashMap<u64, Waiter>>>,
    in_flight: AtomicUsize,
    alive: Arc<AtomicBool>,
    /// Its failure was counted (several requests in flight see the same crash).
    counted: AtomicBool,
    closed: AtomicBool,
    /// Reaped already: `abort` and `close` may both run (Ctrl-C during a failure).
    reaped: AtomicBool,
    control: Mutex<Box<dyn Control>>,
    /// One per pipe thread: a message when it ends.
    ends: Mutex<Vec<mpsc::Receiver<()>>>,
}

impl Conn {
    fn spawn(p: Proc, format: &str, warn: &Warn) -> Arc<Conn> {
        let pending: Arc<Mutex<HashMap<u64, Waiter>>> = Default::default();
        let alive = Arc::new(AtomicBool::new(true));
        let mut ends = Vec::new();
        let mut thread = |f: Box<dyn FnOnce() + Send>| {
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                f();
                let _ = tx.send(());
            });
            ends.push(rx);
        };
        // Writer: owns stdin. A kill makes a blocked write fail; dropping the queue closes stdin.
        let (qtx, qrx) = mpsc::channel::<Vec<u8>>();
        let (pend, live, mut stdin) = (pending.clone(), alive.clone(), p.stdin);
        thread(Box::new(move || {
            for buf in qrx {
                if stdin.write_all(&buf).and_then(|_| stdin.flush()).is_err() {
                    fail_all(&pend, &live, "closed its input");
                    return;
                }
            }
        }));
        // Reader (D41).
        let (pend, live, stdout) = (pending.clone(), alive.clone(), p.stdout);
        thread(Box::new(move || {
            let mut why = "exited".to_string();
            for l in BufReader::new(stdout).lines() {
                let Ok(l) = l else {
                    why = "closed its output".into();
                    break;
                };
                if l.trim().is_empty() {
                    continue;
                }
                let v: serde_json::Value = match serde_json::from_str(&l) {
                    Ok(v) => v,
                    Err(e) => {
                        why = format!("wrote a line that is not JSON ({e})");
                        break;
                    }
                };
                let Some(id) = v.get("id").and_then(serde_json::Value::as_u64) else {
                    why = "wrote a line without an id".into();
                    break;
                };
                // An id nobody waits for (an answer after its timeout) is dropped.
                let Some(w) = pend.lock().unwrap().remove(&id) else { continue };
                let r = serde_json::from_value::<Reply>(v)
                    .map_err(|e| Fail::Malformed(format!("answered with fields that do not fit protocol v1 ({e})")));
                let _ = w.send(r);
            }
            fail_all(&pend, &live, &why);
        }));
        if let Some(err) = p.stderr {
            let (warn, format) = (warn.clone(), format.to_string());
            thread(Box::new(move || {
                for l in BufReader::new(err).lines().map_while(Result::ok) {
                    warn(format!("sfor: {format}: {l}"));
                }
            }));
        }
        Arc::new(Conn {
            queue: Mutex::new(Some(qtx)),
            pending,
            in_flight: AtomicUsize::new(0),
            alive,
            counted: AtomicBool::new(false),
            closed: AtomicBool::new(false),
            reaped: AtomicBool::new(false),
            control: Mutex::new(p.control),
            ends: Mutex::new(ends),
        })
    }

    fn alive(&self) -> bool {
        self.alive.load(SeqCst)
    }

    /// Registers `req` and queues it for the writer. Never blocks (D31). None: the process was
    /// already gone, so nothing was sent (Fable r2 #7: not a failure of this request).
    fn send(&self, req: &Request) -> Option<mpsc::Receiver<Result<Reply, Fail>>> {
        let (tx, rx) = mpsc::channel();
        let id = req.id();
        self.pending.lock().unwrap().insert(id, tx);
        let queued = self.alive() && self.queue.lock().unwrap().as_ref().is_some_and(|q| q.send(line(req).into_bytes()).is_ok());
        if !queued {
            self.pending.lock().unwrap().remove(&id);
            return None;
        }
        Some(rx)
    }

    /// D42: kill and reap now, without the grace or the pipe wait of `close` (Ctrl-C).
    fn abort(&self) {
        self.alive.store(false, SeqCst);
        let mut c = self.control.lock().unwrap();
        c.kill();
        if !self.reaped.swap(true, SeqCst) {
            c.reap();
        }
    }

    /// Closes stdin once the queued lines are written (oneshot; shutdown).
    fn close_input(&self) {
        self.queue.lock().unwrap().take();
    }

    /// The one way a process ends (D32): stdin closed, a second of grace unless `kill`, kill,
    /// reap, then at most PIPE_WAIT for the pipe threads; a descendant that inherited a pipe
    /// can keep one alive, and that thread is left behind. Idempotent.
    fn close(&self, kill: bool) {
        if self.closed.swap(true, SeqCst) {
            return;
        }
        self.alive.store(false, SeqCst);
        self.close_input();
        {
            let mut c = self.control.lock().unwrap();
            if !self.reaped.load(SeqCst) && (kill || !c.wait(SHUTDOWN_WAIT)) {
                c.kill();
            }
            if !self.reaped.swap(true, SeqCst) {
                c.reap();
            }
        }
        let deadline = Instant::now() + PIPE_WAIT;
        for end in self.ends.lock().unwrap().drain(..) {
            let _ = end.recv_timeout(deadline.saturating_duration_since(Instant::now()));
        }
    }
}

enum CallError {
    Timeout,
    Gone(String),
    Malformed(String),
}

/// A receiver that already holds «exited»: for `hello`, where a process gone before it
/// answered is a handshake failure.
fn gone() -> mpsc::Receiver<Result<Reply, Fail>> {
    let (tx, rx) = mpsc::channel();
    let _ = tx.send(Err(Fail::Gone("exited".into())));
    rx
}

fn wait(rx: mpsc::Receiver<Result<Reply, Fail>>, deadline: Instant) -> Result<Reply, CallError> {
    match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
        Ok(Ok(r)) => Ok(r),
        Ok(Err(Fail::Gone(w))) => Err(CallError::Gone(w)),
        Ok(Err(Fail::Malformed(m))) => Err(CallError::Malformed(m)),
        Err(mpsc::RecvTimeoutError::Timeout) => Err(CallError::Timeout),
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(CallError::Gone("exited".into())),
    }
}

#[derive(Default)]
struct State {
    conns: Vec<Arc<Conn>>,
    /// Processes being started (their `hello` is in flight).
    starting: usize,
    failures: u32,
    disabled: bool,
}

/// The helper processes of one Definition for one run.
pub struct Host {
    cfg: HostConfig,
    launcher: Box<dyn Launcher>,
    warn: Warn,
    st: Mutex<State>,
    freed: Condvar,
    next_id: AtomicU64,
    /// The lifecycle every launch goes through (D42): the process-wide one, or a test's own.
    sup: &'static cancel::Supervisor,
}

impl Host {
    pub fn new(cfg: HostConfig, launcher: Box<dyn Launcher>, warn: Warn) -> Self {
        Self::with_supervisor(cfg, launcher, warn, &cancel::GLOBAL)
    }

    /// Tests give each Host a Supervisor of its own (`Box::leak`), so cancelling one never
    /// touches another test's helpers.
    pub(crate) fn with_supervisor(cfg: HostConfig, launcher: Box<dyn Launcher>, warn: Warn, sup: &'static cancel::Supervisor) -> Self {
        Self { cfg, launcher, warn, st: Mutex::default(), freed: Condvar::new(), next_id: AtomicU64::new(1), sup }
    }

    pub fn format(&self) -> &str {
        &self.cfg.format
    }

    /// Sends the request `make` builds with a fresh id, and returns its answer. The deadline
    /// covers queuing, writing and answering (D31).
    pub fn request(&self, make: impl FnOnce(u64) -> Request) -> Result<Reply, HelperError> {
        let req = make(self.next_id.fetch_add(1, SeqCst));
        if self.cfg.mode == Mode::Oneshot {
            return self.oneshot(&req);
        }
        let deadline = Instant::now() + self.cfg.timeout;
        // A copy that exited with nothing in flight is replaced once, without counting it.
        for attempt in 0..2 {
            let conn = self.checkout()?;
            let sent = conn.send(&req);
            let out = sent.map(|rx| self.outcome(&conn, wait(rx, deadline)));
            self.release(&conn);
            match out {
                Some(out) => return out,
                None if attempt == 0 => continue,
                None => break,
            }
        }
        Err(HelperError::Crashed("the helper exited".into()))
    }

    fn release(&self, conn: &Conn) {
        conn.in_flight.fetch_sub(1, SeqCst);
        let _st = self.st.lock().unwrap();
        self.freed.notify_all();
    }

    fn outcome(&self, conn: &Conn, r: Result<Reply, CallError>) -> Result<Reply, HelperError> {
        match r {
            Ok(Reply { error: Some(e), .. }) => Err(HelperError::Answer(e)),
            Ok(reply) => Ok(reply),
            Err(CallError::Malformed(m)) => Err(HelperError::Protocol(m)),
            Err(CallError::Timeout) => {
                self.failed(conn);
                Err(HelperError::Timeout(self.cfg.timeout.as_millis()))
            }
            Err(CallError::Gone(why)) => {
                self.failed(conn);
                Err(HelperError::Crashed(format!("the helper {why}")))
            }
        }
    }

    /// D40: `hello` and the request, stdin closed, then both answers, under one deadline.
    fn oneshot(&self, req: &Request) -> Result<Reply, HelperError> {
        if self.st.lock().unwrap().disabled {
            return Err(HelperError::Disabled);
        }
        let conn = self.launch()?;
        let deadline = Instant::now() + self.cfg.timeout;
        let hello = conn.send(&Request::hello(&self.cfg.format, &self.cfg.params, 1)).unwrap_or_else(gone);
        let answer = conn.send(req).unwrap_or_else(gone);
        conn.close_input();
        let out = match wait(hello, deadline) {
            Ok(r) if r.protocol == Some(PROTOCOL) && r.error.is_none() => self.outcome(&conn, wait(answer, deadline)),
            Ok(r) => {
                self.disable(refusal(&r));
                Err(HelperError::Disabled)
            }
            // Codex r2 #5: no answer to hello is a handshake failure, as in `start`.
            Err(CallError::Timeout) => {
                conn.close(true);
                self.disable(format!("did not answer hello within {} ms", self.cfg.timeout.as_millis()));
                Err(HelperError::Disabled)
            }
            // A script that dies before writing anything is a failure of this file.
            Err(e) => self.outcome(&conn, Err(e)),
        };
        conn.close(false);
        out
    }

    /// Every process starts here, inside the Supervisor's lifecycle (D42): refused once the run
    /// is cancelled, and registered before a cancellation can miss it.
    fn launch(&self) -> Result<Arc<Conn>, HelperError> {
        let started = self.sup.launch(|| self.launcher.launch().map(|p| Conn::spawn(p, &self.cfg.format, &self.warn)));
        match started {
            Err(cancel::Cancelled) => Err(HelperError::Disabled),
            Ok(Ok(conn)) => Ok(conn),
            Ok(Err(e)) => {
                self.disable(format!("cannot start `{}`: {e}", self.cfg.command.join(" ")));
                Err(HelperError::Disabled)
            }
        }
    }

    /// A connection with room for one more request, started if the mode allows it.
    fn checkout(&self) -> Result<Arc<Conn>, HelperError> {
        let limit = match self.cfg.mode {
            Mode::Pool => self.cfg.threads,
            Mode::Concurrent | Mode::Oneshot => 1,
        };
        let mut st = self.st.lock().unwrap();
        loop {
            if st.disabled {
                return Err(HelperError::Disabled);
            }
            // Dead copies leave the pool through `close`, which reaps them (D32).
            let mut dead = Vec::new();
            st.conns.retain(|c| c.alive() || {
                dead.push(c.clone());
                false
            });
            if !dead.is_empty() {
                drop(st);
                for c in dead {
                    c.close(true);
                }
                st = self.st.lock().unwrap();
                continue;
            }
            let free = match self.cfg.mode {
                // Codex #14: never more than `threads` requests in flight to the process.
                Mode::Concurrent => st.conns.first().filter(|c| c.in_flight.load(SeqCst) < self.cfg.threads).cloned(),
                _ => st.conns.iter().find(|c| c.in_flight.load(SeqCst) == 0).cloned(),
            };
            if let Some(c) = free {
                c.in_flight.fetch_add(1, SeqCst);
                return Ok(c);
            }
            if st.conns.len() + st.starting < limit {
                st.starting += 1;
                drop(st);
                let started = self.start();
                st = self.st.lock().unwrap();
                st.starting -= 1;
                self.freed.notify_all();
                let c = started?;
                c.in_flight.fetch_add(1, SeqCst);
                st.conns.push(c.clone());
                return Ok(c);
            }
            st = self.freed.wait(st).unwrap();
        }
    }

    /// Launches a process and exchanges `hello` (spec §4.4, 1).
    fn start(&self) -> Result<Arc<Conn>, HelperError> {
        let conn = self.launch()?;
        let threads = if self.cfg.mode == Mode::Concurrent { self.cfg.threads } else { 1 };
        let hello = Request::hello(&self.cfg.format, &self.cfg.params, threads);
        let why = match wait(conn.send(&hello).unwrap_or_else(gone), Instant::now() + self.cfg.timeout) {
            Ok(r) if r.protocol == Some(PROTOCOL) && r.error.is_none() => return Ok(conn),
            Ok(r) => refusal(&r),
            Err(CallError::Timeout) => format!("did not answer hello within {} ms", self.cfg.timeout.as_millis()),
            Err(CallError::Gone(w)) | Err(CallError::Malformed(w)) => format!("{w} before answering hello"),
        };
        conn.close(true);
        self.disable(why);
        Err(HelperError::Disabled)
    }

    /// A crash, a timeout or an unattributable line: close (kill, reap) the copy and count it
    /// once (spec §4.4).
    fn failed(&self, c: &Conn) {
        if c.counted.swap(true, SeqCst) {
            return;
        }
        c.close(true);
        let disable = {
            let mut st = self.st.lock().unwrap();
            st.failures += 1;
            st.failures >= MAX_FAILURES && !st.disabled
        };
        if disable {
            self.disable(format!("the helper failed {MAX_FAILURES} times"));
        }
    }

    fn disable(&self, why: String) {
        let first = {
            let mut st = self.st.lock().unwrap();
            !std::mem::replace(&mut st.disabled, true)
        };
        if first {
            (self.warn)(format!("sfor: {}: {why}; its files print without Trails for the rest of this run", self.cfg.format));
        }
        self.freed.notify_all();
    }

    /// End of the run: no more requests; every process gets EOF, a second, then a kill, and is
    /// reaped (spec §4.4, Lifetime). Not a warning: a shutdown is not a failure.
    pub fn shutdown(&self) {
        let conns = {
            let mut st = self.st.lock().unwrap();
            st.disabled = true;
            std::mem::take(&mut st.conns)
        };
        self.freed.notify_all();
        for c in conns {
            c.close(false);
        }
    }
}

/// D42: one lifecycle for every helper process of this sfor process. Every launch runs under
/// the Supervisor's lock: it is refused once the run is cancelled, and the new process is
/// registered before the lock is released, so a cancellation either comes first (nothing
/// starts) or sees the process (it is killed and reaped). The process-wide Supervisor installs
/// its SIGINT/SIGTERM watcher before the first spawn; on a signal it cancels, kills and reaps
/// every helper — one that ignores SIGINT, or a signal sent to sfor's pid alone, included — and
/// then sfor dies by that signal with its default disposition, as ripgrep would. A run that
/// launches no helper installs nothing.
pub(crate) mod cancel {
    use super::Conn;
    use std::sync::{Arc, Mutex, OnceLock, Weak};

    pub(crate) struct Supervisor {
        state: Mutex<Lifecycle>,
        /// Only the process-wide Supervisor watches signals.
        signals: bool,
    }

    struct Lifecycle {
        cancelled: bool,
        live: Vec<Weak<Conn>>,
    }

    #[derive(Debug, PartialEq, Eq)]
    pub(crate) struct Cancelled;

    pub(crate) static GLOBAL: Supervisor = Supervisor::new(true);

    impl Supervisor {
        pub(crate) const fn new(signals: bool) -> Self {
            Self { state: Mutex::new(Lifecycle { cancelled: false, live: Vec::new() }), signals }
        }

        /// Runs `start` (the spawn and `Conn::spawn`) under the lifecycle lock. A signal that
        /// arrives meanwhile waits for the lock, so it finds the new process registered.
        pub(super) fn launch<E>(&self, start: impl FnOnce() -> Result<Arc<Conn>, E>) -> Result<Result<Arc<Conn>, E>, Cancelled> {
            if self.signals {
                watch(); // before the first spawn
            }
            let mut st = self.state.lock().unwrap();
            if st.cancelled {
                return Err(Cancelled);
            }
            let started = start();
            if let Ok(c) = &started {
                st.live.retain(|w| w.strong_count() > 0);
                st.live.push(Arc::downgrade(c));
            }
            Ok(started)
        }

        /// Marks the lifecycle cancelled — every later `launch` is refused — and kills and reaps
        /// every registered process, in parallel. Processes being started hold the lock, so
        /// they finish registering first and are in the list.
        pub(crate) fn cancel(&self) {
            let live: Vec<Arc<Conn>> = {
                let mut st = self.state.lock().unwrap();
                st.cancelled = true;
                st.live.iter().filter_map(Weak::upgrade).collect()
            };
            std::thread::scope(|s| {
                for c in &live {
                    s.spawn(move || c.abort());
                }
            });
        }
    }

    #[cfg(unix)]
    fn watch() {
        static WATCHER: OnceLock<()> = OnceLock::new();
        WATCHER.get_or_init(|| {
            use signal_hook::consts::{SIGINT, SIGTERM};
            // If the handler cannot be installed, Ctrl-C keeps M1's behaviour.
            let Ok(mut signals) = signal_hook::iterator::Signals::new([SIGINT, SIGTERM]) else { return };
            std::thread::spawn(move || {
                if let Some(sig) = signals.forever().next() {
                    GLOBAL.cancel();
                    let _ = signal_hook::low_level::emulate_default_handler(sig);
                    std::process::exit(128 + sig); // not reached: the default action ends sfor
                }
            });
        });
    }

    /// Windows' console Ctrl-C is M5's.
    #[cfg(not(unix))]
    fn watch() {}
}

fn refusal(r: &Reply) -> String {
    match &r.error {
        Some(e) => format!("refused hello: {e}"),
        None => format!("speaks protocol {:?}; sfor speaks {PROTOCOL}", r.protocol),
    }
}
```

  `Conn::spawn`'s `thread` closure collects the `ends` receivers; if the borrow checker
  objects to the closure borrowing `ends` mutably, push the receivers after each spawn instead.
  Helpers stay in sfor's process group, so a terminal's Ctrl-C reaches them too (D32); the
  `cancel` Supervisor (D42) makes sure the ones that ignore it, or a SIGINT sent to sfor's pid
  alone, still end and are reaped before sfor dies by the signal, including a process being
  spawned at that moment, and that no new one starts once cancelled. A sfor killed by SIGKILL
  leaves them an EOF on stdin. `signal-hook` 0.3.18 (Unix): `iterator::Signals` and
  `low_level::emulate_default_handler` were run in the `sig` probe.

- [ ] **Step 2:** GREEN: `cargo test helper::host`, then the same 20 times in a row
  (`for i in $(seq 20); do cargo test -q helper::host || break; done`) to catch races. Commit
  «Add the Helper Processor host: pool, concurrent and oneshot modes».

---

## Chunk 5: `serve` and the example helper

### Task 5.1: serve.rs

**Files:** `src/helper/serve.rs`.

- [ ] **Step 1: failing tests** (in-process, over `std::io::pipe` or `&[u8]` input and a
  `Vec<u8>` output):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::helper::protocol::{line, MatchPos, Reply, Request};
    use std::time::Duration;

    struct Slow;

    impl Handler for Slow {
        /// Sleeps 200 ms for "slow" paths, so a concurrent run answers the fast one first.
        fn trails(&self, path: &Path, m: &[MatchPos]) -> anyhow::Result<Vec<crate::model::Trail>> {
            if path.to_str() == Some("slow") {
                std::thread::sleep(Duration::from_millis(200));
            }
            if path.to_str() == Some("bad") {
                anyhow::bail!("bad file");
            }
            Ok(vec![Vec::new(); m.len()])
        }
        fn names(&self, _: &Path) -> anyhow::Result<Vec<WireName>> {
            Ok(Vec::new())
        }
    }

    fn input(threads: usize, paths: &[&str]) -> Vec<u8> {
        let mut s = line(&Request::hello("x", &toml::Table::new(), threads));
        for (i, p) in paths.iter().enumerate() {
            s.push_str(&line(&Request::Trails { id: i as u64 + 1, path: (*p).into(), matches: vec![MatchPos { line: 1, byte: 0 }] }));
        }
        s.into_bytes()
    }

    fn replies(out: &[u8]) -> Vec<Reply> {
        std::str::from_utf8(out).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect()
    }

    const INFO: Info = Info { name: "test", version: "0" };

    #[test]
    fn answers_hello_then_each_request_in_order() {
        let mut out = Vec::new();
        serve(&input(1, &["slow", "fast"])[..], &mut out, INFO, |_| Ok(Slow)).unwrap();
        let r = replies(&out);
        assert_eq!(r.iter().map(|r| r.id).collect::<Vec<_>>(), [0, 1, 2]);
        assert_eq!(r[0].protocol, Some(1));
    }

    #[test]
    fn concurrent_answers_as_they_finish() {
        let mut out = Vec::new();
        serve(&input(2, &["slow", "fast"])[..], &mut out, INFO, |_| Ok(Slow)).unwrap();
        assert_eq!(replies(&out).iter().map(|r| r.id).collect::<Vec<_>>(), [0, 2, 1]);
    }

    #[test]
    fn errors_are_answers_and_the_loop_goes_on() {
        let mut out = Vec::new();
        let mut i = input(1, &["bad", "fine"]);
        i.extend_from_slice(b"not json\n");
        serve(&i[..], &mut out, INFO, |_| Ok(Slow)).unwrap();
        let r = replies(&out);
        assert_eq!(r[1].error.as_deref(), Some("bad file"));
        assert!(r[2].trails.is_some());
        assert!(r[3].error.as_deref().unwrap().starts_with("not a request"));
    }

    #[test]
    fn a_wrong_first_line_is_refused() {
        let mut out = Vec::new();
        assert!(serve(&b"{\"op\":\"names\",\"id\":1,\"path\":\"a\"}\n"[..], &mut out, INFO, |_| Ok(Slow)).is_err());
        assert!(replies(&out)[0].error.is_some());
    }

    #[test]
    fn hello_params_arrive_as_toml() {
        let mut params = toml::Table::new();
        params.insert("query".into(), toml::Value::String("(x) @segment".into()));
        let i = line(&Request::hello("lua", &params, 1));
        let mut out = Vec::new();
        serve(i.as_bytes(), &mut out, INFO, |h| {
            assert_eq!(h.params["query"].as_str(), Some("(x) @segment"));
            assert_eq!(h.format, "lua");
            Ok(Slow)
        })
        .unwrap();
    }
}
```

- [ ] **Step 2:** RED.

- [ ] **Step 3:** implement:

```rust
//! The helper side of protocol v1 (spec §4.4): the loop a Rust helper runs. The example helper
//! (C5) and the Grammar Helper template (C8) use it. Each request is handled from scratch: a
//! handler keeps nothing between requests except what `make` built from `hello` (8458).
use super::protocol::{line, MatchPos, Reply, Request, WireName, PROTOCOL};
use crate::model::Trail;
use std::io::{BufRead, Write};
use std::path::Path;
use std::sync::Mutex;

/// What `hello` brought.
pub struct Hello {
    pub format: String,
    pub params: toml::Table,
    pub threads: usize,
}

pub trait Handler: Send + Sync {
    fn trails(&self, path: &Path, matches: &[MatchPos]) -> anyhow::Result<Vec<Trail>>;
    fn names(&self, path: &Path) -> anyhow::Result<Vec<WireName>>;
}

/// The helper's name and version, sent in the answer to `hello`.
#[derive(Clone, Copy)]
pub struct Info {
    pub name: &'static str,
    pub version: &'static str,
}

fn err(id: u64, e: impl std::fmt::Display) -> Reply {
    Reply { id, error: Some(e.to_string()), ..Default::default() }
}

/// Reads `hello`, builds the handler with `make`, answers it, then answers every request until
/// EOF. With `threads > 1` (concurrent mode) requests run on that many threads and each answer
/// is written when it is ready, in any order.
pub fn serve<H: Handler>(
    input: impl BufRead,
    output: impl Write + Send,
    info: Info,
    make: impl FnOnce(&Hello) -> anyhow::Result<H>,
) -> anyhow::Result<()> {
    let out = Mutex::new(output);
    let write = |r: &Reply| -> std::io::Result<()> {
        let mut o = out.lock().unwrap();
        o.write_all(line(r).as_bytes())?;
        o.flush()
    };
    let mut lines = input.lines();
    let first = lines.next().transpose()?.unwrap_or_default();
    let hello = match serde_json::from_str::<Request>(&first) {
        Ok(Request::Hello { protocol: PROTOCOL, format, params, threads, .. }) => {
            let params = match toml::Value::try_from(params)? {
                toml::Value::Table(t) => t,
                _ => toml::Table::new(),
            };
            Hello { format, params, threads: threads.max(1) }
        }
        Ok(Request::Hello { protocol, .. }) => {
            write(&err(0, format!("this helper speaks protocol {PROTOCOL}, not {protocol}")))?;
            anyhow::bail!("protocol {protocol}");
        }
        _ => {
            write(&err(0, "the first line must be hello"))?;
            anyhow::bail!("no hello");
        }
    };
    let handler = match make(&hello) {
        Ok(h) => h,
        Err(e) => {
            write(&err(0, format!("{e:#}")))?;
            return Err(e);
        }
    };
    write(&Reply { id: 0, protocol: Some(PROTOCOL), name: Some(info.name.into()), version: Some(info.version.into()), ..Default::default() })?;
    let answer = |l: &str| -> Reply {
        match serde_json::from_str::<Request>(l) {
            Ok(Request::Trails { id, path, matches }) => match handler.trails(Path::new(&path), &matches) {
                Ok(t) => Reply { id, trails: Some(t), ..Default::default() },
                Err(e) => err(id, format!("{e:#}")),
            },
            Ok(Request::Names { id, path }) => match handler.names(Path::new(&path)) {
                Ok(n) => Reply { id, names: Some(n), ..Default::default() },
                Err(e) => err(id, format!("{e:#}")),
            },
            Ok(Request::Hello { id, .. }) => err(id, "hello was already received"),
            Err(e) => {
                let id = serde_json::from_str::<serde_json::Value>(l).ok().and_then(|v| v["id"].as_u64()).unwrap_or(0);
                err(id, format!("not a request: {e}"))
            }
        }
    };
    if hello.threads == 1 {
        for l in lines {
            let l = l?;
            if !l.trim().is_empty() {
                write(&answer(&l))?;
            }
        }
        return Ok(());
    }
    let (tx, rx) = std::sync::mpsc::sync_channel::<String>(hello.threads);
    let rx = Mutex::new(rx);
    std::thread::scope(|s| -> anyhow::Result<()> {
        for _ in 0..hello.threads {
            s.spawn(|| loop {
                let next = rx.lock().unwrap().recv();
                let Ok(l) = next else { break };
                let _ = write(&answer(&l));
            });
        }
        for l in lines {
            let l = l?;
            if !l.trim().is_empty() && tx.send(l).is_err() {
                break;
            }
        }
        drop(tx);
        Ok(())
    })
}
```

  `toml::Value::try_from` serializes any `Serialize` value into a TOML value (verified with
  toml 1.1 in the probe: a `serde_json::Map` becomes a `Table`).

- [ ] **Step 4:** GREEN. Commit «Add serve: the helper side of protocol v1».

### Task 5.2: the example helper (INI)

**Files:** `helpers/example/Cargo.toml` (adds `[dev-dependencies] serde_json = "1"`, so
`Cargo.lock` too),
`helpers/example/src/main.rs`, `helpers/example/tests/protocol.rs`,
`helpers/example/tests/fixtures/app.ini`.

`helpers/example/tests/fixtures/app.ini` (8 lines):

```ini
; example
[server]
host = needle.example
port = 8443

[client]
retries = 3
name = needle client
```

Behaviour: a line `[name]` opens a Segment `section` named `name`, up to the next section line
or the end of the file, labelled by its line; a line `key = value` or `key: value` inside the
file is a Segment `key` over that line, labelled by the key; `;` and `#` lines are comments.
The test knobs, read from the environment once at start, each holding a substring of the
request's path: `SFOR_EXAMPLE_SLOW` (sleep 300 ms), `SFOR_EXAMPLE_CRASH` (exit with code 3),
`SFOR_EXAMPLE_HANG` (sleep until killed), `SFOR_EXAMPLE_GARBAGE` (write `not json` instead of
an answer), `SFOR_EXAMPLE_ERROR` (answer an `error`); and `SFOR_EXAMPLE_LOG` (a file path: the
helper appends `start <pid>` to it when it starts), `SFOR_EXAMPLE_STDERR` (a line it writes to
stderr when it starts), and `SFOR_EXAMPLE_FORK` (when set, the helper starts `sleep 30` at
start with its own stdout and stderr inherited, as a careless helper's child would: C11's
`h17` checks that sfor still ends on time).

- [ ] **Step 1: failing tests** in `helpers/example/tests/protocol.rs`:

```rust
//! The example helper over its pipes (spec §4.4), without sfor.
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

struct H {
    child: Child,
    stdin: Option<ChildStdin>,
    out: BufReader<ChildStdout>,
}

fn fixture() -> String {
    format!("{}/tests/fixtures/app.ini", env!("CARGO_MANIFEST_DIR"))
}

impl H {
    fn start(env: &[(&str, &str)]) -> H {
        let mut c = Command::new(env!("CARGO_BIN_EXE_sfor-example-helper"));
        c.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
        for (k, v) in env {
            c.env(k, v);
        }
        let mut child = c.spawn().unwrap();
        let stdin = child.stdin.take();
        let out = BufReader::new(child.stdout.take().unwrap());
        H { child, stdin, out }
    }
    fn send(&mut self, v: Value) {
        let s = self.stdin.as_mut().unwrap();
        writeln!(s, "{v}").unwrap();
        s.flush().unwrap();
    }
    fn recv(&mut self) -> Value {
        let mut l = String::new();
        self.out.read_line(&mut l).unwrap();
        serde_json::from_str(&l).unwrap_or_else(|e| panic!("{e}: {l:?}"))
    }
    fn hello(&mut self, threads: usize) -> Value {
        self.send(json!({"op":"hello","id":0,"protocol":1,"format":"ini","params":{},"threads":threads}));
        self.recv()
    }
    fn trails(&mut self, id: u64, path: &str, bytes: &[u64]) {
        let m: Vec<Value> = bytes.iter().map(|b| json!({"line": 1, "byte": b})).collect();
        self.send(json!({"op":"trails","id":id,"path":path,"matches":m}));
    }
}

fn names(reply: &Value) -> Vec<Vec<String>> {
    reply["trails"].as_array().unwrap().iter()
        .map(|t| t.as_array().unwrap().iter().map(|s| s["name"].as_str().unwrap().to_string()).collect())
        .collect()
}

#[test]
fn hello_then_trails_of_sections_and_keys() {
    let mut h = H::start(&[]);
    assert_eq!(h.hello(1)["protocol"], 1);
    let src = std::fs::read_to_string(fixture()).unwrap();
    let a = src.find("needle.example").unwrap() as u64;
    let b = src.find("needle client").unwrap() as u64;
    h.trails(1, &fixture(), &[a, b]);
    let r = h.recv();
    assert_eq!(r["id"], 1);
    assert_eq!(names(&r), vec![vec!["server", "host"], vec!["client", "name"]]);
}

#[test]
fn concurrent_requests_are_answered_as_they_finish() {
    let mut h = H::start(&[("SFOR_EXAMPLE_SLOW", "slow")]);
    h.hello(4);
    let slow = std::env::temp_dir().join(format!("slow-{}.ini", std::process::id()));
    std::fs::copy(fixture(), &slow).unwrap();
    h.trails(1, slow.to_str().unwrap(), &[0]);
    h.trails(2, &fixture(), &[0]);
    assert_eq!((h.recv()["id"].as_u64(), h.recv()["id"].as_u64()), (Some(2), Some(1)));
    let _ = std::fs::remove_file(slow);
}

#[test]
fn eof_ends_the_helper() {
    let mut h = H::start(&[]);
    h.hello(1);
    h.stdin = None;
    let t = Instant::now();
    while h.child.try_wait().unwrap().is_none() {
        assert!(t.elapsed() < Duration::from_secs(2), "still running after EOF");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(h.child.wait().unwrap().success());
}

#[test]
fn errors_answer_and_the_helper_goes_on() {
    let mut h = H::start(&[("SFOR_EXAMPLE_ERROR", "app")]);
    h.hello(1);
    h.trails(1, &fixture(), &[0]);
    assert!(h.recv()["error"].is_string());
    h.trails(2, "/no/such/file.ini", &[0]);
    assert!(h.recv()["error"].as_str().unwrap().contains("file.ini"));
}

#[test]
fn names_list_sections_and_keys() { /* {"op":"names"} on the fixture: texts ["server","host","port","client","retries","name"], each with one verbatim piece. */ }

/// 8458: memory does not grow from one file to the next (D23). Linux only.
#[cfg(target_os = "linux")]
#[test]
fn memory_stays_flat_over_five_thousand_requests() {
    let big = std::env::temp_dir().join(format!("big-{}.ini", std::process::id()));
    let mut s = String::new();
    for i in 0..4000 {
        s.push_str(&format!("[section{i}]\nkey = needle {i}\n\n"));
    }
    std::fs::write(&big, &s).unwrap();
    let at = s.rfind("needle").unwrap() as u64;
    let mut h = H::start(&[]);
    h.hello(1);
    let rss = |pid: u32| -> u64 {
        let st = std::fs::read_to_string(format!("/proc/{pid}/status")).unwrap();
        let kb = st.lines().find(|l| l.starts_with("VmRSS:")).unwrap().split_whitespace().nth(1).unwrap();
        kb.parse::<u64>().unwrap() * 1024
    };
    let mut at_500 = 0;
    for id in 1..=5000u64 {
        h.trails(id, big.to_str().unwrap(), &[at]);
        assert_eq!(h.recv()["id"], id);
        if id == 500 {
            at_500 = rss(h.child.id());
        }
    }
    let at_5000 = rss(h.child.id());
    let _ = std::fs::remove_file(big);
    assert!(at_5000 <= at_500 + 2 * 1024 * 1024, "RSS grew from {at_500} to {at_5000}");
}
```

  Write the body of `names_list_sections_and_keys` from its comment.

- [ ] **Step 2:** RED: `cargo test -p sfor-example-helper` (the stub `main` answers nothing).

- [ ] **Step 3:** implement `helpers/example/src/main.rs`: the knobs as a struct read once from
  the environment; `Handler::trails` reads the file with `std::fs::read` (an error names the
  path), applies the knobs, builds the Outline described above with
  `searchfor::trail::{Outline, OutlineSegment}` and answers with `TrailCursor` (anchors in
  order) or `trail_for`; `names` answers `names_of(&outline)` through `WireName::from`;
  `main` appends to `SFOR_EXAMPLE_LOG`, writes `SFOR_EXAMPLE_STDERR`, and runs
  `serve(std::io::stdin().lock(), std::io::stdout(), Info { name: "sfor-example-helper",
  version: env!("CARGO_PKG_VERSION") }, |_| Ok(Ini { knobs }))`, exiting 1 on its error. The
  `GARBAGE` knob writes `not json\n` to `std::io::stdout()` directly and then answers nothing
  for that request.

- [ ] **Step 4:** GREEN: `cargo test -p sfor-example-helper`, and
  `cargo test -p sfor-example-helper --release -- memory` reports the two RSS values; paste them
  in the chunk report. Commit «Add the example helper and its protocol and memory tests».

---

## Chunk 6: treesitter, the compiled-in Grammars and the code Definitions

Rules: spec §5.3, design note 7, D17, D18, D21, D24.

### Task 6.1: the code Definitions and fixtures

**Files:** `formats/{php,javascript,typescript,tsx,bash,go,rust,python}.toml`,
`src/formats/builtin.rs`, `tests/fixtures/code/*`.

The queries are the research's (`reviews/m2-queries/*.scm`) with `@scope` renamed `@segment`,
Go's `@recv` renamed `@prefix` and widened to generic receivers, and Rust's `impl` named by its
type (D17). The probe ran them on these fixtures (see «Verified for this plan»).

- [ ] **Step 1:** write the eight Definitions:

```toml
# formats/php.toml — PHP: classes, functions and methods (spec §5.3).
name       = "php"
extensions = ["php", "phtml"]
shebangs   = ["php"]
processor  = "treesitter"

[params]
grammar = "php"
query = '''
(namespace_definition name: (namespace_name) @name) @segment
(class_declaration name: (name) @name) @segment
(interface_declaration name: (name) @name) @segment
(trait_declaration name: (name) @name) @segment
(enum_declaration name: (name) @name) @segment
(method_declaration name: (name) @name) @segment
(function_definition name: (name) @name) @segment
'''
kinds = { namespace_definition = "namespace", class_declaration = "class", interface_declaration = "interface", trait_declaration = "trait", enum_declaration = "enum", method_declaration = "method", function_definition = "function" }
```

```toml
# formats/javascript.toml — JavaScript (and JSX): classes, functions and methods (spec §5.3).
name       = "javascript"
extensions = ["js", "mjs", "cjs", "jsx"]
shebangs   = ["node", "deno", "bun"]
processor  = "treesitter"

[params]
grammar = "javascript"
query = '''
(class_declaration name: (identifier) @name) @segment
(class name: (identifier) @name) @segment
(method_definition name: (_) @name) @segment
(function_declaration name: (identifier) @name) @segment
(generator_function_declaration name: (identifier) @name) @segment
(variable_declarator name: (identifier) @name value: [(arrow_function) (function_expression)]) @segment
(field_definition property: (property_identifier) @name value: [(arrow_function) (function_expression)]) @segment
(pair key: (property_identifier) @name value: [(arrow_function) (function_expression)]) @segment
'''
kinds = { class_declaration = "class", class = "class", method_definition = "method", function_declaration = "function", generator_function_declaration = "function", variable_declarator = "function", field_definition = "method", pair = "method" }
```

```toml
# formats/typescript.toml — TypeScript: classes, functions and methods (spec §5.3).
name       = "typescript"
extensions = ["ts", "mts", "cts"]
shebangs   = ["ts-node"]
processor  = "treesitter"

[params]
grammar = "typescript"
query = '''
(internal_module name: (_) @name) @segment
(class_declaration name: (type_identifier) @name) @segment
(abstract_class_declaration name: (type_identifier) @name) @segment
(interface_declaration name: (type_identifier) @name) @segment
(method_definition name: (_) @name) @segment
(abstract_method_signature name: (_) @name) @segment
(method_signature name: (_) @name) @segment
(function_declaration name: (identifier) @name) @segment
(variable_declarator name: (identifier) @name value: [(arrow_function) (function_expression)]) @segment
(public_field_definition name: (_) @name value: [(arrow_function) (function_expression)]) @segment
'''
kinds = { internal_module = "namespace", class_declaration = "class", abstract_class_declaration = "class", interface_declaration = "interface", method_definition = "method", abstract_method_signature = "method", method_signature = "method", function_declaration = "function", variable_declarator = "function", public_field_definition = "method" }
```

```toml
# formats/tsx.toml — TSX: classes, functions and methods (spec §5.3).
name       = "tsx"
extensions = ["tsx"]
processor  = "treesitter"

[params]
grammar = "tsx"
query = '''
(internal_module name: (_) @name) @segment
(class_declaration name: (type_identifier) @name) @segment
(abstract_class_declaration name: (type_identifier) @name) @segment
(interface_declaration name: (type_identifier) @name) @segment
(method_definition name: (_) @name) @segment
(abstract_method_signature name: (_) @name) @segment
(method_signature name: (_) @name) @segment
(function_declaration name: (identifier) @name) @segment
(variable_declarator name: (identifier) @name value: [(arrow_function) (function_expression)]) @segment
(public_field_definition name: (_) @name value: [(arrow_function) (function_expression)]) @segment
'''
kinds = { internal_module = "namespace", class_declaration = "class", abstract_class_declaration = "class", interface_declaration = "interface", method_definition = "method", abstract_method_signature = "method", method_signature = "method", function_declaration = "function", variable_declarator = "function", public_field_definition = "method" }
```

```toml
# formats/bash.toml — Bash and POSIX shell: classes, functions and methods (spec §5.3).
name       = "bash"
extensions = ["sh", "bash", "bats"]
names      = [".bashrc", ".bash_profile", ".bash_aliases", ".profile"]
shebangs   = ["bash", "sh", "dash", "ksh"]
processor  = "treesitter"

[params]
grammar = "bash"
query = '''
(function_definition name: (word) @name) @segment
'''
kinds = { function_definition = "function" }
```

```toml
# formats/go.toml — Go: classes, functions and methods (spec §5.3).
name       = "go"
extensions = ["go"]
processor  = "treesitter"

[params]
grammar = "go"
query = '''
(type_spec name: (type_identifier) @name) @segment
(function_declaration name: (identifier) @name) @segment
(method_declaration
  receiver: (parameter_list
    (parameter_declaration
      type: [(type_identifier) @prefix
             (pointer_type (type_identifier) @prefix)
             (generic_type type: (type_identifier) @prefix)
             (pointer_type (generic_type type: (type_identifier) @prefix))]))
  name: (field_identifier) @name) @segment
'''
kinds = { type_spec = "type", function_declaration = "function", method_declaration = "method" }
```

```toml
# formats/rust.toml — Rust: classes, functions and methods (spec §5.3).
name       = "rust"
extensions = ["rs"]
processor  = "treesitter"

[params]
grammar = "rust"
query = '''
(mod_item name: (identifier) @name) @segment
(struct_item name: (type_identifier) @name) @segment
(enum_item name: (type_identifier) @name) @segment
(trait_item name: (type_identifier) @name) @segment
(impl_item type: (_) @name) @segment
(function_item name: (identifier) @name) @segment
'''
kinds = { mod_item = "module", struct_item = "struct", enum_item = "enum", trait_item = "trait", impl_item = "impl", function_item = "function" }
```

```toml
# formats/python.toml — Python: classes, functions and methods (spec §5.3).
name       = "python"
extensions = ["py", "pyi", "pyw"]
shebangs   = ["python", "python3", "python2"]
processor  = "treesitter"

[params]
grammar = "python"
query = '''
(class_definition name: (identifier) @name) @segment
(function_definition name: (identifier) @name) @segment
'''
kinds = { class_definition = "class", function_definition = "function" }
```

  and eight lines in `BUILTIN`, after `markdown` (which stays first), in the order above.

- [ ] **Step 2:** the fixtures, one per Format, in `tests/fixtures/code/`:

`tests/fixtures/code/cart.php`:

```php
<?php
namespace App\Shop;

class Cart
{
    private array $items = [];

    public function addItem(Item $item, int $qty = 1): void
    {
        // needle: merge equal items
        $this->items[] = [$item, $qty];
    }
}

function needle_helper(): string
{
    return "needle";
}
```

`tests/fixtures/code/widget.js`:

```js
export class Widget {
  constructor(el) {
    this.el = el;
  }

  render() {
    return `<p>needle</p>`;
  }

  handler = () => {
    console.log("needle clicked");
  };
}

const helpers = {
  format(x) {
    return x + " needle";
  },
};
```

`tests/fixtures/code/shape.ts`:

```ts
namespace Geometry {
  export abstract class Base {
    abstract area(): number;

    describe(): string {
      return `needle ${this.area()}`;
    }
  }
}

interface Shape {
  area(): number;
}

export const make = (n: number): string => {
  return "needle".repeat(n);
};
```

`tests/fixtures/code/view.tsx`:

```tsx
export function View(props: { title: string }) {
  const onClick = () => {
    console.log("needle");
  };
  return <button onClick={onClick}>{props.title} needle</button>;
}
```

`tests/fixtures/code/deploy.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

greet() {
  echo "hello needle"
}

function deploy {
  rsync -a build/ host:/srv/needle/
}
```

`tests/fixtures/code/server.go`:

```go
package server

type Server struct {
	Addr string // needle address
}

func (s *Server) Start() error {
	return listen(s.Addr, "needle")
}

func helper() string {
	return "needle"
}
```

`tests/fixtures/code/thing.rs`:

```rust
mod outer {
    pub struct Thing {
        needle: u32,
    }

    impl Thing {
        pub fn helper(&self) -> u32 {
            self.needle + 1
        }
    }

    pub trait Greeter {
        fn greet(&self) -> String {
            String::from("needle")
        }
    }
}
```

`tests/fixtures/code/outer.py`:

```python
class Outer:
    def method(self):
        def inner():
            return "needle"
        return inner()

    async def fetch(self):
        return "needle"


def top():
    return "needle"
```

- [ ] **Step 3:** `cargo test` still passes (the treesitter stub is reached by no test yet, and
  `Processors::known` accepts `treesitter`). Commit «Add the code Format Definitions and their
  fixtures».

### Task 6.2: failing tests

**Files:** `src/processors/treesitter/mod.rs` (tests).

The expected Trails are the probe's output on the fixtures above (`fixture-trails.txt`): every
`needle`, its Trail names, and the signatures of its Segments.

- [ ] **Step 1:**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Input;
    use std::path::Path;

    /// The `[params]` of a built-in Definition.
    fn params(format: &str) -> toml::Table {
        let src = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("formats").join(format!("{format}.toml"))).unwrap();
        let t: toml::Table = toml::from_str(&src).unwrap();
        t["params"].as_table().unwrap().clone()
    }

    fn fixture(file: &str) -> Vec<u8> {
        std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/code").join(file)).unwrap()
    }

    /// (line, Trail names, signatures) of every `needle`.
    fn needles(format: &str, file: &str) -> Vec<(u64, String, String)> {
        let src = fixture(file);
        let anchors: Vec<usize> = memchr::memmem::find_iter(&src, b"needle").collect();
        let lines = crate::trail::LineIndex::new(&src);
        let input = Input { bytes: &src, path: Path::new(file), format, disk: None };
        TreeSitter::default()
            .trails(&input, &params(format), &anchors)
            .unwrap()
            .iter()
            .zip(&anchors)
            .map(|(t, &a)| {
                let names: Vec<&str> = t.iter().map(|s| s.name.as_str()).collect();
                let sigs: Vec<&str> = t.iter().map(|s| s.signature.as_deref().unwrap_or("")).collect();
                (lines.line_of(a), names.join(" › "), sigs.join(" ‖ "))
            })
            .collect()
    }

    fn t(line: u64, names: &str, sigs: &str) -> (u64, String, String) {
        (line, names.into(), sigs.into())
    }

    #[test]
    fn php() {
        assert_eq!(needles("php", "cart.php"), [
            t(10, "Cart › addItem", "class Cart ‖ public function addItem(Item $item, int $qty = 1): void"),
            // In the function's own name: not inside it (D18).
            t(15, "", ""),
            t(17, "needle_helper", "function needle_helper(): string"),
        ]);
    }

    #[test]
    fn javascript() {
        assert_eq!(needles("javascript", "widget.js"), [
            t(7, "Widget › render", "class Widget ‖ render()"),
            t(11, "Widget › handler", "class Widget ‖ handler = () =>"),
            t(17, "format", "format(x)"),
        ]);
    }

    #[test]
    fn typescript_and_tsx() {
        assert_eq!(needles("typescript", "shape.ts"), [
            t(6, "Geometry › Base › describe", "namespace Geometry ‖ abstract class Base ‖ describe(): string"),
            t(16, "make", "make = (n: number): string =>"),
        ]);
        assert_eq!(needles("tsx", "view.tsx"), [
            t(3, "View › onClick", "function View(props: { title: string }) ‖ onClick = () =>"),
            t(5, "View", "function View(props: { title: string })"),
        ]);
    }

    #[test]
    fn bash() {
        assert_eq!(needles("bash", "deploy.sh"), [t(5, "greet", "greet()"), t(9, "deploy", "function deploy")]);
    }

    #[test]
    fn go_methods_carry_their_receiver() {
        assert_eq!(needles("go", "server.go"), [
            t(4, "Server", "Server struct"),
            t(8, "Server.Start", "func (s *Server) Start() error"),
            t(12, "helper", "func helper() string"),
        ]);
    }

    #[test]
    fn rust() {
        assert_eq!(needles("rust", "thing.rs"), [
            t(3, "outer › Thing", "mod outer ‖ pub struct Thing"),
            t(8, "outer › Thing › helper", "mod outer ‖ impl Thing ‖ pub fn helper(&self) -> u32"),
            t(14, "outer › Greeter › greet", "mod outer ‖ pub trait Greeter ‖ fn greet(&self) -> String"),
        ]);
    }

    #[test]
    fn python() {
        assert_eq!(needles("python", "outer.py"), [
            t(4, "Outer › method › inner", "class Outer: ‖ def method(self): ‖ def inner():"),
            t(8, "Outer › fetch", "class Outer: ‖ async def fetch(self):"),
            t(12, "top", "def top():"),
        ]);
    }

    #[test]
    fn kinds_come_from_the_definition() {
        let src = fixture("cart.php");
        let at = memchr::memmem::find(&src, b"needle: merge").unwrap();
        let input = Input { bytes: &src, path: Path::new("cart.php"), format: "php", disk: None };
        let tr = &TreeSitter::default().trails(&input, &params("php"), &[at]).unwrap()[0];
        let kinds: Vec<&str> = tr.iter().map(|s| s.kind.as_str()).collect();
        assert_eq!(kinds, ["class", "method"]);
        assert_eq!((tr[1].line, tr[1].end_line), (8, 12));
    }

    #[test]
    fn the_byte_range_changes_nothing() {
        // D24: the query runs only between the first and the last anchor.
        for (format, file) in [("php", "cart.php"), ("rust", "thing.rs"), ("python", "outer.py")] {
            let src = fixture(file);
            let c = TreeSitter::default().compiled(format, &params(format)).unwrap().unwrap();
            let tree = parse(&c, &src).unwrap();
            let full = outline(&c, &tree, &src, None);
            for a in memchr::memmem::find_iter(&src, b"needle") {
                let part = outline(&c, &tree, &src, Some(a..a + 1));
                assert_eq!(crate::trail::trail_for(&part, a), crate::trail::trail_for(&full, a), "{file} at {a}");
            }
        }
    }

    #[test]
    fn names_for_in_headings() {
        let src = fixture("server.go");
        let input = Input { bytes: &src, path: Path::new("server.go"), format: "go", disk: None };
        let names = TreeSitter::default().names(&input, &params("go")).unwrap();
        let texts: Vec<&str> = names.iter().map(|n| n.text.as_str()).collect();
        assert_eq!(texts, ["Server", "Server.Start", "helper"]);
        // "Server.Start": the receiver and the name are two pieces; the dot is not in the file.
        assert_eq!(names[1].pieces.len(), 2);
        assert_eq!(names[1].pieces[1].0, 7..12);
    }

    #[test]
    fn a_broken_query_fails_once_then_gives_empty_trails() {
        let mut p = params("php");
        p.insert("query".into(), toml::Value::String("(no_such_node) @segment".into()));
        let src = fixture("cart.php");
        let input = Input { bytes: &src, path: Path::new("cart.php"), format: "php", disk: None };
        let ts = TreeSitter::default();
        let e = ts.trails(&input, &p, &[0]).unwrap_err();
        assert!(e.to_string().contains("php"), "{e}");
        // Afterwards the Format is unavailable: Plain, without another warning (D33).
        let again = ts.trails(&input, &p, &[0]).unwrap_err();
        assert!(again.downcast_ref::<crate::helper::Unavailable>().is_some(), "{again}");
    }

    #[test]
    fn concurrent_first_calls_compile_and_fail_once() {
        // D39: one OnceLock per Definition.
        let mut p = params("php");
        p.insert("query".into(), toml::Value::String("(no_such_node) @segment".into()));
        let src = fixture("cart.php");
        let ts = TreeSitter::default();
        let errors = std::sync::atomic::AtomicUsize::new(0);
        std::thread::scope(|s| {
            for _ in 0..8 {
                s.spawn(|| {
                    let input = Input { bytes: &src, path: Path::new("cart.php"), format: "php", disk: None };
                    if let Err(e) = ts.trails(&input, &p, &[0]) {
                        if e.downcast_ref::<crate::helper::Unavailable>().is_none() {
                            errors.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        }
                    }
                });
            }
        });
        assert_eq!(errors.into_inner(), 1);
    }

    #[test]
    fn each_run_compiles_its_own_params() {
        // A second run (M5's MCP server runs many in one process) sees the new query.
        let src = fixture("cart.php");
        let at = memchr::memmem::find(&src, b"needle: merge").unwrap();
        let input = Input { bytes: &src, path: Path::new("cart.php"), format: "php", disk: None };
        let full = TreeSitter::default().trails(&input, &params("php"), &[at]).unwrap();
        let mut only_classes = params("php");
        only_classes.insert("query".into(), toml::Value::String("(class_declaration name: (name) @name) @segment".into()));
        let classes = TreeSitter::default().trails(&input, &only_classes, &[at]).unwrap();
        assert_eq!(full[0].len(), 2);
        assert_eq!(classes[0].len(), 1);
    }

    #[test]
    fn files_over_max_bytes_are_not_parsed() {
        let mut p = params("php");
        p.insert("max_bytes".into(), toml::Value::Integer(10));
        let src = fixture("cart.php");
        let input = Input { bytes: &src, path: Path::new("cart.php"), format: "php", disk: None };
        let ts = TreeSitter::default();
        assert!(ts.trails(&input, &p, &[0]).unwrap_err().to_string().contains("max_bytes"));
        let again = ts.trails(&input, &p, &[0]).unwrap_err();
        assert!(again.downcast_ref::<crate::helper::Unavailable>().is_some());
    }

    #[test]
    fn every_compiled_in_grammar_loads() {
        for g in grammars::COMPILED_IN {
            let l = grammars::language(g).unwrap_or_else(|| panic!("{g}"));
            // tree-sitter 0.24 loads ABI 13–14 (D1).
            assert_eq!(l.version(), 14, "{g}");
        }
        assert!(grammars::language("lua").is_none());
    }
}
```

- [ ] **Step 2:** RED: every test fails on the stub.

### Task 6.3: implementation

**Files:** `src/processors/treesitter/{mod,grammars}.rs`.

- [ ] **Step 1:** `grammars.rs`:

```rust
//! The Grammars compiled into sfor (spec §7, ADR 0005).
use tree_sitter::Language;

pub const COMPILED_IN: &[&str] = &["php", "javascript", "typescript", "tsx", "bash", "go", "rust", "python"];

pub fn language(name: &str) -> Option<Language> {
    Some(match name {
        "php" => tree_sitter_php::LANGUAGE_PHP.into(),
        "javascript" => tree_sitter_javascript::LANGUAGE.into(),
        "typescript" => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        "tsx" => tree_sitter_typescript::LANGUAGE_TSX.into(),
        "bash" => tree_sitter_bash::LANGUAGE.into(),
        "go" => tree_sitter_go::LANGUAGE.into(),
        "rust" => tree_sitter_rust::LANGUAGE.into(),
        "python" => tree_sitter_python::LANGUAGE.into(),
        _ => return None,
    })
}
```

- [ ] **Step 2:** `mod.rs`:

```rust
//! treesitter: code Trails from the query of a Format Definition (spec §5.3, design note 7).
//! `compile`, `parse` and `outline` take any Language, so the Grammar Helper template (C8)
//! reuses them for a Grammar that is not compiled in.
pub mod grammars;

use super::Processor;
use crate::model::{Input, Name, Trail};
use crate::trail::{names_of, trail_for, Outline, OutlineSegment, TrailCursor};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::sync::{Arc, Mutex, OnceLock};
use streaming_iterator::StreamingIterator;
// `trail_for` serves anchors out of order; `TrailCursor` the sorted ones.
use tree_sitter::{Language, Node, Parser, Query, QueryCursor, Tree};

pub const DEFAULT_MAX_BYTES: i64 = 16 * 1024 * 1024;

/// A Definition's query, compiled for its Language.
pub struct Compiled {
    pub grammar: String,
    pub language: Language,
    query: Query,
    segment: u32,
    name: u32,
    prefix: Option<u32>,
    signature: Option<u32>,
    kinds: HashMap<String, String>,
}

/// Compiles `params.query` for `language`; `@segment` and `@name` are required captures.
pub fn compile(grammar: &str, language: &Language, params: &toml::Table) -> anyhow::Result<Compiled> {
    let src = params.get("query").and_then(|v| v.as_str()).ok_or_else(|| anyhow::anyhow!("`query` is missing"))?;
    let query = Query::new(language, src).map_err(|e| anyhow::anyhow!("the query does not compile: {e}"))?;
    let idx = |n: &str| query.capture_index_for_name(n);
    let (Some(segment), Some(name)) = (idx("segment"), idx("name")) else {
        anyhow::bail!("the query needs the captures @segment and @name");
    };
    let kinds = params
        .get("kinds")
        .and_then(|v| v.as_table())
        .map(|t| t.iter().filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string()))).collect())
        .unwrap_or_default();
    Ok(Compiled { grammar: grammar.into(), language: language.clone(), segment, name, prefix: idx("prefix"), signature: idx("signature"), kinds, query })
}

thread_local! {
    static PARSERS: RefCell<HashMap<String, Parser>> = RefCell::new(HashMap::new());
}

/// Parses `src` with this thread's Parser for the Grammar.
pub fn parse(c: &Compiled, src: &[u8]) -> Option<Tree> {
    PARSERS.with(|ps| {
        let mut ps = ps.borrow_mut();
        let p = ps.entry(c.grammar.clone()).or_insert_with(|| {
            let mut p = Parser::new();
            p.set_language(&c.language).expect("ABI checked when the Grammar was linked");
            p
        });
        p.parse(src, None)
    })
}

/// The first line of a Segment, without a trailing `{` (spec §5.3).
fn first_line(src: &[u8], n: Node) -> String {
    let text = String::from_utf8_lossy(&src[n.byte_range()]);
    let line = text.lines().next().unwrap_or("").trim_end();
    let line = line.strip_suffix('{').unwrap_or(line).trim_end();
    line.chars().take(200).collect()
}

/// Every `@segment` of the query, or only those that touch `range` (D24).
pub fn outline(c: &Compiled, tree: &Tree, src: &[u8], range: Option<Range<usize>>) -> Outline {
    let mut cursor = QueryCursor::new();
    if let Some(r) = range {
        cursor.set_byte_range(r);
    }
    let mut segs = Vec::new();
    let mut matches = cursor.matches(&c.query, tree.root_node(), src);
    while let Some(m) = matches.next() {
        let (mut seg, mut name, mut prefix, mut sig) = (None, None, None, None);
        // tree-sitter 0.24: `captures` is a field (0.27 made it a method).
        for cap in m.captures {
            match cap.index {
                i if i == c.segment => seg = Some(cap.node),
                i if i == c.name => name = Some(cap.node),
                i if Some(i) == c.prefix => prefix = Some(cap.node),
                i if Some(i) == c.signature => sig = Some(cap.node),
                _ => {}
            }
        }
        let (Some(node), Some(name)) = (seg, name) else { continue };
        let text = |n: Node| String::from_utf8_lossy(&src[n.byte_range()]).lines().next().unwrap_or("").to_string();
        let own = text(name);
        let (label, pieces) = match prefix {
            Some(p) => {
                let pt = text(p);
                let at = pt.len() + 1;
                (format!("{pt}.{own}"), vec![(0..pt.len(), p.start_byte()..p.start_byte() + pt.len()), (at..at + own.len(), name.start_byte()..name.start_byte() + own.len())])
            }
            None => (own.clone(), vec![(0..own.len(), name.start_byte()..name.start_byte() + own.len())]),
        };
        let kind = c.kinds.get(node.kind()).cloned().unwrap_or_else(|| node.kind().to_string());
        let mut s = OutlineSegment::new(&kind, label, node.byte_range(), name.byte_range());
        s.segment.signature = Some(first_line(src, sig.unwrap_or(node)));
        s.name = Some(pieces);
        s.name_anchor = name.start_byte();
        segs.push(s);
    }
    Outline::build(segs, src)
}

/// The run's treesitter (D39): compiled queries and warnings per Format Definition, for one
/// run. `Processors` owns it.
#[derive(Default)]
pub struct TreeSitter {
    /// One slot per Definition, initialized once even under concurrent first calls: the
    /// compiled query, or None after a failure, which only the initializing call reports.
    queries: Mutex<HashMap<String, Arc<OnceLock<Option<Arc<Compiled>>>>>>,
    /// Formats already warned about a file over `max_bytes`.
    big: Mutex<HashSet<String>>,
}

impl TreeSitter {
    pub(crate) fn compiled(&self, format: &str, params: &toml::Table) -> anyhow::Result<Option<Arc<Compiled>>> {
        let slot = self.queries.lock().unwrap().entry(format.to_string()).or_default().clone();
        let mut failure = None;
        let c = slot.get_or_init(|| {
            let grammar = params.get("grammar").and_then(|v| v.as_str()).unwrap_or("");
            let built = grammars::language(grammar)
                .ok_or_else(|| anyhow::anyhow!("grammar `{grammar}` is not compiled in"))
                .and_then(|l| compile(grammar, &l, params));
            match built {
                Ok(c) => Some(Arc::new(c)),
                Err(e) => {
                    failure = Some(e);
                    None
                }
            }
        });
        if let Some(e) = failure {
            return Err(e.context(format!("Format `{format}`")));
        }
        Ok(c.clone())
    }

    /// D21: true when the file is over `max_bytes`; the first time per Format it is an error.
    fn too_big(&self, input: &Input, params: &toml::Table) -> anyhow::Result<bool> {
        let max = params.get("max_bytes").and_then(|v| v.as_integer()).unwrap_or(DEFAULT_MAX_BYTES);
        if (input.bytes.len() as i64) <= max {
            return Ok(false);
        }
        if self.big.lock().unwrap().insert(input.format.to_string()) {
            anyhow::bail!("larger than `max_bytes` ({max}); files this large are not parsed (this warning is shown once)");
        }
        Ok(true)
    }
}

impl Processor for TreeSitter {
    /// A failed query or a file over `max_bytes` is an error the first time and `Unavailable`
    /// afterwards: the file is Plain either way (D33).
    fn trails(&self, input: &Input, params: &toml::Table, anchors: &[usize]) -> anyhow::Result<Vec<Trail>> {
        let empty = || vec![Vec::new(); anchors.len()];
        let Some(c) = self.compiled(input.format, params)? else { return Err(crate::helper::Unavailable.into()) };
        if anchors.is_empty() {
            return Ok(empty());
        }
        if self.too_big(input, params)? {
            return Err(crate::helper::Unavailable.into());
        }
        let Some(tree) = parse(&c, input.bytes) else { return Ok(empty()) };
        let lo = *anchors.iter().min().unwrap();
        let hi = *anchors.iter().max().unwrap();
        let o = outline(&c, &tree, input.bytes, Some(lo..hi + 1));
        if anchors.is_sorted() {
            let mut cur = TrailCursor::new(&o);
            return Ok(anchors.iter().map(|&a| cur.trail(a)).collect());
        }
        Ok(anchors.iter().map(|&a| trail_for(&o, a)).collect())
    }

    fn names(&self, input: &Input, params: &toml::Table) -> anyhow::Result<Vec<Name>> {
        let Some(c) = self.compiled(input.format, params)? else { return Err(crate::helper::Unavailable.into()) };
        if self.too_big(input, params)? {
            return Err(crate::helper::Unavailable.into());
        }
        let Some(tree) = parse(&c, input.bytes) else { return Ok(Vec::new()) };
        Ok(names_of(&outline(&c, &tree, input.bytes, None)))
    }
}
```

  The caches live in the instance, so each test builds its own `TreeSitter::default()`; the
  test `the_byte_range_changes_nothing` calls `compiled`, `parse` and `outline` directly.

- [ ] **Step 3:** GREEN: `cargo test treesitter`. If a fixture's Trail differs from the table,
  the probe's output (`/tmp/subtask/m2-plan/outputs/fixture-trails.txt`) is the reference;
  report any difference instead of editing the expected value.

- [ ] **Step 4:** measure the share of the query, as D24 assumes: on the 404 Rust files of the
  cargo registry matching `impl Drop for` (`rg -l -t rust 'impl Drop for' ~/.cargo/registry/src/index.crates.io-*/`),
  time parse + `outline(…, None)` against parse + `outline(…, Some(first..last+1))` with the
  real anchors, in an `#[ignore]` test that prints both. Paste the numbers in the report.
  Commit «Add the treesitter Processor with eight compiled-in Grammars».

---

## Chunk 7: the Grammar directory, the index, install and list (library)

Rules: spec §7, D13, D14, D28, D30. No command-line wiring here (C10 does it); every function
takes its directory and URL as arguments, so the tests never touch the user's directories or
the network.

### Task 7.1: index.rs and mod.rs

**Files:** `src/grammars/{mod,index}.rs`.

The index, `grammars/index.json` (its schema is C8's `docs/grammar-index.schema.json`):

```json
{
  "version": 1,
  "grammars": [
    {
      "name": "lua",
      "version": "0.2.0",
      "source": "https://github.com/tree-sitter-grammars/tree-sitter-lua",
      "license": "MIT",
      "definition": "name = \"lua\"\nextensions = [\"lua\"]\nprocessor = \"treesitter\"\n…",
      "binaries": {
        "linux-x86_64": { "url": "https://…/sfor-grammar-lua-linux-x86_64", "sha256": "…" }
      }
    }
  ]
}
```

- [ ] **Step 1: failing tests** in `index.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const INDEX: &str = r#"{"version":1,"grammars":[{"name":"lua","version":"0.2.0","source":"https://example.org/lua","license":"MIT","definition":"name = 'lua'\nprocessor = 'treesitter'\n","binaries":{"linux-x86_64":{"url":"file:///tmp/x","sha256":"00"}},"future_field":1}]}"#;

    #[test]
    fn parses_and_ignores_unknown_fields() {
        let i = Index::parse(INDEX.as_bytes()).unwrap();
        assert_eq!(i.grammars[0].name, "lua");
        assert_eq!(i.grammars[0].binaries["linux-x86_64"].sha256, "00");
    }

    #[test]
    fn refuses_another_version_and_bad_names() {
        assert!(Index::parse(br#"{"version":2,"grammars":[]}"#).unwrap_err().to_string().contains("version"));
        let bad = INDEX.replace("\"name\":\"lua\"", "\"name\":\"../lua\"");
        assert!(Index::parse(bad.as_bytes()).unwrap_err().to_string().contains("../lua"));
    }

    #[test]
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    fn the_system_key_is_os_dash_arch() {
        assert_eq!(system(), "linux-x86_64");
    }

    #[test]
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    fn the_system_key_on_apple_silicon() {
        assert_eq!(system(), "macos-aarch64");
    }

    #[test]
    fn the_agent_refuses_http_even_after_a_redirect() {
        // D38: `https_only` is checked on every hop (ureq 3.4.2, run.rs:149), before any
        // connection, so this needs no network. A redirect from https:// to http:// reaches
        // the same check; a test against a live redirect is out of reach without a TLS server.
        let e = agent().get("http://127.0.0.1:9/index.json").call().unwrap_err();
        assert!(e.to_string().contains("https only"), "{e}");
    }

    #[test]
    fn fetch_reads_files_and_refuses_plain_http() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("i.json");
        std::fs::write(&p, "hello").unwrap();
        assert_eq!(fetch(&format!("file://{}", p.display())).unwrap(), b"hello");
        assert!(fetch("http://example.org/i.json").unwrap_err().to_string().contains("https"));
        assert!(fetch("ftp://example.org/i.json").is_err());
    }

    #[test]
    fn names_are_checked() {
        for ok in ["lua", "c_sharp", "x86asm"] {
            assert!(valid_name(ok), "{ok}");
        }
        for bad in ["", "Lua", "../x", "a/b", "a.b", "a-b"] {
            assert!(!valid_name(bad), "{bad}");
        }
    }
}
```

- [ ] **Step 2:** RED.

- [ ] **Step 3:** implement. `index.rs`:

```rust
//! The Grammar index (spec §7): what `--install-grammar` can install, per system.
use serde::Deserialize;
use std::collections::BTreeMap;

pub const INDEX_VERSION: u32 = 1;
/// Replaced by `SFOR_GRAMMAR_INDEX` (spec §7).
pub const DEFAULT_INDEX: &str = "https://raw.githubusercontent.com/neitanod/searchfor/main/grammars/index.json";

#[derive(Debug, Clone, Deserialize)]
pub struct Index {
    pub version: u32,
    pub grammars: Vec<Entry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Entry {
    pub name: String,
    pub version: String,
    pub source: String,
    pub license: String,
    /// The Format Definition installed with the Grammar Helper.
    pub definition: String,
    /// By `system()`.
    pub binaries: BTreeMap<String, Binary>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Binary {
    pub url: String,
    pub sha256: String,
}

/// D30: Grammar names become directory and file names.
pub fn valid_name(n: &str) -> bool {
    !n.is_empty() && n.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// D14: `linux-x86_64`, `macos-aarch64`, `windows-x86_64`, …
pub fn system() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

impl Index {
    pub fn parse(bytes: &[u8]) -> anyhow::Result<Self> {
        let i: Index = serde_json::from_slice(bytes)?;
        if i.version != INDEX_VERSION {
            anyhow::bail!("the Grammar index is version {}; this sfor reads version {INDEX_VERSION}", i.version);
        }
        if let Some(e) = i.grammars.iter().find(|e| !valid_name(&e.name)) {
            anyhow::bail!("the Grammar index has an invalid name `{}`", e.name);
        }
        Ok(i)
    }

    pub fn get(&self, name: &str) -> Option<&Entry> {
        self.grammars.iter().find(|e| e.name == name)
    }
}

/// The HTTP client of `--install-grammar` and `--list-grammars` (D38): HTTPS on every hop,
/// redirects included, a 10 s connect timeout and a 300 s ceiling for the whole download.
fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .https_only(true)
        .timeout_connect(Some(std::time::Duration::from_secs(10)))
        .timeout_global(Some(std::time::Duration::from_secs(300)))
        .build()
        .into()
}

/// The bytes at `url`: `https://` through `agent()`, `file://` from disk; anything else is
/// refused.
pub fn fetch(url: &str) -> anyhow::Result<Vec<u8>> {
    if let Some(path) = url.strip_prefix("file://") {
        return std::fs::read(path).map_err(|e| anyhow::anyhow!("{url}: {e}"));
    }
    if !url.starts_with("https://") {
        anyhow::bail!("{url}: only https:// and file:// URLs are accepted");
    }
    let mut r = agent().get(url).call().map_err(|e| anyhow::anyhow!("{url}: {e}"))?;
    Ok(r.body_mut().with_config().limit(256 * 1024 * 1024).read_to_vec()?)
}
```

  (`with_config().limit(…)` raises ureq's default body limit, since a Grammar Helper may be
  larger; that call, `https_only`, `timeout_connect` and `timeout_global` compiled against
  ureq 3.4.2 in the probe, and the agent refused `http://example.org/` with «configured for
  https only», `r1-probe.txt`.)

  `mod.rs`:

```rust
//! Grammar Helpers on disk (spec §7): where they live, the index, install and list.
pub mod index;
pub mod install;

use std::path::{Path, PathBuf};

/// `SFOR_GRAMMARS_DIR` if set (tests), else `<data dir>/searchfor/grammars`.
pub fn grammars_dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("SFOR_GRAMMARS_DIR") {
        return Some(PathBuf::from(d));
    }
    dirs::data_dir().map(|d| d.join("searchfor").join("grammars"))
}

/// `SFOR_GRAMMAR_INDEX` if set, else the index in the repository.
pub fn index_url() -> String {
    std::env::var("SFOR_GRAMMAR_INDEX").unwrap_or_else(|_| index::DEFAULT_INDEX.to_string())
}

pub fn binary_name(lang: &str) -> String {
    format!("sfor-grammar-{lang}{}", std::env::consts::EXE_SUFFIX)
}

/// An installed Grammar Helper.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    pub name: String,
    pub version: String,
    pub dir: PathBuf,
    pub binary: PathBuf,
    pub definition: PathBuf,
}

/// The installed Grammar Helpers under `dir`, sorted by name; a broken one is skipped.
pub fn installed(dir: &Path) -> Vec<Installed> {
    let mut out: Vec<Installed> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_str()?.to_string();
            if !index::valid_name(&name) {
                return None;
            }
            let d = e.path();
            let meta: serde_json::Value = serde_json::from_slice(&std::fs::read(d.join("installed.json")).ok()?).ok()?;
            let binary = d.join(binary_name(&name));
            let definition = d.join(format!("{name}.toml"));
            (binary.is_file() && definition.is_file()).then(|| Installed {
                version: meta["version"].as_str().unwrap_or("?").to_string(),
                name,
                dir: d.clone(),
                binary,
                definition,
            })
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

pub fn installed_binary(dir: &Path, lang: &str) -> Option<PathBuf> {
    installed(dir).into_iter().find(|i| i.name == lang).map(|i| i.binary)
}
```

- [ ] **Step 4:** GREEN. Commit «Add the Grammar index and the Grammar directory».

### Task 7.2: install.rs

**Files:** `src/grammars/install.rs`.

- [ ] **Step 1: failing tests**:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::grammars::{index::Index, installed};
    use sha2::Digest;

    /// An index offering `lua` for this system, whose binary is `bin` with checksum `sha`.
    fn index(dir: &std::path::Path, bin: &[u8], sha: Option<&str>) -> Index {
        let p = dir.join("payload");
        std::fs::write(&p, bin).unwrap();
        let sha = sha.map(String::from).unwrap_or_else(|| hex(&sha2::Sha256::digest(bin)));
        let json = serde_json::json!({"version":1,"grammars":[{"name":"lua","version":"0.2.0","source":"s","license":"MIT",
            "definition":"name = 'lua'\nextensions = ['lua']\nprocessor = 'treesitter'\n[params]\ngrammar = 'lua'\nquery = '(x) @segment'\n",
            "binaries":{ (crate::grammars::index::system()): {"url": format!("file://{}", p.display()), "sha256": sha} }}]});
        Index::parse(json.to_string().as_bytes()).unwrap()
    }

    #[test]
    fn installs_binary_definition_and_metadata() {
        let src = tempfile::tempdir().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let got = install(&index(src.path(), b"#!fake", None), "lua", dir.path()).unwrap();
        assert_eq!(std::fs::read(&got.binary).unwrap(), b"#!fake");
        assert!(std::fs::read_to_string(&got.definition).unwrap().contains("processor = 'treesitter'"));
        assert_eq!(installed(dir.path()), vec![got.clone()]);
        assert_eq!(got.version, "0.2.0");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&got.binary).unwrap().permissions().mode() & 0o111, 0o111);
        }
    }

    #[test]
    fn a_checksum_mismatch_writes_nothing() {
        let src = tempfile::tempdir().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let e = install(&index(src.path(), b"#!fake", Some("deadbeef")), "lua", dir.path()).unwrap_err();
        assert!(e.to_string().contains("SHA-256"), "{e}");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn unknown_grammars_and_systems_are_errors() {
        let src = tempfile::tempdir().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let i = index(src.path(), b"x", None);
        assert!(install(&i, "cobol", dir.path()).unwrap_err().to_string().contains("cobol"));
        let mut other = i.clone();
        other.grammars[0].binaries = [("plan9-mips".to_string(), other.grammars[0].binaries.values().next().unwrap().clone())].into();
        assert!(install(&other, "lua", dir.path()).unwrap_err().to_string().contains(&crate::grammars::index::system()));
    }

    #[test]
    fn installing_again_replaces() {
        let src = tempfile::tempdir().unwrap();
        let dir = tempfile::tempdir().unwrap();
        install(&index(src.path(), b"one", None), "lua", dir.path()).unwrap();
        let got = install(&index(src.path(), b"two", None), "lua", dir.path()).unwrap();
        assert_eq!(std::fs::read(&got.binary).unwrap(), b"two");
        // The lock stays (removing it would let two installers lock different inodes); no
        // staging or backup directory does (review r2, Codex #4, Fable #3).
        let names: Vec<String> = std::fs::read_dir(dir.path()).unwrap().filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        let mut names = names;
        names.sort();
        assert_eq!(names, [".lua.lock", "lua"]);
        assert_eq!(installed(dir.path()).len(), 1);
    }

    #[test]
    fn a_definition_for_another_format_is_refused() {
        // D37: the index cannot install a Definition for `php`, or an `external` command.
        let src = tempfile::tempdir().unwrap();
        let dir = tempfile::tempdir().unwrap();
        for bad in [
            "name = 'php'\nprocessor = 'treesitter'\n[params]\ngrammar = 'lua'\nquery = '(x) @segment'\n",
            "name = 'lua'\nprocessor = 'external'\n[params]\ncommand = ['sh']\n",
            "name = 'lua'\nprocessor = 'treesitter'\n[params]\ngrammar = 'php'\nquery = '(x) @segment'\n",
        ] {
            let mut i = index(src.path(), b"x", None);
            i.grammars[0].definition = bad.into();
            let e = install(&i, "lua", dir.path()).unwrap_err();
            assert!(e.to_string().contains("Definition"), "{e}");
            assert_eq!(std::fs::read_dir(dir.path()).unwrap().filter_map(Result::ok).filter(|e| !e.file_name().to_string_lossy().starts_with('.')).count(), 0);
        }
    }

    #[test]
    fn a_failed_reinstall_keeps_the_previous_one() {
        let src = tempfile::tempdir().unwrap();
        let dir = tempfile::tempdir().unwrap();
        install(&index(src.path(), b"one", None), "lua", dir.path()).unwrap();
        assert!(install(&index(src.path(), b"two", Some("deadbeef")), "lua", dir.path()).is_err());
        assert_eq!(std::fs::read(installed(dir.path())[0].binary.clone()).unwrap(), b"one");
    }

    #[test]
    fn two_installs_at_once_leave_one_complete_install() {
        let src = tempfile::tempdir().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let i = index(src.path(), b"same", None);
        std::thread::scope(|s| {
            for _ in 0..2 {
                s.spawn(|| install(&i, "lua", dir.path()).unwrap());
            }
        });
        let got = installed(dir.path());
        assert_eq!(got.len(), 1);
        assert_eq!(std::fs::read(&got[0].binary).unwrap(), b"same");
        // No staging or backup directory is left behind.
        let left: Vec<_> = std::fs::read_dir(dir.path()).unwrap().filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n != "lua" && !n.ends_with(".lock"))
            .collect();
        assert!(left.is_empty(), "{left:?}");
    }

    #[test]
    fn list_shows_compiled_installed_and_available() {
        let src = tempfile::tempdir().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let i = index(src.path(), b"x", None);
        let before = list(&["php", "rust"], &installed(dir.path()), Ok(&i));
        assert!(before.contains("php") && before.contains("compiled in"), "{before}");
        assert!(before.contains("lua") && before.contains("available 0.2.0"), "{before}");
        install(&i, "lua", dir.path()).unwrap();
        let after = list(&["php"], &installed(dir.path()), Err("offline"));
        assert!(after.contains("installed 0.2.0"), "{after}");
        assert!(!after.contains("available"), "{after}");
    }
}
```

- [ ] **Step 2:** RED.

- [ ] **Step 3:** implement:

```rust
//! `--install-grammar` and `--list-grammars` (spec §7).
use super::index::{fetch, system, valid_name, Index};
use super::{binary_name, Installed};
use sha2::Digest;
use std::path::Path;

pub(crate) fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Downloads `lang`'s Grammar Helper for this system, checks its SHA-256, and only then writes
/// `<dir>/<lang>/` (binary, Definition, installed.json), replacing an older install.
pub fn install(index: &Index, lang: &str, dir: &Path) -> anyhow::Result<Installed> {
    if !valid_name(lang) {
        anyhow::bail!("`{lang}` is not a Grammar name (lowercase letters, digits and _)");
    }
    let e = index.get(lang).ok_or_else(|| anyhow::anyhow!("the Grammar index has no `{lang}` (see --list-grammars)"))?;
    let sys = system();
    let b = e.binaries.get(&sys).ok_or_else(|| anyhow::anyhow!("the Grammar index has no `{lang}` build for {sys}"))?;
    let bytes = fetch(&b.url)?;
    let got = hex(&sha2::Sha256::digest(&bytes));
    if !got.eq_ignore_ascii_case(&b.sha256) {
        anyhow::bail!("{}: SHA-256 is {got}, the index says {}; nothing was installed", b.url, b.sha256);
    }
    // D37: the Definition must describe this Grammar and nothing else.
    let def = crate::formats::definition::FormatDefinition::parse(&e.definition, crate::processors::Processors::known)
        .map_err(|err| anyhow::anyhow!("the index's Definition for `{lang}` is invalid: {err}"))?;
    let grammar = def.params.get("grammar").and_then(|v| v.as_str());
    if def.name != lang || def.processor != "treesitter" || grammar != Some(lang) {
        anyhow::bail!("the index's Definition for `{lang}` must have name, grammar = \"{lang}\" and processor = \"treesitter\"; nothing was installed");
    }
    std::fs::create_dir_all(dir)?;
    // One install of a Grammar at a time (std's File::lock, Rust ≥ 1.89; verified on 1.90).
    let lock = std::fs::File::create(dir.join(format!(".{lang}.lock")))?;
    lock.lock()?;
    // A staging directory of its own, in the same filesystem so the renames are atomic.
    let part = tempfile::Builder::new().prefix(&format!(".{lang}.part-")).tempdir_in(dir)?;
    let binary = part.path().join(binary_name(lang));
    std::fs::write(&binary, &bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755))?;
    }
    std::fs::write(part.path().join(format!("{lang}.toml")), &e.definition)?;
    let meta = serde_json::json!({"name": lang, "version": e.version, "sha256": got, "source": e.source, "license": e.license});
    std::fs::write(part.path().join("installed.json"), meta.to_string())?;
    let target = dir.join(lang);
    let old = tempfile::Builder::new().prefix(&format!(".{lang}.old-")).tempdir_in(dir)?.keep();
    std::fs::remove_dir(&old)?;
    let had_old = target.exists();
    if had_old {
        std::fs::rename(&target, &old)?;
    }
    if let Err(err) = std::fs::rename(part.path(), &target) {
        // Put the previous install back before reporting.
        if had_old {
            let _ = std::fs::rename(&old, &target);
        }
        return Err(anyhow::anyhow!("{}: {err}; the previous install was kept", target.display()));
    }
    let _ = part.keep(); // already renamed: nothing for TempDir to delete
    if had_old {
        let _ = std::fs::remove_dir_all(&old);
    }
    drop(lock);
    super::installed(dir).into_iter().find(|i| i.name == lang).ok_or_else(|| anyhow::anyhow!("{}: the install is incomplete", target.display()))
}

/// The text of `--list-grammars`: one line per Grammar, `name  state  detail`.
pub fn list(compiled_in: &[&str], installed: &[Installed], index: Result<&Index, &str>) -> String {
    let mut out = String::new();
    for g in compiled_in {
        out.push_str(&format!("{g:<14} compiled in\n"));
    }
    for i in installed {
        out.push_str(&format!("{:<14} installed {:<10} {}\n", i.name, i.version, i.dir.display()));
    }
    if let Ok(index) = index {
        let sys = system();
        for e in &index.grammars {
            let known = compiled_in.contains(&e.name.as_str()) || installed.iter().any(|i| i.name == e.name);
            if !known && e.binaries.contains_key(&sys) {
                out.push_str(&format!("{:<14} available {:<10} {}\n", e.name, e.version, e.source));
            }
        }
    }
    out
}
```

  The tests module imports `hex` with `use super::hex;`. The warning for an unreadable index is
  the caller's (C10), which passes `Err(reason)`. `installed` skips names starting with `.`
  (`valid_name` already refuses them), so locks, staging and backups never list.
  `TempDir::keep` gives up the deletion (tempfile 3.27.0, `src/dir/mod.rs:418`, the version in
  `Cargo.lock`), and `Builder::tempdir_in` creates the unique staging directory (`lib.rs:621`).

- [ ] **Step 4:** GREEN. Commit «Add --install-grammar and --list-grammars as a library».

---

## Chunk 8: the Grammar Helper template, its memory test, the build script and the index

Rules: spec §7, D12, D13, D23.

### Task 8.1: the Lua Grammar Helper

**Files:** `helpers/grammar/src/main.rs`, `helpers/grammar/definitions/lua.toml`,
`helpers/grammar/tests/{protocol.rs,fixtures/player.lua}`, `helpers/grammar/Cargo.toml`
(`[dev-dependencies] serde_json = "1"`) and so `Cargo.lock`.

`helpers/grammar/tests/fixtures/player.lua`:

```lua
local Player = {}

function Player:update(dt)
  self.x = self.x + dt -- needle
end

local function helper()
  return "needle"
end
```

`helpers/grammar/definitions/lua.toml` — the Definition the index ships with the helper:

```toml
# Lua: functions and methods (spec §5.3), through the Lua Grammar Helper (spec §7).
name       = "lua"
extensions = ["lua"]
shebangs   = ["lua", "luajit"]
processor  = "treesitter"

[params]
grammar = "lua"
query = '''
(function_declaration name: (_) @name) @segment
'''
kinds = { function_declaration = "function" }
```

Verified in the probe: tree-sitter-lua 0.2.0 (ABI 14) builds with tree-sitter 0.24.7, and this
query on the fixture captures `Player:update` (a `method_index_expression`) and `helper`
(`ts024.txt`). 0.4.1 and 0.5.0 are ABI 15, which tree-sitter 0.24 refuses (D1).

- [ ] **Step 1: failing tests** in `helpers/grammar/tests/protocol.rs`, with the same `H`
  harness as the example helper's tests (copy it; the two crates do not share test code):
  1. `hello` with `params` = the `[params]` of `definitions/lua.toml` and `threads: 4` answers
     `protocol: 1`, `name: "sfor-grammar-lua"`.
  2. `trails` on the fixture, anchors at both `needle`s: `[["Player:update"], ["helper"]]`.
  3. `names`: `["Player:update", "helper"]`.
  4. `hello` with a query that does not compile answers an `error` naming the query.
  5. `hello` whose `params.grammar` is not `lua` answers an `error` naming both Grammars.
  6. `memory_stays_flat_over_five_thousand_requests`, Linux only, as the example helper's, on a
     generated Lua file of 4 000 functions with one `needle` in the last one, through
     `trails` requests (D23: at most 2 MiB over the value after 500).
- [ ] **Step 2:** RED: `cargo test -p sfor-grammar`.
- [ ] **Step 3:** `helpers/grammar/src/main.rs`:

```rust
//! sfor-grammar: the Grammar Helper template (spec §7, D12). One build carries one Grammar,
//! chosen by a cargo feature, and serves it over protocol v1 with searchfor's own treesitter
//! code, in concurrent mode.
use searchfor::helper::protocol::{MatchPos, WireName};
use searchfor::helper::serve::{serve, Handler, Hello, Info};
use searchfor::model::Trail;
use searchfor::processors::treesitter::{compile, outline, parse, Compiled};
use searchfor::trail::{names_of, trail_for};
use std::path::Path;

#[cfg(feature = "lua")]
fn grammar() -> (&'static str, tree_sitter::Language) {
    ("lua", tree_sitter_lua::LANGUAGE.into())
}

struct Grammar(Compiled);

impl Handler for Grammar {
    fn trails(&self, path: &Path, matches: &[MatchPos]) -> anyhow::Result<Vec<Trail>> {
        let src = std::fs::read(path).map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))?;
        let tree = parse(&self.0, &src).ok_or_else(|| anyhow::anyhow!("{}: not parsed", path.display()))?;
        let anchors: Vec<usize> = matches.iter().map(|m| m.byte as usize).collect();
        let (lo, hi) = (anchors.iter().min().copied().unwrap_or(0), anchors.iter().max().copied().unwrap_or(0));
        let o = outline(&self.0, &tree, &src, Some(lo..hi + 1));
        Ok(anchors.iter().map(|&a| trail_for(&o, a)).collect())
        // `src`, `tree` and `o` are dropped here: nothing outlives the request (8458).
    }

    fn names(&self, path: &Path) -> anyhow::Result<Vec<WireName>> {
        let src = std::fs::read(path).map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))?;
        let tree = parse(&self.0, &src).ok_or_else(|| anyhow::anyhow!("{}: not parsed", path.display()))?;
        Ok(names_of(&outline(&self.0, &tree, &src, None)).iter().map(WireName::from).collect())
    }
}

fn main() {
    let (name, language) = grammar();
    let helper = format!("sfor-grammar-{name}");
    let info = Info { name: Box::leak(helper.into_boxed_str()), version: env!("CARGO_PKG_VERSION") };
    let r = serve(std::io::stdin().lock(), std::io::stdout(), info, |h: &Hello| {
        let asked = h.params.get("grammar").and_then(|v| v.as_str()).unwrap_or("");
        if asked != name {
            anyhow::bail!("this Grammar Helper carries `{name}`, the Definition asks for `{asked}`");
        }
        Ok(Grammar(compile(name, &language, &h.params)?))
    });
    if let Err(e) = r {
        eprintln!("{e:#}");
        std::process::exit(1);
    }
}
```

  `Compiled` must be `Send + Sync` for the `Handler` bound: `Query` and `Language` are
  (verified), and `HashMap<String, String>` is.

- [ ] **Step 4:** GREEN: `cargo test -p sfor-grammar`; paste the two RSS values. Commit «Add the
  Grammar Helper template with Lua».

### Task 8.2: the build script, the index and its schema

**Files:** `scripts/build-grammar-helper.sh`, `grammars/index.json`,
`docs/grammar-index.schema.json`.

- [ ] **Step 1:** `docs/grammar-index.schema.json` (draft 2020-12): an object with `version`
  (const 1) and `grammars` (array of objects with required `name` matching `^[a-z0-9_]+$`,
  `version`, `source`, `license`, `definition`, and `binaries`: an object whose keys match
  `^[a-z0-9]+-[a-z0-9_]+$` and whose values have required `url` (string starting with
  `https://` or `file://`) and `sha256` (`^[0-9a-f]{64}$`)). `grammars/index.json` is
  `{"version": 1, "grammars": []}` (D13: M5 fills it from the release).
- [ ] **Step 2:** `scripts/build-grammar-helper.sh`:

```bash
#!/usr/bin/env bash
# Builds the Grammar Helper of one Grammar for this system and prints its index entry
# (spec §7, M2 plan D13). M5's release pipeline runs it per system and fills grammars/index.json.
# Usage: scripts/build-grammar-helper.sh LANG [OUT_DIR [BASE_URL]]
# Needs jq and sha256sum (or shasum on macOS).
set -euo pipefail
lang="${1:?usage: $0 LANG [OUT_DIR [BASE_URL]]}"
out="${2:-target/grammar-helpers}"
mkdir -p "$out"
out="$(cd "$out" && pwd)"
base="${3:-file://$out}"
case "$(uname -s)" in Linux) os=linux ;; Darwin) os=macos ;; *) os=windows ;; esac
arch="$(uname -m)"; [ "$arch" = arm64 ] && arch=aarch64
system="$os-$arch"
definition="helpers/grammar/definitions/$lang.toml"
[ -f "$definition" ] || { echo "no $definition" >&2; exit 2; }
cargo build --release --quiet -p sfor-grammar --no-default-features --features "$lang"
file="sfor-grammar-$lang-$system"
cp target/release/sfor-grammar "$out/$file"
sha=$( (sha256sum "$out/$file" 2>/dev/null || shasum -a 256 "$out/$file") | cut -d' ' -f1)
version=$(cargo metadata --format-version 1 --quiet \
  | jq -r --arg l "tree-sitter-$lang" '.packages[] | select(.name == $l) | .version' | head -1)
jq -n --arg name "$lang" --arg version "$version" --arg system "$system" \
  --arg url "$base/$file" --arg sha "$sha" --rawfile definition "$definition" \
  '{name: $name, version: $version, source: ("https://crates.io/crates/tree-sitter-" + $name),
    license: "MIT", definition: $definition,
    binaries: {($system): {url: $url, sha256: $sha}}}'
```

  `uname -m` gives `x86_64` and `aarch64` on Linux, matching `std::env::consts::ARCH`; on
  macOS it gives `arm64`, mapped above; Windows is M5's.
- [ ] **Step 3:** run `scripts/build-grammar-helper.sh lua /tmp/sfor-gh` and validate its output,
  wrapped as `{"version":1,"grammars":[<entry>]}`, against the schema with a small `#[test]` in
  `helpers/grammar/tests/protocol.rs` that runs the script only when `SFOR_TEST_BUILD_SCRIPT=1`
  (it rebuilds in release; skipped by default, with a printed note). Paste the entry in the
  report. Commit «Add the Grammar Helper build script and the empty Grammar index».

---

## Chunk 9: the HelperProcessor, Grammar Helpers in the registry, and installed Definitions

Rules: spec §4.2, §7, design note 8, D26.

### Task 9.0: HelperProcessor (src/processors/external.rs)

**Files:** `src/processors/external.rs`.

- [ ] **Step 1: failing tests**, using a `Host` over the fake launcher of Task 4.1 (C4 leaves
  `Fake`, `helper` and `echo` in `pub(crate) mod testing` inside `host.rs`, under
  `#[cfg(test)]`, so this file can use them; moved here from C4 to keep C4 under 1000 lines):

```rust
    #[test]
    fn a_file_on_disk_is_sent_by_its_path() { /* Input with disk: Some("dir/a.ini"): the request's path is "dir/a.ini" (the fake echoes the path as the Segment name). */ }

    #[test]
    fn bytes_that_differ_from_the_file_go_through_a_temporary_file() {
        /* Input with disk: None and bytes b"[s]\nk = needle\n": the fake reads the file at the
           request's path and answers its contents as the Segment name; the name equals the
           bytes, and the temporary file no longer exists after `trails` returns. */
    }

    #[test]
    fn lines_are_computed_from_the_bytes() { /* anchors [0, 4, 9] in "[s]\nk=1\nneedle\n" send lines 1, 2, 3. */ }

    #[test]
    fn a_wrong_number_of_trails_is_an_error() { /* the fake answers one Trail for two anchors: Err naming "ini" and "2 Matches". */ }

    #[test]
    fn a_disabled_helper_is_unavailable() { /* a Launcher that fails: every call returns an error that `downcast_ref::<crate::helper::Unavailable>()` finds, and one warning in total (D33). */ }

    #[test]
    fn names_come_back_as_model_names() { /* the fake answers one WireName; `names` returns its Name with the same pieces. */ }
```

  Write each body in full from its comment.

- [ ] **Step 2:** RED.

- [ ] **Step 3:** implement:

```rust
/// A Processor whose work is done by a Helper Processor (spec §4.4): built-in to sfor's
/// pipeline, it only turns the contract into requests.
pub struct HelperProcessor {
    host: crate::helper::host::Host,
    tmp: std::sync::Arc<RunTemp>,
}

impl HelperProcessor {
    pub fn new(cfg: crate::helper::HostConfig, launcher: Box<dyn crate::helper::Launcher>, tmp: std::sync::Arc<RunTemp>, warn: crate::helper::Warn) -> Self {
        Self { host: crate::helper::host::Host::new(cfg, launcher, warn), tmp }
    }

    pub fn shutdown(&self) {
        self.host.shutdown();
    }

    /// The path the helper reads, and the temporary file to keep alive until it answered: the
    /// file itself when its bytes are the Source's and its path is UTF-8.
    fn path_for(&self, input: &crate::model::Input) -> anyhow::Result<(String, Option<TempFile>)> {
        if let Some(s) = input.disk.and_then(|p| p.to_str()) {
            return Ok((s.to_string(), None));
        }
        let t = self.tmp.write(input.path, input.bytes)?;
        let s = t.path().to_str().ok_or_else(|| anyhow::anyhow!("the temporary path is not UTF-8"))?.to_string();
        Ok((s, Some(t)))
    }
}

impl super::Processor for HelperProcessor {
    fn trails(&self, input: &crate::model::Input, _: &toml::Table, anchors: &[usize]) -> anyhow::Result<Vec<crate::model::Trail>> {
        use crate::helper::{protocol::{MatchPos, Request}, HelperError};
        let (path, _tmp) = self.path_for(input)?;
        let lines = crate::trail::LineIndex::new(input.bytes);
        let matches = anchors.iter().map(|&b| MatchPos { line: lines.line_of(b), byte: b as u64 }).collect();
        let f = self.host.format();
        match self.host.request(|id| Request::Trails { id, path, matches }) {
            Ok(r) => match r.trails {
                Some(t) if t.len() == anchors.len() => Ok(t),
                Some(t) => anyhow::bail!("{f}: the helper returned {} Trails for {} Matches", t.len(), anchors.len()),
                None => anyhow::bail!("{f}: the helper's answer has no `trails`"),
            },
            Err(HelperError::Disabled) => Err(crate::helper::Unavailable.into()),
            Err(e) => anyhow::bail!("{f}: {e}"),
        }
    }

    fn names(&self, input: &crate::model::Input, _: &toml::Table) -> anyhow::Result<Vec<crate::model::Name>> {
        use crate::helper::{protocol::Request, HelperError};
        let (path, _tmp) = self.path_for(input)?;
        let f = self.host.format();
        match self.host.request(|id| Request::Names { id, path }) {
            Ok(r) => match r.names {
                Some(n) => Ok(n.into_iter().map(|w| w.into_name()).collect()),
                None => anyhow::bail!("{f}: the helper's answer has no `names`"),
            },
            Err(HelperError::Disabled) => Err(crate::helper::Unavailable.into()),
            Err(e) => anyhow::bail!("{f}: {e}"),
        }
    }
}
```

  The pipeline (C10) prints a Processor error as `sfor: <path>: <error>`, so a failed request's
  warning names the file and, through the `{f}:` prefix, the Definition (spec §4.4); for
  `Unavailable` it prints nothing. Either way the file is Plain (D33).

- [ ] **Step 4:** GREEN. Commit «Add the HelperProcessor: the Processor contract over protocol
  v1».

### Task 9.1: installed Definitions and unavailable Formats

**Files:** `src/formats/{mod,definition}.rs`.

- [ ] **Step 1: failing tests** in `formats/mod.rs`:

```rust
    #[test]
    fn installed_definitions_load_between_built_ins_and_user_files() {
        let grammars = tempfile::tempdir().unwrap();
        let lua = grammars.path().join("lua");
        std::fs::create_dir(&lua).unwrap();
        std::fs::write(lua.join("lua.toml"), "name='lua'\nextensions=['lua']\nprocessor='treesitter'\n[params]\ngrammar='lua'\nquery='(x) @segment'").unwrap();
        std::fs::write(lua.join(crate::grammars::binary_name("lua")), "").unwrap();
        std::fs::write(lua.join("installed.json"), r#"{"version":"0.2.0"}"#).unwrap();
        let user = tempfile::tempdir().unwrap();
        let (r, w) = Registry::load_all(Some(user.path()), Some(grammars.path()), &crate::processors::Processors::known);
        assert!(w.is_empty(), "{w:?}");
        assert!(matches!(r.get("lua").unwrap().origin, Origin::Installed(_)));
        assert!(r.list().contains("installed ("));
        // A user file still wins (#6).
        std::fs::write(user.path().join("lua.toml"), "name='lua'\nextensions=['luau']\nprocessor='treesitter'\n[params]\ngrammar='lua'\nquery='(x) @segment'").unwrap();
        let (r, _) = Registry::load_all(Some(user.path()), Some(grammars.path()), &crate::processors::Processors::known);
        assert_eq!(r.get("lua").unwrap().extensions, ["luau"]);
        assert!(matches!(r.get("lua").unwrap().origin, Origin::Overridden(_)));
        assert!(r.list().contains("overridden ("));
    }

    #[test]
    fn list_marks_unavailable_formats() {
        let (r, _) = Registry::load(None, &crate::processors::Processors::known);
        let text = r.list_with(&|d| d.name != "php");
        let php = text.lines().find(|l| l.starts_with("php ")).unwrap();
        assert!(php.ends_with("(unavailable)"), "{php}");
        assert!(!text.lines().find(|l| l.starts_with("rust ")).unwrap().contains("unavailable"));
    }
```

- [ ] **Step 2:** RED.
- [ ] **Step 3:** `Origin` gains `Installed(PathBuf)`; `Registry::load_all(user_dir,
  grammars_dir, known)` loads the built-ins, then the Definition of every
  `crate::grammars::installed(grammars_dir)` with `Origin::Installed` (replacing a built-in of
  the same name, which none is in M2), then the user directory, whose overrides keep
  `Origin::Overridden` whatever they replace; `Registry::load(user_dir, known)` becomes
  `load_all(user_dir, None, known)`. An installed Definition that does not parse is a warning
  naming the file, as a user one. `list_with(&self, available: &dyn Fn(&FormatDefinition) ->
  bool)` appends `  (unavailable)` to the line of each Definition `available` rejects;
  `list()` is `list_with(&|_| true)`; the origin column shows `installed (<path>)`.
- [ ] **Step 4:** GREEN. Commit «Load installed Grammar Definitions and mark unavailable
  Formats».

### Task 9.2: Grammar Helpers in `Processors`

**Files:** `src/processors/mod.rs`.

- [ ] **Step 1: failing tests**:

```rust
    #[test]
    fn an_installed_grammar_is_served_by_its_helper() {
        let grammars = tempfile::tempdir().unwrap();
        let lua = grammars.path().join("lua");
        std::fs::create_dir(&lua).unwrap();
        std::fs::write(lua.join(crate::grammars::binary_name("lua")), "").unwrap();
        std::fs::write(lua.join("lua.toml"), "name='lua'\nprocessor='treesitter'").unwrap();
        std::fs::write(lua.join("installed.json"), r#"{"version":"0.2.0"}"#).unwrap();
        let (w, seen) = collecting();
        let ps = Processors::new(4, w).with_grammars(Some(grammars.path().to_path_buf()));
        let d = def("name='lua'\nprocessor='treesitter'\n[params]\ngrammar='lua'\nquery=''");
        assert!(matches!(ps.for_definition(&d), Some(ProcessorRef::Helper(_))));
        assert!(ps.available(&d));
        assert!(ps.describes(&d));
        assert!(seen.lock().unwrap().is_empty());
        let missing = def("name='cobol'\nprocessor='treesitter'\n[params]\ngrammar='cobol'\nquery=''");
        assert!(!ps.available(&missing));
        assert!(ps.available(&def("name='php2'\nprocessor='treesitter'\n[params]\ngrammar='php'\nquery=''")));
    }
```

  (No process is started: `for_definition` only builds the host, which starts its first copy
  on the first request.)
- [ ] **Step 2:** RED.
- [ ] **Step 3:** `Processors` gains `grammars: Option<PathBuf>`, set by
  `pub fn with_grammars(mut self, dir: Option<PathBuf>) -> Self`. In `for_definition`, the
  `treesitter` branch, after the compiled-in check:

```rust
                if let Some(bin) = self.grammars.as_deref().and_then(|d| crate::grammars::installed_binary(d, grammar)) {
                    let timeout = match def.params.get("timeout_ms").and_then(|v| v.as_integer()) {
                        Some(ms) if ms > 0 => std::time::Duration::from_millis(ms as u64),
                        _ => crate::helper::DEFAULT_TIMEOUT,
                    };
                    let bin = bin.to_string_lossy().into_owned();
                    return self.helper(def, || {
                        Ok(crate::helper::HostConfig {
                            format: def.name.clone(),
                            command: vec![bin],
                            mode: crate::helper::Mode::Concurrent,
                            timeout,
                            params: def.params.clone(),
                            threads: self.threads,
                        })
                    });
                }
```

  and `pub fn available(&self, def: &FormatDefinition) -> bool`: false only for a `treesitter`
  Definition whose Grammar is neither compiled in nor installed (it does not warn); `describes`
  (C1) answers true for an installed Grammar too. The installed Grammars are read once per run
  into a `OnceLock<Vec<Installed>>` inside `Processors`, and `for_definition`, `describes` and
  `available` look the binary up there, never on disk again (review r1, Codex #5). The
  `treesitter` branch above uses that list instead of calling `installed_binary`.
- [ ] **Step 4:** GREEN. Commit «Serve Grammars that are not compiled in through their Grammar
  Helper».

---

## Chunk 10: the pipeline

Everything links here. Locate the code by name: `load_registry`, `main` (actions),
`Ctx`, `trail_file`, `outlined_file`, `complete_input`, `describe`, `names_of` in `src/app.rs`.

### Task 10.1: the two flags

**Files:** `src/cli/mod.rs`.

- [ ] **Step 1: failing test** in the module's tests: `sfor --list-grammars` parses with
  `list_grammars == true` and no pattern; `sfor --install-grammar lua` gives
  `install_grammar == Some("lua")`; both are actions like `--list-formats`, so a pattern is not
  required.
- [ ] **Step 2:** RED. **Step 3:** next to `list_formats`:

```rust
    /// List the compiled-in, installed and available Grammars (spec §7).
    #[arg(long)]
    pub list_grammars: bool,
    /// Install a Grammar Helper from the Grammar index (spec §7).
    #[arg(long, value_name = "LANG")]
    pub install_grammar: Option<String>,
```

  and wherever `list_formats` makes the pattern optional, these two do too.
- [ ] **Step 4:** GREEN. Commit «Add --list-grammars and --install-grammar».

### Task 10.2: `Processors` per run, `Input.disk`, and the actions

**Files:** `src/app.rs`.

- [ ] **Step 1:** the end-to-end tests of Task 10.4 that need this task are written first and
  fail (RED); this task makes them pass.
- [ ] **Step 2:** the registry: `load_registry` calls `Registry::load_all(user_dir,
  grammars::grammars_dir().as_deref(), &Processors::known)`.
- [ ] **Step 3:** the actions, before the search, in `main`:
  - `--list-grammars`: `grammars::index::fetch(&grammars::index_url())` then `Index::parse`;
    on an error, the warning `sfor: the Grammar index is not available: <error>` (unless
    `--no-messages`); print `grammars::install::list(COMPILED_IN, &installed(dir), index)`;
    exit 0 (D28).
  - `--install-grammar <LANG>`: fetch and parse the index, `install(&index, lang, &dir)`; print
    `installed <name> <version> in <dir>`; exit 0. Any error: `sfor: <error>` on stderr, exit 2.
    No Grammar directory (no data dir and no `SFOR_GRAMMARS_DIR`) is an error.
  - `--list-formats`: `registry.list_with(&|d| processors.available(d))`, with a `Processors`
    built for it.
- [ ] **Step 4:** `Ctx` gains `processors: Processors`, built once per run with
  `threads = if sequential { 1 } else { search::walker_threads(args) }`, a warning sink that
  prints with `eprintln!` unless `--no-messages`, and `.with_grammars(grammars::grammars_dir())`.
  It is dropped with `Ctx` at the end of `run`, which closes every helper's stdin (spec §4.4,
  Lifetime); `-q` stopping early drops it too.
- [ ] **Step 5: resolve after the search** (design note 2, review r1, Codex #5). The
  `processors::by_name` calls in `app.rs` (`46a4b74`: `trail_file` 686, `complete_input` 1093,
  `describe` 1111, `names_of` 1182) change like this:
  - `trail_file` keeps `processors::by_name(&def.processor)`: it runs before the search and only
    picks M1's streaming path for stateless Outline Processors (Markdown); it builds nothing and
    warns about nothing.
  - `complete_input` asks `self.processors.describes(def)`, which has no side effects.
  - `describe` and `names_of`, which run after the search found a Match (or, for
    `--in-headings`, on the file being read anyway), call `self.processors.for_definition(def)`;
    where a `&dyn Processor` is passed on, pass `&*p`.
- [ ] **Step 6: Plain when there is no Processor (D33, Codex #6).** In `describe` and
  `names_of`, when `for_definition` gives `None`, or the Processor returns an error, the file is
  Plain: `Described.format` is `None` and its Trails are empty, exactly as for a file no
  Definition claims, so every printer renders it as Plain (flat: `path:line: text`, spec §6.2 —
  verified with M1's `sfor` on `misc/anchor.txt`; JSON: `"format": null`). An error is printed as
  M1 does, `sfor: <path>: <error>`, unless it is `crate::helper::Unavailable`
  (`e.downcast_ref`), which prints nothing. `trails()` (`app.rs:1223`) returns that decision with
  the Trails, so `describe` can clear `format`.
- [ ] **Step 7: `Input.disk` (D8, Codex #4).** One helper, used by every `Input` literal:

```rust
/// The canonical path of the file on disk whose bytes are exactly `src`'s: a regular file
/// outside /dev and /proc, searched without a BOM or UTF-16 transcoding, whose length on disk
/// is the Source's. The helper gets this resolved path, never the one typed: `stdin` typed in
/// /dev, `/dev/stdin`, `/dev/fd/0` or a symlink to them name the helper's own protocol pipe
/// in the helper's process (review r2, Fable #1; r3, Codex #2). Resolved here, a stream
/// redirected from a file becomes that file (safe: same bytes), and a pipe or a FIFO does not
/// resolve to a regular file. Anything else reaches the helper as a temporary copy (spec §4.4).
fn disk_of(t: &Target, src: &Source) -> Option<PathBuf> {
    let real = std::fs::canonicalize(t.fs.as_deref()?).ok()?;
    if real.starts_with("/dev") || real.starts_with("/proc") {
        return None;
    }
    let m = std::fs::metadata(&real).ok()?;
    (m.is_file() && !src.had_bom && m.len() == src.bytes().len() as u64).then_some(real)
}
```

  Each caller keeps the `PathBuf` alive for the Processor call:
  `let disk = disk_of(t, &src); let input = Input { disk: disk.as_deref(), … };`. It costs one
  `canonicalize` and one `stat` per file with Matches, never on files without them.
- [ ] **Step 8:** `cargo test --workspace` passes, M1's tests included. Commit «Run the
  Processors of each search through a per-run registry».

### Task 10.3: tree labels of key and element Segments

**Files:** `src/output/tree.rs`.

- [ ] **Step 1: failing test** in its tests: a Trail `server › ports[2] › host` of kinds `key`,
  `element`, `key` renders in the tree as the three names, without a kind word (today `label`
  falls back to `"{kind} {name}"`, which would print `key server`). Snapshot it.
- [ ] **Step 2:** RED. **Step 3:** in `label`, `"key" | "element" => name.clone(),`.
  Code Segments carry a `signature`, which `label` already prints first.
- [ ] **Step 4:** GREEN. Commit «Show key and element Segments by name in the tree».

### Task 10.4: end-to-end tests of the new Formats — tests/cli_m2.rs

**Files:** `tests/common/mod.rs`, `tests/cli_m2.rs`, `tests/snapshots/cli_m2__*.snap`.

- [ ] **Step 1:** `Fixture` isolates Grammars as it isolates Formats: a third `TempDir`,
  `grammars`, and `sfor()` sets `SFOR_GRAMMARS_DIR` to it and `SFOR_GRAMMAR_INDEX` to
  `file://<grammars>/no-index.json` (absent), so no test reads the user's Grammars or the
  network.
- [ ] **Step 2:** the tests (written before Task 10.2's code, RED until it is done):

```rust
//! End-to-end tests of M2's Formats (plan, Chunk 10): fixtures under tests/fixtures/{data,code}.
mod common;

use common::{code, run, stderr, stdout, Fixture};

const FORMATS: &[(&str, &str)] = &[
    ("json", "data/config.json"),
    ("yaml", "data/config.yaml"),
    ("toml", "data/shop.toml"),
    ("php", "code/cart.php"),
    ("javascript", "code/widget.js"),
    ("typescript", "code/shape.ts"),
    ("tsx", "code/view.tsx"),
    ("bash", "code/deploy.sh"),
    ("go", "code/server.go"),
    ("rust", "code/thing.rs"),
    ("python", "code/outer.py"),
];

#[test]
fn m2_01_every_format_in_three_layouts() {
    // Spec §11 #2: a fixture and a snapshot of tree, one-line and JSON output per Format.
    let fx = Fixture::new();
    for (name, file) in FORMATS {
        for (layout, flag) in [("tree", "--tree"), ("flat", "--flat"), ("json", "--json")] {
            let o = run(&mut fx.sfor(["needle", file, flag]));
            assert_eq!(code(&o), 0, "{name} {layout}: {}", stderr(&o));
            assert!(stderr(&o).is_empty(), "{name} {layout}: {}", stderr(&o));
            let mut out = stdout(&o);
            if layout == "json" {
                out = common::mask_json_times(&out);
            }
            insta::assert_snapshot!(format!("m2_01_{name}_{layout}"), out);
        }
    }
}

#[test]
fn m2_02_yaml_key_path() {
    // Spec §11 #4.
    let fx = Fixture::new();
    let o = run(&mut fx.sfor(["needle.example", "data/config.yaml", "--flat"]));
    assert_eq!(stdout(&o), "9: server › ports[2] › host │     - host: needle.example\n");
}

#[test]
fn m2_03_php_method_and_signatures() {
    // Spec §11 #5.
    let fx = Fixture::new();
    let flat = stdout(&run(&mut fx.sfor(["needle: merge", "code/cart.php", "--flat"])));
    assert_eq!(flat, "10: Cart › addItem │         // needle: merge equal items\n");
    let tree = stdout(&run(&mut fx.sfor(["needle: merge", "code/cart.php", "--tree"])));
    assert!(tree.contains("class Cart\n"), "{tree}");
    assert!(tree.contains("public function addItem(Item $item, int $qty = 1): void\n"), "{tree}");
}

#[test]
fn m2_04_within_and_expand_on_code() {
    let fx = Fixture::new();
    let o = stdout(&run(&mut fx.sfor(["needle", "code", "--within", "Cart › addItem", "--flat"])));
    assert_eq!(o.lines().count(), 1, "{o}");
    assert!(o.starts_with("code/cart.php:10:"), "{o}");
    let o = stdout(&run(&mut fx.sfor(["self.needle", "code/thing.rs", "--expand", "--flat"])));
    // The Section of `helper`: lines 7 to 9.
    assert!(o.contains("pub fn helper(&self) -> u32"), "{o}");
    assert!(o.contains("self.needle + 1"), "{o}");
    assert!(!o.contains("pub trait Greeter"), "{o}");
}

#[test]
fn m2_05_in_headings_on_names_and_keys() {
    let fx = Fixture::new();
    let o = stdout(&run(&mut fx.sfor(["--in-headings", "^addItem$", "code", "--flat"])));
    assert_eq!(o, "code/cart.php:8: Cart │     public function addItem(Item $item, int $qty = 1): void\n");
    let o = stdout(&run(&mut fx.sfor(["--in-headings", "^host$", "data/config.yaml", "--flat"])));
    assert_eq!(o.lines().count(), 3, "{o}");
    // D35: keys that hold arrays are names too, in the three syntaxes, and --expand prints
    // their Section (review r1, Codex #11, Fable #3).
    for (file, key, line) in [("data/config.json", "^ports$", 5), ("data/config.yaml", "^ports$", 4), ("data/shop.toml", "^tags$", 13)] {
        let o = stdout(&run(&mut fx.sfor(["--in-headings", key, file, "--flat"])));
        assert_eq!(o.lines().count(), 1, "{file}: {o}");
        assert!(o.starts_with(&format!("{line}:")), "{file}: {o}");
    }
    let o = stdout(&run(&mut fx.sfor(["--in-headings", "^ports$", "data/config.yaml", "--expand", "--flat"])));
    assert!(o.contains("host: needle.example"), "{o}");
}

#[test]
fn m2_06_content_signals_only_without_extension() {
    // D15: a shebang decides an extension-less script, never a `.txt`.
    let fx = Fixture::new();
    std::fs::write(fx.file("deploy"), "#!/usr/bin/env bash\nrun() {\n  echo needle\n}\n").unwrap();
    std::fs::write(fx.file("notes.txt"), "#!/usr/bin/env bash\nrun() {\n  echo needle\n}\n").unwrap();
    assert_eq!(stdout(&run(&mut fx.sfor(["needle", "deploy", "--flat"]))), "3: run │   echo needle\n");
    let plain = stdout(&run(&mut fx.sfor(["needle", "notes.txt", "--flat"])));
    assert_eq!(plain.lines().count(), 1, "{plain}");
    assert!(!plain.contains('›'), "notes.txt must be Plain: {plain}");
}

#[test]
fn m2_07_list_and_export_the_new_formats() {
    let fx = Fixture::new();
    let o = stdout(&run(&mut fx.sfor(["--list-formats"])));
    for (name, _) in FORMATS {
        assert!(o.lines().any(|l| l.starts_with(&format!("{name} ")) && l.contains("built-in")), "{name}: {o}");
    }
    let out = tempfile::tempdir().unwrap();
    let target = out.path().join("t");
    assert_eq!(code(&run(&mut fx.sfor([std::ffi::OsStr::new("--export-formats"), target.as_os_str()]))), 0);
    assert_eq!(std::fs::read_dir(&target).unwrap().count(), 1 + FORMATS.len());
}

#[test]
fn m2_08_list_grammars_offline() {
    let fx = Fixture::new();
    let o = run(&mut fx.sfor(["--list-grammars"]));
    assert_eq!(code(&o), 0);
    assert_eq!(stdout(&o).lines().filter(|l| l.contains("compiled in")).count(), 8, "{}", stdout(&o));
    assert!(stderr(&o).contains("Grammar index is not available"), "{}", stderr(&o));
}

#[test]
fn m2_09_utf16_and_bom_code_keeps_its_offsets() {
    let fx = Fixture::new();
    let src = std::fs::read_to_string(fx.file("code/cart.php")).unwrap();
    let mut bom = b"\xEF\xBB\xBF".to_vec();
    bom.extend_from_slice(src.as_bytes());
    std::fs::write(fx.file("bom.php"), bom).unwrap();
    let mut utf16 = b"\xFF\xFE".to_vec();
    utf16.extend(src.encode_utf16().flat_map(u16::to_le_bytes));
    std::fs::write(fx.file("utf16.php"), utf16).unwrap();
    for f in ["bom.php", "utf16.php"] {
        let o = stdout(&run(&mut fx.sfor(["needle: merge", f, "--flat"])));
        assert!(o.starts_with("10: Cart › addItem │"), "{f}: {o}");
    }
}

#[test]
fn m2_10_a_missing_grammar_is_one_warning() {
    let fx = Fixture::new();
    std::fs::write(fx.formats.path().join("lua.toml"), "name='lua'\nextensions=['lua']\nprocessor='treesitter'\n[params]\ngrammar='lua'\nquery='(x) @segment'\n").unwrap();
    std::fs::write(fx.file("a.lua"), "-- needle\n").unwrap();
    std::fs::write(fx.file("b.lua"), "-- needle\n").unwrap();
    let o = run(&mut fx.sfor(["needle", "a.lua", "b.lua", "--flat"]));
    assert_eq!(code(&o), 0);
    assert_eq!(stderr(&o).lines().count(), 1, "{}", stderr(&o));
    // Plain exactly as a file no Definition claims (D33): the same bytes as a `.txt` copy.
    std::fs::copy(fx.file("a.lua"), fx.file("a.txt")).unwrap();
    std::fs::copy(fx.file("b.lua"), fx.file("b.txt")).unwrap();
    let plain = stdout(&run(&mut fx.sfor(["needle", "a.txt", "b.txt", "--flat"])));
    assert_eq!(stdout(&o).replace(".lua", ".txt"), plain);
    let json = stdout(&run(&mut fx.sfor(["needle", "a.lua", "--json"])));
    let m: serde_json::Value = json.lines().map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap()).find(|v| v["type"] == "match").unwrap();
    assert!(m["data"]["format"].is_null(), "{m}");
    let list = stdout(&run(&mut fx.sfor(["--list-formats"])));
    assert!(list.lines().any(|l| l.starts_with("lua ") && l.ends_with("(unavailable)")), "{list}");
}

#[test]
fn m2_12_deep_json_is_not_a_crash() {
    // D34, Codex #8: a million nested arrays, in a separate process: no stack overflow.
    let fx = Fixture::new();
    let n = 1_000_000;
    let mut s = "[".repeat(n);
    s.push_str("\"needle\"");
    s.push_str(&"]".repeat(n));
    std::fs::write(fx.file("deep.json"), s).unwrap();
    let o = run(&mut fx.sfor(["needle", "deep.json", "--flat", "--fragment-width", "60"]));
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    assert_eq!(stdout(&o).lines().count(), 1);
}

#[test]
fn m2_13_files_without_matches_cost_nothing() {
    // Design note 2, Codex #5: a missing Grammar and an `external` helper whose command does not
    // exist give no warning when none of their files has a Match.
    let fx = Fixture::new();
    std::fs::write(fx.formats.path().join("lua.toml"), "name='lua'\nextensions=['lua']\nprocessor='treesitter'\n[params]\ngrammar='lua'\nquery='(x) @segment'\n").unwrap();
    std::fs::write(fx.formats.path().join("ini.toml"), "name='ini'\nextensions=['ini']\nprocessor='external'\n[params]\ncommand=['/no/such/helper']\n").unwrap();
    std::fs::write(fx.file("a.lua"), "-- nothing\n").unwrap();
    std::fs::write(fx.file("a.ini"), "[s]\nk = v\n").unwrap();
    let o = run(&mut fx.sfor(["zzqq-absent", "."]));
    assert_eq!(code(&o), 1);
    assert_eq!(stderr(&o), "");
}

#[test]
fn m2_11_json_carries_signatures() {
    let fx = Fixture::new();
    let o = stdout(&run(&mut fx.sfor(["needle: merge", "code/cart.php", "--json"])));
    let m: serde_json::Value = o.lines().map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap()).find(|v| v["type"] == "match").unwrap();
    assert_eq!(m["data"]["format"], "php");
    assert_eq!(m["data"]["trail"][1]["signature"], "public function addItem(Item $item, int $qty = 1): void");
    common::assert_valid_json_schema(&o);
}
```

  `common::mask_json_times` and `common::assert_valid_json_schema` exist if M1's tests have
  them under those or other names; reuse M1's helpers, and add them to `tests/common/mod.rs`
  only if M1 keeps them inside `tests/cli.rs`. The exact one-line layout of a file named on
  the command line (with or without the path, with `:` or ` ` after the line number) is M1's:
  if an expected string above differs from M1's layout only in that, follow M1 and fix the
  string (to verify when the test first runs).
- [ ] **Step 3:** GREEN with Task 10.2; read the 33 new snapshots in full (one per Format and
  layout) before committing. Commit «Add end-to-end tests of the M2 Formats».

---

## Chunk 11: Helper Processors end to end, install, benchmark and ceilings

### Task 11.1: building the workspace helpers from sfor's tests

**Files:** `tests/common/mod.rs`.

- [ ] `pub fn workspace_bin(package: &str, bin: &str, features: &[&str]) -> PathBuf`: runs
  `$CARGO build --quiet -p <package> --bin <bin> [--features …] --message-format=json` in
  `CARGO_MANIFEST_DIR` once per (package, features) per test binary (`OnceLock<Mutex<HashMap>>`),
  and returns the `executable` of the `compiler-artifact` message whose `target.name` is
  `bin` (D29). `$CARGO` is `std::env::var("CARGO")`, `cargo` if unset. A build failure panics
  with cargo's stderr.

### Task 11.2: tests/helpers.rs

**Files:** `tests/helpers.rs`, `tests/fixtures/ini/{app.ini,web.ini,db.ini,bom.ini}`.

The fixtures: `app.ini` is the example helper's fixture (Task 5.2); `web.ini`:

```ini
[http]
listen = needle:80
```

`db.ini`:

```ini
[primary]
url = postgres://needle
```

`bom.ini`: the bytes `EF BB BF` followed by `app.ini`'s text.

Every test writes `ini.toml` into `fx.formats` with the absolute path of
`workspace_bin("sfor-example-helper", "sfor-example-helper", &[])`, a `mode`, and
`timeout_ms = 2000` unless the test says otherwise:

```rust
fn ini(fx: &Fixture, mode: &str, timeout_ms: u64) {
    let bin = common::workspace_bin("sfor-example-helper", "sfor-example-helper", &[]);
    let def = format!(
        "name = 'ini'\nextensions = ['ini']\nprocessor = 'external'\n[params]\ncommand = [{:?}]\nmode = '{mode}'\ntimeout_ms = {timeout_ms}\n",
        bin.to_str().unwrap()
    );
    std::fs::write(fx.formats.path().join("ini.toml"), def).unwrap();
}

/// `start` lines the example helper wrote to its log: one per process.
fn starts(log: &std::path::Path) -> usize {
    std::fs::read_to_string(log).unwrap_or_default().lines().filter(|l| l.starts_with("start ")).count()
}
```

- [ ] **Step 1: the tests**, each RED until its behaviour works end to end (C4, C5, C9 and C10 make
  them pass; a test that is green on first run is checked by breaking the behaviour on
  purpose once, then restoring it):
  1. `h01_every_mode_gives_the_same_trails` (spec §11 #15): for `pool`, `concurrent`,
     `oneshot`, `sfor needle ini --flat --sort path` prints the same text, containing
     `ini/app.ini:3: server › host │ host = needle.example`, `ini/db.ini:2: primary › url │ …`
     and `ini/web.ini:2: http › listen │ …`.
  2. `h02_a_pool_runs_at_most_the_thread_count`: twelve generated files `gen/g<i>.ini`, each
     with a `needle`; `SFOR_EXAMPLE_SLOW=gen` and `SFOR_EXAMPLE_LOG=<tmp>/log`; with
     `-j 3` and `pool`, `starts(log)` is between 1 and 3; with `concurrent`, exactly 1; with
     `oneshot`, exactly 12. Every file has its Trail in all three.
  3. `h03_out_of_order_answers_reach_their_files`: `concurrent`, `-j 4`, `SFOR_EXAMPLE_SLOW=app`:
     every line still carries its own file's Trail.
  4. `h04_a_crash_is_plain_with_a_warning_and_a_restart` (spec §11 #11): `pool`, `-j 1`,
     `--sort path`, `SFOR_EXAMPLE_CRASH=db`: `ini/db.ini`'s line prints as Plain, in the
     layout of spec §6.2 (`ini/db.ini:2: url = postgres://needle`, as M1 prints a Plain file;
     review r1, Codex #6), stderr has one line naming `ini` and
     `ini/db.ini`, `ini/web.ini` (after it) still has its Trail, the exit code is 0 and
     `starts(log) == 2`.
  5. `h05_three_failures_disable_the_definition`: six generated files, all crashing,
     `-j 1 --sort path`: stderr has three failure lines and one «failed 3 times» line, then
     nothing; all six print as Plain; exit 0.
  6. `h06_a_timeout_kills_and_goes_on`: `SFOR_EXAMPLE_HANG=web`, `timeout_ms = 300`: stderr
     names the timeout (`did not answer within 300 ms`), the run takes under 5 s, the other
     files have Trails.
  7. `h07_a_broken_line_is_a_failure`: `SFOR_EXAMPLE_GARBAGE=web`: stderr contains
     `not protocol JSON`; the other files have Trails.
  8. `h08_a_missing_command_is_one_warning`: `command = ['/no/such/helper']`: exactly one
     stderr line, containing `cannot start`; every file prints as Plain; exit 0.
  9. `h09_stdin_goes_through_a_temporary_file`: `sfor --format ini needle -` with
     `ini/app.ini` on stdin: the Trails of `app.ini`, and no `sfor-*` directory is left in the
     system's temporary directory afterwards (compare its listing before and after).
  10. `h10_a_bom_file_keeps_its_offsets`: `ini/bom.ini` gives the same Trails as `app.ini`.
  11. `h11_in_headings_asks_for_names`: `--in-headings '^host$' ini --flat --sort path`
      prints the `host` lines of `app.ini` and `bom.ini` (both hold it; review r1, Codex #17),
      and nothing from `db.ini` or `web.ini`.
  12. `h12_helper_stderr_is_relayed`: `SFOR_EXAMPLE_STDERR=hello from the helper`: stderr has
      `sfor: ini: hello from the helper`; with `--no-messages`, stderr is empty.
  13. `h13_helpers_exit_with_the_run` (Linux): after sfor exits, none of the pids in the log
      (`start <pid>`) exists under `/proc`.
  14. `h14_install_a_grammar_and_use_it` (spec §11 #17): build
      `workspace_bin("sfor-grammar", "sfor-grammar", &["lua"])`, copy it to `<tmp>/payload`,
      write `<tmp>/index.json` with one entry `lua` (version `0.2.0`, the Definition from
      `helpers/grammar/definitions/lua.toml`, the binary for `grammars::index::system()` with
      a `file://` URL and its SHA-256), set `SFOR_GRAMMAR_INDEX=file://<tmp>/index.json`;
      `sfor --install-grammar lua` exits 0; `--list-grammars` has a line `lua … installed
      0.2.0`; `--list-formats` shows `lua` as `installed (…)`; with
      `helpers/grammar/tests/fixtures/player.lua` copied into the fixture,
      `sfor needle player.lua --flat` prints `Player:update` and `helper` Trails.
  15. `h15_a_wrong_checksum_installs_nothing`: the same index with a wrong `sha256`: exit 2,
      stderr contains `SHA-256`, the Grammar directory has no `lua`.
  16. `h16_ctrl_c_ends_every_helper` (Linux, D32, D42; review r2, Codex #2): the Definition's
      command is `["sh", "-c", "trap '' INT; exec <example helper>"]`, so the helper ignores
      SIGINT; `SFOR_EXAMPLE_HANG=web` keeps a request in flight and `SFOR_EXAMPLE_LOG` records
      the pids. Two runs: (a) sfor in a new process group
      (`std::os::unix::process::CommandExt::process_group(0)`), SIGINT to the whole group as a
      terminal's Ctrl-C does (`kill -INT -- -<pgid>`); (b) SIGINT to sfor's pid alone. In both,
      once the log has a `start` line: within 2 s sfor's `ExitStatus::signal()` is
      `Some(SIGINT)` (`std::os::unix::process::ExitStatusExt`), and no pid of the log exists
      under `/proc` (killed and reaped, not left as a zombie re-parented to init). And (c) a run
      with no helper (a Markdown search on a FIFO that never ends) killed by SIGINT also ends
      by SIGINT: nothing was installed.
  17. `h17_a_helpers_child_cannot_hold_sfor` (Linux, D32): `SFOR_EXAMPLE_FORK=1`: the helper
      leaves a `sleep 30` holding its stdout and stderr; sfor still finishes in under 4 s, with
      its normal output.
  18. `h18_streams_named_on_the_command_line_go_through_a_temporary_file` (Linux, D8, Codex
      r1 #4, Fable r2 #1): `sfor --format ini needle /dev/stdin` three ways — `app.ini` through a
      pipe on stdin, `app.ini` redirected as a file (`Stdio::from(File::open("ini/app.ini"))`,
      where `/dev/stdin` is a regular file), and `/dev/fd/0` redirected the same way — and
      `sfor --format ini needle <fifo>` with a writer thread filling a `mkfifo`; then, with the
      command's working directory set to `/dev`, `sfor --format ini needle stdin` with
      `app.ini` redirected as a file and through a pipe; and a symlink `ini/link.ini →
      /dev/stdin` searched as `sfor needle ini/link.ini`, with `app.ini` redirected as a file and
      through a pipe (review r3, Codex #2): every run gives `app.ini`'s Trails, no timeout
      warning, in under 4 s.
  19. `h19_a_failing_helper_is_plain_like_rg` (D33, Codex #6): with `SFOR_EXAMPLE_CRASH=long`, a
      file `long.ini` with a 5 000-character line holding `needle` prints exactly what a `.txt`
      copy of it prints (Fragments included), and `--json` gives `"format": null` for it.
- [ ] **Step 2:** GREEN: `cargo test --test helpers`, then 10 runs in a row
  (`for i in $(seq 10); do cargo test -q --test helpers || break; done`) to catch races.
  Commit «Test Helper Processors and Grammar installs end to end».
  `h14`–`h15` run with the index in a temporary directory and never touch the network.

### Task 11.3: the code benchmark — scripts/bench-code.sh

**Files:** `scripts/bench-code.sh`.

```bash
#!/usr/bin/env bash
# M2 acceptance (spec §11 #13, plan D22), on code corpora:
#   - a pattern no file matches, and --no-trail: sfor ≤ 1.1 × rg;
#   - a pattern matching under 5 % of the files: sfor − rg ≤ matching bytes ÷ (2.5 MB/s × threads);
#   - a dense pattern: reported only.
# Usage: scripts/bench-code.sh [DIR_A [SPARSE_A [DIR_B [SPARSE_B]]]]
# Needs hyperfine, jq and GNU stat. SFOR_BIN replaces the sfor under test (to check that a
# broken one fails the script: SFOR_BIN=scripts/fake-sfor-exit-2.sh must make it exit 1).
set -euo pipefail
registry=$(ls -d "$HOME"/.cargo/registry/src/index.crates.io-*/ 2>/dev/null | head -1)
dir_a="${1:-$registry}"; sparse_a="${2:-impl Drop for}"
dir_b="${3:-$HOME/robotin/apps}"; sparse_b="${4:-TODO}"
none='zzqq-searchfor-bench-none'
cargo build --release --quiet
sfor="${SFOR_BIN:-./target/release/sfor}"
export SFOR_BIN_PATH="$sfor"
# Isolation (review r1, Codex #12): no user Definitions or Grammars, no network, no rg config.
iso=$(mktemp -d)
trap 'rm -rf "$iso"' EXIT
mkdir -p "$iso/formats" "$iso/grammars"
export SFOR_FORMATS_DIR="$iso/formats" SFOR_GRAMMARS_DIR="$iso/grammars" SFOR_GRAMMAR_INDEX="file://$iso/none.json"
unset RIPGREP_CONFIG_PATH
# The code Formats of M2, for the parsing budget (Fable #7): rg's types for them.
code_types=(-t php -t js -t ts -t sh -t go -t rust -t py -t json -t yaml -t toml)
# ripgrep 14.1.1's default: the available parallelism, at most 12 (search.rs, walker_threads).
threads=$(nproc); [ "$threads" -gt 12 ] && threads=12
fail=0

# median NAME CMD_RG CMD_SFOR → prints sfor/rg and sets RG_S, SFOR_S (seconds).
median() {
  hyperfine --ignore-failure --warmup 3 --runs 20 --export-json "/tmp/sfor-bench-code-$1.json" "$2" "$3" >/dev/null
  RG_S=$(jq '.results[0].median' "/tmp/sfor-bench-code-$1.json")
  SFOR_S=$(jq '.results[1].median' "/tmp/sfor-bench-code-$1.json")
  awk -v a="$SFOR_S" -v b="$RG_S" -v n="$1" 'BEGIN { printf "%-22s rg %.3f s  sfor %.3f s  ratio %.2f\n", n, b, a, a / b }'
}

# check NAME EXPECTED_CODE SFOR_FLAG RG_ARGS…: sfor must exit EXPECTED_CODE, write to stderr
# only what rg writes (an unreadable file warns in both; any other line is a fallback
# warning and fails — review r2, Fable #6), and print at least as many lines as rg (`--flat`
# prints a line once per Trail, so a table row can print twice; `--no-trail` exactly rg's).
check() {
  local name="$1" want="$2" flag="$3"; shift 3
  local code=0 rgcode=0 err rgerr
  err=$("$sfor" --color never "$flag" "$@" 2>&1 >/tmp/sfor-bench-check.out | sed 's/^sfor: //' | sort) || true
  "$sfor" --color never "$flag" "$@" >/dev/null 2>&1 || code=$?
  rgerr=$( (rg --color never "$@" 2>&1 >/dev/null || true) | sed 's/^rg: //' | sort)
  rg --color never "$@" >/dev/null 2>&1 || rgcode=$?
  # sfor exits as rg does (2 for both when a file cannot be read is fine), and the pattern
  # behaves as the benchmark assumes: rg itself found (sparse) or did not find (none) Matches,
  # which `-q` below checks apart from the errors (review r3, Codex #4).
  if [ "$code" != "$rgcode" ] || [ "$err" != "$rgerr" ]; then
    echo "  FAIL $name: sfor exited $code, rg $rgcode; or stderr differs from rg's: ${err:0:300}"; fail=1; return
  fi
  local found=0
  rg -q --no-messages "$@" || found=$?
  if { [ "$want" = 0 ] && [ "$found" = 1 ]; } || { [ "$want" = 1 ] && [ "$found" = 0 ]; }; then
    echo "  FAIL $name: the pattern does not behave as the benchmark assumes"; fail=1; return
  fi
  local ours rgs
  ours=$(wc -l </tmp/sfor-bench-check.out)
  rgs=$( (rg --color never "$@" 2>/dev/null || true) | wc -l)
  if [ "$ours" -lt "$rgs" ] || { [ "$flag" = --no-trail ] && [ "$ours" != "$rgs" ]; }; then
    echo "  FAIL $name: $ours lines, rg printed $rgs"; fail=1
  fi
}

corpus() {
  local name="$1" dir="$2" sparse="$3"
  export SFOR_BENCH_DIR="$dir" SFOR_BENCH_PATTERN="$sparse" SFOR_BENCH_NONE="$none"
  local total hits bytes
  total=$(rg --files -- "$dir" | wc -l)
  hits=$( (rg -l -e "$sparse" -- "$dir" || true) | wc -l)
  bytes=$( (rg -l -0 "${code_types[@]}" -e "$sparse" -- "$dir" || true) | xargs -0 -r stat -c %s | awk '{ s += $1 } END { print s + 0 }')
  echo "== $name: $dir, files $total, matching $hits ($bytes bytes of code and data), threads $threads"
  awk -v h="$hits" -v t="$total" 'BEGIN { if (h * 20 >= t) { print "the sparse pattern matches 5% or more of the files"; exit 1 } }'
  if rg -q -e "$none" -- "$dir"; then echo "  FAIL: '$none' matches in $dir"; fail=1; return; fi
  # Before timing: every command does what it is timed for.
  check none 1 --flat -e "$none" -- "$dir"
  check sparse 0 --flat -e "$sparse" -- "$dir"
  check no-trail 0 --no-trail -e "$sparse" -- "$dir"
  [ "$fail" = 0 ] || return
  median "$name-none" 'rg --color never -e "$SFOR_BENCH_NONE" -- "$SFOR_BENCH_DIR"' \
    '"$SFOR_BIN_PATH" --color never --flat -e "$SFOR_BENCH_NONE" -- "$SFOR_BENCH_DIR"'
  awk -v a="$SFOR_S" -v b="$RG_S" 'BEGIN { exit !(a <= 1.1 * b) }' || { echo "  FAIL: over 1.1 × rg"; fail=1; }
  median "$name-no-trail" 'rg --color never -e "$SFOR_BENCH_PATTERN" -- "$SFOR_BENCH_DIR"' \
    '"$SFOR_BIN_PATH" --color never --no-trail -e "$SFOR_BENCH_PATTERN" -- "$SFOR_BENCH_DIR"'
  awk -v a="$SFOR_S" -v b="$RG_S" 'BEGIN { exit !(a <= 1.1 * b) }' || { echo "  FAIL: over 1.1 × rg"; fail=1; }
  median "$name-sparse" 'rg --color never -e "$SFOR_BENCH_PATTERN" -- "$SFOR_BENCH_DIR"' \
    '"$SFOR_BIN_PATH" --color never --flat -e "$SFOR_BENCH_PATTERN" -- "$SFOR_BENCH_DIR"'
  awk -v a="$SFOR_S" -v b="$RG_S" -v by="$bytes" -v t="$threads" \
    'BEGIN { budget = by / (2.5e6 * t); printf "  extra %.3f s, budget %.3f s\n", a - b, budget; exit !(a - b <= budget) }' \
    || { echo "  FAIL: over the parsing budget"; fail=1; }
}

corpus registry "$dir_a" "$sparse_a"
corpus apps "$dir_b" "$sparse_b"
export SFOR_BENCH_DIR="$dir_a" SFOR_BENCH_PATTERN='fn new\('
median registry-dense 'rg --color never -e "$SFOR_BENCH_PATTERN" -- "$SFOR_BENCH_DIR"' \
  '"$SFOR_BIN_PATH" --color never --flat -e "$SFOR_BENCH_PATTERN" -- "$SFOR_BENCH_DIR"'
exit "$fail"
```

  `--ignore-failure` stays in `median` only because rg exits 1 for the «none» pattern; `check`
  has already required the exact exit codes, an empty stderr (no fallback warning) and rg's
  line count, so a run that fails cannot pass on time alone. `scripts/fake-sfor-exit-2.sh`
  (`#!/bin/sh` + `exit 2`) is committed with the script for that self-check.

- [ ] Run `SFOR_BIN=scripts/fake-sfor-exit-2.sh scripts/bench-code.sh` once and see it exit 1
  (RED for the script itself). Then run it for real, and `scripts/bench.sh` (M1's, which must
  still pass). M2 is not done until both
  exit 0. If one fails, profile (`perf record` / `cargo flamegraph`) and fix the top cost; a
  ceiling changes only with Sebastián's OK. Report every line both scripts print. Commit «Add
  the code benchmark».

### Task 11.4: ceilings in tests/perf.rs

**Files:** `tests/perf.rs` (ignored tests, run with `cargo test --release --test perf --
--ignored --nocapture`, as M1's).

- [ ] `keypath_json_scans_at_fifty_megabytes_a_second`: a 50 MB minified JSON array of objects
  with three `needle`s: sfor's wall time minus rg's is at most 1 s (50 MB/s), and sfor's peak
  RSS is at most rg's plus 32 MiB (the collector keeps three stacks, not the keys).
- [ ] `treesitter_skips_files_over_max_bytes`: a 20 MB generated JavaScript file with one
  `needle`: one warning naming `max_bytes`, Plain output, and at most 2 × rg's wall time.
- [ ] `broken_toml_recovers_in_bounded_time`: a 1 MB TOML shaped like a Cargo.lock with a
  `"""` left open 300 lines before its end and a `needle` before it: the `needle`'s Trail
  survives, and sfor's wall time is at most rg's plus 2 s (review r2, Fable #4).
- [ ] `yaml_beyond_the_tree_sitter_limit`: a 40 000-line YAML with a `needle` at the end gets
  its Trail end to end, in at most rg's time plus 1 s.
- [ ] Commit «Add the M2 performance ceilings».

---

## Chunk 12: README, skill and spec

**Files:** `README.md`, `README.es.md`, `skills/searchfor/SKILL.md`,
`docs/specs/2026-10-16-searchfor-design.md`, `docs/json-schema.json`.

1. **README.md / README.es.md** (English primary, Spanish translation): «Status» says v0.2;
   «Formats» lists Markdown, JSON, YAML, TOML, PHP, JavaScript, TypeScript, TSX, Bash, Go,
   Rust and Python, with one real example each for YAML (`server › ports[2] › host`) and PHP
   (tree with signatures) copied from C10's snapshots; a section «Helper Processors» (what they
   are, the three modes, a pointer to spec §4.4 and to `helpers/example/` as the reference
   implementation); a section «Grammars» (`--list-grammars`, `--install-grammar`, and that the
   index stays empty until v1.0 publishes the binaries); the roadmap row of M2 rewritten
   without WebAssembly (today: «tree-sitter Grammars as WebAssembly»); «For AI agents» gains
   the code recipes `--within "Cart › addItem"` and `--expand` on a function.
2. **skills/searchfor/SKILL.md**: the description covers code and configuration files («Use
   when searching Markdown, code or YAML/JSON/TOML and you need to know which section,
   function or key each match belongs to — before running rg and opening whole files»); the
   recipes add code and key paths; the «only Markdown has Trails» line goes.
3. **Spec**: bring it in line with what was built: §4.4's failure rules match C4's host
   (D41, the three failures, D32's close); §5.2 and §5.3 match the fixtures' real outputs; §11
   #13 quotes the benchmark's numbers; mark «M2 plan» decisions that changed during
   implementation, with the reason.
4. **docs/json-schema.json**: Segment `kind` stays a free string (new kinds: `key`, `element`,
   `class`, `method`, `function`, …); check that the schema does not enumerate kinds, and that
   `signature` is allowed; change nothing else.

- [ ] Commit «Document v0.2: code, key paths and Helper Processors».

---

## After the plan: closing M2 (ciclo stages 8–10, with /cpd as in 8375)

- Stage 8: `cargo install --path .`, copy `skills/searchfor/` to `~/.claude/skills/searchfor/`,
  demo in the Hilo with real output over `~/robotin/apps` (one PHP or JS Trail, one YAML key
  path), the benchmark lines of `scripts/bench-code.sh` and `scripts/bench.sh`, the binary
  size of `target/release/sfor`, and the RSS values of the two memory tests; `/cpd`: merge to
  `main`, push.
- Stage 9: code review rounds; then `/cpd` again.
- Stage 10: bump to `0.2.0`, `cargo publish -p searchfor` (the workspace members have
  `publish = false`), tag `v0.2.0`, push the tag.
