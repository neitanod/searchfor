# Embed ripgrep's engine as a library instead of calling `rg`

searchfor links ripgrep's published crates (`grep-searcher`, `grep-regex`, `ignore`) and runs
the search in-process, rather than spawning `rg --json` and parsing its output. This keeps
searchfor a single self-contained binary with ripgrep's speed and `.gitignore` handling, and
lets Container and binary Processors feed bytes to the same searcher. The price is that every
ripgrep flag has to be wired up by us instead of passed through, which matters because the
goal is full flag compatibility with `rg`.

The search crates are pinned with `=` in `Cargo.toml` to the versions ripgrep 14.1.1 ships
(`grep-searcher` 0.1.14, `grep-printer` 0.2.2, `grep-regex` 0.1.13, `grep-matcher` 0.1.7,
`grep-cli` 0.1.11, `ignore` 0.4.23, `globset` 0.4.15, and the regex crates under them), because
compatibility is measured against rg 14.1.1: later `grep-searcher` and `grep-printer` count
`bytes_searched` and stop at `-m` differently, and later `regex-syntax` and `regex-automata`
carry newer Unicode tables behind `\w`, `\d`, `\b`, `-w` and `-i`. A `cargo update` or a
version bump keeps them, and moving to a later ripgrep moves them all together.

## Considered Options

- **Spawn `rg --json`**: every flag for free, but requires `rg` on the user's machine and
  cannot search inside Container Members without extracting them to disk first.
