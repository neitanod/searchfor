# searchfor M1 (v0.1: core + Markdown) Implementation Plan

> **For agentic workers:** each chunk below is implemented by one subagent in its own git
> worktree, following `test-driven-development`: every task writes its tests first, runs them
> and sees them fail (RED), implements, and runs them green (GREEN). Steps use checkbox
> (`- [ ]`) syntax. Read `CONTEXT.md` (vocabulary) and
> `docs/specs/2026-10-16-searchfor-design.md` (the spec) before starting; the spec wins over this
> plan if they disagree, and the disagreement is reported. Everything is written in English.

**Goal:** a working `sfor` v0.1, published on crates.io, that searches like ripgrep and prints
the Trail of every Match in Markdown files, in tree, one-line and JSON output, with `--within`,
`--expand`, `--in-headings`, Fragments for long lines and user-overridable Format Definitions.

**Architecture:** every file becomes a **Source**: its bytes, read once (memory-mapped), with
the BOM removed and UTF-16 transcoded, so search, Processor and printer share one coordinate
system. The Detector runs on the Source before the search. The search uses ripgrep's crates
(`ignore`, `grep-searcher`, `grep-regex`) with our own `Sink`, which builds the model
(`model.rs`) and the per-file stats. The Processor of the file's Format turns the Match anchors
into Trails. Filters run on the model; printers read it. `--no-trail` (and the summary flags
without structural filters) bypass all of it and hand the search to `grep-printer`, which is
ripgrep's own output code.

**Tech stack:** Rust 1.90 (edition 2024), clap 4.6, ignore 0.4.33, grep-searcher 0.1.17,
grep-regex 0.1.14, grep-matcher 0.1.9, grep-printer 0.3.1, termcolor 1.4, pulldown-cmark 0.13,
serde + serde_json (feature `preserve_order`), toml 1.1, globset 0.4, memmap2 0.9,
encoding_rs 0.8, terminal_size 0.4, dirs 6, anyhow, thiserror. Tests: insta, assert_cmd,
tempfile, jsonschema, regex-lite.

**Reference ripgrep:** 14.1.1 — the version whose crates we pin. Tests that compare with `rg`
skip themselves, with a printed note, when `rg --version` is not 14.1.1, and always run `rg`
with `RIPGREP_CONFIG_PATH` removed from the environment.

**Branch:** work branch `t-768-searchfor` (base `main`), worktree
`/home/sebas/robotin/apps/mdgrep/.workspace/t-768-mdgrep/repo`. Each chunk branches
`t-768-searchfor-c<N>` from the work branch, as `ciclo` stage 7 describes.

---

## Why M1 and not the whole v1

The spec's v1 has ten Format families, WebAssembly Grammars, External Processors, an MCP
server, every ripgrep flag and four distribution channels. One plan for all of it would be
several thousand lines nobody can review in one sitting, written against interfaces nobody has
run. The v1 is split into milestones, each one a release with its own plan, review rounds and
implementation inside the same Task:

| Milestone | Release | Content |
|---|---|---|
| **M1 (this plan)** | v0.1 | Source, Detector, Format Definitions, Processor interface, Markdown, search with the core ripgrep flags, tree / one-line / JSON output, Fragments, `--within`, `--expand`, `--in-headings`, Claude Code skill, crates.io |
| M2 | v0.2 | tree-sitter with WebAssembly Grammars, `keypath` Processor (YAML, JSON, TOML), `treesitter` Processor (PHP, JS, TS, Bash, Go, Rust, Python), `--install-grammar`, `--list-grammars` |
| M3 | v0.3 | HTML, XML, CSV, subtitles (.srt, .vtt; 8380), and the extra text Formats chosen in Questionnaire 49 |
| M4 | v0.4 | Object files, Containers with `-z`, PDF and SQLite (8380), and the extra binary and office Formats chosen in Questionnaire 49 |
| M5 | v1.0 | External Processors, MCP server, the rest of ripgrep's flags, `dist` (GitHub Releases, Homebrew tap, npm), Windows and macOS in CI |

The interfaces M2–M5 need exist from M1: the Processor contract takes anchors and returns
Trails (External Processors in M5 implement the same contract over a protocol); the Detector
runs before the search and picks the search policy (object files and Containers in M4 need
that); a Source can be built from bytes in memory (Container Members in M4); table cells are
ordinary Segments (CSV in M3 emits them the same way); Segments carry `signature` (code in M2).

## Grill decisions this plan implements

