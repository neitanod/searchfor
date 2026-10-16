# searchfor M1 (v0.1: core + Markdown) Implementation Plan

> **For agentic workers:** each chunk below is implemented by one subagent in its own git
> worktree, following `test-driven-development`. Steps use checkbox (`- [ ]`) syntax. Read
> `CONTEXT.md` (vocabulary) and `docs/specs/2026-10-16-searchfor-design.md` (the spec) before
> starting; the spec wins over this plan if they disagree, and the disagreement is reported.

**Goal:** a working `sfor` v0.1 that searches like ripgrep and prints the Trail of every Match
in Markdown files, in tree, one-line and JSON output, with `--within`, `--expand`,
`--in-headings`, Fragments for long lines and user-overridable Format Definitions.

**Architecture:** the walk and the search reuse ripgrep's crates (`ignore`, `grep-searcher`,
`grep-regex`). Every file is searched through `grep-printer`'s JSON printer into a buffer; the
buffer of a file with Matches is parsed into the model (`model.rs`), the Detector picks a
Format, the Format's Processor builds an Outline, and Trails come from the Outline. The three
printers read the model. `--no-trail` bypasses all of it and hands the search to
`grep-printer`'s Standard or Summary printer, which is ripgrep's own output code.

**Tech stack:** Rust 1.90 (edition 2024), clap 4, ignore 0.4.33, grep-searcher 0.1.17,
grep-regex 0.1.14, grep-matcher 0.1.9, grep-printer 0.3.1, termcolor 1.4, pulldown-cmark 0.13,
serde + serde_json (feature `preserve_order`), toml 1.1, globset, base64, terminal_size 0.4,
dirs 6, anyhow, thiserror. Tests: insta (snapshots), assert_cmd, tempfile.

**Branch:** work branch `t-768-searchfor` (base `main`), worktree
`/home/sebas/robotin/apps/mdgrep/.workspace/t-768-mdgrep/repo`. Each chunk branches
`t-768-searchfor-c<N>` from the work branch, as `ciclo` stage 7 describes.

---

## Why M1 and not the whole v1

The spec's v1 has ten Format families, WebAssembly Grammars, External Processors, an MCP
server, 111 ripgrep flags and four distribution channels. One plan for all of it would be
several thousand lines that nobody can review in one sitting, and its later chunks would be
written against interfaces nobody has run yet. The v1 is split into five milestones, each one
a usable release with its own plan, review rounds and implementation inside the same Task:

| Milestone | Release | Content |
|---|---|---|
| **M1 (this plan)** | v0.1 | Pipeline, CLI with the core ripgrep flags, Detector, Format Definitions, Processor interface and Outline model, Markdown, tree / one-line / JSON output, Fragments, `--within`, `--expand`, `--in-headings`, Claude Code skill |
| M2 | v0.2 | tree-sitter with WebAssembly Grammars, `keypath` Processor (YAML, JSON, TOML), `treesitter` Processor (PHP, JS, TS, Bash, Go, Rust, Python), `--install-grammar`, `--list-grammars`, Grammar index |
| M3 | v0.3 | HTML, XML, CSV |
| M4 | v0.4 | Object files (ELF, PE, Mach-O), Containers with `-z` |
| M5 | v1.0 | External Processors, MCP server, the rest of ripgrep's flags, `dist` (GitHub Releases, Homebrew tap, npm), crates.io |

Every interface M2–M5 needs is created in M1 (Outline with `columns` for CSV and Markdown
tables, Processor registry by name, Segment `signature` for code), so later milestones add
Processors and flags without reshaping the core.

## Grill decisions this plan implements

| # | Decision | Where in M1 |
|---|---|---|
| 1 | Markdown in v1 (the other Formats in M2–M4) | C5 |
| 2 | ripgrep's crates in-process | C3 |
| 3 | Full ripgrep flag compatibility as the target; unsupported flags fail loudly | C2, C3 |
| 4 | `--within`, `--expand`, `--in-headings` | C8 |
| 5 | Detector: extension, exact name, shebang, magic bytes, `--format` | C4 |
| 6 | Embedded TOML Definitions + user directory + `--export-formats` | C4 |
| 9 | The chain is called `trail` | C1 |
| 11 | Markdown Trail: headings, fences, table column, frontmatter title, parent list items | C5 |
| 13 | Fragment centred on the Match, sized to the terminal, with omitted counts | C6 |
| 14 | Nearby Matches share a Fragment, distant ones get their own | C6 |
| 15 + 8365 | `--json` = ripgrep's JSON Lines + `trail`, `format`, `fragment` in each `match` | C7 |
| 16 | Repo at `/home/sebas/robotin/apps/searchfor` | done (stage 1) |
| 17 | Public from the first commit, MIT OR Apache-2.0 | done (stage 1) |
| 19 | Claude Code skill and README section for agents | C10 |
| 21 | `/ciclo` | this document |

Decisions 7, 8, 10, 12, 18, 20 belong to M2–M5.

## Design notes the chunks share

These are decisions made while planning, beyond the spec. Reviewers: challenge them here.

1. **One search path.** Each file is searched once with `grep_printer::JSON` writing into a
   per-file `Vec<u8>`. A file that printed nothing has no Matches and costs nothing more. A
   file with Matches is parsed (`search::parse_json_lines`) into `FileMatches`. This gives
   context lines, binary detection, encoding handling and every searcher option for free, and
   the JSON printer's output is also the base of our `--json` (design note 6).
2. **`--no-trail` is ripgrep.** With `--no-trail` (and with `-c`, `-l`, `--files-without-match`,
   `--count-matches`, which print no lines), the search uses `grep_printer::Standard` or
   `grep_printer::Summary` directly, so the output is ripgrep's own. The spike of 2026-10-16
   confirmed byte-identical output for `rg -n --no-heading --with-filename`.
