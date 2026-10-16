# Format Definitions are TOML files, embedded and overridable

Each Format is described by a TOML Format Definition: detection signals (extensions, exact
names, shebangs, magic bytes), the Processor that handles it, and that Processor's parameters
(for example, which tree-sitter node kinds open a Segment). The built-in Definitions are
embedded in the binary; files in the user's formats directory add new Formats or replace
built-in ones by name. A subcommand exports the embedded Definitions to a directory the user
chooses — deliberately not the formats directory itself — so they can serve as templates
without silently overriding the built-ins.

Adding a Format that reuses an existing Processor therefore needs no recompilation; a new
Processor still needs code, either in Rust or as an External Processor.

## Considered Options

- **Formats hard-coded in Rust**: simplest, but every new extension or grammar tweak needs a
  release.
- **External TOML only**: most flexible, but the binary would not work on its own.