| # | Decision | Where in M1 |
|---|---|---|
| 1 | Markdown in v1 (the other Formats in M2–M4) | C5 |
| 2 | ripgrep's crates in-process | C3 |
| 3 | Full ripgrep flag compatibility as the target; unsupported flags fail loudly | C2, C3 |
| 4 | `--within`, `--expand`, `--in-headings` | C8, C9 |
| 5 | Detector: extension, exact name, shebang, magic bytes, `--format` | C4 |
| 6 | Embedded TOML Definitions + user directory + `--export-formats` | C4 |
| 9 | The chain is called `trail` | C1 |
| 11 | Markdown Trail: headings, fences, table column, frontmatter title, parent list items | C5 |
| 13 | Fragment centred on the Match, sized to the terminal, with omitted counts | C6 |
| 14 | Nearby Matches share a Fragment, distant ones get their own | C6 |
| 15 + 8365 | `--json` = ripgrep's JSON Lines + `trail`, `format`, `fragment` in each `match` | C7 |
| 16, 17 | Repo at `/home/sebas/robotin/apps/searchfor`, public, MIT OR Apache-2.0 | done (stage 1) |
| 19 | Claude Code skill and README section for agents | C10 |
| 20 | crates.io when v0.1 works | M1 close (stage 10) |
| 21 | `/ciclo` | this document |
| 8375 | Everything in English; keep going through M5; Windows and macOS at the end | all chunks; `termcolor` buffers in C7 |
| 8381 | Coloured console output | C7 (ripgrep's colours + cyan Trails, `--color`) |

Decisions 7, 8, 10, 12, 18 belong to M2–M5.

## Design notes the chunks share

1. **One Source per file.** `source::Source` holds the bytes of a file, read once: memory-mapped
   for regular files, read into memory for stdin and (in M4) Members. A UTF-8 BOM is removed and
   UTF-16 (LE/BE, detected by BOM) is transcoded to UTF-8 with `encoding_rs`, exactly as
   ripgrep's decoder does, so the bytes we search are the bytes the Processor parses and the
   printer shows; a searcher over a Source runs with `bom_sniffing(false)`, in memory or with a
   reader exactly when ripgrep would (C3, `rg_in_memory`). Offsets in our output are offsets in
   those decoded bytes, which is also what `rg --json` reports. Passthrough and the quiet fast
   path search raw files and stdin, with BOM sniffing on.
2. **Detector before search.** The Detector reads the start of the Source. In M1 its result only
   decides which Processor runs; from M4 on, a Format may also change the search policy
   (search object files whole, open Containers).
3. **Binary policy as ripgrep.** Files named on the command line and stdin are searched with
   `BinaryDetection::convert(b'\0')` (also every file under `--binary` and `-uuu`), files found
   by walking with `quit(b'\0')`, and `-a` uses `none()`. `CollectSink` records, in
   `Sink::binary_data`, how many items it had when the NUL was found (`binary_cut`). Tree and
   flat print the items before the cut and then ripgrep's message: `binary file matches (found
   "\0" byte around offset N)` for convert, `WARNING: stopped searching binary file after
   match (found "\0" byte around offset N)` for quit (only when something was printed before),
   preceded by `path: ` when file names are shown (grep-printer `standard.rs:823-838`,
   `1395-1420`). JSON and the summary modes use every item, as ripgrep.
4. **Our own Sink.** `search::CollectSink` implements `grep_searcher::Sink`. For each matching
   line it records the line, its absolute offset, its line number and its submatches (found with
   `matcher.find_iter` on the line, none for `-v`); for each context line, the line; and per file
   the stats ripgrep reports (`bytes_searched`, `matched_lines`, `matches`, binary offset).
   This replaces parsing grep-printer's JSON: filters, printers and JSON all read one model.
5. **Trails per submatch.** A line can hold several Matches with different Trails (two cells of
   one table row). `Described.trails[i][k]` is the Trail of submatch `k` of Match item `i`; an
   inverted Match (`-v`) has one Trail, anchored at the line start. `--within` and
   `--in-headings` decide per submatch; a line is kept when any submatch is kept, and only the
   kept submatches are highlighted and reported.
6. **Processor contract.** `Processor::trails(source, params, anchors) -> Vec<Trail>` (one
   Trail per anchor byte) and `Processor::names(source, params) -> Vec<Name>` (every Segment name
   with the byte range of its text, for `--in-headings`). Built-in Processors implement both on
   top of an **Outline** (`trail.rs`); M5's External Processors implement them over the
   protocol of spec §4.4.
7. **Outline.** A Processor returns every Segment with its byte range (the Section), its label
   range (bytes that name it: a heading line, an item's first line), an optional name range
   (the name's text, for `--in-headings`), and lets `Outline::build` sort them and assign
   parents by containment. `trail_for(outline, byte)` picks the innermost Segment containing
   `byte`, walks the parents, and leaves out hidden Segments and any Segment whose label
   contains `byte` — a Match on a heading line is not «inside» that heading.
8. **Table cells are Segments.** Each cell of a Markdown table is a Segment of kind `column`,
   named by its header cell, with the cell's byte range; its `line`/`end_line` are the table's,
   so the Section of a column is its table. The table itself is a hidden Segment. CSV (M3)
   emits cells the same way, and only around the anchors it is given.
9. **`--no-trail` is ripgrep.** `--no-trail`, and `-c`, `--count-matches`, `-l`,
   `--files-without-match` when no structural flag is given, run `grep_printer::Standard`,
   `Summary` or `JSON` directly with ripgrep's defaults (line numbers and headings on a TTY,
   file names when more than one file or any directory is searched), so the output is
   ripgrep's own. With structural flags, the summary flags are computed from the filtered model
   by our printers. `--expand` with a summary flag is a usage error.
10. **Exit codes are ripgrep's:** 0 if at least one file had a kept Match after filters (whether
    its lines or only ripgrep's binary message were printed), 1 if none,
    2 if an error happened — except that `-q` with a Match returns 0 even after an error. `-q`
    prints nothing and stops at the first kept Match.
11. **Parallelism.** As ripgrep: `WalkBuilder::build_parallel`, one searcher per thread; each
    thread renders a whole file into a `termcolor::Buffer` (which handles Windows consoles) and
    sends it to one writer, so a file's output is never interleaved. `--sort path` and `-j1`
    use the sequential walker.
12. **Management actions are flags** (`--list-formats`, `--export-formats`), never subcommands,
    so `sfor formats` keeps searching for the word «formats».

## File structure

```
Cargo.toml                         all dependencies (only C1 edits it)
formats/markdown.toml              built-in Format Definition (embedded)
src/main.rs                        fn main → searchfor::run → exit code
src/lib.rs                         module tree, pub fn run
src/model.rs                       Segment, Trail, Submatch, LineMatch, ContextLine, Item, FileMatches, FileStats, Described
src/text.rs                        lossy UTF-8 with an offset map
src/trail.rs                       LineIndex, Outline, trail_for, names_of
src/cli/mod.rs                     clap Args, resolution of overriding flags, parse
src/cli/rg_flags.rs                ripgrep's flags with their M1 status and arity
src/source.rs                      Source: read once, BOM, UTF-16
src/search.rs                      matcher, searcher, walker, CollectSink
src/formats/{mod,definition,detector,builtin}.rs
src/processors/{mod,markdown}.rs
src/filter.rs                      --within, --in-headings (per submatch), retain, sections
src/output/{mod,fragment,style,tree,flat,json,summary}.rs
src/app.rs                         pipeline
tests/fixtures/md/, tests/fixtures/misc/
tests/cli.rs, tests/rg_compat.rs
docs/rg-compat.md, docs/json-schema.json
skills/searchfor/SKILL.md
scripts/bench.sh
```

## Chunk graph

| Chunk | Content | Depends on | Files (exclusive within its tanda) | Tanda |
|---|---|---|---|---|
| C1 | Cargo project, module tree, `model.rs`, `text.rs`, `trail.rs` | — | `Cargo.toml`, `src/main.rs`, `src/lib.rs`, `src/model.rs`, `src/text.rs`, `src/trail.rs`, stub files | 1 |
| C2 | CLI and ripgrep flag table | C1 | `src/cli/**`, `docs/rg-compat.md` | 2 |
| C4 | Format Definitions, registry, Detector | C1 | `src/formats/**`, `formats/markdown.toml` | 2 |
| C5 | Processor contract and Markdown | C1 | `src/processors/**`, `tests/fixtures/md/**` | 2 |
| C6 | Fragments | C1 | `src/output/fragment.rs` | 2 |
| C3 | Source and search | C1, C2 | `src/source.rs`, `src/search.rs`, `tests/fixtures/misc/**` | 3 |
| C7 | Printers | C1, C6 | `src/output/{mod,style,tree,flat,json,summary}.rs` | 3 |
| C8 | Filters | C1 | `src/filter.rs` | 3 |
| C9 | Pipeline, end-to-end and compat tests, benchmark | C2–C8 | `src/app.rs`, `tests/cli.rs`, `tests/rg_compat.rs`, `scripts/bench.sh`, `tests/fixtures/md/twice.md`, the `-v`/`--in-headings` check in `src/cli/mod.rs` | 4 |
| C10 | Docs, JSON schema, skill, spec and glossary updates | C9 | `README.md`, `README.es.md`, `docs/json-schema.json`, `skills/searchfor/SKILL.md`, `docs/specs/2026-10-16-searchfor-design.md`, `CONTEXT.md`, `tests/cli.rs` (one `#[ignore]`) | 5 |

C1 writes every `mod` declaration, every dependency and an empty stub for every module file, so
later chunks only fill their own files. A chunk uses another chunk's API only through the
signatures fixed in this plan; C9 is where everything links, and it fixes mismatches. After each
tanda: `cargo build`, `cargo clippy --all-targets -- -D warnings`, `cargo test`.

**Snapshots.** Snapshot tests use insta. New snapshots are written with
`INSTA_UPDATE=always cargo test`, then read in full and checked by eye (`git diff` on the
`.snap` files) before committing; `cargo insta` is not required. The chunk report pastes one
snapshot per printer.

---

## Chunk 1: Project, model, text and Trails

### Task 1.1: Cargo project and module tree

**Files:** `Cargo.toml`, `src/main.rs`, `src/lib.rs`, and a stub for every module of «File
structure».

- [ ] **Step 1: Cargo.toml** — the only time any chunk edits it.

```toml
[package]
name = "searchfor"
version = "0.1.0-dev"
edition = "2024"
rust-version = "1.90"
description = "ripgrep-compatible search that shows where each match lives: headings, functions, key paths"
license = "MIT OR Apache-2.0"
repository = "https://github.com/neitanod/searchfor"
readme = "README.md"
keywords = ["grep", "search", "markdown", "ripgrep", "cli"]
categories = ["command-line-utilities", "text-processing"]

[[bin]]
name = "sfor"
path = "src/main.rs"

[dependencies]
anyhow = "1"
clap = { version = "4.6", features = ["derive", "string", "wrap_help"] }
dirs = "6"
encoding_rs = "0.8"
globset = "0.4"
grep-matcher = "0.1.9"
grep-printer = "0.3.1"
grep-regex = "0.1.14"
grep-searcher = "0.1.17"
ignore = "0.4.33"
memchr = "2"
memmap2 = "0.9"
pulldown-cmark = { version = "0.13", default-features = false }
serde = { version = "1", features = ["derive"] }
serde_json = { version = "1", features = ["preserve_order"] }
termcolor = "1.4"
terminal_size = "0.4"
thiserror = "2"
toml = "1.1"

[dev-dependencies]
assert_cmd = "2"
insta = { version = "1", features = ["json"] }
jsonschema = "0.58"
regex-lite = "0.1"
tempfile = "3"

[profile.release]
lto = "thin"
codegen-units = 1
```

`cargo update` must resolve every line; versions checked on crates.io on 2026-10-16 (toml
1.1.8, memmap2 0.9.11, encoding_rs 0.8.42, jsonschema 0.58.6, regex-lite 0.1.9). If one does
not resolve, take the latest compatible and say so in the report.

- [ ] **Step 2: lib.rs**

```rust
//! searchfor: ripgrep-compatible search that reports the Trail of every Match.
pub mod app;
pub mod cli;
pub mod filter;
pub mod formats;
pub mod model;
pub mod output;
pub mod processors;
pub mod search;
pub mod source;
pub mod text;
pub mod trail;

/// Entry point used by `main`. Returns the process exit code (0, 1 or 2, as ripgrep).
pub fn run(args: Vec<std::ffi::OsString>) -> i32 {
    app::run(args)
}
```

`src/main.rs`:

```rust
fn main() {
    std::process::exit(searchfor::run(std::env::args_os().collect()));
}
```

Stubs: `src/app.rs` holds `pub fn run(_args: Vec<std::ffi::OsString>) -> i32 { 2 }`;
`src/cli/mod.rs` holds `pub mod rg_flags;`; `src/formats/mod.rs` holds
`pub mod builtin; pub mod definition; pub mod detector;`; `src/processors/mod.rs` holds
`pub mod markdown;`; `src/output/mod.rs` holds
`pub mod flat; pub mod fragment; pub mod json; pub mod style; pub mod summary; pub mod tree;`.
Every other stub is a file with one `//!` line naming the chunk that fills it.

- [ ] **Step 3:** `cargo build` compiles (dead-code warnings allowed). Commit «Set up the crate
  and its module tree» with `git add -- Cargo.toml Cargo.lock src` first.

### Task 1.2: model.rs

**Files:** `src/model.rs`. These types are the contract between chunks.

```rust
//! Types shared by every stage of the pipeline.
use serde::Serialize;
use std::ops::Range;
use std::path::PathBuf;
use std::time::Duration;

/// One step of a Trail (see CONTEXT.md).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Segment {
    /// "document", "heading", "code", "item", "column"; later milestones add kinds.
    pub kind: String,
    pub name: String,
    /// 1-based first line of the Segment's Section.
    pub line: u64,
    /// 1-based last line of the Section (inclusive).
    pub end_line: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

pub type Trail = Vec<Segment>;

/// Byte range of a submatch, relative to the start of its line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Submatch {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineMatch {
    pub line_number: u64,
    /// Absolute byte offset of the start of the line in the Source.
    pub absolute_offset: u64,
    /// The line, including its terminator if it had one.
    pub bytes: Vec<u8>,
    /// Empty for an inverted search (`-v`).
    pub submatches: Vec<Submatch>,
}

impl LineMatch {
    /// One anchor per submatch (absolute byte of its start); the line start when there is none.
    pub fn anchors(&self) -> Vec<usize> {
        let base = self.absolute_offset as usize;
        if self.submatches.is_empty() {
            vec![base]
        } else {
            self.submatches.iter().map(|s| base + s.start).collect()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextLine {
    pub line_number: u64,
    pub absolute_offset: u64,
    pub bytes: Vec<u8>,
    /// Non-empty only with `-v`, where context lines are the lines that do match; ripgrep's
    /// JSON reports their submatches.
    pub submatches: Vec<Submatch>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    Match(LineMatch),
    Context(ContextLine),
}

/// What ripgrep reports per file in its JSON `end` message.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileStats {
    pub bytes_searched: u64,
    pub matched_lines: u64,
    pub matches: u64,
    pub binary_offset: Option<u64>,
    /// Items recorded before the binary data was detected (design note 3).
    pub binary_cut: Option<usize>,
    pub elapsed: Duration,
}

/// Everything the searcher found in one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileMatches {
    /// The path as it must be printed (relative as given, or `<stdin>`).
    pub path: PathBuf,
    /// True when the path was named on the command line (binary policy, design note 3).
    pub explicit: bool,
    pub items: Vec<Item>,
    pub stats: FileStats,
}

impl FileMatches {
    pub fn matches(&self) -> impl Iterator<Item = &LineMatch> {
        self.items.iter().filter_map(|i| match i {
            Item::Match(m) => Some(m),
            Item::Context(_) => None,
        })
    }
}

/// A file after its Processor and the filters ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Described {
    pub file: FileMatches,
    /// Name of the Format, or None for Plain.
    pub format: Option<String>,
    /// `trails[i][k]`: Trail of submatch `k` of the `i`-th `Item::Match` (design note 5).
    pub trails: Vec<Vec<Trail>>,
    /// With `--expand`, `sections[i][k]`: the Section of submatch `k` of Match `i`
    /// (1-based inclusive lines), aligned with `trails`.
    pub sections: Vec<Vec<Option<(u64, u64)>>>,
}

/// What a Processor receives about one file (design note 6). External Processors (M5) need
/// the path and the Format name; built-in ones mostly use the bytes.
#[derive(Debug, Clone, Copy)]
pub struct Input<'a> {
    pub bytes: &'a [u8],
    pub path: &'a std::path::Path,
    pub format: &'a str,
}

/// A Segment name, for `--in-headings` (design note 6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Name {
    /// The name as shown: text pieces joined, a soft line break becoming one space.
    pub text: String,
    /// Where each piece of `text` comes from: `(range in text, range in the Source)`. The two
    /// ranges have the same length when the text is verbatim, and differ when the Processor
    /// decoded it (an entity like `&lt;` is 1 byte of text over 4 of Source). A setext heading
    /// spanning two lines has one piece per line.
    pub pieces: Vec<(Range<usize>, Range<usize>)>,
    /// Trail of the named Segment, the Segment itself last.
    pub trail: Trail,
    /// Where an empty hit lands when no piece contains it (a name with no text, like `###`):
    /// the end of the heading's markers.
    pub anchor: usize,
}

impl Name {
    /// Source ranges covered by the text range `hit`, one per piece it touches. A verbatim
    /// piece maps byte for byte; a decoded piece maps to its whole Source range. An empty hit
    /// (`^`, `$`) at text position `p` maps to an empty range: inside a verbatim piece at the
    /// corresponding byte; at the start of a decoded piece to its Source start, at its end to its
    /// Source end; with no piece (an empty name) at `anchor`.
    pub fn source_ranges(&self, hit: Range<usize>) -> Vec<Range<usize>> {
        if hit.is_empty() {
            let p = hit.start;
            let at = self.pieces.iter().find(|(t, _)| t.start <= p && p <= t.end).map(|(t, src)| {
                if t.len() == src.len() { src.start + (p - t.start) }
                else if p == t.end && !t.is_empty() { src.end } else { src.start }
            }).unwrap_or(self.anchor);
            return vec![at..at];
        }
        self.pieces.iter().filter_map(|(t, src)| {
            let (s, e) = (hit.start.max(t.start), hit.end.min(t.end));
            (s < e).then(|| if t.len() == src.len() { src.start + (s - t.start)..src.start + (e - t.start) } else { src.clone() })
        }).collect()
    }
}
```

- [ ] **Step 1: tests first** (in `src/model.rs`):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchors_per_submatch_or_line_start() {
        let m = LineMatch {
            line_number: 3,
            absolute_offset: 100,
            bytes: b"a needle b needle\n".to_vec(),
            submatches: vec![Submatch { start: 2, end: 8 }, Submatch { start: 11, end: 17 }],
        };
        assert_eq!(m.anchors(), vec![102, 111]);
        let inverted = LineMatch { submatches: vec![], ..m };
        assert_eq!(inverted.anchors(), vec![100]);
    }

    #[test]
    fn segment_json_skips_empty_options() {
        let s = Segment { kind: "heading".into(), name: "Install".into(), line: 1, end_line: 9, level: Some(1), signature: None };
        assert_eq!(
            serde_json::to_string(&s).unwrap(),
            r#"{"kind":"heading","name":"Install","line":1,"end_line":9,"level":1}"#
        );
    }
}
```

- [ ] **Step 2:** `cargo test model` fails to compile (RED), then passes after the types (GREEN).
  Commit «Add the shared model».

### Task 1.3: text.rs — lossy UTF-8 with an offset map

**Files:** `src/text.rs`. Printers show lines that may contain invalid UTF-8; submatch offsets
are byte offsets in the original line, and `String::from_utf8_lossy` changes lengths. This map
keeps highlighting and Fragments on the right characters.

```rust
//! Text for display: invalid UTF-8 becomes U+FFFD, with a map of offsets.

/// Returns the text and `map`, where `map[i]` is the offset in the text of original byte `i`,
/// for every `i` in `0..=bytes.len()`. Each maximal invalid sequence becomes one U+FFFD, as
/// `String::from_utf8_lossy` does.
pub fn lossy_with_map(bytes: &[u8]) -> (String, Vec<usize>) {
    let mut out = String::with_capacity(bytes.len());
    let mut map = Vec::with_capacity(bytes.len() + 1);
    for chunk in bytes.utf8_chunks() {
        let valid = chunk.valid();
        let start = out.len();
        out.push_str(valid);
        map.extend((0..valid.len()).map(|i| start + i));
        let invalid = chunk.invalid();
        if !invalid.is_empty() {
            let at = out.len();
            out.push('\u{FFFD}');
            map.extend(std::iter::repeat_n(at, invalid.len()));
        }
    }
    map.push(out.len());
    (out, map)
}

/// The line without its `\n` terminator. A `\r` before it stays: ripgrep's default line
/// terminator is `\n` alone (CRLF mode, `--crlf`, is not in M1), and it keeps the `\r` in
/// both its output and its submatch search.
pub fn strip_terminator(bytes: &[u8]) -> &[u8] {
    bytes.strip_suffix(b"\n").unwrap_or(bytes)
}
```

- [ ] **Step 1: tests first**:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_text_maps_one_to_one() {
        let (t, m) = lossy_with_map("añb".as_bytes());
        assert_eq!(t, "añb");
        assert_eq!(m, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn invalid_bytes_become_one_replacement() {
        let (t, m) = lossy_with_map(b"caf\xe9 x");
        assert_eq!(t, "caf\u{FFFD} x");
        assert_eq!(m, vec![0, 1, 2, 3, 6, 7, 8]);
    }

    #[test]
    fn terminators() {
        assert_eq!(strip_terminator(b"a\r\n"), b"a\r");
        assert_eq!(strip_terminator(b"a\n"), b"a");
        assert_eq!(strip_terminator(b"a"), b"a");
    }
}
```

- [ ] **Step 2:** RED, then GREEN. Commit «Add lossy text with an offset map».

### Task 1.4: trail.rs — LineIndex, Outline, trail_for, names_of

**Files:** `src/trail.rs`.

```rust
//! Outline of a Source and the computation of Trails from it (design notes 7 and 8).
use crate::model::{Name, Segment, Trail};
use std::ops::Range;

/// 1-based line lookup by byte offset.
pub struct LineIndex {
    starts: Vec<usize>,
}

impl LineIndex {
    pub fn new(content: &[u8]) -> Self {
        let mut starts = vec![0];
        starts.extend(memchr::memchr_iter(b'\n', content).map(|i| i + 1));
        Self { starts }
    }

    /// Line containing `byte`. A byte past the end belongs to the last line.
    pub fn line_of(&self, byte: usize) -> u64 {
        match self.starts.binary_search(&byte) {
            Ok(i) => i as u64 + 1,
            Err(i) => i as u64,
        }
    }

    pub fn line_start(&self, line: u64) -> Option<usize> {
        self.starts.get((line as usize).checked_sub(1)?).copied()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineSegment {
    /// `line`/`end_line` left at 0 are filled by `Outline::build` from `range`.
    pub segment: Segment,
    /// Bytes covered: the Section.
    pub range: Range<usize>,
    /// Bytes that name the Segment (a heading line, an item's first line). May be empty.
    pub label: Range<usize>,
    /// Pieces of the name's text in the Source, for `--in-headings`, in the format of
    /// `Name::pieces`. None: the Segment is not searchable by name.
    pub name: Option<Vec<(Range<usize>, Range<usize>)>>,
    /// `Name::anchor` for this Segment (meaningful only when `name` is Some).
    pub name_anchor: usize,
    pub parent: Option<usize>,
    /// Hidden Segments take part in containment but never appear in a Trail.
    pub hidden: bool,
}

impl OutlineSegment {
    pub fn new(kind: &str, name: impl Into<String>, range: Range<usize>, label: Range<usize>) -> Self {
        Self {
            segment: Segment { kind: kind.into(), name: name.into(), line: 0, end_line: 0, level: None, signature: None },
            range, label, name: None, name_anchor: 0, parent: None, hidden: false,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Outline {
    /// Sorted by `range.start` ascending, then `range.end` descending; parents precede children.
    pub segments: Vec<OutlineSegment>,
}

impl Outline {
    /// Sorts the Segments, assigns parents by containment and fills missing line numbers.
    /// A Segment that starts inside another and ends after it is clamped to its parent's end,
    /// so a Processor bug can never break the tree.
    pub fn build(mut segments: Vec<OutlineSegment>, content: &[u8]) -> Self {
        segments.sort_by(|a, b| a.range.start.cmp(&b.range.start).then(b.range.end.cmp(&a.range.end)));
        let lines = LineIndex::new(content);
        let mut stack: Vec<usize> = Vec::new();
        for i in 0..segments.len() {
            let start = segments[i].range.start;
            while let Some(&top) = stack.last() {
                if start < segments[top].range.end { break; }
                stack.pop();
            }
            if let Some(&top) = stack.last() {
                let end = segments[top].range.end;
                if segments[i].range.end > end { segments[i].range.end = end; }
            }
            segments[i].parent = stack.last().copied();
            let r = segments[i].range.clone();
            let seg = &mut segments[i].segment;
            if seg.line == 0 {
                seg.line = lines.line_of(r.start);
                seg.end_line = lines.line_of(r.end.saturating_sub(1).max(r.start));
            }
            stack.push(i);
        }
        Self { segments }
    }

    /// Index of the innermost Segment whose range contains `byte`.
    fn innermost(&self, byte: usize) -> Option<usize> {
        self.segments.iter().rposition(|s| s.range.start <= byte && byte < s.range.end)
    }

    fn chain(&self, i: usize) -> Vec<usize> {
        let mut chain = Vec::new();
        let mut cur = Some(i);
        while let Some(j) = cur {
            chain.push(j);
            cur = self.segments[j].parent;
        }
        chain.reverse();
        chain
    }
}

/// The Trail of the Match whose first byte is `byte` (design note 7).
pub fn trail_for(outline: &Outline, byte: usize) -> Trail {
    let Some(inner) = outline.innermost(byte) else { return Vec::new() };
    outline.chain(inner).into_iter()
        .map(|i| &outline.segments[i])
        .filter(|s| !s.hidden && !s.label.contains(&byte))
        .map(|s| s.segment.clone())
        .collect()
}

/// Every visible Segment that has a name range, with its own Trail (it included, last).
pub fn names_of(outline: &Outline) -> Vec<Name> {
    outline.segments.iter().enumerate()
        .filter(|(_, s)| !s.hidden && s.name.is_some())
        .map(|(i, s)| Name {
            text: s.segment.name.clone(),
            pieces: s.name.clone().unwrap(),
            anchor: s.name_anchor,
            trail: outline.chain(i).into_iter().map(|j| &outline.segments[j]).filter(|x| !x.hidden).map(|x| x.segment.clone()).collect(),
        })
        .collect()
}
```

`innermost` uses `rposition`: segments are sorted by start, and the last one that contains
`byte` is the innermost, because a later-starting Segment that contains `byte` is nested inside
every earlier one that also contains it (after the clamping in `build`).

- [ ] **Step 1: tests first**:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "# A\nalpha\n## B\nbeta\n# C\ngamma\n"; // 30 bytes
    // line starts:      0    4      10    15    20   24

    fn heading(name: &str, range: Range<usize>, label: Range<usize>) -> OutlineSegment {
        let mut s = OutlineSegment::new("heading", name, range, label.clone());
        s.name = Some(vec![(0..1, label.end - 2..label.end - 1)]); // the one-letter name before "\n"
        s
    }

    fn outline() -> Outline {
        Outline::build(vec![
            heading("C", 20..30, 20..24),
            heading("A", 0..20, 0..4),
            heading("B", 10..20, 10..15),
        ], DOC.as_bytes())
    }

    fn names(t: &Trail) -> Vec<&str> { t.iter().map(|s| s.name.as_str()).collect() }

    #[test]
    fn build_sorts_assigns_parents_and_lines() {
        let o = outline();
        let n: Vec<_> = o.segments.iter().map(|s| s.segment.name.as_str()).collect();
        assert_eq!(n, ["A", "B", "C"]);
        assert_eq!(o.segments[1].parent, Some(0));
        assert_eq!(o.segments[2].parent, None);
        assert_eq!((o.segments[1].segment.line, o.segments[1].segment.end_line), (3, 4));
    }

    #[test]
    fn trail_walks_parents() {
        assert_eq!(names(&trail_for(&outline(), 16)), ["A", "B"]); // "beta"
    }

    #[test]
    fn a_match_on_a_label_leaves_that_segment_out() {
        assert_eq!(names(&trail_for(&outline(), 13)), ["A"]); // inside "## B"
    }

    #[test]
    fn outside_every_segment_is_an_empty_trail() {
        let o = Outline::build(vec![OutlineSegment::new("heading", "A", 5..10, 5..6)], b"xxxxxyyyyyzzzzz");
        assert!(trail_for(&o, 2).is_empty());
    }

    #[test]
    fn hidden_segments_contain_but_do_not_show() {
        let doc = b"| Plan | Price |\n|---|---|\n| Pro | 10 |\n";
        let mut table = OutlineSegment::new("table", "table", 0..doc.len(), 0..0);
        table.hidden = true;
        let mut cell = OutlineSegment::new("column", "Price", 34..38, 0..0);
        cell.segment.line = 1;
        cell.segment.end_line = 3;
        let o = Outline::build(vec![cell, table], doc);
        let t = trail_for(&o, 35);
        assert_eq!(t.len(), 1);
        assert_eq!((t[0].kind.as_str(), t[0].name.as_str(), t[0].line, t[0].end_line), ("column", "Price", 1, 3));
    }

    #[test]
    fn crossing_ranges_are_clamped_to_the_parent() {
        let o = Outline::build(vec![
            OutlineSegment::new("item", "i", 0..10, 0..2),
            OutlineSegment::new("heading", "h", 2..20, 2..4),
        ], &[b'x'; 20]);
        assert_eq!(o.segments[1].range, 2..10);
        assert_eq!(o.segments[1].parent, Some(0));
    }

    #[test]
    fn names_carry_their_own_trail() {
        let n = names_of(&outline());
        let b = n.iter().find(|x| x.text == "B").unwrap();
        assert_eq!(&DOC[b.pieces[0].1.clone()], "B");
        assert_eq!(b.source_ranges(0..1), vec![13..14]);
        assert_eq!(names(&b.trail), ["A", "B"]);
    }

    #[test]
    fn line_index() {
        let li = LineIndex::new(b"ab\ncd\n");
        assert_eq!((li.line_of(0), li.line_of(2), li.line_of(3), li.line_of(6)), (1, 1, 2, 3));
        assert_eq!(li.line_start(2), Some(3));
    }
}
```

Offsets in `hidden_segments_contain_but_do_not_show`: the doc's third line starts at byte 27
(`| Pro | 10 |`), the pipes around ` 10 ` are at bytes 33 and 38, so the cell spans 34..38 and
`10` sits at 35..37. Recount if the literal changes.

- [ ] **Step 2:** RED, then GREEN; clippy clean. Commit «Add Outline and Trail computation».

---

## Chunk 2: Command line and the ripgrep flag table

### Task 2.1: rg_flags.rs — every ripgrep flag, its arity and its M1 status

**Files:** `src/cli/rg_flags.rs`, `docs/rg-compat.md`.

- [ ] **Step 1: extract the list** from ripgrep 14.1.1:

```bash
rg --help | perl -ne 'print "$2\t$1\t$3$4\n" if /^ {4}(?:-([A-Za-z0-9.])( [A-Z]+)?, )?--([a-z0-9-]+)(=\S+)?/' 
```

Each entry line of `rg --help` is indented four spaces; prose lines are indented further, which
excludes the two prose mentions (`--type-not=all causes…`, `--no-line-number was given…`) that a
looser pattern picks up. Then collect the negations ripgrep documents in prose:

```bash
rg --help | grep -o -- '--no-[a-z0-9-]*' | sort -u
```

and the negations ripgrep writes without `no-`:

```bash
rg --help | grep -o 'disabled with --[a-z0-9-]*' | sed 's/disabled with //' | sort -u
```

(a review counted 12: `--ignore`, `--ignore-dot`, `--ignore-exclude`, `--ignore-files`,
`--ignore-global`, `--ignore-messages`, `--ignore-parent`, `--ignore-vcs`, `--messages`,
`--pcre2-unicode`, `--require-git`, `--unicode`). Add every negation that is not already an
entry, with no value and the status of the flag it negates. Dedup by long name. Report both counts in
the chunk report (a review measured 104 entries, 30 `--no-` negations and 12 negations without `no-`, 146 names
in all; the spec's «111» was an over-count, and C10 corrects it with this chunk's numbers).

```rust
//! ripgrep's flags, their arity, and whether searchfor implements them yet (spec §8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status { Implemented, NotYet }

#[derive(Debug, Clone, Copy)]
pub struct RgFlag {
    pub long: &'static str,
    pub short: Option<char>,
    /// True when the flag takes a value (`--glob=GLOB`, `-g GLOB`).
    pub takes_value: bool,
    pub status: Status,
}

pub const RG_FLAGS: &[RgFlag] = &[ /* one entry per unique long name, sorted */ ];

pub fn by_long(long: &str) -> Option<&'static RgFlag> { RG_FLAGS.iter().find(|f| f.long == long) }

pub fn render_markdown() -> String { /* as below */ }
```

`render_markdown` writes `# ripgrep flag compatibility`, a line saying it is generated by
`cargo test rg_compat_doc -- --ignored` and must not be edited, and a table
`| Flag | Short | Takes a value | searchfor |` with `yes` / `not yet` in the last column.

**Implemented in M1** (everything else is `NotYet`): `regexp`/`-e`, `file`/`-f`,
`ignore-case`/`-i`, `case-sensitive`/`-s`, `smart-case`/`-S`, `word-regexp`/`-w`,
`line-regexp`/`-x`, `fixed-strings`/`-F`, `invert-match`/`-v`, `glob`/`-g`, `iglob`, `type`/`-t`,
`type-not`/`-T`, `type-list`, `hidden`/`-.`, `no-ignore`, `unrestricted`/`-u`, `follow`/`-L`,
`max-depth`/`-d`, `after-context`/`-A`, `before-context`/`-B`, `context`/`-C`, `count`/`-c`,
`count-matches`, `files-with-matches`/`-l`, `files-without-match`, `max-count`/`-m`,
`line-number`/`-n`, `no-line-number`/`-N`, `with-filename`/`-H`, `no-filename`/`-I`, `color`,
`max-columns`/`-M`, `json`, `sort` (values `path` and `none`), `threads`/`-j`, `text`/`-a`,
`binary`, `quiet`/`-q`, `no-messages`, `help`/`-h`, `version`/`-V`, and the negations of these that
ripgrep documents (for example `--no-hidden`, `--no-follow`, `--no-text`, `--no-json`,
`--ignore`). A name in this list that the extraction does not produce is reported, and the
list follows ripgrep's spelling.

