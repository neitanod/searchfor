---
name: searchfor
description: Use when searching Markdown, code, YAML/JSON/TOML, HTML/XML, CSV/TSV or subtitles and you need to know which section, function, key, element, row or cue each match belongs to — before running rg and opening whole files
---

# searchfor (`s4`)

`s4` searches like ripgrep (same flags, same regex engine, same exit codes) and prints, for
every match, its **Trail**: where the match sits in the structure of its file. In Markdown,
the frontmatter title, headings, fenced code block, parent list items and table column; in
code, the namespace, class and function; in YAML, JSON and TOML, the key path; in HTML and
XML, the element path (`section#pricing.card.wide › table › tr[3] › td[2]`); in CSV and TSV,
the row and the column's header (`row 2 › Plan`); in SubRip and WebVTT subtitles, the cue and
its start time (`cue 2 00:00:04,000`). With
`--expand` it prints the whole Section that holds the match, so you read that part of the
file and nothing else.

## When to prefer `s4` over `rg`

- You would otherwise open the whole file to learn which section, function or key a line
  belongs to.
- You want only the matches under some heading, class, function or key (`--within`).
- You want the surrounding section or function of each match without guessing `-A`/`-B`
  (`--expand`).

Trails cover Markdown, YAML, JSON, TOML, PHP, JavaScript, TypeScript, TSX, Bash, Go, Rust,
Python, HTML, XML (also `.svg`, `.rss`, `.csproj`, `.xaml`… and files starting with `<?xml`),
CSV, TSV, SubRip (`.srt`) and WebVTT (`.vtt`); `s4 --list-formats` lists them. Every other
file (logs, C, Java) prints as `rg` prints it, with an empty Trail, so `rg` is just as good
there.

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
- `--no-trail` prints exactly what `rg` prints.
- A ripgrep flag that s4 does not implement yet fails with `s4: --<flag> is not supported
  yet` and exit code 2; drop the flag or use `rg` for that search.
- In `--json`, when two matches on one line have different Trails (two table cells), each
  object in `.data.submatches` carries its own `trail`. The `stats` of `end` and `summary`
  count matches before `--within` filters them.
- Exit codes are ripgrep's: 0 when something matched, 1 when nothing did, 2 on error.