3. **Outline, then Trail.** A Processor never computes Trails. It returns an `Outline`: every
   Segment of the file with its byte range, its label range (the bytes that name it: a heading
   line, a list item's first line) and its parent. `trail::trail_for(outline, byte)` picks the
   innermost Segment whose range contains the Match's first byte and walks up the parents,
   leaving out any Segment whose **label** contains that byte — a Match on a heading line is not
   "inside" that heading. This one function serves `--within`, `--expand` and
   `--in-headings` for every Format.
4. **Table columns are generic.** An Outline Segment may carry `columns: Some(Columns { names,
   delimiter })` and `hidden: true`. When the innermost Segment has columns, `trail_for` appends
   a `column` Segment named after the cell the Match starts in. Markdown tables use it in M1;
   CSV reuses it in M3.
5. **One Trail per matching line.** ripgrep reports Matches per line (a line with three
   submatches is one `match` message). The Trail is computed for the first submatch of the
   line; for an inverted search (`-v`) there are no submatches, so the line start is used.
6. **`--json` injection.** `output::json` re-emits the JSON printer's lines and, for each
   `match` message, appends `format`, `trail` and `fragment` at the end of `data`
   (serde_json with `preserve_order`, so ripgrep's keys keep their order). With `--within` or
   `--in-headings`, filtered `match` messages are dropped, `context` messages are kept only if
   they are within `-A`/`-B` lines of a kept match, and the `end` message's `stats` are left as
   ripgrep computed them (documented in the README). The final `summary` message is written by
   us in ripgrep's shape.
7. **Parallelism.** Like ripgrep: `WalkBuilder::build_parallel`, one searcher per thread, each
   thread does search → detect → process → render into a `Vec<u8>`, and sends the rendered
   bytes of a whole file to one writer thread, so a file's output is never interleaved.
   `--sort path` and `-j1` use the single-threaded walker.
8. **Management actions are flags** (`--list-formats`, `--export-formats`), never subcommands,
   so `sfor formats` keeps searching for the word «formats».
9. **Exit codes are ripgrep's:** 0 if anything matched, 1 if nothing matched, 2 if an error
   happened — except that `-q/--quiet` with a match returns 0 even after an error. C9 checks
   this against the installed `rg`.

## File structure

```
Cargo.toml
formats/markdown.toml              Built-in Format Definitions (embedded)
src/main.rs                        fn main → searchfor::run → exit code
src/lib.rs                         module tree, pub fn run
src/model.rs                       Segment, Trail, Submatch, LineMatch, ContextLine, Item, FileMatches
src/trail.rs                       Outline types + trail_for / section_for / label_hit
src/cli/mod.rs                     clap Args → Config
src/cli/rg_flags.rs                ripgrep's 111 flags with their M1 status
src/search.rs                      walker, searcher, JSON-buffer search, parse_json_lines
src/formats/mod.rs                 Registry (built-in + user), lookup by name
src/formats/definition.rs          FormatDefinition (serde) + validation
src/formats/detector.rs            Detector
src/formats/builtin.rs             include_str! of formats/*.toml
src/processors/mod.rs              Processor trait, registry by name
src/processors/markdown.rs         Markdown Outline
src/filter.rs                      --within, --in-headings, --expand
src/output/mod.rs                  OutputMode selection, Rendered file
src/output/fragment.rs             Fragments of long lines
src/output/style.rs                colours
src/output/tree.rs                 tree printer
src/output/flat.rs                 one-line printer
src/output/json.rs                 --json injection + summary
src/app.rs                         pipeline: walk → search → describe → render → write
tests/fixtures/                    Markdown fixtures
tests/cli.rs                       end-to-end with assert_cmd + insta
tests/rg_compat.rs                 compares with the installed rg
docs/rg-compat.md                  flag status table (generated by a test)
docs/json-schema.json              schema of the added JSON fields
skills/searchfor/SKILL.md          Claude Code skill
scripts/bench.sh                   hyperfine against rg
```

## Chunk graph

| Chunk | Content | Depends on | Files (exclusive within its tanda) | Tanda |
|---|---|---|---|---|
| C1 | Cargo project, module tree, `model.rs`, `trail.rs` | — | `Cargo.toml`, `src/main.rs`, `src/lib.rs`, `src/model.rs`, `src/trail.rs`, stub `mod.rs` files | 1 |
| C2 | CLI and ripgrep flag table | C1 | `src/cli/**`, `docs/rg-compat.md` | 2 |
| C3 | Search: walker, JSON-buffer search, parser | C1, C2 (uses `cli::Args`) | `src/search.rs` | 3 |
| C4 | Format Definitions, registry, Detector | C1 | `src/formats/**`, `formats/markdown.toml` | 2 |
| C5 | Markdown Processor | C1 | `src/processors/**`, `tests/fixtures/md/**` | 2 |
| C6 | Fragments | C1 | `src/output/fragment.rs` | 2 |
| C7 | Printers: style, tree, flat, json | C1, C6 | `src/output/{mod,style,tree,flat,json}.rs` | 3 |
| C8 | Filters | C1 | `src/filter.rs` | 3 |
| C9 | Pipeline, `main`, end-to-end and compat tests, bench | C2–C8 | `src/app.rs`, `src/main.rs`, `src/lib.rs` (only `run`), `tests/cli.rs`, `tests/rg_compat.rs`, `scripts/bench.sh` | 4 |
| C10 | Docs, JSON schema, skill | C1 | `README.md`, `README.es.md`, `docs/json-schema.json`, `skills/searchfor/SKILL.md` | 4 |

C1 creates every `mod` declaration and an empty stub for every module file, so tanda 2 and 3
chunks only fill their own files and never touch `lib.rs` or a sibling's `mod.rs`. A chunk that
needs a type from a later chunk uses the signature fixed in this plan; C9 is the first place
where everything links together, and it fixes mismatches.

After each tanda: `cargo build`, `cargo clippy --all-targets -- -D warnings`, `cargo test`.

---

## Chunk 1: Project, model and Trails

### Task 1.1: Cargo project and module tree

**Files:** Create `Cargo.toml`, `src/main.rs`, `src/lib.rs`, and one stub per module listed in
«File structure» (`src/cli/mod.rs`, `src/cli/rg_flags.rs`, `src/search.rs`,
`src/formats/{mod,definition,detector,builtin}.rs`, `src/processors/{mod,markdown}.rs`,
`src/filter.rs`, `src/output/{mod,fragment,style,tree,flat,json}.rs`, `src/app.rs`).

- [ ] **Step 1: Cargo.toml**

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
base64 = "0.22"
clap = { version = "4.6", features = ["derive", "wrap_help"] }
dirs = "6"
globset = "0.4"
grep-matcher = "0.1.9"
grep-printer = "0.3.1"
grep-regex = "0.1.14"
grep-searcher = "0.1.17"
ignore = "0.4.33"
memchr = "2"
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
tempfile = "3"

[profile.release]
lto = "thin"
codegen-units = 1
```

Verify each version resolves: `cargo update --dry-run` must not error. If `base64 0.22` or
`dirs 6` is not the latest major, use the latest and note it in the chunk report.

- [ ] **Step 2: lib.rs with the whole module tree**

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
pub mod trail;

/// Entry point used by `main`. Returns the process exit code (0, 1 or 2, as ripgrep).
pub fn run(args: Vec<std::ffi::OsString>) -> i32 {
    app::run(args)
}
```

`src/main.rs`:

```rust
fn main() {
    let code = searchfor::run(std::env::args_os().collect());
    std::process::exit(code);
}
```

`src/app.rs` stub, so the crate builds until C9:

```rust
pub fn run(_args: Vec<std::ffi::OsString>) -> i32 {
    2
}
```

`src/cli/mod.rs` stub: `pub mod rg_flags;`. `src/formats/mod.rs` stub:
`pub mod builtin; pub mod definition; pub mod detector;`. `src/processors/mod.rs` stub:
`pub mod markdown;`. `src/output/mod.rs` stub:
`pub mod flat; pub mod fragment; pub mod json; pub mod style; pub mod tree;`. Every other stub
is an empty file with a `//!` line saying which chunk fills it.

- [ ] **Step 3: build** — `cargo build` → compiles with only dead-code warnings.

- [ ] **Step 4: commit** — `git add -- Cargo.toml Cargo.lock src` and
  `git commit -m "Set up the crate and its module tree"`.

### Task 1.2: model.rs

**Files:** `src/model.rs`.

These types are the contract between chunks. Do not rename fields without updating this plan.

```rust
//! Types shared by every stage of the pipeline.
use serde::Serialize;
use std::path::PathBuf;

/// One step of a Trail (see CONTEXT.md).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Segment {
    /// "heading", "code", "item", "column", "document"; later milestones add more.
    pub kind: String,
    pub name: String,
    /// 1-based line where the Segment's Section starts.
    pub line: u64,
    /// 1-based line where the Section ends (inclusive).
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
    /// Absolute byte offset of the start of the line in the file.
    pub absolute_offset: u64,
    /// The line, including its terminator if it had one.
    pub bytes: Vec<u8>,
    pub submatches: Vec<Submatch>,
}

impl LineMatch {
    /// Absolute byte offset used to compute the Trail: the first submatch, or the line start
    /// for an inverted search, which has no submatches.
    pub fn anchor(&self) -> usize {
        let rel = self.submatches.first().map(|s| s.start).unwrap_or(0);
        self.absolute_offset as usize + rel
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextLine {
    pub line_number: u64,
    pub absolute_offset: u64,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    Match(LineMatch),
    Context(ContextLine),
}

/// Everything the searcher found in one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileMatches {
    pub path: PathBuf,
    pub items: Vec<Item>,
    /// The JSON printer's output for this file, one message per element, unchanged.
    pub json_lines: Vec<String>,
}

impl FileMatches {
    pub fn matches(&self) -> impl Iterator<Item = &LineMatch> {
        self.items.iter().filter_map(|i| match i {
            Item::Match(m) => Some(m),
            Item::Context(_) => None,
        })
    }
}

/// A file after the Detector and its Processor ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Described {
    pub file: FileMatches,
    /// Name of the Format, or None for Plain.
    pub format: Option<String>,
    /// One Trail per `Item::Match`, in the same order.
    pub trails: Vec<Trail>,
    /// With `--expand`: per Match, the Section to print, as 1-based inclusive lines.
    pub sections: Vec<Option<(u64, u64)>>,
}
```

- [ ] **Step 1: test** in `src/model.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchor_uses_first_submatch_or_line_start() {
        let m = LineMatch {
            line_number: 3,
            absolute_offset: 100,
            bytes: b"hello world\n".to_vec(),
            submatches: vec![Submatch { start: 6, end: 11 }],
        };
        assert_eq!(m.anchor(), 106);
        let inverted = LineMatch { submatches: vec![], ..m };
        assert_eq!(inverted.anchor(), 100);
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

- [ ] **Step 2:** `cargo test model` → PASS. **Step 3:** commit «Add the shared model».

### Task 1.3: trail.rs — Outline, LineIndex, trail_for, label_hit

**Files:** `src/trail.rs`.

```rust
//! Outline of a file and the computation of Trails from it (design notes 3 and 4).
use crate::model::{Segment, Trail};
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
pub struct Columns {
    pub names: Vec<String>,
    pub delimiter: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineSegment {
    /// `line` and `end_line` are filled by `Outline::build`.
    pub segment: Segment,
    /// Bytes covered: the Section.
    pub range: Range<usize>,
    /// Bytes that name the Segment (a heading line, an item's first line). May be empty.
    pub label: Range<usize>,
    pub parent: Option<usize>,
    pub columns: Option<Columns>,
    /// Hidden Segments take part in containment but never appear in a Trail.
    pub hidden: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Outline {
    /// Sorted by `range.start` ascending, then `range.end` descending; parents precede children.
    pub segments: Vec<OutlineSegment>,
}

impl Outline {
    /// Sorts the Segments, assigns parents by containment and fills line numbers.
    /// Processors build their Segments with `parent: None` and call this once.
    pub fn build(mut segments: Vec<OutlineSegment>, content: &[u8]) -> Self {
        segments.sort_by(|a, b| a.range.start.cmp(&b.range.start).then(b.range.end.cmp(&a.range.end)));
        let lines = LineIndex::new(content);
        let mut stack: Vec<usize> = Vec::new();
        for i in 0..segments.len() {
            let r = segments[i].range.clone();
            while let Some(&top) = stack.last() {
                let t = &segments[top].range;
                if t.start <= r.start && r.end <= t.end { break; }
                stack.pop();
            }
            segments[i].parent = stack.last().copied();
            segments[i].segment.line = lines.line_of(r.start);
            segments[i].segment.end_line = lines.line_of(r.end.saturating_sub(1).max(r.start));
            stack.push(i);
        }
        Self { segments }
    }

    /// Index of the innermost Segment whose range contains `byte`.
    fn innermost(&self, byte: usize) -> Option<usize> {
        // Segments are sorted by start; the innermost container is the last one that
        // starts at or before `byte` and still contains it.
        self.segments
            .iter()
            .enumerate()
            .filter(|(_, s)| s.range.start <= byte && byte < s.range.end)
            .map(|(i, _)| i)
            .last()
    }
}

/// The Trail of the Match whose first byte is `byte` (design note 3).
pub fn trail_for(outline: &Outline, content: &[u8], byte: usize) -> Trail {
    let Some(inner) = outline.innermost(byte) else { return Vec::new() };
    let mut chain = Vec::new();
    let mut cur = Some(inner);
    while let Some(i) = cur {
        chain.push(i);
        cur = outline.segments[i].parent;
    }
    chain.reverse();
    let mut trail: Trail = chain
        .iter()
        .map(|&i| &outline.segments[i])
        .filter(|s| !s.hidden && !s.label.contains(&byte))
        .map(|s| s.segment.clone())
        .collect();
    if let Some(cols) = &outline.segments[inner].columns {
        if let Some(seg) = column_segment(cols, content, byte) {
            trail.push(seg);
        }
    }
    trail
}

/// True when `byte` falls inside the label of a visible Segment (`--in-headings`).
pub fn label_hit(outline: &Outline, byte: usize) -> bool {
    outline.segments.iter().any(|s| !s.hidden && s.label.contains(&byte))
}

fn column_segment(cols: &Columns, content: &[u8], byte: usize) -> Option<Segment> {
    let line_start = memchr::memrchr(b'\n', &content[..byte]).map(|i| i + 1).unwrap_or(0);
    let before = &content[line_start..byte];
    let mut count = 0usize;
    let mut escaped = false;
    for &b in before {
        if escaped { escaped = false; continue; }
        if b == b'\\' { escaped = true; continue; }
        if b == cols.delimiter { count += 1; }
    }
    let leading = content[line_start..].iter().find(|b| !b.is_ascii_whitespace()) == Some(&cols.delimiter);
    let index = if leading { count.checked_sub(1)? } else { count };
    let name = cols.names.get(index)?;
    let line = LineIndex::new(content).line_of(byte);
    Some(Segment { kind: "column".into(), name: name.clone(), line, end_line: line, level: None, signature: None })
}
```

(`column_segment` builds a `LineIndex` per call; that is O(file) per Match in a table and is
acceptable for v0.1. If C9's benchmark shows it, pass the file's `LineIndex` in instead.)

- [ ] **Step 1: tests** in `src/trail.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn seg(kind: &str, name: &str, range: Range<usize>, label: Range<usize>) -> OutlineSegment {
        OutlineSegment {
            segment: Segment { kind: kind.into(), name: name.into(), line: 0, end_line: 0, level: None, signature: None },
            range, label, parent: None, columns: None, hidden: false,
        }
    }

    const DOC: &str = "# A\nalpha\n## B\nbeta\n# C\ngamma\n"; // 30 bytes
    // line starts:      0    4      10    15    20   24

    fn outline() -> Outline {
        Outline::build(vec![
            seg("heading", "C", 20..30, 20..24),
            seg("heading", "A", 0..20, 0..4),
            seg("heading", "B", 10..20, 10..15),
        ], DOC.as_bytes())
    }

    #[test]
    fn build_sorts_and_assigns_parents_and_lines() {
        let o = outline();
        let names: Vec<_> = o.segments.iter().map(|s| s.segment.name.as_str()).collect();
        assert_eq!(names, ["A", "B", "C"]);
        assert_eq!(o.segments[1].parent, Some(0));
        assert_eq!(o.segments[2].parent, None);
        assert_eq!((o.segments[1].segment.line, o.segments[1].segment.end_line), (3, 4));
    }

    #[test]
    fn trail_walks_parents() {
        let t = trail_for(&outline(), DOC.as_bytes(), 16); // "beta"
        let names: Vec<_> = t.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["A", "B"]);
    }

    #[test]
    fn match_on_a_label_leaves_that_segment_out() {
        let t = trail_for(&outline(), DOC.as_bytes(), 13); // inside "## B"
        let names: Vec<_> = t.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["A"]);
        assert!(label_hit(&outline(), 13));
        assert!(!label_hit(&outline(), 16));
    }

    #[test]
    fn outside_every_segment_is_an_empty_trail() {
        let o = Outline::build(vec![seg("heading", "A", 5..10, 5..6)], b"xxxxxyyyyyzzzzz");
        assert!(trail_for(&o, b"xxxxxyyyyyzzzzz", 2).is_empty());
    }

    #[test]
    fn table_cells_add_a_column_segment() {
        let doc = "| Plan | Price |\n|---|---|\n| Pro | 10 |\n";
        let mut table = seg("table", "table", 0..doc.len(), 0..0);
        table.hidden = true;
        table.columns = Some(Columns { names: vec!["Plan".into(), "Price".into()], delimiter: b'|' });
        let o = Outline::build(vec![table], doc.as_bytes());
        let byte = doc.find("10").unwrap();
        let t = trail_for(&o, doc.as_bytes(), byte);
        assert_eq!(t.len(), 1);
        assert_eq!((t[0].kind.as_str(), t[0].name.as_str(), t[0].line), ("column", "Price", 3));
    }

    #[test]
    fn line_index() {
        let li = LineIndex::new(b"ab\ncd\n");
        assert_eq!((li.line_of(0), li.line_of(2), li.line_of(3), li.line_of(6)), (1, 1, 2, 3));
        assert_eq!(li.line_start(2), Some(3));
    }
}
```

- [ ] **Step 2:** `cargo test trail` → FAIL before the implementation, PASS after.
- [ ] **Step 3:** `cargo clippy --all-targets -- -D warnings` clean; commit «Add Outline and Trail computation».

---

## Chunk 2: Command line and the ripgrep flag table

### Task 2.1: rg_flags.rs — every ripgrep flag with its M1 status

**Files:** `src/cli/rg_flags.rs`, `docs/rg-compat.md`.

- [ ] **Step 1: generate the list.** Run, with ripgrep 14.1.1 installed:

```bash
rg --help | perl -ne 'print "$2 $1\n" if /^\s+(?:-([A-Za-z0-9.])(?: [A-Z]+)?, )?--([a-z0-9-]+)/' | sort -u
```

Each output line is `<long> [<short>]`. Write them into a `const RG_FLAGS: &[RgFlag]`, one
entry per long flag, sorted by long name:

```rust
//! ripgrep's flags and whether searchfor implements them yet (spec §8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Implemented,
    NotYet,
}