- [ ] **Step 2: tests first** (`src/cli/rg_flags.rs`):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn reference_rg_help() -> Option<String> {
        let v = std::process::Command::new("rg").arg("--version").env_remove("RIPGREP_CONFIG_PATH").output().ok()?;
        if !String::from_utf8_lossy(&v.stdout).starts_with("ripgrep 14.1.1") {
            eprintln!("skipping: reference rg 14.1.1 not found");
            return None;
        }
        let h = std::process::Command::new("rg").arg("--help").env_remove("RIPGREP_CONFIG_PATH").output().ok()?;
        Some(String::from_utf8_lossy(&h.stdout).into_owned())
    }

    #[test]
    fn table_is_sorted_and_unique() {
        let longs: Vec<_> = RG_FLAGS.iter().map(|f| f.long).collect();
        let mut sorted = longs.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(longs, sorted);
    }

    #[test]
    fn table_covers_reference_rg() {
        let Some(help) = reference_rg_help() else { return };
        let re = regex_lite::Regex::new(r"(?m)^ {4}(?:-[A-Za-z0-9.](?: [A-Z]+)?, )?--([a-z0-9-]+)(=\S+)?").unwrap();
        for c in re.captures_iter(&help) {
            let f = by_long(&c[1]).unwrap_or_else(|| panic!("--{} missing from RG_FLAGS", &c[1]));
            assert_eq!(f.takes_value, c.get(2).is_some(), "--{} arity", &c[1]);
        }
        for neg in regex_lite::Regex::new(r"--no-[a-z0-9-]+").unwrap().find_iter(&help) {
            assert!(by_long(&neg.as_str()[2..]).is_some(), "{} missing", neg.as_str());
        }
        for c in regex_lite::Regex::new(r"disabled with --([a-z0-9-]+)").unwrap().captures_iter(&help) {
            assert!(by_long(&c[1]).is_some(), "--{} missing", &c[1]);
        }
    }

    #[test]
    #[ignore]
    fn rg_compat_doc() {
        std::fs::write(concat!(env!("CARGO_MANIFEST_DIR"), "/docs/rg-compat.md"), render_markdown()).unwrap();
    }

    #[test]
    fn rg_compat_doc_is_current() {
        let on_disk = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/docs/rg-compat.md")).unwrap();
        assert_eq!(on_disk, render_markdown(), "regenerate with: cargo test rg_compat_doc -- --ignored");
    }
}
```

- [ ] **Step 3:** RED (empty table), fill the table, regenerate the doc, GREEN. Commit «Add
  ripgrep's flag table and docs/rg-compat.md».

### Task 2.2: cli/mod.rs — Args, NotYet flags inside clap, flag order

**Files:** `src/cli/mod.rs`.

The not-yet flags are registered in clap as hidden arguments with their arity, so clap does
the tokenising (clusters like `-nP`, values like `-e --pcre2`, `--regexp=--pcre2`, `--`), and
the check runs on what clap parsed. Overriding pairs use clap's `overrides_with`, so the last
one on the command line wins, as in ripgrep.

```rust
//! Command line: ripgrep-compatible flags plus searchfor's own (spec §8).
pub mod rg_flags;

use clap::{parser::ValueSource, Arg, ArgAction, CommandFactory, FromArgMatches, Parser, ValueEnum};
use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ColorWhen { Never, Auto, Always, Ansi }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Case { Sensitive, Insensitive, Smart }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryMode { Quit, Binary, Text }

/// What the command line asked for; `Auto` is ripgrep's default (binary for explicit files and
/// stdin, quit for walked files).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BinaryChoice { #[default] Auto, Binary, Text }

#[derive(Debug, Parser)]
#[command(name = "sfor", version, about = "ripgrep-compatible search that shows where each match lives",
          disable_help_subcommand = true, args_override_self = true)]
pub struct Args {
    /// A regular expression used for searching.
    #[arg(allow_hyphen_values = false)]
    pub pattern: Option<String>,
    /// Files or directories to search. `-` is stdin.
    pub paths: Vec<PathBuf>,

    #[arg(short = 'e', long = "regexp", action = ArgAction::Append, allow_hyphen_values = true)] pub regexp: Vec<String>,
    #[arg(short = 'f', long = "file", action = ArgAction::Append)] pub pattern_files: Vec<PathBuf>,
    #[arg(short = 'i', long, overrides_with_all = ["case_sensitive", "smart_case"])] pub ignore_case: bool,
    #[arg(short = 's', long, overrides_with_all = ["ignore_case", "smart_case"])] pub case_sensitive: bool,
    #[arg(short = 'S', long, overrides_with_all = ["ignore_case", "case_sensitive"])] pub smart_case: bool,
    #[arg(short = 'w', long)] pub word_regexp: bool,
    #[arg(short = 'x', long)] pub line_regexp: bool,
    #[arg(short = 'F', long, overrides_with = "no_fixed_strings")] pub fixed_strings: bool,
    #[arg(long, hide = true)] pub no_fixed_strings: bool,
    #[arg(short = 'v', long)] pub invert_match: bool,
    #[arg(short = 'g', long = "glob", action = ArgAction::Append, allow_hyphen_values = true)] pub globs: Vec<String>,
    #[arg(long = "iglob", action = ArgAction::Append, allow_hyphen_values = true)] pub iglobs: Vec<String>,
    #[arg(short = 't', long = "type", action = ArgAction::Append)] pub types: Vec<String>,
    #[arg(short = 'T', long = "type-not", action = ArgAction::Append)] pub types_not: Vec<String>,
    #[arg(long)] pub type_list: bool,
    #[arg(short = '.', long, overrides_with = "no_hidden")] pub hidden: bool,
    #[arg(long, hide = true)] pub no_hidden: bool,
    #[arg(long, overrides_with = "ignore")] pub no_ignore: bool,
    #[arg(long, hide = true)] pub ignore: bool,
    /// One entry per occurrence, so `parse` can see where the third one is: clap's `Count`
    /// keeps a single index (clap_builder parser.rs:1286–1299). `-uuu` and `-u -u -u` both give
    /// three entries; verify that clap reports one index per short flag inside a cluster, and
    /// that `sfor --help` shows `-u, --unrestricted` with no value placeholder (set `value_name`
    /// or a custom `help` as needed; review r4 could not measure it).
    #[arg(short = 'u', long = "unrestricted", action = ArgAction::Append, num_args = 0, default_missing_value = "1",
          value_parser = clap::value_parser!(u8))]
    pub unrestricted_flags: Vec<u8>,
    #[arg(skip)] pub unrestricted: u8,
    #[arg(short = 'L', long, overrides_with = "no_follow")] pub follow: bool,
    #[arg(long, hide = true)] pub no_follow: bool,
    #[arg(short = 'd', long)] pub max_depth: Option<usize>,
    #[arg(short = 'A', long)] pub after_context: Option<usize>,
    #[arg(short = 'B', long)] pub before_context: Option<usize>,
    #[arg(short = 'C', long)] pub context: Option<usize>,
    // The output modes override each other: the last one on the command line wins, as in
    // ripgrep (`--json -c` counts, `-c --json` prints JSON, `-l -c` counts, `-c -l` lists).
    #[arg(short = 'c', long, overrides_with_all = ["count_matches", "files_with_matches", "files_without_match", "json"])] pub count: bool,
    #[arg(long, overrides_with_all = ["count", "files_with_matches", "files_without_match", "json"])] pub count_matches: bool,
    #[arg(short = 'l', long, overrides_with_all = ["count", "count_matches", "files_without_match", "json"])] pub files_with_matches: bool,
    #[arg(long, overrides_with_all = ["count", "count_matches", "files_with_matches", "json"])] pub files_without_match: bool,
    #[arg(short = 'm', long)] pub max_count: Option<u64>,
    #[arg(short = 'n', long, overrides_with = "no_line_number")] pub line_number: bool,
    #[arg(short = 'N', long, overrides_with = "line_number")] pub no_line_number: bool,
    #[arg(short = 'H', long, overrides_with = "no_filename")] pub with_filename: bool,
    #[arg(short = 'I', long, overrides_with = "with_filename")] pub no_filename: bool,
    #[arg(long, value_enum, default_value = "auto")] pub color: ColorWhen,
    #[arg(short = 'M', long)] pub max_columns: Option<u64>,
    #[arg(long, overrides_with_all = ["no_json", "count", "count_matches", "files_with_matches", "files_without_match"])] pub json: bool,
    #[arg(long, hide = true)] pub no_json: bool,
    /// Only `path` and `none` in M1; any other value is «not supported yet».
    #[arg(long)] pub sort: Option<String>,
    #[arg(short = 'j', long)] pub threads: Option<usize>,
    // The four binary-mode flags override each other (last wins); the third -u competes with
    // the survivor by position, resolved in `parse` into `binary_choice`.
    #[arg(short = 'a', long, overrides_with_all = ["no_text", "binary", "no_binary"])] pub text: bool,
    #[arg(long, hide = true, overrides_with_all = ["text", "binary", "no_binary"])] pub no_text: bool,
    /// ripgrep's --binary: search binary files, stop at the first NUL and say so.
    #[arg(long, overrides_with_all = ["text", "no_text", "no_binary"])] pub binary: bool,
    #[arg(long, hide = true, overrides_with_all = ["text", "no_text", "binary"])] pub no_binary: bool,
    #[arg(skip)] pub binary_choice: BinaryChoice,
    #[arg(short = 'q', long)] pub quiet: bool,
    #[arg(long, overrides_with = "messages")] pub no_messages: bool,
    #[arg(long, hide = true, overrides_with = "no_messages")] pub messages: bool,

    // searchfor's own flags (spec §8)
    /// Keep Matches whose Trail contains these Segment names, in order ("A › B", "A > B", "A/B").
    #[arg(long)] pub within: Option<String>,
    /// Print the whole Section of the last Segment of each Match.
    #[arg(long)] pub expand: bool,
    /// Match only Segment names: headings, keys, function names.
    #[arg(long)] pub in_headings: bool,
    /// Force a Format: NAME for every file, or GLOB=NAME.
    #[arg(long = "format", action = ArgAction::Append)] pub formats: Vec<String>,
    #[arg(long, overrides_with = "tree")] pub flat: bool,
    #[arg(long, overrides_with = "flat")] pub tree: bool,
    /// Print exactly what ripgrep would print.
    #[arg(long)] pub no_trail: bool,
    /// Fragment width when there is no terminal.
    #[arg(long, default_value_t = 200)] pub fragment_width: usize,
    #[arg(long)] pub list_formats: bool,
    #[arg(long, value_name = "DIR")] pub export_formats: Option<PathBuf>,

}

#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error("sfor: --{0} is not supported yet (see docs/rg-compat.md)")]
    NotYet(String),
    #[error("sfor: {0}")]
    Usage(String),
    #[error(transparent)]
    Clap(#[from] clap::Error),
}

fn notyet_id(long: &str) -> String { format!("notyet--{long}") }

/// Args::command() plus every NotYet ripgrep flag, hidden, with its arity.
pub fn command() -> clap::Command {
    let mut cmd = Args::command();
    for f in rg_flags::RG_FLAGS.iter().filter(|f| f.status == rg_flags::Status::NotYet) {
        let mut a = Arg::new(notyet_id(f.long)).long(f.long).hide(true);
        if let Some(c) = f.short { a = a.short(c); }
        a = if f.takes_value {
            a.num_args(1).action(ArgAction::Append).allow_hyphen_values(true)
        } else {
            a.action(ArgAction::Count)
        };
        cmd = cmd.arg(a);
    }
    cmd
}

pub fn parse(args: Vec<OsString>) -> Result<Args, CliError> {
    let m = command().try_get_matches_from(args)?;
    for f in rg_flags::RG_FLAGS.iter().filter(|f| f.status == rg_flags::Status::NotYet) {
        if m.value_source(&notyet_id(f.long)) == Some(ValueSource::CommandLine) {
            return Err(CliError::NotYet(f.long.to_string()));
        }
    }
    let mut a = Args::from_arg_matches(&m)?;
    if a.unrestricted_flags.len() > 3 {
        // ripgrep 14.1.1's exact message and exit 2.
        return Err(CliError::Usage("error parsing flag -u: flag can only be repeated up to 3 times".into()));
    }
    a.unrestricted = a.unrestricted_flags.len() as u8;
    a.binary_choice = binary_choice(&m);
    if let Some(s) = &a.sort {
        if s != "path" && s != "none" { return Err(CliError::NotYet(format!("sort={s}"))); }
    }
    let structural = a.within.is_some() || a.expand || a.in_headings;
    if a.no_trail && structural {
        return Err(CliError::Usage("--no-trail cannot be combined with --within, --expand or --in-headings".into()));
    }
    let summary = a.count || a.count_matches || a.files_with_matches || a.files_without_match;
    if a.expand && (summary || a.quiet) {
        return Err(CliError::Usage("--expand prints lines; it cannot be combined with -c, --count-matches, -l, --files-without-match or -q".into()));
    }
    Ok(a)
}

/// ripgrep's binary-mode flags in command-line order: -a → Text; --binary and the third -u →
/// Binary; --no-text and --no-binary → Auto. The last one wins (`rg -a -uuu` and
/// `rg -a --no-binary` both print the binary message, measured in review r3).
fn binary_choice(m: &clap::ArgMatches) -> BinaryChoice {
    let mut last: Option<(usize, BinaryChoice)> = None;
    let mut see = |idx: Option<usize>, c: BinaryChoice| {
        if let Some(i) = idx { if last.is_none_or(|(j, _)| i > j) { last = Some((i, c)); } }
    };
    for (id, c) in [("text", BinaryChoice::Text), ("binary", BinaryChoice::Binary),
                    ("no_text", BinaryChoice::Auto), ("no_binary", BinaryChoice::Auto)] {
        if m.get_flag(id) { see(m.index_of(id), c); }
    }
    see(m.indices_of("unrestricted_flags").and_then(|mut it| it.nth(2)), BinaryChoice::Binary);
    last.map(|(_, c)| c).unwrap_or_default()
}

/// Where the patterns come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Patterns {
    pub patterns: Vec<String>,
    pub paths: Vec<PathBuf>,
    /// True when `-f -` consumed stdin, so stdin cannot also be searched.
    pub stdin_used: bool,
}

impl Args {
    /// ripgrep's precedence: with -e or -f, the positional is a path. `-f -` reads stdin.
    /// An empty pattern file is an empty pattern set: ripgrep 14.1.1 then searches nothing and
    /// exits 1 in every mode, `-v`, `-c` and `--json` included (no summary either); C9 does the
    /// same before walking.
    pub fn patterns(&self) -> std::io::Result<Patterns> {
        let mut paths = self.paths.clone();
        let mut patterns = self.regexp.clone();
        let mut stdin_used = false;
        for f in &self.pattern_files {
            let text = if f.as_os_str() == "-" {
                stdin_used = true;
                std::io::read_to_string(std::io::stdin())?
            } else {
                std::fs::read_to_string(f)?
            };
            patterns.extend(text.lines().map(str::to_string));
        }
        if self.regexp.is_empty() && self.pattern_files.is_empty() {
            if let Some(p) = &self.pattern { patterns.push(p.clone()); }
        } else if let Some(p) = &self.pattern {
            paths.insert(0, PathBuf::from(p));
        }
        Ok(Patterns { patterns, paths, stdin_used })
    }

    pub fn case(&self) -> Case {
        if self.ignore_case { Case::Insensitive } else if self.smart_case { Case::Smart } else { Case::Sensitive }
    }

    pub fn before(&self) -> usize { self.before_context.or(self.context).unwrap_or(0) }
    pub fn after(&self) -> usize { self.after_context.or(self.context).unwrap_or(0) }
    pub fn hidden(&self) -> bool { (self.hidden && !self.no_hidden) || self.unrestricted >= 2 }
    pub fn no_ignore(&self) -> bool { (self.no_ignore && !self.ignore) || self.unrestricted >= 1 }
    /// ripgrep's three binary modes for one file: quit at NUL, «binary» (search, stop at NUL
    /// and print the binary message), text.
    pub fn binary_mode(&self, explicit: bool) -> BinaryMode {
        match self.binary_choice {
            BinaryChoice::Text => BinaryMode::Text,
            BinaryChoice::Binary => BinaryMode::Binary,
            BinaryChoice::Auto => if explicit { BinaryMode::Binary } else { BinaryMode::Quit },
        }
    }
    pub fn structural(&self) -> bool { self.within.is_some() || self.expand || self.in_headings }
}
```

The negations in this struct (`no_fixed_strings`, `no_hidden`, `ignore`, `no_follow`, `no_json`,
`no_text`) are the ones the plan knows; Task 2.1's extraction lists the full set, and every
negation of an Implemented flag gets the same treatment (a hidden bool with `overrides_with`).
`overrides_with` between two bools leaves only the last one `true`; the accessor methods read
the pair anyway, so a missing `overrides_with` cannot invert the meaning.

- [ ] **Step 1: tests first** (`src/cli/mod.rs`):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    fn os(v: &[&str]) -> Vec<OsString> { v.iter().map(OsString::from).collect() }
    fn p(v: &[&str]) -> Args { parse(os(v)).unwrap() }

    #[test]
    fn not_yet_flags_are_reported_by_name() {
        assert_eq!(parse(os(&["sfor", "--pcre2", "x"])).unwrap_err().to_string(),
                   "sfor: --pcre2 is not supported yet (see docs/rg-compat.md)");
        assert!(matches!(parse(os(&["sfor", "-nP", "x"])), Err(CliError::NotYet(f)) if f == "pcre2"));
        assert!(matches!(parse(os(&["sfor", "--sort", "modified", "x"])), Err(CliError::NotYet(_))));
    }

    #[test]
    fn values_that_look_like_flags_are_values() {
        for argv in [&["sfor", "-e", "--pcre2"][..], &["sfor", "-e--pcre2"], &["sfor", "--regexp=--pcre2"]] {
            assert_eq!(p(argv).patterns().unwrap().patterns, ["--pcre2"], "{argv:?}");
        }
    }

    #[test]
    fn positional_becomes_a_path_with_e() {
        let pats = p(&["sfor", "-e", "foo", "src"]).patterns().unwrap();
        assert_eq!((pats.patterns, pats.paths), (vec!["foo".to_string()], vec![PathBuf::from("src")]));
    }

    #[test]
    fn last_case_flag_wins() {
        assert_eq!(p(&["sfor", "-s", "-i", "x"]).case(), Case::Insensitive);
        assert_eq!(p(&["sfor", "-i", "-s", "x"]).case(), Case::Sensitive);
        assert_eq!(p(&["sfor", "-i", "-S", "x"]).case(), Case::Smart);
    }

    #[test]
    fn last_of_each_pair_wins() {
        let a = p(&["sfor", "-n", "-N", "x"]);
        assert!(!a.line_number && a.no_line_number);
        let b = p(&["sfor", "--hidden", "--no-hidden", "x"]);
        assert!(!b.hidden());
    }

    #[test]
    fn last_output_mode_wins() {
        let a = p(&["sfor", "--json", "-c", "x"]);
        assert!(a.count && !a.json);
        let b = p(&["sfor", "-c", "--json", "x"]);
        assert!(b.json && !b.count);
        let c = p(&["sfor", "-c", "-l", "x"]);
        assert!(c.files_with_matches && !c.count);
    }

    #[test]
    fn binary_modes() {
        assert_eq!(p(&["sfor", "x"]).binary_mode(false), BinaryMode::Quit);
        assert_eq!(p(&["sfor", "x"]).binary_mode(true), BinaryMode::Binary);
        assert_eq!(p(&["sfor", "-uuu", "x"]).binary_mode(false), BinaryMode::Binary);
        assert_eq!(p(&["sfor", "-a", "x"]).binary_mode(false), BinaryMode::Text);
        assert_eq!(p(&["sfor", "-a", "--binary", "x"]).binary_mode(false), BinaryMode::Binary);
        assert_eq!(p(&["sfor", "-a", "-uuu", "x"]).binary_mode(false), BinaryMode::Binary);
        assert_eq!(p(&["sfor", "-uuu", "-a", "x"]).binary_mode(false), BinaryMode::Text);
        assert_eq!(p(&["sfor", "-a", "--no-binary", "x"]).binary_mode(true), BinaryMode::Binary);
        assert_eq!(p(&["sfor", "--no-binary", "-a", "x"]).binary_mode(true), BinaryMode::Text);
        assert_eq!(p(&["sfor", "-auuu", "x"]).binary_mode(false), BinaryMode::Binary);
        assert_eq!(p(&["sfor", "-uuua", "x"]).binary_mode(false), BinaryMode::Text);
        assert_eq!(p(&["sfor", "-u", "-a", "-u", "-u", "x"]).binary_mode(false), BinaryMode::Binary);
        assert!(matches!(parse(os(&["sfor", "-uuuu", "x"])), Err(CliError::Usage(_))));
        assert_eq!(p(&["sfor", "-uu", "x"]).unrestricted, 2);
    }

    #[test]
    fn empty_pattern_file_is_an_empty_set() {
        let f = tempfile::NamedTempFile::new().unwrap();
        let pats = p(&["sfor", "-f", f.path().to_str().unwrap()]).patterns().unwrap();
        assert!(pats.patterns.is_empty());
    }

    #[test]
    fn context_defaults() {
        let a = p(&["sfor", "-C", "2", "-A", "5", "x"]);
        assert_eq!((a.before(), a.after()), (2, 5));
    }

    #[test]
    fn invalid_combinations() {
        assert!(matches!(parse(os(&["sfor", "--no-trail", "--expand", "x"])), Err(CliError::Usage(_))));
        assert!(matches!(parse(os(&["sfor", "--expand", "-c", "x"])), Err(CliError::Usage(_))));
    }

    #[test]
    fn the_word_formats_is_a_pattern() {
        assert_eq!(p(&["sfor", "formats"]).pattern.as_deref(), Some("formats"));
    }
}
```

