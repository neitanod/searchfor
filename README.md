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
from the frontmatter, the headings, and the fenced code block the match sits in. Every example in this README is real output
over [`tests/fixtures/`](tests/fixtures), run from inside that directory.

## Status

**v0.1: Markdown.** The search engine is ripgrep's own crates, so the search behaves like
`rg`; on top of it, Markdown files get Trails (frontmatter title, headings, fenced code
blocks, parent list items and table columns), long lines are shown as Fragments, and
`--within`, `--expand` and `--in-headings` filter and expand by structure. Every other file is
**Plain**: its matches print with an empty Trail. More Formats come by
milestone (see [Roadmap](#roadmap)).

The glossary is in [CONTEXT.md](CONTEXT.md), the design in
[docs/specs](docs/specs/2026-10-16-searchfor-design.md) and the architecture decisions in
[docs/adr](docs/adr).

## Install

```
cargo install searchfor
```

That works once v0.1.0 is published on crates.io. Until then, install from the repository:

```
cargo install --git https://github.com/neitanod/searchfor
```

Both install the `sfor` binary. Prebuilt binaries, Homebrew and npm come in v1.0.

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
are separated by `›`, `>` or `/`, and each one is a case-insensitive substring.

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
1: …[+980] aaaaaaaaaaaaaaaaaaaaneedleaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaneedleaaaaaaaaaaaaaaaaaaaaa […+18923]
1: …[+14955] aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaneedleaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa […+4948]
```

Matches close enough to fit in one Fragment share it; the others get their own line.
ripgrep's `-M/--max-columns` keeps its meaning: with it, long lines print ripgrep's
`[Omitted long matching line]` message instead of Fragments.

### searchfor's own flags

| Flag | Meaning |
|---|---|
| `--within <TRAIL>` | keep matches whose Trail contains these Segment names, in order (`›`, `>` or `/` between names; case-insensitive substrings) |
| `--expand` | print the whole Section of the last Segment of each match, once per Section |
| `--in-headings` | match only inside Segment names (in Markdown, headings); every candidate file is parsed, so it is slower |
| `--format <F>` / `--format <GLOB>=<F>` | force a Format for every file or for the files matching a glob |
| `--flat` / `--tree` | force the one-line or the tree layout |
| `--no-trail` | print exactly what ripgrep would print |
| `--fragment-width <N>` | Fragment width when there is no terminal (default 200) |
| `--list-formats` | list every Format with its origin: built-in, user or overridden |
| `--export-formats <DIR>` | write the built-in Format Definitions to `DIR` |

Format Definitions are TOML files. A file in `searchfor/formats/` under the user config
directory (`~/.config` or `$XDG_CONFIG_HOME` on Linux) with the name of a built-in Format
replaces it, and a new name adds a Format.

### ripgrep compatibility

ripgrep 14.1.1 has 146 flag names (104 flags, 30 `--no-` negations and 12 other negations).
v0.1 implements 51 of them. The other 95 are accepted by name and fail
with `sfor: --<flag> is not supported yet` and exit code 2, so nothing is silently ignored.
The table is [docs/rg-compat.md](docs/rg-compat.md). Exit codes are ripgrep's: 0 when
something matched, 1 when nothing did, 2 on error.

With `--no-trail`, the output is written by ripgrep's own printer, and `--json --no-trail`
prints the same messages as `rg --json`, timings aside.

Known differences with ripgrep 14.1.1:

- In `--json` output, the `stats` of the `end` and `summary` messages count the matches found
  by the search, before `--within` and `--in-headings` filter them out.

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

`--json` emits ripgrep's JSON Lines (`begin`, `match`, `context`, `end`, `summary`), so every
`rg --json` consumer keeps working, and adds three fields to each `match`:

```
{"type":"match","data":{"path":{"text":"md/guide.md"},"lines":{"text":"apt install needle-pkg\n"},"line_number":12,"absolute_offset":96,"submatches":[{"match":{"text":"needle"},"start":12,"end":18}],"format":"markdown","trail":[{"kind":"document","name":"Deploy guide","line":1,"end_line":38},{"kind":"heading","name":"Install","line":5,"end_line":34,"level":1},{"kind":"heading","name":"Linux","line":9,"end_line":18,"level":2},{"kind":"code","name":"bash","line":11,"end_line":13}],"fragment":null}}
```

- `format`: the file's Format, or `null` for Plain files.
- `trail`: the Segments from the root of the file down to the match, each with `kind`,
  `name`, `line` and `end_line` (the lines of its Section), and `level` for headings.
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
| M2 | v0.2 | tree-sitter Grammars as WebAssembly; YAML, JSON and TOML key paths (`server › ports[2] › host`); code (PHP, JavaScript, TypeScript, Bash, Go, Rust, Python) as `class Cart › addItem` |
| M3 | v0.3 | HTML, XML, CSV and subtitles (.srt, .vtt) |
| M4 | v0.4 | object files (ELF, PE, Mach-O); `--docs`, built-in extractors that search PDF, Word/ODT, Excel/ODS, PowerPoint/ODP, EPUB and SQLite with their structure; ripgrep's `--pre` |
| M5 | v1.0 | External Processors, an MCP server, the rest of ripgrep's flags, prebuilt binaries, Homebrew and npm, Windows and macOS |

Archive Formats (zip, tar, 7z, rar, deb, rpm, jar) stay out: searchfor searches what ripgrep
searches, and `-z` keeps ripgrep's meaning (single-stream compressed files such as `.gz`).

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