#[derive(Debug, Clone, Copy)]
pub struct RgFlag {
    pub long: &'static str,
    pub short: Option<char>,
    pub status: Status,
}

pub const RG_FLAGS: &[RgFlag] = &[
    // RgFlag { long: "after-context", short: Some('A'), status: Status::Implemented },
    // … one per line of the command above …
];

/// The entry for `--long`, if ripgrep has such a flag.
pub fn by_long(long: &str) -> Option<&'static RgFlag> {
    RG_FLAGS.iter().find(|f| f.long == long)
}

pub fn by_short(c: char) -> Option<&'static RgFlag> {
    RG_FLAGS.iter().find(|f| f.short == Some(c))
}

/// Markdown table for docs/rg-compat.md.
pub fn render_markdown() -> String {
    let mut s = String::from("# ripgrep flag compatibility\n\nGenerated by `cargo test rg_compat_doc -- --ignored`. Do not edit by hand.\n\n| Flag | Short | searchfor |\n|---|---|---|\n");
    for f in RG_FLAGS {
        let short = f.short.map(|c| format!("`-{c}`")).unwrap_or_default();
        let st = match f.status { Status::Implemented => "yes", Status::NotYet => "not yet" };
        s.push_str(&format!("| `--{}` | {} | {} |\n", f.long, short, st));
    }
    s
}
```

**Implemented in M1** (everything else is `NotYet`): `regexp`/`-e`, `file`/`-f`,
`ignore-case`/`-i`, `case-sensitive`/`-s`, `smart-case`/`-S`, `word-regexp`/`-w`,
`line-regexp`/`-x`, `fixed-strings`/`-F`, `invert-match`/`-v`, `glob`/`-g`, `iglob`, `type`/`-t`,
`type-not`/`-T`, `type-list`, `hidden`/`-.`, `no-ignore`, `unrestricted`/`-u`, `follow`/`-L`,
`max-depth`/`-d`, `after-context`/`-A`, `before-context`/`-B`, `context`/`-C`, `count`/`-c`,
`count-matches`, `files-with-matches`/`-l`, `files-without-match`, `max-count`/`-m`,
`line-number`/`-n`, `no-line-number`/`-N`, `with-filename`/`-H`, `no-filename`/`-I`,
`color`, `max-columns`/`-M`, `json`, `sort` (only `path` and `none`), `threads`/`-j`,
`text`/`-a`, `quiet`/`-q`, `no-messages`, `help`/`-h`, `version`/`-V`. For each one that the
`rg --help` list does not contain under that exact name, keep ripgrep's spelling and fix this
plan in the chunk report.

- [ ] **Step 2: tests** in `src/cli/rg_flags.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_has_no_duplicates() {
        let mut longs: Vec<_> = RG_FLAGS.iter().map(|f| f.long).collect();
        longs.sort();
        let n = longs.len();
        longs.dedup();
        assert_eq!(n, longs.len());
    }

    #[test]
    fn table_covers_installed_rg() {
        let Ok(out) = std::process::Command::new("rg").arg("--help").output() else { return };
        let help = String::from_utf8_lossy(&out.stdout);
        let re = regex_lite::Regex::new(r"(?m)^\s+(?:-[A-Za-z0-9.](?: [A-Z]+)?, )?--([a-z0-9-]+)").unwrap();
        for cap in re.captures_iter(&help) {
            assert!(by_long(&cap[1]).is_some(), "--{} missing from RG_FLAGS", &cap[1]);
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
        assert_eq!(on_disk, render_markdown(), "run: cargo test rg_compat_doc -- --ignored");
    }
}
```

Add `regex-lite = "0.1"` to `[dev-dependencies]` — the only file outside `src/cli/**` this
chunk may touch, and only that line.

- [ ] **Step 3:** `cargo test rg_compat_doc -- --ignored`, then `cargo test cli::rg_flags` → PASS.
  Commit «Add ripgrep's flag table and docs/rg-compat.md».

### Task 2.2: cli/mod.rs — Args, Config, unsupported-flag check

**Files:** `src/cli/mod.rs`.

```rust
//! Command line: ripgrep-compatible flags plus searchfor's own (spec §8).
pub mod rg_flags;

use clap::{ArgAction, Parser, ValueEnum};
use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ColorWhen { Never, Auto, Always, Ansi }

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SortBy { Path, None }

#[derive(Debug, Parser)]
#[command(name = "sfor", version, about = "ripgrep-compatible search that shows where each match lives",
          disable_help_subcommand = true, args_override_self = true)]
pub struct Args {
    /// A regular expression used for searching.
    pub pattern: Option<String>,
    /// Files or directories to search.
    pub paths: Vec<PathBuf>,

    #[arg(short = 'e', long = "regexp", action = ArgAction::Append)] pub regexp: Vec<String>,
    #[arg(short = 'f', long = "file", action = ArgAction::Append)] pub pattern_files: Vec<PathBuf>,
    #[arg(short = 'i', long)] pub ignore_case: bool,
    #[arg(short = 's', long)] pub case_sensitive: bool,
    #[arg(short = 'S', long)] pub smart_case: bool,
    #[arg(short = 'w', long)] pub word_regexp: bool,
    #[arg(short = 'x', long)] pub line_regexp: bool,
    #[arg(short = 'F', long)] pub fixed_strings: bool,
    #[arg(short = 'v', long)] pub invert_match: bool,
    #[arg(short = 'g', long = "glob", action = ArgAction::Append)] pub globs: Vec<String>,
    #[arg(long = "iglob", action = ArgAction::Append)] pub iglobs: Vec<String>,
    #[arg(short = 't', long = "type", action = ArgAction::Append)] pub types: Vec<String>,
    #[arg(short = 'T', long = "type-not", action = ArgAction::Append)] pub types_not: Vec<String>,
    #[arg(long)] pub type_list: bool,
    #[arg(short = '.', long)] pub hidden: bool,
    #[arg(long)] pub no_ignore: bool,
    #[arg(short = 'u', long, action = ArgAction::Count)] pub unrestricted: u8,
    #[arg(short = 'L', long)] pub follow: bool,
    #[arg(short = 'd', long)] pub max_depth: Option<usize>,
    #[arg(short = 'A', long)] pub after_context: Option<usize>,
    #[arg(short = 'B', long)] pub before_context: Option<usize>,
    #[arg(short = 'C', long)] pub context: Option<usize>,
    #[arg(short = 'c', long)] pub count: bool,
    #[arg(long)] pub count_matches: bool,
    #[arg(short = 'l', long)] pub files_with_matches: bool,
    #[arg(long)] pub files_without_match: bool,
    #[arg(short = 'm', long)] pub max_count: Option<u64>,
    #[arg(short = 'n', long)] pub line_number: bool,
    #[arg(short = 'N', long)] pub no_line_number: bool,
    #[arg(short = 'H', long)] pub with_filename: bool,
    #[arg(short = 'I', long)] pub no_filename: bool,
    #[arg(long, value_enum, default_value = "auto")] pub color: ColorWhen,
    #[arg(short = 'M', long)] pub max_columns: Option<u64>,
    #[arg(long)] pub json: bool,
    #[arg(long, value_enum)] pub sort: Option<SortBy>,
    #[arg(short = 'j', long)] pub threads: Option<usize>,
    #[arg(short = 'a', long)] pub text: bool,
    #[arg(short = 'q', long)] pub quiet: bool,
    #[arg(long)] pub no_messages: bool,

    // searchfor's own flags (spec §8)
    /// Keep Matches whose Trail contains these Segment names, in order ("A › B", "A > B", "A/B").
    #[arg(long)] pub within: Option<String>,
    /// Print the whole Section of the last Segment of each Match.
    #[arg(long)] pub expand: bool,
    /// Match only inside Segment names: headings, keys, function names.
    #[arg(long)] pub in_headings: bool,
    /// Force a Format: NAME for every file, or GLOB=NAME.
    #[arg(long = "format", action = ArgAction::Append)] pub formats: Vec<String>,
    #[arg(long, conflicts_with = "tree")] pub flat: bool,
    #[arg(long)] pub tree: bool,
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

/// Fails with `NotYet` when any argument is a ripgrep flag searchfor does not implement yet.
/// Runs before clap, so the user sees «not supported yet» instead of «unexpected argument».
pub fn check_unsupported(args: &[OsString]) -> Result<(), CliError> {
    for a in args.iter().skip(1) {
        let Some(a) = a.to_str() else { continue };
        if a == "--" { break; }
        if let Some(long) = a.strip_prefix("--") {
            let name = long.split('=').next().unwrap_or(long);
            if let Some(f) = rg_flags::by_long(name) {
                if f.status == rg_flags::Status::NotYet { return Err(CliError::NotYet(name.to_string())); }
            }
        } else if let Some(shorts) = a.strip_prefix('-') {
            // Only the first short flag of a cluster is checked: "-Hn" checks -H.
            if let Some(c) = shorts.chars().next() {
                if let Some(f) = rg_flags::by_short(c) {
                    if f.status == rg_flags::Status::NotYet { return Err(CliError::NotYet(f.long.to_string())); }
                }
            }
        }
    }
    Ok(())
}

pub fn parse(args: Vec<OsString>) -> Result<Args, CliError> {
    check_unsupported(&args)?;
    let a = Args::try_parse_from(args)?;
    let structural = a.within.is_some() || a.expand || a.in_headings;
    if a.no_trail && structural {
        return Err(CliError::Usage("--no-trail cannot be combined with --within, --expand or --in-headings".into()));
    }
    Ok(a)
}

impl Args {
    /// Patterns from the positional, -e and -f, in ripgrep's precedence: with -e or -f, the
    /// positional is a path.
    pub fn patterns(&self) -> std::io::Result<(Vec<String>, Vec<PathBuf>)> {
        let mut paths = self.paths.clone();
        let mut pats = self.regexp.clone();
        for f in &self.pattern_files {
            pats.extend(std::fs::read_to_string(f)?.lines().map(str::to_string));
        }
        if pats.is_empty() && self.pattern_files.is_empty() {
            if let Some(p) = &self.pattern { pats.push(p.clone()); }
        } else if let Some(p) = &self.pattern {
            paths.insert(0, PathBuf::from(p));
        }
        Ok((pats, paths))
    }

    pub fn before(&self) -> usize { self.before_context.or(self.context).unwrap_or(0) }
    pub fn after(&self) -> usize { self.after_context.or(self.context).unwrap_or(0) }
}
```

- [ ] **Step 1: tests** in `src/cli/mod.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    fn os(v: &[&str]) -> Vec<OsString> { v.iter().map(OsString::from).collect() }

    #[test]
    fn rg_flag_not_yet_is_reported_by_name() {
        let e = parse(os(&["sfor", "--pcre2", "x"])).unwrap_err();
        assert_eq!(e.to_string(), "sfor: --pcre2 is not supported yet (see docs/rg-compat.md)");
    }

    #[test]
    fn positional_becomes_a_path_with_e() {
        let a = parse(os(&["sfor", "-e", "foo", "src"])).unwrap();
        let (pats, paths) = a.patterns().unwrap();
        assert_eq!(pats, ["foo"]);
        assert_eq!(paths, [PathBuf::from("src")]);
    }

    #[test]
    fn context_defaults() {
        let a = parse(os(&["sfor", "-C", "2", "-A", "5", "x"])).unwrap();
        assert_eq!((a.before(), a.after()), (2, 5));
    }

    #[test]
    fn no_trail_rejects_structural_flags() {
        assert!(matches!(parse(os(&["sfor", "--no-trail", "--expand", "x"])), Err(CliError::Usage(_))));
    }

    #[test]
    fn formats_word_is_a_pattern() {
        let a = parse(os(&["sfor", "formats"])).unwrap();
        assert_eq!(a.pattern.as_deref(), Some("formats"));
    }
}
```

`--pcre2` must be `NotYet` in the table for the first test; if ripgrep 14.1.1 spells it
differently, use any `NotYet` flag and fix the expected string.

- [ ] **Step 2:** `cargo test cli` → PASS; clippy clean; commit «Add the command line».

---

## Chunk 3: Search

### Task 3.1: build the matcher, the searcher and the walker from Args

**Files:** `src/search.rs`.

```rust
//! Walking and searching with ripgrep's crates (ADR 0001, design notes 1 and 7).
use crate::cli::Args;
use crate::model::{ContextLine, FileMatches, Item, LineMatch, Submatch};
use anyhow::{Context, Result};
use base64::Engine;
use grep_printer::JSONBuilder;
use grep_regex::{RegexMatcher, RegexMatcherBuilder};
use grep_searcher::{BinaryDetection, Searcher, SearcherBuilder};
use ignore::{overrides::OverrideBuilder, types::TypesBuilder, WalkBuilder};
use std::path::{Path, PathBuf};

pub fn matcher(args: &Args, patterns: &[String]) -> Result<RegexMatcher> {
    let mut b = RegexMatcherBuilder::new();
    b.case_insensitive(args.ignore_case && !args.case_sensitive)
        .case_smart(args.smart_case && !args.ignore_case && !args.case_sensitive)
        .word(args.word_regexp)
        .whole_line(args.line_regexp)
        .fixed_strings(args.fixed_strings)
        .line_terminator(Some(b'\n'));
    // grep-regex joins several patterns as alternatives, as ripgrep does with -e.
    Ok(b.build_many(patterns)?)
}

pub fn searcher(args: &Args) -> Searcher {
    SearcherBuilder::new()
        .line_number(true) // Trails need line numbers even when they are not printed.
        .before_context(args.before())
        .after_context(args.after())
        .invert_match(args.invert_match)
        .max_matches(args.max_count)
        .binary_detection(if args.text { BinaryDetection::none() } else { BinaryDetection::quit(b'\x00') })
        .build()
}

pub fn walker(args: &Args, paths: &[PathBuf]) -> Result<WalkBuilder> {
    let first = paths.first().cloned().unwrap_or_else(|| PathBuf::from("."));
    let mut w = WalkBuilder::new(&first);
    for p in paths.iter().skip(1) { w.add(p); }
    let unrestricted = args.unrestricted;
    let no_ignore = args.no_ignore || unrestricted >= 1;
    w.hidden(!(args.hidden || unrestricted >= 2))
        .ignore(!no_ignore).git_ignore(!no_ignore).git_global(!no_ignore).git_exclude(!no_ignore)
        .parents(!no_ignore)
        .follow_links(args.follow)
        .max_depth(args.max_depth);
    let mut ob = OverrideBuilder::new(&first);
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
    if matches!(args.sort, Some(crate::cli::SortBy::Path)) { w.sort_by_file_path(|a, b| a.cmp(b)); }
    if let Some(j) = args.threads { w.threads(j); }
    Ok(w)
}
```

`-uuu` (binary as text) maps to `args.text || unrestricted >= 3` in `searcher`; add that.
Verify `build_many`, `case_smart` and `sort_by_file_path` exist with these signatures in the
pinned versions (`cargo doc --open` or the docs.rs page); adapt and report if not.

### Task 3.2: search one file into the JSON buffer and parse it

```rust
/// Searches `path`. Returns None when the file has no Matches (design note 1).
pub fn search_path(searcher: &mut Searcher, matcher: &RegexMatcher, path: &Path) -> Result<Option<FileMatches>> {
    let mut printer = JSONBuilder::new().build(Vec::new());
    let mut sink = printer.sink_with_path(matcher, path);
    searcher.search_path(matcher, path, &mut sink).with_context(|| path.display().to_string())?;
    let found = sink.has_match();
    drop(sink);
    if !found { return Ok(None); }
    parse_json_lines(&printer.into_inner(), path).map(Some)
}

/// Same, for bytes already in memory (stdin).
pub fn search_slice(searcher: &mut Searcher, matcher: &RegexMatcher, bytes: &[u8], path: &Path) -> Result<Option<FileMatches>> {
    let mut printer = JSONBuilder::new().build(Vec::new());
    let mut sink = printer.sink_with_path(matcher, path);
    searcher.search_slice(matcher, bytes, &mut sink)?;
    let found = sink.has_match();
    drop(sink);
    if !found { return Ok(None); }
    parse_json_lines(&printer.into_inner(), path).map(Some)
}

/// Parses grep-printer's JSON Lines (ripgrep's `--json` format) into the model.
pub fn parse_json_lines(buf: &[u8], path: &Path) -> Result<FileMatches> {
    let text = std::str::from_utf8(buf)?;
    let mut items = Vec::new();
    let mut json_lines = Vec::new();
    for line in text.lines() {
        json_lines.push(line.to_string());
        let v: serde_json::Value = serde_json::from_str(line)?;
        let data = &v["data"];
        let bytes = || -> Result<Vec<u8>> { decode(&data["lines"]) };
        match v["type"].as_str() {
            Some("match") => items.push(Item::Match(LineMatch {
                line_number: data["line_number"].as_u64().unwrap_or(0),
                absolute_offset: data["absolute_offset"].as_u64().unwrap_or(0),
                bytes: bytes()?,
                submatches: data["submatches"].as_array().map(|a| a.iter().map(|s| Submatch {
                    start: s["start"].as_u64().unwrap_or(0) as usize,
                    end: s["end"].as_u64().unwrap_or(0) as usize,
                }).collect()).unwrap_or_default(),
            })),
            Some("context") => items.push(Item::Context(ContextLine {
                line_number: data["line_number"].as_u64().unwrap_or(0),
                absolute_offset: data["absolute_offset"].as_u64().unwrap_or(0),
                bytes: bytes()?,
            })),
            _ => {}
        }
    }
    Ok(FileMatches { path: path.to_path_buf(), items, json_lines })
}

/// ripgrep writes valid UTF-8 as {"text": …} and anything else as {"bytes": base64}.
fn decode(v: &serde_json::Value) -> Result<Vec<u8>> {
    if let Some(t) = v["text"].as_str() { return Ok(t.as_bytes().to_vec()); }
    if let Some(b) = v["bytes"].as_str() { return Ok(base64::engine::general_purpose::STANDARD.decode(b)?); }
    Ok(Vec::new())
}
```

- [ ] **Step 1: tests** in `src/search.rs`, using a `tempfile::tempdir()`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::parse;
    use std::ffi::OsString;

    fn args(v: &[&str]) -> Args { parse(v.iter().map(OsString::from).collect()).unwrap() }

    #[test]
    fn no_match_is_none_and_match_is_parsed() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("a.md");
        std::fs::write(&f, "one\ntwo needle\nthree\n").unwrap();
        let a = args(&["sfor", "-C1", "needle"]);
        let m = matcher(&a, &["needle".into()]).unwrap();
        let mut s = searcher(&a);
        assert!(search_path(&mut s, &matcher(&a, &["absent".into()]).unwrap(), &f).unwrap().is_none());
        let fm = search_path(&mut s, &m, &f).unwrap().unwrap();
        assert_eq!(fm.items.len(), 3);
        let Item::Match(lm) = &fm.items[1] else { panic!() };
        assert_eq!((lm.line_number, lm.absolute_offset, lm.submatches[0].start), (2, 4, 4));
        assert_eq!(lm.anchor(), 8);
        assert!(fm.json_lines.first().unwrap().contains(r#""type":"begin""#));
    }

    #[test]
    fn non_utf8_lines_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("b.txt");
        std::fs::write(&f, b"caf\xe9 needle\n").unwrap();
        let a = args(&["sfor", "needle"]);
        let fm = search_path(&mut searcher(&a), &matcher(&a, &["needle".into()]).unwrap(), &f).unwrap().unwrap();
        let Item::Match(lm) = &fm.items[0] else { panic!() };
        assert_eq!(lm.bytes, b"caf\xe9 needle\n");
    }

    #[test]
    fn walker_respects_gitignore_and_globs() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        std::fs::write(dir.path().join(".gitignore"), "skip.md\n").unwrap();
        for n in ["keep.md", "skip.md", "other.txt"] { std::fs::write(dir.path().join(n), "x").unwrap(); }
        let a = args(&["sfor", "-g", "*.md", "x"]);
        let found: Vec<String> = walker(&a, &[dir.path().to_path_buf()]).unwrap().build().flatten()
            .filter(|e| e.file_type().is_some_and(|t| t.is_file()))
            .map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert_eq!(found, ["keep.md"]);
    }
}
```

The first test assumes the JSON printer emits `begin` before the first match; if it does not,
adapt the assertion and say so in the report (C7 depends on the message order).

- [ ] **Step 2:** `cargo test search` → PASS; clippy clean; commit «Add walking and searching».

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

- [ ] **Step 1: tests** in `definition.rs`:

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

- [ ] **Step 2:** PASS; commit «Add Format Definitions».

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

/// `--export-formats DIR`: writes the embedded Definitions. Refuses the user formats
/// directory, so an export never silently overrides the built-ins (grill #6 comment).
pub fn export_builtin(dir: &Path, user_dir: Option<&Path>) -> anyhow::Result<Vec<PathBuf>> {
    std::fs::create_dir_all(dir)?;
    if let Some(u) = user_dir {
        if u.exists() && std::fs::canonicalize(dir)? == std::fs::canonicalize(u)? {
            anyhow::bail!("refusing to export into the user formats directory {}: the files would override the built-ins", u.display());
        }
    }
    let mut written = Vec::new();
    for (file, src) in builtin::BUILTIN {
        let p = dir.join(file);
        std::fs::write(&p, src)?;
        written.push(p);
    }
    Ok(written)
}
```

- [ ] **Step 1: tests** in `mod.rs` (tempdirs; never touch the real config dir):

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
        let written = export_builtin(out.path(), Some(user.path())).unwrap();
        assert_eq!(written.len(), builtin::BUILTIN.len());
        assert!(export_builtin(user.path(), Some(user.path())).unwrap_err().to_string().contains("refusing"));
    }
}
```

- [ ] **Step 2:** PASS; commit «Add the Format registry».

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

- [ ] **Step 1: tests** in `detector.rs`, with a registry loaded from a tempdir that adds three
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

- [ ] **Step 2:** PASS; clippy clean; commit «Add the Detector».

---

## Chunk 5: Markdown Processor

### Task 5.1: processors/mod.rs — the Processor trait and the registry by name

**Files:** `src/processors/mod.rs`.

```rust
//! Processors build the Outline of a file (design note 3).
pub mod markdown;

use crate::trail::Outline;

pub trait Processor: Send + Sync {
    fn outline(&self, content: &[u8], params: &toml::Table) -> anyhow::Result<Outline>;
}

/// The Processor named in a Format Definition's `processor` field.
pub fn by_name(name: &str) -> Option<&'static dyn Processor> {
    match name {
        "markdown" => Some(&markdown::Markdown),
        _ => None,
    }
}
```

### Task 5.2: markdown.rs

**Files:** `src/processors/markdown.rs`, fixtures in `tests/fixtures/md/`.

Rules (spec §5.1, grill #11):

- Headings, ATX and setext: kind `heading`, `level` 1–6, name = heading text (inline code and
  emphasis flattened to their text), `label` = the heading's byte range, `range` = from the
  heading to the start of the next heading of the same or a higher level (smaller number),
  or the end of the file.
- YAML frontmatter with a `title:` line: kind `document`, name = the title without quotes,
  `range` = whole file, `label` = the frontmatter block. No `title` → no document Segment.
- Fenced code block: kind `code`, name = first word of the info string, or `code` when empty;
  `range` = the block, `label` = its opening fence line. Indented code blocks are not Segments.
- Table: kind `table`, `hidden = true`, `columns = Columns { names: header cells, delimiter: b'|' }`,
  `range` = the table, empty `label`.
- List item (when `params.list_items`, default true): kind `item`, name = first line of the
  item's text, cut at `item_name_chars` (default 40) characters with `…` appended when cut;
  `range` = the item, `label` = the item's first line.
- Invalid UTF-8 → empty Outline (the file prints like Plain).

```rust
use super::Processor;
use crate::model::Segment;
use crate::trail::{Columns, Outline, OutlineSegment};
use pulldown_cmark::{CodeBlockKind, Event, MetadataBlockKind, Options, Parser, Tag, TagEnd};
use std::ops::Range;

pub struct Markdown;

fn new_seg(kind: &str, name: String, range: Range<usize>, label: Range<usize>, level: Option<u8>) -> OutlineSegment {
    OutlineSegment {
        segment: Segment { kind: kind.into(), name, line: 0, end_line: 0, level, signature: None },
        range, label, parent: None, columns: None, hidden: false,
    }
}

fn first_line(text: &str, r: &Range<usize>) -> Range<usize> {
    let end = text[r.clone()].find('\n').map(|i| r.start + i + 1).unwrap_or(r.end);
    r.start..end
}

impl Processor for Markdown {
    fn outline(&self, content: &[u8], params: &toml::Table) -> anyhow::Result<Outline> {
        let Ok(text) = std::str::from_utf8(content) else { return Ok(Outline::default()) };
        let list_items = params.get("list_items").and_then(|v| v.as_bool()).unwrap_or(true);
        let item_chars = params.get("item_name_chars").and_then(|v| v.as_integer()).unwrap_or(40) as usize;
        let opts = Options::ENABLE_TABLES | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS | Options::ENABLE_HEADING_ATTRIBUTES;

        let mut segs: Vec<OutlineSegment> = Vec::new();
        // (index in segs, level) of headings, to close their ranges later.
        let mut headings: Vec<(usize, u8)> = Vec::new();
        // Text being collected: (what, segs index) for headings, items and table headers.
        let mut heading_text: Option<(usize, String)> = None;
        let mut item_text: Vec<(usize, String, bool)> = Vec::new(); // (seg idx, text, done)
        let mut table: Option<(usize, Vec<String>, bool, String)> = None; // (seg idx, names, in_head, cell)
        let mut in_meta = false;

        for (ev, r) in Parser::new_ext(text, opts).into_offset_iter() {
            match ev {
                Event::Start(Tag::MetadataBlock(MetadataBlockKind::YamlStyle)) => {
                    in_meta = true;
                    segs.push(new_seg("document", String::new(), 0..text.len(), r.clone(), None));
                }
                Event::End(TagEnd::MetadataBlock(_)) => in_meta = false,
                Event::Text(t) if in_meta => {
                    if let Some(title) = t.lines().find_map(|l| l.strip_prefix("title:")) {
                        let title = title.trim().trim_matches(|c| c == '"' || c == '\'');
                        if let Some(doc) = segs.iter_mut().find(|s| s.segment.kind == "document") {
                            doc.segment.name = title.to_string();
                        }
                    }
                }
                Event::Start(Tag::Heading { level, .. }) => {
                    let lvl = level as u8;
                    segs.push(new_seg("heading", String::new(), r.clone(), r.clone(), Some(lvl)));
                    headings.push((segs.len() - 1, lvl));
                    heading_text = Some((segs.len() - 1, String::new()));
                }
                Event::End(TagEnd::Heading(_)) => {
                    if let Some((i, t)) = heading_text.take() { segs[i].segment.name = t.trim().to_string(); }
                }
                Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) => {
                    let name = info.split_whitespace().next().unwrap_or("code").to_string();
                    let name = if name.is_empty() { "code".into() } else { name };
                    segs.push(new_seg("code", name, r.clone(), first_line(text, &r), None));
                }
                Event::Start(Tag::Table(_)) => {
                    let mut s = new_seg("table", "table".into(), r.clone(), r.start..r.start, None);
                    s.hidden = true;
                    segs.push(s);
                    table = Some((segs.len() - 1, Vec::new(), false, String::new()));
                }
                Event::Start(Tag::TableHead) => if let Some(t) = table.as_mut() { t.2 = true },
                Event::End(TagEnd::TableHead) => if let Some((i, names, in_head, _)) = table.as_mut() {
                    *in_head = false;
                    segs[*i].columns = Some(Columns { names: names.clone(), delimiter: b'|' });
                },
                Event::End(TagEnd::TableCell) => if let Some((_, names, true, cell)) = table.as_mut() {
                    names.push(std::mem::take(cell).trim().to_string());
                },
                Event::End(TagEnd::Table) => table = None,
                Event::Start(Tag::Item) if list_items => {
                    segs.push(new_seg("item", String::new(), r.clone(), first_line(text, &r), None));
                    item_text.push((segs.len() - 1, String::new(), false));
                }
                Event::End(TagEnd::Item) if list_items => {
                    if let Some((i, t, _)) = item_text.pop() { segs[i].segment.name = cut(&t, item_chars); }
                }
                Event::Text(t) | Event::Code(t) => {
                    if let Some((_, s)) = heading_text.as_mut() { s.push_str(&t); }
                    if let Some((_, _, true, cell)) = table.as_mut() { cell.push_str(&t); }
                    // Only the innermost open item collects, and only until its first line ends.
                    if let Some((_, s, done)) = item_text.last_mut() { if !*done { s.push_str(&t); } }
                }
                Event::SoftBreak | Event::HardBreak | Event::End(TagEnd::Paragraph) => {
                    if let Some((_, _, done)) = item_text.last_mut() { *done = true; }
                }
                _ => {}
            }
        }
        // Close heading ranges: up to the next heading of the same or a higher level.
        for (k, &(i, lvl)) in headings.iter().enumerate() {
            let end = headings[k + 1..].iter().find(|&&(_, l)| l <= lvl).map(|&(j, _)| segs[j].range.start).unwrap_or(text.len());
            segs[i].range.end = end;
        }
        segs.retain(|s| !(s.segment.kind == "document" && s.segment.name.is_empty()));
        Ok(Outline::build(segs, content))
    }
}

fn cut(s: &str, n: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= n { return s.to_string(); }
    let mut out: String = s.chars().take(n).collect();
    out.push('…');
    out
}
```

Verify against pulldown-cmark 0.13 that: `Tag::Heading` is a struct variant with `level`,
`HeadingLevel as u8` gives 1–6 (if not, map explicitly), `MetadataBlockKind::YamlStyle` exists,
and the offset range of `Start(Tag::Item)` covers the whole item including nested lists. Fix the
code where the API differs and say so in the report.

- [ ] **Step 1: fixture** `tests/fixtures/md/guide.md`:

````markdown
---
title: "Deploy guide"
---

# Install

Intro paragraph with needle-intro.

## Linux

```bash
apt install needle-pkg
```

- step one
  - nested needle-item
- step two

## Plans

| Plan | Price |
|------|-------|
| Pro  | needle-10 |

Setext Title
============

needle-setext body
````

- [ ] **Step 2: tests** in `markdown.rs`. A helper `trail_names(byte)` loads the fixture with
`include_str!("../../tests/fixtures/md/guide.md")`, builds the Outline with default params and
returns `trail_for(...)` as `Vec<String>` formatted `kind:name`:

| Match text | Expected Trail |
|---|---|
| `needle-intro` | `document:Deploy guide`, `heading:Install` |
| `needle-pkg` | `document:Deploy guide`, `heading:Install`, `heading:Linux`, `code:bash` |
| `needle-item` | `document:Deploy guide`, `heading:Install`, `heading:Linux`, `item:step one` |
| `needle-10` | `document:Deploy guide`, `heading:Install`, `heading:Plans`, `column:Price` |
| `needle-setext` | `document:Deploy guide`, `heading:Setext Title` |
| `# Install` (byte of `I`) | `document:Deploy guide` (a Match on a label leaves it out) |

Also: with `list_items = false` in params, `needle-item` gives the Trail without `item:step one`;
a file with invalid UTF-8 gives an empty Outline; the `Linux` heading has `line` 9 and
`end_line` 17 (count them in the fixture and fix the numbers if the fixture changed).

- [ ] **Step 3:** PASS; clippy clean; commit «Add the Markdown Processor».

---

## Chunk 6: Fragments

**Files:** `src/output/fragment.rs`.

Spec §6.3, grill #13 and #14. Widths are counted in `char`s (display width of wide characters
is out of scope for v0.1).

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
    /// Submatch byte ranges relative to `text`.
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

/// Splits `line` (without terminator) into Fragments around `subs` (byte ranges).
/// Returns None when the line fits in `width` characters.
pub fn fragments(line: &str, subs: &[(usize, usize)], width: usize) -> Option<Vec<Fragment>> {
    let total = line.chars().count();
    if total <= width { return None; }
    // Byte offset of every char, plus the end.
    let offs: Vec<usize> = line.char_indices().map(|(i, _)| i).chain(std::iter::once(line.len())).collect();
    let to_char = |b: usize| offs.partition_point(|&o| o < b);
    let digits = total.to_string().len();
    let inner = width.saturating_sub(2 * (5 + digits)).max(10);

    let mut spans: Vec<(usize, usize)> = subs.iter().map(|&(s, e)| (to_char(s), to_char(e).max(to_char(s) + 1))).collect();
    if spans.is_empty() { spans.push((0, 1)); }
    spans.sort();
    let mut clusters: Vec<(usize, usize)> = Vec::new();
    for (s, e) in spans {
        match clusters.last_mut() {
            Some(c) if s - c.0 <= inner / 2 && e - c.0 <= inner => c.1 = c.1.max(e),
            _ => clusters.push((s, e)),
        }
    }
    Some(clusters.into_iter().map(|(s, e)| {
        let (ws, we) = if e - s >= inner {
            (s, (s + inner).min(total))
        } else {
            let pad = (inner - (e - s)) / 2;
            let ws = s.saturating_sub(pad);
            let we = (ws + inner).min(total);
            (we.saturating_sub(inner), we)
        };
        let (bs, be) = (offs[ws], offs[we]);
        Fragment {
            text: line[bs..be].to_string(),
            start: bs,
            omitted_before: ws,
            omitted_after: total - we,
            submatches: subs.iter().filter(|&&(a, b)| a < be && b > bs)
                .map(|&(a, b)| (a.max(bs) - bs, b.min(be) - bs)).collect(),
        }
    }).collect())
}
```

- [ ] **Step 1: tests**:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_lines_are_untouched() {
        assert!(fragments("short line", &[(0, 5)], 80).is_none());
    }

    #[test]
    fn single_match_is_centred_with_counts() {
        let line = format!("{}NEEDLE{}", "a".repeat(1000), "b".repeat(1000));
        let f = fragments(&line, &[(1000, 1006)], 80).unwrap();
        assert_eq!(f.len(), 1);
        let inner = 80 - 2 * (5 + 4);
        assert_eq!(f[0].text.chars().count(), inner);
        assert!(f[0].text.contains("NEEDLE"));
        assert_eq!(f[0].omitted_before + inner + f[0].omitted_after, 2006);
        let (a, b) = f[0].submatches[0];
        assert_eq!(&f[0].text[a..b], "NEEDLE");
        assert!(f[0].render_plain().starts_with(&format!("…[+{}] ", f[0].omitted_before)));
    }

    #[test]
    fn near_matches_share_far_ones_split() {
        let mut line = "x".repeat(3000);
        line.replace_range(1000..1001, "A");
        line.replace_range(1010..1011, "B");
        line.replace_range(2500..2501, "C");
        let f = fragments(&line, &[(1000, 1001), (1010, 1011), (2500, 2501)], 100).unwrap();
        assert_eq!(f.len(), 2);
        assert_eq!(f[0].submatches.len(), 2);
        assert_eq!(f[1].submatches.len(), 1);
    }

    #[test]
    fn match_at_the_edges_and_multibyte() {
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
        let line = "z".repeat(500);
        let f = fragments(&line, &[], 60).unwrap();
        assert_eq!(f[0].omitted_before, 0);
    }
}
```

- [ ] **Step 2:** PASS; clippy clean; commit «Add Fragments for long lines».

---

## Chunk 7: Printers

**Files:** `src/output/{mod,style,tree,flat,json}.rs`. Reads `model::Described`; uses
`output::fragment` (C6).

### Task 7.1: mod.rs and style.rs

```rust
//! Rendering of Described files (spec §6).
pub mod flat;
pub mod fragment;
pub mod json;
pub mod style;
pub mod tree;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout { Tree, Flat, Json }

#[derive(Debug, Clone)]
pub struct RenderOptions {
    pub layout: Layout,
    pub color: bool,
    /// Show line numbers. Tree and flat always show them unless `-N`.
    pub line_numbers: bool,
    /// Characters available for a line's text before it becomes Fragments.
    pub width: usize,
    /// `-M`: when set, long lines print ripgrep's omission message instead of Fragments.
    pub max_columns: Option<u64>,
    /// Show the file name (flat layout); ripgrep's -H / -I.
    pub with_filename: bool,
}

/// One file, rendered, ready for the writer thread.
pub type Rendered = Vec<u8>;

/// `line` without its terminator, as text (lossy for invalid UTF-8).
pub fn line_text(bytes: &[u8]) -> String {
    let b = bytes.strip_suffix(b"\n").unwrap_or(bytes);
    let b = b.strip_suffix(b"\r").unwrap_or(b);
    String::from_utf8_lossy(b).into_owned()
}
```

`style.rs` — ripgrep's default colours, as ANSI strings (no termcolor needed for our printers):

```rust
pub struct Style { pub on: bool }
impl Style {
    fn wrap(&self, code: &str, s: &str) -> String {
        if self.on { format!("\x1b[{code}m{s}\x1b[0m") } else { s.to_string() }
    }
    pub fn path(&self, s: &str) -> String { self.wrap("35", s) }          // magenta, as rg
    pub fn line_no(&self, s: &str) -> String { self.wrap("32", s) }       // green, as rg
    pub fn matched(&self, s: &str) -> String { self.wrap("1;31", s) }     // bold red, as rg
    pub fn trail(&self, s: &str) -> String { self.wrap("36", s) }         // cyan
    pub fn dim(&self, s: &str) -> String { self.wrap("2", s) }            // markers, separators
}
```

`pub fn highlight(text: &str, subs: &[(usize, usize)], st: &Style) -> String` in `style.rs`
wraps each submatch with `matched`, clamping ranges to `text` and to char boundaries.
Tests: no colour returns the text unchanged; with colour, `highlight("a NEEDLE b", &[(2, 8)])`
is `"a \x1b[1;31mNEEDLE\x1b[0m b"`.

### Task 7.2: the line renderer shared by tree and flat

In `mod.rs`:

```rust
/// The text of a matching line for tree and flat: whole, as Fragments, or ripgrep's omission.
pub fn render_line(lm: &crate::model::LineMatch, o: &RenderOptions, st: &style::Style) -> Vec<String> {
    let text = line_text(&lm.bytes);
    let subs: Vec<(usize, usize)> = lm.submatches.iter().map(|s| (s.start, s.end)).collect();
    if let Some(m) = o.max_columns {
        if text.len() as u64 > m {
            return vec![format!("[Omitted long line with {} matches]", subs.len())];
        }
    }
    match fragment::fragments(&text, &subs, o.width) {
        None => vec![style::highlight(&text, &subs, st)],
        Some(frs) => frs.iter().map(|f| {
            let mut s = String::new();
            if f.omitted_before > 0 { s.push_str(&st.dim(&format!("…[+{}] ", f.omitted_before))); }
            s.push_str(&style::highlight(&f.text, &f.submatches, st));
            if f.omitted_after > 0 { s.push_str(&st.dim(&format!(" […+{}]", f.omitted_after))); }
            s
        }).collect(),
    }
}
```

The `-M` message mirrors ripgrep's wording; C9's compat test checks the exact text with
`rg -M`. Fragments of the same line print one under the other, each prefixed like the line.

### Task 7.3: tree.rs

Spec §6.1. Format, with `n` = depth of the Segment (0-based) and indentation of two spaces
per level:

```
<path>
  <seg 0>
    <seg 1>
      <line>: <text>
      <line>- <context>
  --
```

- Segment labels: `heading` → `#` × level + space + name; `code` → «`name` block» rendered as
  ```` ```name ````; `item` → `• name`; `column` → `column «name»`; `document` → `name`;
  any other kind → `kind name`. When `signature` is set (code, M2), it replaces the label.
- Between two consecutive Matches, only the Segments that differ from the previous Trail are
  printed (shared prefix printed once). A Match with an empty Trail prints at depth 0.
- Context lines use `-` after the line number, Matches `:`, as ripgrep. A gap between
  consecutive items' line numbers when context is on prints `--` at the depth of the next item.
- `--expand` (`Described.sections`): for a Match whose Section was not printed yet, print the
  Section's lines (from the file content, passed in) instead of the matching line, each with its
  number and `:` for matching lines, `-` for the rest. A Section already printed is skipped.
- A blank line between files, as ripgrep's heading mode.

```rust
pub fn render(d: &Described, content: &[u8], o: &RenderOptions, out: &mut Vec<u8>);
```

**Tests** with `insta::assert_snapshot!` on hand-built `Described` values (no search needed):
two Matches under the same `# A › ## B`; one under `# A` only after one under `# A › ## B`
(prints nothing new for `# A`); context lines and a `--` gap; a long line as two Fragments;
`--expand` with two Matches in one Section prints the Section once. Snapshots are reviewed by
eye before accepting (`cargo insta review`), and the chunk report pastes one.

### Task 7.4: flat.rs

Spec §6.2: `path:line: Trail │ text`, Trail joined with ` › `, `│` dimmed. No Trail → no `│`
part (`path:line: text`). `-I` drops `path:`; `-N` drops `line:`. Context lines:
`path-line- text`, as ripgrep. Fragments: one output line per Fragment, same prefix.
`--expand`: `path:start-end: Trail` then the Section's lines verbatim, then an empty line.
Trails longer than `width / 2` characters keep their first and last Segments and replace the
middle with `…` (spec §5.1 for list items).

```rust
pub fn render(d: &Described, content: &[u8], o: &RenderOptions, out: &mut Vec<u8>);
```

**Tests** (snapshots): plain match; no Trail; `-I -N`; context; Fragment; long Trail elided.

### Task 7.5: json.rs

Design note 6. Input: the file's `json_lines` (ripgrep's messages), the Described, `width`
(= `--fragment-width`), and which Matches were kept by the filters.

```rust
pub struct JsonFile<'a> {
    pub described: &'a Described,
    /// Same length as the Match items: false = dropped by --within / --in-headings.
    pub kept: &'a [bool],
    pub before: usize,
    pub after: usize,
    pub width: usize,
    pub no_trail: bool,
    /// --expand: the Section texts, aligned with Matches; Some only on the first Match of a Section.
    pub section_texts: &'a [Option<String>],
}

pub fn render(f: &JsonFile, out: &mut Vec<u8>);

/// ripgrep's final `summary` message, for the whole run.
pub fn summary(stats: &RunStats) -> String;
```

Rules:

- `begin` and `end` pass through unchanged. If no Match of the file is kept, nothing of the
  file is printed.
- The k-th `match` message pairs with the k-th Match item. If dropped, skip it. If kept, parse
  it with serde_json (`preserve_order`), and unless `no_trail`, insert at the end of `data`:
  `"format"` (string or null), `"trail"` (array of Segments), `"fragment"` (null, or
  `{"text","start","omitted_before","omitted_after"}` from `fragment::fragments(text, subs,
  width)` — the first Fragment when there are several), and with `--expand`
  `"section": {"start_line","end_line","text"?}` (`text` only on the first Match of a
  Section). Re-serialise compactly.
- `context` messages pass through only if their line is within `before` lines before or
  `after` lines after a kept Match.
- `RunStats` mirrors ripgrep's summary: `elapsed_total`, and `stats` with `elapsed`,
  `searches`, `searches_with_match`, `bytes_searched`, `bytes_printed`, `matched_lines`,
  `matches`. C9 accumulates it from each file's `end` message (`data.stats`), adding fields.
  Byte-for-byte equality with ripgrep is required except for the elapsed fields.

**Tests:** a hand-written `json_lines` for one file with two matches and one context line,
plus a Described with two Trails: (1) both kept → two match lines carry `"trail"` as the last
keys of `data`, the key order of ripgrep's fields unchanged; (2) second dropped with
`after = 0` → its match and the context line after it disappear; (3) `no_trail` → output equals
input; (4) `summary` of two files adds the counters.

- [ ] Commit after each task: «Add colours and the line renderer», «Add the tree printer»,
  «Add the one-line printer», «Add the JSON printer».

---

## Chunk 8: Filters

**Files:** `src/filter.rs`.

```rust
//! --within, --in-headings and --expand (spec §8, grill #4).
use crate::model::{Item, Trail};
use crate::trail::{label_hit, Outline};

/// "Install › Linux", "Install > Linux" and "Install/Linux" → ["install", "linux"].
pub fn parse_within(s: &str) -> Vec<String> {
    s.split(['›', '>', '/']).map(|p| p.trim().to_lowercase()).filter(|p| !p.is_empty()).collect()
}

/// True when the Segment names contain every wanted name, in order, each as a
/// case-insensitive substring. Gaps are allowed: "install › bash" matches
/// Install › Linux › bash.
pub fn within(trail: &Trail, wanted: &[String]) -> bool {
    let mut it = trail.iter();
    wanted.iter().all(|w| it.any(|s| s.name.to_lowercase().contains(w.as_str())))
}

/// --in-headings: the Match's anchor byte falls inside a Segment's label.
pub fn in_headings(outline: &Outline, anchor: usize) -> bool {
    label_hit(outline, anchor)
}

/// Drops the Match items whose `keep` is false (same order as the Match items), the Context
/// items farther than `before`/`after` lines from every kept Match, and returns the Trails of
/// the kept Matches.
pub fn retain(items: &mut Vec<Item>, trails: Vec<Trail>, keep: &[bool], before: usize, after: usize) -> Vec<Trail> {
    let kept_lines: Vec<u64> = items.iter().filter_map(|i| match i { Item::Match(m) => Some(m.line_number), _ => None })
        .zip(keep).filter(|(_, k)| **k).map(|(l, _)| l).collect();
    let near = |l: u64| kept_lines.iter().any(|&m| (l < m && m - l <= before as u64) || (l > m && l - m <= after as u64));
    let mut k = keep.iter();
    items.retain(|i| match i {
        Item::Match(_) => *k.next().unwrap_or(&false),
        Item::Context(c) => near(c.line_number),
    });
    trails.into_iter().zip(keep).filter(|(_, k)| **k).map(|(t, _)| t).collect()
}

/// --expand: the Section of the last Segment of each Trail, or None for an empty Trail.
pub fn sections(trails: &[Trail]) -> Vec<Option<(u64, u64)>> {
    trails.iter().map(|t| t.last().map(|s| (s.line, s.end_line))).collect()
}
```

**Tests:** `parse_within` with the three separators and extra spaces; `within` with gaps,
wrong order (false), case-insensitivity; `retain` with three Matches and context lines,
dropping the middle one with `before = after = 1` keeps only the context lines adjacent to the
first and third; `sections` returns None for an empty Trail.

- [ ] Commit «Add --within, --in-headings and --expand».

---

## Chunk 9: Pipeline, main, end-to-end tests, benchmark

**Files:** `src/app.rs`, `src/main.rs` (unchanged unless needed), `src/lib.rs` (only `run`),
`tests/cli.rs`, `tests/rg_compat.rs`, `scripts/bench.sh`.

### Task 9.1: app.rs

```rust
pub fn run(args: Vec<OsString>) -> i32
```

1. `cli::parse(args)`. On `CliError::Clap` let clap print (`e.exit_code()` for help/version,
   2 otherwise); on any other error print it to stderr and return 2.
2. Load the registry: `formats::Registry::load(formats::user_formats_dir().as_deref(),
   &|p| processors::by_name(p).is_some())`; print warnings to stderr unless `--no-messages`.
3. Actions: `--list-formats` → print `registry.list()`, return 0. `--export-formats DIR` →
   `export_builtin`, print each written path, return 0 (2 on error). `--type-list` → print
   ignore's default types as ripgrep does (`name: glob, glob`), return 0.
4. Patterns and paths from `args.patterns()`. No pattern → usage error, 2. No path and stdin is
   not a terminal → search stdin with `search::search_slice` and path `<stdin>`; no path and
   stdin is a terminal → `.`.
5. **Passthrough mode** (design note 2): if `--no-trail` (without `--json`), or `-c`,
   `--count-matches`, `-l`, `--files-without-match`, run ripgrep's printer: `grep_printer::Standard`
   (for `--no-trail`) or `Summary` (for the others), configured with ripgrep's defaults for
   headings, line numbers and file names (heading + line numbers on a TTY, `path:line:` in a
   pipe, `-n`/`-N`/`-H`/`-I` override), colour from `--color`, `max_columns`. Walk, search, exit.
6. **Trail mode**: layout = `--json` → Json; `--tree` → Tree; `--flat` → Flat; else Tree if
   stdout is a terminal, Flat otherwise. `width` = terminal width minus the line prefix on a
   TTY, `--fragment-width` otherwise and always for Json.
7. Per file (parallel walk, design note 7):
   `search_path` → `Option<FileMatches>`; read the content (`std::fs::read`); detect with
   `detector.detect(path, &content[..content.len().min(512)])`; Processor → Outline (on error:
   warning, empty Outline); Trails = `trail::trail_for(&outline, &content, m.anchor())` per
   Match; filters: `keep[i] = within(...) && (!in_headings || filter::in_headings(...))`;
   `filter::retain`; sections if `--expand`; render with the layout's printer into a `Vec<u8>`
   and send it to the writer thread with the file's stats.
8. The writer thread prints each file's bytes as they arrive (sorted by path under
   `--sort path`, which also forces a single thread), then the JSON summary in Json layout.
9. Exit code (design note 9).

### Task 9.2: end-to-end tests — tests/cli.rs

`assert_cmd::Command::cargo_bin("sfor")`, always with `--color never`, `SFOR_FORMATS_DIR`
pointing at an empty tempdir, and `current_dir` at a copy of `tests/fixtures/`. Snapshot the
stdout of:

1. `sfor needle-pkg md --tree` → `md/guide.md`, `Deploy guide`, `# Install`, `## Linux`,
   ```` ```bash ````, `11: apt install needle-pkg` (acceptance criterion 3).
2. `sfor needle md --flat` (one line per Match with Trails).
3. `sfor needle md --json` → every `match` line has `trail`; parse each line as JSON.
4. `sfor needle md --within "install › linux" --flat` → only the Linux Matches (criterion 9).
5. `sfor needle-10 md --expand --tree` → the Plans Section printed once.
6. `sfor --in-headings Plans md --flat` → one Match, on the `## Plans` line.
7. A 20 000-character line with three Matches in `long/min.json` (Plain in M1) with
   `--flat --fragment-width 120` → each output line ≤ 120 + prefix characters, omitted counts add
   up (criterion 8).
8. `sfor --list-formats` with a user Definition overriding markdown → `overridden (…)`.
9. `sfor --export-formats <SFOR_FORMATS_DIR>` → exit 2 and «refusing» (criterion 10).
10. `sfor --pcre2 x` → exit 2 and the «not supported yet» message.
11. `sfor formats md` → searches for the word, exit 1 when absent.

### Task 9.3: ripgrep compatibility — tests/rg_compat.rs

Skipped (with a printed note) when `rg` is not on PATH. For each case, run `rg` and
`sfor --no-trail` with the same arguments over `tests/fixtures/` and compare stdout and exit
code byte for byte:

`needle`, `-i NEEDLE`, `-w needle`, `-F needle-10`, `-v needle`, `-C1 needle`, `-A2 -B1 needle`,
`-c needle`, `-l needle`, `--files-without-match needle`, `-m1 needle`, `-n needle`,
`-N needle`, `-H needle md/guide.md`, `-I needle`, `-g '*.md' needle`, `-t markdown needle`,
`--hidden needle`, `-M 40 needle`, `--sort path needle`, `absent` (exit 1), `-e needle -e intro`.

All cases run with `--sort path` added on both sides so the order is deterministic, and with
`--color never`. Plus: `--json --no-trail needle` equals `rg --json needle` after removing the
`elapsed` and `elapsed_total` objects from every line (criterion 12).

### Task 9.4: benchmark — scripts/bench.sh

```bash
#!/usr/bin/env bash
# Compares sfor with rg (acceptance criterion 13). Needs hyperfine: cargo install hyperfine --locked
set -euo pipefail
dir=${1:-$HOME/robotin/knowledge}
pattern=${2:-ExecStart}
cargo build --release --quiet
hyperfine --warmup 3 --runs 10 \
  "rg --color never $pattern $dir" \
  "./target/release/sfor --color never --flat $pattern $dir"
```

Run it at the end of the chunk and paste the result in the report. If `sfor` takes more than
1.5× `rg`, profile (`cargo flamegraph` or `perf`) and report the top cost; do not optimise
blindly.

- [ ] Commits: «Wire the pipeline», «Add end-to-end tests», «Add ripgrep compatibility
  tests», «Add the benchmark script».

---

## Chunk 10: Docs, JSON schema, skill

**Files:** `README.md`, `README.es.md`, `docs/json-schema.json`, `skills/searchfor/SKILL.md`.

1. **README.md / README.es.md**: replace «Status», «Usage» and «Install» with the M1 reality:
   `cargo install --git https://github.com/neitanod/searchfor` until crates.io; the examples of
   C9 test cases 1, 2, 4 and 5 with their real output (copy from the snapshots after C9; until
   then, mark them as to-be-filled and C9 fills them); the list of searchfor's own flags; a
   section **For AI agents** with: why (`--expand` saves reading the whole file), `--json`
   recipe with `jq '.data.trail | map(.name) | join(" › ")'`, `--within` recipe; a link to
   `docs/rg-compat.md`; and the note on `end.stats` counting Matches before filtering (design
   note 6). Formats beyond Markdown appear under «Roadmap» with their milestone.
2. **docs/json-schema.json**: JSON Schema (draft 2020-12) of the fields searchfor adds to
   `match.data`: `format` (string|null), `trail` (array of Segment objects with required
   `kind`, `name`, `line`, `end_line`, optional `level`, `signature`), `fragment` (null or
   object), `section` (optional object).
3. **skills/searchfor/SKILL.md** (Claude Code skill, English, frontmatter `name: searchfor`,
   `description:` "Use when searching Markdown notes, docs or knowledge bases and you need to
   know which section each match belongs to — before running rg and opening whole files"):
   when to prefer `sfor` over `rg`, the three recipes (`--flat`, `--expand`, `--json` + jq),
   `--within`, and a warning that only Markdown has Trails in v0.1.

- [ ] Commit «Document v0.1 and add the Claude Code skill».

---

## After the plan: what stage 8 (first version) means for M1

- `cargo install --path .` on this machine, so `sfor` is on Sebastián's PATH.
- Copy `skills/searchfor/` to `~/.claude/skills/searchfor/` so Robotín uses it.
- Demo in the Hilo: the output of `sfor` over `~/robotin/knowledge` for a real query, tree and
  flat, plus the benchmark numbers.
- crates.io stays for M5 unless Sebastián asks earlier (grill #20 said «when v0.1 works»: ask him
  at stage 8 whether v0.1 counts).