If `--pcre2`/`-P` is not `NotYet` in ripgrep 14.1.1's table, use any `NotYet` flag with a short
form and adjust the expectations.

- [ ] **Step 2:** RED, GREEN, clippy clean. Commit «Add the command line».

---

## Chunk 4: Format Definitions, registry and Detector

### Task 4.1: definition.rs and the built-in Markdown Definition

**Files:** `src/formats/definition.rs`, `src/formats/builtin.rs`, `formats/markdown.toml`.

`formats/markdown.toml`:

```toml
# Markdown: headings, fenced code blocks, table columns, frontmatter title, list items.
name       = "markdown"
extensions = ["md", "markdown", "mdx", "mkd", "mdown"]
names      = []
shebangs   = []
processor  = "markdown"

[params]
# Parent list items as Segments (grill #11).
list_items = true
# Characters of an item's first line used as its name.
item_name_chars = 40
```

`src/formats/builtin.rs`:

```rust
//! Built-in Format Definitions, embedded at compile time (ADR 0002).
/// (file name, TOML source). Add one line per file in formats/.
pub const BUILTIN: &[(&str, &str)] = &[
    ("markdown.toml", include_str!("../../formats/markdown.toml")),
];
```

`src/formats/definition.rs`:

```rust
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    BuiltIn,
    User(PathBuf),
    /// A user file that replaced the built-in with the same name.
    Overridden(PathBuf),
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Magic {
    #[serde(default)]
    pub offset: usize,
    /// Hex string, e.g. "7f454c46" for ELF.
    pub hex: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FormatDefinition {
    pub name: String,
    #[serde(default)] pub extensions: Vec<String>,
    #[serde(default)] pub names: Vec<String>,
    #[serde(default)] pub shebangs: Vec<String>,
    #[serde(default)] pub magic: Vec<Magic>,
    pub processor: String,
    #[serde(default)] pub params: toml::Table,
    #[serde(skip, default = "builtin")] pub origin: Origin,
}

fn builtin() -> Origin { Origin::BuiltIn }

impl FormatDefinition {
    /// `known_processor` says whether a Processor name exists. C9 passes
    /// `|p| crate::processors::by_name(p).is_some()`; tests pass `|p| p == "markdown"`. Taking
    /// it as a parameter keeps this chunk independent of the Processor registry (C5).
    pub fn parse(src: &str, known_processor: impl Fn(&str) -> bool) -> Result<Self, String> {
        let d: Self = toml::from_str(src).map_err(|e| e.to_string())?;
        if d.name.is_empty() { return Err("`name` is empty".into()); }
        if !known_processor(&d.processor) {
            return Err(format!("unknown processor `{}`", d.processor));
        }
        for m in &d.magic {
            if m.hex.is_empty() || m.hex.len() % 2 != 0 || !m.hex.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err(format!("magic `{}` is not an even-length hex string", m.hex));
            }
        }
        Ok(d)
    }

    pub fn magic_bytes(&self) -> Vec<(usize, Vec<u8>)> {
        self.magic.iter().map(|m| {
            let bytes = (0..m.hex.len()).step_by(2).map(|i| u8::from_str_radix(&m.hex[i..i + 2], 16).unwrap()).collect();
            (m.offset, bytes)
        }).collect()
    }
}
```

- [ ] **Step 1: tests first** in `definition.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    fn known(p: &str) -> bool { p == "markdown" }

    #[test]
    fn builtin_markdown_parses() {
        let d = FormatDefinition::parse(crate::formats::builtin::BUILTIN[0].1, known).unwrap();
        assert_eq!(d.name, "markdown");
        assert!(d.extensions.contains(&"md".to_string()));
        assert_eq!(d.params["list_items"].as_bool(), Some(true));
    }

    #[test]
    fn errors_name_the_field() {
        assert!(FormatDefinition::parse("name='x'\nprocessor='nope'", known).unwrap_err().contains("unknown processor `nope`"));
        assert!(FormatDefinition::parse("name='x'\nprocessor='markdown'\nmagic=[{hex='7'}]", known).unwrap_err().contains("magic"));
        assert!(FormatDefinition::parse("name='x'\nprocessor='markdown'\nbogus=1", known).unwrap_err().contains("bogus"));
    }

    #[test]
    fn magic_decodes() {
        let d = FormatDefinition::parse("name='elf'\nprocessor='markdown'\nmagic=[{hex='7f454c46'}]", known).unwrap();
        assert_eq!(d.magic_bytes(), vec![(0, vec![0x7f, b'E', b'L', b'F'])]);
    }
}
```

- [ ] **Step 2:** RED, then GREEN; commit «Add Format Definitions».

### Task 4.2: registry (mod.rs) — load, override, list, export

**Files:** `src/formats/mod.rs`.

```rust
//! Format Definitions: built-in + user directory (ADR 0002).
pub mod builtin;
pub mod definition;
pub mod detector;

use definition::{FormatDefinition, Origin};
use std::path::{Path, PathBuf};

pub struct Registry {
    defs: Vec<FormatDefinition>,
}

/// `SFOR_FORMATS_DIR` if set (tests use it), else `<config dir>/searchfor/formats`.
pub fn user_formats_dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("SFOR_FORMATS_DIR") { return Some(PathBuf::from(d)); }
    dirs::config_dir().map(|d| d.join("searchfor").join("formats"))
}

impl Registry {
    /// Loads the built-ins, then every `*.toml` in `user_dir` (sorted by file name).
    /// Returns warnings for user files that fail to parse; they are skipped.
    pub fn load(user_dir: Option<&Path>, known_processor: &dyn Fn(&str) -> bool) -> (Self, Vec<String>) {
        let mut defs: Vec<FormatDefinition> = builtin::BUILTIN.iter()
            .map(|(file, src)| FormatDefinition::parse(src, known_processor)
                .unwrap_or_else(|e| panic!("built-in {file} is invalid: {e}")))
            .collect();
        let mut warnings = Vec::new();
        if let Some(dir) = user_dir {
            let mut files: Vec<PathBuf> = std::fs::read_dir(dir).into_iter().flatten().flatten()
                .map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "toml")).collect();
            files.sort();
            for f in files {
                let parsed = std::fs::read_to_string(&f).map_err(|e| e.to_string())
                    .and_then(|s| FormatDefinition::parse(&s, known_processor));
                match parsed {
                    Err(e) => warnings.push(format!("sfor: ignoring {}: {e}", f.display())),
                    Ok(mut d) => match defs.iter().position(|x| x.name == d.name) {
                        Some(i) => { d.origin = Origin::Overridden(f); defs[i] = d; }
                        None => { d.origin = Origin::User(f); defs.push(d); }
                    },
                }
            }
        }
        (Self { defs }, warnings)
    }

    pub fn get(&self, name: &str) -> Option<&FormatDefinition> { self.defs.iter().find(|d| d.name == name) }
    pub fn all(&self) -> &[FormatDefinition] { &self.defs }

    /// Text for `--list-formats`: one line per Format, `name  origin  extensions`.
    pub fn list(&self) -> String {
        let mut out = String::new();
        for d in &self.defs {
            let origin = match &d.origin {
                Origin::BuiltIn => "built-in".to_string(),
                Origin::User(p) => format!("user ({})", p.display()),
                Origin::Overridden(p) => format!("overridden ({})", p.display()),
            };
            out.push_str(&format!("{:<12} {:<40} {}\n", d.name, origin, d.extensions.join(",")));
        }
        out
    }
}

/// `--export-formats DIR`: writes the embedded Definitions as templates (grill #6 comment).
/// Refuses the user formats directory, so an export never silently overrides the built-ins,
/// and never overwrites or follows an existing file: every target is checked before anything
/// is written, and each file is created with `create_new` (O_EXCL), which also refuses a
/// symlink sitting at the target path.
pub fn export_builtin(dir: &Path, user_dir: Option<&Path>) -> anyhow::Result<Vec<PathBuf>> {
    if let Some(u) = user_dir {
        // Paths that may not exist yet are compared after resolving their longest existing
        // ancestor (symlinks included) and folding `.` and `..` in the rest.
        let same = resolve(dir)? == resolve(u)?;
        if same {
            anyhow::bail!("refusing to export into the user formats directory {}: the files would override the built-ins", u.display());
        }
    }
    let targets: Vec<PathBuf> = builtin::BUILTIN.iter().map(|(file, _)| dir.join(file)).collect();
    // (the checks above run before anything is created)
    if let Some(t) = targets.iter().find(|t| t.symlink_metadata().is_ok()) {
        anyhow::bail!("refusing to overwrite {}", t.display());
    }
    std::fs::create_dir_all(dir)?;
    for (t, (_, src)) in targets.iter().zip(builtin::BUILTIN) {
        use std::io::Write;
        std::fs::OpenOptions::new().write(true).create_new(true).open(t)?.write_all(src.as_bytes())?;
    }
    Ok(targets)
}

/// Absolute path with every existing prefix canonicalized, re-checked after each component:
/// `..` and `.` are folded lexically, and whenever the result exists again (a `..` that comes
/// back into an existing directory, a symlink after it) it is canonicalized again. So
/// `nuevo/../link/formats` with `link → base` resolves to `base/formats`.
fn resolve(p: &Path) -> std::io::Result<PathBuf> {
    use std::path::Component;
    let mut out = PathBuf::new();
    for c in std::path::absolute(p)?.components() {
        match c {
            Component::ParentDir => { out.pop(); }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
        if std::fs::symlink_metadata(&out).is_ok() {
            out = std::fs::canonicalize(&out)?;
        }
    }
    Ok(out)
}
```

- [ ] **Step 1: tests first** in `mod.rs` (tempdirs only; never the real config dir):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    fn known(p: &str) -> bool { p == "markdown" }

    #[test]
    fn user_file_overrides_builtin_and_adds_new() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("markdown.toml"), "name='markdown'\nextensions=['txt']\nprocessor='markdown'").unwrap();
        std::fs::write(d.path().join("notes.toml"), "name='notes'\nextensions=['note']\nprocessor='markdown'").unwrap();
        std::fs::write(d.path().join("broken.toml"), "name=").unwrap();
        let (r, warnings) = Registry::load(Some(d.path()), &known);
        assert_eq!(r.get("markdown").unwrap().extensions, ["txt"]);
        assert!(matches!(r.get("markdown").unwrap().origin, Origin::Overridden(_)));
        assert!(matches!(r.get("notes").unwrap().origin, Origin::User(_)));
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("broken.toml"));
    }

    #[test]
    fn export_writes_files_and_refuses_user_dir() {
        let user = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        let written = export_builtin(&out.path().join("new"), Some(user.path())).unwrap();
        assert_eq!(written.len(), builtin::BUILTIN.len());
        assert!(export_builtin(user.path(), Some(user.path())).unwrap_err().to_string().contains("refusing"));
    }

    #[test]
    fn export_never_overwrites_or_follows_symlinks() {
        let out = tempfile::tempdir().unwrap();
        let victim = tempfile::tempdir().unwrap();
        let victim_file = victim.path().join("markdown.toml");
        std::fs::write(&victim_file, "precious").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&victim_file, out.path().join("markdown.toml")).unwrap();
        #[cfg(not(unix))]
        std::fs::write(out.path().join("markdown.toml"), "existing").unwrap();
        assert!(export_builtin(out.path(), None).unwrap_err().to_string().contains("refusing to overwrite"));
        assert_eq!(std::fs::read_to_string(&victim_file).unwrap(), "precious");
    }

    #[test]
    fn refusing_a_missing_user_dir_creates_nothing() {
        let base = tempfile::tempdir().unwrap();
        let missing = base.path().join("formats");
        assert!(export_builtin(&missing, Some(&missing)).unwrap_err().to_string().contains("refusing"));
        assert!(!missing.exists());
    }

    #[test]
    fn refusing_through_dotdot_and_symlinked_ancestors() {
        let base = tempfile::tempdir().unwrap();
        std::fs::create_dir(base.path().join("aux")).unwrap();
        let user = base.path().join("formats");
        let sneaky = base.path().join("aux/../formats");
        assert!(export_builtin(&sneaky, Some(&user)).unwrap_err().to_string().contains("refusing"));
        assert!(!user.exists());
        let missing_parent = base.path().join("nuevo/../formats");
        assert!(export_builtin(&missing_parent, Some(&user)).unwrap_err().to_string().contains("refusing"));
        assert!(!user.exists() && !base.path().join("nuevo").exists());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(base.path(), base.path().join("link2")).unwrap();
            let combined = base.path().join("nuevo/../link2/formats");
            assert!(export_builtin(&combined, Some(&user)).unwrap_err().to_string().contains("refusing"));
            assert!(!user.exists() && !base.path().join("nuevo").exists());
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(base.path(), base.path().join("link")).unwrap();
            let via_link = base.path().join("link/formats");
            assert!(export_builtin(&via_link, Some(&user)).unwrap_err().to_string().contains("refusing"));
            assert!(!user.exists());
        }
    }
}
```

- [ ] **Step 2:** RED, then GREEN; commit «Add the Format registry».

### Task 4.3: detector.rs

**Files:** `src/formats/detector.rs`.

```rust
//! The Detector (spec §4.1): override, exact name, extension, magic bytes, shebang.
use super::{definition::FormatDefinition, Registry};
use globset::{Glob, GlobMatcher};
use std::path::Path;

enum Override {
    All(String),
    Glob(GlobMatcher, String),
}

pub struct Detector<'r> {
    registry: &'r Registry,
    overrides: Vec<Override>,
}

impl<'r> Detector<'r> {
    /// `--format` values: `NAME` or `GLOB=NAME`. Unknown names are an error.
    pub fn new(registry: &'r Registry, formats: &[String]) -> anyhow::Result<Self> {
        let mut overrides = Vec::new();
        for f in formats {
            let (glob, name) = match f.rsplit_once('=') {
                Some((g, n)) => (Some(g), n),
                None => (None, f.as_str()),
            };
            if registry.get(name).is_none() {
                anyhow::bail!("unknown format `{name}` (see --list-formats)");
            }
            overrides.push(match glob {
                Some(g) => Override::Glob(Glob::new(g)?.compile_matcher(), name.to_string()),
                None => Override::All(name.to_string()),
            });
        }
        Ok(Self { registry, overrides })
    }

    /// `head` is the start of the file; 512 bytes are enough for every signal.
    pub fn detect(&self, path: &Path, head: &[u8]) -> Option<&'r FormatDefinition> {
        let defs = self.registry.all();
        for o in &self.overrides {
            match o {
                Override::All(n) => return self.registry.get(n),
                Override::Glob(g, n) if g.is_match(path) => return self.registry.get(n),
                Override::Glob(..) => {}
            }
        }
        let file_name = path.file_name()?.to_str()?;
        if let Some(d) = defs.iter().find(|d| d.names.iter().any(|n| n == file_name)) { return Some(d); }
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            let ext = ext.to_ascii_lowercase();
            if let Some(d) = defs.iter().find(|d| d.extensions.iter().any(|e| e.eq_ignore_ascii_case(&ext))) { return Some(d); }
        }
        if let Some(d) = defs.iter().find(|d| d.magic_bytes().iter().any(|(off, bytes)| head.get(*off..off + bytes.len()) == Some(bytes.as_slice()))) {
            return Some(d);
        }
        let interp = shebang_interpreter(head)?;
        defs.iter().find(|d| d.shebangs.iter().any(|s| s == interp))
    }
}

/// `#!/usr/bin/env bash -e` → "bash"; `#!/bin/sh` → "sh".
fn shebang_interpreter(head: &[u8]) -> Option<&str> {
    let line = head.strip_prefix(b"#!")?;
    let line = &line[..memchr::memchr(b'\n', line).unwrap_or(line.len())];
    let line = std::str::from_utf8(line).ok()?.trim();
    let mut words = line.split_whitespace();
    let first = words.next()?;
    let prog = first.rsplit('/').next()?;
    if prog == "env" {
        words.find(|w| !w.starts_with('-'))
    } else {
        Some(prog)
    }
}
```

- [ ] **Step 1: tests first** in `detector.rs`, with a registry loaded from a tempdir that adds three
test Definitions (`bashy` with `shebangs=['bash']`, `elfy` with `magic=[{hex='7f454c46'}]`,
`makey` with `names=['Makefile']`, all with `processor='markdown'`):

```rust
#[test]
fn signals_in_order() {
    // setup as described above → (reg, _)
    let det = Detector::new(&reg, &[]).unwrap();
    assert_eq!(det.detect(Path::new("a/README.MD"), b"").unwrap().name, "markdown");
    assert_eq!(det.detect(Path::new("Makefile"), b"").unwrap().name, "makey");
    assert_eq!(det.detect(Path::new("prog"), b"\x7fELF\x02").unwrap().name, "elfy");
    assert_eq!(det.detect(Path::new("run"), b"#!/usr/bin/env -S bash -e\n").unwrap().name, "bashy");
    assert!(det.detect(Path::new("data.bin"), b"\x00\x01").is_none());
}

#[test]
fn overrides_win_and_unknown_is_an_error() {
    let det = Detector::new(&reg, &["*.txt=markdown".into()]).unwrap();
    assert_eq!(det.detect(Path::new("notes.txt"), b"").unwrap().name, "markdown");
    assert!(det.detect(Path::new("notes.log"), b"").is_none());
    assert!(Detector::new(&reg, &["nope".into()]).is_err());
}
```

`#!/usr/bin/env -S bash -e` returns "bash" because `-S` is skipped by the `!starts_with('-')`
rule.

- [ ] **Step 2:** RED, then GREEN; clippy clean; commit «Add the Detector».

---


---

## Chunk 5: Processor contract and Markdown

### Task 5.1: processors/mod.rs — the contract and the registry by name

**Files:** `src/processors/mod.rs`.

