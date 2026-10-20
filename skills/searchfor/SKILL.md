---
name: searchfor
description: Use when searching Markdown, code, YAML/JSON/TOML, HTML/XML, CSV/TSV, subtitles, PDF/Office/EPUB/SQLite documents or ELF/PE/Mach-O binaries and you need to know which section, function, key, element, row, cue, page, slide, sheet cell or symbol each match belongs to — before running rg and opening whole files
---

# searchfor (`s4`)

`s4` searches like ripgrep (same flags, same regex engine, same exit codes) and prints, for
every match, its **Trail**: where the match sits in the structure of its file. In Markdown,
the frontmatter title, headings, fenced code block, parent list items and table column; in
code, the namespace, class and function; in YAML, JSON and TOML, the key path; in HTML and
XML, the element path (`section#pricing.card.wide › table › tr[3] › td[2]`); in CSV and TSV,
the row and the column's header (`row 2 › Plan`); in SubRip and WebVTT subtitles, the cue and
its start time (`cue 2 00:00:04,000`); in ELF, PE and Mach-O binaries, the section and the
nearest symbol at the hex offset of the match (`ELF x86-64 › .rodata › main.version+0xe`); and
in PDF, Word, Excel, PowerPoint, OpenDocument, EPUB and SQLite files, the
chapter and page, heading and table, sheet and cell, slide, or table, row and column
(`slide 1 › notes`, `Budget › A3`, `users › row 12 › email`). With
`--expand` it prints the whole Section that holds the match, so you read that part of the
file and nothing else.

## As an MCP tool

`s4 --mcp` is an MCP server (`claude mcp add searchfor -- s4 --mcp`). Its tool `search` takes
`pattern`, `paths`, `within`, `expand`, `glob`, `case_insensitive` and `max_results` (100 by
default, at most 1 000) and returns each match's path, line, Trail and text, in at most 256 KiB
(`truncated: true` when the search found more, and `truncated_by` why: for `max_results` raise
it or narrow `paths`, `glob` or `within`; for `bytes` narrow the search or leave out `expand`;
for `long_line` search the files `errors` names, the first 20, with `s4` in a shell). Prefer it
to running `s4` in a shell when the tool is there: the answer is structured and bounded.

## When to prefer `s4` over `rg`

- You would otherwise open the whole file to learn which section, function or key a line
  belongs to.
- You want only the matches under some heading, class, function or key (`--within`).
- You want the surrounding section or function of each match without guessing `-A`/`-B`
  (`--expand`).

Trails cover Markdown, YAML, JSON, TOML, PHP, JavaScript, TypeScript, TSX, Bash, Go, Rust,
Python, HTML, XML (also `.svg`, `.rss`, `.csproj`, `.xaml`… and files starting with `<?xml`),
CSV, TSV, SubRip (`.srt`) and WebVTT (`.vtt`), ELF, PE and Mach-O binaries (by their first
bytes, any name), and, by default, PDF, Word, Excel, PowerPoint, OpenDocument, EPUB and
SQLite; `s4 --list-formats` lists them. Every other file (logs, C, Java) prints as `rg` prints
it, with an empty Trail, so `rg` is just as good there.

## Recipes

One line per match with its Trail (`path:line: Trail │ text`; the default in a pipe):

```bash
s4 --flat 'pattern' docs/ src/
```

The whole Section of each match, printed once per Section (a code block, a list item, a
table, a heading's section, a function, a key's value):

```bash
s4 --expand --flat 'pattern' docs/
```

