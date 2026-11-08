> 🇦🇷 Leer en español: [formats.es.md](formats.es.md)

# searchfor Formats

This is the reference of every Format s4 describes: which files each one claims, the Segments
its Trails are made of, its limits and what it does with input it cannot read. The
[README](../README.md#formats) has the short table, with one example Trail per Format.

Every example below is real output over [`tests/fixtures/`](../tests/fixtures), run from inside
that directory, except where it says what it runs over. The object files and documents under
`m4/` are written there by the writers that [`tests/cli_m4.rs`](../tests/cli_m4.rs) uses
(`searchfor::processors::object::fixture` and `searchfor::docs::fixture`), and the rows of
[images, audio and binaries](#images-audio-and-binaries) come from the files that
`searchfor::processors::object::fixture::m6` writes for
[`tests/cli_m6_object.rs`](../tests/cli_m6_object.rs). `s4 --list-formats` lists every Format with its origin and extensions (without
extensions, the names and paths that detect it, or `magic bytes`), and `s4 --export-formats DIR` writes the built-in Definitions into `DIR` as TOML files to copy and
edit.

## How s4 picks a Format

A file gets the first Format that one of these signals gives, in this order:

1. `--format NAME` for every file, or `--format 'GLOB=NAME'` for the files the glob matches.
2. An exact file name of a Definition's `names`: `Dockerfile`, `.env`, `nginx.conf`.
3. A compound extension of `extensions`, an entry with a dot: `cart.blade.php` is Blade, while
   `cart.php` stays PHP.
4. A glob of `paths` whose last component writes the extension: `**/ChatExport_*/result.json`
   makes a `result.json` inside a Telegram export `telegram`, while a `result.json` anywhere
   else stays JSON. The glob's extension is compared without case, as in 5, so
   `/etc/nginx/conf.d/APP.CONF` is nginx; the rest of the glob keeps its case.
5. The last extension, compared without case.
6. Every other glob of `paths`, whose last component leaves the extension open:
   `**/.ssh/config`, `Dockerfile.*`, `.env.*`. It comes after the extension, so
   `Makefile.toml` stays TOML and `.env.json` stays JSON.
7. The first bytes (`magic`), and 8. the shebang line (`#!/usr/bin/env python3`), both read
   only for a file whose name has no extension that decides a Format.

A glob without a `/` is matched against the file name and one with a `/` against the whole
path, with `/` between components on every system (on Windows the glob sees `\` as `/`).
s4 never looks at a file's content to choose a text Format. A file no signal claims is
**Plain**: its matches print with an empty Trail, as `rg` prints them. A user Definition with
the name of a built-in replaces it, and `--list-formats` shows it as `overridden`.

## Every Format

| Format | `--format` | Files | Segments |
|---|---|---|---|
| Markdown | `markdown` | `.md`, `.markdown`, `.mdx`, `.mkd`, `.mdown` | frontmatter title, headings, fenced code blocks, parent list items, table columns |
| YAML | `yaml` | `.yaml`, `.yml` | keys and array elements; `document N` for each document of a stream |
| JSON | `json` | `.json`, `.jsonc`, `.jsonl`, `.geojson`, `.webmanifest`, `.babelrc`, `.eslintrc` | keys and array elements; comments and trailing commas are accepted |
| TOML | `toml` | `.toml`, `Cargo.lock`, `Pipfile`, `poetry.lock`, `uv.lock` | keys, tables and arrays of tables |
| PHP | `php` | `.php`, `.phtml` | namespaces, classes, interfaces, traits, enums, methods, functions |
| JavaScript | `javascript` | `.js`, `.mjs`, `.cjs`, `.jsx` | classes, methods, functions, and arrow functions or function expressions given a name |
| TypeScript | `typescript` | `.ts`, `.mts`, `.cts` | the JavaScript ones, plus namespaces, interfaces and abstract classes |
| TSX | `tsx` | `.tsx` | the TypeScript ones |
| Bash | `bash` | `.sh`, `.bash`, `.bats`, `.bashrc`, `.bash_profile`, `.bash_aliases`, `.profile` | functions |
| Go | `go` | `.go` | types, functions, and methods named by their receiver (`Server.Start`) |
| Rust | `rust` | `.rs` | modules, structs, enums, traits, `impl` blocks, functions |
| Python | `python` | `.py`, `.pyi`, `.pyw` | classes and functions, nested ones included |
| HTML | `html` | `.html`, `.htm`, `.xhtml` | elements as `tag#id.class`, with `[n]` among siblings of the same tag; `html` and `body` hidden; since 1.1, the code and JSON-LD inside `<script>` ([below](#html-scripts-changed-in-11)) |
| XML | `xml` | `.xml`, `.xsd`, `.xsl`, `.xslt`, `.svg`, `.rss`, `.atom`, `.plist`, `.wsdl`, `.xlf`, `.xliff`, `.kml`, `.gpx`, `.csproj`, `.vbproj`, `.fsproj`, `.nuspec`, `.resx`, `.xaml`, and files starting with `<?xml` | elements by tag, with `[n]` among siblings of the same tag |
| CSV | `csv` | `.csv` | `row N › Column`; the delimiter (`,` `;` tab `\|`) is read from the first line |
| TSV | `tsv` | `.tsv` | `row N › Column` |
| SubRip | `srt` | `.srt` | cues: `cue 12 00:01:02,500` |
| WebVTT | `vtt` | `.vtt` | cues: `cue intro 00:00.000` |
| ELF | `elf` | files whose first bytes say so, any name | image › section › symbol, at the hex offset |
| PE | `pe` | files whose first bytes say so, any name | image › section › symbol, at the hex offset |
| Mach-O | `macho` | files whose first bytes say so, any name; fat (universal) files too | image › section › symbol, at the hex offset |
| PDF | `pdf` | `.pdf`, and files starting with `%PDF-` | bookmarks as chapters, and pages |
| Word | `docx` | `.docx`, `.docm`, `.dotx`, `.dotm` | headings, and tables |
| Excel | `xlsx` | `.xlsx`, `.xlsm`, `.xltx`, `.xltm` | sheets and cells |
| PowerPoint | `pptx` | `.pptx`, `.pptm`, `.potx`, `.potm`, `.ppsx`, `.ppsm` | slides and their notes |
| OpenDocument text, spreadsheet, presentation | `odt`, `ods`, `odp` | `.odt`, `.ott`; `.ods`, `.ots`; `.odp`, `.otp`; and files whose `mimetype` says so | as Word, Excel and PowerPoint |
| EPUB | `epub` | `.epub`, and files whose `mimetype` says so | chapters, named by the table of contents, and headings |
| SQLite | `sqlite` | `.sqlite`, `.sqlite3`, `.db`, `.db3`, `.s3db`, `.sl3`, and files starting with `SQLite format 3` | tables, rows and columns |
| AsciiDoc | `asciidoc` | `.adoc`, `.asciidoc` | document title, headings, source and literal blocks |
| reStructuredText | `rst` | `.rst`, `.rest` | headings, code blocks, directives |
| Org-mode | `org` | `.org` | headings (TODO keywords and tags kept), source and example blocks, table columns |
| LaTeX | `latex` | `.tex`, `.ltx`, `.latex` | title, `\part` to `\subparagraph`, environments, code environments |
| Jupyter | `jupyter` | `.ipynb` | Markdown cells' headings, cells, outputs, keys |
| INI | `ini` | `.ini`, `.cfg`, `.cnf`, `.inf`, `.desktop`, `.gitconfig`, `.gitmodules`, `.editorconfig`, `.npmrc`, `.pypirc`, `.flake8`, `.pylintrc`, `.coveragerc`, `.hgrc`, `**/.git/config`, `**/git/config` | sections and keys |
| systemd units | `systemd` | `.service`, `.socket`, `.timer`, `.mount`, `.automount`, and `**/systemd/**/*.{path,target,slice,scope,swap,network,netdev,link,conf}` | sections and keys |
| Java properties | `properties` | `.properties` | keys, decoded |
| dotenv | `env` | `.env`, `*.env`, `.env.*` | variables |
| SSH configuration | `ssh-config` | `ssh_config`, `sshd_config`, `**/.ssh/config`, `**/.ssh/config.d/*`, `**/ssh/ssh_config.d/*.conf`, `**/ssh/sshd_config.d/*.conf` | `Host` and `Match` sections, keywords |
| nginx | `nginx` | `nginx.conf`, `**/nginx/**/*.conf`, `**/nginx/sites-available/*`, `**/nginx/sites-enabled/*` | blocks (`server` named by its `server_name`), directives |
| Apache | `apache` | `httpd.conf`, `apache2.conf`, `.htaccess`, `**/apache2/**/*.conf`, `**/httpd/**/*.conf`, `**/apache2/sites-available/*`, `**/apache2/sites-enabled/*` | sections (`VirtualHost` named by its `ServerName`), directives |
| Terraform and HCL | `hcl` | `.tf`, `.tfvars`, `.hcl`, `.nomad` | blocks with their labels, attributes, object keys, list elements |
| Dockerfile | `dockerfile` | `Dockerfile`, `Containerfile`, `dockerfile`, `.dockerfile`, `.containerfile`, `Dockerfile.*`, `Containerfile.*` | stages, instructions |
| Makefile | `make` | `Makefile`, `makefile`, `GNUmakefile`, `BSDmakefile`, `.mk`, `.mak`, `.make`, `Makefile.*` | rules, variables, `define`, conditionals |
| justfile | `just` | `justfile`, `Justfile`, `.justfile`, `.just` | recipes, variables, aliases |
| SQL | `sql` | `.sql`, `.ddl`, `.dml` | statements, `CREATE TABLE` columns, `INSERT` and `COPY` rows and columns |
| Diffs and patches | `diff` | `.diff`, `.patch`, `.rej` | commits, files, hunks |
| Vue | `vue` | `.vue` | template elements, the script's functions, `style` |
| Svelte | `svelte` | `.svelte` | elements, `{#if}` / `{#each}` blocks, the script's functions |
| Blade | `blade` | `.blade.php` | elements, paired directives (`@foreach`, `@section`) |
| GraphQL | `graphql` | `.graphql`, `.gql`, `.graphqls` | definitions, fields |
| Protobuf | `protobuf` | `.proto` | messages, enums, services, `rpc`, `oneof`, fields |
| HAR | `har` | `.har` | entries named by method and URL, headers, cookies and parameters by their name |
| JPEG, PNG, WebP, TIFF | `jpeg`, `png`, `webp`, `tiff` | files whose first bytes say so, any name | segments and chunks, EXIF IFDs and tags, XMP elements, at the hex offset |
| ID3 and MP4 | `id3`, `mp4` | files whose first bytes say so, any name | ID3 frames; MP4 boxes, at the hex offset |
| WebAssembly | `wasm` | files whose first bytes say so, any name | sections, exports, imports, functions, data segments, at the hex offset |
| Java class | `java-class` | files whose first bytes say so, any name | class, methods, fields, constant pool, at the hex offset |
| Mail | `eml` | `.eml` | header fields, body, MIME parts and attachments |
| mbox | `mbox` | `mbox`, `.mbox`, `.mbx`, and files without an extension starting with `From ` | messages by subject, header fields, parts |
| iCalendar | `icalendar` | `.ics`, `.ical`, `.ifb` | components named by `SUMMARY`, properties |
| vCard | `vcard` | `.vcf`, `.vcard` | cards named by `FN`, properties |
| BibTeX | `bibtex` | `.bib` | entries by key, fields |
| WhatsApp | `whatsapp` | `_chat.txt`, `*WhatsApp*.txt` | days, messages by time and sender |
| Telegram JSON | `telegram` | `**/ChatExport_*/result.json`, `**/DataExport_*/result.json` | chats, messages by date and sender, keys |
| Telegram HTML | `telegram-html` | `**/ChatExport_*/messages*.html`, `**/DataExport_*/chats/*/messages*.html` | messages by date and sender |

The code Formats are parsed by tree-sitter Grammars compiled into `s4`; YAML, JSON and TOML by
native parsers; HTML, XML, CSV, TSV, subtitles and every Format added in 1.1 by scanners of
s4's own; object files by goblin and by readers of s4's own; and documents by Extractors built
into `s4`, written in Rust, with no external program.

## The Formats of 1.0

### Data and code

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

### HTML and XML

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

### HTML scripts, changed in 1.1

Since 1.1 a Match inside a page's `<script>` gets the Trail of the script's own code below the
`script` element, where 1.0 stopped at the element. This is the one change to the Trails of a
1.0 Format, and [`docs/rg-compat.md`](rg-compat.md#trails-that-changed-since-10) records it:

```
$ s4 needle code-c/m6-scripts.html --flat
4: head › script[1] › onClick │   return 'needle';
8: head › script[2] › load │ export function load() { return 'needle'; }
11: head › script[3] › offers › price │ {"@type":"Product","offers":{"price":"needle"}}
14: head › script[4] │ function tpl() { needle }
```

In 1.0 the first line was `4: head › script[1] │   return 'needle';`. A script with no
`type`, a JavaScript type (`text/javascript` and its aliases) or `module` is read with the
JavaScript Grammar, or TypeScript or TSX when its `lang` says so; `application/ld+json` is read
as JSON, with its key paths. Three limits keep 1.0's Trail, the `script` element alone: any
other type (`text/x-template` on line 14, `text/plain`, `importmap`), a script over 1 MiB
(`script_max_bytes`), and a script with a line over 1 000 bytes (`script_max_line`), which is
minified code. `--no-trail` prints what 1.0 printed. A user Definition that copies `html.toml`
with `scripts = false` gives back 1.0's Trails.

### CSV and TSV

In CSV and TSV the Trail is the row and the column's header:

```
$ s4 needle table/plans.csv --flat
3: row 2 › Plan │ 2,Bea,needle,"likes ""quotes"", and commas"
5: row 3 › Notes │ lines with needle"
7: row 4 › Notes │ 4,Dan,team,needle at the end,extra
```

`row 2` is the second record after the header. Line 5 is the second line of a quoted field of
row 3, so it is still in row 3.

### Subtitles

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

## Text documents

AsciiDoc, reStructuredText, Org-mode and LaTeX get Markdown's Segments: the document title, the
headings with their level, and the code blocks named by their language. Each heading and code
block carries its line as written as a `signature`, which the tree shows:

```
$ s4 needle docs-a/m6.adoc --tree
docs-a/m6.adoc
  Deploy guide
    == Install
      === Linux
        [source,bash]
          6:apt install needle-pkg
          7:== needle literal heading
```

A heading's Section runs to the next heading of the same or a higher level. A heading inside a
code, literal or comment block is text, as line 7 shows. These four Formats have no syntax
errors: a block left open runs to the end of the file, as an unclosed Markdown fence does.
`include::`, `\input` and `\include` are never followed.

### AsciiDoc

`= Title` at the top is the document title, and `==` to `======` are levels 1 to 5, as are the
Markdown-style `#` headings Asciidoctor accepts; a heading needs a space after its markers. A
listing (`----`) after `[source,LANG]` is a code Segment named `LANG`, else `listing`; a
literal block (`....`) is `literal`, and a fence (```` ``` ````) is named by its info string.
Comment, passthrough, sidebar, example and quote blocks hold text. The two-line titles of
AsciiDoc's legacy syntax are text, and tables give no column Segment.

```
$ s4 needle docs-a/m6.adoc --flat
6: Deploy guide › Install › Linux › bash │ apt install needle-pkg
7: Deploy guide › Install › Linux › bash │ == needle literal heading
```

### reStructuredText

A heading is a line underlined, and optionally overlined, by one repeated punctuation
character. Levels follow the order in which each style first appears, as docutils assigns them;
s4 never promotes a lone top section to a document title. `.. code-block:: LANG`, `.. code::`
and `.. sourcecode::` are code Segments named `LANG`; a paragraph ending in `::` opens a
`literal` block; any other directive is a `directive` Segment named by its type. Tables give no
column Segment; substitutions and roles are text.

```
$ s4 needle docs-a/m6.rst --flat
10: Install › Linux › bash │    apt install needle-pkg
14: Install › Linux › note │    needle note
```

### Org-mode

`*` to any depth is a heading of that level, its TODO keyword, priority and tags kept in its
name, so `--in-headings` finds them; `#+TITLE:` is the document title. `#+BEGIN_SRC LANG` is a
code Segment named `LANG` (`src` without one), `#+BEGIN_EXAMPLE` is `example`. A table whose
first row is followed by a rule (`|---+---|`) names its columns, as a Markdown table does.

```
$ s4 needle docs-a/m6.org --flat
5: Budget › TODO Costs :work: › Price │ | Hosting | needle 9 |
7: Budget › TODO Costs :work: › bash │ * needle text
10: Budget › TODO Costs :work: › example │ needle example
```

### LaTeX

`\part` to `\subparagraph`, starred or not, are levels 1 to 7, named by their braced title;
`\title{…}` is the document title. `\begin{X}` … `\end{X}` is an environment Segment named `X`;
`lstlisting` with `[language=L]` and `minted` with `{L}` are code Segments named `L`, and
`verbatim`, `Verbatim` and `comment` are code Segments in which nothing is read as a command.
The environments of the Definition's `hidden_environments` (`document`, `itemize`, `enumerate`,
`description`) are hidden, as HTML's `body`. A `%` comment is text, and so is a macro that
expands to a heading.

```
$ s4 needle docs-a/m6.tex --flat
6: A Study of Needles › Methods › Sampling › figure │ \caption{needle distribution}
9: A Study of Needles › Methods › Sampling › bash │ \section{needle text}
```

### Jupyter notebooks

A notebook is JSON read through the `jupyter` view (`view = "jupyter"`): the headings of its
Markdown cells hold the cells below them, each cell is `cell N`, 1-based, and each output
`output N`. The keys `cells`, `source`, `outputs`, `data` and `text` are hidden; the MIME keys
of an output stay. `cell N` and `output N` are words of s4, so `--in-headings` finds the
headings and the kept keys only. A file without a `cells` array keeps JSON's Trails, and a JSON
syntax error keeps JSON's rule.

```
$ s4 needle docs-a/m6.ipynb --flat
4: Analysis › Load data › cell 2 │   {"cell_type":"code","source":["df = load('needle.csv')\n"],
5: Analysis › Load data › cell 2 › output 1 › text/plain │    "outputs":[{"output_type":"execute_result","data":{"text/plain":["needle"]}}]},
6: Analysis › Load data › cell 3 │   {"cell_type":"raw","source":["needle raw"]}
```

## Configuration and infrastructure

### INI and systemd units

`[section]` opens a section up to the next header, named by the text between the brackets as
written (`[remote "origin"]` is `remote "origin"`, and `[a.b]` one section `a.b`); `key = value`
and `key: value` are keys. `;` and `#` start comments, and `\` at a line's end continues it. An
indented line continues the key above when it holds neither `=` nor `:`, and is a key of its
own otherwise, so `.gitconfig`'s indented keys are keys and `configparser`'s multi-line values
are values. INI has no syntax errors: a line that is neither a header nor a key is text of
what is above it. systemd units are INI with their own Definition, `systemd`.

```
$ s4 needle config/m6.ini --flat
3: server › host │ host = needle.example
4: server › host │   continued needle
7: remote "origin" › url │ url=needle.git
9: a.b › k │ k=needle
```

```
$ s4 needle config/m6.service --flat
4: Service › ExecStart │ ExecStart=/usr/bin/needle \
```

### `.properties`

A key ends at the first unescaped `=`, `:` or white space, and is decoded (`\:`, `\=`, `\ `,
`\uXXXX`): `--json`, `--within` and `--in-headings` see `café:key` for `café\:key`. A
dotted key stays one Segment, since the format has no hierarchy. `#` and `!` start comments.

```
$ s4 needle config/m6.properties --flat
2: spring.datasource.url │ spring.datasource.url=jdbc:needle://db
3: café:key │ café\:key : needle\
```

### `.env`

`KEY=value` and `export KEY=value`; a quoted value may span lines, and its key's Section covers
them; `#` starts a comment outside quotes. `.env`, `.env.production` and `app.env` are `env`.

```
$ s4 needle config/.env.m6 --flat
2: DATABASE_URL │ export DATABASE_URL=postgres://needle@db/app
4: TEXT │ needle # literal
```

### SSH configuration

`Host …` and `Match …` open a section named by its line as written, up to the next one; every
other line is a keyword, compared without case as ssh does. `Include` is never followed. A file
named `config` is SSH's only inside `.ssh/`.

```
$ s4 needle config/ssh/ssh_config --flat
3: Host github.com gitlab.com › IdentityFile │   IdentityFile ~/.ssh/needle_ed25519
5: Match user ana › ProxyCommand │   ProxyCommand=needle --proxy
6: Match user ana › Include │ Include /never/read/needle
```

### nginx

A block (`location /api`, `upstream backend`) is named by its directive and arguments as
written, and a `server` block by `server` and its first `server_name`, read from anywhere in the
block. A directive is a key Segment up to its `;`. `#` comments and quoted strings are read, so
a brace in a string is no structure. A `}` that closes nothing ends the structure: the matches
after it get an empty Trail. A block still open at the end of the file ends there. A `.conf`
is nginx's only under an `nginx/` directory; elsewhere it stays Plain.

```
$ s4 needle config/nginx/nginx.conf --tree
config/nginx/nginx.conf
  http
    server example.com
      location /api
        proxy_pass
          4:   proxy_pass http://needle:8080;
```

### Apache

`<Name args>` … `</Name>` sections nest, compared without case; a `VirtualHost` is named with
its first `ServerName`. A directive is one line, which `\` continues. A closing tag that
matches no open section is ignored, and an open section ends with the file.

```
$ s4 needle config/apache2/apache2.conf --flat
3: VirtualHost *:443 example.com › Directory /var/www › Require │   Require ip 10.0.0.0/8 # needle
```

### Terraform and HCL

A block is named by its type and labels without quotes (`resource aws_instance web`); an
attribute is a key, an object value holds keys and a list holds `[N]` elements, 0-based, as in
JSON. Comments (`#`, `//`, `/* */`), strings with `${…}` and `%{…}`, and heredocs are read, so
line 10's `${not_a_block}` opens nothing. A stray `}` ends the structure, as in nginx.

```
$ s4 needle config/m6.tf --flat
3: resource aws_instance web › tags › Name │   Name = "needle-web"
7: resource aws_instance web › ingress[1] › name │   { name = "needle" }
10: resource aws_instance web › script │   echo needle ${not_a_block}
```

### Dockerfile

Each `FROM` opens a stage named by its `AS` name, or `stage N` (0-based, Docker's numbering for
`--from=N`), up to the next `FROM`. Each instruction is named by its keyword, over its
continuation lines and heredocs; the escape character is `\` or the one `# escape=` sets.
`--expand` prints a whole instruction:

```
$ s4 needle config/m6.dockerfile --flat
5: build › RUN │  needle-build --release
7: build › RUN │ needle heredoc
10: stage 1 › COPY │ COPY --from=build /needle /app
```

```
$ s4 needle config/m6.dockerfile --flat --expand --within build
3-5: build › RUN
RUN echo start \
 # continuation comment
 needle-build --release

6-8: build › RUN
RUN <<EOF
needle heredoc
EOF
```

`stage 1` is a word of s4, so `--in-headings` finds `build` and never `stage`.

### Makefile

A rule is named by its targets as written, over its recipe; a variable (`=`, `:=`, `?=`, `+=`,
`!=`, with `override` or `export`) and `define NAME` … `endef` are keys; `ifeq` … `endif` and
its siblings are conditional Segments named by their line, which hold what is inside them.
`$(…)` is skipped when looking for a rule's `:`. A drive letter (`C:/`, `C:\`) is part of its
target, as GNU make reads it on Windows, so one letter glued to an absolute prerequisite
(`c:/x`) is read as a path and opens no rule, where GNU make on Linux reads the rule `c`; with a
blank after the `:` (`c: /x`) it is the rule `c` on both platforms. An `endif` or `endef` that
closes nothing is text, and an open one ends with the file.

```
$ s4 needle config/m6.mk --flat
3: ifeq ($(OS),Windows_NT) › release test │ 	@echo needle
6: ifeq ($(OS),Windows_NT) › release test │ 	echo needle again
8: ifeq ($(OS),Windows_NT) › NAME │ NAME := needle
11: BODY │ needle body
15: %.o │ >echo needle pattern
```

### justfile

A recipe is named by its name, over its indented body; a variable or an `alias` is a key. The
comments above a recipe are its documentation and lie outside it, as line 1 shows.

```
$ s4 needle config/m6.just --flat
1: # needle documentation
4: deploy │     rsync -a build/ host:/srv/needle/
6: URL │ URL := 'needle'
7: ship │ alias ship := deploy # needle
```

## Next to the code

### SQL

One permissive dialect reads PostgreSQL, MySQL, SQLite and SQL Server scripts and their dumps. A
statement ends at a `;` outside strings, quoted identifiers, dollar quotes and comments, at a
`GO` line, or at the delimiter a `DELIMITER` line sets, and is named by its leading keywords and
object. In `CREATE TABLE … ( … )` each column is a Segment. In `INSERT … VALUES`, each tuple is
`row N`, 1-based in its statement, and its value's column is named from the statement's column
list, else from the same table's earlier `CREATE TABLE`, else `column K`. PostgreSQL's
`COPY … FROM stdin` rows are read the same way, one per line:

```
$ s4 needle code-c/m6-dump.sql --flat
2: INSERT INTO users › row 2 › email │ INSERT INTO users VALUES (1,'plain'),(2,'needle@example.com');
3: INSERT INTO users › row 1 › email │ INSERT INTO users (email,id) VALUES ('needle',3);
6: COPY public.users › row 2 › email │ 2	needle@example.com
8: INSERT INTO unknown › row 1 › column 2 │ INSERT INTO unknown VALUES (1,'needle');
```

```
$ s4 needle code-c/m6-schema.sql --flat
3: CREATE TABLE users › email │  email text NOT NULL, -- needle
6: CREATE OR REPLACE VIEW active_users │ CREATE OR REPLACE VIEW active_users AS SELECT 'needle' AS label;
```

`row N` and `column K` are words of s4, so `--in-headings row` finds nothing. A dump of
gigabytes costs its statements' index only: s4 builds rows and columns for the statements that
hold matches, and reads no further than the statement of the last match. A string or comment
left open ends the structure at its statement. A `CREATE PROCEDURE` body without `DELIMITER`
splits at its inner `;`, and each piece keeps the name of the statement it starts.

### Diffs and patches

Unified diffs, combined diffs, `git log -p` and `git format-patch` series. A commit is named by
its short hash and subject, a file by its new path (the old one for a deletion), and a hunk by
`+N` and its context text. A match in a `diff --git`, `---` or `+++` line has the Trail above
the file, and one in a hunk header is in the file. A commit message's lines lie outside the
commit, as line 4 shows:

```
$ s4 needle code-c/m6.patch --flat
4:     Fix the needle crash
10: 3f2a9c1 Fix the needle crash › src/app.rs › +120 fn describe( │ +    let needle = 1;
```

stdin has no name to detect a Format from, so name it with `--format`:

```
$ git diff | s4 --format diff needle
```

A hunk whose counts run past the next header ends at that header.

### Vue, Svelte and Blade

Single-file components are HTML, read by HTML's rules. The code of a `<script>` gets the Trails
of the Grammar its `lang` names (none or `js` JavaScript, `ts` TypeScript, `tsx` TSX) below the
`script` element; a script of another `lang` keeps the element alone, and `<style>` is text.

```
$ s4 needle code-c/m6.vue --flat
2: template › ul.items › li[2] │ <ul class="items"><li>x</li><li>needle template</li></ul>
6: script › addItem │   const value: string = 'needle';
9: style │ <style>.needle { color:red }</style>
```

Svelte's `{#if}`, `{#each}`, `{#await}`, `{#key}` and `{#snippet}` open a block named by their tag
without braces, closed by their `{/…}`:

```
$ s4 needle code-c/m6.svelte --flat
3: ul › #each items as item › li[1] │ <li>{item.needle}</li>
5: ul › #each items as item › li[2] │ <li>needle empty</li>
```

Blade's paired directives (`@section`, `@if`, `@foreach`, `@php`…) open a block named by the
directive and its argument, up to its `@end…`; `{{-- … --}}` is a comment. `cart.blade.php` is
Blade by its compound extension, and `cart.php` stays PHP.

```
$ s4 needle code-c/m6.blade.php --flat
4: @section('content') › div.cart › @foreach($items as $item) › li │ <li>{{ $item->needle }}</li>
```

A block closer with no open block is text, as an HTML end tag is.

### GraphQL and Protobuf

GraphQL's definitions (`type`, `input`, `query`, `fragment`…) are Segments with their keyword as
kind, and a field with arguments or a selection, and every field of a type, is a `field`
Segment; strings, block strings and `#` comments are read. The tree shows the kind before the
name (`type User`, `field email`). Protobuf's `message`, `enum`, `service`, `rpc`, `oneof` and
`extend` nest, and a field or an enum value is a `field` Segment. In both, a stray `}` ends the
structure.

```
$ s4 needle code-c/m6.graphql --flat
2: User › email │  email: String! # needle
7: GetUser › user › posts › title │    title(needle: true)
```

```
$ s4 needle code-c/m6.proto --flat
2: User › email │  string email = 1 [json_name = "needle"];
4: User › choice › nick │   string nick = 2; // needle
8: Users › Get │  rpc Get (User) returns (User) { option note = "needle"; }
```

### HAR captures

A HAR is JSON read through the `har` view: each entry is named by its request's method and URL
without the scheme (cut at 60 characters), the elements of `headers`, `cookies`, `queryString`
and `postData.params` by their `name`, and `pages` by their `title`. The URL may come after the
match in the file.

```
$ s4 needle code-c/m6.har --flat
3: GET api.example.com/v1/users › response › content › text │   "response":{"content":{"text":"needle"},"status":200},
5: GET api.example.com/v1/users › request › header Authorization › value │    "headers":[{"name":"Authorization","value":"needle"}],
6: GET api.example.com/v1/users › request › cookie session › value │    "cookies":[{"name":"session","value":"needle"}],
7: GET api.example.com/v1/users › request › queryString q › value │    "queryString":[{"name":"q","value":"needle"}],
8: GET api.example.com/v1/users › request › postData › param body › value │    "postData":{"params":[{"name":"body","value":"needle"}]}}
9: Login › id │  }],"pages":[{"title":"Login","id":"needle"}]}}
```

## Images, audio and binaries

JPEG, PNG, WebP, TIFF, ID3 (MP3), MP4 (and QuickTime, 3GP, HEIF and AVIF brands), WebAssembly
and Java class files are told by their first bytes, whatever their name, and read with the
rules of [object files](#object-files): where rg finds a match in a binary file (a file named on
the command line, `--binary` or `-a`), s4 prints one row per printable string that holds
matches, at its hex offset, with its Trail. A walked file without `--binary` prints what rg
prints, and `-v`, `-c`, `-l`, `-q` and `--no-trail` print rg's output. Only the raw bytes rg
finds are decorated: a base64 mail part, a compressed PNG chunk (`zTXt`), a UTF-16 ID3 frame
and the compressed streams of a class stay invisible to the search, as they are to rg.

| Format | Example row |
|---|---|
| `jpeg` | `0x26: JPEG › APP1 Exif › IFD0 › Model │ needle-cam 3000` |
| `png` | `0x31: PNG › tEXt Comment │ needle`; also `PNG › eXIf › IFD0 › Artist` and `PNG › iTXt XML:com.adobe.xmp › …` |
| `webp` | `0x2e: WebP › EXIF › IFD0 › Model │ needle` |
| `tiff` | `0x1a: TIFF › IFD0 › Model │ needle-cam 3000` |
| `id3` | `0x15: ID3v2.4 › TIT2 │ needle song` |
| `mp4` | `0x50: MP4 M4A › moov › udta › meta › ilst › ©nam │ needle` |
| `wasm` | `0x30: WebAssembly › data › data[0] │ needle not found` |
| `java-class` | `0x50: Java 17 › com.example.Cart › addItem │ needle merged` |

- **EXIF** is read wherever it sits (JPEG APP1, PNG `eXIf`, WebP `EXIF`, a TIFF file): the IFDs
  are sections, and a tag is named from the standard table (`Model`, `Artist`), or `tag 0x9c9b`.
- **XMP** is XML: its packet continues with the element path below `rdf:Description`.
- **ID3** frames are named by their id; `TXXX` and `WXXX` add their description. An MP3 with only
  an ID3v1 tag has no magic at its start and is not detected.
- **MP4** boxes are walked through the container boxes, and `mdat` holds nothing.
- **WebAssembly** sections are named by their id, a function by the `name` section or
  `func[N]`, and a data segment `data[N]`.
- **Java classes**: the image is `Java N` from the class version; a string loaded by a method's
  `ldc` belongs to that method, else to the field or method it names, else to `constant pool`.
- `cafebabe` starts both a Java class and a fat Mach-O: each Definition whose magic matches is
  tried until one reads the file.
- A file whose magic matches and whose layout does not read prints as rg prints it, without a
  warning. Archives (zip, tar, jar…) are never opened.

## Mail, calendars, contacts and chats

### Mail and mbox

An `.eml` file is one message; an mbox holds messages, each starting at a `From ` line, and
each named by its decoded `Subject`. A header field is named by its name, over its folded
lines; a body is `body`, and a multipart message's parts are `part N` in IMAP's numbering with
their media type, or `attachment NAME`. RFC 2047 and RFC 2231 names are decoded; bodies never
are.

```
$ s4 needle mail/m6.eml --flat
3: Subject │ Subject: Your needle invoice
5: X-Long │  needle folded
7: body │ needle body
```

```
$ s4 needle mail/m6-mime.eml --flat
10: part 1 multipart/alternative › part 1.1 text/plain │ needle plain
16: attachment café.txt │ needle attachment
```

```
$ s4 needle mail/m6.mbox --tree
mail/m6.mbox
  Ana · Today · Budget
    body
      6:needle one
      7:>From escaped needle
  Next
    body
      12:needle two
```

An mbox of gigabytes costs its messages' index only, as a SQL dump does. A boundary never
closed ends its part at the end of the message. Thunderbird's `Inbox` and `Sent`, which have no
extension, are told by their first bytes, `From `.

### iCalendar and vCard

`BEGIN:X` … `END:X` is a component named `X` and its `SUMMARY`, `FN` or `TZID` (`VEVENT Team
sync`, `VCARD Ana Pérez`), else `X[N]`; a single `VCALENDAR` around the file is hidden. A
property is named without its parameters, over its folded lines. An `END` closes the innermost
component of its name; one with none open is text.

```
$ s4 needle config/m6.ics --flat
3: VEVENT Team sync › DESCRIPTION │ DESCRIPTION:Bring the needle
4: VEVENT Team sync › DESCRIPTION │  folded needle
```

```
$ s4 needle config/m6.vcf --flat
2: VCARD Ana Pérez › EMAIL │ EMAIL;TYPE=work:ana@needle.example
```

### BibTeX

An entry is named by its key, and each field is a key; `@string` and `@preamble` are named so,
while `@comment` and the text between entries are text with an empty Trail. An entry whose
braces never close ends at the next `@` at a line's start.

```
$ s4 needle docs-a/m6.bib --flat
2: knuth1984 › title │  title = {Literate needle {programming}},
3: knuth1984 › note │  note = "needle" # suffix,
5: @comment{needle ignored}
```

### WhatsApp

A WhatsApp text export is detected by its name: `_chat.txt`, the file inside the export's zip,
and `*WhatsApp*.txt`, since the app's name is in the export's file name in every language
(`WhatsApp Chat with Ana.txt`, `Chat de WhatsApp con Ana.txt`). The messages of one date form a
day, named by the date as written, and a message is named by its time and sender:

```
$ s4 needle chat/_chat.txt --flat
1: 31/12/23 › 21:41 Ana │ 31/12/23, 21:41 - Ana: where's the needle?
2: 31/12/23 › 21:41 Ana │ continued needle
3: 31/12/23 › 21:42 │ 31/12/23, 21:42 - system needle
4: 01/01/24 › 09:00:01 Bob │ [01/01/24, 09:00:01] Bob: needle
```

A file whose first line opens no message is no export, and its matches have an empty Trail.
s4 never looks at a file's content to choose its Format, so an export renamed to a name the
Definition does not claim is read by naming its Format:

```
$ s4 --format 'chats/*.txt=whatsapp' needle chats/ana.txt
```

### Telegram

Telegram Desktop exports a chat into a `ChatExport_<date>` folder (an account into
`DataExport_<date>`, each chat's HTML pages in `chats/<chat>/`), as `result.json` or
`messages.html`, `messages2.html`… Those names are Telegram's only inside such a folder;
anywhere else they stay JSON and HTML.

```
$ s4 needle chat/ChatExport_m6/result.json --flat
4: Ana › 2024-01-31T10:04:12 Ana › text │   {"text":"needle","date":"2024-01-31T10:04:12","from":"Ana"},
6: Ana › 2024-01-31T10:05:00 Ana › text │    "text":["needle",{"type":"bold","text":"needle"}],
6: Ana › 2024-01-31T10:05:00 Ana › text › text │    "text":["needle",{"type":"bold","text":"needle"}],
7: Ana › 2024-01-31T10:05:00 Ana › text_entities › text │    "text_entities":[{"type":"plain","text":"needle"}]}
```

```
$ s4 needle chat/ChatExport_m6/messages.html --flat
5: 31.01.2024 10:04:12 Ana │ <div class='text'>needle</div>
9: 31.01.2024 10:05:00 Ana │ <div class='text'>needle</div>
```

A message without a sender name takes the sender of the one before. The HTML view depends on
the class names of Telegram's export template: without them the page keeps HTML's Trails.

## Rules every Format keeps

- **The search is rg's.** A Format decides only the Trail of the matches rg finds:
  `s4 --no-docs --no-trail` prints rg's output byte for byte over every fixture.
- **Input a Format cannot read** keeps the Trails of the matches before the point where reading
  failed and gives the matches after it an empty Trail, without a warning and with rg's exit
  code; a Format with no syntax errors (INI, the text documents, HTML) reads on.
- **Names** are the file's text as written, cut at the Definition's `name_chars` (40 when
  absent) with `…`. The words s4 adds to a name (`row 2`, `cell 3`, `stage 1`, `part 1.1`, `[1]`)
  come from no file, so `--in-headings` never finds them; a decoded name (a mail subject, a
  `.properties` escape, a JSON string) is searched decoded.
- **A match in a Segment's own name** has the Trail of the Segment's parent, as on a heading
  line; cues keep their own.
- **Sections.** `--expand` prints the Section of a match's last Segment, and `--json --expand`
  carries it; for the binary Formats it prints the rows.
- **Encodings and line ends** as in every Format: a UTF-8 BOM removed, UTF-16 transcoded, bytes
  that are not UTF-8 shown as U+FFFD in names, CRLF read as LF.
- **Depth.** Past 256 open Segments no more open, and nesting of any depth never crashes s4.
- **Parameters** a copy of a Definition may change: `syntax` (which rules a shared Processor
  applies), `view` (the JSON or HTML shape: `jupyter`, `har`, `telegram`), `scripts`,
  `script_max_bytes`, `script_max_line` and `templates` for markup, `layout` for binaries,
  `hidden_environments` for LaTeX and `name_chars` for any of them. `s4 --export-formats DIR`
  writes the built-in Definitions to start from.
