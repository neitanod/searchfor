---
name: searchfor
description: Use when searching Markdown, code or YAML/JSON/TOML and you need to know which section, function or key each match belongs to — before running rg and opening whole files
---

# searchfor (`sfor`)

`sfor` searches like ripgrep (same flags, same regex engine, same exit codes) and prints, for
every match, its **Trail**: where the match sits in the structure of its file. In Markdown,
the frontmatter title, headings, fenced code block, parent list items and table column; in
code, the namespace, class and function; in YAML, JSON and TOML, the key path. With
`--expand` it prints the whole Section that holds the match, so you read that part of the
file and nothing else.

## When to prefer `sfor` over `rg`

- You would otherwise open the whole file to learn which section, function or key a line
  belongs to.
- You want only the matches under some heading, class, function or key (`--within`).
- You want the surrounding section or function of each match without guessing `-A`/`-B`
  (`--expand`).

Trails cover Markdown, YAML, JSON, TOML, PHP, JavaScript, TypeScript, TSX, Bash, Go, Rust and
Python (`sfor --list-formats` lists them). Every other file (logs, C, HTML, CSV) prints as
`rg` prints it, with an empty Trail, so `rg` is just as good there.

## Recipes

One line per match with its Trail (`path:line: Trail │ text`; the default in a pipe):

```bash
sfor --flat 'pattern' docs/ src/
```

The whole Section of each match, printed once per Section (a code block, a list item, a
table, a heading's section, a function, a key's value):

```bash
sfor --expand --flat 'pattern' docs/
```

Only the Trails, from `--json` (ripgrep's JSON Lines plus `format`, `trail`, `fragment`):

```bash
sfor --json 'pattern' src/ | jq -r 'select(.type == "match") | .data.path.text + ": " + (.data.trail | map(.name) | join(" › "))'
```

Only the matches under a heading, with their sections:

```bash
sfor --within 'install › linux' --expand --flat 'pattern' docs/
```

Code: the method a match is in, read whole. The one-line output names it `Class › method`
(`Server.Start` for a Go method):

```bash
sfor --within 'Cart › addItem' --expand --flat 'pattern' src/
```

Configuration: the matches under a key path. Array elements are named by their key and
index, so `ports[2]` is the third element of `ports`:

```bash
sfor --within 'server › ports' --flat 'pattern' config/
```

Where something is defined, not used: `--in-headings` matches only Segment names (headings,
keys, function and class names):

```bash
sfor --in-headings --flat '^addItem$' src/
```

`--within` keeps matches whose Trail contains those Segment names in that order; names are
separated by `›`, `>` or `/`, and each is a case-insensitive substring, so `--within linux`
or `--within addItem` is enough when the name is unique.

## Details that matter

- `--in-headings 'regex'` parses every candidate file, so it is slower than a normal search.
- A match on the line that names a function, heading or key is outside that Segment: the
  Trail of `function addItem(...)` is the class, not `addItem`.
- The tree layout (`--tree`, the default on a terminal) shows each code Segment by the line
  that declares it, signature included.
- Long lines (minified JSON, one-line HTML) print as Fragments around the matches,
  `…[+N] text […+M]`, instead of the whole line.
- `--no-trail` prints exactly what `rg` prints.
- A ripgrep flag that sfor does not implement yet fails with `sfor: --<flag> is not supported
  yet` and exit code 2; drop the flag or use `rg` for that search.
- In `--json`, when two matches on one line have different Trails (two table cells), each
  object in `.data.submatches` carries its own `trail`. The `stats` of `end` and `summary`
  count matches before `--within` filters them.
- Exit codes are ripgrep's: 0 when something matched, 1 when nothing did, 2 on error.
