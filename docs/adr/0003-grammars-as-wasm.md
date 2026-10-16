# tree-sitter Grammars are WebAssembly modules

searchfor loads tree-sitter Grammars as `.wasm` files through tree-sitter's `wasm` feature
(wasmtime), the way the Zed editor does, instead of native shared libraries. One `.wasm` file
works on every operating system and architecture, so installing an extra Grammar is a single
download, and the release binaries can stay fully static — a static musl binary cannot reliably
load `.so` files. The Grammars for the v1 Formats are embedded in the binary so the default
install needs nothing else.

## Considered Options

- **Native shared libraries** (`.so`, `.dylib`, `.dll`), what #8 of the grill first chose:
  native parsing speed, but one file per Grammar per OS and architecture, and a Linux binary
  linked dynamically against glibc.
- **Grammars compiled in, selected with cargo features**: simplest, but adding a language needs
  a rebuild.

## Consequences

The binary carries wasmtime, and WebAssembly parsing is slower than native. Both costs are to be
measured in the first chunk that parses code; parsing only runs on files with Matches.
