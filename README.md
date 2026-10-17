> 🇦🇷 Leer en español: [README.es.md](README.es.md)

# searchfor

`sfor` is a ripgrep-compatible search tool that tells you **where** each match lives inside
the structure of its file.

````
$ sfor needle-pkg md --tree
md/guide.md
  Deploy guide
    # Install
      ## Linux
        ```bash
          12:apt install needle-pkg
````

The lines above `12:apt install needle-pkg` are the match's **Trail**: the document title
from the frontmatter, the headings, and the fenced code block the match sits in. In code the
Trail is the class and the function, and in YAML, JSON and TOML it is the key path. Every
example in this README is real output over [`tests/fixtures/`](tests/fixtures), run from
inside that directory, except the one of [Helper Processors](#helper-processors), which says
what it runs over.

## Status

**v0.2: Markdown, configuration files and code.** The search engine is ripgrep's own crates,
so the search behaves like `rg`; on top of it, Markdown, YAML, JSON, TOML and code in eight
languages get Trails (see [Formats](#formats)), long lines are shown as Fragments, and
`--within`, `--expand` and `--in-headings` filter and expand by structure. Other languages
can be described by a [Helper Processor](#helper-processors), a separate program that sfor
talks to. Every other file is **Plain**: its matches print with an empty Trail. More Formats
come by milestone (see [Roadmap](#roadmap)).

The glossary is in [CONTEXT.md](CONTEXT.md), the design in
[docs/specs](docs/specs/2026-10-16-searchfor-design.md) and the architecture decisions in
[docs/adr](docs/adr).

## Install

```
cargo install searchfor
```

That installs the latest version published on crates.io. Until v0.2.0 is published there,
install v0.2 from the repository:

```
cargo install --git https://github.com/neitanod/searchfor
```

Both install the `sfor` binary. Prebuilt binaries, Homebrew and npm come in v1.0.

## Formats

| Format | Files | Segments |
|---|---|---|
| Markdown | `.md`, `.markdown`, `.mdx`, `.mkd`, `.mdown` | frontmatter title, headings, fenced code blocks, parent list items, table columns |
| YAML | `.yaml`, `.yml` | keys and array elements; `document N` for each document of a stream |
| JSON | `.json`, `.jsonc`, `.jsonl`, `.geojson`, `.webmanifest`, `.babelrc`, `.eslintrc` | keys and array elements; comments and trailing commas are accepted |
| TOML | `.toml`, `Cargo.lock`, `Pipfile`, `poetry.lock`, `uv.lock` | keys, tables and arrays of tables |
| PHP | `.php`, `.phtml` | namespaces, classes, interfaces, traits, enums, methods, functions |
| JavaScript | `.js`, `.mjs`, `.cjs`, `.jsx` | classes, methods, functions, and arrow functions or function expressions given a name |
| TypeScript | `.ts`, `.mts`, `.cts` | the JavaScript ones, plus namespaces, interfaces and abstract classes |
| TSX | `.tsx` | the TypeScript ones |
| Bash | `.sh`, `.bash`, `.bats`, `.bashrc`, `.bash_profile`, `.bash_aliases`, `.profile` | functions |
| Go | `.go` | types, functions, and methods named by their receiver (`Server.Start`) |
| Rust | `.rs` | modules, structs, enums, traits, `impl` blocks, functions |
| Python | `.py`, `.pyi`, `.pyw` | classes and functions, nested ones included |

A file without an extension is also recognised by its shebang (`#!/usr/bin/env python3`,
`node`, `bash`…). The code Formats are parsed by tree-sitter Grammars compiled into `sfor`;
YAML, JSON and TOML by native parsers. `sfor --list-formats` lists them all.

The same key path in a YAML file and in a JSON file:

```
$ sfor needle.example data --flat --sort path
data/config.json:8: server › ports[2] › host │       {"host": "needle.example", "port": 8443},
data/config.yaml:9: server › ports[2] › host │     - host: needle.example
```

`ports[2]` is the third element of the array held by `ports`, so `ports` itself is left out
of the Trail. In code, the one-line output prints names and the tree prints the line that
declares each Segment:

```
$ sfor needle code/cart.php --tree
code/cart.php
  class Cart
    public function addItem(Item $item, int $qty = 1): void
      10:        // needle: merge equal items
  15:function needle_helper(): string
    17:    return "needle";
```

