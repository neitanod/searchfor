# searchfor — v1 design

Status: draft · 2026-10-17, aligned with the M1 implementation, the M2 decisions of the Hilo
(8449–8459), the M2 implementation (v0.2), the decisions of the M3 plan and the M3
implementation (v0.3) · Task 768 · source of
truth for decisions: [grill-respuestas.md](grill-respuestas.md) (numbers below, like **#6**, refer
to its questions; four-digit numbers are Hilo messages; «M2 plan» and «M3 plan» mark a decision
taken in [the M2 plan](../plans/2026-10-17-searchfor-m2.md) or
[the M3 plan](../plans/2026-10-17-searchfor-m3.md) where Sebastián left it open)
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
| Formats | Markdown, YAML, JSON, TOML, code via tree-sitter, HTML, XML, CSV and TSV, subtitles (SubRip, WebVTT), ELF/PE/Mach-O; with `--docs`, PDF, Word/ODT, Excel/ODS, PowerPoint/ODP, EPUB and SQLite (§3, M4) | #1, 8380, 8390, 8395 |
| Engine | ripgrep's crates in-process ([ADR 0001](../adr/0001-embed-ripgrep-engine-as-library.md)) | #2 |
| Flags | Full ripgrep flag compatibility as the target, plus searchfor's own flags | #3 |
| Structural search | `--within`, `--expand`, `--in-headings` | #4 |
| Detection | extension, exact name, shebang, magic bytes, `--format` override | #5 |
| Format Definitions | embedded TOML + user directory + export command ([ADR 0002](../adr/0002-format-definitions-as-embedded-toml.md)) | #6 |
| External Processors | any language, as Helper Processors over protocol v1 (§4.4) | #7, 8451–8459 |
| Grammars | compiled into sfor for the v1 Formats; any other language as a Grammar Helper installed with `--install-grammar` (§7, [ADR 0005](../adr/0005-grammars-compiled-in-extras-as-helper-processors.md)) | #8, 8361, 8451–8454 |
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
   describes each Match (§5.7). Any other binary file keeps ripgrep's behaviour, including
   `-a/--text` and `--binary`.
5. **Dense Matches (pending, M5).** The pipeline keeps the Trails of a whole file before
   printing it, so a file where every line matches costs time and memory in proportion to its
   Matches, in every Format. Measured in the M3 plan on 50 MB files where every line matches
   (2026-10-17): JSON Lines 3.51 s and 876 MiB, Markdown 3.62 s and 409 MiB, CSV 5.36 s and
   1.48 GiB, against 0.16–0.46 s with `--no-trail`. M3 reports this cost in
   `scripts/bench-m3.sh` and bounds nothing (M3 plan, D16); streaming the Trails of such a file
   is left for M5, whose flag work touches the printers anyway.

## 4. Formats, Detector and Format Definitions

### 4.1 Detector (#5)

Signals, in order; the first one that decides wins:

1. `--format <name>` given on the command line (applies to every file; repeatable as
   `--format '*.conf=toml'` to map a glob).
2. Exact file name (`Dockerfile`, `Makefile`, `.bashrc`, `Cargo.lock`).
3. Extension.
4. Leading magic bytes (ELF, `MZ`/PE, Mach-O, PDF `%PDF`, SQLite, zip `PK\x03\x04` for the
   Office and EPUB documents of `--docs`; `<?xml` for XML, M3 plan).
5. Shebang on the first line (`#!/usr/bin/env bash` → bash).

Magic bytes and shebangs (signals 4 and 5) are read only for a file whose name has no extension
(M2 plan). A file with an extension no Definition claims (`.txt`, `.c`) is decided Plain by its
path alone, so it keeps ripgrep's printing path even though the code Definitions declare
shebangs; Object Formats (M4) list their extensions (`.so`, `.o`, `.dll`) besides their magic.

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
# A tree-sitter query. Every @segment capture opens a Segment and @name names it. Optional:
# @prefix is joined to the name with a dot (Go's method receivers: `Server.Start`), and
# @signature replaces the Segment's first line as the line shown in tree output (#10).
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

- Built-in Definitions live in `formats/` in the repo and are embedded at compile time.
- An installed Grammar Helper brings its own Definition (§7), loaded after the built-ins.
- User Definitions live in `$XDG_CONFIG_HOME/searchfor/formats/*.toml`, loaded last. A user file
  with the same `name` as a built-in or installed Definition replaces it; a new name adds a
  Format.
