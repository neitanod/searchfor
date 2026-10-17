# v1 Grammars are compiled in; every other Processor outside sfor is a Helper Processor

Supersedes [ADR 0003](0003-grammars-as-wasm.md). Decided in the Hilo, messages 8451–8459
(2026-10-17).

The tree-sitter Grammars of the v1 code Formats (php, javascript, typescript, tsx, bash, go,
rust, python) are linked into sfor from their crates.io crates, and YAML, JSON and TOML use
native parsers, so the default install parses everything natively with no load time. Every
Processor that lives outside the binary — a Grammar installed with `--install-grammar` and the
External Processors written by users — is a **Helper Processor**: a separate program that sfor
starts once per search and feeds over protocol v1, JSON lines on stdin and stdout, with an id
on every request so a helper can answer several at once (`concurrent`), or as a pool of copies
for helpers that answer one at a time (`pool`), or once per file for simple scripts
(`oneshot`). One mechanism serves both cases.

Measured on this machine (8452, M2 research): starting a process costs 1.5 ms and a JSON round
trip over a pipe 60 µs, so a helper started once per search costs one start per run, while
wasmtime added about 11.5 MB to the binary, multiplied the clean build time by ten, parsed 1.5–
1.6× slower than native code, and needed up to 400 ms to load one Grammar (Bash) in each run.
tree-sitter 0.27 with the `wasm` feature also requires rustc 1.95; without it, tree-sitter
0.27.1 builds with our rustc 1.90.

## Considered Options

- **WebAssembly Grammars through wasmtime** (ADR 0003, 8361): one `.wasm` file for every system
  and a sandbox, at the costs above.
- **Native shared libraries** (`.so`, `.dylib`, `.dll`) loaded into sfor, the literal answer to
  grill #8: native speed, but a static musl binary cannot reliably load them, and a crashing
  Grammar takes sfor down with it.
- **A process per file** for every helper (8454's question): 1.5–3 ms per file with Matches, 1.5–
  3 s for 1 000 files against 15 ms for a whole `rg` run (8455). Kept only as the opt-in
  `oneshot` mode.

## Consequences

- A Grammar Helper is one binary per Grammar, operating system and architecture, instead of one
  `.wasm`. The release pipeline builds them with sfor (M5), and the Grammar index lists the
  SHA-256 of each one, which `--install-grammar` verifies before writing anything.
- A downloaded helper runs outside any sandbox, with the user's permissions; the checksum in the
  index and, from M5, signed releases are the mitigation.
- sfor gains the code that starts helpers, restarts the ones that crash (that file prints as
  Plain), kills the ones that time out, reaps every process it started, and spreads requests
  from many search threads. Helpers share sfor's process group, so a terminal's Ctrl-C reaches
  them as it reaches sfor, which keeps sfor's own reaction to Ctrl-C identical to ripgrep's.
- A helper must free everything between requests; searchfor's own helpers are tested to keep
  their memory flat over thousands of files.
- The binary grows by the compiled-in Grammars: a stripped probe with only them and the keypath
  parsers weighs 8.5 MB, against sfor 0.1's 5.5 MB.