```rust
//! Processors turn Match anchors into Trails (design note 6).
pub mod markdown;

use crate::model::{Input, Name, Trail};
use crate::trail::{names_of, trail_for, Outline};

/// The public contract. M5's External Processors implement it over the protocol of spec §4.4,
/// which C10 extends with an `"op"` field: `"trails"` (the request of §4.4) and `"names"`
/// (no `matches`; the answer is
/// `{"names":[{"text","pieces":[[text_start,text_end,src_start,src_end],…],"anchor","trail":[…]}]}`).
/// An External Processor that answers `names` with an error makes `--in-headings` skip its
/// files with one warning.
pub trait Processor: Send + Sync {
    /// One Trail per anchor (absolute byte offset in `input.bytes`), in the same order.
    fn trails(&self, input: &Input, params: &toml::Table, anchors: &[usize]) -> anyhow::Result<Vec<Trail>>;
    /// Every searchable Segment name with the Source pieces of its text (`--in-headings`).
    fn names(&self, input: &Input, params: &toml::Table) -> anyhow::Result<Vec<Name>>;
}

/// Built-in Processors that can describe a whole file as an Outline.
pub trait OutlineProcessor: Send + Sync {
    fn outline(&self, src: &[u8], params: &toml::Table) -> anyhow::Result<Outline>;
}

impl<T: OutlineProcessor> Processor for T {
    fn trails(&self, input: &Input, params: &toml::Table, anchors: &[usize]) -> anyhow::Result<Vec<Trail>> {
        let o = self.outline(input.bytes, params)?;
        Ok(anchors.iter().map(|&a| trail_for(&o, a)).collect())
    }
    fn names(&self, input: &Input, params: &toml::Table) -> anyhow::Result<Vec<Name>> {
        Ok(names_of(&self.outline(input.bytes, params)?))
    }
}

/// The Processor named in a Format Definition's `processor` field.
pub fn by_name(name: &str) -> Option<&'static dyn Processor> {
    match name {
        "markdown" => Some(&markdown::Markdown),
        _ => None,
    }
}
```

A Processor that is expensive on large files (CSV in M3) implements `Processor` directly and
looks only around the anchors; Markdown builds the whole Outline.

### Task 5.2: markdown.rs

**Files:** `src/processors/markdown.rs`, `tests/fixtures/md/guide.md`.

Rules (spec §5.1, grill #11, design note 8):

- **Headings** (ATX and setext): kind `heading`, `level` 1–6, name = the heading's text with
  inline markup flattened, `label` = the heading's byte range, `name` range = from the start of
  its first text event to the end of its last one. The Section runs to the start of the next
  heading of the same or a higher level **in the same container** (list item or blockquote),
  and never past the end of that container; at top level, to the end of the file.
- **Frontmatter** with a `title:` line: kind `document`, name = the title without quotes,
  `range` = the whole file, `label` = the frontmatter block, `name` range = the title's value.
  No `title` → no document Segment.
- **Fenced code block**: kind `code`, name = first word of the info string, `code` when empty,
  `range` = the block, `label` = its opening fence line, no name range. Indented code blocks are
  not Segments.
- **Table**: a hidden Segment `table` over the table, and one Segment `column` per cell (header
  and body), named after the header cell of its column (`column N` when the header is shorter),
  `range` = the cell, empty label, no name range, `line`/`end_line` = the table's lines.
- **List item** (when `params.list_items`, default true): kind `item`, name = the text of the
  item's first line, cut at `item_name_chars` (default 40) characters with `…` when cut,
  `range` = the item, `label` = the item's first line, no name range.
- Invalid UTF-8 → empty Outline (the file prints like Plain).

```rust
use super::OutlineProcessor;
use crate::trail::{LineIndex, Outline, OutlineSegment};
use pulldown_cmark::{CodeBlockKind, Event, MetadataBlockKind, Options, Parser, Tag, TagEnd};
use std::ops::Range;

pub struct Markdown;

fn first_line(text: &str, r: &Range<usize>) -> Range<usize> {
    let end = text[r.clone()].find('\n').map(|i| r.start + i + 1).unwrap_or(r.end);
    r.start..end
}

fn cut(s: &str, n: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= n { return s.to_string(); }
    let mut out: String = s.chars().take(n).collect();
    out.push('…');
    out
}

struct OpenHeading { seg: usize, text: String, pieces: Vec<(Range<usize>, Range<usize>)> }
struct OpenItem { seg: usize, text: String, done: bool }
struct OpenTable { seg: usize, header: Vec<String>, in_head: bool, col: usize, cell_text: String, cells: Vec<(usize, Range<usize>)> }

impl OutlineProcessor for Markdown {
    fn outline(&self, content: &[u8], params: &toml::Table) -> anyhow::Result<Outline> {
        let Ok(text) = std::str::from_utf8(content) else { return Ok(Outline::default()) };
        let list_items = params.get("list_items").and_then(|v| v.as_bool()).unwrap_or(true);
        let item_chars = params.get("item_name_chars").and_then(|v| v.as_integer()).unwrap_or(40) as usize;
        let opts = Options::ENABLE_TABLES | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS | Options::ENABLE_HEADING_ATTRIBUTES;
        let lines = LineIndex::new(content);

        let mut segs: Vec<OutlineSegment> = Vec::new();
        let mut headings: Vec<(usize, u8)> = Vec::new();      // (seg index, level)
        let mut containers: Vec<Range<usize>> = Vec::new();   // items and blockquotes
        let mut heading: Option<OpenHeading> = None;
        let mut items: Vec<OpenItem> = Vec::new();
        let mut table: Option<OpenTable> = None;
        let mut meta: Option<usize> = None;

        for (ev, r) in Parser::new_ext(text, opts).into_offset_iter() {
            match ev {
                Event::Start(Tag::MetadataBlock(MetadataBlockKind::YamlStyle)) => {
                    segs.push(OutlineSegment::new("document", "", 0..text.len(), r.clone()));
                    meta = Some(segs.len() - 1);
                }
                Event::End(TagEnd::MetadataBlock(_)) => meta = None,
                Event::Text(t) if meta.is_some() => {
                    // Find `title:` in the metadata text and point the name range at its value.
                    let base = r.start;
                    let mut off = 0;
                    for line in t.split_inclusive('\n') {
                        if let Some(rest) = line.strip_prefix("title:") {
                            let value = rest.trim_end_matches('\n').trim();
                            let unq = value.trim_matches(|c| c == '"' || c == '\'');
                            let vstart = base + off + "title:".len() + rest.find(unq).unwrap_or(0);
                            let i = meta.unwrap();
                            segs[i].segment.name = unq.to_string();
                            segs[i].name = Some(vec![(0..unq.len(), vstart..vstart + unq.len())]);
                        }
                        off += line.len();
                    }
                }
                Event::Start(Tag::BlockQuote(_)) => containers.push(r.clone()),
                Event::Start(Tag::Heading { level, .. }) => {
                    let mut s = OutlineSegment::new("heading", "", r.clone(), r.clone());
                    s.segment.level = Some(level as u8);
                    segs.push(s);
                    headings.push((segs.len() - 1, level as u8));
                    heading = Some(OpenHeading { seg: segs.len() - 1, text: String::new(), pieces: Vec::new() });
                }
                Event::End(TagEnd::Heading(_)) => {
                    if let Some(h) = heading.take() {
                        // `text` has no leading or trailing blanks: pulldown-cmark trims headings.
                        // An empty heading (`###`) has no pieces; its anchor is the end of the
                        // `#` run (ATX) or the start of the heading (setext).
                        let r0 = segs[h.seg].range.start;
                        let hashes = text[r0..].bytes().take_while(|&b| b == b'#').count();
                        segs[h.seg].name_anchor = r0 + hashes;
                        segs[h.seg].segment.name = h.text;
                        segs[h.seg].name = Some(h.pieces);
                    }
                    // A heading is a whole line: it ends an item's first line too.
                    if let Some(it) = items.last_mut() { it.done = true; }
                }
                Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) => {
                    let name = info.split_whitespace().next().filter(|s| !s.is_empty()).unwrap_or("code").to_string();
                    segs.push(OutlineSegment::new("code", name, r.clone(), first_line(text, &r)));
                }
                Event::Start(Tag::Table(_)) => {
                    let mut s = OutlineSegment::new("table", "table", r.clone(), r.start..r.start);
                    s.hidden = true;
                    segs.push(s);
                    table = Some(OpenTable { seg: segs.len() - 1, header: vec![], in_head: false, col: 0, cell_text: String::new(), cells: vec![] });
                }
                Event::Start(Tag::TableHead) => if let Some(t) = table.as_mut() { t.in_head = true; t.col = 0; },
                Event::End(TagEnd::TableHead) => if let Some(t) = table.as_mut() { t.in_head = false; },
                Event::Start(Tag::TableRow) => if let Some(t) = table.as_mut() { t.col = 0; },
                Event::Start(Tag::TableCell) => if let Some(t) = table.as_mut() {
                    t.cells.push((t.col, r.clone()));
                    t.cell_text.clear();
                },
                Event::End(TagEnd::TableCell) => if let Some(t) = table.as_mut() {
                    if t.in_head { t.header.push(std::mem::take(&mut t.cell_text).trim().to_string()); }
                    t.col += 1;
                },
                Event::End(TagEnd::Table) => if let Some(t) = table.take() {
                    let tr = segs[t.seg].range.clone();
                    let (l0, l1) = (lines.line_of(tr.start), lines.line_of(tr.end.saturating_sub(1)));
                    for (col, cr) in t.cells {
                        let name = t.header.get(col).cloned().unwrap_or_else(|| format!("column {}", col + 1));
                        let mut s = OutlineSegment::new("column", name, cr, 0..0);
                        s.segment.line = l0;
                        s.segment.end_line = l1;
                        segs.push(s);
                    }
                },
                Event::Start(Tag::Item) => {
                    containers.push(r.clone());
                    if list_items {
                        segs.push(OutlineSegment::new("item", "", r.clone(), first_line(text, &r)));
                        items.push(OpenItem { seg: segs.len() - 1, text: String::new(), done: false });
                    }
                }
                Event::End(TagEnd::Item) if list_items => if let Some(it) = items.pop() {
                    segs[it.seg].segment.name = cut(&it.text, item_chars);
                },
                Event::Text(ref t) | Event::Code(ref t) => {
                    if let Some(h) = heading.as_mut() {
                        // The piece points at the text's own bytes inside the event's range (inline
                        // code ranges include the backticks). When the text is not found verbatim
                        // (entities, escapes), the whole range is used.
                        let src = match text[r.clone()].find(&**t) {
                            Some(off) => r.start + off..r.start + off + t.len(),
                            None => r.clone(),
                        };
                        let at = h.text.len();
                        h.text.push_str(t);
                        h.pieces.push((at..h.text.len(), src));
                    }
                    if let Some(tb) = table.as_mut() { tb.cell_text.push_str(t); }
                    if let Some(it) = items.last_mut() { if !it.done { it.text.push_str(t); } }
                }
                Event::SoftBreak | Event::HardBreak | Event::End(TagEnd::Paragraph) => {
                    // A soft break inside a (setext) heading becomes one space in its name.
                    if let (Some(h), Event::SoftBreak) = (heading.as_mut(), &ev) { h.text.push(' '); }
                    if let Some(it) = items.last_mut() { it.done = true; }
                }
                _ => {}
            }
        }

        // Close heading Sections inside their container (design: spec §5.1, review r1).
        let container_of = |pos: usize| containers.iter().filter(|c| c.start <= pos && pos < c.end).min_by_key(|c| c.end - c.start).cloned();
        let starts: Vec<(usize, u8, Option<Range<usize>>)> = headings.iter().map(|&(i, l)| (i, l, container_of(segs[i].range.start))).collect();
        for (k, (i, lvl, cont)) in starts.iter().enumerate() {
            let limit = cont.as_ref().map(|c| c.end).unwrap_or(text.len());
            let next = starts[k + 1..].iter()
                .find(|(j, l, c)| l <= lvl && c == cont && segs[*j].range.start < limit)
                .map(|(j, _, _)| segs[*j].range.start);
            segs[*i].range.end = next.unwrap_or(limit);
        }
        segs.retain(|s| !(s.segment.kind == "document" && s.segment.name.is_empty()));
        Ok(Outline::build(segs, content))
    }
}
```

Verify against pulldown-cmark
0.13.4 that `Tag::BlockQuote` carries an `Option<BlockQuoteKind>`, that `Tag::Item`'s range
covers nested lists, and that `TableCell` ranges sit between the pipes (a review measured
`| Plan | Price |` at byte 180 giving header cells 181..187 and 188..195). Report differences.

- [ ] **Step 1: fixture** `tests/fixtures/md/guide.md` — exactly these 38 lines (line numbers
  on the left are not part of the file):

```text
 1 ---
 2 title: "Deploy guide"
 3 ---
 4
 5 # Install
 6
 7 Intro paragraph with needle-intro.
 8
 9 ## Linux
10
11 ```bash
12 apt install needle-pkg
13 ```
14
15 - step one
16   - nested needle-item
17 - step two
18
19 ## Plans
20
21 | Plan | Price |
22 |------|-------|
23 | Pro  | needle-10 |
24 | needle-a | needle-b |
25
26 > | Quoted | Cell |
27 > |---|---|
28 > | q | needle-quoted |
29
30 - # Heading in item
31   needle-in-item
32
33 needle-after-list
34
35 Setext Title
36 ============
37
38 needle-setext body
```

- [ ] **Step 2: tests first** (`markdown.rs`). Helper `trail(text_of_match, nth_occurrence)`
  finds the byte of the given text in the fixture (`include_str!("../../tests/fixtures/md/guide.md")`),
  calls `Markdown.trails(&input, &params, &[byte])` and returns `kind:name` strings. With
  `D = document:Deploy guide`, `I = heading:Install`:

| Match text | Expected Trail |
|---|---|
| `needle-intro` | `D`, `I` |
| `needle-pkg` | `D`, `I`, `heading:Linux`, `code:bash` |
| `needle-item` | `D`, `I`, `heading:Linux`, `item:step one` |
| `needle-10` | `D`, `I`, `heading:Plans`, `column:Price` |
| `needle-a` | `D`, `I`, `heading:Plans`, `column:Plan` |
| `needle-b` | `D`, `I`, `heading:Plans`, `column:Price` |
| `needle-quoted` | `D`, `I`, `heading:Plans`, `column:Cell` |
| `needle-in-item` | `D`, `I`, `heading:Plans`, `item:Heading in item`, `heading:Heading in item` |
| `needle-after-list` | `D`, `I`, `heading:Plans` (the heading inside the item does not end Plans) |
| `needle-setext` | `D`, `heading:Setext Title` |
| `Install` on line 5 | `D` (a Match on a label leaves that Segment out) |

Also: with `list_items = false`, `needle-item` loses `item:step one` and `needle-in-item` loses
`item:Heading in item`; invalid UTF-8 gives no Trails; `heading:Linux` has `line` 9 and
`end_line` 18; the `column:Price` Segment of `needle-10` has `line` 21 and `end_line` 24;
`names()` returns exactly `Deploy guide`, `Install`, `Linux`, `Plans`, `Heading in item`,
`Setext Title`, and for each, the concatenated `src[piece]` equals the name (for
`Deploy guide`, without quotes). A separate inline document `First\nSecond\n======\n` gives one
heading named `First Second` with two pieces, text `0..5`/Source `0..5` and text
`6..12`/Source `6..12`, and `source_ranges(6..12)` is `[6..12]` on line 2; `# A \`code\` B` gives
the name `A code B` with the `code` piece pointing inside the backticks; `# &lt; B\n` gives the
name `< B` with pieces text `0..1`/Source `2..6` and text `1..3`/Source `6..8`; `source_ranges`
of `B` (text `2..3`) is `[7..8]` only (the `B`, not the entity), of `<` is `[2..6]` (the whole
entity); an empty hit at 0 maps to `[2..2]`, at the end of the text to `[8..8]`. `# &lt;\n`: `$`
(empty hit at 1) maps to `[6..6]`, the end of the entity. `###\n`: an empty name with no
pieces; `^$` maps to `[3..3]`. All calls go through
`Processor::trails`/`names` with an `Input { bytes, path: Path::new("guide.md"), format: "markdown" }`.

- [ ] **Step 3:** RED, then GREEN; clippy clean. Commit «Add the Markdown Processor».

---

## Chunk 6: Fragments

**Files:** `src/output/fragment.rs`.

Spec §6.3, grill #13 and #14. Widths count `char`s (the display width of wide characters is out
of scope for v0.1). Definitions, fixed here and copied into spec §6.3 by C10:

- **Markers** are `…[+N] ` before and ` […+M]` after; each takes `5 + digits(line length)`
  characters, and both are reserved even when one turns out empty, so every Fragment of a line
  has the same text width.
- **Text width** `inner = width - 2 × marker`, never below 10. With a width too small for 10
  characters plus markers, the rendered Fragment is wider than `width`; that is the defined
  behaviour for tiny widths.
- **Clusters:** Matches are taken in order; a Match joins the current cluster when the cluster,
  extended to its end, still fits in `inner` characters. Otherwise it starts a new cluster.
- Each Fragment highlights **only its own cluster's** Matches, so every Match is shown once.

```rust
//! Fragments of long lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fragment {
    pub text: String,
    /// Byte offset of `text` in the line.
    pub start: usize,
    /// Characters omitted before and after `text`.
    pub omitted_before: usize,
    pub omitted_after: usize,
    /// Byte ranges of this Fragment's own Matches, relative to `text`.
    pub submatches: Vec<(usize, usize)>,
}

impl Fragment {
    /// `…[+N] text […+M]`, without colour.
    pub fn render_plain(&self) -> String {
        let mut s = String::new();
        if self.omitted_before > 0 { s.push_str(&format!("…[+{}] ", self.omitted_before)); }
        s.push_str(&self.text);
        if self.omitted_after > 0 { s.push_str(&format!(" […+{}]", self.omitted_after)); }
        s
    }
}

pub const MIN_TEXT: usize = 10;

/// Splits `line` (valid UTF-8, without terminator) into Fragments around `subs` (byte ranges
/// on char boundaries). Returns None when the line fits in `width` characters.
pub fn fragments(line: &str, subs: &[(usize, usize)], width: usize) -> Option<Vec<Fragment>> {
    let total = line.chars().count();
    if total <= width { return None; }
    let offs: Vec<usize> = line.char_indices().map(|(i, _)| i).chain(std::iter::once(line.len())).collect();
    let to_char = |b: usize| offs.partition_point(|&o| o < b);
    let marker = 5 + total.to_string().len();
    let inner = width.saturating_sub(2 * marker).max(MIN_TEXT);

    // (char start, char end, index into subs); an inverted search has no subs: anchor at 0.
    let mut spans: Vec<(usize, usize, Option<usize>)> = subs.iter().enumerate()
        .map(|(k, &(s, e))| { let cs = to_char(s); (cs, to_char(e).max(cs + 1), Some(k)) }).collect();
    if spans.is_empty() { spans.push((0, 1, None)); }
    spans.sort();
    let mut clusters: Vec<(usize, usize, Vec<usize>)> = Vec::new();
    for (s, e, k) in spans {
        match clusters.last_mut() {
            Some(c) if e.max(c.1) - c.0 <= inner => { c.1 = c.1.max(e); c.2.extend(k); }
            _ => clusters.push((s, e, k.into_iter().collect())),
        }
    }
    Some(clusters.into_iter().map(|(s, e, own)| {
        let (ws, we) = if e - s >= inner {
            (s, (s + inner).min(total))
        } else {
            let pad = (inner - (e - s)) / 2;
            let we = (s.saturating_sub(pad) + inner).min(total);
            (we.saturating_sub(inner), we)
        };
        let (bs, be) = (offs[ws], offs[we]);
        Fragment {
            text: line[bs..be].to_string(),
            start: bs,
            omitted_before: ws,
            omitted_after: total - we,
            submatches: own.iter().map(|&k| subs[k]).filter(|&(a, b)| a < be && b > bs)
                .map(|(a, b)| (a.max(bs) - bs, b.min(be) - bs)).collect(),
        }
    }).collect())
}
```

