> 🇦🇷 Leer en español: [README.es.md](README.es.md)

# searchfor

`s4` is a ripgrep-compatible search tool that tells you **where** each match lives inside
the structure of its file. The product is called searchfor (the crate, the repository and the
configuration directories), and its command is `s4`.

````
$ s4 needle-pkg md --tree
md/guide.md
  Deploy guide
    # Install
      ## Linux
        ```bash
          12:apt install needle-pkg
````

The lines above `12:apt install needle-pkg` are the match's **Trail**: the document title
from the frontmatter, the headings, and the fenced code block the match sits in. In code the
Trail is the class and the function; in YAML, JSON and TOML it is the key path; in HTML and XML,
the element path; in CSV and TSV, the row and the column; in subtitles, the cue; in an ELF, PE
or Mach-O binary, the section and the nearest symbol; and in a document (searched by default),
the chapter and page of a PDF, the heading and table of a Word file, the sheet and cell of a
spreadsheet, the slide of a presentation, or the table, row and column of a SQLite database.
Since 1.1 it is also the section and key of a configuration file (`http › server example.com ›
location /api`), the statement and row of a SQL dump, the commit, file and hunk of a patch, the
tag of a photo's metadata (`JPEG › APP1 Exif › IFD0 › Model`), and the message of a mailbox or
a chat export, over the [seventy Formats](#formats).

The project's page is https://searchfor.ip1.cc/

Every example in this README is real output over [`tests/fixtures/`](tests/fixtures), run from
inside that directory. The object files and documents under `m4/` are written there by the
writers that [`tests/cli_m4.rs`](tests/cli_m4.rs) uses (`searchfor::processors::object::fixture`
and `searchfor::docs::fixture`), with the same contents. The examples over `/bin/ls`, over the
`--pre` script of [ripgrep compatibility](#ripgrep-compatibility) and of
[Helper Processors](#helper-processors) say what they run over.

## Status

**v1.0 is prepared and not published yet; 1.1, with the Formats of M6, is in development.**
v1.0 runs on Linux, macOS and Windows, and adds prebuilt binaries, Homebrew, npm and install
scripts (see [Install](#install)); each channel opens when its publication is approved, and
until then you install from the repository. 1.1 adds
forty Formats to v1.0's: text documents (AsciiDoc, reStructuredText, Org-mode, LaTeX, Jupyter),
configuration (INI, systemd, `.properties`, `.env`, SSH, nginx, Apache, Terraform and HCL,
Dockerfile, Makefile, justfile), the files next to code (SQL, diffs, Vue, Svelte, Blade,
GraphQL, Protobuf, HAR), the metadata of images, audio and binaries (JPEG, PNG, WebP, TIFF, ID3,
MP4, WebAssembly, Java classes), and mail, calendars, contacts, BibTeX and WhatsApp and Telegram
chats (see [Formats](#formats)). Its one change to a v1.0 Trail is inside the scripts of HTML
pages ([docs/rg-compat.md](docs/rg-compat.md#trails-that-changed-since-10)).

The search engine is ripgrep's own crates, so the search behaves like `rg`, and every flag of
ripgrep 14.1.1 works (see [ripgrep compatibility](#ripgrep-compatibility)); on top of it, each
Format gives its matches Trails, and PDF, Word, Excel, PowerPoint, OpenDocument, EPUB and SQLite
files are searched in their text, with its structure, by default; `--no-docs` turns that off.
Long lines are shown as Fragments, and `--within`, `--expand` and `--in-headings` filter and
expand by structure. `s4 --mcp` serves the search to AI agents over the Model Context Protocol
(see [For AI agents](#for-ai-agents)). Other languages can be described by a
[Helper Processor](#helper-processors), a separate program that s4 talks to. Every other file
is **Plain**: its matches print with an empty Trail.

The glossary is in [CONTEXT.md](CONTEXT.md), the design in
[docs/specs](docs/specs/2026-10-16-searchfor-design.md) and the architecture decisions in
[docs/adr](docs/adr).

## Install

v1.0 is prepared and not published yet: each channel below opens once its publication is
approved, and the GitHub Release of v1.0 is the first one. Until then, install from the
repository, which builds `s4` with Rust 1.90 or later:

```
cargo install --git https://github.com/neitanod/searchfor
```

Once v1.0 is published, any of these installs the `s4` binary:

| Channel | Command | Systems |
|---|---|---|
| Homebrew | `brew install neitanod/tap/searchfor` | macOS |
| npm | `npm install -g searchfor` | the five systems of the table below |
| crates.io | `cargo install searchfor` | any system with Rust 1.90 or later; it builds from source |
| Shell script | `curl --proto '=https' --tlsv1.2 -LsSf https://github.com/neitanod/searchfor/releases/download/v1.0.0/searchfor-installer.sh \| sh` | Linux and macOS |
| PowerShell script | `powershell -ExecutionPolicy Bypass -c "irm https://github.com/neitanod/searchfor/releases/download/v1.0.0/searchfor-installer.ps1 \| iex"` | Windows |
| GitHub Releases | download the archive of your system from https://github.com/neitanod/searchfor/releases | the five systems of the table below |

The prebuilt binaries cover five systems:

