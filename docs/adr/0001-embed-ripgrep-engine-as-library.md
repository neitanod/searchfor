# Embed ripgrep's engine as a library instead of calling `rg`

searchfor links ripgrep's published crates (`grep-searcher`, `grep-regex`, `ignore`) and runs
the search in-process, rather than spawning `rg --json` and parsing its output. This keeps
searchfor a single self-contained binary with ripgrep's speed and `.gitignore` handling, and
lets Container and binary Processors feed bytes to the same searcher. The price is that every
ripgrep flag has to be wired up by us instead of passed through, which matters because the
goal is full flag compatibility with `rg`.

## Considered Options

- **Spawn `rg --json`**: every flag for free, but requires `rg` on the user's machine and
  cannot search inside Container Members without extracting them to disk first.