- [ ] **Step 1: tests first**:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn line_with(len: usize, at: &[(usize, &str)]) -> String {
        let mut s = "x".repeat(len);
        for (i, t) in at { s.replace_range(*i..*i + t.len(), t); }
        s
    }

    #[test]
    fn short_lines_are_untouched() {
        assert!(fragments("short line", &[(0, 5)], 80).is_none());
    }

    #[test]
    fn single_match_is_centred_with_counts() {
        let line = line_with(2006, &[(1000, "NEEDLE")]);
        let f = fragments(&line, &[(1000, 1006)], 80).unwrap();
        let inner = 80 - 2 * (5 + 4);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].text.chars().count(), inner);
        assert_eq!(f[0].omitted_before + inner + f[0].omitted_after, 2006);
        let (a, b) = f[0].submatches[0];
        assert_eq!(&f[0].text[a..b], "NEEDLE");
        assert!(f[0].render_plain().chars().count() <= 80);
    }

    #[test]
    fn matches_that_fit_together_share_a_fragment() {
        // width 100, total 3000 → marker 9, inner 82; 45 characters apart fit.
        let line = line_with(3000, &[(1000, "A"), (1045, "B")]);
        let f = fragments(&line, &[(1000, 1001), (1045, 1046)], 100).unwrap();
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].submatches.len(), 2);
    }

    #[test]
    fn every_match_is_shown_exactly_once() {
        // width 60, total 3000 → inner 42; matches 30 apart form two clusters of two.
        let at = [(1000, "A"), (1030, "B"), (1060, "C"), (1090, "D")];
        let line = line_with(3000, &at);
        let subs: Vec<_> = at.iter().map(|(i, _)| (*i, i + 1)).collect();
        let f = fragments(&line, &subs, 60).unwrap();
        assert_eq!(f.len(), 2);
        let shown: Vec<String> = f.iter().flat_map(|fr| fr.submatches.iter().map(|&(a, b)| fr.text[a..b].to_string())).collect();
        assert_eq!(shown, ["A", "B", "C", "D"]);
    }

    #[test]
    fn far_matches_split() {
        let line = line_with(3000, &[(1000, "A"), (2500, "C")]);
        let f = fragments(&line, &[(1000, 1001), (2500, 2501)], 100).unwrap();
        assert_eq!(f.len(), 2);
    }

    #[test]
    fn tiny_widths_keep_ten_characters_of_text() {
        let line = line_with(500, &[(250, "N")]);
        let f = fragments(&line, &[(250, 251)], 5).unwrap();
        assert_eq!(f[0].text.chars().count(), MIN_TEXT);
        assert!(f[0].text.contains('N'));
    }

    #[test]
    fn edges_and_multibyte() {
        let line = format!("ñ{}", "é".repeat(500));
        let f = fragments(&line, &[(0, 2)], 60).unwrap();
        assert_eq!(f[0].omitted_before, 0);
        assert!(f[0].text.starts_with('ñ'));
        let end = line.len();
        let g = fragments(&line, &[(end - 2, end)], 60).unwrap();
        assert_eq!(g[0].omitted_after, 0);
    }

    #[test]
    fn inverted_search_without_submatches_shows_the_start() {
        let f = fragments(&"z".repeat(500), &[], 60).unwrap();
        assert_eq!((f.len(), f[0].omitted_before, f[0].submatches.len()), (1, 0, 0));
    }
}
```

- [ ] **Step 2:** RED, then GREEN; clippy clean. Commit «Add Fragments for long lines».

---

## Chunk 3: Source and search

### Task 3.1: source.rs — read once, BOM, UTF-16

**Files:** `src/source.rs`, fixtures in `tests/fixtures/misc/`.

```rust
//! A file's bytes, read once and decoded as ripgrep decodes them (design note 1).
use std::path::{Path, PathBuf};

/// Files at least this large are memory-mapped; smaller ones are read. (A memory-mapped file
/// truncated by another process while we search it can crash the process; ripgrep accepts the
/// same risk.)
pub const MMAP_THRESHOLD: u64 = 8 * 1024 * 1024;

enum Data {
    Owned(Vec<u8>),
    Mapped(memmap2::Mmap),
}

pub struct Source {
    /// The path as printed.
    pub path: PathBuf,
    /// Named on the command line (binary policy, design note 3).
    pub explicit: bool,
    data: Data,
    /// Bytes to skip at the start (a UTF-8 BOM).
    skip: usize,
    /// The raw file started with a BOM (UTF-8 or UTF-16). grep-searcher then searches with a
    /// reader even when memory maps are on (searcher/mod.rs:781), so C9 does the same.
    pub had_bom: bool,
}

impl Source {
    pub fn open(path: &Path, explicit: bool) -> std::io::Result<Self> {
        let file = std::fs::File::open(path)?;
        let len = file.metadata()?.len();
        let data = if len >= MMAP_THRESHOLD {
            // SAFETY: read-only map; see MMAP_THRESHOLD about concurrent truncation.
            Data::Mapped(unsafe { memmap2::Mmap::map(&file)? })
        } else {
            let mut v = Vec::with_capacity(len as usize);
            std::io::Read::read_to_end(&mut &file, &mut v)?;
            Data::Owned(v)
        };
        Ok(Self::decode(path.to_path_buf(), explicit, data))
    }

    /// For stdin and, from M4, Container Members.
    pub fn from_bytes(path: PathBuf, bytes: Vec<u8>, explicit: bool) -> Self {
        Self::decode(path, explicit, Data::Owned(bytes))
    }

    fn decode(path: PathBuf, explicit: bool, data: Data) -> Self {
        let raw: &[u8] = match &data { Data::Owned(v) => v, Data::Mapped(m) => m };
        if raw.starts_with(b"\xEF\xBB\xBF") {
            return Self { path, explicit, data, skip: 3, had_bom: true };
        }
        let utf16 = if raw.starts_with(b"\xFF\xFE") { Some(encoding_rs::UTF_16LE) }
                    else if raw.starts_with(b"\xFE\xFF") { Some(encoding_rs::UTF_16BE) } else { None };
        if let Some(enc) = utf16 {
            let (text, _) = enc.decode_without_bom_handling(&raw[2..]);
            return Self { path, explicit, data: Data::Owned(text.into_owned().into_bytes()), skip: 0, had_bom: true };
        }
        Self { path, explicit, data, skip: 0, had_bom: false }
    }

    /// The decoded bytes every stage works on.
    pub fn bytes(&self) -> &[u8] {
        let raw: &[u8] = match &self.data { Data::Owned(v) => v, Data::Mapped(m) => m };
        &raw[self.skip..]
    }

    /// The first 512 bytes, for the Detector.
    pub fn head(&self) -> &[u8] {
        let b = self.bytes();
        &b[..b.len().min(512)]
    }
}
```

Fixtures (generate them with a small script in the chunk and commit the bytes):
`misc/bom.md` = UTF-8 BOM + `# T\nneedle\n`; `misc/utf16.md` = UTF-16LE with BOM of
`# T\nneedle\n`; `misc/bin.dat` = `needle first\nmore\0binary needle\n`;
`misc/invalid.txt` = `caf\xe9 needle\n`; `misc/late-nul.txt` = `needle early\n`, then
70 000 bytes of `x` lines, then `\0needle late\n` (a NUL after the first 64 KiB); `misc/crlf.txt` = `a\r\nb\n`; `misc/noeol.txt` = `a`
(no newline); `misc/bom-bin.dat` = UTF-8 BOM + `needle first\nmore\0binary needle\n`;
`misc/anchor.txt` = `bar\nfoo needle\nneedle\n`; `misc/min.json` = one line of 20 000 characters with `needle` at offsets 1000,
1050 and 15000 (used by C9).

- [ ] **Step 1: tests first**: `bom.md` → `bytes()` starts with `# T`; `utf16.md` →
  `bytes() == b"# T\nneedle\n"`; a 9 MiB tempfile is mapped and `bytes().len()` is its size;
  `from_bytes` with a UTF-8 BOM strips it; `head()` of a 2-byte file is 2 bytes.
- [ ] **Step 2:** RED, GREEN. Commit «Add Source».

### Task 3.2: search.rs — matcher, searcher, walker

**Files:** `src/search.rs`.

```rust
//! Walking and searching with ripgrep's crates (ADR 0001, design notes 3 and 4).
use crate::cli::{Args, Case};
// (BinaryMode is referenced by path in `searcher`.)
use crate::model::{ContextLine, FileMatches, FileStats, Item, LineMatch, Submatch};
use crate::source::Source;
use anyhow::Result;
use grep_matcher::Matcher;
use grep_regex::{RegexMatcher, RegexMatcherBuilder};
use grep_searcher::{BinaryDetection, Searcher, SearcherBuilder, Sink, SinkContext, SinkFinish, SinkMatch};
use ignore::{overrides::OverrideBuilder, types::TypesBuilder, WalkBuilder};
use std::path::{Path, PathBuf};

pub fn matcher(args: &Args, patterns: &[String]) -> Result<RegexMatcher> {
    let mut b = RegexMatcherBuilder::new();
    match args.case() {
        Case::Insensitive => { b.case_insensitive(true); }
        Case::Smart => { b.case_smart(true); }
        Case::Sensitive => {}
    }
    b.word(args.word_regexp)
        .whole_line(args.line_regexp)
        .fixed_strings(args.fixed_strings && !args.no_fixed_strings)
        .multi_line(true) // as ripgrep: ^ and $ match at line boundaries inside a buffer
        .line_terminator(Some(b'\n'));
    // An empty pattern set never reaches here: C9 exits 1 before searching.
    Ok(b.build_many(patterns)?)
}

/// ripgrep 14.1.1's memory-map choice, which is also its in-memory vs reader choice: the
/// search runs in memory only when every path given is an existing regular file (no
/// directory, no stdin) and there are at most 10 of them (`--mmap`/`--no-mmap` are not in
/// M1). The chunk verifies this rule by behaviour: C9 compares `--json` on `misc/bin.dat` as one
/// explicit file (rg: second Match on line 2, offset 13), walked inside `misc`, as 11 explicit
/// files, and through stdin (line 3, offset 18); adjust the rule to what rg does and say so.
pub fn rg_in_memory(paths: &[PathBuf], stdin: bool) -> bool {
    !stdin && !paths.is_empty() && paths.len() <= 10 && paths.iter().all(|p| p.is_file())
}

/// `explicit`: the file was named on the command line, or is stdin (design note 3).
/// `decoded`: the input is a Source (BOM already handled: sniffing off). Passthrough and the
/// quiet fast path search raw files and stdin, so they pass `decoded = false` and keep
/// grep-searcher's BOM sniffing, as ripgrep.
pub fn searcher(args: &Args, explicit: bool, decoded: bool) -> Searcher {
    let binary = match args.binary_mode(explicit) {
        crate::cli::BinaryMode::Text => BinaryDetection::none(),
        crate::cli::BinaryMode::Binary => BinaryDetection::convert(b'\0'),
        crate::cli::BinaryMode::Quit => BinaryDetection::quit(b'\0'),
    };
    SearcherBuilder::new()
        .line_number(true) // Trails need line numbers even when they are not printed.
        .before_context(args.before())
        .after_context(args.after())
        .invert_match(args.invert_match)
        .max_matches(args.max_count)
        .binary_detection(binary)
        .bom_sniffing(!decoded)
        .build()
}

/// `cwd` anchors the globs, as ripgrep does (`-g 'docs/*.md' x docs` works).
pub fn walker(args: &Args, paths: &[PathBuf], cwd: &Path) -> Result<WalkBuilder> {
    let first = paths.first().cloned().unwrap_or_else(|| PathBuf::from("."));
    let mut w = WalkBuilder::new(&first);
    for p in paths.iter().skip(1) { w.add(p); }
    let no_ignore = args.no_ignore();
    w.hidden(!args.hidden())
        .ignore(!no_ignore).git_ignore(!no_ignore).git_global(!no_ignore).git_exclude(!no_ignore)
        .parents(!no_ignore)
        .follow_links(args.follow && !args.no_follow)
        .max_depth(args.max_depth)
        .skip_stdout(true);
    if !no_ignore { w.add_custom_ignore_filename(".rgignore"); }
    // As ripgrep 14.1.1: every -g first, then every --iglob (case-insensitive); the relative
    // order of the two kinds on the command line does not matter (measured in review r2).
    let mut ob = OverrideBuilder::new(cwd);
    for g in &args.globs { ob.add(g)?; }
    if !args.iglobs.is_empty() {
        ob.case_insensitive(true)?;
        for g in &args.iglobs { ob.add(g)?; }
    }
    w.overrides(ob.build()?);
    let mut tb = TypesBuilder::new();
    tb.add_defaults();
    for t in &args.types { tb.select(t); }
    for t in &args.types_not { tb.negate(t); }
    w.types(tb.build()?);
    if args.sort.as_deref() == Some("path") { w.sort_by_file_path(|a, b| a.cmp(b)); }
    if let Some(j) = args.threads { w.threads(j); }
    Ok(w)
}
```



### Task 3.3: CollectSink — the model and the stats

```rust
pub struct CollectSink<'m> {
    matcher: &'m RegexMatcher,
    invert: bool,
    /// `-q` without structural flags: stop the file at its first Match (design note 10).
    quiet: bool,
    pub items: Vec<Item>,
    pub stats: FileStats,
}

impl<'m> CollectSink<'m> {
    pub fn new(matcher: &'m RegexMatcher, invert: bool, quiet: bool) -> Self {
        Self { matcher, invert, quiet, items: Vec::new(), stats: FileStats::default() }
    }

    /// Exactly grep-printer 0.3.1's JSON sink (`record_matches`, json.rs:645–680, called at
    /// json.rs:729 with the Match's buffer and range, and `find_iter_at_in_context`,
    /// util.rs:473–520), copied because those helpers are `pub(crate)`: take `buffer` up to the
    /// end of `range` minus its `\n` (a `\r` stays; CRLF mode is not in M1), run
    /// `matcher.find_iter_at(that, range.start, …)` stopping at the first match that starts at
    /// or after `range.end`, make offsets relative to `range.start`, and drop a final empty
    /// match at the end of the line's bytes. Searching inside the buffer keeps `\A` and
    /// look-behind seeing what ripgrep's printer sees (`bar\nfoo needle\n` with
    /// `\Afoo|needle` reports only `needle`).
    fn submatches(&self, buffer: &[u8], range: std::ops::Range<usize>) -> Vec<Submatch>;
}

impl Sink for CollectSink<'_> {
    type Error = std::io::Error;

    fn matched(&mut self, _s: &Searcher, m: &SinkMatch<'_>) -> Result<bool, Self::Error> {
        let bytes = m.bytes().to_vec();
        // grep-printer records matches on inverted lines too (none are found there).
        let subs = if self.invert { Vec::new() } else { self.submatches(m.buffer(), m.bytes_range_in_buffer()) };
        self.stats.matched_lines += 1;
        self.stats.matches += subs.len() as u64;
        self.items.push(Item::Match(LineMatch {
            line_number: m.line_number().unwrap_or(0),
            absolute_offset: m.absolute_byte_offset(),
            bytes,
            submatches: subs,
        }));
        Ok(!self.quiet)
    }

    fn context(&mut self, _s: &Searcher, c: &SinkContext<'_>) -> Result<bool, Self::Error> {
        let bytes = c.bytes().to_vec();
        // grep-printer's context path searches the line's own bytes (range 0..len).
        let subs = if self.invert { self.submatches(&bytes, 0..bytes.len()) } else { Vec::new() };
        self.items.push(Item::Context(ContextLine {
            line_number: c.line_number().unwrap_or(0),
            absolute_offset: c.absolute_byte_offset(),
            bytes,
            submatches: subs,
        }));
        Ok(true)
    }

    fn binary_data(&mut self, _s: &Searcher, _offset: u64) -> Result<bool, Self::Error> {
        self.stats.binary_cut.get_or_insert(self.items.len());
        Ok(true)
    }

    fn finish(&mut self, _s: &Searcher, f: &SinkFinish) -> Result<(), Self::Error> {
        self.stats.bytes_searched = f.byte_count();
        self.stats.binary_offset = f.binary_byte_offset();
        Ok(())
    }
}

/// Searches a Source. Returns None when nothing matched (stats are discarded: ripgrep's
/// summary counts only files with Matches, verified with rg 14.1.1).
///
/// The search path decides how `BinaryDetection::convert` behaves: the reader path converts NUL
/// bytes into line terminators, the in-memory path does not
/// (grep-searcher-0.1.17/src/searcher/mod.rs:83), and line numbers and offsets differ
/// accordingly. ripgrep 14.1.1 searches memory-mapped (in memory) only in the cases of
/// `in_memory`, and with a reader otherwise; we do the same on the Source's bytes, so our
/// coordinates are ripgrep's. When the result has a binary offset, C9 skips the Processor.
pub fn search_source(searcher: &mut Searcher, matcher: &RegexMatcher, src: &Source, in_memory: bool, quiet: bool) -> Result<Option<FileMatches>> {
    let started = std::time::Instant::now();
    let mut sink = CollectSink::new(matcher, searcher.invert_match(), quiet);
    if in_memory {
        searcher.search_slice(matcher, src.bytes(), &mut sink)?;
    } else {
        searcher.search_reader(matcher, std::io::Cursor::new(src.bytes()), &mut sink)?;
    }
    if !sink.items.iter().any(|i| matches!(i, Item::Match(_))) { return Ok(None); }
    sink.stats.elapsed = started.elapsed();
    Ok(Some(FileMatches { path: src.path.clone(), explicit: src.explicit, items: sink.items, stats: sink.stats }))
}
```

`stats.matches` adds the number of recorded submatches and `matched_lines` the lines of the
Match, as grep-printer's JSON sink does (json.rs:736–737). If `Searcher::invert_match()` is not
a public getter, pass `args.invert_match` in instead.

**Streaming for `-q` and for passthrough.** `search_source` needs the whole Source because
Trails need it. Two paths never build one: passthrough (C9 step 5) and `-q` without structural
flags (C9 step 10). They use `searcher(args, explicit, false)` (BOM sniffing on), configured
with `memory_map(MmapChoice::auto())` when `rg_in_memory` says so (an `unsafe` call, with the
same truncation caveat as `MMAP_THRESHOLD`), and search files with `searcher.search_path` and
stdin with `searcher.search_reader(matcher, std::io::stdin().lock(), …)`, so `sfor -q needle`
returns as soon as a Match is read, before stdin reaches EOF, as ripgrep does.