```
$ sfor needle code/cart.php --flat
10: Cart › addItem │         // needle: merge equal items
15: function needle_helper(): string
17: needle_helper │     return "needle";
```

A match in the name of a function, as on line 15, is outside that function, as a match on a
heading line is outside its section.

## Usage

`sfor PATTERN [PATHS…]` takes ripgrep's flags with ripgrep's names. On a terminal the default
layout is the tree; in a pipe it is one line per match, `path:line: Trail │ text`.

### One line per match (`--flat`, the default in a pipe)

```
$ sfor needle md --flat
md/guide.md:7: Deploy guide › Install │ Intro paragraph with needle-intro.
md/guide.md:12: Deploy guide › Install › Linux › bash │ apt install needle-pkg
md/guide.md:16: Deploy guide › Install › Linux › step one │   - nested needle-item
md/guide.md:23: Deploy guide › Install › Plans › Price │ | Pro  | needle-10 |
md/guide.md:24: Deploy guide › Install › Plans › Plan │ | needle-a | needle-b |
md/guide.md:24: Deploy guide › Install › Plans › Price │ | needle-a | needle-b |
md/guide.md:28: Deploy guide › Install › Plans › Cell │ > | q | needle-quoted |
md/guide.md:31: Deploy guide › Install › Plans › Heading in item › Heading in item │   needle-in-item
md/guide.md:33: Deploy guide › Install › Plans │ needle-after-list
md/guide.md:38: Deploy guide › Setext Title │ needle-setext body
```

Line 24 holds two matches in two table cells, so it prints once per column.

### Only the matches under a Trail (`--within`)

```
$ sfor needle md --within "install › linux" --flat
md/guide.md:12: Deploy guide › Install › Linux › bash │ apt install needle-pkg
md/guide.md:16: Deploy guide › Install › Linux › step one │   - nested needle-item
```

`--within` keeps the matches whose Trail contains these Segment names in this order. Names
are separated by `›`, `>` or `/`, and each one is a case-insensitive substring. Since `/`
always separates, a heading with a slash in it, like `TCP/IP`, is named by a part without
the slash, between `›` or `>`: `--within "network › tcp"`.

### The whole Section (`--expand`)

```
$ sfor needle-10 md --expand --tree
md/guide.md
  Deploy guide
    # Install
      ## Plans
        column «Price»
          21-| Plan | Price |
          22-|------|-------|
          23:| Pro  | needle-10 |
          24-| needle-a | needle-b |
```

`--expand` prints the Section of the last Segment of each match: the whole code block, list
item, table or heading section. A Section with several matches prints once.

### Long lines

A line wider than the terminal (or than `--fragment-width`, 200 by default, in a pipe) is
shown as Fragments centred on the matches, with the number of omitted characters on each side:

```
$ sfor needle misc/min.json --flat --fragment-width 120
1: [0] │ …[+983] aaaaaaaaaaaaaaaaaneedleaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaneedleaaaaaaaaaaaaaaaaaa […+18926]
1: [0] │ …[+14958] aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaneedleaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa […+4951]
```

Matches close enough to fit in one Fragment share it; the others get their own line.
ripgrep's `-M/--max-columns` keeps its meaning: with it, long lines print ripgrep's
`[Omitted long matching line]` message instead of Fragments.

### searchfor's own flags