| System | Target | Archive |
|---|---|---|
| Linux x86-64 | `x86_64-unknown-linux-musl` | `searchfor-x86_64-unknown-linux-musl.tar.xz` |
| Linux arm64 | `aarch64-unknown-linux-musl` | `searchfor-aarch64-unknown-linux-musl.tar.xz` |
| macOS arm64 (Apple silicon) | `aarch64-apple-darwin` | `searchfor-aarch64-apple-darwin.tar.xz` |
| macOS x86-64 (Intel) | `x86_64-apple-darwin` | `searchfor-x86_64-apple-darwin.tar.xz` |
| Windows x86-64 | `x86_64-pc-windows-msvc` | `searchfor-x86_64-pc-windows-msvc.zip` |

The Linux binaries are static (musl), so they run on any distribution without shared
libraries. Each archive holds `s4`, the two licenses and both READMEs. Windows on arm64 is
planned for a 1.x. The binaries are not Authenticode-signed or notarized by Apple: Homebrew,
npm, cargo and the install scripts download them without the mark that makes SmartScreen or
Gatekeeper ask, while an archive downloaded with a browser may get that question.

### Verify a download

Each archive has a `.sha256` file beside it, and the release has one `sha256.sum` with every
checksum. Compute the archive's SHA-256 and compare it with the one in its `.sha256` file:

```
sha256sum searchfor-x86_64-unknown-linux-musl.tar.xz        # Linux
shasum -a 256 searchfor-aarch64-apple-darwin.tar.xz         # macOS
Get-FileHash searchfor-x86_64-pc-windows-msvc.zip           # Windows, PowerShell
```

