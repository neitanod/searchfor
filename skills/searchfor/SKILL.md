---
name: searchfor
description: Use when searching Markdown notes, docs or knowledge bases and you need to know which section each match belongs to — before running rg and opening whole files
---

# searchfor (`sfor`)

`sfor` searches like ripgrep (same flags, same regex engine, same exit codes) and prints, for
every match, its **Trail**: the frontmatter title, headings, fenced code block, parent list
items and table column the match sits in. With `--expand` it prints the whole Section that
holds the match, so you read that part of the file and nothing else.

## When to prefer `sfor` over `rg`

- The files are Markdown (notes, docs, READMEs, knowledge bases) and you need to know which
  section a line belongs to, or you would otherwise open the whole file to find out.
- You want only the matches under some heading (`--within`).
- You want the surrounding section of each match without guessing `-A`/`-B` (`--expand`).

Use plain `rg` for code, logs and other files: in v0.1 only Markdown has Trails, and every
other file prints with an empty Trail.

## Recipes

One line per match with its Trail (`path:line: Trail │ text`; the default in a pipe):

```bash
sfor --flat 'pattern' docs/
```

The whole Section of each match, printed once per Section (a code block, a list item, a table
or a heading's section):

```bash
sfor --expand --flat 'pattern' docs/
```

Only the Trails, from `--json` (ripgrep's JSON Lines plus `format`, `trail`, `fragment`):

```bash
sfor --json 'pattern' docs/ | jq -r 'select(.type == "match") | .data.trail | map(.name) | join(" › ")'
```

Only the matches under a heading, with their sections:

```bash
sfor --within 'install › linux' --expand --flat 'pattern' docs/
```

`--within` keeps matches whose Trail contains those Segment names in that order; names are
separated by `›`, `>` or `/`, and each is a case-insensitive substring, so `--within linux`
is enough when the name is unique.

## Details that matter

- `--in-headings 'regex'` matches only heading text. It parses every candidate file, so it is
  slower than a normal search.
- Long lines (minified JSON, one-line HTML) print as Fragments around the matches,
  `…[+N] text […+M]`, instead of the whole line.
- `--no-trail` prints exactly what `rg` prints.
- A ripgrep flag that v0.1 does not implement fails with `sfor: --<flag> is not supported
  yet` and exit code 2; drop the flag or use `rg` for that search.
- In `--json`, when two matches on one line have different Trails (two table cells), each
  object in `.data.submatches` carries its own `trail`. The `stats` of `end` and `summary`
  count matches before `--within` filters them.
- Exit codes are ripgrep's: 0 when something matched, 1 when nothing did, 2 on error.