| Flag | Meaning |
|---|---|
| `--within <TRAIL>` | keep matches whose Trail contains these Segment names, in order (`›`, `>` or `/` between names; case-insensitive substrings) |
| `--expand` | print the whole Section of the last Segment of each match, once per Section |
| `--in-headings` | match only inside Segment names (headings, keys, function names); every candidate file is parsed, so it is slower |
| `--format <F>` / `--format <GLOB>=<F>` | force a Format for every file or for the files matching a glob |
| `--flat` / `--tree` | force the one-line or the tree layout |
| `--no-trail` | print exactly what ripgrep would print |
| `--fragment-width <N>` | Fragment width when there is no terminal (default 200) |
| `--list-formats` | list every Format with its origin: built-in, installed, user or overridden |
| `--export-formats <DIR>` | write the built-in Format Definitions to `DIR` |
| `--list-grammars` | list the compiled-in, installed and available Grammars (see [Grammars](#grammars)) |
| `--install-grammar <LANG>` | install a Grammar Helper from the Grammar index |

Format Definitions are TOML files. A file in `searchfor/formats/` under the user config
directory (`~/.config` or `$XDG_CONFIG_HOME` on Linux) with the name of a built-in or
installed Format replaces it, and a new name adds a Format.

### ripgrep compatibility

ripgrep 14.1.1 has 146 flag names (104 flags, 30 `--no-` negations and 12 other negations).
v0.2 implements 51 of them. The other 95 are accepted by name and fail
with `sfor: --<flag> is not supported yet` and exit code 2, so nothing is silently ignored.
The table is [docs/rg-compat.md](docs/rg-compat.md). Exit codes are ripgrep's: 0 when
something matched, 1 when nothing did, 2 on error.

With `--no-trail`, the output is written by ripgrep's own printer, and `--json --no-trail`
prints the same messages as `rg --json`, timings aside.

Known differences with ripgrep 14.1.1:

- In `--json` output, the `stats` of the `end` and `summary` messages count the matches found
  by the search, before `--within` and `--in-headings` filter them out.

## Helper Processors

A Helper Processor is a program, written in any language, that describes the files of one
Format: sfor starts it when the first file of that Format has a match, sends it the path and
the match positions of each file over JSON lines on its stdin, reads the Trails from its
stdout, and closes its stdin when the search ends. A Format Definition with
`processor = "external"` names it:

```toml
name       = "ini"
extensions = ["ini"]
processor  = "external"

[params]
command    = ["sfor-example-helper"]
mode       = "pool"
```

`mode` says how sfor feeds it:

- `pool` (the default): each copy gets one file at a time, and sfor starts more copies, up
  to the search's thread count, when every copy is busy. A program that reads a line,
  answers it and reads the next one works in this mode.
- `concurrent`: one process gets several files at once and may answer in any order, since
  each answer carries the `id` of its request.
- `oneshot`: one process per file, for small scripts that read their whole stdin before
  answering.

A helper never stops the search and never changes the exit code. A file whose request fails
(an `error` answer, a crash, no answer within `timeout_ms`, 5000 by default) prints as Plain,
with a warning; after three crashes, timeouts or lines that are not JSON in one run, the
Format prints as Plain for the rest of the run, with one more warning.

[`helpers/example/`](helpers/example) is the reference implementation, a Rust helper that
describes INI files. With the Definition above in the formats directory and
`sfor-example-helper` on the `PATH` (`cargo build --release -p sfor-example-helper`), over a
file `app.ini` written for this example:

```
$ sfor needle app.ini --tree
app.ini
  section database
    host
      2:host = needle.local
  section cache
    6:; needle in a comment
```

The three modes give this same output. The protocol is in
[spec §4.4](docs/specs/2026-10-16-searchfor-design.md#44-helper-processors-7-84518459).

## Grammars

The Grammars of the eight code Formats are compiled into `sfor`. Any other language comes as
a Grammar Helper: a Helper Processor built by searchfor with one Grammar, which
`--install-grammar` downloads from the Grammar index, checks against its SHA-256, and installs
with its Format Definition under `$XDG_DATA_HOME/searchfor/grammars/` (on Linux,
`~/.local/share/searchfor/grammars/` by default).

```
$ sfor --list-grammars
php            compiled in
javascript     compiled in
typescript     compiled in
tsx            compiled in
bash           compiled in
go             compiled in
rust           compiled in
python         compiled in
```

The index, [`grammars/index.json`](grammars/index.json), is empty until v1.0 builds and
publishes the Grammar Helper binaries for each system, so `--install-grammar` has nothing to
install yet. [`helpers/grammar/`](helpers/grammar) is the template they are built from (Lua
in v0.2), and [`scripts/build-grammar-helper.sh`](scripts/build-grammar-helper.sh) builds one
for the running system and prints its index entry. `SFOR_GRAMMAR_INDEX` points sfor at
another index, such as a `file://` one of your own.

## For AI agents

An agent that finds a match with `rg` usually opens the whole file to learn which section the
line belongs to. `sfor` prints that section in the Trail, and `--expand` prints the section
itself, so the agent reads the part of the file that matters and nothing else.

Which section is each match in:

```
$ sfor --json needle md | jq -r 'select(.type == "match") | .data.trail | map(.name) | join(" › ")'
Deploy guide › Install
Deploy guide › Install › Linux › bash
Deploy guide › Install › Linux › step one
Deploy guide › Install › Plans › Price
Deploy guide › Install › Plans › Plan
Deploy guide › Install › Plans › Cell
Deploy guide › Install › Plans › Heading in item › Heading in item
Deploy guide › Install › Plans
Deploy guide › Setext Title
```

Read only the sections under a heading:

````
$ sfor needle md --within linux --expand --flat
md/guide.md:11-13: Deploy guide › Install › Linux › bash
```bash
apt install needle-pkg
```

md/guide.md:15-16: Deploy guide › Install › Linux › step one
- step one
  - nested needle-item

````

Read only the method a match is in, from code:

```
$ sfor needle code --within "Cart › addItem" --expand --flat
code/cart.php:8-12: Cart › addItem
    public function addItem(Item $item, int $qty = 1): void
    {
        // needle: merge equal items
        $this->items[] = [$item, $qty];
    }

```

Read every function named like `helper` that holds a match:

```
$ sfor needle code --within helper --expand --flat --sort path
code/cart.php:15-18: needle_helper
function needle_helper(): string
{
    return "needle";
}

code/server.go:11-13: helper
func helper() string {
	return "needle"
}

code/thing.rs:7-9: outer › Thing › helper
        pub fn helper(&self) -> u32 {
            self.needle + 1
        }

```

`--json` emits ripgrep's JSON Lines (`begin`, `match`, `context`, `end`, `summary`), so every
`rg --json` consumer keeps working, and adds three fields to each `match`:

```
{"type":"match","data":{"path":{"text":"md/guide.md"},"lines":{"text":"apt install needle-pkg\n"},"line_number":12,"absolute_offset":96,"submatches":[{"match":{"text":"needle"},"start":12,"end":18}],"format":"markdown","trail":[{"kind":"document","name":"Deploy guide","line":1,"end_line":38},{"kind":"heading","name":"Install","line":5,"end_line":34,"level":1},{"kind":"heading","name":"Linux","line":9,"end_line":18,"level":2},{"kind":"code","name":"bash","line":11,"end_line":13}],"fragment":null}}
```

- `format`: the file's Format, or `null` for Plain files.
- `trail`: the Segments from the root of the file down to the match, each with `kind`,
  `name`, `line` and `end_line` (the lines of its Section), `level` for headings, and
  `signature` for code (the line that declares it).
- `fragment`: `null`, or the Fragment of a line longer than `--fragment-width`.
- With `--expand`, `section`: `{"start_line", "end_line", "text"}` of the Section.
- When the matches of one line have different Trails (two table cells), each object in
  `submatches` carries its own `trail`.

The schema of these fields is [docs/json-schema.json](docs/json-schema.json). The `stats` in
the `end` and `summary` messages count matches before `--within` and `--in-headings` filter
them.

A Claude Code skill with these recipes is in [skills/searchfor](skills/searchfor/SKILL.md);
copy that directory to `~/.claude/skills/` to install it.

## Roadmap

| Milestone | Release | Content |
|---|---|---|
| M1 | v0.1 | Markdown, the core ripgrep flags, tree / one-line / JSON output, Fragments, `--within`, `--expand`, `--in-headings`, Claude Code skill |
| M2 | v0.2 | YAML, JSON and TOML key paths (`server › ports[2] › host`); code (PHP, JavaScript, TypeScript, TSX, Bash, Go, Rust, Python) as `Cart › addItem`, with tree-sitter Grammars compiled in; Helper Processors (`processor = "external"`); `--install-grammar` and `--list-grammars` |
| M3 | v0.3 | HTML, XML, CSV and subtitles (.srt, .vtt) |
| M4 | v0.4 | object files (ELF, PE, Mach-O); `--docs`, built-in extractors that search PDF, Word/ODT, Excel/ODS, PowerPoint/ODP, EPUB and SQLite with their structure; ripgrep's `--pre` |
| M5 | v1.0 | an MCP server, the rest of ripgrep's flags, prebuilt binaries, Homebrew and npm, Windows and macOS; Grammar Helper binaries published in the Grammar index; the guide for writing a Helper Processor |

Archive Formats (zip, tar, 7z, rar, deb, rpm, jar) stay out: searchfor searches what ripgrep
searches, and `-z` keeps ripgrep's meaning (single-stream compressed files such as `.gz`).

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