Every archive, install script and checksum file also has a GitHub artifact attestation: a
signature, made by GitHub without any private key of ours (Sigstore, keyless), that binds the
file to the workflow run and the commit that built it. With the [GitHub CLI](https://cli.github.com/):

```
gh attestation verify searchfor-x86_64-unknown-linux-musl.tar.xz --repo neitanod/searchfor
```

It prints the workflow and commit the file came from, and fails if the file was changed after
the build.

The npm package checks no checksum: `npm install -g searchfor` downloads the archive of your
system over HTTPS from the GitHub Release. To verify what it installs, download the same
archive from the release and check its attestation with `gh attestation verify`, as above.

### Supported systems

`s4`, its Helper Processors and its whole test suite build and pass on Linux x86-64, macOS
arm64, macOS x86-64 and Windows x86-64, in CI on every push (the suite skips on each system
only the tests whose feature that system lacks, listed in
[docs/platforms.md](docs/platforms.md)). `--no-docs --no-trail` prints what that system's
ripgrep 14.1.1 prints. Where the systems differ:

| Area | Linux | macOS | Windows |
|---|---|---|---|
| Configuration (Format Definitions, `searchfor/formats/`) | `$XDG_CONFIG_HOME/searchfor` (`~/.config/searchfor`) | `~/Library/Application Support/searchfor` | `%APPDATA%\searchfor` |
| Data (Grammar Helpers, `searchfor/grammars/`) | `$XDG_DATA_HOME/searchfor` (`~/.local/share/searchfor`) | `~/Library/Application Support/searchfor` | `%APPDATA%\searchfor` |
| Printed paths | `/` | `/` | as rg: `\` between the components the walker joins; `--path-separator /` changes it |
| Hyperlinks (`--hyperlink-format`) | `file://{host}{path}` | the same | as rg: `file:///C:/…`, the drive letter kept |
| Hidden files | dot-files | dot-files | dot-files, and files with the hidden attribute, as rg |
| Colours | ANSI | ANSI | as rg: ANSI where the console accepts it, the console API otherwise; `--color ansi` forces ANSI |
| Memory maps (`--mmap`) | as rg | as rg on macOS: a search always reads the file; s4 still maps a file of 8 MiB or more to give its Trails, and a document reader maps its document | as rg |
| A document reader's memory | its own count, and `RLIMIT_DATA` 64 MiB above it | the same | its own count, and a Job Object memory limit 64 MiB above it |
| Ctrl-C | s4 ends every helper and reader it started, and then dies by the signal | the same | s4 ends every helper and reader it started, and exits as an unhandled Ctrl-C does (status `0xC000013A`, rg's) |
| When s4 is killed | its document readers die with it; its helpers get the end of their stdin and exit | the same: each reader watches s4 and exits | a Job Object ends every reader and helper with s4 |
| `-P` (PCRE2) | with JIT | without JIT on Apple silicon, slower, as that system's rg; `--pcre2-version` says so | with JIT |

`--help` prints the two directories of the running system. The `--pre` program is looked up
as rg looks it up: on Windows a relative name is searched in `PATH` only, never in the current
directory, so a program left in the directory you search is never run by accident. When
several threads search, a file's output past 8 MiB goes to a temporary file until
the file ends; on Windows, output coloured through the console API stays in memory instead,
because a copy of its bytes would lose the colours.

## Formats

Each Format below gives its files the Trail of the example, and
[docs/formats.md](docs/formats.md) describes each one: its Segments, its limits, what it does
with input it cannot read, and real output over [`tests/fixtures/`](tests/fixtures). The
`--format` column is the name of its Definition, which `--format` and `--list-formats` use.

| Format | `--format` | Files | Example Trail |
|---|---|---|---|
| Markdown | `markdown` | `.md`, `.markdown`, `.mdx`, `.mkd`, `.mdown` | `Deploy guide › Install › Linux › bash` |
| YAML | `yaml` | `.yaml`, `.yml` | `server › ports[2] › host` |
| JSON | `json` | `.json`, `.jsonc`, `.jsonl`, `.geojson`, `.webmanifest` | `server › ports[2] › host` |
| TOML | `toml` | `.toml`, `Cargo.lock`, `Pipfile`, `poetry.lock`, `uv.lock` | `products[1] › tags[1]` |
| PHP | `php` | `.php`, `.phtml` | `Cart › addItem` |
| JavaScript | `javascript` | `.js`, `.mjs`, `.cjs`, `.jsx` | `Widget › render` |
| TypeScript | `typescript` | `.ts`, `.mts`, `.cts` | `Geometry › Base › describe` |
| TSX | `tsx` | `.tsx` | `View › onClick` |
| Bash | `bash` | `.sh`, `.bash`, `.bats`, `.bashrc`, `.profile` | `deploy` |
| Go | `go` | `.go` | `Server.Start` |
| Rust | `rust` | `.rs` | `outer › Thing › helper` |
| Python | `python` | `.py`, `.pyi`, `.pyw` | `Outer › method › inner` |
| HTML | `html` | `.html`, `.htm`, `.xhtml` | `main#content › section#pricing.card.wide › p.note[2]`; inside a script, since 1.1, `head › script[1] › onClick` ([what changed](docs/rg-compat.md#trails-that-changed-since-10)) |
| XML | `xml` | `.xml`, `.svg`, `.rss`, `.plist`, `.csproj`, and files starting with `<?xml` | `catalog › book[2] › title` |
| CSV | `csv` | `.csv` | `row 2 › Plan` |
| TSV | `tsv` | `.tsv` | `row 2 › Role` |
| SubRip | `srt` | `.srt` | `cue 2 00:00:04,000` |
| WebVTT | `vtt` | `.vtt` | `cue intro 00:00.000` |
| ELF | `elf` | any name, by its first bytes | `ELF x86-64 › .rodata › main.version+0xe` |
| PE | `pe` | any name, by its first bytes | `PE x86-64 › .rdata › version_string+0xe` |
| Mach-O | `macho` | any name, by its first bytes; fat files too | `Mach-O arm64 › __TEXT,__cstring › _version+0xe` |
| PDF | `pdf` | `.pdf` | `1 Intro › page 2` |
| Word | `docx` | `.docx`, `.docm`, `.dotx`, `.dotm` | `2 Costs › table 1 › row 2 › Price` |
| Excel | `xlsx` | `.xlsx`, `.xlsm`, `.xltx`, `.xltm` | `Budget › A3` |
| PowerPoint | `pptx` | `.pptx`, `.pptm`, `.potx`, `.potm`, `.ppsx`, `.ppsm` | `slide 1 › notes` |
| OpenDocument | `odt`, `ods`, `odp` | `.odt`, `.ott`, `.ods`, `.ots`, `.odp`, `.otp` | as Word, Excel and PowerPoint |
| EPUB | `epub` | `.epub` | `1. Arrival › The Harbour` |
| SQLite | `sqlite` | `.sqlite`, `.sqlite3`, `.db`, `.db3`, `.s3db`, `.sl3` | `users › row 12 › email` |
| AsciiDoc | `asciidoc` | `.adoc`, `.asciidoc` | `Deploy guide › Install › Linux › bash` |
| reStructuredText | `rst` | `.rst`, `.rest` | `Install › Linux › bash` |
| Org-mode | `org` | `.org` | `Budget › TODO Costs :work: › Price` |
| LaTeX | `latex` | `.tex`, `.ltx`, `.latex` | `A Study of Needles › Methods › Sampling › figure` |
| Jupyter | `jupyter` | `.ipynb` | `Analysis › Load data › cell 2 › output 1 › text/plain` |
| INI | `ini` | `.ini`, `.cfg`, `.desktop`, `.gitconfig`, `.editorconfig`, `.git/config` | `remote "origin" › url` |
| systemd units | `systemd` | `.service`, `.socket`, `.timer`, `.mount`, `systemd/**/*.target` | `Service › ExecStart` |
| Java properties | `properties` | `.properties` | `spring.datasource.url` |
| dotenv | `env` | `.env`, `.env.*`, `*.env` | `DATABASE_URL` |
| SSH configuration | `ssh-config` | `ssh_config`, `sshd_config`, `.ssh/config` | `Host github.com gitlab.com › IdentityFile` |
| nginx | `nginx` | `nginx.conf`, `nginx/**/*.conf`, `nginx/sites-enabled/*` | `http › server example.com › location /api › proxy_pass` |
| Apache | `apache` | `httpd.conf`, `apache2.conf`, `.htaccess`, `apache2/**/*.conf` | `VirtualHost *:443 example.com › Directory /var/www › Require` |
| Terraform and HCL | `hcl` | `.tf`, `.tfvars`, `.hcl`, `.nomad` | `resource aws_instance web › tags › Name` |
| Dockerfile | `dockerfile` | `Dockerfile`, `Containerfile`, `Dockerfile.*`, `.dockerfile` | `build › RUN` |
| Makefile | `make` | `Makefile`, `GNUmakefile`, `Makefile.*`, `.mk` | `ifeq ($(OS),Windows_NT) › release test` |
| justfile | `just` | `justfile`, `.justfile`, `.just` | `deploy` |
| SQL | `sql` | `.sql`, `.ddl`, `.dml` | `INSERT INTO users › row 2 › email` |
| Diffs and patches | `diff` | `.diff`, `.patch`, `.rej` | `3f2a9c1 Fix the needle crash › src/app.rs › +120 fn describe(` |
| Vue | `vue` | `.vue` | `script › addItem` |
| Svelte | `svelte` | `.svelte` | `ul › #each items as item › li[1]` |
| Blade | `blade` | `.blade.php` | `@section('content') › div.cart › @foreach($items as $item) › li` |
| GraphQL | `graphql` | `.graphql`, `.gql`, `.graphqls` | `GetUser › user › posts › title` |
| Protobuf | `protobuf` | `.proto` | `User › choice › nick` |
| HAR | `har` | `.har` | `GET api.example.com/v1/users › request › header Authorization › value` |
| JPEG | `jpeg` | any name, by its first bytes | `JPEG › APP1 Exif › IFD0 › Model` |
| PNG | `png` | any name, by its first bytes | `PNG › tEXt Comment` |
| WebP | `webp` | any name, by its first bytes | `WebP › EXIF › IFD0 › Model` |
| TIFF | `tiff` | any name, by its first bytes | `TIFF › IFD0 › Model` |
| ID3 (MP3) | `id3` | any name, by its first bytes | `ID3v2.4 › TIT2` |
| MP4 | `mp4` | any name, by its first bytes | `MP4 M4A › moov › udta › meta › ilst › ©nam` |
| WebAssembly | `wasm` | any name, by its first bytes | `WebAssembly › data › data[0]` |
| Java class | `java-class` | any name, by its first bytes | `Java 17 › com.example.Cart › addItem` |
| Mail | `eml` | `.eml` | `part 1 multipart/alternative › part 1.1 text/plain` |
| mbox | `mbox` | `mbox`, `.mbox`, `.mbx` | `Budget › body` |
| iCalendar | `icalendar` | `.ics`, `.ical`, `.ifb` | `VEVENT Team sync › DESCRIPTION` |
| vCard | `vcard` | `.vcf`, `.vcard` | `VCARD Ana Pérez › EMAIL` |
| BibTeX | `bibtex` | `.bib` | `knuth1984 › title` |
| WhatsApp | `whatsapp` | `_chat.txt`, `*WhatsApp*.txt` | `31/12/23 › 21:41 Ana` |
| Telegram | `telegram` | `ChatExport_*/result.json` | `Ana › 2024-01-31T10:04:12 Ana › text` |
| Telegram HTML | `telegram-html` | `ChatExport_*/messages*.html` | `31.01.2024 10:04:12 Ana` |

The files of this table are abridged: [docs/formats.md](docs/formats.md#every-format) lists
every name, extension and path each Format claims. A file without an extension is also
recognised by its shebang (`#!/usr/bin/env python3`, `node`, `bash`…), and the binary Formats by
their first bytes whatever their name. s4 never looks at a text file's content to choose its
Format: `--format 'GLOB=NAME'` names it where the name does not (`--format 'chats/*.txt=whatsapp'`),
and `git diff | s4 --format diff needle` gives Trails to stdin. Every other file is **Plain**: its
matches print with an empty Trail, as `rg` prints them.

The Trail is the same in every layout, and the binary Formats print one row per string at its
hex offset where ripgrep would print `binary file matches`:

```
$ s4 needle config/nginx/nginx.conf --tree
config/nginx/nginx.conf
  http
    server example.com
      location /api
        proxy_pass
          4:   proxy_pass http://needle:8080;
```

```
$ s4 needle m4/bin/app --flat
0x68: ELF x86-64 › .rodata › main.version+0xe │ version=1.2.3-needle
0x77: ELF x86-64 › .rodata │ another needle here
```

By default, PDF, Word, Excel, PowerPoint, OpenDocument, EPUB and SQLite files are searched in
the text that s4's built-in Extractors take from them, in a terminal and in a pipe alike, each
read in a worker process with its own memory and time limits. With `--no-docs`, these Formats
are never detected: the files are searched as ripgrep searches them, at no extra cost, and
`s4 --no-docs --no-trail` prints what `rg` prints. [docs/formats.md](docs/formats.md#documents)
has the details, and [docs/rg-compat.md](docs/rg-compat.md#trails-that-changed-since-10) the
one Trail 1.1 changed: the scripts of HTML pages.

## Usage

`s4 PATTERN [PATHS…]` takes ripgrep's flags with ripgrep's names. On a terminal the default
layout is the tree; in a pipe it is one line per match, `path:line: Trail │ text`.

### One line per match (`--flat`, the default in a pipe)

```
$ s4 needle md --flat
md/guide.md:7: Deploy guide › Install │ Intro paragraph with needle-intro.
md/guide.md:12: Deploy guide › Install › Linux › bash │ apt install needle-pkg
md/guide.md:16: Deploy guide › Install › Linux › step one │   - nested needle-item
md/guide.md:23: Deploy guide › Install › Plans › Price │ | Pro  | needle-10 |
md/guide.md:24: Deploy guide › Install › Plans › Plan │ | needle-a | needle-b |
md/guide.md:24: Deploy guide › Install › Plans › Price │ | needle-a | needle-b |
md/guide.md:28: Deploy guide › Install › Plans › Cell │ > | q | needle-quoted |
md/guide.md:31: Deploy guide › Install › Plans › Heading in item › Heading in item │   needle-in-item
md/guide.md:33: Deploy guide › Install › Plans │ needle-after-list
md/guide.md:38: Deploy guide › Setext Title │ needle-setext body
```

Line 24 holds two matches in two table cells, so it prints once per column.

### Only the matches under a Trail (`--within`)

```
$ s4 needle md --within "install › linux" --flat
md/guide.md:12: Deploy guide › Install › Linux › bash │ apt install needle-pkg
md/guide.md:16: Deploy guide › Install › Linux › step one │   - nested needle-item
```

`--within` keeps the matches whose Trail contains these Segment names in this order. Names
are separated by `›`, `>` or `/`, and each one is a case-insensitive substring. Since `/`
always separates, a heading with a slash in it, like `TCP/IP`, is named by a part without
the slash, between `›` or `>`: `--within "network › tcp"`.

### The whole Section (`--expand`)

```
$ s4 needle-10 md --expand --tree
md/guide.md
  Deploy guide
    # Install
      ## Plans
        column «Price»
          21-| Plan | Price |
          22-|------|-------|
          23:| Pro  | needle-10 |
          24-| needle-a | needle-b |
```

`--expand` prints the Section of the last Segment of each match: the whole code block, list
item, table or heading section. A Section with several matches prints once.

### Long lines

A line wider than the terminal (or than `--fragment-width`, 200 by default, in a pipe) is
shown as Fragments centred on the matches, with the number of omitted characters on each side:

```
$ s4 needle misc/min.json --flat --fragment-width 120
1: [0] │ …[+983] aaaaaaaaaaaaaaaaaneedleaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaneedleaaaaaaaaaaaaaaaaaa […+18926]
1: [0] │ …[+14958] aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaneedleaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa […+4951]
```

Matches close enough to fit in one Fragment share it; the others get their own line.
ripgrep's `-M/--max-columns` keeps its meaning: with it, long lines print ripgrep's
`[Omitted long matching line]` message instead of Fragments.

### searchfor's own flags

| Flag | Meaning |
|---|---|
| `--within <TRAIL>` | keep matches whose Trail contains these Segment names, in order (`›`, `>` or `/` between names; case-insensitive substrings) |
| `--expand` | print the whole Section of the last Segment of each match, once per Section |
| `--in-headings` | match only inside Segment names (headings, keys, function names, elements, CSV headers, cues); every candidate file is parsed, so it is slower |
| `--no-docs` | search PDF, Word/ODT, Excel/ODS, PowerPoint/ODP, EPUB and SQLite files as ripgrep does, at ripgrep's speed, instead of in their text through the built-in Extractors, which is the default (see [Documents](docs/formats.md#documents)) |
| `--docs` | search the documents in their text again after a `--no-docs`; the last of the two wins |
| `--format <F>` / `--format <GLOB>=<F>` | force a Format for every file or for the files matching a glob |
| `--flat` / `--tree` | force the one-line or the tree layout |
| `--no-trail` | print what ripgrep would print; with `--no-docs`, exactly that |
| `--fragment-width <N>` | Fragment width when there is no terminal (default 200) |
| `--list-formats` | list every Format with its origin: built-in, installed, user or overridden |
| `--export-formats <DIR>` | write the built-in Format Definitions to `DIR` |
| `--list-grammars` | list the compiled-in, installed and available Grammars (see [Grammars](#grammars)) |
| `--install-grammar <LANG>` | install a Grammar Helper from the Grammar index |
| `--mcp` | serve the search as an MCP server over stdin and stdout (see [For AI agents](#for-ai-agents)) |

Format Definitions are TOML files. A file in `searchfor/formats/` under the user config
directory (`~/.config` or `$XDG_CONFIG_HOME` on Linux, `~/Library/Application Support` on
macOS, `%APPDATA%` on Windows; see [Supported systems](#supported-systems)) with the name of a built-in or
installed Format replaces it, and a new name adds a Format.

### ripgrep compatibility

ripgrep 14.1.1 has 146 flag names (104 flags, 30 `--no-` negations and 12 other negations),
and s4 implements all of them; the table is [docs/rg-compat.md](docs/rg-compat.md). Exit codes
are ripgrep's: 0 when something matched, 1 when nothing did, 2 on error. `SEARCHFOR_CONFIG_PATH`
names a configuration file in the format of ripgrep's `RIPGREP_CONFIG_PATH`, and `--no-config`
skips it.

With `--no-trail`, the output is written by ripgrep's own printer, and `--json --no-trail`
prints the same messages as `rg --json`, timings aside. Documents are the exception: add
`--no-docs` for ripgrep's output over them too.

ripgrep's output flags also change the Trail layouts: `--column` and `-b` add the column and the
byte offset, `-o` prints each match alone under its own Trail, `--vimgrep` a line per match for
an editor, `-r` replaces the matches, `-U` prints each line of a match that spans lines, and
the separators, `-0`, `--path-separator`, `--colors` (with a `trail` type of its own) and
`--hyperlink-format` work as in ripgrep. `--heading` is the tree and `--no-heading` the one-line
layout:

```
$ s4 --flat --column -o 'needle-\w+' md/guide.md
7:22: Deploy guide › Install │ needle-intro
12:13: Deploy guide › Install › Linux › bash │ needle-pkg
16:12: Deploy guide › Install › Linux › step one │ needle-item
23:10: Deploy guide › Install › Plans › Price │ needle-10
…
$ s4 --vimgrep needle-pkg md
md/guide.md:12:13: Deploy guide › Install › Linux › bash │ apt install needle-pkg
$ s4 -P --flat 'needle-(?=pkg)' md
md/guide.md:12: Deploy guide › Install › Linux › bash │ apt install needle-pkg
```

`--pre`, `--pre-glob`, `--no-pre`, `-z` and `--no-search-zip` work as in ripgrep 14.1.1, errors
and exit codes included, and of `-z` and `--pre` the one given last wins. What they output is
searched as stdin is, with the Trails of the Format that the file's path names. Here `pre.sh`,
written for this example, is `sed s/needle-pkg/NEEDLE-PKG/ "$1"`:

```
$ s4 --pre ./pre.sh --pre-glob '*.md' --flat NEEDLE-PKG md
md/guide.md:12: Deploy guide › Install › Linux › bash │ apt install NEEDLE-PKG
```

Known differences with ripgrep 14.1.1:

- In `--json` output, the `stats` of the `end` and `summary` messages count the matches found
  by the search, before `--within` and `--in-headings` filter them out.
- A file of a binary Format with a match (ELF, PE, Mach-O, and since 1.1 images, audio,
  WebAssembly and Java classes) prints the rows of [Object files](docs/formats.md#object-files) where
  ripgrep prints `binary file matches`, so s4 searches the whole file; in `--json`,
  `bytes_searched` is the file's size and `binary_offset` is null.
- Documents are searched in their text by default, also with `--no-trail`, `-c`, `-l`, `-q`
  and `--json`, and each one is extracted, which costs more than ripgrep's search of its bytes.
  `--no-docs` gives ripgrep's output and cost (see [Documents](docs/formats.md#documents)).
- In the Trail layouts, a file whose Format its name does not decide, stdin among them, is read
  whole before its rows are printed; `--no-trail` prints each line as ripgrep does.
- `--debug` prints the messages of ripgrep's crates as ripgrep does, and s4's own; `--generate`
  writes s4's man page and completions; `-P` uses the PCRE2 s4 is built with.

The full list is in [docs/rg-compat.md](docs/rg-compat.md#known-deviations).

## Helper Processors

A Helper Processor is a program, written in any language, that describes the files of one
Format: s4 starts it when the first file of that Format has a match, sends it the path and
the match positions of each file over JSON lines on its stdin, reads the Trails from its
stdout, and closes its stdin when the search ends. A Format Definition with
`processor = "external"` names it:

```toml
name       = "po-guide"
extensions = ["po"]
processor  = "external"

[params]
command    = ["python3", "/absolute/path/to/docs/examples/po-helper.py"]
mode       = "oneshot"
```

`mode` says how s4 feeds it:

- `pool` (the default): each copy gets one file at a time, and s4 starts more copies, up
  to the search's thread count, when every copy is busy. A program that reads a line,
  answers it and reads the next one works in this mode.
- `concurrent`: one process gets several files at once and may answer in any order, since
  each answer carries the `id` of its request.
- `oneshot`: one process per file, for small scripts that read their whole stdin before
  answering.

A helper never stops the search and never changes the exit code. A file whose request fails
(an `error` answer, a crash, no answer within `timeout_ms`, 5000 by default) prints as Plain,
with a warning; after three crashes, timeouts or lines that are not JSON in one run, the
Format prints as Plain for the rest of the run, with one more warning.

s4 has no gettext Format, so a `.po` file is Plain. With the Definition above in the formats
directory, [`po-helper.py`](docs/examples/po-helper.py) gives the match in
[`docs/examples/sample.po`](docs/examples/sample.po) the Trail of its context and entry, and its
`names` let `--in-headings` find the entry by its source string:

```
$ s4 --flat needle sample.po
3: menu › Open │ msgstr "needle"
$ s4 --flat --in-headings Open sample.po
2: menu │ msgid "Open"
```

The protocol is in
[spec §4.4](docs/specs/2026-10-16-searchfor-design.md#44-helper-processors-7-84518459).
[docs/external-processors.md](docs/external-processors.md) is the guide for writing one: the
protocol message by message, the three modes, the failures and how s4 reports them, and this
worked example in Python, first with `--pre` and then as a Helper Processor, with the recipe
for each system. [`helpers/example/`](helpers/example) is a reference implementation in Rust,
which describes INI files; a Definition of it named `ini` replaces the built-in INI Format.

## Grammars

The Grammars of the eight code Formats are compiled into `s4`. Any other language comes as
a Grammar Helper: a Helper Processor built by searchfor with one Grammar, which
`--install-grammar` downloads from the Grammar index, checks against its SHA-256, and installs
with its Format Definition under `$XDG_DATA_HOME/searchfor/grammars/` (on Linux,
`~/.local/share/searchfor/grammars/` by default).

```
$ s4 --list-grammars
php            compiled in
javascript     compiled in
typescript     compiled in
tsx            compiled in
bash           compiled in
go             compiled in
rust           compiled in
python         compiled in
```

v1.0 adds six Grammar Helpers: **c, cpp, java, ruby, c-sharp and elixir**, each built for the
five systems of [Install](#install) and published in a GitHub Release of their own,
`grammars-v1`. Once that release is published and the index lists it, this installs one:

```
s4 --install-grammar ruby
```

The index, [`grammars/index.json`](grammars/index.json), stays empty until then, so
`--install-grammar` has nothing to install from the repository yet.
[`helpers/grammar/`](helpers/grammar) is the template they are built from, and
[`scripts/build-grammar-helper.sh`](scripts/build-grammar-helper.sh) builds one for the
running system and prints its index entry (the binary is `<lang>`, and `<lang>.exe` on
Windows). `SEARCHFOR_GRAMMAR_INDEX` points s4 at another index, such as a `file://` one of
your own. On macOS and Windows the Grammar directory is under the data directory of
[Supported systems](#supported-systems).

## For AI agents

An agent that finds a match with `rg` usually opens the whole file to learn which section the
line belongs to. `s4` prints that section in the Trail, and `--expand` prints the section
itself, so the agent reads the part of the file that matters and nothing else.

### MCP server

`s4 --mcp` speaks the Model Context Protocol over stdin and stdout and offers one tool,
`search`, with `pattern`, `paths`, `within`, `expand`, `glob`, `case_insensitive` and
`max_results` (100 when not given, at most 1 000). It answers with each match's path, line,
Trail and text, and with `expand` its Section, in at most 256 KiB: a line with matches under
two Trails comes once per Trail, a long line as its Fragment, a long Section cut, a Section
shared by several matches once, and `truncated` says the search found more than the answer
holds, `truncated_by` why (`max_results`, `bytes`, or `long_line` for a line longer than 4 MiB,
whose file `errors` names, the first 20 of them, counting the rest). Its searches read no
configuration file, so a
`--pre` in `SEARCHFOR_CONFIG_PATH` never runs for a client. To add it to Claude Code:

```
claude mcp add searchfor -- s4 --mcp
```

A call of the tool, and its answer:

```
{"name":"search","arguments":{"pattern":"needle","paths":["md"],"within":"Install › Linux"}}
{"matches":[{"path":"md/guide.md","line":12,"trail":["Deploy guide","Install","Linux","bash"],
  "text":"apt install needle-pkg","format":"markdown"}, …],"truncated":false}
```

### From the command line

Which section is each match in:

```
$ s4 --json needle md | jq -r 'select(.type == "match") | .data.trail | map(.name) | join(" › ")'
Deploy guide › Install
Deploy guide › Install › Linux › bash
Deploy guide › Install › Linux › step one
Deploy guide › Install › Plans › Price
Deploy guide › Install › Plans › Plan
Deploy guide › Install › Plans › Cell
Deploy guide › Install › Plans › Heading in item › Heading in item
Deploy guide › Install › Plans
Deploy guide › Setext Title
```

Read only the sections under a heading:

````
$ s4 needle md --within linux --expand --flat
md/guide.md:11-13: Deploy guide › Install › Linux › bash
```bash
apt install needle-pkg
```

md/guide.md:15-16: Deploy guide › Install › Linux › step one
- step one
  - nested needle-item

````

Read only the method a match is in, from code:

```
$ s4 needle code --within "Cart › addItem" --expand --flat
code/cart.php:8-12: Cart › addItem
    public function addItem(Item $item, int $qty = 1): void
    {
        // needle: merge equal items
        $this->items[] = [$item, $qty];
    }

```

Read every function named like `helper` that holds a match:

```
$ s4 needle code --within helper --expand --flat --sort path
code/cart.php:15-18: needle_helper
function needle_helper(): string
{
    return "needle";
}

code/server.go:11-13: helper
func helper() string {
	return "needle"
}

code/thing.rs:7-9: outer › Thing › helper
        pub fn helper(&self) -> u32 {
            self.needle + 1
        }

```

Find the slide, sheet or page where a phrase is:

```
$ s4 --flat 'needle joke' m4/deck.pptx m4/deck.odp
m4/deck.pptx:3: slide 1 › notes │ tell the needle joke
m4/deck.odp:3: slide 1 › notes │ tell the needle joke
```

Find which section and symbol of a binary hold a string, with `--binary` over a directory:

```
$ s4 --binary --flat --sort path version= m4/bin
m4/bin/app:0x5a: ELF x86-64 › .rodata › main.version │ version=1.2.3-needle
m4/bin/app-mac:0x1133: Mach-O x86-64 › __TEXT,__cstring › _version │ version=1.2.3-needle
m4/bin/app-mac:0x2133: Mach-O arm64 › __TEXT,__cstring › _version │ version=1.2.3-needle
m4/bin/app-stripped:0x5a: ELF x86-64 › .rodata │ version=1.2.3-needle
m4/bin/app.exe:0x402: PE x86-64 › .rdata › version_string │ version=1.2.3-needle
```

`--json` emits ripgrep's JSON Lines (`begin`, `match`, `context`, `end`, `summary`), so every
`rg --json` consumer keeps working, and adds three fields to each `match`:

```
{"type":"match","data":{"path":{"text":"md/guide.md"},"lines":{"text":"apt install needle-pkg\n"},"line_number":12,"absolute_offset":96,"submatches":[{"match":{"text":"needle"},"start":12,"end":18}],"format":"markdown","trail":[{"kind":"document","name":"Deploy guide","line":1,"end_line":38},{"kind":"heading","name":"Install","line":5,"end_line":34,"level":1},{"kind":"heading","name":"Linux","line":9,"end_line":18,"level":2},{"kind":"code","name":"bash","line":11,"end_line":13}],"fragment":null}}
```

- `format`: the file's Format, or `null` for Plain files.
- `trail`: the Segments from the root of the file down to the match, each with `kind`,
  `name`, `line` and `end_line` (the lines of its Section), `level` for headings, and
  `signature` for code (the line that declares it) and for subtitle cues (the cue with its end
  time).
- `fragment`: `null`, or the Fragment of a line longer than `--fragment-width`.
- With `--expand`, `section`: `{"start_line", "end_line", "text"}` of the Section.
- When the matches of one line have different Trails (two table cells), each object in
  `submatches` carries its own `trail`.

The schema of these fields is [docs/json-schema.json](docs/json-schema.json). The `stats` in
the `end` and `summary` messages count matches before `--within` and `--in-headings` filter
them.

A Claude Code skill with these recipes is in [skills/searchfor](skills/searchfor/SKILL.md);
copy that directory to `~/.claude/skills/` to install it.

## Roadmap

| Milestone | Release | Content |
|---|---|---|
| M1 | v0.1 | Markdown, the core ripgrep flags, tree / one-line / JSON output, Fragments, `--within`, `--expand`, `--in-headings`, Claude Code skill |
| M2 | v0.2 | YAML, JSON and TOML key paths (`server › ports[2] › host`); code (PHP, JavaScript, TypeScript, TSX, Bash, Go, Rust, Python) as `Cart › addItem`, with tree-sitter Grammars compiled in; Helper Processors (`processor = "external"`); `--install-grammar` and `--list-grammars` |
| M3 (done, 2026-10-17) | v0.3 | HTML and XML element paths (`section#pricing › table › tr[3] › td[2]`); CSV and TSV rows and columns (`row 2 › Plan`); SubRip and WebVTT cues (`cue 2 00:00:04,000`); control characters in printed names shown as escapes |
| M4 (done, 2026-10-19) | v0.4 | object files (ELF, PE, Mach-O, fat Mach-O) as `ELF x86-64 › .rodata › main.version+0xe`; documents searched by default (`--no-docs` turns it off), with Extractors for PDF, Word, Excel, PowerPoint, OpenDocument text, spreadsheet and presentation, EPUB and SQLite, each document read in a worker process that counts its own memory; ripgrep's `--pre`, `--pre-glob`, `--no-pre`, `-z` and `--no-search-zip`; the M4 benchmark, performance ceilings and fuzz targets |
| M5a (done, 2026-10-20) | v0.5 | every ripgrep 14.1.1 flag, in `--no-trail` as ripgrep prints it and in the Trail layouts (PCRE2, multi-line search, encodings, the ignore and sort flags, `--files`, `-o`, `--column`, `--vimgrep`, `-r`, `--stats`, hyperlinks, a configuration file, `--generate`, `--debug`); an MCP server (`s4 --mcp`) |
| M5b (prepared, 2026-10-20; publication pending) | v1.0 | prebuilt binaries (GitHub Releases), Homebrew and npm; Windows and macOS in CI, where the readers' memory count is run, and a Windows Job Object over each document reader as its second barrier; Grammar Helper binaries published in the Grammar index; signed releases; the guide for writing a Helper Processor; streaming the Trails of files where most lines match |
| M6 (in development) | v1.1 | forty Formats: AsciiDoc, reStructuredText, Org-mode, LaTeX and Jupyter; INI, systemd, `.properties`, `.env`, SSH, nginx, Apache, Terraform and HCL, Dockerfile, Makefile and justfile; SQL, diffs, Vue, Svelte, Blade, GraphQL, Protobuf and HAR; JPEG, PNG, WebP, TIFF, ID3, MP4, WebAssembly and Java classes (at the hex offset of the match, as object files); mail, mbox, iCalendar, vCard, BibTeX, WhatsApp and Telegram; Trails inside the scripts of HTML pages; `paths` and compound extensions in Format Definitions |

The binary Office formats (`.doc`, `.xls`, `.ppt`) and the headers, footers, footnotes and
comments of Office documents, listed for M6 before its design, stay out of it. Archive Formats
(zip, tar, 7z, rar, deb, rpm, jar) stay out too: searchfor searches what ripgrep
searches, and `-z` keeps ripgrep's meaning (single-stream compressed files such as `.gz`).

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
