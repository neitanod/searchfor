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

Every example in this README is real output over [`tests/fixtures/`](tests/fixtures), run from
inside that directory. The object files and documents under `m4/` are written there by the
writers that [`tests/cli_m4.rs`](tests/cli_m4.rs) uses (`searchfor::processors::object::fixture`
and `searchfor::docs::fixture`), with the same contents. The examples over `/bin/ls`, over the
`--pre` script of [ripgrep compatibility](#ripgrep-compatibility) and of
[Helper Processors](#helper-processors) say what they run over.

## Status

**v1.0 is prepared and not published yet.** It runs on Linux, macOS and Windows, and adds
prebuilt binaries, Homebrew, npm and install scripts (see [Install](#install)); each channel
opens when its publication is approved, and until then the version you install from the
repository is v0.5. **v0.5: every ripgrep flag, and an MCP server.** The search engine is ripgrep's own crates, so
the search behaves like `rg`, and every flag of ripgrep 14.1.1 works (see
[ripgrep compatibility](#ripgrep-compatibility)); on top of it, Markdown, YAML, JSON, TOML, code in eight languages, HTML, XML, CSV, TSV,
SubRip and WebVTT get Trails, and so do the matches ripgrep finds in ELF, PE and Mach-O binaries
(see [Formats](#formats)). PDF, Word, Excel, PowerPoint, OpenDocument, EPUB and SQLite files
are searched in their text, with its structure, by default; `--no-docs` turns that off (see
[Documents](#documents)). Long lines are shown as Fragments, and
`--within`, `--expand` and `--in-headings` filter and expand by structure. `s4 --mcp` serves
the search to AI agents over the Model Context Protocol (see [For AI agents](#for-ai-agents)).
Other languages
can be described by a [Helper Processor](#helper-processors), a separate program that s4
talks to. Every other file is **Plain**: its matches print with an empty Trail. More Formats
come by milestone (see [Roadmap](#roadmap)).

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

| Format | Files | Segments |
|---|---|---|
| Markdown | `.md`, `.markdown`, `.mdx`, `.mkd`, `.mdown` | frontmatter title, headings, fenced code blocks, parent list items, table columns |
| YAML | `.yaml`, `.yml` | keys and array elements; `document N` for each document of a stream |
| JSON | `.json`, `.jsonc`, `.jsonl`, `.geojson`, `.webmanifest`, `.babelrc`, `.eslintrc` | keys and array elements; comments and trailing commas are accepted |
| TOML | `.toml`, `Cargo.lock`, `Pipfile`, `poetry.lock`, `uv.lock` | keys, tables and arrays of tables |
| PHP | `.php`, `.phtml` | namespaces, classes, interfaces, traits, enums, methods, functions |
| JavaScript | `.js`, `.mjs`, `.cjs`, `.jsx` | classes, methods, functions, and arrow functions or function expressions given a name |
| TypeScript | `.ts`, `.mts`, `.cts` | the JavaScript ones, plus namespaces, interfaces and abstract classes |
| TSX | `.tsx` | the TypeScript ones |
| Bash | `.sh`, `.bash`, `.bats`, `.bashrc`, `.bash_profile`, `.bash_aliases`, `.profile` | functions |
| Go | `.go` | types, functions, and methods named by their receiver (`Server.Start`) |
| Rust | `.rs` | modules, structs, enums, traits, `impl` blocks, functions |
| Python | `.py`, `.pyi`, `.pyw` | classes and functions, nested ones included |
| HTML | `.html`, `.htm`, `.xhtml` | elements as `tag#id.class`, with `[n]` among siblings of the same tag; `html` and `body` hidden |
| XML | `.xml`, `.xsd`, `.xsl`, `.xslt`, `.svg`, `.rss`, `.atom`, `.plist`, `.wsdl`, `.xlf`, `.xliff`, `.kml`, `.gpx`, `.csproj`, `.vbproj`, `.fsproj`, `.nuspec`, `.resx`, `.xaml`, and files starting with `<?xml` | elements by tag, with `[n]` among siblings of the same tag |
| CSV | `.csv` | `row N › Column`; the delimiter (`,` `;` tab `\|`) is read from the first line |
| TSV | `.tsv` | `row N › Column` |
| SubRip | `.srt` | cues: `cue 12 00:01:02,500` |
| WebVTT | `.vtt` | cues: `cue intro 00:00.000` |
| ELF | files whose first bytes say so, any name | image › section › symbol, at the hex offset: `ELF x86-64 › .rodata › main.version+0xe` |
| PE | files whose first bytes say so, any name | image › section › symbol, at the hex offset: `PE x86-64 › .rdata › version_string+0xe` |
| Mach-O | files whose first bytes say so, any name; fat (universal) files too | image › section › symbol, at the hex offset: `Mach-O arm64 › __TEXT,__cstring › _version+0xe` |
| PDF | `.pdf`, and files starting with `%PDF-` | bookmarks as chapters, and pages: `1 Intro › page 2` |
| Word | `.docx`, `.docm`, `.dotx`, `.dotm` | headings, and tables: `2 Costs › table 1 › row 2 › Price` |
| Excel | `.xlsx`, `.xlsm`, `.xltx`, `.xltm` | sheets and cells: `Budget › A3` |
| PowerPoint | `.pptx`, `.pptm`, `.potx`, `.potm`, `.ppsx`, `.ppsm` | slides and their notes: `slide 1 › notes` |
| OpenDocument text | `.odt`, `.ott`, and files whose `mimetype` says so | as Word |
| OpenDocument spreadsheet | `.ods`, `.ots`, and files whose `mimetype` says so | as Excel |
| OpenDocument presentation | `.odp`, `.otp`, and files whose `mimetype` says so | as PowerPoint |
| EPUB | `.epub`, and files whose `mimetype` says so | chapters, named by the table of contents, and headings: `1. Arrival › The Harbour` |
| SQLite | `.sqlite`, `.sqlite3`, `.db`, `.db3`, `.s3db`, `.sl3`, and files starting with `SQLite format 3` | tables, rows and columns: `users › row 12 › email` |

A file without an extension is also recognised by its shebang (`#!/usr/bin/env python3`,
`node`, `bash`…). The code Formats are parsed by tree-sitter Grammars compiled into `s4`;
YAML, JSON and TOML by native parsers; HTML, XML, CSV, TSV and subtitles by scanners of s4's
own; object files by goblin; and documents by Extractors built into `s4`, written in Rust, with
no external program. `s4 --list-formats` lists them all.

The same key path in a YAML file and in a JSON file:

```
$ s4 needle.example data --flat --sort path
data/config.json:8: server › ports[2] › host │       {"host": "needle.example", "port": 8443},
data/config.yaml:9: server › ports[2] › host │     - host: needle.example
```

`ports[2]` is the third element of the array held by `ports`, so `ports` itself is left out
of the Trail. In code, the one-line output prints names and the tree prints the line that
declares each Segment:

```
$ s4 needle code/cart.php --tree
code/cart.php
  class Cart
    public function addItem(Item $item, int $qty = 1): void
      10:        // needle: merge equal items
  15:function needle_helper(): string
    17:    return "needle";
```

```
$ s4 needle code/cart.php --flat
10: Cart › addItem │         // needle: merge equal items
15: function needle_helper(): string
17: needle_helper │     return "needle";
```

A match in the name of a function, as on line 15, is outside that function, as a match on a
heading line is outside its section.

In HTML and XML the Trail is the element path:

```
$ s4 needle markup/page.html --flat
4: head › title │   <title>Pricing needle</title>
5: head › script │   <script>if (a < b) { label = "</div> needle"; }</script>
12: nav.top.menu › ul › li[3] │       <li><a href="/needle">Needle</a>
19: main#content › section#pricing.card.wide › p.note[2] │       <p class="note">The needle plan is free.
23: main#content › section#pricing.card.wide › table › tr[3] › td[2] │         <tr><td>Pro<td>needle 9
26: main#content › div › div › div │     <div><div><div>deep needle</div></div></div>
27: main#content │     <img src="needle.png" alt="">
```

`html` and `body` hold the whole page, so Trails start below them. `li[3]` is the third `li`
of its `ul`, an element with an `id` gets no index, and the end tags HTML lets you leave out
(`<li>`, `<td>`, `<p>`) are inferred. A match inside a start tag, as on line 27, belongs to the
parent element. In XML the first construct that is not well formed ends the structure, and the
matches after it print with an empty Trail.

In CSV and TSV the Trail is the row and the column's header:

```
$ s4 needle table/plans.csv --flat
3: row 2 › Plan │ 2,Bea,needle,"likes ""quotes"", and commas"
5: row 3 › Notes │ lines with needle"
7: row 4 › Notes │ 4,Dan,team,needle at the end,extra
```

`row 2` is the second record after the header. Line 5 is the second line of a quoted field of
row 3, so it is still in row 3.

In subtitles the Trail is the cue, named by its number (or its WebVTT identifier) and its start
time as written; the tree shows the whole timing line:

```
$ s4 needle subs/movie.srt --tree
subs/movie.srt
  cue 1 00:00:01,000 --> 00:00:03,500
    3:Where is the needle?
  cue 2 00:00:04,000 --> 00:00:06,000
    8:- A needle, in there?
  cue 3 00:01:02,500 --> 00:01:04,000
    12:<i>needle</i>
```

### Object files

An ELF, PE or Mach-O file is recognised by its first bytes, whatever its name (`libc.so.6`,
`ls`). Where ripgrep would print `binary file matches`, s4 prints one row per printable string
that holds matches, at the hex offset of its first match. The Trail is the image (the Format and
the architecture), the section, and the nearest symbol at or before the match, with the distance
to it:

```
$ s4 needle m4/bin/app --flat
0x68: ELF x86-64 › .rodata › main.version+0xe │ version=1.2.3-needle
0x77: ELF x86-64 › .rodata │ another needle here
```

```
$ s4 needle m4/bin/app --tree
m4/bin/app
  ELF x86-64
    .rodata
      main.version+0xe
        0x68:version=1.2.3-needle
      0x77:another needle here
```

The second string lies past the end of `main.version`, so its Trail stops at the section, as
every Trail of a stripped binary does. A PE and each architecture of a fat Mach-O get the same
three Segments:

```
$ s4 needle m4/bin/app.exe m4/bin/app-mac --flat --sort path
m4/bin/app.exe:0x410: PE x86-64 › .rdata › version_string+0xe │ version=1.2.3-needle
m4/bin/app-mac:0x1141: Mach-O x86-64 › __TEXT,__cstring › _version+0xe │ version=1.2.3-needle
m4/bin/app-mac:0x2141: Mach-O arm64 › __TEXT,__cstring › _version+0xe │ version=1.2.3-needle
```

Over a real binary, this machine's `/bin/ls`:

```
$ s4 --flat -m3 'GLIBC_2\.3' /bin/ls
0x14af: ELF x86-64 › .dynstr │ GLIBC_2.38
0x14c5: ELF x86-64 › .dynstr │ GLIBC_2.33
0x14fb: ELF x86-64 › .dynstr │ GLIBC_2.34
```

Rows appear where ripgrep finds a match in a binary file: a file named on the command line, any
file with `--binary`, or any file with `-a`. A walked object file without `--binary` stops at
its first NUL byte and prints nothing, as in ripgrep. With `-v`, `-c`, `-l`, `-q` and
`--no-trail` the file prints what ripgrep prints. A string with several matches is one row, and
`-m N` keeps the first N rows. Rust symbols are demangled without their hash
(`core::fmt::write`).

### Documents

By default, PDF, Word, Excel, PowerPoint, OpenDocument, EPUB and SQLite files are searched in
the text that s4's built-in Extractors take from them, in a terminal and in a pipe alike. Each paragraph, heading, list item or
table row is one line, with the cells of a row separated by tabs, and each match gets the Trail
of the document's structure:

```
$ s4 needle m4/report.docx --tree
m4/report.docx
  # 1 Intro
    2:The intro needle.
    ## 1.1 Scope
      4:Scope needle.
  # 2 Costs
    table 1
      row 2
        column «Price»
          7:Pens	1.50 needle
```

A Word or OpenDocument heading nests as a Markdown heading does, and a table is `table N › row R`
with the column named by the first row's cell. A spreadsheet cell is `Sheet › cell`, a slide is
`slide N`, and its speaker notes are `slide N › notes`:

```
$ s4 needle m4/budget.xlsx --flat
3: Budget › A3 │ needle pins	2
```

```
$ s4 needle m4/deck.pptx --flat
2: slide 1 │ a needle margin
3: slide 1 › notes │ tell the needle joke
5: slide 2 │ find the needle
```

A PDF's bookmarks are chapters that hold its pages, and each page is `page N`. An EPUB's
chapters are named by its table of contents. A SQLite row is `table › row N › column`, with its
values separated by tabs:

```
$ s4 needle m4/report.pdf --tree
m4/report.pdf
  1 Intro
    page 2
      3:intro needle
  2 Costs
    page 3
      5:costs needle
```

```
$ s4 ana.needle docs/shop.sqlite --flat
12: users › row 12 › email │ 12	User 12	ana.needle@example.com	18.0
```

Every document of the search is extracted, whether it holds a match or not, so a tree with
many documents takes longer than ripgrep over it, and `--no-trail` prints the lines of their
text where ripgrep prints what it finds in their bytes. With `--no-docs`, these Formats are
never detected: the files are searched as ripgrep searches them, at no extra cost, and print
what ripgrep prints. Use it when speed over a tree full of documents matters more than their
text, or when a script needs ripgrep's exact output (`s4 --no-docs --no-trail` prints what `rg`
prints). `--docs` turns the default back on, and of `--docs` and `--no-docs` the one given last
wins:

```
$ s4 --no-docs needle m4/report.pdf
binary file matches (found "\0" byte around offset 1136)
```

A document that the Extractor cannot read gets one warning, which `--no-messages` silences,
and is then searched as ripgrep searches it; the exit code stays ripgrep's. Here `broken.docx`
starts with a zip's first bytes and is not a zip, and the match is counted in its bytes:

```
$ s4 -c needle m4/locked.pdf m4/broken.docx
s4: m4/locked.pdf: pdf: encrypted, and the empty password does not open it
s4: m4/broken.docx: docx: not a zip file the reader can open (invalid Zip archive: Could not find EOCD)
m4/broken.docx:1
```

Each document is read in a worker process, `s4` itself run again, never an external program. A
worker may allocate 1 024 MiB, a count that the worker keeps itself, and may spend 60 s on one
document. Past either limit, the document gets one warning (`reading it needed more than the
1024 MiB a reader may use`, or `reading it took more than 60 s`) and is searched as ripgrep
searches it. The count is the same code on Linux, macOS and Windows, and each system adds a
second barrier 64 MiB above it: `RLIMIT_DATA` on Linux and macOS (where macOS accepts it), and
a Job Object memory limit on Windows. The CI runs the count on the four systems, and the Job
Object's limit on Windows. Against decompression
bombs, a zip part decompresses to at most 256 MiB and a whole document to 1 GiB, a PDF stream to
64 MiB, and at most 256 MiB of text is taken from one document.

- Only regular files are documents: stdin and pipes are searched as ripgrep searches them.
- Searching documents is independent of `-z`, `-a` and `--binary`. A file that `--pre` or `-z`
  reads is never extracted, so `report.pdf.gz` with `-z` is searched as decompressed bytes.
- `--within` uses every Segment, and `--expand` prints the Section of the last one: the row of
  a cell, the page of a PDF match. Page, sheet, slide, table, row, column, cell, chapter and
  notes names are not in the text, so `--in-headings` finds only headings in documents.
- An encrypted PDF that opens with the empty password is read. Text drawn as images (scans)
  has no text, since there is no OCR. A PDF object with an integer past 64 bits is left out by
  the PDF parser, and with it the text it draws, without a warning.
- The binary Office formats (`.doc`, `.xls`, `.ppt`) are not read, nor the headers, footers,
  footnotes and comments of Office documents, nor a SQLite `-wal` file.
- A zip archive is never opened.

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
| `--no-docs` | search PDF, Word/ODT, Excel/ODS, PowerPoint/ODP, EPUB and SQLite files as ripgrep does, at ripgrep's speed, instead of in their text through the built-in Extractors, which is the default (see [Documents](#documents)) |
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
- An ELF, PE or Mach-O file with a match prints the rows of [Object files](#object-files) where
  ripgrep prints `binary file matches`, so s4 searches the whole file; in `--json`,
  `bytes_searched` is the file's size and `binary_offset` is null.
- Documents are searched in their text by default, also with `--no-trail`, `-c`, `-l`, `-q`
  and `--json`, and each one is extracted, which costs more than ripgrep's search of its bytes.
  `--no-docs` gives ripgrep's output and cost (see [Documents](#documents)).
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
name       = "ini"
extensions = ["ini"]
processor  = "external"

[params]
command    = ["searchfor-example-helper"]
mode       = "pool"
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

[`helpers/example/`](helpers/example) is the reference implementation, a Rust helper that
describes INI files. With the Definition above in the formats directory and
`searchfor-example-helper` on the `PATH` (`cargo build --release -p searchfor-example-helper`), over a
file `app.ini` written for this example:

```
$ s4 needle app.ini --tree
app.ini
  section database
    host
      2:host = needle.local
  section cache
    6:; needle in a comment
```

The three modes give this same output. The protocol is in
[spec §4.4](docs/specs/2026-10-16-searchfor-design.md#44-helper-processors-7-84518459).

[docs/external-processors.md](docs/external-processors.md) is the guide for writing one: the
protocol message by message, the three modes, the failures and how s4 reports them, and a
worked example in Python that does an INI Format first with `--pre` and then as a Helper
Processor, with the recipe for each system.

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
| M6 | v1.x | more Formats: image and audio metadata, WebAssembly and Java `.class` (at the hex offset of the match, as object files); the binary Office formats (`.doc`, `.xls`, `.ppt`); the headers, footers, footnotes and comments of Office documents |

Archive Formats (zip, tar, 7z, rar, deb, rpm, jar) stay out: searchfor searches what ripgrep
searches, and `-z` keeps ripgrep's meaning (single-stream compressed files such as `.gz`).

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