- [ ] **Step 1: tests first** (`src/search.rs`):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::parse;
    use std::ffi::OsString;

    fn args(v: &[&str]) -> Args { parse(v.iter().map(OsString::from).collect()).unwrap() }
    fn fixture(p: &str) -> PathBuf { Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(p) }

    fn run(a: &Args, pats: &[&str], src: &Source) -> Option<FileMatches> {
        let pats: Vec<String> = pats.iter().map(|s| s.to_string()).collect();
        search_source(&mut searcher(a, src.explicit, true), &matcher(a, &pats).unwrap(), src, false, false).unwrap()
    }

    #[test]
    fn match_and_context_with_offsets() {
        let src = Source::from_bytes("a.md".into(), b"one\ntwo needle\nthree\n".to_vec(), false);
        let fm = run(&args(&["sfor", "-C1", "x"]), &["needle"], &src).unwrap();
        assert_eq!(fm.items.len(), 3);
        let Item::Match(lm) = &fm.items[1] else { panic!() };
        assert_eq!((lm.line_number, lm.absolute_offset, lm.submatches[0].start), (2, 4, 4));
        assert_eq!(lm.anchors(), vec![8]);
        assert_eq!((fm.stats.matched_lines, fm.stats.bytes_searched), (1, 21));
        assert!(run(&args(&["sfor", "x"]), &["absent"], &src).is_none());
    }

    #[test]
    fn offsets_are_after_the_bom_and_utf16_is_decoded() {
        for f in ["misc/bom.md", "misc/utf16.md"] {
            let src = Source::open(&fixture(f), true).unwrap();
            let fm = run(&args(&["sfor", "x"]), &["needle"], &src).unwrap();
            let Item::Match(lm) = &fm.items[0] else { panic!() };
            assert_eq!((lm.line_number, lm.absolute_offset), (2, 4), "{f}");
        }
    }

    #[test]
    fn binary_policy_explicit_vs_walked() {
        let walked = Source::open(&fixture("misc/bin.dat"), false).unwrap();
        assert!(run(&args(&["sfor", "x"]), &["binary"], &walked).is_none());
        let explicit = Source::open(&fixture("misc/bin.dat"), true).unwrap();
        let fm = run(&args(&["sfor", "x"]), &["binary"], &explicit).unwrap();
        assert!(fm.stats.binary_offset.is_some());
    }

    #[test]
    fn submatches_as_ripgrep_records_them() {
        let a = args(&["sfor", "x"]);
        let crlf = Source::from_bytes("a".into(), b"a\r\n".to_vec(), false);
        let Item::Match(m) = &run(&a, &["\\r"], &crlf).unwrap().items[0] else { panic!() };
        assert_eq!(m.submatches, vec![Submatch { start: 1, end: 2 }]);
        let Item::Match(m) = &run(&a, &["$"], &crlf).unwrap().items[0] else { panic!() };
        assert_eq!(m.submatches, vec![Submatch { start: 2, end: 2 }]);
        let eof = Source::from_bytes("a".into(), b"a".to_vec(), false);
        let Item::Match(m) = &run(&a, &["$"], &eof).unwrap().items[0] else { panic!() };
        assert!(m.submatches.is_empty());
    }

    #[test]
    fn submatches_see_the_buffer_like_ripgrep() {
        let src = Source::from_bytes("a".into(), b"bar\nfoo needle\n".to_vec(), false);
        let fm = run(&args(&["sfor", "x"]), &["\\Afoo|needle"], &src).unwrap();
        let Item::Match(m) = &fm.items[0] else { panic!() };
        assert_eq!(m.submatches, vec![Submatch { start: 4, end: 10 }]);
        let anchored = Source::from_bytes("a".into(), b"x\nneedle\n".to_vec(), false);
        let fm = run(&args(&["sfor", "x"]), &["^needle"], &anchored).unwrap();
        let Item::Match(m) = &fm.items[0] else { panic!() };
        assert_eq!(m.submatches, vec![Submatch { start: 0, end: 6 }]);
    }

    #[test]
    fn explicit_file_in_memory_keeps_the_nul() {
        // rg 14.1.1 on one explicit file (memory-mapped): second match on line 2, offset 13.
        let src = Source::from_bytes("bin.dat".into(), b"needle first\nmore\0binary needle\n".to_vec(), true);
        let a = args(&["sfor", "x"]);
        let fm = search_source(&mut searcher(&a, true, true), &matcher(&a, &["needle".into()]).unwrap(), &src, true, false).unwrap().unwrap();
        assert_eq!(fm.matches().map(|m| (m.line_number, m.absolute_offset)).last(), Some((2, 13)));
    }

    #[test]
    fn binary_stdin_coordinates_follow_nul_conversion() {
        // rg 14.1.1: `printf 'needle first\nmore\0binary needle\n' | rg --json needle` reports
        // the second match on line 3, absolute offset 18.
        let src = Source::from_bytes("<stdin>".into(), b"needle first\nmore\0binary needle\n".to_vec(), true);
        let fm = run(&args(&["sfor", "x"]), &["needle"], &src).unwrap();
        let lines: Vec<(u64, u64)> = fm.matches().map(|m| (m.line_number, m.absolute_offset)).collect();
        assert_eq!(lines.last(), Some(&(3, 18)));
        assert!(fm.stats.binary_offset.is_some());
    }

    #[test]
    fn quiet_stops_at_the_first_match() {
        let src = Source::from_bytes("a".into(), b"needle\nneedle\n".to_vec(), false);
        let a = args(&["sfor", "x"]);
        let fm = search_source(&mut searcher(&a, false, true), &matcher(&a, &["needle".into()]).unwrap(), &src, false, true).unwrap().unwrap();
        assert_eq!(fm.matches().count(), 1);
    }

    #[test]
    fn inverted_context_lines_carry_submatches() {
        let src = Source::from_bytes("a".into(), b"keep\nneedle\n".to_vec(), false);
        let fm = run(&args(&["sfor", "-v", "-C1", "x"]), &["needle"], &src).unwrap();
        let Item::Context(c) = &fm.items[1] else { panic!("{:?}", fm.items) };
        assert_eq!(c.submatches.len(), 1);
    }

    fn walk_names(a: &Args, paths: &[PathBuf], cwd: &Path) -> Vec<String> {
        let mut v: Vec<String> = walker(a, paths, cwd).unwrap().build().flatten()
            .filter(|e| e.file_type().is_some_and(|t| t.is_file()))
            .map(|e| e.path().strip_prefix(cwd).unwrap_or(e.path()).to_string_lossy().into_owned()).collect();
        v.sort();
        v
    }

    #[test]
    fn walker_respects_gitignore_and_rgignore() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir(d.path().join(".git")).unwrap();
        std::fs::write(d.path().join(".gitignore"), "skip.md\n").unwrap();
        std::fs::write(d.path().join(".rgignore"), "also.md\n").unwrap();
        for n in ["keep.md", "skip.md", "also.md"] { std::fs::write(d.path().join(n), "x").unwrap(); }
        assert_eq!(walk_names(&args(&["sfor", "x"]), &[d.path().into()], d.path()), ["keep.md"]);
    }

    #[test]
    fn a_glob_whitelists_over_gitignore_as_in_ripgrep() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir(d.path().join(".git")).unwrap();
        std::fs::write(d.path().join(".gitignore"), "skip.md\n").unwrap();
        for n in ["keep.md", "skip.md", "other.txt"] { std::fs::write(d.path().join(n), "x").unwrap(); }
        assert_eq!(walk_names(&args(&["sfor", "-g", "*.md", "x"]), &[d.path().into()], d.path()), ["keep.md", "skip.md"]);
    }

    #[test]
    fn iglobs_go_after_globs_whatever_the_order() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir(d.path().join(".git")).unwrap();
        std::fs::write(d.path().join(".gitignore"), "skip.md\n").unwrap();
        for n in ["keep.md", "skip.md"] { std::fs::write(d.path().join(n), "x").unwrap(); }
        for argv in [&["sfor", "--iglob", "*.MD", "-g", "!skip.md", "x"][..], &["sfor", "-g", "!skip.md", "--iglob", "*.MD", "x"]] {
            assert_eq!(walk_names(&args(argv), &[d.path().into()], d.path()), ["keep.md", "skip.md"], "{argv:?}");
        }
    }

    #[test]
    fn globs_are_relative_to_cwd() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir(d.path().join("sub")).unwrap();
        std::fs::write(d.path().join("sub/x.md"), "x").unwrap();
        let a = args(&["sfor", "-g", "sub/*.md", "x"]);
        assert_eq!(walk_names(&a, &[d.path().join("sub")], d.path()), ["sub/x.md"]);
    }
}
```

`a_glob_whitelists_over_gitignore_as_in_ripgrep` records ripgrep's documented precedence
(`ignore-0.4.33/src/dir.rs:511`: overrides win over ignore files); C9's compat test confirms it
against `rg`.

- [ ] **Step 2:** RED, GREEN; clippy clean. Commit «Add walking and searching».

---

## Chunk 7: Printers

**Files:** `src/output/{mod,style,tree,flat,json,summary}.rs`. Reads `model::Described` and the
Source bytes (for `--expand`); uses `output::fragment` (C6) and `text` (C1). Every printer
writes into a `&mut dyn termcolor::WriteColor` (C9 passes a `termcolor::Buffer` per file, which
handles Windows consoles; tests pass `termcolor::NoColor<Vec<u8>>` or `termcolor::Ansi<Vec<u8>>`).

### Task 7.1: mod.rs and style.rs

```rust
//! Rendering of Described files (spec §6).
pub mod flat;
pub mod fragment;
pub mod json;
pub mod style;
pub mod summary;
pub mod tree;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout { Tree, Flat, Json }

#[derive(Debug, Clone)]
pub struct RenderOptions {
    pub layout: Layout,
    /// Show line numbers (tree and flat: on unless -N; JSON: `line_number` null with -N).
    pub line_numbers: bool,
    /// Show file names in the flat layout (-H / -I / ripgrep's default).
    pub with_filename: bool,
    /// Total characters available per output line: the terminal width on a TTY,
    /// `--fragment-width` otherwise. Printers subtract their own prefix (design note on C6).
    pub width: usize,
    /// -M: ripgrep's omission messages instead of Fragments.
    pub max_columns: Option<u64>,
    /// Context is on (-A/-B/-C > 0): print `--` between non-adjacent groups, as ripgrep.
    pub context: bool,
    /// --expand
    pub expand: bool,
}
```

`style.rs`:

```rust
use termcolor::{Color, ColorSpec, WriteColor};

/// ripgrep's default colours (path magenta, line number green, match bold red), plus cyan
/// Trails and dim markers (grill decision 8381).
pub fn spec(kind: &str) -> ColorSpec {
    let mut c = ColorSpec::new();
    match kind {
        "path" => { c.set_fg(Some(Color::Magenta)); }
        "line" => { c.set_fg(Some(Color::Green)); }
        "match" => { c.set_fg(Some(Color::Red)).set_bold(true); }
        "trail" => { c.set_fg(Some(Color::Cyan)); }
        _ => { c.set_dimmed(true); }
    }
    c
}

pub fn write_styled(w: &mut dyn WriteColor, kind: &str, s: &str) -> std::io::Result<()> {
    w.set_color(&spec(kind))?;
    w.write_all(s.as_bytes())?;
    w.reset()
}

/// Writes `text`, with each byte range of `subs` in the match colour. Ranges are clamped to
/// `text` and moved to char boundaries.
pub fn write_highlighted(w: &mut dyn WriteColor, text: &str, subs: &[(usize, usize)]) -> std::io::Result<()>;
```

**Tests first:** `write_highlighted` into `termcolor::NoColor<Vec<u8>>` returns the text
unchanged; into `termcolor::Ansi<Vec<u8>>` with `("a NEEDLE b", [(2, 8)])` wraps `NEEDLE` in
the bold red escape and resets after it; ranges past the end or inside a multi-byte char do not
panic.

### Task 7.2: the line renderer

In `mod.rs`:

```rust
/// Writes the text of one line for tree and flat: whole, as Fragments (one per output line,
/// each preceded by `prefix`), or ripgrep's omission message with -M.
/// `bytes` is the raw line; `subs` are byte ranges in it; `avail` is the width left after the
/// caller's prefix.
pub fn write_line(w: &mut dyn WriteColor, prefix: &dyn Fn(&mut dyn WriteColor) -> std::io::Result<()>,
                  bytes: &[u8], subs: &[(usize, usize)], is_match: bool, avail: usize,
                  o: &RenderOptions) -> std::io::Result<()>;
```

1. `-M`: copy the exact rule of `grep-printer-0.3.1/src/standard.rs` around line 1329 (which
   length is compared, terminator included or not, and which of `[Omitted long matching line]`,
   `[Omitted long line with N matches]`, `[Omitted long context line]` is printed, depending on
   colour and on whether submatches were computed). C9 compares tree/flat output against
   `rg -M` for matching and context lines, with colour off and on.
2. Otherwise convert with `text::lossy_with_map` and remap `subs` through the map, so invalid
   UTF-8 never shifts the highlight.
3. `fragment::fragments(text, subs, avail)`: None → write the line highlighted; Some → one
   output line per Fragment: `prefix`, dim `…[+N] `, highlighted text, dim ` […+M]`.

### Task 7.3: tree.rs

```rust
pub fn render(d: &Described, src: &[u8], o: &RenderOptions, w: &mut dyn WriteColor) -> std::io::Result<()>;
```

Spec §6.1. Layout, two spaces of indentation per depth:

```
<path>
  <segment depth 0>
    <segment depth 1>
      <line>:<text>
      <line>-<context>
    --
```

- Segment labels: `heading` → `#` × level + space + name; `code` → ```` ```name ````;
  `item` → `• name`; `column` → `column «name»`; `document` → the name; other kinds →
  `kind name`. A Segment with `signature` (code, M2) prints the signature instead.
- **Per-submatch Trails** (design note 5): a Match line whose submatches have different Trails
  is printed once per distinct Trail, in submatch order, each time highlighting only the
  submatches of that Trail. Equal consecutive Trails are one group.
- Only the Segments that differ from the previously printed Trail are printed (shared prefix
  once). An empty Trail prints the line at depth 0.
- Context lines use `-` after the line number, Matches `:`, both without a space, as ripgrep.
  When `o.context` and two consecutive items are not adjacent lines, print `--` at the depth of
  the next item. Line numbers are omitted with `-N`.
- Width for a line at depth `n`: `o.width.saturating_sub(2 × (n + 1) + len(line number
  prefix))`; C6 turns anything below its minimum into a 10-character Fragment text. No width
  arithmetic in any printer may underflow (`saturating_sub` everywhere).
- `--expand`: every distinct Section of the file's submatches (`d.sections[i][k]`) is printed
  once, under the Trail of the first submatch that has it, in file order: the Section's lines
  from `src` (lines with kept Matches with `:` and highlighted, the rest with `-`). A Match
  whose Sections were all printed already prints nothing more.
- Binary files (design note 3): the items before `binary_cut`, then the binary message at
  depth 0 under the path.
- A blank line between files (C9 writes it between buffers).

**Tests first** (snapshots, hand-built `Described` values): two Matches under `# A › ## B`;
then one under `# A` only (prints nothing new for `# A`); context lines and a `--` gap; a long
line as two Fragments; one table row with two submatches in different columns printed twice;
`--expand` with two Matches in one Section prints it once; one line whose two submatches have
different Sections prints both Sections; `-N`; the binary message; `--fragment-width 5` with a
four-Segment Trail renders without panicking.

### Task 7.4: flat.rs

```rust
pub fn render(d: &Described, src: &[u8], o: &RenderOptions, w: &mut dyn WriteColor) -> std::io::Result<()>;
```

Spec §6.2: `path:line: Trail │ text`, Trail joined with ` › ` in cyan, `│` dimmed. No Trail →
`path:line: text`. `-I` drops `path:`; `-N` drops `line:`. Context lines: `path-line-text`,
as ripgrep. `--` between non-adjacent groups when context is on. Per-submatch Trails: one
output line per distinct Trail of a Match line, as in the tree. Fragments: one output line per
Fragment, same prefix; the space left for the text is `o.width.saturating_sub(prefix width)`.
`--expand`: for every distinct Section (per submatch, as in the tree), a header line
`path:start-end: Trail`, the Section's lines verbatim (Matches highlighted), then an empty
line. When the whole line
(prefix + Trail + ` │ ` + text) is wider than `o.width`, the Trail is shortened first by
replacing `item` Segments with `…` (spec §5.1), then, if still too wide, by keeping its first
and last Segments with `…` in between; the text then becomes Fragments as needed. Binary
files (design note 3): the items before `binary_cut`, then the message with `path: ` in front
when file names are shown.

**Tests first** (snapshots): plain Match; no Trail; `-I -N`; context with `--`; Fragment; two
Trails on one line; elided long Trail (items collapse first, then the middle); `--expand`; binary message; a prefix wider than
`o.width` does not panic.

### Task 7.5: json.rs — ripgrep's JSON Lines from the model

```rust
/// Run-wide totals for the final `summary` message. Lives here; C9 accumulates it.
#[derive(Debug, Clone, Default)]
pub struct RunStats {
    pub elapsed_total: std::time::Duration,
    pub elapsed: std::time::Duration,
    pub searches: u64,
    pub searches_with_match: u64,
    pub bytes_searched: u64,
    pub bytes_printed: u64,
    pub matched_lines: u64,
    pub matches: u64,
}

pub struct JsonOptions {
    pub line_numbers: bool,
    pub width: usize,
    /// --no-trail never reaches this printer (C9 uses grep-printer's JSON); kept for clarity.
    pub expand: bool,
}

/// Writes begin, match and context messages, and end, for one Described file, in ripgrep's
/// format, adding `format`, `trail`, `fragment` (and `section` with --expand) to each match.
/// `bytes_printed` in the `end` stats is, as in grep-printer (json.rs:815), the bytes written
/// for this file **before** the `end` message. Returns all bytes written, `end` included.
pub fn render(d: &Described, src: &[u8], o: &JsonOptions, w: &mut dyn std::io::Write) -> std::io::Result<u64>;

/// ripgrep's final message, keys in ripgrep's order.
pub fn summary(s: &RunStats) -> String;
```

Rules:

- Message shapes and key order are grep-printer 0.3.1's: read `json.rs` (the `Begin`,
  `Match`, `Context`, `End` serialisation and the `{"text": …}` / `{"bytes": base64}` rule for
  `path`, `lines` and submatch `match`) and produce the same bytes, so that removing our fields
  and the elapsed values gives `rg --json`'s output exactly (C9 test). Use serde_json with
  `preserve_order` or write the JSON by hand.
- Our fields go at the end of `data`, in this order: `"format"` (string or null), `"trail"`
  (the Trail of the first submatch, or of the line for `-v`), `"fragment"` (null, or
  `{"text","start","omitted_before","omitted_after"}` of the line's first Fragment at
  `o.width`), and with `--expand` `"section"` with `start_line`, `end_line` and `text` on
  **every** match message (spec §6.4), for the first submatch. When submatches of one line have
  different Trails, each submatch object also gets `"trail"`, and with `--expand` its own
  `"section"` when it differs from the line's (spec §6.4 is amended by C10 to say so).
- `line_number` is `null` when `o.line_numbers` is false (`-N`), as ripgrep.
- `end.data.stats` comes from `d.file.stats`; `elapsed` objects are
  `{"secs","nanos","human"}` with `human` formatted like ripgrep (`"0.000028s"`).
- `summary` is exactly, with values filled in:
  `{"data":{"elapsed_total":{"human":…,"nanos":…,"secs":…},"stats":{"bytes_printed":…,"bytes_searched":…,"elapsed":{"human":…,"nanos":…,"secs":…},"matched_lines":…,"matches":…,"searches":…,"searches_with_match":…}},"type":"summary"}`
  (measured with rg 14.1.1). Its totals add the `end` stats of printed files only: ripgrep's
  summary counts only files with Matches (`rg --json needle a.md nomatch.txt` →
  `"searches":1`).