Only the Trails, from `--json` (ripgrep's JSON Lines plus `format`, `trail`, `fragment`):

```bash
s4 --json 'pattern' src/ | jq -r 'select(.type == "match") | .data.path.text + ": " + (.data.trail | map(.name) | join(" › "))'
```

Only the matches under a heading, with their sections:

```bash
s4 --within 'install › linux' --expand --flat 'pattern' docs/
```

Code: the method a match is in, read whole. The one-line output names it `Class › method`
(`Server.Start` for a Go method):

```bash
s4 --within 'Cart › addItem' --expand --flat 'pattern' src/
```

Configuration: the matches under a key path. Array elements are named by their key and
index, so `ports[2]` is the third element of `ports`:

```bash
s4 --within 'server › ports' --flat 'pattern' config/
```

HTML and XML: the matches inside an element. Elements are named `tag#id.class`, with `[n]`
among siblings of the same tag; `html` and `body` never appear:

```bash
s4 --within 'pricing › table' needle site/
```

```
site/page.html:23: main#content › section#pricing.card.wide › table › tr[3] › td[2] │         <tr><td>Pro<td>needle 9
```

Subtitles: the cues that start at a time. `--in-headings` searches the cue names, so
`'01:02'` finds the cues starting at 1:02:

```bash
s4 --in-headings '01:02' subs/
```

```
subs/movie.srt:11: cue 3 00:01:02,500 │ 00:01:02,500 --> 00:01:04,000
```

Documents: find the slide, sheet or page where a phrase is. By default, in a terminal and in a
pipe alike, s4 searches the text of PDF, Word, Excel, PowerPoint, OpenDocument, EPUB and SQLite
files, one line per paragraph, heading, list item or table row:

```bash
s4 --flat 'needle joke' m4/deck.pptx m4/budget.xlsx m4/report.pdf
```

```
m4/deck.pptx:3: slide 1 › notes │ tell the needle joke
```

Every document of the search is extracted, a match in it or not, so a tree with many documents
takes longer than `rg`. Add `--no-docs` to search documents as `rg` searches them (a PDF or a
Word file prints `binary file matches` or nothing, with no Trail), when you want `rg`'s speed
over a tree full of documents or `rg`'s exact output. `--docs` turns documents back on after a
`--no-docs`; the last of the two wins.

Binaries: which section and symbol hold a string. With `--binary`, a walked ELF, PE or Mach-O
file with a match is searched whole (a file named on the command line already is), and each
printable string with matches becomes a row at its hex offset:

```bash
s4 --binary --flat --sort path 'version=' m4/bin
```

```
m4/bin/app:0x5a: ELF x86-64 › .rodata › main.version │ version=1.2.3-needle
m4/bin/app-mac:0x1133: Mach-O x86-64 › __TEXT,__cstring › _version │ version=1.2.3-needle
m4/bin/app-mac:0x2133: Mach-O arm64 › __TEXT,__cstring › _version │ version=1.2.3-needle
m4/bin/app-stripped:0x5a: ELF x86-64 › .rodata │ version=1.2.3-needle
m4/bin/app.exe:0x402: PE x86-64 › .rdata › version_string │ version=1.2.3-needle
```

Each match alone, with its column and its Trail (`-o` prints the matched text, `--column` its
column; `--vimgrep` gives `path:line:column:` lines an editor reads):

```bash
s4 --flat --column -o 'needle-\w+' md/guide.md
```

```
7:22: Deploy guide › Install │ needle-intro
12:13: Deploy guide › Install › Linux › bash │ needle-pkg
16:12: Deploy guide › Install › Linux › step one │ needle-item
…
```

Look-around and backreferences: `-P` searches with PCRE2 (`--engine auto` falls back to it only
when the default engine refuses the pattern); `-U` lets a match span lines, and each line of it
prints as a row:

```bash
s4 -P --flat 'needle-(?=pkg)' md
```

Where something is defined, not used: `--in-headings` matches only Segment names (headings,
keys, function and class names, elements, CSV header cells, cues):

```bash
s4 --in-headings --flat '^addItem$' src/
```

`--within` keeps matches whose Trail contains those Segment names in that order; names are
separated by `›`, `>` or `/`, and each is a case-insensitive substring, so `--within linux`
or `--within addItem` is enough when the name is unique.

## Details that matter

- `--in-headings 'regex'` parses every candidate file, so it is slower than a normal search.
- A match on the line that names a function, heading or key is outside that Segment: the
  Trail of `function addItem(...)` is the class, not `addItem`. The same holds for an HTML or
  XML start tag and its attributes, and for a CSV header row, whose matches have an empty
  Trail. A cue's timing line is the exception: it is inside its cue.
- The CSV delimiter (`,`, `;`, tab or `|`) is read from the first line; `--expand` on a CSV
  match prints its record, not the file.
- The tree layout (`--tree`, the default on a terminal) shows each code Segment by the line
  that declares it, signature included.
- Long lines (minified JSON, one-line HTML) print as Fragments around the matches,
  `…[+N] text […+M]`, instead of the whole line.
- `--no-trail` prints what `rg` prints, except inside documents, where it prints the lines of
  their text; `--no-docs --no-trail` prints exactly what `rg` prints.
- A document s4 cannot read (an encrypted PDF, a damaged Word file) gets one warning and
  is searched as `rg` searches it; `--no-messages` silences the warning.
- Page, sheet, slide, table, row, column, cell and chapter names are not in a document's text,
  so `--in-headings` finds only headings there; use `--within` to keep the matches under them
  (`--within 'slide 3'`, `--within 'Budget'`).
- Every ripgrep 14.1.1 flag works. With `--no-trail` they print what `rg` prints; in the Trail
  layouts `--column`, `-b`, `-o`, `--vimgrep`, `-r`, `--trim`, `-0` and the separators change
  s4's rows the same way, each row keeping its Trail (docs/rg-compat.md lists the details).
- s4 reads a file named by `SEARCHFOR_CONFIG_PATH` (rg's format) before the command line; add
  `--no-config` when a search must not depend on it.
- In the Trail layouts, stdin is read whole before its rows print; for a stream that never ends
  (`tail -f`), use `--no-trail`.
- In `--json`, when two matches on one line have different Trails (two table cells), each
  object in `.data.submatches` carries its own `trail`. The `stats` of `end` and `summary`
  count matches before `--within` filters them.
- Exit codes are ripgrep's: 0 when something matched, 1 when nothing did, 2 on error.