- `sfor --export-formats <DIR>` writes every embedded Definition to `<DIR>` (#6 comment). It
  refuses to write into the user formats directory, so an export never silently overrides
  the built-ins.
- `sfor --list-formats` prints every Format with its origin (built-in, user, overridden).
- An invalid user Definition is a warning naming the file and the field; the rest still load.

### 4.3 Processors

A Processor gets the bytes of one file (or of a document's extracted text), its Format
Definition parameters and the list of Matches (line numbers and byte offsets). It returns, for
each Match, its Trail, and for each Segment its Section (start and end line). That contract is
the same for built-in Processors and Helper Processors: for a Helper Processor, sfor implements
it by speaking protocol v1 (§4.4).

| Processor | Formats | Built on |
|---|---|---|
| `markdown` | Markdown | pulldown-cmark, with source offsets |
| `keypath` | YAML, JSON, TOML (the Definition's `syntax`) | native parsers: a JSON scanner of our own, `saphyr-parser` for YAML (its character offsets converted to bytes), `toml_edit` for TOML |
| `treesitter` | code: PHP, JavaScript, TypeScript, TSX, Bash, Go, Rust and Python with their Grammars compiled in; any other language through an installed Grammar Helper (§7) | a tree-sitter query from the Definition |
| `markup` | HTML, XML (the Definition's `syntax`) | a tag scanner of our own (§5.4) |
| `csv` | CSV, TSV | `csv-core`, which reads fields and reports the bytes each one takes (§5.5) |
| `subtitles` | SubRip (`.srt`), WebVTT (`.vtt`) | a line scanner of our own (§5.6) |
| `object` | ELF, PE, Mach-O | goblin |
| `docs` extractors | PDF, Word/ODT, Excel/ODS, PowerPoint/ODP, EPUB, SQLite, only with `--docs` (M4) | Rust crates, no external programs; chosen in the M4 plan |
| `external` | anything | a Helper Processor named in the Definition (§4.4) |

`keypath` uses native parsers instead of tree-sitter (M2 research, 2026-10-16): tree-sitter-yaml
turns any file over 32 768 lines into one error node (its scanner counts rows in an `int16_t`),
and a lock file can pass that size; tree-sitter's TOML tree does not model TOML tables, so
`[[products.variants]]` would need TOML's table rules reimplemented; and the JSON scanner took
11 ms on an 859 KB file where tree-sitter took 300 ms.

`markup`, `csv` and `subtitles` scan the file themselves instead of using a tree-sitter Grammar
(M3 plan, measured on this machine on 2026-10-17): tree-sitter-html 0.23.2 parsed a generated
20 MB HTML file at 5.0 MB/s and its query took 1.5 s more, and tree-sitter-xml 0.7.0 parsed a
20 MB XML file at 3.9 MB/s, while ripgrep searches either in 13 ms; the `markup` scanner walks
them, Trails included, in about 0.15 s and 0.11 s. A Grammar would make one HTML file with a Match
cost seconds, against 8385. The three keep in memory only what the Matches need, as `keypath`
does, and read a file no further than its last Match where the Format allows it (CSV, subtitles).
They have no `max_bytes`: each walks a file once, at 60 MB/s or more on this machine (66 MB/s on
a tag-dense 50 MB HTML file, the slowest case measured; §11 #13).

The generic `treesitter` Processor is what makes most new languages a TOML change: a query in a
Format Definition, over a Grammar that is compiled in or installed as a Grammar Helper.

### 4.4 Helper Processors (#7, 8451–8459)

A **Helper Processor** is a Processor that runs as a separate program and talks to sfor over
protocol v1: JSON lines on its stdin and stdout. It is the one mechanism behind both External
Processors, written by anyone in any language, and the Grammar Helpers of §7 (8453).

```toml
name       = "proto"
extensions = ["proto"]
processor  = "external"
[params]
command    = ["proto-trail", "--strict"]   # resolved through PATH
mode       = "pool"                         # "pool" (default), "concurrent" or "oneshot"
timeout_ms = 5000                           # per request; 5000 when absent
```

**Lifetime: once per search (8455).** sfor starts a Helper Processor when the first file of its
Format needs Trails — after the search found a Match in that file (§3) — sends it every other
file of that Format in the same run, and closes its stdin when the run ends. The helper exits on
that EOF; sfor waits up to one second and then kills it. The next run starts it again. Every
process sfor starts is also reaped: a copy that crashed, timed out or was killed is collected at
once, never left as a zombie until sfor exits (M2 plan, D32). Helpers run in sfor's process
group, so a terminal's Ctrl-C reaches them too. On Unix, before it starts its first helper, sfor
also watches SIGINT and SIGTERM: on one, it stops starting helpers, kills and reaps every helper
— including one that ignores the signal, one being started at that moment, and also when the
signal was sent to sfor alone — and then dies by that signal, as ripgrep would (M2 plan, D42). A sfor killed with SIGKILL leaves its helpers an EOF
on stdin. A helper that starts processes of its own must stop them when it exits; sfor waits
at most one more second for pipes such a child keeps open, and then exits anyway.

**Modes (8458, 8459)**, chosen by `mode` in the Definition:

- **`pool`** (default, M2 plan): each copy of the helper has one request at a time. sfor starts a
  new copy only when every running copy is busy, up to the search's thread count (`-j`, or
  ripgrep's default), and gives the files to whichever copy is free. A helper that reads a line,
  answers it and reads the next one is correct in this mode; that is why it is the default.
- **`concurrent`**: one process, which receives up to the search's thread count of requests
  without waiting for answers and may answer them in any order: each answer carries the `id` of
  its request.
- **`oneshot`** (8455): one process per request. sfor writes `hello` and the request, closes
  stdin, and then reads the two answers, to `hello` and to the request, so a script may read its
  whole stdin before writing anything. Meant for helpers written as small scripts; each file
  pays the start of a process (1.5–3 ms, measured in 8452).

**Wire format.** UTF-8 JSON, one object per line, ended by `\n`, in both directions. Both sides
ignore fields they do not know, so later versions can add fields. Every line the helper writes to
stderr is relayed as a warning, `sfor: <format>: <line>`, silenced by `--no-messages`; once a
SIGINT or SIGTERM has cancelled the run (Lifetime, above), nothing more is relayed, as ripgrep
prints nothing then.

1. **`hello`**, the first line of every process:
   `{"id":0,"op":"hello","protocol":1,"format":"proto","params":{…},"threads":8}`. `params` is
   the Definition's `[params]` table and `threads` the most requests sfor will have in flight to
   this process (1 in `pool` and `oneshot`). The helper answers `{"id":0,"protocol":1}` and may
   add `"name"` and `"version"`. Another protocol number, an `error` answer, no answer within
   `timeout_ms`, or a process that exits before it answers disables the Definition for the
   run, with one warning.
2. **`trails`**, the Trail of each Match:
   `{"id":7,"op":"trails","path":"api/user.proto","matches":[{"line":42,"byte":1730}]}` →
   `{"id":7,"trails":[[{"kind":"message","name":"User","line":40,"end_line":55}]]}`, one Trail
   per Match, in the same order. `byte` is the absolute byte offset of the Match in the file at
   `path`, and `line` its 1-based line. A Segment is the object of `--json` (§6.4): `kind`,
   `name`, `line`, `end_line`, and optional `level` and `signature`.
3. **`names`**, every searchable Segment name, for `--in-headings`:
   `{"id":8,"op":"names","path":"api/user.proto"}` →
   `{"id":8,"names":[{"text":"User","pieces":[[0,4,1702,1706]],"anchor":1702,"trail":[…]}]}`.
   `text` is the name as shown. `pieces` says where each piece of `text` comes from, as
   `[text_start, text_end, src_start, src_end]` byte ranges: equal lengths when the text is
   verbatim, different when the Processor decoded it (an entity like `&lt;` is 1 byte of
   text over 4 of the file); a name spanning two lines has one piece per line. `anchor` is
   the byte where an empty hit (`^`, `$`) lands when no piece contains it (a name with no
   text). `trail` is the Trail of the named Segment, the Segment itself last.
4. **Errors**: `{"id":7,"error":"message"}` fails that request only.

**Paths.** sfor sends the path and the Match positions, never the content (8452); the helper reads
the file. `path` is relative to sfor's working directory, which the helper inherits, or absolute.
sfor sends a file's own path only when it is a regular file whose bytes sfor searched unchanged,
outside `/dev` and `/proc`, and then it sends the resolved (canonical) path, never the one typed. For anything else — stdin, `/dev/stdin`, `/dev/fd/N` or a FIFO named
on the command line (a helper must never reopen a stream; `/dev/stdin` redirected from a file is
a regular file, and in the helper it is the protocol pipe), a file whose BOM sfor removed or whose UTF-16 it transcoded, a path that is not
UTF-8, a document's extracted text with `--docs` (M4) — sfor writes the bytes it searched to a
temporary file of the run, removed after the answer, and sends that path, so `byte` always
counts the bytes at `path`.

**Releasing resources (8458).** Requests are independent: a helper keeps nothing from one file
for the next, and frees the file, its tree and its answer before reading the next request. Only
what belongs to the Format stays loaded: the Grammar, the compiled query, the `params` of
`hello`. searchfor's own helpers are tested to keep their memory flat over thousands of requests
(§11).

**Failures.** A broken Helper Processor never aborts the search and never changes the exit code.
The thresholds below are M2 plan decisions.

- A file whose request fails prints as Plain — `format` `null` in `--json`, empty Trails, the
  layout of §6.2 — with one warning naming the file and the Definition,
  `sfor: <path>: <format>: <reason>`. For `names`, `--in-headings` skips that file, with one
  warning.
- An `error` answer, a wrong number of Trails, or a JSON line with the request's `id` whose
  fields do not fit the protocol fails that request only; the process goes on.
- A line that is not JSON, or that has no `id`, cannot be attributed to a request: the process
  is killed and every request it had in flight fails.
- No answer within `timeout_ms` (counted from when sfor queues the request, so a helper that
  stops reading cannot stall sfor): the request fails, and the process is killed, which fails
  the other requests it had in flight.
- A process that exits, or closes its stdout, with requests in flight fails them as above; the
  next request starts a new process (8455). A process that exits between requests is reaped at
  once, and the next request replaces it without counting a failure. After three failures in
  one run — crashes, timeouts or lines that are not valid JSON or have no `id`, each process
  counted once however many requests it failed — the Definition is disabled for the rest of
  the run, with one warning, and its files print as Plain. An `error` answer and a reply whose
  fields do not fit the protocol are not counted.
- A command that cannot be started disables the Definition for the run, with one warning.
- While a Definition is disabled, its files print as Plain with no further warning.
- A `treesitter` Definition whose Grammar is neither compiled in nor installed prints its files
  as Plain with one warning per run (§7), printed when the first file of that Format with a
  Match needs Trails; a run in which no such file matches prints none.

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

- Each key is a Segment (`kind = "key"`). An array element is a Segment (`kind = "element"`)
  named after the key that holds the array plus its zero-based index, and that key is hidden,
  so the Trail reads `server › ports[2] › host`. Nested arrays chain the indexes
  (`matrix[1][1]`); the elements of a top-level array are `[0]`, `[1]`…
- Real output over the fixtures (`tests/fixtures/data/`, one-line layout):

  ```
  config.yaml:9: server › ports[2] › host │     - host: needle.example
  config.json:10: server › matrix[1][1] │     "matrix": [[1, 2], [3, "needle"]]
  shop.toml:16: products[1] › variants[0] › sku │ sku = "needle-sku"
  shop.toml:20: database › replica › inline › user │ inline = { user = "needle" }
  multi.yaml:4: document 2 › second[0] │   - needle
  ```

  The tree layout shows every Segment by its name, one per level (`server`, `ports[2]`,
  `host`).
- TOML tables and arrays of tables follow the same model: the second `[[products]]` is
  `products[1]`, and a table's Section runs to the next header that is not inside it, so
  `[[products.variants]]` lands under `products[1]`. A dotted key (`a.b.c = 1`) is one Segment
  per part.
- A Match inside a multi-line string belongs to its key. A Match in the text of a key itself is
  not inside that key, as a Match on a heading line (§5.1).
- A YAML stream with more than one document gives each document a Segment named `document N`
  (`kind = "document"`, 1-based, as `row 128` and `page 42`). Merge keys are keys named `<<`;
  an alias is text of the key it sits in; a complex key (`? [a, b]`) is named by its first 40
  characters.
- JSON accepts comments and trailing commas (`tsconfig.json`, editor settings) and nothing
  else outside JSON: a malformed number, literal, escape or control character is a syntax
  error. A key is named by its decoded text (`"caf\u00e9"` is `café`). A file may hold several
  top-level values, read in turn, so each line of a JSON Lines file gets its own Trails
  (`{"b": {"c": "needle"}}` gives `b › c`); a Match in a comment between keys has the Trail of
  the container it sits in, empty at the top level. Past 256 nested containers no more
  Segments open; deeper Matches get the Trail of the deepest one, and nesting of any depth
  never crashes sfor (M2 plan).
- TOML: a table's Section runs from its header to the next header that is not inside it. When a
  header's parent has no table of its own whose Section holds that header (`a` of `[a.b]`, or
  `[p]` whose Section ended before a later `[p.r]`), the parent is repeated over that header's
  Section, so `[a.b]`, `[x]`, `[a.c]` give `a › b`, `x`, `a › c`, never `a › x`. A quoted key
  holding a dot (`["a.b"]`) is one key. Dotted keys spread over a table give the parent per
  key-value.
- A key that holds an array is hidden in the Trail but remains a name for `--in-headings`:
  `--in-headings '^ports$'` finds `ports:`.
- On a syntax error the Trails of the Matches before it are kept, and the Matches after it print
  with an empty Trail, without a warning: Helm templates are often not valid YAML, and a warning
  per file would bury the output (M2 plan). For TOML, toml_edit can report an error many lines
  after the unclosed string or array that caused it, so «before it» is the longest prefix sfor
  finds that parses and ends at a statement start candidate (a line whose first non-blank
  character is `[`, or a key character on a line that contains `=`) at or before the reported
  error's line. The candidate filter is a heuristic: a line inside a string or an array that
  looks like a statement is a candidate too, and only costs a try, so when the budget runs out
  the best prefix found may end well before the damage, and the keys between them lose their
  Trails. sfor looks for it by stepping back 1, 2, 4… lines from the error's line (the last line
  when the error is at the end of the file), each cut moved up to a statement start, then
  bisecting, then trying the statement starts it skipped, from the one nearest the error towards
  the start of the file, never parsing a cut twice, and stops when it has parsed 64 times the
  file's size, or 32 MiB, with the best prefix found (M2 plan, Task 3.2).

### 5.3 Code (#10)

Segments come from the Definition's query (§4.2): every `@segment` capture is a Segment named
by its `@name`, with its kind from `kinds` (the node kind when absent). One-line output shows
the names (`Cart › addItem`), as it does for every Format; tree output shows the signature of
each Segment: the `@signature` capture, or else the first line of the Segment without a trailing
`{` (`public function addItem(Item $item, int $qty = 1): void`).

- A Go method is not nested in its type, so the query captures the receiver as `@prefix` and the
  Segment is named `Server.Start` (M2 plan).
- A Rust `impl` block is a Segment named by its type, `kind = "impl"`: `impl Thing` in the tree,
  `Thing` in the one-line output (M2 plan).
- A Match in a Segment's name (`addItem` on its own declaration line) is not inside that
  Segment, as a Match on a heading line (§5.1); a Match in its parameters or body is.
- A Segment covers its node: a PHP `namespace App\Shop;` statement ends at its `;`, so it holds
  nothing and appears in no Trail; a braced `namespace App\Shop { … }` does.
- The signature keeps everything but a trailing `{`: Python's `class Outer:` keeps its colon.
- A file over the Definition's `max_bytes` (16 MiB when absent) is not parsed: its Matches
  print as Plain, with one warning per Format per run (M2 plan).
- Real output over the fixtures (`tests/fixtures/code/`, one-line layout), one per code Format:

  ```
  cart.php:10: Cart › addItem │         // needle: merge equal items
  widget.js:11: Widget › handler │     console.log("needle clicked");
  shape.ts:6: Geometry › Base › describe │       return `needle ${this.area()}`;
  view.tsx:3: View › onClick │     console.log("needle");
  deploy.sh:9: deploy │   rsync -a build/ host:/srv/needle/
  server.go:8: Server.Start │ 	return listen(s.Addr, "needle")
  thing.rs:8: outer › Thing › helper │             self.needle + 1
  outer.py:4: Outer › method › inner │             return "needle"
  ```

  and the tree of `cart.php`, with the signatures:

  ```
  code/cart.php
    class Cart
      public function addItem(Item $item, int $qty = 1): void
        10:        // needle: merge equal items
    15:function needle_helper(): string
      17:    return "needle";
  ```

### 5.4 HTML and XML

The `markup` Processor reads both; the Definition's `syntax` (`html` or `xml`) chooses the rules.

- Each element is a Segment (`kind = "tag"`). In HTML it is named `tag#id.class1.class2`: the tag
  as written, then `#` and the value of `id`, then `.` and each word of `class`, in order. In XML
  it is named by its tag as written, prefix included (`dc:creator`). A name longer than the
  Definition's `name_chars` (40 characters) is cut with `…`.
- An element without an `id` whose parent has two or more child elements with its tag gets its
  1-based position among them, as XPath counts: `ul › li[3]`, `table › tr[3] › td[2]`,
  `catalog › book[2] › title`. An element alone with its tag has no index (`div › div › div`),
  and one with an `id` never has one. Empty elements count among the siblings. Tags compare
  without case in HTML and with case in XML (M3 plan).
- The Section of an element runs from its start tag to its end tag. A Match inside a start tag —
  in its name or in an attribute value — is not inside that element, as a Match on a heading
  line (§5.1): it gets the Trail of the element's parent. A Match in text, a comment or a CDATA
  section belongs to the element around it.
- An element that holds nothing but its tag is never a Segment: HTML's void elements (`area`,
  `base`, `br`, `col`, `embed`, `hr`, `img`, `input`, `link`, `meta`, `param`, `source`, `track`,
  `wbr`) and empty elements (`<x/>`, in both syntaxes).
- **HTML.** `html` and `body` are hidden, because they hold the whole page: Trails start below
  them (`nav.top.menu › ul › li[3]`), and `head` stays; `--in-headings` does not search their
  names either (M3 plan, D7). `script`, `style`, `textarea`, `title`,
  `xmp`, `iframe`, `noembed`, `noframes` and `plaintext` hold text up to their own end tag, so a
  `"</div>"` inside a script closes nothing. The end tags HTML lets an author leave out are
  inferred (M3 plan, D11): a `<li>` ends the innermost open `li`, searching inward-out and stopping
  at `ul`, `ol`, `menu`, `table`, `td`, `th` and `template`; `<td>` and `<th>` end the open cell,
  stopping at `tr`, `table` and `template`; `<tr>` the open row and `<thead>`, `<tbody>` and
  `<tfoot>` the open table section, stopping at `table` and `template`; `<dt>` and `<dd>` the open
  term or definition, stopping at `dl`, `table`, `td`, `th` and `template`; `<option>` the open
  option, stopping at `select`, `datalist` and `optgroup`; `<optgroup>` the open group, stopping at
  `select`, or else an option that is the current element; a heading start tag (`h1`–`h6`) a
  heading that is the current element; and the start of a block
  (`address`, `article`, `aside`, `blockquote`, `details`, `dialog`, `dd`, `div`, `dl`, `dt`,
  `fieldset`, `figcaption`, `figure`, `footer`, `form`, `h1`–`h6`, `header`, `hgroup`, `hr`, `li`,
  `main`, `menu`, `nav`, `ol`, `p`, `pre`, `section`, `table`, `ul`) ends an open `p`, unless a
  table cell, a `button` or another scope boundary of HTML lies between them. An end tag closes
  the innermost open element with its name and every element inside it; an end tag with no open
  element of its name is ignored. These rules are a subset of HTML's parsing algorithm, whose
  stop lists are longer: `<ul><li><div>a<li>b</ul>` gives two sibling items here, where a
  browser nests the second inside the `div`. A quote opens an attribute value only right after
  `=`, and an unquoted value runs to the next space or `>`, so the slashes of `href=/docs/` are
  part of it and only a `/` right before `>`, outside a value, makes a tag self-closing. An element ended this way has a Section that ends at its last
  byte that is not white space. searchfor describes the markup as written and never inserts the
  elements a browser would add: `<table><tr>` gives `table › tr`, with no `tbody`. HTML has no
  syntax errors: a construct the file ends inside (a comment, a tag) is dropped, as the HTML
  parser drops it.
- **XML.** The first construct that is not well formed — an end tag that does not close the
  current element, a `<` that starts no tag, comment, CDATA section, processing instruction or
  doctype, a tag whose name or attributes do not follow XML's syntax (`<a=b>`, an unquoted value
  `<a x=bare>`, a `<` inside a value, attributes without a space between them, `</a junk>`, a
  name with a character XML names do not allow, such as `<©>`, a form feed where XML wants white
  space, an attribute given twice), or one of those the file ends inside — ends the structure
  there. Matches before it
  keep their Trails and the Matches from it on print with an empty Trail, without a warning, as
  YAML and JSON (§5.2). Elements still open when the file ends end there. A doctype ends at the
  first `>` outside its quoted literals and, in its internal subset, outside comments and
  processing instructions, so `<!ENTITY e "]> <fake>">` declares no element. A Match in the
  doctype, its internal subset included, or in a processing instruction has the Trail of where
  it sits: empty before the root element.
- Past 256 open elements no more Segments open; deeper Matches get the Trail of the deepest one,
  and nesting of any depth never crashes sfor (as JSON, §5.2).
- `--in-headings` searches element names. The tag, the `id` and each class word come from the
  file; `#`, `.`, a cut's `…` and an index `[n]` are text of the name only, so a pattern that
  matches only them finds nothing. A cut name stays searchable up to its cut: an `id` of 50
  letters shows its first ones, and `--in-headings` finds them there, also when a byte that is
  not UTF-8 (a Latin-1 `é`) precedes them (M3 plan, D31).
- **Encodings.** The bytes are the ones ripgrep searches: a UTF-8 BOM removed, UTF-16 transcoded
  (§4.4, Paths). A `<meta charset>` or an `encoding=` declaration is not read: a name whose bytes
  are not UTF-8 shows each invalid sequence as U+FFFD. CRLF line ends need nothing.
- Real output over the fixtures (`tests/fixtures/markup/`, one-line layout):

  ```
  page.html:12: nav.top.menu › ul › li[3] │       <li><a href="/needle">Needle</a>
  page.html:23: main#content › section#pricing.card.wide › table › tr[3] › td[2] │         <tr><td>Pro<td>needle 9
  page.html:27: main#content │     <img src="needle.png" alt="">
  feed.xml:12: catalog › book[2] › title │     <title>The needle</title>
  broken.xml:3: root › a › b │   <a><b>needle inside</a>
  broken.xml:4:   <c>needle after</c>
  ```

  The tree layout shows each element by its name (`li[3]`, `section#pricing.card.wide`).

### 5.5 CSV and TSV

The `csv` Processor reads both, with four Definition parameters (M3 plan): `delimiter`, `quote`,
`headers` and `name_chars`.

- **Dialects.** CSV: `delimiter = "auto"`, which picks the most frequent of `,`, `;`, tab and `|`
  in the first line, outside quotes (`,` on a tie or when none appears), so the `;` files that a
  spreadsheet exports in many locales read as they are; `quote = "\""`. TSV: `delimiter = "\t"`
  and `quote = ""`, so quotes are text, as in IANA's TSV, and a stray `"` cannot swallow the
  rest of the file.
- **Trail.** `row N › Column`. The row Segment (`kind = "row"`) is named `row N`, the N-th record
  after the header, 1-based; blank lines are not records. The column Segment (`kind = "column"`,
  as in Markdown) is named by its header cell — quotes removed, `""` decoded, white space trimmed,
  cut at `name_chars` (40) with `…` — or `column K`, 1-based, when the header cell is empty, when
  the record has more fields than the header, or with `headers = false`. A Match spanning cells
  uses the cell where it starts; a field ends with the delimiter or the line break after it, so a
  Match that starts on a delimiter belongs to the field before it.
- **Sections.** The Section of a row is its record, which a quoted field may spread over several
  lines. The Section of a CSV column is its row: in CSV the table is the whole file, and
  `--expand` must print the record, not the file (M3 plan; in Markdown a column's Section is its
  table, §5.1).
- A Match in the header row has an empty Trail: it is in a column's name, as a Match in a key's
  own text (§5.2). `--in-headings` searches the header cells, a cut one up to its cut; a header
  cell that a stray quote made csv-core read leniently still names its column but is not
  searched.
- **Quotes.** As csv-core reads them: a quote opens a quoted field only as the field's first
  byte, and inside an unquoted field it is text; inside a quoted field `""` is a quote, and a
  closing quote followed by anything but a delimiter or a line break turns the rest of the field
  into text, whose quotes are literal (`"needle"junk"` is the closed field `needlejunk"`). When
  the file ends inside a quoted field, the structure ends at that record: Matches in it and
  after it print with an empty Trail, without a warning.
- sfor reads the records only up to the one that holds the file's last Match, so a Match on row
  3 of a 1 GB file reads three rows.
- **Encodings.** A UTF-8 BOM and UTF-16 as ripgrep (§5.4); a header whose bytes are not UTF-8
  (Latin-1 exports) shows each invalid sequence as U+FFFD. CRLF and LF line ends both end a
  record.
- A file without a header row is read with a Definition that says `headers = false`: copy
  `csv.toml` from `--export-formats` under another name (`csv-raw`, no extensions) and pick it
  with `--format 'exports/*.csv=csv-raw'`. The first draft's `--no-headers` flag is this
  parameter (M3 plan): searchfor's own flags apply to every Format.
- Real output over the fixtures (`tests/fixtures/table/`, one-line layout):

  ```
  plans.csv:3: row 2 › Plan │ 2,Bea,needle,"likes ""quotes"", and commas"
  plans.csv:5: row 3 › Notes │ lines with needle"
  semi.csv:3: row 2 › Precio │ Bea;pro;9,50 needle
  people.tsv:2: row 1 › Note │ "Ana	admin	needle "quoted
  ```

  The tree layout shows `row 2`, then `column «Plan»`.

### 5.6 Subtitles (8380)

The `subtitles` Processor reads SubRip (`.srt`) and WebVTT (`.vtt`) files; its Definitions have
no parameters (M3 plan).

- A **cue** is the block of a timing line, `START --> END` followed by optional settings
  (`align:start`, SRT coordinates). A timestamp is two or three groups of digits joined by `:`,
  then `,` or `.` and one to three digits, so `00:01:02,500`, `01:02.500` and `01:00:00.000` are
  all read.
- The cue Segment (`kind = "cue"`) is named `cue ID START`. ID is the cue's identifier line —
  the non-blank line right before the timing line, when a blank line or the start of the file
  precedes it and it is not itself a timing line: the SRT counter, or a WebVTT identifier, its
  first 40 characters, searchable up to the cut — and, when the cue has none, its 1-based
  position among the file's cues. START is the start time as written.
  `signature` is `cue ID START --> END`, which the tree layout shows.
- The Section of a cue runs from its identifier line, or its timing line, to its last line
  before a blank line, the next timing line, or the end of the file; a missing blank line between
  two cues still separates them. A Match in the identifier or the timing line is inside its cue,
  unlike a Match on a heading line: a cue has no parent to fall back to, and its time is what a
  person searches for.
- **WebVTT blocks.** In a file that starts with `WEBVTT`, its header block and every block whose
  first line is `NOTE`, `STYLE` or `REGION` (alone or followed by white space) run to the next
  blank line and hold no cue, even when a line in them looks like a timing line; they are not
  counted among the cues either. Here sfor departs on purpose from WebVTT's parsing algorithm,
  where a line with `-->` ends the open block and starts a cue (named `NOTE` when the block had
  one line): such a file is malformed either way, and a timing line inside a `NOTE` or `STYLE`
  block is almost always a comment about that time or a quoted example, not a cue someone wants
  to find by its number (M3 plan, D29). A file without the `WEBVTT` header follows SubRip's
  rules, where a line `NOTE` is an identifier like any other.
- A Match outside every cue — the `WEBVTT` header, a `NOTE`, `STYLE` or `REGION` block, text
  under no timing line — has an empty Trail. A timing line that does not parse starts no cue.
  Nothing in a subtitle file causes a warning.
- `--in-headings` searches cue names: ID (when it is an identifier line) and START come from the
  file, and `cue ` is text of the name only, so `--in-headings '01:02'` finds the cues that start
  at that time and `--in-headings cue` finds nothing.
- **Encodings.** The BOM many SRT files start with is removed and UTF-16 is transcoded, as by
  ripgrep (§5.4); text in Latin-1 is searched as ripgrep searches it, and timing lines are ASCII.
  CRLF, LF and files that mix them are read the same; a lone CR is not a line break, as for
  ripgrep.
- Real output over the fixtures (`tests/fixtures/subs/`, one-line layout; `movie.srt` has a BOM
  and CRLF line ends):

  ```
  movie.srt:8: cue 2 00:00:04,000 │ - A needle, in there?
  talk.vtt:3: NOTE needle in a note
  talk.vtt:10: cue 2 00:02.500 │ <v Ana>The needle is here.
  ```

  and the tree of `movie.srt`:

  ```
  subs/movie.srt
    cue 1 00:00:01,000 --> 00:00:03,500
      3:Where is the needle?
    cue 2 00:00:04,000 --> 00:00:06,000
      8:- A needle, in there?
    cue 3 00:01:02,500 --> 00:01:04,000
      12:<i>needle</i>
  ```

### 5.7 Object files (#12)

Trail: `ELF x86-64 › .rodata › main.version+0x10`. The Match line shows the offset in hex and
the printable string around the Match, as `strings` would extract it. The nearest symbol is the
closest preceding symbol in the same section; when the binary is stripped, that Segment is
omitted.

### 5.8 Documents (`--docs`, M4)

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
  `--within` filters them. With `--in-headings` the search is the one in Segment names, which
  reads the whole Source: `bytes_searched` is its size, even with `-m`, and `matched_lines` and
  `matches` count the Matches in names (a deviation from ripgrep, in `docs/rg-compat.md`). A
  Match over contiguous pieces of a name — an entity, an escape, a byte that is not UTF-8 between
  letters — is one submatch over all of them and counts once (M3 plan, D35).
- A document searched with `--docs` (M4) keeps the document in `path`; its structure appears
  as Segments of `trail`.
- Added field names are stable from v0.1; new ones may appear, existing ones are never renamed.
  Their schema lives in `docs/json-schema.json`.
- `--json --no-trail` prints the same messages as `rg --json`; only the timings differ.

### 6.5 Control characters in names (M3 plan)

In tree and one-line output, each control character of a Segment name or signature — C0 and C1
controls, DEL, and the separators U+2028 and U+2029 — prints as its escape: `\n`, `\r`, `\t`,
`\0`, and `\u{1b}` for the others. A JSON, TOML, YAML or JavaScript key `"a\nb"` prints `a\nb`
and its row stays one line, in every Format, and a name cannot write escape sequences to the
terminal. A backslash prints as itself, so the key `a\\nb` (a backslash and an `n`) prints the
same `a\nb`; `--json` carries the name as decoded, and `--within` and `--in-headings` match the
decoded name. The text of the printed line is ripgrep's and is not escaped.

## 7. Grammars (#8, 8361, 8451–8454)

Requirements from #8: every Grammar of the v1 Formats included in the default install, extra
ones easy to add, and a flag that installs them. 8454 replaced the WebAssembly Grammars of 8361
([ADR 0003](../adr/0003-grammars-as-wasm.md), superseded) with
[ADR 0005](../adr/0005-grammars-compiled-in-extras-as-helper-processors.md):

- **Compiled in.** The Grammars of the code Formats of §2 — php, javascript, typescript, tsx,
  bash, go, rust, python — are linked into sfor from their crates.io crates. YAML, JSON and TOML
  use native parsers (§4.3) and Markdown uses pulldown-cmark, so they need no Grammar. HTML,
  XML, CSV and subtitles use scanners of their own (§4.3, M3 plan), so they need none either.
- **Grammar Helpers.** Any other language is a **Grammar Helper**: a separate binary, built by
  searchfor from the same `treesitter` code with one Grammar compiled in, that speaks the Helper
  Processor protocol (§4.4) in `concurrent` mode. It lives in
  `$XDG_DATA_HOME/searchfor/grammars/<lang>/` (on Linux `~/.local/share/searchfor/grammars/`)
  together with its Format Definition, `<lang>.toml`, which sfor loads after the built-ins and
  before the user formats directory. Its Definition says `processor = "treesitter"` and
  `grammar = "<lang>"`, as a compiled-in one does.
- **`sfor --install-grammar <lang>`** reads the Grammar index: a JSON file in the GitHub
  repository, `grammars/index.json`, listing for each Grammar its name, version, source
  repository, license and Format Definition, and for each system (`linux-x86_64`,
  `macos-aarch64`, `windows-x86_64`, …) the URL and SHA-256 of its binary. sfor downloads the
  binary for the running system, verifies the checksum, and only then writes the binary, the
  Definition and `installed.json` (version and checksum) to the Grammar directory. A checksum
  mismatch, or no binary for the system, is an error (exit 2) that leaves the directory as it
  was. The Definition in the index must be for that Grammar (`name` and `grammar` equal to it,
  `processor = "treesitter"`), or nothing is installed. Installing a Grammar again replaces it,
  under a lock per Grammar, and a failed replacement keeps the previous install. Only `https://`
  and `file://` URLs are accepted, and HTTPS is required on every redirect too; downloads have a
  10 s connect timeout and a 300 s ceiling. `SFOR_GRAMMAR_INDEX` replaces the index URL (tests
  and mirrors).
- **`sfor --list-grammars`** lists the compiled-in Grammars, the installed ones with their
  version and directory, and the ones the index offers for this system; when the index cannot
  be read it lists the first two and warns.
- A Format Definition whose Grammar is neither compiled in nor installed is listed by
  `--list-formats` as unavailable, and its files are treated as Plain with one warning per run,
  ``sfor: <format>: grammar `<lang>` is neither compiled in nor installed (see --list-grammars)``,
  printed only when a file of that Format has a Match (§4.4, Failures).
- A Grammar Helper binary per Grammar and system is built and published by the release pipeline
  (§10, M5). M2 ships the client side, the Grammar Helper template and a script that builds one
  for the running system (M2 plan).

Measured in the M2 research (2026-10-16 and 2026-10-17, on this machine): native parsing is
1.5–1.6× faster than WebAssembly; a stripped release binary with only the seven code Grammars
and the keypath parsers weighs 8.5 MB (sfor 0.1 weighs 5.5 MB); and tree-sitter parses and
queries real Rust code at about 3 MB/s per thread. Only files with Matches are parsed (§3).
The v0.2 release binary (Linux x86-64, thin LTO), with the eight Grammars, keypath, the
Helper Processor host and the index client, weighs 16.4 MB, 15.2 MB stripped (2026-10-17). The
v0.3 release binary, which adds `markup`, `csv` (with `csv-core`) and `subtitles`, weighs 16.6 MB
(16 575 912 bytes), 15.4 MB stripped (2026-10-17), under the M3 plan's ceiling of 20 MB. Since
8612 the ceiling is 23 MB (§11 #22).

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
| `--install-grammar <LANG>`, `--list-grammars` | Grammar Helpers (§7) |
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
- The same pipeline builds a Grammar Helper per Grammar for the same systems and writes their
  URLs and SHA-256 to `grammars/index.json` (§7, M5).
- The crate `searchfor` is published on crates.io when v0.1 works (#20).

## 11. Acceptance criteria

1. `sfor PATTERN` over a tree with no structured files prints byte-for-byte what `rg PATTERN`
   prints when given `--no-trail`, for the flags marked implemented in `docs/rg-compat.md`.
2. Every Format of §2 has a fixture under `tests/fixtures/` and a snapshot test of its tree,
   one-line and JSON output.
3. A Match inside a fenced code block under `## Linux` under `# Install` gets the Trail
   `Install › Linux › bash`.
4. A Match in `config.yaml` at `server.ports[2].host` gets that Trail.
5. A Match inside a PHP method gets `Cart › addItem` in the one-line output, and the tree
   output shows the class's and the method's signatures.
6. A Match in a stripped ELF binary shows format, section, offset and string, and no symbol.
7. With `--docs`, a Match in a PDF gets the PDF's chapter Trail; without `--docs`, the PDF
   is treated as ripgrep treats it. A zip is never opened (8390).
8. A 20 000-character minified JSON line with three Matches prints Fragments no wider than
   the terminal, with correct omitted counts.
9. `--within "Install › Linux"` keeps only Matches under that Trail; `--expand` prints each
   Section once.
10. A user Definition in the formats directory overrides the built-in with the same name;
    `--export-formats` refuses the formats directory.
11. A Helper Processor that crashes leaves the Matches of the file it was serving with an empty
    Trail and a warning, is started again for the next file, and the exit code is still
    ripgrep's (0 if anything matched).
12. Every `match` message of `--json` carries `trail`, `format` and `fragment` valid against
    `docs/json-schema.json`; `--json --no-trail` equals `rg --json`, timings aside, for the
    implemented flags.
13. Performance: on `~/robotin/knowledge` (619 Markdown files on 2026-10-16), `sfor` takes at
    most 1.5× the wall time of `rg` for a pattern matching under 5 % of the files, measured as
    the median of 10 runs with `hyperfine`. On code (M2, `scripts/bench-code.sh`): with a
    pattern no file matches, and with `--no-trail`, at most 1.1× `rg`; with a pattern matching
    under 5 % of the files, sfor's time over `rg`'s is at most the size of the matching files
    divided by 2.5 MB/s per search thread, the cost of parsing them. On HTML, XML, CSV, TSV and
    subtitles (M3, `scripts/bench-m3.sh`): with a pattern no file matches, and with `--no-trail`,
    at most 1.1× `rg`; with a pattern matching under 5 % of the files, sfor's time over `rg`'s is
    at most the largest matching file divided by 50 MB/s, plus the size of all the matching files
    divided by 50 MB/s per search thread, plus 0.05 s. A 50 MB file of each of these Formats with
    three Matches gets its Trails within `rg`'s time plus 1 s (`tests/perf.rs`). A pattern that
    matches every line is measured and reported, not bounded (§3, item 5; pending for M5). The
    reference is `/usr/bin/rg`, which must be ripgrep 14.1.1 (another `rg` may come first on
    `PATH`), and a comparison that cannot run makes an acceptance run fail instead of passing
    silently (`SFOR_REQUIRE_REFERENCE=1`, M3 plan, D33).
14. `sfor --mcp` answers a `search` call from Claude Code.
15. In `pool`, `concurrent` and `oneshot` modes, an example Helper Processor gives the same
    Trails; a pool never runs more copies than search threads; a concurrent helper that answers
    out of order still gets each answer to its own file.
16. A Grammar Helper and the example helper keep their memory flat over 5 000 requests: their
    resident memory after 5 000 requests is at most 2 MiB over its value after 500.
17. `sfor --install-grammar` installs a Grammar Helper from an index, refuses a binary whose
    SHA-256 does not match, and the installed Grammar gives Trails.
18. A Match in the third `<li>` of the `<ul>` of `<nav class="top menu">` gets
    `nav.top.menu › ul › li[3]`; in XML, a Match in the title of the second `book` gets
    `catalog › book[2] › title`, and the Matches after an XML syntax error get an empty Trail.
19. A Match in the `Plan` column of the second record of a CSV file gets `row 2 › Plan`; a CSV
    file whose first line is separated by `;` is read with `;`.
20. A Match in an SRT cue gets `cue 2 00:00:04,000` in the one-line output and
    `cue 2 00:00:04,000 --> 00:00:06,000` in the tree, the same with a BOM and CRLF line ends.
21. A key `"a\nb"` in JSON, TOML, YAML or JavaScript prints `a\nb` on one row in tree and one-line
    output, and as the decoded name in `--json`.
22. The release binary (`target/release/sfor`, Linux x86-64, the profile of `Cargo.toml`) weighs
    at most 23 MB (8612; 20 MB until then, M3 plan).
