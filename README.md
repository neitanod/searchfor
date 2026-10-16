> 🇦🇷 Leer en español: [README.es.md](README.es.md)

# searchfor

`sfor` is a ripgrep-compatible search tool that tells you **where** each match lives inside
the structure of its file.

```
$ sfor ExecStart docs/
docs/deploy.md
  # Deploying Go apps
    ## systemd
      42: ExecStart=/opt/apps/notes/notes
```

- **Markdown**: the chain of headings, plus the code block and table column.
- **Code**: class › function, via tree-sitter.
- **YAML, JSON, TOML**: the key path, like `server.ports[2].host`.
- **HTML, XML**: the element path.
- **CSV**: the row and the column name.
- **Binaries (ELF, PE, Mach-O)**: format, section, offset, nearest symbol and the printable
  string around the match.
- **Containers (zip, docx, epub, jar, tar.gz)**: it searches inside and shows the member.

Long lines are shown as a fragment centred on the match instead of being hidden, and
`--json` emits one object per match for AI agents.

## Status

Design stage. Nothing to install yet. The glossary is in [CONTEXT.md](CONTEXT.md) and the
architecture decisions are in [docs/adr](docs/adr).

## Usage

Not available yet. The command line will accept ripgrep's flags with the same names.

## Install

Not available yet. Planned channels: crates.io, GitHub Releases, Homebrew and npm.

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
