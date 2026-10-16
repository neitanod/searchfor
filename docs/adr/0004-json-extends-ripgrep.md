# `--json` is ripgrep's JSON Lines plus searchfor's fields

`--json` emits exactly ripgrep's messages (`begin`, `match`, `context`, `end`, `summary`), one
per line, and adds `trail`, `format` and `fragment` inside each `match.data`. Consumers of
`rg --json` keep working because they ignore unknown fields, the output streams, and searchfor
needs no second JSON flag: `--json --no-trail` is ripgrep's output byte for byte.

## Considered Options

- **A JSON contract of our own**, one self-contained object per Match with its context lines
  inside, plus a `--json-rg` flag for ripgrep's format. Chosen first (Hilo message 8362) and
  replaced (8365): two formats to maintain, and existing ripgrep tooling would not read the
  default one.