**Tests first:** a Described with two Matches (one a table row with two Trails) and one
context line: every line parses as JSON; `type` sequence is `begin, match, context, match,
end`; ripgrep's keys come first and in grep-printer's order; `trail` is last-but-one before
`fragment`; the two-Trail row carries `submatches[k].trail`; `-N` gives `"line_number":null`;
`summary` of two `RunStats` adds counters and matches the literal key order; `end`'s
`bytes_printed` equals the length of the begin/match/context lines written before it (an
assertion computed independently from the output, not from the function's return).

### Task 7.6: summary.rs — -c, --count-matches, -l, --files-without-match after filters

Used only when a structural flag is present (design note 9); without one, C9 runs
grep-printer's `Summary`. Same output as ripgrep: `path:N` (`N` alone without file names) for
`-c` (kept lines) and `--count-matches` (kept submatches), `path` for `-l`, and for
`--files-without-match` every searched file that ends with no kept Match — including files the
search found nothing in and files whose Matches were all filtered out (C9 passes both kinds).

**Tests first:** each mode with and without file names.

- [ ] Commits: «Add colours and the line renderer», «Add the tree printer», «Add the one-line
  printer», «Add the JSON printer», «Add summary output after filters».

---

## Chunk 8: Filters

**Files:** `src/filter.rs`.

```rust
//! --within and the reshaping of a file's items after filtering (spec §8, grill #4).
use crate::model::{ContextLine, Described, Item, Trail};

/// "Install › Linux", "Install > Linux" and "Install/Linux" → ["install", "linux"].
pub fn parse_within(s: &str) -> Vec<String> {
    s.split(['›', '>', '/']).map(|p| p.trim().to_lowercase()).filter(|p| !p.is_empty()).collect()
}

/// True when the Segment names contain every wanted name, in order, each as a
/// case-insensitive substring; gaps are allowed ("install › bash" matches
/// Install › Linux › bash).
pub fn within(trail: &Trail, wanted: &[String]) -> bool {
    let mut it = trail.iter();
    wanted.iter().all(|w| it.any(|s| s.name.to_lowercase().contains(w.as_str())))
}

/// Applies per-submatch decisions. `keep[i][k]` is for submatch `k` of Match item `i`
/// (one entry for an inverted Match). A Match keeps only its kept submatches and their Trails;
/// a Match with none kept becomes a Context line when it lies within `before`/`after` lines of
/// a kept Match, and is dropped otherwise. Context lines stay only near a kept Match.
pub fn retain(d: &mut Described, keep: &[Vec<bool>], before: usize, after: usize);

/// --expand: per submatch, the Section of the last Segment of its Trail, or None for an
/// empty Trail. Aligned with `d.trails`.
pub fn sections(d: &Described) -> Vec<Vec<Option<(u64, u64)>>> {
    d.trails.iter().map(|ts| ts.iter().map(|t| t.last().map(|s| (s.line, s.end_line))).collect()).collect()
}
```

`retain` rebuilds `d.file.items` and `d.trails` together, keeping them aligned. The lines of
kept Matches are computed first; then each item is kept, converted or dropped. A converted Match
becomes `Item::Context(ContextLine { line_number, absolute_offset, bytes, submatches: vec![] })`.

**Tests first:**

- `parse_within` with the three separators and extra spaces.
- `within` with gaps, the wrong order (false), case-insensitivity.
- Two consecutive matching lines 5 and 6; keep only line 5 with `after = 1` → line 6 becomes
  Context; with `after = 0` → it is dropped.
- Three Matches (lines 2, 10, 20) with context lines 1, 3, 9, 11, 19, 21 and `before = after
  = 1`; drop the one on line 10 → lines 9, 10, 11 disappear, the rest stay.
- A Match with two submatches in different columns, keep only the second → one submatch, one
  Trail.
- `sections` on a Trail ending in a `column` Segment returns the table's lines; a line with two
  submatches whose Trails end in `heading:Plans` and `column:Price` returns two different
  Sections (Codex r2 #4: `-e '\| Pro' -e needle-10` on line 23 of the fixture).

- [ ] Commit «Add --within and filtering».

---

## Chunk 9: Pipeline, end-to-end and compatibility tests, benchmark

**Files:** `src/app.rs`, `tests/cli.rs`, `tests/rg_compat.rs`, `scripts/bench.sh`,
`tests/fixtures/md/twice.md` (new; C5's directory, created after C5 merged), the one
`-v`/`--in-headings` check in `src/cli/mod.rs`, and `src/lib.rs`/`src/main.rs` only if `run`
needs a fix.

### Task 9.1: app.rs

```rust
pub fn run(args: Vec<OsString>) -> i32
```

1. `cli::parse`. `CliError::Clap` → let clap print; exit `e.exit_code()` (0 for help/version,
   2 otherwise). Other errors → stderr, exit 2.
2. Registry: `formats::Registry::load(formats::user_formats_dir().as_deref(),
   &|p| processors::by_name(p).is_some())`; warnings to stderr unless `--no-messages`.
3. Actions: `--list-formats`, `--export-formats DIR` (prints written paths; 2 on error),
   `--type-list` (ripgrep's format: `name: glob, glob`, sorted). Each returns.
4. `args.patterns()`. No pattern and no `-e`/`-f` → usage error, 2. An empty pattern set (only
   empty `-f` files) → no output, exit 1, before walking, in every mode. Paths: none and stdin is not
   a terminal and not used by `-f -` → stdin (`Source::from_bytes("<stdin>")`); none otherwise
   → `.`. A path `-` → stdin. Paths named on the command line are `explicit`.
5. **Passthrough** (design note 9): `--no-trail`, or a summary flag without structural flags →
   grep-printer `Standard`, `Summary` or `JSON` with ripgrep's defaults: line numbers when
   stdout is a TTY (overridden by `-n`/`-N`; always for JSON unless `-N`), headings when stdout
   is a TTY, file names when more than one path is given or any path is a directory (overridden
   by `-H`/`-I`), colour from `--color` (auto = TTY), `max_columns`, binary policy per file, the
   searcher's `line_number` set to the printer's setting. Walk, search, print, exit. With
   `--json --no-trail`, the run's `summary` line is written at the end from the accumulated
   `JSONSink::stats()` of files with Matches, through `json::summary`.
6. **Trail mode.** Layout: `--json` → Json; `--tree` → Tree; `--flat` → Flat; else Tree on a
   TTY, Flat otherwise. Width: terminal width on a TTY (`terminal_size`), `--fragment-width`
   otherwise and always for Json. Detector built with `args.formats`.
7. Per file (parallel walk, design note 11): `Source::open` (errors → stderr unless
   `--no-messages`, remember that an error happened) → `detector.detect(&src.path, src.head())` →
   `search::search_source(&mut searcher(args, src.explicit, true), &matcher, &src, rg_in_memory(&paths, stdin) && !src.had_bom, false)` (a BOM sends grep-searcher to the reader
   even with memory maps on; the quiet fast path of step 10 never comes here) → None: nothing (stats not
   counted) → Some: anchors per Match (`LineMatch::anchors`), `processor.trails(&Input { bytes:
   src.bytes(), path: &src.path, format: &def.name }, &def.params, &all_anchors)` (error →
   warning, empty Trails), regroup into `trails[i][k]` →
   `keep[i][k] = within(...)` when `--within` → `filter::retain` → `filter::sections` with
   `--expand` → if no Match is left, nothing → render the layout's printer into a
   `termcolor::Buffer` (or JSON into a `Vec<u8>`) and send it with the file's stats.
   A file whose search reported a binary offset skips the Processor (its coordinates follow
   NUL conversion, C3) and prints ripgrep's binary message.
   **Summary flags with structural flags** are dispatched before any discard: every searched
   file reaches `output::summary` with its kept count, including files where the search found
   nothing (for `--files-without-match`).
8. **`--in-headings`** replaces step 7's search: for every walked file whose Format has a
   Processor (Plain files are skipped), `processor.names(&input, params)`; run the matcher's
   `find_iter` on each `Name.text` (empty hits included, as `^` produces); map each hit through
`Name::source_ranges` into byte ranges
   of the Source, which may lie on several lines (a setext heading); group the ranges by line,
   in file order, into one `LineMatch` per line with one submatch per range, each submatch's
   Trail being its Name's `trail` minus its last Segment. Then `-m N` keeps the first N lines,
   `-A/-B/-C` add `Item::Context` lines read from the Source around them (merging overlaps, as
   the searcher would), and filters and rendering continue as in step 7. `-v` with
   `--in-headings` is a usage error («--in-headings selects names; -v has no meaning there»),
   checked in `cli::parse` (C9 adds it there; it is a one-line change to C2's file and the only
   one C9 makes in it).
9. Writer: prints each file's buffer as it arrives (blank line between files in tree layout),
   or in path order under `--sort path` (which also forces the sequential walker); in Json
   layout accumulates `RunStats` from printed files and prints `summary` at the end.
10. `-q`: no output; stop the walk at the first kept Match (a shared `AtomicBool` checked by
    every worker); with structural flags the decision is made after filtering.
11. Exit code (design note 10), decided by what the mode prints: for `--files-without-match`,
    0 when at least one file is listed; for `-l`, `-c` and `--count-matches`, 0 when at least one
    file had a kept Match; otherwise 0 when at least one file had a kept Match after filters,
    whether its lines or only ripgrep's binary message were printed. Errors make it 2
    unless `-q` found a Match.

### Task 9.2: end-to-end tests — tests/cli.rs (written first, RED until 9.1 is done)

`assert_cmd::Command::cargo_bin("sfor")`, always `--color never`, `SFOR_FORMATS_DIR` pointing at
an empty tempdir, `current_dir` at a copy of `tests/fixtures/`. Snapshot stdout of:

1. `sfor needle-pkg md --tree` → `md/guide.md`, `Deploy guide`, `# Install`, `## Linux`,
   ```` ```bash ````, `12:apt install needle-pkg` (acceptance criterion 3).
2. `sfor needle md --flat` (every Match with its Trail; line 24 printed twice, once per column).
3. `sfor needle md --json` → each line parses; every `match` validates `data`'s added fields
   against `docs/json-schema.json` with the `jsonschema` crate (C10 writes the schema; until
   then this test is `#[ignore]` with a comment, and C10 removes the ignore).
4. `sfor needle md --within "install › linux" --flat` → only Linux Matches (criterion 9).
5. `sfor needle-10 md --expand --tree` → the table (lines 21–24) printed once, under
   `## Plans`.
6. `sfor needle md --expand --flat` with two Matches in one Section (`needle-a`, `needle-b`) →
   the Section once.
7. `sfor --in-headings '^Linux$' md --flat` → one Match, line 9. `sfor --in-headings needle md`
   → nothing (items and cells are not names), exit 1.
8. A 20 000-character line with three Matches in `misc/min.json` (Plain in M1) with
   `--flat --fragment-width 120` → each output line ≤ 120 characters plus the prefix, omitted
   counts add up (criterion 8).
9. `--list-formats` with a user Definition overriding markdown → `overridden (…)`.
10. `--export-formats <SFOR_FORMATS_DIR>` → exit 2, «refusing» (criterion 10).
11. `sfor --pcre2 x` and `sfor -nP x` → exit 2, «not supported yet».
12. `sfor formats md` → searches the word; exit 1.
13. `sfor -q needle md` → no output, exit 0; `sfor -q absent md` → exit 1.
14. `sfor --within nowhere needle md` → exit 1 (nothing left after filters).
15. `-c --within linux needle md` and `-l --within linux needle md` → counts and files after
    filtering.
16. `sfor binary misc/bin.dat --flat` → ripgrep's binary message.
17. With a separate fixture `md/twice.md` (`# Plan of plans\n\n## Plan B\n\n## Plan C\n`):
    `--in-headings -i plan md/twice.md --json` (only `twice.md`, since `guide.md` also has
    `Plans`) → the line `# Plan of plans` is one match message with two submatches; `--in-headings -m1 -i plan md/twice.md --flat` → only line 1;
    `--in-headings -m0 -i plan md/twice.md` → nothing, exit 1;
    `--in-headings -C1 '^Linux$' md/guide.md --flat` → line 9 with lines 8 and 10 as context;
    `--in-headings '^' md/twice.md --json` → one empty submatch per heading;
    `--in-headings -v x md` → exit 2.
18. Read errors, run with `-j1 --sort path`: a missing path given before `md` → stderr error,
    exit 2; the same with `-q` and a Match → no output, exit 0; with `-q` and no Match → exit 2;
    with `--no-messages` → no stderr, same exit codes.
19. `--files-without-match --within linux needle md misc` → lists the files whose Matches are
    all outside Linux and the files with no Match, exit 0.
20. Stdin: `printf 'needle\n' | sfor -q needle` exits 0. Open pipe: spawn only `sfor -q needle`
    with `Stdio::piped()` stdin, write `needle\n`, keep the writer open (do not drop it), and
    assert that this child exits with 0 within 2 seconds (polling `try_wait`); then drop the
    writer.

### Task 9.3: ripgrep compatibility — tests/rg_compat.rs (written first)

Skipped with a note unless `rg --version` is 14.1.1. `rg` runs with `RIPGREP_CONFIG_PATH`
removed. For each case, run `rg` and `sfor --no-trail` with the same arguments over
`tests/fixtures/`, both with `--color never`, both through a pipe (not a TTY), and compare
stdout and exit code byte for byte:

`needle`, `needle md/guide.md` (one explicit file: no file name), `-i NEEDLE`, `-s -i NEEDLE`,
`-i -s NEEDLE`, `-w needle`, `-F needle-10`, `-v needle md/guide.md`, `-C1 needle`,
`-A2 -B1 needle`, `-c needle`, `--count-matches needle`, `-l needle`,
`--files-without-match needle`, `-m1 needle`, `-n needle`, `-N needle`, `-H needle md/guide.md`,
`-I needle`, `-g '*.md' needle`, `-g 'md/*.md' needle md`, `--iglob '*.MD' needle`,
`--iglob '*.MD' -g '!guide.md' needle` and the same with the two flags swapped,
`-t markdown needle`, `--hidden needle`, `-M 40 needle`, `absent` (exit 1),
`-e needle -e intro`, `-e --pcre2` (searching for that text), `-f <empty file>` (exit 1),
`-v -f <empty file> md/guide.md`, `-c -v -f <empty file> md/guide.md` and
`--json -v -f <empty file> md/guide.md` (no output at all, exit 1), `binary misc/bin.dat` (explicit binary),
`binary misc` (walked binary: nothing, exit 1), `needle -` with stdin from `md/guide.md`,
`binary` with stdin from `misc/bin.dat` (implicit stdin) and `binary -` (explicit),
`--json needle misc/bin.dat` (one explicit file), the same with `misc/bin.dat` given 11 times,
`--json needle misc` (walked), `--json needle` with stdin from `misc/bin.dat`,
`--json needle misc/bom-bin.dat` (fixture: UTF-8 BOM + `needle first\nmore\0binary needle\n`;
rg 14.1.1: second Match on line 3, offset 18) in trail mode and passthrough, and the BOM files
both as explicit files and through stdin, in passthrough and with `-q`,
`-uuu binary misc` and `-a binary misc/bin.dat` (binary vs text modes), `-a -uuu needle`,
`-uuu -a needle`, `-a --no-binary needle` and `--no-binary -a needle` with stdin from
`misc/bin.dat`, `needle
misc/late-nul.txt` (explicit, NUL after 64 KiB: early Match then message) and `needle misc`
(walked: early Match then the WARNING line), compared also in sfor's `--flat` layout for the
message lines, `--json -c needle`
and `-c --json needle`, `-l -c needle` and `-c -l needle` (last mode wins).

Ordering: cases that walk a directory get `--sort path` on both sides. Then JSON: for
`--json needle`, `--json -C1 needle`, `--json -N needle`, `--json '\r' misc/crlf.txt`,
`--json '$' misc/crlf.txt` (fixture `a\r\nb\n`), `--json '$' misc/noeol.txt` (fixture `a`,
no newline), `--json -v needle md/guide.md`, `--json -v -C1 needle md/guide.md` and
`--json binary` with stdin from `misc/bin.dat`, `--json '\Afoo|needle' misc/anchor.txt`
(fixture `bar\nfoo needle\n`) and `--json '^needle' misc/anchor.txt`, `sfor --json --no-trail` equals
`rg --json` byte for byte except the `elapsed` and `elapsed_total` objects (replaced by
placeholders on both sides); and `sfor --json` (trail mode) equals it too after also removing
our added keys (`format`, `trail`, `fragment`, `section`, `submatches[].trail`) and the
`bytes_printed` values. Finally tree/flat `-M` against rg: `sfor --flat -M 40 needle md/guide.md`
lines that are omissions equal ripgrep's omission lines for the same Matches.

### Task 9.4: benchmark — scripts/bench.sh

```bash
#!/usr/bin/env bash
# Acceptance criterion 13: sfor ≤ 1.5 × rg on a pattern matching < 5 % of the files, and
# sfor --no-trail ≤ 1.1 × rg (Hilo 8386).
# Needs hyperfine (cargo install hyperfine --locked) and jq.
set -euo pipefail
export SFOR_BENCH_DIR="${1:-$HOME/robotin/knowledge}"
export SFOR_BENCH_PATTERN="${2:-ExecStart}"
cargo build --release --quiet
total=$(rg --files -- "$SFOR_BENCH_DIR" | wc -l)
hits=$(rg -l -e "$SFOR_BENCH_PATTERN" -- "$SFOR_BENCH_DIR" | wc -l || true)
echo "files: $total, matching: $hits"
awk -v h="$hits" -v t="$total" 'BEGIN { if (h * 20 >= t) { print "pattern matches 5% or more of the files"; exit 1 } }'
hyperfine --warmup 3 --runs 10 --export-json /tmp/sfor-bench.json \
  'rg --color never -e "$SFOR_BENCH_PATTERN" -- "$SFOR_BENCH_DIR"' \
  './target/release/sfor --color never --flat -e "$SFOR_BENCH_PATTERN" -- "$SFOR_BENCH_DIR"'
jq -r '.results | "trail mode / rg: \(.[1].median / .[0].median)"' /tmp/sfor-bench.json
jq -e '.results[1].median / .results[0].median <= 1.5' /tmp/sfor-bench.json >/dev/null
# Hilo 8386: the search itself must be as fast as ripgrep; --no-trail runs only ripgrep's engine.
hyperfine --warmup 3 --runs 10 --export-json /tmp/sfor-bench-nt.json \
  'rg --color never -e "$SFOR_BENCH_PATTERN" -- "$SFOR_BENCH_DIR"' \
  './target/release/sfor --color never --no-trail -e "$SFOR_BENCH_PATTERN" -- "$SFOR_BENCH_DIR"'
jq -r '.results | "--no-trail / rg: \(.[1].median / .[0].median)"' /tmp/sfor-bench-nt.json
jq -e '.results[1].median / .results[0].median <= 1.1' /tmp/sfor-bench-nt.json >/dev/null
```

The pattern and directory reach the commands only through environment variables quoted
inside single-quoted command strings, so spaces and shell metacharacters stay literal. Run it at
the end of the chunk; M1 is not done until it exits 0. If it fails, profile (`perf record` /
`cargo flamegraph`) and fix the top cost; report the numbers either way.

- [ ] Commits: «Add end-to-end tests» (RED), «Add ripgrep compatibility tests» (RED), «Wire the
  pipeline» (GREEN), «Add the benchmark script».

---

## Chunk 10: Docs, JSON schema, skill

**Files:** `README.md`, `README.es.md`, `docs/json-schema.json`, `skills/searchfor/SKILL.md`,
`docs/specs/2026-10-16-searchfor-design.md`, `CONTEXT.md`, `tests/cli.rs` (only to remove the
`#[ignore]` of test 3).

1. **README.md / README.es.md** (English primary, Spanish translation): «Status», «Usage» and
   «Install» describe v0.1: `cargo install searchfor`; the outputs of C9 tests 1, 2, 4 and 5
   copied from their snapshots; searchfor's own flags; **For AI agents**: why (`--expand` saves
   reading whole files), the JSON recipe
   `sfor --json needle docs | jq -r 'select(.type == "match") | .data.trail | map(.name) | join(" › ")'`,
   the `--within` recipe; a link to `docs/rg-compat.md`; the note that `end.stats` counts
   Matches before filtering. Formats beyond Markdown go under «Roadmap» by milestone.
2. **docs/json-schema.json** (draft 2020-12) of the added fields of `match.data`: `format`
   (string|null), `trail` (array of Segments with required `kind`, `name`, `line`, `end_line`
   and optional `level`, `signature`), `fragment` (null or object), `section` (optional object
   with `start_line`, `end_line`, `text`), and the optional `trail` inside submatch objects.
   Remove the `#[ignore]` of C9 test 3 and run it.
3. **Spec and glossary** — bring them in line with what was decided in review round 1:
   §6.3 cluster rule and minimum Fragment width (from C6); §6.4 `section.text` on every match and
   per-submatch `trail`; §8 the real count of ripgrep flags (from C2's report); CONTEXT.md: the
   Section of a `column` Segment is its table.
4. **skills/searchfor/SKILL.md** (frontmatter `name: searchfor`, `description:` "Use when
   searching Markdown notes, docs or knowledge bases and you need to know which section each
   match belongs to — before running rg and opening whole files"): when to prefer `sfor`, the
   three recipes (`--flat`, `--expand`, `--json` + jq), `--within`, and that only Markdown has
   Trails in v0.1.

- [ ] Commit «Document v0.1 and add the Claude Code skill».

---

## After the plan: closing M1 (ciclo stages 8–10, with /cpd as asked in 8375)

- Stage 8: `cargo install --path .`, copy `skills/searchfor/` to `~/.claude/skills/searchfor/`,
  demo in the Hilo with real output over `~/robotin/knowledge` and the benchmark numbers;
  `/cpd`: merge to `main`, push.
- Stage 9: code review rounds; then `/cpd` again.
- Stage 10: bump to `0.1.0`, `cargo publish` (grill #20: crates.io when v0.1 works), tag
  `v0.1.0`, push the tag.
