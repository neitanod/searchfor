# searchfor — v1 design

Status: draft · 2026-10-16, aligned with the M1 implementation · Task 768 · source of truth
for decisions: [grill-respuestas.md](grill-respuestas.md) (numbers below, like **#6**, refer to its questions)
and [../../CONTEXT.md](../../CONTEXT.md) for the vocabulary.

## 1. Goal

`sfor` searches like ripgrep and, for every **Match**, shows its **Trail**: where the Match
sits inside the structure of its file. A person reads the Trail to decide whether a Match is
the one they want without opening the file; an AI agent reads it to avoid loading a whole
file just to learn which section a line belongs to.

The market check of 2026-10-16 found Markdown-only tools that print a heading Trail
(`jutley/mdgrep`, `riadafridishibly/mdgrep`) and code-only tools (`grep-ast`). Nothing combines
every Format behind one ripgrep-compatible command with one Trail model and one JSON
contract. That combination is the product.

## 2. Scope

### In v1

| Area | What ships | Grill |
|---|---|---|
| Formats | Markdown, YAML, JSON, TOML, code via tree-sitter, HTML, XML, CSV, subtitles, ELF/PE/Mach-O; with `--docs`, PDF, Word/ODT, Excel/ODS, PowerPoint/ODP, EPUB and SQLite (§3, M4) | #1, 8380, 8390, 8395 |
| Engine | ripgrep's crates in-process ([ADR 0001](../adr/0001-embed-ripgrep-engine-as-library.md)) | #2 |
| Flags | Full ripgrep flag compatibility as the target, plus searchfor's own flags | #3 |
| Structural search | `--within`, `--expand`, `--in-headings` | #4 |
| Detection | extension, exact name, shebang, magic bytes, `--format` override | #5 |
| Format Definitions | embedded TOML + user directory + export command ([ADR 0002](../adr/0002-format-definitions-as-embedded-toml.md)) | #6 |
| External Processors | any language, over a documented protocol | #7 |
| Grammars | WebAssembly; all v1 Grammars embedded, extra ones installable with a helper flag (§7) | #8, 8361 |
| Trail details | §5 | #9–#12 |
| Output | tree on a TTY, one line per Match in a pipe, `--json` = ripgrep's messages plus `trail`; Fragments for long lines | #13–#15, 8365 |
| Agents | Claude Code skill, README section for agents, MCP server | #19 |
| Distribution | crates.io, GitHub Releases, Homebrew tap, npm | #18 |
| Repo | public from the first commit, MIT OR Apache-2.0 | #17 |

### Out of v1

- Semantic or fuzzy search, indexes, embeddings. searchfor is exact and needs no setup.
- Editing or replacing text (`sd`/`srgn` territory). ripgrep's `--replace` output flag is in
  scope only as part of flag compatibility: it rewrites the printed line, never the file.
- A TUI or GUI.
- Debian/AUR packages (#18 chose four channels).
- Archive Formats: zip, tar, 7z, rar, deb, rpm, jar (8390). searchfor searches what ripgrep
  searches, and `-z` keeps ripgrep's meaning: single-stream compressed files (`.gz`, `.xz`,
  `.bz2`, …).

## 3. Pipeline

```
paths ─▶ walker (ignore crate: .gitignore, -g, -t, --hidden…)
          │
          ├─ regular file ──────────────────────────────┐
          └─ document (only with --docs) ─▶ extractor ──┤
                                                        ▼
                                    searcher (grep-searcher + grep-regex)
                                                        │ Matches, by file
                                                        ▼
                                   Detector ─▶ Format ─▶ Processor
                                                        │ Trail + Section per Match
                                                        ▼
                         filters (--within, --in-headings) ─▶ printer (tree | line | json)
```

1. **Search first, describe later.** The Detector and the Processor run only on files that
   have at least one Match. A tree with ten thousand files and three Matches pays for three
   Processor runs. This keeps the cost of searchfor over ripgrep proportional to the number
   of matching files.
2. **`--in-headings` is the exception**: it needs Trails to decide what counts as a Match, so
   with it every candidate file is processed. It is documented as slower.
3. **Documents with `--docs` (M4, decisions 8393–8395).** PDF, Word/ODT, Excel/ODS,
   PowerPoint/ODP, EPUB and SQLite are searched only when `--docs` is given. Built-in
   extractors, written in Rust with no external programs, turn each document into text plus
   its structure (chapter, sheet, slide, table, row, column), and that text goes through the
   same searcher, Detector and Processor as a file on disk. Without `--docs` those files are
   treated as ripgrep treats them. `--docs` is independent of `-z`, which keeps ripgrep's
   meaning (single-stream compressed files). ripgrep's `--pre` is implemented in the same
   milestone. Archive Formats (zip, tar, 7z, rar, deb, rpm, jar) are out of scope (8390).
4. **Binary files.** ripgrep prints `binary file matches` and stops. When the Detector
   recognises an object Format (ELF, PE, Mach-O), searchfor searches the whole file and
   describes each Match (§5.6). Any other binary file keeps ripgrep's behaviour, including
   `-a/--text` and `--binary`.

## 4. Formats, Detector and Format Definitions

### 4.1 Detector (#5)

Signals, in order; the first one that decides wins:

1. `--format <name>` given on the command line (applies to every file; repeatable as
   `--format '*.conf=toml'` to map a glob).
2. Exact file name (`Dockerfile`, `Makefile`, `.bashrc`, `Cargo.lock`).
3. Extension.
4. Leading magic bytes (ELF, `MZ`/PE, Mach-O, PDF `%PDF`, SQLite, zip `PK\x03\x04` for the
   Office and EPUB documents of `--docs`).
5. Shebang on the first line (`#!/usr/bin/env bash` → bash).

A file no signal decides is **Plain**: its Matches print with an empty Trail, exactly like
ripgrep. There is no content-sniffing heuristic.

### 4.2 Format Definition (#6)

```toml
# formats/php.toml
name       = "php"
extensions = ["php", "phtml"]
shebangs   = ["php"]
processor  = "treesitter"

[params]
grammar = "php"
# A tree-sitter query. Every @segment capture opens a Segment; @name labels it,
# @signature (optional) is the line shown in tree output (#10).
query = '''
(class_declaration name: (name) @name) @segment
(method_declaration name: (name) @name) @segment
(function_definition name: (name) @name) @segment
'''
kinds = { class_declaration = "class", method_declaration = "method", function_definition = "function" }
```

- Built-in Definitions live in `formats/` in the repo and are embedded at compile time.
- User Definitions live in `$XDG_CONFIG_HOME/searchfor/formats/*.toml`. A user file with the
  same `name` as a built-in replaces it; a new name adds a Format.
- `sfor --export-formats <DIR>` writes every embedded Definition to `<DIR>` (#6 comment). It
  refuses to write into the user formats directory, so an export never silently overrides
  the built-ins.
- `sfor --list-formats` prints every Format with its origin (built-in, user, overridden).
- An invalid user Definition is a warning naming the file and the field; the rest still load.

### 4.3 Processors

A Processor gets the bytes of one file (or of a document's extracted text), its Format
Definition parameters and the list of Matches (line numbers and byte offsets). It returns, for each Match, its Trail, and for
each Segment its Section (start and end line). That contract is the same for built-in and
External Processors.

| Processor | Formats | Built on |
|---|---|---|
| `markdown` | Markdown | pulldown-cmark, with source offsets |
| `keypath` | YAML, JSON, TOML | tree-sitter grammars, with code to compute array indexes |
| `treesitter` | code (PHP, JS, TS, Bash, Go, Rust, Python and every other installed Grammar), HTML, XML | a tree-sitter query from the Definition |
| `csv` | CSV, TSV | the csv crate |
| `object` | ELF, PE, Mach-O | goblin |
| `docs` extractors | PDF, Word/ODT, Excel/ODS, PowerPoint/ODP, EPUB, SQLite, only with `--docs` (M4) | Rust crates, no external programs; chosen in the M4 plan |
| `external` | anything | a command named in the Definition (§4.4) |

The generic `treesitter` Processor is what makes most new Formats a pure TOML change: adding a
language means installing its Grammar and writing a query.

### 4.4 External Processors (#7)

```toml
name       = "proto"
extensions = ["proto"]
processor  = "external"
[params]
command = ["proto-trail"]     # resolved through PATH
timeout_ms = 2000
```

Protocol, version 1, one process per request. The request's `op` says what is asked; both
operations mirror the built-in Processor contract (`trails` and `names`):

- **`"op":"trails"`**, the Trail of each Match:
  - stdin: `{"protocol":1,"op":"trails","path":"api/user.proto","format":"proto","matches":[{"line":42,"byte":1730}]}`
    followed by EOF; the External Processor reads the file content from `path`. For
    extracted document text (`--docs`), searchfor writes it to a temporary file and passes
    that path.
  - stdout: `{"trails":[[{"kind":"message","name":"User","line":40,"end_line":55}]]}` — one
    Trail per input Match, in the same order.
- **`"op":"names"`**, every searchable Segment name, for `--in-headings` (no `matches`):
  - stdin: `{"protocol":1,"op":"names","path":"api/user.proto","format":"proto"}` and EOF.
  - stdout: `{"names":[{"text":"User","pieces":[[0,4,1702,1706]],"anchor":1702,"trail":[…]}]}`.
    `text` is the name as shown. `pieces` says where each piece of `text` comes from, as
    `[text_start, text_end, src_start, src_end]` byte ranges: equal lengths when the text is
    verbatim, different when the Processor decoded it (an entity like `&lt;` is 1 byte of
    text over 4 of the file); a name spanning two lines has one piece per line. `anchor` is
    the byte where an empty hit (`^`, `$`) lands when no piece contains it (a name with no
    text). `trail` is the Trail of the named Segment, the Segment itself last.
- Non-zero exit, timeout or invalid JSON on `trails` → the Matches of that file print with an
  empty Trail and one warning on stderr naming the Definition. The same failure on `names`
  makes `--in-headings` skip that file, with one warning. A broken External Processor never
  aborts the search.

## 5. Trails

A Segment has `kind`, `name`, `line` and `end_line` (the lines of its Section), `level` for
headings, and an optional `signature` that tree output may show.

### 5.1 Markdown (#11)

- ATX (`#`) and setext (`===`, `---`) headings; headings inside code blocks and HTML blocks
  are ignored.
- The frontmatter `title`, when present, is the root Segment (`kind = "document"`).
- A fenced code block is a Segment named by its info string (`kind = "code"`, `name = "bash"`).
- A Match in a table row adds a Segment for its column, named by the header cell
  (`kind = "column"`). A Match spanning several cells uses the cell where the Match starts.
  The Section of a `column` Segment is its table.
- Parent list items are Segments (`kind = "item"`, `name` = first 40 characters of the item).
  #11 chose to include them; they are shown in tree output and in `--json`, and collapsed to
  `…` in the one-line output when the Trail would exceed the terminal width.

### 5.2 YAML, JSON, TOML

Each key is a Segment; array elements add `[n]` to the previous Segment's name, zero-based:
`server › ports[2] › host`. TOML tables and arrays of tables follow the same model. A Match
inside a multi-line string belongs to that key.

### 5.3 Code (#10)

Segments come from the Definition's query. One-line output shows names
(`class Cart › addItem`); tree output shows the signature line of each Segment
(`public function addItem(Item $item, int $qty = 1): void`).

### 5.4 HTML and XML

Each element is a Segment named `tag#id.class1.class2` (HTML) or the tag name (XML). Elements
with neither id nor class collapse runs of the same tag: `div › div › div` stays as is, but
`ul › li` gets its index, `li[3]`.

### 5.5 CSV

Segments: `row 128` and `column «Plan»`, from the header row. A file whose first row is not a
header (`--no-headers`) names columns by number.

### 5.6 Object files (#12)

Trail: `ELF x86-64 › .rodata › main.version+0x10`. The Match line shows the offset in hex and
the printable string around the Match, as `strings` would extract it. The nearest symbol is the
closest preceding symbol in the same section; when the binary is stripped, that Segment is
omitted.

### 5.7 Documents (`--docs`, M4)

Decisions 8390 and 8393–8395 replaced the Containers of the first draft. Archive Formats (zip,
tar, 7z, rar, deb, rpm, jar) are out of scope. PDF, Word/ODT, Excel/ODS, PowerPoint/ODP, EPUB
and SQLite are searched with `--docs` (§3): their extractor returns the text and the
structure, and the Trail shows it, for example `report.pdf › 3 Results › 3.2 Costs` or
`data.sqlite › users › row 12 › email`. The exact Trail of each document Format is fixed in
the M4 plan.

## 6. Output (#13–#15)

### 6.1 Tree (default on a TTY)

Real output of `sfor needle-pkg md --tree` over `tests/fixtures/`:

````
md/guide.md
  Deploy guide
    # Install
      ## Linux
        ```bash
          12:apt install needle-pkg
````

Matches sharing a Trail prefix are grouped under it. Each Match line is `line:text`, and a
context line (`-A/-B/-C`) is `line-text`, under its Match, as in ripgrep. Headings show their
Markdown markers, a code block its fence and info string, a table column `column «Name»`.

### 6.2 One line per Match (default in a pipe, or `--flat`)

```
md/guide.md:12: Deploy guide › Install › Linux › bash │ apt install needle-pkg
```

The layout is `path:line: Trail │ text`; with an empty Trail (Plain files) it is
`path:line: text`. A line with Matches in Segments with different Trails (two table cells)
prints once per Trail. `--no-trail` prints exactly what ripgrep would print, for scripts that
expect it.

### 6.3 Fragments of long lines

- A line longer than the available width is shown as a Fragment centred on the Match, with
  the omitted count on each side: `…[+1834] text before MATCH text after […+2210]`. Widths
  count characters (`char`s); the display width of wide characters is out of scope for v0.1.
- Width: the terminal width on a TTY; `--fragment-width`, by default 200, in a pipe and in
  `--json`. In tree and one-line output the width covers the whole printed line, so the
  Fragment gets what the path, line number, indentation and Trail leave; in `--json` the
  Fragment gets the whole width.
- **Markers.** `…[+N] ` before and ` […+M]` after take `5 + digits(line length)` characters
  each, and both are reserved even when one turns out empty, so every Fragment of a line has
  the same text width: `inner = width − 2 × marker`, never below 10 characters. With a width
  too small for 10 characters plus the markers, the Fragment is wider than the width; that is
  the defined behaviour for tiny widths.
- **Clusters** (#14). Matches are taken in order; a Match joins the current cluster when the
  cluster, extended to the Match's end, still fits in `inner` characters; otherwise it starts
  a new cluster. Each cluster is one Fragment, printed one under the other, and each Fragment
  highlights only its own cluster's Matches, so every Match is shown once.
- ripgrep's `-M/--max-columns` keeps its meaning: with it, searchfor prints ripgrep's
  `[Omitted long matching line]` messages instead of Fragments.

### 6.4 `--json` (#15, follow-ups 8362 and 8365)

`--json` emits ripgrep's JSON Lines messages — `begin`, `match`, `context`, `end`, `summary`,
one per line, so it streams — and adds three fields inside each `match.data`
([ADR 0004](../adr/0004-json-extends-ripgrep.md)):

The `match` message for line 12 of `sfor --json needle md` over `tests/fixtures/` (real output,
one line, wrapped here):

```json
{"type":"match","data":{"path":{"text":"md/guide.md"},"lines":{"text":"apt install needle-pkg\n"},
 "line_number":12,"absolute_offset":96,"submatches":[{"match":{"text":"needle"},"start":12,"end":18}],
 "format":"markdown",
 "trail":[{"kind":"document","name":"Deploy guide","line":1,"end_line":38},
          {"kind":"heading","name":"Install","line":5,"end_line":34,"level":1},
          {"kind":"heading","name":"Linux","line":9,"end_line":18,"level":2},
          {"kind":"code","name":"bash","line":11,"end_line":13}],
 "fragment":null}}
```

- Every existing consumer of `rg --json` keeps working: it ignores the added fields.
- `format` is the Format's name, or `null` for Plain files (whose `trail` is `[]`).
- `trail` is the Trail of the line's first submatch; for an inverted Match (`-v`), of the line
  start.
- `fragment` is `null` unless the line exceeds `--fragment-width`; then it is the line's
  first Fragment,
  `{"text":"…","start":<byte offset of the Fragment in the line>,"omitted_before":N,"omitted_after":M}`.
- With `--expand`, every `match.data` also carries the Section of the last Segment of
  `trail`, text included: `"section":{"start_line":30,"end_line":55,"text":"…"}`, or `null`
  when the Trail is empty.
- **Per-submatch Trails.** When the submatches of one line have different Trails (two cells of
  one table row), every object in `submatches` also carries its own `trail`; with `--expand`,
  a submatch whose Section differs from the line's also carries its own `section`. When all
  submatches share the line's Trail, these fields are absent.
- The `stats` of `end` and `summary` count the Matches found by the search, before
  `--within` and `--in-headings` filter them.
- A document searched with `--docs` (M4) keeps the document in `path`; its structure appears
  as Segments of `trail`.
- Added field names are stable from v0.1; new ones may appear, existing ones are never renamed.
  Their schema lives in `docs/json-schema.json`.
- `--json --no-trail` prints the same messages as `rg --json`; only the timings differ.

## 7. Grammars (#8, follow-up 8361)

Requirements from #8: every Grammar included in the default install, extra ones easy to add,
and a helper command to install them.

Grammars are WebAssembly modules loaded through tree-sitter's `wasm` feature, the approach the
Zed editor uses ([ADR 0003](../adr/0003-grammars-as-wasm.md)).

- The Grammars needed by the Formats of §2 (markdown, yaml, json, toml, html, xml, php,
  javascript, typescript, bash, go, rust, python) are embedded in the binary.
- Extra Grammars live in `$XDG_DATA_HOME/searchfor/grammars/<lang>.wasm`; a file there with
  the name of an embedded Grammar replaces it.
- `sfor --install-grammar <lang>` downloads one `.wasm` from the searchfor Grammar index — a
  JSON file in the GitHub repository listing language, source repository, version, download
  URL and SHA-256 — verifies the checksum, and writes it to the Grammar directory. The same
  file works on every platform.
- `sfor --list-grammars` shows embedded, installed and available Grammars.
- A Format Definition whose Grammar is missing is listed by `--list-formats` as unavailable,
  and its files are treated as Plain with one warning per run.

Cost to measure in the first chunk that parses code: the size wasmtime adds to the binary, and
parsing speed against a native Grammar. Only files with Matches are parsed (§3).

## 8. Command line (#3, #4)

- **ripgrep compatibility.** ripgrep 14.1.1 has 146 flag names: 104 flags in `rg --help`,
  30 `--no-` negations and 12 negations with other names (counted from its source in M1, C2).
  v1 accepts all of them by name; they are implemented in order of use. v0.1 implements 51
  and accepts the other 95 as «not supported yet». A flag that is accepted but not yet
  implemented fails with `sfor: --<flag> is not supported yet` and exit code 2, never silently
  ignored. The compatibility table lives in `docs/rg-compat.md` and is checked by a test that
  parses `rg --help`.
- **searchfor's own flags** use names ripgrep does not use:

| Flag | Meaning |
|---|---|
| `--within <TRAIL>` | keep Matches whose Trail contains these Segment names, in order; `›`, `>` or `/` separate them; each name is a case-insensitive substring |
| `--expand` | print the whole Section of the last Segment of each Match, once per Section |
| `--in-headings` | match only inside Segment names (headings, keys, function names) |
| `--format <F>` / `--format <GLOB>=<F>` | force a Format |
| `--flat` / `--tree` | force an output layout |
| `--no-trail` | ripgrep-identical output |
| `--fragment-width <N>` | Fragment width when there is no terminal |
| `--list-formats`, `--export-formats <DIR>` | Format Definitions |
| `--install-grammar <LANG>`, `--list-grammars` | Grammars |
| `--docs` | search PDF, Word/ODT, Excel/ODS, PowerPoint/ODP, EPUB and SQLite through the built-in extractors (M4, 8395) |
| `--mcp` | run as an MCP server over stdio |

Management actions are flags, not subcommands, because `sfor formats` must keep meaning
"search for the word formats", as `rg formats` does.

## 9. Agents (#19)

- **MCP server** (`sfor --mcp`): one tool, `search`, with `pattern`, `paths`, `within`,
  `expand`, `glob`, `case_insensitive`, `max_results`; returns the `--json` matches.
- **Claude Code skill** in `skills/searchfor/SKILL.md`: when to use `sfor` instead of `rg`,
  with `--within` and `--expand` recipes.
- **README section "For AI agents"** with copy-ready commands.

## 10. Distribution (#17, #18, #20)

- Public repository: https://github.com/neitanod/searchfor (account chosen in 8366).
- `dist` (formerly cargo-dist) builds GitHub Releases for Linux, macOS and Windows on x86-64
  and arm64, and publishes the Homebrew tap and the npm package from the same configuration.
- The crate `searchfor` is published on crates.io when v0.1 works (#20).

## 11. Acceptance criteria

1. `sfor PATTERN` over a tree with no structured files prints byte-for-byte what `rg PATTERN`
   prints when given `--no-trail`, for the flags marked implemented in `docs/rg-compat.md`.
2. Every Format of §2 has a fixture under `tests/fixtures/` and a snapshot test of its tree,
   one-line and JSON output.
3. A Match inside a fenced code block under `## Linux` under `# Install` gets the Trail
   `Install › Linux › bash`.
4. A Match in `config.yaml` at `server.ports[2].host` gets that Trail.
5. A Match inside a PHP method gets `class Cart › addItem`, and the tree output shows the
   method's signature.
6. A Match in a stripped ELF binary shows format, section, offset and string, and no symbol.
7. With `--docs`, a Match in a PDF gets the PDF's chapter Trail; without `--docs`, the PDF
   is treated as ripgrep treats it. A zip is never opened (8390).
8. A 20 000-character minified JSON line with three Matches prints Fragments no wider than
   the terminal, with correct omitted counts.
9. `--within "Install › Linux"` keeps only Matches under that Trail; `--expand` prints each
   Section once.
10. A user Definition in the formats directory overrides the built-in with the same name;
    `--export-formats` refuses the formats directory.
11. An External Processor that crashes leaves its Matches with an empty Trail and a warning;
    the exit code is still ripgrep's (0 if anything matched).
12. Every `match` message of `--json` carries `trail`, `format` and `fragment` valid against
    `docs/json-schema.json`; `--json --no-trail` equals `rg --json`, timings aside, for the
    implemented flags.
13. Performance: on `~/robotin/knowledge` (619 Markdown files on 2026-10-16), `sfor` takes at
    most 1.5× the wall time of `rg` for a pattern matching under 5 % of the files, measured as
    the median of 10 runs with `hyperfine`.
14. `sfor --mcp` answers a `search` call from Claude Code.
